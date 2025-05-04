use crate::get_tiff_from_file;

use super::camera::Camera;
use super::data::{PostprocessingUniforms, Uniforms};
use super::render_environment::{GeoTiffUpdate, RenderEnvironment};
use geotiff::GeoTiff;
use glam::Vec3;
use std::f32::consts::PI;
use std::time::{Duration, Instant};
use std::{io::Cursor, iter};
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::Window;

struct CameraController {
    speed: f32,
    is_up_pressed: bool,
    is_down_pressed: bool,
    is_left_pressed: bool,
    is_right_pressed: bool,
}

impl CameraController {
    fn new(speed: f32) -> Self {
        Self {
            speed,
            is_up_pressed: false,
            is_down_pressed: false,
            is_left_pressed: false,
            is_right_pressed: false,
        }
    }

    fn process_events(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        state,
                        physical_key: PhysicalKey::Code(keycode),
                        ..
                    },
                ..
            } => {
                let is_pressed = *state == ElementState::Pressed;
                match keycode {
                    KeyCode::KeyW | KeyCode::ArrowUp => {
                        self.is_up_pressed = is_pressed;
                        true
                    }
                    KeyCode::KeyS | KeyCode::ArrowDown => {
                        self.is_down_pressed = is_pressed;
                        true
                    }

                    KeyCode::KeyA | KeyCode::ArrowLeft => {
                        self.is_left_pressed = is_pressed;
                        true
                    }
                    KeyCode::KeyD | KeyCode::ArrowRight => {
                        self.is_right_pressed = is_pressed;
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    fn update_camera(&self, camera: &mut Camera, time_delta: Duration) {
        let increment = self.speed * 0.0001 * time_delta.as_micros() as f32;
        if self.is_up_pressed {
            camera.set_fovy(camera.fov_y() - increment);
        }
        if self.is_down_pressed {
            camera.set_fovy(camera.fov_y() + increment);
        }
        if self.is_right_pressed {
            camera.rotate(-increment);
        }
        if self.is_left_pressed {
            camera.rotate(increment);
        }
    }
}

pub struct State<'a> {
    surface: wgpu::Surface<'a>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: PhysicalSize<u32>,
    camera: Camera,
    camera_controller: CameraController,
    gtiff: GeoTiff,
    uniforms: Uniforms,
    postprocessing_uniforms: PostprocessingUniforms,
    render_environment: RenderEnvironment,
    window: &'a Window,
    prev_instant: Instant,
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
        camera.set_eye(Vec3::new(0.0, 10.0, 0.0));
        camera.set_direction(0.75 * PI);
        let camera_controller = CameraController::new(0.01);

        let gtiff = GeoTiff::read(Cursor::new(get_tiff_from_file().unwrap().as_ref())).unwrap();

        let pixelize_n = 100.0;
        let center_coord = gtiff.model_extent().center();
        println!("Center coord: {center_coord:#?}");
        let lambda_0 = 20.13715;
        let phi_0 = 49.36991;

        let h = gtiff.get_value_at(&(lambda_0, phi_0).into(), 0).unwrap();
        let bounds = (size.width as f32, size.height as f32).into();
        let uniforms = Uniforms::new(&camera, bounds, lambda_0 as f32, phi_0 as f32, h);
        let postprocessing_uniforms = PostprocessingUniforms::new(bounds, pixelize_n);

        let render_environment =
            RenderEnvironment::new(&device, format.add_srgb_suffix(), size.into());

        let prev_instant = Instant::now();

        Self {
            surface,
            device,
            queue,
            config,
            size,
            camera,
            camera_controller,
            gtiff,
            uniforms,
            postprocessing_uniforms,
            render_environment,
            window,
            prev_instant,
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
        self.uniforms = self.uniforms.update_projection(&self.camera, bounds);
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
        let current_instant = Instant::now();
        let time_delta = current_instant - self.prev_instant;
        println!("duration: {time_delta:#?}");
        self.prev_instant = current_instant;

        let bounds = (self.size.width as f32, self.size.height as f32).into();
        self.uniforms = self.uniforms.update_projection(&self.camera, bounds);
        self.camera_controller
            .update_camera(&mut self.camera, time_delta);
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

    pub fn input(&mut self, event: &WindowEvent) -> bool {
        self.camera_controller.process_events(event)
    }
}
