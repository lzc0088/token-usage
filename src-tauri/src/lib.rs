// Entry point. M6: Tauri-native system tray + auto-hide on blur.
#![allow(unexpected_cfgs)]

pub mod collector;
pub mod commands;
pub mod config;
pub mod credentials;
pub mod install_probe;
pub mod paths;
pub mod query;
pub mod quota;
pub mod state;
pub mod storage;
pub mod tray;
pub mod window_ctl;

use state::AppState;
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::menu::ContextMenu;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager};

/// Timestamp (ms since epoch) of the last menu event. Used to suppress the
/// `Focused(false)` blur event that macOS fires when a popup menu closes —
/// without this guard the popover would hide right after a menu selection.
static MENU_CLOSE_MS: AtomicU64 = AtomicU64::new(0);

fn mark_menu_close() {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    MENU_CLOSE_MS.store(ms, Ordering::SeqCst);
}

fn menu_just_closed() -> bool {
    let last = MENU_CLOSE_MS.load(Ordering::SeqCst);
    if last == 0 {
        return false;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    now.saturating_sub(last) < 500
}

/// Build a 44×44 RGBA template icon: rounded-rect border enclosing a bold
/// centred "T". Supersampled 4×, then box-filtered to 44×44.
fn build_tray_icon() -> tauri::image::Image<'static> {
    const SF: usize = 4; // supersampling factor
    const HW: usize = 44 * SF; // 176×176 hires canvas
    let mut hi = vec![0u8; HW * HW]; // alpha only

    // ── helper: is (fx, fy) inside a rounded rectangle? ────────────────
    fn in_rr(mut fx: f64, mut fy: f64, l: f64, t: f64, r: f64, b: f64, rad: f64) -> bool {
        // Mirror into the top-left quadrant relative to centre
        let cx = (l + r) * 0.5;
        let cy = (t + b) * 0.5;
        fx = (fx - cx).abs();
        fy = (fy - cy).abs();
        let hw = (r - l) * 0.5;
        let hh = (b - t) * 0.5;
        if fx > hw || fy > hh {
            return false;
        }
        // Corner: (fx, fy) is relative to centre in quadrant I
        let dx = (fx - (hw - rad)).max(0.0);
        let dy = (fy - (hh - rad)).max(0.0);
        dx * dx + dy * dy <= rad * rad
    }

    let sf = SF as f64;

    // Outer rounded rect: border starts at margin 1.5px, extends 4px thick
    let ol = 1.5 * sf;
    let ot = 1.5 * sf;
    let or_ = (44.0 - 1.5) * sf;
    let ob = (44.0 - 1.5) * sf;
    let rad_o = 11.0 * sf; // outer corner radius

    // Inner rounded rect (hole): border is 4px thick
    let il = (1.5 + 4.0) * sf;
    let it_ = (1.5 + 4.0) * sf;
    let ir = (44.0 - 1.5 - 4.0) * sf;
    let ib = (44.0 - 1.5 - 4.0) * sf;
    let rad_i = f64::max(11.0 - 4.0, 0.0) * sf; // inner radius

    // Draw border
    for y in 0..HW {
        for x in 0..HW {
            let fx = x as f64 + 0.5;
            let fy = y as f64 + 0.5;
            if in_rr(fx, fy, ol, ot, or_, ob, rad_o) && !in_rr(fx, fy, il, it_, ir, ib, rad_i) {
                hi[y * HW + x] = 255;
            }
        }
    }

    // Bold "T" centred inside the inner area (more padding, smaller letter)
    let pad = 6.0 * sf; // more padding → smaller overall
    let tl = (il + pad) as usize;
    let tr = (ir - pad) as usize;
    let tt = (it_ + pad) as usize;
    let tb = (ib - pad) as usize;
    let tcx = (tl + tr) / 2;
    let bar_h = ((tb - tt) as f64 * 0.28) as usize; // thicker crossbar
    let stem_w = ((tr - tl) as f64 * 0.24) as usize; // thicker stem

    // Crossbar (top)
    for y in tt..tt + bar_h {
        for x in tl..tr {
            hi[y * HW + x] = 255;
        }
    }
    // Stem (centre, from crossbar to bottom)
    for y in tt..tb {
        for x in tcx - stem_w / 2..tcx + stem_w / 2 {
            hi[y * HW + x] = 255;
        }
    }

    // ── down-sample 4× → 44×44 RGBA ────────────────────────────────────
    let ow = 44usize;
    let oh = 44usize;
    let mut rgba = vec![0u8; ow * oh * 4];
    let block = SF * SF;
    for oy in 0..oh {
        for ox in 0..ow {
            let mut sum = 0u32;
            for dy in 0..SF {
                for dx in 0..SF {
                    sum += hi[(oy * SF + dy) * HW + (ox * SF + dx)] as u32;
                }
            }
            let a = (sum / block as u32) as u8;
            let i = (oy * ow + ox) * 4;
            rgba[i] = 0;
            rgba[i + 1] = 0;
            rgba[i + 2] = 0;
            rgba[i + 3] = a;
        }
    }
    tauri::image::Image::new_owned(rgba, ow as u32, oh as u32)
}

