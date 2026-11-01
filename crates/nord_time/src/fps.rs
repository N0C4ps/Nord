use std::time::Duration;

/// Periodic FPS and mean frame-time sampler.
pub struct FpsCounter {
    sample_window: Duration,
    elapsed: Duration,
    frames: u32,
    latest: Option<FpsSnapshot>,
}

impl FpsCounter {
    /// Create a sampler that reports after each `sample_window` of frame time.
    ///
    /// # Errors
    ///
    /// Returns [`FpsWindowError`] when the window is zero.
    pub fn new(sample_window: Duration) -> Result<Self, FpsWindowError> {
        if sample_window.is_zero() {
            return Err(FpsWindowError);
        }
        Ok(Self { sample_window, elapsed: Duration::ZERO, frames: 0, latest: None })
    }

    /// Add one frame sample. The returned snapshot updates when a full window
    /// has elapsed; no allocations occur while sampling.
    pub fn record_frame(&mut self, frame_duration: Duration) -> Option<FpsSnapshot> {
        self.elapsed = self.elapsed.saturating_add(frame_duration);
        self.frames = self.frames.saturating_add(1);
        if self.elapsed < self.sample_window {
            return None;
        }
        let snapshot = FpsSnapshot {
            frames: self.frames,
            sample_duration: self.elapsed,
            frames_per_second: f64::from(self.frames) / self.elapsed.as_secs_f64(),
            average_frame_time: self.elapsed / self.frames,
        };
        self.elapsed = Duration::ZERO;
        self.frames = 0;
        self.latest = Some(snapshot);
        Some(snapshot)
    }

    /// Most recently completed sample, if a full window has elapsed.
    #[must_use]
    pub const fn latest(&self) -> Option<FpsSnapshot> {
        self.latest
    }

    /// Configured sample window.
    #[must_use]
    pub const fn sample_window(&self) -> Duration {
        self.sample_window
    }

    /// Reset the partial sample and clear the last completed sample.
    pub fn reset(&mut self) {
        self.elapsed = Duration::ZERO;
        self.frames = 0;
        self.latest = None;
    }
}

impl Default for FpsCounter {
    fn default() -> Self {
        Self::new(Duration::from_secs(1)).expect("one second is a valid FPS sample window")
    }
}

/// A completed FPS sample.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FpsSnapshot {
    /// Number of frames in this sample window.
    pub frames: u32,
    /// Actual elapsed duration used for the sample.
    pub sample_duration: Duration,
    /// Mean frames per second over the sample duration.
    pub frames_per_second: f64,
    /// Mean duration of one frame in the sample.
    pub average_frame_time: Duration,
}

/// Invalid zero-length FPS sample window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FpsWindowError;

impl std::fmt::Display for FpsWindowError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("FPS sample window must be greater than zero")
    }
}

impl std::error::Error for FpsWindowError {}
