use crate::common::data::{pad_256, Size};
use crate::render::geometry::transform;
use crate::render::peaks::Peak;
use crate::{get_tiff_from_file, UserEvent};

use super::camera::Camera;
use super::camera_controller::CameraController;
use super::data::{PostprocessingUniforms, Uniforms};
use super::render_environment::{GeoTiffUpdate, RenderEnvironment};
use super::text::TextState;
use bytes::Buf;
use geotiff::GeoTiff;
use glam::Vec3;
use itertools::Itertools;
use log::debug;
use std::f32::consts::PI;
use std::io::Cursor;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Instant;
use wgpu::{TexelCopyBufferInfo, TexelCopyBufferLayout};
use winit::dpi::PhysicalSize;
use winit::event::{DeviceEvent, WindowEvent};
use winit::event_loop::EventLoopProxy;
use winit::window::Window;

// This structure holds settings that if changed
// require a recalculation of depth buffer to adjust visible peaks
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct DepthState {
    size: Size<u32>,
    camera: Camera,
}

#[derive(Debug)]
pub enum StateEvent {
    FrameFinished(DepthState),
}

pub enum Message {
    DepthBufferReady(DepthState),
}

#[derive(Clone)]
pub struct PeakInstance {
    pub position: Vec3,
    pub name: String,
    pub visible: bool,
}

impl PeakInstance {
    pub fn new(position: Vec3, name: String) -> Self {
        Self {
            position,
            name,
            visible: false,
        }
    }
}

pub struct State {
    event_loop_proxy: EventLoopProxy<UserEvent>,
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
    text_state: TextState,
    window: Arc<Window>,
    prev_instant: Instant,
    sender: Sender<Message>,
    receiver: Receiver<Message>,
    depth_state: Option<DepthState>,
}

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("State").finish()
    }
}

impl State {
    pub async fn new(window: Arc<Window>, event_loop_proxy: EventLoopProxy<UserEvent>) -> State {
        let (sender, receiver) = channel();
        let size = window.inner_size();
        let scale_factor = window.scale_factor();

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

        let mut text_state = TextState::new(
            &device,
            &queue,
            &config,
            (
                size.width as f32 * scale_factor as f32,
                size.height as f32 * scale_factor as f32,
            )
                .into(),
        );

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
                        PeakInstance::new(
                            transform(h, p.longitude, p.latitude, lambda_0 as f32, phi_0 as f32),
                            p.name,
                        )
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

        text_state.prepare_peak_labels(&peaks);

        Self {
            event_loop_proxy,
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
            text_state,
            window,
            prev_instant,
            sender,
            receiver,
            depth_state: None,
        }
    }

    pub fn window(&self) -> &Window {
        &self.window
    }

    pub fn size(&self) -> PhysicalSize<u32> {
        self.size
    }

    pub fn new_depth_state(&self) -> DepthState {
        DepthState {
            size: self.size.into(),
            camera: self.camera,
        }
    }

    pub fn update_size(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.surface.configure(&self.device, &self.config);
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

            self.text_state.viewport.update(
                &self.queue,
                glyphon::Resolution {
                    width: self.config.width,
                    height: self.config.height,
                },
            );

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
                Message::DepthBufferReady(depth_state) => {
                    if depth_state.size == self.size.into() {
                        let peak_labels = {
                            self.depth_state = Some(depth_state);
                            let depth_buffer = self
                                .render_environment
                                .get_depth_read_buffer()
                                .raw
                                .slice(..)
                                .get_mapped_range();
                            let projection = depth_state.camera.build_view_proj_matrix(
                                depth_state.size.width as f32,
                                depth_state.size.height as f32,
                            );

                            self.peaks
                                .iter_mut()
                                .enumerate()
                                .map(|(i, peak)| {
                                    let projected_point = projection.project_point3(peak.position);
                                    if projected_point.x > -1.0
                                        && projected_point.x < 1.0
                                        && projected_point.y > -1.0
                                        && projected_point.y < 1.0
                                    {
                                        let (x_pos, y_pos) = (
                                            (0.5 * (projected_point.x + 1.0)
                                                * self.size.width as f32)
                                                as u32,
                                            (-0.5
                                                * (projected_point.y - 1.0)
                                                * self.size.height as f32)
                                                as u32,
                                        );

                                        let pos = (x_pos * 4
                                            + y_pos * pad_256(depth_state.size.width * 4))
                                            as usize;

                                        let depth_value = depth_buffer
                                            .get(pos..pos + 4)
                                            .expect("Failed depth buffer lookup")
                                            .get_f32_le();

                                        /*
                                        debug!(
                                            "Projected point {}: {projected_point}, pos: ({}, {})",
                                            peak.name, x_pos, y_pos
                                        );
                                        debug!("depth value: {:.16}", depth_value);
                                        */

                                        if projected_point.z < 1.000001 * depth_value {
                                            peak.visible = true;
                                            //debug!("visible");
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
                                .filter_map(|(i, _, vis_pos)| vis_pos.map(|pos| (i as u32, pos)))
                                .collect::<Vec<_>>()
                        };

                        self.text_state
                            .prepare(&self.device, &self.queue, peak_labels);
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

        let mut copying_depth_texture = false;
        {
            let mut pass = self
                .render_environment
                .render(&view, &mut encoder, self.size.into());
            self.text_state.render(&mut pass);
        }

        if !self.render_environment.get_depth_read_buffer().mapped
            && (self
                .depth_state
                .is_none_or(|depth_state| depth_state != self.new_depth_state()))
        {
            debug!("Copying");
            copying_depth_texture = true;
            let depth_texture = self
                .render_environment
                .get_texture_view()
                .get_textures()
                .get(1)
                .expect("missing depth texture")
                .get_texture();

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
        self.text_state.atlas.trim();

        if copying_depth_texture {
            let event_loop_proxy = self.event_loop_proxy.clone();
            let new_depth_state = self.new_depth_state();
            self.queue.on_submitted_work_done(move || {
                event_loop_proxy
                    .send_event(UserEvent::StateEvent(StateEvent::FrameFinished(
                        new_depth_state,
                    )))
                    .unwrap();
            });
        }

        Ok(())
    }

    pub fn input(&mut self, event: &WindowEvent) -> bool {
        self.camera_controller.process_events(event)
    }

    pub fn device_input(&mut self, event: &DeviceEvent) {
        self.camera_controller.process_device_events(event)
    }

    pub fn handle_event(&mut self, event: StateEvent) {
        match event {
            StateEvent::FrameFinished(new_depth_state) => {
                debug!("Render map");
                self.render_environment
                    .get_depth_read_buffer_mut()
                    .map(self.sender.clone(), new_depth_state);
            }
        }
    }
}
