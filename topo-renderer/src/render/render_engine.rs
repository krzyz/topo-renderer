use std::{collections::BTreeMap, sync::Arc};

use bytes::Buf;
use color_eyre::Result;
use glam::Mat4;
use itertools::Itertools;
use topo_common::{GeoCoord, GeoLocation};
use wgpu::{BufferView, TexelCopyBufferInfo, TexelCopyBufferLayout};
use winit::{dpi::PhysicalSize, event_loop::EventLoopProxy, window::Window};

use crate::{
    app::ApplicationEvent,
    data::{DepthState, Size, application_data::ApplicationData, camera::dist_from_depth, pad_256},
    render::{
        data::{PeakInstance, Uniforms, Vertex},
        text_renderer::LabelId,
    },
};

use super::application_renderers::ApplicationRenderers;

pub enum RenderEvent {
    TerrainReady(GeoLocation, Vec<Vertex>, Vec<u32>),
    DepthBufferReady(DepthState),
    FrameFinished(DepthState),
    ResetCamera(GeoCoord, f32),
}

/// This struct handles logic that necessarily requires access to wgpu primitives
/// and so must be done synchronously in a tight loop
pub struct RenderEngine {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: PhysicalSize<u32>,
    renderers: ApplicationRenderers,
    depth_state: Option<DepthState>,
    event_loop_proxy: EventLoopProxy<ApplicationEvent>,
}

