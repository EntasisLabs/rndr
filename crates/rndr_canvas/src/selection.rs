//! Edits over a selection. Most return operations to submit in a
//! [`Transaction`](crate::ops::Transaction) rather than applying them.

use crate::error::{invalid, Result};
use crate::geometry::{descendants, local_matrix, resize_node, Matrix, Rect, SceneIndex, Vec2};
use crate::ops::{Operation, MAX_OPERATIONS};
use crate::schema::{Content, Node, Transform};
use crate::validate::MAX_NODES;
use std::collections::{BTreeSet, HashMap};

/// Operations to submit, and the ids to select once they are applied.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SelectionEdit {
    pub operations: Vec<Operation>,
    pub selected: Vec<String>,
}

fn union(rects: impl IntoIterator<Item = Rect>) -> Option<Rect> {
    Rect::union_all(rects).map(|r| Rect::new(r.x, r.y, r.width.max(1.0), r.height.max(1.0)))
}

/// Places `node` under `parent` so its local box maps through `matrix`
/// (rotation and translation only).
fn at_matrix(node: &Node, matrix: Matrix, parent: Option<&str>) -> Node {
    let mut next = node.clone();
    next.parent_id = parent.map(str::to_string);
    next.transform.x = 0.0;
    next.transform.y = 0.0;
    next.transform.rotation = matrix.rotation();
    let origin = local_matrix(&next.transform).0;
    next.transform.x = matrix.0[4] - origin[4];
    next.transform.y = matrix.0[5] - origin[5];
    next
}

fn check_size(operations: &[Operation]) -> Result<()> {
    if operations.len() > MAX_OPERATIONS {
        return Err(invalid(
            "select fewer objects for this edit (up to 1,000 operations)",
        ));
    }
    Ok(())
}

impl<'a> SceneIndex<'a> {
    /// Like [`SceneIndex::bounds`], but a group's bounds are those of its
    /// visible contents.
    pub fn object_bounds(&self, node: &Node) -> Rect {
        if !matches!(node.content, Content::Group) {
            return self.bounds(node);
        }
        union(
            self.children(Some(&node.id))
                .into_iter()
                .filter(|n| n.visible)
                .map(|n| self.object_bounds(n)),
        )
        .unwrap_or_else(|| self.bounds(node))
    }

    /// Object bounds of the selection roots, at least 1 x 1.
    pub fn selection_object_bounds(&self, ids: &[String]) -> Option<Rect> {
        union(
            self.selection_roots(ids)
                .into_iter()
                .map(|n| self.object_bounds(n)),
        )
    }

    /// Two or more selection roots that share a parent.
    pub fn can_group(&self, ids: &[String]) -> bool {
        let roots = self.selection_roots(ids);
        roots.len() >= 2
            && roots.len() <= MAX_OPERATIONS - 2
            && self.nodes().len() < MAX_NODES
            && roots.iter().all(|n| n.parent_id == roots[0].parent_id)
    }

    /// Wraps the selection roots in a new group `group_id`, placed where the
    /// topmost of them was in paint order.
    pub fn group(&self, ids: &[String], group_id: &str) -> Result<SelectionEdit> {
        if !self.can_group(ids) {
            return Err(invalid(
                "select at least two objects in the same frame or group to group them",
            ));
        }
        let roots = self.selection_roots(ids);
        let parent = roots[0].parent_id.clone();
        let rects = roots.iter().map(|n| {
            let m = local_matrix(&n.transform);
            let (w, h) = self.dimensions(n);
            Rect::around(Rect::new(0.0, 0.0, w, h).corners().map(|p| m.apply(p)))
                .unwrap_or_default()
        });
        let rect = union(rects).unwrap_or_default();
        let groups = self
            .nodes()
            .iter()
            .filter(|n| matches!(n.content, Content::Group))
            .count();
        let mut group = Node::new(
            group_id,
            Content::Group,
            Transform {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height,
                rotation: 0.0,
            },
        );
        group.parent_id = parent;
        group.name = Some(format!("Group {}", groups + 1));
        let inverse = local_matrix(&group.transform).inverse();
        let last = roots
            .last()
            .and_then(|n| self.position(&n.id))
            .unwrap_or_default();
        let mut order = Vec::with_capacity(self.nodes().len() + 1);
        for (i, n) in self.nodes().iter().enumerate() {
            if i == last {
                order.push(group.id.clone());
            }
            order.push(n.id.clone());
        }
        let mut operations = vec![Operation::put(group.clone())];
        operations.extend(roots.iter().map(|n| {
            Operation::put(at_matrix(
                n,
                inverse.then(local_matrix(&n.transform)),
                Some(&group.id),
            ))
        }));
        operations.push(Operation::Reorder { ids: order });
        check_size(&operations)?;
        Ok(SelectionEdit {
            operations,
            selected: vec![group.id],
        })
    }

