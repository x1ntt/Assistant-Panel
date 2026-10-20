use crate::modules::process_manager;
use crate::state::{AppState, ProcessEntry};
use chrono::Local;
use nanoid::nanoid;
use std::path::Path;
use tauri::{AppHandle, Emitter, Manager};
use windows::Win32::Foundation::HANDLE;

const STORE_KEY: &str = "process-manager.items";

fn ensure_restored(state: &AppState) -> Result<(), String> {
    state.process_restore_status.lock().unwrap().clone()
}

/// Save all process definitions (without runtime state) to Tauri Store
fn save_definitions(app: &AppHandle, state: &AppState) -> Result<(), String> {
    ensure_restored(state)?;
    let processes = state.processes.lock().unwrap();
    let defs: Vec<ProcessEntry> = processes.values().cloned().collect();
    state.logger.plugin("process-manager", "INFO", &format!("PROCESS_SAVE_BEGIN count={}", defs.len()));
    let result = (|| {
        let path = app.path().app_data_dir().map_err(|error| error.to_string())?.join("settings.json");
        read_definitions(&path)?;
        let store = tauri_plugin_store::StoreBuilder::new(app, "settings.json")
            .build().map_err(|error| error.to_string())?;
        let value = serde_json::to_value(&defs).map_err(|error| error.to_string())?;
        store.set(STORE_KEY, value);
        store.save().map_err(|error| error.to_string())
    })();
    match result {
        Ok(()) => {
            state.logger.plugin("process-manager", "INFO", &format!("PROCESS_SAVE_OK count={}", defs.len()));
            Ok(())
        }
        Err(error) => {
            let message = format!("进程配置未保存，运行状态已改变，请重试保存: {error}");
            state.logger.plugin("process-manager", "ERROR", &format!("PROCESS_SAVE_FAILED count={} error={error}", defs.len()));
            Err(message)
        }
    }
}

fn parse_definitions(bytes: &[u8]) -> Result<Vec<ProcessEntry>, String> {
    let settings: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(bytes).map_err(|error| format!("settings.json 无效: {error}"))?;
    let Some(value) = settings.get(STORE_KEY) else {
        return Ok(Vec::new());
    };
    let entries: Vec<ProcessEntry> = serde_json::from_value(value.clone())
        .map_err(|error| format!("进程定义无效: {error}"))?;
    let mut ids = std::collections::HashSet::new();
    for entry in &entries {
        if entry.id.is_empty() || !ids.insert(&entry.id) || entry.command.trim().is_empty() {
            return Err("进程定义无效: ID 为空、重复或命令为空".into());
        }
    }
    Ok(entries)
}

