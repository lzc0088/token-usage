//! Ollama Cloud subscription adapter (HTML-scraped).
//!
//! Ollama Cloud exposes no JSON usage API; limits are read from the rendered
//! `https://ollama.com/settings` page. We GET it with the user's session cookie
//! and regex-extract the "Session usage" / "Weekly usage" meters, reset times,
//! plan badge, and account email.
//!
//! Faithfully ported from token-monitor src/shared/ollamaLimits.js.

use regex::Regex;

use super::types::{parse_iso, Quota, QuotaStatus, QuotaWindow};
use super::VendorError;

const SETTINGS_URL: &str = "https://ollama.com/settings";
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36";

/// Recognized Ollama session cookie names. The cookie header must contain at
/// least one of these (or a `next-auth.session-token.*` variant) to be accepted.
const SESSION_COOKIE_NAMES: &[&str] = &[
    "session",
    "__Secure-session",
    "ollama_session",
    "__Host-ollama_session",
    "wos-session",
    "__Secure-next-auth.session-token",
    "next-auth.session-token",
];

fn is_recognized_session_cookie(name: &str) -> bool {
    if SESSION_COOKIE_NAMES.contains(&name) {
        return true;
    }
    name.starts_with("__Secure-next-auth.session-token.")
        || name.starts_with("next-auth.session-token.")
}

/// Parse a `name=value; name=value` cookie header into validated pairs.
fn cookie_pairs(header: &str) -> Vec<(String, String)> {
    header.split(';').filter_map(|part| {
        let sep = part.find('=')?;
        if sep == 0 {
            return None;
        }
        let name = part[..sep].trim();
        let value = part[sep + 1..].trim();
        let valid_name = !name.is_empty()
            && name.chars().all(|c| c.is_ascii_alphanumeric() || "!#$%&'*+.^_`|~-".contains(c));
        let valid_value = !value.is_empty() && !value.chars().any(|c| c.is_control());
        if valid_name && valid_value {
            Some((name.to_string(), value.to_string()))
        } else {
            None
        }
    }).collect()
}

/// Strip a leading `Cookie:` prefix and surrounding quotes.
fn clean_secret(raw: &str) -> String {
    let trimmed = raw.trim();
    let unquoted = if (trimmed.starts_with('"') && trimmed.ends_with('"'))
        || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
    {
        trimmed[1..trimmed.len() - 1].trim()
    } else {
        trimmed
    };
    let re = Regex::new(r"(?i)^cookie\s*:").unwrap();
    re.replace(unquoted, "").trim().to_string()
}

/// Keep the cookie header only if it contains a recognized session cookie.
/// Returns the normalized `name=value; ...` form, or empty when no recognized
/// session cookie is present (caller rejects).
fn normalize_cookie(raw: &str) -> String {
    let header = clean_secret(raw);
    if header.is_empty() {
        return String::new();
    }
    let pairs = cookie_pairs(&header);
    if pairs.iter().any(|(n, _)| is_recognized_session_cookie(n)) {
        pairs
            .into_iter()
            .map(|(n, v)| format!("{n}={v}"))
            .collect::<Vec<_>>()
            .join("; ")
    } else {
        String::new()
    }
}

/// One parsed usage meter (Session / Weekly).
struct ParsedWindow {
    label: &'static str,
    used_pct: f64,
    resets_at: Option<String>,
    is_weekly: bool,
}

