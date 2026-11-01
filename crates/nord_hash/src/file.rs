use crate::{ContentHash, ContentHasher};
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

/// Hash bytes from a reader without requiring the whole stream in memory.
///
/// # Errors
///
/// Returns the I/O error encountered while reading the stream.
pub fn hash_reader<R: Read>(reader: R) -> io::Result<ContentHash> {
    ContentHasher::new().update_reader(reader).map(|hasher| hasher.finish())
}

/// Hash a file's contents. The path and file metadata are not included in the digest.
///
/// # Errors
///
/// Returns [`HashFileError`] if the file cannot be opened or read.
pub fn hash_file(path: impl AsRef<Path>) -> Result<ContentHash, HashFileError> {
    let path = path.as_ref();
    let file =
        File::open(path).map_err(|source| HashFileError { path: path.to_path_buf(), source })?;
    hash_reader(file).map_err(|source| HashFileError { path: path.to_path_buf(), source })
}

/// File hashing failure with its path and underlying I/O source.
#[derive(Debug)]
pub struct HashFileError {
    /// File path that could not be opened or read.
    pub path: PathBuf,
    /// Underlying I/O failure.
    pub source: io::Error,
}

impl std::fmt::Display for HashFileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "could not hash file `{}`: {}", self.path.display(), self.source)
    }
}

impl std::error::Error for HashFileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}
