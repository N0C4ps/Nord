use crate::ContentHash;

const STRUCTURED_HASH_PREFIX: &[u8] = b"nord-hash\0v1\0";
const CONTENT_HASH_PREFIX: &[u8] = b"nord-content\0v1\0";

/// Incremental BLAKE3 hasher for a byte stream.
pub struct ContentHasher {
    hasher: blake3::Hasher,
}

impl ContentHasher {
    /// Create a hasher compatible with the raw BLAKE3 digest of the input.
    #[must_use]
    pub fn new() -> Self {
        Self { hasher: blake3::Hasher::new() }
    }

    /// Create a domain-separated hasher for a specific kind of content.
    ///
    /// Use distinct stable domains such as `nord.texture.raw.v1` and
    /// `nord.shader.source.v1` when the resulting identifiers have different
    /// meanings.
    #[must_use]
    pub fn with_domain(domain: &str) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(CONTENT_HASH_PREFIX);
        write_framed(&mut hasher, domain.as_bytes());
        Self { hasher }
    }

    /// Add bytes to the input without buffering the full content.
    pub fn update(&mut self, bytes: &[u8]) -> &mut Self {
        self.hasher.update(bytes);
        self
    }

    /// Stream bytes from a reader into this hasher.
    ///
    /// # Errors
    ///
    /// Returns the underlying I/O error if reading fails. Bytes read before
    /// the error remain in this hasher.
    pub fn update_reader<R: std::io::Read>(&mut self, reader: R) -> std::io::Result<&mut Self> {
        self.hasher.update_reader(reader)?;
        Ok(self)
    }

    /// Finalize the stream and return its digest.
    #[must_use]
    pub fn finish(&self) -> ContentHash {
        ContentHash::from_blake3(self.hasher.finalize())
    }
}

impl Default for ContentHasher {
    fn default() -> Self {
        Self::new()
    }
}

/// Deterministic hasher for records composed from named, typed fields.
///
/// Each field includes a type tag, name length, name, value length, and value,
/// avoiding ambiguous encodings such as concatenating `("ab", "c")` with
/// `("a", "bc")`.
pub struct HashBuilder {
    hasher: blake3::Hasher,
}

impl HashBuilder {
    /// Start a structured hash with a stable type/version domain.
    #[must_use]
    pub fn new(domain: &str) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(STRUCTURED_HASH_PREFIX);
        write_field(&mut hasher, 0, b"domain", domain.as_bytes());
        Self { hasher }
    }

    /// Add a byte field.
    pub fn field_bytes(&mut self, name: &str, value: &[u8]) -> &mut Self {
        write_field(&mut self.hasher, 1, name.as_bytes(), value);
        self
    }

    /// Add a UTF-8 string field.
    pub fn field_str(&mut self, name: &str, value: &str) -> &mut Self {
        write_field(&mut self.hasher, 2, name.as_bytes(), value.as_bytes());
        self
    }

    /// Add a boolean field.
    pub fn field_bool(&mut self, name: &str, value: bool) -> &mut Self {
        write_field(&mut self.hasher, 3, name.as_bytes(), &[u8::from(value)]);
        self
    }

    /// Add an unsigned 8-bit integer field.
    pub fn field_u8(&mut self, name: &str, value: u8) -> &mut Self {
        write_field(&mut self.hasher, 4, name.as_bytes(), &[value]);
        self
    }

    /// Add an unsigned 32-bit integer field using little-endian encoding.
    pub fn field_u32(&mut self, name: &str, value: u32) -> &mut Self {
        write_field(&mut self.hasher, 5, name.as_bytes(), &value.to_le_bytes());
        self
    }

    /// Add an unsigned 64-bit integer field using little-endian encoding.
    pub fn field_u64(&mut self, name: &str, value: u64) -> &mut Self {
        write_field(&mut self.hasher, 6, name.as_bytes(), &value.to_le_bytes());
        self
    }

    /// Add a signed 64-bit integer field using little-endian encoding.
    pub fn field_i64(&mut self, name: &str, value: i64) -> &mut Self {
        write_field(&mut self.hasher, 7, name.as_bytes(), &value.to_le_bytes());
        self
    }

    /// Finalize the structured record and return its stable digest.
    #[must_use]
    pub fn finish(&self) -> ContentHash {
        ContentHash::from_blake3(self.hasher.finalize())
    }
}

fn write_field(hasher: &mut blake3::Hasher, type_tag: u8, name: &[u8], value: &[u8]) {
    hasher.update(&[type_tag]);
    write_framed(hasher, name);
    write_framed(hasher, value);
}

fn write_framed(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    // Encode every length into a fixed-width little-endian u128 field while
    // preserving the same output on 32-bit and 64-bit targets.
    let native = bytes.len().to_le_bytes();
    let mut length = [0_u8; 16];
    length[..native.len()].copy_from_slice(&native);
    hasher.update(&length);
    hasher.update(bytes);
}
