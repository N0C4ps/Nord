//! Typed event contracts and a low-overhead, thread-safe event dispatcher.

mod dispatch;
mod window;

pub use dispatch::{DispatchReport, EventSubscription, EventSystem, EventSystemError};
pub use window::{
    ApplicationEvent, ApplicationLifecycleEvent, WindowEvent, WindowHandle, WindowIdAllocator,
    WindowIdExhausted,
};
