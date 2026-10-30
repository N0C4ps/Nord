use std::time::{Duration, Instant};

/// Monotonic time elapsed since platform initialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(Duration);

impl Timestamp {
    /// Elapsed duration represented by this timestamp.
    #[must_use]
    pub const fn elapsed(self) -> Duration {
        self.0
    }
}

/// High-resolution monotonic clock for simulation and frame timing.
#[derive(Debug, Clone)]
pub struct MonotonicClock {
    origin: Instant,
}

impl MonotonicClock {
    pub(super) fn new() -> Self {
        Self { origin: Instant::now() }
    }

    /// Read the current elapsed timestamp.
    #[must_use]
    pub fn now(&self) -> Timestamp {
        Timestamp(self.origin.elapsed())
    }

    /// Measure elapsed time since an earlier timestamp.
    ///
    /// Returns zero if the supplied timestamp is later than this clock reading.
    #[must_use]
    pub fn elapsed_since(&self, earlier: Timestamp) -> Duration {
        self.now().0.saturating_sub(earlier.0)
    }
}
