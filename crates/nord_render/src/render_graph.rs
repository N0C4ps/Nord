//! Deterministic render-graph compilation and a typed callback submission queue.

use crate::{
    ClearColor, CompiledShader, ComputePipelineDesc, DescriptorHandle, GpuResourceHandle,
    GraphicsPipelineDesc, PipelineDescriptorBinding, PipelineDescriptorTableBinding, TextureDesc,
    TextureUsage,
};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt::{Display, Formatter};

/// Stable identity assigned to a graph pass when it is declared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RenderPassId(usize);

impl RenderPassId {
    #[must_use]
    pub const fn index(self) -> usize {
        self.0
    }
}

/// Identity of a transient texture declared in a frame graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TransientTextureId(u32);

impl TransientTextureId {
    pub(crate) const fn new(index: u32) -> Self {
        Self(index)
    }

    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }
}

/// Imported persistent resource or graph-owned transient texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GraphResourceRef {
    Imported(GpuResourceHandle),
    Transient(TransientTextureId),
}

impl From<GpuResourceHandle> for GraphResourceRef {
    fn from(resource: GpuResourceHandle) -> Self {
        Self::Imported(resource)
    }
}

impl From<TransientTextureId> for GraphResourceRef {
    fn from(resource: TransientTextureId) -> Self {
        Self::Transient(resource)
    }
}

/// How a pass uses a resource. The graph uses this to derive hazards and
/// transition/UAV ordering barriers; the backend maps it to native states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceUsage {
    SampledRead,
    StorageRead,
    StorageWrite,
    StorageReadWrite,
    ColorAttachmentWrite,
    DepthAttachmentWrite,
    CopySource,
    CopyDestination,
    VertexBufferRead,
    IndexBufferRead,
    ConstantBufferRead,
    IndirectArgumentRead,
}

impl ResourceUsage {
    const fn reads(self) -> bool {
        matches!(
            self,
            Self::SampledRead
                | Self::StorageRead
                | Self::StorageReadWrite
                | Self::CopySource
                | Self::VertexBufferRead
                | Self::IndexBufferRead
                | Self::ConstantBufferRead
                | Self::IndirectArgumentRead
        )
    }

    const fn writes(self) -> bool {
        matches!(
            self,
            Self::StorageWrite
                | Self::StorageReadWrite
                | Self::ColorAttachmentWrite
                | Self::DepthAttachmentWrite
                | Self::CopyDestination
        )
    }

    const fn state_class(self) -> u8 {
        match self {
            Self::SampledRead | Self::StorageRead => 0,
            Self::StorageWrite | Self::StorageReadWrite => 1,
            Self::ColorAttachmentWrite => 2,
            Self::DepthAttachmentWrite => 3,
            Self::CopySource => 4,
            Self::CopyDestination => 5,
            Self::VertexBufferRead | Self::ConstantBufferRead => 6,
            Self::IndexBufferRead => 7,
            Self::IndirectArgumentRead => 8,
        }
    }
}

/// One resource declaration in a pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceAccess {
    pub resource: GraphResourceRef,
    pub usage: ResourceUsage,
}

/// Declarative pass description. Explicit dependencies can supplement the
/// dependencies inferred from read/write hazards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderPassDesc {
    name: String,
    accesses: Vec<ResourceAccess>,
    dependencies: Vec<RenderPassId>,
}

impl RenderPassDesc {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), accesses: Vec::new(), dependencies: Vec::new() }
    }

    #[must_use]
    pub fn use_resource(
        mut self,
        resource: impl Into<GraphResourceRef>,
        usage: ResourceUsage,
    ) -> Self {
        self.accesses.push(ResourceAccess { resource: resource.into(), usage });
        self
    }

    #[must_use]
    pub fn depends_on(mut self, pass: RenderPassId) -> Self {
        self.dependencies.push(pass);
        self
    }
}

/// Barrier requirement compiled before a pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderBarrier {
    pub resource: GraphResourceRef,
    /// Previous resource occupying the same transient heap range when this is
    /// an aliasing barrier. `None` means the range has no prior resource.
    pub alias_before: Option<GraphResourceRef>,
    pub before: Option<ResourceUsage>,
    pub after: ResourceUsage,
    pub kind: RenderBarrierKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderBarrierKind {
    Transition,
    UavOrdering,
    Aliasing,
}

/// Render pass in stable execution order with its incoming barriers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledRenderPass {
    pub id: RenderPassId,
    pub name: String,
    pub accesses: Vec<ResourceAccess>,
    pub barriers_before: Vec<RenderBarrier>,
}

/// Compiled topological order and resource synchronization plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledRenderGraph {
    passes: Vec<CompiledRenderPass>,
    dependency_count: usize,
    transient_textures: Vec<TextureDesc>,
    transient_lifetimes: Vec<TransientTextureLifetime>,
    transient_alias_groups: Vec<TransientAliasGroup>,
}

impl CompiledRenderGraph {
    #[must_use]
    pub fn passes(&self) -> &[CompiledRenderPass] {
        &self.passes
    }

    #[must_use]
    pub const fn dependency_count(&self) -> usize {
        self.dependency_count
    }

    #[must_use]
    pub fn barrier_count(&self) -> usize {
        self.passes.iter().map(|pass| pass.barriers_before.len()).sum()
    }

    #[must_use]
    pub fn transient_lifetimes(&self) -> &[TransientTextureLifetime] {
        &self.transient_lifetimes
    }

    #[must_use]
    pub fn transient_alias_groups(&self) -> &[TransientAliasGroup] {
        &self.transient_alias_groups
    }

    #[must_use]
    pub fn transient_texture(&self, id: TransientTextureId) -> Option<TextureDesc> {
        self.transient_textures.get(id.index() as usize).copied()
    }
}

/// First and last topological pass that touches a transient texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransientTextureLifetime {
    pub resource: TransientTextureId,
    pub first_pass: RenderPassId,
    pub last_pass: RenderPassId,
}

/// Textures in one alias group have disjoint lifetimes and compatible D3D12
/// heap classes. The backend sizes and allocates the shared range from device
/// allocation requirements.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransientAliasGroup {
    pub resources: Vec<TransientTextureId>,
    pub render_target_class: bool,
}

#[derive(Debug, Clone)]
struct PassNode {
    description: RenderPassDesc,
}

#[derive(Default)]
struct PassHistory {
    writer: Option<usize>,
    readers: BTreeSet<usize>,
}

type RenderPassCallback<E> =
    Box<dyn FnOnce(&mut RenderCommandEncoder) -> Result<(), E> + Send + 'static>;
type RenderPassEncodeResult<E> = Result<(RenderPassId, RenderCommandBuffer), RenderQueueError<E>>;
type RenderPassWorkItem<E> =
    (Option<(RenderPassId, RenderPassCallback<E>)>, Option<RenderPassEncodeResult<E>>);

