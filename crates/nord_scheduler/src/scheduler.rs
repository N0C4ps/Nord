use nord_threading::ThreadPool;
use std::any::Any;
use std::collections::VecDeque;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::num::NonZeroUsize;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TryRecvError};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, Weak};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const QUEUED: u8 = 0;
const RUNNING: u8 = 1;
const FINISHED: u8 = 2;
const CANCEL_REQUESTED: u8 = 3;
const CANCELLED: u8 = 4;
const PRIORITY_COUNT: usize = 5;

/// Scheduling class. Higher classes are selected first while queued; no class
/// can preempt a task that has already started.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(usize)]
pub enum TaskPriority {
    /// Time-sensitive work that directly affects the next visible frame.
    Critical = 4,
    /// Important foreground work.
    High = 3,
    /// Normal simulation and game-system work.
    Normal = 2,
    /// Useful work that can wait briefly.
    Low = 1,
    /// Speculative or distant background work.
    Background = 0,
}

/// Bounded-queue and fairness settings for [`TaskScheduler`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulerConfig {
    queue_capacity: NonZeroUsize,
    max_in_flight: Option<NonZeroUsize>,
    priority_aging_interval: Duration,
}

impl SchedulerConfig {
    /// Set the maximum number of waiting tasks. Running tasks do not use queue
    /// capacity.
    #[must_use]
    pub fn with_queue_capacity(mut self, capacity: NonZeroUsize) -> Self {
        self.queue_capacity = capacity;
        self
    }

    /// Limit dispatched tasks. `None` uses the worker-pool size.
    #[must_use]
    pub fn with_max_in_flight(mut self, maximum: Option<NonZeroUsize>) -> Self {
        self.max_in_flight = maximum;
        self
    }

    /// Promote old queued work over time to prevent priority starvation.
    #[must_use]
    pub fn with_priority_aging_interval(mut self, interval: Duration) -> Self {
        self.priority_aging_interval = interval;
        self
    }

    /// Maximum number of tasks waiting to be dispatched.
    #[must_use]
    pub const fn queue_capacity(&self) -> NonZeroUsize {
        self.queue_capacity
    }

    /// Maximum number of submitted tasks dispatched to the worker pool.
    #[must_use]
    pub const fn max_in_flight(&self) -> Option<NonZeroUsize> {
        self.max_in_flight
    }

    /// Time needed for queued work to gain one priority level.
    #[must_use]
    pub const fn priority_aging_interval(&self) -> Duration {
        self.priority_aging_interval
    }
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            queue_capacity: NonZeroUsize::new(2_048).unwrap_or(NonZeroUsize::MIN),
            max_in_flight: None,
            priority_aging_interval: Duration::from_millis(250),
        }
    }
}

/// Unique identifier for one accepted scheduler task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TaskId(u64);

impl TaskId {
    /// Numeric task identity, unique within this scheduler instance.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl Display for TaskId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, formatter)
    }
}

/// Cooperative signal that task code checks at safe interruption points.
#[derive(Clone)]
pub struct CancellationToken {
    control: Arc<TaskControl>,
}

impl CancellationToken {
    /// Whether cancellation has been requested for this task.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        matches!(self.control.status.load(Ordering::Acquire), CANCEL_REQUESTED | CANCELLED)
    }
}

/// Context passed to every scheduled task.
#[derive(Clone)]
pub struct TaskContext {
    id: TaskId,
    cancellation: CancellationToken,
}

impl TaskContext {
    /// Stable task identity.
    #[must_use]
    pub const fn id(&self) -> TaskId {
        self.id
    }

    /// Cancellation token for polling inside longer-running work.
    #[must_use]
    pub fn cancellation(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    /// Return `Cancelled` when the task should stop at this safe point.
    ///
    /// # Errors
    ///
    /// Returns [`TaskExecutionError::Cancelled`] after a cancellation request.
    pub fn checkpoint(&self) -> Result<(), TaskExecutionError> {
        if self.cancellation.is_cancelled() { Err(TaskExecutionError::Cancelled) } else { Ok(()) }
    }
}

/// Result failure reported by a task handle.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TaskExecutionError {
    /// The task was cancelled before it started or observed a cooperative
    /// cancellation request while running.
    Cancelled,
    /// The scheduler stopped before it could deliver a task result.
    SchedulerStopped,
    /// The task panicked while the executable uses unwind panic behavior.
    Panicked(String),
}

