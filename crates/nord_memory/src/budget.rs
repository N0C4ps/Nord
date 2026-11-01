use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Concurrent accounting budget for explicitly managed memory.
///
/// This budget does not intercept allocations. Callers reserve bytes before
/// allocating and retain the returned token for the allocation's lifetime.
#[derive(Clone, Debug)]
pub struct MemoryBudget {
    inner: Arc<BudgetInner>,
}

#[derive(Debug)]
struct BudgetInner {
    limit: Option<u64>,
    current: AtomicU64,
    peak: AtomicU64,
}

impl MemoryBudget {
    /// Create a budget with an optional hard limit. `None` means unlimited.
    #[must_use]
    pub fn new(limit_bytes: Option<u64>) -> Self {
        Self {
            inner: Arc::new(BudgetInner {
                limit: limit_bytes,
                current: AtomicU64::new(0),
                peak: AtomicU64::new(0),
            }),
        }
    }

    /// Reserve bytes atomically. The reservation releases them when dropped.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryBudgetError`] if the counter overflows or the new
    /// reservation would exceed this budget's limit.
    pub fn reserve(&self, bytes: u64) -> Result<MemoryReservation, MemoryBudgetError> {
        self.inner.reserve(bytes)?;
        Ok(MemoryReservation { inner: Arc::clone(&self.inner), bytes })
    }

    /// Return current, peak, and configured limit values.
    #[must_use]
    pub fn snapshot(&self) -> MemoryBudgetSnapshot {
        MemoryBudgetSnapshot {
            current_bytes: self.inner.current.load(Ordering::Relaxed),
            peak_bytes: self.inner.peak.load(Ordering::Relaxed),
            limit_bytes: self.inner.limit,
        }
    }
}

impl Default for MemoryBudget {
    fn default() -> Self {
        Self::new(None)
    }
}

impl BudgetInner {
    fn reserve(&self, bytes: u64) -> Result<(), MemoryBudgetError> {
        let mut current = self.current.load(Ordering::Relaxed);
        loop {
            let new_current = current.checked_add(bytes).ok_or(MemoryBudgetError {
                requested_bytes: bytes,
                current_bytes: current,
                limit_bytes: self.limit,
                counter_overflowed: true,
            })?;
            if self.limit.is_some_and(|limit| new_current > limit) {
                return Err(MemoryBudgetError {
                    requested_bytes: bytes,
                    current_bytes: current,
                    limit_bytes: self.limit,
                    counter_overflowed: false,
                });
            }
            match self.current.compare_exchange_weak(
                current,
                new_current,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    self.peak.fetch_max(new_current, Ordering::Relaxed);
                    return Ok(());
                }
                Err(actual) => current = actual,
            }
        }
    }
}

/// A snapshot of the memory accounted to a [`MemoryBudget`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryBudgetSnapshot {
    /// Bytes currently held by live reservations.
    pub current_bytes: u64,
    /// Highest concurrent reservation total observed so far.
    pub peak_bytes: u64,
    /// Configured limit, or `None` when unbounded.
    pub limit_bytes: Option<u64>,
}

/// A fallible reservation that returns its bytes to the budget on drop.
#[derive(Debug)]
pub struct MemoryReservation {
    inner: Arc<BudgetInner>,
    bytes: u64,
}

impl MemoryReservation {
    /// Number of bytes currently held by this token.
    #[must_use]
    pub const fn bytes(&self) -> u64 {
        self.bytes
    }

    /// Grow or shrink this reservation while retaining the same budget token.
    ///
    /// Growth is checked against the limit before this token is changed.
    /// Shrinking releases the difference immediately.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryBudgetError`] if growing exceeds the budget or its
    /// accounting counter would overflow. In that case, the token is unchanged.
    pub fn resize(&mut self, new_bytes: u64) -> Result<(), MemoryBudgetError> {
        if new_bytes > self.bytes {
            self.inner.reserve(new_bytes - self.bytes)?;
        } else {
            self.inner.current.fetch_sub(self.bytes - new_bytes, Ordering::AcqRel);
        }
        self.bytes = new_bytes;
        Ok(())
    }
}

impl Drop for MemoryReservation {
    fn drop(&mut self) {
        self.inner.current.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

/// A rejected reservation, with enough context to report the budget pressure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryBudgetError {
    /// Bytes requested by the new reservation or growth operation.
    pub requested_bytes: u64,
    /// Bytes already reserved when the operation was evaluated.
    pub current_bytes: u64,
    /// Configured limit, or `None` when this budget is unbounded.
    pub limit_bytes: Option<u64>,
    /// Whether adding the request overflowed the accounting counter.
    pub counter_overflowed: bool,
}

impl std::fmt::Display for MemoryBudgetError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.counter_overflowed {
            formatter.write_str("memory budget accounting counter overflowed")
        } else {
            write!(
                formatter,
                "memory budget exceeded: requested {} bytes with {} of {} bytes reserved",
                self.requested_bytes,
                self.current_bytes,
                self.limit_bytes.map_or_else(|| "unlimited".to_owned(), |limit| limit.to_string())
            )
        }
    }
}

impl std::error::Error for MemoryBudgetError {}
