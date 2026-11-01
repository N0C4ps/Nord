//! Platform-neutral monitor and desktop-display queries.

use std::fmt::{Display, Formatter};

/// Integer rectangle in virtual-desktop pixels. Coordinates may be negative
/// when a monitor is positioned left of or above the primary display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DisplayRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl DisplayRect {
    /// Left edge in virtual-desktop pixels.
    #[must_use]
    pub const fn left(self) -> i32 {
        self.left
    }
    /// Top edge in virtual-desktop pixels.
    #[must_use]
    pub const fn top(self) -> i32 {
        self.top
    }
    /// Right edge in virtual-desktop pixels.
    #[must_use]
    pub const fn right(self) -> i32 {
        self.right
    }
    /// Bottom edge in virtual-desktop pixels.
    #[must_use]
    pub const fn bottom(self) -> i32 {
        self.bottom
    }
    /// Rectangle width.
    #[must_use]
    pub fn width(self) -> u32 {
        u32::try_from(self.right.saturating_sub(self.left)).unwrap_or_default()
    }
    /// Rectangle height.
    #[must_use]
    pub fn height(self) -> u32 {
        u32::try_from(self.bottom.saturating_sub(self.top)).unwrap_or_default()
    }
    /// Whether a point lies inside this rectangle.
    #[must_use]
    pub const fn contains(self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }

    #[cfg(target_os = "windows")]
    pub(crate) const fn from_native(rect: windows_sys::Win32::Foundation::RECT) -> Self {
        Self { left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom }
    }
}

/// Stable monitor snapshot returned by native enumeration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayMonitor {
    device_name: String,
    bounds: DisplayRect,
    work_area: DisplayRect,
    primary: bool,
}

impl DisplayMonitor {
    #[cfg(target_os = "windows")]
    pub(crate) fn from_native(
        device_name: String,
        bounds: DisplayRect,
        work_area: DisplayRect,
        primary: bool,
    ) -> Self {
        Self { device_name, bounds, work_area, primary }
    }

    /// Native display-device path, such as `\\.\DISPLAY1` on Windows.
    #[must_use]
    pub fn device_name(&self) -> &str {
        &self.device_name
    }
    /// Full monitor rectangle in virtual-desktop coordinates.
    #[must_use]
    pub const fn bounds(&self) -> DisplayRect {
        self.bounds
    }
    /// Usable rectangle excluding taskbars and reserved desktop areas.
    #[must_use]
    pub const fn work_area(&self) -> DisplayRect {
        self.work_area
    }
    /// Whether this is the system's primary display.
    #[must_use]
    pub const fn is_primary(&self) -> bool {
        self.primary
    }
}

/// Display enumeration or lookup failure.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DisplayError {
    /// Native monitor enumeration is not implemented for the active backend.
    Unsupported,
    /// A native API call failed.
    Native { operation: &'static str, code: u32 },
    /// The native enumeration callback could not produce a complete snapshot.
    CallbackFailed,
}

impl Display for DisplayError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported => {
                formatter.write_str("display queries are not supported by this backend")
            }
            Self::Native { operation, code } => {
                write!(formatter, "{operation} failed with native error {code}")
            }
            Self::CallbackFailed => {
                formatter.write_str("monitor enumeration callback could not complete")
            }
        }
    }
}

impl std::error::Error for DisplayError {}

/// Queries connected monitors. Results are fresh snapshots, not cached handles,
/// so display hot-plug and layout changes are visible on the next query.
#[derive(Debug, Default, Clone, Copy)]
pub struct DisplayManager;

impl DisplayManager {
    pub(crate) const fn new() -> Self {
        Self
    }

    /// Enumerate all desktop monitors, including their bounds and usable work areas.
    ///
    /// # Errors
    ///
    /// Returns [`DisplayError`] when enumeration fails or the backend is not implemented.
    pub fn monitors(&self) -> Result<Vec<DisplayMonitor>, DisplayError> {
        #[cfg(target_os = "windows")]
        {
            crate::backend::WindowsDisplayBackend::enumerate()
        }
        #[cfg(not(target_os = "windows"))]
        {
            Err(DisplayError::Unsupported)
        }
    }

    /// Return the primary monitor, or `None` if the desktop reports no monitors.
    ///
    /// # Errors
    ///
    /// Returns [`DisplayError`] when native enumeration fails.
    pub fn primary_monitor(&self) -> Result<Option<DisplayMonitor>, DisplayError> {
        Ok(self.monitors()?.into_iter().find(DisplayMonitor::is_primary))
    }

    /// Return the monitor containing a point in virtual-desktop pixel coordinates.
    ///
    /// # Errors
    ///
    /// Returns [`DisplayError`] when native enumeration fails.
    pub fn monitor_at(&self, x: i32, y: i32) -> Result<Option<DisplayMonitor>, DisplayError> {
        Ok(self.monitors()?.into_iter().find(|monitor| monitor.bounds.contains(x, y)))
    }
}
