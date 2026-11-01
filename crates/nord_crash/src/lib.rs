//! Crash diagnostics for Rust panics and native process failures.
//!
//! The crate chains the process panic hook, writes a crash report synchronously
//! to JSONL, and optionally keeps an out-of-process native minidump monitor
//! alive. It has no dependency on Nord's logger; callers may provide a bounded,
//! non-panicking callback to flush their own diagnostic queue before the crash
//! report is written.

use minidumper_child::MinidumperChild;
use serde::Serialize;
use serde_json::Value;
use std::backtrace::Backtrace;
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static PANIC_HOOK_INSTALLED: AtomicBool = AtomicBool::new(false);
static REPORT_LOCK: Mutex<()> = Mutex::new(());

/// Settings for local panic reports and optional native minidumps.
#[derive(Debug, Clone)]
pub struct CrashReporterConfig {
    report_path: PathBuf,
    capture_backtrace: bool,
    native_minidumps: bool,
    application_name: Option<String>,
    application_version: Option<String>,
}

impl CrashReporterConfig {
    /// Write panic reports to `report_path`; backtraces and native dumps are on.
    #[must_use]
    pub fn new(report_path: impl Into<PathBuf>) -> Self {
        Self {
            report_path: report_path.into(),
            capture_backtrace: true,
            native_minidumps: true,
            application_name: None,
            application_version: None,
        }
    }

    /// Attach application identity to every panic report.
    #[must_use]
    pub fn with_application_identity(
        mut self,
        name: impl Into<String>,
        version: impl Into<String>,
    ) -> Self {
        self.application_name = Some(name.into());
        self.application_version = Some(version.into());
        self
    }

    /// Enable or disable forced backtrace capture.
    #[must_use]
    pub const fn with_backtrace(mut self, enabled: bool) -> Self {
        self.capture_backtrace = enabled;
        self
    }

    /// Enable or disable native dumps captured by the monitor process.
    #[must_use]
    pub const fn with_native_minidumps(mut self, enabled: bool) -> Self {
        self.native_minidumps = enabled;
        self
    }

    /// The JSON Lines crash report destination.
    #[must_use]
    pub fn report_path(&self) -> &Path {
        &self.report_path
    }
}

/// Startup failure for the synchronous panic report handler.
#[derive(Debug)]
pub enum CrashReporterInitError {
    /// The crash report file could not be prepared for append-only writes.
    ReportFile { path: PathBuf, source: io::Error },
    /// Nord's process-global panic hook has already been installed.
    PanicHookAlreadyInstalled,
}

impl Display for CrashReporterInitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReportFile { path, source } => {
                write!(formatter, "cannot prepare crash report file `{}`: {source}", path.display())
            }
            Self::PanicHookAlreadyInstalled => {
                formatter.write_str("Nord crash reporter was already initialized")
            }
        }
    }
}

impl Error for CrashReporterInitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ReportFile { source, .. } => Some(source),
            Self::PanicHookAlreadyInstalled => None,
        }
    }
}

/// Current status of optional native minidump capture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrashReporterDiagnostics {
    /// Whether an out-of-process native dump monitor was started successfully.
    pub native_minidumps_enabled: bool,
    /// Initialization error when native dumps were requested but unavailable.
    pub native_minidump_error: Option<String>,
}

/// Process-lifetime crash handler and native minidump monitor owner.
///
/// Retain this value for the process lifetime. Rust's panic hook is process
/// global and cannot be uninstalled safely while the application is running.
#[must_use = "keep the crash reporter alive for the process lifetime"]
pub struct CrashReporter {
    diagnostics: CrashReporterDiagnostics,
    _native_monitor: Option<minidumper_child::ClientHandle>,
}

impl CrashReporter {
    /// Install the chained panic hook and start optional native dump capture.
    ///
    /// `before_panic` should be a small, bounded callback that does not panic;
    /// it is called on the panicking thread before the report is written.
    ///
    /// Native monitor startup failure is non-fatal: panic JSONL diagnostics
    /// remain enabled and the failure is available through [`Self::diagnostics`].
    ///
    /// # Errors
    ///
    /// Returns an error if the report file cannot be prepared or another Nord
    /// crash reporter has already installed the process panic hook.
    pub fn initialize<F>(
        config: CrashReporterConfig,
        before_panic: F,
    ) -> Result<Self, CrashReporterInitError>
    where
        F: Fn() + Send + Sync + 'static,
    {
        prepare_report_file(&config.report_path)?;
        PANIC_HOOK_INSTALLED
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| CrashReporterInitError::PanicHookAlreadyInstalled)?;

        let (native_monitor, native_minidump_error) = start_native_monitor(&config);
        let native_minidumps_enabled = native_monitor.is_some();
        install_panic_hook(config, Box::new(before_panic));

