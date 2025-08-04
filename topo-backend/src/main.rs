use axum::body::Body;
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::{Router, routing::get};
use color_eyre::Result;
use config::Config;
use http::{Method, header};
use serde::Deserialize;
use std::path::Path;
use tokio::fs::File;
use tokio_util::io::ReaderStream;
use topo_common::{GeoLocation, LatitudeDirection, LongitudeDirection};
use tower::ServiceBuilder;
use tower_http::CompressionLevel;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};

#[derive(Clone, Deserialize)]
struct AppState {
    data_dir: String,
}

impl AppState {
    fn from_config(settings: Config) -> Result<Self> {
        let app_state = settings.try_deserialize()?;

        Ok(app_state)
    }
}

async fn get_peaks(
    State(state): State<AppState>,
    geo_location: Query<GeoLocation>,
) -> impl IntoResponse {
    let file_name = Path::new(&state.data_dir).join(format!(
        "peaks/peaks_{}{}_{}{}.csv",
        match geo_location.latitude.direction {
            LatitudeDirection::N => "",
            LatitudeDirection::S => "-",
        },
        geo_location.latitude.degree.to_string(),
        match geo_location.longitude.direction {
            LongitudeDirection::E => "",
            LongitudeDirection::W => "-",
        },
        geo_location.longitude.degree.to_string()
    ));

    match File::open(file_name).await {
        Ok(file) => {
            let stream = ReaderStream::with_capacity(file, 256 * 1024);
            let body = Body::from_stream(stream);

            ([(header::CONTENT_TYPE, "text/csv")], body)
        }
        Err(_) => {
            let body = Body::empty();
            ([(header::CONTENT_TYPE, "text/html")], body)
        }
    }
}

async fn get_dem(
    State(state): State<AppState>,
    geo_location: Query<GeoLocation>,
) -> impl IntoResponse {
    let file_name = Path::new(&state.data_dir).join(format!(
        "COP90/COP90_hh/Copernicus_DSM_30_{}{:02}_00_{}{:03}_00_DEM.tif",
        match geo_location.latitude.direction {
            LatitudeDirection::N => "N",
            LatitudeDirection::S => "S",
        },
        geo_location.latitude.degree,
        match geo_location.longitude.direction {
            LongitudeDirection::E => "E",
            LongitudeDirection::W => "W",
        },
        geo_location.longitude.degree
    ));

    match File::open(file_name).await {
        Ok(file) => {
            let stream = ReaderStream::with_capacity(file, 10 * 1024 * 1024);
            let body = Body::from_stream(stream);

            ([(header::CONTENT_TYPE, "image/tiff")], body)
        }
        Err(_) => {
            let body = Body::empty();
            ([(header::CONTENT_TYPE, "text/html")], body)
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    log::info!("Starting api backend service");

    let cors = CorsLayer::new()
        .allow_methods([Method::GET])
        .allow_origin(Any);

    let settings = Config::builder()
        .add_source(config::File::with_name("Settings"))
        .add_source(config::Environment::with_prefix("TOPO"))
        .set_default("address", "0.0.0.0")?
        .set_default("port", 3333)?
        .build()
        .unwrap();

    let address = settings.get_string("address")?;
    let port = settings.get_int("port")?;

    let state = AppState::from_config(settings)?;

    let app = Router::new()
        .route("/peaks", get(get_peaks))
        .layer(
            ServiceBuilder::new().layer(
                CompressionLayer::new()
                    .zstd(true)
                    .quality(CompressionLevel::Fastest),
            ),
        )
        .route("/dem", get(get_dem))
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("{address}:{port}"))
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();

    Ok(())
}
