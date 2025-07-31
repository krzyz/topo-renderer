use crate::common::data::{Size, pad_256};
use crate::render::geometry::transform;
use crate::render::peaks::Peak;
use crate::render::pipeline::Pipeline;
use crate::{ADDITIONAL_FONTS_LOADED, ApplicationSettings, UserEvent};

use super::camera::Camera;
use super::camera_controller::CameraController;
use super::data::{PostprocessingUniforms, Uniforms, Vertex};
use super::geometry::R0;
use super::lines::LineRenderer;
use super::render_buffer::RenderBuffer;
use super::render_environment::RenderEnvironment;
use super::text::{Label, LabelId, TextState};
use bytes::{Buf, Bytes};
use color_eyre::Result;
use geotiff::GeoTiff;
use glam::Vec3;
use itertools::Itertools;
use log::debug;
use std::collections::BTreeMap;
use std::io::Cursor;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
use topo_common::{GeoCoord, GeoLocation};
#[cfg(target_arch = "wasm32")]
use web_time::Instant;
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
    ChangeLocation(GeoCoord),
    LoadAdditionalFonts,
}

pub enum Message {
    DepthBufferReady(DepthState),
    TerrainQueued(GeoLocation),
    TerrainReceived((GeoLocation, GeoTiff, Vec<PeakInstance>)),
    TerrainProcessed(GeoLocation, Vec<Vertex>, Vec<u32>),
    PeakLabelsPrepared(GeoLocation, Vec<Label>),
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

async fn get_tiff_from_http(backend_url: &str, location: GeoLocation) -> Result<Bytes> {
    Ok(reqwest::get(format!(
        "{backend_url}/dem?{}",
        location.to_request_params()
    ))
    .await?
    .bytes()
    .await?)
}

async fn get_peaks_from_http(backend_url: &str, location: GeoLocation) -> Result<Bytes> {
    Ok(reqwest::get(format!(
        "{backend_url}/peaks?{}",
        location.to_request_params()
    ))
    .await?
    .bytes()
    .await?)
}

pub struct State {
    event_loop_proxy: EventLoopProxy<UserEvent>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pub force_render: bool,
    size: PhysicalSize<u32>,
    camera: Camera,
    camera_controller: CameraController,
    peaks: BTreeMap<GeoLocation, Vec<PeakInstance>>,
    uniforms: Uniforms,
    postprocessing_uniforms: PostprocessingUniforms,
    render_environment: RenderEnvironment,
    text_state: TextState,
    line_renderer: LineRenderer,
    window: Arc<Window>,
    prev_instant: Instant,
    sender: Sender<Message>,
    receiver: Receiver<Message>,
    depth_state: Option<DepthState>,
    settings: ApplicationSettings,
    coord_0: Option<GeoCoord>,
}

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("State").finish()
    }
}

impl State {
    pub async fn new(
        window: Arc<Window>,
        event_loop_proxy: EventLoopProxy<UserEvent>,
        settings: ApplicationSettings,
    ) -> State {
        let (sender, receiver) = channel();
        let size = window.inner_size();
        // let scale_factor = window.scale_factor();

        // The instance is a handle to our GPU
        // BackendBit::PRIMARY => Vulkan + Metal + DX12 + Browser WebGPU
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            #[cfg(not(target_arch = "wasm32"))]
            backends: wgpu::Backends::PRIMARY,
            #[cfg(target_arch = "wasm32")]
            backends: wgpu::Backends::BROWSER_WEBGPU,
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

        let mut camera = Camera::default();
        camera.set_eye(Vec3::new(0.0, 0.0, 0.0));
        let camera_controller = CameraController::new(0.01);

        let pixelize_n = 100.0;
        let bounds = (size.width as f32, size.height as f32).into();
        let uniforms = Uniforms::new(&camera, bounds);
        let postprocessing_uniforms = PostprocessingUniforms::new(bounds, pixelize_n);

        let render_environment = RenderEnvironment::new(&device, format, size.into());

        let text_state = TextState::new(
            &device,
            &queue,
            &config,
            Pipeline::get_postprocessing_depth_stencil_state(),
        );

        let prev_instant = Instant::now();

        let mut line_renderer = LineRenderer::new(&device, format);
        line_renderer.prepare(&device, &queue, vec![]);

        debug!("Finished State::new()");
        Self {
            event_loop_proxy,
            surface,
            device,
            queue,
            config,
            force_render: true,
            size,
            camera,
            camera_controller,
            peaks: BTreeMap::new(),
            uniforms,
            postprocessing_uniforms,
            render_environment,
            text_state,
            line_renderer,
            window,
            prev_instant,
            sender,
            receiver,
            depth_state: None,
            settings,
            coord_0: None,
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
            debug!("Updating size");
            // TODO: Might be a better way to do this; buffer gets touched during resize
            // so we unmap it so that there's no chance of crashing
            self.render_environment.get_depth_read_buffer_mut().unmap();
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

            self.line_renderer
                .update_resolution(self.config.width, self.config.height);

            self.render_environment.update(
                &self.device,
                &self.queue,
                new_size.into(),
                &self.uniforms,
                &self.postprocessing_uniforms,
            );
        }
    }

