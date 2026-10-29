use crate::modules::process_runtime::{self, ProcessRuntime};
use crate::state::{AppState, PidMapEntry, ProcessEntry};
use chrono::Local;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

pub fn parse_command(command: &str) -> (String, Vec<String>) {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    for ch in command.chars() {
        if let Some(quoted) = quote {
            if ch == quoted {
                quote = None;
            } else {
                current.push(ch);
            }
        } else if ch == '"' || ch == '\'' {
            quote = Some(ch);
        } else if ch == ' ' || ch == '\t' {
            if !current.is_empty() {
                parts.push(std::mem::take(&mut current));
            }
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    let cmd = if parts.is_empty() {
        String::new()
    } else {
        parts.remove(0)
    };
    (cmd, parts)
}

fn pid_map_path(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("process_manager.json")
}

pub fn save_pid_map(app: &AppHandle, state: &AppState) {
    write_pid_map(app, &state.processes.lock().unwrap());
}

fn write_pid_map(app: &AppHandle, processes: &HashMap<String, ProcessEntry>) {
    let entries: Vec<PidMapEntry> = processes
        .values()
        .filter(|p| p.status == "running")
        .filter_map(|p| {
            p.pid.map(|pid| PidMapEntry {
                id: p.id.clone(),
                pid,
            })
        })
        .collect();
    let path = pid_map_path(app);
    if let Ok(json) = serde_json::to_string_pretty(&entries) {
        let _ = fs::create_dir_all(path.parent().unwrap());
        let _ = fs::write(path, json);
    }
}

fn load_pid_map(app: &AppHandle) -> HashMap<String, u32> {
    fs::read_to_string(pid_map_path(app))
        .ok()
        .and_then(|text| serde_json::from_str::<Vec<PidMapEntry>>(&text).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|e| (e.id, e.pid))
        .collect()
}

fn append_output(app: &AppHandle, id: &str, stream: &str, text: String) {
    let line = crate::state::OutputLine {
        id: id.into(),
        stream: stream.into(),
        line: text,
        time: Local::now().format("%H:%M:%S").to_string(),
    };
    if let Some(state) = app.try_state::<AppState>() {
        let mut map = state.process_output.lock().unwrap();
        let buffer = map.entry(id.into()).or_default();
        buffer.push_back(line.clone());
        while buffer.len() > crate::state::PROCESS_OUTPUT_MAX_LINES {
            buffer.pop_front();
        }
    }
    let _ = app.emit("process-manager:output", &line);
}

fn spawn_output_reader(id: String, stream: &'static str, reader: std::fs::File, app: AppHandle) {
    std::thread::spawn(move || {
        let mut first = true;
        let result = process_runtime::read_output_lines(reader, |text| {
            if first {
                first = false;
                if let Some(state) = app.try_state::<AppState>() {
                    state.logger.plugin(
                        "process-manager",
                        "INFO",
                        &format!("捕获输出首行: id={id}, stream={stream}, len={}", text.len()),
                    );
                }
            }
            append_output(&app, &id, stream, strip_ansi(&text));
        });
        if let Some(state) = app.try_state::<AppState>() {
            if let Err(error) = result {
                state.logger.plugin(
                    "process-manager",
                    "WARN",
                    &format!("输出读取错误: id={id}, stream={stream}, error={error}"),
                );
            }
            state.logger.plugin(
                "process-manager",
                "INFO",
                &format!("输出流结束: id={id}, stream={stream}"),
            );
        }
    });
}

/// Caller holds state.processes until the entry is inserted/updated. A very
/// short-lived child cannot be observed before its runtime is registered.
pub fn start_process(entry: &mut ProcessEntry, app: &AppHandle) -> Result<(), String> {
    if let Some(runtime) = &entry.runtime {
        if runtime.is_alive()? {
            return Err("进程组仍在运行，请先停止".into());
        }
    }
    let (cmd, args) = if entry.cmd.is_empty() {
        parse_command(&entry.command)
    } else {
        (entry.cmd.clone(), entry.args.clone())
    };
    if cmd.is_empty() {
        return Err("命令为空".into());
    }
    let spawned = process_runtime::spawn_captured(&cmd, &args, &entry.working_dir)?;
    let runtime = spawned.runtime;
    entry.pid = Some(runtime.pid);
    entry.runtime = Some(runtime.clone());
    entry.status = "running".into();
    entry.started_at = Some(Local::now().format("%Y-%m-%d %H:%M:%S").to_string());
    spawn_output_reader(entry.id.clone(), "stdout", spawned.stdout, app.clone());
    spawn_output_reader(entry.id.clone(), "stderr", spawned.stderr, app.clone());
    spawn_monitor(entry.id.clone(), runtime, app.clone());
    Ok(())
}

pub fn stop_entry(entry: &mut ProcessEntry) -> Result<(), String> {
    if let Some(runtime) = &entry.runtime {
        runtime.terminate()?;
    } else if let Some(pid) = entry.pid {
        // A restored definition has no borrowed raw Job handle to accidentally
        // close after Windows has already reused the handle value.
        if let Some(runtime) = ProcessRuntime::reconnect(pid)? {
            runtime.terminate()?;
        }
    }
    entry.status = "stopped".into();
    entry.pid = None;
    entry.runtime = None;
    Ok(())
}

fn is_current_run(entry: &ProcessEntry, runtime: &Arc<ProcessRuntime>) -> bool {
    entry
        .runtime
        .as_ref()
        .is_some_and(|current| Arc::ptr_eq(current, runtime))
}

fn spawn_monitor(id: String, runtime: Arc<ProcessRuntime>, app: AppHandle) {
    std::thread::spawn(move || {
        let mut warned = false;
        loop {
            let Some(state) = app.try_state::<AppState>() else {
                return;
            };
            let current = state
                .processes
                .lock()
                .unwrap()
                .get(&id)
                .is_some_and(|entry| entry.status == "running" && is_current_run(entry, &runtime));
            if !current {
                return;
            }
            match runtime.is_alive() {
                Ok(false) => {
                    handle_exit(&id, &runtime, &app, &state);
                    return;
                }
                Ok(true) => warned = false,
                Err(error) if !warned => {
                    // Query errors are not evidence of a crash; never spawn a
                    // duplicate Node just because a status query failed.
                    warned = true;
                    state.logger.plugin(
                        "process-manager",
                        "WARN",
                        &format!("状态查询失败，保留运行状态: id={id}, {error}"),
                    );
                }
                Err(_) => {}
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    });
}

fn handle_exit(id: &str, runtime: &Arc<ProcessRuntime>, app: &AppHandle, state: &AppState) {
    let auto_restart = {
        let mut processes = state.processes.lock().unwrap();
        let Some(entry) = processes.get_mut(id) else {
            return;
        };
        if entry.status != "running"
            || !is_current_run(entry, runtime)
            || runtime.is_alive() != Ok(false)
        {
            return;
        }
        entry.status = "crashed".into();
        entry.pid = None;
        let restart = entry.auto_restart;
        if !restart {
            entry.runtime = None;
        }
        state.logger.plugin(
            "process-manager",
            "INFO",
            &format!("进程组已退出: id={id}, root_pid={}", runtime.pid),
        );
        let _ = app.emit("process-manager:status", entry.clone());
        write_pid_map(app, &processes);
        restart
    };
    if !auto_restart {
        return;
    }
    std::thread::sleep(std::time::Duration::from_secs(1));
    let mut processes = state.processes.lock().unwrap();
    let Some(entry) = processes.get_mut(id) else {
        return;
    };
    // A stop/edit/remove/new run invalidates this monitor, even when a later
    // run has already crashed again. Both monitor and reconciliation use this.
    if entry.status != "crashed" || !is_current_run(entry, runtime) {
        return;
    }
    match start_process(entry, app) {
        Ok(()) => state.logger.plugin(
            "process-manager",
            "INFO",
            &format!("自动重启: id={id}, pid={:?}", entry.pid),
        ),
        Err(error) => {
            entry.runtime = None;
            state.logger.plugin(
                "process-manager",
                "ERROR",
                &format!("自动重启失败: id={id}, {error}"),
            );
        }
    }
    let _ = app.emit("process-manager:status", entry.clone());
    write_pid_map(app, &processes);
}

pub fn startup_auto_processes(app: &AppHandle, state: &AppState) {
    let pid_map = load_pid_map(app);
    {
        let mut processes = state.processes.lock().unwrap();
        for entry in processes.values_mut() {
            if let Some(&pid) = pid_map.get(&entry.id) {
                match ProcessRuntime::reconnect(pid) {
                    Ok(Some(runtime)) => {
                        entry.pid = Some(pid);
                        entry.status = "running".into();
                        entry.runtime = Some(runtime.clone());
                        spawn_monitor(entry.id.clone(), runtime, app.clone());
                        append_output(app, &entry.id, "stderr", "[进程管理] 已重连原有进程，无法接管旧输出管道；停止并重新启动后可捕获日志。".into());
                        state.logger.plugin(
                            "process-manager",
                            "INFO",
                            &format!("重连已有进程: id={}, pid={pid}", entry.id),
                        );
                        continue;
                    }
                    Err(error) => {
                        entry.status = "crashed".into();
                        state.logger.plugin(
                            "process-manager",
                            "ERROR",
                            &format!("{error}；本次不自动启动，避免重复进程"),
                        );
                        continue;
                    }
                    Ok(None) => {}
                }
            }
            if entry.auto_start {
                if let Err(error) = start_process(entry, app) {
                    state.logger.plugin(
                        "process-manager",
                        "ERROR",
                        &format!("自动启动失败: id={}, {error}", entry.id),
                    );
                    entry.status = "stopped".into();
                }
            }
        }
        write_pid_map(app, &processes);
    }
    start_reconcile_loop(app.clone());
}

/// Safety net uses the SAME job-aware liveness and generation checks as the
/// per-run monitor. It cannot mark an active launcher child as crashed.
fn start_reconcile_loop(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(3));
        let Some(state) = app.try_state::<AppState>() else {
            continue;
        };
        let candidates: Vec<_> = state
            .processes
            .lock()
            .unwrap()
            .values()
            .filter(|entry| entry.status == "running")
            .filter_map(|entry| {
                entry
                    .runtime
                    .as_ref()
                    .map(|runtime| (entry.id.clone(), runtime.clone()))
            })
            .collect();
        for (id, runtime) in candidates {
            if runtime.is_alive() == Ok(false) {
                handle_exit(&id, &runtime, &app, &state);
            }
        }
    });
}

fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('[') => {
                for c in chars.by_ref() {
                    if ('\x40'..='\x7e').contains(&c) {
                        break;
                    }
                }
            }
            Some(']') => {
                while let Some(c) = chars.next() {
                    if c == '\x07' {
                        break;
                    }
                    if c == '\x1b' {
                        if chars.peek() == Some(&'\\') {
                            chars.next();
                        }
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_old_monitor_does_not_own_a_restarted_or_stopped_entry() {
        let old = process_runtime::spawn_captured(
            "cmd.exe",
            &["/d".into(), "/c".into(), "exit 0".into()],
            "",
        )
        .unwrap();
        let new = process_runtime::spawn_captured(
            "cmd.exe",
            &["/d".into(), "/c".into(), "exit 0".into()],
            "",
        )
        .unwrap();
        let mut entry: ProcessEntry = serde_json::from_value(serde_json::json!({
            "id":"one", "command":"example", "cmd":"example", "args":[], "workingDir":"",
            "autoStart":false, "autoRestart":true, "pid":null, "status":"running", "startedAt":null
        }))
        .unwrap();
        entry.runtime = Some(old.runtime.clone());
        assert!(is_current_run(&entry, &old.runtime));
        entry.runtime = Some(new.runtime.clone());
        assert!(!is_current_run(&entry, &old.runtime));
        assert!(is_current_run(&entry, &new.runtime));
        let saved = serde_json::to_value(&entry).unwrap();
        assert!(
            saved.get("runtime").is_none(),
            "runtime handles must not enter saved definitions or IPC"
        );
        entry.runtime = None;
        assert!(!is_current_run(&entry, &new.runtime));
    }

    #[test]
    fn parses_quoted_windows_paths_and_strips_console_colors() {
        let (cmd, args) = parse_command(
            r#""C:\Program Files\Python\python.exe" node.py --config "D:\node config.json""#,
        );
        assert_eq!(cmd, r"C:\Program Files\Python\python.exe");
        assert_eq!(args, ["node.py", "--config", r"D:\node config.json"]);
        assert_eq!(strip_ansi("\x1b[31m节点日志\x1b[0m"), "节点日志");
    }
}
