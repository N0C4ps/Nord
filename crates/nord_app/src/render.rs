use crate::{ApplicationJobSystem, ApplicationPlatform, ApplicationWindowHandle};
use std::error::Error;
use std::fmt::{Display, Formatter};

pub use nord_render::{
    ClearColor as ApplicationClearColor, GpuMemoryBudget as ApplicationGpuMemoryBudget,
    GpuMemorySegment as ApplicationGpuMemorySegment, RenderBackend as ApplicationRenderBackend,
};

pub use nord_render::{
    ColorAttachmentBinding as ApplicationColorAttachmentBinding,
    CompiledRenderGraph as ApplicationCompiledRenderGraph, DrawCall as ApplicationDrawCall,
    GraphResourceRef as ApplicationGraphResourceRef,
    IndexBufferBinding as ApplicationIndexBufferBinding, IndexFormat as ApplicationIndexFormat,
    RenderBarrier as ApplicationRenderBarrier, RenderBarrierKind as ApplicationRenderBarrierKind,
    RenderCommandEncoder as ApplicationRenderCommandEncoder,
    RenderCommandEncodingError as ApplicationRenderCommandEncodingError,
    RenderPassDesc as ApplicationRenderPassDesc, RenderPassId as ApplicationRenderPassId,
    ResourceAccess as ApplicationResourceAccess, ResourceUsage as ApplicationResourceUsage,
    ScissorRect as ApplicationScissorRect, TransientAliasGroup as ApplicationTransientAliasGroup,
    TransientTextureId as ApplicationTransientTextureId,
    TransientTextureLifetime as ApplicationTransientTextureLifetime,
};

/// Backend-neutral GPU resource, descriptor, shader and pipeline APIs managed
/// through the application-owned renderer facade.
pub use nord_render::{
    Allocation as ApplicationGpuAllocation, AllocationClass as ApplicationGpuAllocationClass,
    BlendMode as ApplicationBlendMode, BufferDesc as ApplicationBufferDesc,
    BufferMemory as ApplicationBufferMemory, BufferUsage as ApplicationBufferUsage,
    CompiledShader as ApplicationCompiledShader,
    ComputePipelineDesc as ApplicationComputePipelineDesc, CullMode as ApplicationCullMode,
    DeferredGpuAllocator as ApplicationDeferredGpuAllocator,
    DescriptorHandle as ApplicationDescriptorHandle, DescriptorKind as ApplicationDescriptorKind,
    FrontFace as ApplicationFrontFace, GpuAllocatorStats as ApplicationGpuAllocatorStats,
    GpuResourceHandle as ApplicationGpuResourceHandle,
    GraphicsPipelineDesc as ApplicationGraphicsPipelineDesc,
    PipelineCacheEntry as ApplicationPipelineCacheEntry,
    PipelineDescriptorBinding as ApplicationPipelineDescriptorBinding,
    PipelineDescriptorTableBinding as ApplicationPipelineDescriptorTableBinding,
    PolygonFillMode as ApplicationPolygonFillMode,
    PrimitiveTopology as ApplicationPrimitiveTopology,
    RenderTargetDesc as ApplicationRenderTargetDesc,
    RenderTargetResources as ApplicationRenderTargetResources,
    ResourceDescriptionError as ApplicationResourceDescriptionError,
    ResourceKind as ApplicationGpuResourceKind,
    SamplerAddressMode as ApplicationSamplerAddressMode, SamplerDesc as ApplicationSamplerDesc,
    SamplerDescriptionError as ApplicationSamplerDescriptionError,
    SamplerFilter as ApplicationSamplerFilter, ShaderBindingKind as ApplicationShaderBindingKind,
    ShaderCompileError as ApplicationShaderCompileError,
    ShaderCompileOptions as ApplicationShaderCompileOptions,
    ShaderCompiler as ApplicationShaderCompiler, ShaderDiagnostic as ApplicationShaderDiagnostic,
    ShaderId as ApplicationShaderId, ShaderReflection as ApplicationShaderReflection,
    ShaderResourceBinding as ApplicationShaderResourceBinding,
    ShaderStage as ApplicationShaderStage, TextureDesc as ApplicationTextureDesc,
    TextureFormat as ApplicationTextureFormat, TextureUsage as ApplicationTextureUsage,
    VertexAttribute as ApplicationVertexAttribute,
    VertexBufferBinding as ApplicationVertexBufferBinding, VertexFormat as ApplicationVertexFormat,
    Viewport as ApplicationViewport,
};