fn read_definitions(path: &Path) -> Result<Vec<ProcessEntry>, String> {
    match std::fs::read(path) {
        Ok(bytes) => parse_definitions(&bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(format!("读取进程定义失败: {error}")),
    }
}

/// Called during app startup to restore saved definitions into state
pub fn restore_definitions(app: &AppHandle, state: &AppState) -> Result<(), String> {
    let started = std::time::Instant::now();
    let result = (|| {
        let path = app.path().app_data_dir().map_err(|error| error.to_string())?.join("settings.json");
        state.logger.plugin("process-manager", "INFO", &format!("PROCESS_RESTORE_BEGIN settings={} windows={}", path.display(), app.webview_windows().len()));
        let defs = read_definitions(&path)?;
        let mut processes = state.processes.lock().unwrap();
        for mut entry in defs {
            state.logger.plugin(
                "process-manager",
                "INFO",
                &format!("恢复定义: id={}, autoStart={}, autoRestart={}", entry.id, entry.auto_start, entry.auto_restart),
            );
            entry.status = "stopped".to_string();
            entry.pid = None;
            entry.job_handle = None;
            entry.started_at = None;
            processes.insert(entry.id.clone(), entry);
        }
        state.logger.plugin("process-manager", "INFO", &format!("PROCESS_RESTORE_OK count={} elapsed_ms={}", processes.len(), started.elapsed().as_millis()));
        Ok(())
    })();
    *state.process_restore_status.lock().unwrap() = result.clone();
    if let Err(error) = &result {
        state.logger.plugin("process-manager", "ERROR", &format!("PROCESS_RESTORE_FAILED error={error}; 已禁止修改配置"));
    }
    result
}

#[tauri::command]
pub fn save_process_definitions(app: AppHandle, state: tauri::State<'_, AppState>) -> Result<(), String> {
    save_definitions(&app, &state)
}

#[tauri::command]
pub fn add_process(
    app: AppHandle,
    command: String,
    working_dir: String,
    auto_start: bool,
    auto_restart: bool,
    state: tauri::State<'_, AppState>,
) -> Result<ProcessEntry, String> {
    ensure_restored(&state)?;
    let (cmd, args) = process_manager::parse_command(&command);
    if cmd.is_empty() {
        return Err("命令不能为空".to_string());
    }

    let id = nanoid!();
    let mut entry = ProcessEntry {
        id,
        command,
        cmd,
        args,
        working_dir,
        auto_start,
        auto_restart,
        pid: None,
        job_handle: None,
        status: "stopped".to_string(),
        started_at: None,
    };

    process_manager::start_process(&mut entry, &app, &state)?;

    state.logger.plugin(
        "process-manager",
        "INFO",
        &format!("进程添加: {} (id: {})", entry.command, entry.id),
    );

    let result = entry.clone();
    state.processes.lock().unwrap().insert(entry.id.clone(), entry);
    save_definitions(&app, &state)?;
    let _ = app.emit("process-manager:status", result.clone());

    Ok(result)
}

#[tauri::command]
pub fn start_process_cmd(
    app: AppHandle,
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let mut processes = state.processes.lock().unwrap();
    let entry = processes.get_mut(&id).ok_or("进程不存在")?;

    if entry.status == "running" {
        return Err("进程已在运行".to_string());
    }

    let (pid, job_handle, _, _) = process_manager::spawn_and_monitor(entry, &app)?;
    entry.pid = Some(pid);
    entry.job_handle = Some(job_handle.0 as isize);
    entry.status = "running".to_string();
    entry.started_at = Some(Local::now().format("%Y-%m-%d %H:%M:%S").to_string());

    state.logger.plugin(
        "process-manager",
        "INFO",
        &format!("进程启动: {} (pid: {}, id: {})", entry.command, pid, id),
    );

    let _ = app.emit("process-manager:status", entry.clone());

    Ok(())
}

#[tauri::command]
pub async fn stop_process(
    app: AppHandle,
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let (job_handle_isize, pid_opt, cmd_display) = {
        let mut processes = state.processes.lock().unwrap();
        let entry = processes.get_mut(&id).ok_or("进程不存在")?;

        if entry.status != "running" {
            return Err("进程未在运行".to_string());
        }

        entry.status = "stopped".to_string();
        let job = entry.job_handle.take();
        let pid = entry.pid.take();
        (job, pid, entry.command.clone())
    };

    // Kill order matters: taskkill /T walks the process TREE starting from the
    // parent, so we must do it while the parent is still alive to reach all
    // descendants. Then close the Job Object as a backstop.
    if let Some(pid) = pid_opt {
        process_manager::kill_process_tree(pid);
    }
    if let Some(jh) = job_handle_isize {
        process_manager::kill_job(HANDLE(jh as *mut _));
    }

    process_manager::save_pid_map(&app, &state);

    state.logger.plugin(
        "process-manager",
        "INFO",
        &format!("进程停止: {} (id: {})", cmd_display, id),
    );

    let snapshot = {
        let processes = state.processes.lock().unwrap();
        processes.get(&id).cloned()
    };
    if let Some(entry) = snapshot {
        let _ = app.emit("process-manager:status", entry);
    }

    Ok(())
}

#[tauri::command]
pub fn remove_process(
    app: AppHandle,
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    ensure_restored(&state)?;
    let entry = {
        let mut processes = state.processes.lock().unwrap();
        processes.remove(&id).ok_or("进程不存在")?
    };

    if entry.status == "running" {
        if let Some(pid) = entry.pid {
            process_manager::kill_process_tree(pid);
        }
        if let Some(jh) = entry.job_handle {
            process_manager::kill_job(HANDLE(jh as *mut _));
        }
    }

    process_manager::save_pid_map(&app, &state);
    save_definitions(&app, &state)?;

    // Drop any buffered output for the removed process.
    state.process_output.lock().unwrap().remove(&id);

    state.logger.plugin(
        "process-manager",
        "INFO",
        &format!("进程删除: {} (id: {})", entry.command, id),
    );

    Ok(())
}

#[tauri::command]
pub fn update_process(
    app: AppHandle,
    id: String,
    command: String,
    working_dir: String,
    auto_start: bool,
    auto_restart: bool,
    state: tauri::State<'_, AppState>,
) -> Result<ProcessEntry, String> {
    ensure_restored(&state)?;
    let (cmd, args) = process_manager::parse_command(&command);
    if cmd.is_empty() {
        return Err("命令不能为空".to_string());
    }

    let mut processes = state.processes.lock().unwrap();
    let entry = processes.get_mut(&id).ok_or("进程不存在")?;

    // Stop if running
    if entry.status == "running" {
        if let Some(pid) = entry.pid.take() {
            process_manager::kill_process_tree(pid);
        }
        if let Some(jh) = entry.job_handle.take() {
            process_manager::kill_job(HANDLE(jh as *mut _));
        }
        entry.status = "stopped".to_string();
    }

    let cmd_display = command.clone();
    entry.command = command;
    entry.cmd = cmd;
    entry.args = args;
    entry.working_dir = working_dir;
    entry.auto_start = auto_start;
    entry.auto_restart = auto_restart;

    // Restart with new config
    let (pid, job_handle, _, _) = process_manager::spawn_and_monitor(entry, &app)?;
    entry.pid = Some(pid);
    entry.job_handle = Some(job_handle.0 as isize);
    entry.status = "running".to_string();
    entry.started_at = Some(Local::now().format("%Y-%m-%d %H:%M:%S").to_string());

    // Clone AFTER the entry reflects the running state, so the UI gets the
    // real pid/status instead of the pre-spawn (stopped/None) snapshot.
    let result = entry.clone();
    let _ = app.emit("process-manager:status", entry.clone());

    drop(processes);
    save_definitions(&app, &state)?;

    state.logger.plugin(
        "process-manager",
        "INFO",
        &format!("进程更新: {} (id: {})", cmd_display, id),
    );

    Ok(result)
}

#[tauri::command]
pub fn list_processes(window: tauri::WebviewWindow, state: tauri::State<'_, AppState>) -> Result<Vec<ProcessEntry>, String> {
    if let Err(error) = ensure_restored(&state) {
        state.logger.plugin("process-manager", "WARN", &format!("PROCESS_LIST_BLOCKED window={} ready=false error={error}", window.label()));
        return Err(error);
    }
    let processes = state.processes.lock().unwrap();
    let entries: Vec<ProcessEntry> = processes.values().cloned().collect();
    state.logger.plugin("process-manager", "INFO", &format!("PROCESS_LIST_OK window={} ready=true count={}", window.label(), entries.len()));
    Ok(entries)
}

#[tauri::command]
pub fn report_process_view(window: tauri::WebviewWindow, visible_count: usize, state: tauri::State<'_, AppState>) {
    let count = state.processes.lock().unwrap().len();
    state.logger.plugin("process-manager", if count == visible_count { "INFO" } else { "WARN" },
        &format!("PROCESS_VIEW_APPLIED window={} visible_count={visible_count} backend_count={count} match={}", window.label(), count == visible_count));
}

/// Return the buffered stdout/stderr output lines for a process.
#[tauri::command]
pub fn get_process_output(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Vec<crate::state::OutputLine> {
    let map = state.process_output.lock().unwrap();
    map.get(&id)
        .map(|dq| dq.iter().cloned().collect())
        .unwrap_or_default()
}

/// Clear the buffered output for a process.
#[tauri::command]
pub fn clear_process_output(id: String, state: tauri::State<'_, AppState>) {
    let mut map = state.process_output.lock().unwrap();
    if let Some(dq) = map.get_mut(&id) {
        dq.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_definitions, read_definitions};

    fn definition() -> serde_json::Value {
        serde_json::json!({"id":"one","command":"example","cmd":"example","args":[],
            "workingDir":"","autoStart":false,"autoRestart":false,"pid":null,
            "status":"stopped","startedAt":null})
    }

    #[test]
    fn reads_existing_definitions_and_only_missing_key_means_empty() {
        let bytes = serde_json::to_vec(&serde_json::json!({"process-manager.items":[definition()]})).unwrap();
        let entries = parse_definitions(&bytes).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "one");
        assert!(parse_definitions(b"{}").unwrap().is_empty());
        for bytes in [b"{".as_slice(), b"[]", br#"{"process-manager.items":null}"#,
            br#"{"process-manager.items":[{}]}"#] {
            assert!(parse_definitions(bytes).is_err());
        }
    }

    #[test]
    fn duplicate_ids_and_blank_commands_fail_without_dropping_entries() {
        let duplicate = serde_json::to_vec(&serde_json::json!({"process-manager.items":[definition(),definition()]})).unwrap();
        assert!(parse_definitions(&duplicate).is_err());
        let mut blank = definition();
        blank["command"] = " ".into();
        let bytes = serde_json::to_vec(&serde_json::json!({"process-manager.items":[blank]})).unwrap();
        assert!(parse_definitions(&bytes).is_err());
    }

    #[test]
    fn read_errors_are_not_treated_as_an_empty_configuration() {
        let directory = std::env::temp_dir().join(format!("process-read-test-{}", nanoid::nanoid!()));
        std::fs::create_dir_all(&directory).unwrap();
        assert!(read_definitions(&directory).is_err());
        assert!(read_definitions(&directory.join(format!("missing-process-settings-{}", nanoid::nanoid!()))).unwrap().is_empty());
        std::fs::remove_dir(directory).unwrap();
    }
}
