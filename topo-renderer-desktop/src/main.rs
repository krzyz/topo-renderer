use color_eyre::{Report, Result};
use tokio::runtime::Runtime;
use tokio_with_wasm::alias as tokio;
use topo_renderer_new::app::ApplicationRunner;
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

    Ok::<(), Report>(app_runner.run()?)
}
