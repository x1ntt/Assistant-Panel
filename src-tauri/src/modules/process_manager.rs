use crate::state::{AppState, PidMapEntry, ProcessEntry};
use chrono::Local;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::{AppHandle, Emitter, Manager};
use tokio_util::sync::CancellationToken;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Console::{ClosePseudoConsole, CreatePseudoConsole, COORD, HPCON};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, TerminateJobObject,
    JobObjectExtendedLimitInformation, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows::Win32::System::Pipes::CreatePipe;
use windows::Win32::System::Threading as thr;

pub fn parse_command(command: &str) -> (String, Vec<String>) {
    let parts: Vec<String> = split_command(command);
    if parts.is_empty() {
        return (String::new(), vec![]);
    }
    let cmd = parts[0].clone();
    let args = parts[1..].to_vec();
    (cmd, args)
}

fn split_command(command: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    let mut quote_char = ' ';
    for ch in command.chars() {
        if in_quote {
            if ch == quote_char {
                in_quote = false;
            } else {
                current.push(ch);
            }
        } else if ch == '"' || ch == '\'' {
            in_quote = true;
            quote_char = ch;
        } else if ch == ' ' || ch == '\t' {
            if !current.is_empty() {
                parts.push(current.clone());
                current.clear();
            }
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

fn pid_map_path(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("process_manager.json")
}

pub fn save_pid_map(app: &AppHandle, state: &AppState) {
    let processes = state.processes.lock().unwrap();
    write_pid_map(app, &processes);
}

fn write_pid_map(app: &AppHandle, processes: &HashMap<String, ProcessEntry>) {
    let entries: Vec<PidMapEntry> = processes
        .values()
        .filter_map(|p| p.pid.map(|pid| PidMapEntry { id: p.id.clone(), pid }))
        .collect();
    let path = pid_map_path(app);
    if let Ok(json) = serde_json::to_string_pretty(&entries) {
        let _ = fs::create_dir_all(path.parent().unwrap());
        let _ = fs::write(&path, json);
    }
}

fn load_pid_map(app: &AppHandle) -> HashMap<String, u32> {
    let path = pid_map_path(app);
    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(entries) = serde_json::from_str::<Vec<PidMapEntry>>(&content) {
            return entries.into_iter().map(|e| (e.id, e.pid)).collect();
        }
    }
    HashMap::new()
}

pub fn check_alive(pid: u32) -> bool {
    use windows::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    const STILL_ACTIVE: u32 = 259;
    unsafe {
        let handle = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(h) => h,
            Err(_) => return false,
        };
        let mut exit_code: u32 = 0;
        let result = GetExitCodeProcess(handle, &mut exit_code);
        let _ = CloseHandle(handle);
        result.is_ok() && exit_code == STILL_ACTIVE
    }
}

/// Kill an entire process tree by closing the Job Object.
/// All processes assigned to the job are terminated, including launcher children.
pub fn kill_job(job_handle: HANDLE) {
    unsafe {
        // TerminateJobObject kills all processes in the job
        let _ = TerminateJobObject(job_handle, 1);
        let _ = CloseHandle(job_handle);
    }
}

/// Fallback kill by PID that terminates the whole process tree.
/// Used in addition to the Job Object kill, because the job handle may be
/// missing (e.g. reconnected/restored processes have `#[serde(skip)]` handles)
/// or the process may not have been assigned to the job successfully.
pub fn kill_process_tree(pid: u32) {
    if pid == 0 {
        return;
    }
    // Use taskkill /T /F to terminate the process and all descendants.
    // CREATE_NO_WINDOW keeps it from flashing a console.
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let output = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    if let Ok(o) = output {
        if !o.status.success() {
            // Non-zero (e.g. 128 = process already gone) is usually harmless;
            // log for diagnostics only.
            eprintln!(
                "[process-manager] taskkill pid={} status={} stderr={}",
                pid,
                o.status,
                String::from_utf8_lossy(&o.stderr).trim()
            );
        }
    }
}

/// Spawn a child process inside a Windows Job Object and start a monitor thread.
/// Returns (pid, job_handle, child_arc, cancel).
pub fn spawn_and_monitor(
    entry: &ProcessEntry,
    app: &AppHandle,
) -> Result<(u32, HANDLE, Arc<Mutex<Option<Child>>>, CancellationToken), String> {
    let (cmd, args) = if entry.cmd.is_empty() {
        parse_command(&entry.command)
    } else {
        (entry.cmd.clone(), entry.args.clone())
    };
    if cmd.is_empty() {
        return Err("命令为空".to_string());
    }

    let working_dir = if entry.working_dir.is_empty() {
        ".".to_string()
    } else {
        entry.working_dir.clone()
    };

    // Create Job Object with KILL_ON_JOB_CLOSE — all children die when handle closes
    let job_handle = unsafe {
        let job = CreateJobObjectW(None, None)
            .map_err(|e| format!("创建 Job Object 失败: {}", e))?;

        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &limits as *const _ as *const _,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
        .map_err(|e| format!("设置 Job Object 失败: {}", e))?;

        job
    };

    // Create the child SUSPENDED so we can assign it to the Job Object BEFORE
    // it runs. Use ConPTY so the child believes it has a real console (line-
    // buffered stdout for Python/Node/C/Java, not just PYTHONUNBUFFERED).
    let (session, stdout_read) = match create_conpty() {
        Ok(v) => v,
        Err(e) => {
            unsafe { let _ = CloseHandle(job_handle); }
            return Err(e);
        }
    };
    let conpty_raw = session.hpc;
    install_conpty(&entry.id, session);

    let cmdline = build_command_line(&cmd, &args);
    let mut cmdline_w = to_wide(&cmdline);
    let working_dir_abs = resolve_working_dir(&working_dir);
    let mut workdir_w = to_wide(&working_dir_abs);
    let env_block = build_env_block(&[
        // Belt-and-suspenders for Python in case ConPTY is bypassed.
        ("PYTHONUNBUFFERED", "1"),
        ("PYTHONIOENCODING", "utf-8"),
    ]);

    let mut attr_size: usize = 0;
    unsafe {
        // The first call is a size probe: it is REQUIRED to fail with
        // ERROR_INSUFFICIENT_BUFFER (0x8007007A) and write the buffer size.
        // Do not treat that failure as fatal.
        let _ = thr::InitializeProcThreadAttributeList(None, 1, None, &mut attr_size);
        if attr_size == 0 {
            close_conpty_owned(&entry.id, conpty_raw);
            let _ = CloseHandle(job_handle);
            let _ = CloseHandle(stdout_read);
            return Err("初始化进程属性失败: 无法获取属性列表大小".to_string());
        }
        let mut attr_buf = vec![0u8; attr_size];
        let attr_list = thr::LPPROC_THREAD_ATTRIBUTE_LIST(attr_buf.as_mut_ptr() as *mut _);
        if let Err(e) = thr::InitializeProcThreadAttributeList(Some(attr_list), 1, None, &mut attr_size) {
            close_conpty_owned(&entry.id, conpty_raw);
            let _ = CloseHandle(job_handle);
            let _ = CloseHandle(stdout_read);
            return Err(format!("初始化进程属性列表失败: {}", e));
        }
        if let Err(e) = thr::UpdateProcThreadAttribute(
            attr_list,
            0,
            thr::PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
            Some(conpty_raw as *const core::ffi::c_void),
            std::mem::size_of::<HPCON>(),
            None,
            None,
        ) {
            thr::DeleteProcThreadAttributeList(attr_list);
            close_conpty_owned(&entry.id, conpty_raw);
            let _ = CloseHandle(job_handle);
            let _ = CloseHandle(stdout_read);
            return Err(format!("绑定伪控制台失败: {}", e));
        }

        let mut si_ex = thr::STARTUPINFOEXW::default();
        si_ex.StartupInfo.cb = std::mem::size_of::<thr::STARTUPINFOEXW>() as u32;
        si_ex.lpAttributeList = attr_list;

        let mut pi = thr::PROCESS_INFORMATION::default();
        let created = thr::CreateProcessW(
            None,
            Some(windows::core::PWSTR(cmdline_w.as_mut_ptr())),
            None,
            None,
            false,
            thr::CREATE_SUSPENDED
                | thr::CREATE_UNICODE_ENVIRONMENT
                | thr::EXTENDED_STARTUPINFO_PRESENT,
            Some(env_block.as_ptr() as *const core::ffi::c_void),
            windows::core::PWSTR(workdir_w.as_mut_ptr()),
            &si_ex.StartupInfo,
            &mut pi,
        );

        thr::DeleteProcThreadAttributeList(attr_list);

        if let Err(e) = created {
            close_conpty_owned(&entry.id, conpty_raw);
            let _ = CloseHandle(job_handle);
            let _ = CloseHandle(stdout_read);
            return Err(format!("启动失败: {} (cmd: {})", e, cmd));
        }

        let pid = pi.dwProcessId;

        // Assign to job while still suspended, then resume the primary thread
        // (directly via hThread — no toolhelp race).
        let _ = AssignProcessToJobObject(job_handle, pi.hProcess);

        if thr::ResumeThread(pi.hThread) == u32::MAX {
            close_conpty_owned(&entry.id, conpty_raw);
            let _ = CloseHandle(job_handle);
            let _ = CloseHandle(stdout_read);
            let _ = CloseHandle(pi.hThread);
            let _ = CloseHandle(pi.hProcess);
            if let Some(state) = app.try_state::<AppState>() {
                state.logger.plugin(
                    "process-manager",
                    "ERROR",
                    &format!("恢复挂起进程失败 (pid: {}, id: {}): ResumeThread 失败", pid, entry.id),
                );
            }
            return Err("进程未能启动（仍处于挂起）: ResumeThread 失败".to_string());
        }

        let _ = CloseHandle(pi.hThread);
        // Liveness is tracked via pid + check_alive.
        let _ = CloseHandle(pi.hProcess);

        // Keep stdin_write / in_read / out_write open until the console is
        // closed (held in the ConPtySession registry). Closing stdin_write
        // here makes ConPTY tear down and kill the client.

        let cancel = CancellationToken::new();
        // Single merged console stream (stdout+stderr) into the ring buffer.
        spawn_output_reader(entry.id.clone(), "stdout", conpty_handle_reader(stdout_read), app.clone());

        let child_arc: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));
        let app_clone = app.clone();
        let id = entry.id.clone();
        let child_clone = child_arc.clone();
        let cancel_clone = cancel.clone();
        let conpty_for_monitor = conpty_raw;

        std::thread::spawn(move || {
            monitor_process(id.clone(), pid, child_clone, cancel_clone, app_clone);
            // Process is gone: closing the pseudo console unblocks the reader
            // (EOF) so its thread can exit instead of hanging on the pipe.
            close_conpty_owned(&id, conpty_for_monitor);
        });

        Ok((pid, job_handle, child_arc, cancel))
    }
}

