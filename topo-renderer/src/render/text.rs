use super::state::PeakInstance;
use glyphon::fontdb::{Database, Source};
use glyphon::{
    Attrs, Buffer, Cache, Family, FontSystem, Metrics, Shaping, SwashCache, TextArea, TextAtlas,
    TextBounds, TextRenderer, Viewport,
};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound::{Included, Unbounded};
use std::rc::Rc;
use std::sync::Arc;
use topo_common::GeoLocation;
use wgpu::MultisampleState;

pub const LINE_HEIGHT: f32 = 16.0;
pub const LINE_PADDING: f32 = 4.0;
pub const LABEL_PADDING_LEFT: f32 = 1.0;
pub const MAX_ROWS: usize = 8;

thread_local! {
    pub static FONT_SYSTEM: RefCell<FontSystem> = {
        let font_source = Source::Binary(Arc::new(include_bytes!(
            "../../../resources/Roboto-Regular.ttf"
        )));
        let mut font_db = Database::new();
        font_db.load_font_source(font_source);

        RefCell::new(FontSystem::new_with_locale_and_db(String::from("en-US"), font_db))
    };
}

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

pub struct LabelLayout {
    pub location: GeoLocation,
    pub id: LabelId,
    pub label_x: f32,
    pub label_y: f32,
    pub label_width: f32,
    pub peak_x: f32,
    pub peak_y: f32,
}

pub struct TextState {
    pub swash_cache: SwashCache,
    pub viewport: Viewport,
    pub atlas: TextAtlas,
    pub text_renderer: TextRenderer,
    pub labels: BTreeMap<GeoLocation, Vec<Label>>,
}

impl TextState {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        config: &wgpu::SurfaceConfiguration,
        depth_stencil: Option<wgpu::DepthStencilState>,
    ) -> Self {
        let swapchain_format = config.format;

        let swash_cache = SwashCache::new();
        let cache = Cache::new(device);
        let viewport = Viewport::new(device, &cache);
        // let mut atlas = TextAtlas::new(device, queue, &cache, swapchain_format);
        let mut atlas = TextAtlas::new(device, queue, &cache, swapchain_format);
        let text_renderer = TextRenderer::new(
            &mut atlas,
            device,
            MultisampleState::default(),
            depth_stencil,
        );
        let labels = BTreeMap::new();

        Self {
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

    pub fn add_labels(&mut self, location: GeoLocation, labels: Vec<Label>) {
        self.labels.insert(location, labels);
    }

    pub fn prepare_peak_labels(peaks: &Vec<PeakInstance>) -> Vec<Label> {
        let metric = Metrics::new(12.0, LINE_HEIGHT as f32);
        FONT_SYSTEM.with_borrow_mut(|mut font_system| {
            peaks
                .iter()
                .map(|peak| {
                    let mut buffer = Buffer::new(&mut font_system, metric);
                    buffer.set_size(&mut font_system, None, None);
                    buffer.set_text(
                        &mut font_system,
                        peak.name.as_str(),
                        &Attrs::new().family(Family::SansSerif),
                        Shaping::Advanced,
                    );
                    buffer.shape_until_scroll(&mut font_system, false);
                    let width = buffer
                        .layout_runs()
                        .next()
                        .expect("Unable to layout peak label")
                        .line_w;
                    Label { buffer, width }
                })
                .collect::<Vec<_>>()
        })
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        peak_labels: BTreeMap<GeoLocation, Vec<(LabelId, (u32, u32))>>,
    ) -> Vec<LabelLayout> {
        let laid_out_labels = layout_labels(
            peak_labels.clone(),
            |location, id| {
                self.labels
                    .get(&location)
                    .map(|labels| labels[id.0 as usize].width)
            },
            LINE_HEIGHT + LINE_PADDING,
        );
        let text_areas = laid_out_labels
            .iter()
            .map(
                |LabelLayout {
                     location,
                     id,
                     label_x,
                     label_y,
                     label_width: _,
                     peak_x: _,
                     peak_y: _,
                 }| TextArea {
                    buffer: &self.labels.get(&location).unwrap()[id.0 as usize].buffer,
                    left: label_x + LABEL_PADDING_LEFT,
                    top: *label_y,
                    scale: 1.0,
                    bounds: TextBounds::default(),
                    default_color: glyphon::Color::rgb(0, 0, 0),
                    custom_glyphs: &[],
                },
            )
            .collect::<Vec<_>>();
        FONT_SYSTEM.with_borrow_mut(|mut font_system| {
            self.text_renderer
                .prepare_with_depth(
                    device,
                    queue,
                    &mut font_system,
                    &mut self.atlas,
                    &mut self.viewport,
                    text_areas,
                    &mut self.swash_cache,
                    |_| 100.0 / 4096.0,
                )
                .unwrap();
        });

        laid_out_labels
    }
}

fn process_label_layout(edges: &mut Vec<BTreeSet<LabelEdge>>, x: u32, width: f32) -> Option<usize> {
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
    if row_i < MAX_ROWS {
        edges[row_i].insert(left_edge);
        edges[row_i].insert(right_edge);

        Some(row_i)
    } else {
        None
    }
}

fn layout_labels(
    peak_labels: BTreeMap<GeoLocation, Vec<(LabelId, (u32, u32))>>,
    widths: impl Fn(GeoLocation, LabelId) -> Option<f32>,
    line_height: f32,
) -> Vec<LabelLayout> {
    let edges: Rc<RefCell<Vec<BTreeSet<LabelEdge>>>> = Rc::new(RefCell::new(vec![]));

    peak_labels
        .into_iter()
        .flat_map(|(location, labels)| {
            let edges = edges.clone();
            labels
                .iter()
                .filter_map(|(i, (x, y))| {
                    if let Some(width) = widths(location, *i) {
                        let mut edges = edges.borrow_mut();

                        process_label_layout(&mut edges, *x, width).map(|row_i| LabelLayout {
                            location,
                            id: *i,
                            label_x: *x as f32,
                            label_y: line_height as f32 * (0.5 + row_i as f32),
                            label_width: width,
                            peak_x: *x as f32,
                            peak_y: *y as f32,
                        })
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
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
        let mut labels_map = BTreeMap::new();
        labels_map.insert(GeoLocation::from_coord(0, 0), labels);
        let layout = layout_labels(labels_map, |_, id| widths.get(&id).copied(), 1.0)
            .into_iter()
            .map(
                |LabelLayout {
                     location: _,
                     id,
                     label_x,
                     label_y,
                     label_width: _,
                     peak_x: _,
                     peak_y: _,
                 }| (id, (label_x.floor() as u32, label_y.floor() as u32)),
            )
            .collect::<Vec<_>>();
        let expected = expected_positions
            .into_iter()
            .enumerate()
            .map(|(i, position)| (LabelId(i as u32), position))
            .collect::<Vec<_>>();
        assert_eq!(layout, expected)
    }
}
