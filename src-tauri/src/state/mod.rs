//! Managed application state (M3). The DB connection sits behind an
//! `Arc<Mutex<Connection>>` so Tauri commands (via `State<AppState>`) and the
//! background collector consumer (Graph → upsert) can share it.
//!
//! NOTE: `rusqlite::Connection` is NOT `Send` (contains `RefCell` internally),
//! so `RwLock` (which requires `T: Send`) cannot be used. `Mutex` serializes
//! all Rust-level access, but SQLite WAL mode still allows concurrent reads
//! at the storage driver level — the Mutex only serializes guard acquisition,
//! which is a fast no-I-O operation.
//!
//! To help callers express intent, `db_read()` and `db_write()` methods are
//! provided alongside the legacy `db_guard()` (deprecated). All three return
//! the same `MutexGuard` type — the distinction is documentation + future
//! optimization potential if the lock type ever changes.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use tokio::sync::mpsc;

use crate::config::Config;
use crate::quota::burn_rate::BurnRateTracker;
use crate::storage;

pub struct AppState {
    pub(crate) db: Arc<Mutex<Connection>>,
    /// Cached config — shared between the scheduler (reader) and `set_config`
    /// (writer). The scheduler reads this without a DB lock on every loop
    /// iteration, eliminating the per-tick Mutex lock in the hot path.
    pub(crate) config_cache: Arc<Mutex<Option<Config>>>,
    /// Cross-window bridge: the settings page the settings window should
    /// navigate to on its next focus. `None` = no pending open (window just
    /// regained focus from another app — don't reset the user's current page).
    /// `Some(page)` = an open_settings call is pending; consume → navigate.
    /// Set by `open_settings` (main popover) / tray, consumed by the settings
    /// window's focus handler — the two windows are separate webviews with
    /// independent JS contexts, so JS module state can't cross between them.
    pub(crate) settings_target: Mutex<Option<String>>,
    /// Window labels whose native `MovableByWindowBackground` has been
    /// temporarily disabled (e.g. while a row-drag is in progress). Tracked
    /// so `set_drag_suspended(false)` restores the baseline per-window
    /// instead of unconditionally enabling drag.
    pub(crate) drag_suspended: Mutex<HashSet<String>>,
    /// The most recent live today Summary (from a `tokscale --today` scan).
    /// Cached so that `get_summary("day")` returns the same data as the
    /// tray menu title, avoiding a ~15-minute staleness gap between the
    /// tray (live events) and the popover (DB-backed daily_usage query).
    pub(crate) last_today: Mutex<Option<crate::query::summary::Summary>>,
    /// Senders to trigger an immediate collector scan: `collector_tick` forces
    /// a `tokscale --today` scan, `collector_history` forces graph+sessions.
    /// Populated by `collector::runtime::start` once the channels exist; `None`
    /// until then. The `collect_now` command uses these so the refresh button
    /// can force a scan instead of waiting on the watcher / history timer.
    pub(crate) collector_tick: Mutex<Option<mpsc::Sender<()>>>,
    pub(crate) collector_history: Mutex<Option<mpsc::Sender<()>>>,
    /// Shared burn-rate tracker for adaptive quota refresh. The scheduler loop
    /// records rates on each cycle; manual refreshes read it so projections
    /// and notifications persist across restarts (within the tracker's TTL).
    pub(crate) burn_rate_tracker: Mutex<BurnRateTracker>,
    /// Shared quota-notification dedup. Process-wide so manual refreshes don't
    /// re-notify for a window the scheduler already alerted.
    pub(crate) notify_dedup: Mutex<crate::quota::notify::NotifyDedup>,
}

impl AppState {
    /// Open (and migrate) the DB at the platform data path, then wrap in state.
    pub fn open_default() -> Result<Self, storage::StorageError> {
        let path = storage::db_path().ok_or(storage::StorageError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no platform data dir",
        )))?;
        let conn = storage::open_db(&path)?;
        let initial_config = crate::config::load(&conn).ok();
        Ok(Self {
            db: Arc::new(Mutex::new(conn)),
            config_cache: Arc::new(Mutex::new(initial_config)),
            settings_target: Mutex::new(None),
            drag_suspended: Mutex::new(HashSet::new()),
            last_today: Mutex::new(None),
            collector_tick: Mutex::new(None),
            collector_history: Mutex::new(None),
            burn_rate_tracker: Mutex::new(BurnRateTracker::new()),
            notify_dedup: Mutex::new(crate::quota::notify::NotifyDedup::default()),
        })
    }

    /// Lock the DB for reading (expressed intent — guard type is `MutexGuard`,
    /// same as write, but callers should only read through this guard).
    ///
    /// Recovers from mutex poisoning (data is still valid after a thread panic
    /// while holding the lock).
    pub fn db_read(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(|e| {
            tracing::warn!("db mutex poisoned (read), recovering: {e}");
            e.into_inner()
        })
    }

    /// Lock the DB for writing (expressed intent — guard type is `MutexGuard`,
    /// same as read, but callers may mutate through this guard). Only one
    /// writer at a time.
    ///
    /// Recovers from mutex poisoning.
    pub fn db_write(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(|e| {
            tracing::warn!("db mutex poisoned (write), recovering: {e}");
            e.into_inner()
        })
    }

    /// Lock the DB. Recovers from mutex poisoning (data is still valid after a
    /// thread panic while holding the lock). Commands are sync; the lock is held
    /// only across the (fast) synchronous query, never across an `.await`.
    #[deprecated(note = "use db_read() or db_write() for explicit read/write intent")]
    pub fn db_guard(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(|e| {
            tracing::warn!("db mutex poisoned, recovering: {e}");
            e.into_inner()
        })
    }

    /// Load config from the DB (helper for callers with a shared DB handle).
    pub fn load_config(&self) -> Result<Config, storage::StorageError> {
        let conn = self.db_read();
        crate::config::load(&conn)
    }

    /// Update the cached config. Called by `set_config` after every save so
    /// the scheduler picks up changes without a DB read.
    pub fn update_config_cache(&self, cfg: Config) {
        *self.config_cache.lock().unwrap_or_else(|e| e.into_inner()) = Some(cfg);
    }

    /// Read the cached config (no DB lock). Returns None if not yet loaded.
    pub fn cached_config(&self) -> Option<Config> {
        self.config_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Construct an AppState from an already-built shared DB handle. Used by
    /// tests that build an in-memory DB directly instead of via `open_default`.
    #[cfg(test)]
    pub(crate) fn with_db(db: Arc<Mutex<Connection>>) -> Self {
        let initial_config =
            crate::config::load(&db.lock().unwrap_or_else(|e| e.into_inner())).ok();
        Self {
            db,
            config_cache: Arc::new(Mutex::new(initial_config)),
            settings_target: Mutex::new(None),
            drag_suspended: Mutex::new(HashSet::new()),
            last_today: Mutex::new(None),
            collector_tick: Mutex::new(None),
            collector_history: Mutex::new(None),
            burn_rate_tracker: Mutex::new(BurnRateTracker::new()),
            notify_dedup: Mutex::new(crate::quota::notify::NotifyDedup::default()),
        }
    }
}
