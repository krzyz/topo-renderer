use geotiff::GeoTiff;

use crate::common::data::Size;

use super::{
    bound_texture_view::BoundTextureView,
    data::{PostprocessingUniforms, Uniforms},
    pipeline::Pipeline,
    render_buffer::RenderBuffer,
    texture::Texture,
};

pub enum GeoTiffUpdate<'a> {
    Old(&'a GeoTiff),
    New(&'a GeoTiff),
}

pub struct RenderEnvironment {
    first_pass_pipeline: Pipeline,
    postprocessing_pipeline: Pipeline,
    texture_view: BoundTextureView,
    render_buffer: RenderBuffer,
    format: wgpu::TextureFormat,
    target_size: Size<u32>,
}

impl RenderEnvironment {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, target_size: Size<u32>) -> Self {
        let first_pass_pipeline = Pipeline::create_first_pass_pipeline(device, format);

        let texture_view = Self::create_texture_view(device, format, target_size);

        let postprocessing_pipeline = Pipeline::create_postprocessing_pipeline(
            device,
            format,
            &texture_view.get_texture_bind_group_layout(),
        );

        Self {
            first_pass_pipeline,
            postprocessing_pipeline,
            texture_view,
            render_buffer: RenderBuffer::new(device),
            format,
            target_size,
        }
    }

    fn create_texture_view(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        target_size: Size<u32>,
    ) -> BoundTextureView {
        let render_texture = Texture::create_render_texture(
            device,
            format,
            (target_size.width, target_size.height),
            "render_texture",
        );

        let depth_texture = Texture::create_depth_texture(
            &device,
            (target_size.width, target_size.height),
            "depth_texture",
        );

        BoundTextureView::create(device, vec![render_texture, depth_texture])
    }

    fn update_texture_view(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        size: Size<u32>,
    ) {
        if self.target_size.height != size.height || self.target_size.width != size.width {
            self.texture_view = Self::create_texture_view(device, format, size);

            self.target_size = size;
        }
    }

    pub fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_size: Size<u32>,
        geotiff_update: GeoTiffUpdate,
        uniforms: &Uniforms,
        postprocessing_uniforms: &PostprocessingUniforms,
    ) {
        self.update_texture_view(device, self.format, target_size);

        queue.write_buffer(
            self.first_pass_pipeline.get_uniforms(),
            0,
            bytemuck::bytes_of(uniforms),
        );
        queue.write_buffer(
            self.postprocessing_pipeline.get_uniforms(),
            0,
            bytemuck::bytes_of(postprocessing_uniforms),
        );

        let geotiff = match geotiff_update {
            GeoTiffUpdate::New(geotiff) => Some(geotiff),
            GeoTiffUpdate::Old(geotiff) if self.render_buffer.get_num_indices() == 0 => {
                Some(geotiff)
            }
            _ => None,
        };

        if let Some(geotiff) = geotiff {
            self.render_buffer.update(device, queue, geotiff);
        }
    }

    pub fn render(
        &self,
        target: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
        viewport: Size<u32>,
    ) {
        {
            {
                let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("render.pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.texture_view.get_textures()[0].get_view(),
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.0,
                                g: 0.0,
                                b: 0.0,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.texture_view.get_textures()[1].get_view(),
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });

                render_pass.set_pipeline(self.first_pass_pipeline.get_pipeline());
                render_pass.set_bind_group(
                    0,
                    self.first_pass_pipeline.get_uniform_bind_group(),
                    &[],
                );

                render_pass.set_vertex_buffer(0, self.render_buffer.get_vertices().raw.slice(..));
                render_pass.set_index_buffer(
                    self.render_buffer.get_indices().raw.slice(..),
                    wgpu::IndexFormat::Uint32,
                );
                render_pass.draw_indexed(0..self.render_buffer.get_num_indices(), 0, 0..1);
            }

            let mut postprocessing_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("postprocessing.pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            postprocessing_pass.set_scissor_rect(0, 0, viewport.width, viewport.height);
            postprocessing_pass.set_pipeline(&self.postprocessing_pipeline.get_pipeline());
            postprocessing_pass.set_bind_group(0, Some(self.texture_view.get_bind_group()), &[]);
            postprocessing_pass.set_bind_group(
                1,
                Some(self.postprocessing_pipeline.get_uniform_bind_group()),
                &[],
            );
            postprocessing_pass.draw(0..6, 0..1);
        }
    }
}
