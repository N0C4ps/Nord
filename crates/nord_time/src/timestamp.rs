use std::time::{SystemTime, SystemTimeError, UNIX_EPOCH};

/// Wall-clock timestamp represented as nanoseconds since the Unix epoch.
///
/// Use this for persisted or externally correlated timestamps. Use
/// [`crate::FrameClock`] or [`crate::Stopwatch`] for elapsed-time measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnixTimestamp {
    unix_nanos: u128,
}

impl UnixTimestamp {
    /// Capture the current wall-clock time.
    ///
    /// # Errors
    ///
    /// Returns an error if the system clock is earlier than the Unix epoch.
    pub fn now() -> Result<Self, UnixTimestampError> {
        Self::from_system_time(SystemTime::now())
    }

    /// Convert an operating-system wall-clock value to Unix nanoseconds.
    ///
    /// # Errors
    ///
    /// Returns an error if `time` precedes the Unix epoch.
    pub fn from_system_time(time: SystemTime) -> Result<Self, UnixTimestampError> {
        let duration = time.duration_since(UNIX_EPOCH).map_err(UnixTimestampError)?;
        Ok(Self { unix_nanos: duration.as_nanos() })
    }

    /// Nanoseconds since 1970-01-01T00:00:00Z.
    #[must_use]
    pub const fn as_unix_nanos(self) -> u128 {
        self.unix_nanos
    }

    /// Milliseconds since 1970-01-01T00:00:00Z.
    #[must_use]
    pub const fn as_unix_millis(self) -> u128 {
        self.unix_nanos / 1_000_000
    }
}

/// Wall-clock timestamp conversion failure.
#[derive(Debug)]
pub struct UnixTimestampError(SystemTimeError);

impl std::fmt::Display for UnixTimestampError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "system time is before the Unix epoch: {}", self.0)
    }
}

impl std::error::Error for UnixTimestampError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}
