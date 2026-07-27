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
use crate::tray;
use crate::{paths, storage};

/// Start the collector pipeline. Best-effort: any setup failure (no tokscale,
/// no watchable dirs) logs and returns silently rather than crashing the app —
/// the popover still works with whatever's already in the DB.
pub async fn start(app: AppHandle, db: Arc<Mutex<Connection>>) {
    // 1. resolve tokscale: prefer the bundled binary (packaged at build time);
    //    fall back to the legacy install-on-first-run path only if the bundle
    //    is missing/corrupt (e.g. dev without `npm run fetch-tokscale` yet).
    let data = match tokscale::app_bin_dir() {
        Some(d) => d,
        None => return,
    };
    let custom = tokscale::bundled_bin_path(&app);
    let bin = match tokscale::resolve_bin(custom.as_deref(), &data) {
        Ok(b) => b,
        Err(_) => match tokscale::install(&data).await {
            Ok(b) => b,
            Err(_) => return,
        },
    };

    // Warm the pricing cache in the background if missing/stale so the
    // cache-only env var forced in `run_json` has fresh data to read. The cache
    // is what keeps every tokscale call ~2s instead of ~50s (network fetch).
    tokscale::ensure_pricing_cache(&bin);

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
    // Persist the discovered installed-clients list so backend commands can
    // compute 归档会话 (sessions whose tool is no longer installed) without
    // re-running tokscale. Best-effort — a write failure only means the archive
    // count is unavailable until the next startup.
    if let Ok(conn) = db.lock() {
        let _ = crate::config::set_json(&conn, "installed_clients", &clients);
    }
    let installed_clients = clients.clone();
    let cfg = scheduler::SchedulerConfig {
        history_interval: Duration::from_secs(15 * 60),
        enabled_clients: clients,
    };
    tauri::async_runtime::spawn(scheduler::run(
        scanner,
        cfg,
        tick_rx,
        ev_tx,
        Some(db.clone()),
    ));

    // 4. consumer: persist graph, emit today:updated, update tray title.
    tauri::async_runtime::spawn(async move {
        let _watch_guard = watch_guard; // keep the watcher alive for the app's lifetime
        while let Some(ev) = ev_rx.recv().await {
            match ev {
                scheduler::CollectionEvent::Graph(v) => {
                    if let Ok(mut conn) = db.lock() {
                        let _ = storage::daily_usage::ingest_graph(&mut conn, &v);
                    }
                }
                scheduler::CollectionEvent::Sessions(v) => {
                    if let Ok(mut conn) = db.lock() {
                        let _ = storage::sessions::ingest_sessions(&mut conn, &v);
                        // 会话保留 OFF → prune sessions whose tool is no longer
                        // installed (auto-cleanup). ON (default) keeps everything.
                        let keep = crate::config::load(&conn)
                            .map(|c| c.session_archive_enabled)
                            .unwrap_or(true);
                        if !keep {
                            let _ = storage::sessions::prune_uninstalled(&conn, &installed_clients);
                        }
                    }
                }
                scheduler::CollectionEvent::TodaySummary(v) => {
                    if let Ok(conn) = db.lock() {
                        tray::update_from_json(&app, &v, &conn);
                    }
                    if let Some(s) = summary::from_today_json(&v) {
                        let _ = app.emit("today:updated", s);
                    }
                }
                scheduler::CollectionEvent::ScanError(_) => {}
            }
        }
    });
}
