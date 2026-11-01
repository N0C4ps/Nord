//! Thin Win32 window backend. All unsafe code is isolated at the OS boundary.
#![allow(unsafe_code)]

use crate::{
    ApplicationLifecycleEvent, EventEmitter, InputEvent, KeyCode, Modifiers, PlatformEventKind,
    PointerButton, WindowConfig, WindowError, WindowEvent, WindowHandle,
};
use raw_window_handle::{RawWindowHandle, Win32WindowHandle};
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, OnceLock};
use windows_sys::Win32::Foundation::{GetLastError, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{ClientToScreen, HBRUSH, UpdateWindow};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows_sys::Win32::UI::Input::{
    GetRawInputData, RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER, RID_INPUT, RIDEV_REMOVE,
    RIM_TYPEMOUSE, RegisterRawInputDevices,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, CreateWindowExW, DefWindowProcW,
    DestroyWindow, DispatchMessageW, GWLP_USERDATA, GetClientRect, GetCursorPos, GetWindowLongPtrW,
    HTCLIENT, IDC_ARROW, LoadCursorW, MSG, PM_REMOVE, PeekMessageW, RegisterClassExW, SW_HIDE,
    SW_SHOW, SWP_NOACTIVATE, SWP_NOZORDER, SetCursor, SetCursorPos, SetWindowLongPtrW,
    SetWindowPos, ShowWindow, TranslateMessage, WHEEL_DELTA, WM_CLOSE, WM_DPICHANGED, WM_INPUT,
    WM_KEYDOWN, WM_KEYUP, WM_KILLFOCUS, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP,
    WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCREATE, WM_NCDESTROY, WM_RBUTTONDOWN,
    WM_RBUTTONUP, WM_SETCURSOR, WM_SETFOCUS, WM_SIZE, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_XBUTTONDOWN,
    WM_XBUTTONUP, WNDCLASSEXW, WS_MAXIMIZEBOX, WS_OVERLAPPEDWINDOW, WS_THICKFRAME, WS_VISIBLE,
    XBUTTON1,
};

const CLASS_NAME: &[u16] = &[78, 111, 114, 100, 84, 101, 99, 104, 87, 105, 110, 100, 111, 119, 0];

struct WindowContext {
    handle: WindowHandle,
    events: EventEmitter,
    native_reference_released: std::sync::atomic::AtomicBool,
    cursor_locked: AtomicBool,
    saved_cursor: [AtomicI32; 2],
    has_saved_cursor: AtomicBool,
}

struct WindowRecord {
    hwnd: isize,
    context: Arc<WindowContext>,
}

#[derive(Default)]
pub(crate) struct WindowsWindowBackend {
    windows: HashMap<WindowHandle, WindowRecord>,
}

impl WindowsWindowBackend {
    pub(crate) fn raw_handle(&self, handle: WindowHandle) -> Result<RawWindowHandle, WindowError> {
        let hwnd = self.windows.get(&handle).ok_or(WindowError::UnknownHandle)?.hwnd;
        let hwnd = std::num::NonZeroIsize::new(hwnd).ok_or(WindowError::UnknownHandle)?;
        Ok(RawWindowHandle::Win32(Win32WindowHandle::new(hwnd)))
    }

    pub(crate) fn create(
        &mut self,
        config: &WindowConfig,
        events: &EventEmitter,
    ) -> Result<WindowHandle, WindowError> {
        ensure_class()?;
        let handle = events.allocate_window_handle().map_err(WindowError::Event)?;
        let context = Arc::new(WindowContext {
            handle,
            events: events.clone(),
            native_reference_released: std::sync::atomic::AtomicBool::new(false),
            cursor_locked: AtomicBool::new(false),
            saved_cursor: [AtomicI32::new(0), AtomicI32::new(0)],
            has_saved_cursor: AtomicBool::new(false),
        });
        // The raw Arc is transferred to the HWND and released by WM_NCDESTROY.
        let native_context = Arc::into_raw(Arc::clone(&context));
        let title: Vec<u16> = config.title().encode_utf16().chain(std::iter::once(0)).collect();
        let style = if config.resizable() {
            WS_OVERLAPPEDWINDOW
        } else {
            WS_OVERLAPPEDWINDOW & !WS_THICKFRAME & !WS_MAXIMIZEBOX
        };
        let width = i32::try_from(config.width()).map_err(|_| WindowError::InvalidDimensions)?;
        let height = i32::try_from(config.height()).map_err(|_| WindowError::InvalidDimensions)?;
        let mut rect = RECT { left: 0, top: 0, right: width, bottom: height };
        // SAFETY: `rect` is valid writable storage and styles match the created top-level window.
        if unsafe { AdjustWindowRectEx(&raw mut rect, style, 0, 0) } == 0 {
            // SAFETY: CreateWindowExW did not take ownership because it has not been called yet.
            unsafe {
                drop(Arc::from_raw(native_context));
            }
            return Err(native_error("AdjustWindowRectEx"));
        }
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        // SAFETY: all strings are NUL-terminated for this call; native_context remains alive
        // through WM_NCCREATE and until WM_NCDESTROY releases its transferred Arc reference.
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                CLASS_NAME.as_ptr(),
                title.as_ptr(),
                style | if config.visible() { WS_VISIBLE } else { 0 },
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                width,
                height,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                GetModuleHandleW(std::ptr::null()),
                native_context.cast::<c_void>(),
            )
        };
        if hwnd.is_null() {
            // CreateWindowExW failure can occur before WM_NCCREATE; if that message did run,
            // WM_NCDESTROY already released the transferred reference. Avoid double-free by
            // relying on the class procedure's cleanup flag attached below.
            let native_reference_released =
                context.native_reference_released.load(std::sync::atomic::Ordering::Acquire);
            if !native_reference_released {
                // SAFETY: this raw strong reference was transferred above and was not consumed.
                unsafe {
                    drop(Arc::from_raw(native_context));
                }
            }
            return Err(native_error("CreateWindowExW"));
        }
        if config.visible() {
            // SAFETY: hwnd is a live window created on this thread.
            unsafe {
                ShowWindow(hwnd, SW_SHOW);
                UpdateWindow(hwnd);
            }
        }
        self.windows.insert(handle, WindowRecord { hwnd: hwnd as isize, context });
        Ok(handle)
    }

    pub(crate) fn destroy(&mut self, handle: WindowHandle) -> Result<(), WindowError> {
        let record = self.windows.get(&handle).ok_or(WindowError::UnknownHandle)?;
        // SAFETY: hwnd is owned by this backend and destruction is checked to occur on its owner thread.
        if unsafe { DestroyWindow(record.hwnd as HWND) } == 0 {
            return Err(native_error("DestroyWindow"));
        }
        self.windows.remove(&handle);
        Ok(())
    }

    pub(crate) fn set_visible(
        &mut self,
        handle: WindowHandle,
        visible: bool,
    ) -> Result<(), WindowError> {
        let record = self.windows.get(&handle).ok_or(WindowError::UnknownHandle)?;
        // SAFETY: hwnd is owned by this backend and access is on its owner thread.
        unsafe {
            ShowWindow(record.hwnd as HWND, if visible { SW_SHOW } else { SW_HIDE });
        }
        Ok(())
    }

    pub(crate) fn set_cursor_locked(
        &mut self,
        handle: WindowHandle,
        locked: bool,
    ) -> Result<(), WindowError> {
        let record = self.windows.get(&handle).ok_or(WindowError::UnknownHandle)?;
        let hwnd = record.hwnd as HWND;
        let context = &record.context;
        if locked {
            if !context.cursor_locked.load(Ordering::Acquire) {
                let mut saved = windows_sys::Win32::Foundation::POINT::default();
                // SAFETY: `saved` points to writable storage for the current desktop position.
                if unsafe { GetCursorPos(&raw mut saved) } == 0 {
                    return Err(native_error("GetCursorPos"));
                }
                context.saved_cursor[0].store(saved.x, Ordering::Relaxed);
                context.saved_cursor[1].store(saved.y, Ordering::Relaxed);
                context.has_saved_cursor.store(true, Ordering::Release);
            }

            let device =
                RAWINPUTDEVICE { usUsagePage: 0x01, usUsage: 0x02, dwFlags: 0, hwndTarget: hwnd };
            // SAFETY: device is initialized and its mouse usage/target are valid.
            if unsafe {
                RegisterRawInputDevices(
                    &raw const device,
                    1,
                    u32::try_from(std::mem::size_of::<RAWINPUTDEVICE>()).unwrap_or(u32::MAX),
                )
            } == 0
            {
                context.has_saved_cursor.store(false, Ordering::Release);
                return Err(native_error("RegisterRawInputDevices"));
            }
            let (clip, center) = match cursor_geometry(hwnd) {
                Ok(geometry) => geometry,
                Err(error) => {
                    unregister_raw_mouse();
                    context.has_saved_cursor.store(false, Ordering::Release);
                    return Err(error);
                }
            };
            // SAFETY: clip is a valid screen-space RECT and remains valid for the call.
            if unsafe { windows_sys::Win32::UI::WindowsAndMessaging::ClipCursor(&raw const clip) }
                == 0
            {
                unregister_raw_mouse();
                context.has_saved_cursor.store(false, Ordering::Release);
                return Err(native_error("ClipCursor"));
            }
            // Set state before moving so WM_SETCURSOR immediately hides the cursor.
            context.cursor_locked.store(true, Ordering::Release);
            // SAFETY: center is a desktop pixel within this window's client rect.
            if unsafe { SetCursorPos(center.x, center.y) } == 0 {
                release_cursor(hwnd, context, true);
                return Err(native_error("SetCursorPos"));
            }
            // SAFETY: a null cursor intentionally hides it while captured; WM_SETCURSOR reapplies it.
            unsafe { SetCursor(std::ptr::null_mut()) };
            Ok(())
        } else {
            release_cursor(hwnd, context, true);
            Ok(())
        }
    }

    pub(crate) fn pump_messages(&mut self) {
        let mut message = MSG::default();
        // PeekMessageW with PM_REMOVE pumps all current messages without waiting for input.
        // SAFETY: message is valid writable storage and a null HWND selects this thread's queue.
        while unsafe { PeekMessageW(&raw mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) } != 0
        {
            if message.message == windows_sys::Win32::UI::WindowsAndMessaging::WM_QUIT {
                if let Some(window) = self.windows.values().next() {
                    let _ = window.context.events.emit(PlatformEventKind::Lifecycle(
                        ApplicationLifecycleEvent::QuitRequested,
                    ));
                }
                continue;
            }
            // SAFETY: message came from this thread's queue and is dispatched using Win32's documented pair.
            unsafe {
                TranslateMessage(&raw const message);
                DispatchMessageW(&raw const message);
            }
        }
    }

    pub(crate) fn destroy_all(&mut self) {
        for (_, record) in self.windows.drain() {
            // SAFETY: WindowSystem calls this cleanup only on the thread that created the HWNDs.
            unsafe {
                DestroyWindow(record.hwnd as HWND);
            }
        }
    }
}

