//! Backend-independent 3D camera state and projection math.
//!
//! The crate owns camera geometry only. It does not depend on a window,
//! renderer, input system, or game-specific world representation.

use nord_math::{Mat4, Vec3};
use std::error::Error;
use std::fmt::{Display, Formatter};

/// Invalid camera lens or pose parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CameraError {
    NonFiniteParameter,
    InvalidFieldOfView,
    InvalidAspectRatio,
    InvalidClipPlanes,
    UnrepresentableProjection,
    DegenerateDirection,
    DegenerateUp,
    ParallelDirectionAndUp,
}

impl Display for CameraError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::NonFiniteParameter => "camera parameters must be finite",
            Self::InvalidFieldOfView => "vertical field of view must be between zero and pi",
            Self::InvalidAspectRatio => "camera aspect ratio must be greater than zero",
            Self::InvalidClipPlanes => "camera clip planes require 0 < near < far",
            Self::UnrepresentableProjection => {
                "camera projection cannot be represented with finite matrix values"
            }
            Self::DegenerateDirection => "camera forward direction must be non-zero",
            Self::DegenerateUp => "camera up direction must be non-zero",
            Self::ParallelDirectionAndUp => "camera forward and up directions must not be parallel",
        })
    }
}

impl Error for CameraError {}

/// Right-handed perspective camera using a zero-to-one depth range.
///
/// The camera looks along `forward`; its view and projection matrices match
/// the right-handed, `[0, 1]` clip-depth convention used by Direct3D 12.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PerspectiveCamera {
    position: Vec3,
    forward: Vec3,
    up: Vec3,
    vertical_fov_radians: f32,
    aspect_ratio: f32,
    near_plane: f32,
    far_plane: f32,
}

impl PerspectiveCamera {
    /// Create a perspective camera at the origin looking down negative Z.
    ///
    /// # Errors
    /// Returns a [`CameraError`] when lens parameters are invalid.
    pub fn new(
        vertical_fov_radians: f32,
        aspect_ratio: f32,
        near_plane: f32,
        far_plane: f32,
    ) -> Result<Self, CameraError> {
        validate_lens(vertical_fov_radians, aspect_ratio, near_plane, far_plane)?;
        if Mat4::perspective_rh_zo(vertical_fov_radians, aspect_ratio, near_plane, far_plane)
            .is_none()
        {
            return Err(CameraError::UnrepresentableProjection);
        }
        Ok(Self {
            position: Vec3::ZERO,
            forward: -Vec3::Z,
            up: Vec3::Y,
            vertical_fov_radians,
            aspect_ratio,
            near_plane,
            far_plane,
        })
    }

    /// Update the camera pose after validating that the basis is usable.
    ///
    /// # Errors
    /// Returns an error for non-finite positions or degenerate directions.
    pub fn set_pose(&mut self, position: Vec3, forward: Vec3, up: Vec3) -> Result<(), CameraError> {
        if !position.is_finite() || !forward.is_finite() || !up.is_finite() {
            return Err(CameraError::NonFiniteParameter);
        }
        let forward = forward.try_normalize().ok_or(CameraError::DegenerateDirection)?;
        let up = up.try_normalize().ok_or(CameraError::DegenerateUp)?;
        if forward.cross(up).try_normalize().is_none() {
            return Err(CameraError::ParallelDirectionAndUp);
        }
        if Mat4::look_to_rh(position, forward, up).is_none() {
            return Err(CameraError::NonFiniteParameter);
        }
        self.position = position;
        self.forward = forward;
        self.up = up;
        Ok(())
    }

    /// Point the camera from `position` toward `target`.
    ///
    /// # Errors
    /// Returns an error for coincident points or an invalid up vector.
    pub fn look_at(&mut self, position: Vec3, target: Vec3, up: Vec3) -> Result<(), CameraError> {
        if !target.is_finite() {
            return Err(CameraError::NonFiniteParameter);
        }
        self.set_pose(position, target - position, up)
    }

    /// Change the aspect ratio, typically after a non-zero surface resize.
    ///
    /// # Errors
    /// Returns [`CameraError::InvalidAspectRatio`] for zero, negative, or
    /// non-finite values.
    pub fn set_aspect_ratio(&mut self, aspect_ratio: f32) -> Result<(), CameraError> {
        if !aspect_ratio.is_finite() {
            return Err(CameraError::NonFiniteParameter);
        }
        if aspect_ratio <= 0.0 {
            return Err(CameraError::InvalidAspectRatio);
        }
        if Mat4::perspective_rh_zo(
            self.vertical_fov_radians,
            aspect_ratio,
            self.near_plane,
            self.far_plane,
        )
        .is_none()
        {
            return Err(CameraError::UnrepresentableProjection);
        }
        self.aspect_ratio = aspect_ratio;
        Ok(())
    }

