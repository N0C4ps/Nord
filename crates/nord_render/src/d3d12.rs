use crate::render_graph::{EncodedGraphicsDraw, EncodedGraphicsPass, IndexFormat};
use crate::{
    Allocation, AllocationClass, BlendMode, BufferDesc, BufferMemory, BufferUsage, ClearColor,
    CompiledShader, ComputePipelineDesc, CullMode, DescriptorHandle, DescriptorKind, DrawCall,
    FRAMES_IN_FLIGHT, FrontFace, GpuMemoryBudget, GpuMemorySegment, GpuResourceHandle,
    GraphResourceRef, GraphicsPipelineDesc, PipelineCacheEntry, PipelineDescriptorTableBinding,
    PipelineKey, PolygonFillMode, PrimitiveTopology, RenderBarrier, RenderBarrierKind,
    RenderConfig, RenderError, RenderServices, RenderTargetDesc, ResourceKind, ResourceUsage,
    SamplerAddressMode, SamplerDesc, SamplerFilter, ShaderBindingKind, ShaderStage, TextureDesc,
    TextureFormat, TextureUsage, VertexFormat,
};
use raw_window_handle::RawWindowHandle;
use std::collections::{HashMap, HashSet};
use std::ffi::{CString, c_void};
use std::mem::ManuallyDrop;
use std::num::NonZeroU32;
use std::ptr::NonNull;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND};
use windows::Win32::Graphics::Direct3D::D3D_FEATURE_LEVEL_11_0;
use windows::Win32::Graphics::Direct3D12::{
    D3D_ROOT_SIGNATURE_VERSION_1, D3D12_BLEND_DESC, D3D12_BLEND_INV_DEST_COLOR,
    D3D12_BLEND_INV_SRC_ALPHA, D3D12_BLEND_ONE, D3D12_BLEND_OP_ADD, D3D12_BLEND_OP_SUBTRACT,
    D3D12_BLEND_SRC_ALPHA, D3D12_BLEND_ZERO, D3D12_BUFFER_SRV, D3D12_BUFFER_SRV_FLAG_RAW,
    D3D12_BUFFER_UAV, D3D12_BUFFER_UAV_FLAG_RAW, D3D12_COMMAND_LIST_TYPE_DIRECT,
    D3D12_COMMAND_QUEUE_DESC, D3D12_COMMAND_SIGNATURE_DESC, D3D12_COMPARISON_FUNC_ALWAYS,
    D3D12_COMPUTE_PIPELINE_STATE_DESC, D3D12_CONSERVATIVE_RASTERIZATION_MODE_OFF,
    D3D12_CONSTANT_BUFFER_VIEW_DESC, D3D12_CPU_DESCRIPTOR_HANDLE, D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
    D3D12_CULL_MODE_BACK, D3D12_CULL_MODE_FRONT, D3D12_CULL_MODE_NONE, D3D12_DEFAULT_DEPTH_BIAS,
    D3D12_DEFAULT_DEPTH_BIAS_CLAMP, D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
    D3D12_DEFAULT_SLOPE_SCALED_DEPTH_BIAS, D3D12_DEPTH_STENCIL_DESC, D3D12_DEPTH_STENCILOP_DESC,
    D3D12_DEPTH_WRITE_MASK_ALL, D3D12_DEPTH_WRITE_MASK_ZERO, D3D12_DESCRIPTOR_HEAP_DESC,
    D3D12_DESCRIPTOR_HEAP_FLAG_NONE, D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,
    D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV, D3D12_DESCRIPTOR_HEAP_TYPE_DSV,
    D3D12_DESCRIPTOR_HEAP_TYPE_RTV, D3D12_DESCRIPTOR_HEAP_TYPE_SAMPLER, D3D12_DESCRIPTOR_RANGE,
    D3D12_DESCRIPTOR_RANGE_OFFSET_APPEND, D3D12_DESCRIPTOR_RANGE_TYPE_CBV,
    D3D12_DESCRIPTOR_RANGE_TYPE_SAMPLER, D3D12_DESCRIPTOR_RANGE_TYPE_SRV,
    D3D12_DESCRIPTOR_RANGE_TYPE_UAV, D3D12_FENCE_FLAG_NONE, D3D12_FILL_MODE_SOLID,
    D3D12_FILL_MODE_WIREFRAME, D3D12_FILTER_ANISOTROPIC, D3D12_FILTER_MIN_MAG_MIP_LINEAR,
    D3D12_FILTER_MIN_MAG_MIP_POINT, D3D12_GRAPHICS_PIPELINE_STATE_DESC, D3D12_HEAP_DESC,
    D3D12_HEAP_FLAG_ALLOW_ONLY_BUFFERS, D3D12_HEAP_FLAG_ALLOW_ONLY_NON_RT_DS_TEXTURES,
    D3D12_HEAP_FLAG_ALLOW_ONLY_RT_DS_TEXTURES, D3D12_HEAP_FLAG_NONE, D3D12_HEAP_PROPERTIES,
    D3D12_HEAP_TYPE_DEFAULT, D3D12_HEAP_TYPE_READBACK, D3D12_HEAP_TYPE_UPLOAD,
    D3D12_INDEX_BUFFER_VIEW, D3D12_INDIRECT_ARGUMENT_DESC,
    D3D12_INDIRECT_ARGUMENT_TYPE_DRAW_INDEXED, D3D12_INPUT_CLASSIFICATION_PER_INSTANCE_DATA,
    D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA, D3D12_INPUT_ELEMENT_DESC, D3D12_INPUT_LAYOUT_DESC,
    D3D12_LOGIC_OP_NOOP, D3D12_MEMORY_POOL_UNKNOWN, D3D12_PLACED_SUBRESOURCE_FOOTPRINT,
    D3D12_PRIMITIVE_TOPOLOGY_TYPE_LINE, D3D12_PRIMITIVE_TOPOLOGY_TYPE_POINT,
    D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE, D3D12_RANGE, D3D12_RASTERIZER_DESC,
    D3D12_RENDER_TARGET_BLEND_DESC, D3D12_RESOURCE_ALIASING_BARRIER, D3D12_RESOURCE_BARRIER,
    D3D12_RESOURCE_BARRIER_0, D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
    D3D12_RESOURCE_BARRIER_FLAG_NONE, D3D12_RESOURCE_BARRIER_TYPE_ALIASING,
    D3D12_RESOURCE_BARRIER_TYPE_TRANSITION, D3D12_RESOURCE_BARRIER_TYPE_UAV, D3D12_RESOURCE_DESC,
    D3D12_RESOURCE_DIMENSION_BUFFER, D3D12_RESOURCE_DIMENSION_TEXTURE2D,
    D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL, D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET,
    D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS, D3D12_RESOURCE_FLAG_NONE,
    D3D12_RESOURCE_STATE_COMMON, D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_COPY_SOURCE,
    D3D12_RESOURCE_STATE_GENERIC_READ, D3D12_RESOURCE_STATE_INDEX_BUFFER,
    D3D12_RESOURCE_STATE_INDIRECT_ARGUMENT, D3D12_RESOURCE_STATE_PRESENT,
    D3D12_RESOURCE_STATE_RENDER_TARGET, D3D12_RESOURCE_STATE_VERTEX_AND_CONSTANT_BUFFER,
    D3D12_RESOURCE_STATES, D3D12_RESOURCE_TRANSITION_BARRIER, D3D12_RESOURCE_UAV_BARRIER,
    D3D12_ROOT_DESCRIPTOR_TABLE, D3D12_ROOT_PARAMETER, D3D12_ROOT_PARAMETER_0,
    D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE, D3D12_ROOT_SIGNATURE_DESC,
    D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT, D3D12_SAMPLER_DESC,
    D3D12_SHADER_BYTECODE, D3D12_SHADER_RESOURCE_VIEW_DESC, D3D12_SHADER_RESOURCE_VIEW_DESC_0,
    D3D12_SHADER_VISIBILITY_ALL, D3D12_SHADER_VISIBILITY_PIXEL, D3D12_SHADER_VISIBILITY_VERTEX,
    D3D12_SRV_DIMENSION_BUFFER, D3D12_STENCIL_OP_KEEP, D3D12_TEX2D_ARRAY_UAV, D3D12_TEX2D_UAV,
    D3D12_TEXTURE_ADDRESS_MODE_CLAMP, D3D12_TEXTURE_ADDRESS_MODE_MIRROR,
    D3D12_TEXTURE_ADDRESS_MODE_WRAP, D3D12_TEXTURE_COPY_LOCATION, D3D12_TEXTURE_COPY_LOCATION_0,
    D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT, D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
    D3D12_TEXTURE_LAYOUT_ROW_MAJOR, D3D12_TEXTURE_LAYOUT_UNKNOWN, D3D12_UAV_DIMENSION_BUFFER,
    D3D12_UAV_DIMENSION_TEXTURE2D, D3D12_UAV_DIMENSION_TEXTURE2DARRAY,
    D3D12_UNORDERED_ACCESS_VIEW_DESC, D3D12_UNORDERED_ACCESS_VIEW_DESC_0, D3D12_VERTEX_BUFFER_VIEW,
    D3D12CreateDevice, D3D12SerializeRootSignature, ID3D12CommandAllocator, ID3D12CommandQueue,
    ID3D12CommandSignature, ID3D12DescriptorHeap, ID3D12Device, ID3D12Fence,
    ID3D12GraphicsCommandList, ID3D12Heap, ID3D12PipelineState, ID3D12Resource,
    ID3D12RootSignature,
};
#[cfg(debug_assertions)]
use windows::Win32::Graphics::Direct3D12::{D3D12GetDebugInterface, ID3D12Debug};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_IGNORE, DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_D24_UNORM_S8_UINT,
    DXGI_FORMAT_D32_FLOAT, DXGI_FORMAT_R8G8B8A8_UINT, DXGI_FORMAT_R8G8B8A8_UNORM,
    DXGI_FORMAT_R8G8B8A8_UNORM_SRGB, DXGI_FORMAT_R16_FLOAT, DXGI_FORMAT_R16_UINT,
    DXGI_FORMAT_R16G16_FLOAT, DXGI_FORMAT_R16G16_UINT, DXGI_FORMAT_R16G16B16A16_FLOAT,
    DXGI_FORMAT_R32_FLOAT, DXGI_FORMAT_R32_TYPELESS, DXGI_FORMAT_R32_UINT,
    DXGI_FORMAT_R32G32_FLOAT, DXGI_FORMAT_R32G32B32_FLOAT, DXGI_FORMAT_R32G32B32A32_FLOAT,
    DXGI_FORMAT_UNKNOWN, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory2, DXGI_ADAPTER_FLAG_SOFTWARE, DXGI_CREATE_FACTORY_FLAGS,
    DXGI_MEMORY_SEGMENT_GROUP_LOCAL, DXGI_MEMORY_SEGMENT_GROUP_NON_LOCAL, DXGI_PRESENT,
    DXGI_QUERY_VIDEO_MEMORY_INFO, DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1,
    DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT, DXGI_SWAP_EFFECT_FLIP_DISCARD,
    DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGIAdapter1, IDXGIAdapter3, IDXGIFactory4, IDXGISwapChain2,
    IDXGISwapChain3,
};
use windows::Win32::System::Threading::{CreateEventW, INFINITE, WaitForSingleObject};
use windows::core::Interface;

const BACK_BUFFER_COUNT: u32 = 3;
const RENDER_TARGET_DESCRIPTOR_CAPACITY: u32 = 4_096;
const WAIT_OBJECT_0: u32 = 0;

