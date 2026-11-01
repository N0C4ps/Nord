use std::time::{Duration, Instant};

/// Frame-clock behavior, including a clamp for long stalls and debugger pauses.
#[derive(Debug, Clone, Copy)]
pub struct ClockConfig {
    /// Largest accepted frame delta. Larger raw deltas remain observable but
    /// are clamped for simulation time.
    pub max_frame_delta: Duration,
}

impl ClockConfig {
    /// Create a clock configuration with a maximum simulation delta.
    #[must_use]
    pub const fn new(max_frame_delta: Duration) -> Self {
        Self { max_frame_delta }
    }
}

impl Default for ClockConfig {
    fn default() -> Self {
        Self::new(Duration::from_millis(250))
    }
}

/// Monotonic frame clock for measuring delta and game elapsed time.
pub struct FrameClock {
    config: ClockConfig,
    started_at: Instant,
    last_tick: Instant,
    frame_index: u64,
    elapsed: Duration,
    time_scale: f64,
}

impl FrameClock {
    /// Start measuring from the current monotonic instant.
    #[must_use]
    pub fn new(config: ClockConfig) -> Self {
        let now = Instant::now();
        Self {
            config,
            started_at: now,
            last_tick: now,
            frame_index: 0,
            elapsed: Duration::ZERO,
            time_scale: 1.0,
        }
    }

    /// Record a frame using the current monotonic instant.
    #[must_use]
    pub fn tick(&mut self) -> FrameTime {
        self.tick_at(Instant::now())
    }

    /// Record a frame at a supplied monotonic instant.
    ///
    /// This is useful when the host already sampled `Instant` or wants to
    /// share one exact timing boundary among multiple systems.
    #[must_use]
    pub fn tick_at(&mut self, now: Instant) -> FrameTime {
        let now = now.max(self.last_tick);
        let raw_delta = now.saturating_duration_since(self.last_tick);
        self.last_tick = now;
        let unscaled_delta = raw_delta.min(self.config.max_frame_delta);
        let delta = scale_duration(unscaled_delta, self.time_scale);
        self.elapsed = self.elapsed.saturating_add(delta);
        self.frame_index = self.frame_index.saturating_add(1);
        FrameTime {
            frame_index: self.frame_index,
            raw_delta,
            unscaled_delta,
            delta,
            elapsed: self.elapsed,
            real_elapsed: now.saturating_duration_since(self.started_at),
        }
    }

    /// Reset elapsed game time and establish a fresh monotonic baseline.
    pub fn reset(&mut self) {
        let now = Instant::now();
        self.started_at = now;
        self.last_tick = now;
        self.frame_index = 0;
        self.elapsed = Duration::ZERO;
    }

    /// Current clamped game elapsed time.
    #[must_use]
    pub const fn elapsed(&self) -> Duration {
        self.elapsed
    }

    /// Number of frames sampled since creation or reset.
    #[must_use]
    pub const fn frame_index(&self) -> u64 {
        self.frame_index
    }

    /// Current clock configuration.
    #[must_use]
    pub const fn config(&self) -> ClockConfig {
        self.config
    }

    /// Set game-time scaling. Zero pauses simulation time; one is real-time.
    ///
    /// The scale is bounded to 100x to avoid runaway updates from a
    /// configuration mistake.
    ///
    /// # Errors
    ///
    /// Returns [`TimeScaleError`] for negative, non-finite, or greater than
    /// 100x values. The current scale remains unchanged on error.
    pub fn set_time_scale(&mut self, scale: f64) -> Result<(), TimeScaleError> {
        if !scale.is_finite() || !(0.0..=100.0).contains(&scale) {
            return Err(TimeScaleError { requested: scale });
        }
        self.time_scale = scale;
        Ok(())
    }

    /// Current game-time scale.
    #[must_use]
    pub const fn time_scale(&self) -> f64 {
        self.time_scale
    }
}

impl Default for FrameClock {
    fn default() -> Self {
        Self::new(ClockConfig::default())
    }
}

/// Timing values sampled for one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameTime {
    /// Monotonic frame number, starting at one.
    pub frame_index: u64,
    /// Unclamped time since the prior frame sample.
    pub raw_delta: Duration,
    /// Delta limited by [`ClockConfig::max_frame_delta`] before time scaling.
    pub unscaled_delta: Duration,
    /// Delta clamped by [`ClockConfig::max_frame_delta`].
    pub delta: Duration,
    /// Accumulated clamped game time since clock creation or reset.
    pub elapsed: Duration,
    /// Unclamped monotonic elapsed time since clock creation or reset.
    pub real_elapsed: Duration,
}

/// Small monotonic stopwatch for profiling a scoped operation.
pub struct Stopwatch {
    started_at: Instant,
}

impl Stopwatch {
    /// Start a stopwatch.
    #[must_use]
    pub fn start() -> Self {
        Self { started_at: Instant::now() }
    }

    /// Elapsed monotonic duration.
    #[must_use]
    pub fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }

    /// Return the elapsed duration and start measuring again.
    pub fn restart(&mut self) -> Duration {
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.started_at);
        self.started_at = now;
        elapsed
    }
}

impl Default for Stopwatch {
    fn default() -> Self {
        Self::start()
    }
}

/// Invalid time scale requested for a [`FrameClock`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimeScaleError {
    /// Rejected time-scale value.
    pub requested: f64,
}

impl std::fmt::Display for TimeScaleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "time scale {} is outside the supported range 0..=100", self.requested)
    }
}

impl std::error::Error for TimeScaleError {}

fn scale_duration(duration: Duration, scale: f64) -> Duration {
    if scale == 0.0 || duration.is_zero() {
        return Duration::ZERO;
    }
    Duration::try_from_secs_f64(duration.as_secs_f64() * scale).unwrap_or(Duration::MAX)
}
