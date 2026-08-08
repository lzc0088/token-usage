// Entry point. M6: Tauri-native system tray + auto-hide on blur.
// `unexpected_cfgs` suppressed crate-wide: Tauri/macos-private-api macros emit
// `cfg(mobile)` and `cfg(cargo-clippy)` which are not declared in Cargo.toml.
#![allow(unexpected_cfgs)]

pub mod auth;
pub mod collector;
pub mod commands;
pub mod config;
pub mod query;
pub mod quota;
pub mod state;
pub mod storage;
pub mod ui;
pub mod utils;

include!(concat!(env!("OUT_DIR"), "/generated.rs"));

use state::AppState;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Mutex;
use tauri::menu::ContextMenu;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, PhysicalPosition, PhysicalSize};
use utils::time::now_ms;

/// Timestamp (ms since epoch) of the last menu event. Used to suppress the
/// `Focused(false)` blur event that macOS fires when a popup menu closes —
/// without this guard the popover would hide right after a menu selection.
static MENU_CLOSE_MS: AtomicI64 = AtomicI64::new(0);

/// Last-known tray icon rect (physical pixels). Updated on every tray event
/// that carries a rect, so `show_main_under_tray` can position the popover
/// flush against the menu bar bottom, centred on the tray icon.
static LAST_TRAY_RECT: Mutex<Option<(PhysicalPosition<f64>, PhysicalSize<f64>)>> = Mutex::new(None);

fn mark_menu_close() {
    MENU_CLOSE_MS.store(now_ms(), Ordering::SeqCst);
}

