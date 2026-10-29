use crate::modules::network_monitor::{LoadedSettings, NetworkTrafficState, Snapshot, PINNED_KEY, REMOVED_KEY};
use std::collections::BTreeSet;
use std::path::Path;
use tauri::{AppHandle, Manager};
use serde_json::{Map, Value};

// StoreBuilder ignores initial JSON load failures. Validate disk explicitly
// before migration or saving; a missing file is normal on first launch.
fn read_settings(path: &Path) -> Result<Map<String, Value>, String> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| e.to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Map::new()),
        Err(error) => Err(error.to_string()),
    }
}

fn parse_pinned(settings: &Map<String, Value>) -> Result<Vec<String>, String> {
    let order: Vec<String> = match settings.get(PINNED_KEY) {
        Some(value) => serde_json::from_value(value.clone())
            .map_err(|e| format!("Invalid {PINNED_KEY}: {e}"))?,
        None => Vec::new(),
    };
    let mut seen = BTreeSet::new();
    Ok(order.into_iter().filter(|id| seen.insert(id.clone())).collect())
}

pub fn load_settings(app: &AppHandle) -> Result<LoadedSettings, String> {
    let path = app.path().app_data_dir().map_err(|e| e.to_string())?.join("settings.json");
    load_from_path(&path, || save_settings(app, None))
}

fn load_from_path(path: &Path, cleanup: impl FnOnce() -> Result<(), String>) -> Result<LoadedSettings, String> {
    let settings = read_settings(&path)?;
    let pinned_order = parse_pinned(&settings)?;
    // The legacy value is intentionally never parsed or used for filtering.
    let migration_warning = if settings.contains_key(REMOVED_KEY) {
        cleanup().err().map(|error| format!("Legacy network settings cleanup failed: {error}"))
    } else { None };
    Ok(LoadedSettings { pinned_order, migration_warning })
}

// Restore only the affected cache keys on failure, preserving other settings.
// This is runtime/cache rollback, NOT an atomic disk transaction.
trait SettingsStore {
    fn get(&self, key: &str) -> Option<Value>;
    fn set(&self, key: &str, value: Value);
    fn delete(&self, key: &str);
    fn save(&self) -> Result<(), String>;
}

impl SettingsStore for tauri_plugin_store::Store<tauri::Wry> {
    fn get(&self, key: &str) -> Option<Value> { self.get(key) }
    fn set(&self, key: &str, value: Value) { self.set(key, value); }
    fn delete(&self, key: &str) { self.delete(key); }
    fn save(&self) -> Result<(), String> { self.save().map_err(|e| e.to_string()) }
}

fn persist(store: &impl SettingsStore, order: Option<&[String]>) -> Result<(), String> {
    let previous_pinned = store.get(PINNED_KEY);
    let previous_removed = store.get(REMOVED_KEY);
    if let Some(order) = order { store.set(PINNED_KEY, serde_json::to_value(order).map_err(|e| e.to_string())?); }
    store.delete(REMOVED_KEY);
    if let Err(error) = store.save() {
        for (key, previous) in [(PINNED_KEY, previous_pinned), (REMOVED_KEY, previous_removed)] {
            match previous { Some(value) => store.set(key, value), None => store.delete(key) }
        }
        return Err(format!("Failed to save network traffic settings: {error}"));
    }
    Ok(())
}

fn save_settings(app: &AppHandle, order: Option<&[String]>) -> Result<(), String> {
    let path = app.path().app_data_dir().map_err(|e| e.to_string())?.join("settings.json");
    save_if_valid(&path, || {
        let store = tauri_plugin_store::StoreBuilder::new(app, "settings.json").disable_auto_save().build().map_err(|e| e.to_string())?;
        persist(store.as_ref(), order)
    })
}

fn save_if_valid(path: &Path, save: impl FnOnce() -> Result<(), String>) -> Result<(), String> {
    let settings = read_settings(path)?;
    parse_pinned(&settings)?;
    save()
}