/// Wrap a raw ConPTY output HANDLE in a Read impl (takes ownership).
fn conpty_handle_reader(handle: HANDLE) -> std::fs::File {
    use std::os::windows::io::FromRawHandle;
    unsafe { std::fs::File::from_raw_handle(handle.0 as _) }
}

/// Decode pipe bytes. Prefer strict UTF-8 (set via PYTHONIOENCODING for Python);
/// fall back to lossy UTF-8 so a bad byte cannot stall the whole stream.
fn decode_pipe_bytes(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// Read a process output stream line-by-line, appending to the ring buffer and
/// emitting a live `process-manager:output` event for each line.
fn spawn_output_reader<R: std::io::Read + Send + 'static>(
    id: String,
    stream: &'static str,
    reader: R,
    app: AppHandle,
) {
    use std::io::{BufRead, BufReader};
    use std::sync::atomic::{AtomicBool, Ordering};
    std::thread::spawn(move || {
        let logged_first = AtomicBool::new(false);
        let buf = BufReader::new(reader);
        // split(b'\n') yields a final Ok(item) on EOF even without a trailing
        // newline, so a last partial line is not lost when the process exits.
        for line in buf.split(b'\n') {
            let bytes = match line {
                Ok(b) => b,
                Err(e) => {
                    if let Some(state) = app.try_state::<AppState>() {
                        state.logger.plugin(
                            "process-manager",
                            "WARN",
                            &format!("输出读取错误 (id: {}, stream: {}): {}", id, stream, e),
                        );
                    }
                    break;
                }
            };
            // ConPTY emits ANSI + CR; normalize into plain display lines.
            let mut text = decode_pipe_bytes(&bytes);
            if text.ends_with('\r') {
                text.pop();
            }
            text = strip_ansi(&text);
            let entry = crate::state::OutputLine {
                id: id.clone(),
                stream: stream.to_string(),
                line: text,
                time: Local::now().format("%H:%M:%S").to_string(),
            };
            if !logged_first.swap(true, Ordering::Relaxed) {
                if let Some(state) = app.try_state::<AppState>() {
                    state.logger.plugin(
                        "process-manager",
                        "INFO",
                        &format!(
                            "捕获输出首行: id={}, stream={}, len={}",
                            id,
                            stream,
                            entry.line.len()
                        ),
                    );
                }
            }
            if let Some(state) = app.try_state::<AppState>() {
                let mut map = state.process_output.lock().unwrap();
                let dq = map.entry(id.clone()).or_default();
                dq.push_back(entry.clone());
                while dq.len() > crate::state::PROCESS_OUTPUT_MAX_LINES {
                    dq.pop_front();
                }
            }
            let _ = app.emit("process-manager:output", &entry);
        }
        if let Some(state) = app.try_state::<AppState>() {
            state.logger.plugin(
                "process-manager",
                "INFO",
                &format!("输出流结束: id={}, stream={}", id, stream),
            );
        }
    });
}

