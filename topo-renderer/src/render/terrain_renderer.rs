use std::collections::BTreeMap;

use topo_common::GeoLocation;
use wgpu::RenderPass;
use winit::event_loop::EventLoopProxy;

use crate::{
    app::ApplicationEvent,
    common::coordinate_transform::CoordinateTransform,
    data::{Size, pad_256},
    render::{
        compute_pipeline::{
            ComputeEdgePatchesOrientation, ComputePipeline, ComputePipelineCorner,
            ComputePipelineEdge,
        },
        pipeline::TerrainRenderPipeline,
    },
};

use super::{
    bound_texture_view::BoundTextureView, buffer::Buffer, data::PostprocessingUniforms,
    data::Uniforms, pipeline::Pipeline, render_buffer::RenderBuffer, texture::Texture,
};

pub struct TerrainRenderer {
    first_pass_pipeline: TerrainRenderPipeline,
    postprocessing_pipeline: Pipeline,
    texture_view: BoundTextureView,
    postprocessing_depth_texture_view: BoundTextureView,
    render_buffers: BTreeMap<GeoLocation, RenderBuffer>,
    depth_read_buffer: Buffer,
    format: wgpu::TextureFormat,
    target_size: Size<u32>,
}

impl TerrainRenderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, target_size: Size<u32>) -> Self {
        let first_pass_pipeline = TerrainRenderPipeline::new(device, format);

        let texture_view = Self::create_texture_view(device, format, target_size);
        let postprocessing_depth_texture_view =
            Self::create_postprocessing_depth_texture_view(device, target_size);

        let postprocessing_pipeline = Pipeline::create_postprocessing_pipeline(
            device,
            format,
            &texture_view.get_texture_bind_group_layout(),
        );

        let x = pad_256(target_size.width) * target_size.height * 4;

        let depth_read_buffer = Buffer::new(
            device,
            "Depth read buffer",
            x as u64,
            wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        );

