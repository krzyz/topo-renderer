use std::{pin::Pin, sync::Arc};

use color_eyre::Report;
use futures::channel::oneshot;
use tokio::{sync::broadcast::Receiver, task::JoinHandle};
use tokio_with_wasm::alias as tokio;
use topo_common::{GeoCoord, GeoLocation};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    error::EventLoopError,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
    window::WindowAttributes,
};

use crate::{
    control::{
        application_controllers::ApplicationControllers,
        background_runner::{BackgroundEvent, BackgroundNotification},
    },
    data::application_data::{ApplicationData, PeakLabel},
    render::{
        data::PeakInstance,
        render_engine::{RenderEngine, RenderEvent},
    },
};

#[derive(Debug, Clone)]
pub struct ApplicationSettings {
    pub backend_url: String,
}

pub enum ApplicationEvent {
    TerminateWithError(Report),
    ChangeLocation(GeoCoord),
    PeaksReady((GeoLocation, Vec<PeakInstance>)),
    PeakLabelsReady((GeoLocation, Vec<PeakLabel>)),
    RenderEvent(RenderEvent),
    LoadAdditionalFonts,
}

pub struct Application {
    engine: Option<RenderEngine>,
    controllers: ApplicationControllers,
    data: ApplicationData,
    window_attributes: WindowAttributes,
    event_loop_proxy: EventLoopProxy<ApplicationEvent>,
    surface_configured: bool,
    require_render: bool,
    receiver: Option<oneshot::Receiver<RenderEngine>>,
    resized: Option<PhysicalSize<u32>>,
}

impl Application {
    pub fn new(
        window_attributes: WindowAttributes,
        event_loop_proxy: EventLoopProxy<ApplicationEvent>,
    ) -> Self {
        let settings = Arc::new(ApplicationSettings {
            backend_url: env!("TOPO_backend_url").to_string(),
        });

        let controllers =
            ApplicationControllers::new(event_loop_proxy.clone(), Arc::clone(&settings));

        let bounds = window_attributes
            .inner_size
            .map(|s| s.to_physical(1.0).into())
            .unwrap_or((800.0, 600.0).into());
        let data = ApplicationData::new(bounds);

        Self {
            engine: None,
            controllers,
            data,
            window_attributes,
            event_loop_proxy,
            surface_configured: false,
            require_render: false,
            receiver: None,
            resized: None,
        }
    }
}

pub struct ApplicationRunner {
    event_loop: EventLoop<ApplicationEvent>,
    app: Application,
}

impl ApplicationRunner {
    pub fn new(window_attributes: WindowAttributes) -> Self {
        let mut event_loop = EventLoop::<ApplicationEvent>::with_user_event();
        let event_loop = event_loop.build().unwrap();
        let event_loop_proxy = event_loop.create_proxy();

        let app = Application::new(window_attributes, event_loop_proxy);

        Self { app, event_loop }
    }

    pub fn get_event_loop_proxy(&self) -> EventLoopProxy<ApplicationEvent> {
        self.event_loop.create_proxy()
    }

    pub fn configure_background_runner(
        &mut self,
        async_runner: impl FnOnce(Pin<Box<dyn Future<Output = ()> + Send + Sync>>) -> JoinHandle<()>,
    ) -> Result<(), Report> {
        self.app
            .controllers
            .configure_background_runner(async_runner)
    }

    pub fn subscribe_to_background_notifications(
        &mut self,
    ) -> Option<Receiver<BackgroundNotification>> {
        self.app.controllers.subscribe_to_notifications()
    }

    pub fn run(self) -> Result<(), EventLoopError> {
        let mut app = self.app;
        self.event_loop.run_app(&mut app)
    }
}

impl ApplicationHandler<ApplicationEvent> for Application {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.engine.is_some() {
            return;
        }

        let window = Arc::new(
            event_loop
                .create_window(self.window_attributes.clone())
                .unwrap(),
        );

        let event_loop_proxy = self.event_loop_proxy.clone();

        let (sender, receiver) = oneshot::channel();
        self.receiver = Some(receiver);

