use rndr_canvas::*;
use std::collections::BTreeMap;

fn tf(x: f64, y: f64, width: f64, height: f64, rotation: f64) -> Transform {
    Transform {
        x,
        y,
        width,
        height,
        rotation,
    }
}

fn shape(id: &str, kind: ShapeKind, fill: &str, t: Transform) -> Node {
    Node::new(
        id,
        Content::Shape {
            shape: kind,
            fill: fill.into(),
            stroke: "#000".into(),
            stroke_width: 2.0,
            line: None,
        },
        t,
    )
}

fn rect(id: &str, x: f64, y: f64, w: f64, h: f64) -> Node {
    shape(id, ShapeKind::Rectangle, "#fff", tf(x, y, w, h, 0.0))
}

fn ink(id: &str, points: &[(f64, f64)]) -> Node {
    Node::new(
        id,
        Content::Ink {
            color: "#000".into(),
            input: InputKind::Pen,
            brush: Brush {
                size: 2.0,
                ..Brush::default()
            },
            points: points.iter().map(|&(x, y)| InkPoint::new(x, y)).collect(),
        },
        tf(0.0, 0.0, 100.0, 100.0, 0.0),
    )
}

fn doc(nodes: Vec<Node>) -> Document {
    Document {
        id: "d".into(),
        revision: 0,
        title: "D".into(),
        pinned: false,
        created_at: String::new(),
        updated_at: String::new(),
        scene: Scene {
            nodes,
            ..Scene::default()
        },
        assets: BTreeMap::new(),
    }
}

