pub(crate) mod config;
mod dependencies;
mod error;
mod lifecycle;
mod runtime;

use crate::gameloop::GameLoop;
use dependencies::AppDependencies;
use error::ApplicationError;
use lifecycle::{ApplicationPhase, Lifecycle};
use runtime::{ApplicationRuntime, ShutdownReason};
use std::time::Duration;

const LOGGER_FLUSH_TIMEOUT: Duration = Duration::from_secs(2);

pub fn run() -> Result<(), ApplicationError> {
    let dependencies = AppDependencies::build().map_err(|failure| {
        let reported = failure.was_reported();
        let source = failure.into_source();
        if reported {
            ApplicationError::reported("bootstrap", source)
        } else {
            ApplicationError::unreported("bootstrap", source)
        }
    })?;
    Application::new(dependencies).run()
}

struct Application {
    dependencies: AppDependencies,
    lifecycle: Lifecycle,
}

impl Application {
    fn new(dependencies: AppDependencies) -> Self {
        Self { dependencies, lifecycle: Lifecycle::new() }
    }

    fn run(mut self) -> Result<(), ApplicationError> {
        let application_span = nord_app::info_span!(
            "application",
            application_name = %self.dependencies.config.application.title,
            application_version = env!("CARGO_PKG_VERSION"),
            process_id = std::process::id(),
            platform_backend = ?self.dependencies.platform.backend(),
            platform_architecture = self.dependencies.platform.architecture(),
        );
        let _entered = application_span.enter();

        let mut game_loop = GameLoop::new(&self.dependencies.memory, &self.dependencies.config)
            .map_err(|source| ApplicationError::unreported("game loop configuration", source))?;
        let (mut runtime, mut runtime_error) = self.initialize_runtime();
        let (shutdown_reason, loop_error) = self.run_runtime(runtime.as_mut(), &mut game_loop);
        if loop_error.is_some() {
            runtime_error = loop_error;
        }

        self.lifecycle.transition(ApplicationPhase::ShuttingDown);
        nord_app::info!(
            name: "application_stopping",
            target: "nord::app",
            reason = shutdown_reason.as_str(),
            "Nord application is shutting down"
        );

        // Stop accepting platform events before the logger is drained.
        drop(runtime);
        if let Err(error) = self.dependencies.renderer.shutdown() {
            nord_app::warn!(
                name: "renderer_shutdown_wait_failed",
                target: "nord::render",
                %error,
                "Renderer did not shut down cleanly before window destruction"
            );
        }
        self.destroy_main_window();
        self.stop_scheduler();
        self.report_logging_health();
        self.log_stopped(shutdown_reason, &game_loop);
        self.flush_logs();
        self.lifecycle.transition(ApplicationPhase::Stopped);

        if let Some(error) = runtime_error {
            return Err(ApplicationError::reported("runtime", error));
        }

        Ok(())
    }

    fn initialize_runtime(
        &mut self,
    ) -> (Option<ApplicationRuntime>, Option<nord_app::ApplicationDiagnosticError>) {
        match ApplicationRuntime::initialize(&self.dependencies.events) {
            Ok(runtime) => {
                self.lifecycle.transition(ApplicationPhase::Running);
                nord_app::info!(
                    name: "application_started",
                    target: "nord::app",
                    runtime_mode = "windowed",
                    platform_backend = ?self.dependencies.platform.backend(),
                    platform_architecture = self.dependencies.platform.architecture(),
                    job_worker_threads = self.dependencies.jobs.worker_threads().get(),
                    task_queue_capacity = self.dependencies.scheduler.config().queue_capacity().get(),
                    "Nord application runtime started"
                );
                (Some(runtime), None)
            }
            Err(error) => {
                nord_app::error!(
                    name: "application_runtime_initialization_failed",
                    target: "nord::app",
                    %error,
                    cause = ?error,
                    "Could not initialize the application runtime"
                );
                self.lifecycle.transition(ApplicationPhase::Failed);
                (None, Some(error))
            }
        }
    }

    fn run_runtime(
        &mut self,
        runtime: Option<&mut ApplicationRuntime>,
        game_loop: &mut GameLoop,
    ) -> (ShutdownReason, Option<nord_app::ApplicationDiagnosticError>) {
        let mut shutdown_reason = ShutdownReason::InitializationFailure;
        let mut runtime_error = None;

        if let Some(runtime) = runtime {
            match runtime.run(
                &self.dependencies.platform,
                &self.dependencies.events,
                &self.dependencies.inputs,
                &mut self.dependencies.renderer,
                game_loop,
            ) {
                Ok(reason) => shutdown_reason = reason,
                Err(error) => {
                    nord_app::error!(
                        name: "application_runtime_failed",
                        target: "nord::app",
                        %error,
                        cause = ?error,
                        "Application runtime stopped because of an error"
                    );
                    shutdown_reason = ShutdownReason::RuntimeFailure;
                    runtime_error = Some(error);
                    self.lifecycle.transition(ApplicationPhase::Failed);
                }
            }
        }
        (shutdown_reason, runtime_error)
    }

    fn report_logging_health(&self) {
        let diagnostics = self.dependencies.logger.diagnostics();
        if diagnostics.dropped_events > 0
            || diagnostics.sink_errors > 0
            || diagnostics.telemetry_export_errors > 0
        {
            nord_app::warn!(
                name: "logging_health_degraded",
                target: "nord::app",
                dropped_events = diagnostics.dropped_events,
                sink_errors = diagnostics.sink_errors,
                telemetry_export_errors = diagnostics.telemetry_export_errors,
                "Logging pipeline reported output issues"
            );
        }
    }

    fn flush_logs(&self) {
        if let Err(error) = self.dependencies.logger.flush(LOGGER_FLUSH_TIMEOUT) {
            nord_app::warn!(
                name: "application_log_flush_failed",
                target: "nord::app",
                %error,
                "Could not flush all logging output during shutdown"
            );
        }
    }

    fn stop_scheduler(&self) {
        if let Err(error) =
            self.dependencies.scheduler.shutdown(nord_app::ApplicationShutdownMode::CancelPending)
        {
            nord_app::warn!(
                name: "task_scheduler_shutdown_failed",
                target: "nord::scheduler",
                %error,
                "Task scheduler did not shut down cleanly"
            );
        }
    }

    fn destroy_main_window(&self) {
        if let Err(error) = self.dependencies.platform.destroy_window(self.dependencies.main_window)
        {
            nord_app::warn!(
                name: "main_window_destroy_failed",
                target: "nord::platform",
                %error,
                "Could not explicitly destroy the main game window"
            );
        }
    }

    fn log_stopped(&self, reason: ShutdownReason, game_loop: &GameLoop) {
        let task_stats = self.dependencies.scheduler.snapshot();
        nord_app::info!(
            name: "application_stopped",
            target: "nord::app",
            reason = reason.as_str(),
            uptime_ms = u64::try_from(self.lifecycle.uptime().as_millis()).unwrap_or(u64::MAX),
            loop_iterations = game_loop.iterations(),
            scheduled_simulation_steps = game_loop.scheduled_steps(),
            dropped_simulation_time_ms = game_loop.dropped_simulation_time().as_millis(),
            last_fps = ?game_loop.latest_fps(),
            tasks_submitted = task_stats.submitted,
            tasks_completed = task_stats.completed,
            tasks_cancelled = task_stats.cancelled,
            tasks_rejected = task_stats.rejected,
            "Nord application runtime stopped"
        );
    }
}
