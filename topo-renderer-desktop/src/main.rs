use color_eyre::{Report, Result};
use tokio::runtime::Runtime;
use tokio_with_wasm::alias as tokio;
use topo_renderer::{app::ApplicationRunner, control::background_runner::BackgroundNotification};
use winit::window::Window;

pub fn main() -> Result<()> {
    env_logger::init();
    use winit::dpi::LogicalSize;
    use winit::platform::x11::WindowAttributesExtX11;

    let (width, height) = (800, 600);
    let window_attributes = Window::default_attributes()
        .with_base_size(LogicalSize::new(width as f64, height as f64))
        .with_min_inner_size(LogicalSize::new(width as f64, height as f64))
        .with_inner_size(LogicalSize::new(width as f64, height as f64));

    let background_runtime = Runtime::new()?;

    let mut app_runner = ApplicationRunner::new(window_attributes);

    if let Err(err) = app_runner.configure_background_runner(|f| background_runtime.spawn(f)) {
        log::error!("{err:?}");
    }

    if let Some(mut notifications_receiver) = app_runner.subscribe_to_background_notifications() {
        background_runtime.spawn(async move {
            loop {
                if let Ok(event) = notifications_receiver.recv().await {
                    match event {
                        BackgroundNotification::TaskStarted(task_info) => {
                            log::info!(
                                "Task {:?} started, {} still running",
                                task_info.task,
                                task_info.running_tasks_left
                            );
                        }
                        BackgroundNotification::TaskFinished(task_info) => {
                            log::info!(
                                "Task {:?} finished, {} still running",
                                task_info.task,
                                task_info.running_tasks_left
                            );
                        }
                        BackgroundNotification::TaskErrored { task, error } => {
                            log::error!(
                                "Task {:?} errored, {} still running. Error: {error:?}",
                                task.task,
                                task.running_tasks_left
                            );
                        }
                        BackgroundNotification::JoinError(error) => {
                            log::error!("Error on joining task: {error}");
                        }
                    };
                } else {
                    break Ok::<_, Report>(());
                }
            }
        });
    } else {
        log::error!("Unable to get notifications receiver");
    }

    Ok::<(), Report>(app_runner.run()?)
}