impl Display for TaskExecutionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("task was cancelled"),
            Self::SchedulerStopped => {
                formatter.write_str("task scheduler stopped before completion")
            }
            Self::Panicked(message) => write!(formatter, "task panicked: {message}"),
        }
    }
}

impl Error for TaskExecutionError {}

/// Completion handle returned by [`TaskScheduler::submit`]. Dropping a handle
/// detaches observation; it does not cancel the task.
pub struct TaskHandle<Output> {
    id: TaskId,
    control: Arc<TaskControl>,
    receiver: Receiver<Result<Output, TaskExecutionError>>,
    cancellation_sender: SyncSender<Result<Output, TaskExecutionError>>,
}

impl<Output> TaskHandle<Output> {
    /// Identity of the submitted task.
    #[must_use]
    pub const fn id(&self) -> TaskId {
        self.id
    }

    /// Whether execution or cancellation has reached a terminal state.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        matches!(self.control.status.load(Ordering::Acquire), FINISHED | CANCELLED)
    }

    /// Request cancellation. A queued task is prevented from starting; a
    /// running task receives a cooperative signal and must check its context.
    /// Returns `true` only when this call made a new cancellation request.
    #[must_use]
    pub fn cancel(&self) -> bool {
        loop {
            let status = self.control.status.load(Ordering::Acquire);
            let next = match status {
                QUEUED => CANCELLED,
                RUNNING => CANCEL_REQUESTED,
                _ => return false,
            };
            if self
                .control
                .status
                .compare_exchange(status, next, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                if next == CANCELLED {
                    let _ = self.cancellation_sender.send(Err(TaskExecutionError::Cancelled));
                    if let Some(shared) = self.control.shared.upgrade() {
                        let mut state = lock(&shared.state);
                        state.cancelled = state.cancelled.saturating_add(1);
                        drop(state);
                        shared.changed.notify_all();
                    }
                } else if let Some(shared) = self.control.shared.upgrade() {
                    shared.changed.notify_all();
                }
                return true;
            }
        }
    }

    /// Wait for task completion and consume its result.
    ///
    /// # Errors
    ///
    /// Returns [`TaskExecutionError::Cancelled`] when cancellation wins, or
    /// [`TaskExecutionError::SchedulerStopped`] if the completion channel closes.
    pub fn wait(self) -> Result<Output, TaskExecutionError> {
        self.receiver.recv().unwrap_or(Err(TaskExecutionError::SchedulerStopped))
    }

    /// Poll for completion without blocking. A completed result is consumed.
    ///
    /// # Errors
    ///
    /// Returns [`TaskExecutionError::Cancelled`] or
    /// [`TaskExecutionError::SchedulerStopped`] when no output is available.
    pub fn try_take(&self) -> Result<Option<Output>, TaskExecutionError> {
        match self.receiver.try_recv() {
            Ok(result) => result.map(Some),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(TaskExecutionError::SchedulerStopped),
        }
    }

    /// Wait up to a duration for a result. A returned result is consumed.
    ///
    /// # Errors
    ///
    /// Returns [`TaskExecutionError::Cancelled`] or
    /// [`TaskExecutionError::SchedulerStopped`] when no output can be delivered.
    pub fn wait_timeout(&self, timeout: Duration) -> Result<Option<Output>, TaskExecutionError> {
        match self.receiver.recv_timeout(timeout) {
            Ok(result) => result.map(Some),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => Err(TaskExecutionError::SchedulerStopped),
        }
    }
}

/// Queue counters and live scheduler occupancy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchedulerSnapshot {
    /// Tasks accepted by the scheduler.
    pub submitted: u64,
    /// Tasks that produced a result.
    pub completed: u64,
    /// Tasks cancelled while queued or running.
    pub cancelled: u64,
    /// Tasks rejected because the queue was full or shutting down.
    pub rejected: u64,
    /// Tasks currently waiting in the priority queues.
    pub queued: usize,
    /// Tasks dispatched to the worker pool and not yet returned.
    pub in_flight: usize,
}

/// Choose how shutdown treats queued tasks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownMode {
    /// Stop accepting work, dispatch queued work, and wait for all tasks.
    Drain,
    /// Stop accepting work, cancel queued tasks, and wait for running tasks.
    CancelPending,
}

