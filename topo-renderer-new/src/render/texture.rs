use wgpu::{Sampler, TextureView};

pub enum TextureType {
    Render,
    Depth,
}

pub struct Texture {
    texture: wgpu::Texture,
    view: TextureView,
    sampler: Sampler,
    t_type: TextureType,
}

impl Texture {
    pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float; // 1.

    pub fn get_texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    pub fn get_view(&self) -> &TextureView {
        &self.view
    }

    pub fn get_sampler(&self) -> &Sampler {
        &self.sampler
    }

    pub fn get_t_type(&self) -> &TextureType {
        &self.t_type
    }

    pub fn create_render_texture(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        (width, height): (u32, u32),
        label: &str,
    ) -> Self {
        let size = wgpu::Extent3d {
            // 2.
            width,
            height,
            depth_or_array_layers: 1,
        };
        let desc = wgpu::TextureDescriptor {
            label: Some(label),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        };
        let texture = device.create_texture(&desc);

        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(format),
            ..Default::default()
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            // 4.
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        Self {
            texture,
            view,
            sampler,
            t_type: TextureType::Render,
        }
    }

    pub fn create_depth_texture(
        device: &wgpu::Device,
        (width, height): (u32, u32),
        label: &str,
        usage: wgpu::TextureUsages,
    ) -> Self {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let desc = wgpu::TextureDescriptor {
            label: Some(label),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::DEPTH_FORMAT,
            usage,
            view_formats: &[],
        };
        let texture = device.create_texture(&desc);

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            lod_min_clamp: 0.0,
            lod_max_clamp: 100.0,
            ..Default::default()
        });

        Self {
            texture,
            view,
            sampler,
            t_type: TextureType::Depth,
        }
    }
}
