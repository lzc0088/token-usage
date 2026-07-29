//! Shared HTTP agent utilities.
//!
//! All quota adapters that need to reach external (potentially region-blocked)
//! sites should use [`proxy_agent`] instead of `ureq::get/post` directly.
//! `ureq` does NOT auto-detect the system proxy (unlike Electron's `net` or
//! Node's `fetch`), so a user behind a VPN/proxy gets connection timeouts on
//! blocked hosts (e.g. claude.ai from regions where it's restricted).
//!
//! This builder reads the standard `HTTPS_PROXY` / `https_proxy` (then
//! `ALL_PROXY` / `all_proxy`, then `HTTP_PROXY` / `http_proxy`) environment
//! variables. On macOS it also falls back to the system proxy configured in
//! System Preferences → Network → Proxies (via `scutil --proxy`).

use std::sync::OnceLock;

use ureq::Agent;

/// Resolve the proxy URL from environment variables (HTTPS_PROXY preferred).
fn resolve_proxy_url() -> Option<String> {
    for key in [
        "HTTPS_PROXY",
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
        "HTTP_PROXY",
        "http_proxy",
    ] {
        if let Ok(val) = std::env::var(key) {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

/// On macOS, parse `scutil --proxy` output for the HTTPS (preferred) or HTTP
/// proxy host + port. Returns `http://host:port` if found.
#[cfg(target_os = "macos")]
fn resolve_macos_system_proxy() -> Option<String> {
    use std::process::Command;
    let output = Command::new("scutil").arg("--proxy").output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    // Look for HTTPS proxy first, then HTTP.
    for proto in ["HTTPS", "HTTP"] {
        let enabled = text
            .lines()
            .any(|l| l.trim() == format!("{proto}Enable : 1"));
        if !enabled {
            continue;
        }
        let host = text
            .lines()
            .find(|l| l.trim().starts_with(&format!("{proto}Proxy : ")))
            .and_then(|l| l.split(':').nth(1))
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())?;
        let port = text
            .lines()
            .find(|l| l.trim().starts_with(&format!("{proto}Port : ")))
            .and_then(|l| l.split(':').nth(1))
            .map(|s| s.trim())
            .filter(|s| !s.is_empty() && *s != "0")?;
        return Some(format!("http://{host}:{port}"));
    }
    None
}

#[cfg(not(target_os = "macos"))]
fn resolve_macos_system_proxy() -> Option<String> {
    None
}

/// Combine env-var proxy with macOS system proxy fallback.
fn resolve_any_proxy() -> Option<String> {
    resolve_proxy_url().or_else(resolve_macos_system_proxy)
}

/// A shared `ureq::Agent` configured with the system proxy (if any).
/// Built once and reused for the process lifetime.
fn shared_agent() -> &'static Agent {
    static AGENT: OnceLock<Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        let mut builder = ureq::AgentBuilder::new();
        if let Some(proxy_url) = resolve_any_proxy() {
            if let Ok(proxy) = ureq::Proxy::new(&proxy_url) {
                builder = builder.proxy(proxy);
            }
        }
        builder.build()
    })
}

/// Get the shared proxy-aware agent. Use this for all outbound quota requests.
pub fn proxy_agent() -> &'static Agent {
    shared_agent()
}

/// Get a proxy-aware `AgentBuilder`. Use this when you need to customise the
/// agent further (e.g. `redirects(0)`).
pub fn proxy_agent_builder() -> ureq::AgentBuilder {
    let mut builder = ureq::AgentBuilder::new();
    if let Some(proxy_url) = resolve_any_proxy() {
        if let Ok(proxy) = ureq::Proxy::new(&proxy_url) {
            builder = builder.proxy(proxy);
        }
    }
    builder
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_proxy_returns_none_without_env() {
        // Env vars are not set in test → None (or whatever the host has).
        // Just verify it doesn't panic.
        let _ = resolve_proxy_url();
    }

    #[test]
    fn proxy_agent_does_not_panic() {
        let _ = proxy_agent();
    }

    #[test]
    fn proxy_agent_builder_does_not_panic() {
        let _ = proxy_agent_builder().build();
    }
}
