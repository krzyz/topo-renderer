use std::{
    fs::File,
    io::{self, BufReader},
};

use approx::{AbsDiffEq, UlpsEq};
use color_eyre::{
    Section,
    eyre::{Result, eyre},
};

#[derive(Debug, serde::Deserialize, Clone, PartialEq)]
pub struct Peak {
    pub latitude: f32,
    pub longitude: f32,
    pub name: String,
    pub elevation: f32,
}

impl AbsDiffEq for Peak {
    type Epsilon = <f32 as AbsDiffEq>::Epsilon;

    fn default_epsilon() -> <f32 as AbsDiffEq>::Epsilon {
        f32::default_epsilon()
    }

    fn abs_diff_eq(&self, other: &Self, epsilon: <f32 as AbsDiffEq>::Epsilon) -> bool {
        f32::abs_diff_eq(&self.latitude, &other.latitude, epsilon)
            && f32::abs_diff_eq(&self.longitude, &other.longitude, epsilon)
            && self.name == other.name
            && f32::abs_diff_eq(&self.elevation, &other.elevation, epsilon)
    }
}

impl UlpsEq for Peak {
    fn default_max_ulps() -> u32 {
        f32::default_max_ulps()
    }

    fn ulps_eq(&self, other: &Self, epsilon: <f32 as AbsDiffEq>::Epsilon, max_ulps: u32) -> bool {
        f32::ulps_eq(&self.latitude, &other.latitude, epsilon, max_ulps)
            && f32::ulps_eq(&self.longitude, &other.longitude, epsilon, max_ulps)
            && self.name == other.name
            && f32::ulps_eq(&self.elevation, &other.elevation, epsilon, max_ulps)
    }
}

impl Peak {
    pub fn read_from_lat_lon(lat: i32, lon: i32) -> Result<Vec<Self>> {
        //let f = File::open(format!("../data/peaks_{lat}_{lon}.csv"))?;
        //let reader = BufReader::new(f);
        let reader = BufReader::new(
            include_bytes!("/home/krzyz/projects/rust/topo2/data/peaks_49_20.csv").as_slice(),
        );
        Self::read_peaks(reader)
    }

    pub fn read_peaks<R: io::Read>(reader: R) -> Result<Vec<Self>> {
        let mut rdr = csv::Reader::from_reader(reader);
        let results = rdr.deserialize().collect::<Vec<_>>();

        if results.iter().all(|r| r.is_ok()) {
            return Ok(results.into_iter().map(|res| res.unwrap()).collect());
        }

        let err = results
            .into_iter()
            .filter(Result::is_err)
            .map(Result::unwrap_err)
            .fold(
                eyre!("encountered multiple erorrs while reading peaks csv"),
                |report, e| report.error(e),
            );

        Err(err)
    }
}

#[cfg(test)]
mod tests {
    use approx::assert_ulps_eq;

    use super::*;

    #[test]
    fn read_from_csv() {
        let csv_sample = r#"
latitude,longitude,name,elevation
49.542824,20.111383,Turbacz,1310.0
50.054916,19.893354,Kopiec Kościuszki,326.5"#;

        let expected = vec![
            Peak {
                latitude: 49.542824,
                longitude: 20.111383,
                name: "Turbacz".to_owned(),
                elevation: 1310.0,
            },
            Peak {
                latitude: 50.054916,
                longitude: 19.893354,
                name: "Kopiec Kościuszki".to_owned(),
                elevation: 326.5,
            },
        ];

        let read = Peak::read_peaks(BufReader::new(csv_sample.as_bytes()));

        if let Err(e) = &read {
            println!("error: {e}");
        }

        read.unwrap()
            .iter()
            .zip(expected.iter())
            .for_each(|(read, expected)| {
                assert_ulps_eq!(read, expected);
            });
    }
}
