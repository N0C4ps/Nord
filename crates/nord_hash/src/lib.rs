//! Stable content hashing for assets, cache keys, and structured engine data.
//!
//! Persistent digests use BLAKE3. The crate deliberately does not replace
//! standard `HashMap` hashers, whose randomized behavior serves a different
//! purpose and should not be persisted as a content identifier.

mod builder;
mod digest;
mod file;

pub use builder::{ContentHasher, HashBuilder};
pub use digest::{ContentHash, ParseContentHashError};
pub use file::{HashFileError, hash_file, hash_reader};

/// Hash an in-memory byte slice with the raw BLAKE3 content mode.
#[must_use]
pub fn hash_bytes(bytes: &[u8]) -> ContentHash {
    ContentHasher::new().update(bytes).finish()
}

/// Hash bytes under an application-defined domain.
#[must_use]
pub fn hash_bytes_with_domain(domain: &str, bytes: &[u8]) -> ContentHash {
    ContentHasher::with_domain(domain).update(bytes).finish()
}
