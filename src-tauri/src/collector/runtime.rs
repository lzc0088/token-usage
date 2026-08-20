//! Collector runtime wiring (M3 T3.3). At startup: resolve tokscale, discover
//! tool dirs, start the watcher + scheduler, and run a consumer that persists
//! Graph events into `daily_usage` and emits `today:updated` Tauri events for
//! TodaySummary events (real-time Hero refresh).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::Connection;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc;
use tokio::time::Instant;

use super::{scheduler, tokscale, watcher};

/// Minimum interval between project snapshot rebuilds triggered by TodaySummary
/// events. The full rebuild (~4s of tokscale calls) is too expensive to run on
/// every watcher tick; rate-limiting to every 2 minutes keeps projects fresh
/// without excessive CPU/IO.
const PROJECT_SNAPSHOT_MIN_INTERVAL: Duration = Duration::from_secs(120);

#[cfg(test)]
mod tests {
    use super::scheduler;
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    /// Verify the SchedulerConfig defaults used by runtime::start() produce
    /// sensible values. The history_interval of 15 minutes is the fallback when
    /// no config-based refresh_interval is available.
    #[test]
    fn scheduler_config_defaults_are_reasonable() {
        let cfg = scheduler::SchedulerConfig {
            history_interval: std::time::Duration::from_secs(15 * 60),
            smart_keepalive: std::time::Duration::from_secs(10 * 60),
            enabled_clients: vec![],
            cached_config: Arc::new(Mutex::new(None)),
        };
        assert_eq!(cfg.history_interval, std::time::Duration::from_secs(900));
        assert_eq!(cfg.smart_keepalive, std::time::Duration::from_secs(600));
        assert!(cfg.enabled_clients.is_empty());
    }

    /// Verify that CollectionEvent variants used in the consumer loop preserve
    /// their data through the channel. This is the contract between the
    /// scheduler and the consumer in runtime.rs.
    #[test]
    fn graph_event_roundtrips_through_serde() {
        let data = json!({"entries": [{"date": "2026-07-29", "tokens": 500}]});
        // scheduler::CollectionEvent::Graph holds serde_json::Value.
        let ev = scheduler::CollectionEvent::Graph(data.clone());
        match ev {
            scheduler::CollectionEvent::Graph(v) => {
                assert_eq!(v["entries"][0]["date"], "2026-07-29");
                assert_eq!(v["entries"][0]["tokens"], 500);
            }
            _ => panic!("expected Graph variant"),
        }
    }

    #[test]
    fn today_summary_event_roundtrips() {
        let report = json!({
            "today": {"tokens": 1000, "cost": 5.0, "messages": 42},
            "tools": []
        });
        let ev = scheduler::CollectionEvent::TodaySummary(report.clone());
        match ev {
            scheduler::CollectionEvent::TodaySummary(v) => {
                assert_eq!(v["today"]["tokens"], 1000);
                assert_eq!(v["today"]["cost"], 5.0);
                assert_eq!(v["today"]["messages"], 42);
            }
            _ => panic!("expected TodaySummary variant"),
        }
    }

    #[test]
    fn scan_error_event_preserves_message() {
        let msg = "tokscale graph failed: timeout".to_string();
        let ev = scheduler::CollectionEvent::ScanError(msg.clone());
        match ev {
            scheduler::CollectionEvent::ScanError(m) => {
                assert_eq!(m, "tokscale graph failed: timeout");
            }
            _ => panic!("expected ScanError variant"),
        }
    }

    /// The sessions event test verifies the contract for session ingestion.
    #[test]
    fn sessions_event_roundtrips() {
        let data = json!({"sessions": [{"tool": "claude", "session_id": "abc123"}]});
        let ev = scheduler::CollectionEvent::Sessions(data.clone());
        match ev {
            scheduler::CollectionEvent::Sessions(v) => {
                assert_eq!(v["sessions"][0]["tool"], "claude");
                assert_eq!(v["sessions"][0]["session_id"], "abc123");
            }
            _ => panic!("expected Sessions variant"),
        }
    }

