//! Immutable graphics-pipeline descriptions and runtime cache keys.

use crate::TextureFormat;
use std::collections::{HashMap, HashSet};

/// Shader execution stage for pipeline construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShaderStage {
    Vertex,
    Pixel,
    Compute,
}

/// Stable BLAKE3 identity of compiled shader bytecode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ShaderId(pub [u8; 32]);

/// Primitive type processed by the input assembler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrimitiveTopology {
    Point,
    Line,
    Triangle,
}

/// Rasterizer face culling mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CullMode {
    None,
    Front,
    Back,
}

/// Front-face winding convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FrontFace {
    Clockwise,
    CounterClockwise,
}

/// Polygon fill mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PolygonFillMode {
    Solid,
    Wireframe,
}

/// Input-assembler attribute format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VertexFormat {
    Float,
    Float2,
    Float3,
    Float4,
    Uint8x4,
    Uint16x2,
    Uint32,
}

impl VertexFormat {
    #[must_use]
    pub const fn byte_size(self) -> u32 {
        match self {
            Self::Float | Self::Uint32 | Self::Uint8x4 | Self::Uint16x2 => 4,
            Self::Float2 => 8,
            Self::Float3 => 12,
            Self::Float4 => 16,
        }
    }

    #[must_use]
    pub const fn alignment(self) -> u32 {
        match self {
            Self::Uint8x4 => 1,
            Self::Uint16x2 => 2,
            Self::Float | Self::Float2 | Self::Float3 | Self::Float4 | Self::Uint32 => 4,
        }
    }
}

/// One input element in a vertex buffer layout.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VertexAttribute {
    pub semantic: String,
    pub semantic_index: u32,
    pub format: VertexFormat,
    pub input_slot: u32,
    pub byte_offset: u32,
    pub per_instance: bool,
    pub instance_step_rate: u32,
}

/// Immutable graphics-pipeline description used to create and key a native PSO.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GraphicsPipelineDesc {
    pub vertex_shader: ShaderId,
    pub pixel_shader: ShaderId,
    pub color_format: TextureFormat,
    /// Additional pixel-shader outputs after `color_format` (`SV_Target1`+).
    pub additional_color_formats: Vec<TextureFormat>,
    pub depth_format: Option<TextureFormat>,
    pub sample_count: u8,
    pub depth_test: bool,
    pub depth_write: bool,
    pub alpha_blending: bool,
    pub topology: PrimitiveTopology,
    pub cull_mode: CullMode,
    pub front_face: FrontFace,
    pub fill_mode: PolygonFillMode,
    pub vertex_attributes: Vec<VertexAttribute>,
}

/// Immutable compute-pipeline identity derived from a compiled compute shader.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ComputePipelineDesc {
    pub compute_shader: ShaderId,
}

impl GraphicsPipelineDesc {
    /// Construct an opaque triangle pipeline for the supplied shader stages and
    /// color format. Optional states are changed with the public fields/builders.
    #[must_use]
    pub fn new(
        vertex_shader: ShaderId,
        pixel_shader: ShaderId,
        color_format: TextureFormat,
    ) -> Self {
        Self {
            vertex_shader,
            pixel_shader,
            color_format,
            additional_color_formats: Vec::new(),
            depth_format: None,
            sample_count: 1,
            depth_test: false,
            depth_write: false,
            alpha_blending: false,
            topology: PrimitiveTopology::Triangle,
            cull_mode: CullMode::Back,
            front_face: FrontFace::CounterClockwise,
            fill_mode: PolygonFillMode::Solid,
            vertex_attributes: Vec::new(),
        }
    }

    /// Append one vertex or instance input element.
    #[must_use]
    pub fn with_vertex_attribute(mut self, attribute: VertexAttribute) -> Self {
        self.vertex_attributes.push(attribute);
        self
    }

    /// Append another color attachment in pixel-shader output order.
    #[must_use]
    pub fn with_color_attachment_format(mut self, format: TextureFormat) -> Self {
        self.additional_color_formats.push(format);
        self
    }