/// Full start: spawn + update entry + save pid map.
pub fn start_process(entry: &mut ProcessEntry, app: &AppHandle, state: &AppState) -> Result<(), String> {
    let (pid, job_handle, _, _) = spawn_and_monitor(entry, app)?;
    entry.pid = Some(pid);
    entry.job_handle = Some(job_handle.0 as isize);
    entry.status = "running".to_string();
    entry.started_at = Some(Local::now().format("%Y-%m-%d %H:%M:%S").to_string());
    save_pid_map(app, state);
    Ok(())
}

fn monitor_process(
    id: String,
    pid: u32,
    child: Arc<Mutex<Option<Child>>>,
    cancel: CancellationToken,
    app: AppHandle,
) {
    loop {
        if cancel.is_cancelled() {
            return;
        }

        // Use try_wait on the child handle — this works for the direct child.
        // For launcher processes, the child exits quickly but the Job Object
        // keeps the real process alive. When we kill via Job, everything dies.
        let exited = {
            let mut guard = child.lock().unwrap();
            if let Some(ref mut c) = *guard {
                match c.try_wait() {
                    Ok(Some(_)) => true,
                    _ => false,
                }
            } else {
                !check_alive(pid)
            }
        };

        if exited {
            let state = app.try_state::<AppState>();
            if let Some(state) = state {
                let mut processes = state.processes.lock().unwrap();
                if let Some(entry) = processes.get_mut(&id) {
                    if entry.status == "running" {
                        entry.status = "crashed".to_string();
                        entry.pid = None;
                    }
                }
                write_pid_map(&app, &processes);

                let should_restart = processes
                    .get(&id)
                    .map(|e| e.auto_restart && e.status == "crashed")
                    .unwrap_or(false);

                if let Some(entry) = processes.get(&id) {
                    let _ = app.emit("process-manager:status", entry.clone());
                }

                drop(processes);

                if should_restart {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    let mut processes = state.processes.lock().unwrap();
                    if let Some(entry) = processes.get_mut(&id) {
                        if entry.status == "crashed" {
                            match spawn_and_monitor(entry, &app) {
                                Ok((new_pid, new_job, _, _)) => {
                                    entry.pid = Some(new_pid);
                                    entry.job_handle = Some(new_job.0 as isize);
                                    entry.status = "running".to_string();
                                    entry.started_at = Some(Local::now().format("%Y-%m-%d %H:%M:%S").to_string());
                                    state.logger.plugin(
                                        "process-manager",
                                        "INFO",
                                        &format!("自动重启: {} (pid: {}, id: {})", entry.command, new_pid, id),
                                    );
                                }
                                Err(e) => {
                                    entry.status = "crashed".to_string();
                                    entry.pid = None;
                                    entry.job_handle = None;
                                    state.logger.plugin(
                                        "process-manager",
                                        "ERROR",
                                        &format!("自动重启失败: {} — {}", entry.command, e),
                                    );
                                }
                            }
                        }
                    }
                    // Emit the post-restart state so the UI reflects the new pid/status.
                    write_pid_map(&app, &processes);
                    if let Some(entry) = processes.get(&id) {
                        let _ = app.emit("process-manager:status", entry.clone());
                    }
                }
            }
            return;
        }

        std::thread::sleep(std::time::Duration::from_secs(2));
    }
}

