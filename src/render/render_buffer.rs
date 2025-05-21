use std::sync::{LazyLock, Mutex};

use color_eyre::owo_colors::OwoColorize;
use geotiff::GeoTiff;
use rayon::iter::{IntoParallelIterator, ParallelIterator};

use super::{
    buffer::Buffer,
    data::{PeakInstanceRaw, Vertex},
    geometry::{generate_icosahedron, Mesh},
    state::transform,
};

const LAMBDA_0: f32 = 20.13715; // longitude
const PHI_0: f32 = 49.36991; // latitude

const R0: f32 = 6371000.0;
const PI: f32 = 3.14159265359;

const TO_RAD_FACTOR: f32 = 2.0 * PI / 180.0;

static SPHERE_MESH: LazyLock<Mutex<Mesh>> = LazyLock::new(|| {
    let phi_0_rad = 49.37 * TO_RAD_FACTOR;
    let scale = glam::Vec3::new(1e4 / R0, 100.0, 1e4 / R0 / phi_0_rad.cos());
    let mut mesh = generate_icosahedron(scale);
    mesh.vertices = mesh
        .vertices
        .iter()
        .map(|v| Vertex {
            position: v.position + glam::Vec3::new(PHI_0, 0.0, LAMBDA_0),
            normal: v.normal,
        })
        .collect();
    Mutex::new(mesh)
});

pub struct RenderBuffer {
    vertices: Buffer,
    indices: Buffer,
    peak_instances: Buffer,
    num_indices: u32,
    num_peak_instances: u32,
    vertex_offset: u64,
    index_offset: u64,
}

impl RenderBuffer {
    pub fn get_vertices(&self) -> &Buffer {
        &self.vertices
    }

    pub fn get_indices(&self) -> &Buffer {
        &self.indices
    }

    pub fn get_peak_instances(&self) -> &Buffer {
        &self.peak_instances
    }

    pub fn get_num_indices(&self) -> u32 {
        self.num_indices
    }

    pub fn get_num_peak_instances(&self) -> u32 {
        self.num_peak_instances
    }

    pub fn is_terrain_empty(&self) -> bool {
        self.num_indices == self.index_offset as u32
    }

    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let mut vertices = Buffer::new(
            device,
            "vertex buffer",
            0,
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );

        let mut indices = Buffer::new(
            device,
            "Index buffer",
            0,
            wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        );

        let peak_instances = Buffer::new(
            device,
            "instances buffer",
            0,
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );

        let vertex_offset;
        let index_offset;
        {
            let sphere_mesh = SPHERE_MESH.lock().expect("Unable to get sphere vertices");
            vertex_offset = sphere_mesh.vertices.len() as u64;
            index_offset = sphere_mesh.indices.len() as u64;
            vertices.resize(device, vertex_offset * std::mem::size_of::<Vertex>() as u64);

            sphere_mesh.vertices.iter().for_each(|v| {
                println!("Sphere vertex: {:#?}", v.position);
                println!(
                    "Transformed: {:#?}",
                    transform(v.position.y, v.position.z, v.position.x, LAMBDA_0, PHI_0)
                );
            });

            queue.write_buffer(
                &vertices.raw,
                0,
                bytemuck::cast_slice(sphere_mesh.vertices.as_slice()),
            );

            indices.resize(device, index_offset * std::mem::size_of::<u32>() as u64);

            sphere_mesh.indices.iter().for_each(|v| {
                println!("Index: {v}");
            });

            queue.write_buffer(
                &indices.raw,
                0,
                bytemuck::cast_slice(sphere_mesh.indices.as_slice()),
            );
        }

