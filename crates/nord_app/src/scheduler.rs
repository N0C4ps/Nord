//! Stable application facade for queued, prioritized CPU tasks.

pub use nord_scheduler::{
    CancellationToken as ApplicationCancellationToken,
    SchedulerBuildError as ApplicationSchedulerBuildError,
    SchedulerConfig as ApplicationSchedulerConfig,
    SchedulerSnapshot as ApplicationSchedulerSnapshot,
    SchedulerSubmitError as ApplicationSchedulerSubmitError,
    ShutdownError as ApplicationSchedulerShutdownError, ShutdownMode as ApplicationShutdownMode,
    TaskContext as ApplicationTaskContext, TaskExecutionError as ApplicationTaskExecutionError,
    TaskHandle as ApplicationTaskHandle, TaskId as ApplicationTaskId,
    TaskPriority as ApplicationTaskPriority, TaskScheduler as ApplicationTaskScheduler,
};
