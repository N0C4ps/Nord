//! GPU upload and frustum drawing for game-owned voxel chunk meshes.

use super::material::BlockTextureMaterials;
use crate::world::{
    VoxelWorld,
    meshing::{ChunkMesh, MeshSection},
};
use nord_app::{
    ApplicationAabb3, ApplicationBufferDesc, ApplicationBufferUsage, ApplicationDiagnosticError,
    ApplicationErrorContext, ApplicationFrustum3, ApplicationGpuResourceHandle,
    ApplicationRenderer, ApplicationResourceUsage, ApplicationVec3,
};
use nord_world::{CHUNK_SIZE, ChunkPosition};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

const VERTEX_STRIDE: u32 = 48;
const MAX_CHUNK_MESH_BUILDS_PER_FRAME: usize = 4;
const MESH_BATCH_CHUNKS: i32 = 4;

#[derive(Clone, Copy)]
pub(super) struct VoxelDraw {
    pub vertex: ApplicationGpuResourceHandle,
    pub vertex_size: u32,
    pub index: ApplicationGpuResourceHandle,
    pub index_size: u32,
    pub index_count: u32,
    pub base_color_view: nord_app::ApplicationDescriptorHandle,
    bounds: Option<ApplicationAabb3>,
}

struct ChunkGpuMesh {
    sections: Vec<VoxelDraw>,
}

pub(super) struct VoxelRenderer {
    chunk_meshes: HashMap<ChunkPosition, ChunkMesh>,
    batches: HashMap<ChunkPosition, ChunkGpuMesh>,
    pending_meshes: HashSet<ChunkPosition>,
    visible_draw_cache: Option<(ApplicationFrustum3, Arc<[VoxelDraw]>)>,
    instance_buffer: ApplicationGpuResourceHandle,
    instance_size: u32,
}

impl VoxelRenderer {
    pub(super) fn create(renderer: &mut ApplicationRenderer) -> nord_app::ApplicationResult<Self> {
        let bytes = [0.0_f32, 0.0, 0.0, 1.0, 1.0, 1.0]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect::<Vec<_>>();
        let instance_size = u32::try_from(bytes.len())
            .map_err(ApplicationDiagnosticError::msg)
            .context("checking the voxel mesh instance data size")?;
        let instance_buffer = renderer
            .create_buffer(ApplicationBufferDesc::new(
                u64::from(instance_size),
                ApplicationBufferUsage::COPY_DESTINATION.union(ApplicationBufferUsage::VERTEX),
            )?)
            .map_err(ApplicationDiagnosticError::msg)
            .context("allocating the voxel mesh instance buffer")?;
        renderer
            .upload_buffer(instance_buffer, 0, &bytes)
            .map_err(ApplicationDiagnosticError::msg)
            .context("uploading the voxel mesh instance data")?;
        Ok(Self {
            chunk_meshes: HashMap::new(),
            batches: HashMap::new(),
            pending_meshes: HashSet::new(),
            visible_draw_cache: None,
            instance_buffer,
            instance_size,
        })
    }