struct OwnedHandle(HANDLE, bool);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: this handle has one owner and is closed exactly once.
        unsafe {
            if self.1 {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

struct FrameResources {
    allocator: ID3D12CommandAllocator,
    fence_value: u64,
    upload: MappedUploadArena,
}

#[derive(Clone)]
struct NativeHeapBlock {
    class: AllocationClass,
    heap: ID3D12Heap,
}

#[derive(Clone)]
struct NativeResourceSlot {
    generation: NonZeroU32,
    kind: ResourceKind,
    allocation: Option<Allocation>,
    resource: Option<ID3D12Resource>,
    buffer_memory: Option<BufferMemory>,
    shader_resource: bool,
    buffer_usage: Option<BufferUsage>,
    byte_size: u64,
    desc: D3D12_RESOURCE_DESC,
    descriptors: Vec<(DescriptorHandle, NativeViewKind)>,
    state: D3D12_RESOURCE_STATES,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeViewKind {
    ShaderResource,
    ConstantBuffer,
    UnorderedAccess,
    RenderTarget,
    DepthStencil,
}

#[derive(Clone)]
struct NativePipeline {
    root_signature: ID3D12RootSignature,
    pipeline_state: ID3D12PipelineState,
}

enum PendingGraphBarrier {
    Transition(ID3D12Resource, D3D12_RESOURCE_STATES, D3D12_RESOURCE_STATES),
    Uav(ID3D12Resource),
    Aliasing(Option<ID3D12Resource>, ID3D12Resource),
}

pub(crate) struct RecordedBarrierPass {
    _allocator: ID3D12CommandAllocator,
    list: ID3D12GraphicsCommandList,
    state_updates: Vec<(usize, D3D12_RESOURCE_STATES)>,
}

pub(crate) struct RecordedGraphBarriers {
    passes: Vec<Option<RecordedBarrierPass>>,
}

pub(crate) struct RecordedGraphCommands {
    passes: Vec<Vec<(ID3D12CommandAllocator, ID3D12GraphicsCommandList)>>,
}

struct SendRecordingSnapshot(D3d12Renderer);

// SAFETY: only `recording_snapshot` values enter this wrapper. They contain no
// owned OS handles or frame upload mappings, have an empty frame vector, and
// own independent COM references plus resource/PSO snapshots for one worker.
unsafe impl Send for SendRecordingSnapshot {}

impl SendRecordingSnapshot {
    fn record_commands(
        mut self,
        commands: &[crate::render_graph::EncodedRenderCommand],
        mut services: RenderServices,
    ) -> Result<Vec<(ID3D12CommandAllocator, ID3D12GraphicsCommandList)>, RenderError> {
        for command in commands {
            match command {
                crate::render_graph::EncodedRenderCommand::Graphics(pass) => {
                    self.0.execute_graphics_pass_batch(&mut services, pass)?;
                }
                crate::render_graph::EncodedRenderCommand::Compute {
                    description,
                    compute,
                    bindings,
                    groups,
                } => self.0.execute_compute_pass(
                    &mut services,
                    *description,
                    compute,
                    bindings,
                    *groups,
                )?,
                crate::render_graph::EncodedRenderCommand::ComputeTransientUav { .. } => {
                    return Err(RenderError::Backend(
                        "transient UAV command was not resolved before recording".into(),
                    ));
                }
            }
        }
        Ok(std::mem::take(&mut self.0.inflight_pass_lists))
    }
}

type RootBindingKey = (ShaderBindingKind, u32, u32, u32);
type RootSignatureParts =
    (Vec<D3D12_DESCRIPTOR_RANGE>, Vec<D3D12_ROOT_PARAMETER>, Vec<RootBindingKey>);

/// Lazily consumed, persistently mapped upload heap owned by one frame slot.
/// Its linear pointer is reset only after the frame's GPU fence retires.
struct MappedUploadArena {
    resource: ID3D12Resource,
    mapped: NonNull<u8>,
    capacity: usize,
    cursor: usize,
}

impl MappedUploadArena {
    fn new(device: &ID3D12Device, capacity: usize) -> Result<Self, RenderError> {
        // SAFETY: valid descriptors create a buffer resource; Map returns a
        // pointer kept alive by the owned COM resource until Drop calls Unmap.
        unsafe {
            let heap = D3D12_HEAP_PROPERTIES {
                Type: D3D12_HEAP_TYPE_UPLOAD,
                CPUPageProperty: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
                MemoryPoolPreference: D3D12_MEMORY_POOL_UNKNOWN,
                CreationNodeMask: 1,
                VisibleNodeMask: 1,
            };
            let desc = D3D12_RESOURCE_DESC {
                Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
                Alignment: 0,
                Width: capacity as u64,
                Height: 1,
                DepthOrArraySize: 1,
                MipLevels: 1,
                Format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_UNKNOWN,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
                Flags: D3D12_RESOURCE_FLAG_NONE,
            };
            let mut resource = None;
            device
                .CreateCommittedResource::<ID3D12Resource>(
                    &raw const heap,
                    D3D12_HEAP_FLAG_NONE,
                    &raw const desc,
                    D3D12_RESOURCE_STATE_GENERIC_READ,
                    None,
                    &raw mut resource,
                )
                .map_err(backend_error)?;
            let resource = resource.ok_or_else(|| {
                RenderError::Backend("D3D12 returned an empty upload resource".into())
            })?;
            let mut pointer = std::ptr::null_mut();
            let read_range = D3D12_RANGE { Begin: 0, End: 0 };
            resource
                .Map(0, Some(&raw const read_range), Some(&raw mut pointer))
                .map_err(backend_error)?;
            let Some(mapped) = NonNull::new(pointer.cast::<u8>()) else {
                resource.Unmap(0, Some(&raw const read_range));
                return Err(RenderError::Backend(
                    "D3D12 mapped an upload resource to a null address".into(),
                ));
            };
            Ok(Self { resource, mapped, capacity, cursor: 0 })
        }
    }

    fn reset(&mut self) {
        self.cursor = 0;
    }

    fn allocate(&mut self, size: usize, alignment: usize) -> Option<NonNull<u8>> {
        if alignment == 0 || !alignment.is_power_of_two() {
            return None;
        }
        let offset = self.cursor.checked_add(alignment - 1)? & !(alignment - 1);
        let end = offset.checked_add(size)?;
        if end > self.capacity {
            return None;
        }
        self.cursor = end;
        // SAFETY: the computed range is within the mapped resource capacity.
        NonNull::new(unsafe { self.mapped.as_ptr().add(offset) })
    }
}

impl Drop for MappedUploadArena {
    fn drop(&mut self) {
        // SAFETY: Map succeeded exactly once for this resource.
        unsafe {
            let written_range = D3D12_RANGE { Begin: 0, End: self.cursor };
            self.resource.Unmap(0, Some(&raw const written_range));
        }
    }
}

/// D3D12 bootstrap plus a fence-retired, frame-local allocation cursor.
///
/// Swap-chain images are allocated by DXGI. The per-frame cursors provide the
/// initial transient upload-allocation policy and can only be reset after the
/// corresponding GPU fence retires.
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct D3d12Renderer {
    adapter: IDXGIAdapter3,
    device: ID3D12Device,
    queue: ID3D12CommandQueue,
    swapchain: IDXGISwapChain3,
    command_list: ID3D12GraphicsCommandList,
    fence: ID3D12Fence,
    fence_event: OwnedHandle,
    frame_latency: OwnedHandle,
    rtv_heap: ID3D12DescriptorHeap,
    rtv_increment: u32,
    back_buffers: Vec<ID3D12Resource>,
    frames: Vec<FrameResources>,
    next_frame_slot: usize,
    next_fence_value: u64,
    vertical_sync: bool,
    width: u32,
    height: u32,
    suspended: bool,
    heaps: Vec<Option<NativeHeapBlock>>,
    resources: Vec<NativeResourceSlot>,
    free_resource_slots: Vec<u32>,
    resource_descriptor_heap: ID3D12DescriptorHeap,
    sampler_descriptor_heap: ID3D12DescriptorHeap,
    dsv_heap: ID3D12DescriptorHeap,
    resource_descriptor_increment: u32,
    sampler_descriptor_increment: u32,
    dsv_increment: u32,
    pipelines: HashMap<PipelineKey, NativePipeline>,
    compute_pipelines: HashMap<crate::ShaderId, NativePipeline>,
    indexed_draw_command_signature: ID3D12CommandSignature,
    graph_submission_active: bool,
    inflight_pass_lists: Vec<(ID3D12CommandAllocator, ID3D12GraphicsCommandList)>,
    record_only: bool,
}

impl D3d12Renderer {
    fn recording_snapshot(&self) -> Self {
        Self {
            adapter: self.adapter.clone(),
            device: self.device.clone(),
            queue: self.queue.clone(),
            swapchain: self.swapchain.clone(),
            command_list: self.command_list.clone(),
            fence: self.fence.clone(),
            fence_event: OwnedHandle(HANDLE::default(), false),
            frame_latency: OwnedHandle(HANDLE::default(), false),
            rtv_heap: self.rtv_heap.clone(),
            rtv_increment: self.rtv_increment,
            back_buffers: self.back_buffers.clone(),
            frames: Vec::new(),
            next_frame_slot: 0,
            next_fence_value: self.next_fence_value,
            vertical_sync: self.vertical_sync,
            width: self.width,
            height: self.height,
            suspended: self.suspended,
            heaps: self.heaps.clone(),
            resources: self.resources.clone(),
            free_resource_slots: self.free_resource_slots.clone(),
            resource_descriptor_heap: self.resource_descriptor_heap.clone(),
            sampler_descriptor_heap: self.sampler_descriptor_heap.clone(),
            dsv_heap: self.dsv_heap.clone(),
            resource_descriptor_increment: self.resource_descriptor_increment,
            sampler_descriptor_increment: self.sampler_descriptor_increment,
            dsv_increment: self.dsv_increment,
            pipelines: self.pipelines.clone(),
            compute_pipelines: self.compute_pipelines.clone(),
            indexed_draw_command_signature: self.indexed_draw_command_signature.clone(),
            graph_submission_active: true,
            inflight_pass_lists: Vec::new(),
            record_only: true,
        }
    }

    pub(crate) const fn is_suspended(&self) -> bool {
        self.suspended
    }

    pub(crate) fn memory_budget(
        &self,
        segment: GpuMemorySegment,
    ) -> Result<GpuMemoryBudget, RenderError> {
        let mut info = DXGI_QUERY_VIDEO_MEMORY_INFO::default();
        let segment = match segment {
            GpuMemorySegment::Local => DXGI_MEMORY_SEGMENT_GROUP_LOCAL,
            GpuMemorySegment::NonLocal => DXGI_MEMORY_SEGMENT_GROUP_NON_LOCAL,
        };
        // SAFETY: adapter is a live IDXGIAdapter3 and info is writable.
        unsafe { self.adapter.QueryVideoMemoryInfo(0, segment, &raw mut info) }
            .map_err(backend_error)?;
        Ok(GpuMemoryBudget {
            budget_bytes: info.Budget,
            current_usage_bytes: info.CurrentUsage,
            available_for_reservation_bytes: info.AvailableForReservation,
            current_reservation_bytes: info.CurrentReservation,
        })
    }

    pub(crate) fn trim_memory_pools(
        &mut self,
        services: &mut RenderServices,
    ) -> Result<u64, RenderError> {
        self.wait_idle()?;
        let mut retained_classes = HashSet::new();
        let mut released_bytes = 0_u64;
        for (index, block) in self.heaps.iter_mut().enumerate() {
            let Some(native) = block.as_ref() else { continue };
            let index = u32::try_from(index).map_err(backend_error)?;
            if !services.allocator().is_block_empty(index) {
                continue;
            }
            if retained_classes.insert(native.class) {
                continue;
            }
            released_bytes =
                released_bytes.saturating_add(services.allocator().block_size(index).unwrap_or(0));
            *block = None;
        }
        Ok(released_bytes)
    }

    pub(crate) fn create_buffer(
        &mut self,
        services: &mut RenderServices,
        desc: BufferDesc,
    ) -> Result<GpuResourceHandle, RenderError> {
        if desc.memory() == BufferMemory::Upload && desc.usage().contains(BufferUsage::STORAGE) {
            return Err(RenderError::Backend(
                "CPU-visible upload buffers cannot request unordered-access storage".into(),
            ));
        }
        if desc.usage().contains(BufferUsage::CONSTANT) && !desc.size().is_multiple_of(256) {
            return Err(RenderError::Backend(
                "constant buffer size must be a multiple of 256 bytes".into(),
            ));
        }
        if desc.usage().contains(BufferUsage::STORAGE) && !desc.size().is_multiple_of(4) {
            return Err(RenderError::Backend(
                "raw storage buffer size must be a multiple of four bytes".into(),
            ));
        }
        let mut native_desc = buffer_resource_desc(desc);
        if desc.usage().contains(crate::BufferUsage::STORAGE) {
            native_desc.Flags = D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS;
        }
        if desc.memory() == BufferMemory::Readback
            && (!desc.usage().contains(BufferUsage::COPY_DESTINATION)
                || desc.usage().bits() != BufferUsage::COPY_DESTINATION.bits())
        {
            return Err(RenderError::Backend(
                "readback buffers must be used only as copy destinations".into(),
            ));
        }
        if desc.memory() == BufferMemory::Upload
            && desc.usage().contains(BufferUsage::COPY_DESTINATION)
        {
            return Err(RenderError::Backend("upload buffers cannot be copy destinations".into()));
        }
        let class = match desc.memory() {
            BufferMemory::DeviceLocal => AllocationClass::DeviceLocalBuffer,
            BufferMemory::Upload => AllocationClass::Upload,
            BufferMemory::Readback => AllocationClass::Readback,
        };
        let state = match desc.memory() {
            BufferMemory::DeviceLocal => D3D12_RESOURCE_STATE_COMMON,
            BufferMemory::Upload => D3D12_RESOURCE_STATE_GENERIC_READ,
            BufferMemory::Readback => D3D12_RESOURCE_STATE_COPY_DEST,
        };
        self.create_placed_resource(
            services,
            native_desc,
            class,
            state,
            ResourceKind::Buffer,
            desc.usage().contains(BufferUsage::STORAGE),
            Some(desc.usage()),
            Some(desc.memory()),
            desc.size(),
            None,
        )
    }

    pub(crate) fn create_texture(
        &mut self,
        services: &mut RenderServices,
        desc: TextureDesc,
    ) -> Result<GpuResourceHandle, RenderError> {
        self.create_placed_resource(
            services,
            texture_resource_desc(desc),
            if desc.usage().contains(TextureUsage::COLOR_ATTACHMENT)
                || desc.usage().contains(TextureUsage::DEPTH_STENCIL_ATTACHMENT)
            {
                AllocationClass::DeviceLocalRenderTarget
            } else {
                AllocationClass::DeviceLocal
            },
            D3D12_RESOURCE_STATE_COMMON,
            ResourceKind::Texture,
            desc.usage().contains(TextureUsage::SAMPLED),
            None,
            None,
            0,
            None,
        )
    }

    /// Allocate all textures in a graph alias group at one shared heap offset.
    /// The graph guarantees their pass lifetimes do not overlap.
    pub(crate) fn create_transient_alias_group(
        &mut self,
        services: &mut RenderServices,
        descriptions: &[TextureDesc],
    ) -> Result<(Vec<GpuResourceHandle>, Allocation), RenderError> {
        let Some(first) = descriptions.first().copied() else {
            return Err(RenderError::Backend("transient alias group cannot be empty".into()));
        };
        let render_target_class = first.usage().contains(TextureUsage::COLOR_ATTACHMENT)
            || first.usage().contains(TextureUsage::DEPTH_STENCIL_ATTACHMENT);
        let class = if render_target_class {
            AllocationClass::DeviceLocalRenderTarget
        } else {
            AllocationClass::DeviceLocal
        };
        let mut size = 0_u64;
        let mut alignment = 0_u64;
        for description in descriptions {
            let is_render_target = description.usage().contains(TextureUsage::COLOR_ATTACHMENT)
                || description.usage().contains(TextureUsage::DEPTH_STENCIL_ATTACHMENT);
            if is_render_target != render_target_class {
                return Err(RenderError::Backend(
                    "transient alias group mixes incompatible D3D12 heap classes".into(),
                ));
            }
            // SAFETY: each texture descriptor was validated by TextureDesc.
            let info = unsafe {
                self.device.GetResourceAllocationInfo(0, &[texture_resource_desc(*description)])
            };
            if info.SizeInBytes == 0 || info.SizeInBytes == u64::MAX || info.Alignment == 0 {
                return Err(RenderError::Backend(
                    "D3D12 returned invalid transient allocation requirements".into(),
                ));
            }
            size = size.max(info.SizeInBytes);
            alignment = alignment.max(info.Alignment);
        }
        let allocation =
            services.allocator().allocate(class, size, alignment).ok_or_else(|| {
                RenderError::Backend("GPU allocator could not reserve transient alias range".into())
            })?;
        if let Err(error) = self.ensure_heap(allocation, services) {
            services.allocator().deallocate(allocation);
            return Err(error);
        }
        let mut handles = Vec::with_capacity(descriptions.len());
        for description in descriptions {
            let is_attachment = description.usage().contains(TextureUsage::COLOR_ATTACHMENT)
                || description.usage().contains(TextureUsage::DEPTH_STENCIL_ATTACHMENT);
            match self.create_placed_resource(
                services,
                texture_resource_desc(*description),
                class,
                D3D12_RESOURCE_STATE_COMMON,
                if is_attachment { ResourceKind::RenderTarget } else { ResourceKind::Texture },
                description.usage().contains(TextureUsage::SAMPLED),
                None,
                None,
                0,
                Some(allocation),
            ) {
                Ok(handle) => handles.push(handle),
                Err(error) => {
                    for handle in handles {
                        let _ = self.destroy_resource(services, handle);
                    }
                    services.allocator().deallocate(allocation);
                    return Err(error);
                }
            }
        }
        Ok((handles, allocation))
    }

    pub(crate) fn destroy_transient_alias_group(
        &mut self,
        services: &mut RenderServices,
        handles: &[GpuResourceHandle],
        allocation: Allocation,
    ) -> Result<(), RenderError> {
        self.wait_idle()?;
        for &handle in handles {
            self.destroy_resource(services, handle)?;
        }
        if !services.allocator().deallocate(allocation) {
            return Err(RenderError::Backend(
                "transient alias allocation bookkeeping was inconsistent".into(),
            ));
        }
        Ok(())
    }

    pub(crate) fn create_render_target(
        &mut self,
        services: &mut RenderServices,
        desc: RenderTargetDesc,
    ) -> Result<crate::RenderTargetResources, RenderError> {
        let (width, height) = desc.extent();
        let color = if let Some(format) = desc.color_format() {
            let usage = TextureUsage::COLOR_ATTACHMENT.union(TextureUsage::SAMPLED);
            let texture = TextureDesc::new(width, height, format, usage)
                .and_then(|texture| texture.with_samples(desc.samples()))
                .map_err(backend_error)?;
            Some(self.create_placed_resource(
                services,
                texture_resource_desc(texture),
                AllocationClass::DeviceLocalRenderTarget,
                D3D12_RESOURCE_STATE_COMMON,
                ResourceKind::RenderTarget,
                true,
                None,
                None,
                0,
                None,
            )?)
        } else {
            None
        };
        let depth_result = if let Some(format) = desc.depth_format() {
            let usage = TextureUsage::DEPTH_STENCIL_ATTACHMENT;
            TextureDesc::new(width, height, format, usage)
                .and_then(|texture| texture.with_samples(desc.samples()))
                .map_err(backend_error)
                .and_then(|texture| {
                    self.create_placed_resource(
                        services,
                        texture_resource_desc(texture),
                        AllocationClass::DeviceLocalRenderTarget,
                        D3D12_RESOURCE_STATE_COMMON,
                        ResourceKind::RenderTarget,
                        false,
                        None,
                        None,
                        0,
                        None,
                    )
                })
                .map(Some)
        } else {
            Ok(None)
        };
        let depth = match depth_result {
            Ok(depth) => depth,
            Err(error) => {
                if let Some(color) = color {
                    let _ = self.destroy_resource(services, color);
                }
                return Err(error);
            }
        };
        Ok(crate::RenderTargetResources { color, depth })
    }

    pub(crate) fn destroy_resource(
        &mut self,
        services: &mut RenderServices,
        handle: GpuResourceHandle,
    ) -> Result<(), RenderError> {
        self.wait_idle()?;
        let Some(slot) = self.resources.get_mut(handle.index() as usize) else {
            return Err(RenderError::Backend("GPU resource handle index is invalid".into()));
        };
        if slot.generation != handle.generation()
            || slot.kind != handle.kind()
            || slot.resource.is_none()
        {
            return Err(RenderError::Backend(
                "GPU resource handle is stale or has the wrong kind".into(),
            ));
        }
        for (descriptor, _) in std::mem::take(&mut slot.descriptors) {
            if !services.descriptors().release(descriptor) {
                return Err(RenderError::Backend(
                    "GPU descriptor handle was stale during resource destruction".into(),
                ));
            }
        }
        slot.resource.take();
        if let Some(allocation) = slot.allocation.take()
            && !services.allocator().deallocate(allocation)
        {
            return Err(RenderError::Backend(
                "GPU resource allocation bookkeeping was inconsistent".into(),
            ));
        }
        slot.generation =
            NonZeroU32::new(slot.generation.get().wrapping_add(1)).unwrap_or(NonZeroU32::MIN);
        self.free_resource_slots.push(handle.index());
        Ok(())
    }

    pub(crate) fn create_texture_srv(
        &mut self,
        services: &mut RenderServices,
        handle: GpuResourceHandle,
    ) -> Result<DescriptorHandle, RenderError> {
        let slot = self.validate_resource(handle)?;
        if !slot.shader_resource
            || slot.desc.Flags.contains(D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL)
        {
            return Err(RenderError::Backend(
                "texture SRV creation requires a non-depth texture resource".into(),
            ));
        }
        let resource = slot.resource.as_ref().expect("validated live resource").clone();
        let descriptor =
            services.descriptors().allocate(DescriptorKind::Resource).ok_or_else(|| {
                RenderError::Backend("shader-visible resource descriptor heap is full".into())
            })?;
        let start = unsafe { self.resource_descriptor_heap.GetCPUDescriptorHandleForHeapStart() };
        let cpu = D3D12_CPU_DESCRIPTOR_HANDLE {
            ptr: start.ptr
                + descriptor.index() as usize * self.resource_descriptor_increment as usize,
        };
        // SAFETY: the resource is a live texture and the slot belongs to the
        // renderer's stable shader-visible CBV/SRV/UAV descriptor heap.
        unsafe { self.device.CreateShaderResourceView(&resource, None, cpu) };
        self.resources[handle.index() as usize]
            .descriptors
            .push((descriptor, NativeViewKind::ShaderResource));
        Ok(descriptor)
    }

    pub(crate) fn create_buffer_srv(
        &mut self,
        services: &mut RenderServices,
        handle: GpuResourceHandle,
    ) -> Result<DescriptorHandle, RenderError> {
        let slot = self.validate_resource(handle)?;
        if slot.kind != ResourceKind::Buffer
            || !slot.shader_resource
            || !slot.buffer_usage.is_some_and(|usage| usage.contains(BufferUsage::STORAGE))
            || !slot.byte_size.is_multiple_of(4)
        {
            return Err(RenderError::Backend(
                "raw buffer SRV creation requires a four-byte-aligned storage buffer".into(),
            ));
        }
        let resource = slot.resource.as_ref().expect("validated live resource").clone();
        let descriptor =
            services.descriptors().allocate(DescriptorKind::Resource).ok_or_else(|| {
                RenderError::Backend("shader-visible resource descriptor heap is full".into())
            })?;
        let heap = unsafe { self.resource_descriptor_heap.GetCPUDescriptorHandleForHeapStart() };
        let cpu = D3D12_CPU_DESCRIPTOR_HANDLE {
            ptr: heap.ptr
                + descriptor.index() as usize * self.resource_descriptor_increment as usize,
        };
        let desc = D3D12_SHADER_RESOURCE_VIEW_DESC {
            Format: DXGI_FORMAT_R32_TYPELESS,
            ViewDimension: D3D12_SRV_DIMENSION_BUFFER,
            Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
            Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
                Buffer: D3D12_BUFFER_SRV {
                    FirstElement: 0,
                    NumElements: u32::try_from(slot.byte_size / 4).map_err(backend_error)?,
                    StructureByteStride: 0,
                    Flags: D3D12_BUFFER_SRV_FLAG_RAW,
                },
            },
        };
        // SAFETY: the live storage buffer is four-byte aligned and the raw SRV
        // covers exactly the validated buffer byte range.
        unsafe { self.device.CreateShaderResourceView(&resource, Some(&raw const desc), cpu) };
        self.resources[handle.index() as usize]
            .descriptors
            .push((descriptor, NativeViewKind::ShaderResource));
        Ok(descriptor)
    }

    pub(crate) fn create_texture_srv_array(
        &mut self,
        services: &mut RenderServices,
        handles: &[GpuResourceHandle],
    ) -> Result<Vec<DescriptorHandle>, RenderError> {
        if handles.is_empty() {
            return Err(RenderError::Backend("texture SRV arrays cannot be empty".into()));
        }
        let mut resources = Vec::with_capacity(handles.len());
        for handle in handles {
            let slot = self.validate_resource(*handle)?;
            if !slot.shader_resource
                || slot.desc.Flags.contains(D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL)
            {
                return Err(RenderError::Backend(
                    "texture SRV array contains a resource that cannot be sampled".into(),
                ));
            }
            resources.push(slot.resource.as_ref().expect("validated live resource").clone());
        }
        let descriptors = services
            .descriptors()
            .allocate_range(DescriptorKind::Resource, handles.len())
            .ok_or_else(|| {
                RenderError::Backend(
                    "no contiguous shader-visible resource descriptor range is available".into(),
                )
            })?;
        let start = unsafe { self.resource_descriptor_heap.GetCPUDescriptorHandleForHeapStart() };
        for ((handle, resource), descriptor) in handles.iter().zip(resources).zip(&descriptors) {
            let cpu = D3D12_CPU_DESCRIPTOR_HANDLE {
                ptr: start.ptr
                    + descriptor.index() as usize * self.resource_descriptor_increment as usize,
            };
            // SAFETY: each resource was validated as shader-readable and each
            // destination is a live slot in the renderer's resource heap.
            unsafe { self.device.CreateShaderResourceView(&resource, None, cpu) };
            self.resources[handle.index() as usize]
                .descriptors
                .push((*descriptor, NativeViewKind::ShaderResource));
        }
        Ok(descriptors)
    }

    pub(crate) fn create_constant_buffer_view(
        &mut self,
        services: &mut RenderServices,
        handle: GpuResourceHandle,
    ) -> Result<DescriptorHandle, RenderError> {
        let slot = self.validate_resource(handle)?;
        if slot.kind != ResourceKind::Buffer
            || !slot.buffer_usage.is_some_and(|usage| usage.contains(BufferUsage::CONSTANT))
            || slot.byte_size == 0
            || slot.byte_size % 256 != 0
        {
            return Err(RenderError::Backend(
                "CBV requires a buffer whose size is a non-zero multiple of 256 bytes".into(),
            ));
        }
        let resource = slot.resource.as_ref().expect("validated live resource").clone();
        let byte_size = u32::try_from(slot.byte_size).map_err(backend_error)?;
        let descriptor =
            services.descriptors().allocate(DescriptorKind::Resource).ok_or_else(|| {
                RenderError::Backend("shader-visible resource descriptor heap is full".into())
            })?;
        let start = unsafe { self.resource_descriptor_heap.GetCPUDescriptorHandleForHeapStart() };
        let cpu = D3D12_CPU_DESCRIPTOR_HANDLE {
            ptr: start.ptr
                + descriptor.index() as usize * self.resource_descriptor_increment as usize,
        };
        // SAFETY: the GPU virtual address belongs to a live buffer and size is
        // validated to the 256-byte D3D12 CBV alignment.
        let desc = D3D12_CONSTANT_BUFFER_VIEW_DESC {
            BufferLocation: unsafe { resource.GetGPUVirtualAddress() },
            SizeInBytes: byte_size,
        };
        unsafe { self.device.CreateConstantBufferView(Some(&raw const desc), cpu) };
        self.resources[handle.index() as usize]
            .descriptors
            .push((descriptor, NativeViewKind::ConstantBuffer));
        Ok(descriptor)
    }

    pub(crate) fn create_constant_buffer_view_array(
        &mut self,
        services: &mut RenderServices,
        handles: &[GpuResourceHandle],
    ) -> Result<Vec<DescriptorHandle>, RenderError> {
        if handles.is_empty() {
            return Err(RenderError::Backend("CBV arrays cannot be empty".into()));
        }
        let mut resources = Vec::with_capacity(handles.len());
        let mut sizes = Vec::with_capacity(handles.len());
        for handle in handles {
            let slot = self.validate_resource(*handle)?;
            if slot.kind != ResourceKind::Buffer
                || !slot.buffer_usage.is_some_and(|usage| usage.contains(BufferUsage::CONSTANT))
                || slot.byte_size == 0
                || slot.byte_size % 256 != 0
            {
                return Err(RenderError::Backend(
                    "CBV array elements must be non-zero, 256-byte-aligned constant buffers".into(),
                ));
            }
            resources.push(slot.resource.as_ref().expect("validated live resource").clone());
            sizes.push(u32::try_from(slot.byte_size).map_err(backend_error)?);
        }
        let descriptors = services
            .descriptors()
            .allocate_range(DescriptorKind::Resource, handles.len())
            .ok_or_else(|| {
                RenderError::Backend("no contiguous resource descriptor range is available".into())
            })?;
        let start = unsafe { self.resource_descriptor_heap.GetCPUDescriptorHandleForHeapStart() };
        for (((handle, resource), size), descriptor) in
            handles.iter().zip(resources).zip(sizes).zip(&descriptors)
        {
            let desc = D3D12_CONSTANT_BUFFER_VIEW_DESC {
                BufferLocation: unsafe { resource.GetGPUVirtualAddress() },
                SizeInBytes: size,
            };
            let cpu = D3D12_CPU_DESCRIPTOR_HANDLE {
                ptr: start.ptr
                    + descriptor.index() as usize * self.resource_descriptor_increment as usize,
            };
            // SAFETY: the resource was validated as a live, aligned constant buffer.
            unsafe { self.device.CreateConstantBufferView(Some(&raw const desc), cpu) };
            self.resources[handle.index() as usize]
                .descriptors
                .push((*descriptor, NativeViewKind::ConstantBuffer));
        }
        Ok(descriptors)
    }

    pub(crate) fn create_unordered_access_view(
        &mut self,
        services: &mut RenderServices,
        handle: GpuResourceHandle,
    ) -> Result<DescriptorHandle, RenderError> {
        let slot = self.validate_resource(handle)?;
        if !slot.desc.Flags.contains(D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS) {
            return Err(RenderError::Backend(
                "UAV creation requires storage usage on the resource".into(),
            ));
        }
        let resource = slot.resource.as_ref().expect("validated live resource").clone();
        let desc = uav_description(slot)?;
        let descriptor =
            services.descriptors().allocate(DescriptorKind::Resource).ok_or_else(|| {
                RenderError::Backend("shader-visible resource descriptor heap is full".into())
            })?;
        let start = unsafe { self.resource_descriptor_heap.GetCPUDescriptorHandleForHeapStart() };
        let cpu = D3D12_CPU_DESCRIPTOR_HANDLE {
            ptr: start.ptr
                + descriptor.index() as usize * self.resource_descriptor_increment as usize,
        };
        // SAFETY: the resource was created with ALLOW_UNORDERED_ACCESS and
        // the descriptor is allocated in the renderer's stable resource heap.
        unsafe {
            self.device.CreateUnorderedAccessView(&resource, None, Some(&raw const desc), cpu);
        }
        self.resources[handle.index() as usize]
            .descriptors
            .push((descriptor, NativeViewKind::UnorderedAccess));
        Ok(descriptor)
    }

    pub(crate) fn create_unordered_access_view_array(
        &mut self,
        services: &mut RenderServices,
        handles: &[GpuResourceHandle],
    ) -> Result<Vec<DescriptorHandle>, RenderError> {
        if handles.is_empty() {
            return Err(RenderError::Backend("UAV arrays cannot be empty".into()));
        }
        let mut resources = Vec::with_capacity(handles.len());
        let mut views = Vec::with_capacity(handles.len());
        for handle in handles {
            let slot = self.validate_resource(*handle)?;
            if !slot.desc.Flags.contains(D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS) {
                return Err(RenderError::Backend(
                    "UAV array elements must have storage usage".into(),
                ));
            }
            resources.push(slot.resource.as_ref().expect("validated live resource").clone());
            views.push(uav_description(slot)?);
        }
        let descriptors = services
            .descriptors()
            .allocate_range(DescriptorKind::Resource, handles.len())
            .ok_or_else(|| {
                RenderError::Backend("no contiguous resource descriptor range is available".into())
            })?;
        let start = unsafe { self.resource_descriptor_heap.GetCPUDescriptorHandleForHeapStart() };
        for (((handle, resource), desc), descriptor) in
            handles.iter().zip(resources).zip(views).zip(&descriptors)
        {
            let cpu = D3D12_CPU_DESCRIPTOR_HANDLE {
                ptr: start.ptr
                    + descriptor.index() as usize * self.resource_descriptor_increment as usize,
            };
            // SAFETY: the resource was validated for UAV use and the range is live.
            unsafe {
                self.device.CreateUnorderedAccessView(&resource, None, Some(&raw const desc), cpu);
            }
            self.resources[handle.index() as usize]
                .descriptors
                .push((*descriptor, NativeViewKind::UnorderedAccess));
        }
        Ok(descriptors)
    }

    pub(crate) fn create_render_target_view(
        &mut self,
        services: &mut RenderServices,
        handle: GpuResourceHandle,
    ) -> Result<DescriptorHandle, RenderError> {
        let slot = self.validate_resource(handle)?;
        if slot.kind != ResourceKind::RenderTarget
            || !slot.desc.Flags.contains(D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET)
        {
            return Err(RenderError::Backend(
                "RTV creation requires a color render-target resource".into(),
            ));
        }
        let resource = slot.resource.as_ref().expect("validated live resource").clone();
        let descriptor = services
            .descriptors()
            .allocate(DescriptorKind::RenderTarget)
            .ok_or_else(|| RenderError::Backend("render-target descriptor heap is full".into()))?;
        let start = unsafe { self.rtv_heap.GetCPUDescriptorHandleForHeapStart() };
        let cpu = D3D12_CPU_DESCRIPTOR_HANDLE {
            ptr: start.ptr
                + (BACK_BUFFER_COUNT as usize + descriptor.index() as usize)
                    * self.rtv_increment as usize,
        };
        // SAFETY: the resource allows render-target use and this slot lies
        // beyond the three swapchain views in the CPU RTV heap.
        unsafe { self.device.CreateRenderTargetView(&resource, None, cpu) };
        self.resources[handle.index() as usize]
            .descriptors
            .push((descriptor, NativeViewKind::RenderTarget));
        Ok(descriptor)
    }

    pub(crate) fn create_depth_stencil_view(
        &mut self,
        services: &mut RenderServices,
        handle: GpuResourceHandle,
    ) -> Result<DescriptorHandle, RenderError> {
        let slot = self.validate_resource(handle)?;
        if slot.kind != ResourceKind::RenderTarget
            || !slot.desc.Flags.contains(D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL)
        {
            return Err(RenderError::Backend(
                "DSV creation requires a depth-stencil resource".into(),
            ));
        }
        let resource = slot.resource.as_ref().expect("validated live resource").clone();
        let descriptor = services
            .descriptors()
            .allocate(DescriptorKind::DepthStencil)
            .ok_or_else(|| RenderError::Backend("depth-stencil descriptor heap is full".into()))?;
        let start = unsafe { self.dsv_heap.GetCPUDescriptorHandleForHeapStart() };
        let cpu = D3D12_CPU_DESCRIPTOR_HANDLE {
            ptr: start.ptr + descriptor.index() as usize * self.dsv_increment as usize,
        };
        // SAFETY: the resource allows depth-stencil use and the descriptor is
        // a valid slot in the renderer's non-shader-visible DSV heap.
        unsafe { self.device.CreateDepthStencilView(&resource, None, cpu) };
        self.resources[handle.index() as usize]
            .descriptors
            .push((descriptor, NativeViewKind::DepthStencil));
        Ok(descriptor)
    }

    pub(crate) fn create_sampler(
        &mut self,
        services: &mut RenderServices,
        sampler: SamplerDesc,
    ) -> Result<DescriptorHandle, RenderError> {
        let sampler = sampler.validate().map_err(backend_error)?;
        let descriptor = services
            .descriptors()
            .allocate(DescriptorKind::Sampler)
            .ok_or_else(|| RenderError::Backend("shader-visible sampler heap is full".into()))?;
        let desc = sampler_description(sampler);
        // SAFETY: the sampler descriptor was validated and the destination is
        // an allocated slot in the stable shader-visible sampler heap.
        unsafe {
            let start = self.sampler_descriptor_heap.GetCPUDescriptorHandleForHeapStart();
            let cpu = D3D12_CPU_DESCRIPTOR_HANDLE {
                ptr: start.ptr
                    + descriptor.index() as usize * self.sampler_descriptor_increment as usize,
            };
            self.device.CreateSampler(&raw const desc, cpu);
        }
        Ok(descriptor)
    }

    pub(crate) fn create_sampler_array(
        &mut self,
        services: &mut RenderServices,
        samplers: &[SamplerDesc],
    ) -> Result<Vec<DescriptorHandle>, RenderError> {
        if samplers.is_empty() {
            return Err(RenderError::Backend("sampler arrays cannot be empty".into()));
        }
        let samplers = samplers
            .iter()
            .map(|sampler| sampler.validate().map_err(backend_error))
            .collect::<Result<Vec<_>, _>>()?;
        let descriptors = services
            .descriptors()
            .allocate_range(DescriptorKind::Sampler, samplers.len())
            .ok_or_else(|| {
                RenderError::Backend(
                    "no contiguous shader-visible sampler descriptor range is available".into(),
                )
            })?;
        let start = unsafe { self.sampler_descriptor_heap.GetCPUDescriptorHandleForHeapStart() };
        for (sampler, descriptor) in samplers.into_iter().zip(&descriptors) {
            let desc = sampler_description(sampler);
            let cpu = D3D12_CPU_DESCRIPTOR_HANDLE {
                ptr: start.ptr
                    + descriptor.index() as usize * self.sampler_descriptor_increment as usize,
            };
            // SAFETY: validated sampler state is written into its allocated
            // shader-visible sampler descriptor slot.
            unsafe { self.device.CreateSampler(&raw const desc, cpu) };
        }
        Ok(descriptors)
    }

    pub(crate) fn destroy_descriptor(
        &mut self,
        services: &mut RenderServices,
        descriptor: DescriptorHandle,
    ) -> Result<(), RenderError> {
        self.wait_idle()?;
        if !services.descriptors().release(descriptor) {
            return Err(RenderError::Backend(
                "descriptor handle is stale or already released".into(),
            ));
        }
        // Resource-owned descriptors are also released when their resource is
        // destroyed. Remove this independently retired descriptor from that
        // ownership list so later resource destruction remains consistent.
        for resource in &mut self.resources {
            resource.descriptors.retain(|(owned, _)| *owned != descriptor);
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn create_graphics_pipeline(
        &mut self,
        services: &mut RenderServices,
        description: &GraphicsPipelineDesc,
        vertex: &CompiledShader,
        pixel: &CompiledShader,
        frame: u64,
    ) -> Result<PipelineCacheEntry, RenderError> {
        description.validate().map_err(backend_error)?;
        if vertex.stage != ShaderStage::Vertex
            || pixel.stage != ShaderStage::Pixel
            || vertex.shader_id != description.vertex_shader
            || pixel.shader_id != description.pixel_shader
        {
            return Err(RenderError::Backend(
                "pipeline shaders do not match the declared stages and shader IDs".into(),
            ));
        }
        let key = PipelineKey(description.clone());
        if self.pipelines.contains_key(&key) {
            return services.pipelines().get_or_insert(description, frame).map_err(backend_error);
        }
        let (_descriptor_ranges, root_parameters, _binding_keys) =
            root_signature_parts(&[(vertex, 1), (pixel, 2)])?;
        let root_desc = D3D12_ROOT_SIGNATURE_DESC {
            NumParameters: u32::try_from(root_parameters.len()).map_err(backend_error)?,
            pParameters: root_parameters.as_ptr(),
            Flags: D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT,
            ..Default::default()
        };
        let mut serialized = None;
        // SAFETY: this is a valid empty root signature with no pointer arrays.
        unsafe {
            D3D12SerializeRootSignature(
                &raw const root_desc,
                D3D_ROOT_SIGNATURE_VERSION_1,
                &raw mut serialized,
                None,
            )
        }
        .map_err(backend_error)?;
        let serialized = serialized.ok_or_else(|| {
            RenderError::Backend("D3D12 root-signature serialization returned no data".into())
        })?;
        // SAFETY: the serializer-owned blob remains alive until the API copies
        // the serialized bytes into the native root signature object.
        let root_bytes = unsafe {
            std::slice::from_raw_parts(
                serialized.GetBufferPointer().cast::<u8>(),
                serialized.GetBufferSize(),
            )
        };
        let root_signature: ID3D12RootSignature =
            unsafe { self.device.CreateRootSignature(0, root_bytes) }.map_err(backend_error)?;

        let blend_target = D3D12_RENDER_TARGET_BLEND_DESC {
            SrcBlend: match description.blend_mode {
                BlendMode::Opaque => {
                    if description.alpha_blending {
                        D3D12_BLEND_SRC_ALPHA
                    } else {
                        D3D12_BLEND_ONE
                    }
                }
                BlendMode::Alpha => D3D12_BLEND_SRC_ALPHA,
                BlendMode::InvertDestinationColor | BlendMode::HighContrastInvert => {
                    D3D12_BLEND_INV_DEST_COLOR
                }
            },
            DestBlend: match description.blend_mode {
                BlendMode::Opaque => {
                    if description.alpha_blending {
                        D3D12_BLEND_INV_SRC_ALPHA
                    } else {
                        D3D12_BLEND_ZERO
                    }
                }
                BlendMode::Alpha => D3D12_BLEND_ZERO,
                BlendMode::InvertDestinationColor => D3D12_BLEND_ZERO,
                BlendMode::HighContrastInvert => D3D12_BLEND_ONE,
            },
            BlendOp: if description.blend_mode == BlendMode::HighContrastInvert {
                D3D12_BLEND_OP_SUBTRACT
            } else {
                D3D12_BLEND_OP_ADD
            },
            SrcBlendAlpha: D3D12_BLEND_ONE,
            DestBlendAlpha: D3D12_BLEND_ZERO,
            BlendOpAlpha: D3D12_BLEND_OP_ADD,
            LogicOp: D3D12_LOGIC_OP_NOOP,
            RenderTargetWriteMask: 0x0f,
            BlendEnable: windows::Win32::Foundation::BOOL::from(
                description.alpha_blending || description.blend_mode != BlendMode::Opaque,
            ),
            ..Default::default()
        };
        let mut blend_state = D3D12_BLEND_DESC::default();
        let color_formats = description.color_formats().collect::<Vec<_>>();
        for blend_target_state in blend_state.RenderTarget.iter_mut().take(color_formats.len()) {
            *blend_target_state = blend_target;
        }

        let rasterizer = D3D12_RASTERIZER_DESC {
            FillMode: match description.fill_mode {
                PolygonFillMode::Solid => D3D12_FILL_MODE_SOLID,
                PolygonFillMode::Wireframe => D3D12_FILL_MODE_WIREFRAME,
            },
            CullMode: match description.cull_mode {
                CullMode::None => D3D12_CULL_MODE_NONE,
                CullMode::Front => D3D12_CULL_MODE_FRONT,
                CullMode::Back => D3D12_CULL_MODE_BACK,
            },
            FrontCounterClockwise: (description.front_face == FrontFace::CounterClockwise).into(),
            DepthBias: D3D12_DEFAULT_DEPTH_BIAS,
            DepthBiasClamp: D3D12_DEFAULT_DEPTH_BIAS_CLAMP,
            SlopeScaledDepthBias: D3D12_DEFAULT_SLOPE_SCALED_DEPTH_BIAS,
            DepthClipEnable: true.into(),
            ConservativeRaster: D3D12_CONSERVATIVE_RASTERIZATION_MODE_OFF,
            ..Default::default()
        };
        let stencil_face = D3D12_DEPTH_STENCILOP_DESC {
            StencilFailOp: D3D12_STENCIL_OP_KEEP,
            StencilDepthFailOp: D3D12_STENCIL_OP_KEEP,
            StencilPassOp: D3D12_STENCIL_OP_KEEP,
            StencilFunc: D3D12_COMPARISON_FUNC_ALWAYS,
        };
        let depth_state = D3D12_DEPTH_STENCIL_DESC {
            DepthEnable: windows::Win32::Foundation::BOOL::from(
                description.depth_test || description.depth_write,
            ),
            DepthWriteMask: if description.depth_write {
                D3D12_DEPTH_WRITE_MASK_ALL
            } else {
                D3D12_DEPTH_WRITE_MASK_ZERO
            },
            DepthFunc: windows::Win32::Graphics::Direct3D12::D3D12_COMPARISON_FUNC_LESS_EQUAL,
            StencilEnable: false.into(),
            StencilReadMask: u8::MAX,
            StencilWriteMask: u8::MAX,
            FrontFace: stencil_face,
            BackFace: stencil_face,
        };
        let semantic_names = description
            .vertex_attributes
            .iter()
            .map(|attribute| CString::new(attribute.semantic.as_bytes()).map_err(backend_error))
            .collect::<Result<Vec<_>, _>>()?;
        let input_elements = description
            .vertex_attributes
            .iter()
            .zip(&semantic_names)
            .map(|(attribute, semantic)| D3D12_INPUT_ELEMENT_DESC {
                SemanticName: windows::core::PCSTR(semantic.as_ptr().cast()),
                SemanticIndex: attribute.semantic_index,
                Format: vertex_format(attribute.format),
                InputSlot: attribute.input_slot,
                AlignedByteOffset: attribute.byte_offset,
                InputSlotClass: if attribute.per_instance {
                    D3D12_INPUT_CLASSIFICATION_PER_INSTANCE_DATA
                } else {
                    D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA
                },
                InstanceDataStepRate: attribute.instance_step_rate,
            })
            .collect::<Vec<_>>();
        let input_layout = D3D12_INPUT_LAYOUT_DESC {
            pInputElementDescs: input_elements.as_ptr(),
            NumElements: u32::try_from(input_elements.len()).map_err(backend_error)?,
        };
        let mut native = D3D12_GRAPHICS_PIPELINE_STATE_DESC {
            pRootSignature: std::mem::ManuallyDrop::new(Some(root_signature.clone())),
            VS: shader_bytecode(vertex),
            PS: shader_bytecode(pixel),
            BlendState: blend_state,
            SampleMask: u32::MAX,
            RasterizerState: rasterizer,
            DepthStencilState: depth_state,
            InputLayout: input_layout,
            PrimitiveTopologyType: match description.topology {
                PrimitiveTopology::Point => D3D12_PRIMITIVE_TOPOLOGY_TYPE_POINT,
                PrimitiveTopology::Line => D3D12_PRIMITIVE_TOPOLOGY_TYPE_LINE,
                PrimitiveTopology::Triangle => D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE,
            },
            NumRenderTargets: u32::try_from(color_formats.len()).map_err(backend_error)?,
            DSVFormat: description.depth_format.map_or(DXGI_FORMAT_UNKNOWN, dxgi_format),
            SampleDesc: DXGI_SAMPLE_DESC { Count: u32::from(description.sample_count), Quality: 0 },
            ..Default::default()
        };
        for (native_format, format) in native.RTVFormats.iter_mut().zip(color_formats) {
            *native_format = dxgi_format(format);
        }
        // SAFETY: bytecode is valid compiler output retained for this call and
        // the root signature/state description are fully initialized.
        let state = unsafe {
            self.device.CreateGraphicsPipelineState::<ID3D12PipelineState>(&raw const native)
        }
        .map_err(backend_error)?;
        self.pipelines.insert(key, NativePipeline { root_signature, pipeline_state: state });
        services.pipelines().get_or_insert(description, frame).map_err(backend_error)
    }

    pub(crate) fn create_compute_pipeline(
        &mut self,
        services: &mut RenderServices,
        description: ComputePipelineDesc,
        shader: &CompiledShader,
        frame: u64,
    ) -> Result<PipelineCacheEntry, RenderError> {
        if shader.stage != ShaderStage::Compute || shader.shader_id != description.compute_shader {
            return Err(RenderError::Backend(
                "compute pipeline shader does not match the declared stage and shader ID".into(),
            ));
        }
        if self.compute_pipelines.contains_key(&description.compute_shader) {
            return Ok(services
                .pipelines()
                .get_or_insert_compute(description.compute_shader, frame));
        }
        let (_descriptor_ranges, root_parameters, _binding_keys) =
            root_signature_parts(&[(shader, 4)])?;
        let root_desc = D3D12_ROOT_SIGNATURE_DESC {
            NumParameters: u32::try_from(root_parameters.len()).map_err(backend_error)?,
            pParameters: root_parameters.as_ptr(),
            ..Default::default()
        };
        let mut serialized = None;
        // SAFETY: root parameters and their ranges remain alive through serialization.
        unsafe {
            D3D12SerializeRootSignature(
                &raw const root_desc,
                D3D_ROOT_SIGNATURE_VERSION_1,
                &raw mut serialized,
                None,
            )
        }
        .map_err(backend_error)?;
        let serialized = serialized.ok_or_else(|| {
            RenderError::Backend("D3D12 root-signature serialization returned no data".into())
        })?;
        // SAFETY: serialized blob remains alive until CreateRootSignature copies it.
        let root_bytes = unsafe {
            std::slice::from_raw_parts(
                serialized.GetBufferPointer().cast::<u8>(),
                serialized.GetBufferSize(),
            )
        };
        let root_signature: ID3D12RootSignature =
            unsafe { self.device.CreateRootSignature(0, root_bytes) }.map_err(backend_error)?;
        let native = D3D12_COMPUTE_PIPELINE_STATE_DESC {
            pRootSignature: ManuallyDrop::new(Some(root_signature.clone())),
            CS: shader_bytecode(shader),
            ..Default::default()
        };
        // SAFETY: shader bytecode and root signature remain live during creation.
        let state = unsafe {
            self.device.CreateComputePipelineState::<ID3D12PipelineState>(&raw const native)
        }
        .map_err(backend_error)?;
        self.compute_pipelines.insert(
            description.compute_shader,
            NativePipeline { root_signature, pipeline_state: state },
        );
        Ok(services.pipelines().get_or_insert_compute(description.compute_shader, frame))
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines, clippy::cast_precision_loss)]
    pub(crate) fn execute_graphics_pass(
        &mut self,
        services: &mut RenderServices,
        description: &GraphicsPipelineDesc,
        vertex: &CompiledShader,
        pixel: &CompiledShader,
        color: GpuResourceHandle,
        color_view: DescriptorHandle,
        depth: Option<(GpuResourceHandle, DescriptorHandle)>,
        bindings: &[PipelineDescriptorTableBinding],
        clear_color: ClearColor,
        vertex_count: u32,
        instance_count: u32,
    ) -> Result<(), RenderError> {
        let pass = EncodedGraphicsPass {
            color_attachments: vec![crate::ColorAttachmentBinding {
                resource: color,
                view: color_view,
                clear_color,
            }],
            depth,
            clear_depth: 1.0,
            viewport: None,
            scissor: None,
            draws: vec![EncodedGraphicsDraw {
                description: description.clone(),
                vertex: vertex.clone(),
                pixel: pixel.clone(),
                bindings: bindings.to_vec(),
                vertex_buffers: Vec::new(),
                index_buffer: None,
                call: DrawCall::Vertices {
                    vertex_count,
                    instance_count,
                    first_vertex: 0,
                    first_instance: 0,
                },
                indirect: None,
            }],
        };
        self.execute_graphics_pass_batch(services, &pass)
    }

    #[allow(clippy::too_many_lines, clippy::cast_precision_loss)]
    pub(crate) fn execute_graphics_pass_batch(
        &mut self,
        services: &mut RenderServices,
        pass: &EncodedGraphicsPass,
    ) -> Result<(), RenderError> {
        if pass.color_attachments.is_empty() || pass.color_attachments.len() > 8 {
            return Err(RenderError::Backend(
                "graphics pass requires between one and eight color attachments".into(),
            ));
        }
        if !pass.clear_depth.is_finite() || !(0.0..=1.0).contains(&pass.clear_depth) {
            return Err(RenderError::InvalidClearColor);
        }
        let mut color_descs = Vec::with_capacity(pass.color_attachments.len());
        let mut color_resources = Vec::with_capacity(pass.color_attachments.len());
        let mut color_states = Vec::with_capacity(pass.color_attachments.len());
        for attachment in &pass.color_attachments {
            if attachment
                .clear_color
                .0
                .iter()
                .any(|channel| !channel.is_finite() || !(0.0..=1.0).contains(channel))
            {
                return Err(RenderError::InvalidClearColor);
            }
            let slot = self.validate_resource(attachment.resource)?;
            if slot.kind != ResourceKind::RenderTarget
                || slot.desc.Flags.contains(D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL)
                || !slot.desc.Flags.contains(D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET)
                || attachment.view.kind() != DescriptorKind::RenderTarget
                || !services.descriptors().is_live(attachment.view)
                || self.descriptor_owner(attachment.view)
                    != Some((attachment.resource, NativeViewKind::RenderTarget))
            {
                return Err(RenderError::Backend(
                    "graphics pass color target or RTV is invalid".into(),
                ));
            }
            let desc = slot.desc;
            if color_descs.first().is_some_and(|first: &D3D12_RESOURCE_DESC| {
                desc.Width != first.Width
                    || desc.Height != first.Height
                    || desc.SampleDesc.Count != first.SampleDesc.Count
            }) {
                return Err(RenderError::Backend(
                    "all color attachments must have matching extents and sample counts".into(),
                ));
            }
            color_descs.push(desc);
            color_resources.push(slot.resource.as_ref().expect("validated color target").clone());
            color_states.push(slot.state);
        }
        let color_desc = color_descs[0];

        let (depth_resource, depth_view, depth_native) = match pass.depth {
            Some((resource, view)) => {
                let slot = self.validate_resource(resource)?;
                if slot.kind != ResourceKind::RenderTarget
                    || slot.desc.Width != color_desc.Width
                    || slot.desc.Height != color_desc.Height
                    || slot.desc.SampleDesc.Count != color_desc.SampleDesc.Count
                    || !matches!(
                        slot.desc.Format,
                        DXGI_FORMAT_D32_FLOAT | DXGI_FORMAT_D24_UNORM_S8_UINT
                    )
                    || view.kind() != DescriptorKind::DepthStencil
                    || !services.descriptors().is_live(view)
                    || self.descriptor_owner(view) != Some((resource, NativeViewKind::DepthStencil))
                {
                    return Err(RenderError::Backend(
                        "graphics pass depth target or DSV is invalid".into(),
                    ));
                }
                let native = slot.resource.as_ref().expect("validated depth target").clone();
                (Some(resource), Some(view), Some((native, slot.state)))
            }
            None => (None, None, None),
        };

        let mut prepared = Vec::with_capacity(pass.draws.len());
        let mut uses_samplers = false;
        for draw in &pass.draws {
            draw.description.validate().map_err(|error| {
                RenderError::Backend(format!("invalid graphics pipeline description: {error}"))
            })?;
            if draw.vertex.shader_id != draw.description.vertex_shader
                || draw.pixel.shader_id != draw.description.pixel_shader
                || draw.vertex.stage != ShaderStage::Vertex
                || draw.pixel.stage != ShaderStage::Pixel
            {
                return Err(RenderError::Backend(
                    "draw shaders do not match the graphics pipeline".into(),
                ));
            }
            let pipeline_color_formats =
                draw.description.color_formats().map(dxgi_format).collect::<Vec<_>>();
            let pass_color_formats = color_descs.iter().map(|desc| desc.Format).collect::<Vec<_>>();
            if pipeline_color_formats != pass_color_formats
                || color_desc.SampleDesc.Count != u32::from(draw.description.sample_count)
                || draw.description.depth_format.map(dxgi_format)
                    != depth_resource.map(|resource| {
                        self.validate_resource(resource)
                            .expect("validated depth target")
                            .desc
                            .Format
                    })
            {
                return Err(RenderError::Backend(
                    "graphics pipeline formats do not match the pass attachments".into(),
                ));
            }
            let key = PipelineKey(draw.description.clone());
            let pipeline = self.pipelines.get(&key).ok_or_else(|| {
                RenderError::Backend(
                    "graphics pipeline must be created before pass execution".into(),
                )
            })?;
            let (root_signature, pipeline_state) =
                (pipeline.root_signature.clone(), pipeline.pipeline_state.clone());
            let (_, _, expected_bindings) =
                root_signature_parts(&[(&draw.vertex, 1), (&draw.pixel, 2)])?;
            if draw.bindings.len() != expected_bindings.len() {
                return Err(RenderError::Backend(
                    "draw must bind every reflected descriptor exactly once".into(),
                ));
            }
            for (kind, space, register, count) in expected_bindings.iter().copied() {
                let Some(binding) = draw.bindings.iter().find(|binding| {
                    (binding.kind, binding.space, binding.register) == (kind, space, register)
                }) else {
                    return Err(RenderError::Backend(format!(
                        "missing shader descriptor at {kind:?} register {register}, space {space}"
                    )));
                };
                let expected_kind = if kind == ShaderBindingKind::Sampler {
                    DescriptorKind::Sampler
                } else {
                    DescriptorKind::Resource
                };
                if binding.descriptors.len() != count as usize
                    || !is_contiguous_descriptor_range(&binding.descriptors)
                    || binding.descriptors.iter().any(|descriptor| {
                        descriptor.kind() != expected_kind
                            || !services.descriptors().is_live(*descriptor)
                    })
                {
                    return Err(RenderError::Backend(
                        "draw contains a stale or incompatible descriptor".into(),
                    ));
                }
                if kind != ShaderBindingKind::Sampler {
                    let expected_view = match kind {
                        ShaderBindingKind::ConstantBuffer => NativeViewKind::ConstantBuffer,
                        ShaderBindingKind::SampledResource => NativeViewKind::ShaderResource,
                        ShaderBindingKind::StorageResource => NativeViewKind::UnorderedAccess,
                        ShaderBindingKind::Sampler => unreachable!(),
                    };
                    if binding.descriptors.iter().any(|descriptor| {
                        self.descriptor_owner(*descriptor)
                            .is_none_or(|(_, actual_view)| actual_view != expected_view)
                    }) {
                        return Err(RenderError::Backend(
                            "draw binds a descriptor with the wrong view type".into(),
                        ));
                    }
                }
                uses_samplers |= kind == ShaderBindingKind::Sampler;
            }

            let layout_slots = draw
                .description
                .vertex_attributes
                .iter()
                .map(|attribute| attribute.input_slot)
                .collect::<HashSet<_>>();
            let bound_slots =
                draw.vertex_buffers.iter().map(|binding| binding.slot).collect::<HashSet<_>>();
            if layout_slots != bound_slots {
                return Err(RenderError::Backend(
                    "vertex buffer slots must exactly match the pipeline input layout".into(),
                ));
            }
            for attribute in &draw.description.vertex_attributes {
                let binding = draw
                    .vertex_buffers
                    .iter()
                    .find(|binding| binding.slot == attribute.input_slot)
                    .expect("validated vertex input slot");
                if attribute
                    .byte_offset
                    .checked_add(attribute.format.byte_size())
                    .is_none_or(|end| end > binding.stride)
                {
                    return Err(RenderError::Backend(
                        "vertex attribute extends beyond its bound stream stride".into(),
                    ));
                }
            }
            for binding in &draw.vertex_buffers {
                let slot = self.validate_resource(binding.buffer)?;
                let end = binding
                    .offset
                    .checked_add(u64::from(binding.size))
                    .ok_or_else(|| RenderError::Backend("vertex buffer range overflow".into()))?;
                if slot.kind != ResourceKind::Buffer
                    || !slot.buffer_usage.is_some_and(|usage| usage.contains(BufferUsage::VERTEX))
                    || end > slot.byte_size
                    || binding.stride == 0
                    || binding.size < binding.stride
                    || !matches!(
                        slot.state,
                        D3D12_RESOURCE_STATE_VERTEX_AND_CONSTANT_BUFFER
                            | D3D12_RESOURCE_STATE_GENERIC_READ
                    )
                {
                    return Err(RenderError::Backend(
                        "vertex buffer binding is invalid for this pass".into(),
                    ));
                }
                let instance_step = draw
                    .description
                    .vertex_attributes
                    .iter()
                    .find(|attribute| attribute.input_slot == binding.slot)
                    .filter(|attribute| attribute.per_instance)
                    .map(|attribute| attribute.instance_step_rate);
                let required_elements =
                    if let (Some(indirect), Some(step_rate)) = (draw.indirect, instance_step) {
                        Some(indirect.maximum_instances.div_ceil(step_rate))
                    } else if draw.indirect.is_some() {
                        None
                    } else {
                        match (draw.call, instance_step) {
                            (DrawCall::Vertices { vertex_count, first_vertex, .. }, None) => {
                                Some(first_vertex.checked_add(vertex_count).ok_or_else(|| {
                                    RenderError::Backend("vertex draw range overflow".into())
                                })?)
                            }
                            (
                                DrawCall::Vertices { instance_count, first_instance, .. }
                                | DrawCall::Indexed { instance_count, first_instance, .. },
                                Some(step_rate),
                            ) => Some(
                                first_instance
                                    .checked_add(instance_count)
                                    .ok_or_else(|| {
                                        RenderError::Backend("instance draw range overflow".into())
                                    })?
                                    .div_ceil(step_rate),
                            ),
                            (DrawCall::Indexed { .. }, None) => None,
                        }
                    };
                if required_elements.is_some_and(|elements| {
                    u64::from(elements) * u64::from(binding.stride) > u64::from(binding.size)
                }) {
                    return Err(RenderError::Backend("draw exceeds a vertex buffer view".into()));
                }
            }
            match (draw.call, draw.index_buffer) {
                (DrawCall::Indexed { index_count, first_index, .. }, Some(index)) => {
                    let slot = self.validate_resource(index.buffer)?;
                    let end = index.offset.checked_add(u64::from(index.size)).ok_or_else(|| {
                        RenderError::Backend("index buffer range overflow".into())
                    })?;
                    let element_size = match index.format {
                        IndexFormat::Uint16 => 2_u32,
                        IndexFormat::Uint32 => 4_u32,
                    };
                    let draw_end = first_index
                        .checked_add(index_count)
                        .ok_or_else(|| RenderError::Backend("index range overflow".into()))?;
                    if slot.kind != ResourceKind::Buffer
                        || !slot
                            .buffer_usage
                            .is_some_and(|usage| usage.contains(BufferUsage::INDEX))
                        || end > slot.byte_size
                        || !index.size.is_multiple_of(element_size)
                        || u64::from(draw_end) * u64::from(element_size) > u64::from(index.size)
                        || index.offset % u64::from(element_size) != 0
                        || !matches!(
                            slot.state,
                            D3D12_RESOURCE_STATE_INDEX_BUFFER | D3D12_RESOURCE_STATE_GENERIC_READ
                        )
                    {
                        return Err(RenderError::Backend(
                            "index buffer binding or draw range is invalid".into(),
                        ));
                    }
                }
                (DrawCall::Indexed { .. }, None) => {
                    return Err(RenderError::Backend("indexed draw has no index buffer".into()));
                }
                (DrawCall::Vertices { .. }, Some(_)) => {
                    return Err(RenderError::Backend(
                        "non-indexed draw has an index buffer".into(),
                    ));
                }
                (DrawCall::Vertices { .. }, None) => {}
            }
            if let Some(indirect) = draw.indirect {
                let slot = self.validate_resource(indirect.resource)?;
                let index = draw.index_buffer.expect("indirect draws are indexed");
                let index_element_size = match index.format {
                    IndexFormat::Uint16 => 2_u64,
                    IndexFormat::Uint32 => 4_u64,
                };
                let end = indirect
                    .offset
                    .checked_add(u64::from(indirect.maximum_commands) * 20)
                    .ok_or_else(|| {
                        RenderError::Backend("indirect argument range overflow".into())
                    })?;
                if slot.kind != ResourceKind::Buffer
                    || !slot
                        .buffer_usage
                        .is_some_and(|usage| usage.contains(BufferUsage::INDIRECT_ARGUMENT))
                    || end > slot.byte_size
                    || u64::from(indirect.maximum_indices)
                        > u64::from(index.size) / index_element_size
                    || !indirect.offset.is_multiple_of(4)
                    || !matches!(
                        slot.state,
                        D3D12_RESOURCE_STATE_INDIRECT_ARGUMENT | D3D12_RESOURCE_STATE_GENERIC_READ
                    )
                {
                    return Err(RenderError::Backend(
                        "indirect argument buffer, range, usage or state is invalid".into(),
                    ));
                }
            }
            prepared.push((root_signature, pipeline_state, expected_bindings));
        }

        let color_start = unsafe { self.rtv_heap.GetCPUDescriptorHandleForHeapStart() };
        let color_cpus = pass
            .color_attachments
            .iter()
            .map(|attachment| windows::Win32::Graphics::Direct3D12::D3D12_CPU_DESCRIPTOR_HANDLE {
                ptr: color_start.ptr
                    + (BACK_BUFFER_COUNT as usize + attachment.view.index() as usize)
                        * self.rtv_increment as usize,
            })
            .collect::<Vec<_>>();
        let depth_cpu = depth_view.map(|view| {
            let start = unsafe { self.dsv_heap.GetCPUDescriptorHandleForHeapStart() };
            windows::Win32::Graphics::Direct3D12::D3D12_CPU_DESCRIPTOR_HANDLE {
                ptr: start.ptr + view.index() as usize * self.dsv_increment as usize,
            }
        });
        if !self.graph_submission_active && !self.record_only {
            self.wait_idle()?;
        }
        let allocator: ID3D12CommandAllocator =
            unsafe { self.device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT) }
                .map_err(backend_error)?;
        let list: ID3D12GraphicsCommandList = unsafe {
            self.device.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &allocator, None)
        }
        .map_err(backend_error)?;
        for (resource, before) in color_resources.iter().zip(&color_states) {
            if *before != D3D12_RESOURCE_STATE_RENDER_TARGET {
                record_transition(&list, resource, *before, D3D12_RESOURCE_STATE_RENDER_TARGET);
            }
        }
        if let Some((native, before)) = &depth_native
            && *before != windows::Win32::Graphics::Direct3D12::D3D12_RESOURCE_STATE_DEPTH_WRITE
        {
            record_transition(
                &list,
                native,
                *before,
                windows::Win32::Graphics::Direct3D12::D3D12_RESOURCE_STATE_DEPTH_WRITE,
            );
        }
        let mut heaps = vec![Some(self.resource_descriptor_heap.clone())];
        if uses_samplers {
            heaps.push(Some(self.sampler_descriptor_heap.clone()));
        }
        let viewport = pass.viewport.unwrap_or(crate::Viewport::new(
            0.0,
            0.0,
            color_desc.Width as f32,
            color_desc.Height as f32,
        ));
        let viewports = [windows::Win32::Graphics::Direct3D12::D3D12_VIEWPORT {
            TopLeftX: viewport.x,
            TopLeftY: viewport.y,
            Width: viewport.width,
            Height: viewport.height,
            MinDepth: viewport.min_depth,
            MaxDepth: viewport.max_depth,
        }];
        let full_scissor = crate::ScissorRect::new(
            0,
            0,
            i32::try_from(color_desc.Width).map_err(backend_error)?,
            i32::try_from(color_desc.Height).map_err(backend_error)?,
        );
        let scissor = pass.scissor.unwrap_or(full_scissor);
        let scissors = [windows::Win32::Foundation::RECT {
            left: scissor.left,
            top: scissor.top,
            right: scissor.right,
            bottom: scissor.bottom,
        }];
        // SAFETY: pass resources, descriptors, PSOs, and COM references are retained through submission.
        unsafe {
            list.SetDescriptorHeaps(&heaps);
            list.OMSetRenderTargets(
                u32::try_from(color_cpus.len()).map_err(backend_error)?,
                Some(color_cpus.as_ptr()),
                false,
                depth_cpu.as_ref().map(std::ptr::from_ref),
            );
            for (attachment, cpu) in pass.color_attachments.iter().zip(&color_cpus) {
                list.ClearRenderTargetView(*cpu, &attachment.clear_color.0, None);
            }
            if let Some(depth_cpu) = depth_cpu {
                list.ClearDepthStencilView(
                    depth_cpu,
                    windows::Win32::Graphics::Direct3D12::D3D12_CLEAR_FLAG_DEPTH,
                    pass.clear_depth,
                    0,
                    &[],
                );
            }
            list.RSSetViewports(&viewports);
            list.RSSetScissorRects(&scissors);
            for (draw, (root_signature, pipeline_state, expected_bindings)) in
                pass.draws.iter().zip(&prepared)
            {
                list.SetGraphicsRootSignature(root_signature);
                list.SetPipelineState(pipeline_state);
                for (root_index, (kind, space, register, _)) in
                    expected_bindings.iter().copied().enumerate()
                {
                    let binding = draw
                        .bindings
                        .iter()
                        .find(|binding| {
                            (binding.kind, binding.space, binding.register)
                                == (kind, space, register)
                        })
                        .expect("validated reflected binding");
                    let (start, increment) = if kind == ShaderBindingKind::Sampler {
                        (
                            self.sampler_descriptor_heap.GetGPUDescriptorHandleForHeapStart(),
                            self.sampler_descriptor_increment,
                        )
                    } else {
                        (
                            self.resource_descriptor_heap.GetGPUDescriptorHandleForHeapStart(),
                            self.resource_descriptor_increment,
                        )
                    };
                    let gpu = windows::Win32::Graphics::Direct3D12::D3D12_GPU_DESCRIPTOR_HANDLE {
                        ptr: start.ptr
                            + u64::from(binding.descriptors[0].index()) * u64::from(increment),
                    };
                    list.SetGraphicsRootDescriptorTable(
                        u32::try_from(root_index).map_err(backend_error)?,
                        gpu,
                    );
                }
                let topology = match draw.description.topology {
                    PrimitiveTopology::Point => {
                        windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_POINTLIST
                    }
                    PrimitiveTopology::Line => {
                        windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_LINELIST
                    }
                    PrimitiveTopology::Triangle => {
                        windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST
                    }
                };
                list.IASetPrimitiveTopology(topology);
                for binding in &draw.vertex_buffers {
                    let slot =
                        self.validate_resource(binding.buffer).expect("validated vertex buffer");
                    let view = D3D12_VERTEX_BUFFER_VIEW {
                        BufferLocation: slot
                            .resource
                            .as_ref()
                            .expect("validated resource")
                            .GetGPUVirtualAddress()
                            + binding.offset,
                        SizeInBytes: binding.size,
                        StrideInBytes: binding.stride,
                    };
                    list.IASetVertexBuffers(binding.slot, Some(&[view]));
                }
                if let Some(indirect) = draw.indirect {
                    let index = draw.index_buffer.expect("indirect draws are indexed");
                    let index_slot =
                        self.validate_resource(index.buffer).expect("validated index buffer");
                    let index_view = D3D12_INDEX_BUFFER_VIEW {
                        BufferLocation: index_slot
                            .resource
                            .as_ref()
                            .expect("validated resource")
                            .GetGPUVirtualAddress()
                            + index.offset,
                        SizeInBytes: index.size,
                        Format: match index.format {
                            IndexFormat::Uint16 => DXGI_FORMAT_R16_UINT,
                            IndexFormat::Uint32 => DXGI_FORMAT_R32_UINT,
                        },
                    };
                    list.IASetIndexBuffer(Some(&raw const index_view));
                    let arguments = self
                        .validate_resource(indirect.resource)
                        .expect("validated indirect arguments");
                    let arguments = arguments.resource.as_ref().expect("validated resource");
                    list.ExecuteIndirect(
                        &self.indexed_draw_command_signature,
                        indirect.maximum_commands,
                        arguments,
                        indirect.offset,
                        None::<&ID3D12Resource>,
                        0,
                    );
                    continue;
                }
                match (draw.call, draw.index_buffer) {
                    (
                        DrawCall::Vertices {
                            vertex_count,
                            instance_count,
                            first_vertex,
                            first_instance,
                        },
                        None,
                    ) => {
                        list.DrawInstanced(
                            vertex_count,
                            instance_count,
                            first_vertex,
                            first_instance,
                        );
                    }
                    (
                        DrawCall::Indexed {
                            index_count,
                            instance_count,
                            first_index,
                            base_vertex,
                            first_instance,
                        },
                        Some(index),
                    ) => {
                        let slot =
                            self.validate_resource(index.buffer).expect("validated index buffer");
                        let view = D3D12_INDEX_BUFFER_VIEW {
                            BufferLocation: slot
                                .resource
                                .as_ref()
                                .expect("validated resource")
                                .GetGPUVirtualAddress()
                                + index.offset,
                            SizeInBytes: index.size,
                            Format: match index.format {
                                IndexFormat::Uint16 => DXGI_FORMAT_R16_UINT,
                                IndexFormat::Uint32 => DXGI_FORMAT_R32_UINT,
                            },
                        };
                        list.IASetIndexBuffer(Some(&raw const view));
                        list.DrawIndexedInstanced(
                            index_count,
                            instance_count,
                            first_index,
                            base_vertex,
                            first_instance,
                        );
                    }
                    _ => unreachable!("draw/index binding was validated above"),
                }
            }
            for (resource, before) in color_resources.iter().zip(&color_states) {
                if *before != D3D12_RESOURCE_STATE_RENDER_TARGET {
                    record_transition(&list, resource, D3D12_RESOURCE_STATE_RENDER_TARGET, *before);
                }
            }
            if let Some((native, before)) = &depth_native
                && *before != windows::Win32::Graphics::Direct3D12::D3D12_RESOURCE_STATE_DEPTH_WRITE
            {
                record_transition(
                    &list,
                    native,
                    windows::Win32::Graphics::Direct3D12::D3D12_RESOURCE_STATE_DEPTH_WRITE,
                    *before,
                );
            }
            list.Close().map_err(backend_error)?;
        }
        self.submit_pass_command_list(allocator, list)
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn execute_compute_pass(
        &mut self,
        services: &mut RenderServices,
        description: ComputePipelineDesc,
        compute: &CompiledShader,
        bindings: &[PipelineDescriptorTableBinding],
        groups: [u32; 3],
    ) -> Result<(), RenderError> {
        if compute.stage != ShaderStage::Compute || compute.shader_id != description.compute_shader
        {
            return Err(RenderError::Backend("compute shader does not match the pipeline".into()));
        }
        let (root_signature, pipeline_state) = self
            .compute_pipelines
            .get(&description.compute_shader)
            .map(|pipeline| (pipeline.root_signature.clone(), pipeline.pipeline_state.clone()))
            .ok_or_else(|| {
                RenderError::Backend("compute pipeline must be created before dispatch".into())
            })?;
        let (_, _, expected_bindings) = root_signature_parts(&[(compute, 4)])?;
        if bindings.len() != expected_bindings.len() {
            return Err(RenderError::Backend(
                "compute dispatch must bind every reflected descriptor exactly once".into(),
            ));
        }
        let mut transitions = Vec::new();
        for (kind, space, register, count) in expected_bindings.iter().copied() {
            let binding = bindings
                .iter()
                .find(|binding| {
                    (binding.kind, binding.space, binding.register) == (kind, space, register)
                })
                .ok_or_else(|| {
                    RenderError::Backend(format!(
                        "missing compute descriptor at {kind:?} register {register}, space {space}"
                    ))
                })?;
            let descriptor_kind = if kind == ShaderBindingKind::Sampler {
                DescriptorKind::Sampler
            } else {
                DescriptorKind::Resource
            };
            if binding.descriptors.len() != count as usize
                || !is_contiguous_descriptor_range(&binding.descriptors)
                || binding.descriptors.iter().any(|descriptor| {
                    descriptor.kind() != descriptor_kind
                        || !services.descriptors().is_live(*descriptor)
                })
            {
                return Err(RenderError::Backend(
                    "compute descriptor is stale or from the wrong heap".into(),
                ));
            }
            if kind != ShaderBindingKind::Sampler {
                let expected_view = match kind {
                    ShaderBindingKind::ConstantBuffer => NativeViewKind::ConstantBuffer,
                    ShaderBindingKind::SampledResource => NativeViewKind::ShaderResource,
                    ShaderBindingKind::StorageResource => NativeViewKind::UnorderedAccess,
                    ShaderBindingKind::Sampler => unreachable!(),
                };
                if binding.descriptors.iter().any(|descriptor| {
                    self.descriptor_owner(*descriptor)
                        .is_none_or(|(_, actual_view)| actual_view != expected_view)
                }) {
                    return Err(RenderError::Backend(
                        "compute binding uses a descriptor with the wrong view type".into(),
                    ));
                }
            }
            if kind == ShaderBindingKind::StorageResource {
                for descriptor in &binding.descriptors {
                    let (owner, _) = self.descriptor_owner(*descriptor).ok_or_else(|| {
                        RenderError::Backend("storage descriptor has no live resource owner".into())
                    })?;
                    let index = owner.index() as usize;
                    if transitions.iter().any(|(existing, _, _)| *existing == index) {
                        continue;
                    }
                    let slot = self.validate_resource(owner)?;
                    let resource = slot.resource.as_ref().expect("live descriptor owner").clone();
                    transitions.push((index, resource, slot.state));
                }
            }
        }
        if !self.graph_submission_active {
            self.wait_idle()?;
        }
        let allocator: ID3D12CommandAllocator =
            unsafe { self.device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT) }
                .map_err(backend_error)?;
        let list: ID3D12GraphicsCommandList = unsafe {
            self.device.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &allocator, None)
        }
        .map_err(backend_error)?;
        for (_, resource, before) in &transitions {
            if *before
                != windows::Win32::Graphics::Direct3D12::D3D12_RESOURCE_STATE_UNORDERED_ACCESS
            {
                record_transition(
                    &list,
                    resource,
                    *before,
                    windows::Win32::Graphics::Direct3D12::D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
                );
            }
        }
        let mut heaps = vec![Some(self.resource_descriptor_heap.clone())];
        if expected_bindings.iter().any(|(kind, _, _, _)| *kind == ShaderBindingKind::Sampler) {
            heaps.push(Some(self.sampler_descriptor_heap.clone()));
        }
        // SAFETY: pipeline, descriptors and resources have been validated for this device.
        unsafe {
            list.SetDescriptorHeaps(&heaps);
            list.SetComputeRootSignature(&root_signature);
            list.SetPipelineState(&pipeline_state);
            for (root_index, (kind, space, register, _count)) in
                expected_bindings.iter().copied().enumerate()
            {
                let binding = bindings
                    .iter()
                    .find(|binding| {
                        (binding.kind, binding.space, binding.register) == (kind, space, register)
                    })
                    .expect("validated reflected descriptor");
                let (start, increment) = if kind == ShaderBindingKind::Sampler {
                    (
                        self.sampler_descriptor_heap.GetGPUDescriptorHandleForHeapStart(),
                        self.sampler_descriptor_increment,
                    )
                } else {
                    (
                        self.resource_descriptor_heap.GetGPUDescriptorHandleForHeapStart(),
                        self.resource_descriptor_increment,
                    )
                };
                list.SetComputeRootDescriptorTable(
                    u32::try_from(root_index).map_err(backend_error)?,
                    windows::Win32::Graphics::Direct3D12::D3D12_GPU_DESCRIPTOR_HANDLE {
                        ptr: start.ptr
                            + u64::from(binding.descriptors[0].index()) * u64::from(increment),
                    },
                );
            }
            list.Dispatch(groups[0], groups[1], groups[2]);
            for (_, resource, before) in &transitions {
                if *before
                    != windows::Win32::Graphics::Direct3D12::D3D12_RESOURCE_STATE_UNORDERED_ACCESS
                {
                    record_transition(
                        &list,
                        resource,
                        windows::Win32::Graphics::Direct3D12::D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
                        *before,
                    );
                }
            }
            list.Close().map_err(backend_error)?;
        }
        let result = self.submit_pass_command_list(allocator, list);
        if result.is_ok() {
            for (index, _, before) in transitions {
                if let Some(slot) = self.resources.get_mut(index) {
                    slot.state = before;
                }
            }
        }
        result
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn read_texture_pixel(
        &mut self,
        handle: GpuResourceHandle,
        x: u32,
        y: u32,
    ) -> Result<[u8; 4], RenderError> {
        let slot = self.validate_resource(handle)?;
        let texture_width = u32::try_from(slot.desc.Width).map_err(backend_error)?;
        if !matches!(slot.kind, ResourceKind::RenderTarget | ResourceKind::Texture)
            || slot.desc.Format != DXGI_FORMAT_R8G8B8A8_UNORM
            || slot.desc.SampleDesc.Count != 1
            || x >= texture_width
            || y >= slot.desc.Height
        {
            return Err(RenderError::Backend(
                "pixel readback requires an in-bounds single-sample RGBA8 texture".into(),
            ));
        }
        let texture = slot.resource.as_ref().expect("validated render target").clone();
        let state = slot.state;
        let texture_desc = slot.desc;
        let mut footprint = D3D12_PLACED_SUBRESOURCE_FOOTPRINT::default();
        let mut rows = 0;
        let mut row_size = 0u64;
        let mut total_bytes = 0u64;
        // SAFETY: the live texture and subresource zero are valid for this query.
        unsafe {
            self.device.GetCopyableFootprints(
                &raw const texture_desc,
                0,
                1,
                0,
                Some(&raw mut footprint),
                Some(&raw mut rows),
                Some(&raw mut row_size),
                Some(&raw mut total_bytes),
            );
        }
        if rows == 0 || row_size < u64::from(texture_width) * 4 || total_bytes == 0 {
            return Err(RenderError::Backend(
                "D3D12 returned an invalid readback footprint".into(),
            ));
        }
        let heap = D3D12_HEAP_PROPERTIES {
            Type: D3D12_HEAP_TYPE_READBACK,
            CPUPageProperty: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
            MemoryPoolPreference: D3D12_MEMORY_POOL_UNKNOWN,
            CreationNodeMask: 1,
            VisibleNodeMask: 1,
        };
        let readback_desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
            Alignment: 0,
            Width: total_bytes,
            Height: 1,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Format: DXGI_FORMAT_UNKNOWN,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            Flags: D3D12_RESOURCE_FLAG_NONE,
        };
        let mut readback = None;
        // SAFETY: D3D12 readback buffers require a readback heap and COPY_DEST state.
        unsafe {
            self.device.CreateCommittedResource::<ID3D12Resource>(
                &raw const heap,
                D3D12_HEAP_FLAG_NONE,
                &raw const readback_desc,
                D3D12_RESOURCE_STATE_COPY_DEST,
                None,
                &raw mut readback,
            )
        }
        .map_err(backend_error)?;
        let readback = readback.ok_or_else(|| {
            RenderError::Backend("D3D12 returned an empty readback buffer".into())
        })?;
        self.wait_idle()?;
        let allocator: ID3D12CommandAllocator =
            unsafe { self.device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT) }
                .map_err(backend_error)?;
        let list: ID3D12GraphicsCommandList = unsafe {
            self.device.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &allocator, None)
        }
        .map_err(backend_error)?;
        if state != D3D12_RESOURCE_STATE_COPY_SOURCE {
            record_transition(&list, &texture, state, D3D12_RESOURCE_STATE_COPY_SOURCE);
        }
        let mut destination = D3D12_TEXTURE_COPY_LOCATION {
            pResource: ManuallyDrop::new(Some(readback.clone())),
            Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
            Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 { PlacedFootprint: footprint },
        };
        let mut source = D3D12_TEXTURE_COPY_LOCATION {
            pResource: ManuallyDrop::new(Some(texture.clone())),
            Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
            Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 { SubresourceIndex: 0 },
        };
        // SAFETY: footprint was queried for this exact texture description.
        unsafe {
            list.CopyTextureRegion(&raw const destination, 0, 0, 0, &raw const source, None);
            ManuallyDrop::drop(&mut destination.pResource);
            ManuallyDrop::drop(&mut source.pResource);
            if state != D3D12_RESOURCE_STATE_COPY_SOURCE {
                record_transition(&list, &texture, D3D12_RESOURCE_STATE_COPY_SOURCE, state);
            }
            list.Close().map_err(backend_error)?;
        }
        self.execute_and_wait(&list)?;
        let byte_offset = u64::from(footprint.Footprint.RowPitch)
            .checked_mul(u64::from(y))
            .and_then(|offset| offset.checked_add(u64::from(x) * 4))
            .ok_or_else(|| RenderError::Backend("pixel readback offset overflowed".into()))?;
        let begin = usize::try_from(byte_offset).map_err(backend_error)?;
        let end = begin
            .checked_add(4)
            .ok_or_else(|| RenderError::Backend("pixel readback range overflowed".into()))?;
        let mut pixel = [0u8; 4];
        // SAFETY: the offset is within the queried footprint; queue fence completed the copy.
        unsafe {
            let mut mapped = std::ptr::null_mut();
            readback
                .Map(0, Some(&D3D12_RANGE { Begin: begin, End: end }), Some(&raw mut mapped))
                .map_err(backend_error)?;
            std::ptr::copy_nonoverlapping(mapped.cast::<u8>().add(begin), pixel.as_mut_ptr(), 4);
            readback.Unmap(0, Some(&D3D12_RANGE { Begin: 0, End: 0 }));
        }
        Ok(pixel)
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn read_texture_rgba8(
        &mut self,
        handle: GpuResourceHandle,
    ) -> Result<(u32, u32, Vec<u8>), RenderError> {
        let slot = self.validate_resource(handle)?;
        let width = u32::try_from(slot.desc.Width).map_err(backend_error)?;
        let height = slot.desc.Height;
        if !matches!(slot.kind, ResourceKind::RenderTarget | ResourceKind::Texture)
            || slot.desc.Format != DXGI_FORMAT_R8G8B8A8_UNORM
            || slot.desc.SampleDesc.Count != 1
        {
            return Err(RenderError::Backend(
                "RGBA8 readback requires a single-sample RGBA8 texture".into(),
            ));
        }
        let texture = slot.resource.as_ref().expect("validated RGBA8 texture").clone();
        let state = slot.state;
        let texture_desc = slot.desc;
        let mut footprint = D3D12_PLACED_SUBRESOURCE_FOOTPRINT::default();
        let mut rows = 0;
        let mut row_size = 0u64;
        let mut total_bytes = 0u64;
        // SAFETY: the live texture and subresource zero are valid for this query.
        unsafe {
            self.device.GetCopyableFootprints(
                &raw const texture_desc,
                0,
                1,
                0,
                Some(&raw mut footprint),
                Some(&raw mut rows),
                Some(&raw mut row_size),
                Some(&raw mut total_bytes),
            );
        }
        let tight_row_bytes = u64::from(width) * 4;
        if rows != height || row_size < tight_row_bytes || total_bytes == 0 {
            return Err(RenderError::Backend(
                "D3D12 returned an invalid RGBA8 readback footprint".into(),
            ));
        }
        let heap = D3D12_HEAP_PROPERTIES {
            Type: D3D12_HEAP_TYPE_READBACK,
            CPUPageProperty: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
            MemoryPoolPreference: D3D12_MEMORY_POOL_UNKNOWN,
            CreationNodeMask: 1,
            VisibleNodeMask: 1,
        };
        let readback_desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
            Alignment: 0,
            Width: total_bytes,
            Height: 1,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Format: DXGI_FORMAT_UNKNOWN,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            Flags: D3D12_RESOURCE_FLAG_NONE,
        };
        let mut readback = None;
        // SAFETY: D3D12 readback buffers require a readback heap and COPY_DEST state.
        unsafe {
            self.device.CreateCommittedResource::<ID3D12Resource>(
                &raw const heap,
                D3D12_HEAP_FLAG_NONE,
                &raw const readback_desc,
                D3D12_RESOURCE_STATE_COPY_DEST,
                None,
                &raw mut readback,
            )
        }
        .map_err(backend_error)?;
        let readback = readback.ok_or_else(|| {
            RenderError::Backend("D3D12 returned an empty texture readback buffer".into())
        })?;
        self.wait_idle()?;
        let allocator: ID3D12CommandAllocator =
            unsafe { self.device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT) }
                .map_err(backend_error)?;
        let list: ID3D12GraphicsCommandList = unsafe {
            self.device.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &allocator, None)
        }
        .map_err(backend_error)?;
        if state != D3D12_RESOURCE_STATE_COPY_SOURCE {
            record_transition(&list, &texture, state, D3D12_RESOURCE_STATE_COPY_SOURCE);
        }
        let mut destination = D3D12_TEXTURE_COPY_LOCATION {
            pResource: ManuallyDrop::new(Some(readback.clone())),
            Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
            Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 { PlacedFootprint: footprint },
        };
        let mut source = D3D12_TEXTURE_COPY_LOCATION {
            pResource: ManuallyDrop::new(Some(texture.clone())),
            Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
            Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 { SubresourceIndex: 0 },
        };
        // SAFETY: the footprint was queried for this exact texture description.
        unsafe {
            list.CopyTextureRegion(&raw const destination, 0, 0, 0, &raw const source, None);
            ManuallyDrop::drop(&mut destination.pResource);
            ManuallyDrop::drop(&mut source.pResource);
            if state != D3D12_RESOURCE_STATE_COPY_SOURCE {
                record_transition(&list, &texture, D3D12_RESOURCE_STATE_COPY_SOURCE, state);
            }
            list.Close().map_err(backend_error)?;
        }
        self.execute_and_wait(&list)?;

        let total_bytes = usize::try_from(total_bytes).map_err(backend_error)?;
        let row_pitch = usize::try_from(footprint.Footprint.RowPitch).map_err(backend_error)?;
        let tight_row_bytes = usize::try_from(tight_row_bytes).map_err(backend_error)?;
        let pixel_count = usize::try_from(width)
            .ok()
            .and_then(|width| {
                usize::try_from(height).ok().and_then(|height| width.checked_mul(height))
            })
            .ok_or_else(|| RenderError::Backend("RGBA8 pixel count overflowed".into()))?;
        let mut pixels = vec![
            0_u8;
            pixel_count.checked_mul(4).ok_or_else(|| {
                RenderError::Backend("RGBA8 image byte count overflowed".into())
            })?
        ];
        // SAFETY: mapping covers the full placed footprint and the queue fence has completed.
        unsafe {
            let mut mapped = std::ptr::null_mut();
            readback
                .Map(0, Some(&D3D12_RANGE { Begin: 0, End: total_bytes }), Some(&raw mut mapped))
                .map_err(backend_error)?;
            for row in 0..height as usize {
                let source_offset = row * row_pitch;
                let destination_offset = row * tight_row_bytes;
                std::ptr::copy_nonoverlapping(
                    mapped.cast::<u8>().add(source_offset),
                    pixels.as_mut_ptr().add(destination_offset),
                    tight_row_bytes,
                );
            }
            readback.Unmap(0, Some(&D3D12_RANGE { Begin: 0, End: 0 }));
        }
        Ok((width, height, pixels))
    }

    pub(crate) fn apply_graph_barriers(
        &mut self,
        barriers: &[RenderBarrier],
    ) -> Result<(), RenderError> {
        self.apply_graph_barriers_resolved(barriers, &[])
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn apply_graph_barriers_resolved(
        &mut self,
        barriers: &[RenderBarrier],
        transients: &[Option<GpuResourceHandle>],
    ) -> Result<(), RenderError> {
        if barriers.is_empty() {
            return Ok(());
        }
        let mut pending = Vec::with_capacity(barriers.len());
        let mut state_updates = Vec::new();
        for barrier in barriers {
            let handle = match barrier.resource {
                GraphResourceRef::Imported(handle) => handle,
                GraphResourceRef::Transient(id) => {
                    transients.get(id.index() as usize).copied().flatten().ok_or_else(|| {
                        RenderError::Backend("transient graph resource is unresolved".into())
                    })?
                }
            };
            let slot = self.validate_resource(handle)?;
            let resource = slot.resource.as_ref().expect("validated resource").clone();
            match barrier.kind {
                RenderBarrierKind::Transition => {
                    validate_graph_usage(slot, barrier.after)?;
                    let after = if slot.buffer_memory == Some(BufferMemory::Upload) {
                        D3D12_RESOURCE_STATE_GENERIC_READ
                    } else {
                        graph_resource_state(barrier.after)
                    };
                    if slot.state != after {
                        pending.push(PendingGraphBarrier::Transition(resource, slot.state, after));
                        state_updates.push((handle.index() as usize, after));
                    }
                }
                RenderBarrierKind::UavOrdering => {
                    if slot.state != windows::Win32::Graphics::Direct3D12::D3D12_RESOURCE_STATE_UNORDERED_ACCESS {
                        return Err(RenderError::Backend(
                            "graph UAV ordering barrier requires the resource to be in UAV state".into(),
                        ));
                    }
                    pending.push(PendingGraphBarrier::Uav(resource));
                }
                RenderBarrierKind::Aliasing => {
                    let before = barrier
                        .alias_before
                        .map(|reference| match reference {
                            GraphResourceRef::Imported(previous) => {
                                self.validate_resource(previous)
                            }
                            GraphResourceRef::Transient(id) => transients
                                .get(id.index() as usize)
                                .copied()
                                .flatten()
                                .ok_or_else(|| {
                                    RenderError::Backend("aliased transient is unresolved".into())
                                })
                                .and_then(|previous| self.validate_resource(previous)),
                        })
                        .transpose()?;
                    let before = before.and_then(|slot| slot.resource.clone());
                    pending.push(PendingGraphBarrier::Aliasing(before, resource));
                }
            }
        }
        if pending.is_empty() {
            return Ok(());
        }
        self.wait_idle()?;
        let allocator: ID3D12CommandAllocator =
            unsafe { self.device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT) }
                .map_err(backend_error)?;
        let list: ID3D12GraphicsCommandList = unsafe {
            self.device.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &allocator, None)
        }
        .map_err(backend_error)?;
        let mut native_barriers = pending
            .into_iter()
            .map(|barrier| match barrier {
                PendingGraphBarrier::Transition(resource, before, after) => {
                    transition(&resource, before, after)
                }
                PendingGraphBarrier::Uav(resource) => uav_barrier(&resource),
                PendingGraphBarrier::Aliasing(before, after) => D3D12_RESOURCE_BARRIER {
                    Type: D3D12_RESOURCE_BARRIER_TYPE_ALIASING,
                    Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
                    Anonymous: D3D12_RESOURCE_BARRIER_0 {
                        Aliasing: ManuallyDrop::new(D3D12_RESOURCE_ALIASING_BARRIER {
                            pResourceBefore: ManuallyDrop::new(before),
                            pResourceAfter: ManuallyDrop::new(Some(after)),
                        }),
                    },
                },
            })
            .collect::<Vec<_>>();
        // SAFETY: each barrier owns the cloned resource reference in its union
        // arm until ResourceBarrier has copied the native descriptors.
        unsafe {
            list.ResourceBarrier(&native_barriers);
            for barrier in &mut native_barriers {
                match barrier.Type {
                    D3D12_RESOURCE_BARRIER_TYPE_TRANSITION => {
                        let transition = &mut barrier.Anonymous.Transition;
                        drop(ManuallyDrop::take(&mut transition.pResource));
                    }
                    D3D12_RESOURCE_BARRIER_TYPE_UAV => {
                        let uav = &mut barrier.Anonymous.UAV;
                        drop(ManuallyDrop::take(&mut uav.pResource));
                    }
                    D3D12_RESOURCE_BARRIER_TYPE_ALIASING => {
                        let aliasing = &mut barrier.Anonymous.Aliasing;
                        drop(ManuallyDrop::take(&mut aliasing.pResourceBefore));
                        drop(ManuallyDrop::take(&mut aliasing.pResourceAfter));
                    }
                    _ => unreachable!("only transition/UAV/aliasing barriers are constructed"),
                }
            }
            list.Close().map_err(backend_error)?;
        }
        self.execute_and_wait(&list)?;
        for (index, state) in state_updates {
            if let Some(slot) = self.resources.get_mut(index) {
                slot.state = state;
            }
        }
        Ok(())
    }

    /// Record one barrier command list per graph pass concurrently. Allocators
    /// and lists are private to each worker; execution remains ordered on the
    /// renderer's single direct queue.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn record_graph_barriers_parallel(
        &self,
        graph: &crate::CompiledRenderGraph,
        transients: &[Option<GpuResourceHandle>],
        job_system: &nord_jobs::JobSystem,
    ) -> Result<RecordedGraphBarriers, RenderError> {
        let mut planned_states = HashMap::<usize, D3D12_RESOURCE_STATES>::new();
        let mut jobs = Vec::with_capacity(graph.passes().len());
        for (pass_index, pass) in graph.passes().iter().enumerate() {
            let mut barriers = Vec::new();
            let mut updates = Vec::new();
            for barrier in &pass.barriers_before {
                let handle = resolve_graph_resource(barrier.resource, transients)?;
                let slot = self.validate_resource(handle)?;
                let resource = slot.resource.as_ref().expect("validated graph resource").clone();
                let state =
                    planned_states.get(&(handle.index() as usize)).copied().unwrap_or(slot.state);
                match barrier.kind {
                    RenderBarrierKind::Transition => {
                        validate_graph_usage(slot, barrier.after)?;
                        let after = if slot.buffer_memory == Some(BufferMemory::Upload) {
                            D3D12_RESOURCE_STATE_GENERIC_READ
                        } else {
                            graph_resource_state(barrier.after)
                        };
                        if state != after {
                            barriers.push(PendingGraphBarrier::Transition(resource, state, after));
                            planned_states.insert(handle.index() as usize, after);
                            updates.push((handle.index() as usize, after));
                        }
                    }
                    RenderBarrierKind::UavOrdering => {
                        if state != windows::Win32::Graphics::Direct3D12::D3D12_RESOURCE_STATE_UNORDERED_ACCESS {
                            return Err(RenderError::Backend(
                                "graph UAV ordering barrier requires UAV state".into(),
                            ));
                        }
                        barriers.push(PendingGraphBarrier::Uav(resource));
                    }
                    RenderBarrierKind::Aliasing => {
                        let previous = barrier
                            .alias_before
                            .map(|reference| resolve_graph_resource(reference, transients))
                            .transpose()?
                            .map(|previous| self.validate_resource(previous))
                            .transpose()?
                            .and_then(|slot| slot.resource.clone());
                        barriers.push(PendingGraphBarrier::Aliasing(previous, resource));
                    }
                }
            }
            jobs.push((pass_index, barriers, updates));
        }

        let device = self.device.clone();
        let recorded = job_system
            .map_owned(jobs, move |(pass_index, barriers, state_updates)| {
                if barriers.is_empty() {
                    return Ok((pass_index, None));
                }
                let allocator: ID3D12CommandAllocator =
                    unsafe { device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT) }
                        .map_err(backend_error)?;
                let list: ID3D12GraphicsCommandList = unsafe {
                    device.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &allocator, None)
                }
                .map_err(backend_error)?;
                let mut native = barriers
                    .into_iter()
                    .map(|barrier| match barrier {
                        PendingGraphBarrier::Transition(resource, before, after) => {
                            transition(&resource, before, after)
                        }
                        PendingGraphBarrier::Uav(resource) => uav_barrier(&resource),
                        PendingGraphBarrier::Aliasing(before, after) => D3D12_RESOURCE_BARRIER {
                            Type: D3D12_RESOURCE_BARRIER_TYPE_ALIASING,
                            Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
                            Anonymous: D3D12_RESOURCE_BARRIER_0 {
                                Aliasing: ManuallyDrop::new(D3D12_RESOURCE_ALIASING_BARRIER {
                                    pResourceBefore: ManuallyDrop::new(before),
                                    pResourceAfter: ManuallyDrop::new(Some(after)),
                                }),
                            },
                        },
                    })
                    .collect::<Vec<_>>();
                // SAFETY: this pool task owns a unique allocator/list pair and
                // all COM resources in the barrier descriptors stay alive.
                unsafe {
                    list.ResourceBarrier(&native);
                    for barrier in &mut native {
                        match barrier.Type {
                            D3D12_RESOURCE_BARRIER_TYPE_TRANSITION => {
                                let transition = &mut barrier.Anonymous.Transition;
                                drop(ManuallyDrop::take(&mut transition.pResource));
                            }
                            D3D12_RESOURCE_BARRIER_TYPE_UAV => {
                                let uav = &mut barrier.Anonymous.UAV;
                                drop(ManuallyDrop::take(&mut uav.pResource));
                            }
                            D3D12_RESOURCE_BARRIER_TYPE_ALIASING => {
                                let aliasing = &mut barrier.Anonymous.Aliasing;
                                drop(ManuallyDrop::take(&mut aliasing.pResourceBefore));
                                drop(ManuallyDrop::take(&mut aliasing.pResourceAfter));
                            }
                            _ => unreachable!(),
                        }
                    }
                    list.Close().map_err(backend_error)?;
                }
                Ok((
                    pass_index,
                    Some(RecordedBarrierPass { _allocator: allocator, list, state_updates }),
                ))
            })
            .into_iter()
            .collect::<Result<Vec<_>, RenderError>>()?;
        // Preserve each pass's exact slot (including passes with no barriers),
        // regardless of the worker-pool completion order.
        let mut passes =
            std::iter::repeat_with(|| None).take(graph.passes().len()).collect::<Vec<_>>();
        for (pass_index, pass) in recorded {
            passes[pass_index] = pass;
        }
        Ok(RecordedGraphBarriers { passes })
    }

    pub(crate) fn execute_recorded_graph_barriers(
        &mut self,
        recorded: &mut RecordedGraphBarriers,
        pass_index: usize,
    ) -> Result<(), RenderError> {
        let Some(Some(pass)) = recorded.passes.get_mut(pass_index) else { return Ok(()) };
        // SAFETY: list was closed after recording and targets this renderer's direct queue.
        unsafe {
            self.queue.ExecuteCommandLists(&[Some(pass.list.cast().map_err(backend_error)?)]);
        }
        for &(index, state) in &pass.state_updates {
            if let Some(slot) = self.resources.get_mut(index) {
                slot.state = state;
            }
        }
        Ok(())
    }

    pub(crate) fn record_graph_commands_parallel(
        &self,
        services: &RenderServices,
        graph: &crate::CompiledRenderGraph,
        transients: &[Option<GpuResourceHandle>],
        buffers: &[crate::RenderCommandBuffer],
        job_system: &nord_jobs::JobSystem,
    ) -> Result<RecordedGraphCommands, RenderError> {
        for (pass_index, buffer) in buffers.iter().enumerate() {
            let pass = graph
                .passes()
                .iter()
                .find(|pass| pass.id.index() == pass_index)
                .ok_or_else(|| {
                    RenderError::Backend("render command buffer has no compiled graph pass".into())
                })?;
            self.validate_graph_descriptor_accesses(pass, &buffer.commands, transients, services)?;
        }
        let jobs = buffers
            .iter()
            .enumerate()
            .map(|(index, buffer)| {
                let mut renderer = self.recording_snapshot();
                let position = graph.passes().iter().position(|pass| pass.id.index() == index);
                if let Some(position) = position {
                    for pass in graph.passes().iter().take(position + 1) {
                        for barrier in &pass.barriers_before {
                            let handle = resolve_graph_resource(barrier.resource, transients)?;
                            if let Some(slot) = renderer.resources.get_mut(handle.index() as usize)
                            {
                                match barrier.kind {
                                    RenderBarrierKind::Transition => {
                                        slot.state =
                                            if slot.buffer_memory == Some(BufferMemory::Upload) {
                                                D3D12_RESOURCE_STATE_GENERIC_READ
                                            } else {
                                                graph_resource_state(barrier.after)
                                            };
                                    }
                                    RenderBarrierKind::Aliasing => {
                                        slot.state = D3D12_RESOURCE_STATE_COMMON;
                                    }
                                    RenderBarrierKind::UavOrdering => {}
                                }
                            }
                        }
                    }
                }
                Ok((
                    index,
                    SendRecordingSnapshot(renderer),
                    services.recording_snapshot(),
                    buffer.commands.clone(),
                ))
            })
            .collect::<Result<Vec<_>, RenderError>>()?;
        if jobs.is_empty() {
            return Ok(RecordedGraphCommands { passes: Vec::new() });
        }
        let recorded = job_system
            .map_owned(jobs, |(index, renderer, services, commands)| {
                renderer.record_commands(&commands, services).map(|recorded| (index, recorded))
            })
            .into_iter()
            .collect::<Result<Vec<_>, RenderError>>()?;
        let mut passes = std::iter::repeat_with(Vec::new).take(buffers.len()).collect::<Vec<_>>();
        for (index, command_lists) in recorded {
            passes[index] = command_lists;
        }
        Ok(RecordedGraphCommands { passes })
    }

    pub(crate) fn execute_recorded_graph_commands(
        &mut self,
        recorded: &mut RecordedGraphCommands,
        pass_index: usize,
    ) -> Result<(), RenderError> {
        let Some(commands) = recorded.passes.get_mut(pass_index) else { return Ok(()) };
        for (allocator, list) in std::mem::take(commands) {
            // SAFETY: the list is closed, device-owned, and submitted in graph order.
            unsafe {
                self.queue.ExecuteCommandLists(&[Some(list.cast().map_err(backend_error)?)]);
            }
            self.inflight_pass_lists.push((allocator, list));
        }
        Ok(())
    }

    pub(crate) fn trim_pipeline_cache(
        &mut self,
        services: &mut RenderServices,
        minimum_frame: u64,
    ) -> Result<usize, RenderError> {
        self.wait_idle()?;
        let retired = services.pipelines().retain_since(minimum_frame);
        let retired_compute = services.pipelines().retain_compute_since(minimum_frame);
        let count = retired.len() + retired_compute.len();
        for key in retired {
            self.pipelines.remove(&key);
        }
        for shader in retired_compute {
            self.compute_pipelines.remove(&shader);
        }
        Ok(count)
    }

    fn validate_resource(
        &self,
        handle: GpuResourceHandle,
    ) -> Result<&NativeResourceSlot, RenderError> {
        let slot = self
            .resources
            .get(handle.index() as usize)
            .ok_or_else(|| RenderError::Backend("GPU resource handle index is invalid".into()))?;
        if slot.generation != handle.generation()
            || slot.kind != handle.kind()
            || slot.resource.is_none()
        {
            return Err(RenderError::Backend(
                "GPU resource handle is stale or has the wrong kind".into(),
            ));
        }
        Ok(slot)
    }

    fn descriptor_owner(
        &self,
        descriptor: DescriptorHandle,
    ) -> Option<(GpuResourceHandle, NativeViewKind)> {
        self.resources.iter().enumerate().find_map(|(index, slot)| {
            slot.resource.as_ref()?;
            let (_, view) = slot.descriptors.iter().find(|(owned, _)| *owned == descriptor)?;
            Some((
                GpuResourceHandle::new(u32::try_from(index).ok()?, slot.generation, slot.kind),
                *view,
            ))
        })
    }

    fn validate_graph_descriptor_accesses(
        &self,
        pass: &crate::CompiledRenderPass,
        commands: &[crate::render_graph::EncodedRenderCommand],
        transients: &[Option<GpuResourceHandle>],
        services: &RenderServices,
    ) -> Result<(), RenderError> {
        let validate_binding =
            |binding: &PipelineDescriptorTableBinding| -> Result<(), RenderError> {
                if binding.descriptors.is_empty()
                    || !is_contiguous_descriptor_range(&binding.descriptors)
                {
                    return Err(RenderError::Backend(
                        "descriptor table must be a non-empty contiguous range".into(),
                    ));
                }
                for descriptor in &binding.descriptors {
                    if binding.kind == ShaderBindingKind::Sampler {
                        if descriptor.kind() != DescriptorKind::Sampler
                            || !services.descriptor_is_live(*descriptor)
                        {
                            return Err(RenderError::Backend(
                                "sampler descriptor is stale or invalid".into(),
                            ));
                        }
                        continue;
                    }
                    let Some((owner, view)) = self.descriptor_owner(*descriptor) else {
                        return Err(RenderError::Backend(
                            "shader descriptor has no live GPU resource owner".into(),
                        ));
                    };
                    let (expected_view, expected_usages): (NativeViewKind, &[ResourceUsage]) =
                        match binding.kind {
                            ShaderBindingKind::ConstantBuffer => (
                                NativeViewKind::ConstantBuffer,
                                &[ResourceUsage::ConstantBufferRead],
                            ),
                            ShaderBindingKind::SampledResource => {
                                (NativeViewKind::ShaderResource, &[ResourceUsage::SampledRead])
                            }
                            ShaderBindingKind::StorageResource => (
                                NativeViewKind::UnorderedAccess,
                                &[
                                    ResourceUsage::StorageRead,
                                    ResourceUsage::StorageWrite,
                                    ResourceUsage::StorageReadWrite,
                                ],
                            ),
                            ShaderBindingKind::Sampler => unreachable!(),
                        };
                    if view != expected_view {
                        return Err(RenderError::Backend(
                            "shader binding uses a descriptor with the wrong view type".into(),
                        ));
                    }
                    let resource = transients
                        .iter()
                        .position(|handle| *handle == Some(owner))
                        .and_then(|index| u32::try_from(index).ok())
                        .map_or(GraphResourceRef::Imported(owner), |index| {
                            GraphResourceRef::Transient(crate::TransientTextureId::new(index))
                        });
                    if !pass.accesses.iter().any(|access| {
                        access.resource == resource && expected_usages.contains(&access.usage)
                    }) {
                        return Err(RenderError::Backend(format!(
                            "pass {:?} binds a resource without declaring its compatible graph usage",
                            pass.name
                        )));
                    }
                }
                Ok(())
            };

        for command in commands {
            match command {
                crate::render_graph::EncodedRenderCommand::Graphics(graphics) => {
                    for draw in &graphics.draws {
                        for binding in &draw.bindings {
                            validate_binding(binding)?;
                        }
                    }
                }
                crate::render_graph::EncodedRenderCommand::Compute { bindings, .. }
                | crate::render_graph::EncodedRenderCommand::ComputeTransientUav {
                    bindings, ..
                } => {
                    for binding in bindings {
                        validate_binding(binding)?;
                    }
                }
            }
        }
        Ok(())
    }

    pub(crate) fn write_buffer(
        &mut self,
        handle: GpuResourceHandle,
        offset: u64,
        bytes: &[u8],
    ) -> Result<(), RenderError> {
        let Some(slot) = self.resources.get(handle.index() as usize) else {
            return Err(RenderError::Backend("GPU buffer handle index is invalid".into()));
        };
        if slot.generation != handle.generation()
            || slot.kind != ResourceKind::Buffer
            || slot.buffer_memory != Some(BufferMemory::Upload)
        {
            return Err(RenderError::Backend(
                "buffer is stale, not CPU-visible, or has the wrong kind".into(),
            ));
        }
        let end = offset
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| RenderError::Backend("buffer write range overflowed".into()))?;
        if end > slot.byte_size {
            return Err(RenderError::Backend("buffer write exceeds the resource bounds".into()));
        }
        let resource = slot
            .resource
            .as_ref()
            .ok_or_else(|| RenderError::Backend("GPU buffer was already destroyed".into()))?;
        // SAFETY: range is bounds-checked, this is an upload-heap resource and
        // caller synchronizes CPU writes against frames consuming that range.
        unsafe {
            let mut mapped = std::ptr::null_mut();
            resource
                .Map(0, Some(&D3D12_RANGE { Begin: 0, End: 0 }), Some(&raw mut mapped))
                .map_err(backend_error)?;
            let start = usize::try_from(offset).map_err(backend_error)?;
            let end = usize::try_from(end).map_err(backend_error)?;
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                mapped.cast::<u8>().add(start),
                bytes.len(),
            );
            resource.Unmap(0, Some(&D3D12_RANGE { Begin: start, End: end }));
        }
        Ok(())
    }

    pub(crate) fn upload_buffer(
        &mut self,
        handle: GpuResourceHandle,
        destination_offset: u64,
        bytes: &[u8],
    ) -> Result<(), RenderError> {
        let slot = self.validate_resource(handle)?;
        if slot.kind != ResourceKind::Buffer
            || slot.buffer_memory != Some(BufferMemory::DeviceLocal)
        {
            return Err(RenderError::Backend(
                "staged buffer uploads require a device-local buffer".into(),
            ));
        }
        if !destination_offset.is_multiple_of(4) || !bytes.len().is_multiple_of(4) {
            return Err(RenderError::Backend(
                "D3D12 buffer copy offset and size must be multiples of four bytes".into(),
            ));
        }
        let end = destination_offset
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| RenderError::Backend("buffer upload range overflowed".into()))?;
        if end > slot.byte_size {
            return Err(RenderError::Backend("buffer upload exceeds the resource bounds".into()));
        }
        if bytes.is_empty() {
            return Ok(());
        }
        let resource = slot.resource.as_ref().expect("validated live resource").clone();
        let before = slot.state;
        self.wait_idle()?;
        let mut staging = MappedUploadArena::new(&self.device, bytes.len().max(4))?;
        let source = staging.allocate(bytes.len(), 4).ok_or_else(|| {
            RenderError::Backend("buffer upload staging allocation failed".into())
        })?;
        // SAFETY: staging memory is mapped and the requested slice is in bounds.
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), source.as_ptr(), bytes.len());
        }
        let allocator: ID3D12CommandAllocator =
            unsafe { self.device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT) }
                .map_err(backend_error)?;
        let command_list: ID3D12GraphicsCommandList = unsafe {
            self.device.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &allocator, None)
        }
        .map_err(backend_error)?;
        if before != D3D12_RESOURCE_STATE_COPY_DEST {
            record_transition(&command_list, &resource, before, D3D12_RESOURCE_STATE_COPY_DEST);
        }
        // SAFETY: both resources are live, source bytes are in the staging
        // upload resource, and the destination range was bounds-checked.
        unsafe {
            command_list.CopyBufferRegion(
                &resource,
                destination_offset,
                &staging.resource,
                source.as_ptr().addr() as u64 - staging.mapped.as_ptr().addr() as u64,
                bytes.len() as u64,
            );
        }
        if before != D3D12_RESOURCE_STATE_COPY_DEST {
            record_transition(&command_list, &resource, D3D12_RESOURCE_STATE_COPY_DEST, before);
        }
        unsafe { command_list.Close() }.map_err(backend_error)?;
        self.execute_and_wait(&command_list)?;
        Ok(())
    }

    pub(crate) fn read_buffer(
        &mut self,
        handle: GpuResourceHandle,
        offset: u64,
        length: usize,
    ) -> Result<Vec<u8>, RenderError> {
        self.wait_idle()?;
        let slot = self.validate_resource(handle)?;
        if slot.kind != ResourceKind::Buffer || slot.buffer_memory != Some(BufferMemory::Readback) {
            return Err(RenderError::Backend("CPU reads require a GPU readback buffer".into()));
        }
        let end = offset
            .checked_add(u64::try_from(length).map_err(backend_error)?)
            .ok_or_else(|| RenderError::Backend("buffer read range overflowed".into()))?;
        if end > slot.byte_size {
            return Err(RenderError::Backend("buffer read exceeds the resource bounds".into()));
        }
        if length == 0 {
            return Ok(Vec::new());
        }
        let resource = slot.resource.as_ref().expect("validated live resource").clone();
        let begin = usize::try_from(offset).map_err(backend_error)?;
        let end = usize::try_from(end).map_err(backend_error)?;
        let mut bytes = vec![0; length];
        // SAFETY: the read range is within a readback heap resource and queue
        // idle guarantees prior GPU writes have completed.
        unsafe {
            let mut mapped = std::ptr::null_mut();
            resource
                .Map(0, Some(&D3D12_RANGE { Begin: begin, End: end }), Some(&raw mut mapped))
                .map_err(backend_error)?;
            std::ptr::copy_nonoverlapping(
                mapped.cast::<u8>().add(begin),
                bytes.as_mut_ptr(),
                length,
            );
            resource.Unmap(0, Some(&D3D12_RANGE { Begin: 0, End: 0 }));
        }
        Ok(bytes)
    }

    pub(crate) fn copy_buffer_to_readback(
        &mut self,
        source: GpuResourceHandle,
        source_offset: u64,
        destination: GpuResourceHandle,
        destination_offset: u64,
        length: u64,
    ) -> Result<(), RenderError> {
        let source_slot = self.validate_resource(source)?;
        let destination_slot = self.validate_resource(destination)?;
        if source_slot.kind != ResourceKind::Buffer
            || source_slot.buffer_memory != Some(BufferMemory::DeviceLocal)
            || !source_slot
                .buffer_usage
                .is_some_and(|usage| usage.contains(BufferUsage::COPY_SOURCE))
        {
            return Err(RenderError::Backend(
                "copy source must be a device-local buffer with COPY_SOURCE usage".into(),
            ));
        }
        if destination_slot.kind != ResourceKind::Buffer
            || destination_slot.buffer_memory != Some(BufferMemory::Readback)
            || !destination_slot
                .buffer_usage
                .is_some_and(|usage| usage.contains(BufferUsage::COPY_DESTINATION))
        {
            return Err(RenderError::Backend(
                "copy destination must be a readback buffer with COPY_DESTINATION usage".into(),
            ));
        }
        if !source_offset.is_multiple_of(4)
            || !destination_offset.is_multiple_of(4)
            || !length.is_multiple_of(4)
        {
            return Err(RenderError::Backend(
                "D3D12 buffer-copy offsets and length must be multiples of four bytes".into(),
            ));
        }
        let source_end = source_offset
            .checked_add(length)
            .ok_or_else(|| RenderError::Backend("buffer-copy source range overflowed".into()))?;
        let destination_end = destination_offset.checked_add(length).ok_or_else(|| {
            RenderError::Backend("buffer-copy destination range overflowed".into())
        })?;
        if source_end > source_slot.byte_size || destination_end > destination_slot.byte_size {
            return Err(RenderError::Backend("buffer-copy range exceeds resource bounds".into()));
        }
        if length == 0 {
            return Ok(());
        }

        let source_resource = source_slot.resource.as_ref().expect("validated live source").clone();
        let destination_resource =
            destination_slot.resource.as_ref().expect("validated live destination").clone();
        let source_state = source_slot.state;
        self.wait_idle()?;
        let allocator: ID3D12CommandAllocator =
            unsafe { self.device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT) }
                .map_err(backend_error)?;
        let command_list: ID3D12GraphicsCommandList = unsafe {
            self.device.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &allocator, None)
        }
        .map_err(backend_error)?;
        if source_state != D3D12_RESOURCE_STATE_COPY_SOURCE {
            record_transition(
                &command_list,
                &source_resource,
                source_state,
                D3D12_RESOURCE_STATE_COPY_SOURCE,
            );
        }
        unsafe {
            command_list.CopyBufferRegion(
                &destination_resource,
                destination_offset,
                &source_resource,
                source_offset,
                length,
            );
        }
        if source_state != D3D12_RESOURCE_STATE_COPY_SOURCE {
            record_transition(
                &command_list,
                &source_resource,
                D3D12_RESOURCE_STATE_COPY_SOURCE,
                source_state,
            );
        }
        unsafe { command_list.Close() }.map_err(backend_error)?;
        self.execute_and_wait(&command_list)
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn upload_texture(
        &mut self,
        handle: GpuResourceHandle,
        mip_level: u16,
        array_layer: u16,
        source_row_pitch: usize,
        bytes: &[u8],
    ) -> Result<(), RenderError> {
        let slot = self.validate_resource(handle)?;
        if !matches!(slot.desc.Dimension, D3D12_RESOURCE_DIMENSION_TEXTURE2D)
            || slot.desc.SampleDesc.Count != 1
            || mip_level >= slot.desc.MipLevels
            || array_layer >= slot.desc.DepthOrArraySize
        {
            return Err(RenderError::Backend(
                "texture upload subresource or sample count is unsupported".into(),
            ));
        }
        let resource = slot.resource.as_ref().expect("validated live resource").clone();
        let before = slot.state;
        let subresource = u32::from(mip_level)
            .checked_add(u32::from(array_layer) * u32::from(slot.desc.MipLevels))
            .ok_or_else(|| RenderError::Backend("texture subresource index overflowed".into()))?;
        let mut footprint = D3D12_PLACED_SUBRESOURCE_FOOTPRINT::default();
        let mut rows = 0;
        let mut row_size = 0u64;
        let mut total_bytes = 0u64;
        // SAFETY: the resource description and subresource index were validated.
        unsafe {
            self.device.GetCopyableFootprints(
                &raw const slot.desc,
                subresource,
                1,
                0,
                Some(&raw mut footprint),
                Some(&raw mut rows),
                Some(&raw mut row_size),
                Some(&raw mut total_bytes),
            );
        }
        let source_pitch = u64::try_from(source_row_pitch).map_err(backend_error)?;
        if source_pitch < row_size || rows == 0 || total_bytes == 0 {
            return Err(RenderError::Backend(
                "texture source row pitch or subresource layout is invalid".into(),
            ));
        }
        let required = source_pitch
            .checked_mul(u64::from(rows - 1))
            .and_then(|prefix| prefix.checked_add(row_size))
            .ok_or_else(|| RenderError::Backend("texture upload byte count overflowed".into()))?;
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) < required {
            return Err(RenderError::Backend(
                "texture upload source does not contain all rows".into(),
            ));
        }
        self.wait_idle()?;
        let staging_size = usize::try_from(total_bytes).map_err(backend_error)?;
        let mut staging = MappedUploadArena::new(&self.device, staging_size)?;
        let staging_bytes = staging.allocate(staging_size, 512).ok_or_else(|| {
            RenderError::Backend("texture upload staging allocation failed".into())
        })?;
        // SAFETY: footprint rows and the source byte span have both been checked.
        unsafe {
            for row in 0..rows as usize {
                let source_offset = row
                    .checked_mul(source_row_pitch)
                    .ok_or_else(|| RenderError::Backend("source row offset overflowed".into()))?;
                let destination_offset = usize::try_from(footprint.Offset)
                    .map_err(backend_error)?
                    .checked_add(
                        row.checked_mul(footprint.Footprint.RowPitch as usize).ok_or_else(
                            || RenderError::Backend("GPU row pitch overflowed".into()),
                        )?,
                    )
                    .ok_or_else(|| {
                        RenderError::Backend("GPU staging row offset overflowed".into())
                    })?;
                let row_size = usize::try_from(row_size).map_err(backend_error)?;
                std::ptr::copy_nonoverlapping(
                    bytes.as_ptr().add(source_offset),
                    staging_bytes.as_ptr().add(destination_offset),
                    row_size,
                );
            }
        }
        let allocator: ID3D12CommandAllocator =
            unsafe { self.device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT) }
                .map_err(backend_error)?;
        let command_list: ID3D12GraphicsCommandList = unsafe {
            self.device.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &allocator, None)
        }
        .map_err(backend_error)?;
        if before != D3D12_RESOURCE_STATE_COPY_DEST {
            record_transition(&command_list, &resource, before, D3D12_RESOURCE_STATE_COPY_DEST);
        }
        let mut destination = D3D12_TEXTURE_COPY_LOCATION {
            pResource: ManuallyDrop::new(Some(resource.clone())),
            Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
            Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 { SubresourceIndex: subresource },
        };
        let mut source = D3D12_TEXTURE_COPY_LOCATION {
            pResource: ManuallyDrop::new(Some(staging.resource.clone())),
            Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
            Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 { PlacedFootprint: footprint },
        };
        // SAFETY: the source footprint was obtained from D3D12 for this exact
        // resource description and populated with its required row alignment.
        unsafe {
            command_list.CopyTextureRegion(
                &raw const destination,
                0,
                0,
                0,
                &raw const source,
                None,
            );
        }
        unsafe {
            ManuallyDrop::drop(&mut destination.pResource);
            ManuallyDrop::drop(&mut source.pResource);
        }
        if before != D3D12_RESOURCE_STATE_COPY_DEST {
            record_transition(&command_list, &resource, D3D12_RESOURCE_STATE_COPY_DEST, before);
        }
        unsafe { command_list.Close() }.map_err(backend_error)?;
        self.execute_and_wait(&command_list)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn create_placed_resource(
        &mut self,
        services: &mut RenderServices,
        desc: D3D12_RESOURCE_DESC,
        class: AllocationClass,
        initial_state: D3D12_RESOURCE_STATES,
        kind: ResourceKind,
        shader_resource: bool,
        buffer_usage: Option<BufferUsage>,
        buffer_memory: Option<BufferMemory>,
        byte_size: u64,
        shared_allocation: Option<Allocation>,
    ) -> Result<GpuResourceHandle, RenderError> {
        // SAFETY: D3D12 returns the adapter-specific allocation size/alignment
        // needed by CreatePlacedResource; invalid sentinel sizes are rejected.
        let info = unsafe { self.device.GetResourceAllocationInfo(0, &[desc]) };
        if info.SizeInBytes == u64::MAX || info.SizeInBytes == 0 || info.Alignment == 0 {
            return Err(RenderError::Backend(
                "D3D12 returned invalid resource allocation requirements".into(),
            ));
        }
        let owns_allocation = shared_allocation.is_none();
        let allocation = match shared_allocation {
            Some(allocation)
                if allocation.class() == class
                    && allocation.size() >= info.SizeInBytes
                    && allocation.offset() % info.Alignment == 0 =>
            {
                allocation
            }
            Some(_) => {
                return Err(RenderError::Backend(
                    "shared transient allocation does not satisfy resource requirements".into(),
                ));
            }
            None => {
                services.allocator().allocate(class, info.SizeInBytes, info.Alignment).ok_or_else(
                    || RenderError::Backend("GPU suballocator is out of addressable blocks".into()),
                )?
            }
        };
        if let Err(error) = self.ensure_heap(allocation, services) {
            if owns_allocation {
                services.allocator().deallocate(allocation);
            }
            return Err(error);
        }
        let heap = self
            .heaps
            .get(allocation.block() as usize)
            .and_then(Option::as_ref)
            .ok_or_else(|| RenderError::Backend("native GPU heap block is missing".into()))?
            .heap
            .clone();
        let mut resource = None;
        // SAFETY: allocation came from this block's free-range allocator, and
        // its offset/size satisfy the allocation requirements returned above.
        if let Err(error) = unsafe {
            self.device.CreatePlacedResource::<_, ID3D12Resource>(
                &heap,
                allocation.offset(),
                &raw const desc,
                initial_state,
                None,
                &raw mut resource,
            )
        } {
            if owns_allocation {
                services.allocator().deallocate(allocation);
            }
            return Err(RenderError::Backend(format!("CreatePlacedResource failed: {error}")));
        }
        let resource = resource.ok_or_else(|| {
            RenderError::Backend("D3D12 returned an empty placed resource".into())
        })?;
        let index = if let Some(index) = self.free_resource_slots.pop() {
            index
        } else {
            let index = u32::try_from(self.resources.len()).map_err(backend_error)?;
            self.resources.push(NativeResourceSlot {
                generation: NonZeroU32::MIN,
                kind,
                allocation: None,
                resource: None,
                buffer_memory: None,
                shader_resource: false,
                buffer_usage: None,
                byte_size: 0,
                desc,
                descriptors: Vec::new(),
                state: initial_state,
            });
            index
        };
        let slot = &mut self.resources[index as usize];
        slot.kind = kind;
        slot.allocation = owns_allocation.then_some(allocation);
        slot.resource = Some(resource);
        slot.buffer_memory = buffer_memory;
        slot.shader_resource = shader_resource;
        slot.buffer_usage = buffer_usage;
        slot.byte_size = byte_size;
        slot.desc = desc;
        slot.state = initial_state;
        Ok(GpuResourceHandle::new(index, slot.generation, kind))
    }

    fn ensure_heap(
        &mut self,
        allocation: Allocation,
        services: &mut RenderServices,
    ) -> Result<(), RenderError> {
        let index = allocation.block() as usize;
        if let Some(block) = self.heaps.get(index).and_then(Option::as_ref) {
            if block.class != allocation.class() {
                return Err(RenderError::Backend(
                    "GPU heap class does not match allocation class".into(),
                ));
            }
            return Ok(());
        }
        let size = services.allocator.block_size(allocation.block()).ok_or_else(|| {
            RenderError::Backend("GPU allocator block size is unavailable".into())
        })?;
        let segment = match allocation.class() {
            AllocationClass::DeviceLocal
            | AllocationClass::DeviceLocalBuffer
            | AllocationClass::DeviceLocalRenderTarget => GpuMemorySegment::Local,
            AllocationClass::Upload | AllocationClass::Readback => GpuMemorySegment::NonLocal,
        };
        let mut budget = self.memory_budget(segment)?;
        if budget.budget_bytes > 0
            && budget.current_usage_bytes.saturating_add(size) > budget.budget_bytes
        {
            self.trim_memory_pools(services)?;
            budget = self.memory_budget(segment)?;
        }
        if budget.budget_bytes > 0
            && budget.current_usage_bytes.saturating_add(size) > budget.budget_bytes
        {
            return Err(RenderError::MemoryBudgetExceeded {
                budget: budget.budget_bytes,
                current_usage: budget.current_usage_bytes,
                requested: size,
            });
        }
        self.heaps.resize_with(index + 1, || None);
        let class = allocation.class();
        let heap_type = match class {
            AllocationClass::DeviceLocal
            | AllocationClass::DeviceLocalBuffer
            | AllocationClass::DeviceLocalRenderTarget => D3D12_HEAP_TYPE_DEFAULT,
            AllocationClass::Upload => D3D12_HEAP_TYPE_UPLOAD,
            AllocationClass::Readback => D3D12_HEAP_TYPE_READBACK,
        };
        let properties = D3D12_HEAP_PROPERTIES {
            Type: heap_type,
            CPUPageProperty: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
            MemoryPoolPreference: D3D12_MEMORY_POOL_UNKNOWN,
            CreationNodeMask: 1,
            VisibleNodeMask: 1,
        };
        let flags = match class {
            AllocationClass::DeviceLocal => D3D12_HEAP_FLAG_ALLOW_ONLY_NON_RT_DS_TEXTURES,
            AllocationClass::DeviceLocalBuffer => D3D12_HEAP_FLAG_ALLOW_ONLY_BUFFERS,
            AllocationClass::DeviceLocalRenderTarget => D3D12_HEAP_FLAG_ALLOW_ONLY_RT_DS_TEXTURES,
            AllocationClass::Upload | AllocationClass::Readback => D3D12_HEAP_FLAG_NONE,
        };
        let desc = D3D12_HEAP_DESC {
            SizeInBytes: size,
            Properties: properties,
            Alignment: 0,
            Flags: flags,
        };
        // SAFETY: heap properties and size are valid for a single-node device.
        let mut heap = None;
        unsafe { self.device.CreateHeap::<ID3D12Heap>(&raw const desc, &raw mut heap) }
            .map_err(|error| RenderError::Backend(format!("CreateHeap failed: {error}")))?;
        let heap =
            heap.ok_or_else(|| RenderError::Backend("D3D12 returned an empty heap".into()))?;
        self.heaps[index] = Some(NativeHeapBlock { class, heap });
        Ok(())
    }

    pub(crate) fn wait_for_frame(&self) -> Result<(), RenderError> {
        if self.suspended {
            return Ok(());
        }
        // SAFETY: the waitable handle is owned by this renderer and remains
        // open for the entire wait.
        unsafe {
            if WaitForSingleObject(self.frame_latency.0, INFINITE).0 != WAIT_OBJECT_0 {
                return Err(RenderError::Backend("waiting for DXGI frame latency failed".into()));
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn new(
        native_window: RawWindowHandle,
        config: RenderConfig,
    ) -> Result<Self, RenderError> {
        let RawWindowHandle::Win32(native_window) = native_window else {
            return Err(RenderError::InvalidWindowHandle);
        };
        // SAFETY: all COM calls receive valid descriptors and output types; the
        // native HWND remains owned by nord_platform for this renderer's life.
        unsafe {
            #[cfg(debug_assertions)]
            {
                let mut debug: Option<ID3D12Debug> = None;
                if D3D12GetDebugInterface(&raw mut debug).is_ok()
                    && let Some(debug) = debug
                {
                    debug.EnableDebugLayer();
                }
            }
            let factory: IDXGIFactory4 =
                CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0)).map_err(backend_error)?;
            let (adapter, device) = create_device(&factory)?;
            let adapter: IDXGIAdapter3 = adapter.cast().map_err(backend_error)?;
            let indirect_arguments = [D3D12_INDIRECT_ARGUMENT_DESC {
                Type: D3D12_INDIRECT_ARGUMENT_TYPE_DRAW_INDEXED,
                ..Default::default()
            }];
            let indirect_desc = D3D12_COMMAND_SIGNATURE_DESC {
                ByteStride: 20,
                NumArgumentDescs: 1,
                pArgumentDescs: indirect_arguments.as_ptr(),
                NodeMask: 0,
            };
            let mut indexed_draw_command_signature = None;
            device
                .CreateCommandSignature::<_, ID3D12CommandSignature>(
                    &raw const indirect_desc,
                    None,
                    &raw mut indexed_draw_command_signature,
                )
                .map_err(backend_error)?;
            let indexed_draw_command_signature =
                indexed_draw_command_signature.ok_or_else(|| {
                    RenderError::Backend(
                        "D3D12 returned an empty indexed-draw command signature".into(),
                    )
                })?;
            let queue_desc = D3D12_COMMAND_QUEUE_DESC {
                Type: D3D12_COMMAND_LIST_TYPE_DIRECT,
                ..Default::default()
            };
            let queue: ID3D12CommandQueue =
                device.CreateCommandQueue(&raw const queue_desc).map_err(backend_error)?;
            let (swapchain, rtv_heap, rtv_increment, back_buffers) =
                create_swapchain_and_targets(&factory, &device, &queue, native_window, config)?;
            let resource_descriptor_heap = create_descriptor_heap(
                &device,
                D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
                65_536,
                true,
            )?;
            let sampler_descriptor_heap =
                create_descriptor_heap(&device, D3D12_DESCRIPTOR_HEAP_TYPE_SAMPLER, 2_048, true)?;
            let dsv_heap =
                create_descriptor_heap(&device, D3D12_DESCRIPTOR_HEAP_TYPE_DSV, 2_048, false)?;
            let mut frames = Vec::with_capacity(FRAMES_IN_FLIGHT);
            for _ in 0..FRAMES_IN_FLIGHT {
                frames.push(FrameResources {
                    allocator: device
                        .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)
                        .map_err(backend_error)?,
                    fence_value: 0,
                    upload: MappedUploadArena::new(&device, 64 * 1024)?,
                });
            }
            let command_list: ID3D12GraphicsCommandList = device
                .CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &frames[0].allocator, None)
                .map_err(backend_error)?;
            command_list.Close().map_err(backend_error)?;
            let fence: ID3D12Fence =
                device.CreateFence(0, D3D12_FENCE_FLAG_NONE).map_err(backend_error)?;
            let fence_event =
                OwnedHandle(CreateEventW(None, false, false, None).map_err(backend_error)?, true);
            let swapchain2: IDXGISwapChain2 = swapchain.cast().map_err(backend_error)?;
            let frame_latency = swapchain2.GetFrameLatencyWaitableObject();
            if frame_latency.0.is_null() {
                return Err(RenderError::Backend(
                    "DXGI did not provide its frame-latency waitable object".into(),
                ));
            }
            let frame_latency = OwnedHandle(frame_latency, true);
            let resource_descriptor_increment =
                device.GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV);
            let sampler_descriptor_increment =
                device.GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_SAMPLER);
            let dsv_increment =
                device.GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_DSV);
            let mut renderer = Self {
                adapter,
                device,
                queue,
                swapchain,
                command_list,
                fence,
                fence_event,
                frame_latency,
                rtv_heap,
                rtv_increment,
                back_buffers,
                frames,
                next_frame_slot: 0,
                next_fence_value: 1,
                vertical_sync: config.vertical_sync(),
                width: config.width(),
                height: config.height(),
                suspended: false,
                heaps: Vec::new(),
                resources: Vec::new(),
                free_resource_slots: Vec::new(),
                resource_descriptor_heap,
                sampler_descriptor_heap,
                dsv_heap,
                resource_descriptor_increment,
                sampler_descriptor_increment,
                dsv_increment,
                pipelines: HashMap::new(),
                compute_pipelines: HashMap::new(),
                indexed_draw_command_signature,
                graph_submission_active: false,
                inflight_pass_lists: Vec::new(),
                record_only: false,
            };
            renderer.wait_idle()?;
            Ok(renderer)
        }
    }

    pub(crate) fn render_frame(&mut self, clear_color: ClearColor) -> Result<(), RenderError> {
        if self.suspended {
            return Ok(());
        }
        // SAFETY: each allocator is selected by the live swap-chain index and
        // reset only after its recorded fence value has completed.
        unsafe {
            let buffer_index = self.swapchain.GetCurrentBackBufferIndex() as usize;
            let frame_index = self.next_frame_slot;
            let completed = self.fence.GetCompletedValue();
            if completed == u64::MAX {
                return Err(RenderError::Backend("D3D12 device was removed".into()));
            }
            if self.frames[frame_index].fence_value > completed {
                self.wait_for_fence(self.frames[frame_index].fence_value)?;
            }
            let frame = &mut self.frames[frame_index];
            frame.upload.reset();
            frame.allocator.Reset().map_err(backend_error)?;
            self.command_list.Reset(&frame.allocator, None).map_err(backend_error)?;
            let resource = &self.back_buffers[buffer_index];
            record_transition(
                &self.command_list,
                resource,
                D3D12_RESOURCE_STATE_PRESENT,
                D3D12_RESOURCE_STATE_RENDER_TARGET,
            );
            let start = self.rtv_heap.GetCPUDescriptorHandleForHeapStart();
            let target = D3D12_CPU_DESCRIPTOR_HANDLE {
                ptr: start.ptr + buffer_index * self.rtv_increment as usize,
            };
            let clear_bytes =
                frame.upload.allocate(std::mem::size_of::<[f32; 4]>(), 256).ok_or_else(|| {
                    RenderError::Backend("per-frame upload arena is exhausted".into())
                })?;
            std::ptr::copy_nonoverlapping(
                clear_color.0.as_ptr().cast::<u8>(),
                clear_bytes.as_ptr(),
                std::mem::size_of::<[f32; 4]>(),
            );
            let mut clear_values = [0.0; 4];
            std::ptr::copy_nonoverlapping(
                clear_bytes.as_ptr(),
                clear_values.as_mut_ptr().cast::<u8>(),
                std::mem::size_of::<[f32; 4]>(),
            );
            self.command_list.ClearRenderTargetView(target, &clear_values, None);
            record_transition(
                &self.command_list,
                resource,
                D3D12_RESOURCE_STATE_RENDER_TARGET,
                D3D12_RESOURCE_STATE_PRESENT,
            );
            self.command_list.Close().map_err(backend_error)?;
            self.queue
                .ExecuteCommandLists(&[Some(self.command_list.cast().map_err(backend_error)?)]);
            self.swapchain
                .Present(u32::from(self.vertical_sync), DXGI_PRESENT(0))
                .ok()
                .map_err(backend_error)?;
            let fence_value = self.next_fence_value;
            self.next_fence_value = self
                .next_fence_value
                .checked_add(1)
                .ok_or_else(|| RenderError::Backend("D3D12 fence value space exhausted".into()))?;
            self.queue.Signal(&self.fence, fence_value).map_err(backend_error)?;
            self.frames[frame_index].fence_value = fence_value;
            self.next_frame_slot = (frame_index + 1) % self.frames.len();
            Ok(())
        }
    }

    pub(crate) fn present_texture(
        &mut self,
        source_handle: GpuResourceHandle,
    ) -> Result<(), RenderError> {
        if self.suspended {
            return Ok(());
        }
        let source_slot = self.validate_resource(source_handle)?;
        if source_slot.kind != ResourceKind::RenderTarget
            || source_slot.desc.Dimension != D3D12_RESOURCE_DIMENSION_TEXTURE2D
            || source_slot.desc.Width != u64::from(self.width)
            || source_slot.desc.Height != self.height
            || source_slot.desc.Format != DXGI_FORMAT_R8G8B8A8_UNORM
            || source_slot.desc.SampleDesc.Count != 1
            || source_slot.desc.DepthOrArraySize != 1
            || source_slot.desc.MipLevels != 1
            || !source_slot.desc.Flags.contains(D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET)
            || source_slot.desc.Flags.contains(D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL)
        {
            return Err(RenderError::Backend(
                "present source must be a surface-sized, single-sample RGBA8 color target".into(),
            ));
        }
        let source_resource = source_slot.resource.as_ref().expect("validated live source").clone();
        let source_state = source_slot.state;
        let source_index = source_handle.index() as usize;

        // SAFETY: the frame allocator is reused only after its fence completes,
        // both textures are surface-sized RGBA8 resources, and every state
        // transition brackets the GPU copy before the backbuffer is presented.
        unsafe {
            let buffer_index = self.swapchain.GetCurrentBackBufferIndex() as usize;
            let frame_index = self.next_frame_slot;
            let completed = self.fence.GetCompletedValue();
            if completed == u64::MAX {
                return Err(RenderError::Backend("D3D12 device was removed".into()));
            }
            if self.frames[frame_index].fence_value > completed {
                self.wait_for_fence(self.frames[frame_index].fence_value)?;
            }
            let frame = &mut self.frames[frame_index];
            frame.upload.reset();
            frame.allocator.Reset().map_err(backend_error)?;
            self.command_list.Reset(&frame.allocator, None).map_err(backend_error)?;

            let back_buffer = &self.back_buffers[buffer_index];
            if source_state != D3D12_RESOURCE_STATE_COPY_SOURCE {
                record_transition(
                    &self.command_list,
                    &source_resource,
                    source_state,
                    D3D12_RESOURCE_STATE_COPY_SOURCE,
                );
            }
            record_transition(
                &self.command_list,
                back_buffer,
                D3D12_RESOURCE_STATE_PRESENT,
                D3D12_RESOURCE_STATE_COPY_DEST,
            );
            self.command_list.CopyResource(back_buffer, &source_resource);
            record_transition(
                &self.command_list,
                back_buffer,
                D3D12_RESOURCE_STATE_COPY_DEST,
                D3D12_RESOURCE_STATE_PRESENT,
            );
            if source_state != D3D12_RESOURCE_STATE_COPY_SOURCE {
                record_transition(
                    &self.command_list,
                    &source_resource,
                    D3D12_RESOURCE_STATE_COPY_SOURCE,
                    source_state,
                );
            }
            self.command_list.Close().map_err(backend_error)?;
            self.queue
                .ExecuteCommandLists(&[Some(self.command_list.cast().map_err(backend_error)?)]);
            self.swapchain
                .Present(u32::from(self.vertical_sync), DXGI_PRESENT(0))
                .ok()
                .map_err(backend_error)?;
            let fence_value = self.next_fence_value;
            self.next_fence_value = self
                .next_fence_value
                .checked_add(1)
                .ok_or_else(|| RenderError::Backend("D3D12 fence value space exhausted".into()))?;
            self.queue.Signal(&self.fence, fence_value).map_err(backend_error)?;
            self.frames[frame_index].fence_value = fence_value;
            self.next_frame_slot = (frame_index + 1) % self.frames.len();
        }
        if let Some(slot) = self.resources.get_mut(source_index) {
            slot.state = source_state;
        }
        Ok(())
    }

    pub(crate) fn resize(&mut self, width: u32, height: u32) -> Result<(), RenderError> {
        if width == 0 || height == 0 {
            self.suspended = true;
            return Ok(());
        }
        // SAFETY: the GPU is idle and all references to the previous buffers are
        // dropped before DXGI resizes them.
        unsafe {
            self.wait_idle()?;
            // Keep rendering disabled if DXGI or back-buffer recovery fails.
            self.suspended = true;
            self.back_buffers.clear();
            self.swapchain
                .ResizeBuffers(
                    BACK_BUFFER_COUNT,
                    width,
                    height,
                    DXGI_FORMAT_R8G8B8A8_UNORM,
                    DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT,
                )
                .map_err(backend_error)?;
            let rtv_start = self.rtv_heap.GetCPUDescriptorHandleForHeapStart();
            for index in 0..BACK_BUFFER_COUNT {
                let resource: ID3D12Resource =
                    self.swapchain.GetBuffer(index).map_err(backend_error)?;
                let handle = D3D12_CPU_DESCRIPTOR_HANDLE {
                    ptr: rtv_start.ptr + index as usize * self.rtv_increment as usize,
                };
                self.device.CreateRenderTargetView(&resource, None, handle);
                self.back_buffers.push(resource);
            }
            self.width = width;
            self.height = height;
            self.suspended = false;
            Ok(())
        }
    }

    pub(crate) fn wait_idle(&mut self) -> Result<(), RenderError> {
        // SAFETY: the event belongs to this renderer and remains live through
        // the wait; queue signals are monotonic and checked by the fence.
        unsafe {
            let value = self.next_fence_value;
            self.next_fence_value = self
                .next_fence_value
                .checked_add(1)
                .ok_or_else(|| RenderError::Backend("D3D12 fence value space exhausted".into()))?;
            self.queue.Signal(&self.fence, value).map_err(backend_error)?;
            self.wait_for_fence(value)
        }
    }

    pub(crate) fn begin_graph_submission(&mut self) -> Result<(), RenderError> {
        if self.graph_submission_active || !self.inflight_pass_lists.is_empty() {
            return Err(RenderError::Backend("a graph submission is already active".into()));
        }
        self.graph_submission_active = true;
        Ok(())
    }

    pub(crate) fn end_graph_submission(&mut self) -> Result<(), RenderError> {
        self.graph_submission_active = false;
        self.wait_idle()?;
        self.inflight_pass_lists.clear();
        Ok(())
    }

    fn submit_pass_command_list(
        &mut self,
        allocator: ID3D12CommandAllocator,
        list: ID3D12GraphicsCommandList,
    ) -> Result<(), RenderError> {
        if self.record_only {
            self.inflight_pass_lists.push((allocator, list));
            return Ok(());
        }
        if self.graph_submission_active {
            // Retain both objects until the queue fence retires this list.
            // SAFETY: the list is closed, belongs to this device, and is sent
            // to the renderer's one direct queue in graph order.
            unsafe {
                self.queue.ExecuteCommandLists(&[Some(list.cast().map_err(backend_error)?)]);
            }
            self.inflight_pass_lists.push((allocator, list));
            Ok(())
        } else {
            self.execute_and_wait(&list)
        }
    }

    fn execute_and_wait(
        &mut self,
        command_list: &ID3D12GraphicsCommandList,
    ) -> Result<(), RenderError> {
        // SAFETY: the list is closed, belongs to this D3D12 device, and staging
        // resources remain alive until the queue fence signals completion.
        unsafe {
            let base: windows::Win32::Graphics::Direct3D12::ID3D12CommandList =
                command_list.cast().map_err(backend_error)?;
            self.queue.ExecuteCommandLists(&[Some(base)]);
            let value = self.next_fence_value;
            self.next_fence_value = self
                .next_fence_value
                .checked_add(1)
                .ok_or_else(|| RenderError::Backend("D3D12 fence value space exhausted".into()))?;
            self.queue.Signal(&self.fence, value).map_err(backend_error)?;
            self.wait_for_fence(value)
        }
    }

    unsafe fn wait_for_fence(&self, value: u64) -> Result<(), RenderError> {
        // SAFETY: caller ensures the event/fence belong to this renderer.
        unsafe {
            let completed = self.fence.GetCompletedValue();
            if completed == u64::MAX {
                return Err(RenderError::Backend(
                    "D3D12 device was removed while waiting for a fence".into(),
                ));
            }
            if completed < value {
                self.fence
                    .SetEventOnCompletion(value, self.fence_event.0)
                    .map_err(backend_error)?;
                if WaitForSingleObject(self.fence_event.0, INFINITE).0 != WAIT_OBJECT_0 {
                    return Err(RenderError::Backend(
                        "waiting for the D3D12 GPU fence failed".into(),
                    ));
                }
            }
        }
        Ok(())
    }
}

