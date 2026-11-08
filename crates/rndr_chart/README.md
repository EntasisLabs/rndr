# rndr_chart

Charts for rndr, painted with `rndr_paint`: bar, line, area, pie, donut,
radar, radial, scatter, combo and heatmap.

A `Chart` describes the data and options. `Chart::validate` drops blank
categories and non-finite values, names unnamed series, and refuses charts
a family cannot draw (radar under three categories, scatter under two
points, a ragged heatmap). Painting has two steps:

1. `ChartScene::new(&chart, &theme, width, height)` lays the plot body out
   headlessly: paths, labels, and hover regions with tooltip rows.
2. `paint` records the scene into a `rndr_paint::Canvas` at an origin,
   dimming marks outside the hovered group. `ChartPainter` does both for a
   surface slot.

```rust
use rndr_chart::{Chart, ChartKind, ChartScene, ChartTheme, Series};

let chart = Chart::new(ChartKind::Line)
    .categories(["Jan", "Feb", "Mar"])
    .series(Series::new("north", "North", vec![12.0, 19.0, 15.0]))
    .series(Series::new("south", "South", vec![8.0, 11.0, 14.0]))
    .validate()?;
let theme = ChartTheme::dark();
let scene = ChartScene::new(&chart, &theme, 320.0, chart.default_height(320.0));
if let Some(hit) = scene.hit(pointer_in_chart) {
    // hit.key groups the marks to keep lit; hit.title and hit.lines are the tooltip.
}
```

## Host responsibilities

The scene is the plot body only. Title, description, legend
(`ChartScene::legend`), trend/caption footer and the tooltip card are the
host's widgets, so they follow the host's typography and layout.

Hover is pull-based: hit-test the pointer with `ChartScene::hit`, then pass
the hit's key to `paint` or `ChartPainter::hover`. On the desktop runner, set
`rndr_core::MotionWatch { active: true }` so cursor motion without a button
requests frames.

## Colors

`ChartTheme::dark()` and `ChartTheme::light()` hold explicit colors: eight
named swatches, the default series palette, text, grid and card tones, and
an optional font family. Per-chart `colors` take swatch names or `#rgb`,
`#rrggbb`, `#rrggbbaa`; `surface` takes `soft`, `muted`, `strong`, `none`
or a color with `/alpha`.

## Liquid

With the `liquid` feature, `Chart::from_liquid` converts a
`rndr_liquid::ChartProps` parsed from a ` ```chart ` fence.

## Example

```text
cargo run -p chart-gallery                       # hover to dim, tooltip on stdout
cargo run -p chart-gallery -- --export out.png   # all ten families to a PNG
cargo run -p chart-gallery -- --light --export out.png
```
