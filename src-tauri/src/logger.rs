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
        let mut log_dir = log_base.join(&timestamp);
        let probe = |path: &std::path::Path| -> std::io::Result<()> {
            fs::create_dir_all(path)?;
            OpenOptions::new().create(true).append(true).open(path.join("system.log"))?;
            Ok(())
        };
        if let Err(error) = probe(&log_dir) {
            crate::startup_diagnostics::record(&format!("ordinary log initialization failed: {error}"));
            log_dir = std::env::temp_dir().join("assistant-panel-logs").join(&timestamp);
            if let Err(error) = probe(&log_dir) {
                crate::startup_diagnostics::record(&format!("ordinary log fallback failed: {error}"));
            }
        }
        crate::startup_diagnostics::record(&format!("ordinary log directory={}", log_dir.display()));
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
        match OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_file)
        {
            Ok(mut file) => {
                if let Err(error) = file.write_all(line.as_bytes()) {
                    crate::startup_diagnostics::record(&format!("ordinary log write failed: {error}"));
                }
            }
            Err(error) => crate::startup_diagnostics::record(&format!("ordinary log open failed: {error}")),
        }
    }
}
