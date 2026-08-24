//! Quota burn-rate tracking for the adaptive refresh mode (M8).
//!
//! A polled remaining percentage is an *upper* bound: whatever was consumed
//! since the last probe is invisible, and that error grows with the burn rate.
//! Harmless at 90% remaining, and exactly the wrong direction at 10% — at
//! roughly 4%/min a 5-minute interval can show 12% left on a window that has
//! already hit 100%.
//!
//! So the adaptive mode drives extra probes by time-to-exhaustion rather than
//! by a "remaining < N%" threshold. A threshold is wrong at both ends: it polls
//! all night for a quota that is low but idle, and stays slow for one being
//! consumed fast from a comfortable level. The single quantity that captures
//! both is
//!
//!   ttl   = remaining% / burnRate(%/min)
//!   delay = ttl / SAMPLES_AHEAD            // probe when ~1/4 of the remainder
//!                                          // has burned, floored at 60s
//!
//! Read that as a control target, NOT an invariant: the rate is measured
//! between two samples and says nothing about acceleration inside the next
//! gap. The schedule only ever *shortens* the 5-minute baseline, never
//! lengthens it, and an idle quota produces a zero rate and falls straight
//! back to the baseline cadence — so the steady state costs nothing.
//!
//! Concept ported from token-monitor's `limitsBurnRate.js` (commit 593cb69),
//! simplified for this codebase's full-cycle scheduler: instead of
//! provider-scoped probe lanes we track per-window history and return the set
//! of vendors owning urgent windows, which the scheduler refreshes as a
//! targeted pass between the baseline full refreshes.

use std::collections::HashMap;

/// Baseline full-refresh interval while adaptive is selected. The fixed
/// 1/3/5/10/15-minute options keep their exact previous meaning; adaptive is
/// its own scheduling policy ("5 minutes normally, faster when a quota is
/// about to run out"), not a modifier on the fixed intervals.
pub const ADAPTIVE_BASE_SECS: u64 = 300;

/// Urgency probes never fire faster than once a minute.
const URGENCY_FLOOR_SECS: u64 = 60;

/// Probe when roughly this fraction of the remaining quota has burned.
const SAMPLES_AHEAD: f64 = 4.0;

/// A faster burn is adopted at once, while a quiet interval only decays the
/// estimate: pausing to read code at 8% remaining must not relax the cadence
/// back to the baseline just in time for the next burst to land unseen.
const RELEASE_WEIGHT: f64 = 0.3;

/// Minimum spacing between two samples of the same window for the rate to be
/// meaningful (guards against double-recording one refresh cycle).
const MIN_SAMPLE_GAP_MS: i64 = 5_000;

/// History for one (vendor, window-label) pair.
#[derive(Debug, Clone, Copy)]
struct WindowHistory {
    /// Last observed used percentage (0..100).
    last_used_pct: f64,
    /// Last sample time (unix ms).
    last_at_ms: i64,
    /// Smoothed burn rate in used-percentage-points per minute (≥ 0).
    rate_pct_per_min: f64,
}

/// A pending urgency probe: how soon to fire and which vendors own the
/// urgent windows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Urgency {
    pub delay_secs: u64,
    pub vendors: Vec<String>,
}

/// Per-window burn-rate state, fed from the scheduler's successful quota
/// commits. In-memory only: history restarts with the app and self-heals
/// after two samples — persisting it would seed rates measured across a
/// session gap, which is exactly the "offline consumption in a second"
/// artifact token-monitor guards against.
#[derive(Debug, Default)]
pub struct BurnRateTracker {
    history: HashMap<(String, String), WindowHistory>,
}

