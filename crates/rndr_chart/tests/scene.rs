use rndr_chart::{
    Chart, ChartKind, ChartScene, ChartTheme, Item, Matrix, Orientation, Point, Region, Series,
    Shape, Surface, ValueLabels,
};
use rndr_paint::Affine;

fn sales(kind: ChartKind) -> Chart {
    Chart::new(kind)
        .categories(["Jan", "Feb", "Mar"])
        .series(Series::new("a", "North", vec![10.0, 20.0, 30.0]))
        .series(Series::new("b", "South", vec![5.0, 15.0, 5.0]))
        .validate()
        .unwrap()
}

fn layout(chart: &Chart) -> ChartScene {
    ChartScene::new(
        chart,
        &ChartTheme::dark(),
        320.0,
        chart.default_height(320.0),
    )
}

fn fills(scene: &ChartScene, key: &str) -> Vec<rndr_core::Rect> {
    scene
        .items
        .iter()
        .filter(|i| i.key.as_deref() == Some(key))
        .filter_map(|i| match &i.shape {
            Shape::Fill(p, _) => p.bounds(),
            _ => None,
        })
        .collect()
}

fn texts(scene: &ChartScene) -> Vec<&str> {
    scene
        .items
        .iter()
        .filter_map(|i| match &i.shape {
            Shape::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .collect()
}

fn scene_center(scene: &ChartScene, r: Region) -> [f32; 2] {
    let local = match r {
        Region::Rect(r) => [r.x + r.w / 2.0, r.y + r.h / 2.0],
        Region::Circle(c, _) => c,
        Region::Wedge {
            center,
            inner,
            outer,
            start,
            end,
        } => {
            let a = (start + end) / 2.0;
            let r = (inner + outer) / 2.0;
            [center[0] + a.sin() * r, center[1] - a.cos() * r]
        }
    };
    scene.view.transform_point(local)
}

#[test]
fn every_family_lays_out_marks_and_hits() {
    let mut scatter = Chart::new(ChartKind::Scatter);
    scatter.points = (0..6)
        .map(|i| Point {
            x: i as f64,
            y: (i * i) as f64,
            group: Some(if i % 2 == 0 { "even" } else { "odd" }.into()),
        })
        .collect();
    let mut heat = Chart::new(ChartKind::Heatmap);
    heat.matrix = Some(Matrix {
        rows: vec!["Mon".into(), "Tue".into()],
        cols: vec!["AM".into(), "PM".into(), "Eve".into()],
        values: vec![vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0]],
    });
    let mut charts: Vec<Chart> = [
        ChartKind::Bar,
        ChartKind::Line,
        ChartKind::Area,
        ChartKind::Pie,
        ChartKind::Donut,
        ChartKind::Radar,
        ChartKind::Radial,
        ChartKind::Combo,
    ]
    .into_iter()
    .map(sales)
    .collect();
    charts.push(scatter.validate().unwrap());
    charts.push(heat.validate().unwrap());
    for chart in &charts {
        let scene = layout(chart);
        assert!(
            scene.items.iter().any(|i| i.key.is_some()),
            "{:?} has no marks",
            chart.kind
        );
        assert!(!scene.hits.is_empty(), "{:?} has no hits", chart.kind);
        for hit in &scene.hits {
            let found = scene.hit(scene_center(&scene, hit.region)).unwrap();
            assert!(!found.title.is_empty(), "{:?}", chart.kind);
        }
    }
}

#[test]
fn grouped_bars_scale_to_the_largest_value() {
    let chart = sales(ChartKind::Bar);
    let scene = layout(&chart);
    let mar = fills(&scene, "Mar");
    assert_eq!(mar.len(), 2);
    let plot_h = 232.0 - 10.0 - 28.0;
    assert!((mar[0].h - plot_h).abs() < 0.01, "{}", mar[0].h);
    assert!((mar[1].h - plot_h / 6.0).abs() < 0.01);
    assert!(
        (mar[0].y + mar[0].h - plot_h).abs() < 0.01,
        "bars sit on the baseline"
    );
    assert!(mar[0].x < mar[1].x);
    let hit = scene.hits.iter().find(|h| h.title == "Feb").unwrap();
    let values: Vec<&str> = hit.lines.iter().map(|l| l.value.as_str()).collect();
    assert_eq!(values, ["20", "15"]);
}

#[test]
fn stacked_bars_share_a_column_and_label_totals() {
    let mut chart = sales(ChartKind::Bar);
    chart.stacked = true;
    chart.labels = ValueLabels::Value;
    let scene = layout(&chart);
    let jan = fills(&scene, "Jan");
    assert_eq!(jan.len(), 2);
    assert!((jan[0].x - jan[1].x).abs() < 0.01);
    assert!(
        (jan[1].y + jan[1].h - jan[0].y).abs() < 0.01,
        "second segment sits on the first"
    );
    assert!(texts(&scene).contains(&"35"));
}

#[test]
fn horizontal_bars_grow_rightward() {
    let mut chart = sales(ChartKind::Bar);
    chart.orientation = Orientation::Horizontal;
    let scene = layout(&chart);
    let mar = fills(&scene, "Mar");
    assert!(mar[0].w > mar[0].h);
    assert!(mar[0].x.abs() < 0.01);
    assert_eq!(scene.view, Affine::translate(56.0, 8.0));
}

#[test]
fn combo_draws_bars_left_and_lines_on_a_right_axis() {
    let chart = sales(ChartKind::Combo);
    let scene = layout(&chart);
    assert_eq!(
        fills(&scene, "Jan").len(),
        1,
        "only the bar series is a bar"
    );
    assert!(scene
        .items
        .iter()
        .any(|i| i.key.as_deref() == Some("b") && matches!(i.shape, Shape::Stroke(..))));
    let labels = texts(&scene);
    assert_eq!(
        labels[..8],
        ["0", "10", "20", "30", "0", "5", "10", "15"],
        "left ticks cover 30 × 1.08, right ticks cover the line series"
    );
}

#[test]
fn ticks_are_round_and_fit_the_domain() {
    let scene = layout(&sales(ChartKind::Bar));
    let grid = scene
        .items
        .iter()
        .filter(|i| i.key.is_none() && matches!(i.shape, Shape::Stroke(..)))
        .count();
    assert_eq!(&texts(&scene)[..4], ["0", "10", "20", "30"]);
    assert_eq!(grid, 4, "three tick rules plus the baseline");
}

#[test]
fn line_hits_sit_on_points() {
    let scene = layout(&sales(ChartKind::Line));
    let hit = scene
        .hits
        .iter()
        .find(|h| h.key.as_deref() == Some("a") && h.title == "Mar")
        .unwrap();
    let Region::Circle(c, _) = hit.region else {
        panic!()
    };
    assert!(c[1].abs() < 0.01, "the max sits on the top grid line");
}

#[test]
fn pie_slices_are_proportional_wedges() {
    let chart = sales(ChartKind::Pie);
    let scene = layout(&chart);
    let wedges: Vec<(f32, f32)> = scene
        .hits
        .iter()
        .map(|h| match h.region {
            Region::Wedge {
                start, end, inner, ..
            } => {
                assert_eq!(inner, 0.0);
                (start, end)
            }
            _ => panic!(),
        })
        .collect();
    let tau = std::f32::consts::TAU;
    assert!((wedges[0].1 - tau / 6.0).abs() < 1e-4);
    assert!((wedges[2].1 - tau).abs() < 1e-4);
    assert_eq!(scene.hits[1].lines[0].label, "North");
    assert!((scene.view.uniform_scale() - 272.0 / 260.0).abs() < 1e-4);
}

#[test]
fn donut_center_shows_total_or_override() {
    let mut chart = sales(ChartKind::Donut);
    assert!(texts(&layout(&chart)).contains(&"60"));
    chart.center_value = "$1.2M".into();
    chart.center_label = "Revenue".into();
    let scene = layout(&chart);
    let t = texts(&scene);
    assert!(t.contains(&"$1.2M") && t.contains(&"Revenue"));
    let Region::Wedge { inner, .. } = scene.hits[0].region else {
        panic!()
    };
    assert_eq!(inner, 44.0);
}

#[test]
fn polar_charts_fit_and_center() {
    let chart = sales(ChartKind::Radar);
    let scene = ChartScene::new(&chart, &ChartTheme::dark(), 600.0, 150.0);
    assert!((scene.view.uniform_scale() - 0.5).abs() < 1e-4);
    let origin = scene.view.transform_point([0.0, 0.0]);
    assert!((origin[0] - 225.0).abs() < 1e-3 && origin[1].abs() < 1e-3);
}

#[test]
fn radial_single_ring_reports_progress() {
    let chart = Chart::new(ChartKind::Radial)
        .categories(["Done"])
        .series(Series::new("p", "Progress", vec![72.0]))
        .validate()
        .unwrap();
    let scene = layout(&chart);
    assert!(texts(&scene).contains(&"72"));
    assert!(texts(&scene).contains(&"Progress"));
    assert_eq!(scene.hits.len(), 1);
    assert_eq!(scene.hits[0].key.as_deref(), Some("single"));
    let rings = layout(&sales(ChartKind::Radial));
    assert_eq!(rings.hits.len(), 3, "one ring per category");
}

#[test]
fn scatter_groups_points_and_reports_coordinates() {
    let mut chart = Chart::new(ChartKind::Scatter);
    chart.points = vec![
        Point {
            x: 1.0,
            y: 2.0,
            group: Some("A".into()),
        },
        Point {
            x: 3.0,
            y: 4.0,
            group: Some("B".into()),
        },
        Point {
            x: 5.0,
            y: 1.0,
            group: None,
        },
    ];
    let chart = chart.validate().unwrap();
    let scene = layout(&chart);
    let titles: Vec<&str> = scene.hits.iter().map(|h| h.title.as_str()).collect();
    assert_eq!(titles, ["A", "B", "A"]);
    assert_eq!(scene.hits[1].lines[0].value, "3");
    assert_eq!(scene.hits[1].lines[1].value, "4");
    assert_eq!(scene.legend.len(), 2);
}

#[test]
fn heatmap_sizes_rows_and_shades_by_value() {
    let mut chart = Chart::new(ChartKind::Heatmap);
    chart.matrix = Some(Matrix {
        rows: vec!["Mon".into(), "Tue".into()],
        cols: vec!["AM".into(), "PM".into()],
        values: vec![vec![0.0, 5.0], vec![10.0, 10.0]],
    });
    let chart = chart.validate().unwrap();
    let scene = layout(&chart);
    assert!((scene.height - chart.default_height(320.0)).abs() < 1e-3);
    let alpha_of = |key: &str| {
        scene
            .items
            .iter()
            .find_map(|i: &Item| match (&i.shape, i.key.as_deref()) {
                (Shape::Fill(_, c), Some(k)) if k == key => Some(c.a),
                _ => None,
            })
            .unwrap()
    };
    assert!((alpha_of("Mon · AM") - 0.18).abs() < 1e-4);
    assert!((alpha_of("Tue · PM") - 0.96).abs() < 1e-4);
    assert_eq!(scene.hits[1].title, "Mon · PM");
    assert!(scene.legend.is_empty());
}

#[test]
fn tooltip_and_interaction_off_drop_hits() {
    let mut chart = sales(ChartKind::Bar);
    chart.tooltip = false;
    chart.interactive = false;
    assert!(layout(&chart).hits.is_empty());
}

#[test]
fn active_key_marks_matching_items() {
    let mut chart = sales(ChartKind::Line);
    chart.active_key = "south".into();
    let scene = layout(&chart);
    assert!(scene
        .items
        .iter()
        .filter(|i| i.key.as_deref() == Some("a"))
        .all(|i| !i.active));
    assert!(scene
        .items
        .iter()
        .filter(|i| i.key.as_deref() == Some("b"))
        .all(|i| i.active));
    assert!(scene.legend.iter().any(|l| l.label == "South" && l.active));
    assert!(scene.legend.iter().any(|l| l.label == "North" && !l.active));
}

#[test]
fn plate_follows_surface() {
    let mut chart = sales(ChartKind::Pie);
    let plain = layout(&chart)
        .items
        .iter()
        .filter(|i| i.key.is_none())
        .count();
    chart.surface = Surface::Muted;
    let plated = layout(&chart)
        .items
        .iter()
        .filter(|i| i.key.is_none())
        .count();
    assert_eq!(plated, plain + 1);
}
