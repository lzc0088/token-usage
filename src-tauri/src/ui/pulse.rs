//! Pulse-style floating quota panel — a row or column of circular progress
//! rings, one per connected vendor account. Renders in a borderless, transparent,
//! always-on-top Tauri webview window with SVG rings.
//!
//! Inspired by the Pulse macOS app's NSPanel dock, but implemented as a Tauri
//! webview for consistency with the existing window stack. The window is
//! resizable between collapsed (ring-only) and expanded (ring + detail card)
//! states, driven by hover.
//!
//! macOS only. Other platforms: no-op (the floating widget covers Windows/Linux).

use rusqlite::Connection;
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager};

use crate::config;
use crate::quota::types::Quota;
use crate::state::AppState;

/// Ring diameter presets (logical px).
const RING_SMALL: f64 = 36.0;
const RING_MEDIUM: f64 = 48.0;
const RING_LARGE: f64 = 60.0;

/// Gap between rings (logical px).
const RING_GAP: f64 = 12.0;

/// Panel padding (logical px).
const PANEL_PAD: f64 = 16.0;

/// Detail card width (logical px).
const CARD_WIDTH: f64 = 220.0;

/// Detail card row height (logical px).
const CARD_ROW_H: f64 = 52.0;

/// KV key persisting the pulse panel's on-screen position.
const POS_KEY: &str = "pulse_pos";

/// Resolve ring diameter from config size string.
fn ring_diameter(size: &str) -> f64 {
    match size {
        "small" => RING_SMALL,
        "large" => RING_LARGE,
        _ => RING_MEDIUM,
    }
}

/// Sync pulse panel visibility + position with config (startup + on change).
pub fn sync_pulse(app: &AppHandle, conn: &Connection) {
    if std::env::consts::OS != "macos" {
        hide_pulse(app);
        return;
    }
    let cfg = config::load(conn).unwrap_or_default();
    if cfg.pulse_enabled {
        position_pulse(app, conn);
        if let Some(h) = app.get_webview_window("pulse") {
            apply_floating_level(&h);
            let _ = h.show();
        }
        push_pulse_data(app, conn);
    } else {
        hide_pulse(app);
    }
}

/// Hide the pulse panel.
pub fn hide_pulse(app: &AppHandle) {
    if let Some(h) = app.get_webview_window("pulse") {
        let _ = h.hide();
    }
}

/// Push quota data to the pulse frontend via event.
pub fn push_pulse_data(app: &AppHandle, conn: &Connection) {
    let cfg = config::load(conn).unwrap_or_default();
    if !cfg.pulse_enabled {
        return;
    }

    let quotas = load_quotas(conn);
    let payload = PulseData {
        quotas,
        layout: cfg.pulse_layout.clone(),
        size: cfg.pulse_size.clone(),
        ring_diameter: ring_diameter(&cfg.pulse_size),
        theme: resolved_theme(app, &cfg),
    };

    if let Some(w) = app.get_webview_window("pulse") {
        let _ = w.emit("pulse:update", &payload);
    }
}

