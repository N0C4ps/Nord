//! Greedy face meshing for game-owned voxel chunks.

use super::{VoxelWorld, registry::BlockFace};
use nord_app::ApplicationVec3 as Vec3;
use nord_world::{BlockId, BlockPosition, CHUNK_SIZE, CHUNK_SIZE_U8, ChunkPosition};
use std::collections::{BTreeSet, HashMap};

/// Vertex layout consumed by the game's textured voxel pipeline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct VoxelVertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub tangent: [f32; 4],
    pub tex_coord: [f32; 2],
}

/// One chunk-mesh section that uses a single registered texture path.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MeshSection {
    pub texture_path: String,
    pub vertices: Vec<VoxelVertex>,
    pub indices: Vec<u32>,
}

/// Greedily merged visible surfaces for one chunk.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ChunkMesh {
    pub position: ChunkPosition,
    pub sections: Vec<MeshSection>,
}

impl ChunkMesh {
    /// Build a material-grouped greedy mesh without allocating neighboring chunks.
    pub fn build(world: &VoxelWorld, position: ChunkPosition) -> Option<Self> {
        let chunk = world.chunk(position)?;
        let mut texture_paths = BTreeSet::new();
        for definition in world.registry().definitions() {
            if let Some(texture) = definition.texture() {
                for face in all_faces() {
                    if let Some(path) = texture.for_face(face) {
                        texture_paths.insert(path.to_owned());
                    }
                }
            }
        }
        let texture_paths = texture_paths.into_iter().collect::<Vec<_>>();
        let texture_ids = texture_paths
            .iter()
            .enumerate()
            .map(|(index, path)| (path.clone(), index))
            .collect::<HashMap<_, _>>();
        let mut sections = texture_paths
            .into_iter()
            .map(|texture_path| MeshSection {
                texture_path,
                vertices: Vec::new(),
                indices: Vec::new(),
            })
            .collect::<Vec<_>>();
        let mut mask = vec![None; usize::from(CHUNK_SIZE_U8) * usize::from(CHUNK_SIZE_U8)];

        for axis in 0..3 {
            let u_axis = (axis + 1) % 3;
            let v_axis = (axis + 2) % 3;
            for positive in [false, true] {
                let face = face_for_axis(axis, positive);
                for slice in 0..CHUNK_SIZE_U8 {
                    mask.fill(None);
                    for v in 0..CHUNK_SIZE_U8 {
                        for u in 0..CHUNK_SIZE_U8 {
                            let mut local = [0; 3];
                            local[axis] = slice;
                            local[u_axis] = u;
                            local[v_axis] = v;
                            let Some(block) = chunk.get(nord_world::LocalBlockPosition::new(
                                local[0], local[1], local[2],
                            )) else {
                                continue;
                            };
                            let Some(definition) = world.registry().get_by_id(block) else {
                                continue;
                            };
                            if !definition.properties().renderable {
                                continue;
                            }
                            let neighbor = sample_neighbor(
                                world,
                                position,
                                local,
                                axis,
                                if positive { 1 } else { -1 },
                            )?;
                            let neighbor_occludes =
                                world.registry().get_by_id(neighbor).is_some_and(|adjacent| {
                                    adjacent.properties().renderable
                                        && !adjacent.properties().transparent
                                });
                            if neighbor_occludes {
                                continue;
                            }
                            let Some(path) =
                                definition.texture().and_then(|texture| texture.for_face(face))
                            else {
                                continue;
                            };
                            let Some(&texture_id) = texture_ids.get(path) else {
                                continue;
                            };
                            mask[usize::from(v) * usize::from(CHUNK_SIZE_U8) + usize::from(u)] =
                                Some(texture_id);
                        }
                    }

                    greedy_emit_slice(
                        &mut mask,
                        &mut sections,
                        position,
                        axis,
                        u_axis,
                        v_axis,
                        slice,
                        positive,
                    )?;
                }
            }
        }
        sections.retain(|section| !section.indices.is_empty());
        Some(Self { position, sections })
    }
}