/// One vertex stream bound to an input-assembler slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VertexBufferBinding {
    pub slot: u32,
    pub buffer: GpuResourceHandle,
    pub offset: u64,
    pub size: u32,
    pub stride: u32,
}

/// Render target and per-attachment clear value for a graphics pass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorAttachmentBinding {
    pub resource: GpuResourceHandle,
    pub view: DescriptorHandle,
    pub clear_color: ClearColor,
}

/// Floating-point viewport in target pixel coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub min_depth: f32,
    pub max_depth: f32,
}

impl Viewport {
    #[must_use]
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height, min_depth: 0.0, max_depth: 1.0 }
    }

    #[must_use]
    pub const fn with_depth_range(mut self, min_depth: f32, max_depth: f32) -> Self {
        self.min_depth = min_depth;
        self.max_depth = max_depth;
        self
    }

    fn is_valid(self) -> bool {
        [self.x, self.y, self.width, self.height, self.min_depth, self.max_depth]
            .iter()
            .all(|value| value.is_finite())
            && self.x >= -32768.0
            && self.y >= -32768.0
            && self.x <= 32767.0
            && self.y <= 32767.0
            && self.width > 0.0
            && self.height > 0.0
            && self.width <= 32767.0
            && self.height <= 32767.0
            && self.x + self.width <= 32767.0
            && self.y + self.height <= 32767.0
            && self.min_depth >= 0.0
            && self.max_depth <= 1.0
            && self.min_depth <= self.max_depth
    }
}

/// Integer scissor rectangle in target pixel coordinates (right/bottom exclusive).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScissorRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl ScissorRect {
    #[must_use]
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self { left, top, right, bottom }
    }

    fn is_valid(self) -> bool {
        self.left < self.right
            && self.top < self.bottom
            && self.left >= -32768
            && self.top >= -32768
            && self.right <= 32767
            && self.bottom <= 32767
    }
}

/// Element width used by an indexed draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexFormat {
    Uint16,
    Uint32,
}

/// Index stream bound to the input-assembler stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexBufferBinding {
    pub buffer: GpuResourceHandle,
    pub offset: u64,
    pub size: u32,
    pub format: IndexFormat,
}

/// Draw ranges use explicit first-element offsets and instance ranges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawCall {
    Vertices {
        vertex_count: u32,
        instance_count: u32,
        first_vertex: u32,
        first_instance: u32,
    },
    Indexed {
        index_count: u32,
        instance_count: u32,
        first_index: u32,
        base_vertex: i32,
        first_instance: u32,
    },
}

#[derive(Clone)]
pub(crate) struct EncodedGraphicsDraw {
    pub(crate) description: GraphicsPipelineDesc,
    pub(crate) vertex: CompiledShader,
    pub(crate) pixel: CompiledShader,
    pub(crate) bindings: Vec<PipelineDescriptorTableBinding>,
    pub(crate) vertex_buffers: Vec<VertexBufferBinding>,
    pub(crate) index_buffer: Option<IndexBufferBinding>,
    pub(crate) call: DrawCall,
    pub(crate) indirect: Option<IndirectDrawArguments>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct IndirectDrawArguments {
    pub(crate) resource: GpuResourceHandle,
    pub(crate) offset: u64,
    pub(crate) maximum_commands: u32,
    pub(crate) maximum_instances: u32,
    pub(crate) maximum_indices: u32,
}

#[derive(Clone)]
pub(crate) struct EncodedGraphicsPass {
    pub(crate) color_attachments: Vec<ColorAttachmentBinding>,
    pub(crate) depth: Option<(GpuResourceHandle, DescriptorHandle)>,
    pub(crate) clear_depth: f32,
    pub(crate) viewport: Option<Viewport>,
    pub(crate) scissor: Option<ScissorRect>,
    pub(crate) draws: Vec<EncodedGraphicsDraw>,
}

#[derive(Clone)]
pub(crate) enum EncodedRenderCommand {
    Graphics(EncodedGraphicsPass),
    Compute {
        description: ComputePipelineDesc,
        compute: CompiledShader,
        bindings: Vec<PipelineDescriptorTableBinding>,
        groups: [u32; 3],
    },
    ComputeTransientUav {
        description: ComputePipelineDesc,
        compute: CompiledShader,
        bindings: Vec<PipelineDescriptorTableBinding>,
        output: TransientTextureId,
        register: u32,
        space: u32,
        groups: [u32; 3],
    },
}

/// Invalid command-encoder state or draw parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderCommandEncodingError {
    GraphicsPassAlreadyActive,
    GraphicsPassNotActive,
    GraphicsPassNotEnded,
    MixedGraphicsAndComputeCommands,
    ZeroDrawCount,
    InvalidVertexBufferBinding,
    InvalidIndexBufferBinding,
    MissingIndexBuffer,
    UnexpectedIndexBuffer,
    InvalidClearColor,
    InvalidColorAttachments,
    InvalidViewport,
    InvalidScissorRect,
    InvalidDescriptorTable,
    InvalidIndirectDraw,
}

impl std::fmt::Display for RenderCommandEncodingError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid render command encoder operation: {self:?}")
    }
}

impl std::error::Error for RenderCommandEncodingError {}

/// Backend-independent command stream assembled privately by one pass callback.
#[derive(Default)]
pub struct RenderCommandEncoder {
    commands: Vec<EncodedRenderCommand>,
    graphics_pass: Option<EncodedGraphicsPass>,
    mode: Option<EncoderMode>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EncoderMode {
    Graphics,
    Compute,
}

/// Owned commands recorded for one graph pass.
pub struct RenderCommandBuffer {
    pub(crate) commands: Vec<EncodedRenderCommand>,
}

impl RenderCommandEncoder {
    /// Begin one graphics pass. Attachments are bound and cleared once; any
    /// number of `draw` calls can then be encoded before `end_graphics_pass`.
    ///
    /// # Errors
    /// Returns an error for nested/mixed pass modes or an invalid clear color.
    pub fn begin_graphics_pass(
        &mut self,
        color: GpuResourceHandle,
        color_view: DescriptorHandle,
        depth: Option<(GpuResourceHandle, DescriptorHandle)>,
        clear_color: ClearColor,
    ) -> Result<(), RenderCommandEncodingError> {
        self.begin_graphics_pass_with_attachments(
            &[ColorAttachmentBinding { resource: color, view: color_view, clear_color }],
            depth,
            1.0,
            None,
            None,
        )
    }