    /// All color output formats in pixel-shader target order.
    pub fn color_formats(&self) -> impl Iterator<Item = TextureFormat> + '_ {
        std::iter::once(self.color_format).chain(self.additional_color_formats.iter().copied())
    }

    /// Reject invalid attachment formats and sample counts before PSO creation.
    ///
    /// # Errors
    /// Returns the specific incompatible sample, attachment, or depth-state error.
    pub fn validate(&self) -> Result<(), PipelineDescriptionError> {
        if !matches!(self.sample_count, 1 | 2 | 4 | 8) {
            return Err(PipelineDescriptionError::InvalidSampleCount);
        }
        if matches!(self.color_format, TextureFormat::D32Float | TextureFormat::D24UnormS8) {
            return Err(PipelineDescriptionError::ColorFormatIsDepth);
        }
        if self.additional_color_formats.len() >= 8
            || self
                .additional_color_formats
                .iter()
                .any(|format| matches!(format, TextureFormat::D32Float | TextureFormat::D24UnormS8))
        {
            return Err(PipelineDescriptionError::InvalidColorAttachmentFormats);
        }
        if self.depth_format.is_some_and(|format| {
            !matches!(format, TextureFormat::D32Float | TextureFormat::D24UnormS8)
        }) {
            return Err(PipelineDescriptionError::DepthFormatIsColor);
        }
        if (self.depth_test || self.depth_write) && self.depth_format.is_none() {
            return Err(PipelineDescriptionError::DepthStateWithoutAttachment);
        }
        let mut semantics = HashSet::new();
        let mut slots = HashMap::<u32, (bool, u32)>::new();
        let mut ranges = HashMap::<u32, Vec<(u32, u32)>>::new();
        for attribute in &self.vertex_attributes {
            if attribute.semantic.trim().is_empty()
                || attribute.input_slot >= 16
                || (attribute.per_instance && attribute.instance_step_rate == 0)
                || (!attribute.per_instance && attribute.instance_step_rate != 0)
                || !semantics
                    .insert((attribute.semantic.to_ascii_uppercase(), attribute.semantic_index))
                || slots
                    .insert(
                        attribute.input_slot,
                        (attribute.per_instance, attribute.instance_step_rate),
                    )
                    .is_some_and(|existing| {
                        existing != (attribute.per_instance, attribute.instance_step_rate)
                    })
            {
                return Err(PipelineDescriptionError::InvalidVertexLayout);
            }
            let size = attribute.format.byte_size();
            let end = attribute
                .byte_offset
                .checked_add(size)
                .ok_or(PipelineDescriptionError::InvalidVertexLayout)?;
            if !attribute.byte_offset.is_multiple_of(attribute.format.alignment()) {
                return Err(PipelineDescriptionError::InvalidVertexLayout);
            }
            let slot_ranges = ranges.entry(attribute.input_slot).or_default();
            if slot_ranges
                .iter()
                .any(|(start, existing_end)| attribute.byte_offset < *existing_end && *start < end)
            {
                return Err(PipelineDescriptionError::InvalidVertexLayout);
            }
            slot_ranges.push((attribute.byte_offset, end));
        }
        Ok(())
    }
}

/// Stable pipeline cache key; changing any bytecode or fixed-function choice
/// naturally selects a distinct native pipeline state.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PipelineKey(pub GraphicsPipelineDesc);

/// Runtime metadata about a cached native pipeline object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PipelineCacheEntry {
    pub native_id: u64,
    pub last_used_frame: u64,
}

/// Deduplicates backend pipeline states using complete immutable descriptions.
#[derive(Default)]
pub struct PipelineCache {
    entries: HashMap<PipelineKey, PipelineCacheEntry>,
    compute_entries: HashMap<ShaderId, PipelineCacheEntry>,
    next_id: u64,
}
impl PipelineCache {
    /// Get a cached entry, or create metadata for a backend PSO to attach.
    ///
    /// # Errors
    /// Returns a validation error for incompatible formats or depth state.
    pub fn get_or_insert(
        &mut self,
        desc: &GraphicsPipelineDesc,
        frame: u64,
    ) -> Result<PipelineCacheEntry, PipelineDescriptionError> {
        desc.validate()?;
        let key = PipelineKey(desc.clone());
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.last_used_frame = frame;
            return Ok(*entry);
        }
        self.next_id = self.next_id.checked_add(1).unwrap_or(1);
        let entry = PipelineCacheEntry { native_id: self.next_id, last_used_frame: frame };
        self.entries.insert(key, entry);
        Ok(entry)
    }
    /// Get cached compute-pipeline metadata or allocate a new identity.
    pub(crate) fn get_or_insert_compute(
        &mut self,
        shader: ShaderId,
        frame: u64,
    ) -> PipelineCacheEntry {
        if let Some(entry) = self.compute_entries.get_mut(&shader) {
            entry.last_used_frame = frame;
            return *entry;
        }
        self.next_id = self.next_id.checked_add(1).unwrap_or(1);
        let entry = PipelineCacheEntry { native_id: self.next_id, last_used_frame: frame };
        self.compute_entries.insert(shader, entry);
        entry
    }
    /// Drop stale metadata after the backend has retired corresponding native PSOs.
    pub(crate) fn retain_since(&mut self, minimum_frame: u64) -> Vec<PipelineKey> {
        let mut retired = Vec::new();
        self.entries.retain(|key, entry| {
            let keep = entry.last_used_frame >= minimum_frame;
            if !keep {
                retired.push(key.clone());
            }
            keep
        });
        retired
    }
    pub(crate) fn retain_compute_since(&mut self, minimum_frame: u64) -> Vec<ShaderId> {
        let mut retired = Vec::new();
        self.compute_entries.retain(|shader, entry| {
            let keep = entry.last_used_frame >= minimum_frame;
            if !keep {
                retired.push(*shader);
            }
            keep
        });
        retired
    }
    /// Number of cache entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len() + self.compute_entries.len()
    }
    /// Whether no pipeline descriptions are cached.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.compute_entries.is_empty()
    }
}