    pub(super) fn sync(
        &mut self,
        renderer: &mut ApplicationRenderer,
        world: &mut VoxelWorld,
        textures: &BlockTextureMaterials,
        focus: ChunkPosition,
    ) -> nord_app::ApplicationResult<()> {
        let mut retired = Vec::new();
        let mut uploads = Vec::new();
        let mut geometry_changed = false;
        let mut dirty_batches = HashSet::new();
        self.pending_meshes.extend(world.take_dirty_chunks());
        for _ in 0..MAX_CHUNK_MESH_BUILDS_PER_FRAME {
            let Some(position) = self.pending_meshes.iter().copied().min_by_key(|position| {
                let dx = i128::from(position.x) - i128::from(focus.x);
                let dy = i128::from(position.y) - i128::from(focus.y);
                let dz = i128::from(position.z) - i128::from(focus.z);
                dx.pow(2) + dy.pow(2) + dz.pow(2)
            }) else {
                break;
            };
            self.pending_meshes.remove(&position);
            if world.chunk(position).is_none() {
                if self.chunk_meshes.remove(&position).is_some() {
                    dirty_batches.insert(mesh_batch_position(position));
                }
            } else if let Some(mesh) = ChunkMesh::build(world, position) {
                self.chunk_meshes.insert(position, mesh);
                dirty_batches.insert(mesh_batch_position(position));
            }
        }
        for batch_position in dirty_batches {
            if let Some(old) = self.batches.remove(&batch_position) {
                append_chunk_resources(old, &mut retired);
                geometry_changed = true;
            }
            let mesh = combine_batch_mesh(batch_position, &self.chunk_meshes)?;
            if !mesh.sections.is_empty() {
                let (gpu_mesh, section_uploads) =
                    upload_chunk_mesh(renderer, mesh, batch_position, textures)?;
                self.batches.insert(batch_position, gpu_mesh);
                uploads.extend(section_uploads);
                geometry_changed = true;
            }
        }
        renderer
            .destroy_resources(&retired)
            .map_err(ApplicationDiagnosticError::msg)
            .context("retiring streamed voxel buffers in one GPU synchronization")?;
        if !uploads.is_empty() {
            let upload_views = uploads
                .iter()
                .map(|(resource, bytes)| (*resource, 0, bytes.as_slice()))
                .collect::<Vec<_>>();
            renderer
                .upload_buffers(&upload_views)
                .map_err(ApplicationDiagnosticError::msg)
                .context("uploading changed voxel meshes in one GPU submission")?;
        }
        if geometry_changed {
            self.visible_draw_cache = None;
        }
        Ok(())
    }

    pub(super) fn visible_draws(&mut self, frustum: ApplicationFrustum3) -> Arc<[VoxelDraw]> {
        if let Some((cached_frustum, draws)) = &self.visible_draw_cache
            && *cached_frustum == frustum
        {
            return Arc::clone(draws);
        }
        let visible = self
            .batches
            .values()
            .flat_map(|mesh| mesh.sections.iter())
            .filter(|draw| draw.bounds.is_none_or(|bounds| frustum.intersects_aabb(bounds)))
            .copied()
            .collect::<Vec<_>>();
        let draws: Arc<[VoxelDraw]> = visible.into();
        self.visible_draw_cache = Some((frustum, Arc::clone(&draws)));
        draws
    }

    pub(super) fn has_pending_meshes(&self) -> bool {
        !self.pending_meshes.is_empty()
    }

    pub(super) const fn instance_buffer(&self) -> ApplicationGpuResourceHandle {
        self.instance_buffer
    }
    pub(super) const fn instance_size(&self) -> u32 {
        self.instance_size
    }

    pub(super) fn shutdown(
        &mut self,
        renderer: &mut ApplicationRenderer,
    ) -> nord_app::ApplicationResult<()> {
        let mut resources = Vec::new();
        for (_, mesh) in self.batches.drain() {
            append_chunk_resources(mesh, &mut resources);
        }
        self.chunk_meshes.clear();
        self.pending_meshes.clear();
        resources.push(self.instance_buffer);
        renderer
            .destroy_resources(&resources)
            .map_err(ApplicationDiagnosticError::msg)
            .context("retiring voxel mesh buffers in one GPU synchronization")
    }
}

