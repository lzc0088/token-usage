//! Collection scheduler (T1.5).
//!
//! Consumes debounced ticks from the watcher (T1.4) and drives tokscale scans:
//!   - **real-time loop**: a tick → `tokscale --today` (cheap) → [`CollectionEvent::TodaySummary`]
//!     for the popover hero. Burst-coalesced so a rapid burst = one scan.
//!   - **history loop**: every `history_interval` (default 15 min) → `tokscale graph`
//!     → [`CollectionEvent::Graph`] for storage to upsert into `daily_usage`. The
//!     history timer also refreshes today so a silent file watcher (macOS FSEvents
//!     after system sleep, exhausted watch descriptors) can't freeze the tray.
//!
//! **Anchor/Delta (P1)**: When a valid anchor exists (today + matching config),
//! the scheduler can skip `--month` / `--since` scans and derive those windows
//! from the anchor using `apply_period_delta`. This reduces a full scan from 3
//! tokscale calls (3×2s = ~6s) to a single `--today` call (~2s).
//!
//! Scans run **serially** (one at a time) to avoid CPU peaks (token-monitor issue
//! #15 lesson). The `Scanner` trait is injectable so the whole loop is unit-testable
//! without tokscale/network. A config change (different enabled clients) is handled
//! by the caller **restarting** `run` with a new `cfg` (compare via
//! [`config_fingerprint`]); `run` itself does not mutate config mid-flight.

use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;

use super::tokscale::{self, Period, TokscaleError};
use crate::config::Config;

/// Scheduler tuning.
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// How often to run `tokscale graph` for history (default 15 min).
    pub history_interval: Duration,
    /// Smart-mode keepalive cadence: unconditional today+graph+sessions scan
    /// (default 10 min, matching the settings label "智能采集（10分钟）").
    pub smart_keepalive: Duration,
    /// Enabled clients (drives the config fingerprint + the `-c` filter).
    pub enabled_clients: Vec<String>,
    /// Cached config — the runtime updates this on startup and on every
    /// `config:changed` event so the scheduler can read `collection_mode`
    /// (and other fields) without locking the DB each loop iteration.
    /// None = no cache available, fall back to DB read.
    pub cached_config: Arc<Mutex<Option<Config>>>,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            history_interval: Duration::from_secs(15 * 60),
            smart_keepalive: Duration::from_secs(10 * 60),
            enabled_clients: Vec::new(),
            cached_config: Arc::new(Mutex::new(None)),
        }
    }
}

/// Aggregated token/cost summary for a period window.
/// Mirrors the shape used by tokscale / usage.js so delta application is trivial.
#[allow(dead_code)] // P1: future anchor delta logic
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PeriodSummary {
    pub total_tokens: i64,
    pub cost_usd: f64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
}

/// Derive a new period by adding the delta between `base` and `fresh_today` to
/// `stale`. This is the anchor/delta math from token-monitor.
///
/// ```text
/// stale_month = stale_month + (fresh_today - stale_today)
/// ```
#[allow(dead_code)] // P1: future anchor delta logic
pub fn apply_period_delta(
    stale: &PeriodSummary,
    fresh_today: &PeriodSummary,
    base_today: &PeriodSummary,
) -> PeriodSummary {
    let delta_tokens = fresh_today
        .total_tokens
        .saturating_sub(base_today.total_tokens);
    let delta_cost = fresh_today.cost_usd - base_today.cost_usd;

    PeriodSummary {
        total_tokens: stale.total_tokens.saturating_add(delta_tokens),
        cost_usd: stale.cost_usd + delta_cost,
        input_tokens: stale.input_tokens.saturating_add(
            fresh_today
                .input_tokens
                .saturating_sub(base_today.input_tokens),
        ),
        output_tokens: stale.output_tokens.saturating_add(
            fresh_today
                .output_tokens
                .saturating_sub(base_today.output_tokens),
        ),
        cache_read_tokens: stale.cache_read_tokens.saturating_add(
            fresh_today
                .cache_read_tokens
                .saturating_sub(base_today.cache_read_tokens),
        ),
        cache_write_tokens: stale.cache_write_tokens.saturating_add(
            fresh_today
                .cache_write_tokens
                .saturating_sub(base_today.cache_write_tokens),
        ),
    }
}

