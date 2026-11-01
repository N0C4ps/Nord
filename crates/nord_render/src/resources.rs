//! Backend-neutral descriptions and generation-checked resource identifiers.

use std::num::NonZeroU32;

/// Stale-safe identifier for a buffer, texture, or render target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GpuResourceHandle {
    index: u32,
    generation: NonZeroU32,
    kind: ResourceKind,
}
impl GpuResourceHandle {
    #[must_use]
    pub const fn index(self) -> u32 {
        self.index
    }
    #[must_use]
    pub const fn generation(self) -> NonZeroU32 {
        self.generation
    }
    #[must_use]
    pub const fn kind(self) -> ResourceKind {
        self.kind
    }

    pub(crate) const fn new(index: u32, generation: NonZeroU32, kind: ResourceKind) -> Self {
        Self { index, generation, kind }
    }
}

/// GPU resource category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceKind {
    Buffer,
    Texture,
    RenderTarget,
}

/// Buffer access patterns; backends use this to select heap and resource flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BufferUsage(u32);
impl BufferUsage {
    pub const COPY_SOURCE: Self = Self(1 << 0);
    pub const COPY_DESTINATION: Self = Self(1 << 1);
    pub const VERTEX: Self = Self(1 << 2);
    pub const INDEX: Self = Self(1 << 3);
    pub const CONSTANT: Self = Self(1 << 4);
    pub const STORAGE: Self = Self(1 << 5);
    /// Resource may be consumed by a command signature through `ExecuteIndirect`.
    pub const INDIRECT_ARGUMENT: Self = Self(1 << 6);
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
    #[must_use]
    pub const fn bits(self) -> u32 {
        self.0
    }
}

/// Backing-memory access policy for a buffer allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BufferMemory {
    /// GPU-local memory for frequently accessed resources.
    DeviceLocal,
    /// CPU-write/GPU-read memory for upload data.
    Upload,
    /// GPU-write/CPU-read memory for copy results.
    Readback,
}

/// Validated buffer allocation request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BufferDesc {
    size: u64,
    usage: BufferUsage,
    memory: BufferMemory,
}
impl BufferDesc {
    /// # Errors
    /// Returns `ZeroSize` for an empty buffer and `EmptyUsage` without a usage.
    pub fn new(size: u64, usage: BufferUsage) -> Result<Self, ResourceDescriptionError> {
        if size == 0 {
            return Err(ResourceDescriptionError::ZeroSize);
        }
        if usage.bits() == 0 {
            return Err(ResourceDescriptionError::EmptyUsage);
        }
        Ok(Self { size, usage, memory: BufferMemory::DeviceLocal })
    }
    /// Select the memory policy used by the graphics backend.
    #[must_use]
    pub const fn with_memory(mut self, memory: BufferMemory) -> Self {
        self.memory = memory;
        self
    }
    /// Compatibility helper selecting CPU-write upload or device-local memory.
    #[must_use]
    pub const fn with_cpu_visibility(mut self, visible: bool) -> Self {
        self.memory = if visible { BufferMemory::Upload } else { BufferMemory::DeviceLocal };
        self
    }
    #[must_use]
    pub const fn size(self) -> u64 {
        self.size
    }
    #[must_use]
    pub const fn usage(self) -> BufferUsage {
        self.usage
    }
    #[must_use]
    pub const fn cpu_visible(self) -> bool {
        matches!(self.memory, BufferMemory::Upload)
    }
    /// Backing-memory access policy.
    #[must_use]
    pub const fn memory(self) -> BufferMemory {
        self.memory
    }
}

/// Supported color/depth formats, independent of a graphics API's enum values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextureFormat {
    Rgba8Unorm,
    Rgba8Srgb,
    Bgra8Unorm,
    R16Float,
    R32Float,
    Rg16Float,
    Rgba16Float,
    D32Float,
    D24UnormS8,
}

/// Texture usage flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureUsage(u32);
impl TextureUsage {
    pub const SAMPLED: Self = Self(1 << 0);
    pub const STORAGE: Self = Self(1 << 1);
    pub const COLOR_ATTACHMENT: Self = Self(1 << 2);
    pub const DEPTH_STENCIL_ATTACHMENT: Self = Self(1 << 3);
    pub const COPY_SOURCE: Self = Self(1 << 4);
    pub const COPY_DESTINATION: Self = Self(1 << 5);
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
    #[must_use]
    pub const fn bits(self) -> u32 {
        self.0
    }
}

