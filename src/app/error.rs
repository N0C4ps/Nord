use std::error::Error;
use std::fmt::{Display, Formatter};

/// Application failure with a stage label and its original source retained.
pub struct ApplicationError {
    stage: &'static str,
    source: nord_app::ApplicationDiagnosticError,
    logged: bool,
}

impl ApplicationError {
    pub(super) fn unreported(
        stage: &'static str,
        source: nord_app::ApplicationDiagnosticError,
    ) -> Self {
        Self { stage, source, logged: false }
    }

    pub(super) fn reported(
        stage: &'static str,
        source: nord_app::ApplicationDiagnosticError,
    ) -> Self {
        Self { stage, source, logged: true }
    }

    pub(crate) const fn was_logged(&self) -> bool {
        self.logged
    }
}

impl Display for ApplicationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        if formatter.alternate() {
            write!(formatter, "application {} failed: {:#}", self.stage, self.source)
        } else {
            write!(formatter, "application {} failed: {}", self.stage, self.source)
        }
    }
}

impl std::fmt::Debug for ApplicationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ApplicationError")
            .field("stage", &self.stage)
            .field("source", &self.source)
            .field("logged", &self.logged)
            .finish()
    }
}

impl std::error::Error for ApplicationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.source.as_ref())
    }
}
