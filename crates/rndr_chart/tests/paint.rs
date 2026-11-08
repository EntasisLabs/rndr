use rndr_chart::{paint, Chart, ChartKind, ChartPainter, ChartScene, ChartTheme, Series};
use rndr_core::{Rect, World};
use rndr_paint::{Canvas, CanvasPainter, Op};

fn chart() -> Chart {
    Chart::new(ChartKind::Bar)
        .categories(["Jan", "Feb"])
        .series(Series::new("a", "North", vec![1.0, 2.0]))
        .series(Series::new("b", "South", vec![2.0, 1.0]))
        .validate()
        .unwrap()
}

fn count(ops: &[Op], f: impl Fn(&Op) -> bool) -> usize {
    ops.iter().filter(|op| f(op)).count()
}

#[test]
fn paint_records_paths_and_text() {
    let world = World::new();
    let theme = ChartTheme::dark();
    let scene = ChartScene::new(&chart(), &theme, 300.0, 200.0);
    let mut canvas = Canvas::new();
    paint(&scene, &theme, &world, &mut canvas, [10.0, 20.0], None);
    let ops = &canvas.scene().commands;
    let shapes = scene
        .items
        .iter()
        .filter(|i| !matches!(i.shape, rndr_chart::Shape::Text(_)))
        .count();
    assert_eq!(count(ops, |op| matches!(op, Op::Path(_))), shapes);
    assert!(count(ops, |op| matches!(op, Op::Glyphs(_))) > 0);
    assert_eq!(
        count(ops, |op| matches!(op, Op::PushLayer(_))),
        0,
        "nothing dims at rest"
    );
    assert_eq!(
        count(ops, |op| matches!(op, Op::PushTransform(_))),
        count(ops, |op| matches!(op, Op::PopTransform))
    );
}

#[test]
fn hover_dims_other_groups() {
    let world = World::new();
    let theme = ChartTheme::dark();
    let scene = ChartScene::new(&chart(), &theme, 300.0, 200.0);
    let mut canvas = Canvas::new();
    paint(&scene, &theme, &world, &mut canvas, [0.0, 0.0], Some("Jan"));
    let feb = scene
        .items
        .iter()
        .filter(|i| i.key.as_deref() == Some("Feb"))
        .count();
    let layers: Vec<f32> = canvas
        .scene()
        .commands
        .iter()
        .filter_map(|op| match op {
            Op::PushLayer(l) => Some(l.opacity),
            _ => None,
        })
        .collect();
    assert_eq!(layers.len(), feb);
    assert!(layers.iter().all(|o| (*o - 0.38).abs() < 1e-6));

    let mut still = scene.clone();
    still.interactive = false;
    let mut canvas = Canvas::new();
    paint(&still, &theme, &world, &mut canvas, [0.0, 0.0], Some("Jan"));
    assert_eq!(
        count(&canvas.scene().commands, |op| matches!(
            op,
            Op::PushLayer(_)
        )),
        0
    );
}

#[test]
fn painter_lays_out_in_its_rect_and_dims_around_hover() {
    let world = World::new();
    let rect = Rect {
        x: 40.0,
        y: 30.0,
        w: 300.0,
        h: 200.0,
    };
    let mut painter = ChartPainter::new(chart(), ChartTheme::light(), rect);
    let mut canvas = Canvas::new();
    painter.paint(&world, &mut canvas);
    let ops = &canvas.scene().commands;
    assert!(count(ops, |op| matches!(op, Op::Path(_))) > 0);
    assert_eq!(count(ops, |op| matches!(op, Op::PushLayer(_))), 0);

    painter.hover = painter.scene().hits[0].key.clone();
    let mut canvas = Canvas::new();
    painter.paint(&world, &mut canvas);
    assert!(
        count(&canvas.scene().commands, |op| matches!(
            op,
            Op::PushLayer(_)
        )) > 0
    );
}
