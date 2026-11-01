//! Thread-affine native window capability with platform-neutral handles.

use crate::{EventEmitter, EventError, WindowHandle};
use std::fmt::{Display, Formatter};
use std::sync::Mutex;
use std::thread::{self, ThreadId};

/// Configuration for a native top-level game window. Dimensions are client-area pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowConfig {
    title: String,
    width: u32,
    height: u32,
    resizable: bool,
    visible: bool,
}

impl WindowConfig {
    /// Create a visible, resizable game window configuration.
    #[must_use]
    pub fn new(title: impl Into<String>, width: u32, height: u32) -> Self {
        Self { title: title.into(), width, height, resizable: true, visible: true }
    }

    /// Set whether the user can resize the window.
    #[must_use]
    pub fn with_resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    /// Set whether the window is shown immediately after creation.
    #[must_use]
    pub fn with_visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    /// Window title.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }
    /// Requested client width in pixels.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }
    /// Requested client height in pixels.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }
    /// Whether the user can resize the window.
    #[must_use]
    pub const fn resizable(&self) -> bool {
        self.resizable
    }
    /// Whether the window should be shown at creation.
    #[must_use]
    pub const fn visible(&self) -> bool {
        self.visible
    }
}

/// Failure while creating, pumping, or managing a native window.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum WindowError {
    /// The current target has no window backend yet.
    Unsupported,
    /// Window operations and message pumping must run on the creating thread.
    WrongThread,
    /// Client dimensions must be non-zero and fit the native API.
    InvalidDimensions,
    /// Window titles cannot contain a native UTF-16 terminator.
    InvalidTitle,
    /// A native API call failed.
    Native { operation: &'static str, code: u32 },
    /// Platform event or identity allocation failed.
    Event(EventError),
    /// The handle does not identify a window owned by this platform instance.
    UnknownHandle,
}

impl Display for WindowError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported => {
                formatter.write_str("native windows are not supported by this backend")
            }
            Self::WrongThread => {
                formatter.write_str("native window operation called from a non-owner thread")
            }
            Self::InvalidDimensions => {
                formatter.write_str("window dimensions must be non-zero and fit Win32 dimensions")
            }
            Self::InvalidTitle => {
                formatter.write_str("window title cannot contain a NUL character")
            }
            Self::Native { operation, code } => {
                write!(formatter, "{operation} failed with native error {code}")
            }
            Self::Event(error) => Display::fmt(error, formatter),
            Self::UnknownHandle => {
                formatter.write_str("window handle is not owned by this platform instance")
            }
        }
    }
}

impl std::error::Error for WindowError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Event(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(target_os = "windows")]
type NativeBackend = crate::backend::WindowsWindowBackend;

#[cfg(not(target_os = "windows"))]
#[derive(Default)]
struct NativeBackend;

/// Owns native windows and pumps the creating thread's message queue.
///
/// Window creation, message dispatch, and destruction are deliberately
/// thread-affine as required by the native windowing APIs. The object remains
/// shareable for service registration; calls from another thread return an
/// explicit error rather than touching native state.
pub struct WindowSystem {
    owner: ThreadId,
    backend: Mutex<NativeBackend>,
    events: EventEmitter,
}

impl WindowSystem {
    pub(crate) fn new(events: EventEmitter) -> Self {
        Self {
            owner: thread::current().id(),
            backend: Mutex::new(NativeBackend::default()),
            events,
        }
    }

    fn check_thread(&self) -> Result<(), WindowError> {
        if thread::current().id() == self.owner { Ok(()) } else { Err(WindowError::WrongThread) }
    }

    /// Create a window and return its opaque platform handle.
    ///
    /// # Errors
    ///
    /// Returns [`WindowError`] for invalid dimensions, unsupported backends,
    /// wrong-thread access, native failures, or exhausted event identity space.
    pub fn create(&self, config: &WindowConfig) -> Result<WindowHandle, WindowError> {
        self.check_thread()?;
        if config.width == 0
            || config.height == 0
            || config.width > i32::MAX as u32
            || config.height > i32::MAX as u32
        {
            return Err(WindowError::InvalidDimensions);
        }
        if config.title.contains('\0') {
            return Err(WindowError::InvalidTitle);
        }
        #[cfg(target_os = "windows")]
        {
            return self
                .backend
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .create(config, &self.events);
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = config;
            Err(WindowError::Unsupported)
        }
    }

    /// Destroy a window owned by this system.
    ///
    /// # Errors
    ///
    /// Returns [`WindowError::WrongThread`], [`WindowError::UnknownHandle`],
    /// or a native destruction failure.
    pub fn destroy(&self, handle: WindowHandle) -> Result<(), WindowError> {
        self.check_thread()?;
        #[cfg(target_os = "windows")]
        {
            return self
                .backend
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .destroy(handle);
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = handle;
            Err(WindowError::Unsupported)
        }
    }

    /// Show or hide a window owned by this system.
    ///
    /// # Errors
    ///
    /// Returns [`WindowError::WrongThread`], [`WindowError::UnknownHandle`],
    /// or [`WindowError::Unsupported`] when the backend has no window support.
    pub fn set_visible(&self, handle: WindowHandle, visible: bool) -> Result<(), WindowError> {
        self.check_thread()?;
        #[cfg(target_os = "windows")]
        {
            self.backend
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .set_visible(handle, visible)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (handle, visible);
            Err(WindowError::Unsupported)
        }
    }

    /// Return the native window token for an engine-owned graphics backend.
    /// This is an integration hook; game code should use the `nord_app` facade.
    #[doc(hidden)]
    pub fn native_handle(
        &self,
        handle: WindowHandle,
    ) -> Result<raw_window_handle::RawWindowHandle, WindowError> {
        self.check_thread()?;
        #[cfg(target_os = "windows")]
        {
            return self
                .backend
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .raw_handle(handle);
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = handle;
            Err(WindowError::Unsupported)
        }
    }

    /// Dispatch pending native messages without blocking.
    ///
    /// # Errors
    ///
    /// Returns [`WindowError::WrongThread`] if called off the platform owner thread.
    pub fn pump_messages(&self) -> Result<(), WindowError> {
        self.check_thread()?;
        #[cfg(target_os = "windows")]
        {
            self.backend.lock().unwrap_or_else(std::sync::PoisonError::into_inner).pump_messages();
            Ok(())
        }
        #[cfg(not(target_os = "windows"))]
        {
            Ok(())
        }
    }
}

impl Drop for WindowSystem {
    fn drop(&mut self) {
        if thread::current().id() != self.owner {
            // HWND destruction is owner-thread-only. Native context references remain
            // owned by Windows until its thread tears down any surviving HWND.
            return;
        }
        #[cfg(target_os = "windows")]
        {
            let backend = self.backend.get_mut().unwrap_or_else(std::sync::PoisonError::into_inner);
            backend.destroy_all();
        }
    }
}
