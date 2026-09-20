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

/// Fixed vertical padding (logical px) — mirrors PulseApp.svelte CSS (fixed, not scale-dependent).
const PAD_V: f64 = 36.0;
/// Bottom padding is slightly larger for visual breathing room.
const PAD_V_BOT: f64 = 44.0;

/// Panel height cap: 80% of the current screen's height — with many vendors
/// enabled the ring dock scrolls only past that (user-tuned "show as much
/// as possible"). `MAX_PANEL_H` is the fallback when no monitor is readable.
const MAX_PANEL_H_RATIO: f64 = 0.8;
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

/// Card window offset from the panel edge (logical px). The card body
/// hugs the window's panel-facing edge (slot padding 13 = arrow room), so
/// the arrow tip lands at CARD_GAP − 1 relative to the panel edge —
/// −9 → the tip stops 10px SHORT of the edge (user-tuned −10px),
/// deterministic regardless of card width.
const CARD_GAP: f64 = -9.0;

/// ── Peek mode (auto-hidden panel) ──────────────────────────────────────
/// Modeled on token-monitor's edgeDock: the peek handle is its OWN tiny
/// window ("pulse-peek", 8×58 — a 34px grip with 12px shoulder curves),
/// flush with the screen edge and vertically centered on the rail. The
/// panel window itself NEVER resizes — it is only shown/hidden, which is
/// what keeps the transparent-window ghost/flash away (pure visibility
/// toggles present no stale re-anchored frame).
const PEEK_WIN_W: f64 = 8.0;
const PEEK_WIN_H: f64 = 58.0;
/// Delay before expanding after the cursor enters the handle (ms).
const PEEK_REVEAL_MS: u64 = 140;
/// Grace period after the cursor leaves before collapsing (ms).
const PEEK_HIDE_MS: u64 = 200;

/// Peek state machine — tracks whether the panel is fully visible,
/// collapsed behind the peek handle, or in a transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PeekState {
    Visible,
    Hidden,
    Revealing,
    Hiding,
}

/// Global peek state (protected by a separate mutex from the dock cache).
static PEEK_STATE: std::sync::Mutex<(PeekState, Option<std::time::Instant>)> =
    std::sync::Mutex::new((PeekState::Visible, None));

fn peek_state() -> (PeekState, Option<std::time::Instant>) {
    *PEEK_STATE.lock().unwrap_or_else(|e| e.into_inner())
}

fn set_peek_state(st: PeekState, t: Option<std::time::Instant>) {
    *PEEK_STATE.lock().unwrap_or_else(|e| e.into_inner()) = (st, t);
}

/// Place the peek handle window at the configured screen edge, vertically
/// centered on the panel's rail. The handle's edge always matches
/// `cfg.pulse_side` — it does not follow the panel's current x position
/// (the panel may have been dragged away from the edge, but the handle must
/// stay glued to the configured edge so users always know where to find it).
/// Takes `conn` from the caller — callers (set_config / lib.rs setup) already
/// hold the DB guard, and re-locking it on this thread self-deadlocks.
fn position_peek(app: &AppHandle, conn: &Connection) {
    let (Some(peek), Some(ring)) = (
        app.get_webview_window("pulse-peek"),
        app.get_webview_window("pulse"),
    ) else {
        return;
    };
    let cfg = config::load(conn).unwrap_or_default();
    let (Ok(Some(mon)), Ok(pos), Ok(size)) = (
        peek.current_monitor(),
        ring.outer_position(),
        ring.outer_size(),
    ) else {
        return;
    };
    let scale = ring.scale_factor().unwrap_or(1.0).max(1.0);
    let py = pos.y as f64 / scale;
    let panel_h = size.height as f64 / scale;
    let mon_left = mon.position().x as f64 / scale;
    let mon_right = mon_left + mon.size().width as f64 / scale;
    let mon_top = mon.position().y as f64 / scale;
    let mon_bottom = mon_top + mon.size().height as f64 / scale;
    // Handle x = configured edge (left edge → mon_left; right edge → mon_right - handle_w).
    let x = if cfg.pulse_side == "left" {
        mon_left
    } else {
        mon_right - PEEK_WIN_W
    };
    // Vertical center of the panel's rail (not the whole window — the rail
    // starts at RAIL_TOP below the title block).
    let rail_top = (PAD_V + TITLE_BLOCK) * layout_scale(ring_diameter(&cfg.pulse_size));
    let handle_y = (py + rail_top + (panel_h - rail_top - PAD_V_BOT) / 2.0 - PEEK_WIN_H / 2.0)
        .max(mon_top + 4.0)
        .min(mon_bottom - PEEK_WIN_H - 4.0);
    let _ = peek.set_position(LogicalPosition::new(x, handle_y));
}

