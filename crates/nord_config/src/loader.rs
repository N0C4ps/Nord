use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Configuration file format supported by the loader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConfigFileFormat {
    /// JavaScript Object Notation.
    Json,
    /// Tom's Obvious Minimal Language.
    Toml,
}

impl ConfigFileFormat {
    fn from_path(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "json" => Some(Self::Json),
            "toml" => Some(Self::Toml),
            _ => None,
        }
    }
}

/// Layered configuration loader for a serde-compatible application schema.
pub struct ConfigLoader<T> {
    defaults: T,
    files: Vec<FileSource>,
    environment_prefix: Option<String>,
    explicit_environment: BTreeMap<String, String>,
    validator: Option<Box<ConfigValidator<T>>>,
}

struct FileSource {
    path: PathBuf,
    format: Option<ConfigFileFormat>,
    required: bool,
}

/// Validation callback accepted by [`ConfigLoader`].
pub type ConfigValidator<T> = dyn Fn(&T) -> Result<(), String> + Send + Sync + 'static;

impl<T> ConfigLoader<T>
where
    T: Serialize + DeserializeOwned,
{
    /// Start with typed defaults. They are the lowest-precedence layer.
    #[must_use]
    pub fn new(defaults: T) -> Self {
        Self {
            defaults,
            files: Vec::new(),
            environment_prefix: None,
            explicit_environment: BTreeMap::new(),
            validator: None,
        }
    }

    /// Add a required JSON or TOML overlay, inferred from the file extension.
    /// Multiple files are applied in call order; later files override earlier
    /// ones, while nested tables are merged recursively.
    #[must_use]
    pub fn with_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.files.push(FileSource { path: path.into(), format: None, required: true });
        self
    }

    /// Add an optional JSON or TOML overlay. A missing file is ignored; other
    /// read or parse failures remain errors.
    #[must_use]
    pub fn with_optional_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.files.push(FileSource { path: path.into(), format: None, required: false });
        self
    }

    /// Add a required overlay with an explicit format, useful for extensionless paths.
    #[must_use]
    pub fn with_file_format(mut self, path: impl Into<PathBuf>, format: ConfigFileFormat) -> Self {
        self.files.push(FileSource { path: path.into(), format: Some(format), required: true });
        self
    }

    /// Add an optional overlay with an explicit format.
    #[must_use]
    pub fn with_optional_file_format(
        mut self,
        path: impl Into<PathBuf>,
        format: ConfigFileFormat,
    ) -> Self {
        self.files.push(FileSource { path: path.into(), format: Some(format), required: false });
        self
    }

    /// Read environment variables with this prefix, such as
    /// `NORD__TIMING__MAX_FRAME_DELTA_MS=120`.
    ///
    /// Double underscores separate nested fields; remaining underscores are
    /// preserved. Names are matched case-insensitively and converted to
    /// lowercase serde field paths.
    #[must_use]
    pub fn with_environment_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.environment_prefix = Some(prefix.into());
        self
    }

    /// Add explicit environment values after process environment values.
    /// This supports embedding applications and deterministic configuration
    /// sources; keys use the full prefixed name.
    #[must_use]
    pub fn with_environment_overrides<I, K, V>(mut self, values: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        self.explicit_environment = values
            .into_iter()
            .map(|(key, value)| (key.into().to_ascii_uppercase(), value.into()))
            .collect();
        self
    }

    /// Validate the fully merged and deserialized value.
    #[must_use]
    pub fn validate_with<F>(mut self, validator: F) -> Self
    where
        F: Fn(&T) -> Result<(), String> + Send + Sync + 'static,
    {
        self.validator = Some(Box::new(validator));
        self
    }

    /// Load, merge, deserialize, and validate the configuration.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] for default serialization, file, environment,
    /// merge, deserialization, or validation failures.
    pub fn load(self) -> Result<T, ConfigError> {
        if !self.explicit_environment.is_empty() && self.environment_prefix.is_none() {
            return Err(ConfigError::MissingEnvironmentPrefix);
        }
        if let Some(prefix) = &self.environment_prefix
            && (prefix.is_empty()
                || !prefix
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '_')
                || prefix.contains("__"))
        {
            return Err(ConfigError::InvalidEnvironmentPrefix { prefix: prefix.clone() });
        }
        let mut merged =
            serde_json::to_value(self.defaults).map_err(ConfigError::DefaultsEncode)?;

        for file in self.files {
            let Some(overlay) = read_file_overlay(&file)? else {
                continue;
            };
            merge_value(&mut merged, overlay, "")?;
        }

        let mut environment = match &self.environment_prefix {
            Some(prefix) => read_environment(prefix)?,
            None => BTreeMap::new(),
        };
        environment.extend(self.explicit_environment);
        for (name, raw_value) in environment {
            let Some(prefix) = self.environment_prefix.as_deref() else {
                continue;
            };
            let Some(path) = split_environment_name(&name, prefix)? else {
                continue;
            };
            set_environment_value(&mut merged, &name, &path, parse_environment_value(&raw_value))?;
        }

        let config = serde_json::from_value(merged).map_err(ConfigError::Deserialize)?;
        if let Some(validator) = self.validator {
            validator(&config).map_err(ConfigError::Validation)?;
        }
        Ok(config)
    }
}