        Self {
            vertices,
            indices,
            peak_instances,
            num_indices: index_offset as u32,
            num_peak_instances: 0,
            vertex_offset,
            index_offset,
        }
    }

    pub fn get_terrain_range(&self) -> std::ops::Range<u32> {
        (self.index_offset as u32)..self.get_num_indices()
    }

    pub fn get_vertex_offset(&self) -> i32 {
        self.vertex_offset as i32
    }

    pub fn get_index_offset(&self) -> u32 {
        self.index_offset as u32
    }

    fn from_geo_coord_to_meters(geo_coord: glam::Vec3, phi: f32) -> glam::Vec3 {
        let dlambda = geo_coord.x * TO_RAD_FACTOR;
        let dphi = geo_coord.z * TO_RAD_FACTOR;
        let z = dlambda * R0 * phi.cos();
        let x = dphi * R0;

        glam::vec3(x, geo_coord.y, z)
    }

    fn generate_indices(&self, raster_width: usize, raster_height: usize) -> Vec<u32> {
        (0..(raster_width * raster_height))
            .into_iter()
            .flat_map(|i| {
                let row = i / raster_height;
                let col = i % raster_height;

                let mut inds = vec![];

                if col < raster_height - 1 {
                    if row < raster_width - 1 {
                        inds.push(i + 1);
                        inds.push(i);
                        inds.push(i + raster_height);
                    }
                    if row > 0 {
                        inds.push(i);
                        inds.push(i + 1);
                        inds.push(i + 1 - raster_height);
                    }
                }

                inds.into_iter()
            })
            .map(|i| i as u32)
            .collect::<Vec<_>>()
    }

    fn copy_vertices_and_indices(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        geotiff: &GeoTiff,
    ) {
        let raster_width = geotiff.raster_width;
        let raster_height = geotiff.raster_height;

        let dx = (geotiff.model_extent().max().x - geotiff.model_extent().min().x)
            / (geotiff.raster_width as f64);

        let dy = (geotiff.model_extent().max().y - geotiff.model_extent().min().y)
            / (geotiff.raster_height as f64);

        let geotiff_min = geotiff.model_extent().min();

        let mut vertices = //iproduct!(0..raster_width, 0..raster_height)
            (0..raster_width).into_par_iter().flat_map(|row| (0..raster_height).into_iter().map(|col| {
                let lambda = (0.5 + row as f64) * dx;
                let phi = (0.5 + col as f64) * dy;
                let coord = geotiff_min + (lambda, phi).into();
                let height = geotiff.get_value_at(&coord, 0).expect(&format!(
                    "Unable to find value for {coord:#?} (row {row}, col {col}"
                ));
                let position = glam::vec3(coord.x as f32, height, coord.y as f32);
                Vertex::new(position, glam::Vec3::ZERO)
            }).collect::<Vec<_>>())
            .collect::<Vec<_>>();

        let indices = self.generate_indices(raster_width, raster_height);

        // Calculate normals using indices

        for chunk in indices.as_slice().chunks_exact(3) {
            let [i0, i1, i2]: [u32; 3] = chunk.try_into().unwrap();
            let v0 = vertices.get(i0 as usize).unwrap().position;
            let v1 = vertices.get(i1 as usize).unwrap().position;
            let v2 = vertices.get(i2 as usize).unwrap().position;

            let phi = v0.z * TO_RAD_FACTOR;

            let side1 = v1 - v0;
            let side2 = v2 - v1;

            let contribution = Self::from_geo_coord_to_meters(side1, phi)
                .cross(Self::from_geo_coord_to_meters(side2, phi));

            for &i in chunk {
                if let Some(vertex) = vertices.get_mut(i as usize) {
                    vertex.normal -= contribution;
                }
            }
        }

        println!("Vertex offset: {}", self.vertex_offset);

        queue.write_buffer(
            &self.vertices.raw,
            self.vertex_offset * std::mem::size_of::<Vertex>() as u64,
            bytemuck::cast_slice(vertices.as_slice()),
        );

        self.num_indices = indices.len() as u32;
        let new_indices_size =
            (self.index_offset as u64 + indices.len() as u64) * std::mem::size_of::<u32>() as u64;
        self.indices.resize(device, new_indices_size);
        println!("Index offset: {}", self.index_offset);
        queue.write_buffer(
            &self.indices.raw,
            self.index_offset * std::mem::size_of::<u32>() as u64,
            bytemuck::cast_slice(indices.as_slice()),
        );
    }

    pub fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        geotiff: &GeoTiff,
        peak_instances: Option<&Vec<glam::Vec3>>,
    ) {
        let new_vertices_size = ((geotiff.raster_width * geotiff.raster_height
            + self.vertex_offset as usize)
            * std::mem::size_of::<Vertex>()) as u64;

        if new_vertices_size != self.vertices.raw.size() {
            self.vertices.resize(device, new_vertices_size);
            self.copy_vertices_and_indices(device, queue, geotiff);
        }

        if let Some(peak_instances) = peak_instances {
            self.peak_instances.resize(
                device,
                ((1 + peak_instances.len()) * std::mem::size_of::<PeakInstanceRaw>()) as u64,
            );
            self.num_peak_instances = 1 + peak_instances.len() as u32;
            println!(
                "Updating instances buffer to size: {}",
                self.num_peak_instances
            );
            peak_instances.iter().take(5).for_each(|pi| {
                println!("{pi:#?}");
            });
            queue.write_buffer(
                &self.peak_instances.raw,
                std::mem::size_of::<PeakInstanceRaw>() as u64,
                bytemuck::cast_slice(peak_instances.as_slice()),
            )
        }
    }
}