    /// Lifts the children of selected groups into the groups' parents and
    /// removes the groups. Refuses translucent groups, whose opacity could
    /// not be kept.
    pub fn ungroup(&self, ids: &[String]) -> Result<SelectionEdit> {
        let roots = self.selection_roots(ids);
        let groups: Vec<_> = roots
            .iter()
            .filter(|n| matches!(n.content, Content::Group))
            .collect();
        if groups.is_empty() {
            return Ok(SelectionEdit {
                operations: Vec::new(),
                selected: ids.to_vec(),
            });
        }
        if groups.iter().any(|n| n.opacity != 1.0) {
            return Err(invalid(
                "set group opacity to 100% before ungrouping to preserve overlapping artwork",
            ));
        }
        let mut operations = Vec::new();
        let mut selected: Vec<String> = roots
            .iter()
            .filter(|n| !matches!(n.content, Content::Group))
            .map(|n| n.id.clone())
            .collect();
        let mut replacements: HashMap<&str, Vec<String>> = HashMap::new();
        let mut lifted = BTreeSet::new();
        for group in groups {
            let children = self.children(Some(&group.id));
            replacements.insert(&group.id, children.iter().map(|n| n.id.clone()).collect());
            for child in children {
                lifted.insert(child.id.as_str());
                selected.push(child.id.clone());
                operations.push(Operation::put(at_matrix(
                    child,
                    local_matrix(&group.transform).then(local_matrix(&child.transform)),
                    group.parent_id.as_deref(),
                )));
            }
            operations.push(Operation::remove(group.id.clone()));
        }
        let order = self
            .nodes()
            .iter()
            .flat_map(|n| match replacements.get(n.id.as_str()) {
                Some(children) => children.clone(),
                None if lifted.contains(n.id.as_str()) => Vec::new(),
                None => vec![n.id.clone()],
            })
            .collect();
        operations.push(Operation::Reorder { ids: order });
        check_size(&operations)?;
        Ok(SelectionEdit {
            operations,
            selected,
        })
    }

    /// The node a click on `id` selects: the outermost group between it and
    /// `scope` (the group being edited, or the root), unless `id` is already
    /// selected.
    pub fn selection_target(&self, id: &str, scope: Option<&str>, selected: &[String]) -> String {
        if selected.iter().any(|s| s == id) {
            return id.to_string();
        }
        let mut target = id.to_string();
        let mut node = self.get(id);
        let mut seen = BTreeSet::new();
        while let Some(current) = node {
            if current.parent_id.is_none()
                || current.parent_id.as_deref() == scope
                || !seen.insert(current.id.as_str())
            {
                break;
            }
            node = self.parent(current);
            if let Some(parent) = node.filter(|p| matches!(p.content, Content::Group)) {
                target = parent.id.clone();
            }
        }
        target
    }

