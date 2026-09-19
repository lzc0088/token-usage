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
const LABEL_H: f64 = 13.0;
const RING_GAP: f64 = 8.0;
const TITLE_BLOCK: f64 = 24.0;
const PAD_V_MIN: f64 = 14.0;

/// Swoop amplitude (flush silhouette): fraction of the content height,
/// clamped. Per the reference mock, the inner-side corners are carved LOWEST
/// (~1/3 of panel height) and the boundary rises in an S-shaped SLOPE
/// (corner fillet → steep sweep → flat arrival at the fused screen edge).
const SWOOP_FRAC: f64 = 0.45;
const SWOOP_MIN: f64 = 48.0;
const SWOOP_MAX: f64 = 170.0;

/// Panel height cap (logical px) — beyond this the ring dock scrolls.
const MAX_PANEL_H: f64 = 520.0;

/// Horizontal window padding (logical px) — flush panels use 18px on the
/// screen-interior side + 10px on the fused edge; the window keeps slack.
const PANEL_PAD: f64 = 16.0;

/// Gap between the ring rail and the detail card (logical px) — matches the
/// CSS tooltip offset (22px clear of the panel edge).
const CARD_SPACING: f64 = 22.0;

/// Detail card width (logical px) — CSS max-width 236 + pointer overhang.
const CARD_WIDTH: f64 = 240.0;

/// Max half-height of the detail card (header + 3 rows + balance). The card
/// is CENTERED on the hovered ring, so the window is lifted / extended by
/// whatever this half lacks above/below the ring.
const CARD_HALF_H: f64 = 132.0;

/// KV key persisting the pulse panel's on-screen position.
const POS_KEY: &str = "pulse_pos";

/// Lift applied by the last expand (logical px) — collapse reverses exactly
/// this amount (the height delta alone also includes the bottom extension).
static LAST_LIFT: std::sync::Mutex<f64> = std::sync::Mutex::new(0.0);

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

/// Inverted-S swoop amplitude for the flush silhouette (logical px).
fn swoop_c(d: f64, n: usize) -> f64 {
    ((TITLE_BLOCK + dock_height(d, n)) * SWOOP_FRAC).clamp(SWOOP_MIN, SWOOP_MAX)
}

/// Vertical padding — clears the swoop curve at the content's x-extent.
fn pad_v(d: f64, n: usize) -> f64 {
    (swoop_c(d, n) * 0.3).ceil().max(PAD_V_MIN)
}

/// Where the ring rail starts inside the window (pad + title block).
fn rail_top(d: f64, n: usize) -> f64 {
    pad_v(d, n) + TITLE_BLOCK
}

/// Dock height for n rings, capped so the panel never exceeds MAX_PANEL_H —
/// the CSS dock scrolls when the natural height overflows.
fn dock_height_capped(d: f64, n: usize) -> f64 {
    let avail = (MAX_PANEL_H - pad_v(d, n) * 2.0 - TITLE_BLOCK).max(60.0);
    dock_height(d, n).min(avail)
}

/// Collapsed (ring-only) window size, logical px. Mirrors the CSS layout:
/// vertical padding (swoop clearance) + title block + capped dock height.
fn collapsed_size(cfg: &config::Config, ring_count: usize) -> (f64, f64) {
    let d = ring_diameter(&cfg.pulse_size);
    let n = ring_count.max(1);
    (
        d + PANEL_PAD * 2.0,
        pad_v(d, n) * 2.0 + TITLE_BLOCK + dock_height_capped(d, n),
    )
}

