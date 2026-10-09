use crate::state::Settings;
use tauri::{AppHandle, Manager};

pub(crate) fn set_all_windows_always_on_top(app: &AppHandle, enabled: bool) -> Result<(), String> {
    for label in ["main", "floating", "close-btn", "alert"] {
        if let Some(window) = app.get_webview_window(label) {
            window.set_always_on_top(enabled).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn get_settings(app: AppHandle) -> Result<Settings, String> {
    let store = tauri_plugin_store::StoreBuilder::new(&app, "settings.json")
        .build()
        .map_err(|e| e.to_string())?;

    let hotkey = store
        .get("panel.hotkey")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| Settings::default().hotkey);

    let width = store
        .get("panel.width")
        .and_then(|v| v.as_u64())
        .unwrap_or(420) as u32;

    let theme = store
        .get("panel.theme")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "system".to_string());

    let always_on_top = store
        .get("panel.alwaysOnTop")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let auto_start = store
        .get("panel.autoStart")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    Ok(Settings {
        hotkey,
        width,
        theme,
        always_on_top,
        auto_start,
    })
}

#[tauri::command]
pub async fn set_setting(app: AppHandle, key: String, value: serde_json::Value) -> Result<(), String> {
    let store = tauri_plugin_store::StoreBuilder::new(&app, "settings.json")
        .build()
        .map_err(|e| e.to_string())?;

    store.set(&key, value);
    store.save().map_err(|e| e.to_string())?;
    Ok(())
}

/// Apply the preference to every Assistant Panel window and persist it only
/// after Windows accepts the change.
#[tauri::command]
pub fn set_app_always_on_top(app: AppHandle, enabled: bool) -> Result<(), String> {
    set_all_windows_always_on_top(&app, enabled)?;

    let store = tauri_plugin_store::StoreBuilder::new(&app, "settings.json")
        .build()
        .map_err(|e| e.to_string())?;
    store.set("panel.alwaysOnTop", serde_json::Value::Bool(enabled));
    store.save().map_err(|e| e.to_string())?;
    Ok(())
}
