use super::config::GameConfig;
use crate::render::GameRenderer;
use nord_app::{
    ApplicationConfigLoader, ApplicationCrashReporter, ApplicationCrashReporterConfig,
    ApplicationDiagnosticError, ApplicationErrorContext, ApplicationEventSystem,
    ApplicationInputSystem, ApplicationJobSystem, ApplicationJsonFileConfig, ApplicationLogger,
    ApplicationLoggerConfig, ApplicationMemoryManager, ApplicationOpenTelemetryConfig,
    ApplicationPlatform, ApplicationResult, ApplicationSchedulerConfig, ApplicationServicesBuilder,
    ApplicationTaskScheduler, ApplicationThreadPool, ApplicationThreadPoolConfig,
    ApplicationWindowConfig,
};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(super) struct AppDependencies {
    pub(super) config: Arc<GameConfig>,
    pub(super) logger: Arc<ApplicationLogger>,
    pub(super) _crash_reporter: Arc<ApplicationCrashReporter>,
    pub(super) memory: Arc<ApplicationMemoryManager>,
    pub(super) platform: Arc<ApplicationPlatform>,
    pub(super) main_window: nord_app::ApplicationWindowHandle,
    pub(super) renderer: GameRenderer,
    pub(super) events: Arc<ApplicationEventSystem>,
    pub(super) inputs: Arc<ApplicationInputSystem>,
    pub(super) jobs: Arc<ApplicationJobSystem>,
}

impl AppDependencies {
    pub(super) fn build() -> Result<Self, BootstrapError> {
        // Platform paths are needed to locate the log directory. Start the
        // structured logger before loading user configuration or creating a
        // window so those failures are captured by the normal log pipeline.
        let platform = ApplicationPlatform::initialize("nord")
            .context("initializing native platform capabilities")
            .map_err(BootstrapError::unreported)?;
        let log_directory = platform.data_dir().join("logs");
        let telemetry = ApplicationOpenTelemetryConfig::from_environment()
            .map_err(ApplicationDiagnosticError::msg)
            .context("reading OpenTelemetry configuration")
            .map_err(BootstrapError::unreported)?;
        let logger = Arc::new(
            ApplicationLogger::initialize(
                ApplicationLoggerConfig::new()
                    .with_json_file(ApplicationJsonFileConfig::daily(&log_directory, "nord"))
                    .with_open_telemetry(telemetry),
            )
            .context("initializing the application logger")
            .map_err(BootstrapError::unreported)?,
        );

        match Self::build_with_logger(platform, Arc::clone(&logger), &log_directory) {
            Ok(dependencies) => Ok(dependencies),
            Err(error) => {
                nord_app::error!(
                    name: "application_bootstrap_failed",
                    target: "nord::app",
                    %error,
                    cause = ?error,
                    "Nord could not complete application bootstrap"
                );
                Err(BootstrapError::reported(error))
            }
        }
    }

    fn build_with_logger(
        platform: ApplicationPlatform,
        logger: Arc<ApplicationLogger>,
        log_directory: &Path,
    ) -> ApplicationResult<Self> {
        let mut builder = ApplicationServicesBuilder::new();
        let crash_reporter = ApplicationCrashReporter::initialize(
            ApplicationCrashReporterConfig::new(log_directory.join("crashes.jsonl"))
                .with_application_identity("Nord", env!("CARGO_PKG_VERSION")),
            logger.panic_flush_callback(),
        )
        .context("initializing application crash diagnostics")?;
        let loader = ApplicationConfigLoader::new(GameConfig::default())
            .with_environment_prefix("NORD")
            .validate_with(GameConfig::validate);
        let config = match std::env::var_os("NORD_CONFIG_FILE") {
            Some(path) => loader.with_file(PathBuf::from(path)).load(),
            None => loader.with_optional_file(platform.config_dir().join("nord.toml")).load(),
        }
        .context("loading and validating game configuration")?;
        let memory = Arc::new(ApplicationMemoryManager::new(config.memory.budget_bytes));
        let configured_worker_threads = config.threading.worker_threads;
        let events = Arc::new(ApplicationEventSystem::new());
        let inputs = Arc::new(ApplicationInputSystem::new());
        let main_window = create_main_window(&platform, &config)?;
        builder.register(config.clone()).context("registering game configuration")?;
        let thread_pool_config = match configured_worker_threads {
            Some(count) => ApplicationThreadPoolConfig::default().with_worker_threads(
                NonZeroUsize::new(count).ok_or_else(|| {
                    ApplicationDiagnosticError::msg(
                        "threading.worker_threads must be greater than zero",
                    )
                })?,
            ),
            None => ApplicationThreadPoolConfig::default(),
        };
        let thread_pool = Arc::new(
            ApplicationThreadPool::new(thread_pool_config)
                .context("creating the CPU worker pool")?,
        );
        let jobs = Arc::new(ApplicationJobSystem::new(Arc::clone(&thread_pool)));
        let max_in_flight = match config.tasks.max_in_flight {
            Some(value) => Some(NonZeroUsize::new(value).ok_or_else(|| {
                ApplicationDiagnosticError::msg(
                    "tasks.max_in_flight must be greater than zero when set",
                )
            })?),
            None => None,
        };
        let task_config = ApplicationSchedulerConfig::default()
            .with_queue_capacity(NonZeroUsize::new(config.tasks.queue_capacity).ok_or_else(
                || {
                    ApplicationDiagnosticError::msg(
                        "tasks.queue_capacity must be greater than zero",
                    )
                },
            )?)
            .with_max_in_flight(max_in_flight)
            .with_priority_aging_interval(std::time::Duration::from_millis(
                config.tasks.priority_aging_interval_ms,
            ));
        let scheduler = Arc::new(
            ApplicationTaskScheduler::new(Arc::clone(&thread_pool), task_config)
                .context("creating the bounded priority task scheduler")?,
        );
        if let Some(error) = &crash_reporter.diagnostics().native_minidump_error {
            nord_app::warn!(
                name: "native_crash_dumps_unavailable",
                target: "nord::crash",
                %error,
                panic_reports_enabled = true,
                "Native minidump monitor could not be started; panic reports remain enabled"
            );
        }
        builder.register(platform).context("registering platform capabilities")?;
        builder.register(main_window).context("registering the main game window handle")?;
        builder.register_shared(events).context("registering the event system")?;
        builder.register_shared(inputs).context("registering the input system")?;
        builder.register_shared(logger).context("registering the application logger")?;
        builder.register(crash_reporter).context("registering application crash diagnostics")?;
        builder.register_shared(memory).context("registering application memory manager")?;
        builder.register_shared(thread_pool).context("registering the worker pool")?;
        builder.register_shared(jobs).context("registering the job system")?;
        builder.register_shared(scheduler).context("registering the task scheduler")?;

        // Keep the DI implementation behind this game-owned composition adapter.
        let container = builder.build();
        Self::resolve(&container)
    }

