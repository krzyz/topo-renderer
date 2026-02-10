use super::data::Vertex;

use glam::Vec3;

pub const R0: f32 = 6_371_000.0;

pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

pub fn transform(h: f32, longitude_deg: f32, latitude_deg: f32) -> Vec3 {
    let r = R0 + h;
    let longitude = longitude_deg.to_radians();
    let latitude = latitude_deg.to_radians();
    let x = r * latitude.cos() * longitude.cos();
    let y = r * latitude.cos() * longitude.sin();
    let z = r * latitude.sin();
    Vec3::new(x, y, z)
}
