use std::fs;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

const FLOAT_WIDTH: i32 = 420;
const GAP: i32 = 40; // enough for 32px close button + padding
const BTN_SIZE: i32 = 32;

/// Desired end-state of the main panel (not `is_visible()`, which stays true
/// while the slide-out animation is still running).
static PANEL_OPEN: AtomicBool = AtomicBool::new(true);
/// Monotonic id for the **main** panel slide. Starting a new slide bumps it
/// and cancels the old one so a second toggle mid-slide reverses from the
/// current x.
static ANIM_GEN: AtomicU64 = AtomicU64::new(0);
/// Separate id for floating/alert animations so they never cancel a main
/// panel open/close mid-flight.
static AUX_ANIM_GEN: AtomicU64 = AtomicU64::new(0);

fn next_anim_gen() -> u64 {
    ANIM_GEN.fetch_add(1, Ordering::SeqCst) + 1
}

fn anim_cancelled(gen: u64) -> bool {
    ANIM_GEN.load(Ordering::SeqCst) != gen
}

fn next_aux_anim_gen() -> u64 {
    AUX_ANIM_GEN.fetch_add(1, Ordering::SeqCst) + 1
}

fn aux_anim_cancelled(gen: u64) -> bool {
    AUX_ANIM_GEN.load(Ordering::SeqCst) != gen
}

/// Return the display that owns the right edge of the virtual desktop.
///
/// The panel stays anchored to this display while hidden so Windows retains a
/// stable DPI coordinate system for the next reveal.
pub(crate) fn rightmost_monitor(app: &AppHandle) -> Option<tauri::Monitor> {
    app.available_monitors().ok()?.into_iter().max_by_key(|monitor| {
        let position = monitor.position();
        (position.x.saturating_add(monitor.size().width as i32), position.x)
    })
}

/// Usable work-area rect (excludes taskbar / status bar) of the rightmost
/// monitor, as (x, y, width, height) in physical pixels.
///
/// Full `monitor.size()` includes the taskbar band, so a full-height panel
/// ends up underneath the taskbar and its bottom content is unclickable.
pub(crate) fn rightmost_work_area(app: &AppHandle) -> Option<(i32, i32, u32, u32)> {
    let monitor = rightmost_monitor(app)?;
    let work = monitor.work_area();
    Some((
        work.position.x,
        work.position.y,
        work.size.width,
        work.size.height,
    ))
}

#[tauri::command]
pub fn toggle_panel(app: AppHandle) -> Result<bool, String> {
    let want_open = !PANEL_OPEN.load(Ordering::SeqCst);
    PANEL_OPEN.store(want_open, Ordering::SeqCst);
    if want_open {
        slide_in_main_sync(&app);
    } else {
        slide_out_main_sync(&app);
    }
    Ok(want_open)
}

#[tauri::command]
pub fn slide_out_main(app: AppHandle) -> Result<(), String> {
    PANEL_OPEN.store(false, Ordering::SeqCst);
    slide_out_main_sync(&app);
    Ok(())
}

#[tauri::command]
pub fn slide_in_main(app: AppHandle) -> Result<(), String> {
    PANEL_OPEN.store(true, Ordering::SeqCst);
    slide_in_main_sync(&app);
    Ok(())
}

