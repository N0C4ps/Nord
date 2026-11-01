use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock, Weak};

type Listener<E> = Arc<dyn Fn(&E) + Send + Sync + 'static>;

struct Inner<E> {
    listeners: RwLock<BTreeMap<u64, Listener<E>>>,
    next_id: AtomicU64,
}

/// Multicast dispatcher for one event type.
///
/// Producers should enqueue platform events in their bounded native queue;
/// the application drains that queue and calls `dispatch` on its main thread.
/// Listener snapshots are copied under a read lock and callbacks run after the
/// lock is released, so listeners can subscribe/unsubscribe without deadlocks.
pub struct EventSystem<E> {
    inner: Arc<Inner<E>>,
}

impl<E> Clone for EventSystem<E> {
    fn clone(&self) -> Self {
        Self { inner: Arc::clone(&self.inner) }
    }
}

impl<E> Default for EventSystem<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E> EventSystem<E> {
    /// Create an empty typed dispatcher.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                listeners: RwLock::new(BTreeMap::new()),
                next_id: AtomicU64::new(1),
            }),
        }
    }

    /// Subscribe to events. Dropping the returned token removes the listener.
    ///
    /// # Errors
    ///
    /// Returns [`EventSystemError::ListenerIdExhausted`] if IDs can no longer
    /// be assigned uniquely.
    pub fn subscribe<F>(&self, callback: F) -> Result<EventSubscription<E>, EventSystemError>
    where
        E: Send + Sync + 'static,
        F: Fn(&E) + Send + Sync + 'static,
    {
        let id = self
            .inner
            .next_id
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| EventSystemError::ListenerIdExhausted)?;
        self.inner
            .listeners
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(id, Arc::new(callback));
        Ok(EventSubscription { id, inner: Arc::downgrade(&self.inner) })
    }

    /// Dispatch one event to a stable listener snapshot, in subscription order.
    /// With panic unwinding enabled, listener panics are isolated so one
    /// subscriber cannot prevent delivery to later listeners; the report lets
    /// the application log the failure. With `panic = "abort"`, the process
    /// aborts before a panic can be caught.
    pub fn dispatch(&self, event: &E) -> DispatchReport {
        let listeners: Vec<_> = self
            .inner
            .listeners
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
            .cloned()
            .collect();
        let mut report = DispatchReport::default();
        for listener in listeners {
            report.listeners_invoked = report.listeners_invoked.saturating_add(1);
            if catch_unwind(AssertUnwindSafe(|| listener(event))).is_err() {
                report.listeners_panicked = report.listeners_panicked.saturating_add(1);
            }
        }
        report
    }

    /// Number of currently active subscribers.
    #[must_use]
    pub fn listener_count(&self) -> usize {
        self.inner.listeners.read().unwrap_or_else(std::sync::PoisonError::into_inner).len()
    }
}

/// RAII token that unregisters a listener when dropped.
pub struct EventSubscription<E> {
    id: u64,
    inner: Weak<Inner<E>>,
}

impl<E> Drop for EventSubscription<E> {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.upgrade() {
            inner
                .listeners
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&self.id);
        }
    }
}

/// Summary of a single event dispatch.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DispatchReport {
    listeners_invoked: usize,
    listeners_panicked: usize,
}

impl DispatchReport {
    /// Number of listener callbacks attempted.
    #[must_use]
    pub const fn listeners_invoked(self) -> usize {
        self.listeners_invoked
    }
    /// Number of listener callbacks that panicked.
    #[must_use]
    pub const fn listeners_panicked(self) -> usize {
        self.listeners_panicked
    }

    /// Add counts from another dispatch, saturating at the integer limit.
    #[must_use]
    pub const fn merge(self, other: Self) -> Self {
        Self {
            listeners_invoked: self.listeners_invoked.saturating_add(other.listeners_invoked),
            listeners_panicked: self.listeners_panicked.saturating_add(other.listeners_panicked),
        }
    }
}

/// Event dispatcher setup failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum EventSystemError {
    /// The monotonic listener identifier space is exhausted.
    ListenerIdExhausted,
}

impl Display for EventSystemError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ListenerIdExhausted => {
                formatter.write_str("event listener ID space is exhausted")
            }
        }
    }
}

impl Error for EventSystemError {}
