//! Stable application-facing facade over Nord's clocks and timing utilities.

pub use nord_time::{
    ClockConfig as ApplicationClockConfig, FixedStepAccumulator as ApplicationFixedStepAccumulator,
    FixedStepConfig as ApplicationFixedStepConfig,
    FixedStepConfigError as ApplicationFixedStepConfigError,
    FixedStepPlan as ApplicationFixedStepPlan, FpsCounter as ApplicationFpsCounter,
    FpsSnapshot as ApplicationFpsSnapshot, FpsWindowError as ApplicationFpsWindowError,
    FrameClock as ApplicationFrameClock, FrameTime as ApplicationFrameTime,
    Stopwatch as ApplicationStopwatch, TimeScaleError as ApplicationTimeScaleError,
    UnixTimestamp as ApplicationUnixTimestamp, UnixTimestampError as ApplicationUnixTimestampError,
};
