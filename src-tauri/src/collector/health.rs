//! Collection health record (状态 segment data source).
//!
//! Complements the live `get_tools_status` probe (which spawns tokscale on
//! demand) with a lightweight persisted record of *when* the collector last
//! succeeded/failed, written from the runtime consumer. Answers the questions
//! tokscale's merged JSON cannot: "上次采集是什么时候" and "上次为什么失败".
//!
//! Known limitation (by design): tokscale output is a single merged JSON across
//! all tools, so a single tool's failure is invisible; `ScanError` is global.
//! Per-tool "last seen" is derived from which clients appear in TodaySummary.
//!
//! All record* helpers are pure: they return a NEW `CollectionHealth` and never
//! mutate the input, per the project's immutability rule.

use std::collections::HashMap;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::storage::StorageError;

/// KV key inside `collection_state` (schema v1+; table previously unused).
const HEALTH_KEY: &str = "health";

/// Persisted collection health: global scan timestamps/errors + per-tool
/// last-seen info.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CollectionHealth {
    /// Last successful `tokscale --today` scan (unix ms).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_today_ms: Option<i64>,
    /// Last successful history scan (graph + sessions, unix ms).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_history_ms: Option<i64>,
    /// Last scan failure (message + time). Persists until the next success.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<ScanError>,
    /// Per-tool last-seen info, keyed by tokscale client id.
    #[serde(default)]
    pub clients: HashMap<String, ClientHealth>,
}

/// Per-tool last-seen record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClientHealth {
    /// When this client last appeared in a TodaySummary scan (unix ms).
    pub last_seen_ms: i64,
    /// Message count observed at that time.
    pub message_count: i64,
}

/// A recorded scan failure.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScanError {
    pub message: String,
    pub at_ms: i64,
}

/// Record a successful today scan: stamp `last_today_ms` and merge the clients
/// present in the TodaySummary JSON (entries[].client + messageCount sums).
/// Clients absent from this scan keep their previous entries.
pub fn record_today(h: &CollectionHealth, today_json: &Value, now_ms: i64) -> CollectionHealth {
    let mut message_counts: HashMap<String, i64> = HashMap::new();
    if let Some(entries) = today_json.get("entries").and_then(|e| e.as_array()) {
        for entry in entries {
            let Some(client) = entry
                .get("client")
                .or_else(|| entry.get("clientName"))
                .and_then(|c| c.as_str())
                .filter(|c| !c.is_empty())
            else {
                continue;
            };
            let msgs = ["messageCount", "messages", "message_count"]
                .iter()
                .find_map(|k| entry.get(*k).and_then(|x| x.as_i64()))
                .unwrap_or(0);
            // A client can appear in several per-model entries — sum messages.
            *message_counts.entry(client.to_string()).or_insert(0) += msgs;
        }
    }
    let mut clients = h.clients.clone();
    for (client, msgs) in message_counts {
        clients.insert(
            client,
            ClientHealth {
                last_seen_ms: now_ms,
                message_count: msgs,
            },
        );
    }
    CollectionHealth {
        last_today_ms: Some(now_ms),
        last_history_ms: h.last_history_ms,
        // A successful scan supersedes any earlier error.
        last_error: None,
        clients,
    }
}

/// Record a successful history scan (graph + sessions ingest).
pub fn record_history(h: &CollectionHealth, now_ms: i64) -> CollectionHealth {
    CollectionHealth {
        last_today_ms: h.last_today_ms,
        last_history_ms: Some(now_ms),
        last_error: h.last_error.clone(),
        clients: h.clients.clone(),
    }
}

/// Record a scan failure. Does NOT clear the success timestamps (they describe
/// when data was last good, which stays informative during an outage).
pub fn record_error(h: &CollectionHealth, message: &str, now_ms: i64) -> CollectionHealth {
    CollectionHealth {
        last_today_ms: h.last_today_ms,
        last_history_ms: h.last_history_ms,
        last_error: Some(ScanError {
            message: message.to_string(),
            at_ms: now_ms,
        }),
        clients: h.clients.clone(),
    }
}

/// Load the persisted health record. Missing/corrupted → default (empty).
/// Best-effort: corruption logs and returns default rather than failing the
/// whole command.
pub fn load_health(conn: &Connection) -> CollectionHealth {
    read_state_json(conn, HEALTH_KEY)
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// Persist the health record. Best-effort: errors are surfaced to the caller
/// (the runtime consumer logs and continues).
pub fn save_health(conn: &Connection, h: &CollectionHealth) -> Result<(), StorageError> {
    write_state_json(conn, HEALTH_KEY, h)
}

// ── collection_state kv helpers ─────────────────────────────────────────────
// (config::get_json/set_json target app_config; collection_state is the
// collector-owned kv table, so health gets its own small parameterized pair.)

fn read_state_json<T: serde::de::DeserializeOwned>(
    conn: &Connection,
    key: &str,
) -> Result<Option<T>, StorageError> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM collection_state WHERE key = ?",
            rusqlite::params![key],
            |r| r.get::<_, String>(0),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;
    match raw {
        Some(s) => Ok(Some(serde_json::from_str(&s)?)),
        None => Ok(None),
    }
}