/// A cached snapshot never starts another sampler or performs native queries.
#[tauri::command]
pub async fn get_network_traffic_snapshot(app: AppHandle) -> Result<Snapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.try_state::<NetworkTrafficState>().ok_or_else(|| {
            crate::startup_diagnostics::record("NetworkTrafficState not ready; IPC rejected without panic");
            "Network traffic backend is not ready; retry shortly".to_string()
        })?;
        let monitor = state.inner.lock().map_err(|e| e.to_string())?;
        Ok(monitor.snapshot())
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn pin_network_traffic_interface(app: AppHandle, id: String, pinned: Option<bool>) -> Result<Snapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.try_state::<NetworkTrafficState>().ok_or_else(|| {
            crate::startup_diagnostics::record("NetworkTrafficState not ready; IPC rejected without panic");
            "Network traffic backend is not ready; retry shortly".to_string()
        })?;
        let mut monitor = state.inner.lock().map_err(|e| e.to_string())?;
        if pinned.unwrap_or(true) {
            monitor.pin(&id, |order| save_settings(&app, Some(order)))?;
        } else {
            monitor.unpin(&id, |order| save_settings(&app, Some(order)))?;
        }
        Ok(monitor.publish(&app))
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn get_network_traffic_double_click_interval() -> u32 {
    unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetDoubleClickTime() }
}

#[tauri::command]
pub async fn set_network_traffic_highlight(app: AppHandle, id: String, highlighted: bool) -> Result<Snapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.try_state::<NetworkTrafficState>().ok_or_else(|| {
            crate::startup_diagnostics::record("NetworkTrafficState not ready; IPC rejected without panic");
            "Network traffic backend is not ready; retry shortly".to_string()
        })?;
        let mut monitor = state.inner.lock().map_err(|e| e.to_string())?;
        monitor.highlight(&id, highlighted)?;
        Ok(monitor.publish(&app))
    }).await.map_err(|e| e.to_string())?
}

