//! API-independent graphics infrastructure used by Nord applications.
//!
//! The first backend presents a color-cleared surface through Direct3D 12 on
//! Windows. It deliberately contains no game, voxel, scene, or asset policy.

use std::error::Error;
use std::fmt::{Display, Formatter};

mod allocator;
mod descriptors;
mod pipeline;
mod render_graph;
mod resources;
mod shader;

#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
mod dxc_reflection;

pub use allocator::{
    Allocation, AllocationClass, DeferredGpuAllocator, GpuAllocator, GpuAllocatorStats,
};
pub use descriptors::{
    DescriptorArena, DescriptorHandle, DescriptorKind, SamplerAddressMode, SamplerDesc,
    SamplerDescriptionError, SamplerFilter,
};
pub use pipeline::{
    ComputePipelineDesc, CullMode, FrontFace, GraphicsPipelineDesc, PipelineCache,
    PipelineCacheEntry, PipelineKey, PolygonFillMode, PrimitiveTopology, ShaderId, ShaderStage,
    VertexAttribute, VertexFormat,
};
pub use render_graph::{
    ColorAttachmentBinding, CompiledRenderGraph, CompiledRenderPass, DrawCall, GraphResourceRef,
    IndexBufferBinding, IndexFormat, RenderBarrier, RenderBarrierKind, RenderCommandBuffer,
    RenderCommandEncoder, RenderCommandEncodingError, RenderGraph, RenderGraphError,
    RenderPassDesc, RenderPassId, RenderQueue, RenderQueueError, ResourceAccess, ResourceUsage,
    ScissorRect, TransientAliasGroup, TransientTextureId, TransientTextureLifetime,
    VertexBufferBinding, Viewport,
};
pub use resources::{
    BufferDesc, BufferMemory, BufferUsage, GpuResourceHandle, RenderTargetDesc,
    RenderTargetResources, ResourceDescriptionError, ResourceKind, TextureDesc, TextureFormat,
    TextureUsage,
};
pub use shader::{
    CompiledShader, ShaderBindingKind, ShaderCompileError, ShaderCompileOptions, ShaderCompiler,
    ShaderDiagnostic, ShaderReflection, ShaderResourceBinding,
};

#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
mod d3d12;

/// Number of independently retired frames kept in flight.
pub const FRAMES_IN_FLIGHT: usize = 2;

/// GPU textures materialized for one compiled frame graph. Destroy this set
/// after its submissions retire; aliased resources share one allocator range.
pub struct TransientResources {
    handles: Vec<Option<GpuResourceHandle>>,
    groups: Vec<(Vec<GpuResourceHandle>, Allocation)>,
}

/// Per-pass D3D command lists recorded concurrently and consumed in graph
/// order by the renderer's direct queue.
pub struct RecordedGraphBarriers {
    #[cfg(target_os = "windows")]
    backend: d3d12::RecordedGraphBarriers,
}

/// Native per-pass graphics/compute command lists recorded on worker threads.
pub struct RecordedGraphCommands {
    #[cfg(target_os = "windows")]
    backend: d3d12::RecordedGraphCommands,
}

impl TransientResources {
    /// Resolve a graph-local transient ID to its opaque GPU handle.
    #[must_use]
    pub fn get(&self, id: TransientTextureId) -> Option<GpuResourceHandle> {
        self.handles.get(id.index() as usize).copied().flatten()
    }
}

/// GPU resource/descriptor services owned by one renderer instance.
///
/// Keeping these services in the renderer gives every graphics object a
/// single device lifetime while leaving the API backend-neutral.
pub struct RenderServices {
    allocator: GpuAllocator,
    deferred_allocator: DeferredGpuAllocator,
    descriptors: DescriptorArena,
    pipelines: PipelineCache,
}

impl Default for RenderServices {
    fn default() -> Self {
        Self {
            allocator: GpuAllocator::new(32 * 1024 * 1024)
                .with_class_block_size(AllocationClass::DeviceLocalBuffer, 16 * 1024 * 1024)
                .with_class_block_size(AllocationClass::DeviceLocalRenderTarget, 16 * 1024 * 1024)
                .with_class_block_size(AllocationClass::Upload, 8 * 1024 * 1024)
                .with_class_block_size(AllocationClass::Readback, 4 * 1024 * 1024),
            deferred_allocator: DeferredGpuAllocator::default(),
            descriptors: DescriptorArena::new(65_536, 2_048, 4_096, 2_048),
            pipelines: PipelineCache::default(),
        }
    }
}

