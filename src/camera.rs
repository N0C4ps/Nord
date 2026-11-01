//! Game-owned controls that connect normalized input to the camera math crate.

use nord_app::{
    ApplicationDiagnosticError, ApplicationErrorContext, ApplicationInputEvent,
    ApplicationWindowHandle,
};
use nord_camera::PerspectiveCamera;
use nord_math::{Vec3, wrap_angle_pi};

const MOUSE_SENSITIVITY_RADIANS_PER_PIXEL: f32 = 0.003;
const MAX_PITCH_RADIANS: f32 = 1.553_343;

/// First-person mouse-look policy owned by the game.
///
/// Input events are delivered by `nord_inputs`; the controller translates
/// their pointer deltas into a pose update through `nord_camera`. Neither
/// engine crate depends on or calls the other.
pub(crate) struct GameCameraController {
    window: ApplicationWindowHandle,
    camera: PerspectiveCamera,
    yaw_radians: f32,
    pitch_radians: f32,
    last_pointer: Option<(f64, f64)>,
}

impl GameCameraController {
    /// Attach game-owned mouse-look behavior to a camera and one game window.
    #[must_use]
    pub(crate) fn new(camera: PerspectiveCamera, window: ApplicationWindowHandle) -> Self {
        let forward = camera.forward();
        Self {
            window,
            camera,
            yaw_radians: forward.x.atan2(-forward.z),
            pitch_radians: forward.y.clamp(-1.0, 1.0).asin(),
            last_pointer: None,
        }
    }

    /// Apply a normalized pointer event to the game camera.
    ///
    /// The pointer's first position is only a baseline. Subsequent positions
    /// rotate the camera by their pixel delta, independent of frame time.
    ///
    /// # Errors
    /// Returns an application diagnostic if invalid input produces a camera
    /// orientation that cannot be represented.
    pub(crate) fn handle_input(
        &mut self,
        event: &ApplicationInputEvent,
    ) -> nord_app::ApplicationResult<()> {
        let ApplicationInputEvent::PointerMoved { window, x, y } = event else {
            return Ok(());
        };
        if *window != self.window {
            return Ok(());
        }
        if !x.is_finite() || !y.is_finite() {
            return Err(ApplicationDiagnosticError::msg(
                "platform supplied a non-finite pointer position",
            ));
        }
        let Some((previous_x, previous_y)) = self.last_pointer.replace((*x, *y)) else {
            return Ok(());
        };
        let delta_x = mouse_angle_delta(*x - previous_x)
            .ok_or_else(|| ApplicationDiagnosticError::msg("horizontal mouse delta is invalid"))?;
        let delta_y = mouse_angle_delta(*y - previous_y)
            .ok_or_else(|| ApplicationDiagnosticError::msg("vertical mouse delta is invalid"))?;

        self.yaw_radians = wrap_angle_pi(self.yaw_radians + delta_x)
            .ok_or_else(|| ApplicationDiagnosticError::msg("camera yaw became non-finite"))?;
        self.pitch_radians =
            (self.pitch_radians - delta_y).clamp(-MAX_PITCH_RADIANS, MAX_PITCH_RADIANS);
        let cos_pitch = self.pitch_radians.cos();
        let forward = Vec3::new(
            cos_pitch * self.yaw_radians.sin(),
            self.pitch_radians.sin(),
            -cos_pitch * self.yaw_radians.cos(),
        );
        self.camera
            .set_pose(self.camera.position(), forward, Vec3::Y)
            .map_err(ApplicationDiagnosticError::msg)
            .context("updating the game camera from mouse movement")
    }

    /// Forget the last pointer sample when the game window loses focus.
    pub(crate) fn reset_pointer_tracking(&mut self) {
        self.last_pointer = None;
    }

    /// Current camera used by the game's render pass.
    #[must_use]
    pub(crate) const fn camera(&self) -> &PerspectiveCamera {
        &self.camera
    }

    /// Mutable camera lens for game-owned resize handling.
    pub(crate) fn camera_mut(&mut self) -> &mut PerspectiveCamera {
        &mut self.camera
    }
}

fn mouse_angle_delta(delta_pixels: f64) -> Option<f32> {
    let radians = delta_pixels * f64::from(MOUSE_SENSITIVITY_RADIANS_PER_PIXEL);
    if !radians.is_finite() || radians.abs() > f64::from(f32::MAX) {
        return None;
    }
    // Native pixel deltas are bounded and do not need more than f32 angular
    // precision for the camera pose API.
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    Some(radians as f32)
}

#[cfg(test)]
mod tests {
    use super::{GameCameraController, mouse_angle_delta};
    use nord_app::{ApplicationInputEvent, ApplicationPlatform, ApplicationPointerButton};
    use nord_camera::PerspectiveCamera;
    use nord_math::Vec3;

    fn test_camera() -> PerspectiveCamera {
        let mut camera = PerspectiveCamera::new(1.0, 16.0 / 9.0, 0.1, 100.0).unwrap();
        camera.look_at(Vec3::ZERO, -Vec3::Z, Vec3::Y).unwrap();
        camera
    }

    #[test]
    fn pointer_delta_rotates_camera_and_focus_reset_prevents_reentry_jump() {
        let platform = ApplicationPlatform::initialize("nord-camera-tests").unwrap();
        let window = platform.event_emitter().allocate_window_handle().unwrap();
        let mut controller = GameCameraController::new(test_camera(), window);

        controller
            .handle_input(&ApplicationInputEvent::PointerMoved { window, x: 100.0, y: 100.0 })
            .unwrap();
        let initial_forward = controller.camera.forward();
        controller
            .handle_input(&ApplicationInputEvent::PointerMoved { window, x: 200.0, y: 130.0 })
            .unwrap();
        let rotated_forward = controller.camera.forward();
        assert_ne!(rotated_forward, initial_forward);
        assert!(rotated_forward.y < initial_forward.y);

        controller.reset_pointer_tracking();
        controller
            .handle_input(&ApplicationInputEvent::PointerMoved { window, x: 900.0, y: 700.0 })
            .unwrap();
        assert_eq!(controller.camera.forward(), rotated_forward);
    }

    #[test]
    fn other_windows_and_non_pointer_events_do_not_change_camera_pose() {
        let platform = ApplicationPlatform::initialize("nord-camera-tests").unwrap();
        let emitter = platform.event_emitter();
        let window = emitter.allocate_window_handle().unwrap();
        let other_window = emitter.allocate_window_handle().unwrap();
        let mut controller = GameCameraController::new(test_camera(), window);
        let initial_forward = controller.camera.forward();

        controller
            .handle_input(&ApplicationInputEvent::PointerMoved {
                window: other_window,
                x: 100.0,
                y: 100.0,
            })
            .unwrap();
        controller
            .handle_input(&ApplicationInputEvent::PointerButton {
                window,
                button: ApplicationPointerButton::Primary,
                pressed: true,
            })
            .unwrap();
        assert_eq!(controller.camera.forward(), initial_forward);
    }

    #[test]
    fn mouse_angle_conversion_rejects_non_finite_values() {
        assert_eq!(mouse_angle_delta(f64::NAN), None);
        assert!(mouse_angle_delta(100.0).is_some_and(|delta| (delta - 0.3).abs() < 1.0e-6));
    }
}
