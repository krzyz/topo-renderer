use axum::body::Body;
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::{Router, routing::get};
use color_eyre::Result;
use config::Config;
use http::{Method, header};
use serde::de::Error;
use serde::{Deserialize, Deserializer};
use std::path::Path;
use std::str::FromStr;
use strum::EnumString;
use tokio::fs::File;
use tokio_util::io::ReaderStream;
use tower::ServiceBuilder;
use tower_http::CompressionLevel;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, EnumString)]
enum LatitudeDirection {
    S,
    N,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, EnumString)]
enum LongitudeDirection {
    W,
    E,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Latitude {
    degree: i32,
    direction: LatitudeDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Longitude {
    degree: i32,
    direction: LongitudeDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
struct GeoLocation {
    #[serde(deserialize_with = "latitude_from_str")]
    latitude: Latitude,
    #[serde(deserialize_with = "longitude_from_str")]
    longitude: Longitude,
}

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

fn latitude_from_str<'de, D>(deserializer: D) -> Result<Latitude, D::Error>
where
    D: Deserializer<'de>,
{
    let (degree, direction): (i32, LatitudeDirection) =
        degree_with_direction_from_str(deserializer)?;
    Ok(Latitude { degree, direction })
}

fn longitude_from_str<'de, D>(deserializer: D) -> Result<Longitude, D::Error>
where
    D: Deserializer<'de>,
{
    let (degree, direction): (i32, LongitudeDirection) =
        degree_with_direction_from_str(deserializer)?;
    Ok(Longitude { degree, direction })
}

fn degree_with_direction_from_str<'de, D, T>(deserializer: D) -> Result<(i32, T), D::Error>
where
    T: FromStr,
    <T as FromStr>::Err: std::fmt::Display,
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    if s.is_empty() {
        return Err("Can't deserialize empty string to degree and direction")
            .map_err(D::Error::custom);
    }
    let (deg_str, dir_str) = s.split_at(s.len() - 1);
    Ok((
        deg_str.parse::<i32>().map_err(D::Error::custom)?,
        T::from_str(dir_str).map_err(D::Error::custom)?,
    ))
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

    log::info!("Opening file {}", file_name.display());

    let file = File::open(file_name).await.expect("file missing");
    let stream = ReaderStream::with_capacity(file, 256 * 1024);
    let body = Body::from_stream(stream);

    ([(header::CONTENT_TYPE, "text/csv")], body)
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

    let file = File::open(file_name).await.expect("file missing");
    let stream = ReaderStream::with_capacity(file, 10 * 1024 * 1024);
    let body = Body::from_stream(stream);

    ([(header::CONTENT_TYPE, "image/tiff")], body)
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    log::info!("Log test");

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_geo_location_query() {
        let json = r#"{"latitude": "49N", "longitude": "20E"}"#;
        let query: GeoLocation = serde_json::from_str(json).unwrap();
        assert_eq!(
            query,
            GeoLocation {
                latitude: Latitude {
                    degree: 49,
                    direction: LatitudeDirection::N,
                },

                longitude: Longitude {
                    degree: 20,
                    direction: LongitudeDirection::E,
                },
            },
        )
    }
}
