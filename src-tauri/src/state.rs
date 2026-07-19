//! Managed application state (M3). Holds the open DB connection behind a Mutex
//! (rusqlite `Connection` is `Send` but not `Sync`); Tauri commands borrow it via
//! `State<AppState>`. The collector scheduler (T3.3) attaches here too.

use std::sync::Mutex;

use rusqlite::Connection;

use crate::storage;

pub struct AppState {
    pub db: Mutex<Connection>,
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
            db: Mutex::new(conn),
        })
    }
}
