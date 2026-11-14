use crate::entity::Entity;
use crate::flex;
use crate::font::{layout_cache_key, text_face, TextLayout};
use crate::grid::{self, Grid};
use crate::hierarchy::{children_of, roots, Parent};
use crate::input::{Input, Viewport};
use crate::split::Splitter;
use crate::style::{label_box, Label, TextOverflow};
use crate::text::{style_for, text_engine};
use crate::world::{Component, World};

impl Component for Layout {}
impl Component for Rect {}
impl Component for Clip {}
impl Component for Scroll {}
impl Component for IgnorePointer {}

/// Main axis children stack along.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    /// Children left to right. Width is the main size.
    Row,
    /// Children top to bottom. Height is the main size.
    Column,
}

/// How one side of a [`Layout`] is measured.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Size {
    /// Fixed length in points.
    Px(f32),
    /// Fit the children, or the label when there are none.
    Hug,
    /// Share leftover space on the parent's main axis, or take the parent's
    /// inner size on the cross axis.
    Fill,
}

/// Intent for how a node sizes and stacks its children.
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    /// Stack direction.
    pub axis: Axis,
    /// Space between children, in points. Not an outer margin.
    pub gap: f32,
    /// Inset on every side, in points. The content box shrinks by twice this.
    pub padding: f32,
    /// Horizontal measure. [`Size::Fill`] is resolved while placing.
    pub width: Size,
    /// Vertical measure. [`Size::Fill`] is resolved while placing.
    pub height: Size,
    /// Floor on the measured or filled width, in points.
    pub min_width: f32,
    /// Floor on the measured or filled height, in points.
    pub min_height: f32,
}

impl Layout {
    /// Hug both axes, no gap or padding, [`Axis::Column`].
    pub fn column() -> Self {
        Self {
            axis: Axis::Column,
            gap: 0.0,
            padding: 0.0,
            width: Size::Hug,
            height: Size::Hug,
            min_width: 0.0,
            min_height: 0.0,
        }
    }

    /// Hug both axes, no gap or padding, [`Axis::Row`].
    pub fn row() -> Self {
        Self {
            axis: Axis::Row,
            gap: 0.0,
            padding: 0.0,
            width: Size::Hug,
            height: Size::Hug,
            min_width: 0.0,
            min_height: 0.0,
        }
    }
}

/// Placed rectangle in logical points, origin top-left.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    /// Left edge in logical points.
    pub x: f32,
    /// Top edge in logical points.
    pub y: f32,
    /// Width in logical points.
    pub w: f32,
    /// Height in logical points.
    pub h: f32,
}

impl Rect {
    /// `x` and `y` lie inside. The right and bottom edges are excluded.
    pub fn contains(self, x: f32, y: f32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
}

/// Visible rectangle after ancestor intersection. Origin top-left.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Clip {
    /// Left edge in logical points.
    pub x: f32,
    /// Top edge in logical points.
    pub y: f32,
    /// Width in logical points. A non-positive width contains nothing.
    pub w: f32,
    /// Height in logical points. A non-positive height contains nothing.
    pub h: f32,
}

impl Clip {
    /// `x` and `y` lie inside a positive area. The right and bottom edges
    /// are excluded.
    pub fn contains(self, x: f32, y: f32) -> bool {
        self.w > 0.0
            && self.h > 0.0
            && x >= self.x
            && y >= self.y
            && x < self.x + self.w
            && y < self.y + self.h
    }

    /// Overlap of the two rectangles. Width and height are never negative.
    pub fn intersect(self, other: Self) -> Self {
        let x1 = self.x.max(other.x);
        let y1 = self.y.max(other.y);
        let x2 = (self.x + self.w).min(other.x + other.w);
        let y2 = (self.y + self.h).min(other.y + other.h);
        Self {
            x: x1,
            y: y1,
            w: (x2 - x1).max(0.0),
            h: (y2 - y1).max(0.0),
        }
    }

    /// Same edges as `rect`, including a zero size.
    pub fn from_rect(rect: Rect) -> Self {
        Self {
            x: rect.x,
            y: rect.y,
            w: rect.w,
            h: rect.h,
        }
    }
}

/// Bounded port whose children may overflow. Offset is how far content has moved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scroll {
    /// Content translation in points, x then y. Zero shows the top-left.
    /// [`layout_propagate`] adds this to the child origin. [`Self::clamp_to`]
    /// keeps each component at most zero.
    pub offset: [f32; 2],
    /// Content size in points, width then height, padding included.
    /// [`layout_propagate`] writes this from the children.
    pub content: [f32; 2],
}

impl Default for Scroll {
    fn default() -> Self {
        Self {
            offset: [0.0, 0.0],
            content: [0.0, 0.0],
        }
    }
}

/// Movement, in points, before a held contact starts scrolling.
pub const SCROLL_DRAG_SLOP: f32 = 4.0;

/// Hit testing skips this entity and its descendants. Paint is unchanged.
///
/// A drag preview uses this so the ghost under the pointer does not become
/// the drop target.
#[derive(Clone, Copy, Debug, Default)]
pub struct IgnorePointer;

/// Claims a press so [`apply_scroll`] does not capture that contact.
///
/// The function sees the entity under the pointer and must not mutate the
/// world. Returning true leaves [`ScrollDrag::entity`] empty. The claimant
/// owns the gesture, including any edge autoscroll.
#[derive(Clone, Copy)]
pub struct PointerDragOwner(
    /// Called with the hit entity on a press that would otherwise scroll.
    pub fn(&World, Entity) -> bool,
);

/// One captured scroll drag. Cleared when the pointer is released.
/// A press that starts on a [`Splitter`] does not capture.
#[derive(Clone, Copy, Debug, Default)]
pub struct ScrollDrag {
    /// Scrollport captured by the press. `None` when the press missed one.
    pub entity: Option<Entity>,
    /// Pointer position at the press, in logical points.
    pub origin: [f32; 2],
    /// Pointer position at the previous move, in logical points.
    pub last: [f32; 2],
    /// The pointer has moved at least [`SCROLL_DRAG_SLOP`] from [`Self::origin`].
    pub active: bool,
}

impl Scroll {
    /// Keep [`Self::offset`] inside the port. Content smaller than the port
    /// pins the offset at zero.
    pub fn clamp_to(&mut self, port_w: f32, port_h: f32) {
        let min_x = (port_w - self.content[0]).min(0.0);
        let min_y = (port_h - self.content[1]).min(0.0);
        self.offset[0] = self.offset[0].clamp(min_x, 0.0);
        self.offset[1] = self.offset[1].clamp(min_y, 0.0);
    }
}

/// Parent-before-child placement after a bottom-up hug measure.
pub fn layout_propagate(world: &mut World) {
    reconcile_viewports(world);
    layout_once(world);
    // A first frame/resize establishes viewport geometry. Reconcile once more,
    // then measure/place the realized rows before any hit testing or paint.
    if reconcile_viewports(world) {
        layout_once(world);
    }
}