impl RenderServices {
    pub(crate) fn recording_snapshot(&self) -> Self {
        Self {
            allocator: GpuAllocator::new(1),
            deferred_allocator: DeferredGpuAllocator::default(),
            descriptors: self.descriptors.clone(),
            pipelines: PipelineCache::default(),
        }
    }
    /// Device-memory suballocator bookkeeping. Backends provide device-reported
    /// size/alignment before requesting an allocation.
    pub fn allocator(&mut self) -> &mut GpuAllocator {
        &mut self.allocator
    }
    /// Fence-ordered range retirement paired with the suballocator.
    pub fn deferred_allocator(&mut self) -> &mut DeferredGpuAllocator {
        &mut self.deferred_allocator
    }
    /// Persistent descriptor-slot allocator; descriptor slots must be retired
    /// after the last fence that can reference them.
    pub fn descriptors(&mut self) -> &mut DescriptorArena {
        &mut self.descriptors
    }
    /// Description-keyed graphics pipeline cache.
    pub fn pipelines(&mut self) -> &mut PipelineCache {
        &mut self.pipelines
    }

    pub(crate) fn descriptor_is_live(&self, descriptor: DescriptorHandle) -> bool {
        self.descriptors.is_live(descriptor)
    }
}

/// Generic renderer initialization parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderConfig {
    width: u32,
    height: u32,
    vertical_sync: bool,
}

impl RenderConfig {
    /// Create a renderer configuration for a non-zero surface extent.
    #[must_use]
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height, vertical_sync: true }
    }

    /// Set whether presentation synchronizes to the display refresh.
    #[must_use]
    pub const fn with_vertical_sync(mut self, enabled: bool) -> Self {
        self.vertical_sync = enabled;
        self
    }

    /// Surface width in physical pixels.
    #[must_use]
    pub const fn width(self) -> u32 {
        self.width
    }

    /// Surface height in physical pixels.
    #[must_use]
    pub const fn height(self) -> u32 {
        self.height
    }

    /// Whether presentation waits for vertical blank.
    #[must_use]
    pub const fn vertical_sync(self) -> bool {
        self.vertical_sync
    }
}

/// RGBA clear color in linear floating-point channels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClearColor(pub [f32; 4]);

impl ClearColor {
    /// Construct an RGBA clear color.
    #[must_use]
    pub const fn rgba(red: f32, green: f32, blue: f32, alpha: f32) -> Self {
        Self([red, green, blue, alpha])
    }
}

/// One shader register bound to a live descriptor for a direct graphics pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PipelineDescriptorBinding {
    pub kind: ShaderBindingKind,
    pub register: u32,
    pub space: u32,
    pub descriptor: DescriptorHandle,
}

/// One shader register bound to a contiguous descriptor table range.
/// `descriptors` must match the reflected shader binding count and occupy
/// consecutive slots in the matching descriptor heap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipelineDescriptorTableBinding {
    pub kind: ShaderBindingKind,
    pub register: u32,
    pub space: u32,
    pub descriptors: Vec<DescriptorHandle>,
}

#[cfg(target_os = "windows")]
fn descriptor_tables_from_single(
    bindings: &[PipelineDescriptorBinding],
) -> Vec<PipelineDescriptorTableBinding> {
    bindings
        .iter()
        .map(|binding| PipelineDescriptorTableBinding {
            kind: binding.kind,
            register: binding.register,
            space: binding.space,
            descriptors: vec![binding.descriptor],
        })
        .collect()
}

/// Public backend identity, independent of game policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RenderBackend {
    Direct3D12,
    Unsupported,
}

/// DXGI memory segment queried by the device-backed allocator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuMemorySegment {
    /// Memory directly local to the graphics adapter.
    Local,
    /// System memory visible to the graphics adapter.
    NonLocal,
}

/// Operating-system budget and process usage for one GPU memory segment.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GpuMemoryBudget {
    pub budget_bytes: u64,
    pub current_usage_bytes: u64,
    pub available_for_reservation_bytes: u64,
    pub current_reservation_bytes: u64,
}

/// Renderer initialization, presentation, or resource-lifetime failure.
#[derive(Debug)]
#[non_exhaustive]
pub enum RenderError {
    InvalidExtent,
    InvalidClearColor,
    UnsupportedBackend,
    InvalidWindowHandle,
    MemoryBudgetExceeded { budget: u64, current_usage: u64, requested: u64 },
    Backend(String),
}