/// Errors while constructing the scheduler.
#[derive(Debug)]
#[non_exhaustive]
pub enum SchedulerBuildError {
    /// Configured in-flight count exceeds the backing pool worker count.
    MaxInFlightExceedsWorkers { requested: usize, workers: usize },
    /// Priority aging must be enabled with a positive interval.
    ZeroPriorityAgingInterval,
    /// The operating system could not create the dispatch thread.
    DispatcherThread(std::io::Error),
}

impl Display for SchedulerBuildError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MaxInFlightExceedsWorkers { requested, workers } => write!(
                formatter,
                "scheduler in-flight limit {requested} exceeds worker count {workers}"
            ),
            Self::ZeroPriorityAgingInterval => {
                formatter.write_str("scheduler priority aging interval must be greater than zero")
            }
            Self::DispatcherThread(source) => {
                write!(formatter, "could not start scheduler dispatcher thread: {source}")
            }
        }
    }
}

impl Error for SchedulerBuildError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::DispatcherThread(source) => Some(source),
            Self::MaxInFlightExceedsWorkers { .. } | Self::ZeroPriorityAgingInterval => None,
        }
    }
}

/// Errors returned when a submission is not accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SchedulerSubmitError {
    /// The bounded waiting queue has reached its configured capacity.
    QueueFull,
    /// The scheduler has entered shutdown and accepts no new work.
    ShuttingDown,
    /// The scheduler exhausted its unique task identity space.
    TaskIdExhausted,
}

impl Display for SchedulerSubmitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::QueueFull => formatter.write_str("scheduler queue is full"),
            Self::ShuttingDown => formatter.write_str("scheduler is shutting down"),
            Self::TaskIdExhausted => {
                formatter.write_str("scheduler task identifier space exhausted")
            }
        }
    }
}

impl Error for SchedulerSubmitError {}

/// Errors while waiting for scheduler shutdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ShutdownError {
    /// Shutdown cannot be joined from one of this scheduler's own worker tasks.
    CalledFromWorker,
    /// The dispatcher thread panicked unexpectedly.
    DispatcherPanicked,
}

impl Display for ShutdownError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CalledFromWorker => formatter
                .write_str("scheduler shutdown must be joined from outside its worker pool"),
            Self::DispatcherPanicked => formatter.write_str("scheduler dispatcher thread panicked"),
        }
    }
}

impl Error for ShutdownError {}

/// Priority scheduler backed by a bounded, aging queue and a Nord worker pool.
#[derive(Clone)]
pub struct TaskScheduler {
    core: Arc<SchedulerCore>,
}

impl TaskScheduler {
    /// Create a scheduler over an existing worker pool.
    ///
    /// # Errors
    ///
    /// Returns [`SchedulerBuildError`] for invalid limits or if the
    /// dispatcher thread cannot be started.
    pub fn new(
        pool: Arc<ThreadPool>,
        config: SchedulerConfig,
    ) -> Result<Self, SchedulerBuildError> {
        if config.priority_aging_interval.is_zero() {
            return Err(SchedulerBuildError::ZeroPriorityAgingInterval);
        }
        let workers = pool.worker_threads().get();
        let max_in_flight = config.max_in_flight.map_or(workers, NonZeroUsize::get);
        if max_in_flight > workers {
            return Err(SchedulerBuildError::MaxInFlightExceedsWorkers {
                requested: max_in_flight,
                workers,
            });
        }

        let shared = Arc::new(SchedulerShared {
            state: Mutex::new(SchedulerState::new()),
            changed: Condvar::new(),
            queue_capacity: config.queue_capacity.get(),
            priority_aging_interval: config.priority_aging_interval,
        });
        let dispatcher_shared = Arc::clone(&shared);
        let dispatcher_pool = Arc::clone(&pool);
        let dispatcher = thread::Builder::new()
            .name("nord-task-scheduler".to_owned())
            .spawn(move || {
                if catch_unwind(AssertUnwindSafe(|| {
                    dispatch_loop(&dispatcher_shared, &dispatcher_pool, max_in_flight);
                }))
                .is_err()
                {
                    fail_dispatcher(&dispatcher_shared);
                }
            })
            .map_err(SchedulerBuildError::DispatcherThread)?;

        Ok(Self {
            core: Arc::new(SchedulerCore {
                shared,
                pool,
                config,
                dispatcher: Mutex::new(Some(dispatcher)),
            }),
        })
    }

