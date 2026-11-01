//! Monotonic frame timing, fixed-step scheduling, FPS sampling, and wall stamps.

mod clock;
mod fixed;
mod fps;
mod timestamp;

pub use clock::{ClockConfig, FrameClock, FrameTime, Stopwatch, TimeScaleError};
pub use fixed::{FixedStepAccumulator, FixedStepConfig, FixedStepConfigError, FixedStepPlan};
pub use fps::{FpsCounter, FpsSnapshot, FpsWindowError};
pub use timestamp::{UnixTimestamp, UnixTimestampError};
