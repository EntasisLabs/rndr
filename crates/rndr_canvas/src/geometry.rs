//! Scene-space geometry: transforms, bounds, hierarchy and line endpoints.

use crate::schema::{Content, Line, LinePoint, Node, ShapeKind, Transform};
use crate::text::{layout_node, EstimateMeasure, TextMeasure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl Vec2 {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn distance(self, other: Vec2) -> f64 {
        (self.x - other.x).hypot(self.y - other.y)
    }
}

impl From<LinePoint> for Vec2 {
    fn from(p: LinePoint) -> Self {
        Self::new(p.x, p.y)
    }
}

impl From<Vec2> for LinePoint {
    fn from(p: Vec2) -> Self {
        LinePoint { x: p.x, y: p.y }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// The smallest rectangle containing `points`.
    pub fn around(points: impl IntoIterator<Item = Vec2>) -> Option<Rect> {
        let mut iter = points.into_iter();
        let first = iter.next()?;
        let (mut x0, mut y0, mut x1, mut y1) = (first.x, first.y, first.x, first.y);
        for p in iter {
            x0 = x0.min(p.x);
            y0 = y0.min(p.y);
            x1 = x1.max(p.x);
            y1 = y1.max(p.y);
        }
        Some(Rect::new(x0, y0, x1 - x0, y1 - y0))
    }

    pub fn union(self, other: Rect) -> Rect {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Rect::new(
            x,
            y,
            self.right().max(other.right()) - x,
            self.bottom().max(other.bottom()) - y,
        )
    }

    /// The union of all `rects`, if any.
    pub fn union_all(rects: impl IntoIterator<Item = Rect>) -> Option<Rect> {
        rects.into_iter().reduce(Rect::union)
    }

    pub fn right(self) -> f64 {
        self.x + self.width
    }

    pub fn bottom(self) -> f64 {
        self.y + self.height
    }

    pub fn center(self) -> Vec2 {
        Vec2::new(self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    pub fn contains(self, p: Vec2) -> bool {
        p.x >= self.x && p.x <= self.right() && p.y >= self.y && p.y <= self.bottom()
    }

    pub fn intersects(self, other: Rect) -> bool {
        self.x <= other.right()
            && other.x <= self.right()
            && self.y <= other.bottom()
            && other.y <= self.bottom()
    }

    pub fn corners(self) -> [Vec2; 4] {
        [
            Vec2::new(self.x, self.y),
            Vec2::new(self.right(), self.y),
            Vec2::new(self.right(), self.bottom()),
            Vec2::new(self.x, self.bottom()),
        ]
    }
}

/// A 2D affine transform `[a, b, c, d, e, f]` mapping `(x, y)` to
/// `(a x + c y + e, b x + d y + f)`, the same layout as SVG and canvas.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Matrix(pub [f64; 6]);

impl Default for Matrix {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Matrix {
    pub const IDENTITY: Matrix = Matrix([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    pub fn translate(x: f64, y: f64) -> Self {
        Matrix([1.0, 0.0, 0.0, 1.0, x, y])
    }

    /// `self` after `other`: maps through `other` first.
    pub fn then(self, other: Matrix) -> Matrix {
        let [a0, a1, a2, a3, a4, a5] = self.0;
        let [b0, b1, b2, b3, b4, b5] = other.0;
        Matrix([
            a0 * b0 + a2 * b1,
            a1 * b0 + a3 * b1,
            a0 * b2 + a2 * b3,
            a1 * b2 + a3 * b3,
            a0 * b4 + a2 * b5 + a4,
            a1 * b4 + a3 * b5 + a5,
        ])
    }

    pub fn apply(self, p: Vec2) -> Vec2 {
        let [a, b, c, d, e, f] = self.0;
        Vec2::new(a * p.x + c * p.y + e, b * p.x + d * p.y + f)
    }

    /// The inverse, or identity for a degenerate matrix.
    pub fn inverse(self) -> Matrix {
        let [a, b, c, d, e, f] = self.0;
        let det = a * d - b * c;
        if det == 0.0 || !det.is_finite() {
            return Matrix::IDENTITY;
        }
        Matrix([
            d / det,
            -b / det,
            -c / det,
            a / det,
            (c * f - d * e) / det,
            (b * e - a * f) / det,
        ])
    }

    /// The rotation in degrees.
    pub fn rotation(self) -> f64 {
        self.0[1].atan2(self.0[0]).to_degrees()
    }
}

/// Maps a node's local box into its parent: rotation turns the box around
/// its center, then it is placed at `x, y`.
pub fn local_matrix(t: &Transform) -> Matrix {
    let (s, c) = t.rotation.to_radians().sin_cos();
    let (w, h) = (t.width, t.height);
    Matrix([
        c,
        s,
        -s,
        c,
        t.x + w / 2.0 - c * w / 2.0 + s * h / 2.0,
        t.y + h / 2.0 - s * w / 2.0 - c * h / 2.0,
    ])
}

/// Where a line draws, in the node's local coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineGeometry {
    pub start: Vec2,
    pub end: Vec2,
    pub stroke_width: f64,
    pub start_arrow: bool,
    pub end_arrow: bool,
    pub dashed: bool,
}

/// A visible arrowhead as a triangle, tip first.
pub type Arrowhead = [Vec2; 3];

impl LineGeometry {
    /// The line of a [`ShapeKind::Line`] node. Without explicit endpoints a
    /// line runs corner to corner across its box.
    pub fn of(node: &Node) -> Option<LineGeometry> {
        let Content::Shape {
            shape: ShapeKind::Line,
            stroke_width,
            line,
            ..
        } = &node.content
        else {
            return None;
        };
        let t = &node.transform;
        Some(LineGeometry {
            start: line.map_or(Vec2::new(0.0, 0.0), |l| l.start.into()),
            end: line.map_or(Vec2::new(t.width, t.height), |l| l.end.into()),
            stroke_width: *stroke_width,
            start_arrow: line.is_some_and(|l| l.start_arrow),
            end_arrow: line.is_some_and(|l| l.end_arrow),
            dashed: line.is_some_and(|l| l.dashed),
        })
    }

    fn direction(&self) -> (f64, f64, f64) {
        let length = self.start.distance(self.end);
        if length == 0.0 {
            (1.0, 0.0, 0.0)
        } else {
            (
                (self.end.x - self.start.x) / length,
                (self.end.y - self.start.y) / length,
                length,
            )
        }
    }

    fn head(&self) -> f64 {
        let (_, _, length) = self.direction();
        let share = if self.start_arrow && self.end_arrow {
            0.4
        } else {
            0.6
        };
        (length * share).min(self.stroke_width * 4.0)
    }

    /// The stroked segment, shortened so it ends inside any arrowheads.
    pub fn shaft(&self) -> (Vec2, Vec2) {
        let (ux, uy, _) = self.direction();
        let head = self.head() * 0.8;
        let inset = |arrow: bool| if arrow { head } else { 0.0 };
        (
            Vec2::new(
                self.start.x + ux * inset(self.start_arrow),
                self.start.y + uy * inset(self.start_arrow),
            ),
            Vec2::new(
                self.end.x - ux * inset(self.end_arrow),
                self.end.y - uy * inset(self.end_arrow),
            ),
        )
    }

    pub fn arrowheads(&self) -> Vec<Arrowhead> {
        let (ux, uy, _) = self.direction();
        let head = self.head();
        let triangle = |tip: Vec2, dir: f64| {
            [
                tip,
                Vec2::new(
                    tip.x + dir * ux * head - uy * head * 0.45,
                    tip.y + dir * uy * head + ux * head * 0.45,
                ),
                Vec2::new(
                    tip.x + dir * ux * head + uy * head * 0.45,
                    tip.y + dir * uy * head - ux * head * 0.45,
                ),
            ]
        };
        let mut heads = Vec::new();
        if head > 0.001 {
            if self.start_arrow {
                heads.push(triangle(self.start, 1.0));
            }
            if self.end_arrow {
                heads.push(triangle(self.end, -1.0));
            }
        }
        heads
    }

    /// Points whose hull covers the stroke and arrowheads.
    pub fn outline(&self) -> Vec<Vec2> {
        let pad = self.stroke_width / 2.0;
        [self.start, self.end]
            .into_iter()
            .chain(self.arrowheads().into_iter().flatten())
            .flat_map(|p| {
                [
                    Vec2::new(p.x - pad, p.y - pad),
                    Vec2::new(p.x + pad, p.y + pad),
                ]
            })
            .collect()
    }

    /// Whether `point` (local) is within `tolerance` of the stroke or inside
    /// an arrowhead.
    pub fn hit(&self, point: Vec2, tolerance: f64) -> bool {
        let (a, b) = (self.start, self.end);
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let length = dx * dx + dy * dy;
        let t = if length > 0.0 {
            (((point.x - a.x) * dx + (point.y - a.y) * dy) / length).clamp(0.0, 1.0)
        } else {
            0.0
        };
        if (point.x - a.x - dx * t).hypot(point.y - a.y - dy * t)
            <= tolerance + self.stroke_width / 2.0
        {
            return true;
        }
        self.arrowheads()
            .iter()
            .any(|&[a, b, c]| point_in_triangle(point, a, b, c))
    }
}

pub(crate) fn point_in_triangle(p: Vec2, a: Vec2, b: Vec2, c: Vec2) -> bool {
    let cross = |u: Vec2, v: Vec2| (p.x - v.x) * (u.y - v.y) - (u.x - v.x) * (p.y - v.y);
    let signs = [cross(a, b), cross(b, c), cross(c, a)];
    signs.iter().all(|n| *n >= 0.0) || signs.iter().all(|n| *n <= 0.0)
}

/// With `snap`, rounds the direction from `anchor` to `point` to a multiple
/// of 45 degrees, keeping its length.
pub fn snap_line_point(anchor: Vec2, point: Vec2, snap: bool) -> Vec2 {
    if !snap {
        return point;
    }
    let (dx, dy) = (point.x - anchor.x, point.y - anchor.y);
    let step = std::f64::consts::FRAC_PI_4;
    let angle = (dy.atan2(dx) / step).round() * step;
    let length = dx.hypot(dy);
    Vec2::new(
        anchor.x + angle.cos() * length,
        anchor.y + angle.sin() * length,
    )
}

/// Rebuilds a line node between two points in its parent's coordinates. The
/// box becomes the unrotated bounds of the endpoints; style is kept.
pub fn set_line_endpoints(node: &Node, start: Vec2, end: Vec2) -> Node {
    let previous = LineGeometry::of(node);
    let (stroke, stroke_width) = match &node.content {
        Content::Shape {
            stroke,
            stroke_width,
            ..
        } => (stroke.clone(), *stroke_width),
        Content::Ink { color, brush, .. } => (color.clone(), brush.size),
        _ => ("#393e33".to_string(), 4.0),
    };
    let x = start.x.min(end.x);
    let y = start.y.min(end.y);
    let mut next = node.clone();
    next.transform = Transform {
        x,
        y,
        width: (end.x - start.x).abs().max(1.0),
        height: (end.y - start.y).abs().max(1.0),
        rotation: 0.0,
    };
    next.content = Content::Shape {
        shape: ShapeKind::Line,
        fill: "transparent".into(),
        stroke,
        stroke_width,
        line: Some(Line {
            start: LinePoint {
                x: start.x - x,
                y: start.y - y,
            },
            end: LinePoint {
                x: end.x - x,
                y: end.y - y,
            },
            start_arrow: previous.is_some_and(|l| l.start_arrow),
            end_arrow: previous.is_some_and(|l| l.end_arrow),
            dashed: previous.is_some_and(|l| l.dashed),
        }),
    };
    next
}

/// Resizes a node's box, scaling ink points and line endpoints with it and
/// keeping the box's top-left corner where it appears after rotation.
pub fn resize_node(node: &Node, width: f64, height: f64) -> Node {
    let width = width.max(1.0);
    let height = height.max(1.0);
    let t = node.transform;
    let sx = if t.width > 0.0 { width / t.width } else { 1.0 };
    let sy = if t.height > 0.0 {
        height / t.height
    } else {
        1.0
    };
    let mut next = node.clone();
    match &mut next.content {
        Content::Ink { points, .. } => {
            for p in points {
                p.x *= sx;
                p.y *= sy;
            }
        }
        Content::Shape {
            line: Some(line), ..
        } => {
            for p in [&mut line.start, &mut line.end] {
                p.x *= sx;
                p.y *= sy;
            }
        }
        _ => {}
    }
    next.transform.width = width;
    next.transform.height = height;
    let before = local_matrix(&t).0;
    let after = local_matrix(&next.transform).0;
    next.transform.x += before[4] - after[4];
    next.transform.y += before[5] - after[5];
    next
}

/// All of `roots` and everything nested under them.
pub fn descendants<'a>(
    nodes: &[Node],
    roots: impl IntoIterator<Item = &'a str>,
) -> BTreeSet<String> {
    let mut children: HashMap<&str, Vec<&str>> = HashMap::new();
    for node in nodes {
        if let Some(parent) = node.parent_id.as_deref() {
            children.entry(parent).or_default().push(&node.id);
        }
    }
    let mut found: BTreeSet<String> = BTreeSet::new();
    let mut queue: Vec<&str> = Vec::new();
    for root in roots {
        if found.insert(root.to_string()) {
            queue.push(root);
        }
    }
    while let Some(parent) = queue.pop() {
        for child in children.get(parent).into_iter().flatten() {
            if found.insert(child.to_string()) {
                queue.push(child);
            }
        }
    }
    found
}

/// A read-only view of a node list with id lookup, for geometry queries.
/// Text is measured with the given [`TextMeasure`].
pub struct SceneIndex<'a> {
    nodes: &'a [Node],
    by_id: HashMap<&'a str, usize>,
    measure: &'a dyn TextMeasure,
}

impl<'a> SceneIndex<'a> {
    /// Indexes `nodes`, measuring text with [`EstimateMeasure`].
    pub fn new(nodes: &'a [Node]) -> Self {
        Self::with_measure(nodes, &EstimateMeasure)
    }

    pub fn with_measure(nodes: &'a [Node], measure: &'a dyn TextMeasure) -> Self {
        Self {
            nodes,
            by_id: nodes
                .iter()
                .enumerate()
                .map(|(i, n)| (n.id.as_str(), i))
                .collect(),
            measure,
        }
    }

    pub fn nodes(&self) -> &'a [Node] {
        self.nodes
    }

    pub fn measure(&self) -> &'a dyn TextMeasure {
        self.measure
    }

    pub fn get(&self, id: &str) -> Option<&'a Node> {
        self.by_id.get(id).map(|&i| &self.nodes[i])
    }

    /// Paint-order position.
    pub fn position(&self, id: &str) -> Option<usize> {
        self.by_id.get(id).copied()
    }

    pub fn parent(&self, node: &Node) -> Option<&'a Node> {
        node.parent_id.as_deref().and_then(|id| self.get(id))
    }

    /// Parents from nearest to the root. Stops on a cycle.
    pub fn ancestors(&self, node: &Node) -> Vec<&'a Node> {
        let mut out: Vec<&'a Node> = Vec::new();
        let mut cursor = self.parent(node);
        while let Some(parent) = cursor {
            if parent.id == node.id || out.iter().any(|n| n.id == parent.id) {
                break;
            }
            out.push(parent);
            cursor = self.parent(parent);
        }
        out
    }

    /// Direct children in paint order.
    pub fn children(&self, parent: Option<&str>) -> Vec<&'a Node> {
        self.nodes
            .iter()
            .filter(|n| n.parent_id.as_deref() == parent)
            .collect()
    }

    /// Maps the node's local coordinates to the scene.
    pub fn world_matrix(&self, node: &Node) -> Matrix {
        self.ancestors(node)
            .iter()
            .rev()
            .fold(Matrix::IDENTITY, |m, a| m.then(local_matrix(&a.transform)))
            .then(local_matrix(&node.transform))
    }

    /// Maps the parent's coordinates to the scene; identity at the root.
    pub fn parent_matrix(&self, node: &Node) -> Matrix {
        self.parent(node)
            .map_or(Matrix::IDENTITY, |p| self.world_matrix(p))
    }

    /// A scene point in the coordinates of the node's parent, where its
    /// transform lives.
    pub fn to_parent(&self, node: &Node, point: Vec2) -> Vec2 {
        self.parent_matrix(node).inverse().apply(point)
    }

    /// The node's local box size. Text uses its laid-out size, which can
    /// differ from the saved transform.
    pub fn dimensions(&self, node: &Node) -> (f64, f64) {
        match layout_node(node, self.measure) {
            Some(text) => (text.width, text.height),
            None => (node.transform.width, node.transform.height),
        }
    }

    /// Scene-space axis-aligned bounds of the node itself, ignoring children.
    pub fn bounds(&self, node: &Node) -> Rect {
        let m = self.world_matrix(node);
        let points = match LineGeometry::of(node) {
            Some(line) => line.outline(),
            None => {
                let (w, h) = self.dimensions(node);
                Rect::new(0.0, 0.0, w, h).corners().to_vec()
            }
        };
        Rect::around(points.into_iter().map(|p| m.apply(p))).unwrap_or_default()
    }

    /// Bounds of every visible node.
    pub fn scene_bounds(&self) -> Option<Rect> {
        Rect::union_all(
            self.nodes
                .iter()
                .filter(|n| n.visible)
                .map(|n| self.bounds(n)),
        )
    }

    /// Visible and unlocked, as are all of its ancestors.
    pub fn editable(&self, node: &Node) -> bool {
        node.visible && !node.locked && self.ancestors(node).iter().all(|a| a.visible && !a.locked)
    }

    /// Editable selected nodes that have no selected ancestor, in paint
    /// order. Moving these moves the whole selection once.
    pub fn selection_roots(&self, ids: &[String]) -> Vec<&'a Node> {
        let selected: BTreeSet<&str> = ids.iter().map(String::as_str).collect();
        self.nodes
            .iter()
            .filter(|n| {
                selected.contains(n.id.as_str())
                    && self.editable(n)
                    && !self
                        .ancestors(n)
                        .iter()
                        .any(|a| selected.contains(a.id.as_str()))
            })
            .collect()
    }

    /// Bounds of the selection roots.
    pub fn selection_bounds(&self, ids: &[String]) -> Option<Rect> {
        Rect::union_all(self.selection_roots(ids).iter().map(|n| self.bounds(n)))
    }
}