fn ids(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn close_rect(a: Rect, b: Rect) -> bool {
    close(a.x, b.x) && close(a.y, b.y) && close(a.width, b.width) && close(a.height, b.height)
}

fn world_corners(index: &SceneIndex<'_>, id: &str) -> Vec<Vec2> {
    let node = index.get(id).unwrap();
    let m = index.world_matrix(node);
    let (w, h) = index.dimensions(node);
    Rect::new(0.0, 0.0, w, h)
        .corners()
        .iter()
        .map(|p| m.apply(*p))
        .collect()
}

#[test]
fn rotation_turns_about_the_center_and_nests() {
    let nodes = vec![
        Node::new("g", Content::Group, tf(100.0, 0.0, 20.0, 20.0, 90.0)),
        rect("a", 0.0, 0.0, 20.0, 10.0).with_parent("g"),
    ];
    let index = SceneIndex::new(&nodes);
    let center = local_center(&nodes[0]);
    assert!(close(center.x, 110.0) && close(center.y, 10.0));
    // The child's (0,0) maps through the group's 90 degree turn.
    let p = index
        .world_matrix(index.get("a").unwrap())
        .apply(Vec2::new(0.0, 0.0));
    assert!(close(p.x, 120.0) && close(p.y, 0.0), "{p:?}");
    assert!(close_rect(
        index.bounds(index.get("a").unwrap()),
        Rect::new(110.0, 0.0, 10.0, 20.0)
    ));
    let back = index.to_parent(index.get("a").unwrap(), p);
    assert!(close(back.x, 0.0) && close(back.y, 0.0));
}

fn local_center(node: &Node) -> Vec2 {
    let t = node.transform;
    geometry::local_matrix(&t).apply(Vec2::new(t.width / 2.0, t.height / 2.0))
}

#[test]
fn resize_keeps_the_rotated_corner_and_scales_ink() {
    let mut stroke = ink("i", &[(0.0, 0.0), (100.0, 50.0)]);
    stroke.transform.rotation = 30.0;
    let before = geometry::local_matrix(&stroke.transform).apply(Vec2::default());
    let resized = geometry::resize_node(&stroke, 200.0, 50.0);
    let after = geometry::local_matrix(&resized.transform).apply(Vec2::default());
    assert!(close(before.x, after.x) && close(before.y, after.y));
    let Content::Ink { points, .. } = &resized.content else {
        unreachable!()
    };
    assert!(close(points[1].x, 200.0) && close(points[1].y, 25.0));
}

#[test]
fn selection_roots_skip_descendants_and_locked_ancestry() {
    let mut locked = Node::new("locked", Content::Group, Transform::default());
    locked.locked = true;
    let nodes = vec![
        Node::new("g", Content::Group, Transform::default()),
        rect("a", 0.0, 0.0, 1.0, 1.0).with_parent("g"),
        locked,
        rect("b", 0.0, 0.0, 1.0, 1.0).with_parent("locked"),
        rect("c", 0.0, 0.0, 1.0, 1.0),
    ];
    let index = SceneIndex::new(&nodes);
    let roots: Vec<_> = index
        .selection_roots(&ids(&["a", "g", "b", "c"]))
        .iter()
        .map(|n| n.id.as_str())
        .collect();
    assert_eq!(roots, ["g", "c"]);
    assert!(!index.editable(index.get("b").unwrap()));
}

#[test]
fn group_then_ungroup_keeps_world_geometry() {
    let mut turned = rect("b", 50.0, 20.0, 30.0, 10.0);
    turned.transform.rotation = 45.0;
    let start = doc(vec![
        rect("a", 0.0, 0.0, 10.0, 10.0),
        turned,
        rect("c", 200.0, 0.0, 5.0, 5.0),
    ]);
    let before = SceneIndex::new(&start.scene.nodes);
    let corners_a = world_corners(&before, "a");
    let corners_b = world_corners(&before, "b");

    let edit = before.group(&ids(&["a", "b"]), "grp").unwrap();
    assert_eq!(edit.selected, ["grp"]);
    let grouped = apply_operations(&start, &edit.operations).unwrap();
    let order: Vec<_> = grouped.scene.nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(order, ["a", "grp", "b", "c"]);
    let index = SceneIndex::new(&grouped.scene.nodes);
    assert_eq!(index.get("b").unwrap().parent_id.as_deref(), Some("grp"));
    for (p, q) in world_corners(&index, "b").iter().zip(&corners_b) {
        assert!(close(p.x, q.x) && close(p.y, q.y));
    }

    let edit = index.ungroup(&ids(&["grp"])).unwrap();
    let flat = apply_operations(&grouped, &edit.operations).unwrap();
    let index = SceneIndex::new(&flat.scene.nodes);
    assert!(index.get("grp").is_none());
    for (id, corners) in [("a", &corners_a), ("b", &corners_b)] {
        assert_eq!(index.get(id).unwrap().parent_id, None);
        for (p, q) in world_corners(&index, id).iter().zip(corners) {
            assert!(close(p.x, q.x) && close(p.y, q.y), "{id}");
        }
    }
    assert!(before.group(&ids(&["a"]), "x").is_err());
}

#[test]
fn scale_and_rotate_bake_into_nodes() {
    let nodes = vec![
        rect("a", 0.0, 0.0, 10.0, 10.0),
        rect("b", 20.0, 0.0, 10.0, 10.0),
    ];
    let index = SceneIndex::new(&nodes);
    let scaled = index.scale(&ids(&["a", "b"]), Vec2::new(0.0, 0.0), 2.0);
    let b = scaled.iter().find(|n| n.id == "b").unwrap();
    assert!(close_rect(
        Rect::new(
            b.transform.x,
            b.transform.y,
            b.transform.width,
            b.transform.height
        ),
        Rect::new(40.0, 0.0, 20.0, 20.0)
    ));
    let Content::Shape { stroke_width, .. } = b.content else {
        unreachable!()
    };
    assert!(close(stroke_width, 4.0));

    let rotated = index.rotate(&ids(&["a"]), Vec2::new(5.0, 5.0), 90.0);
    let a = &rotated[0];
    assert!(
        close(a.transform.rotation, 90.0) && close(a.transform.x, 0.0) && close(a.transform.y, 0.0)
    );
}

#[test]
fn align_distribute_and_arrange() {
    let nodes = vec![
        rect("a", 0.0, 0.0, 10.0, 10.0),
        rect("b", 15.0, 5.0, 10.0, 10.0),
        rect("c", 100.0, 20.0, 10.0, 10.0),
    ];
    let start = doc(nodes.clone());
    let index = SceneIndex::new(&nodes);
    let top =
        apply_operations(&start, &index.align(&ids(&["a", "b", "c"]), Alignment::Top)).unwrap();
    assert!(top.scene.nodes.iter().all(|n| close(n.transform.y, 0.0)));

    let spread =
        apply_operations(&start, &index.distribute(&ids(&["a", "b", "c"]), Axis::X)).unwrap();
    assert!(close(spread.scene.node("b").unwrap().transform.x, 50.0));

    let forward = index.arrange(&ids(&["a"]), true);
    let moved = apply_operations(&start, &forward).unwrap();
    let order: Vec<_> = moved.scene.nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(order, ["b", "a", "c"]);
    assert!(index.arrange(&ids(&["c"]), true).is_empty());
}

#[test]
fn layer_rows_list_front_first_with_depth() {
    let nodes = vec![
        Node::new("g", Content::Group, Transform::default()),
        rect("a", 0.0, 0.0, 1.0, 1.0).with_parent("g"),
        rect("b", 0.0, 0.0, 1.0, 1.0).with_parent("g"),
        rect("top", 0.0, 0.0, 1.0, 1.0),
    ];
    let index = SceneIndex::new(&nodes);
    let rows: Vec<_> = index
        .layer_rows(|_| false)
        .iter()
        .map(|r| (r.node.id.as_str(), r.depth))
        .collect();
    assert_eq!(rows, [("top", 0), ("g", 0), ("b", 1), ("a", 1)]);
    let collapsed = index.layer_rows(|n| n.id == "g");
    assert_eq!(collapsed.len(), 2);
    assert_eq!(collapsed[1].children, 2);
}

#[test]
fn smart_guides_beat_the_grid_and_scale_with_zoom() {
    let snapping = Snapping {
        snap_grid: true,
        ..Snapping::default()
    };
    let bounds = Rect::new(0.0, 0.0, 10.0, 10.0);
    let target = Rect::new(103.0, 50.0, 10.0, 10.0);
    let moved = snapping.snap_move(bounds, &[target], Vec2::new(100.0, 1.0), 1.0, false);
    assert!(
        close(moved.delta.x, 98.0),
        "right edge onto target center: {moved:?}"
    );
    assert!(close(moved.delta.y, 0.0), "grid snaps y");
    assert_eq!(moved.guides.len(), 1);
    assert_eq!(moved.guides[0].axis, Axis::X);
    let zoomed = snapping.snap_move(bounds, &[target], Vec2::new(100.0, 0.0), 4.0, false);
    assert!(
        close(zoomed.delta.x, 96.0),
        "beyond 6/4 tolerance, onto grid: {zoomed:?}"
    );
    let off = snapping.snap_move(bounds, &[target], Vec2::new(100.0, 1.0), 1.0, true);
    assert_eq!(off.delta, Vec2::new(100.0, 1.0));
}

#[test]
fn hits_respect_fill_lines_ink_and_rounded_frames() {
    let mut frame = Node::new(
        "f",
        Content::Frame {
            name: "F".into(),
            background: "#fff".into(),
        },
        tf(0.0, 0.0, 100.0, 100.0, 0.0),
    );
    frame.appearance = Some(Appearance {
        corner_radius: 40.0,
        shadow: None,
    });
    let line = Node::new(
        "line",
        Content::Shape {
            shape: ShapeKind::Line,
            fill: "transparent".into(),
            stroke: "#000".into(),
            stroke_width: 2.0,
            line: None,
        },
        tf(200.0, 0.0, 100.0, 100.0, 0.0),
    );
    let nodes = vec![
        frame,
        rect("corner", 0.0, 0.0, 20.0, 20.0).with_parent("f"),
        shape(
            "hollow",
            ShapeKind::Ellipse,
            "transparent",
            tf(400.0, 0.0, 100.0, 100.0, 0.0),
        ),
        line,
        ink("i", &[(600.0, 0.0), (700.0, 0.0)]),
    ];
    let index = SceneIndex::new(&nodes);
    let hit = |x, y| index.hit(Vec2::new(x, y), 1.0).map(|n| n.id.clone());
    assert_eq!(hit(2.0, 2.0), None, "clipped by the frame's rounded corner");
    assert_eq!(hit(18.0, 18.0).as_deref(), Some("corner"));
    assert_eq!(hit(50.0, 50.0), None, "frames are not picked");
    assert_eq!(hit(450.0, 50.0), None, "hollow ellipse center");
    assert_eq!(hit(500.0, 50.0).as_deref(), Some("hollow"));
    assert_eq!(hit(250.0, 50.0).as_deref(), Some("line"));
    assert_eq!(hit(250.0, 10.0), None);
    assert_eq!(hit(650.0, 1.5).as_deref(), Some("i"));
}

#[test]
fn marquee_lasso_and_erase() {
    let nodes = vec![
        rect("a", 0.0, 0.0, 10.0, 10.0),
        rect("b", 50.0, 0.0, 10.0, 10.0),
        ink(
            "i",
            &[(0.0, 50.0), (10.0, 50.0), (100.0, 50.0), (110.0, 50.0)],
        ),
    ];
    let index = SceneIndex::new(&nodes);
    assert_eq!(index.marquee(Rect::new(-1.0, -1.0, 20.0, 20.0), &[]), ["a"]);
    assert_eq!(
        index.marquee(Rect::new(-1.0, -1.0, 20.0, 20.0), &ids(&["b"])),
        ["a", "b"]
    );
    let lasso = [
        Vec2::new(-5.0, -5.0),
        Vec2::new(70.0, -5.0),
        Vec2::new(70.0, 60.0),
        Vec2::new(-5.0, 60.0),
    ];
    assert_eq!(index.lasso(&lasso), ["a", "b", "i"]);

    let mut next = 0;
    let ops = index.erase_ink(&[Vec2::new(55.0, 50.0)], 50.0, || {
        next += 1;
        format!("piece{next}")
    });
    let erased = apply_operations(&doc(nodes.clone()), &ops).unwrap();
    let pieces: Vec<_> = erased
        .scene
        .nodes
        .iter()
        .filter_map(|n| match &n.content {
            Content::Ink { points, .. } => Some((n.id.as_str(), points.len())),
            _ => None,
        })
        .collect();
    assert_eq!(pieces, [("i", 1), ("piece1", 1)]);
    let removed = index.erase_nodes(&[Vec2::new(5.0, 5.0)], 1.0);
    assert_eq!(removed, [Operation::remove("a")]);
}

#[test]
fn line_endpoints_and_handles() {
    let line = Node::new(
        "l",
        Content::Shape {
            shape: ShapeKind::Line,
            fill: "transparent".into(),
            stroke: "#123456".into(),
            stroke_width: 3.0,
            line: None,
        },
        tf(0.0, 0.0, 10.0, 10.0, 0.0),
    );
    let nodes = vec![line, rect("r", 0.0, 0.0, 10.0, 10.0)];
    let index = SceneIndex::new(&nodes);
    let moved = index.move_line_endpoint(
        &nodes[0],
        selection::Endpoint::End,
        Vec2::new(30.0, 2.0),
        true,
    );
    let geometry = LineGeometry::of(&moved).unwrap();
    let m = index.world_matrix(&moved);
    let end = m.apply(geometry.end);
    assert!(
        close(end.y, 0.0) && close(end.x, 30.066592756745816),
        "snapped to 0 degrees: {end:?}"
    );
    assert!(matches!(moved.content, Content::Shape { ref stroke, .. } if stroke == "#123456"));

    let frame = index.selection_frame(&ids(&["l"])).unwrap();
    assert!(close(frame.height, 0.0) && close(frame.angle, 45.0));

    let resized = index.resize_from_handle(&ids(&["r"]), Handle::Se, Vec2::new(20.0, 30.0), false);
    let r = &resized[1].transform;
    assert!(close(r.width, 20.0) && close(r.height, 30.0) && close(r.x, 0.0));
    let pinned = index.resize_from_handle(&ids(&["r"]), Handle::Nw, Vec2::new(-10.0, -10.0), false);
    let r = &pinned[1].transform;
    assert!(close(r.x, -10.0) && close(r.width, 20.0));
}

#[test]
fn text_wraps_and_measures() {
    let mut node = Node::new(
        "t",
        Content::Text {
            text: "hello wide world".into(),
            color: "#000".into(),
            font_family: "Inter".into(),
            font_size: 10.0,
            font_weight: None,
            font_style: None,
        },
        Transform::default(),
    );
    let single = text::layout_node(&node, &EstimateMeasure).unwrap();
    assert_eq!(single.lines, ["hello wide world"]);
    assert!(close(single.width, 16.0 * 6.0 + 1.2));
    node.text_layout = Some(TextLayout {
        width: Some(40.0),
        ..TextLayout::default()
    });
    let wrapped = text::layout_node(&node, &EstimateMeasure).unwrap();
    assert_eq!(wrapped.lines, ["hello ", "wide ", "world"]);
    assert_eq!(wrapped.starts, [0, 6, 11]);
    let index = SceneIndex::new(std::slice::from_ref(&node));
    assert!(close(index.dimensions(&node).0, 40.0));
}

#[test]
fn camera_fits_and_zooms_about_an_anchor() {
    let cam = Camera::fit(Some(Rect::new(0.0, 0.0, 100.0, 50.0)), 220.0, 220.0, 10.0);
    assert!(close(cam.zoom, 2.0));
    let center = cam.scene_to_view(Vec2::new(50.0, 25.0));
    assert!(close(center.x, 110.0) && close(center.y, 110.0));
    let anchor = Vec2::new(30.0, 40.0);
    let under = cam.view_to_scene(anchor);
    let zoomed = cam.zoom_at(anchor, 100.0);
    assert!(close(zoomed.zoom, Camera::MAX_ZOOM));
    let still = zoomed.view_to_scene(anchor);
    assert!(close(still.x, under.x) && close(still.y, under.y));
    assert_eq!(Camera::fit(None, 10.0, 10.0, 0.0), Camera::default());
}