fn read_file_overlay(file: &FileSource) -> Result<Option<Value>, ConfigError> {
    let bytes = match fs::read(&file.path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound && !file.required => return Ok(None),
        Err(source) => {
            return Err(ConfigError::FileRead { path: file.path.clone(), source });
        }
    };
    let format = file
        .format
        .or_else(|| ConfigFileFormat::from_path(&file.path))
        .ok_or_else(|| ConfigError::UnsupportedFormat { path: file.path.clone() })?;
    let value = match format {
        ConfigFileFormat::Json => {
            serde_json::from_slice(&bytes).map_err(|source| ConfigError::FileParse {
                path: file.path.clone(),
                source: ConfigParseError::Json(source),
            })?
        }
        ConfigFileFormat::Toml => {
            let text = std::str::from_utf8(&bytes)
                .map_err(|source| ConfigError::FileEncoding { path: file.path.clone(), source })?;
            let value: toml::Value =
                toml::from_str(text).map_err(|source| ConfigError::FileParse {
                    path: file.path.clone(),
                    source: ConfigParseError::Toml(source),
                })?;
            serde_json::to_value(value).map_err(ConfigError::DefaultsEncode)?
        }
    };
    if !value.is_object() {
        return Err(ConfigError::RootMustBeObject { path: file.path.clone() });
    }
    Ok(Some(value))
}

fn read_environment(prefix: &str) -> Result<BTreeMap<String, String>, ConfigError> {
    let marker = format!("{}__", prefix.to_ascii_uppercase());
    let mut values = BTreeMap::new();
    for (key, value) in std::env::vars_os() {
        let Ok(key) = key.into_string() else {
            continue;
        };
        if !key.to_ascii_uppercase().starts_with(&marker) {
            continue;
        }
        let value = value
            .into_string()
            .map_err(|_| ConfigError::EnvironmentEncoding { variable: key.clone() })?;
        values.insert(key.to_ascii_uppercase(), value);
    }
    Ok(values)
}

fn split_environment_name(name: &str, prefix: &str) -> Result<Option<Vec<String>>, ConfigError> {
    let upper = name.to_ascii_uppercase();
    let marker = format!("{}__", prefix.to_ascii_uppercase());
    if !upper.starts_with(&marker) {
        return Ok(None);
    }
    let suffix = &name[marker.len()..];
    let segments: Vec<_> = suffix.split("__").collect();
    if segments.is_empty() || segments.iter().any(|segment| segment.is_empty()) {
        return Err(ConfigError::InvalidEnvironmentPath {
            variable: name.to_owned(),
            reason: "nested fields must be separated by exactly two underscores".to_owned(),
        });
    }
    Ok(Some(segments.into_iter().map(str::to_ascii_lowercase).collect()))
}

fn parse_environment_value(value: &str) -> Value {
    serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_owned()))
}

fn set_environment_value(
    root: &mut Value,
    variable: &str,
    path: &[String],
    value: Value,
) -> Result<(), ConfigError> {
    if path.is_empty() {
        return Err(ConfigError::InvalidEnvironmentPath {
            variable: variable.to_owned(),
            reason: "expected one or more field names after the prefix".to_owned(),
        });
    }
    let mut current = root;
    let mut traversed = Vec::new();
    for (index, segment) in path.iter().enumerate() {
        traversed.push(segment.as_str());
        let Value::Object(object) = current else {
            return Err(ConfigError::MergeConflict {
                path: traversed.join("."),
                source: variable.to_owned(),
            });
        };
        if index + 1 == path.len() {
            object.insert(segment.clone(), value);
            return Ok(());
        }
        let entry = object.entry(segment.clone()).or_insert_with(|| Value::Object(Map::new()));
        if !entry.is_object() {
            return Err(ConfigError::MergeConflict {
                path: traversed.join("."),
                source: variable.to_owned(),
            });
        }
        current = entry;
    }
    Ok(())
}

fn merge_value(base: &mut Value, overlay: Value, path: &str) -> Result<(), ConfigError> {
    match (base, overlay) {
        (Value::Object(base), Value::Object(overlay)) => {
            for (key, value) in overlay {
                let next_path = if path.is_empty() { key.clone() } else { format!("{path}.{key}") };
                if let Some(existing) = base.get_mut(&key) {
                    merge_value(existing, value, &next_path)?;
                } else {
                    base.insert(key, value);
                }
            }
            Ok(())
        }
        (base, overlay) => {
            *base = overlay;
            Ok(())
        }
    }
}

