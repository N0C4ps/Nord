//! Structured asynchronous logging infrastructure for Nord Tech.
//!
//! Events and spans are captured into owned, typed records on producer
//! threads. Formatting, JSON serialization, and sink I/O run on a dedicated
//! worker so file and terminal writes stay off latency-sensitive threads.

use crossbeam_channel::{Receiver, Sender, TrySendError, bounded, unbounded};
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::{LogExporter, SpanExporter, WithExportConfig};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::logs::SdkLoggerProvider;
use opentelemetry_sdk::trace::SdkTracerProvider;
use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};
use smallvec::SmallVec;
use std::error::Error;
use std::fmt::{self, Display, Formatter, Write as FmtWrite};
use std::fs;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Subscriber};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

pub use tracing::{
    Instrument, Level, debug, error, event, info, info_span, instrument, span, trace, warn,
};

/// Destination for readable terminal output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalOutput {
    /// Write to standard error, the conventional diagnostics stream.
    Stderr,
    /// Write to standard output.
    Stdout,
}

/// Behavior when the bounded asynchronous event queue reaches capacity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueFullPolicy {
    /// Drop new events to keep latency-sensitive producer threads moving.
    Drop,
    /// Apply backpressure until the worker accepts the event.
    Block,
}

/// Rotation schedule for structured JSON log files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileRotation {
    /// Keep one file until manually moved or deleted.
    Never,
    /// Start a new file each day.
    Daily,
    /// Start a new file each hour.
    Hourly,
    /// Start a new file each week (Sunday, UTC).
    Weekly,
}

impl From<FileRotation> for Rotation {
    fn from(rotation: FileRotation) -> Self {
        match rotation {
            FileRotation::Never => Self::NEVER,
            FileRotation::Daily => Self::DAILY,
            FileRotation::Hourly => Self::HOURLY,
            FileRotation::Weekly => Self::WEEKLY,
        }
    }
}

/// JSON Lines output location and retention policy.
#[derive(Debug, Clone)]
pub struct JsonFileConfig {
    directory: PathBuf,
    filename_prefix: String,
    rotation: FileRotation,
    max_files: Option<usize>,
}

impl JsonFileConfig {
    /// Create a daily rolling JSON Lines sink retaining up to 14 files.
    #[must_use]
    pub fn daily(directory: impl Into<PathBuf>, filename_prefix: impl Into<String>) -> Self {
        Self {
            directory: directory.into(),
            filename_prefix: filename_prefix.into(),
            rotation: FileRotation::Daily,
            max_files: Some(14),
        }
    }

    /// Set the rotation schedule.
    #[must_use]
    pub const fn with_rotation(mut self, rotation: FileRotation) -> Self {
        self.rotation = rotation;
        self
    }

    /// Limit retained files; `None` disables retention.
    #[must_use]
    pub const fn with_max_files(mut self, max_files: Option<usize>) -> Self {
        self.max_files = max_files;
        self
    }

    /// Directory where the sink creates its files.
    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }
}

/// Optional OpenTelemetry Protocol over HTTP exporter configuration.
#[derive(Debug, Clone)]
pub struct OpenTelemetryConfig {
    endpoint: String,
    logs_endpoint: Option<String>,
    traces_endpoint: Option<String>,
}

impl OpenTelemetryConfig {
    /// Enable OTLP/HTTP export to the supplied Collector endpoint.
    ///
    /// Use `http://localhost:4318` for a local OpenTelemetry Collector.
    #[must_use]
    pub fn otlp_http(endpoint: impl Into<String>) -> Self {
        let endpoint = endpoint.into();
        let endpoint = endpoint.trim_end_matches('/');
        let endpoint = endpoint
            .strip_suffix("/v1/logs")
            .or_else(|| endpoint.strip_suffix("/v1/traces"))
            .unwrap_or(endpoint)
            .trim_end_matches('/');
        Self { endpoint: endpoint.to_owned(), logs_endpoint: None, traces_endpoint: None }
    }

