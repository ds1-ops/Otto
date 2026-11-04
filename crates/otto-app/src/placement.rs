//! Fits the main window to its monitor's work area: the whole screen except the taskbar.
//!
//! The window opens hidden, is sized to the work area here, and is then shown. After that
//! it is an ordinary window: move it, resize it, maximize it.

/// Sizes the window to its monitor's work area and shows it.
#[cfg(windows)]
pub fn fit_to_work_area(window: &dyn iced::window::Window) {
    use iced::window::raw_window_handle::RawWindowHandle;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
    use windows::Win32::UI::WindowsAndMessaging::{SetForegroundWindow, SetWindowPos, ShowWindow, HWND_TOP, SWP_NOACTIVATE, SW_SHOWNORMAL};

    let Ok(handle) = window.window_handle() else { return };
    let RawWindowHandle::Win32(h) = handle.as_raw() else { return };
    let hwnd = HWND(h.hwnd.get() as *mut core::ffi::c_void);
    // SAFETY: `hwnd` is the live window iced hands us for the duration of this call.
    unsafe {
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return;
        }
        let work = info.rcWork;
        let _ = SetWindowPos(hwnd, Some(HWND_TOP), work.left, work.top, work.right - work.left, work.bottom - work.top, SWP_NOACTIVATE);
        // When started by another program (e.g. Claude desktop opening it in the background)
        // Windows applies that program's "start hidden" to the first show; the second one counts.
        let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
        let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
        let _ = SetForegroundWindow(hwnd);
    }
}

#[cfg(not(windows))]
pub fn fit_to_work_area(_window: &dyn iced::window::Window) {}