    /// Scales the selection uniformly about `anchor` (scene coordinates).
    /// Sizes, brushes, strokes, fonts and corner radii are baked into each
    /// descendant, and `factor` is clamped so every result stays valid.
    /// Returns the full node list.
    pub fn scale(&self, ids: &[String], anchor: Vec2, factor: f64) -> Vec<Node> {
        let roots: Vec<&str> = self
            .selection_roots(ids)
            .iter()
            .map(|n| n.id.as_str())
            .collect();
        let affected = descendants(self.nodes(), roots);
        let (mut min, mut max) = (0.01_f64, 100.0_f64);
        for node in self.nodes().iter().filter(|n| affected.contains(&n.id)) {
            if node.transform.width > 0.0 {
                max = max.min(1e7 / node.transform.width);
            }
            if node.transform.height > 0.0 {
                max = max.min(1e7 / node.transform.height);
            }
            match &node.content {
                Content::Ink { brush, .. } => {
                    min = min.max(0.25 / brush.size);
                    max = max.min(160.0 / brush.size);
                }
                Content::Shape { stroke_width, .. } if *stroke_width > 0.0 => {
                    max = max.min(1000.0 / stroke_width);
                }
                Content::Text { font_size, .. } => {
                    min = min.max(4.0 / font_size);
                    max = max.min(512.0 / font_size);
                }
                _ => {}
            }
        }
        let factor = factor.clamp(min, max.max(min));
        let worlds: HashMap<&str, Matrix> = self
            .nodes()
            .iter()
            .map(|n| (n.id.as_str(), self.world_matrix(n)))
            .collect();
        let scaled: HashMap<&str, Matrix> = affected
            .iter()
            .filter_map(|id| worlds.get_key_value(id.as_str()))
            .map(|(id, m)| {
                let mut s = m.0;
                s[4] = anchor.x + (s[4] - anchor.x) * factor;
                s[5] = anchor.y + (s[5] - anchor.y) * factor;
                (*id, Matrix(s))
            })
            .collect();
        self.nodes()
            .iter()
            .map(|n| {
                if !affected.contains(&n.id) {
                    return n.clone();
                }
                let parent = n.parent_id.as_deref().map_or(Matrix::IDENTITY, |p| {
                    scaled
                        .get(p)
                        .or_else(|| worlds.get(p))
                        .copied()
                        .unwrap_or(Matrix::IDENTITY)
                });
                let mut next =
                    resize_node(n, n.transform.width * factor, n.transform.height * factor);
                match &mut next.content {
                    Content::Ink { brush, .. } => {
                        brush.size = (brush.size * factor).clamp(0.25, 160.0)
                    }
                    Content::Text { font_size, .. } => {
                        *font_size = (*font_size * factor).clamp(4.0, 512.0)
                    }
                    Content::Shape { stroke_width, .. } => *stroke_width *= factor,
                    _ => {}
                }
                if let Some(appearance) = &mut next.appearance {
                    appearance.corner_radius = (appearance.corner_radius * factor).min(1e7);
                    if let Some(shadow) = &mut appearance.shadow {
                        shadow.x = (shadow.x * factor).clamp(-1000.0, 1000.0);
                        shadow.y = (shadow.y * factor).clamp(-1000.0, 1000.0);
                        shadow.blur = (shadow.blur * factor).min(128.0);
                    }
                }
                if let Some(width) = next.text_layout.as_mut().and_then(|l| l.width.as_mut()) {
                    *width = (*width * factor).min(1e7);
                }
                at_matrix(
                    &next,
                    parent.inverse().then(scaled[n.id.as_str()]),
                    n.parent_id.as_deref(),
                )
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alignment {
    Left,
    Center,
    Right,
    Top,
    Middle,
    Bottom,
}

/// A resize handle on a selection frame, named by compass direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handle {
    N,
    S,
    E,
    W,
    Ne,
    Nw,
    Se,
    Sw,
}

impl Handle {
    fn west(self) -> bool {
        matches!(self, Handle::W | Handle::Nw | Handle::Sw)
    }
    fn east(self) -> bool {
        matches!(self, Handle::E | Handle::Ne | Handle::Se)
    }
    fn north(self) -> bool {
        matches!(self, Handle::N | Handle::Ne | Handle::Nw)
    }
    fn south(self) -> bool {
        matches!(self, Handle::S | Handle::Se | Handle::Sw)
    }
}

/// A row of a layers panel.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerRow<'a> {
    pub node: &'a Node,
    pub depth: usize,
    pub children: usize,
}

/// The oriented box drawn around a selection, with handles at its corners.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SelectionFrame {
    /// Maps the frame's local `0..width, 0..height` box to the scene.
    pub matrix: Matrix,
    pub width: f64,
    pub height: f64,
    pub center: Vec2,
    /// Degrees.
    pub angle: f64,
}

fn rotation_about(center: Vec2, degrees: f64) -> Matrix {
    let (s, c) = degrees.to_radians().sin_cos();
    Matrix([
        c,
        s,
        -s,
        c,
        center.x - c * center.x + s * center.y,
        center.y - s * center.x - c * center.y,
    ])
}

impl SelectionFrame {
    /// The frame turned by `degrees` about its center. Keeps a multi-node
    /// frame attached to a rotation gesture instead of its changing bounds.
    pub fn rotated(&self, degrees: f64) -> SelectionFrame {
        SelectionFrame {
            matrix: rotation_about(self.center, degrees).then(self.matrix),
            angle: self.angle + degrees,
            ..*self
        }
    }
}

/// Which line endpoint is being dragged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endpoint {
    Start,
    End,
}