    fn signal_endpoint(&self, signal: &str) -> String {
        let configured = match signal {
            "logs" => self.logs_endpoint.as_deref(),
            "traces" => self.traces_endpoint.as_deref(),
            _ => None,
        };
        configured.map_or_else(|| format!("{}/v1/{signal}", self.endpoint), str::to_owned)
    }

    /// Enable OTLP/HTTP when a standard OTLP endpoint environment variable is set.
    ///
    /// If an OTLP protocol variable is set, it must be `http/protobuf`.
    ///
    /// # Errors
    ///
    /// Returns an error if an environment variable requests a transport or
    /// encoding that this logger was not built to support.
    pub fn from_environment() -> Result<Option<Self>, String> {
        for key in [
            "OTEL_EXPORTER_OTLP_LOGS_PROTOCOL",
            "OTEL_EXPORTER_OTLP_TRACES_PROTOCOL",
            "OTEL_EXPORTER_OTLP_PROTOCOL",
        ] {
            if let Ok(protocol) = std::env::var(key)
                && protocol != "http/protobuf"
            {
                return Err(format!(
                    "Nord currently supports OTLP/HTTP protobuf, but `{key}` is `{protocol}`"
                ));
            }
        }

        let logs_endpoint = std::env::var("OTEL_EXPORTER_OTLP_LOGS_ENDPOINT").ok();
        let traces_endpoint = std::env::var("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT").ok();
        let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
            .ok()
            .or_else(|| logs_endpoint.clone())
            .or_else(|| traces_endpoint.clone());
        let Some(endpoint) = endpoint else {
            return Ok(None);
        };
        let mut config = Self::otlp_http(endpoint);
        config.logs_endpoint =
            logs_endpoint.map(|endpoint| endpoint.trim_end_matches('/').to_owned());
        config.traces_endpoint =
            traces_endpoint.map(|endpoint| endpoint.trim_end_matches('/').to_owned());
        Ok(Some(config))
    }
}

/// Configuration for the process-wide logging pipeline.
#[derive(Debug, Clone)]
pub struct LoggerConfig {
    terminal: Option<TerminalOutput>,
    json_file: Option<JsonFileConfig>,
    open_telemetry: Option<OpenTelemetryConfig>,
    filter: Option<String>,
    queue_capacity: usize,
    queue_full_policy: QueueFullPolicy,
}

impl Default for LoggerConfig {
    fn default() -> Self {
        Self {
            terminal: Some(TerminalOutput::Stderr),
            json_file: None,
            open_telemetry: None,
            filter: None,
            queue_capacity: 16_384,
            queue_full_policy: QueueFullPolicy::Drop,
        }
    }
}

impl LoggerConfig {
    /// Create a configuration with readable terminal output enabled.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Select a terminal stream.
    #[must_use]
    pub const fn with_terminal(mut self, output: TerminalOutput) -> Self {
        self.terminal = Some(output);
        self
    }

    /// Disable terminal output.
    #[must_use]
    pub const fn without_terminal(mut self) -> Self {
        self.terminal = None;
        self
    }

    /// Configure a rolling JSON Lines sink.
    #[must_use]
    pub fn with_json_file(mut self, config: JsonFileConfig) -> Self {
        self.json_file = Some(config);
        self
    }

    /// Enable batched OpenTelemetry log export over OTLP/HTTP.
    #[must_use]
    pub fn with_open_telemetry(mut self, config: Option<OpenTelemetryConfig>) -> Self {
        self.open_telemetry = config;
        self
    }

    /// Set the `tracing-subscriber` filter directive.
    ///
    /// When omitted, the logger reads `NORD_LOG`, then `RUST_LOG`, then uses
    /// `info`.
    #[must_use]
    pub fn with_filter(mut self, filter: impl Into<String>) -> Self {
        self.filter = Some(filter.into());
        self
    }

    /// Configure the bounded queue and its full-queue behavior.
    #[must_use]
    pub const fn with_queue(mut self, capacity: usize, policy: QueueFullPolicy) -> Self {
        self.queue_capacity = capacity;
        self.queue_full_policy = policy;
        self
    }
}

