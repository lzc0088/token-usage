//! Managed application state (M3). The DB connection sits behind an
//! `Arc<Mutex<Connection>>` so Tauri commands (via `State<AppState>`) and the
//! background collector consumer (Graph → upsert) can share it.

use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use crate::config::Config;
use crate::storage;

pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
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

    /// Load config from the DB (helper for callers with a shared DB handle).
    pub fn load_config(&self) -> Result<Config, storage::StorageError> {
        let conn = self.db.lock().expect("db poisoned");
        crate::config::load(&conn)
    }
}
