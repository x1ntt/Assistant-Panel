use crate::state::LogEntry;
use chrono::Local;
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[tauri::command]
pub async fn get_logs(app: AppHandle, source: Option<String>) -> Result<Vec<LogEntry>, String> {
    let log_base = app
        .path()
        .app_log_dir()
        .unwrap_or_else(|_| PathBuf::from("./logs"));

    // Find the latest log directory
    let latest_dir = fs::read_dir(&log_base)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .max_by_key(|e| e.file_name())
        .map(|e| e.path());

    let Some(dir) = latest_dir else {
        return Ok(vec![]);
    };

    let log_source = source.unwrap_or_else(|| "system".to_string());
    let log_file = dir.join(format!("{}.log", log_source));

    if !log_file.exists() {
        return Ok(vec![]);
    }

    let content = fs::read_to_string(&log_file).map_err(|e| e.to_string())?;
    let entries: Vec<LogEntry> = content
        .lines()
        .filter_map(|line| parse_log_line(line, &log_source))
        .collect();

    Ok(entries)
}

#[tauri::command]
pub async fn list_log_sources(app: AppHandle) -> Result<Vec<String>, String> {
    let log_base = app
        .path()
        .app_log_dir()
        .unwrap_or_else(|_| PathBuf::from("./logs"));

    let latest_dir = fs::read_dir(&log_base)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .max_by_key(|e| e.file_name())
        .map(|e| e.path());

    let Some(dir) = latest_dir else {
        return Ok(vec![]);
    };

    let sources: Vec<String> = fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "log"))
        .filter_map(|e| {
            e.path()
                .file_stem()
                .and_then(|s| s.to_str().map(|s| s.to_string()))
        })
        .collect();

    Ok(sources)
}

fn parse_log_line(line: &str, source: &str) -> Option<LogEntry> {
    // Format: [2026-09-14 15:30:01] [INFO] [system] message
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    let rest = line.strip_prefix('[')?;
    let (timestamp, rest) = rest.split_once(']')?;
    let rest = rest.strip_prefix(" [")?;
    let (level, rest) = rest.split_once(']')?;
    let rest = rest.strip_prefix(" [")?;
    let (_, rest) = rest.split_once(']')?;
    let message = rest.trim();

    Some(LogEntry {
        timestamp: timestamp.to_string(),
        level: level.to_string(),
        source: source.to_string(),
        message: message.to_string(),
    })
}
