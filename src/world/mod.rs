//! Nord-specific voxel catalog, flat terrain, and block editing rules.

pub(crate) mod meshing;
mod registry;
mod streaming;

pub use registry::{BlockDefinition, BlockKey, BlockProperties, BlockRegistry, BlockRegistryError};
pub use streaming::{ChunkStreamConfig, ChunkStreamSnapshot, ChunkStreamer};

use nord_world::{BlockId, BlockPosition, Chunk, ChunkPosition, LocalBlockPosition, World};
use std::collections::{HashMap, HashSet};

/// A flat test world backed by generic cubic chunk storage.
///
/// Y=-1 is the grass surface, Y=-2..=-6 is dirt, and Y<=-7 is stone.
/// Chunks at Y>=0 remain sparse until a block is placed in them.
#[derive(Debug)]
pub struct VoxelWorld {
    storage: World,
    modified_chunks: HashSet<ChunkPosition>,
    block_edits: HashMap<ChunkPosition, HashMap<LocalBlockPosition, BlockId>>,
    dirty_chunks: HashSet<ChunkPosition>,
    registry: BlockRegistry,
    minimum_chunk_y: i32,
    maximum_chunk_y: i32,
}

impl VoxelWorld {
    /// Create the initial flat world.
    #[must_use]
    pub fn new(registry: BlockRegistry) -> Self {
        Self::with_vertical_bounds(registry, 2, 64)
    }

    /// Create a world with vertical bounds counted in chunks from Y=0.
    ///
    /// # Panics
    /// Panics when either chunk count is less than one.
    #[must_use]
    pub fn with_vertical_bounds(
        registry: BlockRegistry,
        chunks_above_origin: i32,
        chunks_below_origin: i32,
    ) -> Self {
        assert!(chunks_above_origin >= 1, "chunks_above_origin must be at least one");
        assert!(chunks_below_origin >= 1, "chunks_below_origin must be at least one");
        Self {
            storage: World::new(),
            modified_chunks: HashSet::new(),
            block_edits: HashMap::new(),
            dirty_chunks: HashSet::new(),
            registry,
            minimum_chunk_y: -chunks_below_origin,
            maximum_chunk_y: chunks_above_origin - 1,
        }
    }

    /// Read a block, generating its terrain chunk on demand.
    pub fn block_at(&mut self, position: BlockPosition) -> BlockId {
        if !self.chunk_y_in_bounds(position.chunk().y) {
            return BlockId::AIR;
        }
        self.ensure_terrain_chunk(position.chunk());
        self.storage.get_block(position)
    }

    /// Remove a non-air block and return its previous kind.
    ///
    /// Returns `None` when the target is already air.
    pub fn break_block(&mut self, position: BlockPosition) -> Option<BlockId> {
        let previous = self.block_at(position);
        if previous.is_air() || !self.registry.get_by_id(previous)?.properties().breakable {
            return None;
        }
        self.storage.set_block(position, BlockId::AIR);
        self.record_edit(position, BlockId::AIR);
        self.modified_chunks.insert(position.chunk());
        Some(previous)
    }

    /// Place a non-air block into an empty cell.
    ///
    /// Returns `false` if the requested block is air or the cell is occupied.
    pub fn place_block(&mut self, position: BlockPosition, block: BlockId) -> bool {
        if !self.chunk_y_in_bounds(position.chunk().y) {
            return false;
        }
        let Some(definition) = self.registry.get_by_id(block) else {
            return false;
        };
        if block.is_air()
            || !definition.properties().placeable
            || self.block_at(position) != BlockId::AIR
        {
            return false;
        }
        self.storage.set_block(position, block);
        self.record_edit(position, block);
        self.modified_chunks.insert(position.chunk());
        true
    }

    /// Explicitly generate and load a terrain chunk. Chunks fully above the
    /// plane remain absent until edited.
    pub fn generate_chunk(&mut self, position: ChunkPosition) {
        self.ensure_terrain_chunk(position);
    }

    /// Insert terrain produced by a background streaming job, applying any
    /// in-memory player edits recorded for this chunk.
    pub fn insert_generated_chunk(&mut self, position: ChunkPosition, mut chunk: Chunk) {
        if !self.chunk_y_in_bounds(position.y) {
            return;
        }
        if let Some(edits) = self.block_edits.get(&position) {
            for (&local, &block) in edits {
                let _ = chunk.set(local, block);
            }
        }
        self.storage.insert_chunk(position, chunk);
        self.dirty_chunks.insert(position);
    }

    /// Unload a chunk's generated data while retaining its edit overlay.
    pub fn unload_chunk(&mut self, position: ChunkPosition) -> Option<Chunk> {
        let removed = self.storage.remove_chunk(position);
        if removed.is_some() {
            self.dirty_chunks.insert(position);
        }
        removed
    }

