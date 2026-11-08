//! Backend-independent storage and addressing for block worlds.
//!
//! This crate deliberately does not define a game's block catalog, terrain
//! rules, rendering, streaming, or persistence format.

use std::collections::HashMap;

/// Edge length of a cubic chunk, in blocks.
pub const CHUNK_SIZE: i32 = 16;
/// Chunk edge length represented in local-coordinate storage.
pub const CHUNK_SIZE_U8: u8 = 16;
const CHUNK_VOLUME: usize = (CHUNK_SIZE as usize).pow(3);

/// Compact, game-defined block type identifier. ID zero is reserved for empty
/// space so newly allocated chunks need no initialization pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct BlockId(u32);

impl BlockId {
    /// Identifier for empty space.
    pub const AIR: Self = Self(0);

    /// Construct an identifier from its serialized or game-defined value.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Numeric identifier value.
    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }

    /// Whether this identifier represents empty space.
    #[must_use]
    pub const fn is_air(self) -> bool {
        self.0 == 0
    }
}

/// Integer position in the infinite block grid.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockPosition {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl BlockPosition {
    /// Create a block-grid position.
    #[must_use]
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    /// Chunk containing this block, using floor division for negative axes.
    #[must_use]
    pub fn chunk(self) -> ChunkPosition {
        ChunkPosition::new(
            self.x.div_euclid(CHUNK_SIZE),
            self.y.div_euclid(CHUNK_SIZE),
            self.z.div_euclid(CHUNK_SIZE),
        )
    }

    /// Coordinates local to the containing chunk, each in `0..CHUNK_SIZE`.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)] // rem_euclid(CHUNK_SIZE) is in 0..16.
    pub fn local(self) -> LocalBlockPosition {
        LocalBlockPosition::new(
            self.x.rem_euclid(CHUNK_SIZE) as u8,
            self.y.rem_euclid(CHUNK_SIZE) as u8,
            self.z.rem_euclid(CHUNK_SIZE) as u8,
        )
    }
}

/// Integer coordinate of a cubic chunk.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkPosition {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl ChunkPosition {
    /// Create a chunk coordinate.
    #[must_use]
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }
}

/// Local position in a chunk.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LocalBlockPosition {
    pub x: u8,
    pub y: u8,
    pub z: u8,
}

impl LocalBlockPosition {
    /// Create local coordinates. Values outside the chunk are rejected by
    /// [`Chunk::get`] and [`Chunk::set`].
    #[must_use]
    pub const fn new(x: u8, y: u8, z: u8) -> Self {
        Self { x, y, z }
    }

    const fn is_in_bounds(self) -> bool {
        self.x < CHUNK_SIZE_U8 && self.y < CHUNK_SIZE_U8 && self.z < CHUNK_SIZE_U8
    }
}

/// Dense cubic block storage. The x axis varies fastest, then z, then y.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    blocks: Vec<BlockId>,
}

impl Default for Chunk {
    fn default() -> Self {
        Self::empty()
    }
}

impl Chunk {
    /// Create an all-air chunk.
    #[must_use]
    pub fn empty() -> Self {
        Self { blocks: vec![BlockId::AIR; CHUNK_VOLUME] }
    }

    /// Read a local block. Returns `None` for out-of-range coordinates.
    #[must_use]
    pub fn get(&self, position: LocalBlockPosition) -> Option<BlockId> {
        position.is_in_bounds().then(|| self.blocks[index(position)])
    }

    /// Replace a local block and return its previous value. Returns `None`
    /// for out-of-range coordinates.
    pub fn set(&mut self, position: LocalBlockPosition, block: BlockId) -> Option<BlockId> {
        if !position.is_in_bounds() {
            return None;
        }
        let slot = &mut self.blocks[index(position)];
        Some(std::mem::replace(slot, block))
    }

    /// Whether the chunk contains only air.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.blocks.iter().all(|block| block.is_air())
    }

    /// Number of block slots in a chunk.
    #[must_use]
    pub const fn block_count(&self) -> usize {
        CHUNK_VOLUME
    }

    /// Read-only access to the packed block identifiers.
    #[must_use]
    pub fn blocks(&self) -> &[BlockId] {
        &self.blocks
    }
}

const fn index(position: LocalBlockPosition) -> usize {
    position.x as usize
        + CHUNK_SIZE as usize * position.z as usize
        + (CHUNK_SIZE as usize * CHUNK_SIZE as usize) * position.y as usize
}

/// Sparse collection of loaded chunks. Generation and saving are owned by
/// consumers; absent chunks read as air.
#[derive(Debug, Default, Clone)]
pub struct World {
    chunks: HashMap<ChunkPosition, Chunk>,
}

impl World {
    /// Create an empty world with no loaded chunks.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Read a block. An unloaded chunk is treated as air.
    #[must_use]
    pub fn get_block(&self, position: BlockPosition) -> BlockId {
        self.chunks
            .get(&position.chunk())
            .and_then(|chunk| chunk.get(position.local()))
            .unwrap_or(BlockId::AIR)
    }

    /// Set a block, allocating its chunk when necessary, and return the old
    /// block. Setting air in an unloaded chunk does not allocate it.
    pub fn set_block(&mut self, position: BlockPosition, block: BlockId) -> BlockId {
        let chunk_position = position.chunk();
        if block.is_air() && !self.chunks.contains_key(&chunk_position) {
            return BlockId::AIR;
        }
        self.chunks
            .entry(chunk_position)
            .or_default()
            .set(position.local(), block)
            .unwrap_or(BlockId::AIR)
    }

    /// Get a loaded chunk by coordinate.
    #[must_use]
    pub fn chunk(&self, position: ChunkPosition) -> Option<&Chunk> {
        self.chunks.get(&position)
    }

    /// Get a mutable loaded chunk by coordinate.
    pub fn chunk_mut(&mut self, position: ChunkPosition) -> Option<&mut Chunk> {
        self.chunks.get_mut(&position)
    }

    /// Insert or replace a loaded chunk and return the previous chunk, if any.
    pub fn insert_chunk(&mut self, position: ChunkPosition, chunk: Chunk) -> Option<Chunk> {
        self.chunks.insert(position, chunk)
    }

    /// Remove and return a loaded chunk.
    pub fn remove_chunk(&mut self, position: ChunkPosition) -> Option<Chunk> {
        self.chunks.remove(&position)
    }

    /// Number of currently loaded chunks.
    #[must_use]
    pub fn loaded_chunk_count(&self) -> usize {
        self.chunks.len()
    }

    /// Iterate over loaded chunks and their coordinates.
    pub fn chunks(&self) -> impl Iterator<Item = (ChunkPosition, &Chunk)> {
        self.chunks.iter().map(|(&position, chunk)| (position, chunk))
    }
}
