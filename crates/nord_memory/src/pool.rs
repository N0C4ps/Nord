use std::collections::TryReserveError;

/// A generational key for a value held by an [`ObjectPool`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PoolHandle {
    index: usize,
    generation: u64,
}

impl PoolHandle {
    /// Slot index inside the pool.
    #[must_use]
    pub const fn index(self) -> usize {
        self.index
    }

    /// Generation used to reject stale handles after slot reuse.
    #[must_use]
    pub const fn generation(self) -> u64 {
        self.generation
    }
}

/// Reusable, typed storage for objects with stable generational handles.
///
/// This pool is single-owner and not internally synchronized. Reserve capacity
/// ahead of hot paths with [`Self::try_reserve`] to avoid growth during play.
pub struct ObjectPool<T> {
    slots: Vec<Slot<T>>,
    free_head: Option<usize>,
    len: usize,
}

struct Slot<T> {
    generation: u64,
    value: Option<T>,
    next_free: Option<usize>,
}

impl<T> ObjectPool<T> {
    /// Create an empty object pool.
    #[must_use]
    pub const fn new() -> Self {
        Self { slots: Vec::new(), free_head: None, len: 0 }
    }

    /// Ensure room for at least `additional` more slots without panicking.
    ///
    /// # Errors
    ///
    /// Returns the standard capacity error if the backing vector cannot grow.
    pub fn try_reserve(&mut self, additional: usize) -> Result<(), TryReserveError> {
        self.slots.try_reserve(additional)
    }

    /// Insert a value, reusing a free slot when possible.
    ///
    /// # Errors
    ///
    /// Returns [`PoolInsertError`] with the original value if slot storage
    /// cannot grow.
    pub fn try_insert(&mut self, value: T) -> Result<PoolHandle, PoolInsertError<T>> {
        if let Some(index) = self.free_head {
            let slot = &mut self.slots[index];
            self.free_head = slot.next_free.take();
            slot.value = Some(value);
            self.len += 1;
            return Ok(PoolHandle { index, generation: slot.generation });
        }

        if let Err(source) = self.slots.try_reserve(1) {
            return Err(PoolInsertError { value, source });
        }
        let index = self.slots.len();
        self.slots.push(Slot { generation: 0, value: Some(value), next_free: None });
        self.len += 1;
        Ok(PoolHandle { index, generation: 0 })
    }

    /// Get a shared reference if the handle still identifies a live value.
    #[must_use]
    pub fn get(&self, handle: PoolHandle) -> Option<&T> {
        let slot = self.slots.get(handle.index)?;
        (slot.generation == handle.generation).then_some(slot.value.as_ref()).flatten()
    }

    /// Get a mutable reference if the handle still identifies a live value.
    pub fn get_mut(&mut self, handle: PoolHandle) -> Option<&mut T> {
        let slot = self.slots.get_mut(handle.index)?;
        (slot.generation == handle.generation).then_some(slot.value.as_mut()).flatten()
    }

    /// Remove a value and invalidate its handle.
    pub fn remove(&mut self, handle: PoolHandle) -> Option<T> {
        let slot = self.slots.get_mut(handle.index)?;
        if slot.generation != handle.generation {
            return None;
        }
        let value = slot.value.take()?;
        self.len -= 1;

        if slot.generation == u64::MAX {
            // Retire this slot instead of allowing a stale handle to become valid again.
        } else {
            slot.generation += 1;
            slot.next_free = self.free_head;
            self.free_head = Some(handle.index);
        }
        Some(value)
    }

    /// Number of live values.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Whether the pool contains no live values.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Number of slots allocated, including free and retired slots.
    #[must_use]
    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    /// Capacity reserved for slots before the backing vector grows again.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.slots.capacity()
    }
}

impl<T> Default for ObjectPool<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Failed pool insertion that preserves ownership of its value.
#[derive(Debug)]
pub struct PoolInsertError<T> {
    /// Value that could not be inserted.
    pub value: T,
    /// Backing vector capacity failure.
    pub source: TryReserveError,
}

impl<T> std::fmt::Display for PoolInsertError<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "object pool could not grow its slot storage: {}", self.source)
    }
}

impl<T: std::fmt::Debug> std::error::Error for PoolInsertError<T> {}

#[cfg(test)]
mod tests {
    use super::ObjectPool;

    #[test]
    fn reused_slots_advance_generation_and_reject_stale_handles() {
        let mut pool = ObjectPool::new();
        pool.try_reserve(1).unwrap();
        let first = pool.try_insert("first").unwrap();
        assert_eq!(pool.remove(first), Some("first"));
        assert_eq!(pool.get(first), None);

        let second = pool.try_insert("second").unwrap();
        assert_eq!(second.index(), first.index());
        assert_eq!(second.generation(), first.generation() + 1);
        assert_eq!(pool.get(first), None);
        assert_eq!(pool.get(second), Some(&"second"));
        assert_eq!(pool.len(), 1);
    }
}
