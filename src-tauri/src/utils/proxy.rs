//! System proxy sync.
//!
//! `reqwest` (used by the updater) and `ureq` (used by check-update, exchange
//! rate, quota fetches) read proxy config from `HTTPS_PROXY` / `HTTP_PROXY` /
//! `ALL_PROXY` env vars — but neither reads the OS-level system proxy. So a
//! user running Clash/V2Ray in "system proxy" mode (the default for most GUI
//! clients) gets a direct connection, which is slow or blocked for GitHub's
//! CDN in CN.
//!
//! [`sync_system_proxy`] is called once at startup. It detects the OS system
//! proxy and, when found, writes it into `HTTPS_PROXY`/`HTTP_PROXY` so every
//! HTTP client in the process routes through it. Existing env vars are
//! respected — if the user already exported `HTTPS_PROXY`, we leave it.

/// Detect the OS system proxy (if any) and mirror it into `HTTPS_PROXY` /
/// `HTTP_PROXY` so reqwest/ureq pick it up. Safe to call multiple times.
///
/// Must be called before any HTTP request (i.e. early in `setup`).
pub fn sync_system_proxy() {
    // Respect a user-provided env proxy — don't override it.
    if std::env::var_os("HTTPS_PROXY").is_some()
        || std::env::var_os("HTTP_PROXY").is_some()
        || std::env::var_os("ALL_PROXY").is_some()
        || std::env::var_os("https_proxy").is_some()
        || std::env::var_os("http_proxy").is_some()
        || std::env::var_os("all_proxy").is_some()
    {
        tracing::info!("system proxy: using env-provided proxy");
        return;
    }

    match detect_system_proxy() {
        Some(proxy_url) => {
            tracing::info!("system proxy: detected → {proxy_url}");
            std::env::set_var("HTTPS_PROXY", &proxy_url);
            std::env::set_var("HTTP_PROXY", &proxy_url);
            std::env::set_var("https_proxy", &proxy_url);
            std::env::set_var("http_proxy", &proxy_url);
        }
        None => tracing::info!("system proxy: none detected, direct connection"),
    }
}

/// Read the resolved system proxy on the current platform. Returns
/// `http://host:port` when an enabled proxy is configured, else `None`.
#[cfg(target_os = "macos")]
fn detect_system_proxy() -> Option<String> {
    // `scutil --proxy` resolves the effective proxy across all network
    // services — no need to know the service name (unlike `networksetup`).
    let out = std::process::Command::new("scutil")
        .arg("--proxy")
        .output()
        .ok()?;
    parse_scutil_proxy(&String::from_utf8_lossy(&out.stdout))
}

/// Parse the `scutil --proxy` output (a plist-style dict). Prefer the HTTPS
/// fields, fall back to HTTP.
#[cfg(target_os = "macos")]
fn parse_scutil_proxy(text: &str) -> Option<String> {
    let mut https = ProxyFields::default();
    let mut http = ProxyFields::default();
    for line in text.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("HTTPSProxy :") {
            https.server = v.trim().trim_matches('"').to_string();
        } else if let Some(v) = line.strip_prefix("HTTPSPort :") {
            https.port = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("HTTPSEnable :") {
            https.enabled = v.trim() == "1";
        } else if let Some(v) = line.strip_prefix("HTTPProxy :") {
            http.server = v.trim().trim_matches('"').to_string();
        } else if let Some(v) = line.strip_prefix("HTTPPort :") {
            http.port = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("HTTPEnable :") {
            http.enabled = v.trim() == "1";
        }
    }
    if https.enabled {
        if let Some(url) = https.into_url() {
            return Some(url);
        }
    }
    if http.enabled {
        return http.into_url();
    }
    None
}

#[cfg(target_os = "macos")]
#[derive(Default)]
struct ProxyFields {
    server: String,
    port: String,
    enabled: bool,
}

#[cfg(target_os = "macos")]
impl ProxyFields {
    fn into_url(self) -> Option<String> {
        if !self.server.is_empty() && !self.port.is_empty() {
            Some(format!("http://{}:{}", self.server, self.port))
        } else {
            None
        }
    }
}

#[cfg(target_os = "windows")]
fn detect_system_proxy() -> Option<String> {
    // Read the user Internet Settings registry key. `ProxyServer` holds either
    // "host:port" or a per-scheme string like "http=host:port;https=host:port".
    let out = std::process::Command::new("reg")
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
        ])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut enabled = false;
    let mut proxy_server = String::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("ProxyEnable") {
            enabled = v.trim_end_matches("0x").trim().ends_with('1') || v.trim() == "0x1";
        } else if let Some(v) = line.strip_prefix("ProxyServer") {
            proxy_server = v.trim().to_string();
        }
    }
    if !enabled || proxy_server.is_empty() {
        return None;
    }
    // If per-scheme, prefer https= then http=.
    if proxy_server.contains('=') {
        for scheme in ["https", "http"] {
            for part in proxy_server.split(';') {
                if let Some(rest) = part.trim().strip_prefix(&format!("{scheme}=")) {
                    return Some(format!("http://{}", rest.trim()));
                }
            }
        }
        return None;
    }
    Some(format!("http://{}", proxy_server.trim()))
}

#[cfg(target_os = "linux")]
fn detect_system_proxy() -> Option<String> {
    // On Linux the "system proxy" *is* the env vars (there's no separate
    // OS-level proxy config in most DEs that apps read). Those were already
    // checked at the top of sync_system_proxy, so nothing to do here.
    None
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "macos")]
    use super::parse_scutil_proxy;

    #[cfg(target_os = "macos")]
    #[test]
    fn parses_scutil_https_proxy() {
        let out = r#"
<dictionary> {
  HTTPEnable : 1
  HTTPProxy : 127.0.0.1
  HTTPPort : 7890
  HTTPSEnable : 1
  HTTPSProxy : 127.0.0.1
  HTTPSPort : 7890
}
"#;
        assert_eq!(
            parse_scutil_proxy(out),
            Some("http://127.0.0.1:7890".to_string())
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn parses_scutil_disabled() {
        let out = r#"
  HTTPEnable : 0
  HTTPSEnable : 0
"#;
        assert_eq!(parse_scutil_proxy(out), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn prefers_https_over_http() {
        let out = r#"
  HTTPEnable : 1
  HTTPProxy : 10.0.0.1
  HTTPPort : 8080
  HTTPSEnable : 1
  HTTPSProxy : 127.0.0.1
  HTTPSPort : 7890
"#;
        assert_eq!(
            parse_scutil_proxy(out),
            Some("http://127.0.0.1:7890".to_string())
        );
    }
}