    /// Begin a graphics pass with one or more color outputs, optional depth,
    /// per-target clear values, and optional viewport/scissor state.
    ///
    /// # Errors
    /// Returns an error for invalid encoder state, attachment count/clear data,
    /// viewport, or scissor rectangle.
    pub fn begin_graphics_pass_with_attachments(
        &mut self,
        color_attachments: &[ColorAttachmentBinding],
        depth: Option<(GpuResourceHandle, DescriptorHandle)>,
        clear_depth: f32,
        viewport: Option<Viewport>,
        scissor: Option<ScissorRect>,
    ) -> Result<(), RenderCommandEncodingError> {
        if self.graphics_pass.is_some() {
            return Err(RenderCommandEncodingError::GraphicsPassAlreadyActive);
        }
        if self.mode == Some(EncoderMode::Compute) {
            return Err(RenderCommandEncodingError::MixedGraphicsAndComputeCommands);
        }
        if self.mode == Some(EncoderMode::Graphics) {
            return Err(RenderCommandEncodingError::GraphicsPassAlreadyActive);
        }
        if color_attachments.is_empty()
            || color_attachments.len() > 8
            || color_attachments
                .iter()
                .map(|attachment| attachment.resource)
                .collect::<HashSet<_>>()
                .len()
                != color_attachments.len()
            || color_attachments.iter().any(|attachment| {
                attachment
                    .clear_color
                    .0
                    .iter()
                    .any(|channel| !channel.is_finite() || !(0.0..=1.0).contains(channel))
            })
            || !clear_depth.is_finite()
            || !(0.0..=1.0).contains(&clear_depth)
        {
            return Err(RenderCommandEncodingError::InvalidColorAttachments);
        }
        if viewport.is_some_and(|viewport| !viewport.is_valid()) {
            return Err(RenderCommandEncodingError::InvalidViewport);
        }
        if scissor.is_some_and(|scissor| !scissor.is_valid()) {
            return Err(RenderCommandEncodingError::InvalidScissorRect);
        }
        self.mode = Some(EncoderMode::Graphics);
        self.graphics_pass = Some(EncodedGraphicsPass {
            color_attachments: color_attachments.to_vec(),
            depth,
            clear_depth,
            viewport,
            scissor,
            draws: Vec::new(),
        });
        Ok(())
    }

    /// Record one draw inside the active graphics pass.
    ///
    /// # Errors
    /// Returns an error for invalid encoder state, empty draws, or malformed buffer views.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        description: &GraphicsPipelineDesc,
        vertex: &CompiledShader,
        pixel: &CompiledShader,
        bindings: &[PipelineDescriptorBinding],
        vertex_buffers: &[VertexBufferBinding],
        index_buffer: Option<IndexBufferBinding>,
        call: DrawCall,
    ) -> Result<(), RenderCommandEncodingError> {
        let tables = bindings
            .iter()
            .map(|binding| PipelineDescriptorTableBinding {
                kind: binding.kind,
                register: binding.register,
                space: binding.space,
                descriptors: vec![binding.descriptor],
            })
            .collect::<Vec<_>>();
        self.draw_with_descriptor_tables(
            description,
            vertex,
            pixel,
            &tables,
            vertex_buffers,
            index_buffer,
            call,
        )
    }