static CLASS_REGISTERED: OnceLock<Result<(), u32>> = OnceLock::new();

fn ensure_class() -> Result<(), WindowError> {
    match CLASS_REGISTERED.get_or_init(|| {
        // SAFETY: zero initialization is valid for WNDCLASSEXW; every required field is filled below.
        let mut class: WNDCLASSEXW = unsafe { std::mem::zeroed() };
        class.cbSize = u32::try_from(std::mem::size_of::<WNDCLASSEXW>()).unwrap_or(u32::MAX);
        class.style = CS_HREDRAW | CS_VREDRAW;
        class.lpfnWndProc = Some(window_proc);
        // SAFETY: module handle and class name are valid for the process lifetime.
        class.hInstance = unsafe { GetModuleHandleW(std::ptr::null()) };
        // SAFETY: IDC_ARROW is a system cursor resource and requires no owned module handle.
        class.hCursor = unsafe { LoadCursorW(std::ptr::null_mut(), IDC_ARROW) };
        class.lpszClassName = CLASS_NAME.as_ptr();
        // No background brush: the renderer owns surface clearing and avoids GDI erase flicker.
        class.hbrBackground = std::ptr::null_mut::<c_void>() as HBRUSH;
        // SAFETY: class structure and its static class name live through the registration call.
        if unsafe { RegisterClassExW(&raw const class) } == 0 {
            Err(unsafe { GetLastError() })
        } else {
            Ok(())
        }
    }) {
        Ok(()) => Ok(()),
        Err(code) => Err(WindowError::Native { operation: "RegisterClassExW", code: *code }),
    }
}