/// Stable game-facing configuration for Nord's renderer infrastructure.
pub use nord_render::RenderConfig as ApplicationRenderConfig;

/// Failure reported by the application renderer facade.
#[derive(Debug)]
pub struct ApplicationRenderError(nord_render::RenderError);

impl Display for ApplicationRenderError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, formatter)
    }
}

impl Error for ApplicationRenderError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.0)
    }
}

impl From<ApplicationRenderCommandEncodingError> for ApplicationRenderError {
    fn from(error: ApplicationRenderCommandEncodingError) -> Self {
        Self(nord_render::RenderError::Backend(error.to_string()))
    }
}

/// App-owned renderer facade; native handles and backend objects stay out of game code.
pub struct ApplicationRenderer {
    inner: nord_render::Renderer,
}

/// Game-facing render queue. Pass callbacks use the application facade and
/// cannot access native D3D12 objects.
pub type ApplicationRenderQueue = nord_render::RenderQueue<ApplicationRenderError>;

pub type ApplicationRenderQueueError = nord_render::RenderQueueError<ApplicationRenderError>;

impl ApplicationRenderer {
    /// Bind Nord's renderer to a window created by the application platform.
    ///
    /// # Errors
    ///
    /// Returns an error if the window or graphics backend cannot be initialized.
    pub fn initialize(
        platform: &ApplicationPlatform,
        window: ApplicationWindowHandle,
        config: ApplicationRenderConfig,
    ) -> Result<Self, ApplicationRenderError> {
        let native_window = platform.native_window_handle(window).map_err(|error| {
            ApplicationRenderError(nord_render::RenderError::Backend(error.to_string()))
        })?;
        nord_render::Renderer::new(native_window, config)
            .map(|inner| Self { inner })
            .map_err(ApplicationRenderError)
    }

    /// Current graphics backend.
    #[must_use]
    pub const fn backend(&self) -> ApplicationRenderBackend {
        self.inner.backend()
    }

    /// Submit and present a frame.
    ///
    /// # Errors
    ///
    /// Returns an error if recording, GPU synchronization, or presentation fails.
    pub fn render_frame(
        &mut self,
        clear_color: ApplicationClearColor,
    ) -> Result<(), ApplicationRenderError> {
        self.inner.render_frame(clear_color).map_err(ApplicationRenderError)
    }

    /// Present a completed surface-sized RGBA8 texture through the swapchain.
    ///
    /// # Errors
    /// Returns an error when the texture is incompatible with the surface or
    /// GPU submission/presentation fails.
    pub fn present_texture(
        &mut self,
        source: ApplicationGpuResourceHandle,
    ) -> Result<(), ApplicationRenderError> {
        self.inner.present_texture(source).map_err(ApplicationRenderError)
    }

    /// Wait for DXGI frame readiness before input and simulation work.
    ///
    /// # Errors
    ///
    /// Returns an error if the presentation backend cannot be synchronized.
    pub fn wait_for_frame(&self) -> Result<(), ApplicationRenderError> {
        self.inner.wait_for_frame().map_err(ApplicationRenderError)
    }