    /// Record a draw with scalar or array resource bindings.
    ///
    /// # Errors
    /// Returns an error for invalid encoder state, draw or buffer ranges, or a
    /// malformed descriptor table.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_with_descriptor_tables(
        &mut self,
        description: &GraphicsPipelineDesc,
        vertex: &CompiledShader,
        pixel: &CompiledShader,
        bindings: &[PipelineDescriptorTableBinding],
        vertex_buffers: &[VertexBufferBinding],
        index_buffer: Option<IndexBufferBinding>,
        call: DrawCall,
    ) -> Result<(), RenderCommandEncodingError> {
        if self.mode == Some(EncoderMode::Compute) {
            return Err(RenderCommandEncodingError::MixedGraphicsAndComputeCommands);
        }
        let Some(pass) = self.graphics_pass.as_mut() else {
            return Err(RenderCommandEncodingError::GraphicsPassNotActive);
        };
        let non_zero = match call {
            DrawCall::Vertices { vertex_count, instance_count, .. } => {
                vertex_count != 0 && instance_count != 0
            }
            DrawCall::Indexed { index_count, instance_count, .. } => {
                index_count != 0 && instance_count != 0
            }
        };
        if !non_zero {
            return Err(RenderCommandEncodingError::ZeroDrawCount);
        }
        if !descriptor_tables_are_well_formed(bindings) {
            return Err(RenderCommandEncodingError::InvalidDescriptorTable);
        }
        if vertex_buffers.iter().any(|binding| binding.size == 0 || binding.stride == 0)
            || vertex_buffers.iter().map(|binding| binding.slot).collect::<HashSet<_>>().len()
                != vertex_buffers.len()
        {
            return Err(RenderCommandEncodingError::InvalidVertexBufferBinding);
        }
        match (call, index_buffer) {
            (DrawCall::Indexed { .. }, None) => {
                return Err(RenderCommandEncodingError::MissingIndexBuffer);
            }
            (DrawCall::Vertices { .. }, Some(_)) => {
                return Err(RenderCommandEncodingError::UnexpectedIndexBuffer);
            }
            (_, Some(index))
                if index.size == 0
                    || index.offset
                        % match index.format {
                            IndexFormat::Uint16 => 2,
                            IndexFormat::Uint32 => 4,
                        }
                        != 0
                    || !index.size.is_multiple_of(match index.format {
                        IndexFormat::Uint16 => 2,
                        IndexFormat::Uint32 => 4,
                    }) =>
            {
                return Err(RenderCommandEncodingError::InvalidIndexBufferBinding);
            }
            _ => {}
        }
        pass.draws.push(EncodedGraphicsDraw {
            description: description.clone(),
            vertex: vertex.clone(),
            pixel: pixel.clone(),
            bindings: bindings.to_vec(),
            vertex_buffers: vertex_buffers.to_vec(),
            index_buffer,
            call,
            indirect: None,
        });
        Ok(())
    }

    /// Record an indexed draw whose arguments are generated by the GPU.
    ///
    /// The argument buffer uses the D3D12 `DRAW_INDEXED` layout (20 bytes per
    /// command). `maximum_instances` bounds validation of the instance stream;
    /// `maximum_indices` bounds the index stream. The shader that writes the
    /// arguments must keep both counts within those declared capacities.
    ///
    /// # Errors
    /// Returns an error when the encoder state, descriptor bindings, or declared
    /// indirect-draw limits are invalid.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_indexed_indirect(
        &mut self,
        description: &GraphicsPipelineDesc,
        vertex: &CompiledShader,
        pixel: &CompiledShader,
        bindings: &[PipelineDescriptorBinding],
        vertex_buffers: &[VertexBufferBinding],
        index_buffer: IndexBufferBinding,
        arguments: GpuResourceHandle,
        argument_offset: u64,
        maximum_commands: u32,
        maximum_instances: u32,
        maximum_indices: u32,
    ) -> Result<(), RenderCommandEncodingError> {
        let tables = bindings
            .iter()
            .map(|binding| PipelineDescriptorTableBinding {
                kind: binding.kind,
                register: binding.register,
                space: binding.space,
                descriptors: vec![binding.descriptor],
            })
            .collect::<Vec<_>>();
        if self.mode == Some(EncoderMode::Compute) {
            return Err(RenderCommandEncodingError::MixedGraphicsAndComputeCommands);
        }
        let Some(pass) = self.graphics_pass.as_mut() else {
            return Err(RenderCommandEncodingError::GraphicsPassNotActive);
        };
        if maximum_commands == 0
            || maximum_instances == 0
            || maximum_indices == 0
            || !argument_offset.is_multiple_of(4)
            || index_buffer.size == 0
            || !index_buffer.size.is_multiple_of(match index_buffer.format {
                IndexFormat::Uint16 => 2,
                IndexFormat::Uint32 => 4,
            })
        {
            return Err(RenderCommandEncodingError::InvalidIndirectDraw);
        }
        if !descriptor_tables_are_well_formed(&tables) {
            return Err(RenderCommandEncodingError::InvalidDescriptorTable);
        }
        if vertex_buffers.iter().any(|binding| binding.size == 0 || binding.stride == 0)
            || vertex_buffers.iter().map(|binding| binding.slot).collect::<HashSet<_>>().len()
                != vertex_buffers.len()
        {
            return Err(RenderCommandEncodingError::InvalidVertexBufferBinding);
        }
        pass.draws.push(EncodedGraphicsDraw {
            description: description.clone(),
            vertex: vertex.clone(),
            pixel: pixel.clone(),
            bindings: tables,
            vertex_buffers: vertex_buffers.to_vec(),
            index_buffer: Some(index_buffer),
            call: DrawCall::Indexed {
                index_count: 1,
                instance_count: 1,
                first_index: 0,
                base_vertex: 0,
                first_instance: 0,
            },
            indirect: Some(IndirectDrawArguments {
                resource: arguments,
                offset: argument_offset,
                maximum_commands,
                maximum_instances,
                maximum_indices,
            }),
        });
        Ok(())
    }

    /// Finish the active graphics pass and append its atomic command batch.
    ///
    /// # Errors
    /// Returns `GraphicsPassNotActive` if there is no open graphics pass.
    pub fn end_graphics_pass(&mut self) -> Result<(), RenderCommandEncodingError> {
        let pass =
            self.graphics_pass.take().ok_or(RenderCommandEncodingError::GraphicsPassNotActive)?;
        self.commands.push(EncodedRenderCommand::Graphics(pass));
        Ok(())
    }

    /// Record a compute dispatch into this pass's independent command stream.
    ///
    /// # Errors
    /// Returns an error when the graph pass has already selected graphics commands.
    pub fn dispatch(
        &mut self,
        description: ComputePipelineDesc,
        compute: &CompiledShader,
        bindings: &[PipelineDescriptorBinding],
        groups: [u32; 3],
    ) -> Result<(), RenderCommandEncodingError> {
        let tables = bindings
            .iter()
            .map(|binding| PipelineDescriptorTableBinding {
                kind: binding.kind,
                register: binding.register,
                space: binding.space,
                descriptors: vec![binding.descriptor],
            })
            .collect::<Vec<_>>();
        self.dispatch_with_descriptor_tables(description, compute, &tables, groups)
    }

    /// Record a compute dispatch with scalar or array resource bindings.
    ///
    /// # Errors
    /// Returns an error if graphics recording has started or a descriptor
    /// table is empty or non-contiguous.
    pub fn dispatch_with_descriptor_tables(
        &mut self,
        description: ComputePipelineDesc,
        compute: &CompiledShader,
        bindings: &[PipelineDescriptorTableBinding],
        groups: [u32; 3],
    ) -> Result<(), RenderCommandEncodingError> {
        if self.graphics_pass.is_some() || self.mode == Some(EncoderMode::Graphics) {
            return Err(RenderCommandEncodingError::MixedGraphicsAndComputeCommands);
        }
        if !descriptor_tables_are_well_formed(bindings) {
            return Err(RenderCommandEncodingError::InvalidDescriptorTable);
        }
        self.mode = Some(EncoderMode::Compute);
        self.commands.push(EncodedRenderCommand::Compute {
            description,
            compute: compute.clone(),
            bindings: bindings.to_vec(),
            groups,
        });
        Ok(())
    }

    /// Record a compute dispatch whose UAV descriptor is created for a
    /// graph-owned transient texture immediately before native recording.
    ///
    /// # Errors
    /// Returns an error when the graph pass has already selected graphics commands.
    #[allow(clippy::too_many_arguments)]
    pub fn dispatch_transient_uav(
        &mut self,
        description: ComputePipelineDesc,
        compute: &CompiledShader,
        bindings: &[PipelineDescriptorBinding],
        output: TransientTextureId,
        register: u32,
        space: u32,
        groups: [u32; 3],
    ) -> Result<(), RenderCommandEncodingError> {
        let tables = bindings
            .iter()
            .map(|binding| PipelineDescriptorTableBinding {
                kind: binding.kind,
                register: binding.register,
                space: binding.space,
                descriptors: vec![binding.descriptor],
            })
            .collect::<Vec<_>>();
        self.dispatch_transient_uav_with_descriptor_tables(
            description,
            compute,
            &tables,
            output,
            register,
            space,
            groups,
        )
    }

    /// Record a transient-UAV dispatch with additional scalar/array bindings.
    ///
    /// # Errors
    /// Returns an error if graphics recording has started or a descriptor
    /// table is empty or non-contiguous.
    #[allow(clippy::too_many_arguments)]
    pub fn dispatch_transient_uav_with_descriptor_tables(
        &mut self,
        description: ComputePipelineDesc,
        compute: &CompiledShader,
        bindings: &[PipelineDescriptorTableBinding],
        output: TransientTextureId,
        register: u32,
        space: u32,
        groups: [u32; 3],
    ) -> Result<(), RenderCommandEncodingError> {
        if self.graphics_pass.is_some() || self.mode == Some(EncoderMode::Graphics) {
            return Err(RenderCommandEncodingError::MixedGraphicsAndComputeCommands);
        }
        if !descriptor_tables_are_well_formed(bindings) {
            return Err(RenderCommandEncodingError::InvalidDescriptorTable);
        }
        self.mode = Some(EncoderMode::Compute);
        self.commands.push(EncodedRenderCommand::ComputeTransientUav {
            description,
            compute: compute.clone(),
            bindings: bindings.to_vec(),
            output,
            register,
            space,
            groups,
        });
        Ok(())
    }

    pub(crate) fn finish(self) -> Result<RenderCommandBuffer, RenderCommandEncodingError> {
        if self.graphics_pass.is_some() {
            return Err(RenderCommandEncodingError::GraphicsPassNotEnded);
        }
        Ok(RenderCommandBuffer { commands: self.commands })
    }
}

fn descriptor_tables_are_well_formed(bindings: &[PipelineDescriptorTableBinding]) -> bool {
    bindings.iter().all(|binding| {
        let Some(first) = binding.descriptors.first() else { return false };
        binding.descriptors.iter().enumerate().all(|(offset, descriptor)| {
            descriptor.kind() == first.kind()
                && descriptor.index() as usize == first.index() as usize + offset
        })
    })
}