/// Failure to construct or install the logging pipeline.
#[derive(Debug)]
#[non_exhaustive]
pub enum LoggerInitError {
    /// No terminal or JSON sink was configured.
    NoOutput,
    /// The asynchronous queue capacity must be positive.
    InvalidQueueCapacity,
    /// The logging filter directive could not be parsed.
    InvalidFilter(String),
    /// A configured file sink could not be opened.
    FileSink { path: PathBuf, source: io::Error },
    /// The OpenTelemetry exporter could not be initialized.
    OpenTelemetry(String),
    /// The dedicated worker thread could not be created.
    WorkerSpawn(io::Error),
    /// A global tracing subscriber or log bridge is already installed.
    AlreadyInitialized(String),
}

impl Display for LoggerInitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoOutput => {
                formatter.write_str("at least one Nord log output must be configured")
            }
            Self::InvalidQueueCapacity => {
                formatter.write_str("Nord log queue capacity must be positive")
            }
            Self::InvalidFilter(message) => write!(formatter, "invalid Nord log filter: {message}"),
            Self::FileSink { path, source } => {
                write!(formatter, "cannot initialize log file sink `{}`: {source}", path.display())
            }
            Self::OpenTelemetry(message) => {
                write!(formatter, "cannot initialize OpenTelemetry exporter: {message}")
            }
            Self::WorkerSpawn(source) => {
                write!(formatter, "cannot start Nord log worker: {source}")
            }
            Self::AlreadyInitialized(message) => {
                write!(formatter, "the global logging pipeline is already initialized: {message}")
            }
        }
    }
}

impl Error for LoggerInitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::FileSink { source, .. } | Self::WorkerSpawn(source) => Some(source),
            _ => None,
        }
    }
}

/// Failure to flush queued log output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoggerFlushError {
    /// The logging worker stopped before acknowledging the flush.
    WorkerStopped,
    /// The worker did not finish flushing before the supplied timeout.
    TimedOut,
    /// The OpenTelemetry exporter failed while flushing a batch.
    TelemetryFailed,
}

impl Display for LoggerFlushError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::WorkerStopped => formatter.write_str("Nord log worker stopped"),
            Self::TimedOut => formatter.write_str("Nord log flush timed out"),
            Self::TelemetryFailed => formatter.write_str("Nord OpenTelemetry export failed"),
        }
    }
}

impl Error for LoggerFlushError {}

/// Snapshot of asynchronous logger health counters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LoggerDiagnostics {
    /// Number of events discarded because the bounded queue was full.
    pub dropped_events: u64,
    /// Number of failed writes or flushes on configured sinks.
    pub sink_errors: u64,
    /// Number of failed explicit OpenTelemetry flushes.
    pub telemetry_export_errors: u64,
}

/// Process lifetime guard for the global asynchronous logging pipeline.
///
/// Keep this value alive for the whole application. Dropping it shuts down the
/// optional OTLP processor, drains queued events, flushes local sinks, and
/// joins the worker thread.
#[must_use = "keep the logger runtime alive until the application is shutting down"]
pub struct LoggerRuntime {
    controls: Sender<WorkerControl>,
    worker: Option<JoinHandle<()>>,
    diagnostics: Arc<DiagnosticsCounters>,
    telemetry: Option<SdkLoggerProvider>,
    traces: Option<SdkTracerProvider>,
}

