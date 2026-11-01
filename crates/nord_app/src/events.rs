//! Stable application facade for typed window/lifecycle event dispatch.

use nord_events::{DispatchReport, EventSubscription, EventSystem, EventSystemError};

pub use nord_events::{ApplicationEvent, ApplicationLifecycleEvent, WindowEvent, WindowHandle};
pub type ApplicationDispatchReport = DispatchReport;
pub type ApplicationWindowEvent = WindowEvent;
pub type ApplicationWindowHandle = WindowHandle;
pub type ApplicationEventSubscription = EventSubscription<ApplicationEvent>;

/// Application-owned event dispatcher for window and lifecycle events.
#[derive(Clone, Default)]
pub struct ApplicationEventSystem {
    inner: EventSystem<ApplicationEvent>,
}

impl ApplicationEventSystem {
    /// Create an empty application event dispatcher.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Subscribe to lifecycle and window events. Drop the token to unsubscribe.
    ///
    /// # Errors
    ///
    /// Returns [`EventSystemError::ListenerIdExhausted`] if listener IDs run out.
    pub fn subscribe<F>(
        &self,
        callback: F,
    ) -> Result<ApplicationEventSubscription, EventSystemError>
    where
        F: Fn(&ApplicationEvent) + Send + Sync + 'static,
    {
        self.inner.subscribe(callback)
    }

    /// Dispatch an event to the current listener snapshot without holding locks
    /// while user callbacks execute.
    #[must_use]
    pub fn dispatch(&self, event: &ApplicationEvent) -> DispatchReport {
        self.inner.dispatch(event)
    }

    /// Number of registered listeners.
    #[must_use]
    pub fn listener_count(&self) -> usize {
        self.inner.listener_count()
    }
}
