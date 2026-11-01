//! Stable, generation-checked descriptor-slot allocation.

use std::num::NonZeroU32;

/// Descriptor category. Shader-visible resource and sampler heaps stay separate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DescriptorKind {
    Resource,
    Sampler,
    RenderTarget,
    DepthStencil,
}

/// Filtering mode for a texture sampler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SamplerFilter {
    Point,
    Linear,
    Anisotropic,
}

/// Texture coordinate addressing at UV boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SamplerAddressMode {
    Clamp,
    Repeat,
    Mirror,
}

/// Backend-neutral sampler state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SamplerDesc {
    pub filter: SamplerFilter,
    pub address_u: SamplerAddressMode,
    pub address_v: SamplerAddressMode,
    pub address_w: SamplerAddressMode,
    pub max_anisotropy: u8,
    pub min_lod: f32,
    pub max_lod: f32,
}

impl SamplerDesc {
    /// Linear-repeat sampler suitable for tiling world textures.
    #[must_use]
    pub const fn linear_repeat() -> Self {
        Self {
            filter: SamplerFilter::Linear,
            address_u: SamplerAddressMode::Repeat,
            address_v: SamplerAddressMode::Repeat,
            address_w: SamplerAddressMode::Repeat,
            max_anisotropy: 1,
            min_lod: 0.0,
            max_lod: f32::MAX,
        }
    }

    /// Validate finite ordered LODs and the D3D12 anisotropy limit.
    ///
    /// # Errors
    /// Returns an error when LOD bounds are invalid or anisotropy is outside
    /// the supported range for the selected filter.
    pub fn validate(self) -> Result<Self, SamplerDescriptionError> {
        if !self.min_lod.is_finite()
            || self.min_lod < 0.0
            || !self.max_lod.is_finite()
            || self.max_lod < self.min_lod
        {
            return Err(SamplerDescriptionError::InvalidLodRange);
        }
        if self.max_anisotropy == 0 || self.max_anisotropy > 16 {
            return Err(SamplerDescriptionError::InvalidAnisotropy);
        }
        Ok(self)
    }
}

/// Invalid sampler configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SamplerDescriptionError {
    InvalidLodRange,
    InvalidAnisotropy,
}
impl std::fmt::Display for SamplerDescriptionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid sampler description: {self:?}")
    }
}
impl std::error::Error for SamplerDescriptionError {}

/// Opaque descriptor slot; contains no CPU or GPU native handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DescriptorHandle {
    index: u32,
    generation: NonZeroU32,
    kind: DescriptorKind,
}
impl DescriptorHandle {
    #[must_use]
    pub const fn index(self) -> u32 {
        self.index
    }
    #[must_use]
    pub const fn generation(self) -> NonZeroU32 {
        self.generation
    }
    #[must_use]
    pub const fn kind(self) -> DescriptorKind {
        self.kind
    }
}

#[derive(Clone, Copy)]
struct Slot {
    generation: NonZeroU32,
    allocated: bool,
}

/// Fixed-capacity descriptor allocator. The native backend owns the heaps;
/// this allocator owns stable slot IDs and stale-handle detection.
#[derive(Clone)]
pub struct DescriptorArena {
    slots: [Vec<Slot>; 4],
    free: [Vec<u32>; 4],
}
impl DescriptorArena {
    /// Create capacities for CBV/SRV/UAV, sampler, RTV, and DSV descriptors.
    #[must_use]
    pub fn new(resources: u32, samplers: u32, render_targets: u32, depth_stencils: u32) -> Self {
        let capacities = [resources, samplers, render_targets, depth_stencils];
        let slots = capacities.map(|capacity| {
            vec![Slot { generation: NonZeroU32::MIN, allocated: false }; capacity as usize]
        });
        let free = capacities.map(|capacity| (0..capacity).rev().collect());
        Self { slots, free }
    }
    /// Allocate one slot of a specific descriptor class.
    pub fn allocate(&mut self, kind: DescriptorKind) -> Option<DescriptorHandle> {
        let kind_index = index(kind);
        let slot_index = self.free[kind_index].pop()?;
        let slot = self.slots[kind_index].get_mut(slot_index as usize)?;
        slot.allocated = true;
        Some(DescriptorHandle { index: slot_index, generation: slot.generation, kind })
    }
    /// Allocate a contiguous range in one native descriptor heap.
    ///
    /// Descriptor tables are contiguous ranges in D3D12 heaps. Allocation is
    /// all-or-nothing: failure leaves every slot untouched.
    pub fn allocate_range(
        &mut self,
        kind: DescriptorKind,
        count: usize,
    ) -> Option<Vec<DescriptorHandle>> {
        if count == 0 {
            return None;
        }
        let kind_index = index(kind);
        let slots = &self.slots[kind_index];
        let first = (0..=slots.len().checked_sub(count)?)
            .find(|start| slots[*start..*start + count].iter().all(|slot| !slot.allocated))?;
        let range = &mut self.slots[kind_index][first..first + count];
        let first_index = u32::try_from(first).ok()?;
        let mut handles = Vec::with_capacity(count);
        for (offset, slot) in range.iter().enumerate() {
            let index = first_index.checked_add(u32::try_from(offset).ok()?)?;
            handles.push(DescriptorHandle { index, generation: slot.generation, kind });
        }
        for slot in range {
            slot.allocated = true;
        }
        self.free[kind_index].retain(|index| {
            let index = *index as usize;
            index < first || index >= first + count
        });
        Some(handles)
    }
    /// Release a slot after all GPU work referencing it has retired.
    pub fn release(&mut self, handle: DescriptorHandle) -> bool {
        let kind_index = index(handle.kind);
        let Some(slot) = self.slots[kind_index].get_mut(handle.index as usize) else {
            return false;
        };
        if !slot.allocated || slot.generation != handle.generation {
            return false;
        }
        slot.allocated = false;
        slot.generation =
            NonZeroU32::new(slot.generation.get().wrapping_add(1)).unwrap_or(NonZeroU32::MIN);
        self.free[kind_index].push(handle.index);
        true
    }
    /// Whether a handle currently names a live slot.
    #[must_use]
    pub fn is_live(&self, handle: DescriptorHandle) -> bool {
        self.slots[index(handle.kind)]
            .get(handle.index as usize)
            .is_some_and(|slot| slot.allocated && slot.generation == handle.generation)
    }
    /// Number of live slots in a descriptor class.
    #[must_use]
    pub fn allocated_count(&self, kind: DescriptorKind) -> usize {
        self.slots[index(kind)].iter().filter(|slot| slot.allocated).count()
    }
}