    /// Verify that parse_refresh_interval_secs (used by the scheduler's config
    /// read, which the runtime feeds via `Some(db)`) handles all config values.
    #[test]
    fn parse_refresh_interval_secs_covers_all_variants() {
        assert_eq!(scheduler::parse_refresh_interval_secs("manual"), None);
        assert_eq!(scheduler::parse_refresh_interval_secs("30s"), Some(30));
        assert_eq!(scheduler::parse_refresh_interval_secs("60s"), Some(60));
        assert_eq!(scheduler::parse_refresh_interval_secs("300s"), Some(300));
        assert_eq!(scheduler::parse_refresh_interval_secs(""), None);
        assert_eq!(scheduler::parse_refresh_interval_secs("unknown"), None);
    }
}
use crate::query::summary;
use crate::storage;
use crate::ui::floating;
use crate::ui::tray;
use crate::utils::paths;

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
    // History (graph+sessions) can also be triggered on demand by the
    // `collect_now` command via a second channel.
    let (history_tx, history_rx) = mpsc::channel::<()>(8);
    {
        // Expose senders so commands can force an immediate scan.
        let state = app.state::<crate::state::AppState>();
        *state.collector_tick.lock().unwrap() = Some(tick_tx.clone());
        *state.collector_history.lock().unwrap() = Some(history_tx.clone());
    }
    let watch_guard = match watcher::spawn(dirs, watcher::DEFAULT_DEBOUNCE_MS, tick_tx) {
        Ok(g) => g,
        Err(_) => return,
    };
    let (ev_tx, mut ev_rx) = mpsc::channel::<scheduler::CollectionEvent>(64);
    let bin_for_snapshot = bin.clone();
    let scanner = scheduler::TokscaleScanner::new(bin, clients.clone());

    // Config cache: shared between the scheduler (reader) and `set_config`
    // (writer via AppState.update_config_cache). The scheduler reads this
    // without a DB lock on every loop iteration.
    let config_cache = app.state::<crate::state::AppState>().config_cache.clone();

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
        smart_keepalive: Duration::from_secs(10 * 60),
        enabled_clients: clients,
        cached_config: config_cache,
    };
    tauri::async_runtime::spawn(scheduler::run(scanner, cfg, tick_rx, history_rx, ev_tx));

    // 4. consumer: persist graph, emit today:updated, update tray title.
    // Track when we last triggered a project snapshot rebuild from TodaySummary
    // so we can rate-limit it (the rebuild runs ~4s of tokscale calls).
    let mut last_project_snapshot = Instant::now() - PROJECT_SNAPSHOT_MIN_INTERVAL;

    tauri::async_runtime::spawn(async move {
        let _watch_guard = watch_guard; // keep the watcher alive for the app's lifetime
        while let Some(ev) = ev_rx.recv().await {
            match ev {
                scheduler::CollectionEvent::Graph(v) => {
                    if let Ok(mut conn) = db.lock() {
                        match storage::daily_usage::ingest_graph(&mut conn, &v) {
                            Ok(_) => {
                                let _ = app.emit("collection:updated", ());
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "ingest_graph failed");
                                let _ = app
                                    .emit("collection:error", format!("graph ingest failed: {e}"));
                            }
                        }
                    }
                }
                scheduler::CollectionEvent::Sessions(v) => {
                    // Phase 1: ingest sessions batch (fast upsert) — keep the
                    // lock as short as possible.
                    if let Ok(mut conn) = db.lock() {
                        match storage::sessions::ingest_sessions(&mut conn, &v) {
                            Ok(_) => {
                                let _ = app.emit("collection:updated", ());
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "ingest_sessions failed");
                                let _ = app.emit(
                                    "collection:error",
                                    format!("sessions ingest failed: {e}"),
                                );
                            }
                        }
                    }

                    // Phase 2: backfill project_path + prune — spawned off the
                    // consumer loop so file I/O and the second DB lock don't
                    // block event processing. These are best-effort; failures
                    // are logged and surfaced via collection:error.
                    let db2 = db.clone();
                    let app2 = app.clone();
                    let installed2 = installed_clients.clone();
                    tauri::async_runtime::spawn(async move {
                        // Backfill project_path from Claude JSONL cwd.
                        if installed2.iter().any(|c| c == "claude") {
                            if let Ok(mut conn) = db2.lock() {
                                if let Err(e) =
                                    backfill_claude_project_paths(&mut conn, &installed2)
                                {
                                    tracing::warn!(error = %e, "project_path backfill failed");
                                    let _ = app2.emit(
                                        "collection:error",
                                        format!("project backfill failed: {e}"),
                                    );
                                }
                            }
                        }

                        // Backfill rounds from session JSONL files.
                        if let Ok(mut conn) = db2.lock() {
                            if let Err(e) = backfill_rounds(&mut conn) {
                                tracing::warn!(error = %e, "rounds backfill failed");
                                let _ = app2.emit(
                                    "collection:error",
                                    format!("rounds backfill failed: {e}"),
                                );
                            }
                        }

                        // 会话保留 OFF → prune sessions whose tool is no longer
                        // installed (auto-cleanup). ON (default) keeps everything.
                        if let Ok(conn) = db2.lock() {
                            let keep = crate::config::load(&conn)
                                .map(|c| c.session_archive_enabled)
                                .unwrap_or(true);
                            if !keep {
                                if let Err(e) =
                                    storage::sessions::prune_uninstalled(&conn, &installed2)
                                {
                                    tracing::warn!(error = %e, "prune_uninstalled failed");
                                    let _ =
                                        app2.emit("collection:error", format!("prune failed: {e}"));
                                }
                            }
                        }
                    });

                    // P0-阶段3 (complete): precompute project snapshots for all 3
                    // periods and persist to DB. Spawned off the consumer loop so
                    // it doesn't block event processing (~2s of tokscale calls).
                    // get_projects then reads these snapshots directly (pure DB).
                    let bin_snap = bin_for_snapshot.clone();
                    let db_snap = db.clone();
                    tauri::async_runtime::spawn(async move {
                        super::project_snapshot::precompute_and_persist(bin_snap, db_snap).await;
                    });
                }
                scheduler::CollectionEvent::TodaySummary(v) => {
                    // Ingest today's per-client/model entries into daily_usage
                    // so breakdown/trends reflect live data (not 15-min stale).
                    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
                    if let Ok(mut conn) = db.lock() {
                        if let Err(e) =
                            storage::daily_usage::ingest_today_entries(&mut conn, &v, &today)
                        {
                            tracing::warn!(error = %e, "ingest_today_entries failed");
                        }
                        tray::update_from_json(&app, &v, &conn);
                        floating::push_data(&app, &conn);
                    }
                    if let Some(s) = summary::from_today_json(&v) {
                        // Cache the live today Summary so get_summary("day") can
                        // serve the same value the tray shows, instead of the
                        // DB-backed daily_usage (which lags one history tick).
                        if let Some(state) = app.try_state::<crate::state::AppState>() {
                            if let Ok(mut cache) = state.last_today.lock() {
                                *cache = Some(s.clone());
                            }
                        }
                        let _ = app.emit("today:updated", s);
                    }
                    // Notify frontend to refetch breakdown/trends (daily_usage
                    // was just updated with fresh today data).
                    let _ = app.emit("collection:updated", ());

                    // Rate-limited project snapshot rebuild: keep projects page
                    // in sync with live data without running tokscale on every tick.
                    if last_project_snapshot.elapsed() >= PROJECT_SNAPSHOT_MIN_INTERVAL {
                        last_project_snapshot = Instant::now();
                        let bin_snap = bin_for_snapshot.clone();
                        let db_snap = db.clone();
                        tauri::async_runtime::spawn(async move {
                            super::project_snapshot::precompute_and_persist(bin_snap, db_snap)
                                .await;
                        });
                    }
                }
                scheduler::CollectionEvent::ScanError(msg) => {
                    // Surface scan failures to the frontend so the UI can show
                    // a degraded state instead of silently stale data.
                    let _ = app.emit("collection:error", msg);
                }
            }
        }
    });
}