fn native_error(operation: &'static str) -> WindowError {
    // SAFETY: GetLastError has no preconditions and reads the calling thread's Win32 error state.
    WindowError::Native { operation, code: unsafe { GetLastError() } }
}

#[allow(clippy::too_many_lines)] // Native messages share this single Win32 dispatch boundary.
unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: for WM_NCCREATE, Win32 provides a valid CREATESTRUCTW through lParam.
    if message == WM_NCCREATE {
        let create = unsafe {
            &*(lparam as *const windows_sys::Win32::UI::WindowsAndMessaging::CREATESTRUCTW)
        };
        let context = create.lpCreateParams.cast::<WindowContext>();
        if !context.is_null() {
            // SAFETY: pointer originates from Arc::into_raw in create() and remains owned by HWND.
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, context as isize);
            }
        }
    }
    // SAFETY: the userdata slot is set only by WM_NCCREATE to a live WindowContext pointer.
    let context_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const WindowContext };
    let context = if context_ptr.is_null() {
        None
    } else {
        // SAFETY: native Arc strong reference remains alive until WM_NCDESTROY below.
        Some(unsafe { &*context_ptr })
    };

    if let Some(context) = context {
        match message {
            WM_SIZE => {
                let packed = lparam.to_ne_bytes();
                let width = u32::from(u16::from_ne_bytes([packed[0], packed[1]]));
                let height = u32::from(u16::from_ne_bytes([packed[2], packed[3]]));
                let _ = context.events.emit(PlatformEventKind::Window {
                    window: context.handle,
                    event: WindowEvent::Resized { width, height },
                });
                if context.cursor_locked.load(Ordering::Acquire)
                    && let Ok((clip, center)) = cursor_geometry(hwnd)
                {
                    // SAFETY: the client bounds and center were converted to desktop coordinates.
                    unsafe {
                        windows_sys::Win32::UI::WindowsAndMessaging::ClipCursor(&raw const clip);
                        SetCursorPos(center.x, center.y);
                    }
                }
            }
            WM_SETFOCUS | WM_KILLFOCUS => {
                if message == WM_KILLFOCUS {
                    release_cursor(hwnd, context, true);
                }
                let _ = context.events.emit(PlatformEventKind::Window {
                    window: context.handle,
                    event: WindowEvent::FocusChanged { focused: message == WM_SETFOCUS },
                });
            }
            WM_CLOSE => {
                let _ = context.events.emit(PlatformEventKind::Window {
                    window: context.handle,
                    event: WindowEvent::CloseRequested,
                });
                return 0;
            }
            WM_KEYDOWN | WM_KEYUP | WM_SYSKEYDOWN | WM_SYSKEYUP => {
                emit_keyboard_input(context, message, wparam, lparam);
            }
            WM_INPUT => {
                if context.cursor_locked.load(Ordering::Acquire) {
                    emit_raw_mouse_delta(context, lparam);
                    if let Ok((_, center)) = cursor_geometry(hwnd) {
                        // SAFETY: center is a valid desktop location inside this client area.
                        unsafe { SetCursorPos(center.x, center.y) };
                    }
                }
            }
            WM_SETCURSOR
                if context.cursor_locked.load(Ordering::Acquire)
                    && u32::from(u16::from_ne_bytes([
                        lparam.to_ne_bytes()[0],
                        lparam.to_ne_bytes()[1],
                    ])) == HTCLIENT =>
            {
                // SAFETY: null is the documented way to suppress the cursor image.
                unsafe { SetCursor(std::ptr::null_mut()) };
                return 1;
            }
            WM_MOUSEMOVE | WM_LBUTTONDOWN | WM_LBUTTONUP | WM_RBUTTONDOWN | WM_RBUTTONUP
            | WM_MBUTTONDOWN | WM_MBUTTONUP | WM_XBUTTONDOWN | WM_XBUTTONUP | WM_MOUSEWHEEL
            | WM_MOUSEHWHEEL => emit_mouse_input(context, message, wparam, lparam),
            WM_DPICHANGED => {
                let dpi = u32::try_from((wparam >> 16) & 0xFFFF).unwrap_or(96);
                let _ = context.events.emit(PlatformEventKind::Window {
                    window: context.handle,
                    event: WindowEvent::ScaleFactorChanged { scale_factor: f64::from(dpi) / 96.0 },
                });
                // Apply the suggested rectangle supplied by Windows after a monitor DPI transition.
                let suggested = lparam as *const RECT;
                if !suggested.is_null() {
                    // SAFETY: WM_DPICHANGED guarantees this pointer refers to a valid RECT for this callback.
                    let rect = unsafe { &*suggested };
                    // SAFETY: hwnd is live; suggested bounds are the OS-recommended target bounds.
                    unsafe {
                        SetWindowPos(
                            hwnd,
                            std::ptr::null_mut(),
                            rect.left,
                            rect.top,
                            rect.right - rect.left,
                            rect.bottom - rect.top,
                            SWP_NOZORDER | SWP_NOACTIVATE,
                        );
                    }
                }
            }
            WM_NCDESTROY => {
                release_cursor(hwnd, context, true);
                // Clear before dropping the native strong reference so reentrant messages cannot observe it.
                // SAFETY: hwnd is live during WM_NCDESTROY and the slot contains our Arc pointer.
                unsafe {
                    let raw = SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) as *const WindowContext;
                    if !raw.is_null() {
                        (*raw)
                            .native_reference_released
                            .store(true, std::sync::atomic::Ordering::Release);
                        drop(Arc::from_raw(raw));
                    }
                }
            }
            _ => {}
        }
    }
    // SAFETY: default processing preserves standard Win32 behavior for every unhandled message.
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