pub fn startup_auto_processes(app: &AppHandle, state: &AppState) {
    {
        let pid_map = load_pid_map(app);
        let mut processes = state.processes.lock().unwrap();

        for (_, entry) in processes.iter_mut() {
            if let Some(&pid) = pid_map.get(&entry.id) {
                if check_alive(pid) {
                    entry.pid = Some(pid);
                    entry.status = "running".to_string();
                    let app_clone = app.clone();
                    let id = entry.id.clone();
                    let cancel = CancellationToken::new();
                    let child_arc = Arc::new(Mutex::new(None));
                    let child_clone = child_arc.clone();
                    let cancel_clone = cancel.clone();
                    std::thread::spawn(move || {
                        monitor_process(id, pid, child_clone, cancel_clone, app_clone);
                    });
                    state.logger.plugin(
                        "process-manager",
                        "INFO",
                        &format!("重连已有进程: {} (pid: {}, id: {})", entry.command, pid, entry.id),
                    );
                    continue;
                }
            }

            if entry.auto_start {
                match spawn_and_monitor(entry, app) {
                    Ok((pid, job_handle, _, _)) => {
                        entry.pid = Some(pid);
                        entry.job_handle = Some(job_handle.0 as isize);
                        entry.status = "running".to_string();
                        entry.started_at = Some(Local::now().format("%Y-%m-%d %H:%M:%S").to_string());
                        state.logger.plugin(
                            "process-manager",
                            "INFO",
                            &format!("自动启动: {} (pid: {}, id: {})", entry.command, pid, entry.id),
                        );
                    }
                    Err(e) => {
                        state.logger.plugin(
                            "process-manager",
                            "ERROR",
                            &format!("自动启动失败: {} — {}", entry.command, e),
                        );
                        entry.status = "stopped".to_string();
                    }
                }
            } else {
                entry.status = "stopped".to_string();
                entry.pid = None;
            }
        }
    }

    // Start the reconciliation loop as a safety net against missed state updates.
    start_reconcile_loop(app.clone());
}

