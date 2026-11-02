use bumpalo::Bump;
use std::cell::Cell;

/// Phase-oriented arena for frame, job, or loading scratch allocations.
///
/// The arena is intentionally not thread-safe; create one per worker or phase.
/// Values allocated here are not dropped by reset or arena destruction.
pub struct ScratchArena {
    bump: Bump,
    byte_limit: Option<usize>,
    payload_bytes: Cell<usize>,
}

impl ScratchArena {
    /// Create a scratch arena with an optional payload-byte limit.
    #[must_use]
    pub fn new(byte_limit: Option<usize>) -> Self {
        let bump = Bump::new();
        Self { bump, byte_limit, payload_bytes: Cell::new(0) }
    }

    /// Allocate one value, returning an error instead of panicking on failure.
    ///
    /// The arena counts the value's payload bytes; allocator metadata and
    /// alignment padding are not included in this logical limit.
    ///
    /// # Errors
    ///
    /// Returns [`ArenaAllocationError`] when the payload limit would be
    /// exceeded or the underlying arena cannot allocate another chunk.
    pub fn try_alloc<T>(&self, value: T) -> Result<&mut T, ArenaAllocationError> {
        let bytes = size_of::<T>();
        let next = self.reserve_payload(bytes)?;
        match self.bump.try_alloc(value) {
            Ok(value) => {
                self.payload_bytes.set(next);
                Ok(value)
            }
            Err(_) => Err(ArenaAllocationError {
                requested_bytes: bytes,
                allocated_payload_bytes: self.payload_bytes.get(),
                limit_bytes: self.byte_limit,
            }),
        }
    }

    /// Copy a slice into the arena, with checked byte accounting.
    ///
    /// # Errors
    ///
    /// Returns [`ArenaAllocationError`] if size calculation, the configured
    /// payload limit, or the underlying arena allocation fails.
    pub fn try_alloc_slice_copy<T: Copy>(
        &self,
        values: &[T],
    ) -> Result<&mut [T], ArenaAllocationError> {
        let bytes =
            values.len().checked_mul(size_of::<T>()).ok_or_else(|| ArenaAllocationError {
                requested_bytes: usize::MAX,
                allocated_payload_bytes: self.payload_bytes.get(),
                limit_bytes: self.byte_limit,
            })?;
        let next = self.reserve_payload(bytes)?;
        match self.bump.try_alloc_slice_copy(values) {
            Ok(slice) => {
                self.payload_bytes.set(next);
                Ok(slice)
            }
            Err(_) => Err(ArenaAllocationError {
                requested_bytes: bytes,
                allocated_payload_bytes: self.payload_bytes.get(),
                limit_bytes: self.byte_limit,
            }),
        }
    }

    /// Copy a string into the arena.
    ///
    /// # Errors
    ///
    /// Returns [`ArenaAllocationError`] when the payload limit or the
    /// underlying arena allocation fails.
    pub fn try_alloc_str(&self, value: &str) -> Result<&mut str, ArenaAllocationError> {
        let next = self.reserve_payload(value.len())?;
        match self.bump.try_alloc_str(value) {
            Ok(value) => {
                self.payload_bytes.set(next);
                Ok(value)
            }
            Err(_) => Err(ArenaAllocationError {
                requested_bytes: value.len(),
                allocated_payload_bytes: self.payload_bytes.get(),
                limit_bytes: self.byte_limit,
            }),
        }
    }

    /// Reset the arena to its initial phase and release any excess chunks.
    ///
    /// This does not run `Drop` on values allocated in the arena.
    pub fn reset(&mut self) {
        self.bump.reset();
        self.payload_bytes.set(0);
    }

    /// Logical payload bytes allocated since the last reset.
    #[must_use]
    pub fn payload_bytes(&self) -> usize {
        self.payload_bytes.get()
    }

    /// Bytes recorded by `bumpalo` for this arena, excluding its metadata.
    #[must_use]
    pub fn allocated_bytes(&self) -> usize {
        self.bump.allocated_bytes()
    }

    /// Configured maximum payload bytes, if any.
    #[must_use]
    pub const fn byte_limit(&self) -> Option<usize> {
        self.byte_limit
    }

    fn reserve_payload(&self, bytes: usize) -> Result<usize, ArenaAllocationError> {
        let current = self.payload_bytes.get();
        let next = current.checked_add(bytes).ok_or(ArenaAllocationError {
            requested_bytes: bytes,
            allocated_payload_bytes: current,
            limit_bytes: self.byte_limit,
        })?;
        if self.byte_limit.is_some_and(|limit| next > limit) {
            return Err(ArenaAllocationError {
                requested_bytes: bytes,
                allocated_payload_bytes: current,
                limit_bytes: self.byte_limit,
            });
        }
        Ok(next)
    }
}

impl Default for ScratchArena {
    fn default() -> Self {
        Self::new(None)
    }
}

/// Scratch allocation failure with payload and configured limit information.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArenaAllocationError {
    /// Payload bytes requested by the failed allocation.
    pub requested_bytes: usize,
    /// Payload bytes already allocated when it failed.
    pub allocated_payload_bytes: usize,
    /// Configured payload limit, or `None` when unbounded.
    pub limit_bytes: Option<usize>,
}

impl std::fmt::Display for ArenaAllocationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "scratch arena allocation of {} bytes failed with {} of {} payload bytes used",
            self.requested_bytes,
            self.allocated_payload_bytes,
            self.limit_bytes.map_or_else(|| "unlimited".to_owned(), |limit| limit.to_string())
        )
    }
}

impl std::error::Error for ArenaAllocationError {}

#[cfg(test)]
mod tests {
    use super::ScratchArena;

    #[test]
    fn payload_limit_excludes_bump_chunk_overhead_and_reset_reuses_budget() {
        let limit = std::mem::size_of::<u64>();
        let mut arena = ScratchArena::new(Some(limit));

        *arena.try_alloc(42_u64).expect("payload should fit the limit") = 7;
        assert_eq!(arena.payload_bytes(), limit);
        assert!(arena.allocated_bytes() >= limit);

        let error = arena.try_alloc(1_u8).expect_err("additional payload must exceed limit");
        assert_eq!(error.requested_bytes, 1);
        assert_eq!(error.allocated_payload_bytes, limit);
        assert_eq!(error.limit_bytes, Some(limit));

        arena.reset();
        assert_eq!(arena.payload_bytes(), 0);
        assert_eq!(*arena.try_alloc(9_u64).expect("reset should restore payload budget"), 9);
    }

    #[test]
    fn copied_slices_and_strings_count_payload_bytes() {
        let arena = ScratchArena::new(Some(7));
        assert_eq!(arena.try_alloc_slice_copy(&[1_u16, 2]).unwrap(), &[1, 2]);
        assert_eq!(arena.try_alloc_str("abc").unwrap(), "abc");
        assert_eq!(arena.payload_bytes(), 7);
    }
}
