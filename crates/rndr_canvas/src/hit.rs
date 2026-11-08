//! Picking: point hits, marquee and lasso selection, and ink erasing.

use crate::geometry::{LineGeometry, Rect, SceneIndex, Vec2};
use crate::ops::Operation;
use crate::schema::{Content, InkPoint, Node, ShapeKind};

/// Signed distance from a local point to the node's (rounded) box; negative
/// inside. Corner radii apply to rectangles, frames and images.
pub fn rounded_distance(node: &Node, p: Vec2) -> f64 {
    let (w, h) = (node.transform.width, node.transform.height);
    let cornered = matches!(
        node.content,
        Content::Shape {
            shape: ShapeKind::Rectangle,
            ..
        } | Content::Frame { .. }
            | Content::Image { .. }
    );
    let r = if cornered {
        node.appearance
            .as_ref()
            .map_or(0.0, |a| a.corner_radius)
            .min(w / 2.0)
            .min(h / 2.0)
            .max(0.0)
    } else {
        0.0
    };
    let dx = (p.x - w / 2.0).abs() - w / 2.0 + r;
    let dy = (p.y - h / 2.0).abs() - h / 2.0 + r;
    dx.max(0.0).hypot(dy.max(0.0)) + dx.max(dy).min(0.0) - r
}

fn segment_distance(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let length = dx * dx + dy * dy;
    let t = if length > 0.0 {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / length).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p.x - a.x - dx * t).hypot(p.y - a.y - dy * t)
}