    /// World-space camera position.
    #[must_use]
    pub const fn position(self) -> Vec3 {
        self.position
    }

    /// Unit world-space forward direction.
    #[must_use]
    pub const fn forward(self) -> Vec3 {
        self.forward
    }

    /// Current viewport aspect ratio.
    #[must_use]
    pub const fn aspect_ratio(self) -> f32 {
        self.aspect_ratio
    }

    /// Build the world-to-view matrix.
    ///
    /// # Errors
    /// Returns [`CameraError::NonFiniteParameter`] if numerical overflow makes
    /// the view transform unrepresentable.
    pub fn view_matrix(self) -> Result<Mat4, CameraError> {
        Mat4::look_to_rh(self.position, self.forward, self.up)
            .ok_or(CameraError::NonFiniteParameter)
    }

    /// Build a right-handed perspective matrix with D3D-style `[0, 1]` depth.
    ///
    /// # Errors
    /// Returns [`CameraError::UnrepresentableProjection`] if the finite lens
    /// parameters overflow the projection matrix representation.
    pub fn projection_matrix(self) -> Result<Mat4, CameraError> {
        Mat4::perspective_rh_zo(
            self.vertical_fov_radians,
            self.aspect_ratio,
            self.near_plane,
            self.far_plane,
        )
        .ok_or(CameraError::UnrepresentableProjection)
    }

    /// Build the matrix that transforms world-space positions to clip space.
    ///
    /// # Errors
    /// Returns a camera error if either matrix is not representable or their
    /// product overflows.
    pub fn view_projection_matrix(self) -> Result<Mat4, CameraError> {
        let matrix = self.projection_matrix()? * self.view_matrix()?;
        matrix
            .to_cols()
            .iter()
            .all(|column| column.is_finite())
            .then_some(matrix)
            .ok_or(CameraError::NonFiniteParameter)
    }
}

fn validate_lens(
    vertical_fov_radians: f32,
    aspect_ratio: f32,
    near_plane: f32,
    far_plane: f32,
) -> Result<(), CameraError> {
    if !vertical_fov_radians.is_finite()
        || !aspect_ratio.is_finite()
        || !near_plane.is_finite()
        || !far_plane.is_finite()
    {
        return Err(CameraError::NonFiniteParameter);
    }
    if vertical_fov_radians <= 0.0 || vertical_fov_radians >= std::f32::consts::PI {
        return Err(CameraError::InvalidFieldOfView);
    }
    if aspect_ratio <= 0.0 {
        return Err(CameraError::InvalidAspectRatio);
    }
    if near_plane <= 0.0 || far_plane <= near_plane {
        return Err(CameraError::InvalidClipPlanes);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{CameraError, PerspectiveCamera};
    use nord_math::{Vec3, Vec4};

    #[test]
    fn rejects_invalid_lens_and_pose_parameters() {
        assert_eq!(
            PerspectiveCamera::new(1.0, 0.0, 0.1, 100.0).unwrap_err(),
            CameraError::InvalidAspectRatio
        );
        assert_eq!(
            PerspectiveCamera::new(1.0, 1.0, 0.0, 100.0).unwrap_err(),
            CameraError::InvalidClipPlanes
        );
        let mut camera = PerspectiveCamera::new(1.0, 1.0, 0.1, 100.0).unwrap();
        assert_eq!(
            camera.set_pose(Vec3::ZERO, -Vec3::Z, -Vec3::Z),
            Err(CameraError::ParallelDirectionAndUp)
        );
    }

    #[test]
    fn view_projection_maps_near_and_far_to_zero_one_depth() {
        let mut camera = PerspectiveCamera::new(1.0, 16.0 / 9.0, 0.1, 100.0).unwrap();
        camera.look_at(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y).unwrap();
        let matrix = camera.view_projection_matrix().unwrap();
        let near = matrix * Vec4::new(0.0, 0.0, 4.9, 1.0);
        let far = matrix * Vec4::new(0.0, 0.0, -95.0, 1.0);
        assert!((near.z / near.w).abs() < 1.0e-5);
        assert!(((far.z / far.w) - 1.0).abs() < 1.0e-5);
    }

    #[test]
    fn resize_updates_aspect_without_changing_pose_or_lens() {
        let mut camera = PerspectiveCamera::new(1.0, 1.0, 0.1, 100.0).unwrap();
        camera.set_aspect_ratio(2.0).unwrap();
        assert!((camera.aspect_ratio() - 2.0).abs() < f32::EPSILON);
        assert_eq!(camera.position(), Vec3::ZERO);
        assert_eq!(camera.forward(), -Vec3::Z);
    }
}