/// Uniform-window content reconciliation inside canonical layout/scroll refresh.
/// Return true only when geometry/topology changed. Must not invoke layout,
/// scroll or focus reveal recursively. Called at most twice per propagation.
#[derive(Clone, Copy)]
pub struct ViewportRows(pub fn(&mut World, Entity) -> bool);
impl Component for ViewportRows {}

fn reconcile_viewports(world: &mut World) -> bool {
    let mut changed = false;
    for entity in world.entities_with::<ViewportRows>() {
        if let Some(callback) = world.get::<ViewportRows>(entity).copied() {
            changed |= (callback.0)(world, entity);
        }
    }
    changed
}

fn layout_once(world: &mut World) {
    let viewport = world.resource::<Viewport>().copied().unwrap_or_default();
    let root_ids: Vec<Entity> = roots::<Layout>(world);
    for root in root_ids {
        measure(world, root);
    }
    let parent_clip = Clip {
        x: 0.0,
        y: 0.0,
        w: viewport.width,
        h: viewport.height,
    };
    let mut ordered = roots::<Layout>(world);
    // Popups after ordinary roots, and submenus after the popup they open from.
    ordered.sort_by_key(|e| popup_depth(world, *e));
    for root in ordered {
        if world.has::<crate::Hidden>(root) {
            continue;
        }
        if let Some(popup) = world.get::<crate::AnchoredPopup>(root).copied() {
            let anchor = world.get::<Rect>(popup.anchor).copied().unwrap_or_default();
            let placement = world
                .get::<crate::PopupPlacement>(root)
                .copied()
                .unwrap_or_default();
            let hug = world.get::<Rect>(root).map_or(0.0, |r| r.w);
            let width = match placement {
                crate::PopupPlacement::Below => anchor.w.max(240.0),
                _ if hug > 0.0 => hug,
                _ => 240.0,
            }
            .min(viewport.content_width().max(0.0));
            refine_wrapped_heights(world, root, width, 0.0);
            let wanted = world.get::<Rect>(root).copied().unwrap_or_default();
            let height = wanted.h.min(popup.max_height);
            let r = match placement {
                crate::PopupPlacement::Below => crate::popup_rect(anchor, viewport, width, height),
                crate::PopupPlacement::At([x, y]) => {
                    let point = Rect {
                        x,
                        y,
                        w: 0.0,
                        h: 0.0,
                    };
                    crate::popup_rect(point, viewport, width, height)
                }
                crate::PopupPlacement::Beside => {
                    crate::popup::beside_rect(anchor, viewport, width, height)
                }
            };
            // Keep Hug intrinsic content for Scroll while constraining its viewport.
            world.insert(root, r);
            let original = *world.get::<Layout>(root).unwrap();
            world.get_mut::<Layout>(root).unwrap().height = Size::Fill;
            place(world, root, r.x, r.y, r.w, r.h, parent_clip);
            world.insert(root, original);
            continue;
        }
        // Fill widths are unknown during the intrinsic measure. Resolve those
        // widths and wrapped Hug heights before committing any ancestor clip.
        refine_wrapped_heights(world, root, viewport.content_width(), viewport.insets.left);
        place(
            world,
            root,
            viewport.insets.left,
            viewport.insets.top,
            viewport.content_width(),
            viewport.content_height(),
            parent_clip,
        );
    }
    apply_sticky(world);
}

/// Topmost (deepest, then later sibling) entity whose visible clip contains the point.
pub fn hit_test(world: &World, x: f32, y: f32) -> Option<Entity> {
    let mut hit = None;
    let floor = crate::overlay::modal_floor(world);
    for root in layered_roots(world) {
        if floor.is_some_and(|floor| crate::overlay::layer_of(world, root) < floor) {
            continue;
        }
        hit_walk(world, root, [x, y], &mut hit);
    }
    hit
}

// Paint order with the point mapped into each entity's layout space. A
// transformed group is composited inside its parent's clip, so a point outside
// that clip misses the whole group; a collapsed transform draws nothing.
fn hit_walk(world: &World, entity: Entity, point: [f32; 2], hit: &mut Option<Entity>) {
    if !world.is_alive(entity)
        || world.has::<crate::tabs::Hidden>(entity)
        || world.has::<IgnorePointer>(entity)
    {
        return;
    }
    let mut point = point;
    if let (Some(transform), Some(rect)) = (
        world
            .get::<crate::Transform>(entity)
            .filter(|t| !t.is_identity()),
        world.get::<Rect>(entity).copied(),
    ) {
        let parent_clip = world
            .get::<Parent>(entity)
            .and_then(|parent| world.get::<Clip>(parent.entity).copied());
        if parent_clip.is_some_and(|clip| !clip.contains(point[0], point[1])) {
            return;
        }
        let Some(inverse) = crate::compose::invert(transform.matrix(rect)) else {
            return;
        };
        point = crate::compose::apply(inverse, point);
    }
    let [x, y] = point;
    if let Some(clip) = world.get::<Clip>(entity).copied() {
        if clip.contains(x, y) {
            *hit = Some(entity);
        }
    } else if let Some(rect) = world.get::<Rect>(entity).copied() {
        if rect.contains(x, y) {
            *hit = Some(entity);
        }
    }
    for child in flex::paint_order(world, entity) {
        hit_walk(world, child, point, hit);
    }
}

/// Apply the wheel, then a held contact.
/// Each wheel axis goes to the deepest visible viewport under the pointer
/// that can move on that axis. An [`crate::Editor`] consumes vertical motion
/// first (including sub-row deltas); its sibling [`crate::Gutter`] routes to it
/// when unambiguous. Ordinary [`Scroll`] ancestors consume the remainder. Only its bounded consumption is removed; the
/// remainder chains through ancestors at boundaries. A miss never scrolls an
/// unrelated root. Pointer dragging retains its existing captured ownership.
/// A press that is not on a [`Splitter`] captures that scrollport, except a
/// left press inside an [`crate::Editor`] (which owns selection drag). Once the
/// contact moves [`SCROLL_DRAG_SLOP`] points, further movement is added to its
/// offset. A press [`PointerDragOwner`] claims is not captured. The press
/// itself is left for focus. Offset changes are clamped and
/// laid out immediately so later capture, hit testing and focus reveal read
/// matching rectangles/clips, even when this system runs at Input front.
pub fn apply_scroll(world: &mut World) {
    let Some(input) = world.resource::<Input>().cloned() else {
        clear_scroll_drag(world);
        return;
    };
    apply_wheel(world, &input);
    apply_pointer_drag(world, &input);
}

