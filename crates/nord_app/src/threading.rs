//! Stable application facade for engine worker pools.

pub use nord_threading::{
    ThreadPool as ApplicationThreadPool, ThreadPoolBuildError as ApplicationThreadPoolBuildError,
    ThreadPoolConfig as ApplicationThreadPoolConfig,
};
