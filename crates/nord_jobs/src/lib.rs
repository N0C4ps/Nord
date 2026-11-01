//! Data-parallel jobs and task scheduling on Nord's owned worker pool.
//!
//! Rayon supplies the initial work-stealing scheduler. Nord owns the stable
//! job API and can replace or extend scheduling policy when measured game
//! workloads need priorities, cancellation, or dedicated task classes.

mod system;

pub use system::JobSystem;