/// Invalid graphics-pipeline description.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineDescriptionError {
    InvalidSampleCount,
    ColorFormatIsDepth,
    DepthFormatIsColor,
    DepthStateWithoutAttachment,
    InvalidColorAttachmentFormats,
    InvalidVertexLayout,
}
impl std::fmt::Display for PipelineDescriptionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid graphics pipeline description: {self:?}")
    }
}
impl std::error::Error for PipelineDescriptionError {}

#[cfg(test)]
mod tests {
    use super::{
        CullMode, FrontFace, GraphicsPipelineDesc, PipelineCache, PipelineDescriptionError,
        PolygonFillMode, PrimitiveTopology, ShaderId, TextureFormat, VertexAttribute, VertexFormat,
    };

    fn description() -> GraphicsPipelineDesc {
        let mut description = GraphicsPipelineDesc::new(
            ShaderId([1; 32]),
            ShaderId([2; 32]),
            TextureFormat::Rgba8Unorm,
        );
        description.topology = PrimitiveTopology::Triangle;
        description.cull_mode = CullMode::Back;
        description.front_face = FrontFace::CounterClockwise;
        description.fill_mode = PolygonFillMode::Solid;
        description
    }

    #[test]
    fn cache_reuses_identity_and_tracks_last_used_frame() {
        let mut cache = PipelineCache::default();
        let first = cache.get_or_insert(&description(), 3).unwrap();
        let second = cache.get_or_insert(&description(), 9).unwrap();

        assert_eq!(first.native_id, second.native_id);
        assert_eq!(second.last_used_frame, 9);
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.retain_since(10).len(), 1);
        assert!(cache.is_empty());
    }

    #[test]
    fn rejects_depth_state_without_a_depth_attachment() {
        let mut description = description();
        description.depth_test = true;
        assert!(description.validate().is_err());
    }

    #[test]
    fn validates_multiple_color_output_formats_and_d3d12_limit() {
        let mut description =
            description().with_color_attachment_format(TextureFormat::Rgba16Float);
        assert_eq!(
            description.color_formats().collect::<Vec<_>>(),
            [TextureFormat::Rgba8Unorm, TextureFormat::Rgba16Float]
        );
        assert!(description.validate().is_ok());
        description.additional_color_formats = vec![TextureFormat::Rgba8Unorm; 8];
        assert_eq!(
            description.validate(),
            Err(PipelineDescriptionError::InvalidColorAttachmentFormats)
        );
    }

    #[test]
    fn validates_vertex_layout_semantics_slots_alignment_and_overlap() {
        let mut description = description();
        description.vertex_attributes = vec![
            VertexAttribute {
                semantic: "POSITION".into(),
                semantic_index: 0,
                format: VertexFormat::Float3,
                input_slot: 0,
                byte_offset: 0,
                per_instance: false,
                instance_step_rate: 0,
            },
            VertexAttribute {
                semantic: "TEXCOORD".into(),
                semantic_index: 0,
                format: VertexFormat::Float2,
                input_slot: 0,
                byte_offset: 12,
                per_instance: false,
                instance_step_rate: 0,
            },
        ];
        assert!(description.validate().is_ok());

        description.vertex_attributes[1].byte_offset = 8;
        assert_eq!(description.validate(), Err(PipelineDescriptionError::InvalidVertexLayout));

        description.vertex_attributes[1].byte_offset = 12;
        description.vertex_attributes[1].semantic = "position".into();
        assert_eq!(description.validate(), Err(PipelineDescriptionError::InvalidVertexLayout));
    }
}
