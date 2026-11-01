//! Backend-independent indexed triangle mesh data and validation.
//!
//! Meshes own CPU-side vertices, indices and bounds only. GPU buffers,
//! materials and draw scheduling belong to renderer implementations.

use nord_math::{Aabb3, GeometryError, Vec3};
use std::error::Error;
use std::fmt::{Display, Formatter};

/// Basic vertex attributes shared by the initial static-mesh pipeline.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshVertex {
    position: Vec3,
    normal: Vec3,
}

impl MeshVertex {
    /// Create a vertex with a finite position and a normalized surface normal.
    ///
    /// # Errors
    /// Returns [`MeshError`] for non-finite data or a zero-length normal.
    pub fn new(position: Vec3, normal: Vec3) -> Result<Self, MeshError> {
        if !position.is_finite() || !normal.is_finite() {
            return Err(MeshError::NonFiniteVertex);
        }
        let normal = normal.try_normalize().ok_or(MeshError::DegenerateNormal)?;
        Ok(Self { position, normal })
    }

    /// Local-space position.
    #[must_use]
    pub const fn position(self) -> Vec3 {
        self.position
    }

    /// Unit local-space normal.
    #[must_use]
    pub const fn normal(self) -> Vec3 {
        self.normal
    }
}

/// Validated CPU-side indexed triangle mesh.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexedMesh {
    vertices: Vec<MeshVertex>,
    indices: Vec<u32>,
    bounds: Aabb3,
}

impl IndexedMesh {
    /// Construct a triangle-list mesh and calculate its local-space bounds.
    ///
    /// # Errors
    /// Returns an error when vertices/indices are empty, index count is not a
    /// triangle list, an index is out of range, or bounds cannot be formed.
    pub fn new(vertices: Vec<MeshVertex>, indices: Vec<u32>) -> Result<Self, MeshError> {
        if vertices.is_empty() {
            return Err(MeshError::EmptyVertices);
        }
        if indices.is_empty() {
            return Err(MeshError::EmptyIndices);
        }
        if !indices.len().is_multiple_of(3) {
            return Err(MeshError::IndexCountNotTriangleList);
        }
        let vertex_count = u32::try_from(vertices.len()).map_err(|_| MeshError::TooManyVertices)?;
        if indices.iter().any(|&index| index >= vertex_count) {
            return Err(MeshError::IndexOutOfRange);
        }
        let mut minimum = Vec3::splat(f32::INFINITY);
        let mut maximum = Vec3::splat(f32::NEG_INFINITY);
        for vertex in &vertices {
            minimum = minimum.min(vertex.position);
            maximum = maximum.max(vertex.position);
        }
        let bounds = Aabb3::new(minimum, maximum).map_err(MeshError::InvalidBounds)?;
        Ok(Self { vertices, indices, bounds })
    }

    /// Vertices in the mesh's local coordinate space.
    #[must_use]
    pub fn vertices(&self) -> &[MeshVertex] {
        &self.vertices
    }

    /// Triangle-list indices into [`Self::vertices`].
    #[must_use]
    pub fn indices(&self) -> &[u32] {
        &self.indices
    }

    /// Axis-aligned local-space bounds derived from all vertices.
    #[must_use]
    pub const fn bounds(&self) -> Aabb3 {
        self.bounds
    }
}

/// Invalid mesh data rejected before GPU upload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MeshError {
    EmptyVertices,
    EmptyIndices,
    IndexCountNotTriangleList,
    IndexOutOfRange,
    TooManyVertices,
    NonFiniteVertex,
    DegenerateNormal,
    InvalidBounds(GeometryError),
}

impl Display for MeshError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid indexed mesh: {self:?}")
    }
}

impl Error for MeshError {}

#[cfg(test)]
mod tests {
    use super::{IndexedMesh, MeshError, MeshVertex};
    use nord_math::Vec3;

    #[test]
    fn validates_indexed_triangle_mesh_and_derives_bounds() {
        let vertices = vec![
            MeshVertex::new(Vec3::new(-1.0, -2.0, -3.0), Vec3::Y).unwrap(),
            MeshVertex::new(Vec3::new(1.0, 2.0, 3.0), Vec3::Y).unwrap(),
            MeshVertex::new(Vec3::ZERO, Vec3::Y).unwrap(),
        ];
        let mesh = IndexedMesh::new(vertices, vec![0, 1, 2]).unwrap();
        assert_eq!(mesh.indices(), &[0, 1, 2]);
        assert_eq!(mesh.bounds().min(), Vec3::new(-1.0, -2.0, -3.0));
        assert_eq!(mesh.bounds().max(), Vec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn rejects_malformed_triangle_indices_and_normals() {
        let vertex = MeshVertex::new(Vec3::ZERO, Vec3::Y).unwrap();
        assert_eq!(
            IndexedMesh::new(vec![vertex; 3], vec![0, 1]),
            Err(MeshError::IndexCountNotTriangleList)
        );
        assert_eq!(
            IndexedMesh::new(vec![vertex; 3], vec![0, 1, 3]),
            Err(MeshError::IndexOutOfRange)
        );
        assert_eq!(MeshVertex::new(Vec3::ZERO, Vec3::ZERO), Err(MeshError::DegenerateNormal));
    }
}
