use rndr_canvas::text::EstimateMeasure;
use rndr_canvas::*;
use rndr_canvas_ui::paint::*;
use rndr_core::{Color, World};
use rndr_paint::Canvas;

fn rect(id: &str, x: f64, fill: &str) -> Node {
    Node::new(
        id,
        Content::Shape {
            shape: ShapeKind::Rectangle,
            fill: fill.into(),
            stroke: "#000000".into(),
            stroke_width: 2.0,
            line: None,
        },
        Transform {
            x,
            y: 0.0,
            width: 100.0,
            height: 50.0,
            rotation: 0.0,
        },
    )
}

#[test]
fn colors_parse_short_long_and_alpha_forms() {
    assert_eq!(parse_color("#fff"), Color::rgba(1.0, 1.0, 1.0, 1.0));
    assert_eq!(parse_color("#ff000080").a, 128.0 / 255.0);
    assert_eq!(parse_color("transparent").a, 0.0);
    assert_eq!(
        parse_color("#00f8"),
        Color::rgba(0.0, 0.0, 1.0, 136.0 / 255.0)
    );
}

#[test]
fn utf16_offsets_map_to_byte_boundaries() {
    let text = "a😀é";
    assert_eq!(utf16_to_byte(text, 0), 0);
    assert_eq!(utf16_to_byte(text, 1), 1);
    assert_eq!(utf16_to_byte(text, 3), 5);
    assert_eq!(utf16_to_byte(text, 4), 7);
    assert_eq!(utf16_to_byte(text, 99), text.len());
}

#[test]
fn ink_outline_follows_points_and_widens_with_pressure() {
    let brush = Brush {
        size: 8.0,
        ..Brush::default()
    };
    let points = |p: f64| -> Vec<InkPoint> {
        (0..10)
            .map(|i| InkPoint {
                pressure: Some(p),
                ..InkPoint::new(i as f64 * 10.0, 0.0)
            })
            .collect()
    };
    let light = ink_outline(&brush, &points(0.2)).bounds().unwrap();
    let heavy = ink_outline(&brush, &points(1.0)).bounds().unwrap();
    assert!(light.w >= 89.0 && light.w <= 100.0, "{light:?}");
    assert!(heavy.h > light.h, "{light:?} {heavy:?}");
    let dot = ink_outline(&brush, &[InkPoint::new(5.0, 5.0)])
        .bounds()
        .unwrap();
    assert!(dot.w > 0.0 && (dot.w - dot.h).abs() < 0.5);
}

#[test]
fn visual_bounds_include_stroke_and_shadow() {
    let mut node = rect("a", 10.0, "#ff0000");
    let plain = visual_bounds(
        &Scene {
            nodes: vec![node.clone()],
            ..Scene::default()
        },
        &EstimateMeasure,
    )
    .unwrap();
    assert!(
        plain.x < 10.0 && plain.width > 100.0,
        "stroke widens the box: {plain:?}"
    );
    node.appearance = Some(Appearance {
        corner_radius: 0.0,
        shadow: Some(Shadow {
            x: 0.0,
            y: 20.0,
            blur: 10.0,
            color: "#000000".into(),
            opacity: 0.5,
        }),
    });
    let shadowed = visual_bounds(
        &Scene {
            nodes: vec![node],
            ..Scene::default()
        },
        &EstimateMeasure,
    )
    .unwrap();
    assert!(
        shadowed.y + shadowed.height > plain.y + plain.height + 20.0,
        "{shadowed:?}"
    );
    assert!(visual_bounds(&Scene::default(), &EstimateMeasure).is_none());
}

#[test]
fn hidden_nodes_and_their_children_are_not_painted() {
    let world = World::new();
    let paint = |nodes: Vec<Node>| {
        let mut canvas = Canvas::new();
        paint_scene(
            &mut canvas,
            &world,
            &Scene {
                nodes,
                ..Scene::default()
            },
            &ImageIds::new(),
            &EstimateMeasure,
        );
        canvas.into_scene().commands.len()
    };
    let background = paint(vec![]);
    let one = paint(vec![rect("a", 0.0, "#ff0000")]);
    assert!(one > background);
    let mut frame = Node::new(
        "f",
        Content::Frame {
            name: "F".into(),
            background: "#ffffff".into(),
        },
        Transform {
            x: 0.0,
            y: 0.0,
            width: 300.0,
            height: 300.0,
            rotation: 0.0,
        },
    );
    let child = rect("c", 10.0, "#00ff00").with_parent("f");
    let shown = paint(vec![frame.clone(), child.clone()]);
    frame.visible = false;
    assert_eq!(paint(vec![frame, child]), background);
    assert!(shown > one);
}
