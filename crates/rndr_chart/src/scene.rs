//! Headless chart layout: shapes, text and hover regions in scene points.

use rndr_core::{Color, Rect};
use rndr_paint::{Affine, Path, Stroke};

use crate::format::format_number;
use crate::model::{Chart, ChartKind};
use crate::theme::ChartTheme;

/// Horizontal text anchor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    /// `x` is the left edge.
    Start,
    /// `x` is the center.
    Middle,
    /// `x` is the right edge.
    End,
}

/// One label.
#[derive(Clone, Debug, PartialEq)]
pub struct TextItem {
    /// Text.
    pub text: String,
    /// Anchor point x.
    pub x: f32,
    /// Vertical center of the line.
    pub y: f32,
    /// Font size in points.
    pub size: f32,
    /// Font weight.
    pub weight: u16,
    /// Color.
    pub color: Color,
    /// How `x` anchors the line.
    pub anchor: Anchor,
}

/// What an [`Item`] draws.
#[derive(Clone, Debug)]
pub enum Shape {
    /// Filled path.
    Fill(Path, Color),
    /// Stroked path.
    Stroke(Path, Color, Stroke),
    /// One line of text.
    Text(TextItem),
}

/// One drawable, tagged for hover dimming.
#[derive(Clone, Debug)]
pub struct Item {
    /// What to draw.
    pub shape: Shape,
    /// Hover group: a category or series key. Keyless items never dim.
    pub key: Option<String>,
    /// Matched [`Chart::active_key`].
    pub active: bool,
    /// Opacity when dimmed.
    pub dim: f32,
}

/// Hover region shape, in the scene's local units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Region {
    /// Axis-aligned box.
    Rect(Rect),
    /// Disc.
    Circle([f32; 2], f32),
    /// Annular sector; angles are clockwise from twelve o'clock.
    Wedge {
        /// Center.
        center: [f32; 2],
        /// Inner radius.
        inner: f32,
        /// Outer radius.
        outer: f32,
        /// Start angle.
        start: f32,
        /// End angle.
        end: f32,
    },
}

impl Region {
    /// `p` is inside.
    pub fn contains(&self, p: [f32; 2]) -> bool {
        match *self {
            Region::Rect(r) => r.contains(p[0], p[1]),
            Region::Circle(c, r) => (p[0] - c[0]).hypot(p[1] - c[1]) <= r,
            Region::Wedge {
                center,
                inner,
                outer,
                start,
                end,
            } => {
                let (dx, dy) = (p[0] - center[0], p[1] - center[1]);
                let d = dx.hypot(dy);
                if d < inner || d > outer {
                    return false;
                }
                let tau = std::f32::consts::TAU;
                let a = dx.atan2(-dy).rem_euclid(tau);
                let s = start.rem_euclid(tau);
                let sweep = end - start;
                sweep >= tau || (a - s).rem_euclid(tau) <= sweep
            }
        }
    }
}

/// One tooltip row.
#[derive(Clone, Debug, PartialEq)]
pub struct TipLine {
    /// Series or axis name.
    pub label: String,
    /// Formatted value.
    pub value: String,
    /// Swatch.
    pub color: Color,
}

/// Hover target with its tooltip.
#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    /// Where.
    pub region: Region,
    /// Hover group passed to [`crate::paint`].
    pub key: Option<String>,
    /// Tooltip heading.
    pub title: String,
    /// Tooltip rows.
    pub lines: Vec<TipLine>,
}

/// One legend entry.
#[derive(Clone, Debug, PartialEq)]
pub struct LegendEntry {
    /// Text.
    pub label: String,
    /// Swatch.
    pub color: Color,
    /// Matched [`Chart::active_key`].
    pub active: bool,
}

/// A laid-out plot.
///
/// The plot body only: title, legend and footer belong to the host, which
/// reads [`Self::legend`] and the chart's text fields.
#[derive(Clone, Debug)]
pub struct ChartScene {
    /// Width used, in points.
    pub width: f32,
    /// Height used, in points. Heatmaps size themselves.
    pub height: f32,
    /// Local units to scene points.
    pub view: Affine,
    /// Back-to-front drawables, in local units.
    pub items: Vec<Item>,
    /// Hover targets, in local units; later ones win.
    pub hits: Vec<Hit>,
    /// Legend entries; empty when the chart hides its legend.
    pub legend: Vec<LegendEntry>,
    /// Hover dims everything outside the hovered group.
    pub interactive: bool,
}

