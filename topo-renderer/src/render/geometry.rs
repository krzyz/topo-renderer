use super::data::Vertex;

use glam::Vec3;
use std::f32::consts::PI;

const R0: f32 = 6371000.0;

pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

pub fn generate_icosahedron(scale: Vec3) -> Mesh {
    let phi = (1.0 + 5.0f32.sqrt()) * 0.5;
    let length = (1.0 + 1.0 / phi / phi).sqrt();
    let a = 1.0 / length;
    let b = 1.0 / length / phi;

    let vertices = [
        [0.0, b, -a],
        [b, a, 0.0],
        [-b, a, 0.0],
        [0.0, b, a],
        [0.0, -b, a],
        [-a, 0.0, b],
        [0.0, -b, -a],
        [a, 0.0, -b],
        [a, 0.0, b],
        [-a, 0.0, -b],
        [b, -a, 0.0],
        [-b, -a, 0.0],
    ]
    /*
    let vertices = [
        [-0.5, -0.5, -0.5],
        [-0.5, -0.5, 0.5],
        [0.5, -0.5, -0.5],
        [0.5, -0.5, 0.5],
        [-0.5, 0.5, -0.5],
        [-0.5, 0.5, 0.5],
        [0.5, 0.5, -0.5],
        [0.5, 0.5, 0.5],
    ]
    */
    .into_iter()
    .map(|p| {
        let vec = Vec3::from_array(p);
        Vertex::new(vec * scale, vec)
    })
    .collect();

    /*
    let indices = vec![
        2, 1, 0, 3, 1, 2, 0, 1, 5, 0, 5, 4, 1, 3, 5, 5, 3, 7, 3, 2, 7, 2, 6, 7, 4, 2, 0, 2, 4, 6,
        7, 6, 4, 7, 4, 5,
    ];
    */
    let indices = vec![
        2, 1, 0, 1, 2, 3, 5, 4, 3, 4, 8, 3, 7, 6, 0, 6, 9, 0, 11, 10, 4, 10, 11, 6, 9, 5, 2, 5, 9,
        11, 8, 7, 1, 7, 8, 10, 2, 5, 3, 8, 1, 3, 9, 2, 0, 1, 7, 0, 11, 9, 6, 7, 10, 6, 5, 11, 4,
        10, 8, 4,
    ];

    Mesh { vertices, indices }
}

pub fn transform(h: f32, lambda_deg: f32, phi_deg: f32, lambda_0_deg: f32, phi_0_deg: f32) -> Vec3 {
    let r = R0 + h;
    let phi = phi_deg / 180.0 * PI;
    let lambda = lambda_deg / 180.0 * PI;
    let phi_0 = phi_0_deg / 180.0 * PI;
    let lambda_0 = lambda_0_deg / 180.0 * PI;
    let dphi = phi - phi_0;
    let dlambda = lambda - lambda_0;
    // y is up
    let x = -r * (dphi.sin() * dlambda.cos() + (1.0 - dlambda.cos()) * phi.sin() * phi_0.cos());
    let y = r * (dphi.cos() * dlambda.cos() + (1.0 - dlambda.cos()) * phi.sin() * phi_0.sin()) - R0;
    let z = r * phi.cos() * dlambda.sin();

    Vec3::new(x, y, z)
}
