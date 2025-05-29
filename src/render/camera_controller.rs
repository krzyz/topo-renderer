use std::{collections::VecDeque, time::Duration};

use winit::{
    event::{DeviceEvent, ElementState, KeyEvent, WindowEvent},
    keyboard::{KeyCode, PhysicalKey},
};

use super::camera::Camera;

pub enum CameraControllerEvent {
    ToggleViewMode,
}

pub struct CameraController {
    speed: f32,
    is_up_pressed: bool,
    is_down_pressed: bool,
    is_left_pressed: bool,
    is_right_pressed: bool,
    is_e_pressed: bool,
    is_q_pressed: bool,
    mouse_total_delta: (f32, f32),
    events_to_process: VecDeque<CameraControllerEvent>,
}

impl CameraController {
    pub fn new(speed: f32) -> Self {
        Self {
            speed,
            is_up_pressed: false,
            is_down_pressed: false,
            is_left_pressed: false,
            is_right_pressed: false,
            is_e_pressed: false,
            is_q_pressed: false,
            mouse_total_delta: (0.0, 0.0),
            events_to_process: VecDeque::default(),
        }
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
                        self.is_up_pressed = is_pressed;
                        true
                    }
                    KeyCode::KeyS | KeyCode::ArrowDown => {
                        self.is_down_pressed = is_pressed;
                        true
                    }

                    KeyCode::KeyA | KeyCode::ArrowLeft => {
                        self.is_left_pressed = is_pressed;
                        true
                    }
                    KeyCode::KeyD | KeyCode::ArrowRight => {
                        self.is_right_pressed = is_pressed;
                        true
                    }
                    KeyCode::KeyQ => {
                        self.is_q_pressed = is_pressed;
                        true
                    }
                    KeyCode::KeyE => {
                        self.is_e_pressed = is_pressed;
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
            _ => false,
        }
    }

    pub fn process_device_events(&mut self, event: &DeviceEvent) {
        match event {
            DeviceEvent::MouseMotion { delta } => {
                self.mouse_total_delta.0 += delta.0 as f32;
                self.mouse_total_delta.1 += delta.1 as f32;
            }
            _ => {}
        }
    }

    pub fn update_camera(&mut self, camera: &mut Camera, time_delta: Duration) {
        let increment = self.speed * 0.0001 * time_delta.as_micros() as f32;
        if self.is_q_pressed {
            camera.set_fovy(camera.fov_y() - increment);
        }
        if self.is_e_pressed {
            camera.set_fovy(camera.fov_y() + increment);
        }
        if self.is_up_pressed {
            camera.rotate_vertical(-increment);
        }
        if self.is_down_pressed {
            camera.rotate_vertical(increment);
        }
        if self.is_right_pressed {
            camera.rotate(-increment);
        }
        if self.is_left_pressed {
            camera.rotate(increment);
        }
        camera.sun_angle.theta += self.mouse_total_delta.0;
        camera.sun_angle.phi += self.mouse_total_delta.1;

        self.mouse_total_delta = (0.0, 0.0);

        self.events_to_process
            .drain(..)
            .for_each(|event| match event {
                CameraControllerEvent::ToggleViewMode => {
                    camera.view_mode = camera.view_mode.toggle();
                }
            });
    }
}