        Self {
            first_pass_pipeline,
            postprocessing_pipeline,
            texture_view,
            postprocessing_depth_texture_view,
            render_buffers: BTreeMap::new(),
            depth_read_buffer,
            format,
            target_size,
        }
    }

    pub fn get_texture_view(&self) -> &BoundTextureView {
        &self.texture_view
    }

    pub fn get_depth_read_buffer(&self) -> &Buffer {
        &self.depth_read_buffer
    }

    pub fn get_depth_read_buffer_mut(&mut self) -> &mut Buffer {
        &mut self.depth_read_buffer
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
            wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
        );

        BoundTextureView::create(device, vec![render_texture, depth_texture])
    }

    fn create_postprocessing_depth_texture_view(
        device: &wgpu::Device,
        target_size: Size<u32>,
    ) -> BoundTextureView {
        let depth_texture = Texture::create_depth_texture(
            &device,
            (target_size.width, target_size.height),
            "postprocessing_depth_texture",
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );

        BoundTextureView::create(device, vec![depth_texture])
    }

    fn update_texture_view(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        size: Size<u32>,
    ) {
        if self.target_size.height != size.height || self.target_size.width != size.width {
            self.texture_view = Self::create_texture_view(device, format, size);
            self.postprocessing_depth_texture_view =
                Self::create_postprocessing_depth_texture_view(device, size);
            self.depth_read_buffer
                .resize(device, (pad_256(size.width) * size.height * 4) as u64);

            self.target_size = size;
        }
    }

    pub fn get_postprocessing_depth_stencil(
        &'_ self,
    ) -> Option<wgpu::RenderPassDepthStencilAttachment<'_>> {
        Some(wgpu::RenderPassDepthStencilAttachment {
            view: self.postprocessing_depth_texture_view.get_textures()[0].get_view(),
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(0.0),
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        })
    }

    pub fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_size: Size<u32>,
        uniforms: &Uniforms,
        postprocessing_uniforms: &PostprocessingUniforms,
    ) {
        self.update_texture_view(device, self.format, target_size);

        queue.write_buffer(
            self.first_pass_pipeline.get_pipeline().get_uniforms(),
            0,
            bytemuck::bytes_of(uniforms),
        );
        queue.write_buffer(
            self.postprocessing_pipeline.get_uniforms(),
            0,
            bytemuck::bytes_of(postprocessing_uniforms),
        );
    }

    pub fn add_terrain(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        location: GeoLocation,
        height_map_data: &[u8],
        coordinate_transform: CoordinateTransform,
        size: (u32, u32),
        event_loop_proxy: EventLoopProxy<ApplicationEvent>,
    ) {
        let render_buffer = RenderBuffer::new(
            device,
            queue,
            size,
            height_map_data,
            coordinate_transform,
            &self.first_pass_pipeline,
        );

        let compute_pipeline = ComputePipeline::new(device);
        compute_pipeline.dispatch(
            device,
            queue,
            location,
            render_buffer.get_height_map_texture(),
            render_buffer.get_normal_texture(),
            render_buffer.get_uniforms(),
            size,
            event_loop_proxy.clone(),
        );

        let location_left =
            GeoLocation::from_coord(location.latitude.degree, location.longitude.degree - 1);
        let location_right =
            GeoLocation::from_coord(location.latitude.degree, location.longitude.degree + 1);
        let location_top =
            GeoLocation::from_coord(location.latitude.degree + 1, location.longitude.degree);
        let location_bottom =
            GeoLocation::from_coord(location.latitude.degree - 1, location.longitude.degree);

        let location_top_left =
            GeoLocation::from_coord(location.latitude.degree + 1, location.longitude.degree - 1);
        let location_top_right =
            GeoLocation::from_coord(location.latitude.degree + 1, location.longitude.degree + 1);
        let location_bottom_left =
            GeoLocation::from_coord(location.latitude.degree - 1, location.longitude.degree - 1);
        let location_bottom_right =
            GeoLocation::from_coord(location.latitude.degree - 1, location.longitude.degree + 1);

        let left_buffer = self.render_buffers.get(&location_left);
        let right_buffer = self.render_buffers.get(&location_right);
        let top_buffer = self.render_buffers.get(&location_top);
        let bottom_buffer = self.render_buffers.get(&location_bottom);
        let top_left_buffer = self.render_buffers.get(&location_top_left);
        let top_right_buffer = self.render_buffers.get(&location_top_right);
        let bottom_left_buffer = self.render_buffers.get(&location_bottom_left);
        let bottom_right_buffer = self.render_buffers.get(&location_bottom_right);

        let mut selected_buffers_edge = vec![];

        if let Some(left_buffer) = left_buffer {
            selected_buffers_edge.push((
                left_buffer,
                &render_buffer,
                ComputeEdgePatchesOrientation::LeftRight,
            ));
        }

        if let Some(right_buffer) = right_buffer {
            selected_buffers_edge.push((
                &render_buffer,
                right_buffer,
                ComputeEdgePatchesOrientation::LeftRight,
            ));
        }

        if let Some(top_buffer) = top_buffer {
            selected_buffers_edge.push((
                top_buffer,
                &render_buffer,
                ComputeEdgePatchesOrientation::TopBottom,
            ));
        }
        if let Some(bottom_buffer) = bottom_buffer {
            selected_buffers_edge.push((
                &render_buffer,
                bottom_buffer,
                ComputeEdgePatchesOrientation::TopBottom,
            ));
        }

        for (lt_buffer, rb_buffer, orientation) in selected_buffers_edge {
            let compute_pipeline_edge = ComputePipelineEdge::new(device, orientation);

            compute_pipeline_edge.dispatch(
                device,
                queue,
                location,
                lt_buffer.get_height_map_texture(),
                rb_buffer.get_height_map_texture(),
                lt_buffer.get_normal_texture(),
                rb_buffer.get_normal_texture(),
                render_buffer.get_uniforms(),
                size,
                event_loop_proxy.clone(),
            );
        }

        let mut selected_buffers_corner = vec![];

        if let (Some(top_left_buffer), Some(top_buffer), Some(left_buffer)) =
            (top_left_buffer, top_buffer, left_buffer)
        {
            selected_buffers_corner.push((
                top_left_buffer,
                top_buffer,
                left_buffer,
                &render_buffer,
            ));
        }

        if let (Some(top_buffer), Some(top_right_buffer), Some(right_buffer)) =
            (top_buffer, top_right_buffer, right_buffer)
        {
            selected_buffers_corner.push((
                top_buffer,
                top_right_buffer,
                &render_buffer,
                right_buffer,
            ));
        }

        if let (Some(left_buffer), Some(bottom_left_buffer), Some(bottom_buffer)) =
            (left_buffer, bottom_left_buffer, bottom_buffer)
        {
            selected_buffers_corner.push((
                left_buffer,
                &render_buffer,
                bottom_left_buffer,
                bottom_buffer,
            ));
        }

        if let (Some(right_buffer), Some(bottom_buffer), Some(bottom_right_buffer)) =
            (right_buffer, bottom_buffer, bottom_right_buffer)
        {
            selected_buffers_corner.push((
                &render_buffer,
                right_buffer,
                bottom_buffer,
                bottom_right_buffer,
            ));
        }

        for (selected_top_left, selected_top_right, selected_bottom_left, selected_bottom_right) in
            selected_buffers_corner
        {
            let compute_pipeline_corner = ComputePipelineCorner::new(device);

            compute_pipeline_corner.dispatch(
                device,
                queue,
                location,
                selected_top_left.get_height_map_texture(),
                selected_top_right.get_height_map_texture(),
                selected_bottom_left.get_height_map_texture(),
                selected_bottom_right.get_height_map_texture(),
                selected_top_left.get_normal_texture(),
                selected_top_right.get_normal_texture(),
                selected_bottom_left.get_normal_texture(),
                selected_bottom_right.get_normal_texture(),
                render_buffer.get_uniforms(),
                event_loop_proxy.clone(),
            );
        }

        self.render_buffers.insert(location, render_buffer);
    }

    pub fn get_render_buffer_mut_with_pipeline(
        &mut self,
        location: &GeoLocation,
    ) -> Option<(&mut RenderBuffer, &mut TerrainRenderPipeline)> {
        self.render_buffers
            .get_mut(&location)
            .map(|buffer| (buffer, &mut self.first_pass_pipeline))
    }

    pub fn unload_terrain(&mut self, location: &GeoLocation) {
        self.render_buffers.remove(&location);
    }

    pub fn render<'a>(
        &self,
        target: &wgpu::TextureView,
        encoder: &'a mut wgpu::CommandEncoder,
        viewport: Size<u32>,
    ) -> Box<RenderPass<'a>> {
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
                                g: 0.71,
                                b: 0.885,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
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
                    multiview_mask: None,
                });

                let pipeline = self.first_pass_pipeline.get_pipeline();

                render_pass.set_pipeline(pipeline.get_pipeline());
                render_pass.set_bind_group(0, pipeline.get_uniform_bind_group(), &[]);

                self.render_buffers.iter().for_each(|(_, render_buffer)| {
                    render_pass.set_vertex_buffer(0, render_buffer.get_vertices().raw.slice(..));
                    render_pass.set_index_buffer(
                        render_buffer.get_indices().raw.slice(..),
                        wgpu::IndexFormat::Uint32,
                    );
                    render_pass.set_bind_group(
                        1,
                        render_buffer.get_height_map_texture_bind_group(),
                        &[],
                    );

                    render_pass.draw_indexed(0..(render_buffer.get_indices_len() as u32), 0, 0..1);
                });
            }

            let mut postprocessing_pass =
                Box::new(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("postprocessing.pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: target,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: self.get_postprocessing_depth_stencil(),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                }));

            postprocessing_pass.set_scissor_rect(0, 0, viewport.width, viewport.height);
            postprocessing_pass.set_pipeline(&self.postprocessing_pipeline.get_pipeline());
            postprocessing_pass.set_bind_group(0, Some(self.texture_view.get_bind_group()), &[]);
            postprocessing_pass.set_bind_group(
                1,
                Some(self.postprocessing_pipeline.get_uniform_bind_group()),
                &[],
            );
            postprocessing_pass.draw(0..6, 0..1);
            postprocessing_pass
        }
    }
}