impl LoggerRuntime {
    /// Install configured sinks, filters, the `log` bridge, and optional OTLP
    /// export.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid configuration, file or OTLP setup failures,
    /// worker creation failures, or if a global subscriber was already installed.
    pub fn initialize(config: LoggerConfig) -> Result<Self, LoggerInitError> {
        if config.terminal.is_none() && config.json_file.is_none() {
            return Err(LoggerInitError::NoOutput);
        }
        if config.queue_capacity == 0 {
            return Err(LoggerInitError::InvalidQueueCapacity);
        }

        let filter_text = config
            .filter
            .or_else(|| std::env::var("NORD_LOG").ok())
            .or_else(|| std::env::var("RUST_LOG").ok())
            .unwrap_or_else(|| "info".to_owned());
        let filter = EnvFilter::try_new(&filter_text)
            .map_err(|error| LoggerInitError::InvalidFilter(error.to_string()))?;

        let terminal = config.terminal.map(terminal_writer);
        let json = config.json_file.map(create_file_appender).transpose()?;
        let (telemetry, traces) = create_telemetry(config.open_telemetry)?;
        let telemetry_layer = telemetry.as_ref().map(OpenTelemetryTracingBridge::new);
        let trace_layer = traces
            .as_ref()
            .map(|provider| tracing_opentelemetry::layer().with_tracer(provider.tracer("nord")));

        let (events_tx, events_rx) = bounded(config.queue_capacity);
        let (controls_tx, controls_rx) = unbounded();
        let diagnostics = Arc::new(DiagnosticsCounters::default());
        let start = Instant::now();
        let worker_diagnostics = Arc::clone(&diagnostics);
        let worker = thread::Builder::new()
            .name("nord-log-worker".to_owned())
            .spawn(move || run_worker(&events_rx, &controls_rx, terminal, json, worker_diagnostics))
            .map_err(LoggerInitError::WorkerSpawn)?;

        let layer = NordLogLayer {
            events: events_tx,
            policy: config.queue_full_policy,
            diagnostics: Arc::clone(&diagnostics),
            start,
        };
        let subscriber = tracing_subscriber::registry()
            .with(filter)
            .with(trace_layer)
            .with(telemetry_layer)
            .with(layer);
        if let Err(error) = subscriber.try_init() {
            let _ = controls_tx.send(WorkerControl::Shutdown);
            let _ = worker.join();
            return Err(LoggerInitError::AlreadyInitialized(error.to_string()));
        }

        Ok(Self { controls: controls_tx, worker: Some(worker), diagnostics, telemetry, traces })
    }

    /// Current drop and sink error counters.
    #[must_use]
    pub fn diagnostics(&self) -> LoggerDiagnostics {
        LoggerDiagnostics {
            dropped_events: self.diagnostics.dropped_events.load(Ordering::Relaxed),
            sink_errors: self.diagnostics.sink_errors.load(Ordering::Relaxed),
            telemetry_export_errors: self
                .diagnostics
                .telemetry_export_errors
                .load(Ordering::Relaxed),
        }
    }

    /// Flush events already queued before the timeout expires.
    ///
    /// # Errors
    ///
    /// Returns [`LoggerFlushError::TimedOut`] if the sink worker does not
    /// acknowledge the flush in time, [`LoggerFlushError::WorkerStopped`] if
    /// it has exited, or [`LoggerFlushError::TelemetryFailed`] if OTLP export
    /// fails.
    pub fn flush(&self, timeout: Duration) -> Result<(), LoggerFlushError> {
        let (ack_tx, ack_rx) = bounded(1);
        self.controls
            .send(WorkerControl::Flush(ack_tx))
            .map_err(|_| LoggerFlushError::WorkerStopped)?;
        ack_rx.recv_timeout(timeout).map_err(|error| match error {
            crossbeam_channel::RecvTimeoutError::Timeout => LoggerFlushError::TimedOut,
            crossbeam_channel::RecvTimeoutError::Disconnected => LoggerFlushError::WorkerStopped,
        })?;
        let logs_failed =
            self.telemetry.as_ref().is_some_and(|provider| provider.force_flush().is_err());
        let traces_failed =
            self.traces.as_ref().is_some_and(|provider| provider.force_flush().is_err());
        if logs_failed || traces_failed {
            self.diagnostics.telemetry_export_errors.fetch_add(1, Ordering::Relaxed);
            return Err(LoggerFlushError::TelemetryFailed);
        }
        Ok(())
    }

    /// Create a bounded best-effort callback for flushing local sinks in a
    /// crash hook. The returned closure owns only a channel sender and waits
    /// at most 250 ms for the logger worker.
    pub fn panic_flush_callback(&self) -> impl Fn() + Send + Sync + 'static {
        let controls = self.controls.clone();
        move || flush_for_crash(&controls)
    }

    /// Number of Nord's own local sink workers, excluding optional telemetry
    /// processors and the separate native-crash monitor process.
    #[must_use]
    pub const fn worker_count(&self) -> usize {
        1
    }
}