fn slide_out_main_sync(app: &AppHandle) {
    // Close floating + close-btn windows first (no animation for them)
    if let Some(btn) = app.get_webview_window("close-btn") {
        btn.hide().ok();
    }
    if let Some(floating) = app.get_webview_window("floating") {
        floating.hide().ok();
    }

    let Some(win) = app.get_webview_window("main") else { return };
    let Ok(current_pos) = win.outer_position() else { return };
    let Some((_, _, work_w, _)) = rightmost_work_area(app) else { return };
    let Some(monitor) = rightmost_monitor(app) else { return };
    let work = monitor.work_area();

    // Park just inside the work-area right edge (not under a right-side taskbar).
    let screen_right = work.position.x + work_w as i32;
    // Keep the window's origin on the target display. Moving it past the edge
    // makes Windows reassign its DPI context (notably at 200% scaling), which
    // can shrink the restored webview. The final one-pixel sliver is hidden
    // immediately after the slide animation.
    let hidden_x = screen_right - 1;
    let y = current_pos.y;

    // Interruptible: a follow-up slide_in cancels this and reverses from
    // wherever we got to. Only hide if we actually reach the park position.
    let gen = next_anim_gen();
    let win_clone = win.clone();
    let app_clone = app.clone();
    thread::spawn(move || {
        let completed = animate_position_gen(&win_clone, current_pos.x, hidden_x, y, 200, gen);
        if completed && !PANEL_OPEN.load(Ordering::SeqCst) {
            win_clone.hide().ok();
            // Alert window stays visible (manual-close only); just shift it back.
            crate::commands::alert::reposition_alert_inner(&app_clone, false);
        }
    });
}

fn slide_in_main_sync(app: &AppHandle) {
    let Some(win) = app.get_webview_window("main") else { return };
    let Some(monitor) = rightmost_monitor(app) else { return };
    let Some((work_x, work_y, work_w, work_h)) = rightmost_work_area(app) else { return };

    // Width from the saved setting (single source of truth), converted to
    // physical px for this monitor — keeps the right edge flush regardless of
    // DPI scaling.
    let width = panel_width_physical(app, monitor.scale_factor());

    // Fit inside the work area so the taskbar never covers the panel.
    let screen_right = work_x + work_w as i32;
    let target_x = screen_right - width;
    // This is the same in-monitor parking coordinate used when the panel is
    // hidden. It deliberately never crosses into an adjacent monitor.
    let start_x = screen_right - 1;
    let y = work_y;
    let h = work_h;

    // If already fully open and idle, a second expand is a no-op snap.
    let Ok(cur) = win.outer_position() else { return };
    let from_x = if win.is_visible().unwrap_or(false) {
        cur.x
    } else {
        start_x
    };

    // Position first, then apply physical size. This makes Windows associate
    // the resize with the target monitor's DPI instead of the DPI it had while
    // hidden.
    win.set_position(tauri::PhysicalPosition::new(from_x, y)).ok();
    win.set_size(tauri::PhysicalSize::new(width as u32, h)).ok();
    win.show().ok();

    let gen = next_anim_gen();
    let win_clone = win.clone();
    let app_clone = app.clone();
    thread::spawn(move || {
        let completed = animate_position_gen(&win_clone, from_x, target_x, y, 200, gen);
        if completed {
            win_clone.set_focus().ok();
            crate::commands::alert::reposition_alert_inner(&app_clone, true);
        }
    });
}

/// Run a main-panel slide for `total_ms`. Returns `true` only if it reached
/// `to_x` without being cancelled by a newer main-panel animation.
pub(crate) fn animate_position_gen(
    win: &tauri::WebviewWindow,
    from_x: i32,
    to_x: i32,
    y: i32,
    total_ms: u64,
    gen: u64,
) -> bool {
    let step_delay = Duration::from_millis(8); // ~125fps target
    let steps = (total_ms / 8).max(1) as u32;
    for i in 1..=steps {
        if anim_cancelled(gen) {
            return false;
        }
        let progress = i as f64 / steps as f64;
        let eased = 1.0 - (1.0 - progress).powi(3);
        let x = from_x + ((to_x - from_x) as f64 * eased) as i32;
        win.set_position(tauri::PhysicalPosition::new(x, y)).ok();
        thread::sleep(step_delay);
    }
    if anim_cancelled(gen) {
        return false;
    }
    win.set_position(tauri::PhysicalPosition::new(to_x, y)).ok();
    true
}

