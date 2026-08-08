//! User configuration via the `app_config` kv table (T2.4).
//!
//! Each setting is stored as one row keyed by a stable string, value JSON-encoded.
//! Typed accessors on [`Config`] (de)serialize; the raw kv helpers stay available
//! for ad-hoc keys. See design.md §F10 (currency), 主画面/采集 settings.

use rusqlite::Connection;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::storage::StorageError;
use crate::utils::time::now_ms;

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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    #[serde(default)]
    pub currency: Currency,
    /// Exchange-rate source: "auto" (fetch daily) | "manual" (user-supplied).
    #[serde(default = "default_rate_mode")]
    pub rate_mode: String,
    /// Absolute path to a user-supplied tokscale binary (None = auto resolve).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokscale_path: Option<String>,
    /// Start the app on system boot (macOS LaunchAgent / Win registry / Linux autostart).
    #[serde(default)]
    pub auto_start: bool,
    /// UI language — "zh" | "en".
    #[serde(default = "default_language")]
    pub language: String,
    /// Default period in the popover — "day" | "month" | "total".
    #[serde(default = "default_period")]
    pub default_period: String,
    /// Hero token-rate readout mode — "speed" (output tok/s) | "burn" (total tok/min).
    #[serde(default = "default_token_rate_mode")]
    pub token_rate_mode: String,
    /// Auto-hide popover when window loses focus.
    #[serde(default = "default_true")]
    pub auto_close_on_blur: bool,
    /// How the popover is triggered: "click" (tray click) | "hover" (mouse over tray).
    #[serde(default = "default_trigger_mode")]
    pub trigger_mode: String,
    /// Window display mode: "normal" (draggable) | "fixed" (pinned position)
    /// | "always_on_top" (floating above other apps). Main popover only.
    #[serde(default = "default_window_display_mode")]
    pub window_display_mode: String,
    /// Tray display style: today_tokens | today_cost | today_both |
    /// total_tokens | total_cost | total_both | icon_only.
    #[serde(default = "default_tray_display")]
    pub tray_display: String,
    /// Show the app icon in the Dock (menu-bar apps usually hide it).
    #[serde(default)]
    pub show_in_dock: bool,
    /// Global hotkey to show/hide the popover (empty = not set).
    #[serde(default)]
    pub hotkey: String,
    /// UI theme: "dark" | "light" | "system".
    #[serde(default = "default_theme")]
    pub theme: String,
    /// Animation preference: "system" | "on" | "off".
    #[serde(default = "default_animation")]
    pub animation: String,
    /// Data refresh interval: "manual" | "30s" | "60s" | "300s" | "600s".
    #[serde(default = "default_refresh_interval")]
    pub refresh_interval: String,
    /// Collection mode: "live" (file-watch realtime) | "smart" (10min interval, activity-gated)
    /// | "interval" (fixed interval only, no file watch).
    #[serde(default = "default_collection_mode")]
    pub collection_mode: String,
    /// Preserve sessions whose source tool is no longer installed. When false,
    /// the collector prunes them on each ingest.
    #[serde(default = "default_true")]
    pub session_archive_enabled: bool,
    /// Quota data refresh interval: "1m" | "3m" | "5m" | "10m" | "15m".
    #[serde(default = "default_quota_refresh_interval")]
    pub quota_refresh_interval: String,
    /// Quota progress display mode: "用量" (show usage %) or "剩余" (show remaining %).
    #[serde(default = "default_quota_progress_mode")]
    pub quota_progress_mode: String,
    /// Which vendors are enabled in the quota display (None = all enabled).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quota_active_vendors: Option<Vec<String>>,
    /// Custom display order for vendors in the quota list (all vendor ids in
    /// preferred order). New vendors not in this list appear at the end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quota_vendor_order: Option<Vec<String>>,
    /// Collection: tracked tool names (None = all tracked).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection_tracked: Option<Vec<String>>,
    /// Collection: visible tool names (None = all visible).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection_visible: Option<Vec<String>>,
    /// Collection: ordered tool names (None = report order).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection_ordered: Option<Vec<String>>,
    /// Layout: visible top-level segment keys in order (None = all default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_modules: Option<Vec<String>>,
    /// Layout: visible overview sub-item keys in order (None = all default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_overview_sub: Option<Vec<String>>,
    /// Overview: quota vendor IDs to show, in order (None = show all active).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overview_quota_vendors: Option<Vec<String>>,
    /// Show a floating data widget on the desktop (Windows/Linux only).
    #[serde(default = "default_false")]
    pub floating_enabled: bool,
    /// Floating widget display mode: "today_tokens" | "today_cost" | "total_tokens" | "total_cost".
    #[serde(default = "default_floating_display")]
    pub floating_display: String,
}