fn apply_wheel(world: &mut World, input: &Input) {
    if input.scroll[0] == 0.0 && input.scroll[1] == 0.0 {
        return;
    }
    let mut target = hit_test(world, input.pointer.position[0], input.pointer.position[1]);
    let mut remaining = input.scroll.map(|v| if v.is_finite() { v } else { 0. });
    // Shift turns a vertical-only wheel horizontal, as desktop platforms do.
    if input.shift && remaining[0] == 0.0 {
        remaining = [remaining[1], 0.0];
    }
    let mut changed = false;
    // Walk the hit's actual ancestry once; no viewport-wide fallback or axis
    // remapping. Defer relayout until consumption is complete so recycling
    // cannot change this event's ownership midway through the chain.
    while let Some(entity) = target {
        let editor = if world.has::<crate::Editor>(entity) {
            Some(entity)
        } else if world.has::<crate::Gutter>(entity) {
            // Gutters are sibling chrome, not scrollports. Only route when
            // their documented sibling-editor relationship is unambiguous.
            world.get::<Parent>(entity).and_then(|parent| {
                let siblings = children_of(world, parent.entity);
                let mut editors = siblings
                    .into_iter()
                    .filter(|e| world.has::<crate::Editor>(*e));
                let editor = editors.next()?;
                editors.next().is_none().then_some(editor)
            })
        } else {
            None
        };
        if let Some(editor) = editor.filter(|e| {
            crate::interaction_visible(world, *e) && crate::interaction::policy(world, *e).0
        }) {
            let consumed = crate::editor::scroll_editor_wheel(world, editor, remaining[1]);
            remaining[1] -= consumed;
            changed |= consumed != 0.0;
        }
        if let Some(rect) = world.get::<Rect>(entity).copied() {
            if let Some(scroll) = world.get_mut::<Scroll>(entity) {
                let before = scroll.offset;
                scroll.offset[0] += remaining[0];
                scroll.offset[1] += remaining[1];
                scroll.clamp_to(rect.w, rect.h);
                for axis in 0..2 {
                    remaining[axis] -= scroll.offset[axis] - before[axis];
                }
                changed |= scroll.offset != before;
            }
        }
        target = world.get::<Parent>(entity).map(|p| p.entity);
    }
    if changed {
        crate::focus::pause_focus_reveal(world);
        layout_propagate(world);
    }
    if let Some(input) = world.resource_mut::<Input>() {
        input.scroll = [0.0, 0.0];
    }
}

fn apply_pointer_drag(world: &mut World, input: &Input) {
    if !input.pointer.down {
        if let Some(entity) = world.resource::<ScrollDrag>().and_then(|d| d.entity) {
            crate::motion::scroll_release(world, entity);
        }
        clear_scroll_drag(world);
        return;
    }
    let position = input.pointer.position;
    if input.pointer.pressed {
        let entity = hit_test(world, position[0], position[1]).and_then(|hit| {
            if ancestor_with::<Splitter>(world, hit).is_some()
                || (input.pointer.button == crate::PointerButton::Left
                    && ancestor_with::<crate::Editor>(world, hit).is_some())
                || item_drag_claims(world, hit)
            {
                // Editors own left-drag text selection (and bounded internal
                // autoscroll), including inside retained markdown host slots.
                // An item-drag source owns its press the same way. Do not
                // simultaneously move their ancestor scrollport.
                None
            } else {
                ancestor_with::<Scroll>(world, hit)
            }
        });
        if let Some(entity) = entity {
            crate::motion::scroll_catch(world, entity);
        }
        world.insert_resource(ScrollDrag {
            entity,
            origin: position,
            last: position,
            active: false,
        });
        return;
    }
    let Some(mut drag) = world.resource::<ScrollDrag>().copied() else {
        return;
    };
    let Some(entity) = drag
        .entity
        .filter(|entity| world.is_alive(*entity) && world.has::<Scroll>(*entity))
    else {
        return;
    };
    if !drag.active {
        let dx = position[0] - drag.origin[0];
        let dy = position[1] - drag.origin[1];
        if dx * dx + dy * dy < SCROLL_DRAG_SLOP * SCROLL_DRAG_SLOP {
            drag.last = position;
            world.insert_resource(drag);
            return;
        }
        drag.active = true;
    }
    let delta = [position[0] - drag.last[0], position[1] - drag.last[1]];
    let physics = crate::motion::scroll_drag(world, entity, delta);
    if position != drag.last {
        if !physics {
            if let Some(scroll) = world.get_mut::<Scroll>(entity) {
                scroll.offset[0] += delta[0];
                scroll.offset[1] += delta[1];
            }
        }
        crate::focus::pause_focus_reveal(world);
        layout_propagate(world);
    }
    drag.last = position;
    world.insert_resource(drag);
}

fn item_drag_claims(world: &World, hit: Entity) -> bool {
    world
        .resource::<PointerDragOwner>()
        .is_some_and(|owner| (owner.0)(world, hit))
}

fn clear_scroll_drag(world: &mut World) {
    if let Some(drag) = world.resource_mut::<ScrollDrag>() {
        *drag = ScrollDrag::default();
    }
}

/// Roots in paint order: higher [`crate::overlay::Layer`] values last.
pub fn layered_roots(world: &World) -> Vec<Entity> {
    let mut roots = roots::<Rect>(world);
    roots.sort_by_key(|entity| crate::overlay::layer_of(world, *entity));
    roots
}

fn shown_children(world: &World, entity: Entity) -> Vec<Entity> {
    children_of(world, entity)
        .into_iter()
        .filter(|child| !world.has::<crate::tabs::Hidden>(*child))
        .collect()
}

fn measure(world: &mut World, entity: Entity) -> Rect {
    let layout = match world.get::<Layout>(entity).copied() {
        Some(layout) => layout,
        None => {
            return world.get::<Rect>(entity).copied().unwrap_or_default();
        }
    };
    let kids = shown_children(world, entity);
    let mut content_w = 0.0f32;
    let mut content_h = 0.0f32;
    let mut flow = 0usize;
    for child in &kids {
        let size = measure(world, *child);
        if !flex::in_flow(world, *child) {
            continue;
        }
        flow += 1;
        match layout.axis {
            Axis::Column => {
                content_w = content_w.max(size.w);
                content_h += size.h;
            }
            Axis::Row => {
                content_h = content_h.max(size.h);
                content_w += size.w;
            }
        }
    }
    if flow > 1 {
        let extra = layout.gap * (flow as f32 - 1.0);
        match layout.axis {
            Axis::Column => content_h += extra,
            Axis::Row => content_w += extra,
        }
    }
    if let Some(grid) = world.get::<Grid>(entity).cloned() {
        let flow: Vec<Entity> = kids
            .iter()
            .copied()
            .filter(|c| flex::in_flow(world, *c))
            .collect();
        (content_w, content_h) = measure_grid(world, &grid, &flow);
    }
    let [top, right, bottom, left] = flex::insets(world, entity, &layout);
    let (pad_w, pad_h) = (left + right, top + bottom);
    if world.has::<Scroll>(entity) {
        if let Some(scroll) = world.get_mut::<Scroll>(entity) {
            scroll.content = [content_w + pad_w, content_h + pad_h];
        }
    }
    let mut w = match layout.width {
        Size::Px(px) => px,
        Size::Hug => content_w + pad_w,
        Size::Fill => 0.0,
    };
    let mut h = match layout.height {
        Size::Px(px) => px,
        Size::Hug => content_h + pad_h,
        Size::Fill => 0.0,
    };
    if kids.is_empty() {
        if let Some((tw, th)) = label_hug(world, entity, &layout) {
            if matches!(layout.width, Size::Hug) {
                w = w.max(tw);
            }
            if matches!(layout.height, Size::Hug) {
                h = th;
            }
        }
    }
    let (w, h) = flex::constrain(world, entity, &layout, w, h);
    let rect = Rect {
        x: 0.0,
        y: 0.0,
        w,
        h,
    };
    world.insert(entity, rect);
    rect
}

