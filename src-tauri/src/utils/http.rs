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
//! variables and wires them into a shared `ureq::Agent`.

use std::sync::OnceLock;

use ureq::Agent;

/// Resolve the proxy URL from environment variables (HTTPS_PROXY preferred).
fn resolve_proxy_url() -> Option<String> {
    for key in ["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy", "HTTP_PROXY", "http_proxy"] {
        if let Ok(val) = std::env::var(key) {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

/// A shared `ureq::Agent` configured with the system proxy (if any).
/// Built once and reused for the process lifetime.
fn shared_agent() -> &'static Agent {
    static AGENT: OnceLock<Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        let mut builder = ureq::AgentBuilder::new();
        if let Some(proxy_url) = resolve_proxy_url() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_proxy_returns_none_without_env() {
        // Env vars are not set in test → None (or whatever the host has).
        // Just verify it doesn't panic.
        let _ = resolve_proxy_url();
    }
}
