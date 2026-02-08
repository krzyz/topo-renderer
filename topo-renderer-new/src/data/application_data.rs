use std::collections::{BTreeMap, HashSet};

use glam::Vec3;
use topo_common::{GeoCoord, GeoLocation};

use crate::{
    data::{Size, camera::Camera},
    render::data::{PeakInstance, PostprocessingUniforms, Uniforms},
};

pub struct PeakLabel {
    pub buffer: glyphon::Buffer,
    pub width: f32,
}

pub struct ApplicationData {
    pub current_location: Option<GeoCoord>,
    pub loaded_locations: HashSet<GeoLocation>,
    pub camera: Camera,
    pub uniforms: Uniforms,
    pub postprocessing_uniforms: PostprocessingUniforms,
    pub peaks: BTreeMap<GeoLocation, Vec<PeakInstance>>,
    pub peak_labels: BTreeMap<GeoLocation, Vec<PeakLabel>>,
}

impl ApplicationData {
    pub fn new(bounds: Size<f32>) -> Self {
        let mut camera = Camera::default();
        camera.set_eye(Vec3::new(0.0, 0.0, 0.0));

        let pixelize_n = 100.0;
        let uniforms = Uniforms::new(&camera, bounds);
        let postprocessing_uniforms = PostprocessingUniforms::new(bounds, pixelize_n);

        Self {
            current_location: None,
            loaded_locations: HashSet::new(),
            camera,
            uniforms,
            postprocessing_uniforms,
            peaks: BTreeMap::new(),
            peak_labels: BTreeMap::new(),
        }
    }
}
