use std::{pin::Pin, sync::Arc};

use color_eyre::Report;
use futures::channel::oneshot;
use tokio::task::JoinHandle;
use tokio_with_wasm::alias as tokio;
use topo_common::GeoCoord;
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    error::EventLoopError,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
    window::WindowAttributes,
};

#[cfg(not(target_arch = "wasm32"))]
use winit::platform::wayland::EventLoopBuilderExtWayland;

use crate::{
    control::application_controllers::ApplicationControllers,
    data::application_data::ApplicationData,
    render::render_engine::{RenderEngine, RenderEvent},
};

pub enum ApplicationEvent {
    TerminateWithError(Report),
    ChangeLocation(GeoCoord),
    RenderEvent(RenderEvent),
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
        let controllers = ApplicationControllers::new(event_loop_proxy.clone());

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
        #[cfg(not(target_arch = "wasm32"))]
        let event_loop = event_loop.with_any_thread(true);
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
            match RenderEngine::new(window).await {
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
                    log::info!("Resized event before engine created");
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
                        match engine.render() {
                            Ok(_) => {}
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
                        self.require_render = false;
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
        };

        self.require_render = self.require_render || require_render;
    }
}
