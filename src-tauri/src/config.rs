//! User configuration via the `app_config` kv table (T2.4).
//!
//! Each setting is stored as one row keyed by a stable string, value JSON-encoded.
//! Typed accessors on [`Config`] (de)serialize; the raw kv helpers stay available
//! for ad-hoc keys. See design.md §F10 (currency), 主画面/采集 settings.

use rusqlite::Connection;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::storage::StorageError;

/// Display currency for cost fields (design.md §F10). Default 双显.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Currency {
    Usd,
    Cny,
    #[default]
    Both,
}

/// User-tunable settings. Each field maps to one `app_config` row so partial
/// updates don't require rewriting a monolithic blob.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Config {
    #[serde(default)]
    pub currency: Currency,
    /// Absolute path to a user-supplied tokscale binary (None = auto resolve).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokscale_path: Option<String>,
}

// Stable config keys.
const KEY_CONFIG: &str = "config";

/// Load the whole [`Config`] blob. Missing → defaults.
pub fn load(conn: &Connection) -> Result<Config, StorageError> {
    match get_raw(conn, KEY_CONFIG)? {
        Some(json) => Ok(serde_json::from_str(&json).unwrap_or_default()),
        None => Ok(Config::default()),
    }
}

/// Persist the whole [`Config`] blob.
pub fn save(conn: &Connection, cfg: &Config) -> Result<(), StorageError> {
    let json = serde_json::to_string(cfg)?;
    set_raw(conn, KEY_CONFIG, &json)
}

// ── raw kv helpers (also used by credentials-adjacent / scheduler state) ────

/// Read one kv value.
pub fn get_raw(conn: &Connection, key: &str) -> Result<Option<String>, StorageError> {
    let v: Option<String> = conn
        .query_row(
            "SELECT value FROM app_config WHERE key = ?",
            rusqlite::params![key],
            |r| r.get::<_, String>(0),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;
    Ok(v)
}

/// Read one kv value, JSON-deserialized into `T`. Missing → None.
pub fn get_json<T: DeserializeOwned>(
    conn: &Connection,
    key: &str,
) -> Result<Option<T>, StorageError> {
    match get_raw(conn, key)? {
        Some(s) => Ok(Some(serde_json::from_str(&s)?)),
        None => Ok(None),
    }
}

/// Upsert one kv value (JSON-encoded).
pub fn set_json<T: Serialize>(conn: &Connection, key: &str, value: &T) -> Result<(), StorageError> {
    set_raw(conn, key, &serde_json::to_string(value)?)
}

/// Upsert one raw kv string.
pub fn set_raw(conn: &Connection, key: &str, value: &str) -> Result<(), StorageError> {
    conn.execute(
        "INSERT INTO app_config (key, value, updated_at) VALUES (?, ?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        rusqlite::params![key, value, now_ms()],
    )?;
    Ok(())
}

fn now_ms() -> i64 {
    // SystemTime → epoch ms. std-only; the collector elsewhere uses tokio clocks,
    // but config persistence just needs a monotone-ish timestamp.
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::schema;

    fn conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        schema::migrate(&conn).unwrap();
        conn
    }

    #[test]
    fn load_returns_defaults_when_empty() {
        let c = conn();
        let cfg = load(&c).unwrap();
        assert_eq!(cfg.currency, Currency::Both);
        assert_eq!(cfg.tokscale_path, None);
    }

    #[test]
    fn save_then_load_roundtrip() {
        let c = conn();
        let cfg = Config {
            currency: Currency::Cny,
            tokscale_path: Some("/usr/local/bin/tokscale".into()),
        };
        save(&c, &cfg).unwrap();
        let loaded = load(&c).unwrap();
        assert_eq!(loaded, cfg);
    }

    #[test]
    fn currency_serializes_lowercase() {
        // serde rename_all(lowercase) — and unknown values fall back to default on load.
        let s = serde_json::to_string(&Currency::Usd).unwrap();
        assert_eq!(s, "\"usd\"");
        let c: Currency = serde_json::from_str("\"cny\"").unwrap();
        assert_eq!(c, Currency::Cny);
    }

    #[test]
    fn corrupted_config_falls_back_to_default() {
        let c = conn();
        set_raw(&c, KEY_CONFIG, "{not json").unwrap();
        assert_eq!(load(&c).unwrap(), Config::default());
    }

    #[test]
    fn raw_kv_roundtrip_and_overwrite() {
        let c = conn();
        assert_eq!(get_raw(&c, "k").unwrap(), None);
        set_raw(&c, "k", "v1").unwrap();
        assert_eq!(get_raw(&c, "k").unwrap(), Some("v1".into()));
        set_raw(&c, "k", "v2").unwrap();
        assert_eq!(get_raw(&c, "k").unwrap(), Some("v2".into()));
    }

    #[test]
    fn json_kv_roundtrip() {
        let c = conn();
        let list = vec!["claude".to_string(), "codex".to_string()];
        set_json(&c, "enabled", &list).unwrap();
        let got: Vec<String> = get_json(&c, "enabled").unwrap().unwrap();
        assert_eq!(got, list);
        assert!(get_json::<Vec<String>>(&c, "missing").unwrap().is_none());
    }
}
