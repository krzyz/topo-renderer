#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

use color_eyre::Result;
use tokio::{
    sync::mpsc::{Sender, channel},
    task::JoinHandle,
};
use tokio_with_wasm::alias as tokio;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;
use winit::{
    event::{DeviceEvent, WindowEvent},
    event_loop::EventLoopProxy,
};

use crate::{
    app::ApplicationEvent,
    control::{
        background_runner::{BackgroundEvent, BackgroundRunner},
        camera_controller::CameraController,
        ui_controller::UiController,
    },
    data::application_data::ApplicationData,
};

pub struct ApplicationControllers {
    runner_handle: JoinHandle<()>,
    event_sender: Sender<BackgroundEvent>,
    pub ui_controller: UiController,
    pub camera_controller: CameraController,
    previous_instant: Instant,
}

impl ApplicationControllers {
    pub fn new(render_event_loopback: EventLoopProxy<ApplicationEvent>) -> Self {
        let (event_sender, event_receiver) = channel(128);

        let mut runner = BackgroundRunner::new(event_receiver, render_event_loopback);

        let runner_handle = tokio::spawn(async move { runner.run().await });

        let ui_controller = UiController::new(event_sender.clone());
        let camera_controller = CameraController::new(0.01);

        ApplicationControllers {
            runner_handle,
            event_sender,
            ui_controller,
            camera_controller,
            previous_instant: Instant::now(),
        }
    }

    pub fn send_event(&mut self, event: BackgroundEvent) -> Result<()> {
        self.event_sender.blocking_send(event)?;
        Ok(())
    }

    pub fn input(&mut self, event: &WindowEvent) -> bool {
        self.camera_controller.process_events(event)
    }

    pub fn device_input(&mut self, event: &DeviceEvent) {
        self.camera_controller.process_device_events(event)
    }

    pub fn update(&mut self, require_render: bool, data: &mut ApplicationData) -> bool {
        let current_instant = Instant::now();
        let time_delta = current_instant - self.previous_instant;
        self.previous_instant = current_instant;

        let camera_changed = self
            .camera_controller
            .update_camera(&mut data.camera, time_delta);
        require_render || camera_changed
    }
}

impl Drop for ApplicationControllers {
    fn drop(&mut self) {
        self.runner_handle.abort();
    }
}