fn upload_chunk_mesh(
    renderer: &mut ApplicationRenderer,
    mesh: ChunkMesh,
    batch_position: ChunkPosition,
    textures: &BlockTextureMaterials,
) -> nord_app::ApplicationResult<(ChunkGpuMesh, Vec<(ApplicationGpuResourceHandle, Vec<u8>)>)> {
    let bounds = batch_bounds(batch_position);
    let mut sections = Vec::with_capacity(mesh.sections.len());
    let mut uploads = Vec::with_capacity(mesh.sections.len() * 2);
    for section in mesh.sections {
        let texture =
            BlockTextureMaterials::id_for_path(&section.texture_path).ok_or_else(|| {
                ApplicationDiagnosticError::msg(format!(
                    "voxel mesh references unsupported embedded texture `{}`",
                    section.texture_path
                ))
            })?;
        let base_color_view = textures.view_for_id(texture).ok_or_else(|| {
            ApplicationDiagnosticError::msg("voxel texture descriptor was not initialized")
        })?;
        let mut vertex_bytes = Vec::with_capacity(section.vertices.len() * VERTEX_STRIDE as usize);
        for vertex in section.vertices {
            for value in [
                vertex.position.x,
                vertex.position.y,
                vertex.position.z,
                vertex.normal.x,
                vertex.normal.y,
                vertex.normal.z,
                vertex.tangent[0],
                vertex.tangent[1],
                vertex.tangent[2],
                vertex.tangent[3],
                vertex.tex_coord[0],
                vertex.tex_coord[1],
            ] {
                vertex_bytes.extend_from_slice(&value.to_le_bytes());
            }
        }
        let index_bytes =
            section.indices.iter().flat_map(|index| index.to_le_bytes()).collect::<Vec<_>>();
        let vertex_size = u32::try_from(vertex_bytes.len())
            .map_err(ApplicationDiagnosticError::msg)
            .context("checking a voxel chunk vertex buffer size")?;
        let index_size = u32::try_from(index_bytes.len())
            .map_err(ApplicationDiagnosticError::msg)
            .context("checking a voxel chunk index buffer size")?;
        if vertex_size == 0 || index_size == 0 {
            continue;
        }
        let vertex = renderer
            .create_buffer(ApplicationBufferDesc::new(
                u64::from(vertex_size),
                ApplicationBufferUsage::COPY_DESTINATION.union(ApplicationBufferUsage::VERTEX),
            )?)
            .map_err(ApplicationDiagnosticError::msg)
            .context("allocating a voxel chunk vertex buffer")?;
        uploads.push((vertex, vertex_bytes));
        let index = renderer
            .create_buffer(ApplicationBufferDesc::new(
                u64::from(index_size),
                ApplicationBufferUsage::COPY_DESTINATION.union(ApplicationBufferUsage::INDEX),
            )?)
            .map_err(ApplicationDiagnosticError::msg)
            .context("allocating a voxel chunk index buffer")?;
        uploads.push((index, index_bytes));
        let index_count = u32::try_from(section.indices.len())
            .map_err(ApplicationDiagnosticError::msg)
            .context("checking a voxel chunk index count")?;
        sections.push(VoxelDraw {
            vertex,
            vertex_size,
            index,
            index_size,
            index_count,
            base_color_view,
            bounds,
        });
    }
    Ok((ChunkGpuMesh { sections }, uploads))
}

fn combine_batch_mesh(
    batch_position: ChunkPosition,
    chunks: &HashMap<ChunkPosition, ChunkMesh>,
) -> nord_app::ApplicationResult<ChunkMesh> {
    let Some(base_x) = batch_position.x.checked_mul(MESH_BATCH_CHUNKS) else {
        return Err(ApplicationDiagnosticError::msg("voxel mesh batch X coordinate overflowed"));
    };
    let Some(base_y) = batch_position.y.checked_mul(MESH_BATCH_CHUNKS) else {
        return Err(ApplicationDiagnosticError::msg("voxel mesh batch Y coordinate overflowed"));
    };
    let Some(base_z) = batch_position.z.checked_mul(MESH_BATCH_CHUNKS) else {
        return Err(ApplicationDiagnosticError::msg("voxel mesh batch Z coordinate overflowed"));
    };
    let mut sections = BTreeMap::<String, MeshSection>::new();
    for local_y in 0..MESH_BATCH_CHUNKS {
        for local_z in 0..MESH_BATCH_CHUNKS {
            for local_x in 0..MESH_BATCH_CHUNKS {
                let Some(x) = base_x.checked_add(local_x) else {
                    continue;
                };
                let Some(y) = base_y.checked_add(local_y) else {
                    continue;
                };
                let Some(z) = base_z.checked_add(local_z) else {
                    continue;
                };
                let Some(chunk) = chunks.get(&ChunkPosition::new(x, y, z)) else {
                    continue;
                };
                for source in &chunk.sections {
                    if source.indices.is_empty() {
                        continue;
                    }
                    let destination =
                        sections.entry(source.texture_path.clone()).or_insert_with(|| {
                            MeshSection {
                                texture_path: source.texture_path.clone(),
                                vertices: Vec::new(),
                                indices: Vec::new(),
                            }
                        });
                    let base = u32::try_from(destination.vertices.len())
                        .map_err(ApplicationDiagnosticError::msg)
                        .context("checking batched voxel vertex index range")?;
                    destination.vertices.extend_from_slice(&source.vertices);
                    destination.indices.reserve(source.indices.len());
                    for &index in &source.indices {
                        destination.indices.push(index.checked_add(base).ok_or_else(|| {
                            ApplicationDiagnosticError::msg(
                                "batched voxel index exceeded the 32-bit range",
                            )
                        })?);
                    }
                }
            }
        }
    }
    Ok(ChunkMesh { position: batch_position, sections: sections.into_values().collect() })
}