impl ChartScene {
    /// Lay out `chart` in a `width` × `height` box.
    pub fn new(chart: &Chart, theme: &ChartTheme, width: f32, height: f32) -> Self {
        let width = width.max(1.0);
        let height = height.max(1.0);
        let mut b = Builder::new(chart, theme);
        let (view, used_h) = match chart.kind {
            ChartKind::Bar | ChartKind::Line | ChartKind::Area | ChartKind::Combo => {
                (crate::cartesian::build(&mut b, width, height), height)
            }
            ChartKind::Scatter => (crate::cartesian::scatter(&mut b, width, height), height),
            ChartKind::Pie | ChartKind::Donut => (crate::polar::pie(&mut b, width, height), height),
            ChartKind::Radar => (crate::polar::radar(&mut b, width, height), height),
            ChartKind::Radial => (crate::polar::radial(&mut b, width, height), height),
            ChartKind::Heatmap => {
                let h = crate::heatmap::build(&mut b, width);
                (Affine::IDENTITY, h)
            }
        };
        let legend = if chart.legend_placement().is_some() {
            chart
                .legend_items()
                .into_iter()
                .enumerate()
                .map(|(i, label)| LegendEntry {
                    active: chart.is_active(&[&label, series_key(chart, i)]),
                    color: theme.series_color(i, &chart.colors),
                    label,
                })
                .collect()
        } else {
            Vec::new()
        };
        Self {
            width,
            height: used_h,
            view,
            items: b.items,
            hits: if chart.tooltip || chart.interactive {
                b.hits
            } else {
                Vec::new()
            },
            legend,
            interactive: chart.interactive,
        }
    }

    /// Topmost hover target under `point`, in scene points.
    pub fn hit(&self, point: [f32; 2]) -> Option<&Hit> {
        let local = self.view.inverse()?.transform_point(point);
        self.hits.iter().rev().find(|h| h.region.contains(local))
    }
}

fn series_key(chart: &Chart, i: usize) -> &str {
    match chart.kind {
        ChartKind::Pie | ChartKind::Donut | ChartKind::Radial | ChartKind::Scatter => "",
        _ => chart.series.get(i).map_or("", |s| s.key.as_str()),
    }
}

/// Text sizes, in points, from the reference renderer's rem sizes.
pub(crate) mod size {
    pub const AXIS: f32 = 11.2;
    pub const VALUE: f32 = 10.4;
    pub const PIE: f32 = 10.88;
    pub const CENTER_VALUE: f32 = 21.6;
    pub const CENTER_LABEL: f32 = 11.52;
    pub const RING: f32 = 9.28;
}

/// Dim opacity for most marks.
pub(crate) const DIM: f32 = 0.38;

pub(crate) struct Builder<'a> {
    pub chart: &'a Chart,
    pub theme: &'a ChartTheme,
    pub items: Vec<Item>,
    pub hits: Vec<Hit>,
}

impl<'a> Builder<'a> {
    fn new(chart: &'a Chart, theme: &'a ChartTheme) -> Self {
        Self {
            chart,
            theme,
            items: Vec::new(),
            hits: Vec::new(),
        }
    }

    pub fn color(&self, i: usize) -> Color {
        self.theme.series_color(i, &self.chart.colors)
    }

    fn push(&mut self, shape: Shape, key: Option<&str>, active: bool, dim: f32) {
        if let Shape::Fill(p, _) | Shape::Stroke(p, _, _) = &shape {
            if p.is_empty() {
                return;
            }
        }
        self.items.push(Item {
            shape,
            key: key.map(str::to_string),
            active,
            dim,
        });
    }

    /// Chrome: never dims.
    pub fn fill(&mut self, path: Path, color: Color) {
        self.push(Shape::Fill(path, color), None, true, 1.0);
    }

    pub fn stroke(&mut self, path: Path, color: Color, width: f32) {
        self.push(
            Shape::Stroke(path, color, Stroke::solid(width)),
            None,
            true,
            1.0,
        );
    }

    pub fn stroke_track(&mut self, path: Path, color: Color, stroke: Stroke) {
        self.push(Shape::Stroke(path, color, stroke), None, true, 1.0);
    }

    pub fn text(
        &mut self,
        text: impl Into<String>,
        at: [f32; 2],
        size: f32,
        weight: u16,
        color: Color,
        anchor: Anchor,
    ) {
        let text = text.into();
        if text.is_empty() {
            return;
        }
        self.push(
            Shape::Text(TextItem {
                text,
                x: at[0],
                y: at[1],
                size,
                weight,
                color,
                anchor,
            }),
            None,
            true,
            1.0,
        );
    }

    /// Data mark in hover group `key`.
    pub fn mark(&mut self, shape: Shape, key: &str, active: bool, dim: f32) {
        self.push(shape, Some(key), active, dim);
    }

    pub fn hit(
        &mut self,
        region: Region,
        key: Option<&str>,
        title: impl Into<String>,
        lines: Vec<TipLine>,
    ) {
        self.hits.push(Hit {
            region,
            key: key.map(str::to_string),
            title: title.into(),
            lines,
        });
    }

    pub fn tip(&self, label: &str, value: f64, color: Color) -> TipLine {
        TipLine {
            label: label.to_string(),
            value: format_number(value),
            color,
        }
    }
}