    pub fn update(&mut self) -> bool {
        let mut changed = false;

        self.device
            .poll(wgpu::PollType::Poll)
            .expect("Error polling");

        let bounds = (self.size.width as f32, self.size.height as f32).into();

        let messages = self.receiver.try_iter().collect::<Vec<_>>();

        for mes in messages {
            match mes {
                Message::DepthBufferReady(depth_state) => {
                    let depth_buffer = self.render_environment.get_depth_read_buffer();
                    if depth_state.size == self.size.into() && depth_buffer.mapped {
                        let depth_buffer_view = depth_buffer.raw.slice(..).get_mapped_range();
                        let projection = depth_state.camera.build_view_proj_matrix(
                            depth_state.size.width as f32,
                            depth_state.size.height as f32,
                        );
                        self.depth_state = Some(depth_state);

                        self.line_renderer.clear();

                        let visible_labels = self
                            .peaks
                            .iter_mut()
                            .map(|(location, peaks)| {
                                let peak_labels = peaks
                                    .iter_mut()
                                    .enumerate()
                                    .map(|(i, peak)| {
                                        let projected_point =
                                            projection.project_point3(peak.position);
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

                                            let depth_value = depth_buffer_view
                                                .get(pos..pos + 4)
                                                .expect("Failed depth buffer lookup")
                                                .get_f32_le();

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
                                    .filter_map(|(i, _, vis_pos)| {
                                        vis_pos.map(|pos| (LabelId(i as u32), pos))
                                    })
                                    .collect::<Vec<_>>();

                                (*location, peak_labels)
                            })
                            .collect::<BTreeMap<_, _>>();

                        let laid_out_labels =
                            self.text_state
                                .prepare(&self.device, &self.queue, visible_labels);
                        self.line_renderer
                            .prepare(&self.device, &self.queue, laid_out_labels);
                        changed = true;
                    }
                    self.render_environment.get_depth_read_buffer_mut().unmap();
                }
                Message::TerrainQueued(location) => {
                    let backend_url = self.settings.backend_url.clone();
                    let sender = self.sender.clone();
                    let future = async move {
                        let (gtiff, peaks) =
                            Self::fetch_dem_data(&backend_url, location).await.unwrap();

                        sender
                            .send(Message::TerrainReceived((location, gtiff, peaks)))
                            .unwrap();
                    };

                    log::debug!(
                        "Spawning terrain fetch for location {:?}",
                        location.to_numerical()
                    );

                    #[cfg(not(target_arch = "wasm32"))]
                    tokio::spawn(future);
                    #[cfg(target_arch = "wasm32")]
                    wasm_bindgen_futures::spawn_local(future);

                    log::debug!(
                        "Spawned terrain fetch for location {:?}",
                        location.to_numerical()
                    );
                }
                Message::TerrainReceived((location, gtiff, peaks)) => {
                    log::debug!(
                        "Running terrain received for location {:?}",
                        location.to_numerical()
                    );
                    self.uniforms = Uniforms::new(&self.camera, bounds);

                    self.peaks.insert(location, peaks.clone());

                    if let Some(coord_0) = self.coord_0 {
                        if GeoLocation::from(coord_0) == location {
                            let height: f32 = gtiff
                                .get_value_at(&(<(f64, f64)>::from(coord_0)).into(), 0)
                                .unwrap();

                            self.camera.reset(coord_0, height + 10.0);

                            changed = true;
                        }
                    }

                    let sender = self.sender.clone();
                    let process_terrain = move || {
                        let (vertices, indices) = RenderBuffer::process_terrain(&gtiff);
                        sender
                            .send(Message::TerrainProcessed(location, vertices, indices))
                            .ok();
                    };

                    let sender = self.sender.clone();
                    let prepare_peak_labels = move || {
                        let labels = TextState::prepare_peak_labels(&peaks);
                        sender
                            .send(Message::PeakLabelsPrepared(location, labels))
                            .ok();
                    };

                    log::debug!(
                        "Spawning terrain processing for location {:?}",
                        location.to_numerical()
                    );
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        tokio::task::spawn_blocking(process_terrain);
                        tokio::task::spawn_blocking(prepare_peak_labels);
                    }
                    #[cfg(target_arch = "wasm32")]
                    {
                        process_terrain();
                        prepare_peak_labels();
                    }
                    log::debug!(
                        "Spawned terrain processing for location {:?}",
                        location.to_numerical()
                    );
                }
                Message::TerrainProcessed(location, vertices, indices) => {
                    log::debug!("Adding terrain for location {:?}", location.to_numerical());
                    self.render_environment.add_terrain(
                        &self.device,
                        &self.queue,
                        location,
                        &vertices,
                        &indices,
                    );
                    log::debug!("Added terrain for location {:?}", location.to_numerical());

                    changed = true;
                }
                Message::PeakLabelsPrepared(location, labels) => {
                    log::debug!("Adding labels for location {:?}", location.to_numerical());
                    self.text_state.add_labels(location, labels);
                    log::debug!("Added labels for location {:?}", location.to_numerical());
                    changed = true;
                }
            }
        }

        let current_instant = Instant::now();
        let time_delta = current_instant - self.prev_instant;
        self.prev_instant = current_instant;

        let camera_changed = self
            .camera_controller
            .update_camera(&mut self.camera, time_delta);
        changed = changed || camera_changed;
        self.uniforms = self.uniforms.update_projection(&self.camera, bounds);
        if changed {
            self.render_environment.update(
                &self.device,
                &self.queue,
                self.size.into(),
                &self.uniforms,
                &self.postprocessing_uniforms,
            );
        }

        changed
    }

    pub fn render(&mut self, changed: bool) -> std::result::Result<(), wgpu::SurfaceError> {
        if !(changed || self.force_render) {
            return Ok(());
        }
        self.force_render = false;
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

        let mut copying_depth_texture = false;
        {
            let mut pass = self
                .render_environment
                .render(&view, &mut encoder, self.size.into());
            self.line_renderer.render(&mut pass);
            self.text_state.render(&mut pass);
        }

        if !self.render_environment.get_depth_read_buffer().mapped
            && (self
                .depth_state
                .is_none_or(|depth_state| depth_state != self.new_depth_state()))
        {
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

            #[cfg(not(target_arch = "wasm32"))]
            self.queue.on_submitted_work_done(move || {
                event_loop_proxy
                    .send_event(UserEvent::StateEvent(StateEvent::FrameFinished(
                        new_depth_state,
                    )))
                    .ok();
            });
            #[cfg(target_arch = "wasm32")]
            event_loop_proxy
                .send_event(UserEvent::StateEvent(StateEvent::FrameFinished(
                    new_depth_state,
                )))
                .ok();
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
                self.render_environment
                    .get_depth_read_buffer_mut()
                    .map(self.sender.clone(), new_depth_state);
            }
            StateEvent::ChangeLocation(coord) => {
                self.set_coord_0(coord);
            }
            StateEvent::LoadAdditionalFonts => {
                let peaks_map = self.peaks.clone();
                let sender = self.sender.clone();
                let future = async move {
                    if TextState::load_additional_fonts().await.is_ok() {
                        for (location, peaks) in peaks_map {
                            let sender = sender.clone();
                            let prepare_peak_labels = move || {
                                let labels = TextState::prepare_peak_labels(&peaks);
                                sender
                                    .send(Message::PeakLabelsPrepared(location, labels))
                                    .ok();
                            };

                            #[cfg(not(target_arch = "wasm32"))]
                            {
                                tokio::task::spawn_blocking(prepare_peak_labels);
                            }
                            #[cfg(target_arch = "wasm32")]
                            {
                                prepare_peak_labels();
                            }
                        }
                    } else {
                        ADDITIONAL_FONTS_LOADED.with_borrow_mut(|loaded| *loaded = true);
                    };
                };

                #[cfg(not(target_arch = "wasm32"))]
                tokio::spawn(future);
                #[cfg(target_arch = "wasm32")]
                wasm_bindgen_futures::spawn_local(future);
            }
        }
    }

