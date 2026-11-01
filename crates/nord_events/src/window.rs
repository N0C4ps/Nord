use std::sync::atomic::{AtomicU64, Ordering};

/// Opaque identifier for a native window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowHandle(u64);

/// Monotonic allocator for opaque native window identities.
pub struct WindowIdAllocator {
    next: AtomicU64,
}

impl Default for WindowIdAllocator {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowIdAllocator {
    /// Create an allocator whose first identifier is one.
    #[must_use]
    pub const fn new() -> Self {
        Self { next: AtomicU64::new(1) }
    }

    /// Allocate a unique window identifier.
    ///
    /// # Errors
    ///
    /// Returns [`WindowIdExhausted`] if no unique identifier remains.
    pub fn allocate(&self) -> Result<WindowHandle, WindowIdExhausted> {
        self.next
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map(WindowHandle)
            .map_err(|_| WindowIdExhausted)
    }
}

/// The opaque identifier space cannot provide another unique window ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowIdExhausted;

impl std::fmt::Display for WindowIdExhausted {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("window handle space is exhausted")
    }
}

impl std::error::Error for WindowIdExhausted {}

/// Application lifecycle events normalized across operating systems.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ApplicationLifecycleEvent {
    /// The application is entering a suspended/background state.
    Suspended,
    /// The application is returning to an active state.
    Resumed,
    /// A native close or termination request was received.
    QuitRequested,
}

/// Window events normalized across operating systems.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum WindowEvent {
    /// Client-area size changed, in physical pixels.
    Resized { width: u32, height: u32 },
    /// Logical-to-physical scale factor changed.
    ScaleFactorChanged { scale_factor: f64 },
    /// Keyboard focus changed.
    FocusChanged { focused: bool },
    /// The user or operating system requested that the window close.
    CloseRequested,
}

/// Event delivered by the application event system.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ApplicationEvent {
    /// Application activation or termination lifecycle changed.
    Lifecycle(ApplicationLifecycleEvent),
    /// A platform window emitted a normalized event.
    Window { window: WindowHandle, event: WindowEvent },
}
