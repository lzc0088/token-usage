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

/// Vertical layout metrics (logical px) — mirror the CSS in PulseApp.svelte:
/// each ring item is `diameter + LABEL_H` tall (ring + pct label), items are
/// separated by RING_GAP, and the panel adds vertical padding + title block.
const LABEL_H: f64 = 21.0;
const RING_GAP: f64 = 14.0;
const TITLE_BLOCK: f64 = 24.0;

/// Fixed vertical padding (user-specified: padding-top 30px).
const PAD_V: f64 = 30.0;

/// Panel height cap (logical px) — beyond this the ring dock scrolls.
const MAX_PANEL_H: f64 = 520.0;

/// Horizontal window padding (logical px) — flush panels use 18px on the
/// screen-interior side + 10px on the fused edge; the window keeps slack.
const PANEL_PAD: f64 = 16.0;

/// Gap between the ring rail and the detail card (logical px) — matches the
/// CSS tooltip offset (30px clear of the panel's inner edge + 18px pad).
const CARD_SPACING: f64 = 48.0;

/// Detail card width (logical px) — CSS max-width 260 + pointer overhang.
const CARD_WIDTH: f64 = 268.0;

/// Vertical slack (logical px) added symmetrically ABOVE and BELOW the
/// collapsed height while expanded, so a card centered on ANY ring fits
/// with the window top staying put. The top-left corner NEVER moves during
/// hover — no lift, no flicker, no drift.
const CARD_SLACK: f64 = 140.0;

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
            apply_floating_level(&h, cfg.pulse_topmost);
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

/// Build the pulse frontend payload (quotas + panel settings).
pub fn build_pulse_data(app: &AppHandle, conn: &Connection) -> PulseData {
    let cfg = config::load(conn).unwrap_or_default();
    PulseData {
        quotas: load_quotas(conn),
        size: cfg.pulse_size.clone(),
        ring_diameter: ring_diameter(&cfg.pulse_size),
        theme: resolved_theme(app, &cfg),
        opacity: cfg.pulse_opacity.clamp(0.2, 1.0),
    }
}

/// Push quota data to the pulse frontend via event.
pub fn push_pulse_data(app: &AppHandle, conn: &Connection) {
    let cfg = config::load(conn).unwrap_or_default();
    if !cfg.pulse_enabled {
        return;
    }

    let payload = build_pulse_data(app, conn);

    if let Some(w) = app.get_webview_window("pulse") {
        let _ = w.emit("pulse:update", &payload);
    }
}

