//! Fence-aware suballocation bookkeeping shared by graphics backends.

use std::collections::{BTreeMap, VecDeque};

/// D3D12-compatible memory heap classes; heaps from different classes may not
/// be mixed because their CPU visibility and resource-state rules differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AllocationClass {
    /// Device-local non-render-target textures.
    DeviceLocal,
    /// Device-local buffers, isolated from texture heaps for D3D12 heap-tier compatibility.
    DeviceLocalBuffer,
    /// Device-local render/depth targets, isolated because D3D12 tier-1 heaps
    /// cannot mix these resources with ordinary buffers/non-RT textures.
    DeviceLocalRenderTarget,
    /// CPU-write upload data. A renderer's per-frame linear arena is preferred
    /// for short-lived data; this class serves persistent upload allocations.
    Upload,
    /// CPU-readable readback resources.
    Readback,
}

impl AllocationClass {
    const fn index(self) -> usize {
        match self {
            Self::DeviceLocal => 0,
            Self::DeviceLocalBuffer => 1,
            Self::DeviceLocalRenderTarget => 2,
            Self::Upload => 3,
            Self::Readback => 4,
        }
    }
}

/// Opaque suballocation coordinates. A backend maps `block` to a native heap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Allocation {
    block: u32,
    offset: u64,
    size: u64,
    class: AllocationClass,
}

impl Allocation {
    #[must_use]
    pub const fn block(self) -> u32 {
        self.block
    }
    #[must_use]
    pub const fn offset(self) -> u64 {
        self.offset
    }
    #[must_use]
    pub const fn size(self) -> u64 {
        self.size
    }
    #[must_use]
    pub const fn class(self) -> AllocationClass {
        self.class
    }
}

/// Aggregate allocator counters useful for telemetry and memory budgets.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GpuAllocatorStats {
    pub block_count: usize,
    pub reserved_bytes: u64,
    pub live_bytes: u64,
    pub peak_live_bytes: u64,
    pub allocation_count: usize,
}

struct Block {
    class: AllocationClass,
    size: u64,
    free: BTreeMap<u64, u64>,
}

/// Alignment-aware best-fit suballocator metadata. The native backend creates
/// one API heap per block and uses device-reported resource sizes and alignments.
/// Empty blocks are retained for reuse to avoid expensive heap churn.
pub struct GpuAllocator {
    preferred_block_sizes: [u64; 5],
    blocks: Vec<Block>,
    live: BTreeMap<(u32, u64), u64>,
    stats: GpuAllocatorStats,
}

impl GpuAllocator {
    /// Create allocator bookkeeping with a preferred native heap block size.
    #[must_use]
    pub const fn new(block_size: u64) -> Self {
        Self {
            preferred_block_sizes: [block_size; 5],
            blocks: Vec::new(),
            live: BTreeMap::new(),
            stats: GpuAllocatorStats {
                block_count: 0,
                reserved_bytes: 0,
                live_bytes: 0,
                peak_live_bytes: 0,
                allocation_count: 0,
            },
        }
    }

    /// Override the preferred heap size for one class before allocating from it.
    #[must_use]
    pub const fn with_class_block_size(mut self, class: AllocationClass, block_size: u64) -> Self {
        self.preferred_block_sizes[class.index()] = block_size;
        self
    }

    /// Reserve a range using the size and alignment reported by the graphics API.
    pub fn allocate(
        &mut self,
        class: AllocationClass,
        size: u64,
        alignment: u64,
    ) -> Option<Allocation> {
        if size == 0 || alignment == 0 || !alignment.is_power_of_two() {
            return None;
        }
        let mut choice = None;
        for (block_index, block) in self.blocks.iter().enumerate() {
            if block.class != class {
                continue;
            }
            for (&offset, &available) in &block.free {
                let aligned = align_up(offset, alignment)?;
                let end = aligned.checked_add(size)?;
                if end <= offset.checked_add(available)? {
                    let waste = available - size;
                    if choice.is_none_or(|(_, _, best_waste)| waste < best_waste) {
                        choice = Some((block_index, offset, waste));
                    }
                }
            }
        }
        if let Some((block_index, free_offset, _)) = choice {
            let block_id = u32::try_from(block_index).ok()?;
            let block = &mut self.blocks[block_index];
            let available = block.free.remove(&free_offset)?;
            let offset = align_up(free_offset, alignment)?;
            let end = offset.checked_add(size)?;
            let free_end = free_offset.checked_add(available)?;
            if offset > free_offset {
                block.free.insert(free_offset, offset - free_offset);
            }
            if end < free_end {
                block.free.insert(end, free_end - end);
            }
            return Some(self.track(Allocation { block: block_id, offset, size, class }));
        }
        let block_size = self.preferred_block_sizes[class.index()].max(align_up(size, alignment)?);
        let block_id = u32::try_from(self.blocks.len()).ok()?;
        let mut free = BTreeMap::new();
        if block_size > size {
            free.insert(size, block_size - size);
        }
        self.blocks.push(Block { class, size: block_size, free });
        self.stats.block_count = self.blocks.len();
        self.stats.reserved_bytes = self.stats.reserved_bytes.checked_add(block_size)?;
        Some(self.track(Allocation { block: block_id, offset: 0, size, class }))
    }

