use crate::gameloop::GameLoop;
use crate::render::GameRenderer;
use nord_app::{
    ApplicationErrorContext, ApplicationEvent, ApplicationEventSystem, ApplicationInputSystem,
    ApplicationPlatform, ApplicationPlatformEventKind, ApplicationResult, ApplicationWindowEvent,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::thread;
use std::time::Duration;
const SHUTDOWN_NONE: u8 = 0;
const SHUTDOWN_CTRL_C: u8 = 1;
const SHUTDOWN_PLATFORM: u8 = 2;
const SHUTDOWN_WINDOW: u8 = 3;

pub(super) struct ApplicationRuntime {
    shutdown_reason: Arc<AtomicU8>,
    _window_subscription: nord_app::ApplicationEventSubscription,
    events: Vec<nord_app::ApplicationPlatformEvent>,
}

impl ApplicationRuntime {
    pub(super) fn initialize(event_system: &ApplicationEventSystem) -> ApplicationResult<Self> {
        let shutdown_reason = Arc::new(AtomicU8::new(SHUTDOWN_NONE));
        let handler_flag = Arc::clone(&shutdown_reason);
        ctrlc::set_handler(move || {
            handler_flag.store(SHUTDOWN_CTRL_C, Ordering::Release);
        })
        .context("installing the process Ctrl+C handler")?;

        let event_shutdown_flag = Arc::clone(&shutdown_reason);
        let window_subscription = event_system
            .subscribe(move |event| match event {
                ApplicationEvent::Lifecycle(nord_app::ApplicationLifecycleEvent::QuitRequested) => {
                    event_shutdown_flag.store(SHUTDOWN_PLATFORM, Ordering::Release);
                }
                ApplicationEvent::Window {
                    event: ApplicationWindowEvent::CloseRequested, ..
                } => event_shutdown_flag.store(SHUTDOWN_WINDOW, Ordering::Release),
                ApplicationEvent::Lifecycle(nord_app::ApplicationLifecycleEvent::Suspended) => {
                    nord_app::debug!(
                        name: "application_suspended",
                        target: "nord::app",
                        "Application entered a suspended state"
                    );
                }
                ApplicationEvent::Lifecycle(nord_app::ApplicationLifecycleEvent::Resumed) => {
                    nord_app::debug!(
                        name: "application_resumed",
                        target: "nord::app",
                        "Application resumed"
                    );
                }
                _ => {}
            })
            .context("subscribing runtime lifecycle handling")?;

        Ok(Self {
            shutdown_reason,
            _window_subscription: window_subscription,
            events: Vec::with_capacity(64),
        })
    }

    pub(super) fn run(
        &mut self,
        platform: &ApplicationPlatform,
        event_system: &ApplicationEventSystem,
        input_system: &ApplicationInputSystem,
        renderer: &mut GameRenderer,
        game_loop: &mut GameLoop,
    ) -> ApplicationResult<ShutdownReason> {
        loop {
            if let Some(reason) = self.requested_shutdown() {
                return Ok(reason);
            }

            renderer.wait_for_frame()?;
            self.events.clear();
            platform.poll_events(&mut self.events).context("pumping native window messages")?;
            self.dispatch_platform_events(event_system, input_system, renderer)?;

            if let Some(reason) = self.requested_shutdown() {
                return Ok(reason);
            }
            game_loop.advance(self.events.len()).context("advancing the game loop")?;
            renderer.render_frame()?;
            if renderer.surface_suspended() {
                thread::sleep(Duration::from_millis(16));
            }
        }
    }

    fn dispatch_platform_events(
        &self,
        event_system: &ApplicationEventSystem,
        input_system: &ApplicationInputSystem,
        renderer: &mut GameRenderer,
    ) -> ApplicationResult<()> {
        for event in &self.events {
            match event.kind() {
                ApplicationPlatformEventKind::Lifecycle(lifecycle) => {
                    Self::report_event_dispatch(
                        event_system.dispatch(&ApplicationEvent::Lifecycle(*lifecycle)),
                    );
                }
                ApplicationPlatformEventKind::Window { window, event } => {
                    if let ApplicationWindowEvent::Resized { width, height } = event {
                        renderer.resize(*width, *height)?;
                    }
                    if matches!(event, ApplicationWindowEvent::FocusChanged { focused: false }) {
                        Self::report_input_dispatch(input_system.release_window(*window));
                    }
                    let application_event =
                        ApplicationEvent::Window { window: *window, event: event.clone() };
                    Self::report_event_dispatch(event_system.dispatch(&application_event));
                }
                ApplicationPlatformEventKind::Input(input) => {
                    Self::report_input_dispatch(input_system.ingest(input));
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn requested_shutdown(&self) -> Option<ShutdownReason> {
        match self.shutdown_reason.load(Ordering::Acquire) {
            SHUTDOWN_CTRL_C => Some(ShutdownReason::CtrlC),
            SHUTDOWN_PLATFORM => Some(ShutdownReason::PlatformRequest),
            SHUTDOWN_WINDOW => Some(ShutdownReason::WindowClose),
            _ => None,
        }
    }

    fn report_event_dispatch(report: nord_app::ApplicationDispatchReport) {
        Self::report_listener_panics(report.listeners_panicked(), "window_lifecycle");
    }

    fn report_input_dispatch(report: nord_app::ApplicationDispatchReport) {
        Self::report_listener_panics(report.listeners_panicked(), "input");
    }

    fn report_listener_panics(count: usize, system: &'static str) {
        if count > 0 {
            nord_app::error!(
                name: "event_listener_panicked",
                target: "nord::events",
                listener_panics = count,
                event_system = system,
                "Event listeners panicked while dispatching"
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ShutdownReason {
    CtrlC,
    PlatformRequest,
    WindowClose,
    InitializationFailure,
    RuntimeFailure,
}

impl ShutdownReason {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::CtrlC => "ctrl_c",
            Self::PlatformRequest => "platform_request",
            Self::WindowClose => "window_close",
            Self::InitializationFailure => "initialization_failure",
            Self::RuntimeFailure => "runtime_failure",
        }
    }
}
