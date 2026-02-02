use std::sync::Arc;

use color_eyre::Result;
use glam::Vec3;
use topo_common::{GeoCoord, GeoLocation};
use winit::{dpi::PhysicalSize, event_loop::EventLoopProxy, window::Window};

use crate::{
    app::ApplicationEvent,
    data::Size,
    render::{
        camera::Camera,
        data::{PostprocessingUniforms, Uniforms, Vertex},
    },
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
    event_loop_proxy: EventLoopProxy<ApplicationEvent>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: PhysicalSize<u32>,
    camera: Camera,
    uniforms: Uniforms,
    postprocessing_uniforms: PostprocessingUniforms,
    renderers: ApplicationRenderers,
}

impl RenderEngine {
    pub async fn new(
        window: Arc<Window>,
        event_loop_proxy: EventLoopProxy<ApplicationEvent>,
    ) -> Result<Self> {
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

        let mut camera = Camera::default();
        camera.set_eye(Vec3::new(0.0, 0.0, 0.0));

        let pixelize_n = 100.0;
        let bounds = (size.width as f32, size.height as f32).into();
        let uniforms = Uniforms::new(&camera, bounds);
        let postprocessing_uniforms = PostprocessingUniforms::new(bounds, pixelize_n);

        Ok(Self {
            window,
            event_loop_proxy,
            surface,
            device,
            queue,
            config,
            size,
            camera,
            uniforms,
            postprocessing_uniforms,
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

    pub fn update_size(&mut self, new_size: PhysicalSize<u32>) {
        self.surface.configure(&self.device, &self.config);
        self.size = new_size;
        let bounds = (new_size.width as f32, new_size.height as f32).into();
        self.uniforms = self.uniforms.update_projection(&self.camera, bounds);
        self.postprocessing_uniforms = self.postprocessing_uniforms.with_new_viewport(bounds);
    }

    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            // TODO: Might be a better way to do this; buffer gets touched during resize
            // so we unmap it so that there's no chance of crashing
            //self.render_environment.get_depth_read_buffer_mut().unmap();
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.update_size(new_size);

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
                &self.uniforms,
                &self.postprocessing_uniforms,
            );
        }
    }

    pub fn update(&mut self) -> Result<bool> {
        Ok(true)
    }

    pub fn render(&mut self, changed: bool) -> std::result::Result<(), wgpu::SurfaceError> {
        if !changed {
            return Ok(());
        }

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

    pub fn process_event(&mut self, event: RenderEvent) {
        use RenderEvent::*;
        match event {
            TerrainReady(location, vertices, indices) => self.renderers.terrain.add_terrain(
                &self.device,
                &self.queue,
                location,
                &vertices,
                &indices,
            ),
            ResetCamera(current_location, height) => {
                self.camera.reset(current_location, height + 10.0);
                self.uniforms = Uniforms::new(&self.camera, self.bounds());
            }
        }
    }
}
