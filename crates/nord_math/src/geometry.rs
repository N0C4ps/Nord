//! Validated geometric primitives and common intersection operations.

use crate::Vec3;
use std::error::Error;
use std::fmt::{Display, Formatter};

/// A geometric input failed validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum GeometryError {
    /// A point, direction, extent, or radius contained a non-finite value.
    NonFinite,
    /// A direction or normal had effectively zero length.
    DegenerateDirection,
    /// A radius or half extent was negative.
    NegativeExtent,
    /// A minimum bound exceeded its corresponding maximum bound.
    InvertedBounds,
}

impl Display for GeometryError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFinite => formatter.write_str("geometry contains a non-finite value"),
            Self::DegenerateDirection => formatter.write_str("direction or normal has zero length"),
            Self::NegativeExtent => {
                formatter.write_str("radius and half extents must be non-negative")
            }
            Self::InvertedBounds => {
                formatter.write_str("minimum bounds must not exceed maximum bounds")
            }
        }
    }
}

impl Error for GeometryError {}

/// Axis-aligned 3D bounds with inclusive faces.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb3 {
    min: Vec3,
    max: Vec3,
}

impl Aabb3 {
    /// Construct bounds after validating finite, ordered coordinates.
    ///
    /// # Errors
    ///
    /// Returns [`GeometryError::NonFinite`] for invalid coordinates and
    /// [`GeometryError::InvertedBounds`] when any minimum exceeds its maximum.
    pub fn new(min: Vec3, max: Vec3) -> Result<Self, GeometryError> {
        if !is_finite3(min) || !is_finite3(max) {
            return Err(GeometryError::NonFinite);
        }
        if min.x > max.x || min.y > max.y || min.z > max.z {
            return Err(GeometryError::InvertedBounds);
        }
        Ok(Self { min, max })
    }

    /// Construct bounds around a center using non-negative half extents.
    ///
    /// # Errors
    ///
    /// Returns [`GeometryError::NonFinite`] for non-finite values and
    /// [`GeometryError::NegativeExtent`] for a negative half extent.
    pub fn from_center_half_extents(
        center: Vec3,
        half_extents: Vec3,
    ) -> Result<Self, GeometryError> {
        if !is_finite3(center) || !is_finite3(half_extents) {
            return Err(GeometryError::NonFinite);
        }
        if half_extents.min_element() < 0.0 {
            return Err(GeometryError::NegativeExtent);
        }
        Self::new(center - half_extents, center + half_extents)
    }

    /// Construct bounds from an origin and non-negative size.
    ///
    /// # Errors
    ///
    /// Returns [`GeometryError::NonFinite`] for non-finite values and
    /// [`GeometryError::NegativeExtent`] for a negative size component.
    pub fn from_min_size(min: Vec3, size: Vec3) -> Result<Self, GeometryError> {
        if !is_finite3(min) || !is_finite3(size) {
            return Err(GeometryError::NonFinite);
        }
        if size.min_element() < 0.0 {
            return Err(GeometryError::NegativeExtent);
        }
        Self::new(min, min + size)
    }

    /// Lower corner.
    #[must_use]
    pub const fn min(self) -> Vec3 {
        self.min
    }

    /// Upper corner.
    #[must_use]
    pub const fn max(self) -> Vec3 {
        self.max
    }

    /// Center point.
    #[must_use]
    pub fn center(self) -> Vec3 {
        self.min * 0.5 + self.max * 0.5
    }

    /// Half-size on each axis.
    #[must_use]
    pub fn half_extents(self) -> Vec3 {
        (self.max - self.min) * 0.5
    }

    /// Full size on each axis.
    #[must_use]
    pub fn size(self) -> Vec3 {
        self.max - self.min
    }

    /// Surface area, useful for broad-phase heuristics.
    #[must_use]
    pub fn surface_area(self) -> f32 {
        let size = self.size();
        2.0 * (size.x * size.y + size.y * size.z + size.z * size.x)
    }

    /// Whether a point is inside or on the bounds.
    #[must_use]
    pub fn contains_point(self, point: Vec3) -> bool {
        point.greater_equal(self.min) && point.less_equal(self.max)
    }

    /// Whether the other bounds are fully inside or on these bounds.
    #[must_use]
    pub fn contains_aabb(self, other: Self) -> bool {
        self.contains_point(other.min) && self.contains_point(other.max)
    }

