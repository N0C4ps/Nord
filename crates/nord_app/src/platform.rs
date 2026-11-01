//! Stable application-facing facade over Nord's native platform services.

use crate::events::ApplicationWindowHandle;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::Path;
use std::time::Duration;

pub use nord_platform::{
    DisplayError as ApplicationDisplayError, DisplayMonitor as ApplicationDisplayMonitor,
    DisplayRect as ApplicationDisplayRect, EventError as ApplicationEventError,
    FileAccess as ApplicationFileAccess, FileDisposition as ApplicationFileDisposition,
    FileHandle as ApplicationFileHandle, FileOpenOptions as ApplicationFileOpenOptions,
    FileSystem as ApplicationFileSystem, PlatformEvent as ApplicationPlatformEvent,
    PlatformEventKind as ApplicationPlatformEventKind, Timestamp as ApplicationTimestamp,
    WindowConfig as ApplicationWindowConfig, WindowError as ApplicationWindowError,
};

/// Platform backend available to the running application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ApplicationPlatformBackend {
    /// Microsoft Windows backend.
    Windows,
    /// Linux backend.
    Linux,
    /// Apple macOS backend.
    MacOs,
    /// A backend added by a newer Nord platform implementation.
    Other,
}

/// Error encountered while initializing application platform services.
#[derive(Debug)]
pub struct ApplicationPlatformError(nord_platform::PathError);

impl Display for ApplicationPlatformError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, formatter)
    }
}

impl Error for ApplicationPlatformError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.0)
    }
}

/// Application-owned handle to platform capabilities.
///
/// Keep this facade in the application service registry. Game modules consume
/// `nord_app` APIs and do not depend directly on the implementation crate.
pub struct ApplicationPlatform {
    inner: nord_platform::Platform,
}

impl ApplicationPlatform {
    /// Discover native capabilities and per-application directories.
    ///
    /// # Errors
    ///
    /// Returns an error when the application identifier cannot be used for
    /// platform paths.
    pub fn initialize(application_id: &str) -> Result<Self, ApplicationPlatformError> {
        nord_platform::Platform::initialize(application_id)
            .map(|inner| Self { inner })
            .map_err(ApplicationPlatformError)
    }

    /// Active native backend.
    #[must_use]
    pub const fn backend(&self) -> ApplicationPlatformBackend {
        match self.inner.info().backend() {
            nord_platform::BackendKind::Windows => ApplicationPlatformBackend::Windows,
            nord_platform::BackendKind::Linux => ApplicationPlatformBackend::Linux,
            nord_platform::BackendKind::MacOs => ApplicationPlatformBackend::MacOs,
            _ => ApplicationPlatformBackend::Other,
        }
    }

