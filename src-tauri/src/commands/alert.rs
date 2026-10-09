use tauri::{AppHandle, Manager};

use crate::commands::panel::{animate_position, rightmost_work_area};

const ALERT_WIDTH: i32 = 320;
const ALERT_HEIGHT: i32 = 200;
const ALERT_MIN_HEIGHT: i32 = 120;
const ALERT_MAX_HEIGHT: i32 = 420;
const MAIN_WIDTH: i32 = 420;
const GAP: i32 = 12;

/// Compute the target X for the alert window.
/// - If the main panel is visible, dock the alert to the LEFT of the main panel.
/// - Otherwise dock it to the right edge of the screen.
fn compute_target_x(app: &AppHandle, screen_right: i32, main_visible: bool) -> i32 {
    if main_visible {
        if let Some(main) = app.get_webview_window("main") {
            if let Ok(pos) = main.outer_position() {
                return pos.x - ALERT_WIDTH - GAP;
            }
        }
        // fallback: assume main is docked at right edge
        return screen_right - MAIN_WIDTH - ALERT_WIDTH - GAP;
    }
    screen_right - ALERT_WIDTH
}

/// Internal: show the alert window at the correct position (right edge / left of main).
/// Called by the gold monitor when a threshold is breached, and by the `show_alert` command.
pub(crate) fn show_alert_inner(app: &AppHandle) {
    let Some(alert) = app.get_webview_window("alert") else { return };
    let Some((work_x, work_y, work_w, work_h)) = rightmost_work_area(app) else { return };

    let screen_right = work_x + work_w as i32;
    let screen_y = work_y;
    let screen_h = work_h as i32;

    let main_visible = app
        .get_webview_window("main")
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false);

    let x = compute_target_x(app, screen_right, main_visible);
    let y = screen_y + (screen_h / 2) - (ALERT_HEIGHT / 2);

    alert
        .set_size(tauri::PhysicalSize::new(ALERT_WIDTH as u32, ALERT_HEIGHT as u32))
        .ok();
    alert.set_position(tauri::PhysicalPosition::new(x, y)).ok();
    alert.show().ok();
}

#[tauri::command]
pub fn show_alert(app: AppHandle) -> Result<(), String> {
    show_alert_inner(&app);
    Ok(())
}

/// Resize the alert window to fit its content height (clamped), keeping it
/// docked at the correct X and vertically centered on screen.
/// Called by the frontend after rendering the alert content.
#[tauri::command]
pub fn resize_alert(app: AppHandle, height: f64) -> Result<(), String> {
    let Some(alert) = app.get_webview_window("alert") else { return Ok(()) };
    let Some((work_x, work_y, work_w, work_h)) = rightmost_work_area(&app) else { return Ok(()) };

    // Clamp content height to sane bounds.
    let h = (height.ceil() as i32).clamp(ALERT_MIN_HEIGHT, ALERT_MAX_HEIGHT);

    let screen_right = work_x + work_w as i32;
    let screen_y = work_y;
    let screen_h = work_h as i32;

    let main_visible = app
        .get_webview_window("main")
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false);

    let x = compute_target_x(&app, screen_right, main_visible);
    let y = screen_y + (screen_h / 2) - (h / 2);

    alert
        .set_size(tauri::PhysicalSize::new(ALERT_WIDTH as u32, h as u32))
        .ok();
    alert.set_position(tauri::PhysicalPosition::new(x, y)).ok();
    Ok(())
}

#[tauri::command]
pub fn hide_alert(app: AppHandle) -> Result<(), String> {
    if let Some(alert) = app.get_webview_window("alert") {
        alert.hide().ok();
    }
    Ok(())
}

/// Slide the alert window between "right edge" and "left of main panel"
/// when the main panel is summoned / collapsed. No-op if the alert is hidden.
#[tauri::command]
pub fn reposition_alert(app: AppHandle, main_visible: bool) -> Result<(), String> {
    reposition_alert_inner(&app, main_visible);
    Ok(())
}

pub(crate) fn reposition_alert_inner(app: &AppHandle, main_visible: bool) {
    let Some(alert) = app.get_webview_window("alert") else { return };
    if !alert.is_visible().unwrap_or(false) {
        return;
    }
    let Some((work_x, _, work_w, _)) = rightmost_work_area(app) else { return };
    let Ok(cur) = alert.outer_position() else { return };

    let screen_right = work_x + work_w as i32;
    let target_x = compute_target_x(app, screen_right, main_visible);

    if target_x != cur.x {
        animate_position(&alert, cur.x, target_x, cur.y, 150);
    }
}
