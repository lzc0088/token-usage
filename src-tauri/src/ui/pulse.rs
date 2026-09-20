//! Pulse-style floating quota panel — a row or column of circular progress
//! rings, one per connected vendor account. Renders in a borderless, transparent,
//! always-on-top Tauri webview window with SVG rings.
//!
//! Inspired by the Pulse macOS app's NSPanel dock, but implemented as a Tauri
//! webview for consistency with the existing window stack. The ring window
//! NEVER resizes: the hover detail card renders in a second fixed-size
//! window ("pulse-card") that is only ever MOVED — resizing a transparent
//! window whose content re-anchors presents one stale web-process frame
//! (the ghost/flash); pure moves are always clean.
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
const TITLE_BLOCK: f64 = 27.0;

/// Fixed vertical padding (user-specified: padding-top 30px).
const PAD_V: f64 = 30.0;

/// Panel height cap (logical px) — beyond this the ring dock scrolls.
const MAX_PANEL_H: f64 = 520.0;

/// Horizontal window padding (logical px) — flush panels use 18px on the
/// screen-interior side + 10px on the fused edge; the window keeps slack.
const PANEL_PAD: f64 = 16.0;

/// Detail card window size (logical px) — FIXED at creation. The card
/// window is only ever MOVED (on hover), never resized: resizing a
/// transparent window whose content must re-anchor (a flush-right panel
/// grows LEFTWARD) presents one stale frame from the web process before
/// the re-layout lands — that reads as the ghost/flash. Pure moves keep
/// the rendered content viewport-fixed → ghost-free.
const CARD_WIN_W: f64 = 340.0;
const CARD_WIN_H: f64 = 480.0;

/// Clearance between the panel edge and the card window (logical px).
/// Negative = the window stops 10px short of the panel; the arrow
/// protrudes ~12px from the card body, putting its tip ≈20px off the
/// panel edge (user-specified spacing).
const CARD_GAP: f64 = -10.0;

/// Vertical offset of the ring rail inside the pulse window (pad + title).
const RAIL_TOP: f64 = PAD_V + TITLE_BLOCK;

/// KV key persisting the pulse panel's on-screen position.
const POS_KEY: &str = "pulse_pos";

/// Hot-path cache for card-window placement: dock order, ring pitch, theme,
/// opacity. Refreshed by `push_pulse_data` (every quota/config change), so a
/// hover expand does ZERO database work — expand_pulse is a synchronous
/// command on the main thread, and per-hover DB reads made hovering janky.
#[derive(Clone)]
struct DockCache {
    vendors: Vec<String>,
    diameter: f64,
    panel_w: f64,
    theme: String,
    opacity: f64,
}

static DOCK_CACHE: std::sync::Mutex<Option<DockCache>> = std::sync::Mutex::new(None);

/// Rebuild the dock cache from data that is already loaded (no extra reads).
fn refresh_dock_cache(cfg: &config::Config, theme: &str, quotas: &[PulseQuota]) {
    let d = ring_diameter(&cfg.pulse_size);
    let cache = DockCache {
        vendors: quotas.iter().map(|q| q.vendor.clone()).collect(),
        diameter: d,
        panel_w: d + PANEL_PAD * 2.0,
        theme: theme.to_string(),
        opacity: cfg.pulse_opacity.clamp(0.2, 1.0),
    };
    *DOCK_CACHE.lock().unwrap_or_else(|e| e.into_inner()) = Some(cache);
}

/// Dock cache for the hover hot path. Cold (push never ran yet) → build
/// once from the DB; afterwards it is refreshed by push_pulse_data.
fn dock_cache(app: &AppHandle) -> Option<DockCache> {
    if let Some(c) = DOCK_CACHE.lock().unwrap_or_else(|e| e.into_inner()).clone() {
        return Some(c);
    }
    let state = app.try_state::<AppState>()?;
    let conn = state.db.lock().ok()?;
    let cfg = config::load(&conn).ok()?;
    let quotas = load_quotas(&conn);
    let theme = resolved_theme(app, &cfg);
    refresh_dock_cache(&cfg, &theme, &quotas);
    DOCK_CACHE.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

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
        if let Some(c) = app.get_webview_window("pulse-card") {
            apply_floating_level(&c, cfg.pulse_topmost);
            // Ordered-in but parked off-screen: the webview stays live so
            // the first hover shows instantly (hidden webviews get
            // throttled by macOS). The window is empty + transparent here,
            // so the brief on-screen moment is invisible.
            let _ = c.show();
            park_card(&c);
        }
        push_pulse_data(app, conn);
    } else {
        hide_pulse(app);
    }
}

