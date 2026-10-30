//! Stable application-facing facade over Nord's native platform services.

use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::Path;
use std::time::Duration;

pub use nord_platform::{
    ApplicationLifecycleEvent, EventError as ApplicationEventError,
    FileAccess as ApplicationFileAccess, FileDisposition as ApplicationFileDisposition,
    FileHandle as ApplicationFileHandle, FileOpenOptions as ApplicationFileOpenOptions,
    FileSystem as ApplicationFileSystem, InputEvent as ApplicationInputEvent,
    KeyCode as ApplicationKeyCode, Modifiers as ApplicationModifiers,
    PlatformEvent as ApplicationPlatformEvent, PlatformEventKind as ApplicationPlatformEventKind,
    PointerButton as ApplicationPointerButton, Timestamp as ApplicationTimestamp,
    WindowEvent as ApplicationWindowEvent, WindowHandle as ApplicationWindowHandle,
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
    pub fn poll_events(&self, output: &mut Vec<ApplicationPlatformEvent>) {
        self.inner.poll_events(output);
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
