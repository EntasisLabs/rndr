#![cfg(feature = "liquid")]

use rndr_chart::{
    Chart, ChartColor, ChartKind, ChartScene, ChartTheme, Curve, Legend, Orientation, SeriesMark,
    Surface, Swatch, ValueLabels,
};
use rndr_liquid::{parse_fence, LiquidBlock};

fn chart(body: &str) -> Chart {
    let Some(LiquidBlock::Chart(props)) = parse_fence("chart", body) else {
        panic!("not a chart: {body}");
    };
    Chart::from_liquid(&props).unwrap()
}

#[test]
fn bar_fence_maps_options() {
    let c = chart(
        "type: bar\ntitle: Revenue\nlayout: horizontal\nstacked: true\nlabels: value\n\
         legend: top\ncolors: green, #ff0000, nope\nsurface: muted\n\n\
         | Quarter | North | South |\n|---|---|---|\n| Q1 | 10 | 5 |\n| Q2 | 20 | 15 |",
    );
    assert_eq!(c.kind, ChartKind::Bar);
    assert_eq!(c.title, "Revenue");
    assert_eq!(c.orientation, Orientation::Horizontal);
    assert!(c.stacked);
    assert_eq!(c.labels, ValueLabels::Value);
    assert_eq!(c.legend, Legend::Top);
    assert_eq!(c.categories, ["Q1", "Q2"]);
    assert_eq!(c.series.len(), 2);
    assert_eq!(c.series[1].values, [5.0, 15.0]);
    assert_eq!(c.colors[0], ChartColor::Swatch(Swatch::Green));
    assert!(matches!(c.colors[1], ChartColor::Rgba(_)));
    assert_eq!(
        c.colors[2],
        ChartColor::Swatch(Swatch::Green),
        "palette slot 3 is green"
    );
    assert_eq!(c.surface, Surface::Muted);
    assert!(c.tooltip && c.interactive && c.separator);
}

#[test]
fn every_liquid_fixture_family_lays_out() {
    let table =
        "\n\n| Month | A | B |\n|---|---|---|\n| Jan | 1 | 4 |\n| Feb | 3 | 2 |\n| Mar | 5 | 6 |";
    for kind in [
        "bar", "line", "area", "pie", "donut", "radar", "radial", "combo",
    ] {
        let c = chart(&format!("type: {kind}{table}"));
        assert_eq!(c.kind.as_str(), kind);
        let scene = ChartScene::new(&c, &ChartTheme::dark(), 320.0, c.default_height(320.0));
        assert!(!scene.hits.is_empty(), "{kind}");
    }
    let combo = chart(&format!(
        "type: combo\ncurve: step\nseriesMarks: line, bar{table}"
    ));
    assert_eq!(combo.curve, Curve::Step);
    assert_eq!(combo.series_mark(0), SeriesMark::Line);
    assert_eq!(combo.series_mark(1), SeriesMark::Bar);
}
