use crate::logger::Logger;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

/// Max number of output lines retained per process (ring buffer).
pub const PROCESS_OUTPUT_MAX_LINES: usize = 1000;

pub struct AppState {
    pub port_forwards: Mutex<HashMap<String, Arc<ForwardRule>>>,
    pub pinned_windows: Mutex<HashMap<isize, PinnedWindow>>,
    pub pin_enabled: AtomicBool,
    pub processes: Mutex<HashMap<String, ProcessEntry>>,
    pub process_restore_status: Mutex<Result<(), String>>,
    /// Per-process stdout/stderr ring buffer (keyed by process id).
    pub process_output: Mutex<HashMap<String, VecDeque<OutputLine>>>,
    pub logger: Logger,
}

impl AppState {
    pub fn new(logger: Logger, pin_enabled: bool) -> Self {
        Self {
            port_forwards: Mutex::new(HashMap::new()),
            pinned_windows: Mutex::new(HashMap::new()),
            pin_enabled: AtomicBool::new(pin_enabled),
            processes: Mutex::new(HashMap::new()),
            process_restore_status: Mutex::new(Err("进程定义尚未恢复，请稍后重试".into())),
            process_output: Mutex::new(HashMap::new()),
            logger,
        }
    }
}

/// One line of captured process output.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputLine {
    /// Process id this line belongs to (used by the emitted event).
    pub id: String,
    /// "stdout" | "stderr"
    pub stream: String,
    pub line: String,
    pub time: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PinnedWindow {
    pub hwnd: isize,
    pub title: String,
    pub pinned_at: String,
}

pub struct ForwardRule {
    pub id: String,
    pub listen_addr: String,
    pub listen_port: u16,
    pub target_addr: String,
    pub target_port: u16,
    pub protocol: String,
    pub total_bytes_in: AtomicU64,
    pub total_bytes_out: AtomicU64,
    pub connections: Mutex<HashMap<String, Arc<ConnectionStats>>>,
    pub cancel: CancellationToken,
}

pub struct ConnectionStats {
    pub client_addr: String,
    pub bytes_in: AtomicU64,
    pub bytes_out: AtomicU64,
}

impl ConnectionStats {
    pub fn new(client_addr: String) -> Self {
        Self {
            client_addr,
            bytes_in: AtomicU64::new(0),
            bytes_out: AtomicU64::new(0),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForwardRuleSnapshot {
    pub id: String,
    pub listen_addr: String,
    pub listen_port: u16,
    pub target_addr: String,
    pub target_port: u16,
    pub protocol: String,
    pub total_bytes_in: u64,
    pub total_bytes_out: u64,
    pub speed_in: u64,
    pub speed_out: u64,
    pub connection_count: usize,
    pub connections: Vec<ConnectionSnapshot>,
    pub running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionSnapshot {
    pub client_addr: String,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub speed_in: u64,
    pub speed_out: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewRule {
    pub listen_addr: String,
    pub listen_port: u16,
    pub target_addr: String,
    pub target_port: u16,
    pub protocol: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InterfaceInfo {
    pub name: String,
    pub ip: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub source: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub hotkey: String,
    pub width: u32,
    pub theme: String,
    pub always_on_top: bool,
    pub auto_start: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: "Control+Shift+Backquote".to_string(),
            width: 420,
            theme: "system".to_string(),
            always_on_top: true,
            auto_start: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessEntry {
    pub id: String,
    pub command: String,
    pub cmd: String,
    pub args: Vec<String>,
    pub working_dir: String,
    pub auto_start: bool,
    pub auto_restart: bool,
    pub pid: Option<u32>,
    pub status: String,
    pub started_at: Option<String>,
    #[serde(skip)]
    pub job_handle: Option<isize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PidMapEntry {
    pub id: String,
    pub pid: u32,
}