/// Even-odd point-in-polygon test.
pub fn point_in_polygon(p: Vec2, polygon: &[Vec2]) -> bool {
    let mut inside = false;
    let mut j = polygon.len().wrapping_sub(1);
    for i in 0..polygon.len() {
        let (a, b) = (polygon[i], polygon[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn pickable(index: &SceneIndex<'_>, node: &Node) -> bool {
    index.editable(node) && !node.content.is_container()
}

impl<'a> SceneIndex<'a> {
    /// Whether a scene point touches the node within `tolerance`. Groups,
    /// frames and nodes that are hidden, locked or clipped away by a rounded
    /// frame never hit. Unfilled shapes hit only near their outline.
    pub fn hit_node(&self, node: &Node, point: Vec2, tolerance: f64) -> bool {
        if !pickable(self, node) {
            return false;
        }
        for ancestor in self.ancestors(node) {
            if matches!(ancestor.content, Content::Frame { .. })
                && rounded_distance(ancestor, self.world_matrix(ancestor).inverse().apply(point))
                    > 0.0
            {
                return false;
            }
        }
        let p = self.world_matrix(node).inverse().apply(point);
        if let Some(line) = LineGeometry::of(node) {
            return line.hit(p, tolerance);
        }
        let (w, h) = self.dimensions(node);
        match &node.content {
            Content::Image { .. } => rounded_distance(node, p) <= tolerance,
            Content::Ink { points, brush, .. } => {
                let reach = tolerance + brush.size / 2.0;
                points.iter().enumerate().any(|(i, a)| {
                    let b = &points[(i + 1).min(points.len() - 1)];
                    segment_distance(p, Vec2::new(a.x, a.y), Vec2::new(b.x, b.y)) <= reach
                })
            }
            Content::Shape {
                shape,
                fill,
                stroke_width,
                ..
            } => {
                let pad = tolerance + stroke_width / 2.0;
                let filled = fill != "transparent";
                if *shape == ShapeKind::Ellipse {
                    let (rx, ry) = (w / 2.0, h / 2.0);
                    let radius = ((p.x - rx) / rx).hypot((p.y - ry) / ry);
                    let slack = pad / rx.min(ry).max(f64::EPSILON);
                    radius <= 1.0 + slack && (filled || radius >= 1.0 - slack)
                } else {
                    let distance = rounded_distance(node, p);
                    distance <= pad && (filled || distance >= -pad)
                }
            }
            _ => {
                p.x >= -tolerance
                    && p.y >= -tolerance
                    && p.x <= w + tolerance
                    && p.y <= h + tolerance
            }
        }
    }

    /// The front-most node under a scene point, in layer-panel order.
    pub fn hit(&self, point: Vec2, tolerance: f64) -> Option<&'a Node> {
        self.layer_rows(|_| false)
            .into_iter()
            .map(|row| row.node)
            .find(|n| self.hit_node(n, point, tolerance))
    }

    /// Selection roots of editable nodes fully inside `rect`, added to
    /// `initial`. Frames are never marquee-selected.
    pub fn marquee(&self, rect: Rect, initial: &[String]) -> Vec<String> {
        let mut ids = initial.to_vec();
        for n in self.nodes() {
            if matches!(n.content, Content::Frame { .. }) || !self.editable(n) {
                continue;
            }
            let b = self.bounds(n);
            if b.x >= rect.x
                && b.y >= rect.y
                && b.right() <= rect.right()
                && b.bottom() <= rect.bottom()
            {
                ids.push(n.id.clone());
            }
        }
        ids.dedup();
        self.selection_roots(&ids)
            .into_iter()
            .map(|n| n.id.clone())
            .collect()
    }

    /// Selection roots enclosed by a freehand loop (scene points). Ink is
    /// enclosed when at least half its points are inside; other nodes when
    /// all corners of their bounds are.
    pub fn lasso(&self, path: &[Vec2]) -> Vec<String> {
        if path.len() < 3 {
            return Vec::new();
        }
        let ids: Vec<String> = self
            .nodes()
            .iter()
            .filter(|n| pickable(self, n))
            .filter(|n| match &n.content {
                Content::Ink { points, .. } if LineGeometry::of(n).is_none() => {
                    let m = self.world_matrix(n);
                    let inside = points
                        .iter()
                        .filter(|p| point_in_polygon(m.apply(Vec2::new(p.x, p.y)), path))
                        .count();
                    inside * 2 >= points.len()
                }
                _ => self
                    .bounds(n)
                    .corners()
                    .iter()
                    .all(|c| point_in_polygon(*c, path)),
            })
            .map(|n| n.id.clone())
            .collect();
        self.selection_roots(&ids)
            .into_iter()
            .map(|n| n.id.clone())
            .collect()
    }

    /// Removes every editable non-container node an eraser path touches.
    pub fn erase_nodes(&self, path: &[Vec2], radius: f64) -> Vec<Operation> {
        self.nodes()
            .iter()
            .filter(|n| path.iter().any(|p| self.hit_node(n, *p, radius)))
            .map(|n| Operation::remove(n.id.clone()))
            .collect()
    }

    /// Cuts ink where an eraser path passes, splitting strokes into pieces.
    /// The first piece keeps the node's id; `new_id` names the rest. Strokes
    /// erased entirely are removed. Other nodes are untouched.
    pub fn erase_ink(
        &self,
        path: &[Vec2],
        radius: f64,
        mut new_id: impl FnMut() -> String,
    ) -> Vec<Operation> {
        let mut operations = Vec::new();
        for node in self.nodes() {
            let Content::Ink { points, brush, .. } = &node.content else {
                continue;
            };
            if !self.editable(node) || LineGeometry::of(node).is_some() {
                continue;
            }
            let m = self.world_matrix(node);
            let reach = radius + brush.size / 2.0;
            let erased = |p: &InkPoint| {
                let world = m.apply(Vec2::new(p.x, p.y));
                path.iter().any(|e| e.distance(world) <= reach)
            };
            if !points.iter().any(erased) {
                continue;
            }
            let pieces: Vec<Vec<InkPoint>> = points
                .split(erased)
                .filter(|piece| !piece.is_empty())
                .map(<[InkPoint]>::to_vec)
                .collect();
            if pieces.is_empty() {
                operations.push(Operation::remove(node.id.clone()));
                continue;
            }
            for (i, piece) in pieces.into_iter().enumerate() {
                let mut next = node.clone();
                if i > 0 {
                    next.id = new_id();
                }
                if let Content::Ink { points, .. } = &mut next.content {
                    *points = piece;
                }
                operations.push(Operation::put(next));
            }
        }
        operations
    }
}