impl Drop for D3d12Renderer {
    fn drop(&mut self) {
        let _ = self.wait_idle();
    }
}

unsafe fn create_device(
    factory: &IDXGIFactory4,
) -> Result<(IDXGIAdapter1, ID3D12Device), RenderError> {
    // SAFETY: adapter enumeration and D3D12CreateDevice use valid COM outputs.
    unsafe {
        let mut index = 0;
        loop {
            let Ok(adapter) = factory.EnumAdapters1(index) else { break };
            index += 1;
            let desc = adapter.GetDesc1().map_err(backend_error)?;
            if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
                continue;
            }
            let mut device = None;
            let adapter_unknown: windows::core::IUnknown = adapter.cast().map_err(backend_error)?;
            if D3D12CreateDevice(&adapter_unknown, D3D_FEATURE_LEVEL_11_0, &raw mut device).is_ok()
                && let Some(device) = device
            {
                return Ok((adapter, device));
            }
        }
        Err(RenderError::Backend(
            "no hardware adapter supports Direct3D 12 feature level 11.0".into(),
        ))
    }
}

fn buffer_resource_desc(buffer: BufferDesc) -> D3D12_RESOURCE_DESC {
    D3D12_RESOURCE_DESC {
        Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
        Alignment: 0,
        Width: buffer.size(),
        Height: 1,
        DepthOrArraySize: 1,
        MipLevels: 1,
        Format: DXGI_FORMAT_UNKNOWN,
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
        Flags: D3D12_RESOURCE_FLAG_NONE,
    }
}

