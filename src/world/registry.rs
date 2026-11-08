//! Data-driven block definitions owned by the game.

use nord_world::BlockId;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::Path;

const AIR_JSON: &str = include_str!("../../assets/blocks/air.json");
const DIRT_JSON: &str = include_str!("../../assets/blocks/dirt.json");
const GRASS_JSON: &str = include_str!("../../assets/blocks/grass.json");
const STONE_JSON: &str = include_str!("../../assets/blocks/stone.json");

/// A canonical, namespaced block key such as `nord:stone`.
pub type BlockKey = String;

/// Cube face used to select a block texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockFace {
    Top,
    Bottom,
    North,
    South,
    East,
    West,
}

/// Texture assignment accepted by block JSON: one path for every face, or
/// paths selected by face with an optional shared side texture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BlockTexture {
    Uniform(String),
    Faces(BlockFaceTextures),
}

/// Per-face texture paths relative to the game's `assets/` directory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockFaceTextures {
    #[serde(default)]
    pub top: Option<String>,
    #[serde(default)]
    pub bottom: Option<String>,
    #[serde(default)]
    pub side: Option<String>,
    #[serde(default)]
    pub north: Option<String>,
    #[serde(default)]
    pub south: Option<String>,
    #[serde(default)]
    pub east: Option<String>,
    #[serde(default)]
    pub west: Option<String>,
}

impl BlockTexture {
    /// Resolve the texture for one cube face, applying `side` as a fallback.
    #[must_use]
    pub fn for_face(&self, face: BlockFace) -> Option<&str> {
        match self {
            Self::Uniform(path) => Some(path),
            Self::Faces(faces) => {
                let selected = match face {
                    BlockFace::Top => faces.top.as_ref(),
                    BlockFace::Bottom => faces.bottom.as_ref(),
                    BlockFace::North => faces.north.as_ref(),
                    BlockFace::South => faces.south.as_ref(),
                    BlockFace::East => faces.east.as_ref(),
                    BlockFace::West => faces.west.as_ref(),
                };
                selected.or(faces.side.as_ref()).map(String::as_str)
            }
        }
    }

    fn paths(&self) -> impl Iterator<Item = &str> {
        let paths: Vec<&str> = match self {
            Self::Uniform(path) => vec![path],
            Self::Faces(faces) => [
                faces.top.as_deref(),
                faces.bottom.as_deref(),
                faces.side.as_deref(),
                faces.north.as_deref(),
                faces.south.as_deref(),
                faces.east.as_deref(),
                faces.west.as_deref(),
            ]
            .into_iter()
            .flatten()
            .collect(),
        };
        paths.into_iter()
    }
}

/// Gameplay and rendering hints declared by block data.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[allow(clippy::struct_excessive_bools)] // These independent flags describe separate block behaviors.
pub struct BlockProperties {
    pub collidable: bool,
    pub interactable: bool,
    pub breakable: bool,
    pub transparent: bool,
    pub replaceable: bool,
    pub placeable: bool,
    pub renderable: bool,
    pub hardness: f32,
}

impl Default for BlockProperties {
    fn default() -> Self {
        Self {
            collidable: true,
            interactable: true,
            breakable: true,
            transparent: false,
            replaceable: false,
            placeable: true,
            renderable: true,
            hardness: 1.0,
        }
    }
}

/// Validated block definition loaded from a game JSON file.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockDefinition {
    key: BlockKey,
    numeric_id: BlockId,
    name: String,
    properties: BlockProperties,
    texture: Option<BlockTexture>,
}

impl BlockDefinition {
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }
    #[must_use]
    pub const fn numeric_id(&self) -> BlockId {
        self.numeric_id
    }
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    #[must_use]
    pub const fn properties(&self) -> BlockProperties {
        self.properties
    }
    #[must_use]
    pub fn texture(&self) -> Option<&BlockTexture> {
        self.texture.as_ref()
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BlockFile {
    schema_version: u32,
    key: String,
    numeric_id: u32,
    name: String,
    #[serde(default)]
    properties: BlockProperties,
    #[serde(default)]
    texture: Option<BlockTexture>,
}

/// Errors found while loading or validating the game block registry.
#[derive(Debug)]
pub enum BlockRegistryError {
    Io { path: String, source: std::io::Error },
    Json { path: String, source: serde_json::Error },
    Invalid { path: String, reason: String },
    DuplicateKey(String),
    DuplicateNumericId(u32),
    MissingAir,
    MissingTerrainBlock(&'static str),
}

impl Display for BlockRegistryError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(f, "could not read block definition `{path}`: {source}")
            }
            Self::Json { path, source } => {
                write!(f, "invalid JSON in block definition `{path}`: {source}")
            }
            Self::Invalid { path, reason } => {
                write!(f, "invalid block definition `{path}`: {reason}")
            }
            Self::DuplicateKey(key) => write!(f, "duplicate block key `{key}`"),
            Self::DuplicateNumericId(id) => write!(f, "duplicate block numeric_id `{id}`"),
            Self::MissingAir => {
                f.write_str("block registry must define `nord:air` with numeric_id 0")
            }
            Self::MissingTerrainBlock(key) => write!(f, "flat terrain requires block `{key}`"),
        }
    }
}

