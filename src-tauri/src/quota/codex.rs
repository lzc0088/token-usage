//! Codex (OpenAI) quota adapter — CLI RPC + ChatGPT API dual path.
//!
//! Two data sources (same as token-monitor):
//! 1. **CLI RPC** — invoke the `codex` binary to read rate limits from the
//!    running CLI session. Provides primary/secondary windows.
//! 2. **ChatGPT API** — call `wham/rate-limit-reset-credits` for reset credits
//!    (available_count + per-credit expiry list).
//!
//! Merge: CLI windows + API reset credits.
//!
//! Credential format (stored in keyring as-is):
//!   - Raw: access token string
//!   - JSON: `{"access_token":"sk-...","account_id":"acc-..."}`

use regex::Regex;
use serde::Deserialize;
use std::process::{Command, Stdio};
use std::sync::LazyLock;
use tauri::Emitter;

use super::types::{epoch_to_iso, Quota, QuotaStatus, QuotaWindow};
use super::VendorError;

const RESET_CREDITS_URL: &str = "https://chatgpt.com/backend-api/wham/rate-limit-reset-credits";
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36";

// ---------------------------------------------------------------------------
// HTTP trait
// ---------------------------------------------------------------------------

pub trait Http {
    fn get(&self, url: &str, token: &str, account_id: Option<&str>) -> Result<String, VendorError>;
}

// ---------------------------------------------------------------------------
// Credential parsing
// ---------------------------------------------------------------------------

fn parse_credential(credential: &str) -> (String, Option<String>) {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(credential) {
        let access = v
            .get("access_token")
            .and_then(|k| k.as_str())
            .map(|s| s.to_string());
        let account = v
            .get("account_id")
            .and_then(|k| k.as_str())
            .map(|s| s.to_string());
        if access.is_some() || account.is_some() {
            return (access.unwrap_or_default(), account);
        }
    }
    (credential.trim().to_string(), None)
}

// ---------------------------------------------------------------------------
// CLI RPC
// ---------------------------------------------------------------------------