/// Extract Session / Hourly / Weekly usage windows from the settings HTML.
/// Mirrors `parseOllamaUsageHtml`: for each meter label, scan up to 4000 chars
/// for `X% used` (or `width: X%` fallback) and a `data-time` reset marker.
fn parse_usage_html(html: &str) -> Vec<ParsedWindow> {
    let label_re = Regex::new(r"(?i)(Session usage|Hourly usage|Weekly usage)").unwrap();
    let pct_re = Regex::new(r"(?i)([0-9]+(?:\.[0-9]+)?)\s*%\s*used").unwrap();
    let width_re = Regex::new(r"(?i)width\s*:\s*([0-9]+(?:\.[0-9]+)?)\s*%").unwrap();
    let data_time_re = Regex::new(r#"(?i)data-time=["']([^"']+)["']"#).unwrap();

    let labels: Vec<(usize, String)> = label_re
        .find_iter(html)
        .map(|m| (m.start(), m.as_str().to_lowercase()))
        .collect();

    let kind_of = |l: &str| -> &'static str {
        if l.starts_with("weekly") {
            "weekly"
        } else {
            "session"
        }
    };

    let mut windows: Vec<ParsedWindow> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();

    for (i, (start, label)) in labels.iter().enumerate() {
        let kind = kind_of(label);
        if seen.contains(&kind) {
            continue;
        }
        // End the block at the next label of the *other* kind (or +4000 chars).
        let end = labels[i + 1..]
            .iter()
            .find(|(_, l)| kind_of(l) != kind)
            .map(|(s, _)| *s)
            .unwrap_or_else(|| html.len().min(*start + 4000));
        let block_end = end.min(*start + 4000);
        let block = &html[*start..block_end];

        let pct = pct_re
            .captures(block)
            .or_else(|| width_re.captures(block))
            .and_then(|c| c.get(1).and_then(|m| m.as_str().parse::<f64>().ok()));
        let pct = match pct {
            Some(p) => p.clamp(0.0, 100.0),
            None => continue,
        };

        let resets_at = data_re_to_iso(&data_time_re, block);

        let label_str = if kind == "weekly" {
            "周"
        } else if label.starts_with("hourly") {
            "1h"
        } else {
            "5h"
        };
        windows.push(ParsedWindow {
            label: label_str,
            used_pct: pct,
            resets_at,
            is_weekly: kind == "weekly",
        });
        seen.push(kind);
    }
    // session window first, weekly last (matches token-monitor ordering).
    windows.sort_by_key(|w| w.is_weekly);
    windows
}

fn data_re_to_iso(re: &Regex, block: &str) -> Option<String> {
    re.captures(block)
        .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
        .and_then(|s| parse_iso(&s))
}

/// Extract the plan badge (e.g. "Pro", "free") from the Cloud Usage header.
fn parse_plan_name(html: &str) -> Option<String> {
    let re = Regex::new(r"(?i)Cloud\s*Usage\s*</span\s*>\s*<span[^>]*>([^<]+)</span\s*>").unwrap();
    re.captures(html)
        .and_then(|c| c.get(1).map(|m| m.as_str().trim().to_string()))
        .filter(|s| !s.is_empty())
}

/// Heuristic: does the page look like a signed-out sign-in form?
fn looks_signed_out(html: &str) -> bool {
    let lower = html.to_lowercase();
    let has_form = lower.contains("<form");
    if !has_form {
        return false;
    }
    let auth_route = lower.contains("/api/auth/signin")
        || lower.contains("/auth/signin")
        || lower.contains("href=\"/signin\"")
        || lower.contains("href=\"/login\"")
        || lower.contains("action=\"/signin\"")
        || lower.contains("action=\"/login\"");
    let has_email = lower.contains("type=\"email\"") || lower.contains("name=\"email\"");
    let has_password = lower.contains("type=\"password\"") || lower.contains("name=\"password\"");
    auth_route || lower.contains("sign in to ollama") || (has_email && has_password)
}

/// Map a raw plan badge (e.g. "free", "pro") to a friendly label.
fn plan_label(raw: &str) -> String {
    let lower = raw.trim().to_lowercase();
    match lower.as_str() {
        "free" => "Free Plan".into(),
        "pro" => "Pro Plan".into(),
        "team" | "teams" => "Teams Plan".into(),
        "enterprise" => "Enterprise".into(),
        _ => {
            // Title-case + " Plan"
            let mut chars = lower.chars();
            match chars.next() {
                Some(c) => {
                    let head = c.to_uppercase().collect::<String>();
                    format!("{head}{} Plan", chars.as_str())
                }
                None => raw.into(),
            }
        }
    }
}

