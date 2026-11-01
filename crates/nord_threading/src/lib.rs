//! Explicitly configured CPU worker pools for Nord Tech.
//!
//! Pools are owned values instead of the process-global Rayon pool, so startup
//! configuration and lifetime remain under the application's control.

mod pool;

pub use pool::{ThreadPool, ThreadPoolBuildError, ThreadPoolConfig};
