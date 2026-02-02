use crate::data::Size;

use super::camera::Camera;

#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Vertex {
    pub position: glam::Vec3,
    pub normal: glam::Vec3,
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![
        // position
        0 => Float32x3,
        // normal
        1 => Float32x3
    ];

    pub fn new(position: glam::Vec3, normal: glam::Vec3) -> Self {
        Self { position, normal }
    }

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Uniforms {
    camera_proj: glam::Mat4,
    normal_proj: glam::Mat4,
    camera_pos: glam::Vec4,
    pub sun_direction: glam::Vec3,
    pub view_mode: i32,
}

impl Uniforms {
    pub fn new(camera: &Camera, bounds: Size<f32>) -> Self {
        let camera_proj = camera.build_view_proj_matrix(bounds.width, bounds.height);
        let normal_proj = camera.build_view_normal_matrix();
        let view_mode = camera.view_mode as i32;

        let new_uniforms = Self {
            camera_proj,
            normal_proj,
            camera_pos: camera.position(),
            sun_direction: camera.sun_angle.to_vec3(),
            view_mode,
        };

        new_uniforms
    }

    pub fn update_projection(&self, camera: &Camera, bounds: Size<f32>) -> Self {
        let camera_proj = camera.build_view_proj_matrix(bounds.width, bounds.height);
        let normal_proj = camera.build_view_normal_matrix();

        Self {
            camera_proj,
            normal_proj,
            camera_pos: camera.position(),
            sun_direction: camera.sun_angle.to_vec3(),
            view_mode: camera.view_mode as i32,
        }
    }
}

#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct PostprocessingUniforms {
    viewport: [f32; 2],
    pixelize_n: f32,
    _padding: f32,
}

impl PostprocessingUniforms {
    pub fn new(viewport: Size<f32>, pixelize_n: f32) -> Self {
        Self {
            viewport: [viewport.width, viewport.height],
            pixelize_n,
            _padding: 0.0,
        }
    }

    pub fn with_new_viewport(&self, viewport: Size<f32>) -> Self {
        PostprocessingUniforms::new(viewport, self.pixelize_n)
    }
}
