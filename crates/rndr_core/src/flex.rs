//! Optional layout components that extend [`Layout`](crate::Layout) without
//! changing it. An entity without any of them lays out exactly as before.

use crate::entity::Entity;
use crate::hierarchy::children_of;
use crate::layout::{Axis, Layout, Size};
use crate::world::{Component, World};

impl Component for Align {}
impl Component for AlignSelf {}
impl Component for Padding {}
impl Component for MaxSize {}
impl Component for AspectRatio {}
impl Component for Absolute {}
impl Component for Wrap {}

/// Placement of children along a container's stack axis.
///
/// Only leftover space is distributed, so a container with a [`Size::Fill`]
/// child on its stack axis has none to distribute.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Justify {
    /// Pack children at the start.
    #[default]
    Start,
    /// Pack children around the middle.
    Center,
    /// Pack children at the end.
    End,
    /// First child at the start, last at the end, equal space between.
    SpaceBetween,
    /// Equal space around each child; half a share at either end.
    SpaceAround,
    /// Equal space between children and at both ends.
    SpaceEvenly,
}

/// Placement of a child across a container's stack axis.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Items {
    /// Align to the start of the cross axis.
    #[default]
    Start,
    /// Center on the cross axis.
    Center,
    /// Align to the end of the cross axis.
    End,
    /// Take the full cross size unless the child sets a fixed size there.
    Stretch,
}

/// Container alignment, like CSS `justify-content` and `align-items`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Align {
    /// Main-axis distribution of leftover space.
    pub justify: Justify,
    /// Cross-axis placement of each child.
    pub items: Items,
}

impl Align {
    /// Center children on both axes.
    pub fn center() -> Self {
        Self {
            justify: Justify::Center,
            items: Items::Center,
        }
    }
}

/// Cross-axis placement of one child, overriding its parent's [`Align::items`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AlignSelf(pub Items);

/// Inset per side, in points. Overrides [`Layout::padding`] when present.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Padding {
    /// Top inset.
    pub top: f32,
    /// Right inset.
    pub right: f32,
    /// Bottom inset.
    pub bottom: f32,
    /// Left inset.
    pub left: f32,
}

impl Padding {
    /// The same inset on every side.
    pub fn all(v: f32) -> Self {
        Self::symmetric(v, v)
    }

    /// `vertical` on top and bottom, `horizontal` on left and right.
    pub fn symmetric(vertical: f32, horizontal: f32) -> Self {
        Self {
            top: vertical,
            right: horizontal,
            bottom: vertical,
            left: horizontal,
        }
    }
}

/// Ceiling on the measured or filled size, in points.
///
/// [`Layout::min_width`] and [`Layout::min_height`] win when they exceed it,
/// as in CSS.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaxSize {
    /// Widest the box may be. [`f32::INFINITY`] leaves width unbounded.
    pub width: f32,
    /// Tallest the box may be. [`f32::INFINITY`] leaves height unbounded.
    pub height: f32,
}

impl MaxSize {
    /// Bound width only.
    pub fn width(width: f32) -> Self {
        Self {
            width,
            height: f32::INFINITY,
        }
    }

    /// Bound height only.
    pub fn height(height: f32) -> Self {
        Self {
            width: f32::INFINITY,
            height,
        }
    }
}

/// Width divided by height.
///
/// A box whose height is not [`Size::Px`] takes its height from its width;
/// otherwise a box whose width is not [`Size::Px`] takes its width from its
/// height. Non-positive or non-finite ratios are ignored.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AspectRatio(pub f32);

/// Take a child out of its parent's stack and pin it inside the parent's box.
///
/// Offsets are in points from the parent's matching edge, before padding.
/// With both `left` and `right` set, a width other than [`Size::Px`] stretches
/// between them; likewise `top` and `bottom` for height. With neither on an
/// axis, the child sits at the parent's padded start. The parent's
/// [`crate::Scroll`] offset moves it with the content. Absolute children
/// paint and hit-test after their in-flow siblings.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Absolute {
    /// Distance from the parent's left edge.
    pub left: Option<f32>,
    /// Distance from the parent's top edge.
    pub top: Option<f32>,
    /// Distance from the parent's right edge.
    pub right: Option<f32>,
    /// Distance from the parent's bottom edge.
    pub bottom: Option<f32>,
}

impl Absolute {
    /// Pin to every edge at `inset`.
    pub fn fill(inset: f32) -> Self {
        Self {
            left: Some(inset),
            top: Some(inset),
            right: Some(inset),
            bottom: Some(inset),
        }
    }
}

/// Break a [`Axis::Row`] container's children onto new lines when the next
/// child would overflow its width. [`Layout::gap`] separates children within
/// a line; `line_gap` separates lines. [`Align::justify`] applies per line and
/// [`Align::items`] within each line's height. Ignored on columns.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Wrap {
    /// Space between lines, in points.
    pub line_gap: f32,
}

/// Children in paint and hit-test order: in-flow children, then [`Absolute`]
/// children, each group in child order.
pub fn paint_order(world: &World, entity: Entity) -> Vec<Entity> {
    let mut kids = children_of(world, entity);
    kids.sort_by_key(|child| world.has::<Absolute>(*child));
    kids
}

/// `[top, right, bottom, left]` padding.
pub(crate) fn insets(world: &World, entity: Entity, layout: &Layout) -> [f32; 4] {
    match world.get::<Padding>(entity) {
        Some(p) => [p.top, p.right, p.bottom, p.left].map(|v| if v.is_finite() { v } else { 0.0 }),
        None => [layout.padding; 4],
    }
}

