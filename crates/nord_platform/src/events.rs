use crate::Timestamp;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

const EVENT_QUEUE_CAPACITY: usize = 4096;

/// Opaque identifier for a native window; owned by `nord_window` when added.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowHandle(u64);

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

/// Application lifecycle event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ApplicationLifecycleEvent {
    /// The application is entering a suspended/background state.
    Suspended,
    /// The application is returning to an active state.
    Resumed,
    /// A native close or termination request was received.
    QuitRequested,
}

/// Normalized window event.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum WindowEvent {
    /// Native window dimensions changed, in physical pixels.
    Resized { width: u32, height: u32 },
    /// Logical-to-physical scale factor changed.
    ScaleFactorChanged { scale_factor: f64 },
    /// Focus state changed.
    FocusChanged { focused: bool },
    /// A native close request was received.
    CloseRequested,
}

/// Normalized keyboard or pointer input event.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum InputEvent {
    /// A key changed state.
    Key { key: KeyCode, pressed: bool, repeat: bool, modifiers: Modifiers },
    /// Pointer position changed in physical pixels.
    PointerMoved { window: WindowHandle, x: f64, y: f64 },
    /// A pointer button changed state.
    PointerButton { window: WindowHandle, button: PointerButton, pressed: bool },
    /// A vertical/horizontal wheel delta was received.
    PointerWheel { window: WindowHandle, delta_x: f64, delta_y: f64 },
}

/// Platform-neutral physical key identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum KeyCode {
    Unknown,
    Escape,
    Enter,
    Space,
    Tab,
    Backspace,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Digit(u8),
    Letter(char),
    Function(u8),
}

/// Modifier state captured with a key event.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Modifiers(u8);

impl Modifiers {
    /// No active modifier keys.
    pub const EMPTY: Self = Self(0);
    /// Shift modifier is active.
    pub const SHIFT: Self = Self(1 << 0);
    /// Control modifier is active.
    pub const CONTROL: Self = Self(1 << 1);
    /// Alt/Option modifier is active.
    pub const ALT: Self = Self(1 << 2);
    /// Super/Command/Windows modifier is active.
    pub const SUPER: Self = Self(1 << 3);

    /// Build from raw bits, rejecting unknown modifier bits.
    #[must_use]
    pub const fn from_bits(bits: u8) -> Option<Self> {
        if bits & !0b1111 == 0 { Some(Self(bits)) } else { None }
    }

    /// Return all active modifier bits.
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// Combine this state with another modifier state.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Check whether the provided modifier is active.
    #[must_use]
    pub const fn contains(self, modifier: Self) -> bool {
        self.0 & modifier.0 == modifier.0
    }
}

/// Normalized pointer button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PointerButton {
    Primary,
    Secondary,
    Middle,
    Auxiliary(u8),
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
    next_window_id: AtomicU64,
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
        self.shared
            .next_window_id
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map(WindowHandle)
            .map_err(|_| EventError::WindowHandleExhausted)
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
        next_window_id: AtomicU64::new(1),
    });
    (EventEmitter { shared: Arc::clone(&shared) }, EventReceiver { shared })
}
