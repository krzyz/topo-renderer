use color_eyre::Result;
use tokio::{
    sync::mpsc::{Sender, channel},
    task::JoinHandle,
};
use winit::event_loop::EventLoopProxy;

use crate::{
    app::ApplicationEvent,
    control::{
        background_runner::{BackgroundEvent, BackgroundRunner},
        ui_controller::UiController,
    },
};

pub struct ApplicationControllers {
    runner_handle: JoinHandle<()>,
    event_sender: Sender<BackgroundEvent>,
    pub ui_controller: UiController,
}

impl ApplicationControllers {
    pub fn new(render_event_loopback: EventLoopProxy<ApplicationEvent>) -> Self {
        let (event_sender, event_receiver) = channel(128);

        let mut runner = BackgroundRunner::new(event_receiver, render_event_loopback);

        let runner_handle = tokio::spawn(async move { runner.run().await });

        let ui_controller = UiController::new(event_sender.clone());

        ApplicationControllers {
            runner_handle,
            event_sender,
            ui_controller,
        }
    }

    pub fn send_event(&mut self, event: BackgroundEvent) -> Result<()> {
        self.event_sender.blocking_send(event)?;
        Ok(())
    }
}

impl Drop for ApplicationControllers {
    fn drop(&mut self) {
        self.runner_handle.abort();
    }
}
