use glyphon::{
    Attrs, Buffer, Cache, Family, FontSystem, Metrics, Shaping, SwashCache, TextArea, TextAtlas,
    TextBounds, TextRenderer, Viewport,
};
use log::debug;
use wgpu::MultisampleState;

use crate::common::data::Size;

use super::state::PeakInstance;

pub struct TextState {
    pub font_system: FontSystem,
    pub swash_cache: SwashCache,
    pub viewport: Viewport,
    pub atlas: TextAtlas,
    pub text_renderer: TextRenderer,
    pub text_buffers: Vec<Buffer>,
}

impl TextState {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        config: &wgpu::SurfaceConfiguration,
        physical_size: Size<f32>,
    ) -> Self {
        let swapchain_format = config.format;
        let font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let cache = Cache::new(device);
        let viewport = Viewport::new(device, &cache);
        let mut atlas = TextAtlas::new(device, queue, &cache, swapchain_format);
        let text_renderer =
            TextRenderer::new(&mut atlas, device, MultisampleState::default(), None);
        debug!("Setting test state buffer size: {physical_size:#?}");
        let text_buffers = vec![];

        Self {
            font_system,
            swash_cache,
            viewport,
            atlas,
            text_renderer,
            text_buffers,
        }
    }

    pub fn render(&mut self, pass: &mut wgpu::RenderPass<'_>) {
        self.text_renderer
            .render(&mut self.atlas, &mut self.viewport, pass)
            .unwrap();
    }

    pub fn prepare_peak_labels(&mut self, peaks: &Vec<PeakInstance>) {
        let metric = Metrics::new(12.0, 16.0);
        self.text_buffers = peaks
            .iter()
            .map(|peak| {
                let mut text_buffer = Buffer::new(&mut self.font_system, metric);
                text_buffer.set_size(&mut self.font_system, None, None);
                text_buffer.set_text(
                    &mut self.font_system,
                    peak.name.as_str(),
                    &Attrs::new().family(Family::SansSerif),
                    Shaping::Advanced,
                );
                text_buffer.shape_until_scroll(&mut self.font_system, false);
                text_buffer
            })
            .collect::<Vec<_>>();
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        peak_labels: Vec<(u32, (u32, u32))>,
    ) {
        let text_areas = peak_labels
            .into_iter()
            .map(|(i, (x, y))| TextArea {
                buffer: &self.text_buffers[i as usize],
                left: x as f32,
                top: y as f32,
                scale: 1.0,
                bounds: TextBounds::default(),
                default_color: glyphon::Color::rgb(255, 0, 0),
                custom_glyphs: &[],
            })
            .collect::<Vec<_>>();
        self.text_renderer
            .prepare(
                device,
                queue,
                &mut self.font_system,
                &mut self.atlas,
                &mut self.viewport,
                text_areas,
                &mut self.swash_cache,
            )
            .unwrap();
    }
}