impl Drop for LoggerRuntime {
    fn drop(&mut self) {
        if let Some(traces) = &self.traces {
            let _ = traces.shutdown_with_timeout(Duration::from_secs(2));
        }
        if let Some(telemetry) = &self.telemetry {
            let _ = telemetry.shutdown_with_timeout(Duration::from_secs(2));
        }
        let _ = self.controls.send(WorkerControl::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn create_telemetry(
    config: Option<OpenTelemetryConfig>,
) -> Result<(Option<SdkLoggerProvider>, Option<SdkTracerProvider>), LoggerInitError> {
    let Some(config) = config else {
        return Ok((None, None));
    };
    let resource = Resource::builder().with_service_name("nord").build();
    let logs_exporter = LogExporter::builder()
        .with_http()
        .with_endpoint(config.signal_endpoint("logs"))
        .with_timeout(Duration::from_secs(2))
        .build()
        .map_err(|error| LoggerInitError::OpenTelemetry(error.to_string()))?;
    let spans_exporter = SpanExporter::builder()
        .with_http()
        .with_endpoint(config.signal_endpoint("traces"))
        .with_timeout(Duration::from_secs(2))
        .build()
        .map_err(|error| LoggerInitError::OpenTelemetry(error.to_string()))?;
    let telemetry = SdkLoggerProvider::builder()
        .with_resource(resource.clone())
        .with_batch_exporter(logs_exporter)
        .build();
    let traces = SdkTracerProvider::builder()
        .with_resource(resource)
        .with_batch_exporter(spans_exporter)
        .build();
    Ok((Some(telemetry), Some(traces)))
}

#[derive(Default)]
struct DiagnosticsCounters {
    dropped_events: AtomicU64,
    sink_errors: AtomicU64,
    telemetry_export_errors: AtomicU64,
}

struct NordLogLayer {
    events: Sender<LogRecord>,
    policy: QueueFullPolicy,
    diagnostics: Arc<DiagnosticsCounters>,
    start: Instant,
}

#[derive(Clone, Serialize)]
struct SpanContext {
    name: &'static str,
    fields: Fields,
}

impl<S> Layer<S> for NordLogLayer
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    fn on_new_span(&self, attributes: &Attributes<'_>, id: &Id, context: Context<'_, S>) {
        let mut visitor = FieldVisitor::default();
        attributes.record(&mut visitor);
        if let Some(span) = context.span(id) {
            span.extensions_mut()
                .insert(SpanContext { name: attributes.metadata().name(), fields: visitor.fields });
        }
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, context: Context<'_, S>) {
        let mut visitor = FieldVisitor::default();
        values.record(&mut visitor);
        if let Some(span) = context.span(id)
            && let Some(span_context) = span.extensions_mut().get_mut::<SpanContext>()
        {
            span_context.fields.append(visitor.fields);
        }
    }

    fn on_event(&self, event: &Event<'_>, context: Context<'_, S>) {
        let metadata = event.metadata();
        let mut visitor = FieldVisitor::default();
        event.record(&mut visitor);
        let spans = context
            .event_scope(event)
            .map(|scope| {
                scope
                    .from_root()
                    .filter_map(|span| span.extensions().get::<SpanContext>().cloned())
                    .collect::<SmallVec<[SpanContext; 8]>>()
            })
            .unwrap_or_default();
        let timestamp_unix_ns =
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        let elapsed_ns = self.start.elapsed().as_nanos();
        let thread = thread::current();
        let mut fields = visitor.fields;
        let event_name = match fields.remove("event_name") {
            Some(LogValue::Text(name)) => name,
            _ => SmallText::new(metadata.name()),
        };
        let record = LogRecord {
            sequence: None,
            timestamp_unix_ns,
            elapsed_ns,
            severity_text: severity_text(*metadata.level()),
            severity_number: severity_number(*metadata.level()),
            event_name,
            target: metadata.target(),
            file: metadata.file(),
            line: metadata.line(),
            thread_id: thread.id(),
            thread_name: thread.name().map(SmallText::new),
            fields,
            spans,
        };

        let result = match self.policy {
            QueueFullPolicy::Drop => self.events.try_send(record).map_err(|error| match error {
                TrySendError::Full(_) => QueueSendError::Full,
                TrySendError::Disconnected(_) => QueueSendError::Disconnected,
            }),
            QueueFullPolicy::Block => {
                self.events.send(record).map_err(|_| QueueSendError::Disconnected)
            }
        };
        match result {
            Ok(()) => {}
            Err(QueueSendError::Full | QueueSendError::Disconnected) => {
                self.diagnostics.dropped_events.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

#[derive(Clone, Copy)]
enum QueueSendError {
    Full,
    Disconnected,
}

#[derive(Clone, Serialize)]
#[serde(untagged)]
enum LogValue {
    Signed(i64),
    Unsigned(u64),
    Float(Option<f64>),
    Bool(bool),
    Text(SmallText),
}

#[derive(Clone)]
struct LogField {
    key: &'static str,
    value: LogValue,
}

#[derive(Clone, Default)]
struct Fields(SmallVec<[LogField; 8]>);

impl Fields {
    fn insert(&mut self, key: &'static str, value: LogValue) {
        if let Some(field) = self.0.iter_mut().find(|field| field.key == key) {
            field.value = value;
        } else {
            self.0.push(LogField { key, value });
        }
    }

    fn remove(&mut self, key: &str) -> Option<LogValue> {
        self.0.iter().position(|field| field.key == key).map(|index| self.0.remove(index).value)
    }

    fn get(&self, key: &str) -> Option<&LogValue> {
        self.0.iter().find(|field| field.key == key).map(|field| &field.value)
    }

    fn iter(&self) -> impl Iterator<Item = (&'static str, &LogValue)> {
        self.0.iter().map(|field| (field.key, &field.value))
    }

    fn append(&mut self, other: Self) {
        for field in other.0 {
            self.insert(field.key, field.value);
        }
    }
}

impl Extend<LogField> for Fields {
    fn extend<T: IntoIterator<Item = LogField>>(&mut self, iter: T) {
        for field in iter {
            self.insert(field.key, field.value);
        }
    }
}

impl Serialize for Fields {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (key, value) in self.iter() {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

#[derive(Clone, Default)]
struct SmallText(SmallVec<[u8; 48]>);

impl SmallText {
    fn new(value: &str) -> Self {
        Self(SmallVec::from_slice(value.as_bytes()))
    }

    fn as_str(&self) -> &str {
        // Every value enters through UTF-8 `str` bytes or `fmt::Write::write_str`.
        std::str::from_utf8(&self.0).expect("SmallText only stores valid UTF-8")
    }
}

impl FmtWrite for SmallText {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        self.0.extend_from_slice(value.as_bytes());
        Ok(())
    }
}

impl Display for SmallText {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Serialize for SmallText {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

#[derive(Default)]
struct FieldVisitor {
    fields: Fields,
}

impl FieldVisitor {
    fn insert(&mut self, field: &Field, value: LogValue) {
        self.fields.insert(field.name(), value);
    }
}

impl Visit for FieldVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        let mut text = SmallText::default();
        let _ = write!(text, "{value:?}");
        if field.name() == "message"
            && text.0.len() >= 2
            && text.0.first() == Some(&b'"')
            && text.0.last() == Some(&b'"')
        {
            text.0.remove(0);
            text.0.pop();
        }
        self.insert(field, LogValue::Text(text));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.insert(field, LogValue::Signed(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.insert(field, LogValue::Unsigned(value));
    }

    fn record_i128(&mut self, field: &Field, value: i128) {
        let mut text = SmallText::default();
        let _ = write!(text, "{value}");
        self.insert(field, LogValue::Text(text));
    }

    fn record_u128(&mut self, field: &Field, value: u128) {
        let mut text = SmallText::default();
        let _ = write!(text, "{value}");
        self.insert(field, LogValue::Text(text));
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.insert(field, LogValue::Float(value.is_finite().then_some(value)));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.insert(field, LogValue::Bool(value));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.insert(field, LogValue::Text(SmallText::new(value)));
    }
}

#[derive(Serialize)]
struct LogRecord {
    #[serde(skip_serializing_if = "Option::is_none")]
    sequence: Option<u128>,
    timestamp_unix_ns: u128,
    elapsed_ns: u128,
    severity_text: &'static str,
    severity_number: u8,
    event_name: SmallText,
    target: &'static str,
    file: Option<&'static str>,
    line: Option<u32>,
    #[serde(serialize_with = "serialize_thread_id")]
    thread_id: thread::ThreadId,
    thread_name: Option<SmallText>,
    fields: Fields,
    spans: SmallVec<[SpanContext; 8]>,
}

struct DebugThreadId<'a>(&'a thread::ThreadId);

impl Display for DebugThreadId<'_> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}", self.0)
    }
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn serialize_thread_id<S: Serializer>(
    thread_id: &thread::ThreadId,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.collect_str(&DebugThreadId(thread_id))
}

fn severity_text(level: Level) -> &'static str {
    match level {
        Level::TRACE => "TRACE",
        Level::DEBUG => "DEBUG",
        Level::INFO => "INFO",
        Level::WARN => "WARN",
        Level::ERROR => "ERROR",
    }
}

fn severity_number(level: Level) -> u8 {
    match level {
        Level::TRACE => 1,
        Level::DEBUG => 5,
        Level::INFO => 9,
        Level::WARN => 13,
        Level::ERROR => 17,
    }
}

enum WorkerControl {
    Flush(Sender<()>),
    Shutdown,
}

struct WorkerOutputs {
    terminal: Option<BufWriter<Box<dyn Write + Send>>>,
    json: Option<BufWriter<RollingFileAppender>>,
    diagnostics: Arc<DiagnosticsCounters>,
    sequence: u128,
}

impl WorkerOutputs {
    fn write_record(&mut self, mut record: LogRecord) {
        self.sequence = self.sequence.saturating_add(1);
        let sequence = self.sequence;
        record.sequence = Some(sequence);
        if let Some(terminal) = &mut self.terminal {
            let output = format_terminal(&record, sequence);
            if terminal.write_all(output.as_bytes()).is_err() {
                self.diagnostics.sink_errors.fetch_add(1, Ordering::Relaxed);
                eprintln!("Nord log terminal sink failed");
            }
        }
        if let Some(json) = &mut self.json {
            let result = serde_json::to_writer(&mut *json, &record)
                .and_then(|()| json.write_all(b"\n").map_err(serde_json::Error::io));
            if result.is_err() {
                self.diagnostics.sink_errors.fetch_add(1, Ordering::Relaxed);
                eprintln!("Nord log JSON sink failed");
            }
        }
    }

    fn flush(&mut self) {
        if let Some(terminal) = &mut self.terminal
            && terminal.flush().is_err()
        {
            self.diagnostics.sink_errors.fetch_add(1, Ordering::Relaxed);
        }
        if let Some(json) = &mut self.json
            && json.flush().is_err()
        {
            self.diagnostics.sink_errors.fetch_add(1, Ordering::Relaxed);
        }
    }
}

fn run_worker(
    events: &Receiver<LogRecord>,
    controls: &Receiver<WorkerControl>,
    terminal: Option<Box<dyn Write + Send>>,
    json: Option<RollingFileAppender>,
    diagnostics: Arc<DiagnosticsCounters>,
) {
    let mut outputs = WorkerOutputs {
        terminal: terminal.map(BufWriter::new),
        json: json.map(BufWriter::new),
        diagnostics,
        sequence: 0,
    };

    loop {
        crossbeam_channel::select! {
            recv(controls) -> control => match control {
                Ok(WorkerControl::Flush(ack)) => {
                    drain_events(events, &mut outputs);
                    outputs.flush();
                    let _ = ack.send(());
                }
                Ok(WorkerControl::Shutdown) | Err(_) => {
                    drain_events(events, &mut outputs);
                    outputs.flush();
                    break;
                }
            },
            recv(events) -> event => if let Ok(record) = event {
                outputs.write_record(record);
            } else {
                outputs.flush();
                break;
            },
            default(Duration::from_millis(250)) => outputs.flush(),
        }
    }
}

fn drain_events(events: &Receiver<LogRecord>, outputs: &mut WorkerOutputs) {
    while let Ok(record) = events.try_recv() {
        outputs.write_record(record);
    }
}

fn terminal_writer(output: TerminalOutput) -> Box<dyn Write + Send> {
    match output {
        TerminalOutput::Stderr => Box::new(io::stderr()),
        TerminalOutput::Stdout => Box::new(io::stdout()),
    }
}

fn format_terminal(record: &LogRecord, sequence: u128) -> String {
    let mut output = String::new();
    let seconds = record.elapsed_ns / 1_000_000_000;
    let millis = (record.elapsed_ns / 1_000_000) % 1_000;
    let _ = write!(
        output,
        "+{seconds}.{millis:03}s #{sequence} {} [thread {:?}{}] [{}]",
        record.severity_text,
        record.thread_id,
        record.thread_name.as_ref().map_or_else(String::new, |name| format!(" {name}")),
        record.target
    );
    if let (Some(file), Some(line)) = (&record.file, record.line) {
        let _ = write!(output, " [{file}:{line}]");
    }
    if !record.spans.is_empty() {
        output.push_str(" [");
        for (index, span) in record.spans.iter().enumerate() {
            if index > 0 {
                output.push_str(" > ");
            }
            output.push_str(span.name);
        }
        output.push(']');
    }
    output.push(' ');

    let message = record
        .fields
        .get("message")
        .map_or_else(|| record.event_name.as_str().to_owned(), message_to_console);
    push_escaped(&mut output, &message);
    for (key, value) in record.fields.iter() {
        if key != "message" {
            let _ = write!(output, " {key}={}", value_to_console(value));
        }
    }
    for span in &record.spans {
        for (key, value) in span.fields.iter() {
            let _ = write!(output, " {}.{key}={}", span.name, value_to_console(value));
        }
    }
    output.push('\n');
    output
}

fn value_to_console(value: &LogValue) -> String {
    match value {
        LogValue::Text(value) => {
            serde_json::to_string(value.as_str()).unwrap_or_else(|_| "\"<invalid>\"".to_owned())
        }
        LogValue::Signed(value) => value.to_string(),
        LogValue::Unsigned(value) => value.to_string(),
        LogValue::Float(Some(value)) => value.to_string(),
        LogValue::Float(None) => "null".to_owned(),
        LogValue::Bool(value) => value.to_string(),
    }
}

fn message_to_console(value: &LogValue) -> String {
    match value {
        LogValue::Text(value) => value.as_str().to_owned(),
        _ => value_to_console(value),
    }
}

fn push_escaped(output: &mut String, text: &str) {
    for character in text.chars() {
        match character {
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character.is_control() => output.extend(character.escape_default()),
            character => output.push(character),
        }
    }
}

fn create_file_appender(config: JsonFileConfig) -> Result<RollingFileAppender, LoggerInitError> {
    if config.filename_prefix.is_empty()
        || config.filename_prefix.contains('/')
        || config.filename_prefix.contains('\\')
        || config.max_files == Some(0)
    {
        return Err(LoggerInitError::FileSink {
            path: config.directory,
            source: io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid JSON log filename or retention",
            ),
        });
    }

    fs::create_dir_all(&config.directory)
        .map_err(|source| LoggerInitError::FileSink { path: config.directory.clone(), source })?;
    let mut builder = RollingFileAppender::builder()
        .rotation(config.rotation.into())
        .filename_prefix(&config.filename_prefix)
        .filename_suffix("jsonl");
    if let Some(max_files) = config.max_files {
        builder = builder.max_log_files(max_files);
    }
    builder.build(&config.directory).map_err(|error| LoggerInitError::FileSink {
        path: config.directory,
        source: io::Error::other(error.to_string()),
    })
}

fn flush_for_crash(controls: &Sender<WorkerControl>) {
    let (ack_tx, ack_rx) = bounded(1);
    let _ = controls.send(WorkerControl::Flush(ack_tx));
    let _ = ack_rx.recv_timeout(Duration::from_millis(250));
}