/// Position the main popover just below the menu bar / centered under the
/// tray icon, then show + focus it. No-op if the window is missing.
/// Also re-applies drag mode from the current config so changes made from
/// the tray menu or settings window take effect immediately.
pub(crate) fn show_main_under_tray(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        // Re-apply behavior that depends on live config (may have changed
        // via tray menu or settings window while the popover was hidden).
        let state = app.state::<AppState>();
        if let Ok(conn) = state.db.lock() {
            if let Ok(cfg) = config::load(&conn) {
                crate::window_ctl::apply_drag_mode(app, cfg.window_display_mode == "fixed");
            }
        }

        #[cfg(desktop)]
        {
            use tauri_plugin_positioner::{Position, WindowExt};
            let _ = w.as_ref().window().move_window(Position::TrayBottomCenter);
        }
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Current app version (from Cargo.toml / tauri.conf.json).
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Build the tray right-click context menu dynamically. Reads the current
/// config to show checkmarks on the active tray_display / window_display_mode.
fn build_tray_menu(app: &tauri::AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};

    // Load current config for checkmark state.
    let (tray_sel, win_sel, theme_sel) = app
        .state::<AppState>()
        .load_config()
        .map(|c| (c.tray_display, c.window_display_mode, c.theme))
        .unwrap_or_else(|_| ("icon_only".into(), "normal".into(), "system".into()));

    // Helper: menu item with id.
    let menu_item = |id: &str, text: &str| MenuItem::with_id(app, id, text, true, None::<&str>);

    // ── 系统菜单 submenu ──
    // Strip prefix ("tray_") before comparing with config value.
    let tray = |id: &str, sel: &str, label| {
        let val = id.strip_prefix("tray_").unwrap_or(id);
        let check = if val == sel { "✓ " } else { "    " };
        menu_item(id, &format!("{check}{label}"))
    };
    let td = &tray_sel;
    let tray_sub = Submenu::with_items(
        app,
        "系统菜单",
        true,
        &[
            &tray("tray_today_tokens", td, "今日 Tokens")?,
            &tray("tray_today_cost", td, "今日成本")?,
            &tray("tray_today_both", td, "今日 Tokens + 成本")?,
            &tray("tray_total_tokens", td, "累计 Tokens")?,
            &tray("tray_total_cost", td, "累计成本")?,
            &tray("tray_total_both", td, "累计 Tokens + 成本")?,
            &tray("tray_icon_only", td, "仅显示图标")?,
        ],
    )?;

    // ── 窗口呈现 submenu ──
    let win = |id: &str, sel: &str, label| {
        let val = id.strip_prefix("window_").unwrap_or(id);
        let check = if val == sel { "✓ " } else { "    " };
        menu_item(id, &format!("{check}{label}"))
    };
    let wd = &win_sel;
    let win_sub = Submenu::with_items(
        app,
        "窗口呈现",
        true,
        &[
            &win("window_normal", wd, "普通窗口")?,
            &win("window_fixed", wd, "固定位置")?,
        ],
    )?;

    // ── 切换主题 submenu ──
    let th = |id: &str, sel: &str, label| {
        let val = id.strip_prefix("theme_").unwrap_or(id);
        let check = if val == sel { "✓ " } else { "    " };
        menu_item(id, &format!("{check}{label}"))
    };
    let ts = &theme_sel;
    let theme_sub = Submenu::with_items(
        app,
        "切换主题",
        true,
        &[
            &th("theme_dark", ts, "深色")?,
            &th("theme_light", ts, "浅色")?,
            &th("theme_system", ts, "跟随系统")?,
        ],
    )?;

    let sep = || PredefinedMenuItem::separator(app);
    Menu::with_items(app, &[
        &menu_item("refresh", "立即刷新")?,
        &sep()?,
        &tray_sub,
        &win_sub,
        &theme_sub,
        &sep()?,
        &menu_item("settings", "设置")?,
        &sep()?,
        &MenuItem::with_id(app, "version", format!("v{APP_VERSION}"), false, None::<&str>)?,
        &menu_item("quit", "退出 Token Usage")?,
    ])
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = AppState::open_default().expect("failed to open token-usage DB");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(state)
        .setup(|app| {
            // ── Dock visibility (must be set BEFORE any window is created) ──
            // macOS: NSApplicationActivationPolicy must be chosen during
            // didFinishLaunching, before windows/tray exist, or tao panics.
            {
                let show = app
                    .state::<AppState>()
                    .load_config()
                    .map(|c| c.show_in_dock)
                    .unwrap_or(false);
                window_ctl::apply_dock_visibility(app.handle(), show);
            }

            // Initialize positioner plugin for tray-relative window positioning
            #[cfg(desktop)]
            {
                if let Err(e) = app.handle().plugin(tauri_plugin_positioner::init()) {
                    eprintln!("Failed to initialize positioner plugin: {e}");
                }
            }

            // ── reconcile launch-on-boot with stored config ────────────
            // Wipes a missing LaunchAgent (or a fresh install where the DB says
            // auto_start=true) so the OS registration matches user intent.
            commands::autostart::sync_auto_start_on_boot(app.handle());

            let window = app.get_webview_window("main").expect("main window");

            // ── system tray (Tauri-native, integrated with the event loop) ──
            // Build the tray icon in code — a rounded-rect border with a bold
            // "T" letter, drawn as an RGBA Image. No external files needed.
            let tray_icon = build_tray_icon();
            TrayIconBuilder::with_id("main")
                .icon(tray_icon)
                .icon_as_template(true) // macOS: auto-adapt to light/dark menu bar
                .tooltip("Token Usage")
                .on_tray_icon_event(|tray, event| {
                    // Use positioner plugin to handle tray-relative positioning
                    #[cfg(desktop)]
                    tauri_plugin_positioner::on_tray_event(tray.app_handle(), &event);

                    let app = tray.app_handle();

                    // `hover` trigger mode: show the popover when the cursor
                    // enters the tray icon; auto-hide shortly after it leaves.
                    if let TrayIconEvent::Enter { .. } = &event {
                        let hover = app
                            .state::<AppState>()
                            .load_config()
                            .map(|c| c.trigger_mode == "hover")
                            .unwrap_or(false);
                        if hover {
                            window_ctl::cancel_hover_hide(); // re-hover cancels pending hide
                            if let Some(w) = app.get_webview_window("main") {
                                if !w.is_visible().unwrap_or(false) {
                                    show_main_under_tray(app);
                                }
                            }
                        }
                    }
                    if let TrayIconEvent::Leave { .. } = &event {
                        let hover = app
                            .state::<AppState>()
                            .load_config()
                            .map(|c| c.trigger_mode == "hover")
                            .unwrap_or(false);
                        if hover {
                            window_ctl::schedule_hover_hide(app.clone());
                        }
                    }

                    // Left-click toggles popover visibility (always available,
                    // regardless of trigger_mode, so the user can dismiss).
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        // If settings is open → close it and surface main (mutual
                        // exclusion). Otherwise toggle main as usual.
                        let settings_open = app
                            .get_webview_window("settings")
                            .map(|s| s.is_visible().unwrap_or(false))
                            .unwrap_or(false);
                        if settings_open {
                            if let Some(s) = app.get_webview_window("settings") {
                                let _ = s.hide();
                            }
                            show_main_under_tray(app);
                        } else if let Some(w) = app.get_webview_window("main") {
                            if w.is_visible().unwrap_or(false) {
                                let _ = w.hide();
                            } else {
                                show_main_under_tray(app);
                            }
                        }
                    }

                    // Right-click → close main + settings windows, then show context menu.
                    if let TrayIconEvent::Click {
                        button: MouseButton::Right,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.hide();
                        }
                        if let Some(w) = app.get_webview_window("settings") {
                            let _ = w.hide();
                        }
                        if let Ok(menu) = build_tray_menu(app) {
                            if let Some(w) = app.get_webview_window("main") {
                                let _ = menu.popup(w.as_ref().window().clone());
                            }
                        }
                    }
                })
                .build(app)?;

            // ── auto-hide popover on blur ──────────────────────────────────
            // Always hide when the popover loses focus — standard menu bar behaviour.
            // Using a simple clone-based listener (no Mutex lock in the event callback)
            // ensures Focused(false) triggers reliably on macOS.
            {
                let w = window.clone();
                window.on_window_event(move |ev| {
                    if let tauri::WindowEvent::Focused(false) = ev {
                        // macOS fires Focused(false) when a popup menu closes.
                        // Suppress the hide for a short window to prevent the
                        // popover from disappearing right after a menu selection.
                        if !menu_just_closed() {
                            let _ = w.hide();
                        }
                    }
                });
            }

            // ── make settings window drag-movable by background ──────────
            #[cfg(target_os = "macos")]
            if let Some(ns_win) = app
                .get_webview_window("settings")
                .and_then(|w| w.ns_window().ok())
            {
                use objc::{msg_send, sel, sel_impl};
                let win: *mut objc::runtime::Object = ns_win as *mut _;
                // NSWindow.isMovableByWindowBackground = YES
                let _: () = unsafe { msg_send![win, setMovableByWindowBackground: 1i8] };
            }

            // ── collector (watcher + scheduler + consumer) ──────────────────
            let h = app.handle().clone();
            let db = app.state::<AppState>().db.clone();
            tauri::async_runtime::spawn(collector::runtime::start(h, db.clone()));

            // ── quota refresh scheduler ─────────────────────────────────────
            tauri::async_runtime::spawn(quota::scheduler::run(app.handle().clone(), db.clone()));

            // ── exchange rate: auto-fetch once on launch (mode=auto, daily) ──
            commands::exchange::startup_auto_fetch(app.handle().clone());

            // ── apply window-behaviour config (drag / hotkey / tray) ────────
            // Dock policy was applied early above; the rest is safe to set now.
            {
                let conn = db.lock().expect("db poisoned");
                window_ctl::apply_window_features(app.handle(), &conn);
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::query::get_summary,
            commands::query::get_breakdown,
            commands::query::get_detail_breakdown,
            commands::query::get_trends,
            commands::query::get_sessions,
            commands::query::get_session_detail,
            commands::query::get_session_rounds,
            commands::query::get_projects, // accepts period arg
            commands::status::get_tools_status,
            commands::status::get_tokscale_status,
            commands::collection::get_archived_session_count,
            commands::collection::clear_archived_sessions,
            commands::quota::get_quotas,
            commands::quota::refresh_quotas,
            commands::quota::refresh_quotas_if_stale,
            commands::quota::refresh_quota,
            commands::quota::test_credential,
            commands::settings::get_config,
            commands::settings::set_config,
            commands::settings::get_credential_status,
            commands::settings::set_credential,
            commands::settings::delete_credential,
            commands::settings::update_cookie,
            commands::settings::get_credential_fields,
            commands::settings::clear_credential_fields,
            commands::window_cmd::open_settings,
            commands::window_cmd::close_settings,
            commands::exchange::get_exchange_rate,
            commands::exchange::refresh_exchange_rate,
            commands::exchange::get_latest_rate,
            commands::exchange::set_manual_rate,
            commands::autostart::set_auto_start,
            commands::autostart::get_auto_start,
            commands::update::check_update,
        ])
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();

            // Mark menu as closed FIRST so the blur handler won't interfere
            // with any window operations below (set_focus → Focused(false →
            // blur handler would try to hide the window mid-handler).
            mark_menu_close();

            // ── Actions that don't touch config ──
            match id {
                "refresh" => {
                    show_main_under_tray(app);
                    let _ = app.emit("tray:refresh", ());
                }
                "settings" => {
                    // Defer ALL window operations to avoid deadlock: w.show() can
                    // trigger webview JS init → mount effect calls api.getConfig()
                    // (IPC → main thread). If the main thread is still in
                    // on_menu_event, the IPC waits and the webview waits → freeze.
                    // A short delay ensures the handler has fully returned.
                    let app_c = app.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(80));
                        if let Some(main) = app_c.get_webview_window("main") {
                            let _ = main.hide();
                        }
                        if let Some(w) = app_c.get_webview_window("settings") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    });
                }
                "quit" => {
                    app.exit(0);
                }
                _ => {}
            }

            // ── Config-changing actions: save only ──
            // 系统菜单: real-time tray repaint.
            // 窗口呈现 / 切换主题: just save; frontend reloads on next focus.
            {
                let db = app.state::<AppState>().db.clone();
                let guard = db.lock();
                if let Ok(ref c) = guard {
                    if let Ok(mut cfg) = config::load(c) {
                        match id {
                            "tray_today_tokens" | "tray_today_cost" | "tray_today_both"
                            | "tray_total_tokens" | "tray_total_cost" | "tray_total_both"
                            | "tray_icon_only" => {
                                if let Some(mode) = id.strip_prefix("tray_") {
                                    if cfg.tray_display != mode {
                                        cfg.tray_display = mode.to_string();
                                        let _ = config::save(c, &cfg);
                                        crate::tray::refresh_from_db(app, c);
                                    }
                                }
                            }
                            "window_normal" | "window_fixed" => {
                                if let Some(mode) = id.strip_prefix("window_") {
                                    if cfg.window_display_mode != mode {
                                        cfg.window_display_mode = mode.to_string();
                                        let _ = config::save(c, &cfg);
                                    }
                                }
                            }
                            "theme_dark" | "theme_light" | "theme_system" => {
                                if let Some(theme) = id.strip_prefix("theme_") {
                                    if cfg.theme != theme {
                                        cfg.theme = theme.to_string();
                                        let _ = config::save(c, &cfg);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