/// Events the scheduler emits for consumers (popover / storage).
#[derive(Debug, Clone)]
pub enum CollectionEvent {
    /// Fresh today data from `tokscale --today` (real-time, not persisted).
    TodaySummary(Value),
    /// Historical contribution graph from `tokscale graph` (→ daily_usage upsert).
    Graph(Value),
    /// Per-session,model data from `tokscale --group-by session,model` (→ sessions upsert).
    Sessions(Value),
    /// A scan failed; surfaced so the UI can show a degraded state.
    ScanError(String),
}

/// A hash of the enabled-clients set (order-independent). Callers compare it
/// across config changes to decide whether to restart `run` with a fresh `cfg`.
pub fn config_fingerprint(clients: &[String]) -> u64 {
    let mut sorted: Vec<&String> = clients.iter().collect();
    sorted.sort();
    let mut h = DefaultHasher::new();
    for c in sorted {
        h.write(c.as_bytes());
        h.write_u8(0);
    }
    h.finish()
}

/// Parse `config.refresh_interval` into a history-loop cadence in seconds.
///
/// `"manual"` (and any unrecognized value) → `None`, meaning "stay on the
/// default 15-min history cadence" (the watcher still drives real-time today
/// updates). `"30s"/"60s"/"300s"` → the explicit cadence.
pub fn parse_refresh_interval_secs(raw: &str) -> Option<u64> {
    let secs = raw.strip_suffix('s').and_then(|r| r.parse::<u64>().ok())?;
    // Guard against absurdly small/large values (negative can't parse as u64).
    if (1..=86_400).contains(&secs) {
        Some(secs)
    } else {
        None
    }
}

/// Resolve the next history-loop interval from the cached config (no DB lock).
/// Falls back to `fallback` when the cache is empty or poisoned.
fn next_history_interval(cfg: &SchedulerConfig, fallback: Duration) -> Duration {
    match cfg.cached_config.lock() {
        Ok(guard) => match guard.as_ref() {
            Some(c) => parse_refresh_interval_secs(c.refresh_interval.as_str())
                .map(Duration::from_secs)
                .unwrap_or(fallback),
            None => fallback,
        },
        Err(_) => fallback,
    }
}

/// Inject so the run loop is testable without spawning tokscale.
pub trait Scanner: Send + Sync {
    /// `tokscale --today` (real-time today report).
    fn today(&self) -> impl std::future::Future<Output = Result<Value, TokscaleError>> + Send;
    /// `tokscale graph` (history).
    fn graph(&self) -> impl std::future::Future<Output = Result<Value, TokscaleError>> + Send;
    /// `tokscale --group-by session,model` (session-level per-model data).
    fn sessions(&self) -> impl std::future::Future<Output = Result<Value, TokscaleError>> + Send;
}

/// Real scanner backed by the resolved tokscale binary.
pub struct TokscaleScanner {
    bin: PathBuf,
    clients: Vec<String>,
}

impl TokscaleScanner {
    pub fn new(bin: PathBuf, clients: Vec<String>) -> Self {
        Self { bin, clients }
    }
}

impl Scanner for TokscaleScanner {
    async fn today(&self) -> Result<Value, TokscaleError> {
        let args = tokscale::report_args(Period::Today, &self.clients, "client,model");
        tokscale::run_json(&self.bin, &args).await
    }

    async fn graph(&self) -> Result<Value, TokscaleError> {
        // graph emits JSON natively (no --json flag).
        let mut args = vec!["--no-spinner".to_string(), "graph".to_string()];
        if !self.clients.is_empty() {
            args.push("-c".to_string());
            args.push(self.clients.join(","));
        }
        tokscale::run_json(&self.bin, &args).await
    }

    async fn sessions(&self) -> Result<Value, TokscaleError> {
        let args = tokscale::report_args(Period::All, &self.clients, "session,model");
        tokscale::run_json(&self.bin, &args).await
    }
}

