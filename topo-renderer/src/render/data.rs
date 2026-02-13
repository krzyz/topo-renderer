use glam::{Mat3, Mat4, Vec2, Vec3, Vec4};

use crate::{
    common::coordinate_transform::CoordinateTransform,
    data::{Size, camera::Camera},
};

#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Vertex {
    pub position: [u32; 2],
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![
        // position
        0 => Uint32x2,
    ];

    pub fn new((x, y): (u32, u32)) -> Self {
        Self { position: [x, y] }
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
    camera_proj: Mat4,
    normal_proj: Mat4,
    camera_pos: Vec4,
    pub sun_direction: Vec3,
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

#[derive(Debug, Clone)]
pub struct PeakInstance {
    pub position: Vec3,
    pub name: String,
    pub visible: bool,
}

impl PeakInstance {
    pub fn new(position: Vec3, name: String) -> Self {
        Self {
            position,
            name,
            visible: false,
        }
    }
}

#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct TerrainUniforms {
    raster_point: Vec2,
    model_point: Vec2,
    pixel_scale: Vec2,
    size: Vec2,
    normal_to_world_rot: Mat3,
    padding: Vec3,
}

impl TerrainUniforms {
    pub fn new(coordinate_transform: CoordinateTransform, (width, height): (u32, u32)) -> Self {
        let latitude = coordinate_transform.model_point.1;
        let longitude = coordinate_transform.model_point.0;

        let normal_to_world_rot = Mat3::from_euler(
            glam::EulerRot::XYZEx,
            (90.0 - latitude).to_radians(),
            0.0,
            (longitude).to_radians(),
        );

        Self {
            raster_point: Vec2::new(
                coordinate_transform.raster_point.0,
                coordinate_transform.raster_point.1,
            ),
            model_point: Vec2::new(
                coordinate_transform.model_point.0,
                coordinate_transform.model_point.1,
            ),
            pixel_scale: Vec2::new(
                coordinate_transform.pixel_scale.0,
                coordinate_transform.pixel_scale.1,
            ),
            size: Vec2::new(width as f32, height as f32),
            normal_to_world_rot,
            padding: Vec3::ZERO,
        }
    }
}
