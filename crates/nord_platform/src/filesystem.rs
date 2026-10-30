use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::Mutex;

/// Opaque, generation-checked handle to an open platform file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileHandle {
    slot: u32,
    generation: u32,
}

/// File access flags used by [`FileSystem::open`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileOpenOptions {
    access: FileAccess,
    disposition: FileDisposition,
}

/// Requested file access mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileAccess {
    /// Read existing contents.
    Read,
    /// Write without reading.
    Write,
    /// Read and write.
    ReadWrite,
}

/// How opening a file handles an existing or missing path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileDisposition {
    /// Fail if the path does not exist.
    OpenExisting,
    /// Create a new file and fail if it already exists.
    CreateNew,
    /// Create a file or truncate the existing file.
    CreateOrTruncate,
    /// Open a file or create it if missing.
    OpenOrCreate,
    /// Open or create a file and write at its end.
    Append,
}

impl FileOpenOptions {
    /// Open an existing file for reading.
    #[must_use]
    pub const fn read() -> Self {
        Self { access: FileAccess::Read, disposition: FileDisposition::OpenExisting }
    }

    /// Create or truncate a file for writing.
    #[must_use]
    pub const fn create_write() -> Self {
        Self { access: FileAccess::Write, disposition: FileDisposition::CreateOrTruncate }
    }

    /// Open a file for reading and writing, creating it if it does not exist.
    #[must_use]
    pub const fn create_read_write() -> Self {
        Self { access: FileAccess::ReadWrite, disposition: FileDisposition::OpenOrCreate }
    }

    /// Open or create a file for appending.
    #[must_use]
    pub const fn append() -> Self {
        Self { access: FileAccess::Write, disposition: FileDisposition::Append }
    }

    /// Construct options from explicit access and path disposition.
    #[must_use]
    pub const fn new(access: FileAccess, disposition: FileDisposition) -> Self {
        Self { access, disposition }
    }
}

struct FileSlot {
    generation: u32,
    file: Option<File>,
    retired: bool,
}

struct FileRegistry {
    slots: Vec<FileSlot>,
}

/// Explicit owner of open platform file resources.
///
/// Handles are invalidated by `close`; stale handles fail instead of referring
/// to a later file that reuses the same slot. Operations are synchronized and
/// should not be used for per-frame streaming; high-throughput asset I/O belongs
/// in a dedicated asynchronous asset system.
pub struct FileSystem {
    registry: Mutex<FileRegistry>,
}

impl Default for FileSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl FileSystem {
    pub(super) fn new() -> Self {
        Self { registry: Mutex::new(FileRegistry { slots: Vec::new() }) }
    }

    /// Open a file and return an opaque handle to the owned resource.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the native operation fails or the handle
    /// registry reaches its representable capacity.
    pub fn open(&self, path: impl AsRef<Path>, options: FileOpenOptions) -> io::Result<FileHandle> {
        if options.access == FileAccess::Read
            && options.disposition != FileDisposition::OpenExisting
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "file creation and truncation require write access",
            ));
        }
        let mut open = OpenOptions::new();
        open.read(matches!(options.access, FileAccess::Read | FileAccess::ReadWrite))
            .write(matches!(options.access, FileAccess::Write | FileAccess::ReadWrite));
        match options.disposition {
            FileDisposition::OpenExisting => {}
            FileDisposition::CreateNew => {
                open.create_new(true);
            }
            FileDisposition::CreateOrTruncate => {
                open.create(true).truncate(true);
            }
            FileDisposition::OpenOrCreate => {
                open.create(true);
            }
            FileDisposition::Append => {
                open.append(true).create(true);
            }
        }
        let file = open.open(path)?;

        let mut registry = self.registry.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((index, slot)) = registry
            .slots
            .iter_mut()
            .enumerate()
            .find(|(_, slot)| slot.file.is_none() && !slot.retired)
        {
            let index = u32::try_from(index)
                .map_err(|_| io::Error::other("platform file handle capacity exhausted"))?;
            slot.file = Some(file);
            return Ok(FileHandle { slot: index, generation: slot.generation });
        }

        let index = u32::try_from(registry.slots.len())
            .map_err(|_| io::Error::other("platform file handle capacity exhausted"))?;
        registry.slots.push(FileSlot { generation: 1, file: Some(file), retired: false });
        Ok(FileHandle { slot: index, generation: 1 })
    }

    /// Read bytes from a file resource.
    ///
    /// # Errors
    ///
    /// Returns an I/O error for an invalid handle or failed native read.
    pub fn read(&self, handle: FileHandle, buffer: &mut [u8]) -> io::Result<usize> {
        self.with_file(handle, |file| file.read(buffer))
    }

    /// Write bytes to a file resource.
    ///
    /// # Errors
    ///
    /// Returns an I/O error for an invalid handle or failed native write.
    pub fn write(&self, handle: FileHandle, bytes: &[u8]) -> io::Result<usize> {
        self.with_file(handle, |file| file.write(bytes))
    }

    /// Flush a file resource to its operating-system handle.
    ///
    /// # Errors
    ///
    /// Returns an I/O error for an invalid handle or failed native flush.
    pub fn flush(&self, handle: FileHandle) -> io::Result<()> {
        self.with_file(handle, Write::flush)
    }

    /// Seek to an absolute or relative file position.
    ///
    /// # Errors
    ///
    /// Returns an I/O error for an invalid handle or failed native seek.
    pub fn seek(&self, handle: FileHandle, position: SeekFrom) -> io::Result<u64> {
        self.with_file(handle, |file| file.seek(position))
    }

    /// Close and invalidate a file resource.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the handle is invalid or already closed.
    pub fn close(&self, handle: FileHandle) -> io::Result<()> {
        let mut registry = self.registry.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let slot = registry.slots.get_mut(handle.slot as usize).ok_or_else(invalid_handle)?;
        if slot.generation != handle.generation || slot.file.is_none() {
            return Err(invalid_handle());
        }
        slot.file.take();
        if let Some(generation) = slot.generation.checked_add(1) {
            slot.generation = generation;
        } else {
            slot.retired = true;
        }
        Ok(())
    }

    /// Number of currently open resources.
    #[must_use]
    pub fn open_count(&self) -> usize {
        self.registry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .slots
            .iter()
            .filter(|slot| slot.file.is_some())
            .count()
    }

    fn with_file<T>(
        &self,
        handle: FileHandle,
        operation: impl FnOnce(&mut File) -> io::Result<T>,
    ) -> io::Result<T> {
        let mut registry = self.registry.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let slot = registry.slots.get_mut(handle.slot as usize).ok_or_else(invalid_handle)?;
        if slot.generation != handle.generation {
            return Err(invalid_handle());
        }
        let file = slot.file.as_mut().ok_or_else(invalid_handle)?;
        operation(file)
    }
}

fn invalid_handle() -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, "invalid or closed platform file handle")
}
