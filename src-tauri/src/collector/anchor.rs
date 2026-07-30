//! Collector anchor for incremental updates (P1).
//!
//! An anchor is a persisted snapshot of a full scan (today + month + allTime).
//! When the anchor is still valid (same day + matching config fingerprint), the
//! scheduler can skip the expensive `--month` and `--since` scans and instead
//! derive those windows from the anchor using `apply_period_delta`.
//!
//! Validity rules:
//! - `date_key` must match today (midnight-local)
//! - `config_fingerprint` must match the current enabled-clients set
//! - `full_scan_at` must be non-zero and not in the future
//!
//! The anchor is stored in `app_config` under the key "collector_anchor".

use serde::{Deserialize, Serialize};

use crate::collector::scheduler::PeriodSummary;
use crate::config;

/// A persisted full-scan snapshot that lets warm ticks replace month/allTime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectorAnchor {
    /// Local date key this anchor was captured for, e.g. "2026-07-29".
    pub date_key: String,
    /// Full today scan (absolute totals).
    pub today: PeriodSummary,
    /// Full month scan (absolute totals).
    pub month: PeriodSummary,
    /// Full allTime scan (absolute totals).
    pub all_time: PeriodSummary,
    /// Unix timestamp (seconds) of the last full scan.
    pub full_scan_at: i64,
    /// Fingerprint of the config at anchor time (detects client list changes).
    pub config_fingerprint: u64,
}

impl CollectorAnchor {
    /// Check whether this anchor is still valid for warm-scan use.
    pub fn is_valid(&self, today_key: &str, current_fingerprint: u64) -> bool {
        // Date must match today
        if self.date_key != today_key {
            return false;
        }
        // Config must not have changed
        if self.config_fingerprint != current_fingerprint {
            return false;
        }
        // full_scan_at must be non-zero and not in the future
        if self.full_scan_at == 0 {
            return false;
        }
        let now = chrono::Utc::now().timestamp();
        if self.full_scan_at > now {
            return false;
        }
        true
    }

    /// Load an anchor from app_config. Returns None if missing or corrupted.
    pub fn load(conn: &rusqlite::Connection) -> Result<Option<Self>, String> {
        config::get_json(conn, "collector_anchor").map_err(|e| e.to_string())
    }

    /// Persist this anchor to app_config.
    pub fn save(&self, conn: &rusqlite::Connection) -> Result<(), String> {
        config::set_json(conn, "collector_anchor", self).map_err(|e| e.to_string())
    }

    /// Invalidate the anchor (e.g. on config change or date rollover).
    pub fn invalidate(conn: &rusqlite::Connection) -> Result<(), String> {
        conn.execute("DELETE FROM app_config WHERE key = 'collector_anchor'", [])
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn fresh_anchor() -> CollectorAnchor {
        CollectorAnchor {
            date_key: "2026-07-29".into(),
            today: PeriodSummary {
                total_tokens: 1000,
                cost_usd: 5.0,
                ..Default::default()
            },
            month: PeriodSummary {
                total_tokens: 10_000,
                cost_usd: 50.0,
                ..Default::default()
            },
            all_time: PeriodSummary {
                total_tokens: 100_000,
                cost_usd: 500.0,
                ..Default::default()
            },
            full_scan_at: chrono::Utc::now().timestamp() - 3600, // 1 hour ago
            config_fingerprint: 42,
        }
    }

    #[test]
    fn valid_anchor_passes() {
        let anchor = fresh_anchor();
        let today_key = "2026-07-29";
        assert!(anchor.is_valid(today_key, 42));
    }

    #[test]
    fn invalid_date_fails() {
        let anchor = fresh_anchor();
        assert!(!anchor.is_valid("2026-07-30", 42));
    }

    #[test]
    fn invalid_fingerprint_fails() {
        let anchor = fresh_anchor();
        assert!(!anchor.is_valid("2026-07-29", 99));
    }

    #[test]
    fn zero_full_scan_at_fails() {
        let mut anchor = fresh_anchor();
        anchor.full_scan_at = 0;
        assert!(!anchor.is_valid("2026-07-29", 42));
    }

    #[test]
    fn future_full_scan_at_fails() {
        let mut anchor = fresh_anchor();
        anchor.full_scan_at = chrono::Utc::now().timestamp() + 3600; // 1 hour in future
        assert!(!anchor.is_valid("2026-07-29", 42));
    }

    #[test]
    fn save_and_load_roundtrip() {
        let conn = Connection::open_in_memory().unwrap();
        crate::storage::schema::migrate(&conn).unwrap();
        let anchor = fresh_anchor();
        anchor.save(&conn).unwrap();
        let loaded = CollectorAnchor::load(&conn).unwrap().unwrap();
        assert_eq!(loaded.date_key, anchor.date_key);
        assert_eq!(loaded.today.total_tokens, anchor.today.total_tokens);
        assert_eq!(loaded.config_fingerprint, anchor.config_fingerprint);
    }

    #[test]
    fn invalidate_removes_anchor() {
        let conn = Connection::open_in_memory().unwrap();
        crate::storage::schema::migrate(&conn).unwrap();
        let anchor = fresh_anchor();
        anchor.save(&conn).unwrap();
        CollectorAnchor::invalidate(&conn).unwrap();
        let loaded = CollectorAnchor::load(&conn).unwrap();
        assert!(loaded.is_none());
    }
}