/// Backfill project_path for Claude sessions by reading cwd from JSONL files.
///
/// Scans `~/.claude/projects/` for each known session, reads the first line's
/// `cwd` field, and updates `sessions.project_path` in the DB. Only updates rows
/// where project_path is currently NULL or empty (first-write-wins per session).
///
/// Optimized: collects all updates into a Vec first, then applies them in a
/// single transaction with a prepared statement (was N individual transactions).
#[allow(clippy::items_after_test_module, dead_code)]
fn backfill_claude_project_paths(
    conn: &mut Connection,
    installed_clients: &[String],
) -> Result<usize, String> {
    if !installed_clients.iter().any(|c| c == "claude") {
        return Ok(0);
    }
    let Some(claude_projects) = dirs::home_dir().map(|h| h.join(".claude").join("projects")) else {
        return Ok(0);
    };
    if !claude_projects.is_dir() {
        return Ok(0);
    }

    // Find all Claude sessions in DB that still lack a project_path.
    let session_ids: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT DISTINCT session_id FROM sessions WHERE tool = 'claude' AND (project_path IS NULL OR project_path = '')")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        let collected: Vec<String> = rows.filter_map(Result::ok).collect();
        collected
    };

    if session_ids.is_empty() {
        return Ok(0);
    }

    // Phase 1: resolve all project paths in memory (no DB writes yet).
    let mut updates: Vec<(String, String)> = Vec::with_capacity(session_ids.len());
    for sid in &session_ids {
        let Some(session_file) = super::workspace::find_session_file(&claude_projects, sid) else {
            continue;
        };
        let cwds = super::workspace::read_cwds(&session_file);
        if cwds.is_empty() {
            continue;
        }
        let Some(root) = super::workspace::project_root(&cwds) else {
            continue;
        };
        updates.push((sid.clone(), super::workspace::tilde_prefix(&root)));
    }

    if updates.is_empty() {
        return Ok(0);
    }

    // Phase 2: batch UPDATE in a single transaction with a prepared statement.
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let mut updated = 0;
    {
        let mut upd = tx
            .prepare(
                "UPDATE sessions SET project_path = ?1
             WHERE tool = 'claude' AND session_id = ?2
               AND (project_path IS NULL OR project_path = '')",
            )
            .map_err(|e| e.to_string())?;
        for (sid, path) in &updates {
            let rows = upd
                .execute(rusqlite::params![path, sid])
                .map_err(|e| e.to_string())?;
            updated += rows;
        }
        // `upd` dropped here (before tx.commit), releasing the borrow on `tx`.
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(updated)
}

