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

    fn generate_prenormals(
        vertices: &Vec<Vertex>,
        raster_width: usize,
        raster_height: usize,
    ) -> Vec<glam::Vec3> {
        vertices
            .into_iter()
            .enumerate()
            .map(|(i, vert)| {
                let row = i / raster_height;
                let col = i % raster_height;
                let left = if col > 0 {
                    vertices.get(i - 1).map(|l| l.position - vert.position)
                } else {
                    None
                };
                let right = if col < raster_height - 1 {
                    vertices.get(i + 1).map(|r| r.position - vert.position)
                } else {
                    None
                };
                let bot = if row > 0 {
                    vertices
                        .get(i - raster_height)
                        .map(|b| b.position - vert.position)
                } else {
                    None
                };
                let top = if row < raster_width - 1 {
                    vertices
                        .get(i + raster_height)
                        .map(|t| t.position - vert.position)
                } else {
                    None
                };

                [(left, top), (top, right), (right, bot), (bot, left)]
                    .into_iter()
                    .map(|(v0, v1)| {
                        if let (Some(v0), Some(v1)) = (v0, v1) {
                            v0.cross(v1)
                        } else {
                            glam::Vec3::ZERO
                        }
                    })
                    .fold(glam::Vec3::ZERO, |sum, el| sum + el)
            })
            .collect::<Vec<_>>()
    }

    fn generate_indices(
        vertices: &Vec<Vertex>,
        normals: &Vec<glam::Vec3>,
        raster_width: usize,
        raster_height: usize,
    ) -> Vec<u32> {
        vertices
            .into_iter()
            .enumerate()
            .flat_map(|(i, _)| {
                let row = i / raster_height;
                let col = i % raster_height;

                let mut inds = vec![];

                if col < raster_height - 1 && row < raster_width - 1 {
                    let bl = i;
                    let br = i + 1;
                    let tl = i + raster_height;
                    let tr = i + raster_height + 1;
                    /*
                    let bltr = (vert.position.y - vertices.get(tr).unwrap().position.y).abs();
                    let brtl = (vertices.get(br).unwrap().position.y
                        - vertices.get(tl).unwrap().position.y)
                        .abs();
                    */

                    let bltr = normals.get(bl).unwrap().dot(*normals.get(tr).unwrap());
                    let brtl = normals.get(br).unwrap().dot(*normals.get(tl).unwrap());

                    if brtl > bltr {
                        inds.push(br);
                        inds.push(bl);
                        inds.push(tl);
                        inds.push(tl);
                        inds.push(tr);
                        inds.push(br);
                    } else {
                        inds.push(br);
                        inds.push(bl);
                        inds.push(tr);
                        inds.push(tl);
                        inds.push(tr);
                        inds.push(bl);
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

        let normals = Self::generate_prenormals(&vertices, raster_width, raster_height);
        let indices = Self::generate_indices(&vertices, &normals, raster_width, raster_height);

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
