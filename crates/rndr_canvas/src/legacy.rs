//! Medousa Create documents from before typed lines stored an arrow as a
//! five-point mouse ink stroke: tail, tip, one barb, the tip again and the
//! other barb. [`upgrade_legacy_arrows`] turns exactly that pattern into a
//! line node with an end arrow, so painting, picking and editing only ever
//! see typed lines. Nothing here runs unless the application calls it.

use crate::schema::{
    BrushKind, Content, InkPoint, InputKind, Line, LinePoint, Node, Scene, ShapeKind,
};

/// The line a legacy arrow stroke draws, in the node's local coordinates,
/// with its stroke color, width and opacity; `None` for anything else.
pub fn legacy_arrow(node: &Node) -> Option<(Line, String, f64, f64)> {
    let Content::Ink {
        color,
        input,
        brush,
        points,
    } = &node.content
    else {
        return None;
    };
    if *input != InputKind::Mouse || brush.kind != BrushKind::Pen || points.len() != 5 {
        return None;
    }
    if points.iter().any(|p| {
        p.pressure != Some(0.5)
            || p.elapsed_ms.is_some()
            || p.tilt_x.is_some()
            || p.tilt_y.is_some()
    }) {
        return None;
    }
    let [a, b, left, repeat, right] = [&points[0], &points[1], &points[2], &points[3], &points[4]];
    if b.x != repeat.x || b.y != repeat.y {
        return None;
    }
    let length = |p: &InkPoint| (p.x - b.x).hypot(p.y - b.y);
    let (distance, l, r) = (length(a), length(left), length(right));
    if distance < 1.0 || l < 1.0 || (l - r).abs() > 0.01 || l > distance * 0.36 {
        return None;
    }
    let direction = (b.y - a.y).atan2(b.x - a.x);
    let off = |p: &InkPoint, len: f64, angle: f64| {
        ((b.x - p.x) / len - angle.cos()).abs() > 0.001
            || ((b.y - p.y) / len - angle.sin()).abs() > 0.001
    };
    if off(left, l, direction - 0.5) || off(right, r, direction + 0.5) {
        return None;
    }
    let line = Line {
        start: LinePoint { x: a.x, y: a.y },
        end: LinePoint { x: b.x, y: b.y },
        start_arrow: false,
        end_arrow: true,
        dashed: false,
    };
    Some((line, color.clone(), brush.size, brush.opacity))
}

/// Rewrites every legacy arrow stroke in `scene` as a line node with the
/// same id, transform and look. Returns how many nodes changed.
pub fn upgrade_legacy_arrows(scene: &mut Scene) -> usize {
    let mut changed = 0;
    for node in &mut scene.nodes {
        let Some((line, stroke, stroke_width, opacity)) = legacy_arrow(node) else {
            continue;
        };
        node.content = Content::Shape {
            shape: ShapeKind::Line,
            fill: "transparent".into(),
            stroke,
            stroke_width,
            line: Some(line),
        };
        node.opacity *= opacity;
        changed += 1;
    }
    changed
}