fn shader_bytecode(shader: &CompiledShader) -> D3D12_SHADER_BYTECODE {
    D3D12_SHADER_BYTECODE {
        pShaderBytecode: shader.bytecode.as_ptr().cast(),
        BytecodeLength: shader.bytecode.len(),
    }
}

fn is_contiguous_descriptor_range(descriptors: &[DescriptorHandle]) -> bool {
    let Some(first) = descriptors.first() else { return false };
    descriptors.iter().enumerate().all(|(offset, descriptor)| {
        descriptor.kind() == first.kind()
            && usize::try_from(descriptor.index()).ok()
                == usize::try_from(first.index()).ok().and_then(|index| index.checked_add(offset))
    })
}

fn root_signature_parts(
    shaders: &[(&CompiledShader, u8)],
) -> Result<RootSignatureParts, RenderError> {
    // A descriptor table has one range per reflected binding. Identical
    // registers shared across stages use ALL visibility and one slot.
    let mut bindings = std::collections::BTreeMap::<(u8, u32, u32), (u32, u8)>::new();
    for (shader, visibility) in shaders {
        for binding in shader.reflection.bindings() {
            let kind = match binding.kind {
                ShaderBindingKind::ConstantBuffer => 0,
                ShaderBindingKind::SampledResource => 1,
                ShaderBindingKind::StorageResource => 2,
                ShaderBindingKind::Sampler => 3,
            };
            if binding.count == 0 || binding.register.checked_add(binding.count).is_none() {
                return Err(RenderError::Backend(
                    "shader reflection returned an invalid binding register range".into(),
                ));
            }
            let entry = bindings
                .entry((kind, binding.space, binding.register))
                .or_insert((binding.count, 0));
            entry.0 = entry.0.max(binding.count);
            entry.1 |= *visibility;
        }
    }
    if bindings.len() > 64 {
        return Err(RenderError::Backend(
            "reflected root signature exceeds D3D12's 64-parameter limit".into(),
        ));
    }
    let mut ranges = Vec::with_capacity(bindings.len());
    let mut parameters = Vec::with_capacity(bindings.len());
    let mut keys = Vec::with_capacity(bindings.len());
    for ((kind, space, register), (count, visibility)) in bindings {
        keys.push((
            match kind {
                0 => ShaderBindingKind::ConstantBuffer,
                1 => ShaderBindingKind::SampledResource,
                2 => ShaderBindingKind::StorageResource,
                _ => ShaderBindingKind::Sampler,
            },
            space,
            register,
            count,
        ));
        let range_type = match kind {
            0 => D3D12_DESCRIPTOR_RANGE_TYPE_CBV,
            1 => D3D12_DESCRIPTOR_RANGE_TYPE_SRV,
            2 => D3D12_DESCRIPTOR_RANGE_TYPE_UAV,
            _ => D3D12_DESCRIPTOR_RANGE_TYPE_SAMPLER,
        };
        ranges.push(D3D12_DESCRIPTOR_RANGE {
            RangeType: range_type,
            NumDescriptors: count,
            BaseShaderRegister: register,
            RegisterSpace: space,
            OffsetInDescriptorsFromTableStart: D3D12_DESCRIPTOR_RANGE_OFFSET_APPEND,
        });
        let range_pointer = ranges.as_ptr().wrapping_add(ranges.len() - 1);
        let shader_visibility = match visibility {
            1 => D3D12_SHADER_VISIBILITY_VERTEX,
            2 => D3D12_SHADER_VISIBILITY_PIXEL,
            _ => D3D12_SHADER_VISIBILITY_ALL,
        };
        parameters.push(D3D12_ROOT_PARAMETER {
            ParameterType: D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE,
            Anonymous: D3D12_ROOT_PARAMETER_0 {
                DescriptorTable: D3D12_ROOT_DESCRIPTOR_TABLE {
                    NumDescriptorRanges: 1,
                    pDescriptorRanges: range_pointer,
                },
            },
            ShaderVisibility: shader_visibility,
        });
    }
    Ok((ranges, parameters, keys))
}