impl<'a> SceneIndex<'a> {
    fn moved(&self, node: &Node, dx: f64, dy: f64) -> Node {
        let a = self.to_parent(node, Vec2::new(0.0, 0.0));
        let p = self.to_parent(node, Vec2::new(dx, dy));
        let mut next = node.clone();
        next.transform.x += p.x - a.x;
        next.transform.y += p.y - a.y;
        next
    }

    /// Moves the selection roots by a scene-space offset.
    pub fn translate(&self, ids: &[String], dx: f64, dy: f64) -> Vec<Operation> {
        self.selection_roots(ids)
            .into_iter()
            .map(|n| Operation::put(self.moved(n, dx, dy)))
            .collect()
    }

    /// Aligns two or more selection roots to an edge or center of their
    /// combined bounds.
    pub fn align(&self, ids: &[String], alignment: Alignment) -> Vec<Operation> {
        let roots = self.selection_roots(ids);
        let Some(all) = self
            .selection_object_bounds(ids)
            .filter(|_| roots.len() >= 2)
        else {
            return Vec::new();
        };
        roots
            .into_iter()
            .map(|n| {
                let b = self.object_bounds(n);
                let dx = match alignment {
                    Alignment::Left => all.x - b.x,
                    Alignment::Center => all.center().x - b.center().x,
                    Alignment::Right => all.right() - b.right(),
                    _ => 0.0,
                };
                let dy = match alignment {
                    Alignment::Top => all.y - b.y,
                    Alignment::Middle => all.center().y - b.center().y,
                    Alignment::Bottom => all.bottom() - b.bottom(),
                    _ => 0.0,
                };
                Operation::put(self.moved(n, dx, dy))
            })
            .collect()
    }

    /// Moves each selection root one step forward (`forward`) or backward
    /// among its siblings, past one unselected sibling. Empty when nothing
    /// can move.
    pub fn arrange(&self, ids: &[String], forward: bool) -> Vec<Operation> {
        let roots = self.selection_roots(ids);
        let selected: BTreeSet<&str> = roots.iter().map(|n| n.id.as_str()).collect();
        let mut order: Vec<&str> = self.nodes().iter().map(|n| n.id.as_str()).collect();
        let parents: BTreeSet<Option<&str>> =
            roots.iter().map(|n| n.parent_id.as_deref()).collect();
        for parent in parents {
            let indices: Vec<usize> = self
                .nodes()
                .iter()
                .enumerate()
                .filter(|(_, n)| n.parent_id.as_deref() == parent)
                .map(|(i, _)| i)
                .collect();
            let mut siblings: Vec<&str> = indices.iter().map(|&i| order[i]).collect();
            let steps: Vec<usize> = if forward {
                (0..siblings.len()).rev().collect()
            } else {
                (0..siblings.len()).collect()
            };
            for i in steps {
                let Some(j) = (if forward {
                    i.checked_add(1)
                } else {
                    i.checked_sub(1)
                }) else {
                    continue;
                };
                if j < siblings.len()
                    && selected.contains(siblings[i])
                    && !selected.contains(siblings[j])
                {
                    siblings.swap(i, j);
                }
            }
            for (k, &index) in indices.iter().enumerate() {
                order[index] = siblings[k];
            }
        }
        if order.iter().zip(self.nodes()).all(|(id, n)| *id == n.id) {
            return Vec::new();
        }
        vec![Operation::Reorder {
            ids: order.into_iter().map(str::to_string).collect(),
        }]
    }

    /// Layers panel rows, front-most first, children under their parent.
    /// `collapsed` hides a node's children.
    pub fn layer_rows(&self, collapsed: impl Fn(&Node) -> bool) -> Vec<LayerRow<'a>> {
        let mut children: HashMap<Option<&str>, Vec<&'a Node>> = HashMap::new();
        for node in self.nodes() {
            children
                .entry(node.parent_id.as_deref())
                .or_default()
                .push(node);
        }
        let mut stack: Vec<(&'a Node, usize)> = children
            .get(&None)
            .into_iter()
            .flatten()
            .map(|n| (*n, 0))
            .collect();
        let mut rows = Vec::new();
        let mut seen = BTreeSet::new();
        while let Some((node, depth)) = stack.pop() {
            if !seen.insert(node.id.as_str()) {
                continue;
            }
            let nested = children.get(&Some(node.id.as_str()));
            rows.push(LayerRow {
                node,
                depth,
                children: nested.map_or(0, Vec::len),
            });
            if !collapsed(node) {
                stack.extend(nested.into_iter().flatten().map(|n| (*n, depth + 1)));
            }
        }
        rows
    }

