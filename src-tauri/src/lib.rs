mod commands;
mod logger;
mod modules;
mod state;
mod startup_diagnostics;

use std::sync::Mutex;
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// Current panel shortcut — read by the global handler, updated by `update_hotkey`
static PANEL_SHORTCUT: Mutex<Option<Shortcut>> = Mutex::new(None);
/// Current pin shortcut — read by the global handler, updated by `update_hotkey`
static PIN_SHORTCUT: Mutex<Option<Shortcut>> = Mutex::new(None);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    startup_diagnostics::init();
    startup_diagnostics::record("run entered; Tauri builder/build begin");
    let application = tauri::Builder::default()
        // Register before configured webviews can issue their first Todo IPC.
        .manage(commands::todo::TodoState::default())
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
            startup_diagnostics::record("setup entered; ordinary logger begin");
            let log = logger::Logger::new(app.handle());
            log.system("INFO", "Application starting");
            startup_diagnostics::record("ordinary logger initialized; settings read begin");

            // Read saved panel width (logical px, falls back to 420).
            let panel_width_logical: f64 = tauri_plugin_store::StoreBuilder::new(app.handle(), "settings.json")
                // Explicit saves allow network pin/migration cache rollback without
                // a delayed autosave committing an unconfirmed mutation.
                .disable_auto_save()
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

            startup_diagnostics::record("panel settings read completed; plugin settings read begin");
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

            startup_diagnostics::record("plugin settings read completed; shortcut setup begin");
            let plugin_enabled = |id: &str| !disabled_plugins.iter().any(|d| d == id);

            // window-pin disabled -> force pin off and skip its hotkey registration.
            if !plugin_enabled("window-pin") {
                pin_enabled = false;
            }

            startup_diagnostics::record("shared state registration begin");
            app.manage(state::AppState::new(log.clone(), pin_enabled));
            let network_settings = commands::network_traffic::load_settings(app.handle());
            app.manage(modules::network_monitor::NetworkTrafficState::from_settings(
                plugin_enabled("network-traffic"), network_settings,
            ));
            startup_diagnostics::record("shared states registered before window show and shortcut setup");

            startup_diagnostics::record("process initialization begin; configured windows not created");
            if plugin_enabled("process-manager") {
                let app_state = app.state::<state::AppState>();
                if commands::process::restore_definitions(app.handle(), app_state.inner()).is_ok() {
                    modules::process_manager::startup_auto_processes(app.handle(), app_state.inner());
                    let count = app_state.processes.lock().unwrap().len();
                    log.plugin("process-manager", "INFO", &format!("PROCESS_INIT_READY count={count} windows={}", app.webview_windows().len()));
                }
            } else {
                *app.state::<state::AppState>().process_restore_status.lock().unwrap() = Err("进程管理插件未在启动时启用，请启用后重启应用".into());
                log.plugin("process-manager", "INFO", "PROCESS_INIT_SKIPPED plugin_enabled=false");
            }

            startup_diagnostics::record("configured window creation begin; shared states ready");
            log.plugin("process-manager", "INFO", "PROCESS_WINDOWS_BEGIN");
            for window_config in &app.config().app.windows {
                tauri::WebviewWindowBuilder::from_config(app.handle(), window_config)?.build()?;
            }
            startup_diagnostics::record("configured window creation completed");
            log.plugin("process-manager", "INFO", &format!("PROCESS_WINDOWS_READY count={}", app.webview_windows().len()));

            startup_diagnostics::record("settings read completed; window layout begin");
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

            startup_diagnostics::record("window layout completed; shortcut setup begin");
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
            startup_diagnostics::record("global-shortcut plugin registration begin");
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
                ).map_err(|error| {
                    startup_diagnostics::record(&format!("global-shortcut plugin registration failed: {error}"));
                    error
                })?;

            startup_diagnostics::record("global-shortcut plugin ready; main shortcut registration begin");
            app.global_shortcut().register(panel_shortcut).map_err(|error| {
                startup_diagnostics::record(&format!("main shortcut registration failed: {error}"));
                error
            })?;
            startup_diagnostics::record("main shortcut registration completed");
            // Only register the window-pin hotkey when that plugin is enabled.
            if plugin_enabled("window-pin") {
                if let Err(e) = app.global_shortcut().register(pin_shortcut) {
                    log.system("WARN", &format!("Pin hotkey register failed: {}", e));
                }
            } else {
                log.system("INFO", "Plugin window-pin disabled; skipping pin hotkey");
            }

            startup_diagnostics::record("shortcut setup completed; background monitors begin");
            log.system("INFO", "Application ready");
            modules::network_monitor::start_monitor(app.handle().clone());

            startup_diagnostics::record("network monitor started; gold monitor begin");
            // Start background gold-price monitor (skip if API dashboard disabled).
            if plugin_enabled("api-dashboard") {
                modules::gold_monitor::start_monitor(app.handle().clone());
            }

            startup_diagnostics::record("gold monitor startup completed; setup completed");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::todo::get_todo_document,
            commands::todo::save_todo_document,
            commands::network_traffic::get_network_traffic_snapshot,
            commands::network_traffic::pin_network_traffic_interface,
            commands::network_traffic::set_network_traffic_highlight,
            commands::network_traffic::set_network_traffic_enabled,
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
            commands::port_forward::get_saved_forward_rules,
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
            commands::process::save_process_definitions,
            commands::process::report_process_view,
            commands::process::get_process_output,
            commands::process::clear_process_output,
        ])
        .build(tauri::generate_context!());
    let application = match application {
        Ok(application) => application,
        Err(error) => {
            startup_diagnostics::record(&format!("Tauri build failed: {error}"));
            panic!("error while building tauri application");
        }
    };
    startup_diagnostics::record("Tauri build completed; event loop begin");
    application.run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                startup_diagnostics::record("event loop exit");
                if let Some(state) = app.try_state::<modules::network_monitor::NetworkTrafficState>() {
                    state.shutdown();
                }
            }
        });
}

#[cfg(test)]
mod startup_tests {
    #[test]
    fn webviews_are_deferred_until_shared_states_are_registered() {
        let config: tauri::Config = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert!(!config.app.windows.is_empty());
        for window in config.app.windows {
            assert!(!window.create, "Window {} must be created by setup after state registration", window.label);
        }
    }
}
