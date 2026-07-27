//! Collection scheduler (T1.5).
//!
//! Consumes debounced ticks from the watcher (T1.4) and drives tokscale scans:
//!   - **real-time loop**: a tick → `tokscale --today` (cheap) → [`CollectionEvent::TodaySummary`]
//!     for the popover hero. Burst-coalesced so a rapid burst = one scan.
//!   - **history loop**: every `history_interval` (default 15 min) → `tokscale graph`
//!     → [`CollectionEvent::Graph`] for storage to upsert into `daily_usage`.
//!
//! Scans run **serially** (one at a time) to avoid CPU peaks (token-monitor issue
//! #15 lesson). The `Scanner` trait is injectable so the whole loop is unit-testable
//! without tokscale/network. A config change (different enabled clients) is handled
//! by the caller **restarting** `run` with a new `cfg` (compare via
//! [`config_fingerprint`]); `run` itself does not mutate config mid-flight.
//!
//! Deviation from plan: the anchor/delta incremental derivation (token-monitor)
//! is deferred. With SQLite storing `graph` data and month/total lagging ≤15 min
//! by design, the anchor machinery is unnecessary complexity; revisit if perf
//! demands sub-interval freshness (see docs/plan.md T1.5).

use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::Connection;
use serde_json::Value;
use tokio::sync::mpsc;

use super::tokscale::{self, Period, TokscaleError};
use crate::config;

/// Scheduler tuning.
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// How often to run `tokscale graph` for history (default 15 min).
    pub history_interval: Duration,
    /// Enabled clients (drives the config fingerprint + the `-c` filter).
    pub enabled_clients: Vec<String>,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            history_interval: Duration::from_secs(15 * 60),
            enabled_clients: Vec::new(),
        }
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
/// updates). `"30s"/"60s"/"300s"/"600s"` → the explicit cadence.
pub fn parse_refresh_interval_secs(raw: &str) -> Option<u64> {
    let secs = raw.strip_suffix('s').and_then(|r| r.parse::<u64>().ok())?;
    // Guard against absurdly small/large values (negative can't parse as u64).
    if (1..=86_400).contains(&secs) {
        Some(secs)
    } else {
        None
    }
}

/// Resolve the next history-loop interval from config when a DB handle is
/// available; falls back to `cfg.history_interval` otherwise (tests / no DB).
fn next_history_interval(
    db: &Option<Arc<Mutex<Connection>>>,
    fallback: Duration,
) -> Duration {
    let Some(db) = db else {
        return fallback;
    };
    let Some(conn) = db.lock().ok() else {
        return fallback;
    };
    let cfg = config::load(&conn).unwrap_or_default();
    match parse_refresh_interval_secs(cfg.refresh_interval.as_str()) {
        Some(secs) => Duration::from_secs(secs),
        None => fallback,
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

async fn emit_today<S: Scanner>(scanner: &S, tx: &mpsc::Sender<CollectionEvent>) {
    match scanner.today().await {
        Ok(v) => {
            let _ = tx.send(CollectionEvent::TodaySummary(v)).await;
        }
        Err(e) => {
            let _ = tx.send(CollectionEvent::ScanError(e.to_string())).await;
        }
    }
}

async fn emit_graph<S: Scanner>(scanner: &S, tx: &mpsc::Sender<CollectionEvent>) {
    match scanner.graph().await {
        Ok(v) => {
            let _ = tx.send(CollectionEvent::Graph(v)).await;
        }
        Err(e) => {
            let _ = tx.send(CollectionEvent::ScanError(e.to_string())).await;
        }
    }
}

async fn emit_sessions<S: Scanner>(scanner: &S, tx: &mpsc::Sender<CollectionEvent>) {
    match scanner.sessions().await {
        Ok(v) => {
            let _ = tx.send(CollectionEvent::Sessions(v)).await;
        }
        Err(e) => {
            let _ = tx.send(CollectionEvent::ScanError(e.to_string())).await;
        }
    }
}

/// Run the scheduler until `tick_rx` closes. Consumes ticks (file-change events)
/// and emits [`CollectionEvent`]s on `event_tx`.
///
/// `db` is read on each history-loop iteration to pick up
/// `config.refresh_interval` changes without a restart (None in tests → the
/// scheduler uses `cfg.history_interval` verbatim). A config change of enabled
/// clients is still handled by the caller **restarting** `run` with a new `cfg`.
pub async fn run<S: Scanner>(
    scanner: S,
    cfg: SchedulerConfig,
    mut tick_rx: mpsc::Receiver<()>,
    event_tx: mpsc::Sender<CollectionEvent>,
    db: Option<Arc<Mutex<Connection>>>,
) {
    // Fire the first history (graph + sessions) scan immediately so the DB is
    // not empty for the first 15 min after startup.
    let mut next_graph_at = tokio::time::Instant::now();

    loop {
        tokio::select! {
            // File-change tick (already debounced by the watcher).
            tick = tick_rx.recv() => match tick {
                Some(()) => {
                    // Coalesce: drain any ticks already buffered by the burst,
                    // then run a single today scan. (Ticks arriving *during* the
                    // scan are caught on the next loop iteration — warranted,
                    // they represent new activity.)
                    while tick_rx.try_recv().is_ok() {}
                    emit_today(&scanner, &event_tx).await;
                }
                None => break, // watcher stopped → exit
            },
            // History graph timer — always armed. The interval is re-read from
            // config each iteration so the 采集频率 setting takes effect within
            // one cycle (no restart needed); falls back to cfg.history_interval.
            _ = tokio::time::sleep_until(next_graph_at) => {
                emit_graph(&scanner, &event_tx).await;
                emit_sessions(&scanner, &event_tx).await;
                let interval = next_history_interval(&db, cfg.history_interval);
                next_graph_at = tokio::time::Instant::now() + interval;
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
        assert_eq!(parse_refresh_interval_secs("600s"), Some(600));
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
        let (ev_tx, mut ev_rx) = mpsc::channel::<CollectionEvent>(64);

        // huge history interval so the graph timer never fires during the test
        let cfg = SchedulerConfig {
            history_interval: Duration::from_secs(3600),
            enabled_clients: vec![],
        };
        let handle = tokio::spawn(async move {
            run(mock, cfg, tick_rx, ev_tx, None).await;
        });

        // Burst of 5 ticks in quick succession.
        for _ in 0..5 {
            tick_tx.send(()).await.unwrap();
        }
        // Let the scheduler process.
        tokio::time::sleep(Duration::from_millis(150)).await;

        // today() called once (burst coalesced), not five times.
        let n = today_c.load(Ordering::SeqCst);
        assert!(n == 1, "expected 1 coalesced today scan, got {n}");

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
        let (ev_tx, _ev_rx) = mpsc::channel::<CollectionEvent>(8);
        let cfg = SchedulerConfig {
            history_interval: Duration::from_millis(150),
            enabled_clients: vec![],
        };
        let handle = tokio::spawn(async move {
            run(mock, cfg, tick_rx, ev_tx, None).await;
        });

        // Wait past the first history interval.
        tokio::time::sleep(Duration::from_millis(400)).await;
        let n = graph_c.load(Ordering::SeqCst);
        assert!(n >= 1, "expected at least one graph scan, got {n}");

        drop(tick_tx);
        let _ = tokio::time::timeout(Duration::from_millis(200), handle).await;
    }
}