    /// Whether the bounds overlap, including touching faces and edges.
    #[must_use]
    pub fn intersects(self, other: Self) -> bool {
        self.min.less_equal(other.max) && self.max.greater_equal(other.min)
    }

    /// Closest point in these bounds to `point`.
    #[must_use]
    pub fn closest_point(self, point: Vec3) -> Vec3 {
        point.clamp(self.min, self.max)
    }

    /// Smallest AABB containing both inputs.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        Self { min: self.min.min(other.min), max: self.max.max(other.max) }
    }

    /// Expand uniformly in every direction. Returns `None` for invalid inputs.
    #[must_use]
    pub fn expanded(self, amount: f32) -> Option<Self> {
        if !amount.is_finite() || amount < 0.0 {
            return None;
        }
        Some(Self { min: self.min - Vec3::splat(amount), max: self.max + Vec3::splat(amount) })
    }
}

/// A sphere used for broad-phase and exact primitive queries.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sphere3 {
    center: Vec3,
    radius: f32,
}

impl Sphere3 {
    /// Construct a sphere with finite center and non-negative radius.
    ///
    /// # Errors
    ///
    /// Returns [`GeometryError::NonFinite`] for non-finite values and
    /// [`GeometryError::NegativeExtent`] for a negative radius.
    pub fn new(center: Vec3, radius: f32) -> Result<Self, GeometryError> {
        if !is_finite3(center) || !radius.is_finite() {
            return Err(GeometryError::NonFinite);
        }
        if radius < 0.0 {
            return Err(GeometryError::NegativeExtent);
        }
        Ok(Self { center, radius })
    }

    /// Center point.
    #[must_use]
    pub const fn center(self) -> Vec3 {
        self.center
    }

    /// Radius.
    #[must_use]
    pub const fn radius(self) -> f32 {
        self.radius
    }

    /// Whether a point is inside or on the sphere.
    #[must_use]
    pub fn contains_point(self, point: Vec3) -> bool {
        point.is_finite() && point.distance_squared(self.center) <= self.radius * self.radius
    }

    /// Whether this sphere intersects an AABB.
    #[must_use]
    pub fn intersects_aabb(self, bounds: Aabb3) -> bool {
        bounds.closest_point(self.center).distance_squared(self.center) <= self.radius * self.radius
    }
}

/// A normalized plane satisfying `normal.dot(point) + distance = 0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plane3 {
    normal: Vec3,
    distance: f32,
}

impl Plane3 {
    /// Construct and normalize a plane equation.
    ///
    /// # Errors
    ///
    /// Returns [`GeometryError::NonFinite`] for non-finite values and
    /// [`GeometryError::DegenerateDirection`] for a zero-length normal.
    pub fn new(normal: Vec3, distance: f32) -> Result<Self, GeometryError> {
        if !is_finite3(normal) || !distance.is_finite() {
            return Err(GeometryError::NonFinite);
        }
        let scale = normal.abs().max_element();
        if scale <= 0.0 {
            return Err(GeometryError::DegenerateDirection);
        }
        let scaled_normal = normal / scale;
        let scaled_length = scaled_normal.length();
        let normalized_distance = (distance / scale) / scaled_length;
        if !normalized_distance.is_finite() {
            return Err(GeometryError::NonFinite);
        }
        Ok(Self { normal: scaled_normal / scaled_length, distance: normalized_distance })
    }

    /// Construct a plane from a point and normal.
    ///
    /// # Errors
    ///
    /// Returns [`GeometryError::NonFinite`] for a non-finite point or normal and
    /// [`GeometryError::DegenerateDirection`] for a zero-length normal.
    pub fn from_point_normal(point: Vec3, normal: Vec3) -> Result<Self, GeometryError> {
        if !is_finite3(point) {
            return Err(GeometryError::NonFinite);
        }
        let plane = Self::new(normal, 0.0)?;
        Ok(Self { distance: -plane.normal.dot(point), ..plane })
    }

    /// Unit normal pointing toward the plane's positive half-space.
    #[must_use]
    pub const fn normal(self) -> Vec3 {
        self.normal
    }

    /// Normalized signed offset.
    #[must_use]
    pub const fn distance(self) -> f32 {
        self.distance
    }

    /// Signed distance from a point to the plane.
    #[must_use]
    pub fn signed_distance(self, point: Vec3) -> f32 {
        self.normal.dot(point) + self.distance
    }
}

/// A ray whose direction is normalized, so hit distances are world units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ray3 {
    origin: Vec3,
    direction: Vec3,
}