impl Display for RenderError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidExtent => {
                formatter.write_str("render surface dimensions must be non-zero")
            }
            Self::InvalidClearColor => formatter
                .write_str("clear color channels must be finite values between zero and one"),
            Self::UnsupportedBackend => {
                formatter.write_str("no graphics backend is available for this target")
            }
            Self::InvalidWindowHandle => {
                formatter.write_str("window handle is not supported by the graphics backend")
            }
            Self::MemoryBudgetExceeded { budget, current_usage, requested } => write!(
                formatter,
                "GPU memory budget exceeded: usage {current_usage} + request {requested} > budget {budget}"
            ),
            Self::Backend(message) => formatter.write_str(message),
        }
    }
}

impl Error for RenderError {}

/// A renderer bound to one application-owned native surface.
pub struct Renderer {
    services: RenderServices,
    #[cfg(target_os = "windows")]
    backend: d3d12::D3d12Renderer,
    #[cfg(not(target_os = "windows"))]
    _unsupported: (),
}

impl Renderer {
    /// Initialize the graphics API, device, queue, swapchain, synchronization,
    /// command buffers, and per-frame resources for a native window.
    ///
    /// # Errors
    ///
    /// Returns an error if dimensions, native surface, or graphics API setup
    /// cannot be used on the current machine.
    pub fn new(
        native_window: raw_window_handle::RawWindowHandle,
        config: RenderConfig,
    ) -> Result<Self, RenderError> {
        if config.width == 0 || config.height == 0 {
            return Err(RenderError::InvalidExtent);
        }
        #[cfg(target_os = "windows")]
        {
            d3d12::D3d12Renderer::new(native_window, config)
                .map(|backend| Self { backend, services: RenderServices::default() })
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = native_window;
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Access renderer-owned resource services. The renderer must outlive all
    /// handles created from these services.
    pub fn services(&mut self) -> &mut RenderServices {
        &mut self.services
    }

    /// Query the OS-reported memory budget for one adapter memory segment.
    ///
    /// # Errors
    /// Returns an error when the backend cannot query the adapter budget.
    pub fn memory_budget(&self, segment: GpuMemorySegment) -> Result<GpuMemoryBudget, RenderError> {
        #[cfg(target_os = "windows")]
        {
            self.backend.memory_budget(segment)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = segment;
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Wait for GPU work and return excess empty native heaps to the driver,
    /// retaining one empty heap per allocation class for reuse.
    ///
    /// # Errors
    /// Returns an error if the backend cannot wait for the queue to become idle.
    pub fn trim_memory_pools(&mut self) -> Result<u64, RenderError> {
        #[cfg(target_os = "windows")]
        {
            self.backend.trim_memory_pools(&mut self.services)
        }
        #[cfg(not(target_os = "windows"))]
        {
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Create a placed GPU buffer from the device-reported allocation size/alignment.
    ///
    /// # Errors
    /// Returns an error for unsupported usage combinations or backend allocation failure.
    pub fn create_buffer(&mut self, desc: BufferDesc) -> Result<GpuResourceHandle, RenderError> {
        #[cfg(target_os = "windows")]
        {
            self.backend.create_buffer(&mut self.services, desc)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = desc;
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Read a GPU-written range from a CPU-visible readback buffer.
    ///
    /// # Errors
    /// Returns an error for stale/non-readback handles, invalid ranges, or map failure.
    pub fn read_buffer(
        &mut self,
        handle: GpuResourceHandle,
        offset: u64,
        length: usize,
    ) -> Result<Vec<u8>, RenderError> {
        #[cfg(target_os = "windows")]
        {
            self.backend.read_buffer(handle, offset, length)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (handle, offset, length);
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Copy a device-local source range into a readback buffer and wait for completion.
    ///
    /// # Errors
    /// Returns an error for incompatible resources, invalid ranges/alignment, or queue failure.
    pub fn copy_buffer_to_readback(
        &mut self,
        source: GpuResourceHandle,
        source_offset: u64,
        destination: GpuResourceHandle,
        destination_offset: u64,
        length: u64,
    ) -> Result<(), RenderError> {
        #[cfg(target_os = "windows")]
        {
            self.backend.copy_buffer_to_readback(
                source,
                source_offset,
                destination,
                destination_offset,
                length,
            )
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (source, source_offset, destination, destination_offset, length);
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Create a placed 2D texture.
    ///
    /// # Errors
    /// Returns an error if the texture cannot be represented or allocated by the backend.
    pub fn create_texture(&mut self, desc: TextureDesc) -> Result<GpuResourceHandle, RenderError> {
        #[cfg(target_os = "windows")]
        {
            self.backend.create_texture(&mut self.services, desc)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = desc;
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Allocate every transient alias group in a compiled graph.
    ///
    /// # Errors
    /// Returns an error when the backend cannot reserve or create a group's GPU memory.
    pub fn create_transient_resources(
        &mut self,
        graph: &CompiledRenderGraph,
    ) -> Result<TransientResources, RenderError> {
        let count = graph
            .transient_lifetimes()
            .iter()
            .map(|life| life.resource.index() as usize + 1)
            .max()
            .unwrap_or(0);
        let mut resources = TransientResources { handles: vec![None; count], groups: Vec::new() };
        #[cfg(target_os = "windows")]
        {
            for group in graph.transient_alias_groups() {
                let mut descriptions = Vec::with_capacity(group.resources.len());
                for &id in &group.resources {
                    descriptions.push(graph.transient_texture(id).ok_or_else(|| {
                        RenderError::Backend(
                            "compiled graph lost a transient texture description".into(),
                        )
                    })?);
                }
                match self.backend.create_transient_alias_group(&mut self.services, &descriptions) {
                    Ok((handles, allocation)) => {
                        for (&id, &handle) in group.resources.iter().zip(&handles) {
                            resources.handles[id.index() as usize] = Some(handle);
                        }
                        resources.groups.push((handles, allocation));
                    }
                    Err(error) => {
                        let _ = self.destroy_transient_resources(resources);
                        return Err(error);
                    }
                }
            }
            Ok(resources)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = graph;
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Release frame graph resources after all queues referencing them retire.
    ///
    /// # Errors
    /// Returns an error if GPU work cannot be retired or resource bookkeeping is inconsistent.
    pub fn destroy_transient_resources(
        &mut self,
        resources: TransientResources,
    ) -> Result<(), RenderError> {
        #[cfg(target_os = "windows")]
        {
            for (handles, allocation) in resources.groups {
                self.backend.destroy_transient_alias_group(
                    &mut self.services,
                    &handles,
                    allocation,
                )?;
            }
            Ok(())
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = resources;
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Resolve transient UAV requests into live native descriptors before
    /// worker threads begin recording command lists.
    ///
    /// # Errors
    /// Returns an error for an invalid transient ID or descriptor creation failure.
    pub fn resolve_transient_commands(
        &mut self,
        buffers: &mut [RenderCommandBuffer],
        resources: &TransientResources,
    ) -> Result<(), RenderError> {
        for buffer in buffers {
            for command in &mut buffer.commands {
                let crate::render_graph::EncodedRenderCommand::ComputeTransientUav {
                    description,
                    compute,
                    bindings,
                    output,
                    register,
                    space,
                    groups,
                } = command
                else {
                    continue;
                };
                let output_handle = resources.get(*output).ok_or_else(|| {
                    RenderError::Backend(
                        "transient UAV request references an unused texture".into(),
                    )
                })?;
                let descriptor = self.create_unordered_access_view(output_handle)?;
                bindings.push(PipelineDescriptorTableBinding {
                    kind: ShaderBindingKind::StorageResource,
                    register: *register,
                    space: *space,
                    descriptors: vec![descriptor],
                });
                *command = crate::render_graph::EncodedRenderCommand::Compute {
                    description: *description,
                    compute: compute.clone(),
                    bindings: std::mem::take(bindings),
                    groups: *groups,
                };
            }
        }
        Ok(())
    }

    /// Apply graph barriers after resolving graph-local transient handles.
    ///
    /// # Errors
    /// Returns an error for stale resources, invalid usage declarations, or failed command submission.
    pub fn apply_graph_barriers_resolved(
        &mut self,
        barriers: &[RenderBarrier],
        resources: &TransientResources,
    ) -> Result<(), RenderError> {
        #[cfg(target_os = "windows")]
        {
            self.backend.apply_graph_barriers_resolved(barriers, &resources.handles)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (barriers, resources);
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Record all graph barrier command lists concurrently before submission.
    ///
    /// # Errors
    /// Returns an error when a graph resource is invalid or a command list cannot be recorded.
    pub fn record_graph_barriers_parallel(
        &self,
        graph: &CompiledRenderGraph,
        resources: &TransientResources,
    ) -> Result<RecordedGraphBarriers, RenderError> {
        #[cfg(target_os = "windows")]
        {
            self.backend
                .record_graph_barriers_parallel(graph, &resources.handles)
                .map(|backend| RecordedGraphBarriers { backend })
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (graph, resources);
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Submit one previously recorded barrier list on the direct queue.
    ///
    /// # Errors
    /// Returns an error if the pass index is invalid or queue submission fails.
    pub fn execute_recorded_graph_barriers(
        &mut self,
        recorded: &mut RecordedGraphBarriers,
        pass_index: usize,
    ) -> Result<(), RenderError> {
        #[cfg(target_os = "windows")]
        {
            self.backend.execute_recorded_graph_barriers(&mut recorded.backend, pass_index)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (recorded, pass_index);
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Record pass command streams concurrently on private D3D12 allocators.
    ///
    /// # Errors
    /// Returns an error when command validation or native command recording fails.
    pub fn record_graph_commands_parallel(
        &self,
        graph: &CompiledRenderGraph,
        resources: &TransientResources,
        buffers: &[RenderCommandBuffer],
    ) -> Result<RecordedGraphCommands, RenderError> {
        #[cfg(target_os = "windows")]
        {
            self.backend
                .record_graph_commands_parallel(&self.services, graph, &resources.handles, buffers)
                .map(|backend| RecordedGraphCommands { backend })
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = buffers;
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Submit one pass's already-recorded graphics/compute command lists.
    ///
    /// # Errors
    /// Returns an error if native queue submission fails.
    pub fn execute_recorded_graph_commands(
        &mut self,
        recorded: &mut RecordedGraphCommands,
        pass_index: usize,
    ) -> Result<(), RenderError> {
        #[cfg(target_os = "windows")]
        {
            self.backend.execute_recorded_graph_commands(&mut recorded.backend, pass_index)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (recorded, pass_index);
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Start a frame graph's batched direct-queue submission scope.
    ///
    /// # Errors
    /// Returns an error when another graph scope is active.
    pub fn begin_graph_submission(&mut self) -> Result<(), RenderError> {
        #[cfg(target_os = "windows")]
        {
            self.backend.begin_graph_submission()
        }
        #[cfg(not(target_os = "windows"))]
        {
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Fence-retire all graph submissions and release their command allocators.
    ///
    /// # Errors
    /// Returns an error if the queue fence cannot be signaled or waited.
    pub fn end_graph_submission(&mut self) -> Result<(), RenderError> {
        #[cfg(target_os = "windows")]
        {
            self.backend.end_graph_submission()
        }
        #[cfg(not(target_os = "windows"))]
        {
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Create color and/or depth textures for an offscreen render target.
    ///
    /// # Errors
    /// Returns an error for incompatible attachment descriptions or allocation failure.
    pub fn create_render_target(
        &mut self,
        desc: RenderTargetDesc,
    ) -> Result<RenderTargetResources, RenderError> {
        #[cfg(target_os = "windows")]
        {
            self.backend.create_render_target(&mut self.services, desc)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = desc;
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Create a native SRV in the renderer's stable shader-visible heap.
    ///
    /// # Errors
    /// Returns an error if the resource is stale, incompatible, or descriptor capacity is exhausted.
    pub fn create_texture_srv(
        &mut self,
        resource: GpuResourceHandle,
    ) -> Result<DescriptorHandle, RenderError> {
        #[cfg(target_os = "windows")]
        {
            self.backend.create_texture_srv(&mut self.services, resource)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = resource;
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Create a contiguous SRV descriptor range for one reflected HLSL array.
    ///
    /// # Errors
    /// Returns an error for an empty/incompatible resource list or if no
    /// contiguous descriptor range is available.
    pub fn create_texture_srv_array(
        &mut self,
        resources: &[GpuResourceHandle],
    ) -> Result<Vec<DescriptorHandle>, RenderError> {
        #[cfg(target_os = "windows")]
        {
            self.backend.create_texture_srv_array(&mut self.services, resources)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = resources;
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Create a CBV for a 256-byte-aligned constant buffer.
    ///
    /// # Errors
    /// Returns an error if the handle or buffer description is incompatible with a CBV.
    pub fn create_constant_buffer_view(
        &mut self,
        resource: GpuResourceHandle,
    ) -> Result<DescriptorHandle, RenderError> {
        #[cfg(target_os = "windows")]
        {
            self.backend.create_constant_buffer_view(&mut self.services, resource)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = resource;
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Create a contiguous CBV descriptor range for one reflected CBV array.
    ///
    /// # Errors
    /// Returns an error if any buffer is incompatible or no contiguous range is available.
    pub fn create_constant_buffer_view_array(
        &mut self,
        resources: &[GpuResourceHandle],
    ) -> Result<Vec<DescriptorHandle>, RenderError> {
        #[cfg(target_os = "windows")]
        {
            self.backend.create_constant_buffer_view_array(&mut self.services, resources)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = resources;
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Create a UAV for a buffer or texture created with storage usage.
    ///
    /// # Errors
    /// Returns an error if storage usage is absent or descriptor capacity is exhausted.
    pub fn create_unordered_access_view(
        &mut self,
        resource: GpuResourceHandle,
    ) -> Result<DescriptorHandle, RenderError> {
        #[cfg(target_os = "windows")]
        {
            self.backend.create_unordered_access_view(&mut self.services, resource)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = resource;
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Create a contiguous UAV descriptor range for one reflected UAV array.
    ///
    /// # Errors
    /// Returns an error if any resource lacks storage usage or no contiguous range is available.
    pub fn create_unordered_access_view_array(
        &mut self,
        resources: &[GpuResourceHandle],
    ) -> Result<Vec<DescriptorHandle>, RenderError> {
        #[cfg(target_os = "windows")]
        {
            self.backend.create_unordered_access_view_array(&mut self.services, resources)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = resources;
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Create a native RTV for a color render-target texture.
    ///
    /// # Errors
    /// Returns an error if the resource is not a live color render target or the heap is full.
    pub fn create_render_target_view(
        &mut self,
        resource: GpuResourceHandle,
    ) -> Result<DescriptorHandle, RenderError> {
        #[cfg(target_os = "windows")]
        {
            self.backend.create_render_target_view(&mut self.services, resource)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = resource;
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Create a native DSV for a depth-stencil texture.
    ///
    /// # Errors
    /// Returns an error if the resource is not a live depth target or the heap is full.
    pub fn create_depth_stencil_view(
        &mut self,
        resource: GpuResourceHandle,
    ) -> Result<DescriptorHandle, RenderError> {
        #[cfg(target_os = "windows")]
        {
            self.backend.create_depth_stencil_view(&mut self.services, resource)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = resource;
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Create a native sampler in the renderer's separate shader-visible heap.
    ///
    /// # Errors
    /// Returns an error for invalid sampler settings or descriptor exhaustion.
    pub fn create_sampler(
        &mut self,
        sampler: SamplerDesc,
    ) -> Result<DescriptorHandle, RenderError> {
        #[cfg(target_os = "windows")]
        {
            self.backend.create_sampler(&mut self.services, sampler)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = sampler;
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Create contiguous descriptors for a reflected sampler array.
    ///
    /// # Errors
    /// Returns an error for an empty array, invalid sampler settings, or lack
    /// of contiguous heap capacity.
    pub fn create_sampler_array(
        &mut self,
        samplers: &[SamplerDesc],
    ) -> Result<Vec<DescriptorHandle>, RenderError> {
        #[cfg(target_os = "windows")]
        {
            self.backend.create_sampler_array(&mut self.services, samplers)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = samplers;
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Release a descriptor only after GPU work that may reference it is idle.
    ///
    /// # Errors
    /// Returns an error if the handle is stale or waiting for GPU work fails.
    pub fn destroy_descriptor(&mut self, descriptor: DescriptorHandle) -> Result<(), RenderError> {
        #[cfg(target_os = "windows")]
        {
            self.backend.destroy_descriptor(&mut self.services, descriptor)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = descriptor;
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Create or retrieve a native D3D12 graphics pipeline state and root
    /// signature derived from the compiled stages' reflected resource bindings.
    ///
    /// # Errors
    /// Returns an error for invalid pipeline state, mismatched shader stages, or native PSO creation failure.
    pub fn create_graphics_pipeline(
        &mut self,
        desc: &GraphicsPipelineDesc,
        vertex: &CompiledShader,
        pixel: &CompiledShader,
        frame: u64,
    ) -> Result<PipelineCacheEntry, RenderError> {
        #[cfg(target_os = "windows")]
        {
            self.backend.create_graphics_pipeline(&mut self.services, desc, vertex, pixel, frame)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (desc, vertex, pixel, frame);
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Create or reuse a native compute root signature and pipeline state.
    ///
    /// # Errors
    /// Returns an error when the shader stage/identity is incompatible or native creation fails.
    pub fn create_compute_pipeline(
        &mut self,
        desc: ComputePipelineDesc,
        compute: &CompiledShader,
        frame: u64,
    ) -> Result<PipelineCacheEntry, RenderError> {
        #[cfg(target_os = "windows")]
        {
            self.backend.create_compute_pipeline(&mut self.services, desc, compute, frame)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (desc, compute, frame);
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Execute one synchronous offscreen graphics pass using a previously
    /// created native PSO, reflected shader bindings and opaque RTV/DSV handles.
    /// Intended for bootstrap validation and simple immediate work before the
    /// render queue is available; it waits for completion before returning.
    ///
    /// # Errors
    /// Returns an error for invalid attachments/bindings, incompatible shaders,
    /// command recording failure, or GPU execution failure.
    #[allow(clippy::too_many_arguments)]
    pub fn execute_graphics_pass(
        &mut self,
        desc: &GraphicsPipelineDesc,
        vertex: &CompiledShader,
        pixel: &CompiledShader,
        color: GpuResourceHandle,
        color_view: DescriptorHandle,
        depth: Option<(GpuResourceHandle, DescriptorHandle)>,
        bindings: &[PipelineDescriptorBinding],
        clear_color: ClearColor,
        vertex_count: u32,
        instance_count: u32,
    ) -> Result<(), RenderError> {
        if clear_color.0.iter().any(|c| !c.is_finite() || !(0.0..=1.0).contains(c)) {
            return Err(RenderError::InvalidClearColor);
        }
        if vertex_count == 0 || instance_count == 0 {
            return Err(RenderError::Backend("draw counts must be non-zero".into()));
        }
        #[cfg(target_os = "windows")]
        {
            let bindings = descriptor_tables_from_single(bindings);
            self.backend.execute_graphics_pass(
                &mut self.services,
                desc,
                vertex,
                pixel,
                color,
                color_view,
                depth,
                &bindings,
                clear_color,
                vertex_count,
                instance_count,
            )
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (
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
            );
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Execute a synchronous compute dispatch using a reflected native PSO.
    /// This immediate path is useful for bootstrap validation before the job
    /// and render scheduling layers own asynchronous submissions.
    ///
    /// # Errors
    /// Returns an error for invalid dispatch dimensions/bindings, shader/PSO
    /// mismatch, command recording failure, or GPU execution failure.
    pub fn execute_compute_pass(
        &mut self,
        desc: ComputePipelineDesc,
        compute: &CompiledShader,
        bindings: &[PipelineDescriptorBinding],
        groups: [u32; 3],
    ) -> Result<(), RenderError> {
        if groups.contains(&0) {
            return Err(RenderError::Backend("compute group counts must be non-zero".into()));
        }
        #[cfg(target_os = "windows")]
        {
            let bindings = descriptor_tables_from_single(bindings);
            self.backend.execute_compute_pass(&mut self.services, desc, compute, &bindings, groups)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (desc, compute, bindings, groups);
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Copy one pixel from an RGBA8 single-sample render target into a
    /// temporary readback resource and return the four UNORM channels.
    ///
    /// # Errors
    /// Returns an error for unsupported formats, invalid coordinates, or a
    /// failed GPU copy/map operation.
    pub fn read_texture_pixel(
        &mut self,
        target: GpuResourceHandle,
        x: u32,
        y: u32,
    ) -> Result<[u8; 4], RenderError> {
        #[cfg(target_os = "windows")]
        {
            self.backend.read_texture_pixel(target, x, y)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (target, x, y);
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Apply a compiled pass's transition and UAV ordering barriers.
    ///
    /// # Errors
    /// Returns an error for stale/incompatible resources or command submission failure.
    pub fn apply_graph_barriers(&mut self, barriers: &[RenderBarrier]) -> Result<(), RenderError> {
        #[cfg(target_os = "windows")]
        {
            self.backend.apply_graph_barriers(barriers)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = barriers;
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Evict pipeline states last used before `minimum_frame` after queue idle.
    ///
    /// # Errors
    /// Returns an error if GPU synchronization fails before native PSOs are released.
    pub fn trim_pipeline_cache(&mut self, minimum_frame: u64) -> Result<usize, RenderError> {
        #[cfg(target_os = "windows")]
        {
            self.backend.trim_pipeline_cache(&mut self.services, minimum_frame)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = minimum_frame;
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Write to a CPU-visible upload buffer. GPU/CPU synchronization is the
    /// caller's responsibility for ranges that may be read by in-flight work.
    ///
    /// # Errors
    /// Returns an error for stale/non-upload resources, out-of-range writes, or map failure.
    pub fn write_buffer(
        &mut self,
        handle: GpuResourceHandle,
        offset: u64,
        bytes: &[u8],
    ) -> Result<(), RenderError> {
        #[cfg(target_os = "windows")]
        {
            self.backend.write_buffer(handle, offset, bytes)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (handle, offset, bytes);
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Stage bytes into a device-local buffer and wait for the copy fence before
    /// returning. Hot-path streaming should use the renderer's frame upload arena.
    ///
    /// # Errors
    /// Returns an error for stale/non-buffer handles, invalid ranges, or copy submission failure.
    pub fn upload_buffer(
        &mut self,
        resource: GpuResourceHandle,
        offset: u64,
        bytes: &[u8],
    ) -> Result<(), RenderError> {
        #[cfg(target_os = "windows")]
        {
            self.backend.upload_buffer(resource, offset, bytes)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (resource, offset, bytes);
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Upload one uncompressed 2D texture subresource using D3D12's copyable
    /// footprint row pitch. This bootstrap path waits for completion.
    ///
    /// # Errors
    /// Returns an error for invalid subresources/source pitch or copy submission failure.
    pub fn upload_texture(
        &mut self,
        resource: GpuResourceHandle,
        mip_level: u16,
        array_layer: u16,
        source_row_pitch: usize,
        bytes: &[u8],
    ) -> Result<(), RenderError> {
        #[cfg(target_os = "windows")]
        {
            self.backend.upload_texture(resource, mip_level, array_layer, source_row_pitch, bytes)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (resource, mip_level, array_layer, source_row_pitch, bytes);
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Destroy a GPU resource and return its allocation after all queue work
    /// completes. The conservative initial implementation waits for the queue.
    ///
    /// # Errors
    /// Returns an error if the handle is stale or queue synchronization fails.
    pub fn destroy_resource(&mut self, handle: GpuResourceHandle) -> Result<(), RenderError> {
        #[cfg(target_os = "windows")]
        {
            self.backend.destroy_resource(&mut self.services, handle)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = handle;
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Graphics backend selected for this renderer.
    #[must_use]
    pub const fn backend(&self) -> RenderBackend {
        #[cfg(target_os = "windows")]
        {
            RenderBackend::Direct3D12
        }
        #[cfg(not(target_os = "windows"))]
        {
            RenderBackend::Unsupported
        }
    }

    /// Begin, clear, submit, and present one frame using the owned resources.
    ///
    /// # Errors
    ///
    /// Returns an error when command recording, synchronization, or
    /// presentation fails.
    pub fn render_frame(&mut self, clear_color: ClearColor) -> Result<(), RenderError> {
        if clear_color
            .0
            .iter()
            .any(|channel| !channel.is_finite() || !(0.0..=1.0).contains(channel))
        {
            return Err(RenderError::InvalidClearColor);
        }
        #[cfg(target_os = "windows")]
        {
            self.backend.render_frame(clear_color)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = clear_color;
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Wait until the presentation queue permits the next frame to begin.
    /// Call this before sampling frame input or advancing the simulation.
    ///
    /// # Errors
    ///
    /// Returns an error if DXGI fails to signal frame readiness.
    pub fn wait_for_frame(&self) -> Result<(), RenderError> {
        #[cfg(target_os = "windows")]
        {
            self.backend.wait_for_frame()
        }
        #[cfg(not(target_os = "windows"))]
        {
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Recreate surface-sized resources after a native resize.
    ///
    /// A zero extent is treated as a minimized surface and skipped.
    ///
    /// # Errors
    ///
    /// Returns an error if the swapchain cannot be recreated.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), RenderError> {
        #[cfg(target_os = "windows")]
        {
            self.backend.resize(width, height)
        }
        #[cfg(not(target_os = "windows"))]
        {
            Err(RenderError::UnsupportedBackend)
        }
    }

    /// Whether the native surface is currently minimized or has zero extent.
    #[must_use]
    pub const fn surface_suspended(&self) -> bool {
        #[cfg(target_os = "windows")]
        {
            self.backend.is_suspended()
        }
        #[cfg(not(target_os = "windows"))]
        {
            true
        }
    }

    /// Wait for outstanding GPU work before releasing the native surface.
    ///
    /// # Errors
    ///
    /// Returns an error if queue signaling or the GPU fence wait fails.
    pub fn wait_idle(&mut self) -> Result<(), RenderError> {
        #[cfg(target_os = "windows")]
        {
            self.backend.wait_idle()
        }
        #[cfg(not(target_os = "windows"))]
        {
            Ok(())
        }
    }
}