/// Hide the pulse panel (and its card window).
pub fn hide_pulse(app: &AppHandle) {
    for label in ["pulse", "pulse-card"] {
        if let Some(h) = app.get_webview_window(label) {
            let _ = h.hide();
        }
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

    // Keep the hover hot-path cache in lockstep with what was just pushed.
    refresh_dock_cache(&cfg, &payload.theme, &payload.quotas);

    // Both windows consume the payload (the card window follows theme,
    // opacity, and live usage updates while it happens to be open).
    for w in [
        app.get_webview_window("pulse"),
        app.get_webview_window("pulse-card"),
    ]
    .into_iter()
    .flatten()
    {
        let _ = w.emit("pulse:update", &payload);
    }
}

/// Load vendor quotas from the quota_cache table, filtered AND ordered by
/// the same rules the main window's quota page (`get_quotas`) applies:
/// `quota_active_vendors` for visibility, `quota_vendor_order` (falling back
/// to the active list order) for sorting — the pulse dock mirrors the
/// limits page exactly.
fn load_quotas(conn: &Connection) -> Vec<PulseQuota> {
    let cfg = config::load(conn).unwrap_or_default();
    // null = not configured → show all; otherwise only listed vendors.
    let active: Option<std::collections::HashSet<String>> = cfg
        .quota_active_vendors
        .clone()
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

                    // Plan-less vendor: no subscription expiry, value shows as
                    // balance/credits (e.g. DeepSeek balance, WorkBuddy credits).
                    // Subscriptions WITH a balance (e.g. StepFun) keep the ring.
                    let planless = q.expires_at.is_none()
                        && (q.balance.is_some()
                            || q.windows.iter().any(|w| w.total_value.is_some()));

                    quotas.push(PulseQuota {
                        vendor: q.vendor.clone(),
                        plan: q.plan_label.clone(),
                        planless,
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
    // Mirror get_quotas: sort by the user's custom vendor order, falling
    // back to active-vendors order; unknown vendors go last.
    let order_key = cfg.quota_vendor_order.or(cfg.quota_active_vendors);
    if let Some(order_ref) = order_key {
        let order: std::collections::HashMap<&str, usize> = order_ref
            .iter()
            .enumerate()
            .map(|(i, v)| (v.as_str(), i))
            .collect();
        quotas.sort_by_key(|q| order.get(q.vendor.as_str()).copied().unwrap_or(usize::MAX));
    }
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

/// Ring center Y inside the pulse window (logical px from the window top),
/// mirroring the frontend ring pitch (ring + LABEL_H + RING_GAP per item).
fn ring_center_y(d: f64, index: usize) -> f64 {
    RAIL_TOP + index as f64 * (d + LABEL_H + RING_GAP) + d / 2.0
}

/// Card window origin X for a panel at `px` of width `w`. Flush-right →
/// the card opens LEFT of the panel (into the screen interior); otherwise
/// RIGHT. The window's panel-facing edge sits CARD_GAP off the panel.
fn card_window_x(px: f64, w: f64, flush_right: bool) -> f64 {
    if flush_right {
        px - CARD_WIN_W + CARD_GAP
    } else {
        px + w - CARD_GAP
    }
}

/// Card window origin Y — the window CENTER aligns with the hovered ring's
/// screen Y (the card centers itself in the window, so the arrow lands on
/// the same line). Clamped on screen.
fn card_window_y(ring_center_y_screen: f64, mon_top: f64, mon_bottom: f64) -> f64 {
    (ring_center_y_screen - CARD_WIN_H / 2.0)
        .max(mon_top + 4.0)
        .min(mon_bottom - CARD_WIN_H - 4.0)
        .max(mon_top + 4.0)
}

/// Show the detail card for a vendor. The card renders in its OWN fixed-
/// size window ("pulse-card") that is only ever MOVED — the ring window
/// never resizes. Resizing the ring window made its re-anchored content
/// (flush-right panel grows LEFTWARD) present one stale web-process frame
/// — the ghost users saw. Moving windows keeps content viewport-fixed.
pub fn expand_pulse(app: &AppHandle, vendor: Option<String>) {
    let (Some(ring), Some(card)) = (
        app.get_webview_window("pulse"),
        app.get_webview_window("pulse-card"),
    ) else {
        return;
    };
    // Pre-warm (vendor=None) is obsolete — there is no window resize to
    // warm anymore; only a real ring hover shows the card.
    let Some(vendor) = vendor else {
        return;
    };
    // A fresh expand is hover activity — cancel any stale hide thread.
    pulse_activity();

    // Hot path: dock layout comes from the cache (refreshed on every
    // quota/config push) — no DB access on the main thread per hover.
    let Some(dock) = dock_cache(app) else {
        return;
    };

    let index = dock.vendors.iter().position(|v| *v == vendor).unwrap_or(0);

    let mut card_on_left = false;
    if let (Ok(pos), Ok(Some(mon))) = (ring.outer_position(), ring.current_monitor()) {
        let scale = ring.scale_factor().unwrap_or(1.0).max(1.0);
        let px = pos.x as f64 / scale;
        let py = pos.y as f64 / scale;
        let mon_right = mon.position().x as f64 / scale + mon.size().width as f64 / scale;
        let mon_top = mon.position().y as f64 / scale;
        let mon_bottom = mon.position().y as f64 / scale + mon.size().height as f64 / scale;
        card_on_left = mon_right - (px + dock.panel_w) <= 8.0;
        let x = card_window_x(px, dock.panel_w, card_on_left);
        let y = card_window_y(
            py + ring_center_y(dock.diameter, index),
            mon_top,
            mon_bottom,
        );
        // Pure move (the window keeps its creation size) — never a resize.
        let _ = card.set_position(LogicalPosition::new(x, y));
    }

    let _ = card.emit(
        "pulse:card",
        serde_json::json!({
            "vendor": vendor,
            "cardOnLeft": card_on_left,
            "theme": dock.theme,
            "opacity": dock.opacity,
        }),
    );
    let _ = card.show();
}

/// Hover-activity generation: every pointer enter (panel OR card window)
/// bumps it, cancelling every pending hide scheduled against an older
/// generation. Atomic because idle/activity arrive from two webviews.
static CARD_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Graceful-hide timings: linger before hiding after the last leave, then
/// the fade window before the card is parked off-screen.
const IDLE_MS: u64 = 160;
const FADE_MS: u64 = 190;

/// Pointer entered the panel/card window — cancels any pending card hide.
pub fn pulse_activity() {
    use std::sync::atomic::Ordering;
    CARD_GEN.fetch_add(1, Ordering::Relaxed);
}

/// Pointer left the panel (or the visible card) — hide the card after
/// IDLE_MS unless new activity bumps the generation. Crossing from the
/// panel to the card traverses the card window's inert transparent
/// margin (~20px); the idle window covers that transit.
pub fn pulse_idle(app: &AppHandle) {
    schedule_card_hide(app, IDLE_MS);
}

/// Immediate graceful hide (explicit collapse): fade first, hide after.
pub fn collapse_pulse(app: &AppHandle) {
    schedule_card_hide(app, 0);
}

/// Schedule the graceful hide: after `delay`, emit `pulse:card-hide` (the
/// card fades its content out), then PARK the window off-screen FADE_MS
/// later — unless the generation moved on (re-hover). Parking (instead of
/// hide) keeps the WKWebView live: a hidden window's web process gets
/// throttled by macOS, delaying both the next show and the fade — that
/// read as "hover is sluggish / needs a click".
fn schedule_card_hide(app: &AppHandle, delay: u64) {
    use std::sync::atomic::Ordering;
    let gen = CARD_GEN.fetch_add(1, Ordering::Relaxed);
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(delay));
        if CARD_GEN.load(Ordering::Relaxed) != gen + 1 {
            return; // re-hovered in the meantime — keep the card
        }
        let Some(card) = app.get_webview_window("pulse-card") else {
            return;
        };
        let _ = card.emit("pulse:card-hide", ());
        std::thread::sleep(std::time::Duration::from_millis(FADE_MS));
        if CARD_GEN.load(Ordering::Relaxed) != gen + 1 {
            return;
        }
        park_card(&card);
    });
}

/// Move the card window off-screen (just beyond its monitor's bottom-right
/// corner) instead of hiding it — the webview stays live and event-ready,
/// and an off-screen window owns no interactable pixels. Show is then a
/// pure position move back on-screen.
fn park_card(card: &tauri::WebviewWindow) {
    let (x, y) = match card.current_monitor() {
        Ok(Some(mon)) => {
            let scale = card.scale_factor().unwrap_or(1.0).max(1.0);
            (
                mon.position().x as f64 / scale + mon.size().width as f64 / scale + 100.0,
                mon.position().y as f64 / scale + mon.size().height as f64 / scale + 100.0,
            )
        }
        _ => (-4000.0, -4000.0),
    };
    let _ = card.set_position(LogicalPosition::new(x, y));
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
    /// Credits/balance-only vendor (no plan expiry): the ring shows the
    /// remaining amount instead of a progress arc + percentage.
    pub planless: bool,
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
        // height = 30*2 + 27 (title block) + 235 = 322.
        if d == RING_MEDIUM {
            let (_, h3) = collapsed_size(&cfg, 3);
            assert!((h3 - 322.0).abs() < 0.01, "medium n=3: {h3}");
        }
        // Zero rings degrade to the single-ring minimum, never negative.
        let (_, h0) = collapsed_size(&cfg, 0);
        let (_, h1) = collapsed_size(&cfg, 1);
        assert_eq!(h0, h1);
    }

    #[test]
    fn card_window_x_flanks_the_panel() {
        // Flush-right: the card opens LEFT, its window's right edge sits
        // CARD_GAP inside the panel's left edge.
        let x = card_window_x(1432.0, 80.0, true);
        assert!((x + CARD_WIN_W - (1432.0 + CARD_GAP)).abs() < f64::EPSILON);
        // Otherwise the card opens RIGHT, window's left edge CARD_GAP past
        // the panel's right edge.
        let x = card_window_x(100.0, 80.0, false);
        assert!((x - (100.0 + 80.0 - CARD_GAP)).abs() < f64::EPSILON);
    }

    #[test]
    fn card_window_y_centers_on_ring_and_clamps() {
        // Window center lands on the ring line.
        let y = card_window_y(500.0, 0.0, 982.0);
        assert!((y + CARD_WIN_H / 2.0 - 500.0).abs() < f64::EPSILON);
        // Ring near the screen top → clamp to mon_top + 4.
        assert!((card_window_y(50.0, 0.0, 982.0) - 4.0).abs() < f64::EPSILON);
        // Ring near the screen bottom → clamp to mon_bottom - H - 4.
        assert!(
            (card_window_y(960.0, 0.0, 982.0) - (982.0 - CARD_WIN_H - 4.0)).abs() < f64::EPSILON
        );
    }

    #[test]
    fn ring_center_y_mirrors_frontend_ring_pitch() {
        // medium (48): pitch = 48 + 21 + 14 = 83; first ring center at
        // RAIL_TOP + 24.
        let d = 48.0;
        assert!((ring_center_y(d, 0) - (RAIL_TOP + 24.0)).abs() < f64::EPSILON);
        assert!((ring_center_y(d, 2) - (RAIL_TOP + 2.0 * 83.0 + 24.0)).abs() < f64::EPSILON);
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

    #[test]
    fn load_quotas_mirrors_quota_page_order_and_planless() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::storage::schema::migrate(&conn).unwrap();
        let cfg = config::Config {
            quota_active_vendors: Some(vec!["glm".into(), "workbuddy".into(), "deepseek".into()]),
            quota_vendor_order: Some(vec!["workbuddy".into(), "deepseek".into(), "glm".into()]),
            ..Default::default()
        };
        crate::config::save(&conn, &cfg).unwrap();
        let insert = |v: &str, data: &str| {
            let _ = conn.execute(
                "INSERT INTO quota_cache (vendor, data, fetched_at) VALUES (?, ?, 1)",
                rusqlite::params![v, data],
            );
        };
        insert(
            "glm",
            r#"{"vendor":"glm","status":"ok","expires_at":"2027-05-17T00:00:00+08:00","windows":[{"label":"5h","used_pct":42.0}]}"#,
        );
        insert(
            "workbuddy",
            r#"{"vendor":"workbuddy","status":"ok","windows":[{"label":"Credits","used_pct":1.0,"total_value":3460.0,"used_value":35.0}]}"#,
        );
        insert(
            "deepseek",
            r#"{"vendor":"deepseek","status":"ok","windows":[],"balance":{"amount":19.64,"currency":"CNY"}}"#,
        );
        // Not in the active list → filtered out entirely.
        insert(
            "volcengine",
            r#"{"vendor":"volcengine","status":"ok","windows":[]}"#,
        );

        let qs = load_quotas(&conn);
        let vendors: Vec<&str> = qs.iter().map(|q| q.vendor.as_str()).collect();
        assert_eq!(vendors, vec!["workbuddy", "deepseek", "glm"]);
        assert!(qs[0].planless, "workbuddy credits-only → planless");
        assert!(qs[1].planless, "deepseek balance-only → planless");
        assert!(!qs[2].planless, "glm subscription keeps the ring");
    }
}
