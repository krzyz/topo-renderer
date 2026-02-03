use glam::{Vec3, vec3};
use std::f32::consts::PI;
use topo_common::GeoCoord;

use crate::render::geometry::transform;

pub const NEAR: f32 = 50.0;
pub const FAR: f32 = 500000.0;

pub fn dist_from_depth(depth: f32) -> f32 {
    FAR * NEAR / (FAR - depth * (FAR - NEAR))
}

#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub enum ViewMode {
    #[default]
    Default = 0,
    Normals = 1,
    Position = 2,
}

impl ViewMode {
    pub fn toggle(&self) -> ViewMode {
        use ViewMode::*;
        match self {
            Default => Normals,
            Normals => Position,
            Position => Default,
        }
    }
}

// degrees
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct LightAngle {
    // 0 is down, around X
    pub theta: f32,
    // 0 is in direction of x, around Y
    pub phi: f32,
}

impl LightAngle {
    pub fn to_vec3(&self) -> glam::Vec3 {
        glam::Mat3::from_euler(
            glam::EulerRot::XYZ,
            PI * self.theta / 180.0,
            PI * self.phi / 180.0,
            0.0,
        ) * glam::Vec3::Z
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Camera {
    pub eye: glam::Vec3,
    pub pitch: f32,
    pub yaw: f32,
    fov_y: f32,
    near: f32,
    far: f32,
    pub view_mode: ViewMode,
    pub sun_angle: LightAngle,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            eye: Self::DEFAULT_POSITION,
            pitch: 0.0,
            yaw: 0.0,
            fov_y: 45.0,
            near: NEAR,
            far: FAR,
            view_mode: ViewMode::default(),
            // TODO: Move elsewhere as it's not a part of the camera
            sun_angle: LightAngle {
                theta: 45.0,
                phi: 0.0,
            },
        }
    }
}

impl Camera {
    pub const DEFAULT_POSITION: Vec3 = vec3(0.0, 0.0, 0.0);

    pub fn reset(&mut self, coord: GeoCoord, height: f32) {
        self.eye = transform(height, coord.latitude, coord.longitude);
    }

    pub fn up(&self) -> Vec3 {
        self.eye.normalize()
    }

    pub fn direction(&self) -> Vec3 {
        let glob_rotation = glam::Quat::from_rotation_arc(Vec3::new(0.0, -1.0, 0.0), self.up());
        let x = self.yaw.cos() * self.pitch.cos();
        let y = self.pitch.sin();
        let z = self.yaw.sin() * self.pitch.cos();
        let direction = glob_rotation * Vec3::new(x, y, z);
        direction
    }

    pub fn direction_right(&self) -> Vec3 {
        glam::Quat::from_axis_angle(self.up(), -0.5 * PI) * self.direction()
    }

    pub fn direction_down(&self) -> Vec3 {
        -self.up()
    }

    pub fn get_view(&self) -> glam::Mat4 {
        glam::Mat4::look_to_rh(self.eye, self.direction(), self.up())
    }

    pub fn build_view_proj_matrix(&self, width: f32, height: f32) -> glam::Mat4 {
        let aspect_ratio = width / height;

        let proj = glam::Mat4::perspective_rh(self.fov_y, aspect_ratio, self.near, self.far);

        proj * self.get_view()
    }

    pub fn build_view_normal_matrix(&self) -> glam::Mat4 {
        self.get_view().inverse().transpose()
    }

    pub fn position(&self) -> glam::Vec4 {
        glam::Vec4::from((self.eye, 0.0))
    }

    pub fn fov_y(&self) -> f32 {
        self.fov_y
    }

    pub fn set_eye(&mut self, eye: glam::Vec3) {
        self.eye = eye;
    }

    pub fn set_yaw(&mut self, yaw: f32) {
        self.yaw = yaw;
    }

    pub fn set_pitch(&mut self, pitch: f32) {
        self.pitch = pitch;
    }

    pub fn set_fovy(&mut self, fov: f32) {
        self.fov_y = fov;
    }

    pub fn rotate_yaw(&mut self, clockwise_rotation: f32) {
        self.set_yaw(self.yaw + clockwise_rotation);
    }

    pub fn rotate_pitch(&mut self, clockwise_rotation: f32) {
        let new_pitch = self.pitch + clockwise_rotation;
        if new_pitch <= 90.0f32.to_radians() {
            self.set_pitch(self.pitch + clockwise_rotation);
        }
    }
}
