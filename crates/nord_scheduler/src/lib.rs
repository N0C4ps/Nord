//! Bounded, priority-aware task submission on Nord's CPU worker pool.
//!
//! Priorities affect queued work only. Running CPU tasks are not preempted;
//! cancellation is cooperative and observed through [`CancellationToken`].

mod scheduler;

pub use scheduler::{
    CancellationToken, SchedulerBuildError, SchedulerConfig, SchedulerSnapshot,
    SchedulerSubmitError, ShutdownError, ShutdownMode, TaskContext, TaskExecutionError, TaskHandle,
    TaskId, TaskPriority, TaskScheduler,
};
