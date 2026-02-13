use std::{cell::OnceCell, sync::Arc};

use thiserror::Error;

use crate::{
    common::coordinate_transform::CoordinateTransform,
    render::{data::TerrainUniforms, pipeline::TerrainRenderPipeline, texture::Texture},
};

use super::{buffer::Buffer, data::Vertex};

thread_local! {
    static VERTICES: OnceCell<Arc<Buffer>> = OnceCell::new();
    static INDICES: OnceCell<(Arc<Buffer>, usize)> = OnceCell::new();
    static DUMMY_NORMALS: OnceCell<Arc<Texture>> = OnceCell::new();
}

pub struct NormalTextureResources {
    pub normal_texture: Texture,
    pub dummy_bind_group: wgpu::BindGroup,
}

pub enum TerrainBindGroup {
    DummyNormals(NormalTextureResources),
    CalculatedNormals(wgpu::BindGroup),
}

#[derive(Error, Debug)]
pub enum RenderBufferError {
    #[error("Tried to switch to calculated normal texture twice")]
    SwitchedToCalculatedNormalsTwice,
}

pub struct RenderBuffer {
    vertices: Arc<Buffer>,
    indices: Arc<Buffer>,
    indices_len: usize,
    height_map_texture: Texture,
    height_map_texture_bind_group: TerrainBindGroup,
    uniforms: Buffer,
}

impl RenderBuffer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        (width, height): (u32, u32),
        height_map_data: &[u8],
        coordinate_transform: CoordinateTransform,
        pipeline: &TerrainRenderPipeline,
    ) -> Self {
        let vertices = VERTICES.with(|cell| {
            Arc::clone(cell.get_or_init(|| {
                let vertices = generate_vertices((width, height));
                Arc::new(Buffer::new_init(
                    device,
                    "terrain vertices",
                    bytemuck::cast_slice(vertices.as_slice()),
                    wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                ))
            }))
        });

        let (indices, indices_len) = INDICES.with(|cell| {
            let (indices, indices_len) = cell.get_or_init(|| {
                let indices = generate_indices((width, height));
                (
                    Arc::new(Buffer::new_init(
                        device,
                        "terrain vertices",
                        bytemuck::cast_slice(indices.as_slice()),
                        wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                    )),
                    indices.len(),
                )
            });
            (Arc::clone(indices), *indices_len)
        });

        let height_map_texture = Texture::create_height_map_texture(
            device,
            (width, height),
            "terrain height map texture",
        );

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &height_map_texture.get_texture(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            height_map_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            *height_map_texture.get_size(),
        );

        let uniforms = Buffer::new_init(
            device,
            "terrain uniform buffer",
            bytemuck::bytes_of(&TerrainUniforms::new(coordinate_transform, (width, height))),
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );

        let dummy_bind_group = Self::create_bind_group(
            device,
            pipeline,
            &height_map_texture,
            &Self::create_dummy_normals(device),
            &uniforms,
        );

        let height_map_texture_bind_group =
            TerrainBindGroup::DummyNormals(NormalTextureResources {
                normal_texture: Self::create_normals_texture(device, (width, height)),
                dummy_bind_group,
            });

        Self {
            vertices,
            indices,
            indices_len,
            height_map_texture_bind_group,
            height_map_texture,
            uniforms,
        }
    }

    pub fn get_vertices(&self) -> &Buffer {
        &self.vertices
    }

    pub fn get_indices(&self) -> &Buffer {
        &self.indices
    }

    pub fn get_indices_len(&self) -> usize {
        self.indices_len
    }

    pub fn get_height_map_texture(&self) -> &Texture {
        &self.height_map_texture
    }

    pub fn get_terrain_bind_group(&self) -> &TerrainBindGroup {
        &self.height_map_texture_bind_group
    }

    pub fn get_height_map_texture_bind_group(&self) -> &wgpu::BindGroup {
        match &self.height_map_texture_bind_group {
            TerrainBindGroup::DummyNormals(normal_texture_resources) => {
                &normal_texture_resources.dummy_bind_group
            }
            TerrainBindGroup::CalculatedNormals(bind_group) => bind_group,
        }
    }

    pub fn get_uniforms(&self) -> &Buffer {
        &self.uniforms
    }

    pub fn switch_to_computed_normals(
        &mut self,
        device: &wgpu::Device,
        pipeline: &TerrainRenderPipeline,
    ) -> Result<(), RenderBufferError> {
        self.height_map_texture_bind_group = match &self.height_map_texture_bind_group {
            TerrainBindGroup::DummyNormals(normal_texture_resources) => {
                TerrainBindGroup::CalculatedNormals(Self::create_bind_group(
                    device,
                    pipeline,
                    &self.height_map_texture,
                    &normal_texture_resources.normal_texture,
                    &self.uniforms,
                ))
            }
            TerrainBindGroup::CalculatedNormals(_) => {
                return Err(RenderBufferError::SwitchedToCalculatedNormalsTwice);
            }
        };

        Ok(())
    }

    fn create_dummy_normals(device: &wgpu::Device) -> Arc<Texture> {
        DUMMY_NORMALS.with(|cell| {
            Arc::clone(cell.get_or_init(|| {
                let normal_texture = Texture::create_normal_texture(
                    device,
                    (1, 1),
                    wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                    "dummy normal texture",
                );
                Arc::new(normal_texture)
            }))
        })
    }

    fn create_normals_texture(device: &wgpu::Device, size: (u32, u32)) -> Texture {
        Texture::create_normal_texture(
            device,
            size,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
            "terrain normal texture",
        )
    }

    fn create_bind_group(
        device: &wgpu::Device,
        pipeline: &TerrainRenderPipeline,
        height_map_texture: &Texture,
        normals_texture: &Texture,
        uniforms: &Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("height map bind group"),
            layout: &pipeline.get_height_map_bind_group_layout(),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&height_map_texture.get_view()),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&normals_texture.get_view()),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniforms.raw.as_entire_binding(),
                },
            ],
        })
    }
}

fn generate_vertices(size: (u32, u32)) -> Vec<Vertex> {
    (0..size.0)
        .flat_map(|i| (0..size.1).map(move |j| Vertex::new((i, j))))
        .collect::<Vec<_>>()
}

fn generate_indices(size: (u32, u32)) -> Vec<u32> {
    (0..(size.0 - 1))
        .flat_map(|i| {
            (0..(size.1 - 1)).flat_map(move |j| {
                let index = i * size.1 + j;
                let index_next_row = (i + 1) * size.1 + j;
                if (i + j) % 2 == 0 {
                    [
                        index,
                        index + 1,
                        index_next_row + 1,
                        index_next_row + 1,
                        index_next_row,
                        index,
                    ]
                } else {
                    [
                        index,
                        index + 1,
                        index_next_row,
                        index_next_row + 1,
                        index_next_row,
                        index + 1,
                    ]
                }
            })
        })
        .collect::<Vec<_>>()
}
