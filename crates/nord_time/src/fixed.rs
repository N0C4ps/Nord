use std::time::Duration;

/// Validated configuration for deterministic fixed-step simulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedStepConfig {
    step: Duration,
    max_steps_per_frame: u32,
}

impl FixedStepConfig {
    /// Configure the simulation interval and per-frame catch-up limit.
    ///
    /// # Errors
    ///
    /// Returns [`FixedStepConfigError`] if either value is zero.
    pub fn new(step: Duration, max_steps_per_frame: u32) -> Result<Self, FixedStepConfigError> {
        if step.is_zero() {
            return Err(FixedStepConfigError::ZeroStep);
        }
        if max_steps_per_frame == 0 {
            return Err(FixedStepConfigError::ZeroCatchUpLimit);
        }
        Ok(Self { step, max_steps_per_frame })
    }

    /// Fixed simulation interval.
    #[must_use]
    pub const fn step(self) -> Duration {
        self.step
    }

    /// Maximum simulation updates allowed during one rendered frame.
    #[must_use]
    pub const fn max_steps_per_frame(self) -> u32 {
        self.max_steps_per_frame
    }
}

impl Default for FixedStepConfig {
    fn default() -> Self {
        Self { step: Duration::from_nanos(16_666_667), max_steps_per_frame: 8 }
    }
}

/// Fixed-step accumulator with bounded catch-up and explicit dropped-time data.
pub struct FixedStepAccumulator {
    config: FixedStepConfig,
    accumulated: Duration,
    total_dropped: Duration,
}

impl FixedStepAccumulator {
    /// Start a fixed-step accumulator.
    #[must_use]
    pub const fn new(config: FixedStepConfig) -> Self {
        Self { config, accumulated: Duration::ZERO, total_dropped: Duration::ZERO }
    }

    /// Add a frame delta and calculate how many fixed simulation steps to run.
    /// Excess whole steps beyond the catch-up limit are discarded, while a
    /// fractional step remains as interpolation time.
    #[must_use]
    pub fn advance(&mut self, frame_delta: Duration) -> FixedStepPlan {
        self.accumulated = self.accumulated.saturating_add(frame_delta);
        let step_nanos = self.config.step.as_nanos();
        let available_steps = self.accumulated.as_nanos() / step_nanos;
        let steps = u32::try_from(available_steps.min(u128::from(self.config.max_steps_per_frame)))
            .unwrap_or(self.config.max_steps_per_frame);
        self.accumulated = self.accumulated.saturating_sub(self.config.step.saturating_mul(steps));

        let mut dropped_time = Duration::ZERO;
        if self.accumulated >= self.config.step {
            let remainder_nanos = self.accumulated.as_nanos() % step_nanos;
            let remainder = duration_from_nanos(remainder_nanos);
            dropped_time = self.accumulated.saturating_sub(remainder);
            self.accumulated = remainder;
            self.total_dropped = self.total_dropped.saturating_add(dropped_time);
        }

        FixedStepPlan {
            steps,
            interpolation_alpha: self.accumulated.as_secs_f64() / self.config.step.as_secs_f64(),
            dropped_time,
        }
    }

    /// Current fractional step retained for render interpolation.
    #[must_use]
    pub const fn accumulated(&self) -> Duration {
        self.accumulated
    }

    /// Total whole-step time discarded by the catch-up limit.
    #[must_use]
    pub const fn total_dropped(&self) -> Duration {
        self.total_dropped
    }

    /// Reset accumulated and dropped simulation time.
    pub fn reset(&mut self) {
        self.accumulated = Duration::ZERO;
        self.total_dropped = Duration::ZERO;
    }
}

impl Default for FixedStepAccumulator {
    fn default() -> Self {
        Self::new(FixedStepConfig::default())
    }
}

/// Scheduling result for one rendered frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FixedStepPlan {
    /// Number of fixed updates the caller should run.
    pub steps: u32,
    /// Fractional progress to the next fixed update, in `[0, 1)`.
    pub interpolation_alpha: f64,
    /// Whole-step time discarded this frame to bound catch-up work.
    pub dropped_time: Duration,
}

/// Invalid fixed-step timing configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixedStepConfigError {
    /// A fixed interval must be greater than zero.
    ZeroStep,
    /// A catch-up limit must allow at least one update per frame.
    ZeroCatchUpLimit,
}

impl std::fmt::Display for FixedStepConfigError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroStep => formatter.write_str("fixed simulation step must be non-zero"),
            Self::ZeroCatchUpLimit => {
                formatter.write_str("fixed simulation catch-up limit must be greater than zero")
            }
        }
    }
}

impl std::error::Error for FixedStepConfigError {}

fn duration_from_nanos(nanos: u128) -> Duration {
    let seconds = u64::try_from(nanos / 1_000_000_000).unwrap_or(u64::MAX);
    let subsecond_nanos = u32::try_from(nanos % 1_000_000_000).unwrap_or(u32::MAX);
    Duration::new(seconds, subsecond_nanos)
}