/// Frame-local graph builder for imported resources and transient textures.
#[derive(Debug, Default, Clone)]
pub struct RenderGraph {
    passes: Vec<PassNode>,
    transient_textures: Vec<TextureDesc>,
}

impl RenderGraph {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Declare a texture whose storage is owned by this graph execution.
    ///
    /// # Errors
    /// Returns `TooManyTransientResources` if the graph-local ID space is exhausted.
    pub fn create_transient_texture(
        &mut self,
        desc: TextureDesc,
    ) -> Result<TransientTextureId, RenderGraphError> {
        let id = TransientTextureId(
            u32::try_from(self.transient_textures.len())
                .map_err(|_| RenderGraphError::TooManyTransientResources)?,
        );
        self.transient_textures.push(desc);
        Ok(id)
    }

    /// Add a pass and return its stable graph-local ID.
    ///
    /// # Errors
    /// Returns an error for an empty pass name, duplicate resource declaration,
    /// or dependency on a pass that has not been added.
    pub fn add_pass(
        &mut self,
        description: RenderPassDesc,
    ) -> Result<RenderPassId, RenderGraphError> {
        if description.name.trim().is_empty() {
            return Err(RenderGraphError::EmptyPassName);
        }
        let mut resources = HashSet::new();
        for access in &description.accesses {
            if let GraphResourceRef::Transient(id) = access.resource
                && id.index() as usize >= self.transient_textures.len()
            {
                return Err(RenderGraphError::InvalidTransientResource);
            }
            if !resources.insert(access.resource) {
                return Err(RenderGraphError::DuplicateResourceUse);
            }
        }
        if description.dependencies.iter().any(|dependency| dependency.0 >= self.passes.len()) {
            return Err(RenderGraphError::InvalidDependency);
        }
        let id = RenderPassId(self.passes.len());
        self.passes.push(PassNode { description });
        Ok(id)
    }

    /// Compile inferred read/write hazards and explicit dependencies into a
    /// deterministic topological order and barrier plan.
    ///
    /// # Errors
    /// Returns an error if the dependency graph contains a cycle or an
    /// internal pass/resource invariant is inconsistent.
    #[allow(clippy::too_many_lines)]
    pub fn compile(&self) -> Result<CompiledRenderGraph, RenderGraphError> {
        let count = self.passes.len();
        let mut edges = vec![BTreeSet::<usize>::new(); count];
        let mut reverse = vec![BTreeSet::<usize>::new(); count];
        for (index, node) in self.passes.iter().enumerate() {
            for dependency in &node.description.dependencies {
                edges[dependency.0].insert(index);
                reverse[index].insert(dependency.0);
            }
        }

        let mut histories = HashMap::<GraphResourceRef, PassHistory>::new();
        for (index, node) in self.passes.iter().enumerate() {
            for access in &node.description.accesses {
                let history = histories.entry(access.resource).or_default();
                if access.usage.reads()
                    && let Some(writer) = history.writer
                    && writer != index
                {
                    edges[writer].insert(index);
                    reverse[index].insert(writer);
                }
                if access.usage.writes() {
                    if let Some(writer) = history.writer
                        && writer != index
                    {
                        edges[writer].insert(index);
                        reverse[index].insert(writer);
                    }
                    for reader in std::mem::take(&mut history.readers) {
                        if reader != index {
                            edges[reader].insert(index);
                            reverse[index].insert(reader);
                        }
                    }
                    history.writer = Some(index);
                }
                if access.usage.reads() && !access.usage.writes() {
                    history.readers.insert(index);
                }
            }
        }

        let mut indegree: Vec<usize> = reverse.iter().map(BTreeSet::len).collect();
        let mut ready = indegree
            .iter()
            .enumerate()
            .filter_map(|(index, &degree)| (degree == 0).then_some(index))
            .collect::<BTreeSet<_>>();
        let mut order = Vec::with_capacity(count);
        while let Some(index) = ready.pop_first() {
            order.push(index);
            for &destination in &edges[index] {
                indegree[destination] -= 1;
                if indegree[destination] == 0 {
                    ready.insert(destination);
                }
            }
        }
        if order.len() != count {
            return Err(RenderGraphError::Cycle);
        }

        let mut previous_usage = HashMap::<GraphResourceRef, ResourceUsage>::new();
        let mut compiled = Vec::with_capacity(count);
        for index in order {
            let node = &self.passes[index];
            let mut barriers = Vec::new();
            for access in &node.description.accesses {
                let before = previous_usage.get(&access.resource).copied();
                let kind = match before {
                    Some(previous)
                        if previous.state_class() == access.usage.state_class()
                            && previous.state_class() == 1
                            && (previous.writes() || access.usage.writes()) =>
                    {
                        Some(RenderBarrierKind::UavOrdering)
                    }
                    Some(previous) if previous.state_class() != access.usage.state_class() => {
                        Some(RenderBarrierKind::Transition)
                    }
                    None => Some(RenderBarrierKind::Transition),
                    _ => None,
                };
                if let Some(kind) = kind {
                    barriers.push(RenderBarrier {
                        resource: access.resource,
                        alias_before: None,
                        before,
                        after: access.usage,
                        kind,
                    });
                }
                previous_usage.insert(access.resource, access.usage);
            }
            compiled.push(CompiledRenderPass {
                id: RenderPassId(index),
                name: node.description.name.clone(),
                accesses: node.description.accesses.clone(),
                barriers_before: barriers,
            });
        }
        let mut dependency_count: usize = edges.iter().map(BTreeSet::len).sum();
        let mut use_positions = HashMap::<TransientTextureId, (usize, usize)>::new();
        for (position, pass) in compiled.iter().enumerate() {
            for access in &self.passes[pass.id.index()].description.accesses {
                if let GraphResourceRef::Transient(id) = access.resource {
                    use_positions
                        .entry(id)
                        .and_modify(|lifetime| lifetime.1 = position)
                        .or_insert((position, position));
                }
            }
        }
        let mut transient_lifetimes = use_positions
            .iter()
            .map(|(&resource, &(first, last))| TransientTextureLifetime {
                resource,
                first_pass: compiled[first].id,
                last_pass: compiled[last].id,
            })
            .collect::<Vec<_>>();
        transient_lifetimes
            .sort_by_key(|lifetime| (lifetime.first_pass.index(), lifetime.resource.index()));
        let mut transient_alias_groups = Vec::<TransientAliasGroup>::new();
        let mut alias_group_last_use = Vec::<usize>::new();
        for lifetime in &transient_lifetimes {
            let desc = self.transient_textures[lifetime.resource.index() as usize];
            let render_target_class = desc.usage().contains(TextureUsage::COLOR_ATTACHMENT)
                || desc.usage().contains(TextureUsage::DEPTH_STENCIL_ATTACHMENT);
            let first =
                compiled.iter().position(|pass| pass.id == lifetime.first_pass).unwrap_or(0);
            let last =
                compiled.iter().position(|pass| pass.id == lifetime.last_pass).unwrap_or(first);
            let reusable = transient_alias_groups.iter().enumerate().position(|(index, group)| {
                group.render_target_class == render_target_class
                    && alias_group_last_use[index] < first
            });
            if let Some(index) = reusable {
                transient_alias_groups[index].resources.push(lifetime.resource);
                alias_group_last_use[index] = last;
                // The alias edge serializes resources that otherwise might be
                // independent in the graph; one direct queue submits this order.
                dependency_count += 1;
            } else {
                transient_alias_groups.push(TransientAliasGroup {
                    resources: vec![lifetime.resource],
                    render_target_class,
                });
                alias_group_last_use.push(last);
            }
        }
        // D3D12 requires an aliasing barrier when a placed resource begins
        // using a heap range previously occupied by another resource.
        for group in &transient_alias_groups {
            for pair in group.resources.windows(2) {
                let previous = GraphResourceRef::Transient(pair[0]);
                let current = GraphResourceRef::Transient(pair[1]);
                let first_pass = transient_lifetimes
                    .iter()
                    .find(|lifetime| lifetime.resource == pair[1])
                    .map(|lifetime| lifetime.first_pass)
                    .ok_or(RenderGraphError::InternalCompilationInvariant)?;
                let usage = self.passes[first_pass.index()]
                    .description
                    .accesses
                    .iter()
                    .find(|access| access.resource == current)
                    .map(|access| access.usage)
                    .ok_or(RenderGraphError::InternalCompilationInvariant)?;
                let Some(compiled_pass) = compiled.iter_mut().find(|pass| pass.id == first_pass)
                else {
                    return Err(RenderGraphError::InternalCompilationInvariant);
                };
                compiled_pass.barriers_before.insert(
                    0,
                    RenderBarrier {
                        resource: current,
                        alias_before: Some(previous),
                        before: None,
                        after: usage,
                        kind: RenderBarrierKind::Aliasing,
                    },
                );
            }
        }
        Ok(CompiledRenderGraph {
            passes: compiled,
            dependency_count,
            transient_textures: self.transient_textures.clone(),
            transient_lifetimes,
            transient_alias_groups,
        })
    }
}