/// HTTP client. Injected for unit tests.
pub trait Http {
    fn get(&self, url: &str, cookie: &str) -> Result<String, VendorError>;
}

/// Fetch via `http`. `credential` is the raw cookie header (or `{"cookie": ...}`).
pub fn fetch_with(http: &dyn Http, credential: &str) -> Result<Quota, VendorError> {
    // Accept either a raw cookie string or a JSON `{"cookie": "..."}` blob.
    let raw = serde_json::from_str::<serde_json::Value>(credential)
        .ok()
        .and_then(|v| v.get("cookie").and_then(|c| c.as_str()).map(|s| s.to_string()))
        .unwrap_or_else(|| credential.to_string());

    let cookie = normalize_cookie(&raw);
    if cookie.is_empty() {
        return Err(VendorError::Parse(
            "缺少有效的会话 Cookie（需包含 session / wos-session / next-auth.session-token 等）".into(),
        ));
    }

    let html = http.get(SETTINGS_URL, &cookie)?;
    let windows = parse_usage_html(&html);

    if windows.is_empty() {
        return Err(if looks_signed_out(&html) {
            VendorError::Auth("Ollama 会话已失效".into())
        } else {
            VendorError::Empty
        });
    }

    let plan_name = parse_plan_name(&html).as_deref().map(plan_label);

    let q_windows: Vec<QuotaWindow> = windows
        .into_iter()
        .map(|w| QuotaWindow {
            label: w.label.into(),
            used_pct: w.used_pct,
            resets_at: w.resets_at,
            ..Default::default()
        })
        .collect();

    let used_pct = q_windows
        .iter()
        .map(|w| w.used_pct)
        .fold(0.0f64, f64::max);

    Ok(Quota {
        vendor: "ollama".into(),
        plan_label: plan_name,
        status: QuotaStatus::from_used_pct(used_pct),
        windows: q_windows,
        balance: None,
        refreshed_at: None,
        error: None,
        cookie_error: None,
        expires_at: None,
    })
}

/// Default fetch (real network).
pub async fn fetch(credential: &str) -> Result<Quota, VendorError> {
    let cred = credential.to_string();
    tokio::task::spawn_blocking(move || fetch_with(&UreqHttp, &cred))
        .await
        .map_err(|e| VendorError::Network(format!("join: {e}")))?
}