fn index(kind: DescriptorKind) -> usize {
    match kind {
        DescriptorKind::Resource => 0,
        DescriptorKind::Sampler => 1,
        DescriptorKind::RenderTarget => 2,
        DescriptorKind::DepthStencil => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::{DescriptorArena, DescriptorKind};

    #[test]
    fn capacity_and_kind_spaces_are_enforced() {
        let mut arena = DescriptorArena::new(1, 1, 0, 0);
        let resource = arena.allocate(DescriptorKind::Resource).unwrap();
        let sampler = arena.allocate(DescriptorKind::Sampler).unwrap();

        assert_eq!(resource.index(), 0);
        assert_eq!(sampler.index(), 0);
        assert!(arena.allocate(DescriptorKind::Resource).is_none());
        assert!(arena.is_live(resource));
        assert!(arena.is_live(sampler));
        assert_eq!(arena.allocated_count(DescriptorKind::Resource), 1);
        assert_eq!(arena.allocated_count(DescriptorKind::Sampler), 1);
    }

    #[test]
    fn released_handles_become_stale_when_a_slot_is_reused() {
        let mut arena = DescriptorArena::new(1, 0, 0, 0);
        let original = arena.allocate(DescriptorKind::Resource).unwrap();
        assert!(arena.release(original));
        assert!(!arena.is_live(original));
        assert!(!arena.release(original));

        let replacement = arena.allocate(DescriptorKind::Resource).unwrap();
        assert_eq!(replacement.index(), original.index());
        assert_ne!(replacement.generation(), original.generation());
        assert!(arena.is_live(replacement));
    }

    #[test]
    fn contiguous_ranges_are_atomic_and_reusable() {
        let mut arena = DescriptorArena::new(5, 0, 0, 0);
        let first = arena.allocate_range(DescriptorKind::Resource, 3).unwrap();
        assert_eq!(first.iter().map(|item| item.index()).collect::<Vec<_>>(), [0, 1, 2]);
        assert!(arena.allocate_range(DescriptorKind::Resource, 3).is_none());
        assert_eq!(arena.allocated_count(DescriptorKind::Resource), 3);
        for handle in &first {
            assert!(arena.release(*handle));
        }
        let second = arena.allocate_range(DescriptorKind::Resource, 4).unwrap();
        assert_eq!(second.iter().map(|item| item.index()).collect::<Vec<_>>(), [0, 1, 2, 3]);
        assert!(first.iter().all(|item| !arena.is_live(*item)));
        assert!(arena.allocate_range(DescriptorKind::Resource, 0).is_none());
    }

    #[test]
    fn sampler_descriptions_reject_non_finite_lod_bounds() {
        let mut sampler = super::SamplerDesc::linear_repeat();
        sampler.max_lod = f32::INFINITY;
        assert_eq!(sampler.validate(), Err(super::SamplerDescriptionError::InvalidLodRange));
        sampler.max_lod = f32::NAN;
        assert_eq!(sampler.validate(), Err(super::SamplerDescriptionError::InvalidLodRange));
    }
}
