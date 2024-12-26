pub mod common;
pub mod render;

use anyhow::Result;
use bytes::Bytes;
use geotiff::GeoTiff;
use glam::Vec3;
use render::{
    camera::Camera,
    data::{PostprocessingUniforms, Uniforms},
    render_environment::{GeoTiffUpdate, RenderEnvironment},
};
use std::{
    fs::File,
    io::{Cursor, Write},
    iter,
};
use wasm_bindgen::prelude::*;
use winit::{
    event::{Event, WindowEvent},
    event_loop::EventLoop,
    window::Window,
};

fn get_tiff_from_file() -> Result<Bytes> {
    let buffer = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/resources/small.gtiff"
    ));

    Ok(Bytes::from(buffer.as_slice()))
}

pub async fn get_tiff_from_http() -> Result<Bytes> {
    let api_key = "<snip>";

    Ok(reqwest::get(format!("https://portal.opentopography.org/API/globaldem?demtype=NASADEM&south=49.106&north=49.38&west=19.66&east=20.2&outputFormat=GTiff&API_Key={api_key}"))
        .await?.bytes().await?)
}

pub async fn write_tiff_from_http() -> Result<()> {
    let tiff_bytes = get_tiff_from_http().await?;
    let mut file = File::create("small.tiff")?;
    file.write_all(&tiff_bytes)?;
    Ok(())
}

struct State<'a> {
    surface: wgpu::Surface<'a>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: winit::dpi::PhysicalSize<u32>,
    camera: Camera,
    gtiff: GeoTiff,
    uniforms: Uniforms,
    postprocessing_uniforms: PostprocessingUniforms,
    render_environment: RenderEnvironment,
    window: &'a Window,
}

impl<'a> State<'a> {
    async fn new(window: &'a Window) -> State<'a> {
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
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
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
        let render_environment = RenderEnvironment::new(&device, surface_format, size.into());
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

    fn update_size(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.size = new_size;
        let bounds = (new_size.width as f32, new_size.height as f32).into();
        self.uniforms = self.uniforms.with_changed_bounds(&self.camera, bounds);
        self.postprocessing_uniforms = self.postprocessing_uniforms.with_new_viewport(bounds);
    }

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
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

    fn update(&mut self) {
        self.render_environment.update(
            &self.device,
            &self.queue,
            self.size.into(),
            GeoTiffUpdate::Old(&self.gtiff),
            &self.uniforms,
            &self.postprocessing_uniforms,
        )
    }

    fn render(&mut self) -> std::result::Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

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

async fn run(event_loop: EventLoop<()>, window: Window) {
    let mut state = State::new(&window).await;
    let mut surface_configured = false;

    event_loop
        .run(move |event, control_flow| {
            if let Event::WindowEvent {
                window_id: _,
                event,
            } = event
            {
                match event {
                    WindowEvent::Resized(physical_size) => {
                        surface_configured = true;
                        state.resize(physical_size);
                        // On macos the window needs to be redrawn manually after resizing
                        state.window().request_redraw();
                    }
                    WindowEvent::RedrawRequested => {
                        state.window().request_redraw();

                        if !surface_configured {
                            return;
                        }
                        state.update();
                        match state.render() {
                            Ok(_) => {}
                            // Reconfigure the surface if it's lost or outdated
                            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                                state.resize(state.size)
                            }
                            // The system is out of memory, we should probably quit
                            Err(wgpu::SurfaceError::OutOfMemory) => {
                                log::error!("OutOfMemory");
                                control_flow.exit();
                            }

                            // This happens when the a frame takes too long to present
                            Err(wgpu::SurfaceError::Timeout) => {
                                log::warn!("Surface timeout")
                            }
                        }
                    }
                    WindowEvent::CloseRequested => control_flow.exit(),
                    _ => {}
                };
            }
        })
        .unwrap();
}

#[wasm_bindgen(start)]
pub fn start() {
    let event_loop = EventLoop::new().unwrap();
    #[allow(unused_mut)]
    let mut builder = winit::window::WindowBuilder::new();
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        use winit::platform::web::WindowBuilderExtWebSys;
        let canvas = web_sys::window()
            .unwrap()
            .document()
            .unwrap()
            .get_element_by_id("canvas")
            .unwrap()
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .unwrap();
        builder = builder.with_canvas(Some(canvas));
    }
    let window = builder.build(&event_loop).unwrap();

    #[cfg(not(target_arch = "wasm32"))]
    {
        env_logger::init();
        pollster::block_on(run(event_loop, window));
    }
    #[cfg(target_arch = "wasm32")]
    {
        std::panic::set_hook(Box::new(console_error_panic_hook::hook));
        console_log::init().expect("could not initialize logger");
        wasm_bindgen_futures::spawn_local(run(event_loop, window));
    }
}