// One top-down width / bottom-up height refinement, using the same Fill
// allocation rules as placement. This avoids a frame-late parent height and
// never asks a UI recipe to duplicate the layout engine. Each node is visited
// once; fixed sizes, Fill-height policy, min sizes and explicit colors are unchanged.
fn refine_wrapped_heights(world: &mut World, entity: Entity, available_w: f32, x: f32) -> Rect {
    let Some(layout) = world.get::<Layout>(entity).copied() else {
        return world.get::<Rect>(entity).copied().unwrap_or_default();
    };
    let mut rect = world.get::<Rect>(entity).copied().unwrap_or_default();
    let width = if layout.width == Size::Fill {
        available_w
    } else {
        rect.w
    };
    let scale = world
        .resource::<Viewport>()
        .map(|v| v.pixels_per_point())
        .unwrap_or(1.0);
    let width = flex::constrain(world, entity, &layout, width, rect.h).0;
    let (x, width) = snap_span(x, width, scale);
    rect.w = width;
    let kids = shown_children(world, entity);
    if kids.is_empty() {
        if layout.height == Size::Hug {
            let height = label_hug_at(world, entity, &layout, width).map_or(rect.h, |(_, h)| h);
            rect.h = flex::constrain(world, entity, &layout, width, height).1;
        }
        world.insert(entity, rect);
        return rect;
    }
    let [top, right, bottom, left] = flex::insets(world, entity, &layout);
    let (pad_w, pad_h) = (left + right, top + bottom);
    let inner_w = (width - pad_w).max(0.0);
    let (flow, absolute): (Vec<Entity>, Vec<Entity>) =
        kids.into_iter().partition(|c| flex::in_flow(world, *c));
    let wrap = flex::wraps(world, entity, &layout);
    let gaps = layout.gap * flow.len().saturating_sub(1) as f32;
    let mut fixed_w = gaps;
    let mut fills = 0;
    if layout.axis == Axis::Row && wrap.is_none() {
        for child in &flow {
            if world
                .get::<Layout>(*child)
                .is_some_and(|l| l.width == Size::Fill)
            {
                fills += 1;
            } else {
                fixed_w += world.get::<Rect>(*child).map(|r| r.w).unwrap_or(0.0);
            }
        }
    }
    let share = if fills == 0 {
        0.0
    } else {
        (inner_w - fixed_w).max(0.0) / fills as f32
    };
    let offset_x = crate::motion::scroll_offset(world, entity).map_or(0.0, |o| o[0]);
    let mut child_x = x + left + offset_x;
    let mut content_w = 0.0f32;
    let mut content_h = 0.0f32;
    if let Some(grid) = world.get::<Grid>(entity).cloned() {
        (content_w, content_h) = refine_grid(world, &grid, &flow, inner_w, child_x);
    } else {
        let mut sizes = Vec::with_capacity(flow.len());
        for child in flow {
            let child_layout = world.get::<Layout>(child).copied();
            let intrinsic_w = world.get::<Rect>(child).map(|r| r.w).unwrap_or(0.0);
            let stretch = layout.axis == Axis::Column
                && flex::self_items(world, entity, child) == Some(flex::Items::Stretch)
                && child_layout.is_some_and(|l| !matches!(l.width, Size::Px(_)));
            let child_width = if child_layout.is_some_and(|l| l.width == Size::Fill) {
                if layout.axis == Axis::Row && wrap.is_none() {
                    share
                } else {
                    inner_w
                }
            } else if stretch {
                inner_w
            } else {
                intrinsic_w
            };
            let size = refine_wrapped_heights(world, child, child_width, child_x);
            sizes.push(size);
            match layout.axis {
                Axis::Column => {
                    content_w = content_w.max(size.w);
                    content_h += size.h;
                }
                Axis::Row => {
                    content_w += size.w;
                    content_h = content_h.max(size.h);
                    if wrap.is_none() {
                        child_x += size.w + layout.gap;
                    }
                }
            }
        }
        if let Some(line_gap) = wrap {
            let widths: Vec<f32> = sizes.iter().map(|r| r.w).collect();
            let lines = flex::break_lines(&widths, layout.gap, inner_w);
            content_w = 0.0;
            content_h = line_gap * lines.len().saturating_sub(1) as f32;
            for line in lines {
                let n = line.len();
                let line_w: f32 = widths[line.clone()].iter().sum::<f32>()
                    + layout.gap * n.saturating_sub(1) as f32;
                content_w = content_w.max(line_w);
                content_h += sizes[line].iter().map(|r| r.h).fold(0.0, f32::max);
            }
        } else {
            match layout.axis {
                Axis::Column => content_h += gaps,
                Axis::Row => content_w += gaps,
            }
        }
    }
    for child in absolute {
        let pin = world
            .get::<flex::Absolute>(child)
            .copied()
            .unwrap_or_default();
        let intrinsic_w = world.get::<Rect>(child).map(|r| r.w).unwrap_or(0.0);
        let size = world
            .get::<Layout>(child)
            .map(|l| l.width)
            .unwrap_or(Size::Px(intrinsic_w));
        let (w, _) = flex::absolute_span(pin.left, pin.right, size, intrinsic_w, width);
        refine_wrapped_heights(world, child, w, x + pin.left.unwrap_or(left));
    }
    if layout.height == Size::Hug {
        rect.h = flex::constrain(world, entity, &layout, rect.w, content_h + pad_h).1;
    }
    if let Some(scroll) = world.get_mut::<Scroll>(entity) {
        scroll.content = [content_w + pad_w, content_h + pad_h];
    }
    world.insert(entity, rect);
    rect
}

/// Place `child` with its width and/or height temporarily treated as
/// [`Size::Fill`], so a stretched or pinned child takes the given size.
#[allow(clippy::too_many_arguments)]
fn place_filled(
    world: &mut World,
    child: Entity,
    fill: (bool, bool),
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    clip: Clip,
) -> (f32, f32) {
    let original = world.get::<Layout>(child).copied();
    if let (Some(mut layout), true) = (original, fill.0 || fill.1) {
        if fill.0 {
            layout.width = Size::Fill;
        }
        if fill.1 {
            layout.height = Size::Fill;
        }
        world.insert(child, layout);
        let used = place(world, child, x, y, w, h, clip);
        world.insert(child, original.unwrap());
        return used;
    }
    place(world, child, x, y, w, h, clip)
}