/// Show the full panel, retire the peek handle (reveal transition end).
/// Returns true only when the panel is CONFIRMED visible: the show() call
/// from the poller thread travels the event-loop proxy asynchronously, so
/// the peek handle must not be retired on an unverified show — a failed or
/// raced show would otherwise dead-lock (no handle, no panel, no retry).
/// On false the poller stays in Revealing and retries while the cursor
/// still targets the edge; the handle never disappears on a bad reveal.
fn reveal_pulse(app: &AppHandle) -> bool {
    let Some(ring) = app.get_webview_window("pulse") else {
        return false;
    };
    let shown = match ring.show() {
        Ok(()) => ring.is_visible().unwrap_or(false),
        Err(e) => {
            tracing::warn!("peek reveal: ring show failed: {e}");
            false
        }
    };
    if !shown {
        // is_visible() is ordered after show() through the same event
        // queue, so this is a REAL failure, not a race — log the geometry
        // to identify why (off-screen / zero size / closed window).
        tracing::warn!(
            pos = ?ring.outer_position().ok(),
            size = ?ring.outer_size().ok(),
            "peek reveal: ring not visible after show"
        );
        return false;
    }
    if let Some(peek) = app.get_webview_window("pulse-peek") {
        let _ = peek.hide();
    }
    set_peek_state(PeekState::Visible, None);
    true
}

/// Hide the panel, surface the peek handle at the edge (collapse end).
/// Runs on the poller thread (no DB guard held), so it may lock the DB to
/// read config for the handle's edge.
fn collapse_pulse(app: &AppHandle) {
    if let Some(ring) = app.get_webview_window("pulse") {
        let _ = ring.hide();
    }
    if let Some(card) = app.get_webview_window("pulse-card") {
        park_card(&card);
    }
    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(conn) = state.db.lock() {
            position_peek(app, &conn);
        }
    }
    if let Some(peek) = app.get_webview_window("pulse-peek") {
        let _ = peek.show();
    }
    set_peek_state(PeekState::Hidden, None);
}

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
    /// Whether peek (auto-hide) mode is enabled from config.
    peek_enabled: bool,
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
        peek_enabled: cfg.pulse_display_mode == "auto_hide",
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
    // Resolve under the lock, apply after releasing: resolved_theme queries
    // the window (main thread); holding the DB guard across it deadlocks
    // against a main-thread holder waiting for this lock (set_config).
    let (cfg, quotas) = {
        let conn = state.db.lock().ok()?;
        (config::load(&conn).ok()?, load_quotas(&conn))
    };
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
        let auto_hide = cfg.pulse_display_mode == "auto_hide";
        position_pulse(app, conn);
        if let Some(h) = app.get_webview_window("pulse") {
            apply_floating_level(&h, cfg.pulse_topmost);
            // Auto-hide starts collapsed: panel hidden, peek handle shown.
            // Pure visibility toggles — the window is never resized here.
            if auto_hide {
                // The peek handle sits flush at the SCREEN edge, so the panel
                // must be edge-docked too — otherwise the reveal shows the
                // panel away from the cursor and it collapses in a loop.
                snap_pulse_to_edge(conn, &h);
                let _ = h.hide();
            } else {
                let _ = h.show();
            }
        }
        if let Some(c) = app.get_webview_window("pulse-card") {
            apply_floating_level(&c, cfg.pulse_topmost);
            let _ = c.show();
            // park_card must never lock the DB (its callers — set_config /
            // lib.rs setup — already hold the guard; re-locking self-deadlocks).
            park_card(&c);
        }
        if let Some(p) = app.get_webview_window("pulse-peek") {
            apply_floating_level(&p, cfg.pulse_topmost);
            if auto_hide {
                position_peek(app, conn);
                let _ = p.show();
            } else {
                let _ = p.hide();
            }
        }
        set_peek_state(
            if auto_hide {
                PeekState::Hidden
            } else {
                PeekState::Visible
            },
            None,
        );
        // Cursor-driven hover (see ensure_hover_poller) — one poller ever.
        ensure_hover_poller(app);
        push_pulse_data(app, conn);
    } else {
        hide_pulse(app);
    }
}

/// Hide the pulse panel (and its card + peek handle windows).
pub fn hide_pulse(app: &AppHandle) {
    for label in ["pulse", "pulse-card", "pulse-peek"] {
        if let Some(h) = app.get_webview_window(label) {
            let _ = h.hide();
        }
    }
    set_peek_state(PeekState::Visible, None);
}

