use crate::state::PinnedWindow;
use chrono::Local;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_BORDER_COLOR};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowLongW, GetWindowTextW,
    SetWindowPos,
    GWL_EXSTYLE, HWND_NOTOPMOST, HWND_TOPMOST,
    SWP_NOMOVE, SWP_NOSIZE, WS_EX_TOPMOST,
};

const PIN_BORDER_COLOR: u32 = 0x00FF90_00; // BGR: 蓝色 (0x00BBGGRR)

/// 切换当前焦点窗口的置顶状态
/// skip_titles: 这些标题的窗口不允许取消置顶（如主面板）
pub fn toggle_pin_focused(skip_titles: &[&str]) -> Result<Option<PinnedWindow>, String> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() {
        return Err("无法获取焦点窗口".to_string());
    }

    let is_topmost = is_window_topmost(hwnd);
    let title = get_window_title(hwnd);

    if title.is_empty() {
        return Err("无法获取窗口标题".to_string());
    }

    // 跳过不允许操作的窗口
    if skip_titles.iter().any(|&t| title.contains(t)) {
        return Err("此窗口不允许置顶操作".to_string());
    }

    if is_topmost {
        unsafe {
            SetWindowPos(hwnd, Some(HWND_NOTOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE)
                .map_err(|e| format!("SetWindowPos 失败: {}", e))?;
        }
        clear_border_color(hwnd);
        Ok(None)
    } else {
        unsafe {
            SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE)
                .map_err(|e| format!("SetWindowPos 失败: {}", e))?;
        }
        set_border_color(hwnd);
        Ok(Some(PinnedWindow {
            hwnd: hwnd.0 as isize,
            title,
            pinned_at: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        }))
    }
}

pub fn unpin_window(hwnd_raw: isize) -> Result<(), String> {
    let hwnd = HWND(hwnd_raw as *mut _);
    unsafe {
        SetWindowPos(hwnd, Some(HWND_NOTOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE)
            .map_err(|e| format!("SetWindowPos 失败: {}", e))?;
    }
    clear_border_color(hwnd);
    Ok(())
}

pub fn unpin_all(hwnds: &[isize]) {
    for &hwnd_raw in hwnds {
        let hwnd = HWND(hwnd_raw as *mut _);
        unsafe {
            SetWindowPos(hwnd, Some(HWND_NOTOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE).ok();
        }
        clear_border_color(hwnd);
    }
}

pub fn is_window_topmost(hwnd: HWND) -> bool {
    let ex_style = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) };
    (ex_style & WS_EX_TOPMOST.0 as i32) != 0
}

pub fn get_window_title(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let len = unsafe { GetWindowTextW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..len as usize])
}

fn set_border_color(hwnd: HWND) {
    let color: u32 = PIN_BORDER_COLOR;
    unsafe {
        DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &color as *const _ as *const _, std::mem::size_of::<u32>() as u32).ok();
    }
}

fn clear_border_color(hwnd: HWND) {
    let color: u32 = 0x00FFFFFF;
    unsafe {
        DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &color as *const _ as *const _, std::mem::size_of::<u32>() as u32).ok();
    }
}