/// Background safety-net loop: periodically verifies that every process marked
/// "running" is actually alive. If a process died without its monitor thread
/// catching it (e.g. monitor exited, PID reused, external kill), this corrects
/// the state to "crashed", emits an update, and triggers auto-restart if enabled.
pub fn start_reconcile_loop(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(3));

        let Some(state) = app.try_state::<AppState>() else { continue };

        // Collect ids that are marked running but are actually dead.
        let dead_ids: Vec<String> = {
            let processes = state.processes.lock().unwrap();
            processes
                .values()
                .filter(|e| e.status == "running")
                .filter(|e| match e.pid {
                    Some(pid) => !check_alive(pid),
                    None => true, // running but no pid == inconsistent
                })
                .map(|e| e.id.clone())
                .collect()
        };

        if dead_ids.is_empty() {
            continue;
        }

        for id in dead_ids {
            let mut processes = state.processes.lock().unwrap();
            let Some(entry) = processes.get_mut(&id) else { continue };
            // Re-check under lock in case a monitor already handled it.
            if entry.status != "running" {
                continue;
            }
            let still_dead = match entry.pid {
                Some(pid) => !check_alive(pid),
                None => true,
            };
            if !still_dead {
                continue;
            }

            entry.status = "crashed".to_string();
            entry.pid = None;
            entry.job_handle = None;
            let auto_restart = entry.auto_restart;
            let command = entry.command.clone();
            state.logger.plugin(
                "process-manager",
                "WARN",
                &format!("状态核对: 进程已退出 {} (id: {})", command, id),
            );

            if auto_restart {
                match spawn_and_monitor(entry, &app) {
                    Ok((new_pid, new_job, _, _)) => {
                        entry.pid = Some(new_pid);
                        entry.job_handle = Some(new_job.0 as isize);
                        entry.status = "running".to_string();
                        entry.started_at =
                            Some(Local::now().format("%Y-%m-%d %H:%M:%S").to_string());
                        state.logger.plugin(
                            "process-manager",
                            "INFO",
                            &format!("状态核对: 自动重启 {} (pid: {}, id: {})", command, new_pid, id),
                        );
                    }
                    Err(e) => {
                        entry.status = "crashed".to_string();
                        entry.pid = None;
                        entry.job_handle = None;
                        state.logger.plugin(
                            "process-manager",
                            "ERROR",
                            &format!("状态核对: 自动重启失败 {} — {}", command, e),
                        );
                    }
                }
            }

            write_pid_map(&app, &processes);
            if let Some(entry) = processes.get(&id) {
                let _ = app.emit("process-manager:status", entry.clone());
            }
        }
    });
}