unsafe fn create_descriptor_heap(
    device: &ID3D12Device,
    heap_type: windows::Win32::Graphics::Direct3D12::D3D12_DESCRIPTOR_HEAP_TYPE,
    capacity: u32,
    shader_visible: bool,
) -> Result<ID3D12DescriptorHeap, RenderError> {
    let desc = D3D12_DESCRIPTOR_HEAP_DESC {
        Type: heap_type,
        NumDescriptors: capacity,
        Flags: if shader_visible {
            D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE
        } else {
            D3D12_DESCRIPTOR_HEAP_FLAG_NONE
        },
        NodeMask: 0,
    };
    // SAFETY: descriptor heap type, count and visibility are validated by the
    // renderer's fixed capacities for this one-device backend.
    unsafe { device.CreateDescriptorHeap(&raw const desc) }.map_err(backend_error)
}

fn texture_resource_desc(texture: TextureDesc) -> D3D12_RESOURCE_DESC {
    let mut flags = D3D12_RESOURCE_FLAG_NONE;
    if texture.usage().contains(TextureUsage::STORAGE) {
        flags |= D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS;
    }
    if texture.usage().contains(TextureUsage::COLOR_ATTACHMENT) {
        flags |= D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET;
    }
    if texture.usage().contains(TextureUsage::DEPTH_STENCIL_ATTACHMENT) {
        flags |= D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL;
    }
    D3D12_RESOURCE_DESC {
        Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
        Alignment: 0,
        Width: u64::from(texture.width()),
        Height: texture.height(),
        DepthOrArraySize: texture.array_layers(),
        MipLevels: texture.mip_levels(),
        Format: dxgi_format(texture.format()),
        SampleDesc: DXGI_SAMPLE_DESC { Count: u32::from(texture.samples()), Quality: 0 },
        Layout: D3D12_TEXTURE_LAYOUT_UNKNOWN,
        Flags: flags,
    }
}

