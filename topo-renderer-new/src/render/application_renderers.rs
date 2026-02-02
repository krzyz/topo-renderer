use wgpu::TextureFormat;

use crate::data::Size;

use super::terrain_renderer::TerrainRenderer;

pub struct ApplicationRenderers {
    pub terrain: TerrainRenderer,
}

impl ApplicationRenderers {
    pub fn new(device: &wgpu::Device, format: TextureFormat, target_size: Size<u32>) -> Self {
        let terrain = TerrainRenderer::new(device, format, target_size);
        Self { terrain }
    }
}
