//! Layered, typed configuration loading for engine and game settings.
//!
//! Values are merged in this order: typed defaults, files in insertion order,
//! environment variables, then explicit environment overrides. The completed
//! document is deserialized and validated before it is returned.

mod loader;

pub use loader::{ConfigError, ConfigFileFormat, ConfigLoader, ConfigParseError, ConfigValidator};