const fn dxgi_format(format: TextureFormat) -> DXGI_FORMAT {
    match format {
        TextureFormat::Rgba8Unorm => DXGI_FORMAT_R8G8B8A8_UNORM,
        TextureFormat::Rgba8Srgb => DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
        TextureFormat::Bgra8Unorm => DXGI_FORMAT_B8G8R8A8_UNORM,
        TextureFormat::R16Float => DXGI_FORMAT_R16_FLOAT,
        TextureFormat::R32Float => DXGI_FORMAT_R32_FLOAT,
        TextureFormat::Rg16Float => DXGI_FORMAT_R16G16_FLOAT,
        TextureFormat::Rgba16Float => DXGI_FORMAT_R16G16B16A16_FLOAT,
        TextureFormat::D32Float => DXGI_FORMAT_D32_FLOAT,
        TextureFormat::D24UnormS8 => DXGI_FORMAT_D24_UNORM_S8_UINT,
    }
}

const fn vertex_format(format: VertexFormat) -> DXGI_FORMAT {
    match format {
        VertexFormat::Float => DXGI_FORMAT_R32_FLOAT,
        VertexFormat::Float2 => DXGI_FORMAT_R32G32_FLOAT,
        VertexFormat::Float3 => DXGI_FORMAT_R32G32B32_FLOAT,
        VertexFormat::Float4 => DXGI_FORMAT_R32G32B32A32_FLOAT,
        VertexFormat::Uint8x4 => DXGI_FORMAT_R8G8B8A8_UINT,
        VertexFormat::Uint16x2 => DXGI_FORMAT_R16G16_UINT,
        VertexFormat::Uint32 => DXGI_FORMAT_R32_UINT,
    }
}