    /// Add a task to the bounded priority queue.
    ///
    /// If full, submission fails immediately; callers can choose their own
    /// retry, downgrade, or discard policy. The task closure should be
    /// CPU-bound and periodically check [`TaskContext::checkpoint`] if it may
    /// run long enough to become obsolete.
    ///
    /// # Errors
    ///
    /// Returns [`SchedulerSubmitError::QueueFull`] when the bounded queue is
    /// full, [`SchedulerSubmitError::ShuttingDown`] during shutdown, or
    /// [`SchedulerSubmitError::TaskIdExhausted`] if IDs are exhausted.
    pub fn submit<Output, Operation>(
        &self,
        priority: TaskPriority,
        operation: Operation,
    ) -> Result<TaskHandle<Output>, SchedulerSubmitError>
    where
        Output: Send + 'static,
        Operation: FnOnce(TaskContext) -> Output + Send + 'static,
    {
        let (sender, receiver) = mpsc::sync_channel(1);
        let cancellation_sender = sender.clone();
        let mut state = lock(&self.core.shared.state);
        prune_cancelled(&mut state);
        if !state.accepting {
            state.rejected = state.rejected.saturating_add(1);
            return Err(SchedulerSubmitError::ShuttingDown);
        }
        if queue_len(&state) >= self.core.shared.queue_capacity {
            state.rejected = state.rejected.saturating_add(1);
            return Err(SchedulerSubmitError::QueueFull);
        }
        if state.next_task_id == 0 {
            state.rejected = state.rejected.saturating_add(1);
            return Err(SchedulerSubmitError::TaskIdExhausted);
        }

        let id = TaskId(state.next_task_id);
        state.next_task_id = state.next_task_id.checked_add(1).unwrap_or(0);
        let control = Arc::new(TaskControl {
            status: AtomicU8::new(QUEUED),
            shared: Arc::downgrade(&self.core.shared),
        });
        let task_control = Arc::clone(&control);
        let task_sender = sender;
        let run: Box<dyn FnOnce() + Send + 'static> = Box::new(move || {
            let context = TaskContext {
                id,
                cancellation: CancellationToken { control: Arc::clone(&task_control) },
            };
            let outcome = catch_unwind(AssertUnwindSafe(|| operation(context)));
            let previous = task_control.status.swap(FINISHED, Ordering::AcqRel);
            let result = if previous == CANCEL_REQUESTED {
                Err(TaskExecutionError::Cancelled)
            } else {
                match outcome {
                    Ok(output) => Ok(output),
                    Err(payload) => {
                        Err(TaskExecutionError::Panicked(panic_message(payload.as_ref())))
                    }
                }
            };
            if let Some(shared) = task_control.shared.upgrade() {
                let mut state = lock(&shared.state);
                if matches!(&result, Err(TaskExecutionError::Cancelled)) {
                    state.cancelled = state.cancelled.saturating_add(1);
                } else {
                    state.completed = state.completed.saturating_add(1);
                }
                shared.changed.notify_all();
            }
            let _ = task_sender.send(result);
        });
        let cancel_sender_for_queue = cancellation_sender.clone();
        let cancel: Box<dyn FnOnce() + Send + 'static> = Box::new(move || {
            let _ = cancel_sender_for_queue.send(Err(TaskExecutionError::Cancelled));
        });

        state.queues[priority as usize].push_back(QueuedTask {
            id,
            priority,
            queued_at: Instant::now(),
            control: Arc::clone(&control),
            run: Some(run),
            cancel: Some(cancel),
        });
        state.submitted = state.submitted.saturating_add(1);
        drop(state);
        self.core.shared.changed.notify_all();

        Ok(TaskHandle { id, control, receiver, cancellation_sender })
    }

    /// Snapshot queue occupancy and cumulative scheduler counters.
    #[must_use]
    pub fn snapshot(&self) -> SchedulerSnapshot {
        let state = lock(&self.core.shared.state);
        SchedulerSnapshot {
            submitted: state.submitted,
            completed: state.completed,
            cancelled: state.cancelled,
            rejected: state.rejected,
            queued: state
                .queues
                .iter()
                .flatten()
                .filter(|task| task.control.status.load(Ordering::Acquire) == QUEUED)
                .count(),
            in_flight: state.in_flight,
        }
    }

    /// Borrow the configuration used by this scheduler.
    #[must_use]
    pub fn config(&self) -> &SchedulerConfig {
        &self.core.config
    }

    /// Stop accepting work and wait for the dispatcher and in-flight tasks.
    ///
    /// Must be called from outside this scheduler's worker pool. Running tasks
    /// need to return cooperatively; this call intentionally has no implicit
    /// timeout because abandoning live task state would hide incomplete work.
    ///
    /// # Errors
    ///
    /// Returns [`ShutdownError::CalledFromWorker`] when called from a worker,
    /// or [`ShutdownError::DispatcherPanicked`] if the dispatcher panicked.
    pub fn shutdown(&self, mode: ShutdownMode) -> Result<(), ShutdownError> {
        if self.core.pool.is_current_thread() {
            return Err(ShutdownError::CalledFromWorker);
        }
        request_shutdown(&self.core.shared, mode);
        let dispatcher = lock(&self.core.dispatcher).take();
        if let Some(dispatcher) = dispatcher
            && dispatcher.join().is_err()
        {
            return Err(ShutdownError::DispatcherPanicked);
        } else if !lock(&self.core.shared.state).dispatcher_finished {
            let mut state = lock(&self.core.shared.state);
            while !(state.dispatcher_finished || state.dispatcher_failed && state.in_flight == 0) {
                state = self
                    .core
                    .shared
                    .changed
                    .wait(state)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
        }
        if lock(&self.core.shared.state).dispatcher_failed {
            Err(ShutdownError::DispatcherPanicked)
        } else {
            Ok(())
        }
    }
}