/// A stack child's box before constraints: Fill takes `fill_main` on the stack
/// axis and the inner size across it; other sizes keep the refined rect.
fn stack_box(
    world: &World,
    axis: Axis,
    child: Entity,
    fill_main: f32,
    inner: [f32; 2],
) -> (f32, f32) {
    let layout = world.get::<Layout>(child).copied();
    let rect = world.get::<Rect>(child).copied().unwrap_or_default();
    let w_fill = layout.is_some_and(|l| l.width == Size::Fill);
    let h_fill = layout.is_some_and(|l| l.height == Size::Fill);
    match axis {
        Axis::Column => (
            if w_fill { inner[0] } else { rect.w },
            if h_fill { fill_main } else { rect.h },
        ),
        Axis::Row => (
            if w_fill { fill_main } else { rect.w },
            if h_fill { inner[1] } else { rect.h },
        ),
    }
}

/// Stack-axis size of each Fill child (`None` for others). `leftover` is split
/// evenly; a child whose [`flex::MaxSize`] caps it below its share keeps the
/// capped size and the rest is shared again among the others.
fn resolve_fills(
    world: &World,
    axis: Axis,
    kids: &[Entity],
    leftover: f32,
    inner: [f32; 2],
) -> Vec<Option<f32>> {
    let is_fill = |c: &Entity| {
        world.get::<Layout>(*c).is_some_and(|l| match axis {
            Axis::Column => l.height == Size::Fill,
            Axis::Row => l.width == Size::Fill,
        })
    };
    let mut out: Vec<Option<f32>> = kids.iter().map(|c| is_fill(c).then_some(0.0)).collect();
    let mut frozen = vec![false; kids.len()];
    let mut remaining = leftover;
    loop {
        let open = out
            .iter()
            .zip(&frozen)
            .filter(|(o, f)| o.is_some() && !**f)
            .count();
        if open == 0 {
            break;
        }
        let share = remaining.max(0.0) / open as f32;
        let mut changed = false;
        for (i, child) in kids.iter().enumerate() {
            if out[i].is_none() || frozen[i] {
                continue;
            }
            let (w, h) = stack_box(world, axis, *child, share, inner);
            let (w, h) = child_size(world, *child, w, h);
            let main = match axis {
                Axis::Column => h,
                Axis::Row => w,
            };
            if main < share - 0.01 {
                out[i] = Some(main);
                frozen[i] = true;
                remaining -= main;
                changed = true;
            } else {
                out[i] = Some(share);
            }
        }
        if !changed {
            break;
        }
    }
    out
}

/// Child size inside a stack before placement: Fill resolved, then constrained.
fn child_size(world: &World, child: Entity, w: f32, h: f32) -> (f32, f32) {
    match world.get::<Layout>(child).copied() {
        Some(layout) => flex::constrain(world, child, &layout, w, h),
        None => (w, h),
    }
}

fn place(
    world: &mut World,
    entity: Entity,
    x: f32,
    y: f32,
    avail_w: f32,
    avail_h: f32,
    parent_clip: Clip,
) -> (f32, f32) {
    let Some(layout) = world.get::<Layout>(entity).copied() else {
        return (avail_w, avail_h);
    };
    let mut rect = world.get::<Rect>(entity).copied().unwrap_or_default();
    if matches!(layout.width, Size::Fill) {
        rect.w = avail_w;
    }
    if matches!(layout.height, Size::Fill) {
        rect.h = avail_h;
    }
    (rect.w, rect.h) = flex::constrain(world, entity, &layout, rect.w, rect.h);
    rect.x = x;
    rect.y = y;
    let scale = world
        .resource::<Viewport>()
        .map(|viewport| viewport.pixels_per_point())
        .unwrap_or(1.0);
    (rect.x, rect.w) = snap_span(rect.x, rect.w, scale);
    (rect.y, rect.h) = snap_span(rect.y, rect.h, scale);
    let mut stack_w = avail_w;
    let mut stack_h = avail_h;
    let kids = shown_children(world, entity);
    if kids.is_empty() {
        if let Some((tw, th)) = label_hug_at(world, entity, &layout, rect.w) {
            let hug_w = matches!(layout.width, Size::Hug);
            let hug_h = matches!(layout.height, Size::Hug);
            let (w, h) = flex::constrain(
                world,
                entity,
                &layout,
                if hug_w {
                    tw.max(layout.min_width)
                } else {
                    rect.w
                },
                if hug_h {
                    th.max(layout.min_height)
                } else {
                    rect.h
                },
            );
            if hug_w && (w - avail_w).abs() > 0.01 {
                rect.w = w;
                stack_w = w;
                (rect.x, rect.w) = snap_span(rect.x, rect.w, scale);
            }
            if hug_h && (h - avail_h).abs() > 0.01 {
                rect.h = h;
                stack_h = h;
                (rect.y, rect.h) = snap_span(rect.y, rect.h, scale);
            }
        }
    }
    world.insert(entity, rect);

    if world.has::<Scroll>(entity) {
        if let Some(scroll) = world.get_mut::<Scroll>(entity) {
            scroll.clamp_to(rect.w, rect.h);
        }
    }

    let clip = parent_clip.intersect(Clip::from_rect(rect));
    world.insert(entity, clip);

    if kids.is_empty() {
        return (stack_w, stack_h);
    }

    let [top, right, bottom, left] = flex::insets(world, entity, &layout);
    let inner_x = rect.x + left;
    let inner_y = rect.y + top;
    let inner_w = (rect.w - left - right).max(0.0);
    let inner_h = (rect.h - top - bottom).max(0.0);
    let offset = crate::motion::scroll_offset(world, entity).unwrap_or([0.0, 0.0]);
    let (flow, absolute): (Vec<Entity>, Vec<Entity>) =
        kids.into_iter().partition(|c| flex::in_flow(world, *c));
    let justify = world
        .get::<flex::Align>(entity)
        .map(|a| a.justify)
        .unwrap_or_default();

    if let Some(grid) = world.get::<Grid>(entity).cloned() {
        place_grid(
            world,
            entity,
            &grid,
            &flow,
            [inner_x + offset[0], inner_y + offset[1], inner_w],
            clip,
        );
    } else if let Some(line_gap) = flex::wraps(world, entity, &layout) {
        place_wrapped(
            world,
            entity,
            &flow,
            [inner_x + offset[0], inner_y + offset[1], inner_w],
            layout.gap,
            line_gap,
            justify,
            clip,
        );
    } else {
        place_stack(
            world,
            entity,
            &layout,
            &flow,
            [inner_x, inner_y, inner_w, inner_h],
            offset,
            justify,
            rect,
            clip,
        );
    }

    for child in absolute {
        let pin = world
            .get::<flex::Absolute>(child)
            .copied()
            .unwrap_or_default();
        let child_rect = world.get::<Rect>(child).copied().unwrap_or_default();
        let (size_w, size_h) = world
            .get::<Layout>(child)
            .map(|l| (l.width, l.height))
            .unwrap_or((Size::Px(child_rect.w), Size::Px(child_rect.h)));
        let (w, fill_w) = flex::absolute_span(pin.left, pin.right, size_w, child_rect.w, rect.w);
        let (h, fill_h) = flex::absolute_span(pin.top, pin.bottom, size_h, child_rect.h, rect.h);
        let (w, h) = child_size(world, child, w, h);
        let cx = flex::absolute_origin(pin.left, pin.right, w, rect.x, rect.w, inner_x) + offset[0];
        let cy = flex::absolute_origin(pin.top, pin.bottom, h, rect.y, rect.h, inner_y) + offset[1];
        place_filled(world, child, (fill_w, fill_h), cx, cy, w, h, clip);
    }
    (stack_w, stack_h)
}

