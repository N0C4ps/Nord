//! Stable game-facing aliases for Nord math primitives.

pub use nord_math::{
    Aabb3 as ApplicationAabb3, Affine2 as ApplicationAffine2, Affine3 as ApplicationAffine3,
    DVec2 as ApplicationDVec2, DVec3 as ApplicationDVec3, DVec4 as ApplicationDVec4,
    Frustum3 as ApplicationFrustum3, GeometryError as ApplicationGeometryError,
    I64Vec2 as ApplicationI64Vec2, I64Vec3 as ApplicationI64Vec3, I64Vec4 as ApplicationI64Vec4,
    IVec2 as ApplicationIVec2, IVec3 as ApplicationIVec3, IVec4 as ApplicationIVec4,
    Mat2 as ApplicationMat2, Mat3 as ApplicationMat3, Mat4 as ApplicationMat4,
    OctreeConfig as ApplicationOctreeConfig, OctreeError as ApplicationOctreeError,
    OctreeItemId as ApplicationOctreeItemId, Plane3 as ApplicationPlane3, Quat as ApplicationQuat,
    Ray3 as ApplicationRay3, Sphere3 as ApplicationSphere3, U64Vec2 as ApplicationU64Vec2,
    U64Vec3 as ApplicationU64Vec3, U64Vec4 as ApplicationU64Vec4, UVec2 as ApplicationUVec2,
    UVec3 as ApplicationUVec3, UVec4 as ApplicationUVec4, Vec2 as ApplicationVec2,
    Vec3 as ApplicationVec3, Vec4 as ApplicationVec4, approx_eq_f32 as application_approx_eq_f32,
    approx_eq_f64 as application_approx_eq_f64, inverse_lerp as application_inverse_lerp,
    lerp as application_lerp, remap as application_remap, smoothstep as application_smoothstep,
    wrap_angle_pi as application_wrap_angle_pi,
};

/// Game-facing octree type alias.
pub type ApplicationOctree<T> = nord_math::Octree<T>;
