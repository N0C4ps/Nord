//! Normalized keyboard and pointer events plus per-window input state.

use nord_events::{DispatchReport, EventSubscription, EventSystem, EventSystemError, WindowHandle};
use std::collections::{BTreeSet, HashMap};
use std::sync::Mutex;

/// Platform-neutral keyboard or pointer event.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum InputEvent {
    /// A key changed state. Repeated key-down messages keep `pressed = true` and set `repeat`.
    Key { window: WindowHandle, key: KeyCode, pressed: bool, repeat: bool, modifiers: Modifiers },
    /// Pointer moved in client-area physical pixels.
    PointerMoved { window: WindowHandle, x: f64, y: f64 },
    /// A pointer button changed state.
    PointerButton { window: WindowHandle, button: PointerButton, pressed: bool },
    /// A pointer wheel delta. One whole unit equals one standard wheel detent.
    PointerWheel { window: WindowHandle, delta_x: f64, delta_y: f64 },
}

/// Normalized virtual key used for gameplay bindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

/// Modifier state captured with key transitions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Modifiers(u8);

impl Modifiers {
    /// No active modifier keys.
    pub const EMPTY: Self = Self(0);
    /// Shift is active.
    pub const SHIFT: Self = Self(1 << 0);
    /// Control is active.
    pub const CONTROL: Self = Self(1 << 1);
    /// Alt/Option is active.
    pub const ALT: Self = Self(1 << 2);
    /// Super/Command/Windows is active.
    pub const SUPER: Self = Self(1 << 3);

    /// Build modifier flags, rejecting unknown bits.
    #[must_use]
    pub const fn from_bits(bits: u8) -> Option<Self> {
        if bits & !0b1111 == 0 { Some(Self(bits)) } else { None }
    }
    /// Return the underlying flags.
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }
    /// Combine modifier flags.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
    /// Test whether a modifier is active.
    #[must_use]
    pub const fn contains(self, modifier: Self) -> bool {
        self.0 & modifier.0 == modifier.0
    }
}

/// Normalized mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum PointerButton {
    Primary,
    Secondary,
    Middle,
    Auxiliary(u8),
}

#[derive(Default)]
struct WindowState {
    keys: BTreeSet<KeyCode>,
    buttons: BTreeSet<PointerButton>,
    pointer: Option<(f64, f64)>,
}

/// Per-window, edge-updated input state and input event dispatcher.
///
/// Native events are ingested on the application thread. State queries are
/// synchronized, and listener callbacks run after the state lock is released.
pub struct InputSystem {
    state: Mutex<HashMap<WindowHandle, WindowState>>,
    events: EventSystem<InputEvent>,
}

impl Default for InputSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl InputSystem {
    /// Create an empty input state and dispatcher.
    #[must_use]
    pub fn new() -> Self {
        Self { state: Mutex::new(HashMap::new()), events: EventSystem::new() }
    }

    /// Subscribe to normalized input events. Drop the returned token to unsubscribe.
    ///
    /// # Errors
    ///
    /// Returns [`EventSystemError::ListenerIdExhausted`] if listener IDs are exhausted.
    pub fn subscribe<F>(
        &self,
        callback: F,
    ) -> Result<EventSubscription<InputEvent>, EventSystemError>
    where
        F: Fn(&InputEvent) + Send + Sync + 'static,
    {
        self.events.subscribe(callback)
    }

    /// Update held state and dispatch one native input event.
    pub fn ingest(&self, event: &InputEvent) -> DispatchReport {
        {
            let mut states = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            let window = match &event {
                InputEvent::Key { window, .. }
                | InputEvent::PointerMoved { window, .. }
                | InputEvent::PointerButton { window, .. }
                | InputEvent::PointerWheel { window, .. } => *window,
            };
            let state = states.entry(window).or_default();
            match &event {
                InputEvent::Key { key, pressed: true, repeat: false, .. } => {
                    state.keys.insert(*key);
                }
                InputEvent::Key { key, pressed: false, .. } => {
                    state.keys.remove(key);
                }
                InputEvent::PointerMoved { x, y, .. } => state.pointer = Some((*x, *y)),
                InputEvent::PointerButton { button, pressed: true, .. } => {
                    state.buttons.insert(*button);
                }
                InputEvent::PointerButton { button, pressed: false, .. } => {
                    state.buttons.remove(button);
                }
                _ => {}
            }
        }
        self.events.dispatch(event)
    }

    /// Release all held keys and pointer buttons for a window, normally on focus loss.
    /// Synthetic release events prevent input from remaining stuck after deactivation.
    pub fn release_window(&self, window: WindowHandle) -> DispatchReport {
        let (keys, buttons) = {
            let mut states = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            states.remove(&window).map_or_else(
                || (Vec::new(), Vec::new()),
                |state| {
                    (
                        state.keys.into_iter().collect::<Vec<_>>(),
                        state.buttons.into_iter().collect::<Vec<_>>(),
                    )
                },
            )
        };
        let mut total = DispatchReport::default();
        for key in keys {
            total = total.merge(self.events.dispatch(&InputEvent::Key {
                window,
                key,
                pressed: false,
                repeat: false,
                modifiers: Modifiers::EMPTY,
            }));
        }
        for button in buttons {
            total = total.merge(self.events.dispatch(&InputEvent::PointerButton {
                window,
                button,
                pressed: false,
            }));
        }
        total
    }

    /// Whether the given key is currently held in a window.
    #[must_use]
    pub fn is_key_pressed(&self, window: WindowHandle, key: KeyCode) -> bool {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&window)
            .is_some_and(|state| state.keys.contains(&key))
    }

    /// Whether the given pointer button is currently held in a window.
    #[must_use]
    pub fn is_pointer_button_pressed(&self, window: WindowHandle, button: PointerButton) -> bool {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&window)
            .is_some_and(|state| state.buttons.contains(&button))
    }

    /// Most recent pointer position in client-area pixels, if the pointer moved there.
    #[must_use]
    pub fn pointer_position(&self, window: WindowHandle) -> Option<(f64, f64)> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&window)
            .and_then(|state| state.pointer)
    }
}
