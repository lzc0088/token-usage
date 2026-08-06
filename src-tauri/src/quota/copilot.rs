//! GitHub Copilot quota + OAuth Device Flow login.
//!
//! Two concerns:
//! 1. **Quota fetch** — `GET /copilot_internal/user` with the GitHub OAuth token
//!    (obtained once via Device Flow). Returns `quota_snapshots` (premium +
//!    chat) which we map to two `billing` windows.
//! 2. **Device Flow login** (RFC 8628) — `request_device_code` → user authorizes
//!    in browser → `poll_access_token`. The login command emits a
//!    `copilot:login_status` event carrying `{userCode, verificationUrl}` so the
//!    frontend can open the browser (shell plugin) + show the code; it then
//!    polls until the user finishes and returns the access token.
//!
//! Faithfully ported from token-monitor src/shared/copilotLimits.js +
//! copilotDeviceFlow.js. Client ID `Iv1.b507a08c87ecfe98` is a public OAuth App.

use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager};

use super::types::{parse_iso, Quota, QuotaStatus, QuotaWindow};
use super::VendorError;

const COPILOT_DEVICE_CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";
const COPILOT_DEVICE_SCOPE: &str = "read:user";
const DEFAULT_POLL_INTERVAL_SECS: u64 = 5;
const SLOW_DOWN_EXTRA_MS: u64 = 5_000;

const DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
const ACCESS_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const USAGE_URL: &str = "https://api.github.com/copilot_internal/user";

const USER_AGENT: &str = "GitHubCopilotChat/0.26.7";

/// Storage for the device code between Phase 1 (request) and Phase 2 (poll).
/// Set in `request_device_code_info`, consumed in `poll_for_access_token`.
fn device_code_storage() -> &'static Mutex<Option<DeviceCodeResp>> {
    DEVICE_CODE_STORAGE.get_or_init(|| Mutex::new(None))
}

static DEVICE_CODE_STORAGE: OnceLock<Mutex<Option<DeviceCodeResp>>> = OnceLock::new();

// ── Quota fetch ─────────────────────────────────────────────────────────────

/// HTTP client. Injected for unit tests.
pub trait Http {
    fn get_token(&self, url: &str, token: &str) -> Result<String, VendorError>;
}

