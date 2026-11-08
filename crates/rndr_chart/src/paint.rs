//! Record a [`ChartScene`] into an [`rndr_paint::Canvas`].

use rndr_core::{text_engine, Rect, TextStyle, World};
use rndr_paint::{Affine, Canvas, CanvasPainter, FillRule, SceneLayer, TextSource};

use crate::model::Chart;
use crate::scene::{Anchor, ChartScene, Item, Shape, TextItem};
use crate::theme::ChartTheme;

/// Record `scene` with its top-left at `origin`.
///
/// `hover` is a [`crate::Hit::key`]: while set on an interactive chart, marks
/// outside that group dim. Otherwise marks outside [`Chart::active_key`] dim.
/// Text is shaped at its final scale so polar charts stay crisp.
pub fn paint(
    scene: &ChartScene,
    theme: &ChartTheme,
    world: &World,
    canvas: &mut Canvas,
    origin: [f32; 2],
    hover: Option<&str>,
) {
    let view = scene.view.then(Affine::translate(origin[0], origin[1]));
    let family = theme.family.clone().unwrap_or_else(|| {
        world
            .resource::<TextStyle>()
            .map(|s| s.family.clone())
            .unwrap_or_else(|| TextStyle::default().family)
    });
    let hovering = scene.interactive && hover.is_some();
    for item in &scene.items {
        let dimmed = if hovering {
            item.key.is_some() && item.key.as_deref() != hover
        } else {
            !item.active
        };
        if dimmed {
            canvas.push_layer(SceneLayer::opacity(item.dim));
        }
        draw(item, view, &family, world, canvas);
        if dimmed {
            canvas.pop_layer();
        }
    }
}

fn draw(item: &Item, view: Affine, family: &str, world: &World, canvas: &mut Canvas) {
    match &item.shape {
        Shape::Fill(path, color) => {
            canvas.push_transform(view);
            canvas.fill(path.clone(), *color, FillRule::NonZero);
            canvas.pop_transform();
        }
        Shape::Stroke(path, color, stroke) => {
            canvas.push_transform(view);
            canvas.stroke(path.clone(), *color, stroke.clone());
            canvas.pop_transform();
        }
        Shape::Text(text) => draw_text(text, view, family, world, canvas),
    }
}

fn draw_text(text: &TextItem, view: Affine, family: &str, world: &World, canvas: &mut Canvas) {
    let style = TextStyle {
        family: family.to_string(),
        size: text.size * view.uniform_scale(),
        weight: text.weight,
        ..TextStyle::default()
    };
    let engine = text_engine(world);
    let width = engine.measure(&text.text, &style, &[], f32::INFINITY).width;
    let line = engine.line_height(&style);
    let at = view.transform_point([text.x, text.y]);
    let x = match text.anchor {
        Anchor::Start => at[0],
        Anchor::Middle => at[0] - width / 2.0,
        Anchor::End => at[0] - width,
    };
    canvas.text(
        world,
        TextSource {
            text: text.text.clone(),
            style,
            spans: Vec::new(),
            max_width: f32::INFINITY,
            origin: [x, at[1] - line / 2.0],
            color: text.color,
        },
    );
}

/// Paints a chart into a surface slot.
///
/// Register it in a [`rndr_paint::CanvasRegistry`]. The chart is laid out
/// in [`Self::rect`] each frame and dims around [`Self::hover`], or around
/// [`Chart::active_key`] when nothing is hovered. A host that tracks the
/// pointer hit-tests the same layout with [`ChartScene::hit`].
#[derive(Clone, Debug)]
pub struct ChartPainter {
    /// What to draw.
    pub chart: Chart,
    /// Colors.
    pub theme: ChartTheme,
    /// Where, in canvas units.
    pub rect: Rect,
    /// Hovered [`crate::Hit::key`].
    pub hover: Option<String>,
}

impl ChartPainter {
    /// Paint `chart` in `rect`, nothing hovered.
    pub fn new(chart: Chart, theme: ChartTheme, rect: Rect) -> Self {
        Self {
            chart,
            theme,
            rect,
            hover: None,
        }
    }

    /// The layout [`CanvasPainter::paint`] records, in `rect`-local units.
    pub fn scene(&self) -> ChartScene {
        ChartScene::new(&self.chart, &self.theme, self.rect.w, self.rect.h)
    }
}

impl CanvasPainter for ChartPainter {
    fn paint(&self, world: &World, canvas: &mut Canvas) {
        paint(
            &self.scene(),
            &self.theme,
            world,
            canvas,
            [self.rect.x, self.rect.y],
            self.hover.as_deref(),
        );
    }
}
