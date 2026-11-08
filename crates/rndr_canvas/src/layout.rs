//! Layout helpers: frame alignment, distribution, grid and smart-guide
//! snapping.

use crate::geometry::{descendants, Rect, SceneIndex, Vec2};
use crate::ops::Operation;
use crate::schema::{Content, Node};
use crate::selection::Alignment;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Snapping settings. Not part of a document.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Snapping {
    pub grid_size: f64,
    pub snap_grid: bool,
    pub smart_guides: bool,
    /// Guide tolerance in view pixels; divided by zoom in the scene.
    pub tolerance: f64,
}

impl Default for Snapping {
    fn default() -> Self {
        Self {
            grid_size: 16.0,
            snap_grid: false,
            smart_guides: true,
            tolerance: 6.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Axis {
    X,
    Y,
}

/// A guide line to draw while snapping: `axis` is the coordinate that is
/// fixed at `value`, running from `from` to `to` on the other axis.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Guide {
    pub axis: Axis,
    pub value: f64,
    pub from: f64,
    pub to: f64,
}

/// A snapped drag offset with the guides that caused it.
#[derive(Debug, Clone, PartialEq)]
pub struct SnappedMove {
    pub delta: Vec2,
    pub guides: Vec<Guide>,
}

impl Snapping {
    /// Rounds to the grid when grid snapping is on and not bypassed.
    pub fn snap_point(&self, point: Vec2, bypass: bool) -> Vec2 {
        if !self.snap_grid || bypass || self.grid_size <= 0.0 {
            return point;
        }
        let g = self.grid_size;
        Vec2::new((point.x / g).round() * g, (point.y / g).round() * g)
    }

    /// Adjusts a drag of `bounds` by `delta` so an edge or center lands on
    /// a target's edge or center within tolerance; otherwise onto the grid.
    /// Guides win over the grid on each axis.
    pub fn snap_move(
        &self,
        bounds: Rect,
        targets: &[Rect],
        delta: Vec2,
        zoom: f64,
        bypass: bool,
    ) -> SnappedMove {
        let mut result = SnappedMove {
            delta,
            guides: Vec::new(),
        };
        if bypass || (delta.x == 0.0 && delta.y == 0.0) {
            return result;
        }
        let tolerance = self.tolerance / zoom.max(f64::EPSILON);
        for axis in [Axis::X, Axis::Y] {
            let pick = |r: Rect| match axis {
                Axis::X => (r.x, r.width, r.y, r.height),
                Axis::Y => (r.y, r.height, r.x, r.width),
            };
            let (pos, size, other, other_size) = pick(bounds);
            let (d, d_other) = match axis {
                Axis::X => (delta.x, delta.y),
                Axis::Y => (delta.y, delta.x),
            };
            let start = pos + d;
            let anchors = [start, start + size / 2.0, start + size];
            let mut closest: Option<(f64, f64, Rect)> = None;
            if self.smart_guides {
                for target in targets {
                    let (tp, ts, _, _) = pick(*target);
                    for value in [tp, tp + ts / 2.0, tp + ts] {
                        for anchor in anchors {
                            let offset = value - anchor;
                            if offset.abs() <= tolerance
                                && closest.is_none_or(|(o, _, _)| offset.abs() < o.abs())
                            {
                                closest = Some((offset, value, *target));
                            }
                        }
                    }
                }
            }
            let adjust = if let Some((offset, value, target)) = closest {
                let (_, _, to, tos) = pick(target);
                result.guides.push(Guide {
                    axis,
                    value,
                    from: (other + d_other).min(to),
                    to: (other + d_other + other_size).max(to + tos),
                });
                offset
            } else if self.snap_grid && self.grid_size > 0.0 {
                (start / self.grid_size).round() * self.grid_size - start
            } else {
                0.0
            };
            match axis {
                Axis::X => result.delta.x += adjust,
                Axis::Y => result.delta.y += adjust,
            }
        }
        result
    }
}

impl<'a> SceneIndex<'a> {
    /// The innermost frame containing every selection root.
    pub fn containing_frame(&self, ids: &[String]) -> Option<&'a Node> {
        let roots = self.selection_roots(ids);
        let first = roots.first()?;
        self.ancestors(first).into_iter().find(|frame| {
            matches!(frame.content, Content::Frame { .. })
                && roots
                    .iter()
                    .all(|r| self.ancestors(r).iter().any(|a| a.id == frame.id))
        })
    }

    /// Bounds in the frame's own axes, looking through groups.
    fn frame_bounds(&self, node: &Node, frame: &Node) -> Rect {
        if matches!(node.content, Content::Group) {
            let children: Vec<_> = self
                .children(Some(&node.id))
                .into_iter()
                .filter(|n| n.visible)
                .collect();
            if let Some(r) = Rect::union_all(children.iter().map(|n| self.frame_bounds(n, frame))) {
                return r;
            }
        }
        let m = self
            .world_matrix(frame)
            .inverse()
            .then(self.world_matrix(node));
        let (w, h) = self.dimensions(node);
        Rect::around(Rect::new(0.0, 0.0, w, h).corners().map(|p| m.apply(p))).unwrap_or_default()
    }

    /// Aligns the selection roots to an edge or center of the frame that
    /// contains them all, in the frame's (possibly rotated) axes.
    pub fn align_to_frame(&self, ids: &[String], alignment: Alignment) -> Vec<Operation> {
        let Some(frame) = self.containing_frame(ids) else {
            return Vec::new();
        };
        let matrix = self.world_matrix(frame);
        let origin = matrix.apply(Vec2::default());
        let (fw, fh) = (frame.transform.width, frame.transform.height);
        self.selection_roots(ids)
            .into_iter()
            .map(|node| {
                let b = self.frame_bounds(node, frame);
                let x = match alignment {
                    Alignment::Left => -b.x,
                    Alignment::Center => (fw - b.width) / 2.0 - b.x,
                    Alignment::Right => fw - b.width - b.x,
                    _ => 0.0,
                };
                let y = match alignment {
                    Alignment::Top => -b.y,
                    Alignment::Middle => (fh - b.height) / 2.0 - b.y,
                    Alignment::Bottom => fh - b.height - b.y,
                    _ => 0.0,
                };
                let p = matrix.apply(Vec2::new(x, y));
                self.translate(
                    std::slice::from_ref(&node.id),
                    p.x - origin.x,
                    p.y - origin.y,
                )
                .remove(0)
            })
            .collect()
    }

    /// Spaces three or more selection roots evenly between the outermost
    /// two along `axis`.
    pub fn distribute(&self, ids: &[String], axis: Axis) -> Vec<Operation> {
        let span = |r: &Rect| match axis {
            Axis::X => (r.x, r.width),
            Axis::Y => (r.y, r.height),
        };
        let mut items: Vec<(&Node, Rect)> = self
            .selection_roots(ids)
            .into_iter()
            .map(|n| (n, self.object_bounds(n)))
            .collect();
        if items.len() < 3 {
            return Vec::new();
        }
        items.sort_by(|a, b| {
            let (ap, asz) = span(&a.1);
            let (bp, bsz) = span(&b.1);
            (ap + asz / 2.0).total_cmp(&(bp + bsz / 2.0))
        });
        let (first_pos, first_size) = span(&items[0].1);
        let (last_pos, last_size) = span(&items[items.len() - 1].1);
        let total: f64 = items.iter().map(|(_, b)| span(b).1).sum();
        let gap = (last_pos + last_size - first_pos - total) / (items.len() - 1) as f64;
        let mut cursor = first_pos + first_size + gap;
        items[1..items.len() - 1]
            .iter()
            .map(|(node, b)| {
                let (pos, size) = span(b);
                let delta = cursor - pos;
                cursor += size + gap;
                let (dx, dy) = match axis {
                    Axis::X => (delta, 0.0),
                    Axis::Y => (0.0, delta),
                };
                self.translate(std::slice::from_ref(&node.id), dx, dy)
                    .remove(0)
            })
            .collect()
    }

    /// Bounds a dragged selection can snap to: visible siblings of the
    /// selection roots and their enclosing frames.
    pub fn snap_targets(&self, ids: &[String]) -> Vec<Rect> {
        let roots = self.selection_roots(ids);
        let excluded = descendants(self.nodes(), roots.iter().map(|n| n.id.as_str()));
        let containers: BTreeSet<&str> = roots
            .iter()
            .flat_map(|n| self.ancestors(n))
            .map(|a| a.id.as_str())
            .collect();
        let parents: BTreeSet<Option<&str>> =
            roots.iter().map(|n| n.parent_id.as_deref()).collect();
        self.nodes()
            .iter()
            .filter(|n| {
                !excluded.contains(&n.id)
                    && (!containers.contains(n.id.as_str())
                        || matches!(n.content, Content::Frame { .. }))
                    && (parents.contains(&n.parent_id.as_deref())
                        || containers.contains(n.id.as_str()))
                    && std::iter::once(*n)
                        .chain(self.ancestors(n))
                        .all(|a| a.visible && a.opacity > 0.0)
            })
            .map(|n| self.object_bounds(n))
            .collect()
    }
}