/// Underlying parser error for a configuration file.
#[derive(Debug)]
#[non_exhaustive]
pub enum ConfigParseError {
    /// Invalid JSON configuration.
    Json(serde_json::Error),
    /// Invalid TOML configuration.
    Toml(toml::de::Error),
}

impl Display for ConfigParseError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(error) => Display::fmt(error, formatter),
            Self::Toml(error) => Display::fmt(error, formatter),
        }
    }
}

impl Error for ConfigParseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            Self::Toml(error) => Some(error),
        }
    }
}

/// Configuration loading, precedence, parsing, and validation failures.
#[derive(Debug)]
#[non_exhaustive]
pub enum ConfigError {
    /// Defaults could not be encoded into the merge representation.
    DefaultsEncode(serde_json::Error),
    /// A configuration file could not be read.
    FileRead { path: PathBuf, source: io::Error },
    /// A file's bytes were not valid UTF-8 (TOML requires UTF-8).
    FileEncoding { path: PathBuf, source: std::str::Utf8Error },
    /// A configuration file could not be parsed.
    FileParse { path: PathBuf, source: ConfigParseError },
    /// A file has no `.json` or `.toml` extension and no format was supplied.
    UnsupportedFormat { path: PathBuf },
    /// Configuration overlays must have an object at their root.
    RootMustBeObject { path: PathBuf },
    /// An environment variable name could not be mapped to a nested field.
    InvalidEnvironmentPath { variable: String, reason: String },
    /// Explicit environment values were given without a configured prefix.
    MissingEnvironmentPrefix,
    /// The configured environment prefix contains unsupported characters.
    InvalidEnvironmentPrefix { prefix: String },
    /// An OS environment variable value is not valid Unicode.
    EnvironmentEncoding { variable: String },
    /// An overlay tried to descend through a scalar or incompatible value.
    MergeConflict { path: String, source: String },
    /// The merged document did not match the requested typed schema.
    Deserialize(serde_json::Error),
    /// The typed configuration failed its application validation callback.
    Validation(String),
}

impl Display for ConfigError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DefaultsEncode(error) => {
                write!(formatter, "could not encode configuration defaults: {error}")
            }
            Self::FileRead { path, source } => {
                write!(
                    formatter,
                    "could not read configuration file `{}`: {source}",
                    path.display()
                )
            }
            Self::FileEncoding { path, source } => {
                write!(formatter, "configuration file `{}` is not UTF-8: {source}", path.display())
            }
            Self::FileParse { path, source } => {
                write!(
                    formatter,
                    "could not parse configuration file `{}`: {source}",
                    path.display()
                )
            }
            Self::UnsupportedFormat { path } => write!(
                formatter,
                "configuration file `{}` must use JSON or TOML, or provide an explicit format",
                path.display()
            ),
            Self::RootMustBeObject { path } => {
                write!(
                    formatter,
                    "configuration file `{}` must contain a root object",
                    path.display()
                )
            }
            Self::InvalidEnvironmentPath { variable, reason } => {
                write!(
                    formatter,
                    "invalid configuration environment variable `{variable}`: {reason}"
                )
            }
            Self::MissingEnvironmentPrefix => formatter.write_str(
                "explicit configuration environment values require an environment prefix",
            ),
            Self::InvalidEnvironmentPrefix { prefix } => write!(
                formatter,
                "configuration environment prefix `{prefix}` must contain only ASCII letters, digits, and single underscores"
            ),
            Self::EnvironmentEncoding { variable } => {
                write!(
                    formatter,
                    "configuration environment variable `{variable}` is not valid Unicode"
                )
            }
            Self::MergeConflict { path, source } => write!(
                formatter,
                "configuration source `{source}` cannot merge through incompatible field `{path}`"
            ),
            Self::Deserialize(error) => write!(formatter, "configuration schema mismatch: {error}"),
            Self::Validation(message) => {
                write!(formatter, "configuration validation failed: {message}")
            }
        }
    }
}

impl Error for ConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::DefaultsEncode(source) | Self::Deserialize(source) => Some(source),
            Self::FileRead { source, .. } => Some(source),
            Self::FileEncoding { source, .. } => Some(source),
            Self::FileParse { source, .. } => Some(source),
            Self::UnsupportedFormat { .. }
            | Self::RootMustBeObject { .. }
            | Self::InvalidEnvironmentPath { .. }
            | Self::MissingEnvironmentPrefix
            | Self::InvalidEnvironmentPrefix { .. }
            | Self::EnvironmentEncoding { .. }
            | Self::MergeConflict { .. }
            | Self::Validation(_) => None,
        }
    }
}
