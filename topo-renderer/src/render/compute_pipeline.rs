use topo_common::GeoLocation;
use winit::event_loop::EventLoopProxy;

use crate::{
    app::ApplicationEvent,
    render::{buffer::Buffer, render_engine::RenderEvent, texture::Texture},
};

pub struct ComputePipeline {
    pipeline: wgpu::ComputePipeline,
}

impl ComputePipeline {
    pub fn new(device: &wgpu::Device) -> Self {
        let compute_normals_shader = device.create_shader_module(wgpu::include_wgsl!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../resources/shaders/compute_normals_shader.wgsl"
        )));

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("compute normals pipeline"),
            layout: None,
            module: &compute_normals_shader,
            entry_point: Some("compute_normals"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        Self { pipeline }
    }

    pub fn dispatch(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        location: GeoLocation,
        heightmap_texture: &Texture,
        normal_texture: &Texture,
        uniforms: &Buffer,
        (width, height): (u32, u32),
        event_loop_proxy: EventLoopProxy<ApplicationEvent>,
    ) {
        let texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Compute normals texture bind group"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&heightmap_texture.get_view()),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&normal_texture.get_view()),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniforms.raw.as_entire_binding(),
                },
            ],
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        let (dispatch_width, dispatch_height) = compute_work_group_count((width, height), (16, 16));

        {
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Compute normals pass"),
                ..Default::default()
            });

            compute_pass.set_pipeline(&self.pipeline);
            compute_pass.set_bind_group(0, &texture_bind_group, &[]);
            compute_pass.dispatch_workgroups(dispatch_width, dispatch_height, 1);
        }

        encoder.on_submitted_work_done(move || {
            let _ = event_loop_proxy.send_event(ApplicationEvent::RenderEvent(
                RenderEvent::NormalsComputed(location),
            ));
        });

        queue.submit([encoder.finish()]);
    }
}

fn compute_work_group_count(
    (width, height): (u32, u32),
    (workgroup_width, workgroup_height): (u32, u32),
) -> (u32, u32) {
    let x = (width + workgroup_width - 1) / workgroup_width;
    let y = (height + workgroup_height - 1) / workgroup_height;

    (x, y)
}

#[derive(Clone, Copy, Debug)]
pub enum ComputeEdgePatchesOrientation {
    LeftRight,
    TopBottom,
}

pub struct ComputePipelineEdge {
    pipeline: wgpu::ComputePipeline,
    orientation: ComputeEdgePatchesOrientation,
}

impl ComputePipelineEdge {
    pub fn new(device: &wgpu::Device, orientation: ComputeEdgePatchesOrientation) -> Self {
        let compute_normals_shader = device.create_shader_module(wgpu::include_wgsl!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../resources/shaders/compute_normals_edge_shader.wgsl"
        )));

        let entry_point = Some(match orientation {
            ComputeEdgePatchesOrientation::LeftRight => "compute_normals_left_right",
            ComputeEdgePatchesOrientation::TopBottom => "compute_normals_top_bottom",
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("compute normals pipeline"),
            layout: None,
            module: &compute_normals_shader,
            entry_point,
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        Self {
            pipeline,
            orientation,
        }
    }

    pub fn dispatch(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        location: GeoLocation,
        heightmap_texture_left: &Texture,
        heightmap_texture_right: &Texture,
        normal_texture_left: &Texture,
        normal_texture_right: &Texture,
        uniforms: &Buffer,
        (width, height): (u32, u32),
        event_loop_proxy: EventLoopProxy<ApplicationEvent>,
    ) {
        let texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Compute normals edge texture bind group"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &heightmap_texture_left.get_view(),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(
                        &heightmap_texture_right.get_view(),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&normal_texture_left.get_view()),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&normal_texture_right.get_view()),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: uniforms.raw.as_entire_binding(),
                },
            ],
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        let (dispatch_width, dispatch_height) = compute_work_group_count((width, height), (64, 64));

        {
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Compute normals edge pass"),
                ..Default::default()
            });

            compute_pass.set_pipeline(&self.pipeline);
            compute_pass.set_bind_group(0, &texture_bind_group, &[]);
            let dispatch_size = match self.orientation {
                ComputeEdgePatchesOrientation::LeftRight => dispatch_height, // seam is vertical
                ComputeEdgePatchesOrientation::TopBottom => dispatch_width,  // seam is horizontal
            };
            compute_pass.dispatch_workgroups(dispatch_size, 1, 1);
        }

        encoder.on_submitted_work_done(move || {
            let _ = event_loop_proxy.send_event(ApplicationEvent::RenderEvent(
                RenderEvent::NormalsComputed(location),
            ));
        });

        queue.submit([encoder.finish()]);
    }
}

pub struct ComputePipelineCorner {
    pipeline: wgpu::ComputePipeline,
}

impl ComputePipelineCorner {
    pub fn new(device: &wgpu::Device) -> Self {
        let compute_normals_shader = device.create_shader_module(wgpu::include_wgsl!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../resources/shaders/compute_normals_corner_shader.wgsl"
        )));

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("compute normals pipeline"),
            layout: None,
            module: &compute_normals_shader,
            entry_point: Some("compute_normals_corner"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        Self { pipeline }
    }

    pub fn dispatch(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        location: GeoLocation,
        heightmap_texture_top_left: &Texture,
        heightmap_texture_top_right: &Texture,
        heightmap_texture_bottom_left: &Texture,
        heightmap_texture_bottom_right: &Texture,
        normal_texture_top_left: &Texture,
        normal_texture_top_right: &Texture,
        normal_texture_bottom_left: &Texture,
        normal_texture_bottom_right: &Texture,
        uniforms: &Buffer,
        event_loop_proxy: EventLoopProxy<ApplicationEvent>,
    ) {
        let texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Compute normals edge texture bind group"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &heightmap_texture_top_left.get_view(),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(
                        &heightmap_texture_top_right.get_view(),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(
                        &heightmap_texture_bottom_left.get_view(),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(
                        &heightmap_texture_bottom_right.get_view(),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(
                        &normal_texture_top_left.get_view(),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(
                        &normal_texture_top_right.get_view(),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(
                        &normal_texture_bottom_left.get_view(),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(
                        &normal_texture_bottom_right.get_view(),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: uniforms.raw.as_entire_binding(),
                },
            ],
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        {
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Compute normals corner pass"),
                ..Default::default()
            });

            compute_pass.set_pipeline(&self.pipeline);
            compute_pass.set_bind_group(0, &texture_bind_group, &[]);
            compute_pass.dispatch_workgroups(1, 1, 1);
        }

        encoder.on_submitted_work_done(move || {
            let _ = event_loop_proxy.send_event(ApplicationEvent::RenderEvent(
                RenderEvent::NormalsComputed(location),
            ));
        });

        queue.submit([encoder.finish()]);
    }
}
