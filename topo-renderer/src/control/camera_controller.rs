use itertools::Itertools;
use std::collections::{BTreeMap, VecDeque};
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
use strum::{EnumIter, IntoEnumIterator};
#[cfg(target_arch = "wasm32")]
use web_time::Duration;

use winit::{
    dpi::PhysicalPosition,
    event::{DeviceEvent, ElementState, KeyEvent, MouseButton, Touch, WindowEvent},
    keyboard::{KeyCode, PhysicalKey},
};

use crate::data::camera::Camera;

enum CameraControllerEvent {
    ToggleViewMode,
    UpdateCameraOrientation {
        start_position: StoredMultiPosition,
        end_position: StoredMultiPosition,
    },
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

#[derive(Copy, Clone, Debug, PartialEq, PartialOrd)]
struct TouchPosition {
    id: u64,
    location: PhysicalPosition<f64>,
}

#[derive(Clone, Debug, PartialEq, PartialOrd)]
struct MultiTouchPositions {
    position1: TouchPosition,
    position2: TouchPosition,
    others: VecDeque<TouchPosition>,
}

#[derive(Clone, Debug, PartialEq, PartialOrd)]
enum TouchState {
    Off,
    Single(TouchPosition),
    Multi(MultiTouchPositions),
}

#[derive(Copy, Clone, Debug, PartialEq, PartialOrd)]
struct StoredMultiPosition {
    position1: PhysicalPosition<f64>,
    position2: PhysicalPosition<f64>,
}

impl StoredMultiPosition {
    fn new(
        original_position1: PhysicalPosition<f64>,
        original_position2: PhysicalPosition<f64>,
    ) -> Self {
        Self {
            position1: original_position1,
            position2: original_position2,
        }
    }

    fn from_touch_state(touch_state: &TouchState) -> Option<StoredMultiPosition> {
        match touch_state {
            TouchState::Multi(positions) => Some(Self::from_multi_positions(positions)),
            _ => None,
        }
    }

