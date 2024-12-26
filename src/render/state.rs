use crate::get_tiff_from_file;

use super::camera::Camera;
use super::data::{PostprocessingUniforms, Uniforms};
use super::render_environment::{GeoTiffUpdate, RenderEnvironment};
use geotiff::GeoTiff;
use glam::Vec3;
use std::{io::Cursor, iter};
use winit::dpi::PhysicalSize;
use winit::window::Window;

pub struct State<'a> {
    surface: wgpu::Surface<'a>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: PhysicalSize<u32>,
    camera: Camera,
    gtiff: GeoTiff,
    uniforms: Uniforms,
    postprocessing_uniforms: PostprocessingUniforms,
    render_environment: RenderEnvironment,
    window: &'a Window,
}

impl<'a> State<'a> {
    pub async fn new(window: &'a Window) -> State<'a> {
        let size = window.inner_size();

        // The instance is a handle to our GPU
        // BackendBit::PRIMARY => Vulkan + Metal + DX12 + Browser WebGPU
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            #[cfg(not(target_arch = "wasm32"))]
            backends: wgpu::Backends::PRIMARY,
            #[cfg(target_arch = "wasm32")]
            backends: wgpu::Backends::GL,
            ..Default::default()
        });
        let surface = instance.create_surface(window).unwrap();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: Default::default(),
                },
                // Some(&std::path::Path::new("trace")), // Trace path
                None, // Trace path
            )
            .await
            .unwrap();

        let surface_caps = surface.get_capabilities(&adapter);
        // Shader code in this tutorial assumes an Srgb surface texture. Using a different
        // one will result all the colors comming out darker. If you want to support non
        // Srgb surfaces, you'll need to account for that when drawing to the frame.
        let format = surface_caps.formats[0];

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width,
            height: size.height,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![format.add_srgb_suffix()],
            desired_maximum_frame_latency: 2,
        };

        let mut camera = Camera::default();
        camera.set_eye(Vec3::new(10000.0, 5000.0, 10000.0));

        let gtiff = GeoTiff::read(Cursor::new(get_tiff_from_file().unwrap().as_ref())).unwrap();

        let pixelize_n = 100.0;
        let center_coord = gtiff.model_extent().center();
        let bounds = (size.width as f32, size.height as f32).into();
        let uniforms = Uniforms::new(
            &camera,
            bounds,
            center_coord.x as f32,
            center_coord.y as f32,
        );
        let postprocessing_uniforms = PostprocessingUniforms::new(bounds, pixelize_n);

        log::warn!("Creating render_environment, size: {size:#?}");
        let render_environment =
            RenderEnvironment::new(&device, format.add_srgb_suffix(), size.into());
        log::warn!("Created render_environment");

        Self {
            surface,
            device,
            queue,
            config,
            size,
            camera,
            gtiff,
            uniforms,
            postprocessing_uniforms,
            render_environment,
            window,
        }
    }

    pub fn window(&self) -> &Window {
        &self.window
    }

    pub fn size(&self) -> PhysicalSize<u32> {
        self.size
    }

    pub fn update_size(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.size = new_size;
        let bounds = (new_size.width as f32, new_size.height as f32).into();
        self.uniforms = self.uniforms.with_changed_bounds(&self.camera, bounds);
        self.postprocessing_uniforms = self.postprocessing_uniforms.with_new_viewport(bounds);
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.update_size(new_size);
            self.surface.configure(&self.device, &self.config);
            self.render_environment.update(
                &self.device,
                &self.queue,
                new_size.into(),
                GeoTiffUpdate::Old(&self.gtiff),
                &self.uniforms,
                &self.postprocessing_uniforms,
            );
        }
    }

    pub fn update(&mut self) {
        self.render_environment.update(
            &self.device,
            &self.queue,
            self.size.into(),
            GeoTiffUpdate::Old(&self.gtiff),
            &self.uniforms,
            &self.postprocessing_uniforms,
        )
    }

    pub fn render(&mut self) -> std::result::Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(self.config.format.add_srgb_suffix()),
            ..Default::default()
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        self.render_environment
            .render(&view, &mut encoder, self.size.into());

        self.queue.submit(iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}