fn cursor_geometry(
    hwnd: HWND,
) -> Result<(RECT, windows_sys::Win32::Foundation::POINT), WindowError> {
    let mut client = RECT::default();
    // SAFETY: client is writable and hwnd is the live window being processed.
    if unsafe { GetClientRect(hwnd, &raw mut client) } == 0 {
        return Err(native_error("GetClientRect"));
    }
    if client.right <= client.left || client.bottom <= client.top {
        return Err(WindowError::InvalidDimensions);
    }
    let mut top_left = windows_sys::Win32::Foundation::POINT { x: client.left, y: client.top };
    let mut bottom_right =
        windows_sys::Win32::Foundation::POINT { x: client.right, y: client.bottom };
    // SAFETY: both points are valid client coordinates and become screen coordinates on success.
    if unsafe { ClientToScreen(hwnd, &raw mut top_left) } == 0
        || unsafe { ClientToScreen(hwnd, &raw mut bottom_right) } == 0
    {
        return Err(native_error("ClientToScreen"));
    }
    let center = windows_sys::Win32::Foundation::POINT {
        x: top_left.x + (bottom_right.x - top_left.x) / 2,
        y: top_left.y + (bottom_right.y - top_left.y) / 2,
    };
    Ok((
        RECT { left: top_left.x, top: top_left.y, right: bottom_right.x, bottom: bottom_right.y },
        center,
    ))
}

