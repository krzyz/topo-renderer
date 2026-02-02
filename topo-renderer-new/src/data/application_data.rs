use std::collections::HashSet;

use topo_common::{GeoCoord, GeoLocation};

pub struct ApplicationData {
    pub current_location: Option<GeoCoord>,
    pub loaded_locations: HashSet<GeoLocation>,
}

impl ApplicationData {
    pub fn new() -> Self {
        Self {
            current_location: None,
            loaded_locations: HashSet::new(),
        }
    }
}
