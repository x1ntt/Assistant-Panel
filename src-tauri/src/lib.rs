mod commands;
mod logger;
mod modules;
mod state;

use std::sync::Mutex;
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// Current panel shortcut — read by the global handler, updated by `update_hotkey`
static PANEL_SHORTCUT: Mutex<Option<Shortcut>> = Mutex::new(None);
/// Current pin shortcut — read by the global handler, updated by `update_hotkey`
static PIN_SHORTCUT: Mutex<Option<Shortcut>> = Mutex::new(None);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_single_instance::init(|_app, _args, _cwd| {}))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_positioner::init())
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                }
            }
        })
        .setup(|app| {
            let log = logger::Logger::new(app.handle());
            log.system("INFO", "Application starting");

            // Read saved panel width (logical px, falls back to 420).
            let panel_width_logical: f64 = tauri_plugin_store::StoreBuilder::new(app.handle(), "settings.json")
                .build()
                .ok()
                .and_then(|s| s.get("panel.width").and_then(|v| v.as_u64()))
                .map(|w| w as f64)
                .unwrap_or(420.0)
                .clamp(200.0, 2000.0);
            let always_on_top = tauri_plugin_store::StoreBuilder::new(app.handle(), "settings.json")
                .build()
                .ok()
                .and_then(|s| s.get("panel.alwaysOnTop").and_then(|v| v.as_bool()))
                .unwrap_or(true);

            // Dock the panel to the outermost right display.  Using the primary
            // display here would put a hidden panel into the adjacent display
            // after it slides out, causing Windows to apply that display's DPI
            // when it is shown again.
            let main_win = app.get_webview_window("main").unwrap();
            if let Some(monitor) = commands::panel::rightmost_monitor(&app.handle()) {
                let work = monitor.work_area();
                // Convert logical width to physical px for this monitor's DPI.
                let panel_width = (panel_width_logical * monitor.scale_factor()).round() as u32;
                // Dock inside the work area so the taskbar never covers the panel.
                let x = work.position.x + (work.size.width as i32) - (panel_width as i32);
                let y = work.position.y;
                main_win.set_position(tauri::PhysicalPosition::new(x, y)).ok();
                main_win.set_size(tauri::PhysicalSize::new(panel_width, work.size.height)).ok();
            }
            if let Err(err) = commands::settings::set_all_windows_always_on_top(&app.handle(), always_on_top) {
                log.system("WARN", &format!("Apply always-on-top setting failed: {}", err));
            }
            main_win.show().ok();
            main_win.set_focus().ok();

            if let Some(floating_win) = app.get_webview_window("floating") {
                floating_win.hide().ok();
            }
            if let Some(btn_win) = app.get_webview_window("close-btn") {
                btn_win.hide().ok();
            }
            if let Some(alert_win) = app.get_webview_window("alert") {
                alert_win.hide().ok();
            }

            // Load settings from store (with fallback defaults)
            let store_result = tauri_plugin_store::StoreBuilder::new(app.handle(), "settings.json").build();

            let mut panel_hotkey_str = "Control+Shift+Backquote".to_string();
            let mut pin_hotkey_str = "Control+Alt+Backquote".to_string();
            let mut pin_enabled = true;
            // Per-host disabled plugin ids (settings key `plugins.disabled`).
            let mut disabled_plugins: Vec<String> = Vec::new();

            if let Ok(store) = store_result {
                if let Some(v) = store.get("panel.hotkey").and_then(|v| v.as_str().map(|s| s.to_string())) {
                    panel_hotkey_str = v;
                }
                if let Some(v) = store.get("windowPin.hotkey").and_then(|v| v.as_str().map(|s| s.to_string())) {
                    pin_hotkey_str = v;
                }
                if let Some(v) = store.get("windowPin.enabled").and_then(|v| v.as_bool()) {
                    pin_enabled = v;
                }
                if let Some(v) = store.get("plugins.disabled") {
                    if let Ok(list) = serde_json::from_value::<Vec<String>>(v) {
                        disabled_plugins = list;
                    }
                }
            }

            let plugin_enabled = |id: &str| !disabled_plugins.iter().any(|d| d == id);

            // window-pin disabled -> force pin off and skip its hotkey registration.
            if !plugin_enabled("window-pin") {
                pin_enabled = false;
            }

            // Persist loaded PIN_ENABLED into global AtomicBool
            commands::window_pin::PIN_ENABLED.store(pin_enabled, std::sync::atomic::Ordering::Relaxed);

            let default_panel = "Control+Shift+Backquote";
            let default_pin = "Control+Alt+Backquote";
            let panel_shortcut = match commands::window_pin::parse_hotkey(&panel_hotkey_str) {
                Ok(s) => s,
                Err(e) => {
                    log.system("WARN", &format!("Panel hotkey parse failed ({}), using default: {}", e, default_panel));
                    panel_hotkey_str = default_panel.to_string();
                    commands::window_pin::parse_hotkey(default_panel)
                        .expect("default panel hotkey is valid")
                }
            };
            let pin_shortcut = match commands::window_pin::parse_hotkey(&pin_hotkey_str) {
                Ok(s) => s,
                Err(e) => {
                    log.system("WARN", &format!("Pin hotkey parse failed ({}), using default: {}", e, default_pin));
                    pin_hotkey_str = default_pin.to_string();
                    commands::window_pin::parse_hotkey(default_pin)
                        .expect("default pin hotkey is valid")
                }
            };

            // Store shortcuts in globals for handler access
            *PANEL_SHORTCUT.lock().unwrap() = Some(panel_shortcut);
            *PIN_SHORTCUT.lock().unwrap() = Some(pin_shortcut);

            log.system("INFO", &format!("Panel hotkey: {}", panel_hotkey_str));
            log.system("INFO", &format!("Pin hotkey: {} (enabled: {})", pin_hotkey_str, pin_enabled));

            // Register global shortcuts — handler reads current shortcuts from globals
            app.handle()
                .plugin(
                    tauri_plugin_global_shortcut::Builder::new()
                        .with_handler(move |app, shortcut, event| {
                            if event.state() != ShortcutState::Pressed {
                                return;
                            }
                            let panel = PANEL_SHORTCUT.lock().unwrap();
                            let pin = PIN_SHORTCUT.lock().unwrap();

                            if panel.as_ref() == Some(shortcut) {
                                if let Some(win) = app.get_webview_window("main") {
                                    win.emit("hotkey:toggle", ()).ok();
                                }
                            } else if pin.as_ref() == Some(shortcut) {
                                if commands::window_pin::PIN_ENABLED.load(std::sync::atomic::Ordering::Relaxed) {
                                    let _ = commands::window_pin::toggle_pin_focused_inner(app);
                                }
                            }
                        })
                        .build(),
                )?;

            app.global_shortcut().register(panel_shortcut)?;
            // Only register the window-pin hotkey when that plugin is enabled.
            if plugin_enabled("window-pin") {
                if let Err(e) = app.global_shortcut().register(pin_shortcut) {
                    log.system("WARN", &format!("Pin hotkey register failed: {}", e));
                }
            } else {
                log.system("INFO", "Plugin window-pin disabled; skipping pin hotkey");
            }

            log.system("INFO", "Application ready");
            let app_state = state::AppState::new(log, pin_enabled);
            // Register state before restoring processes. Startup monitor threads
            // use `app.try_state()` immediately after a process is spawned.
            app.manage(app_state);

            // Restore process definitions and auto-start (skip if plugin disabled).
            if plugin_enabled("process-manager") {
                let app_handle = app.handle().clone();
                let app_state = app.state::<state::AppState>();
                commands::process::restore_definitions(&app_handle, app_state.inner());
                modules::process_manager::startup_auto_processes(&app_handle, app_state.inner());
            }

            // Start background gold-price monitor (skip if API dashboard disabled).
            if plugin_enabled("api-dashboard") {
                modules::gold_monitor::start_monitor(app.handle().clone());
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::panel::toggle_panel,
            commands::panel::show_panel,
            commands::panel::hide_panel,
            commands::panel::slide_out_main,
            commands::panel::slide_in_main,
            commands::panel::show_floating,
            commands::panel::hide_floating,
            commands::panel::broadcast_theme,
            commands::panel::write_theme,
            commands::panel::read_theme,
            commands::panel::focus_main,
            commands::panel::exit_app,
            commands::panel::restart_app,
            commands::panel::apply_panel_width,
            commands::alert::show_alert,
            commands::alert::hide_alert,
            commands::alert::reposition_alert,
            commands::alert::resize_alert,
            modules::gold_monitor::get_gold_snapshot,
            commands::settings::get_settings,
            commands::settings::set_setting,
            commands::settings::set_app_always_on_top,
            commands::log::get_logs,
            commands::log::list_log_sources,
            commands::port_forward::get_interfaces,
            commands::port_forward::add_forward_rule,
            commands::port_forward::remove_forward_rule,
            commands::port_forward::disconnect_rule_connection,
            commands::port_forward::get_forward_rules,
            commands::port_forward::pause_forward,
            commands::port_forward::resume_forward,
            commands::window_pin::toggle_pin_focused,
            commands::window_pin::unpin_window,
            commands::window_pin::unpin_all,
            commands::window_pin::list_pinned,
            commands::window_pin::update_hotkey,
            commands::window_pin::set_pin_enabled,
            commands::process::add_process,
            commands::process::start_process_cmd,
            commands::process::stop_process,
            commands::process::remove_process,
            commands::process::update_process,
            commands::process::list_processes,
            commands::process::get_process_output,
            commands::process::clear_process_output,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