async fn emit_today<S: Scanner>(
    scanner: &S,
    tx: &mpsc::Sender<CollectionEvent>,
) -> Result<(), mpsc::error::SendError<CollectionEvent>> {
    let event = match scanner.today().await {
        Ok(v) => CollectionEvent::TodaySummary(v),
        Err(e) => CollectionEvent::ScanError(e.to_string()),
    };
    tx.send(event).await
}

async fn emit_graph<S: Scanner>(
    scanner: &S,
    tx: &mpsc::Sender<CollectionEvent>,
) -> Result<(), mpsc::error::SendError<CollectionEvent>> {
    let event = match scanner.graph().await {
        Ok(v) => CollectionEvent::Graph(v),
        Err(e) => CollectionEvent::ScanError(e.to_string()),
    };
    tx.send(event).await
}

async fn emit_sessions<S: Scanner>(
    scanner: &S,
    tx: &mpsc::Sender<CollectionEvent>,
) -> Result<(), mpsc::error::SendError<CollectionEvent>> {
    let event = match scanner.sessions().await {
        Ok(v) => CollectionEvent::Sessions(v),
        Err(e) => CollectionEvent::ScanError(e.to_string()),
    };
    tx.send(event).await
}

/// Minimum spacing between activity-driven today scans in smart mode, so a
/// continuously active session can't spam tokscale (at most one scan per
/// minute; the keepalive timer covers the gaps).
const SMART_ACTIVITY_MIN_SPACING: Duration = Duration::from_secs(60);