    /// Sample terrain and edits without allocating an unloaded chunk.
    #[must_use]
    pub fn sample_block_without_loading(&self, position: BlockPosition) -> BlockId {
        let chunk_position = position.chunk();
        if !self.chunk_y_in_bounds(chunk_position.y) {
            return BlockId::AIR;
        }
        if let Some(chunk) = self.storage.chunk(chunk_position) {
            return chunk.get(position.local()).unwrap_or(BlockId::AIR);
        }
        if let Some(&edited) =
            self.block_edits.get(&chunk_position).and_then(|edits| edits.get(&position.local()))
        {
            return edited;
        }
        if position.y >= 0 {
            BlockId::AIR
        } else if position.y == -1 {
            self.registry.get("nord:grass").map_or(BlockId::AIR, |block| block.numeric_id())
        } else if (-6..=-2).contains(&position.y) {
            self.registry.get("nord:dirt").map_or(BlockId::AIR, |block| block.numeric_id())
        } else {
            self.registry.get("nord:stone").map_or(BlockId::AIR, |block| block.numeric_id())
        }
    }

    /// Take chunk coordinates whose rendered geometry must be rebuilt.
    pub fn take_dirty_chunks(&mut self) -> Vec<ChunkPosition> {
        self.dirty_chunks.drain().collect()
    }

    /// Read-only access to the loaded generic chunk data.
    #[must_use]
    pub fn chunk(&self, position: ChunkPosition) -> Option<&Chunk> {
        self.storage.chunk(position)
    }

    /// Number of allocated chunks, including edited chunks above ground.
    #[must_use]
    pub fn loaded_chunk_count(&self) -> usize {
        self.storage.loaded_chunk_count()
    }

