//! Game-owned first-person controls connecting input to the camera.

use nord_app::{
    ApplicationDiagnosticError, ApplicationErrorContext, ApplicationInputEvent,
    ApplicationInputSystem, ApplicationKeyCode, ApplicationPointerButton, ApplicationVec3 as Vec3,
    ApplicationWindowHandle, application_wrap_angle_pi as wrap_angle_pi,
};
use nord_camera::PerspectiveCamera;
use std::time::Duration;

const MOUSE_SENSITIVITY_RADIANS_PER_PIXEL: f32 = 0.003;
const MAX_PITCH_RADIANS: f32 = 1.553_343;

/// Cursor policy decision returned to the runtime for execution through `nord_app`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GameCursorAction {
    None,
    Lock,
    Unlock,
}

/// First-person mouse-look policy owned by the game.
pub(crate) struct GameCameraController {
    window: ApplicationWindowHandle,
    camera: PerspectiveCamera,
    yaw_radians: f32,
    pitch_radians: f32,
    focused: bool,
    cursor_locked: bool,
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
            focused: false,
            cursor_locked: false,
        }
    }

    /// Apply a normalized event and return any native cursor transition requested by the game.
    ///
    /// # Errors
    /// Returns a diagnostic if relative mouse input is non-finite or produces an invalid pose.
    pub(crate) fn handle_input(
        &mut self,
        event: &ApplicationInputEvent,
    ) -> nord_app::ApplicationResult<GameCursorAction> {
        match event {
            ApplicationInputEvent::PointerButton {
                window,
                button: ApplicationPointerButton::Primary,
                pressed: true,
            } if *window == self.window && self.focused && !self.cursor_locked => {
                self.cursor_locked = true;
                return Ok(GameCursorAction::Lock);
            }
            ApplicationInputEvent::Key {
                window,
                key: ApplicationKeyCode::Escape,
                pressed: true,
                repeat: false,
                ..
            } if *window == self.window && self.cursor_locked => {
                self.cursor_locked = false;
                return Ok(GameCursorAction::Unlock);
            }
            ApplicationInputEvent::PointerDelta { window, delta_x, delta_y }
                if *window == self.window && self.cursor_locked =>
            {
                if !delta_x.is_finite() || !delta_y.is_finite() {
                    return Err(ApplicationDiagnosticError::msg(
                        "platform supplied a non-finite relative pointer delta",
                    ));
                }
                let delta_x = mouse_angle_delta(*delta_x).ok_or_else(|| {
                    ApplicationDiagnosticError::msg("horizontal mouse delta is invalid")
                })?;
                let delta_y = mouse_angle_delta(*delta_y).ok_or_else(|| {
                    ApplicationDiagnosticError::msg("vertical mouse delta is invalid")
                })?;
                self.rotate_camera(delta_x, delta_y)?;
            }
            _ => {}
        }
        Ok(GameCursorAction::None)
    }

    /// Focus by itself never captures the pointer; focus loss requests a release.
    pub(crate) fn handle_window_focus(&mut self, focused: bool) -> GameCursorAction {
        self.focused = focused;
        if !focused && self.cursor_locked {
            self.cursor_locked = false;
            GameCursorAction::Unlock
        } else {
            GameCursorAction::None
        }
    }

    /// Reconcile the game-owned state when the native platform rejects capture.
    pub(crate) fn handle_cursor_capture_failure(&mut self) {
        self.cursor_locked = false;
    }

    /// Advance deterministic keyboard movement during one simulation step.
    pub(crate) fn advance_fixed_step(
        &mut self,
        inputs: &ApplicationInputSystem,
        step: Duration,
    ) -> nord_app::ApplicationResult<()> {
        let mut movement = Vec3::ZERO;
        let forward = Vec3::new(self.camera.forward().x, 0.0, self.camera.forward().z)
            .try_normalize()
            .unwrap_or(-Vec3::Z);
        let right = forward.cross(Vec3::Y).try_normalize().unwrap_or(Vec3::X);

        if inputs.is_key_pressed(self.window, ApplicationKeyCode::Letter('W')) {
            movement += forward;
        }
        if inputs.is_key_pressed(self.window, ApplicationKeyCode::Letter('S')) {
            movement -= forward;
        }
        if inputs.is_key_pressed(self.window, ApplicationKeyCode::Letter('D')) {
            movement += right;
        }
        if inputs.is_key_pressed(self.window, ApplicationKeyCode::Letter('A')) {
            movement -= right;
        }
        if inputs.is_key_pressed(self.window, ApplicationKeyCode::Letter('E')) {
            movement += Vec3::Y;
        }
        if inputs.is_key_pressed(self.window, ApplicationKeyCode::Letter('Q')) {
            movement -= Vec3::Y;
        }

        let Some(direction) = movement.try_normalize() else {
            return Ok(());
        };
        let distance = 5.0 * step.as_secs_f32();
        let position = self.camera.position() + direction * distance;
        self.camera
            .set_pose(position, self.camera.forward(), Vec3::Y)
            .map_err(ApplicationDiagnosticError::msg)
            .context("advancing the game camera during a fixed simulation step")
    }

    #[cfg(test)]
    #[must_use]
    const fn cursor_locked(&self) -> bool {
        self.cursor_locked
    }

    fn rotate_camera(&mut self, delta_x: f32, delta_y: f32) -> nord_app::ApplicationResult<()> {
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
            .context("updating the game camera from relative mouse movement")
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
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    Some(radians as f32)
}

