//! Stable application-facing names for Nord's JSON compression utilities.

pub use nord_json_compressor::{
    Codec as ApplicationJsonCodec, CompressionError as ApplicationJsonCompressionError,
    CompressionOptions as ApplicationJsonCompressionOptions,
    compress_bytes as application_compress_bytes, compress_json as application_compress_json,
    decompress_bytes as application_decompress_bytes,
    decompress_json as application_decompress_json, minify_json as application_minify_json,
};
