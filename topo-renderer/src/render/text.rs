use super::state::PeakInstance;
use glyphon::{
    Attrs, Buffer, Cache, Family, FontSystem, Metrics, Shaping, SwashCache, TextArea, TextAtlas,
    TextBounds, TextRenderer, Viewport,
};
use std::collections::BTreeSet;
use std::ops::Bound::{Included, Unbounded};
use wgpu::MultisampleState;

const LINE_HEIGHT: f32 = 16.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct LabelId(pub u32);

pub struct Label {
    buffer: Buffer,
    width: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Side {
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct LabelEdge {
    position: u32,
    side: Side,
}

impl LabelEdge {
    fn left(position: u32) -> Self {
        Self {
            position,
            side: Side::Left,
        }
    }

    fn right(position: u32) -> Self {
        Self {
            position,
            side: Side::Right,
        }
    }
}

pub struct TextState {
    pub font_system: FontSystem,
    pub swash_cache: SwashCache,
    pub viewport: Viewport,
    pub atlas: TextAtlas,
    pub text_renderer: TextRenderer,
    pub labels: Vec<Label>,
}

impl TextState {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        config: &wgpu::SurfaceConfiguration,
    ) -> Self {
        let swapchain_format = config.format.add_srgb_suffix();
        let mut font_system = FontSystem::new();
        let font = include_bytes!("../../../resources/Roboto-Regular.ttf");
        font_system.db_mut().load_font_data(font.to_vec());
        let swash_cache = SwashCache::new();
        let cache = Cache::new(device);
        let viewport = Viewport::new(device, &cache);
        // let mut atlas = TextAtlas::new(device, queue, &cache, swapchain_format);
        let mut atlas = TextAtlas::new(device, queue, &cache, swapchain_format);
        let text_renderer =
            TextRenderer::new(&mut atlas, device, MultisampleState::default(), None);
        let labels = vec![];

        Self {
            font_system,
            swash_cache,
            viewport,
            atlas,
            text_renderer,
            labels,
        }
    }

    pub fn render(&mut self, pass: &mut wgpu::RenderPass<'_>) {
        self.text_renderer
            .render(&mut self.atlas, &mut self.viewport, pass)
            .unwrap();
    }

    pub fn prepare_peak_labels(&mut self, peaks: &Vec<PeakInstance>) {
        let metric = Metrics::new(12.0, LINE_HEIGHT as f32);
        self.labels = peaks
            .iter()
            .map(|peak| {
                let mut buffer = Buffer::new(&mut self.font_system, metric);
                buffer.set_size(&mut self.font_system, None, None);
                buffer.set_text(
                    &mut self.font_system,
                    peak.name.as_str(),
                    &Attrs::new().family(Family::SansSerif),
                    Shaping::Advanced,
                );
                buffer.shape_until_scroll(&mut self.font_system, false);
                let width = buffer
                    .layout_runs()
                    .next()
                    .expect("Unable to layout peak label")
                    .line_w;
                Label { buffer, width }
            })
            .collect::<Vec<_>>();
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        peak_labels: Vec<(LabelId, (u32, u32))>,
    ) {
        let text_areas = layout_labels(
            peak_labels,
            |label| self.labels[label.0 as usize].width,
            LINE_HEIGHT,
        )
        .into_iter()
        .map(|(i, (x, y))| TextArea {
            buffer: &self.labels[i.0 as usize].buffer,
            left: x,
            top: y,
            scale: 1.0,
            bounds: TextBounds::default(),
            default_color: glyphon::Color::rgb(0, 0, 0),
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

fn layout_labels(
    peak_labels: Vec<(LabelId, (u32, u32))>,
    widths: impl Fn(LabelId) -> f32,
    line_height: f32,
) -> Vec<(LabelId, (f32, f32))> {
    let mut edges: Vec<BTreeSet<LabelEdge>> = vec![];

    peak_labels
        .into_iter()
        .map(|(i, (x, _))| {
            let width = widths(i);
            let left_edge = LabelEdge::left((x as f32).floor() as u32);
            let right_edge = LabelEdge::right((x as f32 + width).ceil() as u32);
            let row_i = edges
                .iter()
                .enumerate()
                .filter_map(|(row_i, row)| {
                    if row
                        .range((Included(&left_edge), Included(&right_edge)))
                        .next()
                        .is_none()
                    {
                        match row.range((Included(&right_edge), Unbounded)).next() {
                            // If the first edge to the right is the right end of another label here
                            // it means that label is both further to the left and further to the right
                            Some(LabelEdge {
                                side: Side::Right, ..
                            }) => None,
                            _ => Some(row_i),
                        }
                    } else {
                        None
                    }
                })
                .next()
                .unwrap_or_else(|| {
                    edges.push(BTreeSet::new());
                    edges.len() - 1
                });
            edges[row_i].insert(left_edge);
            edges[row_i].insert(right_edge);

            (i, (x as f32, line_height as f32 * (0.5 + row_i as f32)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case(vec![0, 5, 2], vec![1, 1, 5], vec![(0, 0), (5, 0), (2, 1)])]
    #[case(vec![0, 6, 2], vec![1, 2, 5], vec![(0, 0), (6, 0), (2, 1)])]
    #[case(vec![0, 8, 2], vec![1, 1, 5], vec![(0, 0), (8, 0), (2, 0)])]
    #[case(vec![1, 5, 2], vec![2, 1, 5], vec![(1, 0), (5, 0), (2, 1)])]
    #[case(vec![1, 6, 2], vec![2, 2, 5], vec![(1, 0), (6, 0), (2, 1)])]
    #[case(vec![1, 8, 2], vec![2, 1, 5], vec![(1, 0), (8, 0), (2, 1)])]
    #[case(vec![3, 5, 2], vec![1, 1, 5], vec![(3, 0), (5, 0), (2, 1)])]
    #[case(vec![3, 6, 2], vec![1, 2, 5], vec![(3, 0), (6, 0), (2, 1)])]
    #[case(vec![3, 8, 2], vec![1, 1, 5], vec![(3, 0), (8, 0), (2, 1)])]
    #[case(vec![1, 9, 2], vec![7, 1, 5], vec![(1, 0), (9, 0), (2, 1)])]
    fn test_layout(
        #[case] positions: Vec<u32>,
        #[case] widths: Vec<u32>,
        #[case] expected_positions: Vec<(u32, u32)>,
    ) {
        let widths = widths
            .into_iter()
            .enumerate()
            .map(|(i, width)| (LabelId(i as u32), width as f32))
            .collect::<BTreeMap<_, _>>();
        let labels = positions
            .into_iter()
            .enumerate()
            .map(|(i, position)| (LabelId(i as u32), (position, 0)))
            .collect::<Vec<_>>();
        let layout = layout_labels(labels, |id| widths[&id], 1.0)
            .into_iter()
            .map(|(id, (x, y))| (id, (x.floor() as u32, y.floor() as u32)))
            .collect::<Vec<_>>();
        let expected = expected_positions
            .into_iter()
            .enumerate()
            .map(|(i, position)| (LabelId(i as u32), position))
            .collect::<Vec<_>>();
        assert_eq!(layout, expected)
    }
}