/// Graph compilation failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderGraphError {
    EmptyPassName,
    DuplicateResourceUse,
    InvalidDependency,
    Cycle,
    InvalidTransientResource,
    TooManyTransientResources,
    InternalCompilationInvariant,
}

impl Display for RenderGraphError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "render graph error: {self:?}")
    }
}

impl std::error::Error for RenderGraphError {}

/// Error raised while submitting callbacks in a compiled render queue.
#[derive(Debug)]
pub enum RenderQueueError<E> {
    Graph(RenderGraphError),
    TransientResources(E),
    Pass {
        pass: RenderPassId,
        source: E,
    },
    Encoding {
        pass: RenderPassId,
        source: RenderCommandEncodingError,
    },
    ResourceUsageMismatch {
        pass: RenderPassId,
        resource: GraphResourceRef,
        expected: ResourceUsage,
    },
    EncodingWorkerPanicked,
    AlreadySubmitted(RenderPassId),
}

/// Frame-local queue of graph passes whose callbacks encode commands without
/// borrowing the application renderer.
pub struct RenderQueue<E> {
    graph: RenderGraph,
    callbacks: Vec<Option<RenderPassCallback<E>>>,
}

impl<E> Default for RenderQueue<E> {
    fn default() -> Self {
        Self { graph: RenderGraph::new(), callbacks: Vec::new() }
    }
}

impl<E> RenderQueue<E> {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Declare a transient texture before adding passes that use its ID.
    ///
    /// # Errors
    /// Returns `TooManyTransientResources` if the graph-local ID space is exhausted.
    pub fn create_transient_texture(
        &mut self,
        desc: TextureDesc,
    ) -> Result<TransientTextureId, RenderGraphError> {
        self.graph.create_transient_texture(desc)
    }

    /// Add a callback associated with a graph pass.
    ///
    /// # Errors
    /// Returns a graph error when the pass description is invalid.
    pub fn enqueue(
        &mut self,
        description: RenderPassDesc,
        callback: impl FnOnce(&mut RenderCommandEncoder) -> Result<(), E> + Send + 'static,
    ) -> Result<RenderPassId, RenderGraphError> {
        let id = self.graph.add_pass(description)?;
        self.callbacks.push(Some(Box::new(callback)));
        Ok(id)
    }

    /// Compile the current queue without consuming its callbacks.
    ///
    /// # Errors
    /// Returns a graph compilation error for invalid dependencies or cycles.
    pub fn compile(&self) -> Result<CompiledRenderGraph, RenderGraphError> {
        self.graph.compile()
    }