pub(crate) fn in_flow(world: &World, entity: Entity) -> bool {
    !world.has::<Absolute>(entity)
}

pub(crate) fn wraps(world: &World, entity: Entity, layout: &Layout) -> Option<f32> {
    (layout.axis == Axis::Row)
        .then(|| world.get::<Wrap>(entity))
        .flatten()
        .map(|w| w.line_gap.max(0.0))
}

/// Apply the aspect ratio, then [`MaxSize`], then the layout minimums.
pub(crate) fn constrain(
    world: &World,
    entity: Entity,
    layout: &Layout,
    w: f32,
    h: f32,
) -> (f32, f32) {
    let (mut w, mut h) = (w, h);
    if let Some(ratio) = world
        .get::<AspectRatio>(entity)
        .map(|r| r.0)
        .filter(|r| r.is_finite() && *r > 0.0)
    {
        if !matches!(layout.height, Size::Px(_)) {
            h = w / ratio;
        } else if !matches!(layout.width, Size::Px(_)) {
            w = h * ratio;
        }
    }
    if let Some(max) = world.get::<MaxSize>(entity) {
        if !max.width.is_nan() {
            w = w.min(max.width);
        }
        if !max.height.is_nan() {
            h = h.min(max.height);
        }
    }
    (w.max(layout.min_width), h.max(layout.min_height))
}

pub(crate) fn self_items(world: &World, parent: Entity, child: Entity) -> Option<Items> {
    world
        .get::<AlignSelf>(child)
        .map(|a| a.0)
        .or_else(|| world.get::<Align>(parent).map(|a| a.items))
}

/// Leading offset and extra spacing for `count` children with `leftover`
/// main-axis space.
pub(crate) fn distribute(justify: Justify, leftover: f32, count: usize) -> (f32, f32) {
    let leftover = leftover.max(0.0);
    let n = count as f32;
    match justify {
        Justify::Start => (0.0, 0.0),
        Justify::Center => (leftover * 0.5, 0.0),
        Justify::End => (leftover, 0.0),
        Justify::SpaceBetween if count > 1 => (0.0, leftover / (n - 1.0)),
        Justify::SpaceBetween => (0.0, 0.0),
        Justify::SpaceAround if count > 0 => (leftover / n * 0.5, leftover / n),
        Justify::SpaceEvenly if count > 0 => (leftover / (n + 1.0), leftover / (n + 1.0)),
        Justify::SpaceAround | Justify::SpaceEvenly => (0.0, 0.0),
    }
}

/// Offset within `room` for a child of size `size`.
pub(crate) fn cross_offset(items: Items, room: f32, size: f32) -> f32 {
    match items {
        Items::Center => (room - size) * 0.5,
        Items::End => room - size,
        Items::Start | Items::Stretch => 0.0,
    }
}

/// Greedy line breaking: index ranges of children per line.
pub(crate) fn break_lines(widths: &[f32], gap: f32, room: f32) -> Vec<std::ops::Range<usize>> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut used = 0.0f32;
    for (i, w) in widths.iter().enumerate() {
        if i > start && used + gap + w > room + 0.01 {
            lines.push(start..i);
            start = i;
            used = *w;
        } else {
            used += if i > start { gap + w } else { *w };
        }
    }
    if start < widths.len() {
        lines.push(start..widths.len());
    }
    lines
}

/// Size of an [`Absolute`] child on one axis inside a parent of `parent` points.
/// The second value is true when the child stretches between both edges.
pub(crate) fn absolute_span(
    start: Option<f32>,
    end: Option<f32>,
    size: Size,
    intrinsic: f32,
    parent: f32,
) -> (f32, bool) {
    match (start, end, size) {
        (_, _, Size::Px(px)) => (px, false),
        (Some(s), Some(e), _) => ((parent - s - e).max(0.0), true),
        (_, _, Size::Fill) => (parent, true),
        (_, _, Size::Hug) => (intrinsic, false),
    }
}

/// Origin of an [`Absolute`] child of length `len` on one axis.
pub(crate) fn absolute_origin(
    start: Option<f32>,
    end: Option<f32>,
    len: f32,
    origin: f32,
    parent: f32,
    padded: f32,
) -> f32 {
    match (start, end) {
        (Some(s), _) => origin + s,
        (None, Some(e)) => origin + parent - e - len,
        (None, None) => padded,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distribute_matches_css() {
        assert_eq!(distribute(Justify::Center, 100.0, 2), (50.0, 0.0));
        assert_eq!(distribute(Justify::SpaceBetween, 90.0, 4), (0.0, 30.0));
        assert_eq!(distribute(Justify::SpaceAround, 80.0, 4), (10.0, 20.0));
        assert_eq!(distribute(Justify::SpaceEvenly, 100.0, 4), (20.0, 20.0));
        assert_eq!(distribute(Justify::End, -5.0, 2), (0.0, 0.0));
    }

    #[test]
    fn lines_break_greedily() {
        assert_eq!(
            break_lines(&[40.0, 40.0, 40.0], 10.0, 90.0),
            vec![0..2, 2..3]
        );
        assert_eq!(break_lines(&[200.0, 10.0], 0.0, 100.0), vec![0..1, 1..2]);
        assert!(break_lines(&[], 0.0, 100.0).is_empty());
    }
}
