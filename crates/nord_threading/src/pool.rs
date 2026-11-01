use rayon::{ThreadPool as RayonThreadPool, ThreadPoolBuilder};
use std::num::NonZeroUsize;
use std::thread;

/// Configuration for an owned worker pool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadPoolConfig {
    worker_threads: NonZeroUsize,
    stack_size: Option<NonZeroUsize>,
    thread_name_prefix: String,
}

impl ThreadPoolConfig {
    /// Configure the exact number of worker threads.
    #[must_use]
    pub fn with_worker_threads(mut self, worker_threads: NonZeroUsize) -> Self {
        self.worker_threads = worker_threads;
        self
    }

    /// Configure worker stack size in bytes. `None` keeps the platform default.
    #[must_use]
    pub fn with_stack_size(mut self, stack_size: Option<NonZeroUsize>) -> Self {
        self.stack_size = stack_size;
        self
    }

    /// Set the prefix for stable, diagnosable worker names.
    #[must_use]
    pub fn with_thread_name_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.thread_name_prefix = prefix.into();
        self
    }

    /// Number of dedicated workers configured for the pool.
    #[must_use]
    pub const fn worker_threads(&self) -> NonZeroUsize {
        self.worker_threads
    }

    /// Optional worker stack size in bytes.
    #[must_use]
    pub const fn stack_size(&self) -> Option<NonZeroUsize> {
        self.stack_size
    }

    /// Worker thread name prefix.
    #[must_use]
    pub fn thread_name_prefix(&self) -> &str {
        &self.thread_name_prefix
    }
}

impl Default for ThreadPoolConfig {
    fn default() -> Self {
        // Leave a logical processor for the application/render thread where
        // possible. One worker is still useful on single-processor systems.
        let available = thread::available_parallelism().map_or(1, NonZeroUsize::get);
        let workers = available.saturating_sub(1).max(1);
        Self {
            worker_threads: NonZeroUsize::new(workers).unwrap_or(NonZeroUsize::MIN),
            stack_size: None,
            thread_name_prefix: "nord-worker".to_owned(),
        }
    }
}

/// Error while creating a worker pool.
#[derive(Debug)]
#[non_exhaustive]
pub struct ThreadPoolBuildError {
    source: rayon::ThreadPoolBuildError,
}

impl std::fmt::Display for ThreadPoolBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "could not create Nord worker pool: {}", self.source)
    }
}

impl std::error::Error for ThreadPoolBuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

/// Owned Rayon pool behind the stable Nord threading API.
pub struct ThreadPool {
    inner: RayonThreadPool,
    config: ThreadPoolConfig,
}

impl ThreadPool {
    /// Build an independently configured pool.
    ///
    /// # Errors
    ///
    /// Returns [`ThreadPoolBuildError`] if the operating system cannot create
    /// the requested worker threads or if Rayon rejects the configuration.
    pub fn new(config: ThreadPoolConfig) -> Result<Self, ThreadPoolBuildError> {
        let prefix = config.thread_name_prefix.clone();
        let mut builder = ThreadPoolBuilder::new()
            .num_threads(config.worker_threads.get())
            .thread_name(move |index| format!("{prefix}-{index}"));
        if let Some(stack_size) = config.stack_size {
            builder = builder.stack_size(stack_size.get());
        }
        let inner = builder.build().map_err(|source| ThreadPoolBuildError { source })?;
        Ok(Self { inner, config })
    }

    /// Execute an operation within this pool. Nested calls reuse the same pool.
    #[must_use]
    pub fn install<Operation, Output>(&self, operation: Operation) -> Output
    where
        Operation: FnOnce() -> Output + Send,
        Output: Send,
    {
        self.inner.install(operation)
    }

    /// Enqueue an owned operation for asynchronous execution on this pool.
    ///
    /// The operation must be `'static` because it may outlive the caller.
    /// Panics in detached operations follow Rayon’s configured panic policy.
    pub fn spawn<Operation>(&self, operation: Operation)
    where
        Operation: FnOnce() + Send + 'static,
    {
        self.inner.spawn(operation);
    }

    /// Whether the caller is currently executing on this pool.
    #[must_use]
    pub fn is_current_thread(&self) -> bool {
        self.inner.current_thread_index().is_some()
    }

    /// Configured dedicated worker count.
    #[must_use]
    pub const fn worker_threads(&self) -> NonZeroUsize {
        self.config.worker_threads
    }

    /// Return the current worker's stable index when called from this pool.
    #[must_use]
    pub fn current_worker_index(&self) -> Option<usize> {
        self.inner.current_thread_index()
    }

    /// Borrow the effective configuration used to create this pool.
    #[must_use]
    pub const fn config(&self) -> &ThreadPoolConfig {
        &self.config
    }
}
