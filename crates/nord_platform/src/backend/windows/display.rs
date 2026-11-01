//! Win32 monitor enumeration kept behind the platform-neutral display API.
#![allow(unsafe_code)]

use crate::display::{DisplayError, DisplayMonitor, DisplayRect};
use windows_sys::Win32::Foundation::{GetLastError, LPARAM, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::MONITORINFOF_PRIMARY;
use windows_sys::core::BOOL;

#[derive(Default)]
struct Enumeration {
    monitors: Vec<DisplayMonitor>,
    native_error: Option<u32>,
    callback_panicked: bool,
}

pub(crate) struct WindowsDisplayBackend;

impl WindowsDisplayBackend {
    pub(crate) fn enumerate() -> Result<Vec<DisplayMonitor>, DisplayError> {
        let mut context = Enumeration::default();
        let callback_data = LPARAM::try_from((&raw mut context).addr())
            .map_err(|_| DisplayError::CallbackFailed)?;
        // SAFETY: callback data points to `context`, which remains alive until the synchronous
        // EnumDisplayMonitors call returns. A null HDC and clip rectangle enumerate all monitors.
        let success = unsafe {
            EnumDisplayMonitors(
                std::ptr::null_mut(),
                std::ptr::null(),
                Some(monitor_callback),
                callback_data,
            )
        };
        if success == 0 {
            if let Some(code) = context.native_error {
                return Err(DisplayError::Native { operation: "GetMonitorInfoW", code });
            }
            if context.callback_panicked {
                return Err(DisplayError::CallbackFailed);
            }
            // SAFETY: GetLastError reads this thread's error state after the failed Win32 call.
            return Err(DisplayError::Native {
                operation: "EnumDisplayMonitors",
                code: unsafe { GetLastError() },
            });
        }
        Ok(context.monitors)
    }
}

unsafe extern "system" fn monitor_callback(
    monitor: HMONITOR,
    _device_context: HDC,
    _monitor_rect: *mut RECT,
    data: LPARAM,
) -> BOOL {
    // FFI callbacks must never unwind across the Windows ABI boundary.
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: `data` is the live Enumeration pointer supplied by enumerate().
        let context = unsafe { &mut *(data as *mut Enumeration) };
        // SAFETY: zero initialization is valid for MONITORINFOEXW; cbSize is set before use.
        let mut info: MONITORINFOEXW = unsafe { std::mem::zeroed() };
        info.monitorInfo.cbSize =
            u32::try_from(std::mem::size_of::<MONITORINFOEXW>()).unwrap_or(u32::MAX);
        // SAFETY: monitor is the current valid callback HMONITOR and info has the required size.
        if unsafe { GetMonitorInfoW(monitor, &raw mut info.monitorInfo) } == 0 {
            // SAFETY: reads the error from the failed API call on this callback thread.
            context.native_error = Some(unsafe { GetLastError() });
            return false;
        }

        let device_name_end =
            info.szDevice.iter().position(|unit| *unit == 0).unwrap_or(info.szDevice.len());
        context.monitors.push(DisplayMonitor::from_native(
            String::from_utf16_lossy(&info.szDevice[..device_name_end]),
            DisplayRect::from_native(info.monitorInfo.rcMonitor),
            DisplayRect::from_native(info.monitorInfo.rcWork),
            info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
        ));
        true
    }));

    match outcome {
        Ok(true) => 1,
        Ok(false) => 0,
        Err(_) => {
            // SAFETY: `data` remains the callback context for the synchronous enumeration call.
            unsafe {
                (*(data as *mut Enumeration)).callback_panicked = true;
            }
            0
        }
    }
}
