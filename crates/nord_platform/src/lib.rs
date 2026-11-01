//! Capability-oriented platform services for Nord Tech.
//!
//! The crate selects one native backend at compile time. Platform-neutral
//! consumers use the capability types here instead of OS APIs.

mod backend;
mod display;
mod events;
mod filesystem;
mod paths;
mod time;
mod window;

pub use display::{DisplayError, DisplayManager, DisplayMonitor, DisplayRect};
pub use events::{
    ApplicationLifecycleEvent, EventEmitter, EventError, EventReceiver, InputEvent, KeyCode,
    Modifiers, PlatformEvent, PlatformEventKind, PointerButton, WindowEvent, WindowHandle,
};
pub use filesystem::{FileAccess, FileDisposition, FileHandle, FileOpenOptions, FileSystem};
pub use paths::{GamePaths, PathError};
pub use time::{MonotonicClock, Timestamp};
pub use window::{WindowConfig, WindowError, WindowSystem};

/// The native backend selected for the current compilation target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BackendKind {
    /// Microsoft Windows desktop backend.
    Windows,
    /// Linux backend.
    Linux,
    /// Apple macOS backend.
    MacOs,
}

/// Immutable information about the active native backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlatformInfo {
    backend: BackendKind,
    architecture: &'static str,
}

impl PlatformInfo {
    /// Return the selected native backend.
    #[must_use]
    pub const fn backend(self) -> BackendKind {
        self.backend
    }

    /// Return Rust's target architecture identifier.
    #[must_use]
    pub const fn architecture(self) -> &'static str {
        self.architecture
    }
}

/// The application-owned set of platform capabilities.
///
/// This object owns its file handles and event queue. It can be shared across
/// threads; file operations are serialized and event delivery is bounded.
pub struct Platform {
    info: PlatformInfo,
    paths: GamePaths,
    clock: MonotonicClock,
    filesystem: FileSystem,
    displays: DisplayManager,
    events: EventReceiver,
    event_emitter: EventEmitter,
    windows: WindowSystem,
}

impl Platform {
    /// Initialize platform services for an application identifier.
    ///
    /// Initialization discovers platform paths but does not create files or
    /// directories. Call [`GamePaths::ensure_data_dir`] when persistence is needed.
    ///
    /// # Errors
    ///
    /// Returns [`PathError`] if the application identifier is invalid.
    pub fn initialize(application_id: &str) -> Result<Self, PathError> {
        let info = backend::platform_info();
        let paths = GamePaths::discover(application_id)?;
        let clock = MonotonicClock::new();
        let (event_emitter, events) = events::event_queue(clock.clone());
        let windows = WindowSystem::new(event_emitter.clone());

        Ok(Self {
            info,
            paths,
            clock,
            filesystem: FileSystem::new(),
            displays: DisplayManager::new(),
            events,
            event_emitter,
            windows,
        })
    }

    /// Return the active backend and target architecture.
    #[must_use]
    pub const fn info(&self) -> PlatformInfo {
        self.info
    }

    /// Return discovered per-application directories.
    #[must_use]
    pub const fn paths(&self) -> &GamePaths {
        &self.paths
    }

    /// Return the monotonic clock capability.
    #[must_use]
    pub const fn clock(&self) -> &MonotonicClock {
        &self.clock
    }

    /// Return the platform filesystem and its explicit resource handles.
    #[must_use]
    pub const fn filesystem(&self) -> &FileSystem {
        &self.filesystem
    }

    /// Return the display enumeration and monitor-query capability.
    #[must_use]
    pub const fn displays(&self) -> &DisplayManager {
        &self.displays
    }

    /// Return a cloneable event producer for native subsystems.
    #[must_use]
    pub fn event_emitter(&self) -> EventEmitter {
        self.event_emitter.clone()
    }

    /// Drain queued events, preserving their sequence order.
    ///
    /// # Errors
    ///
    /// Returns [`WindowError::WrongThread`] if native messages are pumped from
    /// a thread other than the platform's owner thread.
    pub fn poll_events(&self, output: &mut Vec<PlatformEvent>) -> Result<(), WindowError> {
        self.windows.pump_messages()?;
        self.events.drain_into(output);
        Ok(())
    }

    /// Create a native top-level application window.
    ///
    /// # Errors
    ///
    /// Returns [`WindowError`] for invalid configuration, unsupported native
    /// backends, wrong-thread use, or operating-system failures.
    pub fn create_window(&self, config: &WindowConfig) -> Result<WindowHandle, WindowError> {
        self.windows.create(config)
    }

    /// Destroy a native window owned by this platform instance.
    ///
    /// # Errors
    ///
    /// Returns [`WindowError`] if the handle is unknown, the caller is on the
    /// wrong thread, or the operating system cannot destroy the window.
    pub fn destroy_window(&self, handle: WindowHandle) -> Result<(), WindowError> {
        self.windows.destroy(handle)
    }

    /// Show or hide a native window owned by this platform instance.
    ///
    /// # Errors
    ///
    /// Returns [`WindowError`] for unknown handles, wrong-thread calls, or
    /// unsupported native backends.
    pub fn set_window_visible(
        &self,
        handle: WindowHandle,
        visible: bool,
    ) -> Result<(), WindowError> {
        self.windows.set_visible(handle, visible)
    }

    /// Integration hook used by `nord_app` to bind a graphics surface to a window.
    #[doc(hidden)]
    pub fn native_window_handle(
        &self,
        handle: WindowHandle,
    ) -> Result<raw_window_handle::RawWindowHandle, WindowError> {
        self.windows.native_handle(handle)
    }
}