    /// Release a range only after the GPU has stopped using it.
    pub fn deallocate(&mut self, allocation: Allocation) -> bool {
        let key = (allocation.block, allocation.offset);
        if self.live.remove(&key) != Some(allocation.size) {
            return false;
        }
        let Some(block) = self.blocks.get_mut(allocation.block as usize) else { return false };
        if block.class != allocation.class {
            return false;
        }
        let mut start = allocation.offset;
        let mut end = start.saturating_add(allocation.size);
        if let Some((&previous_start, &previous_size)) = block.free.range(..start).next_back()
            && previous_start.saturating_add(previous_size) == start
        {
            start = previous_start;
            block.free.remove(&previous_start);
        }
        if let Some((&next_start, &next_size)) = block.free.range(end..).next()
            && end == next_start
        {
            end = end.saturating_add(next_size);
            block.free.remove(&next_start);
        }
        block.free.insert(start, end - start);
        self.stats.live_bytes = self.stats.live_bytes.saturating_sub(allocation.size);
        self.stats.allocation_count = self.live.len();
        true
    }

    /// Current allocator statistics.
    #[must_use]
    pub const fn stats(&self) -> GpuAllocatorStats {
        self.stats
    }

    /// Byte capacity of a block, useful when creating native heaps.
    #[must_use]
    pub fn block_size(&self, block: u32) -> Option<u64> {
        self.blocks.get(block as usize).map(|entry| entry.size)
    }

    /// Whether a block has no live allocations and is safe to return to its backend.
    #[must_use]
    pub fn is_block_empty(&self, block: u32) -> bool {
        self.blocks
            .get(block as usize)
            .is_some_and(|entry| entry.free.len() == 1 && entry.free.get(&0) == Some(&entry.size))
    }

    fn track(&mut self, allocation: Allocation) -> Allocation {
        self.live.insert((allocation.block, allocation.offset), allocation.size);
        self.stats.live_bytes = self.stats.live_bytes.saturating_add(allocation.size);
        self.stats.peak_live_bytes = self.stats.peak_live_bytes.max(self.stats.live_bytes);
        self.stats.allocation_count = self.live.len();
        allocation
    }
}

/// Deferred reclamation queue: allocations become reusable once their fence is
/// complete. Queue values must be submitted monotonically on the same queue.
#[derive(Default)]
pub struct DeferredGpuAllocator(VecDeque<(u64, Allocation)>);

impl DeferredGpuAllocator {
    /// Queue a range for reuse only after the submitted GPU fence completes.
    pub fn retire(&mut self, fence: u64, allocation: Allocation) {
        self.0.push_back((fence, allocation));
    }
    /// Reclaim all FIFO allocations whose fence values have completed.
    pub fn collect(&mut self, completed_fence: u64, allocator: &mut GpuAllocator) -> usize {
        let mut count = 0;
        while self.0.front().is_some_and(|(fence, _)| *fence <= completed_fence) {
            if let Some((_, allocation)) = self.0.pop_front() {
                allocator.deallocate(allocation);
                count += 1;
            }
        }
        count
    }
}

fn align_up(value: u64, alignment: u64) -> Option<u64> {
    value.checked_add(alignment - 1).map(|n| n & !(alignment - 1))
}

#[cfg(test)]
mod tests {
    use super::{AllocationClass, DeferredGpuAllocator, GpuAllocator};

    #[test]
    fn aligned_ranges_coalesce_and_reuse_the_block() {
        let mut allocator = GpuAllocator::new(1_024);
        let first = allocator.allocate(AllocationClass::DeviceLocal, 100, 64).unwrap();
        let second = allocator.allocate(AllocationClass::DeviceLocal, 100, 256).unwrap();

        assert_eq!(first.offset(), 0);
        assert_eq!(second.offset() % 256, 0);
        assert_ne!(first.offset(), second.offset());
        assert_eq!(allocator.stats().allocation_count, 2);
        assert!(!allocator.is_block_empty(first.block()));

        assert!(allocator.deallocate(first));
        assert!(allocator.deallocate(second));
        assert!(allocator.is_block_empty(first.block()));
        assert_eq!(allocator.stats().allocation_count, 0);
        let reused = allocator.allocate(AllocationClass::DeviceLocal, 1_024, 256).unwrap();
        assert_eq!(reused.block(), first.block());
        assert_eq!(reused.offset(), 0);
        assert_eq!(allocator.stats().block_count, 1);
    }

    #[test]
    fn deferred_ranges_wait_for_their_fence() {
        let mut allocator = GpuAllocator::new(256);
        let allocation = allocator.allocate(AllocationClass::Upload, 64, 16).unwrap();
        let mut deferred = DeferredGpuAllocator::default();
        deferred.retire(8, allocation);

        assert_eq!(deferred.collect(7, &mut allocator), 0);
        assert_eq!(allocator.stats().live_bytes, 64);
        assert_eq!(deferred.collect(8, &mut allocator), 1);
        assert_eq!(allocator.stats().live_bytes, 0);
        assert!(allocator.is_block_empty(allocation.block()));
    }

    #[test]
    fn preferred_heap_sizes_are_configurable_per_class() {
        let mut allocator = GpuAllocator::new(1_024)
            .with_class_block_size(AllocationClass::Upload, 256)
            .with_class_block_size(AllocationClass::Readback, 128);
        let upload = allocator.allocate(AllocationClass::Upload, 64, 16).unwrap();
        let readback = allocator.allocate(AllocationClass::Readback, 64, 16).unwrap();
        let device_local = allocator.allocate(AllocationClass::DeviceLocalBuffer, 64, 16).unwrap();

        assert_eq!(allocator.block_size(upload.block()), Some(256));
        assert_eq!(allocator.block_size(readback.block()), Some(128));
        assert_eq!(allocator.block_size(device_local.block()), Some(1_024));
    }
}