fn menu_just_closed() -> bool {
    let last = MENU_CLOSE_MS.load(Ordering::SeqCst);
    if last == 0 {
        return false;
    }
    let now = now_ms();
    (now - last).abs() < 500
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

/// Position the main popover under the menu bar tray icon, then show + focus.
///
/// Custom positioning is used instead of `Position::TrayBottomCenter` because
/// the plugin sets `y = tray_y` (top of the tray icon), but we need the popover
/// to appear NEXT to the tray:
///   macOS: menu bar at screen top → window below the tray icon
///   Windows/Linux: taskbar at screen bottom → window above the tray icon
/// Horizontal centring on the tray icon is unchanged.
pub(crate) fn show_main_under_tray(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let state = app.state::<AppState>();
        let conn = state.db_read();
        let is_fixed = config::load(&conn)
            .map(|c| c.window_display_mode == "fixed")
            .unwrap_or(false);
        crate::ui::window::apply_drag_mode(app, is_fixed);

        // Position the window next to the tray icon.
        if let Ok(guard) = LAST_TRAY_RECT.lock() {
            if let Some((tray_pos, tray_size)) = *guard {
                if let Ok(win_size) = w.outer_size() {
                    let x = tray_pos.x + (tray_size.width / 2.0) - (win_size.width as f64 / 2.0);
                    // macOS: menu bar at top → window below the tray.
                    // Windows/Linux: taskbar at bottom → window above the tray.
                    #[cfg(target_os = "macos")]
                    let y = tray_pos.y + tray_size.height;
                    #[cfg(not(target_os = "macos"))]
                    let y = tray_pos.y - win_size.height as f64;
                    let _ = w.set_position(PhysicalPosition::new(x as i32, y as i32));
                }
            }
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

    // Load current config for checkmark state AND language.
    let (tray_sel, win_sel, theme_sel, is_en) = app
        .state::<AppState>()
        .load_config()
        .map(|c| {
            (
                c.tray_display,
                c.window_display_mode,
                c.theme,
                c.language == "en",
            )
        })
        .unwrap_or_else(|_| ("icon_only".into(), "normal".into(), "system".into(), false));

    // Bilingual labels: (zh, en). All call-sites pass &'static str literals.
    let label = |zh: &'static str, en: &'static str| -> &'static str {
        if is_en {
            en
        } else {
            zh
        }
    };

    // Helper: menu item with id.
    let menu_item = |id: &str, text: &str| MenuItem::with_id(app, id, text, true, None::<&str>);

    // ── 系统菜单 submenu ──
    let tray = |id: &str, sel: &str, label| {
        let val = id.strip_prefix("tray_").unwrap_or(id);
        let check = if val == sel { "✓ " } else { "    " };
        menu_item(id, &format!("{check}{label}"))
    };
    let td = &tray_sel;
    let tray_sub = Submenu::with_items(
        app,
        label("系统菜单", "Tray Display"),
        true,
        &[
            &tray(
                "tray_today_tokens",
                td,
                label("今日 Tokens", "Today Tokens"),
            )?,
            &tray("tray_today_cost", td, label("今日成本", "Today Cost"))?,
            &tray(
                "tray_today_both",
                td,
                label("今日 Tokens + 成本", "Today Tokens + Cost"),
            )?,
            &tray(
                "tray_total_tokens",
                td,
                label("累计 Tokens", "Total Tokens"),
            )?,
            &tray("tray_total_cost", td, label("累计成本", "Total Cost"))?,
            &tray(
                "tray_total_both",
                td,
                label("累计 Tokens + 成本", "Total Tokens + Cost"),
            )?,
            &tray("tray_icon_only", td, label("仅显示图标", "Icon Only"))?,
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
        label("窗口呈现", "Window Mode"),
        true,
        &[
            &win("window_normal", wd, label("普通窗口", "Normal"))?,
            &win("window_fixed", wd, label("固定位置", "Fixed"))?,
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
        label("切换主题", "Theme"),
        true,
        &[
            &th("theme_dark", ts, label("深色", "Dark"))?,
            &th("theme_light", ts, label("浅色", "Light"))?,
            &th("theme_system", ts, label("跟随系统", "System"))?,
        ],
    )?;

    let sep = || PredefinedMenuItem::separator(app);
    Menu::with_items(
        app,
        &[
            &menu_item("refresh", label("立即刷新", "Refresh"))?,
            &sep()?,
            &tray_sub,
            &win_sub,
            &theme_sub,
            &sep()?,
            &menu_item("settings", label("设置", "Settings"))?,
            &sep()?,
            &MenuItem::with_id(
                app,
                "version",
                format!("v{APP_VERSION}"),
                false,
                None::<&str>,
            )?,
            &menu_item("quit", label("退出 Token Usage", "Quit Token Usage"))?,
        ],
    )
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = match AppState::open_default() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("无法打开数据库: {e}");
            std::process::exit(1);
        }
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // A second launch → focus the existing instance's windows instead of
            // spawning a duplicate (which would leave a ghost tray icon on Windows).
            tracing::info!("second instance launched — focusing existing windows");
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
            if let Some(s) = app.get_webview_window("settings") {
                let _ = s.show();
                let _ = s.set_focus();
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(state)
        .setup(|app| {
            // Initialize structured logging inside setup (after tao init).
            #[cfg(debug_assertions)]
            let filter = "token_usage=debug";
            #[cfg(not(debug_assertions))]
            let filter = "token_usage=warn";
            let _ = tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter)),
                )
                .try_init();

            // Mirror the OS system proxy into HTTPS_PROXY/HTTP_PROXY so every
            // HTTP client (reqwest updater, ureq check-update/rate/quota) routes
            // through it. Must run before any network request.
            utils::proxy::sync_system_proxy();

            // ── Dock visibility (must be set BEFORE any window is created) ──
            // macOS: NSApplicationActivationPolicy must be chosen during
            // didFinishLaunching, before windows/tray exist, or tao panics.
            {
                let show = app
                    .state::<AppState>()
                    .load_config()
                    .map(|c| c.show_in_dock)
                    .unwrap_or(false);
                ui::window::apply_dock_visibility(app.handle(), show);
            }

            // Initialize positioner plugin for tray-relative window positioning
            #[cfg(desktop)]
            {
                if let Err(e) = app.handle().plugin(tauri_plugin_positioner::init()) {
                    tracing::warn!("Failed to initialize positioner plugin: {e}");
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

                    // Also store the rect for our custom positioning (TrayBottomCenter
                    // sets y=tray_y, but we need y=tray_y+tray_height for flush menu-bar).
                    if let TrayIconEvent::Click { rect, .. }
                    | TrayIconEvent::Enter { rect, .. }
                    | TrayIconEvent::Leave { rect, .. }
                    | TrayIconEvent::Move { rect, .. }
                    | TrayIconEvent::DoubleClick { rect, .. } = &event
                    {
                        let pos = rect.position.to_physical(1.0);
                        let size = rect.size.to_physical(1.0);
                        if let Ok(mut guard) = LAST_TRAY_RECT.lock() {
                            *guard = Some((pos, size));
                        }
                    }

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
                            ui::window::cancel_hover_hide(); // re-hover cancels pending hide
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
                            ui::window::schedule_hover_hide(app.clone());
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
                let conn = db.lock().unwrap_or_else(|e| {
                    tracing::warn!("db mutex poisoned in setup, recovering: {e}");
                    e.into_inner()
                });
                ui::window::apply_window_features(app.handle(), &conn);
                // Sync floating widget visibility + handle position with config.
                ui::floating::sync_floating(app.handle(), &conn);
            }

            // ── persist the floating handle's dragged position ──────────────
            // A low-rate poll captures the resting position after a drag without
            // the bookkeeping of per-move event debouncing. No-op on macOS /
            // when the widget is hidden.
            let persist_h = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    ui::floating::persist_handle_pos(&persist_h);
                }
            });

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
            commands::settings::get_credential_field_values,
            commands::settings::clear_credential_fields,
            commands::copilot::copilot_login,
            commands::copilot::poll_for_token,
            commands::quota::codex_login,
            commands::window_cmd::open_settings,
            commands::window_cmd::consume_settings_target,
            commands::window_cmd::close_settings,
            commands::window_cmd::open_external,
            commands::window_cmd::set_window_draggable,
            commands::window_cmd::set_drag_suspended,
            commands::window_cmd::frontend_log,
            commands::window_cmd::show_main_window,
            commands::window_cmd::get_floating_data,
            commands::window_cmd::show_floating_panel,
            commands::window_cmd::hide_floating_panel,
            commands::exchange::get_exchange_rate,
            commands::exchange::refresh_exchange_rate,
            commands::exchange::get_latest_rate,
            commands::exchange::set_manual_rate,
            commands::autostart::set_auto_start,
            commands::autostart::get_auto_start,
            commands::update::check_update,
            commands::update::get_app_version,
            commands::update::install_update,
            commands::update::restart_app,
            commands::platform::get_platform,
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
                    // Defer window ops + event emit, matching the "settings"
                    // handler: w.show() can trigger webview JS that calls back
                    // into IPC (main thread) while on_menu_event is still on
                    // the stack → deadlock. Deferring also lets a hidden
                    // webview resume before `tray:refresh` is delivered, so the
                    // frontend listener actually receives it (an emit right
                    // after show() can land while the webview is still
                    // suspended and get dropped).
                    let app_c = app.clone();
                    let state_c = app.state::<crate::state::AppState>().db.clone();
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                        show_main_under_tray(&app_c);
                        let _ = app_c.emit("tray:refresh", ());
                        // Also push fresh data to the floating panel.
                        if let Ok(conn) = state_c.lock() {
                            crate::ui::floating::push_data(&app_c, &conn);
                        }
                    });
                }
                "settings" => {
                    // Defer ALL window operations to avoid deadlock: w.show() can
                    // trigger webview JS init → mount effect calls api.getConfig()
                    // (IPC → main thread). If the main thread is still in
                    // on_menu_event, the IPC waits and the webview waits → freeze.
                    // A short async delay ensures the handler has fully returned.
                    // Tray-open always lands on the default "general" page.
                    commands::window_cmd::set_settings_target(&app.state::<AppState>(), "general");
                    let app_c = app.clone();
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                        commands::window_cmd::show_settings_window(&app_c);
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
                let guard = db.lock().unwrap_or_else(|e| {
                    tracing::warn!("db mutex poisoned in menu handler, recovering: {e}");
                    e.into_inner()
                });
                let c = &*guard;
                let needs_tray_refresh = match id {
                    "tray_today_tokens" | "tray_today_cost" | "tray_today_both"
                    | "tray_total_tokens" | "tray_total_cost" | "tray_total_both"
                    | "tray_icon_only" => id
                        .strip_prefix("tray_")
                        .map(|mode| {
                            let _ = config::with_config(c, |cfg| {
                                cfg.tray_display = mode.to_string();
                            });
                        })
                        .is_some(),
                    "window_normal" | "window_fixed" => {
                        let _ = id.strip_prefix("window_").map(|mode| {
                            let _ = config::with_config(c, |cfg| {
                                cfg.window_display_mode = mode.to_string();
                            });
                        });
                        false
                    }
                    "theme_dark" | "theme_light" | "theme_system" => {
                        let _ = id.strip_prefix("theme_").map(|theme| {
                            let _ = config::with_config(c, |cfg| {
                                cfg.theme = theme.to_string();
                            });
                        });
                        false
                    }
                    _ => false,
                };
                if needs_tray_refresh {
                    crate::ui::tray::refresh_from_db(app, c);
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
