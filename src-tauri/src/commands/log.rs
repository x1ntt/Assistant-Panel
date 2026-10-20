use crate::state::LogEntry;
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

fn latest_session(log_base: &std::path::Path) -> Result<Option<PathBuf>, String> {
    Ok(fs::read_dir(log_base)
        .map_err(|error| error.to_string())?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir() && entry.path().join("system.log").is_file())
        .max_by_key(|entry| entry.file_name())
        .map(|entry| entry.path()))
}

#[tauri::command]
pub async fn get_logs(app: AppHandle, source: Option<String>) -> Result<Vec<LogEntry>, String> {
    let log_base = app
        .path()
        .app_log_dir()
        .unwrap_or_else(|_| PathBuf::from("./logs"));

    // Find the latest log directory
    let latest_dir = latest_session(&log_base)?;

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

    let latest_dir = latest_session(&log_base)?;

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

#[cfg(test)]
mod tests {
    use super::latest_session;

    #[test]
    fn diagnostics_directory_does_not_hide_the_latest_application_logs() {
        let root = std::env::temp_dir().join(format!("assistant-log-test-{}", nanoid::nanoid!()));
        for name in ["2026-10-19_09-00-00", "2026-10-20_09-00-00", "diagnostics"] {
            std::fs::create_dir_all(root.join(name)).unwrap();
        }
        for name in ["2026-10-19_09-00-00", "2026-10-20_09-00-00"] {
            std::fs::write(root.join(name).join("system.log"), "").unwrap();
        }
        assert_eq!(latest_session(&root).unwrap(), Some(root.join("2026-10-20_09-00-00")));
        std::fs::remove_dir_all(root).unwrap();
    }
}
