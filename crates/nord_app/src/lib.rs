//! Application lifecycle infrastructure for Nord Tech consumers.

mod config;
mod crash;
mod events;
mod hash;
mod inputs;
mod jobs;
mod logging;
mod math;
mod memory;
mod platform;
mod render;
mod scheduler;
mod threading;
mod time;

pub use config::{
    ApplicationConfigError, ApplicationConfigFileFormat, ApplicationConfigLoader,
    ApplicationConfigParseError, ApplicationConfigValidator,
};
pub use crash::{
    ApplicationCrashReporter, ApplicationCrashReporterConfig, ApplicationCrashReporterDiagnostics,
    ApplicationCrashReporterInitError,
};
pub use events::{
    ApplicationDispatchReport, ApplicationEvent, ApplicationEventSubscription,
    ApplicationEventSystem, ApplicationLifecycleEvent, ApplicationWindowEvent,
    ApplicationWindowHandle,
};
pub use hash::{
    ApplicationContentHash, ApplicationContentHasher, ApplicationHashBuilder,
    ApplicationHashFileError, ApplicationParseContentHashError, application_hash_bytes,
    application_hash_bytes_with_domain, application_hash_file, application_hash_reader,
};
pub use inputs::{
    ApplicationInputEvent, ApplicationInputSubscription, ApplicationInputSystem,
    ApplicationKeyCode, ApplicationModifiers, ApplicationPointerButton,
};
pub use jobs::ApplicationJobSystem;
pub use logging::{
    ApplicationInstrument, ApplicationJsonFileConfig, ApplicationLogDiagnostics,
    ApplicationLogFileRotation, ApplicationLogFlushError, ApplicationLogQueueFullPolicy,
    ApplicationLogger, ApplicationLoggerConfig, ApplicationLoggerInitError,
    ApplicationOpenTelemetryConfig, ApplicationTerminalOutput,
};
pub use math::*;
pub use memory::{
    ApplicationArenaAllocationError, ApplicationMemoryBudgetError, ApplicationMemoryBudgetSnapshot,
    ApplicationMemoryManager, ApplicationMemoryReservation, ApplicationObjectPool,
    ApplicationPoolHandle, ApplicationPoolInsertError, ApplicationScratchArena,
};
pub use nord_diagnostics::{
    Context as ApplicationErrorContext, Error as ApplicationDiagnosticError,
    Result as ApplicationResult, anyhow, bail, ensure, ensure_eq, ensure_ne,
};
pub use nord_diagnostics::{debug_invariant, invariant};
pub use nord_log::{Level, debug, error, event, info, info_span, instrument, span, trace, warn};

pub use platform::{
    ApplicationDisplayError, ApplicationDisplayManager, ApplicationDisplayMonitor,
    ApplicationDisplayRect, ApplicationEventEmitter, ApplicationEventError, ApplicationFileAccess,
    ApplicationFileDisposition, ApplicationFileHandle, ApplicationFileOpenOptions,
    ApplicationFileSystem, ApplicationPlatform, ApplicationPlatformBackend,
    ApplicationPlatformError, ApplicationPlatformEvent, ApplicationPlatformEventKind,
    ApplicationTimestamp, ApplicationWindowConfig, ApplicationWindowError,
};
pub use render::{
    ApplicationBlendMode, ApplicationBufferDesc, ApplicationBufferMemory, ApplicationBufferUsage,
    ApplicationClearColor, ApplicationColorAttachmentBinding, ApplicationCompiledRenderGraph,
    ApplicationCompiledShader, ApplicationComputePipelineDesc, ApplicationCullMode,
    ApplicationDeferredGpuAllocator, ApplicationDescriptorHandle, ApplicationDescriptorKind,
    ApplicationDrawCall, ApplicationFrontFace, ApplicationGpuAllocation,
    ApplicationGpuAllocationClass, ApplicationGpuAllocatorStats, ApplicationGpuMemoryBudget,
    ApplicationGpuMemorySegment, ApplicationGpuResourceHandle, ApplicationGpuResourceKind,
    ApplicationGraphResourceRef, ApplicationGraphicsPipelineDesc, ApplicationIndexBufferBinding,
    ApplicationIndexFormat, ApplicationPipelineCacheEntry, ApplicationPipelineDescriptorBinding,
    ApplicationPipelineDescriptorTableBinding, ApplicationPolygonFillMode,
    ApplicationPrimitiveTopology, ApplicationRenderBackend, ApplicationRenderBarrier,
    ApplicationRenderBarrierKind, ApplicationRenderCommandEncoder,
    ApplicationRenderCommandEncodingError, ApplicationRenderConfig, ApplicationRenderError,
    ApplicationRenderPassDesc, ApplicationRenderPassId, ApplicationRenderQueue,
    ApplicationRenderQueueError, ApplicationRenderTargetDesc, ApplicationRenderTargetResources,
    ApplicationRenderer, ApplicationResourceAccess, ApplicationResourceDescriptionError,
    ApplicationResourceUsage, ApplicationSamplerAddressMode, ApplicationSamplerDesc,
    ApplicationSamplerDescriptionError, ApplicationSamplerFilter, ApplicationScissorRect,
    ApplicationShaderBindingKind, ApplicationShaderCompileError, ApplicationShaderCompileOptions,
    ApplicationShaderCompiler, ApplicationShaderDiagnostic, ApplicationShaderId,
    ApplicationShaderReflection, ApplicationShaderResourceBinding, ApplicationShaderStage,
    ApplicationTextureDesc, ApplicationTextureFormat, ApplicationTextureUsage,
    ApplicationTransientAliasGroup, ApplicationTransientTextureId,
    ApplicationTransientTextureLifetime, ApplicationVertexAttribute,
    ApplicationVertexBufferBinding, ApplicationVertexFormat, ApplicationViewport,
};
pub use scheduler::{
    ApplicationCancellationToken, ApplicationSchedulerBuildError, ApplicationSchedulerConfig,
    ApplicationSchedulerShutdownError, ApplicationSchedulerSnapshot,
    ApplicationSchedulerSubmitError, ApplicationShutdownMode, ApplicationTaskContext,
    ApplicationTaskExecutionError, ApplicationTaskHandle, ApplicationTaskId,
    ApplicationTaskPriority, ApplicationTaskScheduler,
};
pub use threading::{
    ApplicationThreadPool, ApplicationThreadPoolBuildError, ApplicationThreadPoolConfig,
};
pub use time::{
    ApplicationClockConfig, ApplicationFixedStepAccumulator, ApplicationFixedStepConfig,
    ApplicationFixedStepConfigError, ApplicationFixedStepPlan, ApplicationFpsCounter,
    ApplicationFpsSnapshot, ApplicationFpsWindowError, ApplicationFrameClock, ApplicationFrameTime,
    ApplicationStopwatch, ApplicationTimeScaleError, ApplicationUnixTimestamp,
    ApplicationUnixTimestampError,
};

