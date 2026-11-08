//! Lossless JSON minification and framed compression for game data.
//!
//! JSON source files remain readable and reviewable. This crate provides an
//! explicit compacting operation and a versioned binary container for storage
//! or transport; callers choose the codec to match their latency/size needs.

use flate2::Compression as GzipLevel;
use flate2::read::{GzDecoder, GzEncoder};
use lz4_flex::frame::{FrameDecoder, FrameEncoder};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::io::{Read, Write};

const MAGIC: &[u8; 4] = b"NJC\x01";
const HEADER_BYTES: usize = 13;

/// Codec stored in a Nord JSON-compressor frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Codec {
    Zstandard = 1,
    Lz4 = 2,
    Gzip = 3,
}

/// Codec selection and level. The level applies to Zstandard and Gzip; LZ4
/// uses its fast default encoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompressionOptions {
    pub codec: Codec,
    pub level: i32,
}

impl Default for CompressionOptions {
    fn default() -> Self {
        Self { codec: Codec::Zstandard, level: 3 }
    }
}

/// Errors from JSON parsing, frame validation, codecs, or size limits.
#[derive(Debug)]
pub enum CompressionError {
    InvalidJson(serde_json::Error),
    InvalidFrame(&'static str),
    UnsupportedCodec(u8),
    Io(std::io::Error),
    OutputLimitExceeded { limit: usize },
    SizeMismatch { expected: u64, actual: u64 },
}

impl Display for CompressionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidJson(error) => write!(f, "invalid JSON: {error}"),
            Self::InvalidFrame(reason) => write!(f, "invalid Nord compression frame: {reason}"),
            Self::UnsupportedCodec(codec) => {
                write!(f, "unsupported Nord compression codec {codec}")
            }
            Self::Io(error) => write!(f, "compression I/O failed: {error}"),
            Self::OutputLimitExceeded { limit } => {
                write!(f, "decompressed data exceeds {limit} bytes")
            }
            Self::SizeMismatch { expected, actual } => {
                write!(f, "frame declared {expected} bytes but decoded {actual}")
            }
        }
    }
}

impl Error for CompressionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidJson(error) => Some(error),
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for CompressionError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// Parse JSON and serialize it without insignificant whitespace.
///
/// This preserves JSON values, not the original byte representation, object
/// key order, duplicate keys, or formatting.
///
/// # Errors
/// Returns [`CompressionError::InvalidJson`] if parsing or serialization fails.
pub fn minify_json(json: &[u8]) -> Result<Vec<u8>, CompressionError> {
    let value: serde_json::Value =
        serde_json::from_slice(json).map_err(CompressionError::InvalidJson)?;
    serde_json::to_vec(&value).map_err(CompressionError::InvalidJson)
}

/// Compress JSON into a self-describing `NJC` frame. The frame records its
/// codec and uncompressed byte count, and embeds the codec's standard frame.
///
/// # Errors
/// Returns [`CompressionError`] if the input is invalid JSON or compression fails.
pub fn compress_json(
    json: &[u8],
    options: CompressionOptions,
) -> Result<Vec<u8>, CompressionError> {
    let compact = minify_json(json)?;
    compress_bytes(&compact, options)
}

/// Compress arbitrary bytes into a self-describing `NJC` frame.
///
/// # Errors
/// Returns [`CompressionError::Io`] if compression fails or the input length cannot be represented.
pub fn compress_bytes(
    input: &[u8],
    options: CompressionOptions,
) -> Result<Vec<u8>, CompressionError> {
    let compressed = match options.codec {
        Codec::Zstandard => zstd::stream::encode_all(input, options.level)?,
        Codec::Lz4 => {
            let mut encoder = FrameEncoder::new(Vec::new());
            encoder.write_all(input)?;
            encoder.finish().map_err(std::io::Error::other)?
        }
        Codec::Gzip => {
            let level = u32::try_from(options.level.clamp(0, 9)).unwrap_or(6);
            let mut encoder = GzEncoder::new(input, GzipLevel::new(level));
            let mut output = Vec::new();
            encoder.read_to_end(&mut output)?;
            output
        }
    };
    let uncompressed_len = u64::try_from(input.len())
        .map_err(|_| CompressionError::InvalidFrame("input length exceeds u64"))?;
    let mut frame = Vec::with_capacity(HEADER_BYTES + compressed.len());
    frame.extend_from_slice(MAGIC);
    frame.push(options.codec as u8);
    frame.extend_from_slice(&uncompressed_len.to_le_bytes());
    frame.extend_from_slice(&compressed);
    Ok(frame)
}