/// Build the pulse frontend payload (quotas + panel settings).
pub fn build_pulse_data(app: &AppHandle, conn: &Connection) -> PulseData {
    let cfg = config::load(conn).unwrap_or_default();
    let max_panel_h = app
        .get_webview_window("pulse")
        .map(|w| panel_max_h(&w))
        .unwrap_or(MAX_PANEL_H);
    PulseData {
        quotas: load_quotas(conn),
        size: cfg.pulse_size.clone(),
        ring_diameter: ring_diameter(&cfg.pulse_size),
        theme: resolved_theme(app, &cfg),
        opacity: cfg.pulse_opacity.clamp(0.2, 1.0),
        max_panel_h,
        side: cfg.pulse_side.clone(),
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

    // All three windows consume the payload (the card follows theme/opacity
    // while open; the peek handle follows theme + side for its silhouette).
    for w in [
        app.get_webview_window("pulse"),
        app.get_webview_window("pulse-card"),
        app.get_webview_window("pulse-peek"),
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
                        refreshed_at: q.refreshed_at.clone(),
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
/// Label height and gaps scale with the ring preset (layout_scale).
fn dock_height(d: f64, n: usize) -> f64 {
    let s = layout_scale(d);
    let n = n.max(1) as f64;
    n * (d + LABEL_H * s) + (n - 1.0) * RING_GAP * s
}

/// Layout scale for a ring diameter: typography and vertical blocks
/// (padding, title, label line, gaps) track the ring size preset (d/48 →
/// 0.75 / 1 / 1.25) so the panel reads as one proportioned unit at every
/// size — fixed-metric fonts clipped at the small preset. The frontend
/// derives the SAME factor from `ring_diameter` (PulseApp / QuotaRing).
fn layout_scale(d: f64) -> f64 {
    d / RING_MEDIUM
}

/// Dock height for n rings, capped so the panel never exceeds `max_panel_h`
/// — the CSS dock scrolls when the natural height overflows.
fn dock_height_capped(d: f64, n: usize, max_panel_h: f64) -> f64 {
    let avail = (max_panel_h - (PAD_V + PAD_V_BOT + TITLE_BLOCK * layout_scale(d))).max(60.0);
    dock_height(d, n).min(avail)
}

/// Collapsed (ring-only) window size, logical px. Mirrors the CSS layout:
/// fixed vertical padding + scaled title block + capped dock height.
fn collapsed_size(cfg: &config::Config, ring_count: usize, max_panel_h: f64) -> (f64, f64) {
    let d = ring_diameter(&cfg.pulse_size);
    (
        d + PANEL_PAD * 2.0,
        PAD_V
            + PAD_V_BOT
            + TITLE_BLOCK * layout_scale(d)
            + dock_height_capped(d, ring_count, max_panel_h),
    )
}

/// Effective panel height cap (logical px) for the window's CURRENT screen:
/// 80% of that screen's height, floored to whole px so the Rust window size
/// and the frontend CSS (fed via `PulseData.max_panel_h`) agree exactly.
/// Falls back to the fixed `MAX_PANEL_H` when no monitor can be read.
fn panel_max_h(win: &tauri::WebviewWindow) -> f64 {
    let mon = win
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| win.primary_monitor().ok().flatten());
    let Some(mon) = mon else {
        return MAX_PANEL_H;
    };
    let scale = win.scale_factor().unwrap_or(1.0).max(1.0);
    ((mon.size().height as f64 / scale) * MAX_PANEL_H_RATIO).floor()
}

/// Ring center Y inside the pulse window (logical px from the window top),
/// mirroring the frontend ring pitch (ring + scaled label + scaled gap).
fn ring_center_y(d: f64, index: usize) -> f64 {
    let s = layout_scale(d);
    RAIL_TOP * s + index as f64 * (d + (LABEL_H + RING_GAP) * s) + d / 2.0
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

/// Which side of the panel has more room for the fixed-size card window.
/// A panel near an edge (but not fused) has < CARD_WIN_W of room on that
/// side — opening the card there would straddle the screen border
/// invisibly, so the card opens into the roomier side instead ("哪边距离
/// 显示在哪边"). Ties (dead center) stay right.
fn card_on_left_side(px: f64, panel_w: f64, mon_left: f64, mon_right: f64) -> bool {
    let room_left = px - mon_left;
    let room_right = mon_right - (px + panel_w);
    room_left > room_right
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

/// Show the detail card for a vendor: position the card window beside the
/// hovered ring (pure move) and notify its webview. Shared by the hover
/// poller. Returns the window rect so the poller can hit-test keep-alive.
fn show_card_for(app: &AppHandle, dock: &DockCache, index: usize) -> bool {
    let (Some(ring), Some(card)) = (
        app.get_webview_window("pulse"),
        app.get_webview_window("pulse-card"),
    ) else {
        return false;
    };
    let vendor = dock.vendors.get(index).cloned().unwrap_or_default();
    let mut card_on_left = false;
    if let (Ok(pos), Ok(Some(mon))) = (ring.outer_position(), ring.current_monitor()) {
        let scale = ring.scale_factor().unwrap_or(1.0).max(1.0);
        let px = pos.x as f64 / scale;
        let py = pos.y as f64 / scale;
        let mon_left = mon.position().x as f64 / scale;
        let mon_right = mon_left + mon.size().width as f64 / scale;
        let mon_top = mon.position().y as f64 / scale;
        let mon_bottom = mon.position().y as f64 / scale + mon.size().height as f64 / scale;
        // Card opens into the side with more room (flush-right panels have
        // ~0 right-room → left; floating near-edge panels follow distance).
        card_on_left = card_on_left_side(px, dock.panel_w, mon_left, mon_right);
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
    card_on_left
}

/// Move the card window off-screen instead of hiding it — the webview stays
/// live and event-ready, and an off-screen window owns no interactable pixels.
/// Show is then a pure position move back on-screen. Must never lock the DB:
/// callers (set_config / lib.rs setup) already hold the guard, and re-locking
/// it on the same thread self-deadlocks.
fn park_card(card: &tauri::WebviewWindow) {
    // Park the card off-screen.
    let (cx, cy) = match card.current_monitor() {
        Ok(Some(mon)) => {
            let scale = card.scale_factor().unwrap_or(1.0).max(1.0);
            (
                mon.position().x as f64 / scale + mon.size().width as f64 / scale + 100.0,
                mon.position().y as f64 / scale + mon.size().height as f64 / scale + 100.0,
            )
        }
        _ => (-4000.0, -4000.0),
    };
    let _ = card.set_position(LogicalPosition::new(cx, cy));
}

/// Ask the card webview to fade its content out (graceful-hide step one).
fn emit_card_hide(app: &AppHandle) {
    if let Some(card) = app.get_webview_window("pulse-card") {
        let _ = card.emit("pulse:card-hide", ());
    }
}

/// ══ Global-pointer hover poller ═══════════════════════════════════════
/// The card show/hide is driven from the SYSTEM cursor position polled in
/// Rust, not from WKWebView DOM hover events — after the two-window split
/// those proved unreliable (a non-key webview can stop delivering hover
/// until clicked). `NSEvent.mouseLocation` is thread-safe and needs no
/// permissions; 30Hz hit-testing costs nothing.
const POLL_MS: u64 = 33;
/// Linger after the pointer leaves both windows before hiding the card.
const HOVER_LINGER: std::time::Duration = std::time::Duration::from_millis(150);
/// Fade window between the hide signal and parking the window off-screen.
const HOVER_FADE: std::time::Duration = std::time::Duration::from_millis(190);
/// Grace margin around both windows that still counts as "inside".
const HOVER_GRACE: f64 = 12.0;
/// Card height reported by the card webview (measured content height, with
/// an estimate fallback) — the poller hit-tests the VISIBLE card, never
/// the whole (larger) window.
static CARD_H_REPORT: std::sync::Mutex<f64> = std::sync::Mutex::new(250.0);

/// Frontend (PulseCardApp) reports its measured card height here.
pub fn report_card_height(height: f64) {
    let h = height.clamp(60.0, CARD_WIN_H - 16.0);
    *CARD_H_REPORT.lock().unwrap_or_else(|e| e.into_inner()) = h;
}

/// Keep-alive hit-rect of the VISIBLE card, window-relative: the window is
/// larger than the card it hosts, and only the card's own rect keeps it
/// alive — x spans the max card width anchored to the panel-facing side
/// (slot padding 13), y spans the vertically-centered measured height.
fn card_keepalive_rect(card_on_left: bool, card_h: f64) -> ((f64, f64), (f64, f64)) {
    const CARD_MAX_W: f64 = 297.0;
    const SLOT_PAD: f64 = 13.0;
    let slack = 6.0;
    let x = if card_on_left {
        (
            CARD_WIN_W - SLOT_PAD - CARD_MAX_W - slack,
            CARD_WIN_W - SLOT_PAD + slack,
        )
    } else {
        (SLOT_PAD - slack, SLOT_PAD + CARD_MAX_W + slack)
    };
    let y = (
        (CARD_WIN_H - card_h) / 2.0 - 8.0,
        (CARD_WIN_H + card_h) / 2.0 + 8.0,
    );
    (x, y)
}
/// Primary-monitor height cache refresh cadence (ticks).
const PRIMARY_REFRESH: u32 = 90;

static POLLER_STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Ring index for a pointer y INSIDE the panel window, or None. Only the
/// circle band counts (crossing the label/gap zones between rings must not
/// flip cards).
fn ring_index_at(y: f64, d: f64, count: usize) -> Option<usize> {
    let rail = RAIL_TOP * layout_scale(d);
    if y < rail {
        return None;
    }
    let rel = y - rail;
    let pitch = d + (LABEL_H + RING_GAP) * layout_scale(d);
    let idx = (rel / pitch).floor() as usize;
    if idx >= count {
        return None;
    }
    let in_item = rel - idx as f64 * pitch;
    (in_item <= d + 4.0).then_some(idx)
}

/// Thread-safe global cursor position (AppKit coords flipped to top-left
/// logical using the primary screen height, mirroring tao's conversion).
#[cfg(target_os = "macos")]
fn global_mouse_topleft(primary_h: f64) -> Option<(f64, f64)> {
    use objc::{class, msg_send, sel, sel_impl};
    #[repr(C)]
    struct NSPoint {
        x: f64,
        y: f64,
    }
    unsafe {
        let pt: NSPoint = msg_send![class!(NSEvent), mouseLocation];
        Some((pt.x, primary_h - pt.y))
    }
}

#[cfg(not(target_os = "macos"))]
fn global_mouse_topleft(_primary_h: f64) -> Option<(f64, f64)> {
    None
}

/// Start the hover poller once. Every tick: hit-test the system cursor
/// against the ring bands (panel) and the visible card region; show the
/// hovered vendor's card, keep it while the pointer rests on either
/// window, and gracefully hide it after a linger once the pointer leaves.
pub fn ensure_hover_poller(app: &AppHandle) {
    use std::sync::atomic::Ordering;
    if POLLER_STARTED.swap(true, Ordering::Relaxed) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let mut visible = false;
        let mut shown: Option<String> = None;
        let mut card_on_left = false;
        let mut hiding_since: Option<std::time::Instant> = None;
        let mut last_inside = None::<std::time::Instant>;
        let mut last_panel_pos = (0.0, 0.0);
        let mut primary_h = 0.0f64;
        let mut tick: u32 = 0;
        loop {
            std::thread::sleep(std::time::Duration::from_millis(POLL_MS));
            tick = tick.wrapping_add(1);
            if tick % PRIMARY_REFRESH == 0 {
                if let Ok(Some(m)) = app.primary_monitor() {
                    let f = m.scale_factor();
                    primary_h = m.size().height as f64 / f;
                }
            }
            let Some((mx, my)) = global_mouse_topleft(primary_h) else {
                continue;
            };

            let (Some(ring), Some(card)) = (
                app.get_webview_window("pulse"),
                app.get_webview_window("pulse-card"),
            ) else {
                break;
            };
            let Some(dock) = dock_cache(&app) else {
                continue;
            };

            let scale = ring.scale_factor().unwrap_or(1.0).max(1.0);
            let (Ok(pos), Ok(size)) = (ring.outer_position(), ring.outer_size()) else {
                continue;
            };
            let (px, py) = (pos.x as f64 / scale, pos.y as f64 / scale);
            let (w_c, h_c) = (size.width as f64 / scale, size.height as f64 / scale);

            // ── Auto-hide: panel hidden behind the peek handle. The reveal
            // trigger is the handle window itself (with hover grace) — only
            // a deliberate move ONTO the visible handle reveals the panel.
            if !ring.is_visible().unwrap_or(false) {
                if visible {
                    park_card(&card);
                    visible = false;
                    shown = None;
                    hiding_since = None;
                }
                if dock.peek_enabled {
                    // Reveal trigger: the peek handle window ONLY (graced) —
                    // not the whole screen edge, so sweeping the cursor to
                    // the edge (scrollbars, hot corners) never pops the panel.
                    let in_trigger = app
                        .get_webview_window("pulse-peek")
                        .and_then(|p| {
                            let s = p.scale_factor().unwrap_or(1.0).max(1.0);
                            p.outer_position()
                                .ok()
                                .map(|pp| (pp.x as f64 / s, pp.y as f64 / s))
                        })
                        .map(|(qx, qy)| {
                            mx >= qx - HOVER_GRACE
                                && mx < qx + PEEK_WIN_W + HOVER_GRACE
                                && my >= qy - HOVER_GRACE
                                && my < qy + PEEK_WIN_H + HOVER_GRACE
                        })
                        .unwrap_or(false);
                    match peek_state() {
                        (PeekState::Hidden, _) if in_trigger => {
                            tracing::debug!("peek: cursor on trigger → Revealing");
                            set_peek_state(PeekState::Revealing, Some(std::time::Instant::now()));
                        }
                        (PeekState::Revealing, Some(t)) => {
                            if in_trigger {
                                if t.elapsed() >= std::time::Duration::from_millis(PEEK_REVEAL_MS) {
                                    // Pure visibility toggle — the window kept
                                    // its full size while hidden, so no resize
                                    // (and no ghost frame) on reveal. VERIFIED:
                                    // on failure the handle stays visible and
                                    // Revealing retries after another dwell.
                                    if reveal_pulse(&app) {
                                        // Open the hovered ring's card right away.
                                        let idx = ring_index_at(
                                            my - py,
                                            dock.diameter,
                                            dock.vendors.len(),
                                        );
                                        if let Some(idx) = idx {
                                            card_on_left = show_card_for(&app, &dock, idx);
                                            visible = true;
                                            shown = dock.vendors.get(idx).cloned();
                                        }
                                    } else {
                                        // Reset the dwell so the retry lands in
                                        // PEEK_REVEAL_MS (not every 33ms tick).
                                        set_peek_state(
                                            PeekState::Revealing,
                                            Some(std::time::Instant::now()),
                                        );
                                    }
                                }
                            } else {
                                // Cursor left during reveal — cancel.
                                set_peek_state(PeekState::Hidden, None);
                            }
                        }
                        _ => {}
                    }
                }
                continue;
            }

            // Dragging = the panel is moving under the pointer; suppress.
            let dragging = (px - last_panel_pos.0).abs() + (py - last_panel_pos.1).abs() > 1.5;
            last_panel_pos = (px, py);

            let in_panel = mx >= px - HOVER_GRACE
                && mx < px + w_c + HOVER_GRACE
                && my >= py - HOVER_GRACE
                && my < py + h_c + HOVER_GRACE;
            let ring_idx = if in_panel && !dragging {
                ring_index_at(my - py, dock.diameter, dock.vendors.len())
            } else {
                None
            };
            let on_card = visible && {
                let cpos = card.outer_position().ok();
                let cscale = card.scale_factor().unwrap_or(1.0).max(1.0);
                let card_h = *CARD_H_REPORT.lock().unwrap_or_else(|e| e.into_inner());
                let (kx, ky) = card_keepalive_rect(card_on_left, card_h);
                cpos.map(|p| (p.x as f64 / cscale, p.y as f64 / cscale))
                    .map(|(cx, cy)| {
                        mx >= cx + kx.0 && mx < cx + kx.1 && my >= cy + ky.0 && my < cy + ky.1
                    })
                    .unwrap_or(false)
            };

            if dragging {
                if visible && hiding_since.is_none() {
                    emit_card_hide(&app);
                    hiding_since = Some(std::time::Instant::now());
                    // Cancel any in-progress peek transition.
                    if peek_state().0 != PeekState::Visible {
                        set_peek_state(PeekState::Visible, None);
                    }
                }
            } else if let Some(idx) = ring_idx {
                hiding_since = None;
                last_inside = Some(std::time::Instant::now());
                let vendor = dock.vendors.get(idx).cloned();
                if vendor.is_some() && (vendor != shown || !visible) {
                    card_on_left = show_card_for(&app, &dock, idx);
                    visible = true;
                    shown = vendor;
                }
            } else if on_card {
                hiding_since = None;
                last_inside = Some(std::time::Instant::now());
            } else if visible {
                let lingered = last_inside
                    .map(|t| t.elapsed() >= HOVER_LINGER)
                    .unwrap_or(true);
                match (hiding_since, lingered) {
                    (None, true) => {
                        emit_card_hide(&app);
                        hiding_since = Some(std::time::Instant::now());
                        // Transition to Hiding state (peek entry point).
                        if dock.peek_enabled {
                            set_peek_state(PeekState::Hiding, Some(std::time::Instant::now()));
                        }
                    }
                    (Some(t), _) if t.elapsed() >= HOVER_FADE => {
                        park_card(&card);
                        visible = false;
                        shown = None;
                        hiding_since = None;
                        last_inside = None;
                    }
                    _ => {}
                }
            } else if dock.peek_enabled {
                // Card hidden, panel still up: finish the peek collapse once
                // the hide grace elapses (panel hides, peek handle shows).
                if let (PeekState::Hiding, Some(t)) = peek_state() {
                    if t.elapsed() >= std::time::Duration::from_millis(PEEK_HIDE_MS) {
                        collapse_pulse(&app);
                    }
                }
            }
        }
    });
}

/// Position the pulse panel. Restores the saved (dragged) position when one
/// exists; otherwise docks to the configured screen edge, vertically centered.
/// Re-dock a restored x for the CURRENT width: a size change alters the
/// panel width, so a position that hugged an edge drifts by |Δwidth|
/// (or overflows past it after growth). Within REDOCK_THRESH of an edge →
/// snap flush with the new width; floating positions pass through.
fn redock_x(x: f64, w: f64, mon_left: f64, mon_right: f64) -> f64 {
    const REDOCK_THRESH: f64 = 26.0;
    if (mon_right - (x + w)).abs() <= REDOCK_THRESH {
        return mon_right - w;
    }
    if (x - mon_left).abs() <= REDOCK_THRESH {
        return mon_left;
    }
    x
}

/// True when the panel rect [x, x+w] × [y, y+h] overlaps ANY monitor rect
/// (logical px, (left, top, right, bottom)). A saved position that lands on
/// no display (stale value from a disconnected monitor, or corrupt data
/// like the old peek implementation's off-screen x=5100) must be discarded
/// so the panel re-docks fresh instead of rendering somewhere invisible.
fn rect_on_any_monitor(x: f64, y: f64, w: f64, h: f64, mons: &[(f64, f64, f64, f64)]) -> bool {
    mons.iter()
        .any(|&(ml, mt, mr, mb)| x < mr && x + w > ml && y < mb && y + h > mt)
}

/// Snap the panel flush to the edge configured by `cfg.pulse_side`
/// (vertical position unchanged), persisting the new x. Auto-hide needs
/// this: the peek handle lives ON the configured screen edge, so the panel
/// must also be edge-docked — otherwise the reveal shows the panel away from
/// the cursor and it collapses again in a ~560ms loop. Mirrors token-monitor,
/// whose edgeDock rail is always edge-flush.
fn snap_pulse_to_edge(conn: &Connection, win: &tauri::WebviewWindow) {
    let (Ok(Some(mon)), Ok(pos), Ok(size)) = (
        win.current_monitor(),
        win.outer_position(),
        win.outer_size(),
    ) else {
        return;
    };
    let scale = win.scale_factor().unwrap_or(1.0).max(1.0);
    let px = pos.x as f64 / scale;
    let py = pos.y as f64 / scale;
    let w = size.width as f64 / scale;
    let mon_left = mon.position().x as f64 / scale;
    let mon_right = mon_left + mon.size().width as f64 / scale;
    // Use the configured side, not "nearest edge". (config::load — the
    // pulse_side field lives inside the JSON blob on the `config` row, not
    // on its own key.)
    let side = config::load(conn)
        .map(|cfg| cfg.pulse_side)
        .unwrap_or_else(|_| "right".into());
    let snapped = if side == "left" { mon_left } else { mon_right - w };
    if (snapped - px).abs() > f64::EPSILON {
        let _ = win.set_position(LogicalPosition::new(snapped, py));
        tracing::debug!("pulse: auto-hide snapped panel flush to edge x={snapped}");
    }
    // Persist the flushed x either way so the 1s position poller and the
    // restore path agree (the panel is about to be hidden, which skips
    // persisting).
    save_pos(conn, snapped.round() as i32, py.round() as i32);
}

fn position_pulse(app: &AppHandle, conn: &Connection) {
    let Some(win) = app.get_webview_window("pulse") else {
        return;
    };
    let cfg = config::load(conn).unwrap_or_default();
    let ring_count = load_quotas(conn).len().max(1);
    let (w, h) = collapsed_size(&cfg, ring_count, panel_max_h(&win));

    let _ = win.set_size(LogicalSize::new(w.max(60.0), h.max(60.0)));

    // Monitor rects in logical px, for the saved-position sanity check.
    let mon_rects: Vec<(f64, f64, f64, f64)> = win
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|mon| {
            let sf = win.scale_factor().unwrap_or(1.0).max(1.0);
            let ml = mon.position().x as f64 / sf;
            let mt = mon.position().y as f64 / sf;
            (
                ml,
                mt,
                ml + mon.size().width as f64 / sf,
                mt + mon.size().height as f64 / sf,
            )
        })
        .collect();

    // Try to restore saved position first — re-docked for the new width
    // (an edge-hugging panel must stay fused after a size change). A saved
    // position off every monitor is discarded (fresh dock below).
    if let Some((sx, sy)) = load_pos(conn) {
        if !rect_on_any_monitor(sx, sy, w, h, &mon_rects) {
            tracing::warn!(
                "pulse: saved position ({sx},{sy}) is off every monitor; re-docking fresh"
            );
        } else {
            let mut x = sx;
            if let Ok(Some(mon)) = win.current_monitor() {
                let sf = win.scale_factor().unwrap_or(1.0).max(1.0);
                let mon_left = mon.position().x as f64 / sf;
                let mon_right = mon_left + mon.size().width as f64 / sf;
                x = redock_x(x, w, mon_left, mon_right);
            }
            let _ = win.set_position(LogicalPosition::new(x, sy));
            return;
        }
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
    let (_, h_c) = collapsed_size(&cfg, load_quotas(&conn).len(), panel_max_h(&h));
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
    /// Panel height cap (logical px) = 80% of the current screen — the
    /// frontend CSS caps the ring dock at exactly this number so the
    /// window size and the rendered panel always agree.
    pub max_panel_h: f64,
    /// Panel side: "left" | "right" — anchors the peek handle's silhouette.
    pub side: String,
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
    /// Server timestamp (RFC3339) when the data was fetched.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refreshed_at: Option<String>,
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
    fn saved_pos_off_every_monitor_is_rejected() {
        // (left, top, right, bottom) monitor rects, logical px. A single
        // 1512×982 primary at the origin.
        let mons = [(0.0, 0.0, 1512.0, 982.0)];
        // Panel on the primary (flush-right) → accepted.
        assert!(rect_on_any_monitor(1432.0, 300.0, 80.0, 259.0, &mons));
        // Corrupt saved pos (x=5100 — the old peek implementation's stale
        // off-screen value) → rejected → fresh dock instead.
        assert!(!rect_on_any_monitor(5100.0, 1256.0, 80.0, 259.0, &mons));
        // Multi-monitor: a panel on a secondary display is still accepted.
        let dual = [(0.0, 0.0, 1512.0, 982.0), (1512.0, 0.0, 4072.0, 1117.0)];
        assert!(rect_on_any_monitor(2000.0, 400.0, 80.0, 259.0, &dual));
        // Barely overlapping (1px sliver on the primary) → accepted — the
        // redock snap can pull an edge-hugging panel back flush.
        assert!(rect_on_any_monitor(1511.0, 300.0, 80.0, 259.0, &mons));
        // Fully below the screen → rejected.
        assert!(!rect_on_any_monitor(100.0, 2000.0, 80.0, 259.0, &mons));
    }

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
            let (w, h) = collapsed_size(&cfg, n, MAX_PANEL_H);
            assert_eq!(w, d + PANEL_PAD * 2.0);
            let expect_h = PAD_V + PAD_V_BOT + TITLE_BLOCK + dock_height_capped(d, n, MAX_PANEL_H);
            assert!(
                (h - expect_h).abs() < f64::EPSILON,
                "n={n}: {h} != {expect_h}"
            );
        }
        // Concrete anchor (medium rings, n=3): dock = 3*69 + 2*14 = 235,
        // height = 36 + 44 + 27 (title block) + 235 = 342.
        if d == RING_MEDIUM {
            let (_, h3) = collapsed_size(&cfg, 3, MAX_PANEL_H);
            assert!((h3 - 342.0).abs() < 0.01, "medium n=3: {h3}");
        }
        // Zero rings degrade to the single-ring minimum, never negative.
        let (_, h0) = collapsed_size(&cfg, 0, MAX_PANEL_H);
        let (_, h1) = collapsed_size(&cfg, 1, MAX_PANEL_H);
        assert_eq!(h0, h1);
    }

    #[test]
    fn card_side_opens_into_the_roomier_side() {
        // Flush-right (no room right) → card LEFT.
        assert!(card_on_left_side(1444.0, 68.0, 0.0, 1512.0));
        // Flush-left (no room left) → card RIGHT.
        assert!(!card_on_left_side(0.0, 68.0, 0.0, 1512.0));
        // Floating right-of-center: more room on the left → card LEFT
        // (previously a near-edge panel with <340px of room still opened
        // the card right, straddling off-screen invisibly).
        assert!(card_on_left_side(1100.0, 68.0, 0.0, 1512.0));
        // Floating left-of-center → card RIGHT.
        assert!(!card_on_left_side(300.0, 68.0, 0.0, 1512.0));
        // Dead center: equal rooms → right (ties don't flip).
        assert!(!card_on_left_side(722.0, 68.0, 0.0, 1512.0));
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
    fn card_keepalive_rect_matches_the_visible_card_only() {
        let ((x0, x1), (y0, y1)) = card_keepalive_rect(true, 250.0);
        // Card hugs the window's right edge (cardOnLeft): the left part of
        // the window is inert.
        assert!(x1 > CARD_WIN_W - 20.0, "right-anchored: {x1}");
        assert!(x0 > 0.0 && x0 < x1);
        // Centered vertically per the reported height (480 → ±125, ±8).
        assert!((y0 - 107.0).abs() < f64::EPSILON);
        assert!((y1 - 373.0).abs() < f64::EPSILON);
        // The window mid is inside; the top/bottom margins are not.
        assert!(y0 < CARD_WIN_H / 2.0 && y1 > CARD_WIN_H / 2.0);
        assert!(y0 > 0.0);

        // Mirrored anchoring for the card-on-right side.
        let ((rx0, _), _) = card_keepalive_rect(false, 250.0);
        assert!(rx0 < 20.0, "left-anchored: {rx0}");
    }

    #[test]
    fn ring_index_at_hits_circle_bands_only() {
        let d = 48.0;
        // Above the rail: nothing.
        assert_eq!(ring_index_at(10.0, d, 3), None);
        assert_eq!(ring_index_at(RAIL_TOP - 0.1, d, 3), None);
        // First ring band (RAIL_TOP .. +d, +4 slack).
        assert_eq!(ring_index_at(RAIL_TOP + 5.0, d, 3), Some(0));
        assert_eq!(ring_index_at(RAIL_TOP + d + 4.0, d, 3), Some(0));
        // Label/gap zone between rings: no card flip.
        assert_eq!(ring_index_at(RAIL_TOP + d + 5.0, d, 3), None);
        // Second ring (pitch = 48+21+14 = 83).
        assert_eq!(ring_index_at(RAIL_TOP + 83.0 + 24.0, d, 3), Some(1));
        // Past the dock: nothing.
        assert_eq!(ring_index_at(RAIL_TOP + 3.0 * 83.0 + 10.0, d, 3), None);
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
    fn layout_scales_with_ring_preset() {
        // Typography + vertical blocks scale by d/48 (0.75 / 1 / 1.25) so
        // fonts track the size setting instead of clipping at small.
        let cfg_s = config::Config {
            pulse_size: "small".into(),
            ..Default::default()
        };
        let cfg_l = config::Config {
            pulse_size: "large".into(),
            ..Default::default()
        };
        // Large, 3 rings: s=1.25 → dock = 3*(60+26.25) + 2*17.5 = 293.75,
        // height = 36 + 44 + 33.75 + 293.75 = 407.5.
        let (_, h_l) = collapsed_size(&cfg_l, 3, MAX_PANEL_H);
        assert!((h_l - 407.5).abs() < 0.01, "large n=3: {h_l}");
        // Small, 3 rings: s=0.75 → dock = 3*(36+15.75) + 2*10.5 = 176.25,
        // height = 36 + 44 + 20.25 + 176.25 = 276.5.
        let (_, h_s) = collapsed_size(&cfg_s, 3, MAX_PANEL_H);
        assert!((h_s - 276.5).abs() < 0.01, "small n=3: {h_s}");
        // Ring pitch scales too (hover hit-test + card line stay aligned):
        // large rail = 63*1.25 = 78.75, pitch = 60 + 43.75 = 103.75.
        assert!((ring_center_y(60.0, 0) - (78.75 + 30.0)).abs() < f64::EPSILON);
        assert!((ring_center_y(60.0, 1) - (78.75 + 103.75 + 30.0)).abs() < f64::EPSILON);
    }

    #[test]
    fn redock_x_resnaps_edge_positions_after_size_change() {
        // Saved flush-right (mon_right 1512, old w 80 → x 1432); new w 68
        // leaves a 12px drift → re-snapped flush with the NEW width.
        assert!((redock_x(1432.0, 68.0, 0.0, 1512.0) - (1512.0 - 68.0)).abs() < f64::EPSILON);
        // Growth overflows past the edge (negative gap) → pulled back in.
        assert!((redock_x(1420.0, 92.0, 0.0, 1512.0) - (1512.0 - 92.0)).abs() < f64::EPSILON);
        // Flush-left drift re-snaps to the left edge.
        assert!((redock_x(10.0, 68.0, 0.0, 1512.0) - 0.0).abs() < f64::EPSILON);
        // Floating panels are untouched.
        assert!((redock_x(700.0, 68.0, 0.0, 1512.0) - 700.0).abs() < f64::EPSILON);
    }

    #[test]
    fn panel_height_caps_at_max() {
        let cfg = config::Config::default();
        let (_, h10) = collapsed_size(&cfg, 10, MAX_PANEL_H);
        assert!(
            h10 <= MAX_PANEL_H + f64::EPSILON,
            "10 rings capped: {h10} > {MAX_PANEL_H}"
        );
        let (_, h3) = collapsed_size(&cfg, 3, MAX_PANEL_H);
        assert!(h3 < MAX_PANEL_H, "3 rings stay under the cap");
    }

    #[test]
    fn panel_height_cap_is_eighty_percent_of_screen() {
        let cfg = config::Config::default();
        let screen_h = 982.0; // common macOS logical height
        let max = (screen_h * MAX_PANEL_H_RATIO).floor(); // 785
                                                          // 20 large rings: natural dock = 20*81 + 19*14 = 1906 ≫ avail.
        let (w, h20) = collapsed_size(&cfg, 20, max);
        assert!((h20 - max).abs() < f64::EPSILON, "capped to {max}: {h20}");
        // Width is untouched by the height cap.
        let d = ring_diameter(&cfg.pulse_size);
        assert_eq!(w, d + PANEL_PAD * 2.0);
        // A tiny screen still leaves the 60px minimum dock (never negative).
        let (_, h_tiny) = collapsed_size(&cfg, 20, 100.0);
        assert!((h_tiny - (PAD_V + PAD_V_BOT + TITLE_BLOCK + 60.0)).abs() < f64::EPSILON);
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
