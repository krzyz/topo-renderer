use std::sync::Arc;

use super::texture::{Texture, TextureType};

pub struct BoundTextureView {
    textures: Vec<Texture>,
    texture_bind_group_layout: Arc<wgpu::BindGroupLayout>,
    bind_group: wgpu::BindGroup,
}

impl BoundTextureView {
    pub fn get_textures(&self) -> &Vec<Texture> {
        &self.textures
    }

    pub fn get_texture_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.texture_bind_group_layout
    }

    pub fn get_bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    pub fn create(device: &wgpu::Device, textures: Vec<Texture>) -> BoundTextureView {
        let layout_entries = textures
            .iter()
            .enumerate()
            .flat_map(|(i, texture)| {
                let i = i as u32;
                let (sample_type, ty) = match texture.get_t_type() {
                    TextureType::Render => (
                        wgpu::TextureSampleType::Float { filterable: true },
                        wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    ),
                    TextureType::Depth => (
                        wgpu::TextureSampleType::Float { filterable: false },
                        wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    ),
                };

                [
                    wgpu::BindGroupLayoutEntry {
                        binding: 2 * i,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2 * i + 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        // This should match the filterable field of the
                        // corresponding Texture entry above.
                        ty,
                        count: None,
                    },
                ]
            })
            .collect::<Vec<_>>();

        let bind_entries = textures
            .iter()
            .enumerate()
            .flat_map(|(i, texture)| {
                let i = i as u32;
                [
                    wgpu::BindGroupEntry {
                        binding: 2 * i,
                        resource: wgpu::BindingResource::TextureView(texture.get_view()),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2 * i + 1,
                        resource: wgpu::BindingResource::Sampler(texture.get_sampler()),
                    },
                ]
            })
            .collect::<Vec<_>>();

        let texture_bind_group_layout = Arc::new(device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                entries: layout_entries.as_slice(),
                label: Some("postprocessing_bind_group_layout"),
            },
        ));

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &texture_bind_group_layout,
            entries: bind_entries.as_slice(),
            label: Some("texture bind group"),
        });

        BoundTextureView {
            textures,
            texture_bind_group_layout,
            bind_group,
        }
    }
}
