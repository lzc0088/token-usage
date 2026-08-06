//! Claude Code / Claude.ai quota adapter — dual path: OAuth (CLI) + Web Cookie.
//!
//! Path 1 — **OAuth** (CLI credentials):
//!   `GET https://api.anthropic.com/api/oauth/usage` with Bearer token.
//!   Supports refresh_token on 401.
//!
//! Path 2 — **Web Cookie** (browser session):
//!   `GET https://claude.ai/api/organizations` → pick best org → usage.
//!   Mirrors token-monitor's `fetchClaudeWebLimits`.
//!
//! Credential format (stored in keyring as-is):
//!   - OAuth JSON: `{"access_token":"sk-ant-...","refresh_token":"rt-..."}`
//!   - Web cookie JSON: `{"cookie":"sk-ant-session-key"}`
//!   - Web cookie raw: `sk-ant-session-key` or `sessionKey=sk-ant-session-key`
//!
//! Detection: JSON with `access_token` field → OAuth; otherwise → Web cookie.

use serde::Deserialize;

use super::types::{epoch_to_iso, Quota, QuotaStatus, QuotaWindow};
use super::VendorError;

const OAUTH_USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const OAUTH_PROFILE_URL: &str = "https://api.anthropic.com/api/oauth/profile";
const OAUTH_TOKEN_URL: &str = "https://console.anthropic.com/v1/oauth/token";
const WEB_BASE_URL: &str = "https://claude.ai";
const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36";

// ---------------------------------------------------------------------------
// HTTP trait
// ---------------------------------------------------------------------------

pub trait Http {
    /// GET with Authorization: Bearer <token> (OAuth path).
    fn get_bearer(&self, url: &str, token: &str) -> Result<String, VendorError>;
    /// GET with Cookie: sessionKey=<token> (Web path).
    fn get_cookie(&self, url: &str, session_key: &str) -> Result<String, VendorError>;
    /// POST form-urlencoded body (OAuth token refresh).
    fn post_form(&self, url: &str, body: &str) -> Result<String, VendorError>;
}

// ---------------------------------------------------------------------------
// Credential parsing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
enum CredKind {
    OAuth,
    WebCookie,
}

/// Normalize a Claude web cookie value.
/// Rejects values containing whitespace or semicolons (full cookie strings).
/// Strips "sessionKey=" prefix if present.
/// Returns the normalized `sessionKey=<value>` form, or empty string if invalid.
fn normalize_web_cookie(raw: &str) -> String {
    let raw = raw.trim();
    if raw.is_empty() {
        return String::new();
    }
    if raw.chars().any(|c| c.is_whitespace() || c == ';') {
        return String::new();
    }
    let session_key = raw.strip_prefix("sessionKey=").unwrap_or(raw);
    if session_key.starts_with("sk-ant-") && session_key.len() > "sk-ant-".len() {
        format!("sessionKey={}", session_key)
    } else {
        String::new()
    }
}

// ---------------------------------------------------------------------------
// OAuth token refresh
// ---------------------------------------------------------------------------

fn form_encode(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') {
                c.to_string()
            } else {
                format!("%{:02X}", c as u8)
            }
        })
        .collect()
}

fn refresh_access_token(http: &dyn Http, refresh_token: &str) -> Result<String, VendorError> {
    let body = format!(
        "grant_type=refresh_token&refresh_token={}&client_id={}",
        form_encode(refresh_token),
        form_encode(CLIENT_ID)
    );
    let raw = http.post_form(OAUTH_TOKEN_URL, &body)?;
    let v: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| VendorError::Parse(e.to_string()))?;
    v.get("access_token")
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| VendorError::Parse("no access_token in refresh response".into()))
}

// ---------------------------------------------------------------------------
// Web cookie path — mirrors token-monitor fetchClaudeWebLimits
// ---------------------------------------------------------------------------

