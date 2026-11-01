//! Game-oriented math, geometry, and spatial query infrastructure.
//!
//! Scalar/vector/matrix primitives are implemented by Nord. Matrices use
//! column vectors and column-major storage; angles are radians. Coordinate
//! handedness is selected explicitly by the caller and is not globally imposed
//! by this crate.

mod geometry;
mod matrix;
mod scalar;
mod spatial;
mod vector;

pub use geometry::{Aabb3, Frustum3, GeometryError, Plane3, Ray3, Sphere3};
pub use matrix::{Affine2, Affine3, Mat2, Mat3, Mat4, Quat};
pub use scalar::{
    approx_eq_f32, approx_eq_f64, inverse_lerp, lerp, remap, smoothstep, wrap_angle_pi,
};
pub use spatial::{Octree, OctreeConfig, OctreeError, OctreeItemId};
pub use vector::{
    DVec2, DVec3, DVec4, I64Vec2, I64Vec3, I64Vec4, IVec2, IVec3, IVec4, U64Vec2, U64Vec3, U64Vec4,
    UVec2, UVec3, UVec4, Vec2, Vec3, Vec4,
};