/// Resize the pulse window to show the detail card beside the rings.
/// Vertical layout only: the width grows for the card. macOS anchors resizes
/// at the top-left corner, so the window must grow LEFTWARD when the panel is
/// fused to the right screen edge — or when rightward growth would run
/// off-screen. The card is CENTERED on the hovered ring: the window top is
/// LIFTED by whatever the card's upper half lacks above the ring, and the
/// height extended by whatever the lower half lacks below it.
pub fn expand_pulse(app: &AppHandle, vendor: Option<String>) {
    let Some(win) = app.get_webview_window("pulse") else {
        return;
    };
    let cfg = app
        .try_state::<AppState>()
        .and_then(|s| s.load_config().ok())
        .unwrap_or_default();

    let d = ring_diameter(&cfg.pulse_size);
    let ring_count = load_quota_count(app);
    let (_, h_c) = collapsed_size(&cfg, ring_count);
    let pitch = d + LABEL_H + RING_GAP;
    let w_e = d + PANEL_PAD * 2.0 + CARD_SPACING + CARD_WIDTH;

    // Hovered ring's center offset inside the rail (worst case: first ring).
    let idx = vendor
        .as_deref()
        .map(|v| load_quota_index(app, v))
        .unwrap_or(0);
    let ring_center = idx as f64 * pitch + d / 2.0;
    let lift_want = (CARD_HALF_H + 8.0 - (rail_top(d, ring_count) + ring_center)).max(0.0);
    let bottom_want = (CARD_HALF_H + 8.0 - (h_c - rail_top(d, ring_count) - ring_center)).max(0.0);

    let mut grow_left = false;
    let mut lift = lift_want;
    let mut bottom_ext = bottom_want;
    if let (Ok(pos), Ok(size), Ok(Some(mon))) = (
        win.outer_position(),
        win.outer_size(),
        win.current_monitor(),
    ) {
        let scale = win.scale_factor().unwrap_or(1.0).max(1.0);
        let px = pos.x as f64 / scale;
        let py = pos.y as f64 / scale;
        let h_cur = size.height as f64 / scale;
        // Decide from the COLLAPSED y — undo any lift a previous expand left.
        let base_py = py + (h_cur - h_c).max(0.0);
        let mon_top = mon.position().y as f64 / scale;
        let mon_right = mon.position().x as f64 / scale + mon.size().width as f64 / scale;
        let mon_bottom = mon.position().y as f64 / scale + mon.size().height as f64 / scale;
        lift = lift_want.min((base_py - mon_top - 8.0).max(0.0));
        bottom_ext = bottom_want.min((mon_bottom - 8.0 - base_py - h_c).max(0.0));
        let target_x = if mon_right - (px + w_e) < 8.0 {
            grow_left = true;
            mon_right - w_e
        } else {
            px
        };
        let target_y = base_py - lift;
        if grow_left || lift > 0.5 || (target_y - py).abs() > 0.5 {
            let _ = win.set_position(LogicalPosition::new(target_x, target_y));
        }
    }

    let _ = win.set_size(LogicalSize::new(w_e, h_c + lift + bottom_ext));
    if let Ok(mut l) = LAST_LIFT.lock() {
        *l = lift;
    }
    let _ = win.emit(
        "pulse:expand",
        serde_json::json!({ "vendor": vendor, "cardLeft": grow_left, "lift": lift }),
    );
}

/// Collapse the pulse window back to ring-only size. Reverses the expand
/// geometry: drop the lift (move the top back down by the current excess
/// height) and, when flush right, pin the RIGHT edge so the panel stays
/// fused to the screen edge.
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

    if let (Ok(pos), Ok(size), Ok(Some(mon))) = (
        win.outer_position(),
        win.outer_size(),
        win.current_monitor(),
    ) {
        let scale = win.scale_factor().unwrap_or(1.0).max(1.0);
        let px = pos.x as f64 / scale;
        let py = pos.y as f64 / scale;
        let mon_right = mon.position().x as f64 / scale + mon.size().width as f64 / scale;
        let flush_right = mon_right - (px + size.width as f64 / scale) <= 8.0;
        let lift = *LAST_LIFT.lock().unwrap_or_else(|e| e.into_inner());
        let target_x = if flush_right { mon_right - w_c } else { px };
        let target_y = py + lift;
        if flush_right || lift > 0.5 {
            let _ = win.set_position(LogicalPosition::new(target_x, target_y));
        }
    }

    let _ = win.set_size(LogicalSize::new(w_c.max(60.0), h_c.max(60.0)));
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

/// Index of a vendor in the quota list (same order the frontend renders).
fn load_quota_index(app: &AppHandle, vendor: &str) -> usize {
    let Some(state) = app.try_state::<AppState>() else {
        return 0;
    };
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(_) => return 0,
    };
    load_quotas(&conn)
        .iter()
        .position(|q| q.vendor == vendor)
        .unwrap_or(0)
}

/// Position the pulse panel. Restores the saved (dragged) position when one
/// exists; otherwise docks to the configured screen edge, 30% from top.
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

    // Fresh dock: configured edge ("left" | "right"), 30% from top.
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
    let py = my + mh * 0.30;
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
            let expect_h = pad_v(d, n) * 2.0 + TITLE_BLOCK + dock_height(d, n);
            assert!(
                (h - expect_h).abs() < f64::EPSILON,
                "n={n}: {h} != {expect_h}"
            );
        }
        // Concrete anchor (medium rings, n=3): dock = 3*61 + 2*8 = 199,
        // swoop = clamp(223 * 0.45) ≈ 100.4 → pad = ceil(30.1) = 31,
        // height = 31*2 + 24 + 199 = 285.
        if d == RING_MEDIUM {
            let (_, h3) = collapsed_size(&cfg, 3);
            assert!((h3 - 285.0).abs() < 0.01, "medium n=3: {h3}");
        }
        // Zero rings degrade to the single-ring minimum, never negative.
        let (_, h0) = collapsed_size(&cfg, 0);
        let (_, h1) = collapsed_size(&cfg, 1);
        assert_eq!(h0, h1);
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
    fn swoop_grows_with_dock_and_stays_clamped() {
        let c1 = swoop_c(RING_MEDIUM, 1);
        let c3 = swoop_c(RING_MEDIUM, 3);
        let c9 = swoop_c(RING_MEDIUM, 9);
        assert!(c1 >= SWOOP_MIN, "single ring keeps the minimum swoop");
        assert!(c3 > c1, "more rings → deeper swoop");
        assert!(c9 <= SWOOP_MAX, "swoop clamped at the maximum");
        // Padding always clears a fixed fraction of the swoop.
        assert!(pad_v(RING_MEDIUM, 3) >= swoop_c(RING_MEDIUM, 3) * 0.3);
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