        Ok(Self {
            diagnostics: CrashReporterDiagnostics {
                native_minidumps_enabled,
                native_minidump_error,
            },
            _native_monitor: native_monitor,
        })
    }

    /// Snapshot native monitor status.
    #[must_use]
    pub fn diagnostics(&self) -> &CrashReporterDiagnostics {
        &self.diagnostics
    }
}

type BeforePanic = Box<dyn Fn() + Send + Sync + 'static>;

fn prepare_report_file(path: &Path) -> Result<(), CrashReporterInitError> {
    if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(|source| CrashReporterInitError::ReportFile {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map(drop)
        .map_err(|source| CrashReporterInitError::ReportFile { path: path.into(), source })
}

fn start_native_monitor(
    config: &CrashReporterConfig,
) -> (Option<minidumper_child::ClientHandle>, Option<String>) {
    if !config.native_minidumps {
        return (None, None);
    }

    let dump_directory = config
        .report_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .join("dumps");
    let minidumper = MinidumperChild::new()
        .with_crashes_dir(dump_directory)
        .on_process(|command| {
            #[cfg(target_os = "windows")]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x0800_0000);
            }
            command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        })
        .on_minidump(|dump, temporary_path| {
            let Some(stem) = temporary_path.file_stem().and_then(|stem| stem.to_str()) else {
                eprintln!("Nord minidump has an invalid temporary filename");
                return;
            };
            let permanent_path = temporary_path.with_file_name(format!("nord-{stem}.dmp"));
            if let Err(error) = fs::write(permanent_path, dump) {
                eprintln!("Nord could not save native minidump: {error}");
            }
        });

    let is_reporter_process = minidumper.is_crash_reporter_process();
    match minidumper.spawn() {
        Ok(reporter) => (Some(reporter), None),
        Err(error) if is_reporter_process => {
            eprintln!("Nord crash monitor could not start: {error:?}");
            std::process::exit(1);
        }
        Err(error) => (None, Some(format!("{error:?}"))),
    }
}

fn install_panic_hook(config: CrashReporterConfig, before_panic: BeforePanic) {
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        previous_hook(panic_info);
        before_panic();
        let _guard = REPORT_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        write_panic_report(&config, panic_info);
    }));
}

fn write_panic_report(config: &CrashReporterConfig, panic_info: &std::panic::PanicHookInfo<'_>) {
    let payload = panic_info
        .payload()
        .downcast_ref::<&'static str>()
        .copied()
        .or_else(|| panic_info.payload().downcast_ref::<String>().map(String::as_str))
        .unwrap_or("non-string panic payload");
    let location = panic_info.location();
    let thread = std::thread::current();
    let backtrace = config.capture_backtrace.then(|| Backtrace::force_capture().to_string());
    let timestamp_unix_ns =
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let mut fields = BTreeMap::from([
        ("message".to_owned(), Value::String("Rust panic captured".to_owned())),
        ("panic_payload".to_owned(), Value::String(payload.to_owned())),
        ("backtrace".to_owned(), backtrace.map_or(Value::Null, Value::String)),
    ]);
    if let Some(name) = &config.application_name {
        fields.insert("application_name".to_owned(), Value::String(name.clone()));
    }
    if let Some(version) = &config.application_version {
        fields.insert("application_version".to_owned(), Value::String(version.clone()));
    }

    let report = CrashRecord {
        sequence: None,
        timestamp_unix_ns,
        severity_text: "FATAL",
        severity_number: 21,
        event_name: "rust_panic".to_owned(),
        target: "nord::crash".to_owned(),
        file: location.map(|value| value.file().to_owned()),
        line: location.map(std::panic::Location::line),
        column: location.map(std::panic::Location::column),
        thread_id: format!("{:?}", thread.id()),
        thread_name: thread.name().map(str::to_owned),
        fields,
        spans: Vec::new(),
    };

    if let Err(error) = append_report(&config.report_path, &report) {
        eprintln!(
            "Nord could not persist crash diagnostics to `{}`: {error}",
            config.report_path.display()
        );
    } else {
        eprintln!("Nord crash diagnostics saved to `{}`", config.report_path.display());
    }
}

fn append_report(path: &Path, report: &CrashRecord) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(report).map_err(io::Error::other)?;
    bytes.push(b'\n');
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(&bytes)?;
    file.flush()?;
    file.sync_all()
}

#[derive(Serialize)]
struct CrashRecord {
    #[serde(skip_serializing_if = "Option::is_none")]
    sequence: Option<u128>,
    timestamp_unix_ns: u128,
    severity_text: &'static str,
    severity_number: u8,
    event_name: String,
    target: String,
    file: Option<String>,
    line: Option<u32>,
    column: Option<u32>,
    thread_id: String,
    thread_name: Option<String>,
    fields: BTreeMap<String, Value>,
    spans: Vec<serde_json::Value>,
}
