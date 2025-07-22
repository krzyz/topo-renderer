use geotiff::GeoTiff;
use log::debug;
use rayon::iter::{IntoParallelIterator, ParallelIterator};

use super::{buffer::Buffer, data::Vertex, geometry::transform};

const LAMBDA_0: f32 = 20.13715; // longitude
const PHI_0: f32 = 49.36991; // latitude

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

    pub fn is_terrain_empty(&self) -> bool {
        self.num_indices == 0
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

    pub fn get_terrain_range(&self) -> std::ops::Range<u32> {
        (0)..self.get_num_indices()
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

    pub fn add_terrain(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, geotiff: &GeoTiff) {
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
                let position = transform(height, coord.x as f32, coord.y as f32, LAMBDA_0, PHI_0);
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

            let side1 = v1 - v0;
            let side2 = v2 - v1;

            let contribution = side1.cross(side2);

            for &i in chunk {
                if let Some(vertex) = vertices.get_mut(i as usize) {
                    vertex.normal -= contribution;
                }
            }
        }

        vertices.iter().take(5).for_each(|v| {
            debug!("Terrain vertex: {:#?}", v.position);
        });

        let new_vertices_size =
            (geotiff.raster_width * geotiff.raster_height * std::mem::size_of::<Vertex>()) as u64;
        if new_vertices_size != self.vertices.raw.size() {
            self.vertices.resize(device, new_vertices_size);
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
}
