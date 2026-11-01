//! Stable application-facing facade over Nord's crash diagnostics.

pub use nord_crash::{
    CrashReporterConfig as ApplicationCrashReporterConfig,
    CrashReporterDiagnostics as ApplicationCrashReporterDiagnostics,
    CrashReporterInitError as ApplicationCrashReporterInitError,
};

/// Process-lifetime owner of the application's panic hook and native monitor.
#[must_use = "retain the crash reporter for the application lifetime"]
pub struct ApplicationCrashReporter {
    reporter: nord_crash::CrashReporter,
}

impl ApplicationCrashReporter {
    /// Install panic reporting and optionally start the native minidump monitor.
    ///
    /// The callback should perform a bounded, non-panicking flush of other
    /// local diagnostics. Native monitor startup failure is reported through
    /// [`Self::diagnostics`] while JSONL panic reports remain active.
    ///
    /// # Errors
    ///
    /// Returns an initialization error if the local report file cannot be
    /// prepared or another Nord crash reporter already owns the process hook.
    pub fn initialize<F>(
        config: ApplicationCrashReporterConfig,
        before_panic: F,
    ) -> Result<Self, ApplicationCrashReporterInitError>
    where
        F: Fn() + Send + Sync + 'static,
    {
        nord_crash::CrashReporter::initialize(config, before_panic)
            .map(|reporter| Self { reporter })
    }

    /// Snapshot native dump monitor status.
    #[must_use]
    pub fn diagnostics(&self) -> &ApplicationCrashReporterDiagnostics {
        self.reporter.diagnostics()
    }
}
