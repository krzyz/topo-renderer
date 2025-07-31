use geotiff::GeoTiff;
use rayon::iter::{IntoParallelIterator, ParallelIterator};

use super::{buffer::Buffer, data::Vertex, geometry::transform};

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

    pub fn add_terrain(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        vertices: &Vec<Vertex>,
        indices: &Vec<u32>,
    ) {
        let new_vertices_size = (vertices.len() * std::mem::size_of::<Vertex>()) as u64;
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

    fn generate_indices(
        vertices: &Vec<Vertex>,
        raster_width: usize,
        raster_height: usize,
    ) -> Vec<u32> {
        vertices
            .into_iter()
            .enumerate()
            .flat_map(|(i, vert)| {
                let row = i / raster_height;
                let col = i % raster_height;

                let mut inds = vec![];

                if col < raster_height - 1 && row < raster_width - 1 {
                    // Check which pair of opposing corners has the least height difference
                    // for some hopefully nicer (but not ideal) triangles
                    let bl = i;
                    let br = i + 1;
                    let tl = i + raster_height;
                    let tr = i + raster_height + 1;
                    let bl_height = vert.position.length();
                    let br_height = vertices.get(br).unwrap().position.length();
                    let tl_height = vertices.get(tl).unwrap().position.length();
                    let tr_height = vertices.get(tr).unwrap().position.length();
                    let bltr = (bl_height - tr_height).abs();
                    let brtl = (br_height - tl_height).abs();
                    if br_height.min(tl_height) > bl_height.max(tr_height)
                        || bl_height.min(tr_height) < br_height.max(tl_height)
                        || bltr > brtl
                    {
                        inds.push(br);
                        inds.push(bl);
                        inds.push(tl);
                        inds.push(tl);
                        inds.push(tr);
                        inds.push(br);
                    } else {
                        inds.push(tr);
                        inds.push(br);
                        inds.push(bl);
                        inds.push(bl);
                        inds.push(tl);
                        inds.push(tr);
                    }
                }

                inds.into_iter()
            })
            .map(|i| i as u32)
            .collect::<Vec<_>>()
    }

    pub fn process_terrain(geotiff: &GeoTiff) -> (Vec<Vertex>, Vec<u32>) {
        let raster_width = geotiff.raster_width;
        let raster_height = geotiff.raster_height;

        let dx = (geotiff.model_extent().max().x - geotiff.model_extent().min().x)
            / (geotiff.raster_width as f64);

        let dy = (geotiff.model_extent().max().y - geotiff.model_extent().min().y)
            / (geotiff.raster_height as f64);

        let geotiff_min = geotiff.model_extent().min();

        let mut vertices = (0..raster_width)
            .into_par_iter()
            .flat_map(|row| {
                (0..raster_height)
                    .into_iter()
                    .map(|col| {
                        let lambda = (0.5 + row as f64) * dx;
                        let phi = (0.5 + col as f64) * dy;
                        let coord = geotiff_min + (lambda, phi).into();
                        let height = geotiff.get_value_at(&coord, 0).expect(&format!(
                            "Unable to find value for {coord:#?} (row {row}, col {col}"
                        ));
                        let position = transform(height, coord.y as f32, coord.x as f32);
                        Vertex::new(position, glam::Vec3::ZERO)
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        let indices = Self::generate_indices(&vertices, raster_width, raster_height);

        for chunk in indices.as_slice().chunks_exact(3) {
            //let [i0, _, i1, _, i2, _]: [u32; 6] = chunk.try_into().unwrap();
            let [i0, i1, i2]: [u32; 3] = chunk.try_into().unwrap();
            let v0 = vertices.get(i0 as usize).unwrap().position;
            let v1 = vertices.get(i1 as usize).unwrap().position;
            let v2 = vertices.get(i2 as usize).unwrap().position;

            let side1 = v1 - v0;
            let side2 = v2 - v1;

            let contribution = side1.cross(side2);

            for (&i, factor) in chunk.iter().zip([0.5, 1.0, 0.5]) {
                if let Some(vertex) = vertices.get_mut(i as usize) {
                    vertex.normal -= factor * contribution;
                }
            }
        }

        (vertices, indices)
    }
}