    async fn fetch_dem_data(
        backend_url: &str,
        location: GeoLocation,
    ) -> Result<(GeoTiff, Vec<PeakInstance>)> {
        let geotiff = GeoTiff::read(Cursor::new(
            get_tiff_from_http(backend_url, location).await?.as_ref(),
        ))?;

        let peak_bytes = get_peaks_from_http(backend_url, location).await?;

        let peaks = Peak::read_peaks(peak_bytes.reader()).expect("Unable to read peak data");

        let peaks = peaks
            .into_iter()
            .sorted_by(|a, b| {
                PartialOrd::partial_cmp(&b.elevation, &a.elevation)
                    .unwrap_or(std::cmp::Ordering::Less)
            })
            .filter_map(|p| {
                geotiff
                    .get_value_at(&(p.longitude as f64, p.latitude as f64).into(), 0)
                    .map(|h| PeakInstance::new(transform(h, p.latitude, p.longitude), p.name))
            })
            .collect::<Vec<_>>();

        Ok((geotiff, peaks))
    }

    pub fn set_coord_0(&mut self, location: GeoCoord) {
        self.coord_0 = Some(location);
        Self::get_locations_range(location, 100_000.0)
            .into_iter()
            .for_each(|to_fetch| {
                self.sender.send(Message::TerrainQueued(to_fetch)).unwrap();
            });
    }