/// Validated two-dimensional texture allocation request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureDesc {
    width: u32,
    height: u32,
    mip_levels: u16,
    array_layers: u16,
    samples: u8,
    format: TextureFormat,
    usage: TextureUsage,
}
impl TextureDesc {
    /// # Errors
    /// Returns an error for empty dimensions/usage or an incompatible depth format.
    pub fn new(
        width: u32,
        height: u32,
        format: TextureFormat,
        usage: TextureUsage,
    ) -> Result<Self, ResourceDescriptionError> {
        if width == 0 || height == 0 {
            return Err(ResourceDescriptionError::ZeroExtent);
        }
        if usage.bits() == 0 {
            return Err(ResourceDescriptionError::EmptyUsage);
        }
        let depth_format = matches!(format, TextureFormat::D32Float | TextureFormat::D24UnormS8);
        let depth_attachment = usage.contains(TextureUsage::DEPTH_STENCIL_ATTACHMENT);
        let color_attachment = usage.contains(TextureUsage::COLOR_ATTACHMENT);
        let storage = usage.contains(TextureUsage::STORAGE);
        if depth_attachment != depth_format
            || (depth_format && (storage || usage.contains(TextureUsage::SAMPLED)))
            || (depth_attachment && color_attachment)
            || (storage && color_attachment)
            || (storage && format == TextureFormat::Rgba8Srgb)
        {
            return Err(ResourceDescriptionError::IncompatibleFormatUsage);
        }
        Ok(Self { width, height, mip_levels: 1, array_layers: 1, samples: 1, format, usage })
    }
    /// # Errors
    /// Returns `InvalidMipCount` if the level count exceeds the full mip chain.
    pub fn with_mip_levels(mut self, levels: u16) -> Result<Self, ResourceDescriptionError> {
        let maximum = u16::try_from(u32::BITS - self.width.max(self.height).leading_zeros())
            .unwrap_or(u16::MAX);
        if levels == 0 || levels > maximum {
            return Err(ResourceDescriptionError::InvalidMipCount);
        }
        self.mip_levels = levels;
        Ok(self)
    }
    /// # Errors
    /// Returns `ZeroArrayLayers` when the requested array is empty.
    pub fn with_array_layers(mut self, layers: u16) -> Result<Self, ResourceDescriptionError> {
        if layers == 0 {
            return Err(ResourceDescriptionError::ZeroArrayLayers);
        }
        self.array_layers = layers;
        Ok(self)
    }
    /// # Errors
    /// Returns an error for unsupported sample counts or multisampled mip chains.
    pub fn with_samples(mut self, samples: u8) -> Result<Self, ResourceDescriptionError> {
        if !matches!(samples, 1 | 2 | 4 | 8) {
            return Err(ResourceDescriptionError::InvalidSampleCount);
        }
        if samples > 1 && self.mip_levels != 1 {
            return Err(ResourceDescriptionError::MultisampleMipmaps);
        }
        if samples > 1
            && (!self.usage.contains(TextureUsage::COLOR_ATTACHMENT)
                && !self.usage.contains(TextureUsage::DEPTH_STENCIL_ATTACHMENT)
                || self.usage.contains(TextureUsage::STORAGE))
        {
            return Err(ResourceDescriptionError::IncompatibleFormatUsage);
        }
        self.samples = samples;
        Ok(self)
    }
    #[must_use]
    pub const fn width(self) -> u32 {
        self.width
    }
    #[must_use]
    pub const fn height(self) -> u32 {
        self.height
    }
    #[must_use]
    pub const fn mip_levels(self) -> u16 {
        self.mip_levels
    }
    #[must_use]
    pub const fn array_layers(self) -> u16 {
        self.array_layers
    }
    #[must_use]
    pub const fn samples(self) -> u8 {
        self.samples
    }
    #[must_use]
    pub const fn format(self) -> TextureFormat {
        self.format
    }
    #[must_use]
    pub const fn usage(self) -> TextureUsage {
        self.usage
    }
}

/// Color/depth attachments belonging to a render target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RenderTargetDesc {
    width: u32,
    height: u32,
    color: Option<TextureFormat>,
    depth: Option<TextureFormat>,
    samples: u8,
}

