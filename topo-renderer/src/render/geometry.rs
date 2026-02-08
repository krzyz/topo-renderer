use super::data::Vertex;

use glam::Vec3;

pub const R0: f32 = 6_371_000.0;

pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

pub fn transform(h: f32, lambda_deg: f32, phi_deg: f32) -> Vec3 {
    let r = R0 + h;
    let phi = phi_deg.to_radians();
    let lambda = lambda_deg.to_radians();
    let x = r * lambda.cos() * phi.cos();
    let y = r * lambda.cos() * phi.sin();
    let z = -r * lambda.sin();
    Vec3::new(x, y, z)
}