fn greedy_emit_slice(
    mask: &mut [Option<usize>],
    sections: &mut [MeshSection],
    position: ChunkPosition,
    axis: usize,
    u_axis: usize,
    v_axis: usize,
    slice: u8,
    positive: bool,
) -> Option<()> {
    let width = usize::from(CHUNK_SIZE_U8);
    let normal = axis_vector(axis, positive);
    let tangent = axis_vector(u_axis, true);
    let bitangent = axis_vector(v_axis, true);
    let handedness = if normal.cross(tangent).dot(bitangent) < 0.0 { -1.0 } else { 1.0 };
    for v in 0..width {
        let mut u = 0;
        while u < width {
            let mask_index = v * width + u;
            let Some(texture_id) = mask[mask_index] else {
                u += 1;
                continue;
            };
            let mut quad_width = 1;
            while u + quad_width < width && mask[v * width + u + quad_width] == Some(texture_id) {
                quad_width += 1;
            }
            let mut quad_height = 1;
            'height: while v + quad_height < width {
                for offset in 0..quad_width {
                    if mask[(v + quad_height) * width + u + offset] != Some(texture_id) {
                        break 'height;
                    }
                }
                quad_height += 1;
            }
            for row in 0..quad_height {
                for column in 0..quad_width {
                    mask[(v + row) * width + u + column] = None;
                }
            }
            let section = sections.get_mut(texture_id)?;
            let mut origin = [0.0_f32; 3];
            origin[axis] = f32::from(slice) + if positive { 1.0 } else { 0.0 };
            origin[u_axis] = u as f32;
            origin[v_axis] = v as f32;
            let corners = quad_corners(
                origin,
                u_axis,
                v_axis,
                quad_width as f32,
                quad_height as f32,
                positive,
            );
            let tex_coords = if positive {
                [
                    [0.0, 0.0],
                    [quad_width as f32, 0.0],
                    [quad_width as f32, quad_height as f32],
                    [0.0, quad_height as f32],
                ]
            } else {
                [
                    [0.0, 0.0],
                    [0.0, quad_height as f32],
                    [quad_width as f32, quad_height as f32],
                    [quad_width as f32, 0.0],
                ]
            };
            let base = u32::try_from(section.vertices.len()).ok()?;
            for index in 0..4 {
                let mut world_position = corners[index];
                world_position[0] += position.x as f32 * CHUNK_SIZE as f32;
                world_position[1] += position.y as f32 * CHUNK_SIZE as f32;
                world_position[2] += position.z as f32 * CHUNK_SIZE as f32;
                section.vertices.push(VoxelVertex {
                    position: Vec3::new(world_position[0], world_position[1], world_position[2]),
                    normal,
                    tangent: [tangent.x, tangent.y, tangent.z, handedness],
                    tex_coord: tex_coords[index],
                });
            }
            section.indices.extend_from_slice(&[
                base,
                base + 1,
                base + 2,
                base,
                base + 2,
                base + 3,
            ]);
            u += quad_width;
        }
    }
    Some(())
}

fn quad_corners(
    origin: [f32; 3],
    u_axis: usize,
    v_axis: usize,
    width: f32,
    height: f32,
    positive: bool,
) -> [[f32; 3]; 4] {
    let mut along_u = origin;
    along_u[u_axis] += width;
    let mut along_v = origin;
    along_v[v_axis] += height;
    let mut opposite = along_u;
    opposite[v_axis] += height;
    if positive {
        [origin, along_u, opposite, along_v]
    } else {
        [origin, along_v, opposite, along_u]
    }
}

fn sample_neighbor(
    world: &VoxelWorld,
    chunk: ChunkPosition,
    local: [u8; 3],
    axis: usize,
    offset: i32,
) -> Option<BlockId> {
    let mut coordinates = [
        i64::from(chunk.x) * i64::from(CHUNK_SIZE) + i64::from(local[0]),
        i64::from(chunk.y) * i64::from(CHUNK_SIZE) + i64::from(local[1]),
        i64::from(chunk.z) * i64::from(CHUNK_SIZE) + i64::from(local[2]),
    ];
    coordinates[axis] += i64::from(offset);
    let block = BlockPosition::new(
        i32::try_from(coordinates[0]).ok()?,
        i32::try_from(coordinates[1]).ok()?,
        i32::try_from(coordinates[2]).ok()?,
    );
    Some(world.sample_block_without_loading(block))
}

const fn all_faces() -> [BlockFace; 6] {
    [
        BlockFace::Top,
        BlockFace::Bottom,
        BlockFace::North,
        BlockFace::South,
        BlockFace::East,
        BlockFace::West,
    ]
}

const fn face_for_axis(axis: usize, positive: bool) -> BlockFace {
    match (axis, positive) {
        (0, true) => BlockFace::East,
        (0, false) => BlockFace::West,
        (1, true) => BlockFace::Top,
        (1, false) => BlockFace::Bottom,
        (2, true) => BlockFace::South,
        _ => BlockFace::North,
    }
}

const fn axis_vector(axis: usize, positive: bool) -> Vec3 {
    let sign = if positive { 1.0 } else { -1.0 };
    match axis {
        0 => Vec3::new(sign, 0.0, 0.0),
        1 => Vec3::new(0.0, sign, 0.0),
        _ => Vec3::new(0.0, 0.0, sign),
    }
}

#[cfg(test)]
mod tests {
    use super::ChunkMesh;
    use crate::world::{BlockRegistry, VoxelWorld};
    use nord_world::ChunkPosition;

    #[test]
    fn greedy_surface_chunk_is_one_grass_quad_and_seams_are_occluded() {
        let mut world = VoxelWorld::new(BlockRegistry::built_in().expect("built-in registry"));
        let position = ChunkPosition::new(0, -1, 0);
        world.generate_chunk(position);
        let mesh = ChunkMesh::build(&world, position).expect("loaded surface chunk meshes");
        let section = mesh
            .sections
            .iter()
            .find(|section| section.texture_path.ends_with("grass_block.png"))
            .expect("grass top texture section");
        assert_eq!(section.vertices.len(), 4);
        assert_eq!(section.indices.len(), 6);
        assert!(mesh.sections.iter().all(|section| section.indices.len() <= 6));
    }

    #[test]
    fn fully_underground_solid_chunk_has_no_exposed_faces() {
        let mut world = VoxelWorld::new(BlockRegistry::built_in().expect("built-in registry"));
        let position = ChunkPosition::new(-1, -2, 0);
        world.generate_chunk(position);
        let mesh = ChunkMesh::build(&world, position).expect("loaded stone chunk meshes");
        assert!(mesh.sections.is_empty());
    }
}
