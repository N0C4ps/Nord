//! Nord-owned matrices, quaternions, and affine transforms.

use crate::{Vec2, Vec3, Vec4};
use std::ops::{Add, Mul, Neg, Sub};

/// A 2×2 column-major matrix operating on column vectors.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat2 {
    x_axis: Vec2,
    y_axis: Vec2,
}

impl Mat2 {
    /// Zero matrix.
    pub const ZERO: Self = Self::from_cols(Vec2::ZERO, Vec2::ZERO);
    /// Identity matrix.
    pub const IDENTITY: Self = Self::from_cols(Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0));

    /// Build from columns.
    #[must_use]
    pub const fn from_cols(x_axis: Vec2, y_axis: Vec2) -> Self {
        Self { x_axis, y_axis }
    }

    /// Build a 2D rotation matrix in radians.
    #[must_use]
    pub fn from_rotation(angle: f32) -> Self {
        let (sin, cos) = angle.sin_cos();
        Self::from_cols(Vec2::new(cos, sin), Vec2::new(-sin, cos))
    }

    /// Transform a vector.
    #[must_use]
    pub fn transform_vector(self, vector: Vec2) -> Vec2 {
        self.x_axis * vector.x + self.y_axis * vector.y
    }

    /// Matrix columns.
    #[must_use]
    pub const fn to_cols(self) -> [Vec2; 2] {
        [self.x_axis, self.y_axis]
    }

    /// Determinant.
    #[must_use]
    pub fn determinant(self) -> f32 {
        self.x_axis.x * self.y_axis.y - self.y_axis.x * self.x_axis.y
    }

    /// Inverse, or `None` for a singular/non-finite matrix.
    #[must_use]
    pub fn inverse(self) -> Option<Self> {
        let determinant = self.determinant();
        if !determinant.is_finite() || determinant.abs() <= f32::MIN_POSITIVE {
            return None;
        }
        let inverse = Self::from_cols(
            Vec2::new(self.y_axis.y, -self.x_axis.y) / determinant,
            Vec2::new(-self.y_axis.x, self.x_axis.x) / determinant,
        );
        matrix2_is_finite(inverse).then_some(inverse)
    }

    /// Transpose.
    #[must_use]
    pub const fn transpose(self) -> Self {
        Self::from_cols(
            Vec2::new(self.x_axis.x, self.y_axis.x),
            Vec2::new(self.x_axis.y, self.y_axis.y),
        )
    }
}

impl Mul<Vec2> for Mat2 {
    type Output = Vec2;
    fn mul(self, rhs: Vec2) -> Vec2 {
        self.transform_vector(rhs)
    }
}

impl Mul for Mat2 {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        Self::from_cols(self * rhs.x_axis, self * rhs.y_axis)
    }
}

/// A 3×3 column-major matrix operating on column vectors.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(clippy::struct_field_names)] // Axis names are conventional for matrix basis columns.
pub struct Mat3 {
    x_axis: Vec3,
    y_axis: Vec3,
    z_axis: Vec3,
}

impl Mat3 {
    /// Zero matrix.
    pub const ZERO: Self = Self::from_cols(Vec3::ZERO, Vec3::ZERO, Vec3::ZERO);
    /// Identity matrix.
    pub const IDENTITY: Self = Self::from_cols(Vec3::X, Vec3::Y, Vec3::Z);

    /// Build from columns.
    #[must_use]
    pub const fn from_cols(x_axis: Vec3, y_axis: Vec3, z_axis: Vec3) -> Self {
        Self { x_axis, y_axis, z_axis }
    }

    /// Build a diagonal scale matrix.
    #[must_use]
    pub const fn from_scale(scale: Vec3) -> Self {
        Self::from_cols(
            Vec3::new(scale.x, 0.0, 0.0),
            Vec3::new(0.0, scale.y, 0.0),
            Vec3::new(0.0, 0.0, scale.z),
        )
    }

    /// Build a homogeneous 2D translation matrix.
    #[must_use]
    pub const fn from_translation_2d(translation: Vec2) -> Self {
        Self::from_cols(
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(translation.x, translation.y, 1.0),
        )
    }

