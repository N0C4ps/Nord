//! Stable application-facing facade over Nord's hash utilities.

pub use nord_hash::{
    ContentHash as ApplicationContentHash, ContentHasher as ApplicationContentHasher,
    HashBuilder as ApplicationHashBuilder, HashFileError as ApplicationHashFileError,
    ParseContentHashError as ApplicationParseContentHashError,
};

pub use nord_hash::{
    hash_bytes as application_hash_bytes,
    hash_bytes_with_domain as application_hash_bytes_with_domain,
    hash_file as application_hash_file, hash_reader as application_hash_reader,
};
