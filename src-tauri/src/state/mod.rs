//! Managed application state (M3). The DB connection sits behind an
//! `Arc<Mutex<Connection>>` so Tauri commands (via `State<AppState>`) and the
//! background collector consumer (Graph → upsert) can share it.

use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use crate::config::Config;
use crate::storage;

pub struct AppState {
    pub(crate) db: Arc<Mutex<Connection>>,
}

impl AppState {
    /// Open (and migrate) the DB at the platform data path, then wrap in state.
    pub fn open_default() -> Result<Self, storage::StorageError> {
        let path = storage::db_path().ok_or(storage::StorageError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no platform data dir",
        )))?;
        let conn = storage::open_db(&path)?;
        Ok(Self {
            db: Arc::new(Mutex::new(conn)),
        })
    }

    /// Lock the DB, recovering gracefully from mutex poisoning. A poisoned
    /// mutex means a thread panicked while holding the lock — the data inside
    /// is still valid, so we recover the inner guard and continue.
    pub fn db_guard(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(|e| {
            tracing::warn!("db mutex poisoned, recovering: {e}");
            e.into_inner()
        })
    }

    /// Load config from the DB (helper for callers with a shared DB handle).
    pub fn load_config(&self) -> Result<Config, storage::StorageError> {
        let conn = self.db_guard();
        crate::config::load(&conn)
    }
}
