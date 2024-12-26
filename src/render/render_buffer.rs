use geotiff::GeoTiff;
use rayon::iter::{IntoParallelIterator, ParallelIterator};

use super::{buffer::Buffer, data::Vertex};

const R0: f32 = 6371000.0;
const PI: f32 = 3.14159265359;

const TO_RAD_FACTOR: f32 = 2.0 * PI / 180.0;

pub struct RenderBuffer {
    vertices: Buffer,
    indices: Buffer,
    num_indices: u32,
}

impl RenderBuffer {
    pub fn get_vertices(&self) -> &Buffer {
        &self.vertices
    }

    pub fn get_indices(&self) -> &Buffer {
        &self.indices
    }

    pub fn get_num_indices(&self) -> u32 {
        self.num_indices
    }

    pub fn new(device: &wgpu::Device) -> Self {
        let vertices = Buffer::new(
            device,
            "vertex buffer",
            0,
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );

        let indices = Buffer::new(
            device,
            "Index buffer",
            0,
            wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        );

        Self {
            vertices,
            indices,
            num_indices: 0,
        }
    }

    fn from_geo_coord_to_meters(geo_coord: glam::Vec3, phi: f32) -> glam::Vec3 {
        let dlambda = geo_coord.x * TO_RAD_FACTOR;
        let dphi = geo_coord.z * TO_RAD_FACTOR;
        let z = dlambda * R0 * phi.cos();
        let x = dphi * R0;

        glam::vec3(x, geo_coord.y, z)
    }

    fn generate_indices(raster_width: usize, raster_height: usize) -> Vec<u32> {
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

        let indices = Self::generate_indices(raster_width, raster_height);

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

        queue.write_buffer(
            &self.vertices.raw,
            0,
            bytemuck::cast_slice(vertices.as_slice()),
        );

        self.num_indices = indices.len() as u32;
        let new_indices_size = indices.len() as u64 * std::mem::size_of::<u32>() as u64;
        self.indices.resize(device, new_indices_size);
        queue.write_buffer(
            &self.indices.raw,
            0,
            bytemuck::cast_slice(indices.as_slice()),
        );
    }

    pub fn update(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, geotiff: &GeoTiff) {
        let new_vertices_size =
            (geotiff.raster_width * geotiff.raster_height * std::mem::size_of::<Vertex>()) as u64;

        if new_vertices_size != self.vertices.raw.size() {
            self.vertices.resize(device, new_vertices_size);
            self.copy_vertices_and_indices(device, queue, geotiff);
        }
    }
}
