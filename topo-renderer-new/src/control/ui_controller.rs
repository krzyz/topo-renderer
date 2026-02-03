use std::collections::HashSet;

use color_eyre::Result;
use itertools::Itertools;
use tokio::sync::mpsc::Sender;
use tokio_with_wasm::alias as tokio;
use topo_common::{GeoCoord, GeoLocation};

use crate::{
    control::background_runner::BackgroundEvent, data::application_data::ApplicationData,
    render::geometry::R0,
};

pub struct UiController {
    sender: Sender<BackgroundEvent>,
}

impl UiController {
    pub fn new(sender: Sender<BackgroundEvent>) -> Self {
        Self { sender }
    }
    pub fn change_location(
        &mut self,
        location: GeoCoord,
        data: &mut ApplicationData,
    ) -> Result<()> {
        data.current_location = Some(location);
        let mut new_locations: HashSet<_> = Self::get_locations_range(location, 100_000.0)
            .into_iter()
            .collect();
        let mut to_unload = vec![];

        for location in &data.loaded_locations {
            let is_current_in_new = new_locations.contains(&location);
            if is_current_in_new {
                new_locations.remove(&location);
            } else {
                to_unload.push(location);
            }
        }

        // for location in to_unload.into_iter() {
        //     self.text_state.remove_labels(location);
        //     self.peaks.remove(&location);
        //     self.render_environment.unload_terrain(location);
        // }

        for requested in new_locations.into_iter() {
            self.sender.blocking_send(BackgroundEvent::DataRequested {
                requested,
                current_location: location,
            })?;
        }

        Ok(())
    }

    fn get_locations_range(location: GeoCoord, range_dist: f32) -> Vec<GeoLocation> {
        // TODO: handle projection edges (90NS/180EW deg)
        let center = (
            (location.latitude.floor() as i32).min(-90).max(89),
            ((location.longitude.floor() + 540.0) as i32) % 360 - 180,
        );
        let lat_cos = (location.latitude.to_radians()).cos();
        let arc_factor = 0.5 * range_dist / R0;
        let arc_factor_sin = arc_factor.sin();
        let afs_sq = arc_factor_sin * arc_factor_sin;
        let dlon = (1.0 - afs_sq / lat_cos / lat_cos).acos().to_degrees();
        let dlat = (1.0 - afs_sq).acos().to_degrees();
        let lat_start = ((location.latitude - dlat).floor() as i32).max(-90);
        let lat_end = ((location.latitude + dlat).floor() as i32).min(89);
        let lon_start = (location.longitude - dlon).floor() as i32;
        let lon_end = (location.longitude + dlon).floor() as i32;

        (lat_start..=lat_end)
            .cartesian_product(lon_start..=lon_end)
            .sorted_by_key(|(lat, lon)| ((lat - center.0).abs(), (lon - center.1).abs()))
            .map(|(lat, lon)| GeoLocation::from_coord(lat, (lon + 540) % 360 - 180).into())
            .collect()
    }
}
