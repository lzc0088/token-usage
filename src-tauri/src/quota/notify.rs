//! Quota-exhaustion notification logic (Phase C).
//!
//! Pure, synchronous, fully unit-tested. Consumes `&Quota` + `now_ms` and
//! returns the set of windows that satisfy the notification trigger:
//!
//!   - remaining ≤ 20%   (low-remaining signal)
//!   - projected exhaustion within the next 2 hours  (burn-rate signal)
//!
//! A `NotifyDedup` guard prevents repeat notifications for the same
//! (vendor, window-label) pair. A pair is re-allowed only when the window's
//! usage drops (reset), tracked via the previously-notified `used_pct`.

use std::collections::HashMap;

use crate::quota::Quota;

/// A single notification-worthy window.
#[derive(Debug, Clone)]
pub struct NotifyCandidate {
    pub vendor: String,
    pub window_label: String,
    /// Remaining percentage (0..100).
    pub remaining_pct: f64,
    /// Projected exhaustion time as unix-ms, if computable.
    pub projected_ms: Option<i64>,
}

/// De-duplication guard for system notifications.
///
/// Tracks (vendor, label) pairs that have already fired, plus the `used_pct`
/// at notification time so a subsequent drop (window reset) re-allows the alert.
#[derive(Debug, Default)]
pub struct NotifyDedup {
    notified: HashMap<(String, String), f64>,
}

impl NotifyDedup {
    /// Returns `true` only the first time `candidate` is seen for a given
    /// (vendor, label) pair whose `used_pct` has not dropped since the last
    /// notification.
    ///
    /// When the window's usage drops (reset), the pair is cleared automatically
    /// on the next call so the user gets a fresh notification for the next
    /// depletion cycle.
    pub fn should_notify(&mut self, candidate: &NotifyCandidate) -> bool {
        let key = (candidate.vendor.clone(), candidate.window_label.clone());
        match self.notified.get(&key) {
            // Same or still-burning remaining → block.
            Some(&prev_pct) if candidate.remaining_pct <= prev_pct => false,
            // First time, or usage dropped (remaining climbed past prior) → allow.
            _ => {
                self.notified.insert(key, candidate.remaining_pct);
                true
            }
        }
    }
}

const NOTIFY_REMAINING_PCT: f64 = 20.0;
const NOTIFY_PROJECTED_WINDOW_MS: i64 = 2 * 60 * 60 * 1000; // 2 hours

/// Evaluate a single `Quota` against the notification triggers.
///
/// Returns a `Vec<NotifyCandidate>` for every window that satisfies at least
/// one trigger. Already-exhausted windows (remaining ≤ 0) are skipped.
pub fn evaluate(quota: &Quota, now_ms: i64) -> Vec<NotifyCandidate> {
    let mut out = Vec::new();
    for w in &quota.windows {
        let remaining_pct = (100.0 - w.used_pct).max(0.0);
        if remaining_pct <= 0.0 {
            continue;
        }

        let low = remaining_pct <= NOTIFY_REMAINING_PCT;

        let urgent = match parse_projected_ms(&w.projected_exhaustion_at) {
            Some(proj) => {
                let diff = proj - now_ms;
                (0..=NOTIFY_PROJECTED_WINDOW_MS).contains(&diff)
            }
            None => false,
        };

        if low || urgent {
            out.push(NotifyCandidate {
                vendor: quota.vendor.clone(),
                window_label: w.label.clone(),
                remaining_pct,
                projected_ms: parse_projected_ms(&w.projected_exhaustion_at),
            });
        }
    }
    out
}

/// Parse an RFC3339 timestamp string into unix-ms, or `None` if unparseable.
fn parse_projected_ms(s: &Option<String>) -> Option<i64> {
    let iso = s.as_ref()?;
    let dt = chrono::DateTime::parse_from_rfc3339(iso)
        .ok()?
        .with_timezone(&chrono::Utc);
    Some(dt.timestamp_millis())
}

// ── notification copy / platform payload builders ───────────────────────────

/// Notification title for the user's configured language.
pub fn build_title(lang_zh: bool) -> &'static str {
    if lang_zh {
        "额度即将耗尽"
    } else {
        "Quota nearly exhausted"
    }
}

/// Human-readable ETA from a duration in minutes ("35分钟" / "1小时20分钟",
/// "35m" / "1h20m").
pub fn build_eta(mins: i64, lang_zh: bool) -> String {
    if mins < 60 {
        return if lang_zh {
            format!("{mins}分钟")
        } else {
            format!("{mins}m")
        };
    }
    let h = mins / 60;
    let m = mins % 60;
    if m == 0 {
        if lang_zh {
            format!("{h}小时")
        } else {
            format!("{h}h")
        }
    } else if lang_zh {
        format!("{h}小时{m}分钟")
    } else {
        format!("{h}h{m}m")
    }
}