/// One entry in `quota_snapshots` (only the fields we consume).
fn decode_num(v: Option<&serde_json::Value>) -> Option<f64> {
    match v {
        Some(serde_json::Value::Number(n)) => n.as_f64(),
        Some(serde_json::Value::String(s)) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

/// Parse `quota_reset_date` — accepts RFC3339 or date-only `YYYY-MM-DD`
/// (token-monitor parseQuotaResetDate).
fn parse_reset_date(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if let Some(iso) = parse_iso(raw) {
        return Some(iso);
    }
    // date-only YYYY-MM-DD → midnight UTC.
    let date = chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d").ok()?;
    date.and_hms_opt(0, 0, 0)
        .and_then(|dt| dt.and_utc().to_rfc3339().into())
}

/// Derive (percent_remaining, unlimited) from a snapshot, mirroring token-monitor
/// `parseQuotaSnapshot` (entitlement/remaining fallback).
fn snapshot_percent(raw: &serde_json::Value) -> Option<(f64, bool)> {
    let obj = raw.as_object()?;
    let unlimited = obj
        .get("unlimited")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if unlimited {
        return Some((100.0, true));
    }
    let percent_remaining = decode_num(
        obj.get("percent_remaining")
            .or_else(|| obj.get("percentRemaining")),
    );
    if let Some(p) = percent_remaining {
        return Some((p, false));
    }
    let entitlement = decode_num(obj.get("entitlement")).unwrap_or(0.0);
    let remaining = decode_num(obj.get("remaining")).unwrap_or(0.0);
    if entitlement > 0.0 {
        return Some((((remaining / entitlement) * 100.0).clamp(0.0, 100.0), false));
    }
    None
}

/// Classify a dynamic quota key (token-monitor classifyDynamicQuotaKey).
fn classify_key(key: &str) -> &'static str {
    let name = key.to_lowercase();
    if name.contains("chat") {
        "chat"
    } else if name.contains("premium") || name.contains("completion") || name.contains("code") {
        "premium"
    } else {
        "other"
    }
}

/// Parsed usage result: (premium_used_pct, chat_used_pct, quota_reset_date).
pub type UsageResult = (Option<f64>, Option<f64>, Option<String>);

/// Parse the `/copilot_internal/user` payload → (premium_pct, chat_pct, reset_date).
pub fn parse(body: &str) -> Result<UsageResult, VendorError> {
    let root: serde_json::Value =
        serde_json::from_str(body).map_err(|e| VendorError::Parse(e.to_string()))?;
    let snapshots = root
        .get("quota_snapshots")
        .or_else(|| root.get("quotaSnapshots"))
        .and_then(|v| v.as_object());

    let mut premium: Option<(f64, bool)> = None;
    let mut chat: Option<(f64, bool)> = None;
    if let Some(obj) = snapshots {
        // Direct keys first.
        for (key, value) in obj {
            if let Some(sp) = snapshot_percent(value) {
                match (classify_key(key), premium.is_some(), chat.is_some()) {
                    ("premium", false, _) => premium = Some(sp),
                    ("chat", _, false) => chat = Some(sp),
                    _ => {}
                }
            }
        }
    }
    let reset_date = root
        .get("quota_reset_date")
        .or_else(|| root.get("quotaResetDate"))
        .and_then(|v| v.as_str())
        .and_then(parse_reset_date);

    let to_used = |sp: Option<(f64, bool)>| match sp {
        Some((remaining_pct, unlimited)) => {
            if unlimited {
                Some(0.0)
            } else {
                Some((100.0 - remaining_pct).clamp(0.0, 100.0))
            }
        }
        None => None,
    };
    Ok((to_used(premium), to_used(chat), reset_date))
}

/// Fetch via `http`. `credential` is the raw OAuth token (or `{"key": ...}`).
pub fn fetch_with(http: &dyn Http, credential: &str) -> Result<Quota, VendorError> {
    let token = super::extract_key(credential);
    super::validate_header_safe(&token)?;
    let body = http.get_token(USAGE_URL, &token)?;
    let (premium_pct, chat_pct, resets_at) = parse(&body)?;

    let mut windows: Vec<QuotaWindow> = Vec::new();
    if let Some(p) = premium_pct {
        windows.push(QuotaWindow {
            label: "Premium".into(),
            used_pct: p,
            resets_at: resets_at.clone(),
            ..Default::default()
        });
    }
    if let Some(c) = chat_pct {
        windows.push(QuotaWindow {
            label: "Chat".into(),
            used_pct: c,
            resets_at: resets_at.clone(),
            ..Default::default()
        });
    }
    if windows.is_empty() {
        return Err(VendorError::Empty);
    }
    let status = QuotaStatus::worst_of(
        windows
            .iter()
            .map(|w| QuotaStatus::from_used_pct(w.used_pct)),
    );
    Ok(Quota {
        vendor: "copilot".into(),
        plan_label: None,
        status,
        windows,
        balance: None,
        refreshed_at: None,
        error: None,
        cookie_error: None,
        expires_at: None,
    })
}

pub async fn fetch(credential: &str) -> Result<Quota, VendorError> {
    let cred = credential.to_string();
    tokio::task::spawn_blocking(move || fetch_with(&UreqHttp, &cred))
        .await
        .map_err(|e| VendorError::Network(format!("join: {e}")))?
}

struct UreqHttp;
impl Http for UreqHttp {
    fn get_token(&self, url: &str, token: &str) -> Result<String, VendorError> {
        let resp = ureq::get(url)
            .set("Accept", "application/json")
            .set("Authorization", &format!("token {token}"))
            .set("Editor-Version", "vscode/1.96.2")
            .set("Editor-Plugin-Version", "copilot-chat/0.26.7")
            .set("User-Agent", USER_AGENT)
            .set("X-Github-Api-Version", "2025-04-01")
            .call();
        match resp {
            Ok(r) => r
                .into_string()
                .map_err(|e| VendorError::Network(e.to_string())),
            Err(ureq::Error::Status(code, _r)) => {
                if code == 401 || code == 403 {
                    Err(VendorError::Network("status code 401".into()))
                } else if code == 429 {
                    Err(VendorError::Network("status code 429".into()))
                } else {
                    Err(VendorError::Network(format!("status code {code}")))
                }
            }
            Err(e) => Err(VendorError::Network(e.to_string())),
        }
    }
}

// ── Device Flow login ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct DeviceCodeResp {
    #[serde(default)]
    device_code: Option<String>,
    #[serde(default, alias = "user_code")]
    user_code: Option<String>,
    #[serde(default, alias = "verification_uri")]
    verification_uri: Option<String>,
    #[serde(default, alias = "verification_uri_complete")]
    verification_uri_complete: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
    #[serde(default)]
    interval: Option<u64>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct LoginStart {
    pub user_code: String,
    pub verification_url: String,
    pub expires_in: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case", tag = "phase")]
pub enum LoginStatus {
    Authorize {
        user_code: String,
        verification_url: String,
        expires_in: u64,
    },
    Polling,
    Success,
}

/// POST a form-urlencoded body and return the JSON response string.
/// Uses the macOS system proxy (from System Settings → Proxies), ignoring any
/// stale HTTPS_PROXY env vars that may point to a dead proxy.
#[allow(clippy::result_large_err)]
fn post_form(url: &str, body: &str) -> Result<String, VendorError> {
    let resp = without_proxy_env(|| {
        // After clearing env-var proxies, proxy_agent_builder() will only pick
        // up the macOS system proxy (scutil --proxy). If none, connects directly.
        crate::utils::http::proxy_agent_builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .post(url)
            .set("Accept", "application/json")
            .set("Content-Type", "application/x-www-form-urlencoded")
            .send_string(body)
    });
    match resp {
        Ok(r) => {
            let s = r
                .into_string()
                .map_err(|e| VendorError::Network(e.to_string()))?;
            Ok(s)
        }
        Err(ureq::Error::Status(code, r)) => {
            let body = r.into_string().unwrap_or_default();
            Err(VendorError::Network(format!("status code {code}: {body}")))
        }
        Err(e) => Err(VendorError::Network(e.to_string())),
    }
}

/// Temporarily remove proxy-related env vars, run `f`, then restore them.
/// This prevents ureq from using stale HTTPS_PROXY env vars while still
/// allowing proxy_agent_builder() to detect the macOS system proxy.
fn without_proxy_env<T>(f: impl FnOnce() -> T) -> T {
    const KEYS: &[&str] = &[
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
        "ALL_PROXY",
        "all_proxy",
    ];
    let saved: Vec<(String, String)> = KEYS
        .iter()
        .filter_map(|&k| std::env::var(k).ok().map(|v| (k.to_string(), v)))
        .collect();
    for &k in KEYS {
        std::env::remove_var(k);
    }
    let result = f();
    for (k, v) in saved {
        std::env::set_var(k, v);
    }
    result
}

fn form_encode(s: &str) -> String {
    // RFC 3986 unreserved + the OAuth-safe extra chars from token-monitor.
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

/// Request a device code from GitHub. Returns the parsed response.
pub fn request_device_code() -> Result<DeviceCodeResp, VendorError> {
    let body = format!(
        "client_id={}&scope={}",
        form_encode(COPILOT_DEVICE_CLIENT_ID),
        form_encode(COPILOT_DEVICE_SCOPE)
    );
    let raw = post_form(DEVICE_CODE_URL, &body)?;
    let resp: DeviceCodeResp =
        serde_json::from_str(&raw).map_err(|e| VendorError::Parse(e.to_string()))?;
    if resp.device_code.is_none() || resp.user_code.is_none() || resp.verification_uri.is_none() {
        return Err(VendorError::Parse(
            "GitHub device code response incomplete".into(),
        ));
    }
    Ok(resp)
}

/// Poll the access-token endpoint until the user authorizes or it expires.
/// Emits `copilot:login_status` events as the flow progresses.
pub async fn poll_access_token(
    device_code: DeviceCodeResp,
    app: &AppHandle,
) -> Result<String, VendorError> {
    let code = device_code.device_code.clone().unwrap_or_default();
    let body = format!(
        "client_id={}&device_code={}&grant_type={}",
        form_encode(COPILOT_DEVICE_CLIENT_ID),
        form_encode(&code),
        form_encode("urn:ietf:params:oauth:grant-type:device_code")
    );
    let started = std::time::Instant::now();
    let deadline = Duration::from_secs(device_code.expires_in.unwrap_or(15 * 60));
    let mut interval_ms = device_code.interval.unwrap_or(DEFAULT_POLL_INTERVAL_SECS) * 1000;

    let _ = app.emit("copilot:login_status", LoginStatus::Polling);
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.emit("copilot:login_status", LoginStatus::Polling);
    }
    loop {
        if started.elapsed() >= deadline {
            return Err(VendorError::Network("GitHub device code expired".into()));
        }
        tokio::time::sleep(Duration::from_millis(interval_ms)).await;

        let raw = post_form(ACCESS_TOKEN_URL, &body)?;
        let v: serde_json::Value =
            serde_json::from_str(&raw).map_err(|e| VendorError::Parse(e.to_string()))?;

        if let Some(token) = v.get("access_token").and_then(|t| t.as_str()) {
            if !token.is_empty() {
                let _ = app.emit("copilot:login_status", LoginStatus::Success);
                if let Some(w) = app.get_webview_window("settings") {
                    let _ = w.emit("copilot:login_status", LoginStatus::Success);
                }
                return Ok(token.to_string());
            }
        }
        let err = v.get("error").and_then(|e| e.as_str()).unwrap_or("");
        match err {
            "authorization_pending" => continue,
            "slow_down" => {
                interval_ms += SLOW_DOWN_EXTRA_MS;
                continue;
            }
            "expired_token" => {
                return Err(VendorError::Network("GitHub device code expired".into()))
            }
            "access_denied" => {
                return Err(VendorError::Network("GitHub sign-in was denied".into()))
            }
            other => {
                let desc = v
                    .get("error_description")
                    .and_then(|d| d.as_str())
                    .unwrap_or(other);
                return Err(VendorError::Network(format!(
                    "GitHub sign-in failed: {desc}"
                )));
            }
        }
    }
}

/// Phase 1 of Device Flow: request device code from GitHub and return
/// the authorize info (user code + verification URL). The frontend displays
/// the code and opens the browser. No events emitted.
pub async fn request_device_code_info() -> Result<LoginStart, VendorError> {
    let code = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        tokio::task::spawn_blocking(request_device_code),
    )
    .await
    .map_err(|_| VendorError::Network("request device code timed out".into()))?
    .map_err(|e| VendorError::Network(format!("join: {e}")))??;
    let user_code = code.user_code.clone().unwrap_or_default();
    let verification_url = code
        .verification_uri_complete
        .clone()
        .or_else(|| code.verification_uri.clone())
        .unwrap_or_default();
    let expires_in = code.expires_in.unwrap_or(15 * 60);

    // Store the raw device code for Phase 2 polling.
    *device_code_storage().lock().unwrap() = Some(code);

    Ok(LoginStart {
        user_code,
        verification_url,
        expires_in,
    })
}

/// Phase 2 of Device Flow: poll for access token. Called after the user has
/// seen the code and authorized in the browser. Returns the access token on success.
pub async fn poll_for_access_token(app: &AppHandle) -> Result<String, VendorError> {
    let code = device_code_storage()
        .lock()
        .unwrap()
        .take()
        .ok_or_else(|| VendorError::Parse("No pending login — call copilot_login first".into()))?;
    poll_access_token(code, app).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_premium_and_chat_snapshots() {
        let body = r#"{"quota_snapshots":{
            "premium_interactions":{"entitlement":100,"remaining":25,"percent_remaining":25},
            "chat":{"entitlement":50,"remaining":50,"percent_remaining":50}
        },"quota_reset_date":"2030-01-15"}"#;
        let (premium, chat, reset) = parse(body).unwrap();
        assert!((premium.unwrap() - 75.0).abs() < 1e-6); // 100-25
        assert!((chat.unwrap() - 50.0).abs() < 1e-6);
        assert!(reset.as_deref().unwrap().starts_with("2030"));
    }

    #[test]
    fn parse_unlimited_snapshot_is_zero_used() {
        let body = r#"{"quota_snapshots":{"chat":{"unlimited":true}}}"#;
        let (_, chat, _) = parse(body).unwrap();
        assert!((chat.unwrap() - 0.0).abs() < 1e-6);
    }

    #[test]
    fn parse_entitlement_remaining_fallback() {
        // No percent_remaining → derive from entitlement/remaining.
        let body =
            r#"{"quota_snapshots":{"premium_interactions":{"entitlement":200,"remaining":50}}}"#;
        let (premium, _, _) = parse(body).unwrap();
        assert!((premium.unwrap() - 75.0).abs() < 1e-6); // (1 - 50/200)*100 = 75 remaining → 25 used
    }

    #[test]
    fn parse_classifies_dynamic_keys() {
        let body = r#"{"quota_snapshots":{
            "code_completion_quota":{"entitlement":100,"remaining":80,"percent_remaining":80}
        }}"#;
        let (premium, chat, _) = parse(body).unwrap();
        // "code" → premium bucket; used = 100-80 = 20.
        assert!((premium.unwrap() - 20.0).abs() < 1e-6);
        assert!(chat.is_none());
    }

    #[test]
    fn parse_empty_snapshots_returns_none() {
        // No usable snapshots → parse returns Ok with all-None; the Empty error
        // is surfaced by fetch_with when windows end up empty.
        let (premium, chat, reset) = parse(r#"{"quota_snapshots":{}}"#).unwrap();
        assert!(premium.is_none());
        assert!(chat.is_none());
        assert!(reset.is_none());
    }

    #[test]
    fn fetch_with_errors_on_empty_snapshots() {
        struct Mock;
        impl Http for Mock {
            fn get_token(&self, _: &str, _: &str) -> Result<String, VendorError> {
                Ok(r#"{"quota_snapshots":{}}"#.into())
            }
        }
        assert!(matches!(
            fetch_with(&Mock, "gho_token"),
            Err(VendorError::Empty)
        ));
    }

    #[test]
    fn fetch_with_returns_windows() {
        struct Mock;
        impl Http for Mock {
            fn get_token(&self, _: &str, _: &str) -> Result<String, VendorError> {
                Ok(r#"{"quota_snapshots":{
                    "premium_interactions":{"percent_remaining":40},
                    "chat":{"percent_remaining":90}
                }}"#
                .into())
            }
        }
        let q = fetch_with(&Mock, "gho_token").unwrap();
        assert_eq!(q.vendor, "copilot");
        assert_eq!(q.plan_label, None);
        assert_eq!(q.windows.len(), 2);
        let labels: Vec<&str> = q.windows.iter().map(|w| w.label.as_str()).collect();
        assert_eq!(labels, vec!["Premium", "Chat"]);
        assert!((q.windows[0].used_pct - 60.0).abs() < 1e-6);
    }

    #[test]
    fn form_encode_encodes_special_chars() {
        assert_eq!(form_encode("a b&c"), "a%20b%26c");
        assert_eq!(
            form_encode("urn:ietf:params:oauth"),
            "urn%3Aietf%3Aparams%3Aoauth"
        );
    }
}
