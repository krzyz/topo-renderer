use super::{
    data::{PostprocessingUniforms, Uniforms, Vertex},
    texture::Texture,
};

pub struct Pipeline {
    pipeline: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
}

impl Pipeline {
    pub fn get_pipeline(&self) -> &wgpu::RenderPipeline {
        &self.pipeline
    }

    pub fn get_uniforms(&self) -> &wgpu::Buffer {
        &self.uniforms
    }

    pub fn get_uniform_bind_group(&self) -> &wgpu::BindGroup {
        &self.uniform_bind_group
    }

    pub fn get_postprocessing_depth_stencil_state() -> Option<wgpu::DepthStencilState> {
        Some(wgpu::DepthStencilState {
            format: Texture::DEPTH_FORMAT,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Greater,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        })
    }

    pub fn create_postprocessing_pipeline(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        texture_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let postprocessing_uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("postprocessing uniform buffer"),
            size: std::mem::size_of::<PostprocessingUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let postprocessing_uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("postprocessing uniform bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let postprocessing_uniform_bind_group =
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("postprocessing uniform bind group"),
                layout: &postprocessing_uniform_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: postprocessing_uniforms.as_entire_binding(),
                }],
            });

        let postprocessing_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Postprocessing Pipeline Layout"),
                bind_group_layouts: &[
                    texture_bind_group_layout,
                    &postprocessing_uniform_bind_group_layout,
                ],
                immediate_size: 0,
            });

        let postprocessing_shader = device.create_shader_module(wgpu::include_wgsl!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../resources/shaders/postprocessing_shader.wgsl"
        )));

        let post_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Postprocessing Pipeline"),
            layout: Some(&postprocessing_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &postprocessing_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Self::get_postprocessing_depth_stencil_state(),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            fragment: Some(wgpu::FragmentState {
                module: &postprocessing_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            multiview_mask: None,
            cache: None,
        });

        Self {
            pipeline: post_pipeline,
            uniforms: postprocessing_uniforms,
            uniform_bind_group: postprocessing_uniform_bind_group,
        }
    }
}

pub struct TerrainRenderPipeline {
    pipeline: Pipeline,
    height_map_bind_group_layout: wgpu::BindGroupLayout,
}

impl TerrainRenderPipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniform buffer"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("uniform bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let height_map_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("height map group layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("uniform bind group"),
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[&uniform_bind_group_layout, &height_map_bind_group_layout],
                immediate_size: 0,
            });

        let render_shader = device.create_shader_module(wgpu::include_wgsl!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../resources/shaders/render_shader.wgsl"
        )));

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &render_shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState {
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: Texture::DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            fragment: Some(wgpu::FragmentState {
                module: &render_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            multiview_mask: None,
            cache: None,
        });

        let pipeline = Pipeline {
            pipeline: render_pipeline,
            uniforms,
            uniform_bind_group,
        };

        Self {
            pipeline,
            height_map_bind_group_layout,
        }
    }

    pub fn get_height_map_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.height_map_bind_group_layout
    }

    pub fn get_pipeline(&self) -> &Pipeline {
        &self.pipeline
    }
}
