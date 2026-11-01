//! Stable application-facing facade over Nord's memory services.

pub use nord_memory::{
    ArenaAllocationError as ApplicationArenaAllocationError,
    MemoryBudgetError as ApplicationMemoryBudgetError,
    MemoryBudgetSnapshot as ApplicationMemoryBudgetSnapshot,
    MemoryReservation as ApplicationMemoryReservation, ObjectPool as ApplicationObjectPool,
    PoolHandle as ApplicationPoolHandle, PoolInsertError as ApplicationPoolInsertError,
    ScratchArena as ApplicationScratchArena,
};

/// Application-owned entry point for budgets, arenas, and typed pools.
#[derive(Debug, Clone)]
pub struct ApplicationMemoryManager {
    manager: nord_memory::MemoryManager,
}

impl ApplicationMemoryManager {
    /// Create the manager with an optional shared reservation limit.
    #[must_use]
    pub fn new(limit_bytes: Option<u64>) -> Self {
        Self { manager: nord_memory::MemoryManager::new(limit_bytes) }
    }

    /// Reserve bytes against the application's shared memory budget.
    ///
    /// # Errors
    ///
    /// Returns a budget error if the reservation exceeds the configured limit
    /// or its accounting counter overflows.
    pub fn reserve(
        &self,
        bytes: u64,
    ) -> Result<ApplicationMemoryReservation, ApplicationMemoryBudgetError> {
        self.manager.reserve(bytes)
    }

    /// Get current, peak, and configured budget values.
    #[must_use]
    pub fn snapshot(&self) -> ApplicationMemoryBudgetSnapshot {
        self.manager.snapshot()
    }

    /// Create a scratch arena for a bounded frame, job, or loading phase.
    #[must_use]
    pub fn create_scratch_arena(&self, byte_limit: Option<usize>) -> ApplicationScratchArena {
        self.manager.create_scratch_arena(byte_limit)
    }

    /// Create a typed pool for reusable workload-owned objects.
    #[must_use]
    pub const fn create_pool<T>(&self) -> ApplicationObjectPool<T> {
        self.manager.create_pool()
    }
}
