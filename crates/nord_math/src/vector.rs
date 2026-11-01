//! Nord-owned fixed-size vector types and arithmetic.

use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

macro_rules! define_float_vector {
    ($name:ident, $scalar:ty, $($field:ident),+ $(,)?) => {
        #[repr(C)]
        #[derive(Debug, Default, Clone, Copy, PartialEq)]
        pub struct $name { $(pub $field: $scalar),+ }

        impl $name {
            pub const ZERO: Self = Self { $($field: 0.0),+ };
            pub const ONE: Self = Self { $($field: 1.0),+ };

            #[must_use]
            pub const fn new($($field: $scalar),+) -> Self {
                Self { $($field),+ }
            }

            #[must_use]
            pub const fn splat(value: $scalar) -> Self {
                Self { $($field: value),+ }
            }

            #[must_use]
            pub fn dot(self, other: Self) -> $scalar {
                0.0 $(+ self.$field * other.$field)+
            }

            #[must_use]
            pub fn length_squared(self) -> $scalar { self.dot(self) }

            #[must_use]
            pub fn length(self) -> $scalar { self.length_squared().sqrt() }

            #[must_use]
            pub fn try_normalize(self) -> Option<Self> {
                let scale = self.abs().max_element();
                if !scale.is_finite() || scale <= 0.0 {
                    return None;
                }
                let scaled = self / scale;
                let length = scaled.length();
                (length.is_finite() && length > 0.0).then(|| scaled / length)
            }

            #[must_use]
            pub fn normalize_or_zero(self) -> Self {
                self.try_normalize().unwrap_or(Self::ZERO)
            }

            #[must_use]
            pub fn distance_squared(self, other: Self) -> $scalar {
                (self - other).length_squared()
            }

            #[must_use]
            pub fn distance(self, other: Self) -> $scalar {
                self.distance_squared(other).sqrt()
            }

            #[must_use]
            pub fn min(self, other: Self) -> Self {
                Self { $($field: self.$field.min(other.$field)),+ }
            }

            #[must_use]
            pub fn max(self, other: Self) -> Self {
                Self { $($field: self.$field.max(other.$field)),+ }
            }

            #[must_use]
            pub fn abs(self) -> Self { Self { $($field: self.$field.abs()),+ } }

            #[must_use]
            pub fn clamp(self, minimum: Self, maximum: Self) -> Self {
                Self { $($field: self.$field.clamp(minimum.$field, maximum.$field)),+ }
            }

            #[must_use]
            pub fn min_element(self) -> $scalar {
                let mut value = <$scalar>::INFINITY;
                $(value = value.min(self.$field);)+
                value
            }

            #[must_use]
            pub fn max_element(self) -> $scalar {
                let mut value = <$scalar>::NEG_INFINITY;
                $(value = value.max(self.$field);)+
                value
            }

            #[must_use]
            pub fn is_finite(self) -> bool { true $(&& self.$field.is_finite())+ }

            #[must_use]
            pub const fn to_array(self) -> [$scalar; define_float_vector!(@count $($field),+)] {
                [$(self.$field),+]
            }
        }

        impl Add for $name {
            type Output = Self;
            fn add(self, rhs: Self) -> Self { Self { $($field: self.$field + rhs.$field),+ } }
        }
        impl Sub for $name {
            type Output = Self;
            fn sub(self, rhs: Self) -> Self { Self { $($field: self.$field - rhs.$field),+ } }
        }
        impl Mul for $name {
            type Output = Self;
            fn mul(self, rhs: Self) -> Self { Self { $($field: self.$field * rhs.$field),+ } }
        }
        impl Div for $name {
            type Output = Self;
            fn div(self, rhs: Self) -> Self { Self { $($field: self.$field / rhs.$field),+ } }
        }
        impl Mul<$scalar> for $name {
            type Output = Self;
            fn mul(self, rhs: $scalar) -> Self { Self { $($field: self.$field * rhs),+ } }
        }
        impl Mul<$name> for $scalar {
            type Output = $name;
            fn mul(self, rhs: $name) -> $name { rhs * self }
        }
        impl Div<$scalar> for $name {
            type Output = Self;
            fn div(self, rhs: $scalar) -> Self { Self { $($field: self.$field / rhs),+ } }
        }
        impl Neg for $name {
            type Output = Self;
            fn neg(self) -> Self { Self { $($field: -self.$field),+ } }
        }
        impl AddAssign for $name {
            fn add_assign(&mut self, rhs: Self) { $(self.$field += rhs.$field;)+ }
        }
        impl SubAssign for $name {
            fn sub_assign(&mut self, rhs: Self) { $(self.$field -= rhs.$field;)+ }
        }
        impl MulAssign<$scalar> for $name {
            fn mul_assign(&mut self, rhs: $scalar) { $(self.$field *= rhs;)+ }
        }
        impl DivAssign<$scalar> for $name {
            fn div_assign(&mut self, rhs: $scalar) { $(self.$field /= rhs;)+ }
        }
    };
    (@count $first:ident $(, $rest:ident)*) => { 1usize $(+ { let _ = stringify!($rest); 1usize })* };
}

define_float_vector!(Vec2, f32, x, y);
define_float_vector!(Vec3, f32, x, y, z);
define_float_vector!(Vec4, f32, x, y, z, w);
define_float_vector!(DVec2, f64, x, y);
define_float_vector!(DVec3, f64, x, y, z);
define_float_vector!(DVec4, f64, x, y, z, w);

impl Vec2 {
    /// Positive X axis.
    pub const X: Self = Self::new(1.0, 0.0);
    /// Positive Y axis.
    pub const Y: Self = Self::new(0.0, 1.0);

