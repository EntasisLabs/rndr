use rndr_canvas::*;
use std::collections::BTreeMap;

const FIXTURES: [&str; 2] = [
    include_str!("fixtures/medousa-shell-wireframe.json"),
    include_str!("fixtures/v3-from-legacy.json"),
];

fn text_node(text: &str, spans: Vec<TextSpan>) -> Node {
    let mut node = Node::new(
        "t",
        Content::Text {
            text: text.into(),
            color: "#000".into(),
            font_family: "Inter".into(),
            font_size: 16.0,
            font_weight: None,
            font_style: None,
        },
        Transform::default(),
    );
    node.text_layout = Some(TextLayout {
        spans,
        ..TextLayout::default()
    });
    node
}

/// JSON with every number as f64, so `1` and `1.0` compare equal.
fn numeric(value: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match value {
        Value::Number(n) => n.as_f64().map_or(Value::Number(n), |f| f.into()),
        Value::Array(items) => Value::Array(items.into_iter().map(numeric).collect()),
        Value::Object(map) => {
            Value::Object(map.into_iter().map(|(k, v)| (k, numeric(v))).collect())
        }
        other => other,
    }
}

fn scene(nodes: Vec<Node>) -> Scene {
    Scene {
        nodes,
        ..Scene::default()
    }
}

fn valid(nodes: Vec<Node>) -> bool {
    validate_scene(&scene(nodes), &BTreeMap::new()).is_ok()
}

#[test]
fn medousa_scenes_roundtrip_without_loss() {
    for fixture in FIXTURES {
        let raw: serde_json::Value = serde_json::from_str(fixture).unwrap();
        let parsed: Scene = serde_json::from_str(fixture).unwrap();
        assert_eq!(parsed.schema, schema::MEDOUSA_SCHEMA);
        assert!(parsed.is_supported());
        validate_scene(&parsed, &BTreeMap::new()).unwrap();
        assert_eq!(
            numeric(serde_json::to_value(&parsed).unwrap()),
            numeric(raw)
        );
    }
}

#[test]
fn frame_origin_is_opaque_and_frames_only() {
    let json = serde_json::json!({
        "id": "f", "parent_id": null,
        "transform": {"x": 0, "y": 0, "width": 10, "height": 10, "rotation": 0},
        "opacity": 1, "visible": true, "locked": false,
        "content": {"kind": "frame", "name": "F", "background": "#fff"},
        "design_source": {"url": "https://example.com", "viewport_width": 800}
    });
    let frame: Node = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(frame.origin.as_ref().unwrap()["viewport_width"], 800);
    assert_eq!(
        numeric(serde_json::to_value(&frame).unwrap()),
        numeric(json)
    );
    assert!(valid(vec![frame.clone()]));
    let mut group = frame;
    group.content = Content::Group;
    assert!(!valid(vec![group]));
}

#[test]
fn unknown_fields_and_schemas_are_refused() {
    let mut raw: serde_json::Value = serde_json::from_str(FIXTURES[1]).unwrap();
    raw["nodes"][0]["surprise"] = true.into();
    assert!(serde_json::from_value::<Scene>(raw).is_err());
    let future = Scene {
        version: 4,
        ..Scene::medousa()
    };
    assert_eq!(
        validate_scene(&future, &BTreeMap::new()),
        Err(CanvasError::Unsupported)
    );
    assert!(Scene::default().is_supported());
}

#[test]
fn ids_paint_and_numbers_are_bounded() {
    let base = || Node::new("ok_id-1", Content::Group, Transform::default());
    assert!(valid(vec![base()]));
    for id in ["", "has space", "../x", &"x".repeat(129)] {
        let mut n = base();
        n.id = id.into();
        assert!(!valid(vec![n]), "{id:?}");
    }
    assert!(!valid(vec![base(), base()]));
    let mut n = base();
    n.transform.x = f64::NAN;
    assert!(!valid(vec![n]));
    let mut n = base();
    n.transform.width = -1.0;
    assert!(!valid(vec![n]));
    let mut n = base();
    n.name = Some("  ".into());
    assert!(!valid(vec![n]));
    for paint in ["#fff", "#ffff", "#ffffff", "#ffffffff", "transparent"] {
        assert!(validate::validate_paint(paint).is_ok(), "{paint}");
    }
    for paint in ["red", "#ff", "#gggggg", "url(x)"] {
        assert!(validate::validate_paint(paint).is_err(), "{paint}");
    }
}

#[test]
fn text_spans_follow_utf16_boundaries() {
    let span = |start, end| TextSpan {
        start,
        end,
        ..TextSpan::default()
    };
    // "a😀b" is 4 UTF-16 units: a=0..1, emoji=1..3, b=3..4.
    assert!(valid(vec![text_node(
        "a😀b",
        vec![span(0, 1), span(1, 3), span(3, 4)]
    )]));
    assert!(!valid(vec![text_node("a😀b", vec![span(0, 2)])]));
    assert!(!valid(vec![text_node("a😀b", vec![span(0, 5)])]));
    assert!(!valid(vec![text_node("abc", vec![span(0, 2), span(1, 3)])]));
    assert!(!valid(vec![text_node("abc", vec![span(1, 1)])]));
    let heavy = TextSpan {
        font_weight: Some(1000),
        ..span(0, 1)
    };
    assert!(!valid(vec![text_node("abc", vec![heavy])]));
}

#[test]
fn content_specific_metadata_is_restricted() {
    let mut group = Node::new("g", Content::Group, Transform::default());
    group.appearance = Some(Appearance::default());
    assert!(!valid(vec![group]));
    let mut layout_on_group = Node::new("g", Content::Group, Transform::default());
    layout_on_group.text_layout = Some(TextLayout::default());
    assert!(!valid(vec![layout_on_group]));
    let line = Line {
        start: LinePoint { x: 0.0, y: 0.0 },
        end: LinePoint { x: 5.0, y: 5.0 },
        start_arrow: false,
        end_arrow: true,
        dashed: false,
    };
    let shape = |shape| {
        Node::new(
            "s",
            Content::Shape {
                shape,
                fill: "transparent".into(),
                stroke: "#000".into(),
                stroke_width: 2.0,
                line: Some(line),
            },
            Transform::default(),
        )
    };
    assert!(valid(vec![shape(ShapeKind::Line)]));
    assert!(!valid(vec![shape(ShapeKind::Rectangle)]));
    let ink = |points: Vec<InkPoint>| {
        Node::new(
            "i",
            Content::Ink {
                color: "#000".into(),
                input: InputKind::Pen,
                brush: Brush::default(),
                points,
            },
            Transform::default(),
        )
    };
    assert!(valid(vec![ink(vec![InkPoint::new(0.0, 0.0)])]));
    assert!(!valid(vec![ink(Vec::new())]));
    let pressured = InkPoint {
        pressure: Some(2.0),
        ..InkPoint::new(0.0, 0.0)
    };
    assert!(!valid(vec![ink(vec![pressured])]));
}

#[test]
fn labels_fall_back_to_content() {
    let fixture: Scene = serde_json::from_str(FIXTURES[0]).unwrap();
    assert_eq!(fixture.node("shell").unwrap().label(), "Shell");
    assert_eq!(text_node("first\nsecond", vec![]).label(), "first");
    assert_eq!(
        Node::new("g", Content::Group, Transform::default()).label(),
        "Group"
    );
}