    /// Build a homogeneous 2D scale matrix.
    #[must_use]
    pub const fn from_scale_2d(scale: Vec2) -> Self {
        Self::from_cols(
            Vec3::new(scale.x, 0.0, 0.0),
            Vec3::new(0.0, scale.y, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
        )
    }

    /// Build a homogeneous 2D rotation matrix in radians.
    #[must_use]
    pub fn from_rotation_2d(angle: f32) -> Self {
        let (sin, cos) = angle.sin_cos();
        Self::from_cols(
            Vec3::new(cos, sin, 0.0),
            Vec3::new(-sin, cos, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
        )
    }

    /// Build a 3D rotation around the X axis in radians.
    #[must_use]
    pub fn from_rotation_x(angle: f32) -> Self {
        let (sin, cos) = angle.sin_cos();
        Self::from_cols(Vec3::X, Vec3::new(0.0, cos, sin), Vec3::new(0.0, -sin, cos))
    }

    /// Build a 3D rotation around the Y axis in radians.
    #[must_use]
    pub fn from_rotation_y(angle: f32) -> Self {
        let (sin, cos) = angle.sin_cos();
        Self::from_cols(Vec3::new(cos, 0.0, -sin), Vec3::Y, Vec3::new(sin, 0.0, cos))
    }

    /// Build a 3D rotation around the Z axis in radians.
    #[must_use]
    pub fn from_rotation_z(angle: f32) -> Self {
        let (sin, cos) = angle.sin_cos();
        Self::from_cols(Vec3::new(cos, sin, 0.0), Vec3::new(-sin, cos, 0.0), Vec3::Z)
    }

    /// Transform a vector.
    #[must_use]
    pub fn transform_vector(self, vector: Vec3) -> Vec3 {
        self.x_axis * vector.x + self.y_axis * vector.y + self.z_axis * vector.z
    }

    /// Matrix columns.
    #[must_use]
    pub const fn to_cols(self) -> [Vec3; 3] {
        [self.x_axis, self.y_axis, self.z_axis]
    }

    /// Determinant.
    #[must_use]
    pub fn determinant(self) -> f32 {
        self.x_axis.dot(self.y_axis.cross(self.z_axis))
    }

    /// Inverse, or `None` for a singular/non-finite matrix.
    #[must_use]
    pub fn inverse(self) -> Option<Self> {
        let determinant = self.determinant();
        if !determinant.is_finite() || determinant.abs() <= f32::MIN_POSITIVE {
            return None;
        }
        let inverse_rows = [
            self.y_axis.cross(self.z_axis) / determinant,
            self.z_axis.cross(self.x_axis) / determinant,
            self.x_axis.cross(self.y_axis) / determinant,
        ];
        let result = Self::from_cols(
            Vec3::new(inverse_rows[0].x, inverse_rows[1].x, inverse_rows[2].x),
            Vec3::new(inverse_rows[0].y, inverse_rows[1].y, inverse_rows[2].y),
            Vec3::new(inverse_rows[0].z, inverse_rows[1].z, inverse_rows[2].z),
        );
        matrix3_is_finite(result).then_some(result)
    }

    /// Transpose.
    #[must_use]
    pub const fn transpose(self) -> Self {
        Self::from_cols(
            Vec3::new(self.x_axis.x, self.y_axis.x, self.z_axis.x),
            Vec3::new(self.x_axis.y, self.y_axis.y, self.z_axis.y),
            Vec3::new(self.x_axis.z, self.y_axis.z, self.z_axis.z),
        )
    }
}

impl Mul<Vec3> for Mat3 {
    type Output = Vec3;
    fn mul(self, rhs: Vec3) -> Vec3 {
        self.transform_vector(rhs)
    }
}

impl Mul for Mat3 {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        Self::from_cols(self * rhs.x_axis, self * rhs.y_axis, self * rhs.z_axis)
    }
}

/// A 4×4 column-major matrix operating on homogeneous column vectors.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat4 {
    columns: [Vec4; 4],
}

impl Mat4 {
    /// Zero matrix.
    pub const ZERO: Self = Self::from_cols(Vec4::ZERO, Vec4::ZERO, Vec4::ZERO, Vec4::ZERO);
    /// Identity matrix.
    pub const IDENTITY: Self = Self::from_cols(
        Vec4::new(1.0, 0.0, 0.0, 0.0),
        Vec4::new(0.0, 1.0, 0.0, 0.0),
        Vec4::new(0.0, 0.0, 1.0, 0.0),
        Vec4::new(0.0, 0.0, 0.0, 1.0),
    );