    /// Signed 2D determinant of this vector and `other`.
    #[must_use]
    pub fn perp_dot(self, other: Self) -> f32 {
        self.x * other.y - self.y * other.x
    }
}

impl Vec3 {
    /// Positive X axis.
    pub const X: Self = Self::new(1.0, 0.0, 0.0);
    /// Positive Y axis.
    pub const Y: Self = Self::new(0.0, 1.0, 0.0);
    /// Positive Z axis.
    pub const Z: Self = Self::new(0.0, 0.0, 1.0);

    /// Right-handed cross product.
    #[must_use]
    pub fn cross(self, other: Self) -> Self {
        Self::new(
            self.y * other.z - self.z * other.y,
            self.z * other.x - self.x * other.z,
            self.x * other.y - self.y * other.x,
        )
    }

    /// Extend this 3D vector with a fourth component.
    #[must_use]
    pub const fn extend(self, w: f32) -> Vec4 {
        Vec4::new(self.x, self.y, self.z, w)
    }

    /// Component-wise comparison: true when every component is >= the other.
    #[must_use]
    pub const fn greater_equal(self, other: Self) -> bool {
        self.x >= other.x && self.y >= other.y && self.z >= other.z
    }

    /// Component-wise comparison: true when every component is <= the other.
    #[must_use]
    pub const fn less_equal(self, other: Self) -> bool {
        self.x <= other.x && self.y <= other.y && self.z <= other.z
    }
}

impl DVec3 {
    /// Right-handed cross product.
    #[must_use]
    pub fn cross(self, other: Self) -> Self {
        Self::new(
            self.y * other.z - self.z * other.y,
            self.z * other.x - self.x * other.z,
            self.x * other.y - self.y * other.x,
        )
    }
}

impl Vec4 {
    /// Positive X axis.
    pub const X: Self = Self::new(1.0, 0.0, 0.0, 0.0);
    /// Positive Y axis.
    pub const Y: Self = Self::new(0.0, 1.0, 0.0, 0.0);
    /// Positive Z axis.
    pub const Z: Self = Self::new(0.0, 0.0, 1.0, 0.0);
    /// Positive W axis.
    pub const W: Self = Self::new(0.0, 0.0, 0.0, 1.0);

    /// Drop the fourth component.
    #[must_use]
    pub const fn truncate(self) -> Vec3 {
        Vec3::new(self.x, self.y, self.z)
    }
}

macro_rules! define_integer_vector {
    ($name:ident, $scalar:ty, $($field:ident),+ $(,)?) => {
        #[repr(C)]
        #[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $name { $(pub $field: $scalar),+ }

        impl $name {
            pub const ZERO: Self = Self { $($field: 0),+ };
            pub const ONE: Self = Self { $($field: 1),+ };
            #[must_use]
            pub const fn new($($field: $scalar),+) -> Self { Self { $($field),+ } }
            #[must_use]
            pub const fn splat(value: $scalar) -> Self { Self { $($field: value),+ } }
            #[must_use]
            pub const fn to_array(self) -> [$scalar; define_integer_vector!(@count $($field),+)] { [$(self.$field),+] }

            /// Checked component-wise addition.
            #[must_use]
            pub fn checked_add(self, other: Self) -> Option<Self> {
                Some(Self { $($field: self.$field.checked_add(other.$field)?),+ })
            }

            /// Checked component-wise subtraction.
            #[must_use]
            pub fn checked_sub(self, other: Self) -> Option<Self> {
                Some(Self { $($field: self.$field.checked_sub(other.$field)?),+ })
            }

            /// Checked component-wise multiplication.
            #[must_use]
            pub fn checked_mul(self, other: Self) -> Option<Self> {
                Some(Self { $($field: self.$field.checked_mul(other.$field)?),+ })
            }
        }
        impl Add for $name {
            type Output = Self;
            fn add(self, rhs: Self) -> Self { Self { $($field: self.$field + rhs.$field),+ } }
        }
        impl Sub for $name {
            type Output = Self;
            fn sub(self, rhs: Self) -> Self { Self { $($field: self.$field - rhs.$field),+ } }
        }
        impl Mul for $name {
            type Output = Self;
            fn mul(self, rhs: Self) -> Self { Self { $($field: self.$field * rhs.$field),+ } }
        }
        impl Div for $name {
            type Output = Self;
            fn div(self, rhs: Self) -> Self { Self { $($field: self.$field / rhs.$field),+ } }
        }
        impl Mul<$scalar> for $name {
            type Output = Self;
            fn mul(self, rhs: $scalar) -> Self { Self { $($field: self.$field * rhs),+ } }
        }
        impl AddAssign for $name {
            fn add_assign(&mut self, rhs: Self) { $(self.$field += rhs.$field;)+ }
        }
        impl SubAssign for $name {
            fn sub_assign(&mut self, rhs: Self) { $(self.$field -= rhs.$field;)+ }
        }
    };
    (@count $first:ident $(, $rest:ident)*) => { 1usize $(+ { let _ = stringify!($rest); 1usize })* };
}

define_integer_vector!(IVec2, i32, x, y);
define_integer_vector!(IVec3, i32, x, y, z);
define_integer_vector!(IVec4, i32, x, y, z, w);
define_integer_vector!(UVec2, u32, x, y);
define_integer_vector!(UVec3, u32, x, y, z);
define_integer_vector!(UVec4, u32, x, y, z, w);
define_integer_vector!(I64Vec2, i64, x, y);
define_integer_vector!(I64Vec3, i64, x, y, z);
define_integer_vector!(I64Vec4, i64, x, y, z, w);
define_integer_vector!(U64Vec2, u64, x, y);
define_integer_vector!(U64Vec3, u64, x, y, z);
define_integer_vector!(U64Vec4, u64, x, y, z, w);
