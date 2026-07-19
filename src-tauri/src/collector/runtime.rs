//! Collector runtime wiring (M3 T3.3). At startup: resolve tokscale, discover
//! tool dirs, start the watcher + scheduler, and run a consumer that persists
//! Graph events into `daily_usage` and emits `today:updated` Tauri events for
//! TodaySummary events (real-time Hero refresh).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::Connection;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

use super::{scheduler, tokscale, watcher};
use crate::query::summary;
use crate::{paths, storage};

/// Start the collector pipeline. Best-effort: any setup failure (no tokscale,
/// no watchable dirs) logs and returns silently rather than crashing the app —
/// the popover still works with whatever's already in the DB.
pub async fn start(app: AppHandle, db: Arc<Mutex<Connection>>) {
    // 1. resolve tokscale (install on first run if missing).
    let data = match tokscale::app_bin_dir() {
        Some(d) => d,
        None => return,
    };
    let bin = match tokscale::resolve_bin(None, &data) {
        Ok(b) => b,
        Err(_) => match tokscale::install(&data).await {
            Ok(b) => b,
            Err(_) => return,
        },
    };

    // 2. discover watch dirs + installed clients.
    let report = match paths::fetch_clients(&bin).await {
        Ok(r) => r,
        Err(_) => return,
    };
    let dirs = watcher::filter_watch_paths(paths::watch_paths(&report));
    if dirs.is_empty() {
        return;
    }
    let clients: Vec<String> = paths::installed_clients(&report)
        .into_iter()
        .map(|c| c.client.clone())
        .collect();

    // 3. watcher (debounced ticks) → scheduler → events.
    let (tick_tx, tick_rx) = mpsc::channel::<()>(64);
    let watch_guard = match watcher::spawn(dirs, watcher::DEFAULT_DEBOUNCE_MS, tick_tx) {
        Ok(g) => g,
        Err(_) => return,
    };
    let (ev_tx, mut ev_rx) = mpsc::channel::<scheduler::CollectionEvent>(64);
    let scanner = scheduler::TokscaleScanner::new(bin, clients.clone());
    let cfg = scheduler::SchedulerConfig {
        history_interval: Duration::from_secs(15 * 60),
        enabled_clients: clients,
    };
    tauri::async_runtime::spawn(scheduler::run(scanner, cfg, tick_rx, ev_tx));

    // 4. consumer: persist graph, emit today:updated.
    tauri::async_runtime::spawn(async move {
        let _watch_guard = watch_guard; // keep the watcher alive for the app's lifetime
        while let Some(ev) = ev_rx.recv().await {
            match ev {
                scheduler::CollectionEvent::Graph(v) => {
                    if let Ok(mut conn) = db.lock() {
                        let _ = storage::daily_usage::ingest_graph(&mut conn, &v);
                    }
                }
                scheduler::CollectionEvent::TodaySummary(v) => {
                    if let Some(s) = summary::from_today_json(&v) {
                        let _ = app.emit("today:updated", s);
                    }
                }
                scheduler::CollectionEvent::ScanError(_) => {}
            }
        }
    });
}