fn fetch_web_limits(http: &dyn Http, session_cookie: &str) -> Result<Quota, VendorError> {
    // ① Get organizations.
    let orgs_body =
        http.get_cookie(&format!("{WEB_BASE_URL}/api/organizations"), session_cookie)?;
    let orgs: serde_json::Value =
        serde_json::from_str(&orgs_body).map_err(|e| VendorError::Parse(e.to_string()))?;

    // Extract org array: direct array | .organizations | .data
    let org_arr = orgs
        .as_array()
        .or_else(|| orgs.get("organizations").and_then(|d| d.as_array()))
        .or_else(|| orgs.get("data").and_then(|d| d.as_array()))
        .ok_or_else(|| VendorError::Parse("organizations: expected array".into()))?;

    // Select best org: prefer chat capability, fallback to first non-api-only.
    let org_id = select_best_org(org_arr)?;

    // ② Get usage for the selected org.
    let usage_body = http.get_cookie(
        &format!("{WEB_BASE_URL}/api/organizations/{}/usage", org_id),
        session_cookie,
    )?;
    let mut quota = parse_usage_response(&usage_body)?;

    // ③ Plan label from /api/account (best-effort — mirrors token-monitor
    //    claudeWebAccountIdentity → seat_tier + rate_limit_tier).
    if let Ok(account_body) =
        http.get_cookie(&format!("{WEB_BASE_URL}/api/account"), session_cookie)
    {
        quota.plan_label = plan_label_from_account(&account_body, &org_id);
    }

    Ok(quota)
}

/// Extract the plan label from `/api/account`.
/// Mirrors token-monitor `claudeWebAccountIdentity`: the membership's
/// `seat_tier` (or `billing_type`) combined with `rate_limit_tier`.
fn plan_label_from_account(body: &str, org_id: &str) -> Option<String> {
    let root: serde_json::Value = serde_json::from_str(body).ok()?;
    let account = root
        .get("account")
        .filter(|v| v.is_object())
        .unwrap_or(&root);

    // Find the membership matching this org (or the first one).
    let memberships = account
        .get("memberships")
        .and_then(|m| m.as_array())
        .or_else(|| root.get("memberships").and_then(|m| m.as_array()));
    let membership = memberships.and_then(|arr| {
        arr.iter()
            .find(|m| {
                m.get("organization")
                    .and_then(|o| o.get("uuid").and_then(|u| u.as_str()))
                    == Some(org_id)
            })
            .or_else(|| arr.first())
    });

    let seat_tier = membership
        .and_then(|m| m.get("seat_tier").and_then(|v| v.as_str()))
        .or_else(|| membership.and_then(|m| m.get("billing_type").and_then(|v| v.as_str())))
        .or_else(|| account.get("subscription_type").and_then(|v| v.as_str()));
    let rate_limit_tier = membership
        .and_then(|m| m.get("rate_limit_tier").and_then(|v| v.as_str()))
        .or_else(|| account.get("rate_limit_tier").and_then(|v| v.as_str()));

    claude_plan_label(seat_tier, rate_limit_tier)
}

