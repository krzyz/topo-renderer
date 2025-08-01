#![feature(try_blocks)]
extern crate approx;

pub mod common;
pub mod render;

use color_eyre::eyre::Error;
#[cfg(target_arch = "wasm32")]
use color_eyre::eyre::{OptionExt, eyre};
use render::state::{State, StateEvent};
use std::{cell::RefCell, sync::Arc};
use tokio_with_wasm::alias as tokio;
use topo_common::GeoCoord;
use wasm_bindgen::prelude::*;
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
    window::{Window, WindowId},
};

thread_local! {
    pub static EVENT_LOOP_PROXY: RefCell<Option<EventLoopProxy<UserEvent>>> = RefCell::new(None);
    pub static ADDITIONAL_FONTS_LOADED: RefCell<bool> = RefCell::new(false);
}

#[derive(Debug)]
pub enum UserEvent {
    StateEvent(StateEvent),
}

#[derive(Debug, Clone)]
pub struct ApplicationSettings {
    backend_url: String,
}

struct Application {
    state: Option<State>,
    surface_configured: bool,
    event_loop_proxy: EventLoopProxy<UserEvent>,
    receiver: Option<futures::channel::oneshot::Receiver<State>>,
    resized: Option<PhysicalSize<u32>>,
    settings: ApplicationSettings,
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
            console_log::init_with_level(log::Level::Info).expect("could not initialize logger");

            use wasm_bindgen::JsCast;
            use winit::platform::web::WindowAttributesExtWebSys;
            match try {
                wgpu::web_sys::window()
                    .ok_or_eyre("Unable to get window")?
                    .document()
                    .ok_or_eyre("Unable to get document")?
                    .get_element_by_id("canvas")
                    .ok_or_eyre("Unable to get canvas by id \"canvas\"")?
                    .dyn_into::<wgpu::web_sys::HtmlCanvasElement>()
                    .map_err(|_| eyre!("Unable to convert canvas to HtmlCanvasElement"))?
            } {
                Ok::<_, Error>(canvas) => {
                    window_attributes = Window::default_attributes().with_canvas(Some(canvas));
                }
                Err(err) => {
                    log::error!("{err}");
                    window_attributes = Window::default_attributes();
                }
            }
        }

        let window = Arc::new(event_loop.create_window(window_attributes).unwrap());

        let event_loop_proxy = self.event_loop_proxy.clone();
        let settings = self.settings.clone();

        #[cfg(not(target_arch = "wasm32"))]
        {
            env_logger::init();

            match futures::executor::block_on(async move {
                Ok::<_, Error>(State::new(window, event_loop_proxy, settings).await?)
            }) {
                Ok(mut state) => {
                    // While there's no desktop gui, initialize to some location
                    state.set_coord_0(GeoCoord::new(49.35135, 20.21139)).ok();

                    self.state = Some(state);
                }
                Err(err) => {
                    log::error!("{err}");
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let (sender, receiver) = futures::channel::oneshot::channel();
            let future = async move {
                match State::new(window, event_loop_proxy, settings).await {
                    Ok(state) => {
                        if let Err(_) = sender.send(state) {
                            log::error!("Unable to send canvas state")
                        }
                    }
                    Err(err) => {
                        log::error!("{err}");
                    }
                }
            };
            wasm_bindgen_futures::spawn_local(future);
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
            log::debug!("Checking if state ready");
            if let Some(ref mut receiver) = self.receiver {
                log::debug!("Getting the receiver");
                match receiver.try_recv() {
                    Ok(Some(mut state)) => {
                        log::debug!("Received new state");
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
                    Ok(None) => {
                        log::debug!("None state received?");
                    }
                    Err(err) => {
                        log::debug!("canceled error: {err}");
                    }
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
                    match state.update() {
                        Ok(changed) => {
                            match state.render(changed) {
                                Ok(_) => {}
                                // Reconfigure the surface if it's lost or outdated
                                Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                                    state.resize(state.size())
                                }
                                // The system is out of memory, we should probably quit
                                Err(
                                    wgpu::SurfaceError::OutOfMemory | wgpu::SurfaceError::Other,
                                ) => {
                                    log::error!("OutOfMemory");
                                    event_loop.exit()
                                }

                                // This happens when the a frame takes too long to present
                                Err(wgpu::SurfaceError::Timeout) => {
                                    log::warn!("Surface timeout")
                                }
                            }
                        }
                        Err(err) => {
                            log::error!("{err}");
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

#[wasm_bindgen]
pub fn set_location(latitude: f32, longitude: f32) {
    EVENT_LOOP_PROXY.with_borrow_mut(|proxy| {
        if let Some(proxy) = proxy {
            proxy
                .send_event(UserEvent::StateEvent(StateEvent::ChangeLocation(
                    GeoCoord::new(latitude, longitude),
                )))
                .unwrap();
        }
    })
}

#[wasm_bindgen]
pub fn load_fonts() {
    let mut loaded_before = false;
    ADDITIONAL_FONTS_LOADED.with_borrow_mut(|loaded| {
        loaded_before = *loaded;
        *loaded = true
    });

    if !loaded_before {
        EVENT_LOOP_PROXY.with_borrow_mut(|proxy| {
            if let Some(proxy) = proxy {
                proxy
                    .send_event(UserEvent::StateEvent(StateEvent::LoadAdditionalFonts))
                    .unwrap();
            }
        })
    }
}

#[tokio::main]
pub async fn async_main() {
    let event_loop = EventLoop::<UserEvent>::with_user_event().build().unwrap();
    let event_loop_proxy = event_loop.create_proxy();

    EVENT_LOOP_PROXY.with_borrow_mut(|proxy| {
        *proxy = Some(event_loop_proxy.clone());
    });

    let settings = ApplicationSettings {
        backend_url: env!("TOPO_backend_url").to_string(),
    };

    event_loop
        .run_app(&mut Application {
            state: None,
            surface_configured: false,
            event_loop_proxy,
            receiver: None,
            resized: None,
            settings,
        })
        .unwrap();
}

#[wasm_bindgen(start)]
pub fn start() {
    async_main();
}
