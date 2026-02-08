mod js;

use std::cell::OnceCell;

use topo_common::GeoCoord;
use topo_renderer_new::{
    app::{ApplicationEvent, ApplicationRunner},
    control::background_runner::BackgroundNotification,
};

use color_eyre::{
    eyre::{eyre, OptionExt},
    Report, Result,
};
use tokio_with_wasm::alias as tokio;
use wasm_bindgen::prelude::*;
use winit::{event_loop::EventLoopProxy, window::Window};

use crate::js::push_notification;

thread_local! {
    pub static EVENT_LOOP_PROXY: OnceCell<EventLoopProxy<ApplicationEvent>> = OnceCell::new();
    pub static ADDITIONAL_FONTS_LOADED: OnceCell<()> = OnceCell::new();
}

#[wasm_bindgen]
pub fn set_location(latitude: f32, longitude: f32) {
    EVENT_LOOP_PROXY.with(|cell| {
        if let Some(proxy) = cell.get() {
            if let Err(err) = proxy.send_event(ApplicationEvent::ChangeLocation(GeoCoord::new(
                latitude, longitude,
            ))) {
                log::error!("{err}");
            }
        }
    })
}

#[wasm_bindgen]
pub fn load_fonts() {
    ADDITIONAL_FONTS_LOADED.with(|cell| {
        EVENT_LOOP_PROXY.with(|cell| {
            if let Some(proxy) = cell.get() {
                let _ = proxy.send_event(ApplicationEvent::LoadAdditionalFonts);
            }
        });
        let _ = cell.set(());
    });
}

#[tokio::main(flavor = "multi_thread")]
pub async fn async_start() -> Result<()> {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    console_log::init_with_level(log::Level::Info).expect("could not initialize logger");

    use wasm_bindgen::JsCast;
    use winit::platform::web::WindowAttributesExtWebSys;
    match wgpu::web_sys::window()
        .ok_or_eyre("Unable to get window")?
        .document()
        .ok_or_eyre("Unable to get document")?
        .get_element_by_id("canvas")
        .ok_or_eyre("Unable to get canvas by id \"canvas\"")?
        .dyn_into::<wgpu::web_sys::HtmlCanvasElement>()
        .map_err(|_| eyre!("Unable to convert canvas to HtmlCanvasElement"))
    {
        Ok::<_, Report>(canvas) => {
            let window_attributes = Window::default_attributes().with_canvas(Some(canvas));
            let mut app_runner = ApplicationRunner::new(window_attributes);
            EVENT_LOOP_PROXY.with(|cell| cell.set(app_runner.get_event_loop_proxy()).ok());
            if let Err(err) = app_runner.configure_background_runner(|f| tokio::spawn(f)) {
                log::error!("{err:?}");
                push_notification(format!("Error configuring background runner: {err}"));
            }
            if let Some(mut notifications_receiver) =
                app_runner.subscribe_to_background_notifications()
            {
                tokio::spawn(async move {
                    let notify_span = wgpu::web_sys::window()
                        .ok_or_eyre("Unable to get window")?
                        .document()
                        .ok_or_eyre("Unable to get document")?
                        .get_element_by_id("status")
                        .ok_or_eyre("Unable to get status span by id \"status\"")?
                        .dyn_into::<wgpu::web_sys::HtmlSpanElement>()
                        .map_err(|_| eyre!("Unable to convert canvas to HtmlSpanElement"))?;

                    loop {
                        if let Ok(event) = notifications_receiver.recv().await {
                            let running_tasks_left = match event {
                                BackgroundNotification::TaskStarted(task_info) => {
                                    log::debug!(
                                        "Task {:?} started, {} still running",
                                        task_info.task,
                                        task_info.running_tasks_left
                                    );
                                    Some(task_info.running_tasks_left)
                                }
                                BackgroundNotification::TaskFinished(task_info) => {
                                    log::debug!(
                                        "Task {:?} finished, {} still running",
                                        task_info.task,
                                        task_info.running_tasks_left
                                    );
                                    Some(task_info.running_tasks_left)
                                }
                                BackgroundNotification::TaskErrored { task, error } => {
                                    log::error!(
                                        "Task {:?} errored, {} still running. Error: {error:?}",
                                        task.task,
                                        task.running_tasks_left
                                    );
                                    push_notification(format!(
                                        "Error running background task: {error}"
                                    ));
                                    Some(task.running_tasks_left)
                                }
                                BackgroundNotification::JoinError(error) => {
                                    log::error!("Error on joining task: {error}");
                                    None
                                }
                            };

                            if let Some(running_tasks_left) = running_tasks_left {
                                if running_tasks_left > 0 {
                                    notify_span.set_inner_text(&format!(
                                        "Background tasks: {running_tasks_left}"
                                    ));
                                } else {
                                    notify_span.set_inner_text("");
                                }
                            }
                        } else {
                            break Ok::<_, Report>(());
                        }
                    }
                });
            } else {
                log::error!("Unable to get notifications receiver");
            }
            Ok(app_runner.run()?)
        }
        Err(err) => {
            log::error!("{err:?}");
            Err(err)
        }
    }
}

#[wasm_bindgen(start)]
pub fn start() {
    async_start();
}
