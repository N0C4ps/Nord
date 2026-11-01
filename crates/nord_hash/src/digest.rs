use std::fmt::{Debug, Display, Formatter, LowerHex};
use std::str::FromStr;

/// A stable 256-bit BLAKE3 digest.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    /// Construct a digest from its raw 32-byte representation.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Borrow the raw digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Convert the digest to its canonical 64-character lowercase hex form.
    #[must_use]
    pub fn to_hex(self) -> String {
        format!("{self:x}")
    }

    pub(crate) fn from_blake3(hash: blake3::Hash) -> Self {
        Self(*hash.as_bytes())
    }
}

impl AsRef<[u8]> for ContentHash {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Display for ContentHash {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        LowerHex::fmt(self, formatter)
    }
}

impl Debug for ContentHash {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.debug_tuple("ContentHash").field(&self.to_hex()).finish()
    }
}

impl LowerHex for ContentHash {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl FromStr for ContentHash {
    type Err = ParseContentHashError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() != 64 {
            return Err(ParseContentHashError::InvalidLength { actual: value.len() });
        }
        let mut bytes = [0_u8; 32];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            let high = hex_nibble(pair[0]).ok_or(ParseContentHashError::InvalidHex {
                byte_index: index * 2,
                byte: pair[0],
            })?;
            let low = hex_nibble(pair[1]).ok_or(ParseContentHashError::InvalidHex {
                byte_index: index * 2 + 1,
                byte: pair[1],
            })?;
            bytes[index] = (high << 4) | low;
        }
        Ok(Self(bytes))
    }
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Invalid text representation of a [`ContentHash`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseContentHashError {
    /// Hashes must contain exactly 64 hexadecimal characters.
    InvalidLength { actual: usize },
    /// A non-hexadecimal byte was found at the indicated byte offset.
    InvalidHex { byte_index: usize, byte: u8 },
}

impl Display for ParseContentHashError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidLength { actual } => {
                write!(formatter, "expected 64 hexadecimal characters, received {actual}")
            }
            Self::InvalidHex { byte_index, byte } => {
                write!(formatter, "invalid hexadecimal byte 0x{byte:02x} at offset {byte_index}")
            }
        }
    }
}

impl std::error::Error for ParseContentHashError {}