/// GPU color/depth textures created for one render target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderTargetResources {
    pub color: Option<GpuResourceHandle>,
    pub depth: Option<GpuResourceHandle>,
}
impl RenderTargetDesc {
    /// # Errors
    /// Returns an error for empty extents, missing attachments, or invalid formats.
    pub fn new(
        width: u32,
        height: u32,
        color: Option<TextureFormat>,
        depth: Option<TextureFormat>,
    ) -> Result<Self, ResourceDescriptionError> {
        if width == 0 || height == 0 {
            return Err(ResourceDescriptionError::ZeroExtent);
        }
        if color.is_none() && depth.is_none() {
            return Err(ResourceDescriptionError::NoAttachments);
        }
        if color.is_some_and(|format| {
            matches!(format, TextureFormat::D32Float | TextureFormat::D24UnormS8)
        }) || depth.is_some_and(|format| {
            !matches!(format, TextureFormat::D32Float | TextureFormat::D24UnormS8)
        }) {
            return Err(ResourceDescriptionError::IncompatibleFormatUsage);
        }
        Ok(Self { width, height, color, depth, samples: 1 })
    }
    /// # Errors
    /// Returns `InvalidSampleCount` for unsupported multisample counts.
    pub fn with_samples(mut self, samples: u8) -> Result<Self, ResourceDescriptionError> {
        if !matches!(samples, 1 | 2 | 4 | 8) {
            return Err(ResourceDescriptionError::InvalidSampleCount);
        }
        self.samples = samples;
        Ok(self)
    }
    #[must_use]
    pub const fn extent(self) -> (u32, u32) {
        (self.width, self.height)
    }
    #[must_use]
    pub const fn color_format(self) -> Option<TextureFormat> {
        self.color
    }
    #[must_use]
    pub const fn depth_format(self) -> Option<TextureFormat> {
        self.depth
    }
    #[must_use]
    pub const fn samples(self) -> u8 {
        self.samples
    }
}

/// Invalid resource description rejected before reaching a graphics driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceDescriptionError {
    ZeroSize,
    ZeroExtent,
    EmptyUsage,
    InvalidMipCount,
    ZeroArrayLayers,
    InvalidSampleCount,
    MultisampleMipmaps,
    IncompatibleFormatUsage,
    NoAttachments,
}
impl std::fmt::Display for ResourceDescriptionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid GPU resource description: {self:?}")
    }
}
impl std::error::Error for ResourceDescriptionError {}

#[cfg(test)]
mod tests {
    use super::{
        BufferDesc, BufferUsage, RenderTargetDesc, ResourceDescriptionError, TextureDesc,
        TextureFormat, TextureUsage,
    };

    #[test]
    fn rejects_empty_and_incompatible_descriptions() {
        assert_eq!(
            BufferDesc::new(0, BufferUsage::VERTEX),
            Err(ResourceDescriptionError::ZeroSize)
        );
        assert_eq!(
            BufferDesc::new(16, BufferUsage::empty()),
            Err(ResourceDescriptionError::EmptyUsage)
        );
        assert_eq!(
            TextureDesc::new(4, 4, TextureFormat::D32Float, TextureUsage::SAMPLED),
            Err(ResourceDescriptionError::IncompatibleFormatUsage)
        );
        assert_eq!(
            TextureDesc::new(4, 4, TextureFormat::Rgba8Srgb, TextureUsage::STORAGE),
            Err(ResourceDescriptionError::IncompatibleFormatUsage)
        );
        assert_eq!(
            RenderTargetDesc::new(4, 4, None, None),
            Err(ResourceDescriptionError::NoAttachments)
        );
    }

    #[test]
    fn validates_mips_arrays_samples_and_attachment_formats() {
        let texture = TextureDesc::new(8, 4, TextureFormat::Rgba8Unorm, TextureUsage::SAMPLED)
            .unwrap()
            .with_mip_levels(4)
            .unwrap()
            .with_array_layers(6)
            .unwrap();
        assert_eq!(texture.mip_levels(), 4);
        assert_eq!(texture.array_layers(), 6);
        assert_eq!(texture.with_mip_levels(5), Err(ResourceDescriptionError::InvalidMipCount));
        assert_eq!(
            TextureDesc::new(8, 8, TextureFormat::Rgba8Unorm, TextureUsage::SAMPLED)
                .unwrap()
                .with_samples(4),
            Err(ResourceDescriptionError::IncompatibleFormatUsage)
        );
        assert!(
            RenderTargetDesc::new(
                8,
                8,
                Some(TextureFormat::Rgba8Unorm),
                Some(TextureFormat::D32Float)
            )
            .is_ok()
        );
    }
}