    /// Forward native surface dimensions after a resize event.
    ///
    /// # Errors
    ///
    /// Returns an error if the swapchain resize fails.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), ApplicationRenderError> {
        self.inner.resize(width, height).map_err(ApplicationRenderError)
    }

    /// Whether the current native surface is minimized or has zero extent.
    #[must_use]
    pub const fn surface_suspended(&self) -> bool {
        self.inner.surface_suspended()
    }

    /// Snapshot renderer allocator counters without exposing the internal service container.
    #[must_use]
    pub fn gpu_allocator_stats(&mut self) -> ApplicationGpuAllocatorStats {
        self.inner.services().allocator().stats()
    }

    /// Query the OS-reported GPU memory budget for a local/non-local segment.
    ///
    /// # Errors
    /// Returns an error when the graphics backend cannot query the adapter.
    pub fn memory_budget(
        &self,
        segment: ApplicationGpuMemorySegment,
    ) -> Result<ApplicationGpuMemoryBudget, ApplicationRenderError> {
        self.inner.memory_budget(segment).map_err(ApplicationRenderError)
    }

    /// Wait for GPU work and release excess empty native heaps while retaining
    /// one reusable heap per allocation class.
    ///
    /// # Errors
    /// Returns an error if GPU synchronization fails.
    pub fn trim_memory_pools(&mut self) -> Result<u64, ApplicationRenderError> {
        self.inner.trim_memory_pools().map_err(ApplicationRenderError)
    }

    /// Create a GPU buffer through the renderer's device-backed allocator.
    ///
    /// # Errors
    /// Returns an error when the description is incompatible or allocation fails.
    pub fn create_buffer(
        &mut self,
        desc: ApplicationBufferDesc,
    ) -> Result<ApplicationGpuResourceHandle, ApplicationRenderError> {
        self.inner.create_buffer(desc).map_err(ApplicationRenderError)
    }

    /// Read a GPU-written range from a CPU-visible readback buffer.
    ///
    /// # Errors
    /// Returns an error for stale/non-readback handles, invalid ranges, or map failure.
    pub fn read_buffer(
        &mut self,
        handle: ApplicationGpuResourceHandle,
        offset: u64,
        length: usize,
    ) -> Result<Vec<u8>, ApplicationRenderError> {
        self.inner.read_buffer(handle, offset, length).map_err(ApplicationRenderError)
    }

    /// Copy a device-local buffer range into a readback buffer and wait for the GPU fence.
    ///
    /// # Errors
    /// Returns an error for incompatible resources, invalid ranges/alignment, or queue failure.
    pub fn copy_buffer_to_readback(
        &mut self,
        source: ApplicationGpuResourceHandle,
        source_offset: u64,
        destination: ApplicationGpuResourceHandle,
        destination_offset: u64,
        length: u64,
    ) -> Result<(), ApplicationRenderError> {
        self.inner
            .copy_buffer_to_readback(source, source_offset, destination, destination_offset, length)
            .map_err(ApplicationRenderError)
    }

    /// Create a GPU texture through the renderer's device-backed allocator.
    ///
    /// # Errors
    /// Returns an error when the description is incompatible or allocation fails.
    pub fn create_texture(
        &mut self,
        desc: ApplicationTextureDesc,
    ) -> Result<ApplicationGpuResourceHandle, ApplicationRenderError> {
        self.inner.create_texture(desc).map_err(ApplicationRenderError)
    }

    /// Create color/depth texture resources for an offscreen render target.
    ///
    /// # Errors
    /// Returns an error when attachments are incompatible or allocation fails.
    pub fn create_render_target(
        &mut self,
        desc: ApplicationRenderTargetDesc,
    ) -> Result<ApplicationRenderTargetResources, ApplicationRenderError> {
        self.inner.create_render_target(desc).map_err(ApplicationRenderError)
    }

    /// Create a native shader resource view for a texture.
    ///
    /// # Errors
    /// Returns an error for a stale/incompatible resource or exhausted descriptor heap.
    pub fn create_texture_srv(
        &mut self,
        resource: ApplicationGpuResourceHandle,
    ) -> Result<ApplicationDescriptorHandle, ApplicationRenderError> {
        self.inner.create_texture_srv(resource).map_err(ApplicationRenderError)
    }

    /// Create a raw shader-resource view over a storage buffer.
    ///
    /// # Errors
    /// Returns an error for an incompatible buffer or descriptor allocation failure.
    pub fn create_buffer_srv(
        &mut self,
        resource: ApplicationGpuResourceHandle,
    ) -> Result<ApplicationDescriptorHandle, ApplicationRenderError> {
        self.inner.create_buffer_srv(resource).map_err(ApplicationRenderError)
    }

    /// Create a contiguous SRV range matching one HLSL texture array.
    ///
    /// # Errors
    /// Returns an error for incompatible resources or descriptor allocation failure.
    pub fn create_texture_srv_array(
        &mut self,
        resources: &[ApplicationGpuResourceHandle],
    ) -> Result<Vec<ApplicationDescriptorHandle>, ApplicationRenderError> {
        self.inner.create_texture_srv_array(resources).map_err(ApplicationRenderError)
    }

    /// Create a constant-buffer view in the shader-visible resource heap.
    ///
    /// # Errors
    /// Returns an error when the buffer does not satisfy constant-buffer requirements.
    pub fn create_constant_buffer_view(
        &mut self,
        resource: ApplicationGpuResourceHandle,
    ) -> Result<ApplicationDescriptorHandle, ApplicationRenderError> {
        self.inner.create_constant_buffer_view(resource).map_err(ApplicationRenderError)
    }

    /// Create a contiguous CBV descriptor range for a shader CBV array.
    ///
    /// # Errors
    /// Returns an error when a resource is incompatible or allocation fails.
    pub fn create_constant_buffer_view_array(
        &mut self,
        resources: &[ApplicationGpuResourceHandle],
    ) -> Result<Vec<ApplicationDescriptorHandle>, ApplicationRenderError> {
        self.inner.create_constant_buffer_view_array(resources).map_err(ApplicationRenderError)
    }

    /// Create an unordered-access view for a storage buffer or texture.
    ///
    /// # Errors
    /// Returns an error when storage usage is absent or descriptor capacity is exhausted.
    pub fn create_unordered_access_view(
        &mut self,
        resource: ApplicationGpuResourceHandle,
    ) -> Result<ApplicationDescriptorHandle, ApplicationRenderError> {
        self.inner.create_unordered_access_view(resource).map_err(ApplicationRenderError)
    }

    /// Create a contiguous UAV descriptor range for a shader UAV array.
    ///
    /// # Errors
    /// Returns an error when storage usage is missing or allocation fails.
    pub fn create_unordered_access_view_array(
        &mut self,
        resources: &[ApplicationGpuResourceHandle],
    ) -> Result<Vec<ApplicationDescriptorHandle>, ApplicationRenderError> {
        self.inner.create_unordered_access_view_array(resources).map_err(ApplicationRenderError)
    }

    /// Create a native render-target view for a color attachment.
    ///
    /// # Errors
    /// Returns an error when the resource is not a live color target or the heap is full.
    pub fn create_render_target_view(
        &mut self,
        resource: ApplicationGpuResourceHandle,
    ) -> Result<ApplicationDescriptorHandle, ApplicationRenderError> {
        self.inner.create_render_target_view(resource).map_err(ApplicationRenderError)
    }

    /// Create a native depth-stencil view for a depth attachment.
    ///
    /// # Errors
    /// Returns an error when the resource is not a live depth target or the heap is full.
    pub fn create_depth_stencil_view(
        &mut self,
        resource: ApplicationGpuResourceHandle,
    ) -> Result<ApplicationDescriptorHandle, ApplicationRenderError> {
        self.inner.create_depth_stencil_view(resource).map_err(ApplicationRenderError)
    }

    /// Create a native sampler descriptor.
    ///
    /// # Errors
    /// Returns an error for invalid sampler settings or descriptor exhaustion.
    pub fn create_sampler(
        &mut self,
        sampler: ApplicationSamplerDesc,
    ) -> Result<ApplicationDescriptorHandle, ApplicationRenderError> {
        self.inner.create_sampler(sampler).map_err(ApplicationRenderError)
    }

    /// Create a contiguous sampler descriptor range.
    ///
    /// # Errors
    /// Returns an error for invalid sampler state or descriptor allocation failure.
    pub fn create_sampler_array(
        &mut self,
        samplers: &[ApplicationSamplerDesc],
    ) -> Result<Vec<ApplicationDescriptorHandle>, ApplicationRenderError> {
        self.inner.create_sampler_array(samplers).map_err(ApplicationRenderError)
    }

    /// Release a native descriptor after retiring references to it.
    ///
    /// # Errors
    /// Returns an error for a stale descriptor or failed GPU synchronization.
    pub fn destroy_descriptor(
        &mut self,
        descriptor: ApplicationDescriptorHandle,
    ) -> Result<(), ApplicationRenderError> {
        self.inner.destroy_descriptor(descriptor).map_err(ApplicationRenderError)
    }

    /// Create or reuse the native graphics pipeline state for compiled stages.
    ///
    /// # Errors
    /// Returns an error when shaders/states are incompatible or native PSO creation fails.
    pub fn create_graphics_pipeline(
        &mut self,
        desc: &ApplicationGraphicsPipelineDesc,
        vertex: &ApplicationCompiledShader,
        pixel: &ApplicationCompiledShader,
        frame: u64,
    ) -> Result<ApplicationPipelineCacheEntry, ApplicationRenderError> {
        self.inner
            .create_graphics_pipeline(desc, vertex, pixel, frame)
            .map_err(ApplicationRenderError)
    }

    /// Create or reuse the native compute pipeline state for a compiled shader.
    ///
    /// # Errors
    /// Returns an error when the shader stage/identity is incompatible or native creation fails.
    pub fn create_compute_pipeline(
        &mut self,
        desc: ApplicationComputePipelineDesc,
        compute: &ApplicationCompiledShader,
        frame: u64,
    ) -> Result<ApplicationPipelineCacheEntry, ApplicationRenderError> {
        self.inner.create_compute_pipeline(desc, compute, frame).map_err(ApplicationRenderError)
    }

    /// Execute one synchronous offscreen graphics pass through the application
    /// renderer facade. Resource handles remain opaque to game code.
    ///
    /// # Errors
    /// Returns an error when pass validation, command submission or GPU work fails.
    #[allow(clippy::too_many_arguments)]
    pub fn execute_graphics_pass(
        &mut self,
        desc: &ApplicationGraphicsPipelineDesc,
        vertex: &ApplicationCompiledShader,
        pixel: &ApplicationCompiledShader,
        color: ApplicationGpuResourceHandle,
        color_view: ApplicationDescriptorHandle,
        depth: Option<(ApplicationGpuResourceHandle, ApplicationDescriptorHandle)>,
        bindings: &[ApplicationPipelineDescriptorBinding],
        clear_color: ApplicationClearColor,
        vertex_count: u32,
        instance_count: u32,
    ) -> Result<(), ApplicationRenderError> {
        self.inner
            .execute_graphics_pass(
                desc,
                vertex,
                pixel,
                color,
                color_view,
                depth,
                bindings,
                clear_color,
                vertex_count,
                instance_count,
            )
            .map_err(ApplicationRenderError)
    }

    /// Execute and wait for one compute dispatch without exposing backend objects.
    ///
    /// # Errors
    /// Returns an error when bindings are invalid or GPU work fails.
    pub fn execute_compute_pass(
        &mut self,
        desc: ApplicationComputePipelineDesc,
        compute: &ApplicationCompiledShader,
        bindings: &[ApplicationPipelineDescriptorBinding],
        groups: [u32; 3],
    ) -> Result<(), ApplicationRenderError> {
        self.inner
            .execute_compute_pass(desc, compute, bindings, groups)
            .map_err(ApplicationRenderError)
    }

    /// Read one RGBA8 pixel from a completed GPU texture.
    ///
    /// # Errors
    /// Returns an error when the texture/coordinate is unsupported or readback fails.
    pub fn read_texture_pixel(
        &mut self,
        target: ApplicationGpuResourceHandle,
        x: u32,
        y: u32,
    ) -> Result<[u8; 4], ApplicationRenderError> {
        self.inner.read_texture_pixel(target, x, y).map_err(ApplicationRenderError)
    }

    /// Read a completed RGBA8 texture for image validation or comparison.
    /// This operation waits for GPU completion and should stay out of the
    /// ordinary frame path.
    ///
    /// # Errors
    /// Returns an error for an incompatible texture or failed GPU readback.
    pub fn read_texture_rgba8(
        &mut self,
        target: ApplicationGpuResourceHandle,
    ) -> Result<(u32, u32, Vec<u8>), ApplicationRenderError> {
        self.inner.read_texture_rgba8(target).map_err(ApplicationRenderError)
    }

    /// Submit queued passes in compiled dependency order and apply their GPU
    /// barriers through the renderer backend.
    ///
    /// # Errors
    /// Returns a graph compilation, barrier recording, or pass execution error.
    pub fn submit_render_queue(
        &mut self,
        queue: &mut ApplicationRenderQueue,
        jobs: &ApplicationJobSystem,
    ) -> Result<ApplicationCompiledRenderGraph, ApplicationRenderQueueError> {
        let (compiled, buffers) = queue.encode_parallel(jobs)?;
        let resources = self.inner.create_transient_resources(&compiled).map_err(|error| {
            ApplicationRenderQueueError::TransientResources(ApplicationRenderError(error))
        })?;
        let mut buffers = buffers;
        if let Err(error) = self.inner.resolve_transient_commands(&mut buffers, &resources) {
            let _ = self.inner.destroy_transient_resources(resources);
            return Err(ApplicationRenderQueueError::TransientResources(ApplicationRenderError(
                error,
            )));
        }
        let mut recorded =
            match self.inner.record_graph_barriers_parallel(&compiled, &resources, jobs) {
                Ok(recorded) => recorded,
                Err(error) => {
                    let _ = self.inner.destroy_transient_resources(resources);
                    return Err(ApplicationRenderQueueError::TransientResources(
                        ApplicationRenderError(error),
                    ));
                }
            };
        let mut commands = match self
            .inner
            .record_graph_commands_parallel(&compiled, &resources, &buffers, jobs)
        {
            Ok(commands) => commands,
            Err(error) => {
                let _ = self.inner.destroy_transient_resources(resources);
                return Err(ApplicationRenderQueueError::TransientResources(
                    ApplicationRenderError(error),
                ));
            }
        };
        if let Err(error) = self.inner.begin_graph_submission() {
            let _ = self.inner.destroy_transient_resources(resources);
            return Err(ApplicationRenderQueueError::TransientResources(ApplicationRenderError(
                error,
            )));
        }
        let mut submission_error = None;
        for (pass_index, pass) in compiled.passes().iter().enumerate() {
            let result =
                self.inner.execute_recorded_graph_barriers(&mut recorded, pass_index).and_then(
                    |()| self.inner.execute_recorded_graph_commands(&mut commands, pass.id.index()),
                );
            if let Err(error) = result {
                submission_error = Some(ApplicationRenderQueueError::TransientResources(
                    ApplicationRenderError(error),
                ));
                break;
            }
        }
        let retirement = self.inner.end_graph_submission().map_err(|error| {
            ApplicationRenderQueueError::TransientResources(ApplicationRenderError(error))
        });
        let cleanup = self.inner.destroy_transient_resources(resources).map_err(|error| {
            ApplicationRenderQueueError::TransientResources(ApplicationRenderError(error))
        });
        match (submission_error, retirement, cleanup) {
            (Some(error), _, _) | (None, Err(error), _) | (None, Ok(()), Err(error)) => Err(error),
            (None, Ok(()), Ok(())) => Ok(compiled),
        }
    }

    /// Evict pipeline states last used before `minimum_frame` after queue idle.
    ///
    /// # Errors
    /// Returns an error if GPU synchronization fails before native PSOs are released.
    pub fn trim_pipeline_cache(
        &mut self,
        minimum_frame: u64,
    ) -> Result<usize, ApplicationRenderError> {
        self.inner.trim_pipeline_cache(minimum_frame).map_err(ApplicationRenderError)
    }

    /// Write a byte range into a CPU-visible upload buffer.
    ///
    /// # Errors
    /// Returns an error for a stale/non-upload buffer, out-of-range write, or map failure.
    pub fn write_buffer(
        &mut self,
        handle: ApplicationGpuResourceHandle,
        offset: u64,
        bytes: &[u8],
    ) -> Result<(), ApplicationRenderError> {
        self.inner.write_buffer(handle, offset, bytes).map_err(ApplicationRenderError)
    }

    /// Stage bytes into a device-local GPU buffer.
    ///
    /// # Errors
    /// Returns an error for an invalid resource/range or failed copy submission.
    pub fn upload_buffer(
        &mut self,
        resource: ApplicationGpuResourceHandle,
        offset: u64,
        bytes: &[u8],
    ) -> Result<(), ApplicationRenderError> {
        self.inner.upload_buffer(resource, offset, bytes).map_err(ApplicationRenderError)
    }

    /// Upload one mip/array-layer texture subresource with the provided source row pitch.
    ///
    /// # Errors
    /// Returns an error for invalid subresources/source data or failed copy submission.
    pub fn upload_texture(
        &mut self,
        resource: ApplicationGpuResourceHandle,
        mip_level: u16,
        array_layer: u16,
        source_row_pitch: usize,
        bytes: &[u8],
    ) -> Result<(), ApplicationRenderError> {
        self.inner
            .upload_texture(resource, mip_level, array_layer, source_row_pitch, bytes)
            .map_err(ApplicationRenderError)
    }

    /// Destroy a resource after retiring all pending queue work.
    ///
    /// # Errors
    /// Returns an error for a stale resource or failed GPU synchronization.
    pub fn destroy_resource(
        &mut self,
        handle: ApplicationGpuResourceHandle,
    ) -> Result<(), ApplicationRenderError> {
        self.inner.destroy_resource(handle).map_err(ApplicationRenderError)
    }

    /// Retire GPU work before the platform destroys its native window.
    ///
    /// # Errors
    ///
    /// Returns an error if queue signaling or the GPU fence wait fails.
    pub fn wait_idle(&mut self) -> Result<(), ApplicationRenderError> {
        self.inner.wait_idle().map_err(ApplicationRenderError)
    }
}
