mod render;

use anyhow::Result;
use bytes::Bytes;
use geotiff::GeoTiff;
use render::data::Vertex;
use std::{
    fs::File,
    io::{Cursor, Read},
    path::PathBuf,
};
use topo2::start;

fn get_tiff_from_file() -> Result<Bytes> {
    let mut test_tiff = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    test_tiff.push("resources/small.gtiff");
    let mut file = File::open(test_tiff)?;
    // read the same file back into a Vec of bytes
    let mut buffer = Vec::<u8>::new();

    file.read_to_end(&mut buffer)?;

    Ok(buffer.into())
}

fn _get_tiff_from_http() -> Result<Bytes> {
    let api_key = "<snip>";

    Ok(reqwest::blocking::get(format!("https://portal.opentopography.org/API/globaldem?demtype=NASADEM&south=47.586&north=49.78&west=19.12&east=21.6&outputFormat=GTiff&API_Key={api_key}"))
        ?.bytes()?)
}

fn get_indices_and_vertices_from_tiff() -> (Vec<u32>, Vec<Vertex>) {
    (Vec::new(), Vec::new())
}

fn old_main() -> Result<()> {
    let gtiff = GeoTiff::read(Cursor::new(get_tiff_from_file()?.as_ref()))?;

    println!(
        concat!(
            "Number of samples: {}\n",
            "Height: {}\n",
            "Width: {}\n",
            "Model extent: {:#?}\n"
        ),
        gtiff.num_samples,
        gtiff.raster_height,
        gtiff.raster_width,
        gtiff.model_extent()
    );

    let offset = gtiff.model_extent().min();

    let dx =
        (gtiff.model_extent().max().x - gtiff.model_extent().min().x) / (gtiff.raster_width as f64);

    let dy = (gtiff.model_extent().max().y - gtiff.model_extent().min().y)
        / (gtiff.raster_height as f64);

    println!("Width step: {dx}\nHeight step: {dy}");
    println!("Min: {:#?}", gtiff.model_extent().min());
    println!("Center: {:#?}", gtiff.model_extent().center());

    let sample_vals = (0..30)
        .into_iter()
        .map(|i| {
            gtiff.get_value_at(
                &(gtiff.model_extent().min() + (0.5 + dx * i as f64, 0.5 * dy).into()),
                0,
            )
        })
        .collect::<Vec<Option<f32>>>();

    println!("Sample values: {sample_vals:#?}");

    Ok(())
}

fn main() {
    start();
}
