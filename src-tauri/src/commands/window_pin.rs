use crate::modules::window_pinner;
use crate::state::{AppState, PinnedWindow};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

const SKIP_TITLES: &[&str] = &["Assistant Panel", "Floating Panel", "Close Button"];

/// Global flag: is window pin feature enabled?
pub static PIN_ENABLED: AtomicBool = AtomicBool::new(true);

/// 切换当前焦点窗口的置顶状态
#[tauri::command]
pub fn toggle_pin_focused(app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<Option<PinnedWindow>, String> {
    let result = window_pinner::toggle_pin_focused(SKIP_TITLES)?;

    match &result {
        Some(pinned) => {
            let mut pinned_windows = state.pinned_windows.lock().unwrap();
            pinned_windows.insert(pinned.hwnd, pinned.clone());
            state.logger.plugin("window-pin", "INFO",
                &format!("置顶: {} (hwnd: {})", pinned.title, pinned.hwnd));
        }
        None => {
            let mut pinned_windows = state.pinned_windows.lock().unwrap();
            let unpinned: Vec<isize> = pinned_windows.iter()
                .filter(|(_, w)| !window_pinner::is_window_topmost(
                    windows::Win32::Foundation::HWND(w.hwnd as *mut _)))
                .map(|(k, _)| *k)
                .collect();
            for hwnd in unpinned {
                if let Some(w) = pinned_windows.remove(&hwnd) {
                    state.logger.plugin("window-pin", "INFO",
                        &format!("取消置顶: {} (hwnd: {})", w.title, w.hwnd));
                }
            }
        }
    }

    // Re-assert main window topmost so it stays above pinned windows
    if let Some(main_win) = app.get_webview_window("main") {
        let _ = main_win.set_always_on_top(true);
    }

    if let Some(win) = app.get_webview_window("main") {
        win.emit("window-pin:changed", ()).ok();
    }

    Ok(result)
}

/// 内部版本：供热键 handler 调用（无 State 访问，仅切换+emit）
pub fn toggle_pin_focused_inner(app: &tauri::AppHandle) -> Result<Option<PinnedWindow>, String> {
    if !PIN_ENABLED.load(Ordering::Relaxed) {
        return Err("窗口置顶功能已禁用".to_string());
    }
    let result = window_pinner::toggle_pin_focused(SKIP_TITLES)?;

    // Re-assert main window topmost so it stays above pinned windows
    if let Some(main_win) = app.get_webview_window("main") {
        let _ = main_win.set_always_on_top(true);
    }

    if let Some(win) = app.get_webview_window("main") {
        win.emit("window-pin:changed", ()).ok();
    }
    Ok(result)
}

/// 启用/禁用窗口置顶功能
#[tauri::command]
pub fn set_pin_enabled(app: tauri::AppHandle, enabled: bool) {
    PIN_ENABLED.store(enabled, Ordering::Relaxed);
    // Persist to store
    if let Ok(store) = tauri_plugin_store::StoreBuilder::new(&app, "settings.json").build() {
        store.set("windowPin.enabled", serde_json::Value::Bool(enabled));
        let _ = store.save();
    }
}

/// 取消指定窗口的置顶
#[tauri::command]
pub fn unpin_window(app: tauri::AppHandle, hwnd: isize, state: tauri::State<'_, AppState>) -> Result<(), String> {
    window_pinner::unpin_window(hwnd)?;
    let mut pinned_windows = state.pinned_windows.lock().unwrap();
    if let Some(w) = pinned_windows.remove(&hwnd) {
        state.logger.plugin("window-pin", "INFO",
            &format!("取消置顶: {} (hwnd: {})", w.title, w.hwnd));
    }
    if let Some(win) = app.get_webview_window("main") {
        win.emit("window-pin:changed", ()).ok();
    }
    Ok(())
}

/// 取消所有置顶
#[tauri::command]
pub fn unpin_all(app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let mut pinned_windows = state.pinned_windows.lock().unwrap();
    let hwnds: Vec<isize> = pinned_windows.keys().copied().collect();
    window_pinner::unpin_all(&hwnds);
    let count = pinned_windows.len();
    pinned_windows.clear();
    state.logger.plugin("window-pin", "INFO",
        &format!("全部取消置顶 ({} 个窗口)", count));
    if let Some(win) = app.get_webview_window("main") {
        win.emit("window-pin:changed", ()).ok();
    }
    Ok(())
}

/// 获取已置顶窗口列表
#[tauri::command]
pub fn list_pinned(state: tauri::State<'_, AppState>) -> Vec<PinnedWindow> {
    let pinned_windows = state.pinned_windows.lock().unwrap();
    pinned_windows.values().cloned().collect()
}

/// 更新快捷键 — 校验并立即重新注册全局快捷键（无需重启）
#[tauri::command]
pub fn update_hotkey(
    app: tauri::AppHandle,
    hotkey_type: String,
    new_hotkey: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    state.logger.plugin("settings", "INFO",
        &format!("快捷键更新: {} = {}", hotkey_type, new_hotkey));

    let store_key = match hotkey_type.as_str() {
        "panel" => "panel.hotkey",
        "windowPin" => "windowPin.hotkey",
        _ => return Err(format!("未知快捷键类型: {}", hotkey_type)),
    };

    let new_shortcut = parse_hotkey(&new_hotkey)?;

    // Reject collisions between panel and pin bindings.
    {
        let panel = crate::PANEL_SHORTCUT.lock().unwrap();
        let pin = crate::PIN_SHORTCUT.lock().unwrap();
        let (self_slot, other_slot) = if hotkey_type == "panel" {
            (panel.as_ref(), pin.as_ref())
        } else {
            (pin.as_ref(), panel.as_ref())
        };
        if other_slot == Some(&new_shortcut) {
            return Err("该快捷键已被另一个功能占用".to_string());
        }
        // Already the active binding — persist and done.
        if self_slot == Some(&new_shortcut) {
            persist_hotkey(&app, store_key, &new_hotkey)?;
            state.logger.plugin("settings", "INFO",
                &format!("快捷键 {} 已是当前绑定: {}", hotkey_type, new_hotkey));
            return Ok(());
        }
    }

    let old_shortcut = {
        let guard = if hotkey_type == "panel" {
            crate::PANEL_SHORTCUT.lock().unwrap()
        } else {
            crate::PIN_SHORTCUT.lock().unwrap()
        };
        *guard
    };

    if let Some(old) = old_shortcut {
        app.global_shortcut().unregister(old).ok();
    }

    if let Err(e) = app.global_shortcut().register(new_shortcut) {
        // Restore previous binding so the feature keeps working.
        if let Some(old) = old_shortcut {
            if let Err(re) = app.global_shortcut().register(old) {
                state.logger.plugin("settings", "ERROR",
                    &format!("快捷键回滚注册失败: {}", re));
            }
        }
        state.logger.plugin("settings", "WARN",
            &format!("{} 快捷键注册失败: {}", hotkey_type, e));
        return Err(format!("快捷键注册失败: {}", e));
    }

    {
        let mut guard = if hotkey_type == "panel" {
            crate::PANEL_SHORTCUT.lock().unwrap()
        } else {
            crate::PIN_SHORTCUT.lock().unwrap()
        };
        *guard = Some(new_shortcut);
    }

    persist_hotkey(&app, store_key, &new_hotkey)?;

    state.logger.plugin("settings", "INFO",
        &format!("快捷键 {} 已立即生效: {}", hotkey_type, new_hotkey));

    Ok(())
}

fn persist_hotkey(app: &tauri::AppHandle, store_key: &str, value: &str) -> Result<(), String> {
    let store = tauri_plugin_store::StoreBuilder::new(app, "settings.json")
        .build()
        .map_err(|e| e.to_string())?;
    store.set(store_key, serde_json::Value::String(value.to_string()));
    store.save().map_err(|e| e.to_string())
}

/// Parse a shortcut string using global-hotkey's full key table.
pub fn parse_hotkey(s: &str) -> Result<Shortcut, String> {
    s.trim().parse::<Shortcut>().map_err(|e| e.to_string())
}