struct UreqHttp;
impl Http for UreqHttp {
    fn get(&self, url: &str, cookie: &str) -> Result<String, VendorError> {
        let resp = ureq::get(url)
            .set("Cookie", cookie)
            .set("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
            .set("Accept-Language", "en-US,en;q=0.9")
            .set("Referer", "https://ollama.com/")
            .set("User-Agent", USER_AGENT)
            .call();
        match resp {
            Ok(r) => r.into_string().map_err(|e| VendorError::Network(e.to_string())),
            Err(ureq::Error::Status(code, _r)) => {
                if code == 401 || code == 403 {
                    Err(VendorError::Auth("status code 401".into()))
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

#[cfg(test)]
mod tests {
    use super::*;

    const SETTINGS_HTML: &str = r#"
<span>Cloud Usage</span><span>Pro</span>
<span id="header-email">USER@example.com</span>
<section aria-label="Session usage 14.5% used">
  <div style="width: 14.5%"></div>
  <div data-time="2026-07-09T08:00:00Z">Resets soon</div>
</section>
<section><span>Weekly usage</span><span>10.3% used</span>
  <div data-time="2026-07-13T00:00:00Z"></div>
</section>"#;

    #[test]
    fn normalize_cookie_keeps_recognized_names_only() {
        assert_eq!(normalize_cookie("wos-session=current"), "wos-session=current");
        assert_eq!(
            normalize_cookie("aid=1; wos-session=current; cf_clearance=ok"),
            "aid=1; wos-session=current; cf_clearance=ok"
        );
        assert_eq!(normalize_cookie("__Secure-session=legacy"), "__Secure-session=legacy");
        // Bare values rejected.
        assert_eq!(normalize_cookie("raw-token-without-name"), "");
        assert_eq!(normalize_cookie(&format!("{}==", "a".repeat(80))), "");
    }

    #[test]
    fn normalize_cookie_strips_cookie_prefix_and_quotes() {
        assert_eq!(normalize_cookie(r#""wos-session=abc""#), "wos-session=abc");
        assert_eq!(normalize_cookie("Cookie: wos-session=abc"), "wos-session=abc");
    }

    #[test]
    fn parse_html_extracts_windows_and_reset_times() {
        let w = parse_usage_html(SETTINGS_HTML);
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].label, "5h");
        assert!((w[0].used_pct - 14.5).abs() < 1e-6);
        assert!(w[0].resets_at.as_deref().unwrap().starts_with("2026-07-09"));
        assert_eq!(w[1].label, "周");
        assert!((w[1].used_pct - 10.3).abs() < 1e-6);
    }

    #[test]
    fn parse_html_falls_back_to_css_width() {
        let html = r#"
        <section>Weekly usage<div style="width: 80%"></div></section>
        <section>Hourly usage<span>25% used</span></section>"#;
        let w = parse_usage_html(html);
        // session (hourly) first, weekly second
        assert_eq!(w[0].label, "1h");
        assert!((w[0].used_pct - 25.0).abs() < 1e-6);
        assert_eq!(w[1].label, "周");
        assert!((w[1].used_pct - 80.0).abs() < 1e-6);
    }

    #[test]
    fn parse_plan_name_from_cloud_usage_badge() {
        assert_eq!(parse_plan_name(SETTINGS_HTML).as_deref(), Some("Pro"));
    }

    #[test]
    fn plan_label_maps_known_tiers() {
        assert_eq!(plan_label("free"), "Free Plan");
        assert_eq!(plan_label("PRO"), "Pro Plan");
        assert_eq!(plan_label("business"), "Business Plan");
    }

    #[test]
    fn looks_signed_out_detects_signin_form() {
        assert!(looks_signed_out(r#"<form action="/signin"><input type="email"></form>"#));
        assert!(!looks_signed_out(SETTINGS_HTML));
    }

    #[test]
    fn fetch_with_returns_quota_windows() {
        struct Mock;
        impl Http for Mock {
            fn get(&self, _: &str, _: &str) -> Result<String, VendorError> {
                Ok(SETTINGS_HTML.into())
            }
        }
        let q = fetch_with(&Mock, "wos-session=abc").unwrap();
        assert_eq!(q.vendor, "ollama");
        assert_eq!(q.plan_label.as_deref(), Some("Pro Plan"));
        assert_eq!(q.windows.len(), 2);
        assert!((q.windows[0].used_pct - 14.5).abs() < 1e-6);
        assert!(q.windows[0].resets_at.is_some());
    }

    #[test]
    fn fetch_with_rejects_bare_token() {
        struct Mock;
        impl Http for Mock {
            fn get(&self, _: &str, _: &str) -> Result<String, VendorError> {
                unreachable!()
            }
        }
        let err = fetch_with(&Mock, "raw-token").unwrap_err();
        assert!(matches!(err, VendorError::Parse(_)));
    }

    #[test]
    fn fetch_with_signed_out_html_is_auth_error() {
        struct Mock;
        impl Http for Mock {
            fn get(&self, _: &str, _: &str) -> Result<String, VendorError> {
                Ok(r#"<form action="/signin"><input type="email"></form>"#.into())
            }
        }
        let err = fetch_with(&Mock, "wos-session=expired").unwrap_err();
        assert!(matches!(err, VendorError::Auth(_)));
    }

    #[test]
    fn fetch_with_accepts_json_cookie_blob() {
        struct Mock;
        impl Http for Mock {
            fn get(&self, _: &str, _: &str) -> Result<String, VendorError> {
                Ok(SETTINGS_HTML.into())
            }
        }
        let q = fetch_with(&Mock, r#"{"cookie":"wos-session=abc"}"#).unwrap();
        assert_eq!(q.vendor, "ollama");
    }
}