use nord_dependency::{DependencyBuilder, DependencyContainer, DependencyError};
use std::any::{Any, type_name};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::Arc;

/// Stable application-facing error for service registration and lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ApplicationServicesError {
    /// A concrete service type has already been registered.
    AlreadyRegistered { type_name: &'static str },
    /// No service of the requested concrete type is registered.
    Missing { type_name: &'static str },
}

impl Display for ApplicationServicesError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyRegistered { type_name } => {
                write!(formatter, "application service `{type_name}` is already registered")
            }
            Self::Missing { type_name } => {
                write!(formatter, "application service `{type_name}` is not registered")
            }
        }
    }
}

impl Error for ApplicationServicesError {}

fn map_dependency_error(error: &DependencyError) -> ApplicationServicesError {
    match error {
        DependencyError::AlreadyRegistered { type_name } => {
            ApplicationServicesError::AlreadyRegistered { type_name }
        }
        DependencyError::Missing { type_name } => ApplicationServicesError::Missing { type_name },
    }
}

/// Mutable service registration API owned by the application layer.
#[derive(Default)]
pub struct ApplicationServicesBuilder {
    inner: DependencyBuilder,
}

impl ApplicationServicesBuilder {
    /// Create an empty application service builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register one shared service value by its concrete type.
    ///
    /// # Errors
    ///
    /// Returns [`ApplicationServicesError::AlreadyRegistered`] when this type
    /// has already been registered.
    pub fn register<T>(&mut self, service: T) -> Result<(), ApplicationServicesError>
    where
        T: Any + Send + Sync,
    {
        self.inner.register(service).map_err(|error| map_dependency_error(&error))
    }

    /// Register an existing shared service allocation.
    ///
    /// # Errors
    ///
    /// Returns [`ApplicationServicesError::AlreadyRegistered`] when this type
    /// has already been registered.
    pub fn register_shared<T>(&mut self, service: Arc<T>) -> Result<(), ApplicationServicesError>
    where
        T: Any + Send + Sync,
    {
        self.inner.register_shared(service).map_err(|error| map_dependency_error(&error))
    }

    /// Finish registration and create an immutable service collection.
    #[must_use]
    pub fn build(self) -> ApplicationServices {
        ApplicationServices { inner: self.inner.build() }
    }
}

/// Immutable, thread-safe application services.
pub struct ApplicationServices {
    inner: DependencyContainer,
}

impl ApplicationServices {
    /// Resolve a service by its concrete type.
    ///
    /// # Errors
    ///
    /// Returns [`ApplicationServicesError::Missing`] if the type was not
    /// registered.
    pub fn get<T>(&self) -> Result<Arc<T>, ApplicationServicesError>
    where
        T: Any + Send + Sync,
    {
        self.inner
            .get()
            .map_err(|_| ApplicationServicesError::Missing { type_name: type_name::<T>() })
    }

    /// Check whether a concrete service type has been registered.
    #[must_use]
    pub fn contains<T>(&self) -> bool
    where
        T: Any + Send + Sync,
    {
        self.inner.contains::<T>()
    }
}
