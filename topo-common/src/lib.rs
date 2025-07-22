use std::str::FromStr;

use serde::de::Error;
use serde::{Deserialize, Deserializer};
use strum::{Display, EnumString};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, EnumString, Display)]
pub enum LatitudeDirection {
    S,
    N,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, EnumString, Display)]
pub enum LongitudeDirection {
    W,
    E,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Latitude {
    pub degree: i32,
    pub direction: LatitudeDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Longitude {
    pub degree: i32,
    pub direction: LongitudeDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
pub struct GeoLocation {
    #[serde(deserialize_with = "latitude_from_str")]
    pub latitude: Latitude,
    #[serde(deserialize_with = "longitude_from_str")]
    pub longitude: Longitude,
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

impl GeoLocation {
    pub fn to_request_params(&self) -> String {
        format!("latitude={}&longitude={}", self.latitude, self.longitude)
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