const fn address_mode(
    mode: SamplerAddressMode,
) -> windows::Win32::Graphics::Direct3D12::D3D12_TEXTURE_ADDRESS_MODE {
    match mode {
        SamplerAddressMode::Clamp => D3D12_TEXTURE_ADDRESS_MODE_CLAMP,
        SamplerAddressMode::Repeat => D3D12_TEXTURE_ADDRESS_MODE_WRAP,
        SamplerAddressMode::Mirror => D3D12_TEXTURE_ADDRESS_MODE_MIRROR,
    }
}

fn sampler_description(sampler: SamplerDesc) -> D3D12_SAMPLER_DESC {
    D3D12_SAMPLER_DESC {
        Filter: match sampler.filter {
            SamplerFilter::Point => D3D12_FILTER_MIN_MAG_MIP_POINT,
            SamplerFilter::Linear => D3D12_FILTER_MIN_MAG_MIP_LINEAR,
            SamplerFilter::Anisotropic => D3D12_FILTER_ANISOTROPIC,
        },
        AddressU: address_mode(sampler.address_u),
        AddressV: address_mode(sampler.address_v),
        AddressW: address_mode(sampler.address_w),
        MipLODBias: 0.0,
        MaxAnisotropy: u32::from(sampler.max_anisotropy),
        ComparisonFunc: D3D12_COMPARISON_FUNC_ALWAYS,
        BorderColor: [0.0; 4],
        MinLOD: sampler.min_lod,
        MaxLOD: sampler.max_lod,
    }
}

