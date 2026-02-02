use color_eyre::Result;
use tokio_with_wasm::alias as tokio;
use topo_renderer_new::app::run_app;
use winit::window::Window;

#[tokio::main]
pub async fn main() -> Result<()> {
    env_logger::init();
    use winit::dpi::LogicalSize;
    use winit::platform::x11::WindowAttributesExtX11;

    let (width, height) = (800, 600);
    let window_attributes = Window::default_attributes()
        .with_base_size(LogicalSize::new(width as f64, height as f64))
        .with_min_inner_size(LogicalSize::new(width as f64, height as f64))
        .with_inner_size(LogicalSize::new(width as f64, height as f64));

    Ok(run_app(window_attributes).await?)
}
