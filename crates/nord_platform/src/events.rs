use crate::Timestamp;
pub use nord_events::{ApplicationLifecycleEvent, WindowEvent, WindowHandle};
pub use nord_inputs::{InputEvent, KeyCode, Modifiers, PointerButton};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

const EVENT_QUEUE_CAPACITY: usize = 4096;

/// Normalized event with a monotonic timestamp and queue sequence.
#[derive(Debug, Clone, PartialEq)]
pub struct PlatformEvent {
    sequence: u64,
    timestamp: Timestamp,
    kind: PlatformEventKind,
}

impl PlatformEvent {
    /// Monotonic queue sequence number.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Timestamp captured when the event entered the queue.
    #[must_use]
    pub const fn timestamp(&self) -> Timestamp {
        self.timestamp
    }

    /// Normalized event payload.
    #[must_use]
    pub const fn kind(&self) -> &PlatformEventKind {
        &self.kind
    }
}

/// Platform-neutral event categories.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum PlatformEventKind {
    /// Application lifecycle notification.
    Lifecycle(ApplicationLifecycleEvent),
    /// Normalized native window event, produced by `nord_window` backends.
    Window { window: WindowHandle, event: WindowEvent },
    /// Normalized keyboard/pointer event, produced by native input backends.
    Input(InputEvent),
}

/// Event delivery failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventError {
    /// The bounded queue is full; producer should retry or handle backpressure.
    QueueFull,
    /// Window identifier space has been exhausted.
    WindowHandleExhausted,
    /// Event sequence space has been exhausted.
    SequenceExhausted,
}

impl std::fmt::Display for EventError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::QueueFull => formatter.write_str("platform event queue is full"),
            Self::WindowHandleExhausted => formatter.write_str("window handle space is exhausted"),
            Self::SequenceExhausted => formatter.write_str("platform event sequence is exhausted"),
        }
    }
}

impl std::error::Error for EventError {}

struct QueueState {
    events: VecDeque<PlatformEvent>,
    next_sequence: u64,
}

struct SharedQueue {
    state: Mutex<QueueState>,
    clock: crate::MonotonicClock,
    window_ids: nord_events::WindowIdAllocator,
}

/// Cloneable producer for native subsystem and application events.
#[derive(Clone)]
pub struct EventEmitter {
    shared: Arc<SharedQueue>,
}

impl EventEmitter {
    /// Queue a normalized event, preserving producer order under contention.
    ///
    /// # Errors
    ///
    /// Returns [`EventError::QueueFull`] when all bounded queue slots are in
    /// use, or [`EventError::SequenceExhausted`] if event ordering cannot be
    /// represented uniquely.
    pub fn emit(&self, kind: PlatformEventKind) -> Result<u64, EventError> {
        let mut state = self.shared.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.events.len() >= EVENT_QUEUE_CAPACITY {
            return Err(EventError::QueueFull);
        }

        let sequence = state.next_sequence;
        let next_sequence = sequence.checked_add(1).ok_or(EventError::SequenceExhausted)?;
        let timestamp = self.shared.clock.now();
        state.events.push_back(PlatformEvent { sequence, timestamp, kind });
        state.next_sequence = next_sequence;
        Ok(sequence)
    }

    /// Allocate an opaque window identifier for a native window backend.
    ///
    /// # Errors
    ///
    /// Returns [`EventError::WindowHandleExhausted`] if the identifier space
    /// can no longer provide a unique value.
    pub fn allocate_window_handle(&self) -> Result<WindowHandle, EventError> {
        self.shared.window_ids.allocate().map_err(|_| EventError::WindowHandleExhausted)
    }
}

/// Event consumer owned by the platform object.
pub struct EventReceiver {
    shared: Arc<SharedQueue>,
}

impl EventReceiver {
    pub(super) fn drain_into(&self, output: &mut Vec<PlatformEvent>) {
        let mut state = self.shared.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        output.extend(state.events.drain(..));
    }
}

pub(super) fn event_queue(clock: crate::MonotonicClock) -> (EventEmitter, EventReceiver) {
    let shared = Arc::new(SharedQueue {
        state: Mutex::new(QueueState {
            events: VecDeque::with_capacity(EVENT_QUEUE_CAPACITY),
            next_sequence: 0,
        }),
        clock,
        window_ids: nord_events::WindowIdAllocator::new(),
    });
    (EventEmitter { shared: Arc::clone(&shared) }, EventReceiver { shared })
}