    /// Build from columns.
    #[must_use]
    pub const fn from_cols(x: Vec4, y: Vec4, z: Vec4, w: Vec4) -> Self {
        Self { columns: [x, y, z, w] }
    }

    /// Build a translation matrix.
    #[must_use]
    pub const fn from_translation(translation: Vec3) -> Self {
        Self::from_cols(
            Vec4::new(1.0, 0.0, 0.0, 0.0),
            Vec4::new(0.0, 1.0, 0.0, 0.0),
            Vec4::new(0.0, 0.0, 1.0, 0.0),
            Vec4::new(translation.x, translation.y, translation.z, 1.0),
        )
    }

    /// Build a scale matrix.
    #[must_use]
    pub const fn from_scale(scale: Vec3) -> Self {
        Self::from_cols(
            Vec4::new(scale.x, 0.0, 0.0, 0.0),
            Vec4::new(0.0, scale.y, 0.0, 0.0),
            Vec4::new(0.0, 0.0, scale.z, 0.0),
            Vec4::new(0.0, 0.0, 0.0, 1.0),
        )
    }

    /// Build a 4×4 rotation matrix from a quaternion.
    #[must_use]
    pub fn from_rotation(rotation: Quat) -> Option<Self> {
        let [x, y, z] = rotation.to_mat3()?.to_cols();
        Some(Self::from_cols(x.extend(0.0), y.extend(0.0), z.extend(0.0), Vec4::W))
    }

    /// Build a transform in scale, then rotation, then translation order.
    #[must_use]
    pub fn from_translation_rotation_scale(
        translation: Vec3,
        rotation: Quat,
        scale: Vec3,
    ) -> Option<Self> {
        let rotation = Self::from_rotation(rotation)?;
        Some(Self::from_translation(translation) * rotation * Self::from_scale(scale))
    }

    /// Build a right-handed view-projection perspective matrix with depth in `[0, 1]`.
    ///
    /// # Errors
    ///
    /// Returns `None` for non-finite values, invalid clip planes, or an invalid
    /// field-of-view/aspect ratio.
    #[must_use]
    pub fn perspective_rh_zo(fov_y: f32, aspect: f32, near: f32, far: f32) -> Option<Self> {
        if !fov_y.is_finite()
            || !aspect.is_finite()
            || !near.is_finite()
            || !far.is_finite()
            || fov_y <= 0.0
            || fov_y >= std::f32::consts::PI
            || aspect <= 0.0
            || near <= 0.0
            || far <= near
        {
            return None;
        }
        let focal = 1.0 / (fov_y * 0.5).tan();
        let depth = far / (near - far);
        let offset = near * far / (near - far);
        let result = Self::from_cols(
            Vec4::new(focal / aspect, 0.0, 0.0, 0.0),
            Vec4::new(0.0, focal, 0.0, 0.0),
            Vec4::new(0.0, 0.0, depth, -1.0),
            Vec4::new(0.0, 0.0, offset, 0.0),
        );
        matrix4_is_finite(result).then_some(result)
    }

    /// Build a right-handed view matrix looking in `direction` with an up hint.
    #[must_use]
    pub fn look_to_rh(eye: Vec3, direction: Vec3, up: Vec3) -> Option<Self> {
        if !eye.is_finite() || !up.is_finite() {
            return None;
        }
        let forward = direction.try_normalize()?;
        let right = forward.cross(up).try_normalize()?;
        let corrected_up = right.cross(forward);
        let back = -forward;
        Some(Self::from_cols(
            Vec4::new(right.x, corrected_up.x, back.x, 0.0),
            Vec4::new(right.y, corrected_up.y, back.y, 0.0),
            Vec4::new(right.z, corrected_up.z, back.z, 0.0),
            Vec4::new(-right.dot(eye), -corrected_up.dot(eye), -back.dot(eye), 1.0),
        ))
    }

    /// Transform a homogeneous vector.
    #[must_use]
    pub fn transform_vector4(self, vector: Vec4) -> Vec4 {
        self.columns[0] * vector.x
            + self.columns[1] * vector.y
            + self.columns[2] * vector.z
            + self.columns[3] * vector.w
    }