struct SchedulerCore {
    shared: Arc<SchedulerShared>,
    pool: Arc<ThreadPool>,
    config: SchedulerConfig,
    dispatcher: Mutex<Option<JoinHandle<()>>>,
}

impl Drop for SchedulerCore {
    fn drop(&mut self) {
        request_shutdown(&self.shared, ShutdownMode::CancelPending);
        // Detach on implicit drop: explicit `shutdown` is the join point. This
        // prevents a destructor on a worker from joining its own outstanding
        // task; the dispatcher retains the shared state and pool until drained.
        let dispatcher =
            self.dispatcher.get_mut().unwrap_or_else(std::sync::PoisonError::into_inner).take();
        drop(dispatcher);
    }
}

struct SchedulerShared {
    state: Mutex<SchedulerState>,
    changed: Condvar,
    queue_capacity: usize,
    priority_aging_interval: Duration,
}

struct SchedulerState {
    queues: [VecDeque<QueuedTask>; PRIORITY_COUNT],
    accepting: bool,
    shutdown_mode: Option<ShutdownMode>,
    next_task_id: u64,
    in_flight: usize,
    dispatcher_finished: bool,
    dispatcher_failed: bool,
    submitted: u64,
    completed: u64,
    cancelled: u64,
    rejected: u64,
}

impl SchedulerState {
    fn new() -> Self {
        Self {
            queues: std::array::from_fn(|_| VecDeque::new()),
            accepting: true,
            shutdown_mode: None,
            next_task_id: 1,
            in_flight: 0,
            dispatcher_finished: false,
            dispatcher_failed: false,
            submitted: 0,
            completed: 0,
            cancelled: 0,
            rejected: 0,
        }
    }
}

struct QueuedTask {
    id: TaskId,
    priority: TaskPriority,
    queued_at: Instant,
    control: Arc<TaskControl>,
    run: Option<Box<dyn FnOnce() + Send + 'static>>,
    cancel: Option<Box<dyn FnOnce() + Send + 'static>>,
}

struct TaskControl {
    status: AtomicU8,
    shared: Weak<SchedulerShared>,
}