fn unregister_raw_mouse() {
    let device = RAWINPUTDEVICE {
        usUsagePage: 0x01,
        usUsage: 0x02,
        dwFlags: RIDEV_REMOVE,
        hwndTarget: std::ptr::null_mut(),
    };
    // SAFETY: RIDEV_REMOVE requires a null target and removes this process's mouse registration.
    unsafe {
        RegisterRawInputDevices(
            &raw const device,
            1,
            u32::try_from(std::mem::size_of::<RAWINPUTDEVICE>()).unwrap_or(u32::MAX),
        );
    }
}

fn release_cursor(hwnd: HWND, context: &WindowContext, restore_position: bool) {
    let was_locked = context.cursor_locked.swap(false, Ordering::AcqRel);
    if was_locked {
        // SAFETY: null releases the process-wide clipping rectangle as required by Win32.
        unsafe { windows_sys::Win32::UI::WindowsAndMessaging::ClipCursor(std::ptr::null()) };
        unregister_raw_mouse();
        if restore_position && context.has_saved_cursor.swap(false, Ordering::AcqRel) {
            // SAFETY: coordinates were saved from GetCursorPos on capture.
            unsafe {
                SetCursorPos(
                    context.saved_cursor[0].load(Ordering::Relaxed),
                    context.saved_cursor[1].load(Ordering::Relaxed),
                );
            }
        } else {
            context.has_saved_cursor.store(false, Ordering::Release);
        }
        // SAFETY: restore the standard arrow used by the registered window class.
        unsafe { SetCursor(LoadCursorW(std::ptr::null_mut(), IDC_ARROW)) };
        let _ = hwnd;
    }
}

