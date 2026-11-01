use crate::{
    MemoryBudget, MemoryBudgetError, MemoryBudgetSnapshot, MemoryReservation, ObjectPool,
    ScratchArena,
};

/// Application-owned factory and accounting root for memory services.
#[derive(Debug, Clone)]
pub struct MemoryManager {
    budget: MemoryBudget,
}

impl MemoryManager {
    /// Create a memory manager with an optional explicit reservation limit.
    #[must_use]
    pub fn new(limit_bytes: Option<u64>) -> Self {
        Self { budget: MemoryBudget::new(limit_bytes) }
    }

    /// Reserve bytes against the manager's shared accounting budget.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryBudgetError`] if the reservation exceeds the configured
    /// limit or accounting overflows.
    pub fn reserve(&self, bytes: u64) -> Result<MemoryReservation, MemoryBudgetError> {
        self.budget.reserve(bytes)
    }

    /// Read current and peak reservation counters.
    #[must_use]
    pub fn snapshot(&self) -> MemoryBudgetSnapshot {
        self.budget.snapshot()
    }

    /// Create an independent scratch arena for one frame, job, or load phase.
    #[must_use]
    pub fn create_scratch_arena(&self, byte_limit: Option<usize>) -> ScratchArena {
        ScratchArena::new(byte_limit)
    }

    /// Create a typed reusable pool for a workload-owned object type.
    #[must_use]
    pub const fn create_pool<T>(&self) -> ObjectPool<T> {
        ObjectPool::new()
    }
}

impl Default for MemoryManager {
    fn default() -> Self {
        Self::new(None)
    }
}