fn dispatch_loop(shared: &Arc<SchedulerShared>, pool: &Arc<ThreadPool>, max_in_flight: usize) {
    loop {
        let task = {
            let mut state = lock(&shared.state);
            loop {
                prune_cancelled(&mut state);
                if state.shutdown_mode.is_some()
                    && state.queues.iter().all(VecDeque::is_empty)
                    && state.in_flight == 0
                {
                    state.dispatcher_finished = true;
                    shared.changed.notify_all();
                    return;
                }
                if state.in_flight < max_in_flight
                    && let Some(task) = pop_next(&mut state, shared.priority_aging_interval)
                {
                    if task
                        .control
                        .status
                        .compare_exchange(QUEUED, RUNNING, Ordering::AcqRel, Ordering::Acquire)
                        .is_ok()
                    {
                        state.in_flight = state.in_flight.saturating_add(1);
                        break Some(task);
                    }
                    continue;
                }
                state =
                    shared.changed.wait(state).unwrap_or_else(std::sync::PoisonError::into_inner);
            }
        };

        if let Some(mut task) = task
            && let Some(run) = task.run.take()
        {
            let shared_for_task = Arc::clone(shared);
            pool.spawn(move || {
                run();
                let mut state = lock(&shared_for_task.state);
                state.in_flight = state.in_flight.saturating_sub(1);
                shared_for_task.changed.notify_all();
            });
        }
    }
}

fn request_shutdown(shared: &Arc<SchedulerShared>, mode: ShutdownMode) {
    let pending = {
        let mut state = lock(&shared.state);
        if state.shutdown_mode.is_none() {
            state.accepting = false;
            state.shutdown_mode = Some(mode);
        }
        if state.shutdown_mode == Some(ShutdownMode::CancelPending) {
            state.queues.iter_mut().flat_map(|queue| queue.drain(..)).collect::<Vec<_>>()
        } else {
            Vec::new()
        }
    };
    for mut task in pending {
        if task
            .control
            .status
            .compare_exchange(QUEUED, CANCELLED, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            let mut state = lock(&shared.state);
            state.cancelled = state.cancelled.saturating_add(1);
            drop(state);
            if let Some(cancel) = task.cancel.take() {
                cancel();
            }
        }
    }
    shared.changed.notify_all();
}

fn fail_dispatcher(shared: &Arc<SchedulerShared>) {
    let pending = {
        let mut state = lock(&shared.state);
        state.accepting = false;
        state.shutdown_mode = Some(ShutdownMode::CancelPending);
        state.dispatcher_failed = true;
        state.queues.iter_mut().flat_map(|queue| queue.drain(..)).collect::<Vec<_>>()
    };
    for mut task in pending {
        if task
            .control
            .status
            .compare_exchange(QUEUED, CANCELLED, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            let mut state = lock(&shared.state);
            state.cancelled = state.cancelled.saturating_add(1);
            drop(state);
            if let Some(cancel) = task.cancel.take() {
                cancel();
            }
        }
    }
    shared.changed.notify_all();
}

fn pop_next(state: &mut SchedulerState, aging_interval: Duration) -> Option<QueuedTask> {
    let now = Instant::now();
    let aging_nanos = aging_interval.as_nanos();
    let mut selected: Option<(usize, u8, u64)> = None;
    for (queue_index, queue) in state.queues.iter().enumerate() {
        let Some(task) = queue.front() else {
            continue;
        };
        let age_levels = now.saturating_duration_since(task.queued_at).as_nanos() / aging_nanos;
        let age_levels = u8::try_from(age_levels.min(PRIORITY_COUNT as u128)).unwrap_or(u8::MAX);
        let effective_priority = (task.priority as u8).saturating_add(age_levels).min(4);
        match selected {
            None => selected = Some((queue_index, effective_priority, task.id.get())),
            Some((_, best_priority, best_id))
                if effective_priority > best_priority
                    || (effective_priority == best_priority && task.id.get() < best_id) =>
            {
                selected = Some((queue_index, effective_priority, task.id.get()));
            }
            Some(_) => {}
        }
    }
    selected.and_then(|(queue_index, _, _)| state.queues[queue_index].pop_front())
}

fn prune_cancelled(state: &mut SchedulerState) {
    for queue in &mut state.queues {
        queue.retain(|task| task.control.status.load(Ordering::Acquire) == QUEUED);
    }
}

fn queue_len(state: &SchedulerState) -> usize {
    state.queues.iter().map(VecDeque::len).sum()
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn panic_message(payload: &(dyn Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else if let Some(message) = payload.downcast_ref::<&'static str>() {
        (*message).to_owned()
    } else {
        "task panicked with a non-string payload".to_owned()
    }
}