fn emit_raw_mouse_delta(context: &WindowContext, raw_handle: LPARAM) {
    let mut raw = std::mem::MaybeUninit::<RAWINPUT>::zeroed();
    let mut size = u32::try_from(std::mem::size_of::<RAWINPUT>()).unwrap_or(u32::MAX);
    // SAFETY: RAWINPUT is DWORD aligned and size describes its writable storage; lParam is the WM_INPUT handle.
    let read = unsafe {
        GetRawInputData(
            raw_handle as windows_sys::Win32::UI::Input::HRAWINPUT,
            RID_INPUT,
            raw.as_mut_ptr().cast(),
            &raw mut size,
            u32::try_from(std::mem::size_of::<RAWINPUTHEADER>()).unwrap_or(u32::MAX),
        )
    };
    if read == u32::MAX || usize::try_from(read).unwrap_or(0) < std::mem::size_of::<RAWINPUT>() {
        return;
    }
    // SAFETY: GetRawInputData returned at least the full RAWINPUT size above.
    let raw = unsafe { raw.assume_init() };
    if raw.header.dwType != RIM_TYPEMOUSE {
        return;
    }
    // SAFETY: dwType == RIM_TYPEMOUSE selects the mouse member of the RAWINPUT union.
    let mouse = unsafe { raw.data.mouse };
    let delta_x = f64::from(mouse.lLastX);
    let delta_y = f64::from(mouse.lLastY);
    if delta_x != 0.0 || delta_y != 0.0 {
        let _ = context.events.emit(PlatformEventKind::Input(InputEvent::PointerDelta {
            window: context.handle,
            delta_x,
            delta_y,
        }));
    }
}

fn emit_keyboard_input(context: &WindowContext, message: u32, wparam: WPARAM, lparam: LPARAM) {
    let key = map_virtual_key(u32::try_from(wparam).unwrap_or(u32::MAX));
    let pressed = matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN);
    let repeat = pressed && ((lparam.to_ne_bytes()[3] & 0x40) != 0);
    let _ = context.events.emit(PlatformEventKind::Input(InputEvent::Key {
        window: context.handle,
        key,
        pressed,
        repeat,
        modifiers: current_modifiers(),
    }));
}