    /// Rust target architecture identifier.
    #[must_use]
    pub const fn architecture(&self) -> &'static str {
        self.inner.info().architecture()
    }

    /// Persistent per-application data path.
    #[must_use]
    pub fn data_dir(&self) -> &Path {
        self.inner.paths().data_dir()
    }

    /// User configuration path.
    #[must_use]
    pub fn config_dir(&self) -> &Path {
        self.inner.paths().config_dir()
    }

    /// Re-creatable cache path.
    #[must_use]
    pub fn cache_dir(&self) -> &Path {
        self.inner.paths().cache_dir()
    }

    /// Temporary per-application directory.
    #[must_use]
    pub fn temp_dir(&self) -> &Path {
        self.inner.paths().temp_dir()
    }

    /// Create the persistent data directory when needed.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the directory cannot be created.
    pub fn ensure_data_dir(&self) -> std::io::Result<()> {
        self.inner.paths().ensure_data_dir()
    }

    /// Create the configuration directory when needed.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the directory cannot be created.
    pub fn ensure_config_dir(&self) -> std::io::Result<()> {
        self.inner.paths().ensure_config_dir()
    }

    /// Create the cache directory when needed.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the directory cannot be created.
    pub fn ensure_cache_dir(&self) -> std::io::Result<()> {
        self.inner.paths().ensure_cache_dir()
    }

    /// Monotonic time elapsed since application platform initialization.
    #[must_use]
    pub fn elapsed(&self) -> Duration {
        self.inner.clock().now().elapsed()
    }

    /// Platform filesystem capability with explicit, opaque resource handles.
    #[must_use]
    pub const fn filesystem(&self) -> &ApplicationFileSystem {
        self.inner.filesystem()
    }

    /// Queue a producer for normalized platform events.
    #[must_use]
    pub fn event_emitter(&self) -> ApplicationEventEmitter {
        ApplicationEventEmitter { inner: self.inner.event_emitter() }
    }

    /// Drain normalized platform events into the caller's reusable buffer.
    ///
    /// # Errors
    ///
    /// Returns [`ApplicationWindowError`] if called from a thread other than
    /// the one that owns the native window system.
    pub fn poll_events(
        &self,
        output: &mut Vec<ApplicationPlatformEvent>,
    ) -> Result<(), ApplicationWindowError> {
        self.inner.poll_events(output)
    }

    /// Create a native game window through Nord's platform facade.
    ///
    /// # Errors
    ///
    /// Returns [`ApplicationWindowError`] for invalid dimensions, unsupported
    /// backends, wrong-thread access, or native creation failures.
    pub fn create_window(
        &self,
        config: &ApplicationWindowConfig,
    ) -> Result<ApplicationWindowHandle, ApplicationWindowError> {
        self.inner.create_window(config)
    }

    /// Explicitly destroy a window created by this platform instance.
    ///
    /// # Errors
    ///
    /// Returns [`ApplicationWindowError`] if the handle is unknown, the caller
    /// is on the wrong thread, or native destruction fails.
    pub fn destroy_window(
        &self,
        handle: ApplicationWindowHandle,
    ) -> Result<(), ApplicationWindowError> {
        self.inner.destroy_window(handle)
    }

    /// Show or hide a game window through the application platform facade.
    ///
    /// # Errors
    ///
    /// Returns [`ApplicationWindowError`] for unknown handles, wrong-thread
    /// access, or unsupported native backends.
    pub fn set_window_visible(
        &self,
        handle: ApplicationWindowHandle,
        visible: bool,
    ) -> Result<(), ApplicationWindowError> {
        self.inner.set_window_visible(handle, visible)
    }

    /// Lock or release the pointer for a game-owned first-person input mode.
    ///
    /// The platform supplies relative motion and restores the cursor when
    /// released; game code decides when capture starts and stops.
    ///
    /// # Errors
    ///
    /// Returns [`ApplicationWindowError`] for unsupported backends, wrong-thread
    /// access, unknown windows, or native failures.
    pub fn set_cursor_locked(
        &self,
        handle: ApplicationWindowHandle,
        locked: bool,
    ) -> Result<(), ApplicationWindowError> {
        self.inner.set_cursor_locked(handle, locked)
    }

    pub(crate) fn native_window_handle(
        &self,
        handle: ApplicationWindowHandle,
    ) -> Result<raw_window_handle::RawWindowHandle, ApplicationWindowError> {
        self.inner.native_window_handle(handle)
    }

    /// Access monitor enumeration and display-layout queries.
    #[must_use]
    pub fn display_manager(&self) -> ApplicationDisplayManager<'_> {
        ApplicationDisplayManager { inner: self.inner.displays() }
    }
}

/// Stable application facade for querying connected displays.
pub struct ApplicationDisplayManager<'a> {
    inner: &'a nord_platform::DisplayManager,
}

impl ApplicationDisplayManager<'_> {
    /// Enumerate current monitors as a fresh snapshot.
    ///
    /// # Errors
    ///
    /// Returns [`ApplicationDisplayError`] if the backend cannot enumerate displays.
    pub fn monitors(&self) -> Result<Vec<ApplicationDisplayMonitor>, ApplicationDisplayError> {
        self.inner.monitors()
    }

    /// Return the primary monitor, if one is currently available.
    ///
    /// # Errors
    ///
    /// Returns [`ApplicationDisplayError`] if native enumeration fails.
    pub fn primary_monitor(
        &self,
    ) -> Result<Option<ApplicationDisplayMonitor>, ApplicationDisplayError> {
        self.inner.primary_monitor()
    }

    /// Find the monitor containing a virtual desktop point, in pixels.
    ///
    /// # Errors
    ///
    /// Returns [`ApplicationDisplayError`] if native enumeration fails.
    pub fn monitor_at(
        &self,
        x: i32,
        y: i32,
    ) -> Result<Option<ApplicationDisplayMonitor>, ApplicationDisplayError> {
        self.inner.monitor_at(x, y)
    }
}

/// Cloneable producer for normalized application and native subsystem events.
#[derive(Clone)]
pub struct ApplicationEventEmitter {
    inner: nord_platform::EventEmitter,
}

impl ApplicationEventEmitter {
    /// Allocate an opaque window identity for a future native window service.
    ///
    /// # Errors
    ///
    /// Returns [`ApplicationEventError::WindowHandleExhausted`] if no further
    /// unique identifiers can be represented.
    pub fn allocate_window_handle(&self) -> Result<ApplicationWindowHandle, ApplicationEventError> {
        self.inner.allocate_window_handle()
    }

    /// Queue an event and return its monotonically increasing sequence number.
    ///
    /// # Errors
    ///
    /// Returns [`ApplicationEventError::QueueFull`] when the bounded event
    /// queue is full, or [`ApplicationEventError::SequenceExhausted`] if event
    /// ordering can no longer be represented uniquely.
    pub fn emit(&self, event: ApplicationPlatformEventKind) -> Result<u64, ApplicationEventError> {
        self.inner.emit(event)
    }
}
