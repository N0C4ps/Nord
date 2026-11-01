//! Safe, explicit memory policies for engine and game workloads.
//!
//! The crate provides tracked reservations, phase-oriented scratch arenas,
//! and reusable typed object pools. It intentionally does not replace Rust's
//! process-global allocator or implement the unstable allocator API.

mod arena;
mod budget;
mod manager;
mod pool;

pub use arena::{ArenaAllocationError, ScratchArena};
pub use budget::{MemoryBudget, MemoryBudgetError, MemoryBudgetSnapshot, MemoryReservation};
pub use manager::MemoryManager;
pub use pool::{ObjectPool, PoolHandle, PoolInsertError};