    fn get_locations_range(location: GeoCoord, range_dist: f32) -> Vec<GeoLocation> {
        // TODO: handle projection edges (90NS/180EW deg)
        let center = (
            location.latitude.floor() as i32,
            location.longitude.floor() as i32,
        );
        let lat_cos = (location.latitude.to_radians()).cos();
        let arc_factor = 0.5 * range_dist / R0;
        let arc_factor_sin = arc_factor.sin();
        let afs_sq = arc_factor_sin * arc_factor_sin;
        let dlon = (1.0 - afs_sq / lat_cos / lat_cos).acos().to_degrees();
        let dlat = (1.0 - afs_sq).acos().to_degrees();
        let lat_start = (location.latitude - dlat).floor() as i32;
        let lat_end = (location.latitude + dlat).floor() as i32;
        let lon_start = (location.longitude - dlon).floor() as i32;
        let lon_end = (location.longitude + dlon).floor() as i32;

        (lat_start..=lat_end)
            .cartesian_product(lon_start..=lon_end)
            .sorted_by_key(|(lat, lon)| ((lat - center.0).abs(), (lon - center.1).abs()))
            .map(|(lat, lon)| GeoLocation::from_coord(lat, lon).into())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_range() {
        let locations = State::get_locations_range(GeoCoord::new(52.1, 20.1), 100_000.0);

        let expected = vec![
            GeoLocation::from_coord(52, 20),
            GeoLocation::from_coord(52, 19),
            GeoLocation::from_coord(52, 21),
            GeoLocation::from_coord(51, 20),
            GeoLocation::from_coord(51, 19),
            GeoLocation::from_coord(51, 21),
        ];

        assert_eq!(locations, expected);
    }
}
