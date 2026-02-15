use std::{fmt::Display, io::Cursor, sync::Arc};

use bytes::{Buf, Bytes};
use color_eyre::{
    Result,
    eyre::{Context, ContextCompat, OptionExt},
};
use itertools::Itertools;
use tiff::{
    decoder::{Decoder, DecodingResult},
    tags::Tag,
};
use tokio::{
    join, select,
    sync::broadcast,
    sync::mpsc::Receiver,
    task::{JoinSet, spawn_blocking},
};
use tokio_with_wasm::alias as tokio;
use topo_common::{GeoCoord, GeoLocation};
use winit::event_loop::EventLoopProxy;

use crate::{
    app::{ApplicationEvent, ApplicationSettings},
    common::coordinate_transform::{CoordinateTransform, get_height_value_at},
    data::peak::Peak,
    render::{
        data::PeakInstance, geometry::transform, render_engine::RenderEvent,
        text_renderer::TextRenderer,
    },
};

#[derive(Debug, Clone)]
pub enum BackgroundEvent {
    DataRequested {
        requested: GeoLocation,
        current_location: GeoCoord,
    },
}

impl BackgroundEvent {
    pub fn to_task_info(&self, running_tasks_left: usize) -> TaskInfo {
        TaskInfo {
            task: format!("{self}"),
            running_tasks_left,
        }
    }
}

impl Display for BackgroundEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackgroundEvent::DataRequested {
                requested,
                current_location,
            } => write!(
                f,
                "Data requested for location {:?}, current location: {:?}",
                requested, current_location
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TaskInfo {
    pub task: String,
    pub running_tasks_left: usize,
}

impl TaskInfo {
    pub fn new(task: String, running_tasks_left: usize) -> Self {
        Self {
            task,
            running_tasks_left,
        }
    }
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
    settings: Arc<ApplicationSettings>,
    event_receiver: Receiver<BackgroundEvent>,
    render_event_loopback: EventLoopProxy<ApplicationEvent>,
    notification_broadcaster: broadcast::Sender<BackgroundNotification>,
    running_tasks: JoinSet<(String, Result<()>)>,
}

pub async fn fetch_terrain(
    location: GeoLocation,
    settings: &ApplicationSettings,
) -> Result<(
    Vec<PeakInstance>,
    (DecodingResult, CoordinateTransform, (u32, u32)),
)> {
    let (tiff_bytes, peaks_bytes) = join!(
        get_tiff_from_http(settings.backend_url.as_str(), location),
        get_peaks_from_http(settings.backend_url.as_str(), location),
    );

    let mut height_map_decoding_result = DecodingResult::F32(vec![]);

    let mut decoder = Decoder::new(Cursor::new(
        tiff_bytes?.wrap_err("Empty terrain map for location")?,
    ))?;
    let pixel_scale_data = decoder
        .find_tag(Tag::ModelPixelScaleTag)?
        .map(|value| value.into_f64_vec())
        .transpose()?;
    let tie_points_data = decoder
        .find_tag(Tag::ModelTiepointTag)?
        .map(|value| value.into_f64_vec())
        .transpose()?;
    let model_transformation_data = decoder
        .find_tag(Tag::ModelTransformationTag)?
        .map(|value| value.into_f64_vec())
        .transpose()?;

    let coordinate_transform = CoordinateTransform::from_geo_tag_data(
        pixel_scale_data,
        tie_points_data,
        model_transformation_data,
    )?;

    let _ = decoder.read_image_to_buffer(&mut height_map_decoding_result);
    let size = decoder.dimensions()?;

    let peaks = peaks_bytes?
        .map(|response| Peak::read_peaks(response.reader()))
        .transpose()?;

    let peaks = peaks.map(|peaks| {
        peaks
            .into_iter()
            .sorted_by(|a, b| {
                PartialOrd::partial_cmp(&b.elevation, &a.elevation)
                    .unwrap_or(std::cmp::Ordering::Less)
            })
            .filter_map(|p| {
                get_height_value_at(
                    &height_map_decoding_result,
                    &coordinate_transform,
                    size,
                    p.longitude as f64,
                    p.latitude as f64,
                )
                .map(|h: f32| {
                    PeakInstance::new(transform(h + 10.0, p.longitude, p.latitude), p.name)
                })
            })
            .collect::<Vec<_>>()
    });

    Ok((
        peaks.unwrap_or(vec![]),
        (height_map_decoding_result, coordinate_transform, size),
    ))
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

async fn get_peaks_from_http(backend_url: &str, location: GeoLocation) -> Result<Option<Bytes>> {
    let url = format!("{backend_url}/peaks?{}", location.to_request_params());
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
        settings: Arc<ApplicationSettings>,
    ) -> Self {
        let (notification_broadcaster, _notification_subscriber) = broadcast::channel(128);
        Self {
            settings,
            event_receiver,
            render_event_loopback,
            running_tasks: JoinSet::new(),
            notification_broadcaster,
        }
    }

    pub async fn process_event(
        render_event_loopback: EventLoopProxy<ApplicationEvent>,
        event: BackgroundEvent,
        settings: Arc<ApplicationSettings>,
    ) -> Result<()> {
        use BackgroundEvent::*;

        match event {
            DataRequested {
                requested,
                current_location,
            } => {
                let (peaks, (terrain, coordinate_transform, size)) =
                    fetch_terrain(requested, &settings).await?;

                if GeoLocation::from(current_location) == requested {
                    let height = get_height_value_at(
                        &terrain,
                        &coordinate_transform,
                        size,
                        current_location.longitude as f64,
                        current_location.latitude as f64,
                    )
                    .ok_or_eyre("Unable to get current location's height from the height map")?;

                    let _ = render_event_loopback.send_event(ApplicationEvent::RenderEvent(
                        RenderEvent::ResetCamera(current_location, height),
                    ));
                }

                let _ = render_event_loopback
                    .send_event(ApplicationEvent::PeaksReady((requested, peaks.clone())));

                let peak_names_iter = peaks.iter().map(|peak| peak.name.as_str());

                let _ =
                    TextRenderer::load_additional_fonts(TextRenderer::get_scripts(peak_names_iter))
                        .await?;

                let process_peaks = {
                    let render_event_loopback = render_event_loopback.clone();
                    move || {
                        let labels = TextRenderer::prepare_peak_labels(&peaks);
                        let _ = render_event_loopback
                            .send_event(ApplicationEvent::PeakLabelsReady((requested, labels)));
                    }
                };

                let _ = spawn_blocking(process_peaks).await;

                let _ = render_event_loopback.send_event(ApplicationEvent::RenderEvent(
                    RenderEvent::TerrainReady(requested, terrain, coordinate_transform, size),
                ));

                Ok(())
            }
        }
    }

    pub async fn run(&mut self) {
        loop {
            let notification = select! {
                Some(event) = self.event_receiver.recv() => {
                    let sender = self.render_event_loopback.clone();
                    let settings = Arc::clone(&self.settings);
                    let event_name = format!("{event}");
                    {
                        let event_name = event_name.clone();
                    self.running_tasks.spawn(async move {
                        (event_name, Self::process_event(sender, event, settings).await)
                    });
                    }
                    BackgroundNotification::TaskStarted(TaskInfo::new(event_name, self.running_tasks.len()))
                }
                Some(result) = self.running_tasks.join_next() => {
                    match result {
                        Ok((event, task_result)) => {
                            let task = TaskInfo::new(event, self.running_tasks.len());
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
