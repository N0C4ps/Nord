use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ApplicationPhase {
    Bootstrapping,
    Running,
    Failed,
    ShuttingDown,
    Stopped,
}

pub(super) struct Lifecycle {
    phase: ApplicationPhase,
    started_at: Instant,
}

impl Lifecycle {
    pub(super) fn new() -> Self {
        Self { phase: ApplicationPhase::Bootstrapping, started_at: Instant::now() }
    }

    pub(super) fn transition(&mut self, next: ApplicationPhase) {
        nord_app::debug_invariant!(
            self.phase.can_transition_to(next),
            "invalid application lifecycle transition: {:?} -> {:?}",
            self.phase,
            next
        );
        self.phase = next;
    }

    pub(super) fn uptime(&self) -> Duration {
        self.started_at.elapsed()
    }
}

impl ApplicationPhase {
    const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Bootstrapping, Self::Running | Self::Failed)
                | (Self::Running, Self::Failed | Self::ShuttingDown)
                | (Self::Failed, Self::ShuttingDown)
                | (Self::ShuttingDown, Self::Stopped)
        )
    }
}