    /// Execute each pass encoder callback concurrently and return command
    /// streams in declaration order for native parallel recording.
    ///
    /// # Errors
    /// Returns graph compilation, pass encoder, or repeated-submission errors.
    pub fn encode_parallel(
        &mut self,
        jobs: &nord_jobs::JobSystem,
    ) -> Result<(CompiledRenderGraph, Vec<RenderCommandBuffer>), RenderQueueError<E>>
    where
        E: Send,
    {
        let compiled = self.graph.compile().map_err(RenderQueueError::Graph)?;
        let callbacks = self
            .callbacks
            .iter_mut()
            .enumerate()
            .map(|(index, callback)| {
                callback
                    .take()
                    .map(|callback| (RenderPassId(index), callback))
                    .ok_or(RenderQueueError::AlreadySubmitted(RenderPassId(index)))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut work =
            callbacks.into_iter().map(|callback| (Some(callback), None)).collect::<Vec<_>>();
        let encode = |(task, result): &mut RenderPassWorkItem<E>| {
            let Some((pass, callback)) = task.take() else {
                *result = Some(Err(RenderQueueError::EncodingWorkerPanicked));
                return;
            };
            let mut encoder = RenderCommandEncoder::default();
            *result = Some(
                callback(&mut encoder)
                    .map_err(|source| RenderQueueError::Pass { pass, source })
                    .and_then(|()| {
                        encoder
                            .finish()
                            .map(|commands| (pass, commands))
                            .map_err(|source| RenderQueueError::Encoding { pass, source })
                    }),
            );
        };
        if work.len() <= 1 {
            for item in &mut work {
                encode(item);
            }
        } else {
            jobs.for_each_mut(&mut work, encode);
        }
        let mut encoded = work
            .into_iter()
            .map(|(_, result)| result.unwrap_or(Err(RenderQueueError::EncodingWorkerPanicked)))
            .collect::<Result<Vec<_>, _>>()?;
        encoded.sort_by_key(|(pass, _)| pass.index());
        let buffers = encoded.into_iter().map(|(_, commands)| commands).collect::<Vec<_>>();
        for (index, buffer) in buffers.iter().enumerate() {
            let pass_id = RenderPassId(index);
            let declared = &self.graph.passes[index].description.accesses;
            let check_usage = |resource: GraphResourceRef, expected: ResourceUsage| {
                if !declared
                    .iter()
                    .any(|access| access.resource == resource && access.usage == expected)
                {
                    return Err(RenderQueueError::ResourceUsageMismatch {
                        pass: pass_id,
                        resource,
                        expected,
                    });
                }
                Ok(())
            };
            for command in &buffer.commands {
                match command {
                    EncodedRenderCommand::Graphics(graphics) => {
                        for attachment in &graphics.color_attachments {
                            check_usage(
                                attachment.resource.into(),
                                ResourceUsage::ColorAttachmentWrite,
                            )?;
                        }
                        if let Some((depth, _)) = graphics.depth {
                            check_usage(depth.into(), ResourceUsage::DepthAttachmentWrite)?;
                        }
                        for draw in &graphics.draws {
                            for binding in &draw.vertex_buffers {
                                check_usage(
                                    binding.buffer.into(),
                                    ResourceUsage::VertexBufferRead,
                                )?;
                            }
                            if let Some(binding) = draw.index_buffer {
                                check_usage(binding.buffer.into(), ResourceUsage::IndexBufferRead)?;
                            }
                        }
                    }
                    EncodedRenderCommand::ComputeTransientUav { output, .. } => {
                        check_usage((*output).into(), ResourceUsage::StorageWrite)?;
                    }
                    EncodedRenderCommand::Compute { .. } => {}
                }
            }
        }
        Ok((compiled, buffers))
    }

    /// Reset the queue after submission or cancellation for reuse next frame.
    pub fn clear(&mut self) {
        self.graph = RenderGraph::new();
        self.callbacks.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ColorAttachmentBinding, GraphResourceRef, RenderBarrierKind, RenderCommandEncodingError,
        RenderGraph, RenderGraphError, RenderPassDesc, RenderPassId, RenderQueue, RenderQueueError,
        ResourceUsage, ScissorRect, Viewport,
    };
    use crate::{ClearColor, DescriptorArena, DescriptorKind, GpuResourceHandle, ResourceKind};
    use std::num::NonZeroU32;
    use std::sync::Arc;

    fn job_system() -> nord_jobs::JobSystem {
        let pool =
            nord_threading::ThreadPool::new(nord_threading::ThreadPoolConfig::default()).unwrap();
        nord_jobs::JobSystem::new(Arc::new(pool))
    }

    fn resource(index: u32) -> GpuResourceHandle {
        GpuResourceHandle::new(index, NonZeroU32::MIN, ResourceKind::Texture)
    }

    fn render_target(index: u32) -> GpuResourceHandle {
        GpuResourceHandle::new(index, NonZeroU32::MIN, ResourceKind::RenderTarget)
    }

    #[test]
    fn encodes_mrt_viewport_and_scissor_and_rejects_invalid_raster_state() {
        let mut descriptors = DescriptorArena::new(0, 0, 2, 0);
        let first_view = descriptors.allocate(DescriptorKind::RenderTarget).unwrap();
        let second_view = descriptors.allocate(DescriptorKind::RenderTarget).unwrap();
        let attachments = [
            ColorAttachmentBinding {
                resource: render_target(0),
                view: first_view,
                clear_color: ClearColor::rgba(0.0, 0.0, 0.0, 1.0),
            },
            ColorAttachmentBinding {
                resource: render_target(1),
                view: second_view,
                clear_color: ClearColor::rgba(0.0, 1.0, 0.0, 1.0),
            },
        ];
        let mut encoder = super::RenderCommandEncoder::default();
        encoder
            .begin_graphics_pass_with_attachments(
                &attachments,
                None,
                1.0,
                Some(Viewport::new(0.0, 0.0, 64.0, 64.0)),
                Some(ScissorRect::new(4, 4, 60, 60)),
            )
            .unwrap();
        encoder.end_graphics_pass().unwrap();
        assert!(encoder.finish().is_ok());

        let mut invalid_viewport = super::RenderCommandEncoder::default();
        assert_eq!(
            invalid_viewport.begin_graphics_pass_with_attachments(
                &attachments,
                None,
                1.0,
                Some(Viewport::new(0.0, 0.0, 0.0, 64.0)),
                None,
            ),
            Err(RenderCommandEncodingError::InvalidViewport)
        );
        let mut invalid_scissor = super::RenderCommandEncoder::default();
        assert_eq!(
            invalid_scissor.begin_graphics_pass_with_attachments(
                &attachments,
                None,
                1.0,
                None,
                Some(ScissorRect::new(8, 8, 8, 16)),
            ),
            Err(RenderCommandEncodingError::InvalidScissorRect)
        );
    }

    #[test]
    fn infers_write_read_hazard_and_transition_barrier() {
        let image = resource(0);
        let mut graph = RenderGraph::new();
        graph
            .add_pass(
                RenderPassDesc::new("compute").use_resource(image, ResourceUsage::StorageWrite),
            )
            .unwrap();
        graph
            .add_pass(RenderPassDesc::new("shade").use_resource(image, ResourceUsage::SampledRead))
            .unwrap();
        let plan = graph.compile().unwrap();
        assert_eq!(
            plan.passes().iter().map(|pass| pass.name.as_str()).collect::<Vec<_>>(),
            ["compute", "shade"]
        );
        assert_eq!(plan.dependency_count(), 1);
        assert_eq!(plan.passes()[1].barriers_before[0].before, Some(ResourceUsage::StorageWrite));
    }

    #[test]
    fn rejects_ambiguous_same_pass_access_and_invalid_dependency() {
        let image = resource(1);
        let mut graph = RenderGraph::new();
        assert_eq!(
            graph.add_pass(
                RenderPassDesc::new("bad")
                    .use_resource(image, ResourceUsage::SampledRead)
                    .use_resource(image, ResourceUsage::ColorAttachmentWrite)
            ),
            Err(RenderGraphError::DuplicateResourceUse)
        );
        assert_eq!(
            graph.add_pass(RenderPassDesc::new("bad edge").depends_on(RenderPassId(9))),
            Err(RenderGraphError::InvalidDependency)
        );
    }

    #[test]
    fn detects_explicit_dependency_cycle() {
        let mut graph = RenderGraph::new();
        let a = graph.add_pass(RenderPassDesc::new("a")).unwrap();
        let b = graph.add_pass(RenderPassDesc::new("b").depends_on(a)).unwrap();
        graph.passes[b.index()].description.dependencies.push(RenderPassId(b.index()));
        assert_eq!(graph.compile(), Err(RenderGraphError::Cycle));
    }

    #[test]
    fn orders_writers_and_emits_uav_ordering_barrier() {
        let output = resource(2);
        let mut graph = RenderGraph::new();
        graph
            .add_pass(
                RenderPassDesc::new("write one").use_resource(output, ResourceUsage::StorageWrite),
            )
            .unwrap();
        graph
            .add_pass(
                RenderPassDesc::new("write two").use_resource(output, ResourceUsage::StorageWrite),
            )
            .unwrap();
        let plan = graph.compile().unwrap();
        assert_eq!(plan.dependency_count(), 1);
        assert_eq!(plan.passes()[1].barriers_before[0].kind, RenderBarrierKind::UavOrdering);
    }

    #[test]
    fn gpu_culling_output_transitions_to_vertex_and_indirect_argument_states() {
        let arguments = GpuResourceHandle::new(20, NonZeroU32::MIN, ResourceKind::Buffer);
        let visible_instances = GpuResourceHandle::new(21, NonZeroU32::MIN, ResourceKind::Buffer);
        let mut graph = RenderGraph::new();
        graph
            .add_pass(
                RenderPassDesc::new("initialize indirect arguments")
                    .use_resource(arguments, ResourceUsage::StorageWrite),
            )
            .unwrap();
        graph
            .add_pass(
                RenderPassDesc::new("compact visible instances")
                    .use_resource(arguments, ResourceUsage::StorageReadWrite)
                    .use_resource(visible_instances, ResourceUsage::StorageWrite),
            )
            .unwrap();
        graph
            .add_pass(
                RenderPassDesc::new("consume GPU outputs")
                    .use_resource(arguments, ResourceUsage::IndirectArgumentRead)
                    .use_resource(visible_instances, ResourceUsage::VertexBufferRead),
            )
            .unwrap();

        let compiled = graph.compile().unwrap();
        assert_eq!(compiled.passes().len(), 3);
        assert!(compiled.passes()[1].barriers_before.iter().any(|barrier| {
            barrier.resource == GraphResourceRef::Imported(arguments)
                && barrier.kind == RenderBarrierKind::UavOrdering
        }));
        assert!(compiled.passes()[2].barriers_before.iter().any(|barrier| {
            barrier.resource == GraphResourceRef::Imported(arguments)
                && barrier.kind == RenderBarrierKind::Transition
                && barrier.before == Some(ResourceUsage::StorageReadWrite)
                && barrier.after == ResourceUsage::IndirectArgumentRead
        }));
        assert!(compiled.passes()[2].barriers_before.iter().any(|barrier| {
            barrier.resource == GraphResourceRef::Imported(visible_instances)
                && barrier.kind == RenderBarrierKind::Transition
                && barrier.before == Some(ResourceUsage::StorageWrite)
                && barrier.after == ResourceUsage::VertexBufferRead
        }));
    }

    #[test]
    fn queue_encodes_passes_in_parallel_and_returns_graph_order() {
        let output = resource(3);
        let gate = std::sync::Arc::new(std::sync::Barrier::new(2));
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut queue = RenderQueue::<()>::new();
        queue
            .enqueue(
                RenderPassDesc::new("produce")
                    .use_resource(output, ResourceUsage::ColorAttachmentWrite),
                {
                    let gate = gate.clone();
                    let calls = calls.clone();
                    move |_encoder| {
                        gate.wait();
                        calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        Ok(())
                    }
                },
            )
            .unwrap();
        queue
            .enqueue(
                RenderPassDesc::new("consume").use_resource(output, ResourceUsage::CopySource),
                {
                    let gate = gate.clone();
                    let calls = calls.clone();
                    move |_encoder| {
                        gate.wait();
                        calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        Ok(())
                    }
                },
            )
            .unwrap();
        let jobs = job_system();
        let (plan, buffers) = queue.encode_parallel(&jobs).unwrap();
        assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 2);
        assert_eq!(buffers.len(), 2);
        assert_eq!(plan.passes()[0].name, "produce");
        assert_eq!(plan.passes()[1].name, "consume");
        assert_eq!(plan.dependency_count(), 1);
    }

    #[test]
    fn encoder_rejects_unclosed_graphics_pass_and_undeclared_attachment() {
        let target = resource(10);
        let mut descriptors = DescriptorArena::new(0, 0, 1, 0);
        let rtv = descriptors.allocate(DescriptorKind::RenderTarget).unwrap();

        let mut unclosed = RenderQueue::<()>::new();
        unclosed
            .enqueue(RenderPassDesc::new("unclosed"), move |encoder| {
                encoder
                    .begin_graphics_pass(target, rtv, None, ClearColor::rgba(0.0, 0.0, 0.0, 1.0))
                    .unwrap();
                Ok(())
            })
            .unwrap();
        assert!(matches!(
            unclosed.encode_parallel(&job_system()),
            Err(RenderQueueError::Encoding {
                source: RenderCommandEncodingError::GraphicsPassNotEnded,
                ..
            })
        ));

        let mut undeclared = RenderQueue::<()>::new();
        undeclared
            .enqueue(RenderPassDesc::new("undeclared"), move |encoder| {
                encoder
                    .begin_graphics_pass(target, rtv, None, ClearColor::rgba(0.0, 0.0, 0.0, 1.0))
                    .unwrap();
                encoder.end_graphics_pass().unwrap();
                Ok(())
            })
            .unwrap();
        assert!(matches!(
            undeclared.encode_parallel(&job_system()),
            Err(RenderQueueError::ResourceUsageMismatch {
                resource: GraphResourceRef::Imported(handle),
                expected: ResourceUsage::ColorAttachmentWrite,
                ..
            }) if handle == target
        ));
    }

    #[test]
    fn transient_textures_with_disjoint_ordered_lifetimes_alias_and_emit_barrier() {
        use crate::{TextureDesc, TextureFormat, TextureUsage};
        let desc =
            TextureDesc::new(16, 16, TextureFormat::Rgba8Unorm, TextureUsage::STORAGE).unwrap();
        let mut graph = RenderGraph::new();
        let first = graph.create_transient_texture(desc).unwrap();
        let second = graph.create_transient_texture(desc).unwrap();
        graph
            .add_pass(
                RenderPassDesc::new("first use").use_resource(first, ResourceUsage::StorageWrite),
            )
            .unwrap();
        graph
            .add_pass(
                RenderPassDesc::new("second use").use_resource(second, ResourceUsage::StorageWrite),
            )
            .unwrap();
        let plan = graph.compile().unwrap();
        assert_eq!(plan.transient_lifetimes().len(), 2);
        assert_eq!(plan.transient_alias_groups().len(), 1);
        assert_eq!(plan.transient_alias_groups()[0].resources, [first, second]);
        assert_eq!(plan.dependency_count(), 1);
        assert!(plan.passes()[1].barriers_before.iter().any(|barrier| {
            barrier.kind == RenderBarrierKind::Aliasing
                && barrier.resource == GraphResourceRef::Transient(second)
                && barrier.alias_before == Some(GraphResourceRef::Transient(first))
        }));
    }
}
