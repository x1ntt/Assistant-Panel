//! Best-effort diagnostics available before Tauri creates any windows.
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

struct Diagnostics {
    file: Mutex<File>,
    fallback: PathBuf,
    started: Instant,
}
static DIAGNOSTICS: OnceLock<Diagnostics> = OnceLock::new();

fn open(path: &std::path::Path) -> io::Result<File> {
    if let Some(parent) = path.parent() { fs::create_dir_all(parent)?; }
    OpenOptions::new().create_new(true).write(true).open(path)
}

pub fn init() {
    if DIAGNOSTICS.get().is_some() { return; }
    let name = format!("startup-{}-{}.log", chrono::Local::now().format("%Y%m%d-%H%M%S-%f"), std::process::id());
    let fallback = std::env::temp_dir().join("assistant-panel-diagnostics").join(&name);
    let primary = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
        .map(|p| p.join("com.administrator.assistant-panel/logs/diagnostics").join(&name));
    let (file, path, failure) = match primary.as_ref().map(|p| open(p)) {
        Some(Ok(file)) => (Some(file), primary.clone(), None),
        result => {
            let failure = match result { Some(Err(e)) => e.to_string(), _ => "LOCALAPPDATA unavailable".into() };
            match open(&fallback) {
                Ok(file) => (Some(file), Some(fallback.clone()), Some(failure)),
                Err(e) => { eprintln!("Startup diagnostics unavailable: {failure}; fallback: {e}"); (None, None, None) }
            }
        }
    };
    if let Some(file) = file {
        let _ = DIAGNOSTICS.set(Diagnostics { file: Mutex::new(file), fallback, started: Instant::now() });
        record(&format!("diagnostics initialized; file={}", path.map(|p| p.display().to_string()).unwrap_or_default()));
        if let Some(error) = failure { record(&format!("primary diagnostics path failed: {error}")); }
    }
    std::panic::set_hook(Box::new(|info| {
        let thread = std::thread::current();
        // Dynamic payloads can contain process commands, credentials or settings.
        let message = info.payload().downcast_ref::<&str>().copied().unwrap_or_else(|| {
            info.payload().downcast_ref::<String>()
                .map(String::as_str)
                .filter(|message| message.starts_with("state() called before manage() for "))
                .unwrap_or("dynamic panic payload omitted")
        });
        record(&format!("PANIC thread={} location={} message={}\n{}",
            thread.name().unwrap_or("unnamed"),
            info.location().map(|v| v.to_string()).unwrap_or_default(), message,
            std::backtrace::Backtrace::force_capture()));
    }));
}

pub fn record(message: &str) {
    let Some(diagnostics) = DIAGNOSTICS.get() else { eprintln!("{message}"); return; };
    let line = format!("[{}] [+{}ms] {}\n", chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
        diagnostics.started.elapsed().as_millis(), message);
    // A panic hook must never wait for a lock held by the panicking thread.
    if let Ok(mut file) = diagnostics.file.try_lock() {
        if file.write_all(line.as_bytes()).and_then(|_| file.flush()).is_ok() { return; }
    }
    if let Some(parent) = diagnostics.fallback.parent() { let _ = fs::create_dir_all(parent); }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&diagnostics.fallback) {
        if file.write_all(line.as_bytes()).and_then(|_| file.flush()).is_ok() { return; }
    }
    eprintln!("{line}");
}