    /// The selection's handle frame. A single node keeps its rotation (a
    /// group fits its contents, a line runs along its endpoints with zero
    /// height); several nodes get their axis-aligned bounds.
    pub fn selection_frame(&self, ids: &[String]) -> Option<SelectionFrame> {
        let roots = self.selection_roots(ids);
        let bounds = self.selection_object_bounds(ids)?;
        let mut matrix = Matrix::translate(bounds.x, bounds.y);
        let (mut width, mut height) = (bounds.width, bounds.height);
        if let [node] = roots.as_slice() {
            matrix = self.world_matrix(node);
            if matches!(node.content, Content::Group) {
                let to_local = matrix.inverse();
                let mut points = Vec::new();
                let mut queue = vec![node.id.as_str()];
                while let Some(parent) = queue.pop() {
                    for child in self.children(Some(parent)) {
                        if !child.visible {
                            continue;
                        }
                        if matches!(child.content, Content::Group) {
                            queue.push(&child.id);
                            continue;
                        }
                        let m = to_local.then(self.world_matrix(child));
                        points.extend(self.outline(child).into_iter().map(|p| m.apply(p)));
                    }
                }
                match Rect::around(points) {
                    Some(r) => {
                        width = r.width.max(1.0);
                        height = r.height.max(1.0);
                        matrix = matrix.then(Matrix::translate(r.x, r.y));
                    }
                    None => {
                        width = node.transform.width;
                        height = node.transform.height;
                    }
                }
            } else if let Some(line) = crate::geometry::LineGeometry::of(node) {
                let start = matrix.apply(line.start);
                let end = matrix.apply(line.end);
                let (s, c) = (end.y - start.y).atan2(end.x - start.x).sin_cos();
                width = start.distance(end);
                height = 0.0;
                matrix = Matrix([c, s, -s, c, start.x, start.y]);
            } else {
                (width, height) = self.dimensions(node);
            }
        }
        Some(SelectionFrame {
            matrix,
            width,
            height,
            center: matrix.apply(Vec2::new(width / 2.0, height / 2.0)),
            angle: matrix.rotation(),
        })
    }

    /// Local points whose hull is the node's visible box.
    fn outline(&self, node: &Node) -> Vec<Vec2> {
        match crate::geometry::LineGeometry::of(node) {
            Some(line) => line.outline(),
            None => {
                let (w, h) = self.dimensions(node);
                Rect::new(0.0, 0.0, w, h).corners().to_vec()
            }
        }
    }

    /// Rotates the selection roots by `degrees` about a scene point;
    /// children follow their parents. Returns the full node list.
    pub fn rotate(&self, ids: &[String], center: Vec2, degrees: f64) -> Vec<Node> {
        let roots: BTreeSet<&str> = self
            .selection_roots(ids)
            .iter()
            .map(|n| n.id.as_str())
            .collect();
        let rotation = rotation_about(center, degrees);
        self.nodes()
            .iter()
            .map(|n| {
                if !roots.contains(n.id.as_str()) {
                    return n.clone();
                }
                at_matrix(
                    n,
                    self.parent_matrix(n)
                        .inverse()
                        .then(rotation.then(self.world_matrix(n))),
                    n.parent_id.as_deref(),
                )
            })
            .collect()
    }