fn uav_description(
    slot: &NativeResourceSlot,
) -> Result<D3D12_UNORDERED_ACCESS_VIEW_DESC, RenderError> {
    if slot.desc.Dimension == D3D12_RESOURCE_DIMENSION_BUFFER {
        if slot.byte_size == 0 || !slot.byte_size.is_multiple_of(4) {
            return Err(RenderError::Backend(
                "raw buffer UAV size must be a non-zero multiple of four bytes".into(),
            ));
        }
        let count = u32::try_from(slot.byte_size / 4).map_err(backend_error)?;
        Ok(D3D12_UNORDERED_ACCESS_VIEW_DESC {
            Format: DXGI_FORMAT_R32_TYPELESS,
            ViewDimension: D3D12_UAV_DIMENSION_BUFFER,
            Anonymous: D3D12_UNORDERED_ACCESS_VIEW_DESC_0 {
                Buffer: D3D12_BUFFER_UAV {
                    NumElements: count,
                    Flags: D3D12_BUFFER_UAV_FLAG_RAW,
                    ..Default::default()
                },
            },
        })
    } else if slot.desc.DepthOrArraySize > 1 {
        Ok(D3D12_UNORDERED_ACCESS_VIEW_DESC {
            Format: slot.desc.Format,
            ViewDimension: D3D12_UAV_DIMENSION_TEXTURE2DARRAY,
            Anonymous: D3D12_UNORDERED_ACCESS_VIEW_DESC_0 {
                Texture2DArray: D3D12_TEX2D_ARRAY_UAV {
                    MipSlice: 0,
                    FirstArraySlice: 0,
                    ArraySize: u32::from(slot.desc.DepthOrArraySize),
                    PlaneSlice: 0,
                },
            },
        })
    } else {
        Ok(D3D12_UNORDERED_ACCESS_VIEW_DESC {
            Format: slot.desc.Format,
            ViewDimension: D3D12_UAV_DIMENSION_TEXTURE2D,
            Anonymous: D3D12_UNORDERED_ACCESS_VIEW_DESC_0 { Texture2D: D3D12_TEX2D_UAV::default() },
        })
    }
}

fn transition(
    resource: &ID3D12Resource,
    before: D3D12_RESOURCE_STATES,
    after: D3D12_RESOURCE_STATES,
) -> D3D12_RESOURCE_BARRIER {
    D3D12_RESOURCE_BARRIER {
        Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
        Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
        Anonymous: D3D12_RESOURCE_BARRIER_0 {
            Transition: std::mem::ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                pResource: std::mem::ManuallyDrop::new(Some(resource.clone())),
                Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                StateBefore: before,
                StateAfter: after,
            }),
        },
    }
}

fn resolve_graph_resource(
    reference: crate::GraphResourceRef,
    transients: &[Option<GpuResourceHandle>],
) -> Result<GpuResourceHandle, RenderError> {
    match reference {
        crate::GraphResourceRef::Imported(handle) => Ok(handle),
        crate::GraphResourceRef::Transient(id) => {
            transients.get(id.index() as usize).copied().flatten().ok_or_else(|| {
                RenderError::Backend("transient graph resource is unresolved".into())
            })
        }
    }
}

fn uav_barrier(resource: &ID3D12Resource) -> D3D12_RESOURCE_BARRIER {
    D3D12_RESOURCE_BARRIER {
        Type: D3D12_RESOURCE_BARRIER_TYPE_UAV,
        Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
        Anonymous: D3D12_RESOURCE_BARRIER_0 {
            UAV: ManuallyDrop::new(D3D12_RESOURCE_UAV_BARRIER {
                pResource: ManuallyDrop::new(Some(resource.clone())),
            }),
        },
    }
}

fn graph_resource_state(usage: ResourceUsage) -> D3D12_RESOURCE_STATES {
    use windows::Win32::Graphics::Direct3D12::{
        D3D12_RESOURCE_STATE_DEPTH_WRITE, D3D12_RESOURCE_STATE_INDEX_BUFFER,
        D3D12_RESOURCE_STATE_INDIRECT_ARGUMENT, D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE,
        D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE, D3D12_RESOURCE_STATE_RENDER_TARGET,
        D3D12_RESOURCE_STATE_UNORDERED_ACCESS, D3D12_RESOURCE_STATE_VERTEX_AND_CONSTANT_BUFFER,
    };
    match usage {
        ResourceUsage::SampledRead => {
            D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE
                | D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE
        }
        ResourceUsage::StorageRead => D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE,
        ResourceUsage::StorageWrite | ResourceUsage::StorageReadWrite => {
            D3D12_RESOURCE_STATE_UNORDERED_ACCESS
        }
        ResourceUsage::ColorAttachmentWrite => D3D12_RESOURCE_STATE_RENDER_TARGET,
        ResourceUsage::DepthAttachmentWrite => D3D12_RESOURCE_STATE_DEPTH_WRITE,
        ResourceUsage::CopySource => D3D12_RESOURCE_STATE_COPY_SOURCE,
        ResourceUsage::CopyDestination => D3D12_RESOURCE_STATE_COPY_DEST,
        ResourceUsage::VertexBufferRead | ResourceUsage::ConstantBufferRead => {
            D3D12_RESOURCE_STATE_VERTEX_AND_CONSTANT_BUFFER
        }
        ResourceUsage::IndexBufferRead => D3D12_RESOURCE_STATE_INDEX_BUFFER,
        ResourceUsage::IndirectArgumentRead => D3D12_RESOURCE_STATE_INDIRECT_ARGUMENT,
    }
}

fn validate_graph_usage(
    slot: &NativeResourceSlot,
    usage: ResourceUsage,
) -> Result<(), RenderError> {
    let flags = slot.desc.Flags;
    let valid = match usage {
        ResourceUsage::ColorAttachmentWrite => {
            flags.contains(D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET)
        }
        ResourceUsage::DepthAttachmentWrite => {
            flags.contains(D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL)
        }
        ResourceUsage::StorageWrite | ResourceUsage::StorageReadWrite => {
            flags.contains(D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS)
        }
        ResourceUsage::StorageRead | ResourceUsage::SampledRead => slot.shader_resource,
        ResourceUsage::VertexBufferRead => {
            slot.buffer_usage.is_some_and(|usage| usage.contains(BufferUsage::VERTEX))
        }
        ResourceUsage::IndexBufferRead => {
            slot.buffer_usage.is_some_and(|usage| usage.contains(BufferUsage::INDEX))
        }
        ResourceUsage::ConstantBufferRead => {
            slot.buffer_usage.is_some_and(|usage| usage.contains(BufferUsage::CONSTANT))
        }
        ResourceUsage::IndirectArgumentRead => {
            slot.buffer_usage.is_some_and(|usage| usage.contains(BufferUsage::INDIRECT_ARGUMENT))
        }
        ResourceUsage::CopySource | ResourceUsage::CopyDestination => true,
    };
    if valid {
        Ok(())
    } else {
        Err(RenderError::Backend(format!(
            "resource is incompatible with render-graph usage {usage:?}"
        )))
    }
}

fn record_transition(
    command_list: &ID3D12GraphicsCommandList,
    resource: &ID3D12Resource,
    before: D3D12_RESOURCE_STATES,
    after: D3D12_RESOURCE_STATES,
) {
    let mut barrier = transition(resource, before, after);
    // SAFETY: transition() initializes the transition union arm and owns one
    // cloned COM reference, which is released after the barrier is recorded.
    unsafe {
        command_list.ResourceBarrier(std::slice::from_ref(&barrier));
        let transition = &mut barrier.Anonymous.Transition;
        drop(ManuallyDrop::take(&mut transition.pResource));
    }
}

fn backend_error(error: impl std::fmt::Display) -> RenderError {
    RenderError::Backend(error.to_string())
}

unsafe fn create_swapchain_and_targets(
    factory: &IDXGIFactory4,
    device: &ID3D12Device,
    queue: &ID3D12CommandQueue,
    native_window: raw_window_handle::Win32WindowHandle,
    config: RenderConfig,
) -> Result<(IDXGISwapChain3, ID3D12DescriptorHeap, u32, Vec<ID3D12Resource>), RenderError> {
    // SAFETY: valid HWND, queue, descriptors, and resource outputs are supplied.
    unsafe {
        let desc = DXGI_SWAP_CHAIN_DESC1 {
            Width: config.width(),
            Height: config.height(),
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            Stereo: false.into(),
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: BACK_BUFFER_COUNT,
            Scaling: DXGI_SCALING_STRETCH,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
            AlphaMode: DXGI_ALPHA_MODE_IGNORE,
            Flags: DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT.0 as u32,
        };
        let swapchain1 = factory
            .CreateSwapChainForHwnd(
                queue,
                HWND(native_window.hwnd.get() as *mut c_void),
                &raw const desc,
                None,
                None,
            )
            .map_err(backend_error)?;
        let swapchain: IDXGISwapChain3 = swapchain1.cast().map_err(backend_error)?;
        let swapchain2: IDXGISwapChain2 = swapchain.cast().map_err(backend_error)?;
        let maximum_frame_latency = u32::try_from(FRAMES_IN_FLIGHT).map_err(backend_error)?;
        swapchain2.SetMaximumFrameLatency(maximum_frame_latency).map_err(backend_error)?;
        let heap_desc = D3D12_DESCRIPTOR_HEAP_DESC {
            Type: D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
            NumDescriptors: BACK_BUFFER_COUNT + RENDER_TARGET_DESCRIPTOR_CAPACITY,
            ..Default::default()
        };
        let heap: ID3D12DescriptorHeap =
            device.CreateDescriptorHeap(&raw const heap_desc).map_err(backend_error)?;
        let increment = device.GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_RTV);
        let start = heap.GetCPUDescriptorHandleForHeapStart();
        let mut buffers = Vec::with_capacity(BACK_BUFFER_COUNT as usize);
        for index in 0..BACK_BUFFER_COUNT {
            let resource: ID3D12Resource = swapchain.GetBuffer(index).map_err(backend_error)?;
            let handle = D3D12_CPU_DESCRIPTOR_HANDLE {
                ptr: start.ptr + index as usize * increment as usize,
            };
            device.CreateRenderTargetView(&resource, None, handle);
            buffers.push(resource);
        }
        Ok((swapchain, heap, increment, buffers))
    }
}