/// Clean a raw plan token (strip claude/ai prefixes, normalize separators).
fn clean_plan_text(text: &str, prefixes: &[&str]) -> String {
    let raw = text.trim();
    if raw.is_empty() || raw.contains('@') {
        return String::new();
    }
    let mut clean = raw.to_string();
    // Strip leading prefixes like "claude_" / "claude-" / "claude ".
    loop {
        let lower = clean.to_lowercase();
        let mut stripped = false;
        for p in prefixes {
            for sep in ['_', '-', ' '] {
                let pat = format!("{p}{sep}");
                if lower.starts_with(&pat) {
                    clean = clean[pat.len()..].to_string();
                    stripped = true;
                    break;
                }
            }
            if stripped {
                break;
            }
        }
        if !stripped {
            break;
        }
    }
    clean
        .replace(['_', '-'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Map a cleaned plan token to a display label (token-monitor planLabelFromParts).
fn plan_alias(raw: &str) -> Option<String> {
    match raw {
        "free" => Some("Free".into()),
        "plus" => Some("Plus".into()),
        "pro" => Some("Pro".into()),
        "max" => Some("Max".into()),
        "team" | "teams" => Some("Team".into()),
        "enterprise" => Some("Enterprise".into()),
        "ultra" => Some("Ultra".into()),
        _ => None,
    }
}

fn plan_label_from_parts(text: &str) -> String {
    let raw = clean_plan_text(text, &["claude", "chatgpt", "openai"]);
    if raw.is_empty() {
        return String::new();
    }
    if let Some(alias) = plan_alias(&raw) {
        return alias;
    }
    // Title-case each word for display.
    raw.split_whitespace()
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Rate-limit tier label: strip "default"/"claude"/"ai" words (e.g.
/// "default_claude_max_20x" → "Max 20x").
fn rate_limit_tier_label(tier: &str) -> String {
    let raw = clean_plan_text(tier, &[]);
    if raw.is_empty() {
        return String::new();
    }
    let words: Vec<&str> = raw
        .split_whitespace()
        .filter(|w| !matches!(*w, "default" | "claude" | "ai"))
        .collect();
    if words.is_empty() {
        return String::new();
    }
    plan_label_from_parts(&words.join(" "))
}

/// Combine subscription type + rate-limit tier into the display plan label.
/// Mirrors token-monitor `claudePlanLabelFromParts`.
fn claude_plan_label(
    subscription_type: Option<&str>,
    rate_limit_tier: Option<&str>,
) -> Option<String> {
    let subscription_label = subscription_type
        .map(plan_label_from_parts)
        .unwrap_or_default();
    let tier_label = rate_limit_tier
        .map(rate_limit_tier_label)
        .unwrap_or_default();
    // "Max" + "Max 5x/20x" → prefer the more specific tier label.
    if subscription_label == "Max" && (tier_label == "Max 5x" || tier_label == "Max 20x") {
        return Some(tier_label);
    }
    let result = if !subscription_label.is_empty() {
        subscription_label
    } else {
        tier_label
    };
    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

fn select_best_org(orgs: &[serde_json::Value]) -> Result<String, VendorError> {
    if orgs.is_empty() {
        return Err(VendorError::Parse("no organizations found".into()));
    }

    // Score each org: chat capability = best, api-only = worst, none = middle.
    let mut best: Option<(usize, &str)> = None;
    for org in orgs {
        let id = org
            .get("uuid")
            .and_then(|v| v.as_str())
            .or_else(|| org.get("id").and_then(|v| v.as_str()))
            .or_else(|| org.get("organization_uuid").and_then(|v| v.as_str()))
            .ok_or_else(|| VendorError::Parse("org missing id".into()))?;

        let caps = org.get("capabilities").and_then(|c| c.as_array());
        let has_chat = caps
            .map(|arr| {
                arr.iter().any(|c| {
                    c.as_str()
                        .map(|s| s.to_lowercase() == "chat")
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);
        let is_api_only = caps
            .map(|arr| {
                arr.len() == 1
                    && arr[0]
                        .as_str()
                        .map(|s| s.to_lowercase() == "api")
                        .unwrap_or(false)
            })
            .unwrap_or(false);

        let score = if has_chat {
            2
        } else if is_api_only {
            0
        } else {
            1
        };
        if best.map(|b| score > b.0).unwrap_or(true) {
            best = Some((score, id));
        }
    }

    best.map(|b| b.1.to_string())
        .ok_or_else(|| VendorError::Parse("no organizations found".into()))
}

// ---------------------------------------------------------------------------
// Response parsing (shared by both paths)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct UsageResponse {
    #[serde(rename = "five_hour", alias = "fiveHour")]
    five_hour: Option<QuotaWindowData>,
    #[serde(rename = "seven_day", alias = "sevenDay")]
    seven_day: Option<QuotaWindowData>,
}

#[derive(Debug, Deserialize)]
struct QuotaWindowData {
    /// OAuth path returns `usedPercent`; web path returns `utilization` / `percent`.
    /// token-monitor `claudeUsageWindowUsedPercent` tries these in order.
    #[serde(rename = "usedPercent", alias = "used_percent", default)]
    used_percent: Option<f64>,
    #[serde(default)]
    utilization: Option<f64>,
    #[serde(default)]
    percent: Option<f64>,
    #[serde(rename = "resets_at", alias = "resetsAt", default)]
    resets_at: Option<String>,
}

fn window_used_pct(data: &QuotaWindowData) -> f64 {
    // token-monitor order: usedPercent → used_percent → utilization → percent.
    data.used_percent
        .or(data.utilization)
        .or(data.percent)
        .unwrap_or(0.0)
}

fn pct_to_used(pct: f64) -> f64 {
    pct.clamp(0.0, 100.0)
}

#[derive(Debug, Deserialize)]
struct ProfileResponse {
    account: Option<AccountData>,
}

#[derive(Debug, Deserialize)]
struct AccountData {
    #[serde(rename = "subscription_type")]
    subscription_type: Option<String>,
    #[serde(rename = "rate_limit_tier")]
    rate_limit_tier: Option<String>,
}

fn plan_label_from_profile(body: &str) -> Option<String> {
    let profile: ProfileResponse = serde_json::from_str(body).ok()?;
    let account = profile.account.as_ref()?;
    claude_plan_label(
        account.subscription_type.as_deref(),
        account.rate_limit_tier.as_deref(),
    )
}

fn parse_usage_response(body: &str) -> Result<Quota, VendorError> {
    let usage: UsageResponse =
        serde_json::from_str(body).map_err(|e| VendorError::Parse(e.to_string()))?;

    let mut windows: Vec<QuotaWindow> = Vec::new();
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    if let Some(ref sf) = usage.five_hour {
        let resets_ms = sf
            .resets_at
            .as_ref()
            .and_then(|s| parse_reset_ms(s))
            .unwrap_or(now_ms + 5 * 60 * 60 * 1000);
        windows.push(QuotaWindow {
            label: "5h".into(),
            used_pct: pct_to_used(window_used_pct(sf)),
            resets_at: epoch_to_iso(resets_ms as f64),
            ..Default::default()
        });
    }

    if let Some(ref sd) = usage.seven_day {
        let resets_ms = sd
            .resets_at
            .as_ref()
            .and_then(|s| parse_reset_ms(s))
            .unwrap_or(now_ms + 7 * 24 * 60 * 60 * 1000);
        windows.push(QuotaWindow {
            label: "周".into(),
            used_pct: pct_to_used(window_used_pct(sd)),
            resets_at: epoch_to_iso(resets_ms as f64),
            ..Default::default()
        });
    }

    if windows.is_empty() {
        return Err(VendorError::Empty);
    }

    let used_pct = windows.iter().map(|w| w.used_pct).fold(0.0f64, f64::max);

    Ok(Quota {
        vendor: "claude".into(),
        plan_label: None,
        status: QuotaStatus::from_used_pct(used_pct),
        windows,
        balance: None,
        refreshed_at: None,
        error: None,
        cookie_error: None,
        expires_at: None,
    })
}

fn parse_reset_ms(s: &str) -> Option<i64> {
    if let Ok(ts) = s.parse::<i64>() {
        if ts > 1_000_000_000_000 {
            return Some(ts);
        }
        if ts > 0 {
            return Some(ts * 1000);
        }
    }
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.timestamp_millis())
}

// ---------------------------------------------------------------------------
// Main fetch
// ----------------------------------------------------------------------------

pub fn fetch_with(http: &dyn Http, credential: &str) -> Result<Quota, VendorError> {
    let (kind, token, refresh_token) = parse_credential_with_refresh(credential);

    match kind {
        CredKind::WebCookie => {
            // Web cookie path: claude.ai/api/organizations/{id}/usage
            fetch_web_limits(http, &token)
        }
        CredKind::OAuth => {
            // OAuth path: api.anthropic.com/api/oauth/usage
            let mut access_token = token;
            if access_token.is_empty() {
                return Err(VendorError::Parse("缺少 access token".into()));
            }

            let usage_body = match call_oauth_usage(http, &access_token) {
                Ok(body) => body,
                Err(VendorError::Auth(_)) | Err(VendorError::Api { status: 401, .. }) => {
                    let rt = refresh_token.as_ref().ok_or_else(|| {
                        VendorError::Auth("access token expired，未提供 refresh token".into())
                    })?;
                    access_token = refresh_access_token(http, rt)?;
                    call_oauth_usage(http, &access_token)?
                }
                Err(e) => return Err(e),
            };

            let mut q = parse_usage_response(&usage_body)?;

            // Plan label from profile API (best-effort).
            let plan_label = call_oauth_profile(http, &access_token)
                .ok()
                .as_ref()
                .and_then(|body| plan_label_from_profile(body));
            q.plan_label = plan_label;

            Ok(q)
        }
    }
}

fn parse_credential_with_refresh(credential: &str) -> (CredKind, String, Option<String>) {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(credential) {
        let access = v
            .get("access_token")
            .and_then(|k| k.as_str())
            .map(|s| s.to_string());
        let refresh = v
            .get("refresh_token")
            .and_then(|k| k.as_str())
            .map(|s| s.to_string());
        if access.is_some() || refresh.is_some() {
            return (CredKind::OAuth, access.unwrap_or_default(), refresh);
        }
        if let Some(cookie) = v.get("cookie").and_then(|c| c.as_str()) {
            let normalized = normalize_web_cookie(cookie);
            if !normalized.is_empty() {
                return (CredKind::WebCookie, normalized, None);
            }
        }
    }
    let normalized = normalize_web_cookie(credential.trim());
    if !normalized.is_empty() {
        return (CredKind::WebCookie, normalized, None);
    }
    (CredKind::OAuth, credential.trim().to_string(), None)
}

fn call_oauth_usage(http: &dyn Http, token: &str) -> Result<String, VendorError> {
    http.get_bearer(OAUTH_USAGE_URL, token)
}

fn call_oauth_profile(http: &dyn Http, token: &str) -> Result<String, VendorError> {
    http.get_bearer(OAUTH_PROFILE_URL, token)
}

// ---------------------------------------------------------------------------
// Ureq HTTP impl
// ---------------------------------------------------------------------------

struct UreqHttp;
impl Http for UreqHttp {
    fn get_bearer(&self, url: &str, token: &str) -> Result<String, VendorError> {
        let resp = crate::utils::http::proxy_agent()
            .get(url)
            .set("Accept", "application/json")
            .set("Authorization", &format!("Bearer {token}"))
            .set("anthropic-beta", "oauth-2025-04-20")
            .set("User-Agent", USER_AGENT)
            .call();
        match resp {
            Ok(r) if r.status() == 200 => r
                .into_string()
                .map_err(|e| VendorError::Network(e.to_string())),
            Ok(r) if r.status() == 401 || r.status() == 403 => {
                Err(VendorError::Auth(format!("status code {}", r.status())))
            }
            Ok(r) => Err(VendorError::Api {
                status: r.status(),
                body: r.into_string().unwrap_or_default(),
            }),
            Err(ureq::Error::Status(code, _)) if code == 401 || code == 403 => {
                Err(VendorError::Auth(format!("status code {code}")))
            }
            Err(e) => Err(VendorError::Network(e.to_string())),
        }
    }

    fn get_cookie(&self, url: &str, session_cookie: &str) -> Result<String, VendorError> {
        let resp = crate::utils::http::proxy_agent()
            .get(url)
            .set("Accept", "application/json")
            .set("Cookie", session_cookie)
            .set("User-Agent", USER_AGENT)
            .call();
        match resp {
            Ok(r) if r.status() == 200 => r
                .into_string()
                .map_err(|e| VendorError::Network(e.to_string())),
            Ok(r) if r.status() == 401 || r.status() == 403 => {
                Err(VendorError::Auth(format!("status code {}", r.status())))
            }
            Ok(r) => Err(VendorError::Api {
                status: r.status(),
                body: r.into_string().unwrap_or_default(),
            }),
            Err(ureq::Error::Status(code, _)) if code == 401 || code == 403 => {
                Err(VendorError::Auth(format!("status code {code}")))
            }
            Err(e) => Err(VendorError::Network(e.to_string())),
        }
    }

    fn post_form(&self, url: &str, body: &str) -> Result<String, VendorError> {
        let resp = crate::utils::http::proxy_agent()
            .post(url)
            .set("Accept", "application/json")
            .set("Content-Type", "application/x-www-form-urlencoded")
            .send_string(body);
        match resp {
            Ok(r) if r.status() == 200 => r
                .into_string()
                .map_err(|e| VendorError::Network(e.to_string())),
            Ok(r) => Err(VendorError::Api {
                status: r.status(),
                body: r.into_string().unwrap_or_default(),
            }),
            Err(e) => Err(VendorError::Network(e.to_string())),
        }
    }
}

// ---------------------------------------------------------------------------
// Async entry point
// ---------------------------------------------------------------------------

pub async fn fetch(credential: &str) -> Result<Quota, VendorError> {
    let cred = credential.to_string();
    tokio::task::spawn_blocking(move || fetch_with(&UreqHttp, &cred))
        .await
        .map_err(|e| VendorError::Network(format!("join: {e}")))?
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    struct MockHttp {
        oauth_usage: Result<String, VendorError>,
        oauth_profile: Result<String, VendorError>,
        web_orgs: Result<String, VendorError>,
        web_usage: Result<String, VendorError>,
        web_account: Result<String, VendorError>,
        refresh: Result<String, VendorError>,
    }

    impl Http for MockHttp {
        fn get_bearer(&self, url: &str, _: &str) -> Result<String, VendorError> {
            if url.contains("/usage") && !url.contains("organizations") {
                self.oauth_usage.clone()
            } else if url.contains("/profile") {
                self.oauth_profile.clone()
            } else {
                Err(VendorError::Network("unknown oauth url".into()))
            }
        }

        fn get_cookie(&self, url: &str, _: &str) -> Result<String, VendorError> {
            if url.contains("/api/account") {
                self.web_account.clone()
            } else if url.contains("/organizations") && !url.contains("/usage") {
                self.web_orgs.clone()
            } else if url.contains("/usage") {
                self.web_usage.clone()
            } else {
                Err(VendorError::Network("unknown web url".into()))
            }
        }

        fn post_form(&self, _: &str, _: &str) -> Result<String, VendorError> {
            self.refresh.clone()
        }
    }

    #[test]
    fn normalize_web_cookie_accepts_bare_token() {
        assert_eq!(
            normalize_web_cookie("sk-ant-abc123"),
            "sessionKey=sk-ant-abc123"
        );
    }

    #[test]
    fn normalize_web_cookie_accepts_prefix() {
        assert_eq!(
            normalize_web_cookie("sessionKey=sk-ant-abc123"),
            "sessionKey=sk-ant-abc123"
        );
    }

    #[test]
    fn normalize_web_cookie_rejects_semicolons() {
        assert!(normalize_web_cookie("sk-ant-abc; other=val").is_empty());
    }

    #[test]
    fn normalize_web_cookie_rejects_whitespace() {
        assert!(normalize_web_cookie("sk-ant-abc def").is_empty());
    }

    #[test]
    fn parse_credential_oauth_json() {
        let (kind, token, _) =
            parse_credential_with_refresh(r#"{"access_token":"at","refresh_token":"rt"}"#);
        assert_eq!(kind, CredKind::OAuth);
        assert_eq!(token, "at");
    }

    #[test]
    fn parse_credential_cookie_field() {
        let (kind, token, _) = parse_credential_with_refresh(r#"{"cookie":"sk-ant-session-key"}"#);
        assert_eq!(kind, CredKind::WebCookie);
        assert_eq!(token, "sessionKey=sk-ant-session-key");
    }

    #[test]
    fn parse_credential_sessionkey_prefix() {
        let (kind, token, _) = parse_credential_with_refresh("sessionKey=sk-ant-raw");
        assert_eq!(kind, CredKind::WebCookie);
        assert_eq!(token, "sessionKey=sk-ant-raw");
    }

    #[test]
    fn parse_credential_raw_sk_ant_is_web_cookie() {
        // Bare sk-ant-... is a valid web session key.
        let (kind, token, _) = parse_credential_with_refresh("sk-ant-raw-token");
        assert_eq!(kind, CredKind::WebCookie);
        assert_eq!(token, "sessionKey=sk-ant-raw-token");
    }

    #[test]
    fn fetch_web_cookie_success() {
        // Web path returns `utilization` (not usedPercent) — token-monitor format.
        let mock = MockHttp {
            oauth_usage: Err(VendorError::Network("not called".into())),
            oauth_profile: Err(VendorError::Network("not called".into())),
            web_orgs: Ok(r#"[{"uuid":"org-123","capabilities":["chat"]}]"#.into()),
            web_usage: Ok(r#"{"five_hour":{"utilization":30,"resets_at":"2030-01-15T10:00:00Z"},"seven_day":{"utilization":60,"resets_at":"2030-01-22T10:00:00Z"}}"#.into()),
            web_account: Err(VendorError::Network("not called".into())),
            refresh: Err(VendorError::Network("not called".into())),
        };
        let q = fetch_with(&mock, "sk-ant-session-key").unwrap();
        assert_eq!(q.vendor, "claude");
        assert_eq!(q.windows.len(), 2);
        assert_eq!(q.windows[0].label, "5h");
        assert!((q.windows[0].used_pct - 30.0).abs() < 1e-6);
        assert_eq!(q.windows[1].label, "周");
        assert!((q.windows[1].used_pct - 60.0).abs() < 1e-6);
    }

    #[test]
    fn fetch_web_cookie_percent_field() {
        // Some responses use `percent` instead of `utilization`.
        let mock = MockHttp {
            oauth_usage: Err(VendorError::Network("not called".into())),
            oauth_profile: Err(VendorError::Network("not called".into())),
            web_orgs: Ok(r#"[{"uuid":"org-123","capabilities":["chat"]}]"#.into()),
            web_usage: Ok(r#"{"five_hour":{"percent":42}}"#.into()),
            web_account: Err(VendorError::Network("not called".into())),
            refresh: Err(VendorError::Network("not called".into())),
        };
        let q = fetch_with(&mock, "sk-ant-session-key").unwrap();
        assert!((q.windows[0].used_pct - 42.0).abs() < 1e-6);
    }

    #[test]
    fn fetch_web_cookie_with_organizations_wrapper() {
        // Some responses wrap in {organizations: [...]}
        let mock = MockHttp {
            oauth_usage: Err(VendorError::Network("not called".into())),
            oauth_profile: Err(VendorError::Network("not called".into())),
            web_orgs: Ok(
                r#"{"organizations":[{"uuid":"org-456","capabilities":["chat"]}]}"#.into(),
            ),
            web_usage: Ok(
                r#"{"five_hour":{"utilization":10},"seven_day":{"utilization":20}}"#.into(),
            ),
            web_account: Err(VendorError::Network("not called".into())),
            refresh: Err(VendorError::Network("not called".into())),
        };
        let q = fetch_with(&mock, "sk-ant-session-key").unwrap();
        assert_eq!(q.windows.len(), 2);
    }

    #[test]
    fn fetch_web_cookie_selects_chat_org() {
        // Multiple orgs: prefer chat capability.
        let mock = MockHttp {
            oauth_usage: Err(VendorError::Network("not called".into())),
            oauth_profile: Err(VendorError::Network("not called".into())),
            web_orgs: Ok(r#"[{"uuid":"org-api","capabilities":["api"]},{"uuid":"org-chat","capabilities":["chat"]}]"#.into()),
            web_usage: Ok(r#"{"five_hour":{"utilization":5}}"#.into()),
            web_account: Err(VendorError::Network("not called".into())),
            refresh: Err(VendorError::Network("not called".into())),
        };
        let q = fetch_with(&mock, "sk-ant-session-key").unwrap();
        assert_eq!(q.windows.len(), 1);
        assert!((q.windows[0].used_pct - 5.0).abs() < 1e-6);
    }

    #[test]
    fn fetch_web_cookie_auth_failure() {
        let mock = MockHttp {
            oauth_usage: Err(VendorError::Network("not called".into())),
            oauth_profile: Err(VendorError::Network("not called".into())),
            web_orgs: Err(VendorError::Auth("status code 401".into())),
            web_usage: Err(VendorError::Network("not called".into())),
            web_account: Err(VendorError::Network("not called".into())),
            refresh: Err(VendorError::Network("not called".into())),
        };
        let err = fetch_with(&mock, "sk-ant-expired").unwrap_err();
        match err {
            VendorError::Auth(_) => {}
            _ => panic!("expected Auth error, got: {err:?}"),
        }
    }

    #[test]
    fn fetch_oauth_success() {
        let mock = MockHttp {
            oauth_usage: Ok(r#"{"five_hour":{"usedPercent":20,"resets_at":"2030-01-15T10:00:00Z"},"seven_day":{"usedPercent":40,"resets_at":"2030-01-22T10:00:00Z"}}"#.into()),
            oauth_profile: Ok(r#"{"account":{"subscription_type":"Max"}}"#.into()),
            web_orgs: Err(VendorError::Network("not called".into())),
            web_usage: Err(VendorError::Network("not called".into())),
            web_account: Err(VendorError::Network("not called".into())),
            refresh: Err(VendorError::Network("not called".into())),
        };
        let json = r#"{"access_token":"my-token"}"#;
        let q = fetch_with(&mock, json).unwrap();
        assert_eq!(q.plan_label.as_deref(), Some("Max"));
        assert_eq!(q.windows.len(), 2);
    }

    #[test]
    fn fetch_oauth_refreshes_on_401() {
        use std::sync::atomic::Ordering;

        struct CountingMock {
            usages: Vec<Result<String, VendorError>>,
            profile: Result<String, VendorError>,
            refresh: Result<String, VendorError>,
        }
        impl Http for CountingMock {
            fn get_bearer(&self, url: &str, _: &str) -> Result<String, VendorError> {
                if url.contains("/usage") {
                    let n = USAGE_CALL.with(|c| c.fetch_add(1, Ordering::SeqCst));
                    self.usages
                        .get(n)
                        .cloned()
                        .unwrap_or(Err(VendorError::Network("no more usages".into())))
                } else if url.contains("/profile") {
                    self.profile.clone()
                } else {
                    Err(VendorError::Network("unknown oauth url".into()))
                }
            }
            fn get_cookie(&self, _: &str, _: &str) -> Result<String, VendorError> {
                Err(VendorError::Network("not called".into()))
            }
            fn post_form(&self, _: &str, _: &str) -> Result<String, VendorError> {
                self.refresh.clone()
            }
        }

        thread_local! {
                #[allow(clippy::missing_const_for_thread_local)]

            static USAGE_CALL: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        }

        let mock = CountingMock {
            usages: vec![
                Err(VendorError::Auth("status code 401".into())),
                Ok(r#"{"five_hour":{"usedPercent":15,"resets_at":"2030-01-15T10:00:00Z"},"seven_day":{"usedPercent":25,"resets_at":"2030-01-22T10:00:00Z"}}"#.into()),
            ],
            profile: Ok(r#"{"account":{"subscription_type":"Max"}}"#.into()),
            refresh: Ok(r#"{"access_token":"new-access-token"}"#.into()),
        };
        let json = r#"{"access_token":"expired","refresh_token":"rt"}"#;
        let q = fetch_with(&mock, json).unwrap();
        assert_eq!(q.plan_label.as_deref(), Some("Max"));
        assert_eq!(q.windows.len(), 2);
    }

    #[test]
    fn plan_label_account_basic() {
        let account = r#"{"memberships":[{
            "organization":{"uuid":"org-1"},
            "seat_tier":"max",
            "rate_limit_tier":"default_claude_max_20x"
        }]}"#;
        assert_eq!(
            plan_label_from_account(account, "org-1").as_deref(),
            Some("Max 20x")
        );
    }

    #[test]
    fn plan_label_account_pro() {
        let account = r#"{"memberships":[{
            "organization":{"uuid":"org-1"},
            "seat_tier":"pro"
        }]}"#;
        assert_eq!(
            plan_label_from_account(account, "org-1").as_deref(),
            Some("Pro")
        );
    }

    #[test]
    fn plan_label_account_free() {
        let account = r#"{"memberships":[{
            "organization":{"uuid":"org-1"},
            "seat_tier":"free"
        }]}"#;
        assert_eq!(
            plan_label_from_account(account, "org-1").as_deref(),
            Some("Free")
        );
    }

    #[test]
    fn plan_label_account_missing() {
        let account = r#"{"uuid":"account-1","memberships":[]}"#;
        assert_eq!(plan_label_from_account(account, "org-1"), None);
    }

    #[test]
    fn plan_label_profile_with_rate_limit() {
        let body = r#"{"account":{"subscription_type":"claude_max","rate_limit_tier":"default_claude_max_5x"}}"#;
        assert_eq!(plan_label_from_profile(body).as_deref(), Some("Max 5x"));
    }

    #[test]
    fn plan_label_profile_pro() {
        let body = r#"{"account":{"subscription_type":"claude_pro"}}"#;
        assert_eq!(plan_label_from_profile(body).as_deref(), Some("Pro"));
    }

    #[test]
    fn clean_plan_text_strips_prefixes() {
        assert_eq!(clean_plan_text("claude_max", &["claude"]), "max");
        assert_eq!(clean_plan_text("claude-pro", &["claude"]), "pro");
    }

    #[test]
    fn rate_limit_tier_label_extracts_number() {
        assert_eq!(rate_limit_tier_label("default_claude_max_20x"), "Max 20x");
        assert_eq!(rate_limit_tier_label("default_claude_max_5x"), "Max 5x");
    }

    #[test]
    fn fetch_web_with_plan_label() {
        let mock = MockHttp {
            oauth_usage: Err(VendorError::Network("not called".into())),
            oauth_profile: Err(VendorError::Network("not called".into())),
            web_orgs: Ok(r#"[{"uuid":"org-1","capabilities":["chat"]}]"#.into()),
            web_usage: Ok(r#"{"five_hour":{"utilization":10}}"#.into()),
            web_account: Ok(r#"{"memberships":[{"organization":{"uuid":"org-1"},"seat_tier":"max","rate_limit_tier":"default_claude_max_20x"}]}"#.into()),
            refresh: Err(VendorError::Network("not called".into())),
        };
        let q = fetch_with(&mock, "sk-ant-session-key").unwrap();
        assert_eq!(q.plan_label.as_deref(), Some("Max 20x"));
    }
}