/// Load all vendor quotas from the quota_cache table.
fn load_quotas(conn: &Connection) -> Vec<PulseQuota> {
    let mut quotas: Vec<PulseQuota> = Vec::new();
    if let Ok(mut stmt) = conn.prepare("SELECT data FROM quota_cache") {
        if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
            for row in rows.flatten() {
                if let Ok(q) = serde_json::from_str::<Quota>(&row) {
                    let critical_window = q
                        .windows
                        .iter()
                        .max_by(|a, b| {
                            a.used_pct
                                .partial_cmp(&b.used_pct)
                                .unwrap_or(std::cmp::Ordering::Equal)
                        })
                        .cloned();

                    // Sort windows by usage to find the second most critical.
                    let mut sorted_windows: Vec<_> = q.windows.iter().collect();
                    sorted_windows.sort_by(|a, b| {
                        b.used_pct
                            .partial_cmp(&a.used_pct)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                    let second = sorted_windows.get(1);

                    quotas.push(PulseQuota {
                        vendor: q.vendor.clone(),
                        plan: q.plan_label.clone(),
                        status: format!("{:?}", q.status).to_lowercase(),
                        windows: q
                            .windows
                            .iter()
                            .map(|w| PulseWindow {
                                label: w.label.clone(),
                                used_pct: w.used_pct,
                                resets_at: w.resets_at.clone(),
                            })
                            .collect(),
                        critical_pct: critical_window.as_ref().map(|w| w.used_pct).unwrap_or(0.0),
                        critical_label: critical_window
                            .as_ref()
                            .map(|w| w.label.clone())
                            .unwrap_or_default(),
                        balance: q.balance.as_ref().map(|b| PulseBalance {
                            amount: b.amount,
                            currency: b.currency.clone(),
                        }),
                        second_pct: second.map(|w| w.used_pct),
                        second_label: second.map(|w| w.label.clone()),
                        is_running: false,
                        is_refreshing: false,
                    });
                }
            }
        }
    }
    // Sort: most critical first.
    quotas.sort_by(|a, b| {
        b.critical_pct
            .partial_cmp(&a.critical_pct)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    quotas
}

/// Resize the pulse window to show/hide the detail card area.
pub fn expand_pulse(app: &AppHandle, vendor: Option<String>) {
    let Some(win) = app.get_webview_window("pulse") else {
        return;
    };
    let cfg = app
        .try_state::<AppState>()
        .and_then(|s| s.load_config().ok())
        .unwrap_or_default();

    let diameter = ring_diameter(&cfg.pulse_size);
    let (w, h) = if cfg.pulse_layout == "horizontal" {
        // Horizontal: rings in a row; expand height for card below.
        let ring_count = load_quota_count(app);
        let ring_w = ring_count as f64 * (diameter + RING_GAP) - RING_GAP + PANEL_PAD * 2.0;
        (
            ring_w.max(CARD_WIDTH + PANEL_PAD * 2.0),
            diameter + CARD_ROW_H * 3.0 + PANEL_PAD * 2.0 + RING_GAP,
        )
    } else {
        // Vertical: rings in a column; expand width for card beside.
        (diameter + RING_GAP + CARD_WIDTH + PANEL_PAD * 2.0, 300.0)
    };

    let _ = win.set_size(LogicalSize::new(w, h));
    let _ = win.emit("pulse:expand", &vendor);
}

/// Collapse the pulse window back to ring-only size.
pub fn collapse_pulse(app: &AppHandle) {
    let Some(win) = app.get_webview_window("pulse") else {
        return;
    };
    let cfg = app
        .try_state::<AppState>()
        .and_then(|s| s.load_config().ok())
        .unwrap_or_default();

    let diameter = ring_diameter(&cfg.pulse_size);
    let ring_count = load_quota_count(app);
    let (w, h) = if cfg.pulse_layout == "horizontal" {
        (
            ring_count as f64 * (diameter + RING_GAP) - RING_GAP + PANEL_PAD * 2.0,
            diameter + PANEL_PAD * 2.0,
        )
    } else {
        (
            diameter + PANEL_PAD * 2.0,
            ring_count as f64 * (diameter + RING_GAP) - RING_GAP + PANEL_PAD * 2.0,
        )
    };

    let _ = win.set_size(LogicalSize::new(w.max(60.0), h.max(60.0)));
    let _ = win.emit("pulse:collapse", ());
}

/// Count loaded quotas (for sizing).
fn load_quota_count(app: &AppHandle) -> usize {
    let Some(state) = app.try_state::<AppState>() else {
        return 1;
    };
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(_) => return 1,
    };
    load_quotas(&conn).len().max(1)
}

/// Position the pulse panel. Uses saved position if available, otherwise defaults
/// to the right edge of the primary monitor.
fn position_pulse(app: &AppHandle, conn: &Connection) {
    let Some(win) = app.get_webview_window("pulse") else {
        return;
    };
    let cfg = config::load(conn).unwrap_or_default();
    let diameter = ring_diameter(&cfg.pulse_size);
    let ring_count = load_quotas(conn).len().max(1);

    let (w, h) = if cfg.pulse_layout == "horizontal" {
        (
            ring_count as f64 * (diameter + RING_GAP) - RING_GAP + PANEL_PAD * 2.0,
            diameter + PANEL_PAD * 2.0,
        )
    } else {
        (
            diameter + PANEL_PAD * 2.0,
            ring_count as f64 * (diameter + RING_GAP) - RING_GAP + PANEL_PAD * 2.0,
        )
    };

    let _ = win.set_size(LogicalSize::new(w.max(60.0), h.max(60.0)));

    // Try to restore saved position first.
    if let Some((sx, sy)) = load_pos(conn) {
        let _ = win.set_position(LogicalPosition::new(sx, sy));
        return;
    }

    // Default: right edge, 30% from top.
    let Ok(Some(mon)) = win.primary_monitor() else {
        return;
    };
    let scale = win.scale_factor().unwrap_or(1.0).max(1.0);
    let mw = mon.size().width as f64 / scale;
    let mh = mon.size().height as f64 / scale;
    let mx = mon.position().x as f64 / scale;
    let my = mon.position().y as f64 / scale;
    let px = mx + mw - w;
    let py = my + mh * 0.30;
    let _ = win.set_position(LogicalPosition::new(px, py));
}

/// Save the pulse panel position (logical px) to the KV store.
pub fn save_pos(conn: &Connection, x: i32, y: i32) {
    let _ = crate::config::set_raw(conn, POS_KEY, &format!("{x},{y}"));
}
pub fn persist_pulse_pos(app: &AppHandle) {
    let Some(h) = app.get_webview_window("pulse") else {
        return;
    };
    if !h.is_visible().unwrap_or(false) {
        return;
    }
    let scale = h.scale_factor().unwrap_or(1.0).max(1.0);
    let Ok(pos) = h.outer_position() else {
        return;
    };
    let wx = (pos.x as f64 / scale).round() as i32;
    let wy = (pos.y as f64 / scale).round() as i32;
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(_) => return,
    };
    save_pos(&conn, wx, wy);
}

/// Read the persisted window position.
fn load_pos(conn: &Connection) -> Option<(f64, f64)> {
    let raw = crate::config::get_raw(conn, POS_KEY).ok().flatten()?;
    let mut it = raw.split(',');
    let x = it.next()?.parse::<f64>().ok()?;
    let y = it.next()?.parse::<f64>().ok()?;
    Some((x, y))
}

/// Set the window to floating level (above normal windows, below menu bar).
#[cfg(target_os = "macos")]
fn apply_floating_level(win: &tauri::WebviewWindow) {
    use objc::{msg_send, sel, sel_impl};
    if let Ok(ns_win) = win.ns_window() {
        let w: *mut objc::runtime::Object = ns_win as *mut _;
        unsafe {
            // NSWindow.Level.floating = 3
            let _: () = msg_send![w, setLevel: 3i64];
            let _: () = msg_send![w, setCollectionBehavior:
                (1u64 << 0) |  // canJoinAllSpaces
                (1u64 << 4) |  // stationary
                (1u64 << 7)    // ignoresCycle
            ];
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn apply_floating_level(_win: &tauri::WebviewWindow) {}

/// Resolve the effective theme from config.
fn resolved_theme(app: &AppHandle, cfg: &config::Config) -> String {
    match cfg.theme.as_str() {
        "dark" => "dark".into(),
        "light" => "light".into(),
        _ => match app.get_webview_window("pulse").and_then(|w| w.theme().ok()) {
            Some(tauri::Theme::Light) => "light".into(),
            _ => "dark".into(),
        },
    }
}

/// Payload emitted to the pulse frontend.
#[derive(serde::Serialize, Clone)]
pub struct PulseData {
    pub quotas: Vec<PulseQuota>,
    pub layout: String,
    pub size: String,
    pub ring_diameter: f64,
    pub theme: String,
}

#[derive(serde::Serialize, Clone)]
pub struct PulseQuota {
    pub vendor: String,
    pub plan: Option<String>,
    pub status: String,
    pub windows: Vec<PulseWindow>,
    pub critical_pct: f64,
    pub critical_label: String,
    pub balance: Option<PulseBalance>,
    /// Second-most-critical window, for the inner ring.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub second_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub second_label: Option<String>,
    /// Whether a CLI for this vendor is currently active.
    pub is_running: bool,
    /// Whether Pulse is currently fetching a fresh reading.
    pub is_refreshing: bool,
}

#[derive(serde::Serialize, Clone)]
pub struct PulseWindow {
    pub label: String,
    pub used_pct: f64,
    pub resets_at: Option<String>,
}

#[derive(serde::Serialize, Clone)]
pub struct PulseBalance {
    pub amount: f64,
    pub currency: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_diameter_presets() {
        assert_eq!(ring_diameter("small"), RING_SMALL);
        assert_eq!(ring_diameter("medium"), RING_MEDIUM);
        assert_eq!(ring_diameter("large"), RING_LARGE);
        assert_eq!(ring_diameter("unknown"), RING_MEDIUM);
    }

    #[test]
    fn load_pos_roundtrip() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::storage::schema::migrate(&conn).unwrap();
        assert!(load_pos(&conn).is_none());
        let _ = crate::config::set_raw(&conn, POS_KEY, "100,200");
        let (x, y) = load_pos(&conn).unwrap();
        assert!((x - 100.0).abs() < f64::EPSILON);
        assert!((y - 200.0).abs() < f64::EPSILON);
    }

    #[test]
    fn load_pos_ignores_malformed() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::storage::schema::migrate(&conn).unwrap();
        let _ = crate::config::set_raw(&conn, POS_KEY, "garbage");
        assert!(load_pos(&conn).is_none());
    }
}