impl Ray3 {
    /// Construct a ray and normalize its direction.
    ///
    /// # Errors
    ///
    /// Returns [`GeometryError::NonFinite`] for a non-finite origin or direction
    /// and [`GeometryError::DegenerateDirection`] for a zero-length direction.
    pub fn new(origin: Vec3, direction: Vec3) -> Result<Self, GeometryError> {
        if !is_finite3(origin) || !is_finite3(direction) {
            return Err(GeometryError::NonFinite);
        }
        let Some(direction) = direction.try_normalize() else {
            return Err(GeometryError::DegenerateDirection);
        };
        Ok(Self { origin, direction })
    }

    /// Origin point.
    #[must_use]
    pub const fn origin(self) -> Vec3 {
        self.origin
    }

    /// Unit direction vector.
    #[must_use]
    pub const fn direction(self) -> Vec3 {
        self.direction
    }

    /// Point at world-unit distance `distance` along the ray.
    #[must_use]
    pub fn at(self, distance: f32) -> Vec3 {
        self.origin + self.direction * distance
    }

    /// First non-negative intersection distance with an AABB.
    #[must_use]
    pub fn intersect_aabb(self, bounds: Aabb3) -> Option<f32> {
        let mut near = 0.0_f32;
        let mut far = f32::INFINITY;
        let origin = self.origin.to_array();
        let direction = self.direction.to_array();
        let minimum = bounds.min.to_array();
        let maximum = bounds.max.to_array();
        for ((origin, direction), (minimum, maximum)) in
            origin.into_iter().zip(direction).zip(minimum.into_iter().zip(maximum))
        {
            if direction.abs() <= f32::MIN_POSITIVE {
                if origin < minimum || origin > maximum {
                    return None;
                }
                continue;
            }
            let inverse = direction.recip();
            let mut first = (minimum - origin) * inverse;
            let mut second = (maximum - origin) * inverse;
            if first > second {
                std::mem::swap(&mut first, &mut second);
            }
            near = near.max(first);
            far = far.min(second);
            if near > far {
                return None;
            }
        }
        far.is_finite().then_some(near)
    }

    /// First non-negative intersection distance with a sphere.
    #[must_use]
    pub fn intersect_sphere(self, sphere: Sphere3) -> Option<f32> {
        let offset = self.origin - sphere.center;
        let b = offset.dot(self.direction);
        let c = offset.length_squared() - sphere.radius * sphere.radius;
        let discriminant = b * b - c;
        if !discriminant.is_finite() || discriminant < 0.0 {
            return None;
        }
        let root = discriminant.sqrt();
        let near = -b - root;
        let far = -b + root;
        if far < 0.0 || !far.is_finite() {
            None
        } else {
            let distance = near.max(0.0);
            distance.is_finite().then_some(distance)
        }
    }

    /// Non-negative intersection distance with a plane, excluding parallel rays.
    #[must_use]
    pub fn intersect_plane(self, plane: Plane3) -> Option<f32> {
        let denominator = plane.normal.dot(self.direction);
        if denominator.abs() <= f32::EPSILON {
            return None;
        }
        let distance = -plane.signed_distance(self.origin) / denominator;
        (distance >= 0.0 && distance.is_finite()).then_some(distance)
    }
}

/// Six-plane view frustum, with planes facing the visible half-space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frustum3 {
    planes: [Plane3; 6],
}

impl Frustum3 {
    /// Create a frustum from left, right, bottom, top, near, and far planes.
    #[must_use]
    pub const fn from_planes(planes: [Plane3; 6]) -> Self {
        Self { planes }
    }

    /// The six normalized frustum planes.
    #[must_use]
    pub const fn planes(&self) -> &[Plane3; 6] {
        &self.planes
    }

    /// Conservative AABB intersection using each plane's positive vertex.
    #[must_use]
    pub fn intersects_aabb(self, bounds: Aabb3) -> bool {
        self.planes.iter().all(|plane| {
            let normal = plane.normal;
            let positive = Vec3::new(
                if normal.x >= 0.0 { bounds.max.x } else { bounds.min.x },
                if normal.y >= 0.0 { bounds.max.y } else { bounds.min.y },
                if normal.z >= 0.0 { bounds.max.z } else { bounds.min.z },
            );
            plane.signed_distance(positive) >= 0.0
        })
    }
}

fn is_finite3(value: Vec3) -> bool {
    value.is_finite()
}