/// Where the scene appears in a view: view = scene * zoom + pan.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Camera {
    pub pan_x: f64,
    pub pan_y: f64,
    pub zoom: f64,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            pan_x: 0.0,
            pan_y: 0.0,
            zoom: 1.0,
        }
    }
}

impl Camera {
    pub const MIN_ZOOM: f64 = 0.02;
    pub const MAX_ZOOM: f64 = 8.0;

    pub fn view_to_scene(&self, p: Vec2) -> Vec2 {
        Vec2::new(
            (p.x - self.pan_x) / self.zoom,
            (p.y - self.pan_y) / self.zoom,
        )
    }

    pub fn scene_to_view(&self, p: Vec2) -> Vec2 {
        Vec2::new(p.x * self.zoom + self.pan_x, p.y * self.zoom + self.pan_y)
    }

    pub fn matrix(&self) -> Matrix {
        Matrix([self.zoom, 0.0, 0.0, self.zoom, self.pan_x, self.pan_y])
    }

    /// Zooms to `zoom`, clamped, keeping the scene point under `anchor`
    /// (a view point) in place.
    pub fn zoom_at(&self, anchor: Vec2, zoom: f64) -> Camera {
        let zoom = zoom.clamp(Self::MIN_ZOOM, Self::MAX_ZOOM);
        let scene = self.view_to_scene(anchor);
        Camera {
            zoom,
            pan_x: anchor.x - scene.x * zoom,
            pan_y: anchor.y - scene.y * zoom,
        }
    }

    /// Centers `bounds` in a `width` x `height` view with `padding` on each
    /// side. Empty bounds give the default camera.
    pub fn fit(bounds: Option<Rect>, width: f64, height: f64, padding: f64) -> Camera {
        let Some(b) = bounds.filter(|b| b.width > 0.0 || b.height > 0.0) else {
            return Camera::default();
        };
        let zoom = ((width - padding * 2.0).max(1.0) / b.width.max(1.0))
            .min((height - padding * 2.0).max(1.0) / b.height.max(1.0))
            .clamp(Self::MIN_ZOOM, Self::MAX_ZOOM);
        let c = b.center();
        Camera {
            zoom,
            pan_x: width / 2.0 - c.x * zoom,
            pan_y: height / 2.0 - c.y * zoom,
        }
    }
}