        let initialize_engine = async move {
            match RenderEngine::new(window, event_loop_proxy.clone()).await {
                Ok(render_engine) => {
                    if let Err(_) = sender.send(render_engine) {
                        log::error!("Unable to use render engine: sender expired");
                    }
                }
                Err(err) => {
                    log::error!("{err:?}");
                    if let Err(err) =
                        event_loop_proxy.send_event(ApplicationEvent::TerminateWithError(err))
                    {
                        log::error!("{err}");
                    }
                }
            }
        };

        #[cfg(target_arch = "wasm32")]
        tokio::spawn(initialize_engine);
        #[cfg(not(target_arch = "wasm32"))]
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(initialize_engine);
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        let Some(engine) = &mut self.engine else {
            // always check for resized as it may happen before the
            // wgpu engine gets initialized (e.g. in the browser)
            match event {
                WindowEvent::Resized(physical_size) => {
                    self.resized = Some(physical_size);
                }
                _ => (),
            }

            if let Some(ref mut receiver) = self.receiver {
                match receiver.try_recv() {
                    Ok(Some(mut engine)) => {
                        if let Some(physical_size) = self.resized.take() {
                            self.surface_configured = engine.resize(physical_size, &mut self.data);
                            engine.window().request_redraw();
                        }
                        self.engine = Some(engine);
                        self.require_render = true;
                        if let Some(engine) = self.engine.as_mut() {
                            if let Err(err) = self.controllers.ui_controller.change_location(
                                GeoCoord::new(49.35135, 20.21139),
                                &mut self.data,
                                engine,
                            ) {
                                log::error!("{err:?}");
                            }
                        }
                    }
                    Ok(None) => {
                        log::debug!("No engine received at initialization");
                    }
                    Err(err) => {
                        log::debug!("Canceled engine initialization: {err:?}");
                    }
                }
            }
            return;
        };

        if !self.controllers.input(&event) {
            match event {
                WindowEvent::Resized(physical_size) => {
                    self.surface_configured = engine.resize(physical_size, &mut self.data);
                    self.require_render = true;
                    // On macos the window needs to be redrawn manually after resizing
                    engine.window().request_redraw();
                }
                WindowEvent::RedrawRequested => {
                    engine.window().request_redraw();

                    if !self.surface_configured {
                        return;
                    }

                    if self.controllers.update(self.require_render, &mut self.data) {
                        engine.update(&mut self.data);
                        match engine.render(&self.data) {
                            Ok(require_render) => self.require_render = require_render,
                            // Reconfigure the surface if it's lost or outdated
                            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                                self.surface_configured =
                                    engine.resize(engine.size(), &mut self.data);
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
                }
                WindowEvent::CloseRequested => event_loop.exit(),
                _ => {}
            }
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        self.controllers.device_input(&event);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: ApplicationEvent) {
        let require_render = match event {
            ApplicationEvent::TerminateWithError(err) => {
                log::error!("{err:?}");
                event_loop.exit();
                false
            }
            ApplicationEvent::RenderEvent(render_event) => {
                if let Some(state) = &mut self.engine {
                    state.process_event(render_event, &mut self.data)
                } else {
                    false
                }
            }
            ApplicationEvent::ChangeLocation(location) => {
                if let Some(engine) = self.engine.as_mut() {
                    if let Err(err) = self.controllers.ui_controller.change_location(
                        location,
                        &mut self.data,
                        engine,
                    ) {
                        log::error!("{err:?}");
                    }
                    true
                } else {
                    false
                }
            }
            ApplicationEvent::PeaksReady((location, peaks)) => {
                self.data.peaks.insert(location, peaks);
                true
            }
            ApplicationEvent::PeakLabelsReady((location, labels)) => {
                self.data.peak_labels.insert(location, labels);
                true
            }
            ApplicationEvent::LoadAdditionalFonts => {
                let _ = self
                    .controllers
                    .send_event(BackgroundEvent::LoadAdditionalFonts(
                        self.data.peaks.clone(),
                    ));
                false
            }
        };

        self.require_render = self.require_render || require_render;
    }
}
