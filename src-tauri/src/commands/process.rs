use crate::modules::process_manager;
use crate::state::{AppState, ProcessEntry};
use chrono::Local;
use nanoid::nanoid;
use tauri::{AppHandle, Emitter, Manager};
use windows::Win32::Foundation::HANDLE;

const STORE_KEY: &str = "process-manager.items";

/// Save all process definitions (without runtime state) to Tauri Store
fn save_definitions(app: &AppHandle, state: &AppState) {
    let processes = state.processes.lock().unwrap();
    let defs: Vec<ProcessEntry> = processes.values().cloned().collect();
    if let Ok(store) = tauri_plugin_store::StoreBuilder::new(app, "settings.json").build() {
        store.set(STORE_KEY, serde_json::to_value(&defs).unwrap_or_default());
        let _ = store.save();
    }
}

/// Load process definitions from Tauri Store and record every failure point.
fn load_definitions(app: &AppHandle, state: &AppState) -> Vec<ProcessEntry> {
    let settings_path = app
        .path()
        .app_config_dir()
        .map(|path| path.join("settings.json"));
    state.logger.plugin(
        "process-manager",
        "INFO",
        &format!("读取进程定义: key={}, settings={}", STORE_KEY, settings_path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|_| "<路径不可用>".to_string())),
    );

    let store = match tauri_plugin_store::StoreBuilder::new(app, "settings.json").build() {
        Ok(store) => store,
        Err(err) => {
            state.logger.plugin("process-manager", "ERROR", &format!("打开 settings.json 失败: {}", err));
            return vec![];
        }
    };

    let Some(value) = store.get(STORE_KEY) else {
        state.logger.plugin("process-manager", "WARN", &format!("未找到配置键 {}", STORE_KEY));
        return vec![];
    };

    match serde_json::from_value::<Vec<ProcessEntry>>(value) {
        Ok(entries) => {
            state.logger.plugin("process-manager", "INFO", &format!("成功读取 {} 条进程定义", entries.len()));
            entries
        }
        Err(err) => {
            state.logger.plugin("process-manager", "ERROR", &format!("进程定义反序列化失败: {}", err));
            vec![]
        }
    }
}

/// Called during app startup to restore saved definitions into state
pub fn restore_definitions(app: &AppHandle, state: &AppState) {
    let defs = load_definitions(app, state);
    if defs.is_empty() {
        state.logger.plugin("process-manager", "INFO", "没有可恢复的进程定义");
        return;
    }
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
    state.logger.plugin("process-manager", "INFO", &format!("恢复完成，内存中共有 {} 条定义", processes.len()));
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
    save_definitions(&app, &state);
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
    save_definitions(&app, &state);

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
    save_definitions(&app, &state);

    state.logger.plugin(
        "process-manager",
        "INFO",
        &format!("进程更新: {} (id: {})", cmd_display, id),
    );

    Ok(result)
}

#[tauri::command]
pub fn list_processes(state: tauri::State<'_, AppState>) -> Vec<ProcessEntry> {
    let processes = state.processes.lock().unwrap();
    let entries: Vec<ProcessEntry> = processes.values().cloned().collect();
    state.logger.plugin("process-manager", "INFO", &format!("前端请求进程列表，返回 {} 条", entries.len()));
    entries
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