/// Load vendor quotas from the quota_cache table, filtered by the config's
/// `quota_active_vendors` — the SAME rule the main window's quota page
/// applies, so the pulse dock shows exactly the enabled accounts.
fn load_quotas(conn: &Connection) -> Vec<PulseQuota> {
    // null = not configured → show all; otherwise only listed vendors.
    let active: Option<std::collections::HashSet<String>> = config::load(conn)
        .ok()
        .and_then(|c| c.quota_active_vendors)
        .map(|v| v.into_iter().collect());

    let mut quotas: Vec<PulseQuota> = Vec::new();
    if let Ok(mut stmt) = conn.prepare("SELECT data FROM quota_cache") {
        if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
            for row in rows.flatten() {
                if let Ok(q) = serde_json::from_str::<Quota>(&row) {
                    if let Some(set) = &active {
                        if !set.contains(&q.vendor) {
                            continue;
                        }
                    }
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
                                used_value: w.used_value,
                                total_value: w.total_value,
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
                            today_consumption: b.today_consumption,
                            month_consumption: b.month_consumption,
                        }),
                        expires_at: q.expires_at.clone(),
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

/// Dock height for n rings (ring + pct label per item, item gaps between).
fn dock_height(d: f64, n: usize) -> f64 {
    let n = n.max(1) as f64;
    n * (d + LABEL_H) + (n - 1.0) * RING_GAP
}

/// Dock height for n rings, capped so the panel never exceeds MAX_PANEL_H —
/// the CSS dock scrolls when the natural height overflows.
fn dock_height_capped(d: f64, n: usize) -> f64 {
    let avail = (MAX_PANEL_H - PAD_V * 2.0 - TITLE_BLOCK).max(60.0);
    dock_height(d, n).min(avail)
}

/// Collapsed (ring-only) window size, logical px. Mirrors the CSS layout:
/// vertical padding + title block + capped dock height.
fn collapsed_size(cfg: &config::Config, ring_count: usize) -> (f64, f64) {
    let d = ring_diameter(&cfg.pulse_size);
    (
        d + PANEL_PAD * 2.0,
        PAD_V * 2.0 + TITLE_BLOCK + dock_height_capped(d, ring_count),
    )
}

/// Expanded (hover) window size, logical px. The card slot extends the
/// window RIGHTWARD; symmetric vertical slack lets a card centered on ANY
/// ring fit while the TOP-LEFT corner stays anchored — the panel never
/// moves, flickers, or drifts during hover.
fn expanded_size(cfg: &config::Config, ring_count: usize) -> (f64, f64) {
    let (w_c, h_c) = collapsed_size(cfg, ring_count);
    (w_c + CARD_SPACING + CARD_WIDTH, h_c + 2.0 * CARD_SLACK)
}

/// Resize the pulse window to show the detail card beside the rings.
///
/// The top edge NEVER moves (no lift): the window only grows right + down,
/// where macOS's top-left resize anchor keeps the panel pixel-fixed. The
/// card itself clamps/positions within the window (frontend), and its arrow
/// slides along the card edge to point at the hovered ring. A flush-right
/// panel is the one exception on X: the window must shift left so the card
/// opens into the screen interior — the panel stays right-anchored inside.
pub fn expand_pulse(app: &AppHandle, vendor: Option<String>) {
    let Some(win) = app.get_webview_window("pulse") else {
        return;
    };
    let cfg = app
        .try_state::<AppState>()
        .and_then(|s| s.load_config().ok())
        .unwrap_or_default();

    let ring_count = load_quota_count(app);
    let (_, h_c) = collapsed_size(&cfg, ring_count);
    let (w_e, h_e_full) = expanded_size(&cfg, ring_count);

    let mut card_left = false;
    if let (Ok(pos), Ok(size), Ok(Some(mon))) = (
        win.outer_position(),
        win.outer_size(),
        win.current_monitor(),
    ) {
        let scale = win.scale_factor().unwrap_or(1.0).max(1.0);
        let px = pos.x as f64 / scale;
        let py = pos.y as f64 / scale;
        let w_now = size.width as f64 / scale;
        let h_now = size.height as f64 / scale;
        let mon_right = mon.position().x as f64 / scale + mon.size().width as f64 / scale;
        let mon_bottom = mon.position().y as f64 / scale + mon.size().height as f64 / scale;
        // Clamp the downward slack to the screen; never touch y.
        let h_e = h_e_full.min((mon_bottom - 4.0 - py).max(h_c));
        let target_x = if mon_right - (px + w_e) < 8.0 {
            card_left = true;
            mon_right - w_e
        } else {
            px
        };
        // Skip no-op geometry: re-hovering another ring fires expand again,
        // and a redundant setFrame makes the WKWebView flash.
        if card_left && (target_x - px).abs() > 0.5 {
            let _ = win.set_position(LogicalPosition::new(target_x, py));
        }
        if (w_now - w_e).abs() > 0.5 || (h_now - h_e).abs() > 0.5 {
            let _ = win.set_size(LogicalSize::new(w_e, h_e.max(h_c)));
        }
    } else {
        let _ = win.set_size(LogicalSize::new(w_e, h_e_full));
    }

    let _ = win.emit(
        "pulse:expand",
        serde_json::json!({ "vendor": vendor, "cardLeft": card_left }),
    );
}

/// Collapse the pulse window back to ring-only size. Mirrors the expand:
/// y is untouched; only a flush-right panel moves x back so its right edge
/// stays fused to the screen edge.
pub fn collapse_pulse(app: &AppHandle) {
    let Some(win) = app.get_webview_window("pulse") else {
        return;
    };
    let cfg = app
        .try_state::<AppState>()
        .and_then(|s| s.load_config().ok())
        .unwrap_or_default();

    let ring_count = load_quota_count(app);
    let (w_c, h_c) = collapsed_size(&cfg, ring_count);

    let mut already_collapsed = false;
    if let (Ok(pos), Ok(size), Ok(Some(mon))) = (
        win.outer_position(),
        win.outer_size(),
        win.current_monitor(),
    ) {
        let scale = win.scale_factor().unwrap_or(1.0).max(1.0);
        let px = pos.x as f64 / scale;
        let py = pos.y as f64 / scale;
        let w_now = size.width as f64 / scale;
        let h_now = size.height as f64 / scale;
        let mon_right = mon.position().x as f64 / scale + mon.size().width as f64 / scale;
        let flush_right = mon_right - (px + w_now) <= 8.0;
        if flush_right && (mon_right - w_c - px).abs() > 0.5 {
            let _ = win.set_position(LogicalPosition::new(mon_right - w_c, py));
        }
        already_collapsed =
            (w_now - w_c).abs() <= 0.5 && (h_now - h_c).abs() <= 0.5;
    }

    // Skip the no-op resize — a redundant setFrame makes the WKWebView flash
    // (rapid leave/re-enter cycles can collapse while already collapsed).
    if !already_collapsed {
        let _ = win.set_size(LogicalSize::new(w_c.max(60.0), h_c.max(60.0)));
    }
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

/// Position the pulse panel. Restores the saved (dragged) position when one
/// exists; otherwise docks to the configured screen edge, vertically centered.
fn position_pulse(app: &AppHandle, conn: &Connection) {
    let Some(win) = app.get_webview_window("pulse") else {
        return;
    };
    let cfg = config::load(conn).unwrap_or_default();
    let ring_count = load_quotas(conn).len().max(1);
    let (w, h) = collapsed_size(&cfg, ring_count);

    let _ = win.set_size(LogicalSize::new(w.max(60.0), h.max(60.0)));

    // Try to restore saved position first.
    if let Some((sx, sy)) = load_pos(conn) {
        let _ = win.set_position(LogicalPosition::new(sx, sy));
        return;
    }

    // Fresh dock: configured edge ("left" | "right"), vertically centered.
    // (Dragged positions persist and are restored above.)
    let Ok(Some(mon)) = win.primary_monitor() else {
        return;
    };
    let scale = win.scale_factor().unwrap_or(1.0).max(1.0);
    let mw = mon.size().width as f64 / scale;
    let mh = mon.size().height as f64 / scale;
    let mx = mon.position().x as f64 / scale;
    let my = mon.position().y as f64 / scale;
    let px = if cfg.pulse_side == "left" {
        mx
    } else {
        mx + mw - w
    };
    let py = my + (mh - h) / 2.0;
    let _ = win.set_position(LogicalPosition::new(px, py));
}

/// Save the pulse panel position (logical px) to the KV store.
pub fn save_pos(conn: &Connection, x: i32, y: i32) {
    let _ = crate::config::set_raw(conn, POS_KEY, &format!("{x},{y}"));
}

/// Forget the saved position — the next sync docks fresh at the configured edge.
pub fn clear_pos(conn: &Connection) {
    let _ = crate::config::set_raw(conn, POS_KEY, "");
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
    // Skip while expanded — the window is lifted (taller than the collapsed
    // size), so persisting now would save the shifted y and drift the panel
    // upward across restarts.
    let cfg = config::load(&conn).unwrap_or_default();
    let (_, h_c) = collapsed_size(&cfg, load_quotas(&conn).len());
    let h_now = h
        .outer_size()
        .map(|s| s.height as f64 / scale)
        .unwrap_or(0.0);
    if (h_now - h_c).abs() > 2.0 {
        return;
    }
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

/// Set the window level: floating (above normal windows, below menu bar)
/// when topmost, otherwise a normal window that stacks under focused apps.
#[cfg(target_os = "macos")]
fn apply_floating_level(win: &tauri::WebviewWindow, topmost: bool) {
    use objc::{msg_send, sel, sel_impl};
    if let Ok(ns_win) = win.ns_window() {
        let w: *mut objc::runtime::Object = ns_win as *mut _;
        unsafe {
            // NSWindow.Level.floating = 3, .normal = 0
            let level: i64 = if topmost { 3 } else { 0 };
            let _: () = msg_send![w, setLevel: level];
            let _: () = msg_send![w, setCollectionBehavior:
                (1u64 << 0) |  // canJoinAllSpaces
                (1u64 << 4) |  // stationary
                (1u64 << 7)    // ignoresCycle
            ];
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn apply_floating_level(_win: &tauri::WebviewWindow, _topmost: bool) {}

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
    pub size: String,
    pub ring_diameter: f64,
    pub theme: String,
    /// Surface opacity (0.2–1.0), from config.
    pub opacity: f64,
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
    /// Subscription plan expiry (RFC3339) — shown beside the plan badge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
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
    /// Absolute used/total (e.g. credits) for "剩余 X" display.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used_value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_value: Option<f64>,
}

#[derive(serde::Serialize, Clone)]
pub struct PulseBalance {
    pub amount: f64,
    pub currency: String,
    /// Today / month spend (API vendors, e.g. DeepSeek).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub today_consumption: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub month_consumption: Option<f64>,
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
    fn collapsed_size_matches_css_layout() {
        let cfg = config::Config::default();
        let d = ring_diameter(&cfg.pulse_size);
        for n in [1usize, 3, 5] {
            let (w, h) = collapsed_size(&cfg, n);
            assert_eq!(w, d + PANEL_PAD * 2.0);
            let expect_h = PAD_V * 2.0 + TITLE_BLOCK + dock_height_capped(d, n);
            assert!(
                (h - expect_h).abs() < f64::EPSILON,
                "n={n}: {h} != {expect_h}"
            );
        }
        // Concrete anchor (medium rings, n=3): dock = 3*69 + 2*14 = 235,
        // height = 30*2 + 24 + 235 = 319.
        if d == RING_MEDIUM {
            let (_, h3) = collapsed_size(&cfg, 3);
            assert!((h3 - 319.0).abs() < 0.01, "medium n=3: {h3}");
        }
        // Zero rings degrade to the single-ring minimum, never negative.
        let (_, h0) = collapsed_size(&cfg, 0);
        let (_, h1) = collapsed_size(&cfg, 1);
        assert_eq!(h0, h1);
    }

    #[test]
    fn expanded_size_grows_right_and_down_only() {
        let cfg = config::Config::default();
        for n in [1usize, 3, 5] {
            let (w_c, h_c) = collapsed_size(&cfg, n);
            let (w_e, h_e) = expanded_size(&cfg, n);
            // Card slot extends the window RIGHTWARD by spacing + card width.
            assert_eq!(w_e, w_c + CARD_SPACING + CARD_WIDTH, "n={n} width");
            // Vertical slack is symmetric and INDEPENDENT of which ring is
            // hovered — the top edge NEVER moves (no lift → no flicker).
            assert_eq!(h_e, h_c + 2.0 * CARD_SLACK, "n={n} height");
        }
    }

    #[test]
    fn panel_height_caps_at_max() {
        let cfg = config::Config::default();
        let (_, h10) = collapsed_size(&cfg, 10);
        assert!(
            h10 <= MAX_PANEL_H + f64::EPSILON,
            "10 rings capped: {h10} > {MAX_PANEL_H}"
        );
        let (_, h3) = collapsed_size(&cfg, 3);
        assert!(h3 < MAX_PANEL_H, "3 rings stay under the cap");
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