    /// Transform a point with perspective division; `None` when the result is invalid.
    #[must_use]
    pub fn transform_point3(self, point: Vec3) -> Option<Vec3> {
        let transformed = self * point.extend(1.0);
        if !transformed.is_finite() || transformed.w.abs() <= f32::MIN_POSITIVE {
            return None;
        }
        let result = transformed.truncate() / transformed.w;
        result.is_finite().then_some(result)
    }

    /// Transform a direction without applying translation.
    #[must_use]
    pub fn transform_vector3(self, vector: Vec3) -> Vec3 {
        (self * vector.extend(0.0)).truncate()
    }

    /// Matrix columns.
    #[must_use]
    pub const fn to_cols(self) -> [Vec4; 4] {
        self.columns
    }

    /// Determinant computed by partial-pivot elimination.
    #[must_use]
    #[allow(clippy::indexing_slicing)] // Fixed 4×4 matrix with bounded pivot indices.
    #[allow(clippy::needless_range_loop)] // Pivot elimination requires stable row/column indices.
    #[allow(clippy::float_cmp)] // Exact zero pivot means the matrix is singular.
    #[allow(clippy::cast_possible_truncation)] // A determinant outside f32 range is reported as infinity.
    pub fn determinant(self) -> f32 {
        let mut rows = [[0.0_f64; 4]; 4];
        for row in 0..4 {
            for column in 0..4 {
                rows[row][column] = f64::from(self.columns[column].to_array()[row]);
            }
        }
        let mut determinant = 1.0_f64;
        let mut sign = 1.0_f64;
        for pivot_column in 0..4 {
            let mut pivot_row = pivot_column;
            for row in pivot_column + 1..4 {
                if rows[row][pivot_column].abs() > rows[pivot_row][pivot_column].abs() {
                    pivot_row = row;
                }
            }
            let pivot = rows[pivot_row][pivot_column];
            if pivot == 0.0 {
                return 0.0;
            }
            if pivot_row != pivot_column {
                rows.swap(pivot_row, pivot_column);
                sign = -sign;
            }
            determinant *= rows[pivot_column][pivot_column];
            for row in pivot_column + 1..4 {
                let factor = rows[row][pivot_column] / rows[pivot_column][pivot_column];
                for column in pivot_column + 1..4 {
                    rows[row][column] -= factor * rows[pivot_column][column];
                }
            }
        }
        (determinant * sign) as f32
    }

    /// Transpose.
    #[must_use]
    pub const fn transpose(self) -> Self {
        Self::from_cols(
            Vec4::new(self.columns[0].x, self.columns[1].x, self.columns[2].x, self.columns[3].x),
            Vec4::new(self.columns[0].y, self.columns[1].y, self.columns[2].y, self.columns[3].y),
            Vec4::new(self.columns[0].z, self.columns[1].z, self.columns[2].z, self.columns[3].z),
            Vec4::new(self.columns[0].w, self.columns[1].w, self.columns[2].w, self.columns[3].w),
        )
    }

    /// Inverse computed with partial-pivot Gauss-Jordan elimination.
    #[must_use]
    #[allow(clippy::indexing_slicing)] // Fixed 4×8 augmented matrix with bounded indices.
    #[allow(clippy::needless_range_loop)] // Pivot elimination requires stable row/column indices.
    #[allow(clippy::cast_possible_truncation)] // The public matrix is f32; overflow is checked after conversion.
    pub fn inverse(self) -> Option<Self> {
        let mut augmented = [[0.0_f64; 8]; 4];
        for row in 0..4 {
            for column in 0..4 {
                augmented[row][column] = f64::from(self.columns[column].to_array()[row]);
            }
            augmented[row][row + 4] = 1.0;
        }

        for pivot_column in 0..4 {
            let mut pivot_row = pivot_column;
            for row in pivot_column + 1..4 {
                if augmented[row][pivot_column].abs() > augmented[pivot_row][pivot_column].abs() {
                    pivot_row = row;
                }
            }
            let pivot = augmented[pivot_row][pivot_column];
            if !pivot.is_finite() || pivot.abs() <= f64::MIN_POSITIVE {
                return None;
            }
            augmented.swap(pivot_column, pivot_row);
            for column in 0..8 {
                augmented[pivot_column][column] /= pivot;
            }
            for row in 0..4 {
                if row == pivot_column {
                    continue;
                }
                let factor = augmented[row][pivot_column];
                for column in 0..8 {
                    augmented[row][column] -= factor * augmented[pivot_column][column];
                }
            }
        }

        let mut columns = [Vec4::ZERO; 4];
        for row in 0..4 {
            for column in 0..4 {
                let value = augmented[row][column + 4] as f32;
                if !value.is_finite() {
                    return None;
                }
                columns[column] = set_component(columns[column], row, value);
            }
        }
        Some(Self { columns })
    }
}