    /// Resizes the selection by dragging `handle` to a scene point. Several
    /// nodes or a group scale uniformly about the opposite handle. A single
    /// node resizes its box; text scales its font, except that side handles
    /// rewrap it. Returns the full node list.
    pub fn resize_from_handle(
        &self,
        ids: &[String],
        handle: Handle,
        point: Vec2,
        preserve_aspect: bool,
    ) -> Vec<Node> {
        let roots = self.selection_roots(ids);
        let Some(frame) = self.selection_frame(ids) else {
            return self.nodes().to_vec();
        };
        if roots.len() > 1 || matches!(roots[0].content, Content::Group) {
            let local = frame.matrix.inverse().apply(point);
            let anchor = frame.matrix.apply(Vec2::new(
                if handle.west() { frame.width } else { 0.0 },
                if handle.north() { frame.height } else { 0.0 },
            ));
            let rx = if handle.west() {
                (frame.width - local.x) / frame.width
            } else {
                local.x / frame.width
            };
            let ry = if handle.north() {
                (frame.height - local.y) / frame.height
            } else {
                local.y / frame.height
            };
            let factor = match handle {
                Handle::E | Handle::W => rx,
                Handle::N | Handle::S => ry,
                _ if (rx - 1.0).abs() > (ry - 1.0).abs() => rx,
                _ => ry,
            };
            return self.scale(ids, anchor, factor);
        }
        let node = roots[0];
        let (w, h) = self.dimensions(node);
        let q = self.world_matrix(node).inverse().apply(point);
        let mut x = if handle.west() { q.x.min(w - 4.0) } else { 0.0 };
        let mut y = if handle.north() {
            q.y.min(h - 4.0)
        } else {
            0.0
        };
        let mut width = if handle.west() {
            w - x
        } else if handle.east() {
            q.x.max(4.0)
        } else {
            w
        };
        let mut height = if handle.north() {
            h - y
        } else if handle.south() {
            q.y.max(4.0)
        } else {
            h
        };
        let is_text = matches!(node.content, Content::Text { .. });
        let wrap = is_text && matches!(handle, Handle::E | Handle::W) && !preserve_aspect;
        if preserve_aspect || (is_text && !wrap) {
            let rx = width / w.max(1.0);
            let ry = height / h.max(1.0);
            let factor = if (rx - 1.0).abs() > (ry - 1.0).abs() {
                rx
            } else {
                ry
            }
            .max(0.01);
            width = (w * factor).max(4.0);
            height = (h * factor).max(4.0);
            x = if handle.west() {
                w - width
            } else if matches!(handle, Handle::N | Handle::S) {
                (w - width) / 2.0
            } else {
                0.0
            };
            y = if handle.north() {
                h - height
            } else if matches!(handle, Handle::E | Handle::W) {
                (h - height) / 2.0
            } else {
                0.0
            };
        }
        let mut next = resize_node(node, width, height);
        if is_text {
            next = node.clone();
            if wrap {
                next.text_layout.get_or_insert_with(Default::default).width = Some(width.max(4.0));
            } else {
                let factor = width / w.max(1.0);
                if let Content::Text { font_size, .. } = &mut next.content {
                    *font_size = (*font_size * factor).clamp(1.0, 4096.0);
                }
                if let Some(text_width) = next.text_layout.as_mut().and_then(|l| l.width.as_mut()) {
                    *text_width = (*text_width * factor).max(4.0);
                }
            }
            let (fw, fh) = self.dimensions(&next);
            next.transform.width = fw;
            next.transform.height = fh;
            x = if handle.west() { w - fw } else { 0.0 };
            y = if handle.north() { h - fh } else { 0.0 };
        }
        let target = local_matrix(&node.transform).apply(Vec2::new(x, y));
        let origin = local_matrix(&next.transform).0;
        next.transform.x += target.x - origin[4];
        next.transform.y += target.y - origin[5];
        self.nodes()
            .iter()
            .map(|n| {
                if n.id == node.id {
                    next.clone()
                } else {
                    n.clone()
                }
            })
            .collect()
    }

    /// Drags one end of a line to a scene point, optionally snapping its
    /// angle to 45 degrees. Other nodes come back unchanged.
    pub fn move_line_endpoint(
        &self,
        node: &Node,
        endpoint: Endpoint,
        point: Vec2,
        snap: bool,
    ) -> Node {
        let Some(line) = crate::geometry::LineGeometry::of(node) else {
            return node.clone();
        };
        let m = self.world_matrix(node);
        let fixed = m.apply(match endpoint {
            Endpoint::Start => line.end,
            Endpoint::End => line.start,
        });
        let moved = crate::geometry::snap_line_point(fixed, point, snap);
        let (start, end) = match endpoint {
            Endpoint::Start => (moved, fixed),
            Endpoint::End => (fixed, moved),
        };
        crate::geometry::set_line_endpoints(
            node,
            self.to_parent(node, start),
            self.to_parent(node, end),
        )
    }
}