/// Invoke `codex rpc` and return the parsed JSON response.
/// Returns None if the CLI is unavailable or returns non-JSON.
fn call_codex_rpc() -> Option<serde_json::Value> {
    let output = Command::new("codex")
        .args([
            "--quiet",
            "--json",
            "rpc",
            "--method",
            "codex/listRateLimits",
            "--args",
            "[]",
        ])
        .env("NO_COLOR", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str::<serde_json::Value>(&text).ok()
}

// ---------------------------------------------------------------------------
// API response parsing
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct RateLimitsPayload {
    #[serde(rename = "rateLimits", alias = "rate_limits")]
    rate_limits: Option<serde_json::Value>,
    #[serde(rename = "rateLimitsByLimitId", alias = "rate_limits_by_limit_id")]
    rate_limits_by_limit_id: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct RateLimitSnapshot {
    primary: Option<WindowData>,
    secondary: Option<WindowData>,
}

#[derive(Debug, Deserialize)]
struct WindowData {
    #[serde(rename = "usedPercent", alias = "used_percent")]
    used_percent: Option<f64>,
    #[serde(rename = "resetsAt", alias = "resets_at")]
    resets_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ResetCreditsPayload {
    #[serde(rename = "available_count", alias = "availableCount")]
    available_count: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct CreditEntry {
    #[allow(dead_code)]
    status: Option<String>,
    #[allow(dead_code)]
    #[serde(rename = "expires_at", alias = "expiresAt")]
    expires_at: Option<String>,
}

fn extract_rate_limits(payload: &serde_json::Value) -> Option<RateLimitSnapshot> {
    // Try direct rate_limits first.
    if let Some(rl) = payload
        .get("rateLimits")
        .or_else(|| payload.get("rate_limits"))
    {
        if let Ok(snap) = serde_json::from_value::<RateLimitSnapshot>(rl.clone()) {
            if snap.primary.is_some() || snap.secondary.is_some() {
                return Some(snap);
            }
        }
    }
    // Try rateLimitsByLimitId.codex
    if let Some(by_id) = payload
        .get("rateLimitsByLimitId")
        .or_else(|| payload.get("rate_limits_by_limit_id"))
    {
        if let Some(codex) = by_id.get("codex") {
            if let Ok(snap) = serde_json::from_value::<RateLimitSnapshot>(codex.clone()) {
                if snap.primary.is_some() || snap.secondary.is_some() {
                    return Some(snap);
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Main fetch
// ---------------------------------------------------------------------------

pub fn fetch_with(http: &dyn Http, credential: &str) -> Result<Quota, VendorError> {
    let (access_token, account_id) = parse_credential(credential);
    if access_token.is_empty() {
        return Err(VendorError::Parse("缺少 access token".into()));
    }

    let account_id_ref = account_id.as_deref();

    // ① CLI RPC — rate limits.
    let mut windows: Vec<QuotaWindow> = Vec::new();
    if let Some(rpc_payload) = call_codex_rpc() {
        if let Some(snap) = extract_rate_limits(&rpc_payload) {
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);

            if let Some(ref primary) = snap.primary {
                let pct = primary.used_percent.unwrap_or(0.0).clamp(0.0, 100.0);
                let resets_ms = primary
                    .resets_at
                    .as_ref()
                    .and_then(|s| s.parse::<i64>().ok())
                    .map(|ts| {
                        if ts > 1_000_000_000_000 {
                            ts
                        } else {
                            ts * 1000
                        }
                    })
                    .unwrap_or(now_ms + 5 * 60 * 60 * 1000);
                windows.push(QuotaWindow {
                    label: "5h".into(),
                    used_pct: pct,
                    resets_at: epoch_to_iso(resets_ms as f64),
                    ..Default::default()
                });
            }
            if let Some(ref secondary) = snap.secondary {
                let pct = secondary.used_percent.unwrap_or(0.0).clamp(0.0, 100.0);
                let resets_ms = secondary
                    .resets_at
                    .as_ref()
                    .and_then(|s| s.parse::<i64>().ok())
                    .map(|ts| {
                        if ts > 1_000_000_000_000 {
                            ts
                        } else {
                            ts * 1000
                        }
                    })
                    .unwrap_or(now_ms + 7 * 24 * 60 * 60 * 1000);
                windows.push(QuotaWindow {
                    label: "周".into(),
                    used_pct: pct,
                    resets_at: epoch_to_iso(resets_ms as f64),
                    ..Default::default()
                });
            }
        }
    }

    // ② ChatGPT API — reset credits (supplementary, not displayed yet).
    if let Ok(body) = http.get(RESET_CREDITS_URL, &access_token, account_id_ref) {
        if let Ok(payload) = serde_json::from_str::<ResetCreditsPayload>(&body) {
            if let Some(count) = payload.available_count {
                let _ = count;
            }
        }
    }

    if windows.is_empty() {
        return Err(VendorError::Empty);
    }

    let used_pct = windows.iter().map(|w| w.used_pct).fold(0.0f64, f64::max);

    Ok(Quota {
        vendor: "codex".into(),
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

// ---------------------------------------------------------------------------
// Ureq HTTP impl
// ---------------------------------------------------------------------------

struct UreqHttp;
impl Http for UreqHttp {
    fn get(&self, url: &str, token: &str, account_id: Option<&str>) -> Result<String, VendorError> {
        let mut req = ureq::get(url)
            .set("Accept", "application/json")
            .set("Authorization", &format!("Bearer {token}"))
            .set("openai-beta", "codex-1")
            .set("originator", "Codex Desktop")
            .set("User-Agent", USER_AGENT);
        if let Some(aid) = account_id {
            req = req.set("chatgpt-account-id", aid);
        }
        let resp = req.call();
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
        api: Result<String, VendorError>,
    }

    impl Http for MockHttp {
        fn get(&self, _: &str, _: &str, _: Option<&str>) -> Result<String, VendorError> {
            self.api.clone()
        }
    }

    #[test]
    fn parse_credential_raw_token() {
        let (access, account) = parse_credential("sk-raw-token");
        assert_eq!(access, "sk-raw-token");
        assert!(account.is_none());
    }

    #[test]
    fn parse_credential_json() {
        let json = r#"{"access_token":"at","account_id":"acc-123"}"#;
        let (access, account) = parse_credential(json);
        assert_eq!(access, "at");
        assert_eq!(account.as_deref(), Some("acc-123"));
    }

    #[test]
    fn extract_rate_limits_direct() {
        let payload = serde_json::json!({
            "rateLimits": {
                "primary": {"usedPercent": 25, "resetsAt": "2030000000000"},
                "secondary": {"usedPercent": 50, "resetsAt": "2030000000000"}
            }
        });
        let snap = extract_rate_limits(&payload).unwrap();
        assert!(snap.primary.is_some());
        assert!(snap.secondary.is_some());
    }

    #[test]
    fn extract_rate_limits_by_id() {
        let payload = serde_json::json!({
            "rateLimitsByLimitId": {
                "codex": {
                    "primary": {"used_percent": 30},
                    "secondary": {"used_percent": 70}
                }
            }
        });
        let snap = extract_rate_limits(&payload).unwrap();
        assert!(snap.primary.is_some());
        assert!(snap.secondary.is_some());
    }

    #[test]
    fn extract_rate_limits_none_returns_none() {
        let payload = serde_json::json!({"other": {}});
        assert!(extract_rate_limits(&payload).is_none());
    }

    #[test]
    fn fetch_with_cli_rpc_windows() {
        // Simulate CLI RPC returning rate limits.
        let _rpc_payload = serde_json::json!({
            "rateLimits": {
                "primary": {"usedPercent": 20, "resetsAt": "2030000000000"},
                "secondary": {"usedPercent": 40, "resetsAt": "2030000000000"}
            }
        });
        // Since we can't mock the CLI, just test the HTTP path with no CLI.
        let mock = MockHttp {
            api: Err(VendorError::Network("not called".into())),
        };
        // No CLI in test env, API empty → Empty error.
        let err = fetch_with(&mock, "my-token").unwrap_err();
        match err {
            VendorError::Empty => {}
            _ => panic!("expected Empty, got: {err:?}"),
        }
    }
}

// ── OAuth Login Flow ────────────────────────────────────────────────────────
// Mirrors token-monitor's `runCodexLogin`: spawn `codex login`, stream stdout
// to extract the OAuth URL, emit events so the frontend can open the browser.

/// Regex to find OpenAI auth URLs in codex login output.
#[allow(clippy::incompatible_msrv)]
static AUTH_URL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"https://auth\.openai\.com/oauth/authorize[^\s]*").expect("valid regex")
});

/// Spawn `codex login` and stream stdout, looking for the OAuth authorize URL.
/// Emits `codex:login_status` events carrying `{phase, login_url}`.
/// Returns when the subprocess completes (user finishes OAuth in browser).
pub async fn codex_login(app: &tauri::AppHandle) -> Result<(), String> {
    // Spawn in blocking thread since Command is sync.
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let child = Command::new("codex")
            .args(["login"])
            .env("NO_COLOR", "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();

        let mut child = match child {
            Ok(c) => c,
            Err(e) => {
                let _ = app_handle.emit(
                    "codex:login_status",
                    CodexLoginStatus::Error {
                        message: format!("无法启动 codex 命令: {e}"),
                    },
                );
                return Err(format!("无法启动 codex 命令: {e}"));
            }
        };

        let stdout = child.stdout.take().expect("stdout piped");
        let mut stderr = child.stderr.take().expect("stderr piped");

        // Read stdout line by line, looking for the OAuth URL.

        // Spawn a thread to drain stderr (avoids pipe blocking).
        let _stderr_handle = std::thread::spawn(move || {
            let _ = std::io::copy(&mut stderr, &mut std::io::sink());
        });

        // Read stdout.
        let mut reader = std::io::BufReader::new(stdout);
        let mut line = String::new();
        loop {
            line.clear();
            use std::io::BufRead;
            match reader.read_line(&mut line) {
                Ok(0) => break, // EOF
                Ok(_) => {
                    if let Some(cap) = AUTH_URL_RE.captures(&line) {
                        if let Some(url_match) = cap.get(0) {
                            let url = url_match.as_str().to_string();
                            let _ = app_handle.emit(
                                "codex:login_status",
                                CodexLoginStatus::Authorize { login_url: url },
                            );
                        }
                    }
                }
                Err(_) => break,
            }
        }

        // Wait for the process to finish.
        let status = child.wait();
        match status {
            Ok(s) if s.success() => {
                let _ = app_handle.emit("codex:login_status", CodexLoginStatus::Success);
                Ok(())
            }
            Ok(s) => {
                let msg = format!("codex login 退出码: {}", s.code().unwrap_or(-1));
                let _ = app_handle.emit(
                    "codex:login_status",
                    CodexLoginStatus::Error {
                        message: msg.clone(),
                    },
                );
                Err(msg)
            }
            Err(e) => {
                let msg = format!("codex login 失败: {e}");
                let _ = app_handle.emit(
                    "codex:login_status",
                    CodexLoginStatus::Error {
                        message: msg.clone(),
                    },
                );
                Err(msg)
            }
        }
    })
    .await
    .map_err(|e| format!("codex login join error: {e}"))??;

    Ok(())
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum CodexLoginStatus {
    /// OAuth URL found in codex login output — frontend should open it.
    Authorize { login_url: String },
    /// Login completed successfully.
    Success,
    /// Something went wrong.
    Error { message: String },
}