// Re-export SetInformationJobObject for spawn_and_monitor
use windows::Win32::System::JobObjects::SetInformationJobObject;

// ---------------------------------------------------------------------------
// ConPTY: give children a real console so they line-buffer stdout (works for
// Python, Node, C/C++, Java, … — not just interpreters with special env vars).
// ---------------------------------------------------------------------------

/// All ConPTY-related handles that must stay open for the session lifetime.
///
/// CreatePseudoConsole does **not** reliably take its own reference on older
/// Windows builds, and closing the stdin write end makes the console treat the
/// host as gone and **kill the client**. Observed as: first line is ANSI-only
/// (len=0), output EOF immediately, process marked crashed, auto-restart loop.
#[derive(Clone, Copy)]
struct ConPtySession {
    hpc: isize,
    /// Read end of the input pipe (passed to CreatePseudoConsole).
    in_read: isize,
    /// Write end of the output pipe (passed to CreatePseudoConsole).
    out_write: isize,
    /// Write end of the input pipe (we would use this for stdin).
    stdin_write: isize,
}

impl ConPtySession {
    fn close(self) {
        unsafe {
            if self.hpc != 0 {
                ClosePseudoConsole(HPCON(self.hpc));
            }
            // Close console-side pipe ends only after the console is gone.
            for h in [self.in_read, self.out_write, self.stdin_write] {
                if h != 0 {
                    let _ = CloseHandle(HANDLE(h as *mut _));
                }
            }
        }
    }
}

/// process id → session. Closed exactly once (matched by hpc).
fn conpty_registry() -> &'static Mutex<HashMap<String, ConPtySession>> {
    static M: OnceLock<Mutex<HashMap<String, ConPtySession>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(HashMap::new()))
}

fn install_conpty(id: &str, session: ConPtySession) {
    let mut map = conpty_registry().lock().unwrap();
    if let Some(old) = map.insert(id.to_string(), session) {
        if old.hpc != session.hpc {
            old.close();
        }
    }
}

/// Close `session` only if it is still the registered one for `id`.
fn close_conpty_owned(id: &str, hpc: isize) {
    if hpc == 0 {
        return;
    }
    let mut map = conpty_registry().lock().unwrap();
    if map.get(id).map(|s| s.hpc) == Some(hpc) {
        let session = map.remove(id).unwrap();
        drop(map);
        session.close();
    }
}

