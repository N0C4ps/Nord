use nord_threading::ThreadPool;
use rayon::iter::IndexedParallelIterator;
use rayon::prelude::{
    IntoParallelIterator, IntoParallelRefIterator, IntoParallelRefMutIterator, ParallelIterator,
    ParallelSliceMut,
};
use std::num::NonZeroUsize;
use std::sync::Arc;

/// Game-facing parallel job operations backed by a Nord worker pool.
#[derive(Clone)]
pub struct JobSystem {
    pool: Arc<ThreadPool>,
}

impl JobSystem {
    /// Bind the job system to the application's explicitly owned pool.
    #[must_use]
    pub fn new(pool: Arc<ThreadPool>) -> Self {
        Self { pool }
    }

    /// Number of dedicated workers available to scheduled jobs.
    #[must_use]
    pub fn worker_threads(&self) -> NonZeroUsize {
        self.pool.worker_threads()
    }

    /// Run a read-only operation over a slice in parallel.
    ///
    /// Execution order is unspecified. The operation should be independent
    /// for each item and avoid blocking on work submitted to this same pool.
    pub fn for_each<T, Operation>(&self, items: &[T], operation: Operation)
    where
        T: Sync,
        Operation: Fn(&T) + Send + Sync,
    {
        let () = self.pool.install(|| items.par_iter().for_each(operation));
    }

    /// Run a disjoint mutable operation over a slice in parallel.
    ///
    /// Each item is exclusively borrowed by one invocation. Execution order
    /// is unspecified; shared mutable state still requires explicit sync.
    pub fn for_each_mut<T, Operation>(&self, items: &mut [T], operation: Operation)
    where
        T: Send,
        Operation: Fn(&mut T) + Send + Sync,
    {
        let () = self.pool.install(|| items.par_iter_mut().for_each(operation));
    }

    /// Map a slice in parallel, preserving the input order in the result.
    #[must_use]
    pub fn map<T, Output, Operation>(&self, items: &[T], operation: Operation) -> Vec<Output>
    where
        T: Sync,
        Output: Send,
        Operation: Fn(&T) -> Output + Send + Sync,
    {
        self.pool.install(|| items.par_iter().map(operation).collect())
    }

    /// Consume owned items in parallel, preserving their input order in the result.
    ///
    /// Use this when workers need ownership of per-item state, such as native
    /// command-list recording snapshots.
    #[must_use]
    pub fn map_owned<T, Output, Operation>(
        &self,
        items: Vec<T>,
        operation: Operation,
    ) -> Vec<Output>
    where
        T: Send,
        Output: Send,
        Operation: Fn(T) -> Output + Send + Sync,
    {
        self.pool.install(|| items.into_par_iter().map(operation).collect())
    }

    /// Partition a mutable slice into non-overlapping chunks and process them.
    ///
    /// The chunk index is stable, but execution order is unspecified. A
    /// non-zero chunk size is required so invalid scheduling parameters cannot
    /// panic inside Rayon.
    pub fn for_each_chunk_mut<T, Operation>(
        &self,
        items: &mut [T],
        chunk_size: NonZeroUsize,
        operation: Operation,
    ) where
        T: Send,
        Operation: Fn(usize, &mut [T]) + Send + Sync,
    {
        let () = self.pool.install(|| {
            items
                .par_chunks_mut(chunk_size.get())
                .enumerate()
                .for_each(|(index, chunk)| operation(index, chunk));
        });
    }

    /// Schedule two independent operations and return both results.
    ///
    /// Rayon may execute either operation on the calling thread or a worker;
    /// callers must not depend on a particular thread or execution order.
    #[must_use]
    pub fn join<Left, Right, LeftOperation, RightOperation>(
        &self,
        left: LeftOperation,
        right: RightOperation,
    ) -> (Left, Right)
    where
        Left: Send,
        Right: Send,
        LeftOperation: FnOnce() -> Left + Send,
        RightOperation: FnOnce() -> Right + Send,
    {
        self.pool.install(|| rayon::join(left, right))
    }
}
