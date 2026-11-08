use rndr_chart::{
    format_label, format_number, Chart, ChartColor, ChartError, ChartKind, ChartTheme,
    LabelPosition, Legend, Matrix, Point, Series, SeriesMark, Surface, Swatch, ValueLabels,
};

fn sales(kind: ChartKind) -> Chart {
    Chart::new(kind)
        .categories(["Jan", "Feb", "Mar"])
        .series(Series::new("a", "North", vec![10.0, 20.0, 30.0]))
        .series(Series::new("b", "South", vec![5.0, 15.0, 5.0]))
}

#[test]
fn validation_matches_each_family() {
    for kind in ChartKind::ALL {
        let chart = Chart::new(kind)
            .categories(["A"])
            .series(Series::new("s", "S", vec![1.0]));
        let ok = chart.validate().is_ok();
        assert_eq!(ok, kind == ChartKind::Radial, "{kind:?}");
    }
    let radar = sales(ChartKind::Radar).categories(["A", "B"]);
    assert_eq!(
        radar.validate(),
        Err(ChartError::Categories {
            needed: 3,
            found: 2
        })
    );
    let empty = Chart::new(ChartKind::Line)
        .categories(["A", "B"])
        .series(Series::new("s", "S", vec![f64::NAN]));
    assert_eq!(empty.validate(), Err(ChartError::NoSeries));

    let mut scatter = Chart::new(ChartKind::Scatter);
    scatter.points = vec![Point {
        x: 1.0,
        y: 2.0,
        group: None,
    }];
    assert_eq!(scatter.clone().validate(), Err(ChartError::Points));
    scatter.points.push(Point {
        x: 2.0,
        y: 3.0,
        group: None,
    });
    assert!(scatter.validate().is_ok());

    let mut heat = Chart::new(ChartKind::Heatmap);
    heat.matrix = Some(Matrix {
        rows: vec!["r".into()],
        cols: vec!["a".into(), "b".into()],
        values: vec![vec![1.0]],
    });
    assert_eq!(heat.clone().validate(), Err(ChartError::Matrix));
    heat.matrix.as_mut().unwrap().values = vec![vec![1.0, 2.0]];
    assert!(heat.validate().is_ok());
}

#[test]
fn validation_fills_series_names() {
    let chart = Chart::new(ChartKind::Bar)
        .categories(["A", " ", "B"])
        .series(Series::new("", "", vec![1.0, 2.0]))
        .validate()
        .unwrap();
    assert_eq!(chart.categories, ["A", "B"]);
    assert_eq!(chart.series[0].key, "series-0");
    assert_eq!(chart.series[0].label, "Series 1");
}

#[test]
fn maxima_follow_stacking_and_combo_marks() {
    let mut chart = sales(ChartKind::Bar);
    assert_eq!(chart.value_max(), 30.0);
    chart.stacked = true;
    assert_eq!(chart.value_max(), 35.0);
    let combo = sales(ChartKind::Combo);
    assert_eq!(combo.series_mark(0), SeriesMark::Bar);
    assert_eq!(combo.series_mark(1), SeriesMark::Line);
    assert_eq!(combo.value_max_for(SeriesMark::Bar), 30.0);
    assert_eq!(combo.value_max_for(SeriesMark::Line), 15.0);
    let zero = Chart::new(ChartKind::Bar)
        .categories(["A", "B"])
        .series(Series::new("s", "S", vec![0.0, 0.0]));
    assert_eq!(zero.value_max(), 1.0);
}

#[test]
fn legend_and_label_defaults() {
    assert_eq!(
        sales(ChartKind::Bar).legend_placement(),
        Some(Legend::Bottom)
    );
    let one = Chart::new(ChartKind::Bar).series(Series::new("s", "S", vec![1.0]));
    assert_eq!(one.legend_placement(), None);
    let mut top = one.clone();
    top.legend = Legend::Top;
    assert_eq!(top.legend_placement(), Some(Legend::Top));
    assert_eq!(sales(ChartKind::Pie).legend_items(), ["Jan", "Feb", "Mar"]);
    assert_eq!(sales(ChartKind::Line).legend_items(), ["North", "South"]);
    assert!(sales(ChartKind::Heatmap).legend_items().is_empty());

    let mut donut = sales(ChartKind::Donut);
    assert_eq!(donut.resolved_label_position(), None);
    donut.labels = ValueLabels::Value;
    assert_eq!(donut.resolved_label_position(), Some(LabelPosition::Inside));
    donut.center_value = "42".into();
    assert_eq!(
        donut.resolved_label_position(),
        Some(LabelPosition::Outside)
    );
    let mut pie = sales(ChartKind::Pie);
    pie.labels = ValueLabels::Both;
    assert_eq!(pie.resolved_label_position(), Some(LabelPosition::Outside));
}

#[test]
fn active_key_matches_case_insensitively() {
    let mut chart = sales(ChartKind::Bar);
    assert!(chart.is_active(&["anything"]));
    chart.active_key = " north ".into();
    assert!(chart.is_active(&["a", "North"]));
    assert!(!chart.is_active(&["b", "South"]));
}

#[test]
fn numbers_and_labels() {
    assert_eq!(format_number(0.0), "0");
    assert_eq!(format_number(1234567.0), "1,234,567");
    assert_eq!(format_number(12.345), "12.35");
    assert_eq!(format_number(-1500.5), "-1,500.5");
    assert_eq!(format_label(ValueLabels::Both, "Jan", 3.0), "Jan 3");
    assert_eq!(format_label(ValueLabels::Category, "Jan", 3.0), "Jan");
    assert_eq!(format_label(ValueLabels::None, "Jan", 3.0), "");
}

#[test]
fn colors_and_surfaces() {
    let theme = ChartTheme::light();
    assert_eq!(
        ChartColor::parse("Grey"),
        Some(ChartColor::Swatch(Swatch::Gray))
    );
    let ChartColor::Rgba(c) = ChartColor::parse("#f00").unwrap() else {
        panic!()
    };
    assert_eq!((c.r, c.g, c.b, c.a), (1.0, 0.0, 0.0, 1.0));
    assert_eq!(ChartColor::parse("#12345"), None);
    assert_eq!(theme.series_color(0, &[]), theme.swatch(Swatch::Blue));
    assert_eq!(theme.series_color(6, &[]), theme.swatch(Swatch::Purple));
    let over = [ChartColor::Swatch(Swatch::Red)];
    assert_eq!(theme.series_color(0, &over), theme.swatch(Swatch::Red));

    assert_eq!(Surface::parse(""), Surface::Default);
    assert_eq!(Surface::parse("none"), Surface::Transparent);
    assert_eq!(Surface::parse("soft/0.2"), Surface::Ink(0.2));
    assert_eq!(Surface::parse("muted"), Surface::Muted);
    assert_eq!(
        Surface::parse("blue @ 30"),
        Surface::Tint(ChartColor::Swatch(Swatch::Blue), 0.3)
    );
    assert_eq!(
        Surface::parse("gray"),
        Surface::Tint(ChartColor::Swatch(Swatch::Gray), 0.12)
    );
    assert_eq!(theme.surface(Surface::Transparent), None);
    assert_eq!(theme.surface(Surface::Default).unwrap().a, theme.plot_alpha);
}