fn mesh_batch_position(position: ChunkPosition) -> ChunkPosition {
    ChunkPosition::new(
        position.x.div_euclid(MESH_BATCH_CHUNKS),
        position.y.div_euclid(MESH_BATCH_CHUNKS),
        position.z.div_euclid(MESH_BATCH_CHUNKS),
    )
}

fn append_chunk_resources(mesh: ChunkGpuMesh, resources: &mut Vec<ApplicationGpuResourceHandle>) {
    for section in mesh.sections {
        resources.push(section.vertex);
        resources.push(section.index);
    }
}

pub(super) fn declare_draw_resources(
    pass: nord_app::ApplicationRenderPassDesc,
    draws: &[VoxelDraw],
    textures: &BlockTextureMaterials,
) -> nord_app::ApplicationRenderPassDesc {
    let pass = draws.iter().fold(pass, |pass, draw| {
        pass.use_resource(draw.vertex, ApplicationResourceUsage::VertexBufferRead)
            .use_resource(draw.index, ApplicationResourceUsage::IndexBufferRead)
    });
    textures.resources().fold(pass, |pass, resource| {
        pass.use_resource(resource, ApplicationResourceUsage::SampledRead)
    })
}

fn batch_bounds(position: ChunkPosition) -> Option<ApplicationAabb3> {
    let size = (CHUNK_SIZE * MESH_BATCH_CHUNKS) as f32;
    let min = ApplicationVec3::new(
        position.x as f32 * size,
        position.y as f32 * size,
        position.z as f32 * size,
    );
    let max = ApplicationVec3::new(min.x + size, min.y + size, min.z + size);
    ApplicationAabb3::new(min, max).ok()
}

#[cfg(test)]
mod tests {
    use super::{combine_batch_mesh, mesh_batch_position};
    use crate::world::meshing::{ChunkMesh, MeshSection, VoxelVertex};
    use nord_app::ApplicationVec3;
    use nord_world::ChunkPosition;

    #[test]
    fn mesh_batches_use_euclidean_coordinates_for_negative_chunks() {
        assert_eq!(
            mesh_batch_position(ChunkPosition::new(-1, -4, -5)),
            ChunkPosition::new(-1, -1, -2)
        );
    }

    #[test]
    fn batch_mesh_combines_same_material_and_rebases_indices() {
        let vertex = VoxelVertex {
            position: ApplicationVec3::ZERO,
            normal: ApplicationVec3::Y,
            tangent: [1.0, 0.0, 0.0, 1.0],
            tex_coord: [0.0, 0.0],
        };
        let section = || MeshSection {
            texture_path: "stone.png".to_owned(),
            vertices: vec![vertex; 4],
            indices: vec![0, 1, 2, 0, 2, 3],
        };
        let chunks = [
            (
                ChunkPosition::new(0, 0, 0),
                ChunkMesh { position: ChunkPosition::new(0, 0, 0), sections: vec![section()] },
            ),
            (
                ChunkPosition::new(1, 0, 0),
                ChunkMesh { position: ChunkPosition::new(1, 0, 0), sections: vec![section()] },
            ),
        ]
        .into_iter()
        .collect();

        let combined = combine_batch_mesh(ChunkPosition::new(0, 0, 0), &chunks)
            .expect("chunk meshes combine into a render batch");
        assert_eq!(combined.sections.len(), 1);
        assert_eq!(combined.sections[0].vertices.len(), 8);
        assert_eq!(combined.sections[0].indices, [0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7]);
    }
}