impl RenderEngine {
    pub async fn new(
        window: Arc<Window>,
        event_loop_proxy: EventLoopProxy<ApplicationEvent>,
    ) -> Result<Self> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });
        let surface = instance.create_surface(window.clone())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: Default::default(),
                trace: wgpu::Trace::Off,
                experimental_features: Default::default(),
            })
            .await?;

        let surface_caps = surface.get_capabilities(&adapter);

        let format = {
            let mut format = surface_caps.formats[0];
            let format_srgb = format.add_srgb_suffix();
            if surface_caps.formats.contains(&format_srgb) {
                format = format_srgb;
            }
            format
        };

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width,
            height: size.height,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![format],
            desired_maximum_frame_latency: 2,
        };

        let renderers = ApplicationRenderers::new(&device, &queue, &config, format, size.into());

        Ok(Self {
            window,
            surface,
            device,
            queue,
            config,
            size,
            renderers,
            depth_state: None,
            event_loop_proxy,
        })
    }

    pub fn window(&self) -> &Window {
        &self.window
    }

    pub fn size(&self) -> PhysicalSize<u32> {
        self.size
    }

    pub fn bounds(&self) -> Size<f32> {
        (self.size.width as f32, self.size.height as f32).into()
    }

    pub fn new_depth_state(&self, data: &ApplicationData) -> DepthState {
        DepthState {
            size: self.size.into(),
            camera: data.camera,
        }
    }

    pub fn update_size(&mut self, new_size: PhysicalSize<u32>, data: &mut ApplicationData) {
        self.surface.configure(&self.device, &self.config);
        self.size = new_size;
        let bounds = (new_size.width as f32, new_size.height as f32).into();
        data.uniforms = data.uniforms.update_projection(&data.camera, bounds);
        data.postprocessing_uniforms = data.postprocessing_uniforms.with_new_viewport(bounds);
    }

    pub fn resize(&mut self, new_size: PhysicalSize<u32>, data: &mut ApplicationData) -> bool {
        if new_size.width > 0 && new_size.height > 0 {
            // TODO: Might be a better way to do this; buffer gets touched during resize
            // so we unmap it so that there's no chance of crashing
            self.renderers.terrain.get_depth_read_buffer_mut().unmap();
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.update_size(new_size, data);

            self.renderers.text.viewport.update(
                &self.queue,
                glyphon::Resolution {
                    width: self.config.width,
                    height: self.config.height,
                },
            );

            self.renderers
                .line
                .update_resolution(self.config.width, self.config.height);

            self.renderers.terrain.update(
                &self.device,
                &self.queue,
                new_size.into(),
                &data.uniforms,
                &data.postprocessing_uniforms,
            );
            true
        } else {
            false
        }
    }

    pub fn update(&mut self, data: &mut ApplicationData) {
        let size: Size<u32> = self.size.into();
        data.uniforms = data
            .uniforms
            .update_projection(&data.camera, (size.width as f32, size.height as f32).into());
        self.renderers.terrain.update(
            &self.device,
            &self.queue,
            self.size.into(),
            &data.uniforms,
            &data.postprocessing_uniforms,
        )
    }

    pub fn render(
        &mut self,
        data: &ApplicationData,
    ) -> std::result::Result<bool, wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(self.config.format),
            ..Default::default()
        });

        let mut copying_depth_texture = false;

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        {
            let mut pass = self
                .renderers
                .terrain
                .render(&view, &mut encoder, self.size.into());
            self.renderers.line.render(&mut pass);
            self.renderers.text.render(&mut pass);
        }

        let processed_depth_different_than_current = self
            .depth_state
            .is_none_or(|depth_state| depth_state != self.new_depth_state(data));

        if !self.renderers.terrain.get_depth_read_buffer().mapped
            && processed_depth_different_than_current
        {
            copying_depth_texture = true;
            let depth_texture = self
                .renderers
                .terrain
                .get_texture_view()
                .get_textures()
                .get(1)
                .expect("missing depth texture")
                .get_texture();

            let bytes_per_row_unpadded = depth_texture.width() * 4;

            let depth_read_buffer_info = TexelCopyBufferInfo {
                buffer: &self.renderers.terrain.get_depth_read_buffer().raw,
                layout: TexelCopyBufferLayout {
                    bytes_per_row: Some(pad_256(bytes_per_row_unpadded)),
                    ..Default::default()
                },
            };

            encoder.copy_texture_to_buffer(
                depth_texture.as_image_copy(),
                depth_read_buffer_info,
                depth_texture.size(),
            );
        }

        self.queue.submit(Some(encoder.finish()));
        output.present();
        self.renderers.text.atlas.trim();

        if copying_depth_texture {
            let event_loop_proxy = self.event_loop_proxy.clone();
            let new_depth_state = self.new_depth_state(data);

            self.queue.on_submitted_work_done(move || {
                event_loop_proxy
                    .send_event(ApplicationEvent::RenderEvent(RenderEvent::FrameFinished(
                        new_depth_state,
                    )))
                    .ok();
            });
        }

        Ok(processed_depth_different_than_current)
    }

    /// Returns whether scene changed and needs to be rerendered
    pub fn process_event(&mut self, event: RenderEvent, data: &mut ApplicationData) -> bool {
        use RenderEvent::*;
        match event {
            TerrainReady(location, vertices, indices) => {
                self.renderers.terrain.add_terrain(
                    &self.device,
                    &self.queue,
                    location,
                    &vertices,
                    &indices,
                );
                data.loaded_locations.insert(location);
            }
            DepthBufferReady(depth_state) => {
                let depth_buffer = self.renderers.terrain.get_depth_read_buffer();
                if depth_state.size == self.size.into() && depth_buffer.mapped {
                    let depth_buffer_view = depth_buffer.raw.slice(..).get_mapped_range();
                    let projection = depth_state.camera.build_view_proj_matrix(
                        depth_state.size.width as f32,
                        depth_state.size.height as f32,
                    );

                    self.depth_state = Some(depth_state);
                    self.renderers.line.clear();

                    let visible_labels = Self::get_visible_labels(
                        &mut data.peaks,
                        &projection,
                        self.size,
                        depth_state,
                        &depth_buffer_view,
                    );

                    let laid_out_labels = self.renderers.text.prepare(
                        &self.device,
                        &self.queue,
                        visible_labels,
                        data,
                    );

                    self.renderers
                        .line
                        .prepare(&self.device, &self.queue, laid_out_labels);
                }
                self.renderers.terrain.get_depth_read_buffer_mut().unmap();
            }
            FrameFinished(depth_state) => {
                self.renderers
                    .terrain
                    .get_depth_read_buffer_mut()
                    .map(self.event_loop_proxy.clone(), depth_state);
            }
            ResetCamera(current_location, height) => {
                data.camera.reset(current_location, height + 10.0);
                data.uniforms = Uniforms::new(&data.camera, self.bounds());
            }
        }

        true
    }

    pub fn get_visible_labels(
        peaks: &mut BTreeMap<GeoLocation, Vec<PeakInstance>>,
        projection: &Mat4,
        size: PhysicalSize<u32>,
        depth_state: DepthState,
        depth_buffer_view: &BufferView,
    ) -> BTreeMap<GeoLocation, Vec<(LabelId, (u32, u32))>> {
        let visible_labels = peaks
            .iter_mut()
            .map(|(location, peaks)| {
                let peak_labels = peaks
                    .iter_mut()
                    .enumerate()
                    .map(|(i, peak)| {
                        let projected_point = projection.project_point3(peak.position);
                        if projected_point.x > -1.0
                            && projected_point.x < 1.0
                            && projected_point.y > -1.0
                            && projected_point.y < 1.0
                            && projected_point.z < 1.0
                        {
                            let (x_pos, y_pos) = (
                                (0.5 * (projected_point.x + 1.0) * size.width as f32) as u32,
                                (-0.5 * (projected_point.y - 1.0) * size.height as f32) as u32,
                            );

                            let pos =
                                (x_pos * 4 + y_pos * pad_256(depth_state.size.width * 4)) as usize;

                            let depth_value = depth_buffer_view
                                .get(pos..pos + 4)
                                .expect("Failed depth buffer lookup")
                                .get_f32_le();

                            let terrain_distance = dist_from_depth(depth_value);
                            let peak_distance = dist_from_depth(projected_point.z);
                            if peak_distance - 10.0 < terrain_distance {
                                peak.visible = true;
                                (i, peak, Some((x_pos, y_pos)))
                            } else {
                                (i, peak, None)
                            }
                        } else {
                            (i, peak, None)
                        }
                    })
                    .update(|(_, peak, vis_pos)| match vis_pos {
                        Some(_) => peak.visible = true,
                        None => peak.visible = false,
                    })
                    .filter_map(|(i, _, vis_pos)| vis_pos.map(|pos| (LabelId(i as u32), pos)))
                    .collect::<Vec<_>>();

                (*location, peak_labels)
            })
            .collect::<BTreeMap<_, _>>();

        visible_labels
    }

    pub fn renderers_mut(&mut self) -> &mut ApplicationRenderers {
        &mut self.renderers
    }
}