    fn resolve(container: &nord_app::ApplicationServices) -> ApplicationResult<Self> {
        let config = container
            .get::<GameConfig>()
            .context("resolving game configuration from application services")?;
        let platform = container
            .get::<ApplicationPlatform>()
            .context("resolving platform capabilities from application services")?;
        let main_window = *container
            .get::<nord_app::ApplicationWindowHandle>()
            .context("resolving the main game window from application services")?;
        let jobs = container
            .get::<ApplicationJobSystem>()
            .context("resolving the job system from application services")?;
        let scheduler = container
            .get::<ApplicationTaskScheduler>()
            .context("resolving the task scheduler from application services")?;
        let memory = container
            .get::<ApplicationMemoryManager>()
            .context("resolving application memory manager from services")?;
        let renderer = GameRenderer::initialize(
            &platform,
            main_window,
            config.window.width,
            config.window.height,
            crate::render::GameRenderServices {
                jobs: (*jobs).clone(),
                scheduler: (*scheduler).clone(),
                chunks_above_origin: config.world.chunks_above_origin,
                chunks_below_origin: config.world.chunks_below_origin,
            },
            &memory,
        )?;
        Ok(Self {
            config,
            logger: container
                .get::<ApplicationLogger>()
                .context("resolving the application logger from services")?,
            _crash_reporter: container
                .get::<ApplicationCrashReporter>()
                .context("resolving application crash diagnostics from services")?,
            memory,
            platform,
            main_window,
            renderer,
            events: container
                .get::<ApplicationEventSystem>()
                .context("resolving the event system")?,
            inputs: container
                .get::<ApplicationInputSystem>()
                .context("resolving the input system")?,
            jobs,
        })
    }
}

/// Whether a bootstrap failure has already been written to the structured log.
pub(super) struct BootstrapError {
    source: nord_app::ApplicationDiagnosticError,
    reported: bool,
}

impl BootstrapError {
    fn unreported(source: nord_app::ApplicationDiagnosticError) -> Self {
        Self { source, reported: false }
    }

    fn reported(source: nord_app::ApplicationDiagnosticError) -> Self {
        Self { source, reported: true }
    }

    pub(super) const fn was_reported(&self) -> bool {
        self.reported
    }

    pub(super) fn into_source(self) -> nord_app::ApplicationDiagnosticError {
        self.source
    }
}

impl std::fmt::Display for BootstrapError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.source, formatter)
    }
}

impl std::fmt::Debug for BootstrapError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.source, formatter)
    }
}

impl std::error::Error for BootstrapError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.source.as_ref())
    }
}

fn create_main_window(
    platform: &ApplicationPlatform,
    config: &GameConfig,
) -> ApplicationResult<nord_app::ApplicationWindowHandle> {
    log_display_summary(platform);
    let handle = platform
        .create_window(
            &ApplicationWindowConfig::new(
                config.application.title.clone(),
                config.window.width,
                config.window.height,
            )
            .with_resizable(config.window.resizable),
        )
        .context("creating the main game window")?;
    nord_app::info!(
        name: "main_window_created",
        target: "nord::platform",
        title = %config.application.title,
        width = config.window.width,
        height = config.window.height,
        backend = ?platform.backend(),
        "Main game window created"
    );
    Ok(handle)
}

fn log_display_summary(platform: &ApplicationPlatform) {
    match platform.display_manager().monitors() {
        Ok(monitors) => {
            if let Some(primary) = monitors.iter().find(|monitor| monitor.is_primary()) {
                nord_app::info!(
                    name: "primary_display_detected",
                    target: "nord::platform",
                    monitor_count = monitors.len(),
                    device = primary.device_name(),
                    width = primary.bounds().width(),
                    height = primary.bounds().height(),
                    work_width = primary.work_area().width(),
                    work_height = primary.work_area().height(),
                    "Primary display detected"
                );
            } else {
                nord_app::warn!(
                    name: "primary_display_unavailable",
                    target: "nord::platform",
                    monitor_count = monitors.len(),
                    "No primary display was reported by the platform"
                );
            }
        }
        Err(error) => nord_app::warn!(
            name: "display_enumeration_failed",
            target: "nord::platform",
            %error,
            "Could not enumerate connected displays"
        ),
    }
}