fn emit_mouse_input(context: &WindowContext, message: u32, wparam: WPARAM, lparam: LPARAM) {
    match message {
        WM_MOUSEMOVE => {
            let coords = lparam.to_ne_bytes();
            let x = f64::from(i16::from_ne_bytes([coords[0], coords[1]]));
            let y = f64::from(i16::from_ne_bytes([coords[2], coords[3]]));
            let _ = context.events.emit(PlatformEventKind::Input(InputEvent::PointerMoved {
                window: context.handle,
                x,
                y,
            }));
        }
        WM_LBUTTONDOWN | WM_LBUTTONUP => {
            emit_pointer_button(context, PointerButton::Primary, message == WM_LBUTTONDOWN);
        }
        WM_RBUTTONDOWN | WM_RBUTTONUP => {
            emit_pointer_button(context, PointerButton::Secondary, message == WM_RBUTTONDOWN);
        }
        WM_MBUTTONDOWN | WM_MBUTTONUP => {
            emit_pointer_button(context, PointerButton::Middle, message == WM_MBUTTONDOWN);
        }
        WM_XBUTTONDOWN | WM_XBUTTONUP => {
            let button = if ((wparam >> 16) & 0xFFFF) == usize::from(XBUTTON1) {
                PointerButton::Auxiliary(1)
            } else {
                PointerButton::Auxiliary(2)
            };
            emit_pointer_button(context, button, message == WM_XBUTTONDOWN);
        }
        WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
            let packed = (wparam >> 16).to_ne_bytes();
            let delta =
                f64::from(i16::from_ne_bytes([packed[0], packed[1]])) / f64::from(WHEEL_DELTA);
            let (delta_x, delta_y) =
                if message == WM_MOUSEHWHEEL { (delta, 0.0) } else { (0.0, delta) };
            let _ = context.events.emit(PlatformEventKind::Input(InputEvent::PointerWheel {
                window: context.handle,
                delta_x,
                delta_y,
            }));
        }
        _ => {}
    }
}

fn emit_pointer_button(context: &WindowContext, button: PointerButton, pressed: bool) {
    let _ = context.events.emit(PlatformEventKind::Input(InputEvent::PointerButton {
        window: context.handle,
        button,
        pressed,
    }));
}

fn map_virtual_key(virtual_key: u32) -> KeyCode {
    match virtual_key {
        0x08 => KeyCode::Backspace,
        0x09 => KeyCode::Tab,
        0x0D => KeyCode::Enter,
        0x1B => KeyCode::Escape,
        0x20 => KeyCode::Space,
        0x25 => KeyCode::ArrowLeft,
        0x26 => KeyCode::ArrowUp,
        0x27 => KeyCode::ArrowRight,
        0x28 => KeyCode::ArrowDown,
        0x30..=0x39 => KeyCode::Digit(u8::try_from(virtual_key - 0x30).unwrap_or_default()),
        0x41..=0x5A => KeyCode::Letter(char::from_u32(virtual_key).unwrap_or('\0')),
        0x70..=0x87 => KeyCode::Function(u8::try_from(virtual_key - 0x6F).unwrap_or_default()),
        _ => KeyCode::Unknown,
    }
}

fn current_modifiers() -> Modifiers {
    let mut modifiers = Modifiers::EMPTY;
    // SAFETY: GetKeyState has no pointer preconditions; virtual-key constants are valid.
    unsafe {
        if GetKeyState(i32::from(VK_SHIFT)) < 0 {
            modifiers = modifiers.union(Modifiers::SHIFT);
        }
        if GetKeyState(i32::from(VK_CONTROL)) < 0 {
            modifiers = modifiers.union(Modifiers::CONTROL);
        }
        if GetKeyState(i32::from(VK_MENU)) < 0 {
            modifiers = modifiers.union(Modifiers::ALT);
        }
        if GetKeyState(i32::from(VK_LWIN)) < 0 || GetKeyState(i32::from(VK_RWIN)) < 0 {
            modifiers = modifiers.union(Modifiers::SUPER);
        }
    }
    modifiers
}

const _: Option<unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT> =
    Some(window_proc);