impl BurnRateTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one successful sample for every window of a vendor's quota.
    /// Call ONLY from the fetch-commit path (a probe this runtime actually
    /// performed) — never from cached data, whose timestamps predate this
    /// runtime and would plant a bogus baseline.
    pub fn record(&mut self, vendor: &str, quota: &crate::quota::Quota, now_ms: i64) {
        for w in &quota.windows {
            let key = (vendor.to_string(), w.label.clone());
            let raw_rate = match self.history.get(&key) {
                None => {
                    // First sample: no rate measurable yet.
                    self.history.insert(
                        key,
                        WindowHistory {
                            last_used_pct: w.used_pct,
                            last_at_ms: now_ms,
                            rate_pct_per_min: 0.0,
                        },
                    );
                    continue;
                }
                Some(h) => {
                    let dt_ms = now_ms - h.last_at_ms;
                    if dt_ms < MIN_SAMPLE_GAP_MS {
                        continue; // same-cycle duplicate — keep the earlier sample
                    }
                    let dt_min = dt_ms as f64 / 60_000.0;
                    (w.used_pct - h.last_used_pct) / dt_min
                }
            };
            let prev = *self.history.get(&key).expect("history just matched");
            // Asymmetric smoothing: a faster burn is adopted at once; a slower
            // one (including a window reset, which is strongly negative) only
            // decays the estimate. Rates are clamped at 0 for urgency purposes
            // — a decreasing usage means idle, not negative urgency.
            let smoothed = if raw_rate > prev.rate_pct_per_min {
                raw_rate
            } else {
                prev.rate_pct_per_min * (1.0 - RELEASE_WEIGHT) + raw_rate * RELEASE_WEIGHT
            };
            self.history.insert(
                key,
                WindowHistory {
                    last_used_pct: w.used_pct,
                    last_at_ms: now_ms,
                    rate_pct_per_min: smoothed.max(0.0),
                },
            );
        }
    }

    /// Drop a vendor's history (e.g. after its credential is removed).
    pub fn forget(&mut self, vendor: &str) {
        self.history.retain(|(v, _), _| v != vendor);
    }

    /// The tightest urgency across all tracked windows, if any window is
    /// burning fast enough that its time-to-exhaustion lands before the
    /// baseline interval. Returns the delay (already floored at
    /// `URGENCY_FLOOR_SECS`) plus every vendor owning at least one urgent
    /// window. Balance-only vendors never take part: their headline is money,
    /// not a measurable percentage.
    pub fn urgency(&self, now_ms: i64) -> Option<Urgency> {
        let mut best: Option<(u64, String)> = None; // (delay, vendor) min by delay
        for ((vendor, _label), h) in &self.history {
            if h.rate_pct_per_min <= 0.0 {
                continue; // idle — no urgency, baseline cadence applies
            }
            // Stale windows (vendor probed long ago, targeted pass skipped)
            // contribute no urgency: their rate predates the present.
            if now_ms - h.last_at_ms > (ADAPTIVE_BASE_SECS as i64) * 1000 {
                continue;
            }
            let remaining_pct = (100.0 - h.last_used_pct).max(0.0);
            if remaining_pct <= 0.0 {
                // Already exhausted: probing sooner cannot reveal anything
                // the user can act on — the window is spent until it resets.
                continue;
            }
            let ttl_min = remaining_pct / h.rate_pct_per_min;
            let delay = ((ttl_min * 60.0) / SAMPLES_AHEAD).ceil() as u64;
            let delay = delay.max(URGENCY_FLOOR_SECS);
            match &best {
                Some((d, _)) if *d <= delay => {}
                _ => best = Some((delay, vendor.clone())),
            }
        }
        let (delay, _) = best?;
        // Collect every vendor whose own urgency lands within the winner's
        // delay window (they may as well ride the same probe).
        let horizon = delay + URGENCY_FLOOR_SECS;
        let mut vendors: Vec<String> = self
            .history
            .iter()
            .filter(|((_, _), h)| {
                if h.rate_pct_per_min <= 0.0 {
                    return false;
                }
                if now_ms - h.last_at_ms > (ADAPTIVE_BASE_SECS as i64) * 1000 {
                    return false;
                }
                let remaining_pct = (100.0 - h.last_used_pct).max(0.0);
                if remaining_pct <= 0.0 {
                    return false;
                }
                let d = ((remaining_pct / h.rate_pct_per_min) * 60.0 / SAMPLES_AHEAD).ceil()
                    as u64;
                d.max(URGENCY_FLOOR_SECS) <= horizon
            })
            .map(|((v, _), _)| v.clone())
            .collect();
        vendors.sort();
        vendors.dedup();
        Some(Urgency { delay_secs: delay, vendors })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quota::{Quota, QuotaStatus, QuotaWindow};

    fn quota_with(label: &str, used_pct: f64) -> Quota {
        Quota {
            site: None,
            vendor: "glm".into(),
            status: QuotaStatus::Ok,
            windows: vec![QuotaWindow {
                label: label.into(),
                used_pct,
                ..Default::default()
            }],
            balance: None,
            plan_label: None,
            refreshed_at: None,
            error: None,
            cookie_error: None,
            expires_at: None,
        }
    }

    #[test]
    fn first_sample_measures_no_rate() {
        let mut t = BurnRateTracker::new();
        t.record("glm", &quota_with("5h", 40.0), 1_000_000);
        assert!(t.urgency(1_000_000).is_none());
    }

    #[test]
    fn steady_burn_yields_urgency_scaled_by_samples_ahead() {
        let mut t = BurnRateTracker::new();
        // Sample 1 at t=0: 50% used.
        t.record("glm", &quota_with("5h", 50.0), 0);
        // Sample 2 at t=10min: 90% used → 4 %/min.
        t.record("glm", &quota_with("5h", 90.0), 10 * 60_000);
        // ttl = 10% / 4 = 2.5min → delay = 150s/4 = 37.5 → floored at 60s.
        let u = t.urgency(10 * 60_000).unwrap();
        assert_eq!(u.delay_secs, 60);
        assert_eq!(u.vendors, vec!["glm".to_string()]);
    }

    #[test]
    fn slow_burn_probes_before_baseline() {
        let mut t = BurnRateTracker::new();
        t.record("glm", &quota_with("5h", 40.0), 0);
        // 1 %/min: sample 2 at t=10min → 50% used.
        t.record("glm", &quota_with("5h", 50.0), 10 * 60_000);
        // ttl = 50%/1 = 50min → delay = 3000/4 = 750s — beyond the 300s
        // baseline, so the scheduler ignores it (urgency only shortens).
        let u = t.urgency(10 * 60_000).unwrap();
        assert!(u.delay_secs >= ADAPTIVE_BASE_SECS);
    }

    #[test]
    fn idle_window_has_no_urgency() {
        let mut t = BurnRateTracker::new();
        t.record("glm", &quota_with("5h", 80.0), 0);
        // Usage DROPPED (window reset) → rate negative → clamped to 0 → idle.
        t.record("glm", &quota_with("5h", 20.0), 10 * 60_000);
        assert!(t.urgency(10 * 60_000).is_none());
    }

    #[test]
    fn faster_burn_adopted_immediately_slower_decays() {
        let mut t = BurnRateTracker::new();
        t.record("glm", &quota_with("5h", 0.0), 0);
        // Fast: 0→40 in 10min = 4%/min.
        t.record("glm", &quota_with("5h", 40.0), 10 * 60_000);
        // Quiet: 40→41 in 10min = 0.1%/min. Decay: 4*0.7 + 0.1*0.3 = 2.83.
        t.record("glm", &quota_with("5h", 41.0), 20 * 60_000);
        // remaining = 59%, rate 2.83 → ttl ≈ 20.8min → delay ≈ 312s ≥ baseline.
        // (Just verify the rate survived the quiet gap: urgency still exists.)
        let u = t.urgency(20 * 60_000);
        assert!(u.is_some(), "decayed rate must stay > 0 after one quiet gap");
    }

    #[test]
    fn exhausted_window_contributes_no_urgency() {
        let mut t = BurnRateTracker::new();
        t.record("glm", &quota_with("5h", 90.0), 0);
        t.record("glm", &quota_with("5h", 100.0), 5 * 60_000);
        assert!(t.urgency(5 * 60_000).is_none());
    }

    #[test]
    fn same_cycle_duplicate_sample_ignored() {
        let mut t = BurnRateTracker::new();
        t.record("glm", &quota_with("5h", 50.0), 1_000_000);
        // Re-record at nearly the same instant: must not produce an infinite
        // (or bogus) rate from a ~zero time delta.
        t.record("glm", &quota_with("5h", 52.0), 1_000_000 + 1_000);
        assert!(t.urgency(1_000_000).is_none());
    }

    #[test]
    fn stale_history_contributes_no_urgency() {
        let mut t = BurnRateTracker::new();
        t.record("glm", &quota_with("5h", 50.0), 0);
        t.record("glm", &quota_with("5h", 90.0), 10 * 60_000);
        // A vendor that stopped being probed: its sample is now far in the
        // past relative to `now` — no urgency from ancient rates.
        let now = 10 * 60_000 + 30 * 60_000;
        assert!(t.urgency(now).is_none());
    }

    #[test]
    fn multiple_vendors_ride_the_earliest_probe() {
        let mut t = BurnRateTracker::new();
        // Vendor A: burning 4%/min at 50% → delay floored at 60s.
        t.record("aaa", &quota_with("5h", 50.0), 0);
        t.record("aaa", &quota_with("5h", 90.0), 10 * 60_000);
        // Vendor B: ~0.5%/min at 30% → ttl 140min → delay 2100s. NOT urgent
        // within A's horizon (60 + 60 = 120s) → must not ride along.
        t.record("bbb", &quota_with("周", 20.0), 0);
        t.record("bbb", &quota_with("周", 25.0), 10 * 60_000);
        let u = t.urgency(10 * 60_000).unwrap();
        assert_eq!(u.delay_secs, 60);
        assert_eq!(u.vendors, vec!["aaa".to_string()]);

        // Vendor C as urgent as A → rides along.
        t.record("ccc", &quota_with("5h", 50.0), 10 * 60_000);
        t.record("ccc", &quota_with("5h", 90.0), 20 * 60_000);
        let u2 = t.urgency(20 * 60_000).unwrap();
        assert!(u2.vendors.contains(&"ccc".to_string()));
    }

    #[test]
    fn forget_drops_vendor_history() {
        let mut t = BurnRateTracker::new();
        t.record("glm", &quota_with("5h", 50.0), 0);
        t.record("glm", &quota_with("5h", 90.0), 10 * 60_000);
        t.forget("glm");
        assert!(t.urgency(10 * 60_000).is_none());
    }
}