#[allow(clippy::too_many_arguments)]
fn place_stack(
    world: &mut World,
    entity: Entity,
    layout: &Layout,
    kids: &[Entity],
    [inner_x, inner_y, inner_w, inner_h]: [f32; 4],
    offset: [f32; 2],
    justify: flex::Justify,
    rect: Rect,
    clip: Clip,
) {
    let mut fixed = 0.0f32;
    let mut fill_count = 0usize;
    for child in kids {
        let child_layout = world.get::<Layout>(*child).copied();
        let child_rect = world.get::<Rect>(*child).copied().unwrap_or_default();
        match layout.axis {
            Axis::Column => match child_layout.map(|l| l.height) {
                Some(Size::Fill) => fill_count += 1,
                _ => fixed += child_rect.h,
            },
            Axis::Row => match child_layout.map(|l| l.width) {
                Some(Size::Fill) => fill_count += 1,
                _ => fixed += child_rect.w,
            },
        }
    }
    if kids.len() > 1 {
        fixed += layout.gap * (kids.len() as f32 - 1.0);
    }
    let leftover = match layout.axis {
        Axis::Column => (inner_h - fixed).max(0.0),
        Axis::Row => (inner_w - fixed).max(0.0),
    };
    let fills = resolve_fills(world, layout.axis, kids, leftover, [inner_w, inner_h]);
    let unused = leftover - fills.iter().flatten().sum::<f32>();
    let (lead, extra) = if fill_count == 0 || unused > 0.01 {
        flex::distribute(justify, unused.max(0.0), kids.len())
    } else {
        (0.0, 0.0)
    };

    let mut cursor_x = inner_x + offset[0];
    let mut cursor_y = inner_y + offset[1];
    match layout.axis {
        Axis::Column => cursor_y += lead,
        Axis::Row => cursor_x += lead,
    }
    // A list's first pooled row is `floor(offset / row_height)`. The remainder
    // shifts that row up under the clip so a scroll can stop between rows.
    if let Some(list) = world.get::<crate::list::List>(entity).copied() {
        if list.row_height > 0.0 && list.offset.is_finite() {
            let rows = (list.offset / list.row_height).floor();
            cursor_y -= list.offset - rows * list.row_height;
        }
    }
    let mut main_delta = 0.0f32;
    for (&child, fill_main) in kids.iter().zip(&fills) {
        let child_layout = world.get::<Layout>(child).copied();
        let items = flex::self_items(world, entity, child);
        let (w, h) = stack_box(
            world,
            layout.axis,
            child,
            fill_main.unwrap_or(0.0),
            [inner_w, inner_h],
        );
        let (mut cw, mut ch) = child_size(world, child, w, h);
        let mut fill = (false, false);
        if items == Some(flex::Items::Stretch) {
            if let Some(l) = child_layout {
                match layout.axis {
                    Axis::Column if !matches!(l.width, Size::Px(_)) => {
                        cw = child_size(world, child, inner_w, ch).0;
                        fill.0 = true;
                    }
                    Axis::Row if !matches!(l.height, Size::Px(_)) => {
                        ch = child_size(world, child, cw, inner_h).1;
                        fill.1 = true;
                    }
                    _ => {}
                }
            }
        }
        let (mut place_x, mut place_y) = (cursor_x, cursor_y);
        match items {
            Some(items) => match layout.axis {
                Axis::Row => place_y += flex::cross_offset(items, inner_h, ch),
                Axis::Column => place_x += flex::cross_offset(items, inner_w, cw),
            },
            // An image in a row is centered on the cross axis. A 16px icon in a
            // 32px row then sits below a rounded selection instead of on its corner.
            None if world.has::<crate::image_view::ImageView>(child) => match layout.axis {
                Axis::Row => place_y += (inner_h - ch).max(0.0) * 0.5,
                Axis::Column => place_x += (inner_w - cw).max(0.0) * 0.5,
            },
            None => {}
        }
        let (used_w, used_h) = place_filled(world, child, fill, place_x, place_y, cw, ch, clip);
        match layout.axis {
            Axis::Column => {
                main_delta += used_h - ch;
                cursor_y += used_h + layout.gap + extra;
            }
            Axis::Row => {
                main_delta += used_w - cw;
                cursor_x += used_w + layout.gap + extra;
            }
        }
    }
    if main_delta.abs() > 0.01 {
        if let Some(scroll) = world.get_mut::<Scroll>(entity) {
            match layout.axis {
                Axis::Column => scroll.content[1] += main_delta,
                Axis::Row => scroll.content[0] += main_delta,
            }
            scroll.clamp_to(rect.w, rect.h);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn place_wrapped(
    world: &mut World,
    entity: Entity,
    kids: &[Entity],
    [origin_x, origin_y, inner_w]: [f32; 3],
    gap: f32,
    line_gap: f32,
    justify: flex::Justify,
    clip: Clip,
) {
    let sizes: Vec<(f32, f32)> = kids
        .iter()
        .map(|&child| {
            let child_layout = world.get::<Layout>(child).copied();
            let r = world.get::<Rect>(child).copied().unwrap_or_default();
            let w = match child_layout.map(|l| l.width) {
                Some(Size::Fill) => inner_w,
                _ => r.w,
            };
            child_size(world, child, w, r.h)
        })
        .collect();
    let widths: Vec<f32> = sizes.iter().map(|s| s.0).collect();
    let mut cursor_y = origin_y;
    for line in flex::break_lines(&widths, gap, inner_w) {
        let n = line.len();
        let line_w = widths[line.clone()].iter().sum::<f32>() + gap * n.saturating_sub(1) as f32;
        let line_h = sizes[line.clone()].iter().map(|s| s.1).fold(0.0, f32::max);
        let (lead, extra) = flex::distribute(justify, inner_w - line_w, n);
        let mut cursor_x = origin_x + lead;
        for i in line {
            let child = kids[i];
            let (cw, mut ch) = sizes[i];
            let items = flex::self_items(world, entity, child).unwrap_or_default();
            let stretch = items == flex::Items::Stretch
                && world
                    .get::<Layout>(child)
                    .is_some_and(|l| !matches!(l.height, Size::Px(_)));
            if stretch {
                ch = line_h;
            }
            let y = cursor_y + flex::cross_offset(items, line_h, ch);
            let fill_w = world
                .get::<Layout>(child)
                .is_some_and(|l| l.width == Size::Fill);
            place_filled(world, child, (fill_w, stretch), cursor_x, y, cw, ch, clip);
            cursor_x += cw + gap + extra;
        }
        cursor_y += line_h + line_gap;
    }
}

fn grid_widths(world: &World, kids: &[Entity]) -> Vec<Option<f32>> {
    kids.iter()
        .map(|c| {
            let fill = world
                .get::<Layout>(*c)
                .is_some_and(|l| l.width == Size::Fill);
            (!fill).then(|| world.get::<Rect>(*c).map_or(0.0, |r| r.w))
        })
        .collect()
}

fn grid_heights(world: &World, kids: &[Entity]) -> Vec<Option<f32>> {
    kids.iter()
        .map(|c| {
            let fill = world
                .get::<Layout>(*c)
                .is_some_and(|l| l.height == Size::Fill);
            (!fill).then(|| world.get::<Rect>(*c).map_or(0.0, |r| r.h))
        })
        .collect()
}

fn measure_grid(world: &World, grid: &Grid, kids: &[Entity]) -> (f32, f32) {
    let (cells, rows) = grid::auto_place(world, grid::tracks(grid).len(), kids);
    let cols = grid::columns(grid, &cells, &grid_widths(world, kids), None);
    let rows = grid::rows(&cells, &grid_heights(world, kids), rows);
    (
        grid::total(&cols, grid.column_gap),
        grid::total(&rows, grid.row_gap),
    )
}

fn refine_grid(
    world: &mut World,
    grid: &Grid,
    kids: &[Entity],
    inner_w: f32,
    x: f32,
) -> (f32, f32) {
    let (cells, rows) = grid::auto_place(world, grid::tracks(grid).len(), kids);
    let widths = grid_widths(world, kids);
    let cols = grid::columns(grid, &cells, &widths, Some(inner_w));
    let xs = grid::starts(&cols, grid.column_gap);
    for ((child, cell), width) in kids.iter().zip(&cells).zip(&widths) {
        let w = width.unwrap_or_else(|| grid::span_width(&cols, *cell, grid.column_gap));
        refine_wrapped_heights(world, *child, w, x + xs[cell.col]);
    }
    let rows = grid::rows(&cells, &grid_heights(world, kids), rows);
    (
        grid::total(&cols, grid.column_gap),
        grid::total(&rows, grid.row_gap),
    )
}

fn place_grid(
    world: &mut World,
    entity: Entity,
    grid: &Grid,
    kids: &[Entity],
    [origin_x, origin_y, inner_w]: [f32; 3],
    clip: Clip,
) {
    let (cells, rows) = grid::auto_place(world, grid::tracks(grid).len(), kids);
    let widths = grid_widths(world, kids);
    let heights = grid_heights(world, kids);
    let cols = grid::columns(grid, &cells, &widths, Some(inner_w));
    let rows = grid::rows(&cells, &heights, rows);
    let xs = grid::starts(&cols, grid.column_gap);
    let ys = grid::starts(&rows, grid.row_gap);
    for (i, (&child, cell)) in kids.iter().zip(&cells).enumerate() {
        let row_h = rows[cell.row];
        let items = flex::self_items(world, entity, child).unwrap_or_default();
        let stretch = items == flex::Items::Stretch
            && heights[i].is_some()
            && world
                .get::<Layout>(child)
                .is_some_and(|l| !matches!(l.height, Size::Px(_)));
        let w = widths[i].unwrap_or_else(|| grid::span_width(&cols, *cell, grid.column_gap));
        let h = match heights[i] {
            Some(h) if !stretch => h,
            _ => row_h,
        };
        let (w, h) = child_size(world, child, w, h);
        let y = origin_y + ys[cell.row] + flex::cross_offset(items, row_h, h);
        place_filled(
            world,
            child,
            (false, stretch),
            origin_x + xs[cell.col],
            y,
            w,
            h,
            clip,
        );
    }
}

fn hidden_in_tree(world: &World, entity: Entity) -> bool {
    let mut at = Some(entity);
    while let Some(e) = at {
        if world.has::<crate::tabs::Hidden>(e) {
            return true;
        }
        at = world.get::<Parent>(e).map(|p| p.entity);
    }
    false
}

fn depth(world: &World, entity: Entity) -> usize {
    let mut n = 0;
    let mut at = world.get::<Parent>(entity).map(|p| p.entity);
    while let Some(e) = at {
        n += 1;
        at = world.get::<Parent>(e).map(|p| p.entity);
    }
    n
}

/// Nearest ancestor, starting at `from`, whose [`Scroll`] can move vertically.
fn vertical_port(world: &World, from: Entity) -> Option<Entity> {
    let mut at = Some(from);
    while let Some(e) = at {
        if let (Some(scroll), Some(rect)) = (world.get::<Scroll>(e), world.get::<Rect>(e)) {
            if scroll.content[1] > rect.h + 0.5 || scroll.offset[1] != 0.0 {
                return Some(e);
            }
        }
        at = world.get::<Parent>(e).map(|p| p.entity);
    }
    None
}

/// Move [`flex::Sticky`] entities after placement, outermost first, and lay
/// their subtrees out again at the pinned position.
fn apply_sticky(world: &mut World) {
    let mut pinned: Vec<(usize, Entity, flex::Sticky)> = world
        .query::<flex::Sticky>()
        .into_iter()
        .map(|(e, s)| (0, e, *s))
        .collect();
    if pinned.is_empty() {
        return;
    }
    for p in &mut pinned {
        p.0 = depth(world, p.1);
    }
    pinned.sort_by_key(|p| p.0);
    for (_, entity, pin) in pinned {
        let Some(parent) = world.get::<Parent>(entity).map(|p| p.entity) else {
            continue;
        };
        if hidden_in_tree(world, entity) {
            continue;
        }
        let (Some(rect), Some(port)) = (
            world.get::<Rect>(entity).copied(),
            vertical_port(world, parent),
        ) else {
            continue;
        };
        let port_rect = world.get::<Rect>(port).copied().unwrap_or_default();
        let parent_rect = world.get::<Rect>(parent).copied().unwrap_or_default();
        let [top, _, bottom, _] = world
            .get::<Layout>(parent)
            .copied()
            .map_or([0.0; 4], |l| flex::insets(world, parent, &l));
        let (y0, y1) = match world.get::<Scroll>(parent).filter(|_| parent == port) {
            Some(scroll) => {
                let shown = crate::motion::scroll_offset(world, parent).map_or(0.0, |o| o[1]);
                let origin = parent_rect.y + shown;
                (origin + top, origin + scroll.content[1] - bottom)
            }
            None => (parent_rect.y + top, parent_rect.y + parent_rect.h - bottom),
        };
        let mut y = rect.y;
        if let Some(inset) = pin.top {
            y = y.max(port_rect.y + inset);
        }
        if let Some(inset) = pin.bottom {
            y = y.min(port_rect.y + port_rect.h - inset - rect.h);
        }
        let y = y.clamp(y0.min(rect.y), (y1 - rect.h).max(rect.y));
        if (y - rect.y).abs() < 0.01 {
            continue;
        }
        let clip = world.get::<Clip>(parent).copied().unwrap_or(Clip {
            x: f32::MIN / 4.0,
            y: f32::MIN / 4.0,
            w: f32::MAX / 2.0,
            h: f32::MAX / 2.0,
        });
        place(world, entity, rect.x, y, rect.w, rect.h, clip);
    }
}

fn snap_span(origin: f32, size: f32, scale: f32) -> (f32, f32) {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let start = (origin * scale).round() / scale;
    let end = ((origin + size) * scale).round() / scale;
    (start, (end - start).max(0.0))
}

/// Hug size of a label leaf: one line wide, or wrapped to the width already placed.
fn label_hug(world: &World, entity: Entity, layout: &Layout) -> Option<(f32, f32)> {
    let prev_w = world.get::<Rect>(entity).map(|rect| rect.w).unwrap_or(0.0);
    let field_w = match layout.width {
        Size::Px(px) => px,
        Size::Fill if prev_w > 0.0 => prev_w,
        Size::Fill => return label_hug_width(world, entity, layout, f32::INFINITY),
        Size::Hug => hug_wrap_width(world, entity, layout),
    };
    label_hug_width(world, entity, layout, field_w)
}

/// Hug a leaf against `field_w`, the width place is committing.
fn label_hug_at(
    world: &World,
    entity: Entity,
    layout: &Layout,
    field_w: f32,
) -> Option<(f32, f32)> {
    let wrap_w = match layout.width {
        Size::Px(px) => px,
        Size::Hug => hug_wrap_width(world, entity, layout),
        Size::Fill => field_w,
    };
    label_hug_width(world, entity, layout, wrap_w)
}

/// Wrap width for a Hug-width label: its [`flex::MaxSize`] width, else unbounded.
fn hug_wrap_width(world: &World, entity: Entity, layout: &Layout) -> f32 {
    world
        .get::<flex::MaxSize>(entity)
        .map_or(f32::INFINITY, |max| max.width.max(layout.min_width))
}

fn label_hug_width(
    world: &World,
    entity: Entity,
    layout: &Layout,
    field_w: f32,
) -> Option<(f32, f32)> {
    if !matches!(layout.width, Size::Hug) && !matches!(layout.height, Size::Hug) {
        return None;
    }
    world.get::<Label>(entity)?;
    let style = style_for(world, entity);
    let engine = text_engine(world);
    let metrics = label_box(world, entity);
    let max_width = if field_w.is_finite() {
        metrics.text_width(field_w, engine.unit_advance(&style))
    } else {
        f32::INFINITY
    };
    let (mut lines, width) = hug_lines(world, entity, max_width)?;
    if let Some(prompt) = world.get::<crate::prompt::Prompt>(entity) {
        lines = lines.min(crate::prompt::line_cap(prompt));
    }
    let line_h = engine.line_height(&style);
    let h = (2.0 * metrics.inset_y + lines as f32 * line_h).max(metrics.min_height);
    let w = width + 2.0 * metrics.inset_x;
    Some((w, h))
}

fn hug_lines(world: &World, entity: Entity, max_width: f32) -> Option<(usize, f32)> {
    if world.get::<Label>(entity)?.overflow == TextOverflow::Ellipsis {
        let style = style_for(world, entity);
        let engine = text_engine(world);
        let mut text = world.get::<Label>(entity)?.text.clone();
        if text.contains('\n') {
            text = text.replace('\n', " ");
        }
        let spans = world
            .get::<crate::font::Spans>(entity)
            .map(|spans| spans.runs.clone())
            .unwrap_or_default();
        let measured = engine.measure(&text, &style, &spans, f32::INFINITY);
        let width = if max_width.is_finite() {
            measured.width.min(max_width)
        } else {
            measured.width
        };
        return Some((1, width));
    }
    let font = text_face(world);
    let key = {
        let label = world.get::<Label>(entity)?;
        let preview = world
            .get::<crate::field::ImePreview>(entity)
            .map(|preview| preview.text.as_str())
            .unwrap_or("");
        let at = world
            .get::<crate::caret::Caret>(entity)
            .map(|caret| caret.index.min(label.text.len()))
            .unwrap_or(label.text.len());
        let spans = world
            .get::<crate::font::Spans>(entity)
            .map(|spans| spans.runs.as_slice())
            .unwrap_or(&[]);
        layout_cache_key(font.as_ref(), &label.text, max_width, spans, preview, at)
    };
    if let Some(layout) = world.get::<TextLayout>(entity) {
        if layout.cache_key == key {
            return Some((layout.lines.len().max(1), layout.width));
        }
    }
    let mut text = world.get::<Label>(entity)?.text.clone();
    if text.is_empty() {
        text = crate::prompt::placeholder_text(world, entity).unwrap_or(text);
    }
    if let Some(preview) = world.get::<crate::field::ImePreview>(entity) {
        if !preview.text.is_empty() {
            let at = world
                .get::<crate::caret::Caret>(entity)
                .map(|caret| caret.index.min(text.len()))
                .unwrap_or(text.len());
            let at = at.min(text.len());
            if text.is_char_boundary(at) {
                text.insert_str(at, &preview.text);
            }
        }
    }
    let spans = world
        .get::<crate::font::Spans>(entity)
        .map(|spans| spans.runs.clone())
        .unwrap_or_default();
    let style = style_for(world, entity);
    let measured = text_engine(world).measure(&text, &style, &spans, max_width);
    Some((measured.line_count.max(1), measured.width))
}

fn popup_depth(world: &World, root: Entity) -> usize {
    let mut depth = 0;
    let mut current = Some(root);
    while let Some(e) = current.filter(|e| world.has::<crate::AnchoredPopup>(*e)) {
        depth += 1;
        if depth >= 64 {
            break;
        }
        current = crate::popup::parent_popup(world, e);
    }
    depth
}

/// Walk up [`Parent`] links to the nearest ancestor (or self) that has `T`.
pub fn ancestor_with<T: crate::world::Component>(
    world: &World,
    mut entity: Entity,
) -> Option<Entity> {
    loop {
        if !world.is_alive(entity) {
            return None;
        }
        if world.has::<T>(entity) {
            return Some(entity);
        }
        entity = world.get::<Parent>(entity)?.entity;
    }
}
