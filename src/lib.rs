extern crate approx;

pub mod common;
pub mod render;

use bytes::Bytes;
use color_eyre::eyre::Result;
use render::state::State;
use std::{fs::File, io::Write};
use wasm_bindgen::prelude::*;
use winit::{
    event::{Event, WindowEvent},
    event_loop::EventLoop,
    window::Window,
};

fn get_tiff_from_file() -> Result<Bytes> {
    let buffer = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/resources/small.gtiff"
    ));

    Ok(Bytes::from(buffer.as_slice()))
}

pub async fn get_tiff_from_http() -> Result<Bytes> {
    let api_key = "<snip>";

    Ok(reqwest::get(format!("https://portal.opentopography.org/API/globaldem?demtype=NASADEM&south=49.106&north=49.38&west=19.66&east=20.2&outputFormat=GTiff&API_Key={api_key}"))
        .await?.bytes().await?)
}

pub async fn write_tiff_from_http() -> Result<()> {
    let tiff_bytes = get_tiff_from_http().await?;
    let mut file = File::create("small.tiff")?;
    file.write_all(&tiff_bytes)?;
    Ok(())
}

async fn run(event_loop: EventLoop<()>, window: Window) -> Result<()> {
    color_eyre::install()?;
    let mut state = State::new(&window).await;
    let mut surface_configured = false;

    event_loop
        .run(move |event, control_flow| {
            if let Event::DeviceEvent {
                device_id: _,
                ref event,
            } = event
            {
                state.device_input(event);
            }
            if let Event::WindowEvent {
                window_id: _,
                ref event,
            } = event
            {
                if !state.input(event) {
                    match event {
                        WindowEvent::Resized(physical_size) => {
                            surface_configured = true;
                            state.resize(*physical_size);
                            // On macos the window needs to be redrawn manually after resizing
                            state.window().request_redraw();
                        }
                        WindowEvent::RedrawRequested => {
                            state.window().request_redraw();

                            if !surface_configured {
                                return;
                            }
                            state.update();
                            match state.render() {
                                Ok(_) => {}
                                // Reconfigure the surface if it's lost or outdated
                                Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                                    state.resize(state.size())
                                }
                                // The system is out of memory, we should probably quit
                                Err(
                                    wgpu::SurfaceError::OutOfMemory | wgpu::SurfaceError::Other,
                                ) => {
                                    log::error!("OutOfMemory");
                                    control_flow.exit();
                                }

                                // This happens when the a frame takes too long to present
                                Err(wgpu::SurfaceError::Timeout) => {
                                    log::warn!("Surface timeout")
                                }
                            }
                        }
                        WindowEvent::CloseRequested => control_flow.exit(),
                        _ => {}
                    };
                }
            }
        })
        .unwrap();

    Ok(())
}

#[wasm_bindgen(start)]
pub fn start() {
    let event_loop = EventLoop::new().unwrap();
    #[allow(unused_mut)]
    let mut builder = winit::window::WindowBuilder::new();
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        use winit::platform::web::WindowBuilderExtWebSys;
        let canvas = web_sys::window()
            .unwrap()
            .document()
            .unwrap()
            .get_element_by_id("canvas")
            .unwrap()
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .unwrap();
        builder = builder.with_canvas(Some(canvas));
    }
    let window = builder.build(&event_loop).unwrap();

    #[cfg(not(target_arch = "wasm32"))]
    {
        env_logger::init();
        let _ = pollster::block_on(run(event_loop, window));
    }
    #[cfg(target_arch = "wasm32")]
    {
        std::panic::set_hook(Box::new(console_error_panic_hook::hook));
        console_log::init().expect("could not initialize logger");
        wasm_bindgen_futures::spawn_local(run(event_loop, window));
    }
}