/// Decompress a Nord frame with a caller-provided allocation limit.
///
/// # Errors
/// Returns [`CompressionError`] for unsupported codecs, malformed frames, codec errors, size mismatches, or output-limit violations.
pub fn decompress_bytes(
    frame: &[u8],
    max_output_bytes: usize,
) -> Result<Vec<u8>, CompressionError> {
    if frame.len() < HEADER_BYTES || &frame[..4] != MAGIC {
        return Err(CompressionError::InvalidFrame("bad magic or truncated header"));
    }
    let codec = match frame[4] {
        1 => Codec::Zstandard,
        2 => Codec::Lz4,
        3 => Codec::Gzip,
        value => return Err(CompressionError::UnsupportedCodec(value)),
    };
    let expected_bytes: [u8; 8] = frame[5..13]
        .try_into()
        .map_err(|_| CompressionError::InvalidFrame("truncated size field"))?;
    let expected = u64::from_le_bytes(expected_bytes);
    let limit = u64::try_from(max_output_bytes).unwrap_or(u64::MAX);
    if expected > limit {
        return Err(CompressionError::OutputLimitExceeded { limit: max_output_bytes });
    }
    let payload = &frame[HEADER_BYTES..];
    let capacity = usize::try_from(expected).unwrap_or(max_output_bytes);
    let output = match codec {
        Codec::Zstandard => read_bounded(zstd::stream::Decoder::new(payload)?, capacity, limit)?,
        Codec::Lz4 => read_bounded(FrameDecoder::new(payload), capacity, limit)?,
        Codec::Gzip => read_bounded(GzDecoder::new(payload), capacity, limit)?,
    };
    if output.len() > max_output_bytes {
        return Err(CompressionError::OutputLimitExceeded { limit: max_output_bytes });
    }
    let actual = u64::try_from(output.len()).unwrap_or(u64::MAX);
    if actual != expected {
        return Err(CompressionError::SizeMismatch { expected, actual });
    }
    Ok(output)
}

fn read_bounded(
    reader: impl Read,
    capacity: usize,
    limit: u64,
) -> Result<Vec<u8>, CompressionError> {
    let mut output = Vec::with_capacity(capacity);
    reader.take(limit.saturating_add(1)).read_to_end(&mut output)?;
    Ok(output)
}

/// Decompress and validate a JSON frame with a caller-provided allocation limit.
///
/// # Errors
/// Returns [`CompressionError`] if frame decompression fails or the result is not valid JSON.
pub fn decompress_json(frame: &[u8], max_output_bytes: usize) -> Result<Vec<u8>, CompressionError> {
    let json = decompress_bytes(frame, max_output_bytes)?;
    serde_json::from_slice::<serde_json::Value>(&json).map_err(CompressionError::InvalidJson)?;
    Ok(json)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &[u8] = br#"{ "name": "stone", "flags": [true, false], "hardness": 1.5 }"#;

    #[test]
    fn json_minifier_preserves_value_and_removes_whitespace() {
        let compact = minify_json(SAMPLE).expect("valid JSON");
        assert_eq!(compact, br#"{"flags":[true,false],"hardness":1.5,"name":"stone"}"#);
    }

    #[test]
    fn supported_codecs_round_trip_json_frames() {
        for codec in [Codec::Zstandard, Codec::Lz4, Codec::Gzip] {
            let frame =
                compress_json(SAMPLE, CompressionOptions { codec, level: 3 }).expect("compress");
            let decoded = decompress_json(&frame, 1024).expect("decompress");
            assert_eq!(decoded, minify_json(SAMPLE).expect("minify"));
        }
    }

    #[test]
    fn decompression_obeys_output_limit_and_frame_size() {
        let frame =
            compress_bytes(b"oversized output", CompressionOptions::default()).expect("compress");
        assert!(matches!(
            decompress_bytes(&frame, 4),
            Err(CompressionError::OutputLimitExceeded { .. })
        ));
        let mut damaged = frame;
        damaged[5] = 0;
        assert!(matches!(
            decompress_bytes(&damaged, 1024),
            Err(CompressionError::SizeMismatch { .. })
        ));
    }
}
