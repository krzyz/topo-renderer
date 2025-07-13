use std::sync::{LazyLock, Mutex};

use geotiff::GeoTiff;
use log::debug;
use rayon::iter::{IntoParallelIterator, ParallelIterator};

use super::{
    buffer::Buffer,
    data::{PeakInstanceRaw, Vertex},
    geometry::{generate_icosahedron, transform, Mesh},
};

const LAMBDA_0: f32 = 20.13715; // longitude
const PHI_0: f32 = 49.36991; // latitude

static SPHERE_MESH: LazyLock<Mutex<Mesh>> = LazyLock::new(|| {
    let scale = glam::Vec3::new(50.0, 50.0, 50.0);
    let mesh = generate_icosahedron(scale);
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

        let peak_instances = Buffer::new(
            device,
            "instances buffer",
            0,
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );

        let sphere_mesh = SPHERE_MESH.lock().expect("Unable to get sphere vertices");
        let vertex_offset = sphere_mesh.vertices.len() as u64;
        let index_offset = sphere_mesh.indices.len() as u64;
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

    fn write_mesh_vertices(&self, queue: &wgpu::Queue) {
        let sphere_mesh = SPHERE_MESH.lock().expect("Unable to get sphere vertices");

        queue.write_buffer(
            &self.vertices.raw,
            0,
            bytemuck::cast_slice(sphere_mesh.vertices.as_slice()),
        );
    }

    fn write_mesh_indices(&self, queue: &wgpu::Queue) {
        let sphere_mesh = SPHERE_MESH.lock().expect("Unable to get sphere vertices");

        sphere_mesh.indices.iter().for_each(|v| {
            debug!("Index: {v}");
        });

        queue.write_buffer(
            &self.indices.raw,
            0,
            bytemuck::cast_slice(sphere_mesh.indices.as_slice()),
        );
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

        debug!("Vertex offset: {}", self.vertex_offset);

        vertices.iter().take(5).for_each(|v| {
            debug!("Terrain vertex: {:#?}", v.position);
        });

        queue.write_buffer(
            &self.vertices.raw,
            self.vertex_offset * std::mem::size_of::<Vertex>() as u64,
            bytemuck::cast_slice(vertices.as_slice()),
        );

        self.num_indices = indices.len() as u32;
        let new_indices_size =
            (self.index_offset as u64 + indices.len() as u64) * std::mem::size_of::<u32>() as u64;
        self.indices.resize(device, new_indices_size);
        debug!("Index offset: {}", self.index_offset);
        self.write_mesh_indices(queue);
        queue.write_buffer(
            &self.indices.raw,
            self.index_offset * std::mem::size_of::<u32>() as u64,
            bytemuck::cast_slice(indices.as_slice()),
        );
    }

    pub fn update_terrain(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        geotiff: &GeoTiff,
    ) {
        let new_vertices_size = ((geotiff.raster_width * geotiff.raster_height
            + self.vertex_offset as usize)
            * std::mem::size_of::<Vertex>()) as u64;

        if new_vertices_size != self.vertices.raw.size() {
            self.vertices.resize(device, new_vertices_size);
            self.write_mesh_vertices(queue);
            self.copy_vertices_and_indices(device, queue, geotiff);
        }
    }

    pub fn update_peaks(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        peak_instances: &Vec<glam::Vec3>,
    ) {
        self.peak_instances.resize(
            device,
            ((1 + peak_instances.len()) * std::mem::size_of::<PeakInstanceRaw>()) as u64,
        );
        self.num_peak_instances = 1 + peak_instances.len() as u32;
        debug!(
            "Updating instances buffer to size: {}",
            self.num_peak_instances
        );
        peak_instances.iter().take(5).for_each(|pi| {
            debug!("{:#?}", pi);
        });
        queue.write_buffer(
            &self.peak_instances.raw,
            std::mem::size_of::<PeakInstanceRaw>() as u64,
            bytemuck::cast_slice(peak_instances.as_slice()),
        )
    }
}
