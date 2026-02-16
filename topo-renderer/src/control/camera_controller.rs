use std::collections::{BTreeMap, VecDeque};
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
use strum::{EnumIter, IntoEnumIterator};
#[cfg(target_arch = "wasm32")]
use web_time::Duration;

use winit::{
    event::{DeviceEvent, ElementState, KeyEvent, MouseButton, WindowEvent},
    keyboard::{KeyCode, PhysicalKey},
};

use crate::data::camera::Camera;

const MIN_FOV: f32 = 10.0;
const MAX_FOV: f32 = 160.0;

pub enum CameraControllerEvent {
    ToggleViewMode,
}

#[derive(Copy, Clone, Debug, EnumIter, PartialEq, Eq, PartialOrd, Ord)]
pub enum Control {
    Up,
    Down,
    Left,
    Right,
    E,
    Q,
    Ctrl,
    Space,
    Shift,
    MouseRight,
}

pub struct CameraController {
    speed: f32,
    is_pressed_map: BTreeMap<Control, bool>,
    mouse_view_delta: (f32, f32),
    mouse_ctrl_delta: (f32, f32),
    events_to_process: VecDeque<CameraControllerEvent>,
}

impl CameraController {
    pub fn new(speed: f32) -> Self {
        let mut is_pressed = BTreeMap::new();
        for control in Control::iter() {
            is_pressed.insert(control, false);
        }
        Self {
            speed,
            is_pressed_map: is_pressed,
            mouse_view_delta: (0.0, 0.0),
            mouse_ctrl_delta: (0.0, 0.0),
            events_to_process: VecDeque::default(),
        }
    }

    fn is_pressed(&self, control: Control) -> bool {
        *self.is_pressed_map.get(&control).unwrap_or(&false)
    }

    pub fn process_events(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        state,
                        physical_key: PhysicalKey::Code(keycode),
                        ..
                    },
                ..
            } => {
                let is_pressed = *state == ElementState::Pressed;
                match keycode {
                    KeyCode::KeyW | KeyCode::ArrowUp => {
                        self.is_pressed_map
                            .get_mut(&Control::Up)
                            .map(|pressed| *pressed = is_pressed);
                        true
                    }
                    KeyCode::KeyS | KeyCode::ArrowDown => {
                        self.is_pressed_map
                            .get_mut(&Control::Down)
                            .map(|pressed| *pressed = is_pressed);
                        true
                    }

                    KeyCode::KeyA | KeyCode::ArrowLeft => {
                        self.is_pressed_map
                            .get_mut(&Control::Left)
                            .map(|pressed| *pressed = is_pressed);
                        true
                    }
                    KeyCode::KeyD | KeyCode::ArrowRight => {
                        self.is_pressed_map
                            .get_mut(&Control::Right)
                            .map(|pressed| *pressed = is_pressed);
                        true
                    }
                    KeyCode::KeyQ => {
                        self.is_pressed_map
                            .get_mut(&Control::Q)
                            .map(|pressed| *pressed = is_pressed);
                        true
                    }
                    KeyCode::KeyE => {
                        self.is_pressed_map
                            .get_mut(&Control::E)
                            .map(|pressed| *pressed = is_pressed);
                        true
                    }
                    KeyCode::Space => {
                        self.is_pressed_map
                            .get_mut(&Control::Space)
                            .map(|pressed| *pressed = is_pressed);
                        true
                    }
                    KeyCode::ShiftLeft => {
                        self.is_pressed_map
                            .get_mut(&Control::Shift)
                            .map(|pressed| *pressed = is_pressed);
                        true
                    }
                    KeyCode::ControlLeft => {
                        self.is_pressed_map
                            .get_mut(&Control::Ctrl)
                            .map(|pressed| *pressed = is_pressed);
                        true
                    }
                    KeyCode::KeyF if is_pressed => {
                        self.events_to_process
                            .push_front(CameraControllerEvent::ToggleViewMode);
                        true
                    }
                    _ => false,
                }
            }
            WindowEvent::CursorLeft { device_id: _ } => {
                self.is_pressed_map
                    .iter_mut()
                    .for_each(|(_, pressed)| *pressed = false);
                false
            }
            WindowEvent::MouseInput {
                device_id: _,
                state,
                button,
            } if button == &MouseButton::Right => {
                self.is_pressed_map
                    .get_mut(&Control::MouseRight)
                    .map(|pressed| *pressed = state.is_pressed());
                true
            }
            _ => false,
        }
    }

    pub fn process_device_events(&mut self, event: &DeviceEvent) {
        match event {
            DeviceEvent::MouseMotion { delta } => {
                if self.is_pressed(Control::Ctrl) {
                    self.mouse_ctrl_delta.0 += delta.0 as f32;
                    self.mouse_ctrl_delta.1 += delta.1 as f32;
                } else if self.is_pressed(Control::MouseRight) {
                    self.mouse_view_delta.0 += delta.0 as f32;
                    self.mouse_view_delta.1 += delta.1 as f32;
                }
            }
            _ => {}
        }
    }

    pub fn update_camera(&mut self, camera: &mut Camera, time_delta: Duration) -> bool {
        let mut changed = false;
        let increment = self.speed * 0.1 * time_delta.as_micros() as f32;
        if self.is_pressed(Control::Q) {
            camera.set_fovy((camera.fov_y() - 0.001 * increment).max(MIN_FOV.to_radians()));
            changed = true;
        }
        if self.is_pressed(Control::E) {
            camera.set_fovy((camera.fov_y() + 0.001 * increment).min(MAX_FOV.to_radians()));
            changed = true;
        }
        if self.is_pressed(Control::Up) {
            camera.set_eye(camera.eye + camera.direction() * increment);
            changed = true;
        }
        if self.is_pressed(Control::Down) {
            camera.set_eye(camera.eye - camera.direction() * increment);
            changed = true;
        }
        if self.is_pressed(Control::Right) {
            camera.set_eye(camera.eye + camera.direction_right() * increment);
            changed = true;
        }
        if self.is_pressed(Control::Left) {
            camera.set_eye(camera.eye - camera.direction_right() * increment);
            changed = true;
        }
        if self.is_pressed(Control::Shift) {
            camera.set_eye(camera.eye - camera.up() * increment);
            changed = true;
        }
        if self.is_pressed(Control::Space) {
            camera.set_eye(camera.eye + camera.up() * increment);
            changed = true;
        }
        camera.sun_angle.theta += self.mouse_ctrl_delta.0;
        camera.sun_angle.phi += self.mouse_ctrl_delta.1;

        if self.mouse_view_delta != (0.0, 0.0) {
            camera.rotate_yaw(-self.mouse_view_delta.0 * 0.01);
            camera.rotate_pitch(self.mouse_view_delta.1 * 0.01);
            changed = true;
            self.mouse_view_delta = (0.0, 0.0);
        }

        if self.mouse_ctrl_delta != (0.0, 0.0) {
            changed = true;
            self.mouse_ctrl_delta = (0.0, 0.0);
        }

        self.events_to_process
            .drain(..)
            .for_each(|event| match event {
                CameraControllerEvent::ToggleViewMode => {
                    camera.view_mode = camera.view_mode.toggle();
                    changed = true;
                }
            });

        changed
    }
}
