use std::sync::Arc;

use color_eyre::Result;
use topo_common::{GeoCoord, GeoLocation};
use winit::{dpi::PhysicalSize, window::Window};

use crate::{
    data::{Size, application_data::ApplicationData},
    render::data::{Uniforms, Vertex},
};

use super::application_renderers::ApplicationRenderers;

pub enum RenderEvent {
    TerrainReady(GeoLocation, Vec<Vertex>, Vec<u32>),
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
}

impl RenderEngine {
    pub async fn new(window: Arc<Window>) -> Result<Self> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            #[cfg(not(target_arch = "wasm32"))]
            backends: wgpu::Backends::PRIMARY,
            #[cfg(target_arch = "wasm32")]
            backends: wgpu::Backends::BROWSER_WEBGPU,
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
        // Shader code in this tutorial assumes an Srgb surface texture. Using a different
        // one will result all the colors comming out darker. If you want to support non
        // Srgb surfaces, you'll need to account for that when drawing to the frame.
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

        let renderers = ApplicationRenderers::new(&device, format, size.into());

        Ok(Self {
            window,
            surface,
            device,
            queue,
            config,
            size,
            renderers,
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

    pub fn update_size(&mut self, new_size: PhysicalSize<u32>, data: &mut ApplicationData) {
        self.surface.configure(&self.device, &self.config);
        log::info!("surface configured");
        self.size = new_size;
        let bounds = (new_size.width as f32, new_size.height as f32).into();
        data.uniforms = data.uniforms.update_projection(&data.camera, bounds);
        data.postprocessing_uniforms = data.postprocessing_uniforms.with_new_viewport(bounds);
    }

    pub fn resize(&mut self, new_size: PhysicalSize<u32>, data: &mut ApplicationData) -> bool {
        if new_size.width > 0 && new_size.height > 0 {
            // TODO: Might be a better way to do this; buffer gets touched during resize
            // so we unmap it so that there's no chance of crashing
            //self.render_environment.get_depth_read_buffer_mut().unmap();
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.update_size(new_size, data);

            /*
            self.text_state.viewport.update(
                &self.queue,
                glyphon::Resolution {
                    width: self.config.width,
                    height: self.config.height,
                },
            );

            self.line_renderer
                .update_resolution(self.config.width, self.config.height);
            */

            self.renderers.terrain.update(
                &self.device,
                &self.queue,
                new_size.into(),
                &data.uniforms,
                &data.postprocessing_uniforms,
            );
            true
        } else {
            log::info!("Resize with 0,0 size...");
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

    pub fn render(&mut self) -> std::result::Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(self.config.format),
            ..Default::default()
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        {
            let _pass = self
                .renderers
                .terrain
                .render(&view, &mut encoder, self.size.into());
        }

        self.queue.submit(Some(encoder.finish()));
        output.present();

        Ok(())
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
            ResetCamera(current_location, height) => {
                data.camera.reset(current_location, height + 10.0);
                data.uniforms = Uniforms::new(&data.camera, self.bounds());
            }
        }

        true
    }

    pub fn renderers_mut(&mut self) -> &mut ApplicationRenderers {
        &mut self.renderers
    }
}