/// Hand-rolled `Default` so `Config::default()` agrees with the serde defaults
/// above. (`#[derive(Default)]` would give every `String` field `""`, which
/// diverges from e.g. `default_theme()` and breaks the "no DB row yet" path in
/// `load()` — the UI would see `theme = ""` and highlight nothing.)
impl Default for Config {
    fn default() -> Self {
        Self {
            currency: Currency::default(),
            rate_mode: default_rate_mode(),
            tokscale_path: None,
            auto_start: false,
            language: default_language(),
            default_period: default_period(),
            token_rate_mode: default_token_rate_mode(),
            auto_close_on_blur: default_true(),
            trigger_mode: default_trigger_mode(),
            window_display_mode: default_window_display_mode(),
            tray_display: default_tray_display(),
            show_in_dock: false,
            hotkey: String::new(),
            theme: default_theme(),
            animation: default_animation(),
            refresh_interval: default_refresh_interval(),
            collection_mode: default_collection_mode(),
            session_archive_enabled: default_true(),
            quota_refresh_interval: default_quota_refresh_interval(),
            quota_progress_mode: default_quota_progress_mode(),
            quota_active_vendors: None,
            quota_vendor_order: None,
            collection_tracked: None,
            collection_visible: None,
            collection_ordered: None,
            layout_modules: None,
            layout_overview_sub: None,
            overview_quota_vendors: None,
            floating_enabled: default_false(),
            floating_display: default_floating_display(),
        }
    }
}

fn default_language() -> String {
    "zh".into()
}
fn default_period() -> String {
    "day".into()
}
fn default_token_rate_mode() -> String {
    "speed".into()
}
fn default_true() -> bool {
    true
}
fn default_rate_mode() -> String {
    "auto".into()
}
fn default_trigger_mode() -> String {
    "click".into()
}
fn default_window_display_mode() -> String {
    "normal".into()
}
fn default_tray_display() -> String {
    "icon_only".into()
}
fn default_theme() -> String {
    "system".into()
}
fn default_animation() -> String {
    "system".into()
}
fn default_refresh_interval() -> String {
    "manual".into()
}
fn default_collection_mode() -> String {
    "live".into()
}
fn default_quota_refresh_interval() -> String {
    "5m".into()
}
fn default_quota_progress_mode() -> String {
    "剩余".into()
}
fn default_false() -> bool {
    false
}
fn default_floating_display() -> String {
    "today_tokens".into()
}

// Stable config keys.
const KEY_CONFIG: &str = "config";

/// Load the whole [`Config`] blob. Missing → defaults. Corrupted JSON → logs
/// the raw value and falls back to defaults so the user does not lose all
/// settings silently.
pub fn load(conn: &Connection) -> Result<Config, StorageError> {
    match get_raw(conn, KEY_CONFIG)? {
        Some(json) => match serde_json::from_str(&json) {
            Ok(cfg) => Ok(cfg),
            Err(e) => {
                tracing::warn!(error = %e, raw_len = json.len(), "config JSON corrupted, falling back to defaults");
                Ok(Config::default())
            }
        },
        None => Ok(Config::default()),
    }
}

/// Persist the whole [`Config`] blob.
pub fn save(conn: &Connection, cfg: &Config) -> Result<(), StorageError> {
    let json = serde_json::to_string(cfg)?;
    set_raw(conn, KEY_CONFIG, &json)
}

/// Atomic load-modify-save. Reads the current config, passes it to `f` for
/// mutation, and persists the result — all within a single function call so
/// concurrent callers can't overwrite each other's changes.
///
/// The DB connection is held for the duration of the closure; keep `f` fast
/// (no I/O, no network).
pub fn with_config(conn: &Connection, f: impl FnOnce(&mut Config)) -> Result<(), StorageError> {
    let mut cfg = load(conn)?;
    f(&mut cfg);
    save(conn, &cfg)
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

/// Read an integer value from app_config. Returns Ok(None) if the key is missing.
pub fn get_int(conn: &Connection, key: &str) -> Result<Option<u64>, StorageError> {
    match get_raw(conn, key)? {
        Some(s) => {
            let trimmed = s.trim().to_string();
            // Try parsing as a plain integer first, then as JSON number
            if let Ok(n) = trimmed.parse::<u64>() {
                Ok(Some(n))
            } else if let Some(n) = serde_json::from_str::<serde_json::Value>(&s)
                .ok()
                .and_then(|v| v.as_u64())
            {
                Ok(Some(n))
            } else {
                Ok(None)
            }
        }
        None => Ok(None),
    }
}

/// Atomically increment an integer value in app_config. Creates the key if missing.
/// Uses a single SQL statement to avoid the read-modify-write race in `get_int`
/// + `set_raw`.
pub fn incr_int(conn: &Connection, key: &str, delta: u64) -> Result<u64, StorageError> {
    let now_ms = crate::utils::time::now_ms();
    let delta_str = delta.to_string();
    conn.execute(
        "INSERT INTO app_config (key, value, updated_at) VALUES (?, ?, ?)
         ON CONFLICT(key) DO UPDATE SET value = CAST(value AS INTEGER) + ?",
        rusqlite::params![key, delta_str, now_ms, delta_str],
    )?;
    let new_val = get_int(conn, key)?.unwrap_or(0);
    Ok(new_val)
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
            ..Default::default()
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
