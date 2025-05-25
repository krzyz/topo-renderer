use crate::common::data::{pad_256, Size};
use crate::get_tiff_from_file;
use crate::render::geometry::transform;
use crate::render::peaks::Peak;

use super::camera::Camera;
use super::data::{PostprocessingUniforms, Uniforms};
use super::render_environment::{GeoTiffUpdate, RenderEnvironment};
use bytes::Buf;
use geotiff::GeoTiff;
use glam::Vec3;
use log::debug;
use std::collections::VecDeque;
use std::f32::consts::PI;
use std::io::Cursor;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};
use wgpu::{TexelCopyBufferInfo, TexelCopyBufferLayout};
use winit::dpi::PhysicalSize;
use winit::event::{DeviceEvent, ElementState, KeyEvent, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::Window;

enum CameraControllerEvent {
    ToggleViewMode,
}

struct CameraController {
    speed: f32,
    is_up_pressed: bool,
    is_down_pressed: bool,
    is_left_pressed: bool,
    is_right_pressed: bool,
    is_e_pressed: bool,
    is_q_pressed: bool,
    mouse_total_delta: (f32, f32),
    events_to_process: VecDeque<CameraControllerEvent>,
}

impl CameraController {
    fn new(speed: f32) -> Self {
        Self {
            speed,
            is_up_pressed: false,
            is_down_pressed: false,
            is_left_pressed: false,
            is_right_pressed: false,
            is_e_pressed: false,
            is_q_pressed: false,
            mouse_total_delta: (0.0, 0.0),
            events_to_process: VecDeque::default(),
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
                    KeyCode::KeyQ => {
                        self.is_q_pressed = is_pressed;
                        true
                    }
                    KeyCode::KeyE => {
                        self.is_e_pressed = is_pressed;
                        true
                    }
                    KeyCode::KeyF if is_pressed => {
                        self.events_to_process
                            .push_front(CameraControllerEvent::ToggleViewMode);
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    fn process_device_events(&mut self, event: &DeviceEvent) {
        match event {
            DeviceEvent::MouseMotion { delta } => {
                self.mouse_total_delta.0 += delta.0 as f32;
                self.mouse_total_delta.1 += delta.1 as f32;
            }
            _ => {}
        }
    }

    fn update_camera(&mut self, camera: &mut Camera, time_delta: Duration) {
        let increment = self.speed * 0.0001 * time_delta.as_micros() as f32;
        if self.is_q_pressed {
            camera.set_fovy(camera.fov_y() - increment);
        }
        if self.is_e_pressed {
            camera.set_fovy(camera.fov_y() + increment);
        }
        if self.is_up_pressed {
            camera.rotate_vertical(-increment);
        }
        if self.is_down_pressed {
            camera.rotate_vertical(increment);
        }
        if self.is_right_pressed {
            camera.rotate(-increment);
        }
        if self.is_left_pressed {
            camera.rotate(increment);
        }
        camera.sun_angle.theta += self.mouse_total_delta.0;
        camera.sun_angle.phi += self.mouse_total_delta.1;

        self.mouse_total_delta = (0.0, 0.0);

        self.events_to_process
            .drain(..)
            .for_each(|event| match event {
                CameraControllerEvent::ToggleViewMode => {
                    camera.view_mode = camera.view_mode.toggle();
                }
            });
    }
}

pub enum Message {
    DepthBufferReady(Size<u32>),
}

#[derive(Clone, Copy)]
pub struct PeakInstance {
    pub position: Vec3,
    pub visible: bool,
}

impl PeakInstance {
    pub fn new(position: Vec3) -> Self {
        Self {
            position,
            visible: false,
        }
    }
}

pub struct State {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: PhysicalSize<u32>,
    camera: Camera,
    camera_controller: CameraController,
    gtiff: GeoTiff,
    peaks: Vec<PeakInstance>,
    uniforms: Uniforms,
    postprocessing_uniforms: PostprocessingUniforms,
    render_environment: RenderEnvironment,
    window: Arc<Window>,
    prev_instant: Instant,
    sender: Sender<Message>,
    receiver: Receiver<Message>,
}

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("State").finish()
    }
}

impl State {
    pub async fn new(window: Arc<Window>) -> State {
        let (sender, receiver) = channel();
        let size = window.inner_size();

        // The instance is a handle to our GPU
        // BackendBit::PRIMARY => Vulkan + Metal + DX12 + Browser WebGPU
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            #[cfg(not(target_arch = "wasm32"))]
            backends: wgpu::Backends::PRIMARY,
            #[cfg(target_arch = "wasm32")]
            backends: wgpu::Backends::GL,
            ..Default::default()
        });
        let surface = instance.create_surface(window.clone()).unwrap();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: Default::default(),
                trace: wgpu::Trace::Off,
            })
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
        camera.set_eye(Vec3::new(0.0, 1000.0, 0.0));
        camera.set_direction(0.75 * PI);
        let camera_controller = CameraController::new(0.01);

        let gtiff = GeoTiff::read(Cursor::new(get_tiff_from_file().unwrap().as_ref())).unwrap();

        let pixelize_n = 100.0;
        let center_coord = gtiff.model_extent().center();
        debug!("Center coord: {center_coord:#?}");
        let lambda_0: f64 = 20.13715; // longitude
        let phi_0: f64 = 49.36991; // latitude

        let peaks = Peak::read_from_lat_lon(phi_0.round() as i32, lambda_0.round() as i32)
            .expect("Unable to read peak data");

        let peaks = peaks
            .into_iter()
            .filter_map(|p| {
                gtiff
                    .get_value_at(&(p.longitude as f64, p.latitude as f64).into(), 0)
                    .map(|h| {
                        PeakInstance::new(transform(
                            h,
                            p.longitude,
                            p.latitude,
                            lambda_0 as f32,
                            phi_0 as f32,
                        ))
                    })
            })
            .collect::<Vec<_>>();

        debug!("Number of peaks: {}", peaks.len());

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
            peaks,
            uniforms,
            postprocessing_uniforms,
            render_environment,
            window,
            prev_instant,
            sender,
            receiver,
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
                &self.peaks,
                &self.uniforms,
                &self.postprocessing_uniforms,
            );
        }
    }

    pub fn update(&mut self) {
        self.device
            .poll(wgpu::PollType::Poll)
            .expect("Error polling");

        if let Some(mes) = self.receiver.try_iter().last() {
            match mes {
                Message::DepthBufferReady(size) => {
                    if size == self.size.into() {
                        let depth_buffer = self
                            .render_environment
                            .get_depth_read_buffer()
                            .raw
                            .slice(..)
                            .get_mapped_range();
                        let projection = self.camera.build_view_proj_matrix(
                            self.size.width as f32,
                            self.size.height as f32,
                        );
                        self.peaks.iter_mut().enumerate().for_each(|(i, peak)| {
                            let projected_point = projection.project_point3(peak.position);
                            if projected_point.x >= -1.0
                                && projected_point.x <= 1.0
                                && projected_point.y >= -1.0
                                && projected_point.y <= 1.0
                            {
                                let (x_pos, y_pos) = (
                                    (0.5 * (projected_point.x + 1.0) * self.size.width as f32)
                                        as u32,
                                    (0.5 * (projected_point.y + 1.0) * self.size.height as f32)
                                        as u32,
                                );

                                let pos = (x_pos * 4 + y_pos * pad_256(size.width * 4)) as usize;

                                let depth_value = depth_buffer
                                    .get(pos..pos + 4)
                                    .expect("Failed depth buffer lookup")
                                    .get_f32_le();

                                debug!("Projected point {i}: {projected_point}");
                                debug!("depth value: {:.16}", 1.0 - depth_value);

                                if 1.01 * projected_point.z >= 1.0 - depth_value {
                                    peak.visible = true;
                                    debug!("visible");
                                }
                            } else {
                                peak.visible = false;
                            }
                        });
                    } else {
                    }
                    self.render_environment.get_depth_read_buffer_mut().unmap();
                }
            }
        }

        let current_instant = Instant::now();
        let time_delta = current_instant - self.prev_instant;
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
            &self.peaks,
            &self.uniforms,
            &self.postprocessing_uniforms,
        );
    }

    pub fn render(&mut self) -> std::result::Result<(), wgpu::SurfaceError> {
        debug!("Render start");
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

        let mut depth_texture_size = None;
        self.render_environment
            .render(&view, &mut encoder, self.size.into());

        if !self.render_environment.get_depth_read_buffer().mapped {
            let depth_texture = self
                .render_environment
                .get_texture_view()
                .get_textures()
                .get(1)
                .expect("missing depth texture")
                .get_texture();

            depth_texture_size = Some(depth_texture.size());

            let bytes_per_row_unpadded = depth_texture.width() * 4;

            let depth_read_buffer_info = TexelCopyBufferInfo {
                buffer: &self.render_environment.get_depth_read_buffer().raw,
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

        if let Some(depth_texture_size) = depth_texture_size {
            debug!("Render map");
            self.render_environment.get_depth_read_buffer_mut().map(
                self.sender.clone(),
                depth_texture_size.width,
                depth_texture_size.height,
            );
        }

        debug!("Render end");

        Ok(())
    }

    pub fn input(&mut self, event: &WindowEvent) -> bool {
        self.camera_controller.process_events(event)
    }

    pub fn device_input(&mut self, event: &DeviceEvent) {
        self.camera_controller.process_device_events(event)
    }
}
