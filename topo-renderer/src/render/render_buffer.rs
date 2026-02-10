use std::{cell::OnceCell, sync::Arc};

use crate::{
    common::coordinate_transform::CoordinateTransform,
    render::{data::TerrainUniforms, pipeline::TerrainRenderPipeline, texture::Texture},
};

use super::{buffer::Buffer, data::Vertex};

thread_local! {
    static VERTICES: OnceCell<Arc<Buffer>> = OnceCell::new();
    static INDICES: OnceCell<(Arc<Buffer>, usize)> = OnceCell::new();
}

pub struct RenderBuffer {
    vertices: Arc<Buffer>,
    indices: Arc<Buffer>,
    indices_len: usize,
    #[allow(dead_code)]
    texture: Texture,
    height_map_texture_bind_group: wgpu::BindGroup,
    #[allow(dead_code)]
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
            let vertices = generate_vertices((width, height));
            Arc::clone(cell.get_or_init(|| {
                Arc::new(Buffer::new_init(
                    device,
                    "terrain vertices",
                    bytemuck::cast_slice(vertices.as_slice()),
                    wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                ))
            }))
        });

        let (indices, indices_len) = INDICES.with(|cell| {
            let indices = generate_indices((width, height));
            let (indices, indices_len) = cell.get_or_init(|| {
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

        let texture = Texture::create_height_map_texture(
            device,
            (width, height),
            "terrain height map texture",
        );

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture.get_texture(),
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
            *texture.get_size(),
        );

        let uniforms = Buffer::new_init(
            device,
            "terrain uniform buffer",
            bytemuck::bytes_of(&TerrainUniforms::new(coordinate_transform, (width, height))),
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );

        let height_map_texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("height map bind group"),
            layout: &pipeline.get_height_map_bind_group_layout(),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture.get_view()),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: uniforms.raw.as_entire_binding(),
                },
            ],
        });

        Self {
            vertices,
            indices,
            indices_len,
            texture,
            height_map_texture_bind_group,
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

    pub fn get_height_map_texture_bind_group(&self) -> &wgpu::BindGroup {
        &self.height_map_texture_bind_group
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
                [
                    index,
                    index + 1,
                    index_next_row + 1,
                    index_next_row + 1,
                    index_next_row,
                    index,
                ]
            })
        })
        .collect::<Vec<_>>()
}
