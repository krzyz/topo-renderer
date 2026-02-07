use std::io::Cursor;

use bytes::Bytes;
use color_eyre::{
    Report, Result,
    eyre::{Context, OptionExt},
};
use geotiff::GeoTiff;
use tokio::{
    select,
    sync::broadcast,
    sync::mpsc::Receiver,
    task::{JoinSet, spawn_blocking},
};
use tokio_with_wasm::alias as tokio;
use topo_common::{GeoCoord, GeoLocation};
use winit::event_loop::EventLoopProxy;

use crate::{
    app::ApplicationEvent,
    render::{render_buffer::RenderBuffer, render_engine::RenderEvent},
};

#[derive(Debug, Clone, Copy)]
pub enum BackgroundEvent {
    DataRequested {
        requested: GeoLocation,
        current_location: GeoCoord,
    },
}

impl BackgroundEvent {
    pub fn to_task_info(self, running_tasks_left: usize) -> TaskInfo {
        TaskInfo {
            task: self,
            running_tasks_left,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TaskInfo {
    pub task: BackgroundEvent,
    pub running_tasks_left: usize,
}

#[derive(Debug, Clone)]
pub enum BackgroundNotification {
    TaskStarted(TaskInfo),
    TaskFinished(TaskInfo),
    TaskErrored { task: TaskInfo, error: String },
    JoinError(String),
}

/// This handles async operations of the application
/// which includes non-gpu long cpu-bound tasks done in the background
#[derive(Debug)]
pub struct BackgroundRunner {
    event_receiver: Receiver<BackgroundEvent>,
    render_event_loopback: EventLoopProxy<ApplicationEvent>,
    running_tasks: JoinSet<(BackgroundEvent, Result<()>)>,
    notification_broadcaster: broadcast::Sender<BackgroundNotification>,
}

pub async fn fetch_terrain(location: GeoLocation) -> Result<Option<GeoTiff>> {
    let backend_url = "http://localhost:3333";

    Ok(get_tiff_from_http(backend_url, location)
        .await?
        .map(|response| GeoTiff::read(Cursor::new(response)))
        .transpose()?)
}

async fn get_tiff_from_http(backend_url: &str, location: GeoLocation) -> Result<Option<Bytes>> {
    let url = format!("{backend_url}/dem?{}", location.to_request_params());
    let response = reqwest::get(&url)
        .await
        .wrap_err_with(|| format!("Error trying to fetch from {}", &url))?
        .bytes()
        .await
        .wrap_err_with(|| format!("Error decoding response from {}", &url))?;
    if response.len() > 0 {
        Ok(Some(response))
    } else {
        Ok(None)
    }
}

impl BackgroundRunner {
    pub fn new(
        event_receiver: Receiver<BackgroundEvent>,
        render_event_loopback: EventLoopProxy<ApplicationEvent>,
    ) -> Self {
        let (notification_broadcaster, _notification_subscriber) = broadcast::channel(128);
        Self {
            event_receiver,
            render_event_loopback,
            running_tasks: JoinSet::new(),
            notification_broadcaster,
        }
    }

    pub async fn process_event(
        render_event_loopback: EventLoopProxy<ApplicationEvent>,
        event: BackgroundEvent,
    ) -> Result<()> {
        use BackgroundEvent::*;

        match event {
            DataRequested {
                requested,
                current_location,
            } => {
                let terrain = fetch_terrain(requested).await?;
                let process_terrain = {
                    let render_event_loopback = render_event_loopback.clone();
                    move || {
                        let (vertices, indices) = if let Some(terrain) = terrain {
                            if GeoLocation::from(current_location) == requested {
                                let height: f32 = terrain
                                .get_value_at(&(<(f64, f64)>::from(current_location)).into(), 0)
                                .ok_or_eyre(
                                    "Center coordinates not found in the expected geotiff chunk",
                                )?;

                                let _ = render_event_loopback.send_event(
                                    ApplicationEvent::RenderEvent(RenderEvent::ResetCamera(
                                        current_location,
                                        height + 10.0,
                                    )),
                                );
                            }
                            RenderBuffer::process_terrain(&terrain)?
                        } else {
                            log::info!("Processing empty terrain");
                            RenderBuffer::process_empty_terrain(requested)?
                        };

                        Ok::<_, Report>((vertices, indices))
                    }
                };

                let (vertices, indices) = spawn_blocking(process_terrain).await??;

                if let Err(err) = render_event_loopback.send_event(ApplicationEvent::RenderEvent(
                    RenderEvent::TerrainReady(requested, vertices, indices),
                )) {
                    log::error!("{err}");
                }

                Ok(())
            }
        }
    }

    pub async fn run(&mut self) {
        loop {
            let notification = select! {
                Some(event) = self.event_receiver.recv() => {
                    let sender = self.render_event_loopback.clone();
                    self.running_tasks.spawn(async move {
                        (event, Self::process_event(sender, event).await)
                    });
                        BackgroundNotification::TaskStarted(event.to_task_info(self.running_tasks.len()))
                }
                Some(result) = self.running_tasks.join_next() => {
                    match result {
                        Ok((event, task_result)) => {
                            let task = event.to_task_info(self.running_tasks.len());
                            match task_result {
                                Ok(()) => BackgroundNotification::TaskFinished(task),
                                Err(err) => BackgroundNotification::TaskErrored {
                                    task,
                                    error: format!("{err:}")
                                },
                            }
                        }
                        Err(err) => {
                            log::error!("Error joining task: {err:?}");
                            BackgroundNotification::JoinError(format!("{err:}"))
                        }
                    }
                }
            };
            let _ = self.notification_broadcaster.send(notification);
        }
    }

    pub fn get_notification_receiver(&self) -> broadcast::Receiver<BackgroundNotification> {
        self.notification_broadcaster.subscribe()
    }
}
