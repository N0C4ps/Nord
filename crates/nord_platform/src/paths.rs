use std::io;
use std::path::{Path, PathBuf};

/// Invalid application identity used while discovering platform paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathError {
    application_id: String,
}

impl std::fmt::Display for PathError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid platform application id `{}`", self.application_id)
    }
}

impl std::error::Error for PathError {}

/// Per-application directories discovered from the native operating system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GamePaths {
    data: PathBuf,
    config: PathBuf,
    cache: PathBuf,
    temp: PathBuf,
}

impl GamePaths {
    pub(super) fn discover(application_id: &str) -> Result<Self, PathError> {
        if application_id.is_empty()
            || application_id == "."
            || application_id == ".."
            || !application_id
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "._-".contains(character))
        {
            return Err(PathError { application_id: application_id.to_owned() });
        }

        let (data, config, cache) = super::backend::game_paths(application_id);
        Ok(Self { data, config, cache, temp: std::env::temp_dir().join(application_id) })
    }

    /// Persistent application data directory.
    #[must_use]
    pub fn data_dir(&self) -> &Path {
        &self.data
    }

    /// User configuration directory.
    #[must_use]
    pub fn config_dir(&self) -> &Path {
        &self.config
    }

    /// Re-creatable cache directory.
    #[must_use]
    pub fn cache_dir(&self) -> &Path {
        &self.cache
    }

    /// Temporary application directory.
    #[must_use]
    pub fn temp_dir(&self) -> &Path {
        &self.temp
    }

    /// Create the persistent data directory when the application needs it.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the directory cannot be created.
    pub fn ensure_data_dir(&self) -> io::Result<()> {
        std::fs::create_dir_all(&self.data)
    }

    /// Create the configuration directory when the application needs it.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the directory cannot be created.
    pub fn ensure_config_dir(&self) -> io::Result<()> {
        std::fs::create_dir_all(&self.config)
    }

    /// Create the cache directory when the application needs it.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the directory cannot be created.
    pub fn ensure_cache_dir(&self) -> io::Result<()> {
        std::fs::create_dir_all(&self.cache)
    }
}
