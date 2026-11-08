//! Charts painted with [`rndr_paint`].
//!
//! Ten families share one description, [`Chart`]: bar, line, area, pie,
//! donut, radar, radial, scatter, combo and heatmap. Painting has two steps:
//!
//! 1. [`ChartScene::new`] lays a validated chart out in a box. It is
//!    headless: paths, labels and hover regions, with tooltips for each
//!    region. Hosts hit-test pointer positions with [`ChartScene::hit`].
//! 2. [`paint`] records the scene into a [`rndr_paint::Canvas`], dimming the
//!    marks outside a hovered group. [`ChartPainter`] does both for a surface
//!    slot.
//!
//! The scene is the plot body. The title, description, legend
//! ([`ChartScene::legend`]) and footer are left to the host's own widgets.
//!
//! ```
//! use rndr_chart::{Chart, ChartKind, ChartScene, ChartTheme, Series};
//!
//! let chart = Chart::new(ChartKind::Bar)
//!     .categories(["Q1", "Q2", "Q3"])
//!     .series(Series::new("rev", "Revenue", vec![12.0, 18.0, 9.0]))
//!     .validate()
//!     .unwrap();
//! let scene = ChartScene::new(&chart, &ChartTheme::dark(), 320.0, 232.0);
//! let hit = scene.hit([170.0, 150.0]).unwrap();
//! assert_eq!(hit.title, "Q2");
//! assert_eq!(hit.lines[0].value, "18");
//! ```
//!
//! With the `liquid` feature, [`Chart::from_liquid`] reads a
//! ` ```chart ` fence parsed by `rndr_liquid`.

#![deny(missing_docs)]

mod cartesian;
mod format;
mod geom;
mod heatmap;
#[cfg(feature = "liquid")]
mod liquid;
mod model;
mod paint;
mod polar;
mod scene;
mod theme;

pub use format::{format_label, format_number};
pub use model::{
    Chart, ChartError, ChartKind, Curve, LabelPosition, Legend, Matrix, Orientation, Point, Series,
    SeriesMark, TrendDirection, ValueLabels,
};
pub use paint::{paint, ChartPainter};
pub use scene::{Anchor, ChartScene, Hit, Item, LegendEntry, Region, Shape, TextItem, TipLine};
pub use theme::{alpha, parse_hex, ChartColor, ChartTheme, Surface, Swatch};

impl Chart {
    /// Height the reference renderer gives this chart at `width`.
    ///
    /// Heatmaps size to their rows; polar charts are square up to their
    /// view box size; the rest are 232 points tall.
    pub fn default_height(&self, width: f32) -> f32 {
        match self.kind {
            ChartKind::Heatmap => heatmap::height(self.matrix.as_ref().map_or(0, |m| m.rows.len())),
            ChartKind::Pie | ChartKind::Donut => width.min(272.0),
            ChartKind::Radar => width.min(304.0),
            ChartKind::Radial => width.min(240.0),
            _ => 232.0,
        }
    }
}