/// Backfill `rounds` (user-input round count) for sessions that still have 0.
///
/// Queries distinct `(tool, session_id)` pairs where `rounds = 0`, reads each
/// session's JSONL file to count valid user rounds (same filter as the detail
/// view), and batch-updates the DB. Skips sessions whose JSONL file is
/// missing (they'll be retried on a future backfill run if the file appears).
///
/// Capped at 500 sessions per run to keep the watcher consumer responsive.
#[allow(clippy::items_after_test_module, dead_code)]
fn backfill_rounds(conn: &mut Connection) -> Result<usize, String> {
    // Find sessions that still need round counts.
    let session_ids: Vec<(String, String)> = {
        let mut stmt = conn
            .prepare("SELECT DISTINCT tool, session_id FROM sessions WHERE rounds = 0 LIMIT 500")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| e.to_string())?;
        let collected: Vec<(String, String)> = rows.filter_map(Result::ok).collect();
        collected
    };

    if session_ids.is_empty() {
        return Ok(0);
    }

    // Compute round counts from JSONL (no DB writes yet).
    let mut updates: Vec<(i64, String, String)> = Vec::with_capacity(session_ids.len());
    for (tool, sid) in &session_ids {
        let count =
            crate::query::sessions::count_valid_rounds(dirs::home_dir().as_deref(), tool, sid);
        if count > 0 {
            updates.push((count, tool.clone(), sid.clone()));
        }
    }

    if updates.is_empty() {
        return Ok(0);
    }

    // Batch UPDATE in a single transaction.
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let mut updated = 0;
    {
        let mut upd = tx
            .prepare("UPDATE sessions SET rounds = ?1 WHERE tool = ?2 AND session_id = ?3 AND rounds = 0")
            .map_err(|e| e.to_string())?;
        for (count, tool, sid) in &updates {
            let rows = upd
                .execute(rusqlite::params![count, tool, sid])
                .map_err(|e| e.to_string())?;
            updated += rows;
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(updated)
}