/// Exclusive animation for floating/alert/close-btn (own cancel domain).
pub(crate) fn animate_position(
    win: &tauri::WebviewWindow,
    from_x: i32,
    to_x: i32,
    y: i32,
    total_ms: u64,
) {
    let gen = next_aux_anim_gen();
    let step_delay = Duration::from_millis(8);
    let steps = (total_ms / 8).max(1) as u32;
    for i in 1..=steps {
        if aux_anim_cancelled(gen) {
            return;
        }
        let progress = i as f64 / steps as f64;
        let eased = 1.0 - (1.0 - progress).powi(3);
        let x = from_x + ((to_x - from_x) as f64 * eased) as i32;
        win.set_position(tauri::PhysicalPosition::new(x, y)).ok();
        thread::sleep(step_delay);
    }
    if !aux_anim_cancelled(gen) {
        win.set_position(tauri::PhysicalPosition::new(to_x, y)).ok();
    }
}

#[tauri::command]
pub fn show_panel(app: AppHandle) -> Result<(), String> {
    PANEL_OPEN.store(true, Ordering::SeqCst);
    let win = app.get_webview_window("main").ok_or("Main window not found")?;
    win.show().map_err(|e| e.to_string())?;
    win.set_focus().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn hide_panel(app: AppHandle) -> Result<(), String> {
    PANEL_OPEN.store(false, Ordering::SeqCst);
    next_anim_gen(); // cancel any in-flight slide
    let win = app.get_webview_window("main").ok_or("Main window not found")?;
    win.hide().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn show_floating(app: AppHandle) -> Result<(), String> {
    let floating = app.get_webview_window("floating").ok_or("no floating")?;
    let main_win = app.get_webview_window("main").ok_or("no main")?;

    let Ok(main_pos) = main_win.outer_position() else { return Err("no pos".into()) };
    let Ok(main_size) = main_win.outer_size() else { return Err("no size".into()) };

    let monitor = rightmost_monitor(&app);
    let scale = monitor.as_ref().map(|m| m.scale_factor()).unwrap_or(1.0);
    let float_width = (FLOAT_WIDTH as f64 * scale).round() as i32;
    let gap = (GAP as f64 * scale).round() as i32;
    let target_x = main_pos.x - float_width - gap;
    let start_x = main_pos.x;
    let float_height = main_size.height / 2;
    // Vertically center on the work area (not under the taskbar).
    let (screen_y, screen_h) = rightmost_work_area(&app)
        .map(|(_, y, _, h)| (y, h))
        .unwrap_or((0, main_size.height));
    let y = screen_y + (screen_h as i32 / 2) - (float_height as i32 / 2);

    floating.set_size(tauri::PhysicalSize::new(float_width as u32, float_height)).ok();
    floating.set_position(tauri::PhysicalPosition::new(start_x, y)).ok();
    floating.show().ok();

    // Animate slide out
    animate_position(&floating, start_x, target_x, y, 150);
    floating.set_focus().ok();

    // Show close button between windows (centered on floating window)
    position_and_show_close_btn(&app, target_x + float_width, y, float_height, scale);

    Ok(())
}

#[tauri::command]
pub fn hide_floating(app: AppHandle) -> Result<(), String> {
    // Hide close button first
    if let Some(btn) = app.get_webview_window("close-btn") {
        btn.hide().ok();
    }

    let floating = app.get_webview_window("floating").ok_or("no floating")?;
    let Ok(current_pos) = floating.outer_position() else {
        floating.hide().ok();
        return Ok(());
    };

    let main_win = app.get_webview_window("main");
    let target_x = if let Some(mw) = main_win {
        if let Ok(mp) = mw.outer_position() { mp.x } else { current_pos.x + FLOAT_WIDTH + GAP }
    } else {
        current_pos.x + FLOAT_WIDTH + GAP
    };

    animate_position(&floating, current_pos.x, target_x, current_pos.y, 150);
    floating.hide().ok();

    // Focus main window and notify it that floating was closed
    if let Some(main) = app.get_webview_window("main") {
        main.emit("floating:closed", ()).ok();
        main.set_focus().ok();
    }

    Ok(())
}

#[tauri::command]
pub fn focus_main(app: AppHandle) -> Result<(), String> {
    if let Some(main) = app.get_webview_window("main") {
        main.set_focus().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Read the saved panel width (logical px, default 420) and convert to physical
/// pixels for the given monitor's scale factor. This keeps sizing/positioning
/// consistent regardless of display DPI scaling.
fn panel_width_physical(app: &AppHandle, scale: f64) -> i32 {
    let logical = tauri_plugin_store::StoreBuilder::new(app, "settings.json")
        .build()
        .ok()
        .and_then(|s| s.get("panel.width").and_then(|v| v.as_u64()))
        .map(|w| w as f64)
        .unwrap_or(420.0)
        .clamp(200.0, 2000.0);
    (logical * scale).round() as i32
}

/// Apply a new panel width immediately: resize the main window and re-dock it
/// to the right edge (keeping full monitor height). `width` is logical pixels.
#[tauri::command]
pub fn apply_panel_width(app: AppHandle, width: u32) -> Result<(), String> {
    let win = app.get_webview_window("main").ok_or("no main")?;
    let monitor = rightmost_monitor(&app).ok_or("no monitor")?;
    let (work_x, work_y, work_w, work_h) = rightmost_work_area(&app).ok_or("no work area")?;
    let logical = (width.clamp(200, 2000)) as f64;
    let scale = monitor.scale_factor();
    let w_phys = (logical * scale).round() as i32;
    let screen_right = work_x + work_w as i32;
    let x = screen_right - w_phys;
    let y = work_y;
    let h = work_h;
    win.set_size(tauri::PhysicalSize::new(w_phys as u32, h)).ok();
    win.set_position(tauri::PhysicalPosition::new(x, y)).ok();
    Ok(())
}

fn position_and_show_close_btn(app: &AppHandle, x: i32, y: i32, height: u32, scale: f64) {
    let Some(btn) = app.get_webview_window("close-btn") else { return };
    let btn_size = (BTN_SIZE as f64 * scale).round() as i32;
    let center_y = y + (height as i32 / 2) - (btn_size / 2);
    btn.set_position(tauri::PhysicalPosition::new(x, center_y)).ok();
    btn.set_size(tauri::PhysicalSize::new(btn_size as u32, btn_size as u32)).ok();
    btn.show().ok();
}

#[tauri::command]
pub fn broadcast_theme(app: AppHandle, is_dark: bool) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("main") {
        win.emit("theme:change", is_dark).ok();
    }
    if let Some(floating) = app.get_webview_window("floating") {
        floating.emit("theme:change", is_dark).ok();
    }
    Ok(())
}

/// Write theme to a plain file, bypassing store plugin cache
#[tauri::command]
pub fn write_theme(app: AppHandle, is_dark: bool) -> Result<(), String> {
    let path = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("theme.txt");
    fs::write(&path, if is_dark { "dark" } else { "light" }).map_err(|e| e.to_string())?;
    Ok(())
}

/// Read theme from a plain file
#[tauri::command]
pub fn read_theme(app: AppHandle) -> Result<String, String> {
    let path = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("theme.txt");
    if path.exists() {
        fs::read_to_string(&path).map_err(|e| e.to_string())
    } else {
        Ok("light".to_string())
    }
}

/// Exit the application completely
#[tauri::command]
pub fn exit_app(app: AppHandle) {
    if let Some(state) = app.try_state::<crate::modules::network_monitor::NetworkTrafficState>() {
        state.shutdown();
    }
    app.exit(0);
}

/// Restart the application.
#[tauri::command]
pub fn restart_app(app: AppHandle) {
    if let Some(state) = app.try_state::<crate::modules::network_monitor::NetworkTrafficState>() {
        state.shutdown();
    }
    app.restart();
}