impl Mul<Vec4> for Mat4 {
    type Output = Vec4;
    fn mul(self, rhs: Vec4) -> Vec4 {
        self.transform_vector4(rhs)
    }
}

impl Mul for Mat4 {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        Self::from_cols(
            self * rhs.columns[0],
            self * rhs.columns[1],
            self * rhs.columns[2],
            self * rhs.columns[3],
        )
    }
}

/// Unit quaternion for 3D orientation.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quat {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Quat {
    /// Identity rotation.
    pub const IDENTITY: Self = Self::new(0.0, 0.0, 0.0, 1.0);

    /// Construct raw components. Call [`Quat::try_normalize`] for unchecked values.
    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }

    /// Create a unit quaternion from a normalized axis and angle in radians.
    #[must_use]
    pub fn from_axis_angle(axis: Vec3, angle: f32) -> Option<Self> {
        if !angle.is_finite() {
            return None;
        }
        let axis = axis.try_normalize()?;
        let (sin, cos) = (angle * 0.5).sin_cos();
        Some(Self::new(axis.x * sin, axis.y * sin, axis.z * sin, cos))
    }

    /// Squared magnitude.
    #[must_use]
    pub fn length_squared(self) -> f32 {
        self.x * self.x + self.y * self.y + self.z * self.z + self.w * self.w
    }

    /// Normalize, returning `None` for zero or non-finite input.
    #[must_use]
    pub fn try_normalize(self) -> Option<Self> {
        let length = self.length_squared().sqrt();
        if !length.is_finite() || length <= f32::MIN_POSITIVE {
            return None;
        }
        let result = Self::new(self.x / length, self.y / length, self.z / length, self.w / length);
        Some(result)
    }

    /// Conjugate; for unit quaternions this is also the inverse rotation.
    #[must_use]
    pub const fn conjugate(self) -> Self {
        Self::new(-self.x, -self.y, -self.z, self.w)
    }

    /// Rotate a vector after validating and normalizing this quaternion.
    #[must_use]
    pub fn rotate_vec3(self, vector: Vec3) -> Option<Vec3> {
        let rotation = self.try_normalize()?;
        let imaginary = Vec3::new(rotation.x, rotation.y, rotation.z);
        let twice_cross = imaginary.cross(vector) * 2.0;
        Some(vector + twice_cross * rotation.w + imaginary.cross(twice_cross))
    }

    /// Spherical interpolation between unit quaternions.
    #[must_use]
    pub fn slerp(self, mut other: Self, factor: f32) -> Option<Self> {
        if !factor.is_finite() {
            return None;
        }
        let left = self.try_normalize()?;
        other = other.try_normalize()?;
        let mut cosine = quat_dot(left, other);
        if cosine < 0.0 {
            other = -other;
            cosine = -cosine;
        }
        if cosine > 0.9995 {
            return (left + (other - left) * factor).try_normalize();
        }
        let angle = cosine.clamp(-1.0, 1.0).acos();
        let sine = angle.sin();
        if sine.abs() <= f32::MIN_POSITIVE {
            return Some(left);
        }
        let left_weight = ((1.0 - factor) * angle).sin() / sine;
        let right_weight = (factor * angle).sin() / sine;
        (left * left_weight + other * right_weight).try_normalize()
    }

    /// Convert this quaternion to a 3×3 rotation matrix after normalization.
    #[must_use]
    pub fn to_mat3(self) -> Option<Mat3> {
        let q = self.try_normalize()?;
        let (x2, y2, z2) = (q.x + q.x, q.y + q.y, q.z + q.z);
        let (xx, xy, xz) = (q.x * x2, q.x * y2, q.x * z2);
        let (yy, yz, zz) = (q.y * y2, q.y * z2, q.z * z2);
        let (wx, wy, wz) = (q.w * x2, q.w * y2, q.w * z2);
        Some(Mat3::from_cols(
            Vec3::new(1.0 - (yy + zz), xy + wz, xz - wy),
            Vec3::new(xy - wz, 1.0 - (xx + zz), yz + wx),
            Vec3::new(xz + wy, yz - wx, 1.0 - (xx + yy)),
        ))
    }
}