#[cfg(test)]
mod tests {
    use super::{GameCameraController, GameCursorAction, mouse_angle_delta};
    use nord_app::{
        ApplicationInputEvent, ApplicationKeyCode, ApplicationModifiers, ApplicationPlatform,
        ApplicationPointerButton, ApplicationVec3 as Vec3,
    };
    use nord_camera::PerspectiveCamera;

    fn test_camera() -> PerspectiveCamera {
        let mut camera = PerspectiveCamera::new(1.0, 16.0 / 9.0, 0.1, 100.0).unwrap();
        camera.look_at(Vec3::ZERO, -Vec3::Z, Vec3::Y).unwrap();
        camera
    }

    fn primary_click(window: nord_app::ApplicationWindowHandle) -> ApplicationInputEvent {
        ApplicationInputEvent::PointerButton {
            window,
            button: ApplicationPointerButton::Primary,
            pressed: true,
        }
    }

    #[test]
    fn focus_alone_does_not_capture_and_click_then_escape_releases() {
        let platform = ApplicationPlatform::initialize("nord-camera-tests").unwrap();
        let window = platform.event_emitter().allocate_window_handle().unwrap();
        let mut controller = GameCameraController::new(test_camera(), window);

        assert_eq!(controller.handle_window_focus(true), GameCursorAction::None);
        assert!(!controller.cursor_locked());
        assert_eq!(
            controller.handle_input(&primary_click(window)).unwrap(),
            GameCursorAction::Lock
        );
        assert!(controller.cursor_locked());
        assert_eq!(
            controller
                .handle_input(&ApplicationInputEvent::Key {
                    window,
                    key: ApplicationKeyCode::Escape,
                    pressed: true,
                    repeat: false,
                    modifiers: ApplicationModifiers::EMPTY,
                })
                .unwrap(),
            GameCursorAction::Unlock
        );
        assert!(!controller.cursor_locked());
    }

    #[test]
    fn relative_motion_rotates_only_when_captured_and_focus_loss_releases() {
        let platform = ApplicationPlatform::initialize("nord-camera-tests").unwrap();
        let window = platform.event_emitter().allocate_window_handle().unwrap();
        let mut controller = GameCameraController::new(test_camera(), window);
        let initial_forward = controller.camera.forward();

        controller.handle_window_focus(true);
        controller
            .handle_input(&ApplicationInputEvent::PointerDelta {
                window,
                delta_x: 100.0,
                delta_y: 30.0,
            })
            .unwrap();
        assert_eq!(controller.camera.forward(), initial_forward);

        controller.handle_input(&primary_click(window)).unwrap();
        controller
            .handle_input(&ApplicationInputEvent::PointerDelta {
                window,
                delta_x: 100.0,
                delta_y: 30.0,
            })
            .unwrap();
        assert_ne!(controller.camera.forward(), initial_forward);
        assert!(controller.camera.forward().y < initial_forward.y);
        assert_eq!(controller.handle_window_focus(false), GameCursorAction::Unlock);
        assert!(!controller.cursor_locked());
    }

    #[test]
    fn other_window_and_unlocked_pointer_events_do_not_change_camera() {
        let platform = ApplicationPlatform::initialize("nord-camera-tests").unwrap();
        let emitter = platform.event_emitter();
        let window = emitter.allocate_window_handle().unwrap();
        let other_window = emitter.allocate_window_handle().unwrap();
        let mut controller = GameCameraController::new(test_camera(), window);
        let initial_forward = controller.camera.forward();

        controller.handle_window_focus(true);
        controller
            .handle_input(&ApplicationInputEvent::PointerDelta {
                window: other_window,
                delta_x: 100.0,
                delta_y: 100.0,
            })
            .unwrap();
        controller
            .handle_input(&ApplicationInputEvent::PointerDelta {
                window,
                delta_x: 100.0,
                delta_y: 100.0,
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
