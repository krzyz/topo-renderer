use std::ops::Range;

use super::buffer::Buffer;
use super::pipeline::Pipeline;
use super::text::{LINE_HEIGHT, LabelLayout};
use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use lyon::algorithms::rounded_polygon;
use lyon::math::point;
use lyon::path::{NO_ATTRIBUTES, Path, Polygon};
use lyon::tessellation::{
    self, BuffersBuilder, FillOptions, FillTessellator, FillVertex, FillVertexConstructor,
    StrokeOptions, StrokeTessellator, StrokeVertex, StrokeVertexConstructor, VertexBuffers,
};

const SAMPLE_COUNT: u32 = 1;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct GpuVertex {
    position: [f32; 2],
    normal: [f32; 2],
    color: [f32; 3],
    z_index: i32,
}

impl GpuVertex {
    const ATTRIBS: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x2,
        2 => Float32x3,
        3 => Sint32,
    ];

    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GpuVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable, Default)]
struct Primitive {
    width: f32,
    res_width: f32,
    res_height: f32,
}

pub struct WithColor(pub Vec3);

impl FillVertexConstructor<GpuVertex> for WithColor {
    fn new_vertex(&mut self, vertex: FillVertex) -> GpuVertex {
        GpuVertex {
            position: vertex.position().to_array(),
            normal: [0.0, 0.0],
            color: self.0.into(),
            z_index: 3,
        }
    }
}

impl StrokeVertexConstructor<GpuVertex> for WithColor {
    fn new_vertex(&mut self, vertex: StrokeVertex) -> GpuVertex {
        GpuVertex {
            position: vertex.position_on_path().to_array(),
            normal: vertex.normal().to_array(),
            color: self.0.into(),
            z_index: 2,
        }
    }
}

pub struct LineRenderer {
    geometry: VertexBuffers<GpuVertex, u32>,
    geometry_range: Range<u32>,
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniforms: Primitive,
    uniform_buffer: wgpu::Buffer,
    index_buffer: Buffer,
    vertex_buffer: Buffer,
}

impl LineRenderer {
    pub fn update_resolution(&mut self, width: u32, height: u32) {
        self.uniforms.res_width = width as f32;
        self.uniforms.res_height = height as f32;
    }

    pub fn clear(&mut self) {
        self.geometry.clear();
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        laid_out_labels: Vec<LabelLayout>,
    ) {
        let lines_path = {
            let mut builder = Path::builder();
            laid_out_labels.iter().for_each(
                |&LabelLayout {
                     location: _,
                     id: _,
                     label_x,
                     label_y,
                     label_width: _,
                     peak_x,
                     peak_y,
                 }| {
                    builder.begin(point(label_x, label_y));
                    builder.line_to(point(peak_x, peak_y));
                    builder.close();
                },
            );
            builder.build()
        };

        let tolerance = 1.0;

        let mut label_background_tessellator = FillTessellator::new();

        laid_out_labels.iter().for_each(
            |&LabelLayout {
                 location: _,
                 id: _,
                 label_x,
                 label_y,
                 label_width,
                 peak_x: _,
                 peak_y: _,
             }| {
                let label_backgrounds_path = {
                    let mut builder = Path::builder();
                    let label_rect = Polygon {
                        points: &[
                            point(label_x, label_y),
                            point(label_x + label_width, label_y),
                            point(label_x + label_width, label_y + LINE_HEIGHT),
                            point(label_x, label_y + LINE_HEIGHT),
                        ],
                        closed: true,
                    };

                    rounded_polygon::add_rounded_polygon(
                        &mut builder,
                        label_rect,
                        0.2,
                        NO_ATTRIBUTES,
                    );

                    builder.build()
                };

                label_background_tessellator
                    .tessellate_path(
                        &label_backgrounds_path,
                        &FillOptions::tolerance(tolerance)
                            .with_fill_rule(tessellation::FillRule::NonZero),
                        &mut BuffersBuilder::new(
                            &mut self.geometry,
                            WithColor(Vec3::new(1.0, 1.0, 1.0)),
                        ),
                    )
                    .unwrap();
            },
        );

        let mut line_tessellator = StrokeTessellator::new();

        line_tessellator
            .tessellate_path(
                &lines_path,
                &StrokeOptions::tolerance(tolerance),
                &mut BuffersBuilder::new(&mut self.geometry, WithColor(Vec3::new(0.0, 0.0, 0.0))),
            )
            .unwrap();

        self.geometry_range = 0..self.geometry.indices.len() as u32;

        self.index_buffer.resize(
            device,
            self.geometry.indices.len() as u64 * std::mem::size_of::<u32>() as u64,
        );
        queue.write_buffer(
            &self.index_buffer.raw,
            0,
            bytemuck::cast_slice(&self.geometry.indices),
        );

        self.vertex_buffer.resize(
            device,
            self.geometry.vertices.len() as u64 * std::mem::size_of::<GpuVertex>() as u64,
        );

        queue.write_buffer(
            &self.vertex_buffer.raw,
            0,
            bytemuck::cast_slice(&self.geometry.vertices),
        );

        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&self.uniforms));
    }

    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniform buffer"),
            size: std::mem::size_of::<Primitive>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("line pipeline bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("line pipeline bind group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            bind_group_layouts: &[&bind_group_layout],
            label: Some("line pipeline layout"),
            immediate_size: 0,
        });

        let line_shader = device.create_shader_module(wgpu::include_wgsl!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../resources/shaders/line_shader.wgsl"
        )));

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("line pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &line_shader,
                entry_point: Some("vs_main"),
                buffers: &[GpuVertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &line_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                polygon_mode: wgpu::PolygonMode::Fill,
                front_face: wgpu::FrontFace::Ccw,
                strip_index_format: None,
                cull_mode: Some(wgpu::Face::Back),
                conservative: false,
                unclipped_depth: false,
            },
            depth_stencil: Pipeline::get_postprocessing_depth_stencil_state(),
            multisample: wgpu::MultisampleState {
                count: SAMPLE_COUNT,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });

        let geometry = VertexBuffers::new();
        let geometry_range = 0..0;

        let index_buffer = Buffer::new(
            device,
            "line index buffer",
            0,
            wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        );
        let vertex_buffer = Buffer::new(
            device,
            "vertex index buffer",
            0,
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );

        let uniforms = Primitive {
            width: 0.5,
            res_width: 1.0,
            res_height: 1.0,
        };

        Self {
            geometry,
            geometry_range,
            pipeline,
            bind_group,
            uniforms,
            uniform_buffer,
            index_buffer,
            vertex_buffer,
        }
    }

    pub fn render(&mut self, pass: &mut wgpu::RenderPass<'_>) {
        if self.geometry_range.end > 0 {
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.raw.slice(..));
            pass.set_index_buffer(self.index_buffer.raw.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(self.geometry_range.clone(), 0, 0..1);
        }
    }
}