/// Runtime only: SettingsView owns persistence of plugins.disabled.
#[tauri::command]
pub async fn set_network_traffic_enabled(app: AppHandle, enabled: bool) -> Result<Snapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.try_state::<NetworkTrafficState>().ok_or_else(|| {
            crate::startup_diagnostics::record("NetworkTrafficState not ready; IPC rejected without panic");
            "Network traffic backend is not ready; retry shortly".to_string()
        })?;
        let mut monitor = state.inner.lock().map_err(|e| e.to_string())?;
        monitor.set_enabled(enabled)?;
        Ok(monitor.publish(&app))
    }).await.map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    struct FakeStore {
        cache: RefCell<Map<String, Value>>,
        disk: RefCell<Map<String, Value>>,
        fail: Cell<bool>,
    }
    impl FakeStore {
        fn new(settings: Map<String, Value>) -> Self {
            Self { disk: RefCell::new(settings.clone()), cache: RefCell::new(settings), fail: Cell::new(false) }
        }
    }
    impl SettingsStore for FakeStore {
        fn get(&self, key: &str) -> Option<Value> { self.cache.borrow().get(key).cloned() }
        fn set(&self, key: &str, value: Value) { self.cache.borrow_mut().insert(key.into(), value); }
        fn delete(&self, key: &str) { self.cache.borrow_mut().remove(key); }
        fn save(&self) -> Result<(), String> {
            if self.fail.get() { return Err("disk denied".into()); }
            *self.disk.borrow_mut() = self.cache.borrow().clone(); Ok(())
        }
    }
    struct TempSettings(std::path::PathBuf);
    impl TempSettings {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("network-settings-{}", nanoid::nanoid!()));
            std::fs::create_dir_all(&path).unwrap();
            Self(path.join("settings.json"))
        }
        fn write(&self, bytes: &[u8]) { std::fs::write(&self.0, bytes).unwrap(); }
    }
    impl Drop for TempSettings {
        fn drop(&mut self) { let _ = std::fs::remove_dir_all(self.0.parent().unwrap()); }
    }

    #[test]
    fn network_pinned_load_deduplicates_in_order_preserves_absent_ids() {
        let settings = serde_json::from_value(serde_json::json!({
            PINNED_KEY: ["b", "missing", "a", "b", "missing"], "panel.width": 420
        })).unwrap();
        assert_eq!(parse_pinned(&settings).unwrap(), ["b", "missing", "a"]);
        let store = FakeStore::new(settings);
        let order = parse_pinned(&store.disk.borrow()).unwrap();
        persist(&store, Some(&order)).unwrap();
        assert_eq!(parse_pinned(&store.disk.borrow()).unwrap(), ["b", "missing", "a"]);
        assert_eq!(store.get("panel.width"), Some(420.into()));
    }

    #[test]
    fn network_legacy_legal_and_bad_values_migrate_only_old_key() {
        for legacy in [serde_json::json!(["a", "b"]), serde_json::json!(false),
                       serde_json::json!(null), serde_json::json!({"invalid": true})] {
            let file = TempSettings::new();
            let settings = serde_json::json!({ REMOVED_KEY: legacy, PINNED_KEY: ["b", "missing"],
                "panel.width": 420, "plugins.disabled": ["other"] });
            file.write(&serde_json::to_vec(&settings).unwrap());
            let store = FakeStore::new(read_settings(&file.0).unwrap());
            let loaded = load_from_path(&file.0, || persist(&store, None)).unwrap();
            assert_eq!(loaded.pinned_order, ["b", "missing"]);
            assert!(loaded.migration_warning.is_none());
            let disk = store.disk.borrow();
            assert!(!disk.contains_key(REMOVED_KEY));
            assert_eq!(disk.get(PINNED_KEY), settings.get(PINNED_KEY));
            assert_eq!(disk.get("panel.width"), settings.get("panel.width"));
            assert_eq!(disk.get("plugins.disabled"), settings.get("plugins.disabled"));
        }
    }

    #[test]
    fn network_migration_failure_restores_cache_then_pin_cleans_legacy() {
        let file = TempSettings::new();
        file.write(&serde_json::to_vec(&serde_json::json!({ REMOVED_KEY: false, "panel.width": 420 })).unwrap());
        let store = FakeStore::new(read_settings(&file.0).unwrap());
        let original = store.cache.borrow().clone();
        store.fail.set(true);
        let loaded = load_from_path(&file.0, || persist(&store, None)).unwrap();
        assert!(loaded.pinned_order.is_empty());
        assert!(loaded.migration_warning.is_some());
        assert_eq!(*store.cache.borrow(), original);
        assert!(persist(&store, Some(&["a".into()])).is_err());
        assert_eq!(*store.cache.borrow(), original);
        store.fail.set(false);
        persist(&store, Some(&["a".into()])).unwrap();
        assert!(store.get(REMOVED_KEY).is_none());
        assert_eq!(parse_pinned(&store.disk.borrow()).unwrap(), ["a"]);
        assert_eq!(store.get("panel.width"), Some(420.into()));
    }

    #[test]
    fn network_failed_save_restores_existing_pin_and_legacy_cache() {
        let original = serde_json::from_value(serde_json::json!({
            PINNED_KEY: ["b", "missing"], REMOVED_KEY: ["a"], "panel.width": 420
        })).unwrap();
        let store = FakeStore::new(original);
        let before = store.cache.borrow().clone();
        store.fail.set(true);
        assert!(persist(&store, Some(&["a".into(), "b".into()])).is_err());
        assert_eq!(*store.cache.borrow(), before);
        assert_eq!(*store.disk.borrow(), before);
    }

    #[test]
    fn network_missing_file_and_key_are_normal_but_bad_config_never_saves() {
        let file = TempSettings::new();
        assert!(load_from_path(&file.0, || panic!("no legacy cleanup")).unwrap().pinned_order.is_empty());
        file.write(br#"{"panel.width":420}"#);
        assert!(load_from_path(&file.0, || panic!("no legacy cleanup")).unwrap().pinned_order.is_empty());
        for bytes in [b"{broken".as_slice(), b"[]", b"null",
            br#"{"network-traffic.pinnedInterfaces":false,"network-traffic.removedInterfaces":[]}"#,
            br#"{"network-traffic.pinnedInterfaces":["a",42]}"#] {
            file.write(bytes);
            assert!(load_from_path(&file.0, || panic!("invalid settings must not save")).is_err());
            assert!(save_if_valid(&file.0, || panic!("invalid disk must reject pin save")).is_err());
            assert_eq!(std::fs::read(&file.0).unwrap(), bytes);
        }
        // An unreadable path is also fail-closed, not an empty settings object.
        std::fs::remove_file(&file.0).unwrap();
        std::fs::create_dir(&file.0).unwrap();
        assert!(load_from_path(&file.0, || panic!("unreadable settings must not save")).is_err());
    }
}
