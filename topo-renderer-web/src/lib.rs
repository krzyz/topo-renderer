mod js;

use topo_renderer_new::app::run_app;

use color_eyre::{
    eyre::{eyre, OptionExt},
    Report, Result,
};
use tokio_with_wasm::alias as tokio;
use wasm_bindgen::prelude::*;
use winit::window::Window;

#[wasm_bindgen]
pub fn set_location(latitude: f32, longitude: f32) {
    // EVENT_LOOP_PROXY.with_borrow_mut(|proxy| {
    //     if let Some(proxy) = proxy {
    //         proxy
    //             .send_event(UserEvent::StateEvent(StateEvent::ChangeLocation(
    //                 GeoCoord::new(latitude, longitude),
    //             )))
    //             .unwrap();
    //     }
    // })
}

#[wasm_bindgen]
pub fn load_fonts() {
    // let mut loaded_before = false;
    // ADDITIONAL_FONTS_LOADED.with_borrow_mut(|loaded| {
    //     loaded_before = *loaded;
    //     *loaded = true
    // });

    // if !loaded_before {
    //     EVENT_LOOP_PROXY.with_borrow_mut(|proxy| {
    //         if let Some(proxy) = proxy {
    //             proxy
    //                 .send_event(UserEvent::StateEvent(StateEvent::LoadAdditionalFonts))
    //                 .unwrap();
    //         }
    //     })
    // }
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
            Ok(run_app(window_attributes).await?)
        }
        Err(err) => {
            log::error!("{err}");
            Err(err)
        }
    }
}

#[wasm_bindgen(start)]
pub fn start() {
    async_start();
}
