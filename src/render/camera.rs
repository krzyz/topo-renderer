use glam::{mat4, vec3, vec4, Vec3};

#[derive(Copy, Clone)]
pub struct Camera {
    eye: glam::Vec3,
    target: glam::Vec3,
    up: glam::Vec3,
    fov_y: f32,
    near: f32,
    far: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            eye: Self::DEFAULT_POSITION,
            target: vec3(0.0, 10.0, 0.0),
            up: glam::Vec3::Y,
            fov_y: 45.0,
            near: 10.0,
            far: 1000000.0,
        }
    }
}

pub const OPENGL_TO_WGPU_MATRIX: glam::Mat4 = mat4(
    vec4(1.0, 0.0, 0.0, 0.0),
    vec4(0.0, 1.0, 0.0, 0.0),
    vec4(0.0, 0.0, 0.5, 0.0),
    vec4(0.0, 0.0, 0.5, 1.0),
);

impl Camera {
    pub const DEFAULT_POSITION: Vec3 = vec3(0.0, 1500.0, 0.0);

    pub fn get_view(&self) -> glam::Mat4 {
        glam::Mat4::look_at_rh(self.eye, self.target, self.up)
    }

    pub fn build_view_proj_matrix(&self, width: f32, height: f32) -> glam::Mat4 {
        //TODO looks distorted without padding; base on surface texture size instead?
        //let aspect_ratio = bounds.width / (bounds.height + 150.0);
        let aspect_ratio = width / height;

        let proj = glam::Mat4::perspective_rh(self.fov_y, aspect_ratio, self.near, self.far);

        OPENGL_TO_WGPU_MATRIX * proj * self.get_view()
    }

    pub fn build_view_normal_matrix(&self) -> glam::Mat4 {
        self.get_view().inverse().transpose()
    }

    pub fn position(&self) -> glam::Vec4 {
        glam::Vec4::from((self.eye, 0.0))
    }

    pub fn set_eye(&mut self, eye: glam::Vec3) {
        self.eye = eye;
    }
}