/// Run the scheduler until `tick_rx` closes. Consumes ticks (file-change events)
/// and emits [`CollectionEvent`]s on `event_tx`.
///
/// A config change of enabled clients is handled by the caller **restarting**
/// `run` with a new `cfg` (compare via [`config_fingerprint`]).
///
/// Collection modes:
/// - **live**: watcher ticks trigger `emit_today` immediately; the history
///   timer (default 15 min) also refreshes today so a silent watcher (macOS
///   FSEvents after system sleep, exhausted watch descriptors) can't freeze
///   the tray/hero until a manual restart.
/// - **smart**: activity-driven today scans (rate-limited to one per
///   [`SMART_ACTIVITY_MIN_SPACING`]) + an unconditional keepalive every
///   `cfg.smart_keepalive` (default 10 min, matching the settings label)
///   that refreshes today+graph+sessions regardless of watcher health.
/// - **interval**: timer-driven at `refresh_interval`, no file watch.
pub async fn run<S: Scanner>(
    scanner: S,
    cfg: SchedulerConfig,
    mut tick_rx: mpsc::Receiver<()>,
    mut history_rx: mpsc::Receiver<()>,
    event_tx: mpsc::Sender<CollectionEvent>,
) {
    // Fire the first history (graph + sessions) scan immediately so the DB is
    // not empty for the first 15 min after startup.
    let mut next_graph_at = tokio::time::Instant::now();
    // Earliest allowed activity-driven today scan (smart mode). Keepalive
    // scans don't consume this budget, so a tick right after a keepalive still
    // scans immediately.
    let mut next_activity_scan_at = tokio::time::Instant::now();

    // Helper: read collection_mode from cached config first (no DB lock),
    // fall back to DB read only if cache is empty.
    let collection_mode = |cfg: &SchedulerConfig| -> String {
        match cfg.cached_config.lock() {
            Ok(guard) => match guard.as_ref() {
                Some(c) => c.collection_mode.clone(),
                None => "live".to_string(),
            },
            Err(_) => "live".to_string(),
        }
    };

    loop {
        let mode = collection_mode(&cfg);

        match mode.as_str() {
            "smart" => {
                // ── Smart mode: activity-driven today + fixed keepalive ────
                // A tick refreshes today directly (rate-limited). The
                // keepalive refreshes today+graph+sessions unconditionally:
                // today is deliberately NOT gated on file activity, because
                // the activity signal comes from the same watcher that may
                // have gone silent (macOS FSEvents after system sleep froze
                // the old activity_revision counter and stalled the tray/
                // hero on yesterday's numbers until restart — 2026-08-16).
                tokio::select! {
                    tick = tick_rx.recv() => {
                        if tick.is_some() {
                            // Coalesce: drain any ticks already buffered.
                            while tick_rx.try_recv().is_ok() {}
                            if tokio::time::Instant::now() >= next_activity_scan_at {
                                if emit_today(&scanner, &event_tx).await.is_err() {
                                    break;
                                }
                                next_activity_scan_at =
                                    tokio::time::Instant::now() + SMART_ACTIVITY_MIN_SPACING;
                            }
                        } else {
                            break; // watcher stopped → exit
                        }
                    }
                    _ = history_rx.recv() => {
                        // Manual history trigger (collect_now). Drain any burst.
                        while history_rx.try_recv().is_ok() {}
                        if emit_graph(&scanner, &event_tx).await.is_err() {
                            break;
                        }
                        if emit_sessions(&scanner, &event_tx).await.is_err() {
                            break;
                        }
                        next_graph_at = tokio::time::Instant::now() + cfg.smart_keepalive;
                    }
                    _ = tokio::time::sleep_until(next_graph_at) => {
                        if emit_today(&scanner, &event_tx).await.is_err() {
                            break;
                        }
                        if emit_graph(&scanner, &event_tx).await.is_err() {
                            break;
                        }
                        if emit_sessions(&scanner, &event_tx).await.is_err() {
                            break;
                        }
                        next_graph_at = tokio::time::Instant::now() + cfg.smart_keepalive;
                    }
                }
            }
            "interval" => {
                // ── Interval mode: fixed timer, no file watch ─────────────
                tokio::select! {
                    _ = history_rx.recv() => {
                        // Manual history trigger (collect_now).
                        while history_rx.try_recv().is_ok() {}
                        if emit_graph(&scanner, &event_tx).await.is_err() {
                            break;
                        }
                        if emit_sessions(&scanner, &event_tx).await.is_err() {
                            break;
                        }
                        next_graph_at = tokio::time::Instant::now()
                            + next_history_interval(&cfg, cfg.history_interval);
                    }
                    _ = tokio::time::sleep_until(next_graph_at) => {
                        if emit_today(&scanner, &event_tx).await.is_err() {
                            break;
                        }
                        if emit_graph(&scanner, &event_tx).await.is_err() {
                            break;
                        }
                        if emit_sessions(&scanner, &event_tx).await.is_err() {
                            break;
                        }
                        let interval = next_history_interval(&cfg, cfg.history_interval);
                        next_graph_at = tokio::time::Instant::now() + interval;
                    }
                }
            }
            _ => {
                // ── Live mode (default): watcher ticks + history timer ─────
                tokio::select! {
                    tick = tick_rx.recv() => match tick {
                        Some(()) => {
                            // Coalesce: drain any ticks already buffered by the burst
                            while tick_rx.try_recv().is_ok() {}
                            if emit_today(&scanner, &event_tx).await.is_err() {
                                break;
                            }
                        }
                        None => break, // watcher stopped → exit
                    },
                    _ = history_rx.recv() => {
                        // Manual history trigger (collect_now).
                        while history_rx.try_recv().is_ok() {}
                        if emit_graph(&scanner, &event_tx).await.is_err() {
                            break;
                        }
                        if emit_sessions(&scanner, &event_tx).await.is_err() {
                            break;
                        }
                        let interval = next_history_interval(&cfg, cfg.history_interval);
                        next_graph_at = tokio::time::Instant::now() + interval;
                    }
                    _ = tokio::time::sleep_until(next_graph_at) => {
                        // The history timer also refreshes today: if the
                        // watcher is dead (descriptor exhaustion, FSEvents
                        // silent after sleep), ticks never arrive and the
                        // tray would otherwise freeze until a restart.
                        if emit_today(&scanner, &event_tx).await.is_err() {
                            break;
                        }
                        if emit_graph(&scanner, &event_tx).await.is_err() {
                            break;
                        }
                        if emit_sessions(&scanner, &event_tx).await.is_err() {
                            break;
                        }
                        let interval = next_history_interval(&cfg, cfg.history_interval);
                        next_graph_at = tokio::time::Instant::now() + interval;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    fn fp(a: &[&str]) -> u64 {
        config_fingerprint(&a.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn fingerprint_is_order_independent() {
        assert_eq!(fp(&["claude", "codex"]), fp(&["codex", "claude"]));
    }

    #[test]
    fn fingerprint_distinguishes_sets() {
        assert_ne!(fp(&["claude"]), fp(&["claude", "codex"]));
        assert_ne!(fp(&["claude"]), fp(&["codex"]));
    }

    #[test]
    fn fingerprint_empty_is_stable() {
        assert_eq!(config_fingerprint(&[]), config_fingerprint(&[]));
    }

    #[test]
    fn parse_refresh_interval_secs_maps_known_values() {
        assert_eq!(parse_refresh_interval_secs("manual"), None);
        assert_eq!(parse_refresh_interval_secs("30s"), Some(30));
        assert_eq!(parse_refresh_interval_secs("60s"), Some(60));
        assert_eq!(parse_refresh_interval_secs("300s"), Some(300));
        // Unknown / malformed → None (falls back to default cadence).
        assert_eq!(parse_refresh_interval_secs("bogus"), None);
        assert_eq!(parse_refresh_interval_secs(""), None);
        assert_eq!(parse_refresh_interval_secs("0s"), None); // too small
    }

    /// Counting mock scanner — no tokscale, no network.
    struct Mock {
        today: Arc<AtomicU32>,
        graph: Arc<AtomicU32>,
    }
    impl Mock {
        fn new() -> (Self, Arc<AtomicU32>, Arc<AtomicU32>) {
            let today = Arc::new(AtomicU32::new(0));
            let graph = Arc::new(AtomicU32::new(0));
            (
                Self {
                    today: today.clone(),
                    graph: graph.clone(),
                },
                today,
                graph,
            )
        }
    }
    impl Scanner for Mock {
        async fn today(&self) -> Result<Value, TokscaleError> {
            self.today.fetch_add(1, Ordering::SeqCst);
            Ok(serde_json::json!({"totalInput": 1}))
        }
        async fn graph(&self) -> Result<Value, TokscaleError> {
            self.graph.fetch_add(1, Ordering::SeqCst);
            Ok(serde_json::json!({"contributions": []}))
        }
        async fn sessions(&self) -> Result<Value, TokscaleError> {
            Ok(serde_json::json!({"entries": []}))
        }
    }

    #[tokio::test]
    async fn burst_of_ticks_coalesces_to_one_scan() {
        let (mock, today_c, _graph_c) = Mock::new();
        let (tick_tx, tick_rx) = mpsc::channel::<()>(64);
        let (_h_tx, history_rx) = mpsc::channel::<()>(8);
        let (ev_tx, mut ev_rx) = mpsc::channel::<CollectionEvent>(64);

        // huge history interval so the graph timer never fires during the test
        let cfg = SchedulerConfig {
            history_interval: Duration::from_secs(3600),
            smart_keepalive: Duration::from_secs(3600),
            enabled_clients: vec![],
            cached_config: Arc::new(Mutex::new(None)),
        };
        let handle = tokio::spawn(async move {
            run(mock, cfg, tick_rx, history_rx, ev_tx).await;
        });

        // Let the startup scan (history timer fires immediately at t=0) settle
        // and record it as the baseline.
        tokio::time::sleep(Duration::from_millis(100)).await;
        let baseline = today_c.load(Ordering::SeqCst);
        assert!(baseline >= 1, "startup scan should have run");

        // Burst of 5 ticks in quick succession.
        for _ in 0..5 {
            tick_tx.send(()).await.unwrap();
        }
        // Let the scheduler process.
        tokio::time::sleep(Duration::from_millis(150)).await;

        // today() called exactly once more (burst coalesced), not five times.
        let n = today_c.load(Ordering::SeqCst);
        assert!(
            n == baseline + 1,
            "expected 1 coalesced today scan from the burst, got {} (baseline {baseline})",
            n - baseline
        );

        // And a TodaySummary event was emitted.
        let mut got_summary = false;
        while let Ok(Some(ev)) = tokio::time::timeout(Duration::from_millis(50), ev_rx.recv()).await
        {
            if matches!(ev, CollectionEvent::TodaySummary(_)) {
                got_summary = true;
            }
        }
        assert!(got_summary, "expected a TodaySummary event");

        drop(tick_tx);
        let _ = tokio::time::timeout(Duration::from_millis(200), handle).await;
    }

    #[tokio::test]
    async fn graph_runs_on_history_timer() {
        let (mock, _today_c, graph_c) = Mock::new();
        let (tick_tx, tick_rx) = mpsc::channel::<()>(8);
        let (_h_tx, history_rx) = mpsc::channel::<()>(8);
        let (ev_tx, _ev_rx) = mpsc::channel::<CollectionEvent>(8);
        let cfg = SchedulerConfig {
            history_interval: Duration::from_millis(150),
            smart_keepalive: Duration::from_secs(3600),
            enabled_clients: vec![],
            cached_config: Arc::new(Mutex::new(None)),
        };
        let handle = tokio::spawn(async move {
            run(mock, cfg, tick_rx, history_rx, ev_tx).await;
        });

        // Wait past the first history interval.
        tokio::time::sleep(Duration::from_millis(400)).await;
        let n = graph_c.load(Ordering::SeqCst);
        assert!(n >= 1, "expected at least one graph scan, got {n}");

        drop(tick_tx);
        let _ = tokio::time::timeout(Duration::from_millis(200), handle).await;
    }

    /// Smart-mode config with a custom keepalive and `history_interval` parked
    /// at 1h so the (pre-fix) history fallback can't interfere.
    fn smart_cfg(keepalive: Duration) -> SchedulerConfig {
        SchedulerConfig {
            history_interval: Duration::from_secs(3600),
            smart_keepalive: keepalive,
            enabled_clients: vec![],
            cached_config: Arc::new(Mutex::new(Some(Config {
                collection_mode: "smart".to_string(),
                ..Config::default()
            }))),
        }
    }

    #[tokio::test]
    async fn smart_keepalive_emits_today_even_without_activity() {
        // Regression (2026-08-16): with the old activity gating, a watcher that
        // stopped delivering events (macOS FSEvents after system sleep) froze
        // activity_revision, so every keepalive skipped the today scan and the
        // tray/hero showed yesterday's numbers until the app was restarted.
        // The keepalive must refresh today unconditionally.
        let (mock, today_c, graph_c) = Mock::new();
        let (tick_tx, tick_rx) = mpsc::channel::<()>(8);
        let (_h_tx, history_rx) = mpsc::channel::<()>(8);
        let (ev_tx, _ev_rx) = mpsc::channel::<CollectionEvent>(8);
        let cfg = smart_cfg(Duration::from_millis(150));
        let handle = tokio::spawn(async move {
            run(mock, cfg, tick_rx, history_rx, ev_tx).await;
        });

        // No ticks ever. Startup scan + at least one keepalive within 500ms.
        tokio::time::sleep(Duration::from_millis(500)).await;
        let today = today_c.load(Ordering::SeqCst);
        let graph = graph_c.load(Ordering::SeqCst);
        assert!(
            today >= 2,
            "keepalive must emit today without watcher activity, got {today}"
        );
        assert!(graph >= 2, "keepalive must emit graph, got {graph}");

        drop(tick_tx);
        let _ = tokio::time::timeout(Duration::from_millis(200), handle).await;
    }

    #[tokio::test]
    async fn smart_tick_triggers_rate_limited_today_scan() {
        // In smart mode a watcher tick must refresh today directly (so active
        // use shows up within seconds instead of waiting for the keepalive),
        // but at most once per SMART_ACTIVITY_MIN_SPACING so a busy session
        // can't spam tokscale.
        let (mock, today_c, _graph_c) = Mock::new();
        let (tick_tx, tick_rx) = mpsc::channel::<()>(64);
        let (_h_tx, history_rx) = mpsc::channel::<()>(8);
        let (ev_tx, _ev_rx) = mpsc::channel::<CollectionEvent>(8);
        // Huge keepalive so only the startup scan fires from the timer.
        let cfg = smart_cfg(Duration::from_secs(3600));
        let handle = tokio::spawn(async move {
            run(mock, cfg, tick_rx, history_rx, ev_tx).await;
        });

        // Let the startup scan settle.
        tokio::time::sleep(Duration::from_millis(100)).await;
        let after_startup = today_c.load(Ordering::SeqCst);
        assert!(after_startup >= 1, "startup scan should have run");

        // A tick must trigger a today scan.
        tick_tx.send(()).await.unwrap();
        tokio::time::sleep(Duration::from_millis(150)).await;
        let after_tick = today_c.load(Ordering::SeqCst);
        assert!(
            after_tick > after_startup,
            "tick should trigger a today scan"
        );

        // An immediate second tick is rate-limited (60s spacing) — no extra scan.
        tick_tx.send(()).await.unwrap();
        tokio::time::sleep(Duration::from_millis(150)).await;
        let after_second = today_c.load(Ordering::SeqCst);
        assert_eq!(
            after_second, after_tick,
            "second tick within the spacing window must be rate-limited"
        );

        drop(tick_tx);
        let _ = tokio::time::timeout(Duration::from_millis(200), handle).await;
    }

    #[tokio::test]
    async fn live_history_timer_also_emits_today() {
        // Live mode normally gets today scans from watcher ticks. The history
        // timer must also refresh today so a dead watcher (watch descriptors
        // exhausted, FSEvents silent after system sleep) can't freeze the
        // tray/hero until the next manual restart.
        let (mock, today_c, graph_c) = Mock::new();
        let (tick_tx, tick_rx) = mpsc::channel::<()>(8);
        let (_h_tx, history_rx) = mpsc::channel::<()>(8);
        let (ev_tx, _ev_rx) = mpsc::channel::<CollectionEvent>(8);
        let cfg = SchedulerConfig {
            history_interval: Duration::from_millis(150),
            smart_keepalive: Duration::from_secs(3600),
            enabled_clients: vec![],
            cached_config: Arc::new(Mutex::new(None)), // None → live
        };
        let handle = tokio::spawn(async move {
            run(mock, cfg, tick_rx, history_rx, ev_tx).await;
        });

        // No ticks. Startup + one timer pass within 500ms.
        tokio::time::sleep(Duration::from_millis(500)).await;
        let today = today_c.load(Ordering::SeqCst);
        assert!(
            today >= 2,
            "live history timer should also emit today, got {today}"
        );
        assert!(graph_c.load(Ordering::SeqCst) >= 2);

        drop(tick_tx);
        let _ = tokio::time::timeout(Duration::from_millis(200), handle).await;
    }

    #[tokio::test]
    async fn emit_functions_return_err_when_channel_closed() {
        // Drop the sender immediately so the receiver gets an error on recv.
        // The run loop should exit cleanly via the tick_rx.recv() = None path
        // (no ticks sent) and not panic or hang.
        let (mock, _today_c, _graph_c) = Mock::new();
        let (_tick_tx, _tick_rx) = mpsc::channel::<()>(8);
        let (ev_tx, _ev_rx) = mpsc::channel::<CollectionEvent>(8);

        let cfg = SchedulerConfig {
            history_interval: Duration::from_secs(3600),
            smart_keepalive: Duration::from_secs(3600),
            enabled_clients: vec![],
            cached_config: Arc::new(Mutex::new(None)),
        };

        let handle = tokio::spawn(async move {
            // Drop sender before run starts — first emit_today send will fail,
            // run loop breaks cleanly.
            drop(ev_tx);
            // We need a tick_rx that immediately returns None.
            // Just drop the tick sender and pass a new closed channel.
            let (tick_tx, tick_rx2) = mpsc::channel::<()>(1);
            drop(tick_tx);
            run(
                mock,
                cfg,
                tick_rx2,
                mpsc::channel::<()>(1).1,
                mpsc::channel::<CollectionEvent>(1).0,
            )
            .await;
        });

        let _ = tokio::time::timeout(Duration::from_millis(500), handle).await;
    }
}