fn write_state_json<T: serde::Serialize>(
    conn: &Connection,
    key: &str,
    value: &T,
) -> Result<(), StorageError> {
    let json = serde_json::to_string(value)?;
    conn.execute(
        "INSERT INTO collection_state (key, value, updated_at) VALUES (?, ?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        rusqlite::params![key, json, now_ms()],
    )?;
    Ok(())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::storage::schema::migrate(&conn).unwrap();
        conn
    }

    #[test]
    fn record_today_extracts_clients_and_message_counts() {
        let json = serde_json::json!({
            "entries": [
                { "client": "claude", "model": "glm-5.2", "messageCount": 5 },
                { "client": "claude", "model": "gpt-5", "messageCount": 2 },
                { "client": "codex", "model": "gpt-5", "messageCount": 3 },
                { "client": "", "model": "x", "messageCount": 9 },
                { "model": "y", "messageCount": 1 }
            ]
        });
        let h = record_today(&CollectionHealth::default(), &json, 1_000);
        assert_eq!(h.last_today_ms, Some(1_000));
        assert_eq!(h.clients["claude"].message_count, 7);
        assert_eq!(h.clients["codex"].message_count, 3);
        assert!(!h.clients.contains_key("qoder"));
        assert_eq!(h.last_error, None);
    }

    #[test]
    fn record_today_keeps_unseen_clients() {
        let prior = CollectionHealth {
            clients: HashMap::from([(
                "zcode".into(),
                ClientHealth {
                    last_seen_ms: 500,
                    message_count: 4,
                },
            )]),
            ..CollectionHealth::default()
        };
        let json = serde_json::json!({"entries": [{"client": "claude", "messageCount": 1}]});
        let h = record_today(&prior, &json, 1_000);
        // zcode wasn't in this scan — its last-seen survives untouched.
        assert_eq!(h.clients["zcode"].last_seen_ms, 500);
        assert_eq!(h.clients["claude"].last_seen_ms, 1_000);
        // Input not mutated (immutability).
        assert_eq!(prior.clients.len(), 1);
    }

    #[test]
    fn record_today_clears_stale_error_and_handles_missing_entries() {
        let prior = CollectionHealth {
            last_error: Some(ScanError {
                message: "boom".into(),
                at_ms: 100,
            }),
            ..CollectionHealth::default()
        };
        let h = record_today(&prior, &serde_json::json!({}), 2_000);
        assert_eq!(h.last_error, None);
        assert!(h.clients.is_empty());
        assert_eq!(h.last_today_ms, Some(2_000));
    }

    #[test]
    fn record_history_only_touches_its_field() {
        let prior = CollectionHealth {
            last_today_ms: Some(100),
            last_error: Some(ScanError {
                message: "x".into(),
                at_ms: 150,
            }),
            ..CollectionHealth::default()
        };
        let h = record_history(&prior, 300);
        assert_eq!(h.last_history_ms, Some(300));
        assert_eq!(h.last_today_ms, Some(100));
        assert!(
            h.last_error.is_some(),
            "history success alone must not clear an error"
        );
    }

    #[test]
    fn record_error_overwrites_previous() {
        let prior = record_error(&CollectionHealth::default(), "first", 100);
        let h = record_error(&prior, "second", 200);
        let e = h.last_error.as_ref().unwrap();
        assert_eq!(e.message, "second");
        assert_eq!(e.at_ms, 200);
    }

    #[test]
    fn health_kv_roundtrip() {
        let conn = fresh_conn();
        let h = CollectionHealth {
            last_today_ms: Some(42),
            clients: HashMap::from([(
                "claude".into(),
                ClientHealth {
                    last_seen_ms: 42,
                    message_count: 7,
                },
            )]),
            ..CollectionHealth::default()
        };
        save_health(&conn, &h).unwrap();
        let back = load_health(&conn);
        assert_eq!(back, h);
    }

    #[test]
    fn load_health_missing_or_corrupt_returns_default() {
        let conn = fresh_conn();
        assert_eq!(load_health(&conn), CollectionHealth::default());
        conn.execute(
            "INSERT INTO collection_state (key, value, updated_at) VALUES ('health', 'not json', 0)",
            [],
        )
        .unwrap();
        assert_eq!(load_health(&conn), CollectionHealth::default());
    }
}