/// Notification body: "{vendor} · {window}: {n}% left (exhausted in ~{eta})".
pub fn build_body(cand: &NotifyCandidate, now_ms: i64, lang_zh: bool) -> String {
    let remaining = format!("{:.0}", cand.remaining_pct);
    match cand.projected_ms {
        Some(ms) => {
            let eta = build_eta(((ms - now_ms) / 60_000).max(0), lang_zh);
            if lang_zh {
                format!(
                    "{}·{}：剩余 {remaining}%（预计 {eta} 后耗尽）",
                    cand.vendor, cand.window_label
                )
            } else {
                format!(
                    "{} · {}: {}% left (exhausted in ~{eta})",
                    cand.vendor, cand.window_label, remaining
                )
            }
        }
        None => {
            if lang_zh {
                format!("{}·{}：剩余 {remaining}%", cand.vendor, cand.window_label)
            } else {
                format!(
                    "{} · {}: {}% left",
                    cand.vendor, cand.window_label, remaining
                )
            }
        }
    }
}

/// Escape a string for safe interpolation into an AppleScript double-quoted
/// literal (backslash first, then the quote itself).
pub fn escape_applescript(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quota::{QuotaStatus, QuotaWindow};

    fn window(label: &str, used_pct: f64, projected: Option<&str>) -> QuotaWindow {
        QuotaWindow {
            label: label.into(),
            used_pct,
            projected_exhaustion_at: projected.map(|s| s.into()),
            ..Default::default()
        }
    }

    fn quota(vendor: &str, windows: Vec<QuotaWindow>) -> Quota {
        Quota {
            vendor: vendor.into(),
            status: QuotaStatus::Ok,
            windows,
            balance: None,
            plan_label: None,
            refreshed_at: None,
            error: None,
            cookie_error: None,
            expires_at: None,
            site: None,
        }
    }

    // ── evaluate ──────────────────────────────────────────────────────

    #[test]
    fn remaining_below_threshold_triggers() {
        let q = quota("glm", vec![window("5h", 85.0, None)]);
        let cands = evaluate(&q, 1_700_000_000_000i64);
        assert_eq!(cands.len(), 1);
        assert_eq!(cands[0].remaining_pct, 15.0);
    }

    #[test]
    fn remaining_above_threshold_no_trigger_without_projection() {
        let q = quota("glm", vec![window("5h", 50.0, None)]);
        assert!(evaluate(&q, 1_700_000_000_000i64).is_empty());
    }

    #[test]
    fn projected_within_2h_triggers() {
        let proj_iso = chrono::DateTime::from_timestamp_millis(1_700_000_000_000i64 + 3_600_000)
            .map(|dt| dt.to_rfc3339());
        let q = quota("glm", vec![window("5h", 60.0, proj_iso.as_deref())]);
        let cands = evaluate(&q, 1_700_000_000_000i64);
        assert_eq!(cands.len(), 1);
        assert_eq!(
            cands[0].projected_ms,
            Some(1_700_000_000_000i64 + 3_600_000)
        );
    }

    #[test]
    fn projected_beyond_2h_does_not_trigger() {
        let proj_iso =
            chrono::DateTime::from_timestamp_millis(1_700_000_000_000i64 + 5 * 3_600_000)
                .map(|dt| dt.to_rfc3339());
        let q = quota("glm", vec![window("5h", 60.0, proj_iso.as_deref())]);
        assert!(evaluate(&q, 1_700_000_000_000i64).is_empty());
    }

    #[test]
    fn projected_past_now_does_not_trigger() {
        let proj_iso = chrono::DateTime::from_timestamp_millis(1_700_000_000_000i64 - 3_600_000)
            .map(|dt| dt.to_rfc3339());
        let q = quota("glm", vec![window("5h", 60.0, proj_iso.as_deref())]);
        assert!(evaluate(&q, 1_700_000_000_000i64).is_empty());
    }

    #[test]
    fn exhausted_window_skipped() {
        let q = quota("glm", vec![window("5h", 100.0, None)]);
        assert!(evaluate(&q, 1_700_000_000_000i64).is_empty());
    }

    #[test]
    fn malformed_projected_ignored() {
        let q = quota("glm", vec![window("5h", 60.0, Some("not-a-date"))]);
        assert!(evaluate(&q, 1_700_000_000_000i64).is_empty());
    }

    #[test]
    fn both_triggers_same_candidate() {
        let proj_iso = chrono::DateTime::from_timestamp_millis(1_700_000_000_000i64 + 1_800_000)
            .map(|dt| dt.to_rfc3339());
        let q = quota("glm", vec![window("5h", 85.0, proj_iso.as_deref())]);
        assert_eq!(evaluate(&q, 1_700_000_000_000i64).len(), 1);
    }

    #[test]
    fn evaluate_multiple_windows() {
        let q = quota(
            "glm",
            vec![window("5h", 85.0, None), window("周", 90.0, None)],
        );
        let cands = evaluate(&q, 1_700_000_000_000i64);
        assert_eq!(cands.len(), 2);
    }

    #[test]
    fn evaluate_mixed_windows() {
        let proj_iso = chrono::DateTime::from_timestamp_millis(1_700_000_000_000i64 + 1_800_000)
            .map(|dt| dt.to_rfc3339());
        let q = quota(
            "glm",
            vec![
                window("5h", 85.0, None),
                window("周", 50.0, None),
                window("月", 100.0, None),
                window("订阅", 70.0, proj_iso.as_deref()),
            ],
        );
        let cands = evaluate(&q, 1_700_000_000_000i64);
        assert_eq!(cands.len(), 2);
        assert_eq!(cands[0].window_label, "5h");
        assert_eq!(cands[1].window_label, "订阅");
    }

    // ── NotifyDedup ───────────────────────────────────────────────────

    #[test]
    fn dedup_allows_once() {
        let mut d = NotifyDedup::default();
        let c = NotifyCandidate {
            vendor: "glm".into(),
            window_label: "5h".into(),
            remaining_pct: 15.0,
            projected_ms: None,
        };
        assert!(d.should_notify(&c));
        assert!(!d.should_notify(&c)); // same pct → blocked
    }

    #[test]
    fn dedup_reset_on_pct_drop_reallows() {
        let mut d = NotifyDedup::default();
        let c1 = NotifyCandidate {
            vendor: "glm".into(),
            window_label: "5h".into(),
            remaining_pct: 15.0,
            projected_ms: None,
        };
        assert!(d.should_notify(&c1));

        // Usage dropped → reset → re-allowed.
        let c2 = NotifyCandidate {
            vendor: "glm".into(),
            window_label: "5h".into(),
            remaining_pct: 18.0,
            projected_ms: None,
        };
        assert!(d.should_notify(&c2));

        // Back at 15% → blocked (same or lower than last-notified 18%).
        let c3 = NotifyCandidate {
            vendor: "glm".into(),
            window_label: "5h".into(),
            remaining_pct: 15.0,
            projected_ms: None,
        };
        assert!(!d.should_notify(&c3));
    }

    #[test]
    fn dedup_independent_per_pair() {
        let mut d = NotifyDedup::default();
        let a = NotifyCandidate {
            vendor: "glm".into(),
            window_label: "5h".into(),
            remaining_pct: 15.0,
            projected_ms: None,
        };
        let b = NotifyCandidate {
            vendor: "glm".into(),
            window_label: "周".into(),
            remaining_pct: 12.0,
            projected_ms: None,
        };
        assert!(d.should_notify(&a));
        assert!(!d.should_notify(&a));
        assert!(d.should_notify(&b)); // different label → independent
    }

    // ── copy builders ────────────────────────────────────────────────────

    #[test]
    fn build_title_language_variants() {
        assert_eq!(build_title(true), "额度即将耗尽");
        assert_eq!(build_title(false), "Quota nearly exhausted");
    }

    #[test]
    fn build_eta_minutes_and_hours() {
        assert_eq!(build_eta(35, true), "35分钟");
        assert_eq!(build_eta(35, false), "35m");
        assert_eq!(build_eta(60, true), "1小时");
        assert_eq!(build_eta(80, true), "1小时20分钟");
        assert_eq!(build_eta(80, false), "1h20m");
    }

    #[test]
    fn build_body_with_and_without_eta() {
        let now = 1_700_000_000_000i64;
        let with_eta = NotifyCandidate {
            vendor: "GLM".into(),
            window_label: "5h".into(),
            remaining_pct: 15.0,
            projected_ms: Some(now + 80 * 60_000),
        };
        assert_eq!(
            build_body(&with_eta, now, true),
            "GLM·5h：剩余 15%（预计 1小时20分钟 后耗尽）"
        );
        assert_eq!(
            build_body(&with_eta, now, false),
            "GLM · 5h: 15% left (exhausted in ~1h20m)"
        );
        let no_eta = NotifyCandidate {
            projected_ms: None,
            ..with_eta
        };
        assert_eq!(build_body(&no_eta, now, true), "GLM·5h：剩余 15%");
        assert_eq!(build_body(&no_eta, now, false), "GLM · 5h: 15% left");
    }

    #[test]
    fn build_body_clamps_negative_eta() {
        let now = 1_700_000_000_000i64;
        let past = NotifyCandidate {
            vendor: "GLM".into(),
            window_label: "5h".into(),
            remaining_pct: 15.0,
            projected_ms: Some(now - 5 * 60_000),
        };
        assert_eq!(
            build_body(&past, now, false),
            "GLM · 5h: 15% left (exhausted in ~0m)"
        );
    }

    // ── AppleScript escaping ─────────────────────────────────────────────

    #[test]
    fn escape_applescript_quotes_and_backslashes() {
        assert_eq!(escape_applescript("plain"), "plain");
        assert_eq!(escape_applescript(r#"say "hi""#), r#"say \"hi\""#);
        assert_eq!(escape_applescript(r"a\b"), r"a\\b");
        assert_eq!(escape_applescript("\"\\"), "\\\"\\\\");
    }
}
