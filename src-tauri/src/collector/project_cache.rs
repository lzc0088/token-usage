//! Project list cache to avoid expensive tokscale calls on every page load.
//!
//! A cache entry is valid for 5 minutes. It is invalidated early when:
//! - collection_tracked / collection_visible config changes
//! - a manual refresh is triggered
//!
//! The cache lives in `app_config` under the key "projects_cache_v1".

use serde::{Deserialize, Serialize};

const CACHE_TTL_SECS: i64 = 300; // 5 minutes

/// Cached project list with metadata for freshness checks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectCache {
    /// The period string this cache was built for ("day" | "month" | "total").
    pub period: String,
    /// Serialized project list.
    pub projects: Vec<ProjectCacheEntry>,
    /// Unix timestamp (seconds) when this cache was written.
    pub cached_at: i64,
    /// Config fingerprint at cache time — used to detect tracked/visible changes.
    pub config_fingerprint: u64,
}

/// Serializable subset of ProjectAgg for caching.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectCacheEntry {
    pub name: String,
    pub full_path: Option<String>,
    pub latest_date: Option<String>,
    pub tokens: i64,
    pub cost_usd: f64,
    pub messages: i64,
}

impl ProjectCache {
    /// Check whether this cache entry is still fresh.
    pub fn is_fresh(&self, now_secs: i64, current_fingerprint: u64) -> bool {
        // Unix epoch (0) means "never cached" — always stale.
        if self.cached_at == 0 {
            return false;
        }
        self.cached_at + CACHE_TTL_SECS > now_secs && self.config_fingerprint == current_fingerprint
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_cache_passes_when_within_ttl_and_fingerprint_matches() {
        let now = 1_700_000_000i64;
        let cache = ProjectCache {
            period: "day".into(),
            projects: vec![],
            cached_at: now - 100, // 100 secs ago
            config_fingerprint: 42,
        };
        assert!(cache.is_fresh(now, 42));
    }

    #[test]
    fn stale_cache_fails_after_ttl() {
        let now = 1_700_000_000i64;
        let cache = ProjectCache {
            period: "day".into(),
            projects: vec![],
            cached_at: now - CACHE_TTL_SECS - 1, // just expired
            config_fingerprint: 42,
        };
        assert!(!cache.is_fresh(now, 42));
    }

    #[test]
    fn cache_fails_on_fingerprint_mismatch() {
        let now = 1_700_000_000i64;
        let cache = ProjectCache {
            period: "day".into(),
            projects: vec![],
            cached_at: now - 100,
            config_fingerprint: 42,
        };
        assert!(!cache.is_fresh(now, 99)); // different fingerprint
    }

    #[test]
    fn zero_timestamp_is_always_stale() {
        // Unix epoch (0) means "never cached" — always treat as stale.
        let cache = ProjectCache {
            period: "day".into(),
            projects: vec![],
            cached_at: 0, // epoch = uninitialized
            config_fingerprint: 1,
        };
        assert!(!cache.is_fresh(1, 1));
        assert!(!cache.is_fresh(300, 1));
        assert!(!cache.is_fresh(999_999_999, 1));
    }
}