    /// Iterate coordinates for currently resident chunks.
    pub fn loaded_chunk_positions(&self) -> impl Iterator<Item = ChunkPosition> + '_ {
        self.storage.chunks().map(|(position, _)| position)
    }

    /// Chunks changed by player edits since world creation.
    pub fn modified_chunks(&self) -> impl Iterator<Item = ChunkPosition> + '_ {
        self.modified_chunks.iter().copied()
    }

    /// Immutable block catalog used to interpret chunk IDs.
    #[must_use]
    pub const fn registry(&self) -> &BlockRegistry {
        &self.registry
    }

    /// Clear edit markers after a future persistence layer has saved them.
    pub fn clear_modified_chunks(&mut self) {
        self.modified_chunks.clear();
    }

    fn ensure_terrain_chunk(&mut self, position: ChunkPosition) {
        if self.storage.chunk(position).is_some() || !self.chunk_y_in_bounds(position.y) {
            return;
        }

        let chunk = if position.y < 0 {
            Some(generate_flat_terrain_chunk(position, self.terrain_block_ids()))
        } else {
            self.block_edits.get(&position).map(|_| Chunk::empty())
        };
        if let Some(chunk) = chunk {
            self.insert_generated_chunk(position, chunk);
        }
    }

    pub(super) fn terrain_block_ids(&self) -> FlatTerrainBlockIds {
        FlatTerrainBlockIds {
            grass: self
                .registry
                .get("nord:grass")
                .expect("built-in registry requires grass")
                .numeric_id(),
            dirt: self
                .registry
                .get("nord:dirt")
                .expect("built-in registry requires dirt")
                .numeric_id(),
            stone: self
                .registry
                .get("nord:stone")
                .expect("built-in registry requires stone")
                .numeric_id(),
        }
    }

    fn record_edit(&mut self, position: BlockPosition, block: BlockId) {
        let chunk = position.chunk();
        let local = position.local();
        self.block_edits.entry(chunk).or_default().insert(local, block);
        self.dirty_chunks.insert(chunk);
        if local.x == 0 {
            if let Some(x) = chunk.x.checked_sub(1) {
                self.dirty_chunks.insert(ChunkPosition::new(x, chunk.y, chunk.z));
            }
        } else if local.x == nord_world::CHUNK_SIZE_U8 - 1
            && let Some(x) = chunk.x.checked_add(1)
        {
            self.dirty_chunks.insert(ChunkPosition::new(x, chunk.y, chunk.z));
        }
        if local.y == 0 {
            if let Some(y) = chunk.y.checked_sub(1) {
                self.dirty_chunks.insert(ChunkPosition::new(chunk.x, y, chunk.z));
            }
        } else if local.y == nord_world::CHUNK_SIZE_U8 - 1
            && let Some(y) = chunk.y.checked_add(1)
        {
            self.dirty_chunks.insert(ChunkPosition::new(chunk.x, y, chunk.z));
        }
        if local.z == 0 {
            if let Some(z) = chunk.z.checked_sub(1) {
                self.dirty_chunks.insert(ChunkPosition::new(chunk.x, chunk.y, z));
            }
        } else if local.z == nord_world::CHUNK_SIZE_U8 - 1
            && let Some(z) = chunk.z.checked_add(1)
        {
            self.dirty_chunks.insert(ChunkPosition::new(chunk.x, chunk.y, z));
        }
    }

    fn chunk_y_in_bounds(&self, chunk_y: i32) -> bool {
        (self.minimum_chunk_y..=self.maximum_chunk_y).contains(&chunk_y)
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct FlatTerrainBlockIds {
    pub(super) grass: BlockId,
    pub(super) dirt: BlockId,
    pub(super) stone: BlockId,
}

pub(super) fn generate_flat_terrain_chunk(
    position: ChunkPosition,
    ids: FlatTerrainBlockIds,
) -> Chunk {
    let mut chunk = Chunk::empty();
    for local_y in 0..nord_world::CHUNK_SIZE_U8 {
        let world_y =
            i64::from(position.y) * i64::from(nord_world::CHUNK_SIZE) + i64::from(local_y);
        let block = if world_y == -1 {
            ids.grass
        } else if (-6..=-2).contains(&world_y) {
            ids.dirt
        } else {
            ids.stone
        };
        for local_z in 0..nord_world::CHUNK_SIZE_U8 {
            for local_x in 0..nord_world::CHUNK_SIZE_U8 {
                let _ = chunk.set(LocalBlockPosition::new(local_x, local_y, local_z), block);
            }
        }
    }
    chunk
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> VoxelWorld {
        VoxelWorld::new(BlockRegistry::built_in().expect("built-in block data is valid"))
    }

    #[test]
    fn flat_terrain_uses_expected_layers_across_negative_chunks() {
        let mut world = world();
        let registry = world.registry().clone();
        let id = |key| registry.get(key).expect("terrain block is registered").numeric_id();
        assert_eq!(world.block_at(BlockPosition::new(-1, -1, -1)), id("nord:grass"));
        assert_eq!(world.block_at(BlockPosition::new(-1, -2, -1)), id("nord:dirt"));
        assert_eq!(world.block_at(BlockPosition::new(-1, -6, -1)), id("nord:dirt"));
        assert_eq!(world.block_at(BlockPosition::new(-1, -7, -1)), id("nord:stone"));
        assert_eq!(world.block_at(BlockPosition::new(-1, 0, -1)), BlockId::AIR);
    }

    #[test]
    fn block_edits_respect_registry_properties_and_track_dirty_chunks() {
        let mut world = world();
        let grass = world.registry().get("nord:grass").expect("grass is registered").numeric_id();
        let dirt = world.registry().get("nord:dirt").expect("dirt is registered").numeric_id();
        let target = BlockPosition::new(0, -1, 0);
        assert_eq!(world.break_block(target), Some(grass));
        assert!(world.modified_chunks().any(|coord| coord == target.chunk()));
        assert!(world.place_block(target, dirt));
        assert_eq!(world.block_at(target), dirt);
        assert!(!world.place_block(target, grass));
        assert_eq!(world.break_block(target), Some(dirt));
        let chunk_position = target.chunk();
        let _ = world.take_dirty_chunks();
        let _ = world.unload_chunk(chunk_position);
        assert_eq!(world.take_dirty_chunks(), vec![chunk_position]);
        assert_eq!(world.block_at(target), BlockId::AIR);
    }

    #[test]
    fn configured_vertical_chunk_bounds_apply_to_reads_and_placement() {
        let registry = BlockRegistry::built_in().expect("built-in block data is valid");
        let stone = registry.get("nord:stone").expect("stone is registered").numeric_id();
        let mut world = VoxelWorld::with_vertical_bounds(registry, 2, 1);

        assert_eq!(world.block_at(BlockPosition::new(0, -17, 0)), BlockId::AIR);
        assert!(!world.place_block(BlockPosition::new(0, -17, 0), stone));
        assert!(world.place_block(BlockPosition::new(0, 16, 0), stone));
    }

    #[test]
    fn edited_air_chunks_are_restored_when_the_player_returns() {
        let mut world = world();
        let stone = world.registry().get("nord:stone").expect("stone is registered").numeric_id();
        let target = BlockPosition::new(0, 1, 0);
        assert!(world.place_block(target, stone));
        let _ = world.unload_chunk(target.chunk());
        assert_eq!(world.block_at(target), stone);
    }
}