impl Error for BlockRegistryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Json { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Immutable game block catalog with lookups by persistent key and numeric ID.
#[derive(Debug, Clone, Default)]
pub struct BlockRegistry {
    by_key: HashMap<BlockKey, BlockDefinition>,
    key_by_id: BTreeMap<u32, BlockKey>,
}

impl BlockRegistry {
    /// Load all `.json` definitions in a directory in deterministic path order.
    ///
    /// # Errors
    /// Returns an error for filesystem failures, malformed definitions, duplicate IDs/keys, or missing required blocks.
    pub fn load_dir(path: impl AsRef<Path>) -> Result<Self, BlockRegistryError> {
        let directory = path.as_ref();
        let entries = fs::read_dir(directory).map_err(|source| BlockRegistryError::Io {
            path: directory.display().to_string(),
            source,
        })?;
        let mut paths = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .collect::<Vec<_>>();
        paths.sort();
        let mut registry = Self::default();
        for file_path in paths {
            let text = fs::read_to_string(&file_path).map_err(|source| BlockRegistryError::Io {
                path: file_path.display().to_string(),
                source,
            })?;
            registry.register_json(&file_path.display().to_string(), &text)?;
        }
        registry.validate_air()?;
        registry.validate_terrain()?;
        Ok(registry)
    }

    /// Load the built-in game definitions embedded by the build.
    ///
    /// # Errors
    /// Returns an error if any embedded definition is invalid or conflicts with another ID/key.
    pub fn built_in() -> Result<Self, BlockRegistryError> {
        let mut registry = Self::default();
        for (path, json) in [
            ("air.json", AIR_JSON),
            ("dirt.json", DIRT_JSON),
            ("grass.json", GRASS_JSON),
            ("stone.json", STONE_JSON),
        ] {
            registry.register_json(path, json)?;
        }
        registry.validate_air()?;
        registry.validate_terrain()?;
        Ok(registry)
    }

    /// Find a definition using its persistent namespaced key.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&BlockDefinition> {
        self.by_key.get(key)
    }

    /// Find a definition using its compact chunk-storage ID.
    #[must_use]
    pub fn get_by_id(&self, id: BlockId) -> Option<&BlockDefinition> {
        self.key_by_id.get(&id.value()).and_then(|key| self.by_key.get(key))
    }

    /// Iterate definitions in stable numeric ID order.
    pub fn definitions(&self) -> impl Iterator<Item = &BlockDefinition> {
        self.key_by_id.values().filter_map(|key| self.by_key.get(key))
    }