fn quote_arg(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
        return arg.to_string();
    }
    let mut out = String::from("\"");
    let mut slashes = 0usize;
    for c in arg.chars() {
        match c {
            '\\' => slashes += 1,
            '"' => {
                out.push_str(&"\\".repeat(slashes * 2 + 1));
                out.push('"');
                slashes = 0;
            }
            _ => {
                out.push_str(&"\\".repeat(slashes));
                slashes = 0;
                out.push(c);
            }
        }
    }
    out.push_str(&"\\".repeat(slashes * 2));
    out.push('"');
    out
}

fn build_command_line(cmd: &str, args: &[String]) -> String {
    let mut s = quote_arg(cmd);
    for a in args {
        s.push(' ');
        s.push_str(&quote_arg(a));
    }
    s
}

/// Resolve a working directory for CreateProcessW without the `\\?\` verbatim
/// prefix that `fs::canonicalize` returns on Windows. Verbatim paths break
/// `os.getcwd()` / `os.path` in some runtimes (e.g. Python http.server then
/// 404s every request including `/`).
fn resolve_working_dir(raw: &str) -> String {
    let base = if raw.is_empty() { "." } else { raw };
    let abs = fs::canonicalize(base).unwrap_or_else(|_| std::path::PathBuf::from(base));
    let s = abs.to_string_lossy().replace('/', "\\");
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{}", rest);
    }
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        return rest.to_string();
    }
    s
}
fn build_env_block(overrides: &[(&str, &str)]) -> Vec<u16> {
    let mut map: HashMap<String, String> = std::env::vars().collect();
    for (k, v) in overrides {
        map.insert((*k).to_string(), (*v).to_string());
    }
    let mut block: Vec<u16> = Vec::new();
    for (k, v) in &map {
        for u in format!("{}={}", k, v).encode_utf16() {
            block.push(u);
        }
        block.push(0);
    }
    block.push(0);
    block
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Strip CSI / OSC / simple ESC sequences that ConPTY emits for colors & cursor.
fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            out.push(c);
            continue;
        }
        match chars.peek().copied() {
            Some('[') => {
                chars.next();
                // CSI: parameters then final byte @-~
                while let Some(n) = chars.next() {
                    if ('\x40'..='\x7e').contains(&n) {
                        break;
                    }
                }
            }
            Some(']') => {
                chars.next();
                // OSC: until BEL or ST (ESC \)
                while let Some(n) = chars.next() {
                    if n == '\x07' {
                        break;
                    }
                    if n == '\x1b' {
                        if chars.peek() == Some(&'\\') {
                            chars.next();
                        }
                        break;
                    }
                }
            }
            Some(_) => {
                chars.next();
            }
            None => {}
        }
    }
    out
}

/// Create a ConPTY plus the pipe pair. Returns the session and the stdout
/// read end (the only handle the reader thread takes ownership of).
fn create_conpty() -> Result<(ConPtySession, HANDLE), String> {
    unsafe {
        let mut in_read = HANDLE::default();
        let mut in_write = HANDLE::default();
        let mut out_read = HANDLE::default();
        let mut out_write = HANDLE::default();
        CreatePipe(&mut in_read, &mut in_write, None, 0)
            .map_err(|e| format!("创建输入管道失败: {}", e))?;
        if let Err(e) = CreatePipe(&mut out_read, &mut out_write, None, 0) {
            let _ = CloseHandle(in_read);
            let _ = CloseHandle(in_write);
            return Err(format!("创建输出管道失败: {}", e));
        }

        // Wide console so programs don't hard-wrap logs at 80 columns.
        let size = COORD { X: 200, Y: 32767 };
        let hpc = match CreatePseudoConsole(size, in_read, out_write, 0) {
            Ok(h) => h,
            Err(e) => {
                let _ = CloseHandle(in_read);
                let _ = CloseHandle(in_write);
                let _ = CloseHandle(out_read);
                let _ = CloseHandle(out_write);
                return Err(format!("创建伪控制台失败: {}", e));
            }
        };

        // NOTE: do NOT close in_read / out_write / in_write here. The console
        // needs them for the whole session (see ConPtySession docs).
        let session = ConPtySession {
            hpc: hpc.0,
            in_read: in_read.0 as isize,
            out_write: out_write.0 as isize,
            stdin_write: in_write.0 as isize,
        };
        Ok((session, out_read))
    }
}

