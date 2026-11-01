//! Stable application-facing facade over Nord's logging infrastructure.

pub use nord_log::{
    FileRotation as ApplicationLogFileRotation, Instrument as ApplicationInstrument,
    JsonFileConfig as ApplicationJsonFileConfig, LoggerConfig as ApplicationLoggerConfig,
    LoggerDiagnostics as ApplicationLogDiagnostics, LoggerFlushError as ApplicationLogFlushError,
    LoggerInitError as ApplicationLoggerInitError,
    OpenTelemetryConfig as ApplicationOpenTelemetryConfig,
    QueueFullPolicy as ApplicationLogQueueFullPolicy, TerminalOutput as ApplicationTerminalOutput,
};

/// Process-lifetime owner of Nord's configured logging pipeline.
///
/// Store this service in the application container and retain it until the
/// process is shutting down so asynchronous sinks can drain and flush.
pub struct ApplicationLogger {
    runtime: nord_log::LoggerRuntime,
}

impl ApplicationLogger {
    /// Initialize the global logger and retain its asynchronous sink workers.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid configuration, file setup failures, or if
    /// another process-wide tracing subscriber is already installed.
    pub fn initialize(config: ApplicationLoggerConfig) -> Result<Self, ApplicationLoggerInitError> {
        nord_log::LoggerRuntime::initialize(config).map(|runtime| Self { runtime })
    }

    /// Current drop and sink error counters.
    #[must_use]
    pub fn diagnostics(&self) -> ApplicationLogDiagnostics {
        self.runtime.diagnostics()
    }

    /// Number of Nord's local sink workers, excluding optional telemetry and
    /// native-crash support processes.
    #[must_use]
    pub fn worker_count(&self) -> usize {
        self.runtime.worker_count()
    }

    /// Flush events already queued before the timeout expires.
    ///
    /// # Errors
    ///
    /// Returns a worker-stopped or timeout error if output cannot be flushed
    /// within the requested duration.
    pub fn flush(&self, timeout: std::time::Duration) -> Result<(), ApplicationLogFlushError> {
        self.runtime.flush(timeout)
    }

    /// Create a bounded best-effort callback that drains local logs from a
    /// crash hook before the panic report is written.
    pub fn panic_flush_callback(&self) -> impl Fn() + Send + Sync + 'static {
        self.runtime.panic_flush_callback()
    }
}
