extern crate approx;

pub mod common;
pub mod render;

use bytes::Bytes;
use color_eyre::eyre::Result;
use render::state::{State, StateEvent};
use std::{fs::File, io::Write, sync::Arc};
use wasm_bindgen::prelude::*;
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
    window::{Window, WindowId},
};

fn get_tiff_from_file() -> Result<Bytes> {
    /*
    let buffer = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../resources/small.gtiff"
    ));
    */

    let buffer =
        include_bytes!("/home/krzyz/data/COP90/COP90_hh/Copernicus_DSM_30_N49_00_E020_00_DEM.tif");

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

#[derive(Debug)]
pub enum UserEvent {
    StateEvent(StateEvent),
}

struct Application {
    state: Option<State>,
    surface_configured: bool,
    event_loop_proxy: EventLoopProxy<UserEvent>,
    receiver: Option<futures::channel::oneshot::Receiver<State>>,
    resized: Option<PhysicalSize<u32>>,
}

impl ApplicationHandler<UserEvent> for Application {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        #[allow(unused_mut)]
        let window_attributes;
        #[cfg(not(target_arch = "wasm32"))]
        {
            use winit::dpi::LogicalSize;
            use winit::platform::x11::WindowAttributesExtX11;

            let (width, height) = (800, 600);
            window_attributes = Window::default_attributes()
                .with_base_size(LogicalSize::new(width as f64, height as f64))
                .with_min_inner_size(LogicalSize::new(width as f64, height as f64))
                .with_inner_size(LogicalSize::new(width as f64, height as f64));
        }
        #[cfg(target_arch = "wasm32")]
        {
            std::panic::set_hook(Box::new(console_error_panic_hook::hook));
            console_log::init().expect("could not initialize logger");

            use wasm_bindgen::JsCast;
            use winit::platform::web::WindowAttributesExtWebSys;
            let canvas = wgpu::web_sys::window()
                .unwrap()
                .document()
                .unwrap()
                .get_element_by_id("canvas")
                .unwrap()
                .dyn_into::<wgpu::web_sys::HtmlCanvasElement>()
                .unwrap();

            window_attributes = Window::default_attributes().with_canvas(Some(canvas));
        }

        let window = Arc::new(event_loop.create_window(window_attributes).unwrap());

        #[cfg(not(target_arch = "wasm32"))]
        {
            env_logger::init();
            self.state = Some(pollster::block_on(State::new(
                window,
                self.event_loop_proxy.clone(),
            )));
        }
        #[cfg(target_arch = "wasm32")]
        {
            let (sender, receiver) = futures::channel::oneshot::channel();
            let event_loop_proxy = self.event_loop_proxy.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let state = State::new(window, event_loop_proxy).await;
                sender.send(state).unwrap();
            });
            self.receiver = Some(receiver);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        if self.state.is_none() {
            if let Some(ref mut receiver) = self.receiver {
                if let Ok(Some(mut state)) = receiver.try_recv() {
                    state.window().request_redraw();
                    if let Some(physical_size) = self.resized.take() {
                        self.surface_configured = true;
                        state.resize(physical_size);
                        state.force_render = true;
                        // On macos the window needs to be redrawn manually after resizing
                        state.window().request_redraw();
                    }
                    self.state = Some(state);
                }
            }
        }

        let Some(state) = &mut self.state else {
            match event {
                WindowEvent::Resized(physical_size) => {
                    self.resized = Some(physical_size);
                }
                _ => (),
            }
            return;
        };

        if !state.input(&event) {
            match event {
                WindowEvent::Resized(physical_size) => {
                    self.surface_configured = true;
                    state.resize(physical_size);
                    state.force_render = true;
                    // On macos the window needs to be redrawn manually after resizing
                    state.window().request_redraw();
                }
                WindowEvent::RedrawRequested => {
                    state.window().request_redraw();

                    if !self.surface_configured {
                        return;
                    }
                    let changed = state.update();
                    match state.render(changed) {
                        Ok(_) => {}
                        // Reconfigure the surface if it's lost or outdated
                        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                            state.resize(state.size())
                        }
                        // The system is out of memory, we should probably quit
                        Err(wgpu::SurfaceError::OutOfMemory | wgpu::SurfaceError::Other) => {
                            log::error!("OutOfMemory");
                            event_loop.exit()
                        }

                        // This happens when the a frame takes too long to present
                        Err(wgpu::SurfaceError::Timeout) => {
                            log::warn!("Surface timeout")
                        }
                    }
                }
                WindowEvent::CloseRequested => event_loop.exit(),
                _ => {}
            };
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        self.state
            .iter_mut()
            .for_each(|state| state.device_input(&event));
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::StateEvent(event) => {
                if let Some(state) = &mut self.state {
                    state.handle_event(event);
                }
            }
        }
    }
}

#[wasm_bindgen(start)]
pub fn start() {
    let event_loop = EventLoop::<UserEvent>::with_user_event().build().unwrap();
    let event_loop_proxy = event_loop.create_proxy();
    event_loop
        .run_app(&mut Application {
            state: None,
            surface_configured: false,
            event_loop_proxy,
            receiver: None,
            resized: None,
        })
        .unwrap();
}
