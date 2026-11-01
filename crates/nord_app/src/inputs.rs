//! Stable application facade for normalized keyboard and pointer input.

use nord_events::{DispatchReport, EventSubscription, EventSystemError};

pub use nord_inputs::{InputEvent, KeyCode, Modifiers, PointerButton};
pub type ApplicationInputEvent = InputEvent;
pub type ApplicationKeyCode = KeyCode;
pub type ApplicationModifiers = Modifiers;
pub type ApplicationPointerButton = PointerButton;
pub type ApplicationInputSubscription = EventSubscription<InputEvent>;

/// Application-owned input state and input-event dispatcher.
#[derive(Default)]
pub struct ApplicationInputSystem {
    inner: nord_inputs::InputSystem,
}

impl ApplicationInputSystem {
    /// Create an empty input system.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Subscribe to normalized keyboard/pointer events. Drop the token to unsubscribe.
    ///
    /// # Errors
    ///
    /// Returns [`EventSystemError::ListenerIdExhausted`] if listener IDs run out.
    pub fn subscribe<F>(
        &self,
        callback: F,
    ) -> Result<ApplicationInputSubscription, EventSystemError>
    where
        F: Fn(&InputEvent) + Send + Sync + 'static,
    {
        self.inner.subscribe(callback)
    }

    /// Apply a native event to held state and dispatch it to listeners.
    #[must_use]
    pub fn ingest(&self, event: &InputEvent) -> DispatchReport {
        self.inner.ingest(event)
    }

    /// Clear held keyboard/pointer state on focus loss and dispatch synthetic releases.
    #[must_use]
    pub fn release_window(&self, window: nord_events::WindowHandle) -> DispatchReport {
        self.inner.release_window(window)
    }

    /// Whether a key is held in a window.
    #[must_use]
    pub fn is_key_pressed(&self, window: nord_events::WindowHandle, key: KeyCode) -> bool {
        self.inner.is_key_pressed(window, key)
    }

    /// Whether a pointer button is held in a window.
    #[must_use]
    pub fn is_pointer_button_pressed(
        &self,
        window: nord_events::WindowHandle,
        button: PointerButton,
    ) -> bool {
        self.inner.is_pointer_button_pressed(window, button)
    }

    /// Most recent pointer position in client pixels.
    #[must_use]
    pub fn pointer_position(&self, window: nord_events::WindowHandle) -> Option<(f64, f64)> {
        self.inner.pointer_position(window)
    }
}