    fn from_multi_positions(positions: &MultiTouchPositions) -> StoredMultiPosition {
        Self::new(positions.position1.location, positions.position2.location)
    }
}

pub struct CameraController {
    speed: f32,
    is_pressed_map: BTreeMap<Control, bool>,
    mouse_view_delta: (f32, f32),
    mouse_ctrl_delta: (f32, f32),
    touch_state: TouchState,
    touch_single_delta: (f64, f64),
    touch_multi_delta: Option<StoredMultiPosition>,
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
            touch_state: TouchState::Off,
            touch_single_delta: (0.0, 0.0),
            touch_multi_delta: None,
            events_to_process: VecDeque::default(),
        }
    }

    fn is_pressed(&self, control: Control) -> bool {
        *self.is_pressed_map.get(&control).unwrap_or(&false)
    }

    pub fn process_events(&mut self, event: &WindowEvent) -> bool {
        match *event {
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        state,
                        physical_key: PhysicalKey::Code(keycode),
                        ..
                    },
                ..
            } => {
                let is_pressed = state == ElementState::Pressed;
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
                            .push_back(CameraControllerEvent::ToggleViewMode);
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
            } if button == MouseButton::Right => {
                self.is_pressed_map
                    .get_mut(&Control::MouseRight)
                    .map(|pressed| *pressed = state.is_pressed());
                true
            }
            WindowEvent::Touch(Touch {
                phase,
                location,
                id,
                ..
            }) => {
                if let Some(new_state) = match (phase, &mut self.touch_state) {
                    (winit::event::TouchPhase::Started, TouchState::Off) => {
                        Some(TouchState::Single(TouchPosition { id, location }))
                    }
                    (winit::event::TouchPhase::Started, TouchState::Single(prev_position)) => {
                        if prev_position.id != id {
                            Some(TouchState::Multi(MultiTouchPositions {
                                position1: *prev_position,
                                position2: TouchPosition { id, location },
                                others: VecDeque::new(),
                            }))
                        } else {
                            prev_position.location = location;
                            None
                        }
                    }
                    (
                        winit::event::TouchPhase::Started,
                        TouchState::Multi(MultiTouchPositions {
                            position1,
                            position2,
                            others,
                        }),
                    ) => match id {
                        id if id == position1.id => {
                            position1.location = location;
                            None
                        }
                        id if id == position2.id => {
                            position2.location = location;
                            None
                        }
                        id => {
                            others.push_back(TouchPosition { id, location });
                            None
                        }
                    },
                    (winit::event::TouchPhase::Moved, TouchState::Off) => None,
                    (
                        winit::event::TouchPhase::Moved,
                        TouchState::Single(TouchPosition {
                            id: prev_id,
                            location: prev_location,
                        }),
                    ) if id == *prev_id => {
                        self.touch_single_delta.0 += location.x - prev_location.x;
                        self.touch_single_delta.1 += location.y - prev_location.y;
                        *prev_location = location;
                        None
                    }
                    (
                        winit::event::TouchPhase::Moved,
                        TouchState::Multi(MultiTouchPositions {
                            position1,
                            position2,
                            others,
                        }),
                    ) => {
                        if id == position1.id {
                            position1.location = location;
                        } else if id == position2.id {
                            position2.location = location;
                        } else {
                            if let Some(position) = others.iter_mut().find(|x| x.id == id) {
                                position.location = location;
                            }
                        }
                        None
                    }
                    (
                        winit::event::TouchPhase::Ended | winit::event::TouchPhase::Cancelled,
                        TouchState::Single(prev),
                    ) if id == prev.id => Some(TouchState::Off),
                    (
                        winit::event::TouchPhase::Ended | winit::event::TouchPhase::Cancelled,
                        TouchState::Multi(prev),
                    ) => match id {
                        id if id == prev.position1.id || id == prev.position2.id => {
                            let position_to_keep = if id == prev.position1.id {
                                prev.position2
                            } else {
                                prev.position1
                            };
                            if let Some(start_position) = self.touch_multi_delta.take() {
                                self.events_to_process.push_back(
                                    CameraControllerEvent::UpdateCameraOrientation {
                                        start_position,
                                        end_position: StoredMultiPosition::from_multi_positions(
                                            &prev,
                                        ),
                                    },
                                );
                            }
                            if let Some(position_popped) = prev.others.pop_front() {
                                Some(TouchState::Multi(MultiTouchPositions {
                                    position1: position_to_keep,
                                    position2: position_popped,
                                    others: std::mem::take(&mut prev.others),
                                }))
                            } else {
                                Some(TouchState::Single(position_to_keep))
                            }
                        }
                        id => {
                            if let Some((index, _)) =
                                prev.others.iter().find_position(|x| x.id == id)
                            {
                                prev.others.remove(index);
                            }
                            None
                        }
                    },
                    _ => None,
                } {
                    self.touch_multi_delta = StoredMultiPosition::from_touch_state(&new_state);
                    self.touch_state = new_state;
                }

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

    pub fn update_camera(
        &mut self,
        camera: &mut Camera,
        size: (u32, u32),
        time_delta: Duration,
    ) -> bool {
        let mut changed = false;
        let increment = self.speed * 0.1 * time_delta.as_micros() as f32;
        if self.is_pressed(Control::Q) {
            camera.set_fovy(camera.fov_y() - 0.001 * increment);
            changed = true;
        }
        if self.is_pressed(Control::E) {
            camera.set_fovy(camera.fov_y() + 0.001 * increment);
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

        if self.touch_single_delta != (0.0, 0.0) {
            const MOVE_SCALING: f32 = 5.0;
            camera.set_eye(
                camera.eye + camera.direction() * MOVE_SCALING * self.touch_single_delta.1 as f32
                    - camera.direction_right() * MOVE_SCALING * self.touch_single_delta.0 as f32,
            );
            self.touch_single_delta = (0.0, 0.0);
            changed = true;
        }

        self.events_to_process
            .drain(..)
            .for_each(|event| match event {
                CameraControllerEvent::ToggleViewMode => {
                    camera.view_mode = camera.view_mode.toggle();
                    changed = true;
                }
                CameraControllerEvent::UpdateCameraOrientation {
                    start_position,
                    end_position,
                } => {
                    let (rotation_change, new_fov) = get_rotation_and_fov_change(
                        start_position,
                        end_position,
                        camera.get_fovy(),
                        size,
                    );

                    if rotation_change != 0.0 || new_fov != 0.0 {
                        camera.rotate_yaw(-rotation_change);
                        camera.set_fovy(new_fov);
                        changed = true;
                    }
                }
            });

        if let (Some(delta), TouchState::Multi(positions)) =
            (self.touch_multi_delta.take(), &self.touch_state)
        {
            let (rotation_change, new_fov) = get_rotation_and_fov_change(
                delta,
                StoredMultiPosition::from_multi_positions(positions),
                camera.get_fovy(),
                size,
            );

            if rotation_change != 0.0 || new_fov != 0.0 {
                camera.rotate_yaw(-rotation_change);
                camera.set_fovy(new_fov);
                changed = true;
            }

            self.touch_multi_delta = StoredMultiPosition::from_touch_state(&self.touch_state);
        }

        changed
    }
}

fn get_rotation_and_fov_change(
    start_position: StoredMultiPosition,
    end_position: StoredMultiPosition,
    fov: f32,
    size: (u32, u32),
) -> (f32, f32) {
    if ((end_position.position2.x - end_position.position1.x) as i32).abs() < 1 {
        return (0.0, fov);
    }

    let fov_p = (start_position.position2.x - start_position.position1.x) as f32
        / (end_position.position2.x - end_position.position1.x) as f32
        * fov;
    let angle_change =
        fov / size.1 as f32 / (end_position.position2.x - end_position.position1.x) as f32
            * ((start_position.position1.x * end_position.position2.x
                - end_position.position1.x * start_position.position2.x) as f32
                + 0.5
                    * size.1 as f32
                    * (start_position.position2.x
                        - start_position.position1.x
                        - end_position.position2.x
                        + end_position.position1.x) as f32);

    (angle_change, fov_p)
}