    /// Number of registered block definitions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_key.len()
    }
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_key.is_empty()
    }

    fn register_json(&mut self, path: &str, json: &str) -> Result<(), BlockRegistryError> {
        let file: BlockFile = serde_json::from_str(json)
            .map_err(|source| BlockRegistryError::Json { path: path.to_owned(), source })?;
        let invalid = |reason: &str| BlockRegistryError::Invalid {
            path: path.to_owned(),
            reason: reason.to_owned(),
        };
        if file.schema_version != 1 {
            return Err(invalid("schema_version must be 1"));
        }
        if !valid_key(&file.key) {
            return Err(invalid("key must use lowercase namespace:path syntax"));
        }
        if file.name.trim().is_empty() {
            return Err(invalid("name must not be empty"));
        }
        if !file.properties.hardness.is_finite() || file.properties.hardness < 0.0 {
            return Err(invalid("properties.hardness must be finite and non-negative"));
        }
        if file.properties.replaceable && file.properties.collidable {
            return Err(invalid("replaceable blocks cannot be collidable"));
        }
        if file.properties.renderable && file.texture.is_none() {
            return Err(invalid("renderable blocks must declare a texture"));
        }
        if let Some(texture) = &file.texture {
            for path in texture.paths() {
                if !valid_texture_path(path) {
                    return Err(invalid(
                        "texture paths must be relative assets paths ending in a lowercase snake_case .png filename",
                    ));
                }
            }
            if file.properties.renderable
                && [
                    BlockFace::Top,
                    BlockFace::Bottom,
                    BlockFace::North,
                    BlockFace::South,
                    BlockFace::East,
                    BlockFace::West,
                ]
                .into_iter()
                .any(|face| texture.for_face(face).is_none())
            {
                return Err(invalid("renderable blocks must resolve a texture for every face"));
            }
        }
        if file.numeric_id == 0 && file.key != "nord:air" {
            return Err(invalid("numeric_id 0 is reserved for nord:air"));
        }
        if self.by_key.contains_key(&file.key) {
            return Err(BlockRegistryError::DuplicateKey(file.key));
        }
        if self.key_by_id.contains_key(&file.numeric_id) {
            return Err(BlockRegistryError::DuplicateNumericId(file.numeric_id));
        }
        let definition = BlockDefinition {
            key: file.key.clone(),
            numeric_id: BlockId::new(file.numeric_id),
            name: file.name,
            properties: file.properties,
            texture: file.texture,
        };
        self.key_by_id.insert(file.numeric_id, file.key.clone());
        self.by_key.insert(file.key, definition);
        Ok(())
    }

    fn validate_air(&self) -> Result<(), BlockRegistryError> {
        let air = self
            .by_key
            .get("nord:air")
            .filter(|block| block.numeric_id.value() == 0)
            .ok_or(BlockRegistryError::MissingAir)?;
        let p = air.properties;
        if p.collidable
            || p.interactable
            || p.breakable
            || p.placeable
            || p.renderable
            || !p.transparent
            || !p.replaceable
        {
            return Err(BlockRegistryError::Invalid { path: "nord:air".to_owned(), reason: "air must be non-collidable, non-interactable, non-breakable, transparent, replaceable, and not renderable".to_owned() });
        }
        Ok(())
    }

    fn validate_terrain(&self) -> Result<(), BlockRegistryError> {
        for key in ["nord:dirt", "nord:grass", "nord:stone"] {
            if !self.by_key.contains_key(key) {
                return Err(BlockRegistryError::MissingTerrainBlock(key));
            }
        }
        Ok(())
    }
}

fn valid_key(key: &str) -> bool {
    let Some((namespace, path)) = key.split_once(':') else {
        return false;
    };
    !namespace.is_empty()
        && !path.is_empty()
        && !path.contains(':')
        && namespace.bytes().all(key_char)
        && path.bytes().all(|c| key_char(c) || c == b'/')
}

fn valid_texture_path(path: &str) -> bool {
    if !path.starts_with("textures/")
        || path.contains('\\')
        || path.starts_with('/')
        || path.contains("://")
    {
        return false;
    }
    let Some(filename) = path.rsplit('/').next() else {
        return false;
    };
    Path::new(filename).extension().is_some_and(|extension| extension == "png")
        && filename.bytes().all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'_' | b'-' | b'.')
        })
        && !path.split('/').any(|part| part.is_empty() || part == "." || part == "..")
}

const fn key_char(value: u8) -> bool {
    value.is_ascii_lowercase()
        || value.is_ascii_digit()
        || value == b'_'
        || value == b'-'
        || value == b'.'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_registry_has_stable_round_trip_ids() {
        let registry = BlockRegistry::built_in().expect("built-in block data is valid");
        assert_eq!(registry.len(), 4);
        for (key, id) in [("nord:air", 0), ("nord:dirt", 1), ("nord:grass", 2), ("nord:stone", 3)] {
            let definition = registry.get(key).expect("block key is registered");
            assert_eq!(definition.numeric_id().value(), id);
            assert_eq!(
                registry.get_by_id(definition.numeric_id()).map(BlockDefinition::key),
                Some(key)
            );
        }
    }

    #[test]
    fn air_properties_match_empty_space_semantics() {
        let registry = BlockRegistry::built_in().expect("built-in block data is valid");
        let properties = registry.get("nord:air").expect("air is registered").properties();
        assert!(!properties.collidable);
        assert!(!properties.interactable);
        assert!(!properties.breakable);
        assert!(properties.transparent);
        assert!(properties.replaceable);
    }

    #[test]
    fn block_textures_support_uniform_and_per_face_assignments() {
        let registry = BlockRegistry::built_in().expect("built-in block data is valid");
        let dirt = registry.get("nord:dirt").expect("dirt is registered");
        assert_eq!(
            dirt.texture().and_then(|texture| texture.for_face(BlockFace::Top)),
            Some("textures/blocks/dirt_block.png")
        );
        let grass = registry.get("nord:grass").expect("grass is registered");
        let faces = grass.texture().expect("grass defines face textures");
        assert_eq!(faces.for_face(BlockFace::Top), Some("textures/blocks/grass_block.png"));
        assert_eq!(faces.for_face(BlockFace::Bottom), Some("textures/blocks/dirt_block.png"));
        assert_eq!(faces.for_face(BlockFace::North), Some("textures/blocks/grass_block_side.png"));
    }
}
