use std::sync::Arc;

use color_eyre::{Report, Result};
use tokio::task::spawn_blocking;
use tokio_with_wasm::alias as tokio;
use topo_common::GeoCoord;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
    platform::wayland::EventLoopBuilderExtWayland,
    window::WindowAttributes,
};

use crate::{
    control::{
        application_controllers::ApplicationControllers, background_runner::BackgroundEvent,
    },
    data::application_data::ApplicationData,
    render::render_engine::{RenderEngine, RenderEvent},
};

pub enum ApplicationEvent {
    RenderEngineReady(RenderEngine),
    TerminateWithError(Report),
    RenderEvent(RenderEvent),
}

pub struct Application {
    state: Option<RenderEngine>,
    controllers: ApplicationControllers,
    data: ApplicationData,
    window_attributes: WindowAttributes,
    event_loop_proxy: EventLoopProxy<ApplicationEvent>,
    surface_configured: bool,
}

impl Application {
    pub fn new(
        window_attributes: WindowAttributes,
        event_loop_proxy: EventLoopProxy<ApplicationEvent>,
    ) -> Self {
        let controllers = ApplicationControllers::new(event_loop_proxy.clone());
        let data = ApplicationData::new();

        Self {
            state: None,
            controllers,
            data,
            window_attributes,
            event_loop_proxy,
            surface_configured: false,
        }
    }
}

impl ApplicationHandler<ApplicationEvent> for Application {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        let window = Arc::new(
            event_loop
                .create_window(self.window_attributes.clone())
                .unwrap(),
        );

        let event_loop_proxy = self.event_loop_proxy.clone();

        if let Err(err) = self
            .controllers
            .send_event(BackgroundEvent::InitializeState {
                window,
                event_loop_proxy,
            })
        {
            log::error!("{err}");
            event_loop.exit();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        let Some(engine) = &mut self.state else {
            return;
        };

        match event {
            WindowEvent::Resized(physical_size) => {
                engine.resize(physical_size);
                // On macos the window needs to be redrawn manually after resizing
                engine.window().request_redraw();
            }
            WindowEvent::RedrawRequested => {
                engine.window().request_redraw();

                if !self.surface_configured {
                    return;
                }

                match engine.update() {
                    Ok(changed) => {
                        match engine.render(changed) {
                            Ok(_) => {}
                            // Reconfigure the surface if it's lost or outdated
                            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                                engine.resize(engine.size())
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
                    Err(err) => {
                        log::error!("{err}");
                    }
                }
            }
            WindowEvent::CloseRequested => event_loop.exit(),
            _ => {}
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: ApplicationEvent) {
        match event {
            ApplicationEvent::RenderEngineReady(mut render_engine) => {
                render_engine.resize(render_engine.size());
                render_engine.window().request_redraw();
                self.surface_configured = true;
                self.state = Some(render_engine);
                let _ = self
                    .controllers
                    .ui_controller
                    .change_location(GeoCoord::new(49.35135, 20.21139), &mut self.data);
            }
            ApplicationEvent::TerminateWithError(err) => {
                eprintln!("{err}");
                event_loop.exit();
            }
            ApplicationEvent::RenderEvent(render_event) => {
                if let Some(state) = &mut self.state {
                    state.process_event(render_event);
                }
            }
        }
    }
}

pub async fn run_app(window_attributes: WindowAttributes) -> Result<()> {
    let app_result = spawn_blocking(|| {
        let event_loop = EventLoop::<ApplicationEvent>::with_user_event()
            .with_any_thread(true)
            .build()
            .unwrap();
        let event_loop_proxy = event_loop.create_proxy();

        let mut app = Application::new(window_attributes, event_loop_proxy);

        event_loop.run_app(&mut app)
    })
    .await?;

    Ok(app_result?)
}
