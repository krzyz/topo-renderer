use std::str::FromStr;

use serde::de::Error;
use serde::{Deserialize, Deserializer};
use strum::{Display, EnumString};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, EnumString, Display, Hash)]
pub enum LatitudeDirection {
    S,
    N,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, EnumString, Display, Hash)]
pub enum LongitudeDirection {
    W,
    E,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Latitude {
    pub degree: i32,
    pub direction: LatitudeDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Longitude {
    pub degree: i32,
    pub direction: LongitudeDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Hash)]
pub struct GeoLocation {
    #[serde(deserialize_with = "latitude_from_str")]
    pub latitude: Latitude,
    #[serde(deserialize_with = "longitude_from_str")]
    pub longitude: Longitude,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct GeoCoord {
    pub latitude: f32,
    pub longitude: f32,
}

impl Into<f32> for Latitude {
    fn into(self) -> f32 {
        match self.direction {
            LatitudeDirection::S => -self.degree as f32,
            LatitudeDirection::N => self.degree as f32,
        }
    }
}

impl Into<f32> for Longitude {
    fn into(self) -> f32 {
        match self.direction {
            LongitudeDirection::E => self.degree as f32,
            LongitudeDirection::W => -self.degree as f32,
        }
    }
}

impl From<GeoCoord> for (f64, f64) {
    fn from(value: GeoCoord) -> Self {
        (value.longitude as f64, value.latitude as f64)
    }
}

impl std::fmt::Display for Latitude {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.degree.to_string().as_str(), self.direction)
    }
}

impl std::fmt::Display for Longitude {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.degree.to_string().as_str(), self.direction)
    }
}

impl From<GeoCoord> for GeoLocation {
    fn from(value: GeoCoord) -> Self {
        Self::from_coord(
            value.latitude.floor() as i32,
            value.longitude.floor() as i32,
        )
    }
}

impl From<GeoLocation> for GeoCoord {
    fn from(value: GeoLocation) -> Self {
        Self {
            latitude: value.latitude.into(),
            longitude: value.longitude.into(),
        }
    }
}

impl GeoLocation {
    pub fn from_coord(latitude: i32, longitude: i32) -> Self {
        Self {
            latitude: Latitude {
                degree: latitude.abs(),
                direction: if latitude.signum() > 0 {
                    LatitudeDirection::N
                } else {
                    LatitudeDirection::S
                },
            },
            longitude: Longitude {
                degree: longitude.abs() as i32,
                direction: if longitude.signum() > 0 {
                    LongitudeDirection::E
                } else {
                    LongitudeDirection::W
                },
            },
        }
    }

    pub fn to_request_params(&self) -> String {
        format!("latitude={}&longitude={}", self.latitude, self.longitude)
    }

    pub fn to_numerical(&self) -> (f32, f32) {
        (self.latitude.into(), self.longitude.into())
    }
}

impl GeoCoord {
    pub fn new(latitude: f32, longitude: f32) -> Self {
        Self {
            latitude,
            longitude,
        }
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