impl Add for Quat {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z, self.w + rhs.w)
    }
}
impl Sub for Quat {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z, self.w - rhs.w)
    }
}
impl Mul<f32> for Quat {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        Self::new(self.x * rhs, self.y * rhs, self.z * rhs, self.w * rhs)
    }
}
impl Neg for Quat {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y, -self.z, -self.w)
    }
}
impl Mul for Quat {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        Self::new(
            self.w * rhs.x + self.x * rhs.w + self.y * rhs.z - self.z * rhs.y,
            self.w * rhs.y - self.x * rhs.z + self.y * rhs.w + self.z * rhs.x,
            self.w * rhs.z + self.x * rhs.y - self.y * rhs.x + self.z * rhs.w,
            self.w * rhs.w - self.x * rhs.x - self.y * rhs.y - self.z * rhs.z,
        )
    }
}

/// 2D affine transform represented by a homogeneous 3×3 matrix.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine2(Mat3);

impl Affine2 {
    /// Identity transform.
    pub const IDENTITY: Self = Self(Mat3::IDENTITY);
    /// Build from translation, rotation in radians, and non-uniform scale.
    #[must_use]
    pub fn from_translation_rotation_scale(translation: Vec2, rotation: f32, scale: Vec2) -> Self {
        Self(
            Mat3::from_translation_2d(translation)
                * Mat3::from_rotation_2d(rotation)
                * Mat3::from_scale_2d(scale),
        )
    }
    /// Construct from a matrix.
    #[must_use]
    pub const fn from_matrix(matrix: Mat3) -> Self {
        Self(matrix)
    }
    /// Matrix representation.
    #[must_use]
    pub const fn matrix(self) -> Mat3 {
        self.0
    }
    /// Transform a point in homogeneous 2D coordinates.
    #[must_use]
    pub fn transform_point2(self, point: Vec2) -> Option<Vec2> {
        let value = self.0 * Vec3::new(point.x, point.y, 1.0);
        if !value.is_finite() || value.z.abs() <= f32::MIN_POSITIVE {
            return None;
        }
        let point = Vec2::new(value.x / value.z, value.y / value.z);
        point.is_finite().then_some(point)
    }
}

/// 3D affine transform represented by a homogeneous 4×4 matrix.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine3(Mat4);

impl Affine3 {
    /// Identity transform.
    pub const IDENTITY: Self = Self(Mat4::IDENTITY);
    /// Build from translation, rotation, and non-uniform scale.
    #[must_use]
    pub fn from_translation_rotation_scale(
        translation: Vec3,
        rotation: Quat,
        scale: Vec3,
    ) -> Option<Self> {
        Some(Self(Mat4::from_translation_rotation_scale(translation, rotation, scale)?))
    }
    /// Construct from a matrix.
    #[must_use]
    pub const fn from_matrix(matrix: Mat4) -> Self {
        Self(matrix)
    }
    /// Matrix representation.
    #[must_use]
    pub const fn matrix(self) -> Mat4 {
        self.0
    }
    /// Transform a point.
    #[must_use]
    pub fn transform_point3(self, point: Vec3) -> Option<Vec3> {
        self.0.transform_point3(point)
    }
    /// Transform a direction.
    #[must_use]
    pub fn transform_vector3(self, vector: Vec3) -> Vec3 {
        self.0.transform_vector3(vector)
    }
}

fn quat_dot(left: Quat, right: Quat) -> f32 {
    left.x * right.x + left.y * right.y + left.z * right.z + left.w * right.w
}

fn matrix2_is_finite(value: Mat2) -> bool {
    value.to_cols().iter().all(|column| column.is_finite())
}
fn matrix3_is_finite(value: Mat3) -> bool {
    value.to_cols().iter().all(|column| column.is_finite())
}
fn matrix4_is_finite(value: Mat4) -> bool {
    value.to_cols().iter().all(|column| column.is_finite())
}

#[allow(clippy::indexing_slicing)] // Components are selected from a fixed four-element vector.
fn set_component(mut vector: Vec4, index: usize, value: f32) -> Vec4 {
    match index {
        0 => vector.x = value,
        1 => vector.y = value,
        2 => vector.z = value,
        3 => vector.w = value,
        _ => return vector,
    }
    vector
}
