use chrono::Local;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Manager};

use crate::state::LogEntry;

#[derive(Clone)]
pub struct Logger {
    log_dir: Arc<PathBuf>,
}

impl Logger {
    pub fn new(app_handle: &AppHandle) -> Self {
        let timestamp = Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
        let log_base = app_handle
            .path()
            .app_log_dir()
            .unwrap_or_else(|_| PathBuf::from("./logs"));
        let log_dir = log_base.join(&timestamp);
        fs::create_dir_all(&log_dir).ok();
        Self {
            log_dir: Arc::new(log_dir),
        }
    }

    pub fn system(&self, level: &str, message: &str) {
        self.write_log("system", level, message);
    }

    pub fn plugin(&self, plugin_id: &str, level: &str, message: &str) {
        self.write_log(plugin_id, level, message);
        // Also write to system log for visibility
        self.write_log("system", level, &format!("[{}] {}", plugin_id, message));
    }

    fn write_log(&self, source: &str, level: &str, message: &str) {
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
        let line = format!("[{}] [{}] [{}] {}\n", timestamp, level, source, message);
        let log_file = self.log_dir.join(format!("{}.log", source));
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_file)
        {
            file.write_all(line.as_bytes()).ok();
        }
    }
}
