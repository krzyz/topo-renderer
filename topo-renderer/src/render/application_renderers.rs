use wgpu::TextureFormat;

use crate::{
    data::Size,
    render::{line_renderer::LineRenderer, pipeline::Pipeline, text_renderer::TextRenderer},
};

use super::terrain_renderer::TerrainRenderer;

pub struct ApplicationRenderers {
    pub terrain: TerrainRenderer,
    pub text: TextRenderer,
    pub line: LineRenderer,
}

impl ApplicationRenderers {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        config: &wgpu::SurfaceConfiguration,
        format: TextureFormat,
        target_size: Size<u32>,
    ) -> Self {
        let terrain = TerrainRenderer::new(device, format, target_size);

        let text = TextRenderer::new(
            device,
            queue,
            config,
            Pipeline::get_postprocessing_depth_stencil_state(),
        );

        let mut line = LineRenderer::new(device, format);
        line.prepare(device, queue, vec![]);

        Self {
            terrain,
            text,
            line,
        }
    }
}
