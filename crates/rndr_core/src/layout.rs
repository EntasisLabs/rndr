use crate::entity::Entity;
use crate::font::{field_text_width, layout_cache_key, text_face, TextLayout};
use crate::hierarchy::{children_of, roots, Parent};
use crate::input::{Input, Viewport};
use crate::split::Splitter;
use crate::style::{Label, TEXT_ORIGIN_X, TEXT_ORIGIN_Y};
use crate::text::{style_of, text_engine};
use crate::world::{Component, World};

impl Component for Layout {}
impl Component for Rect {}
impl Component for Clip {}
impl Component for Scroll {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Row,
    Column,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Size {
    Px(f32),
    Hug,
    Fill,
}

/// Intent for how a node sizes and stacks its children.
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub axis: Axis,
    pub gap: f32,
    pub padding: f32,
    pub width: Size,
    pub height: Size,
    /// Floor on the measured or filled width, in points.
    pub min_width: f32,
    /// Floor on the measured or filled height, in points.
    pub min_height: f32,
}

impl Layout {
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
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn contains(self, x: f32, y: f32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
}

/// Visible rectangle after ancestor intersection. Origin top-left.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Clip {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Clip {
    pub fn contains(self, x: f32, y: f32) -> bool {
        self.w > 0.0
            && self.h > 0.0
            && x >= self.x
            && y >= self.y
            && x < self.x + self.w
            && y < self.y + self.h
    }

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
    pub offset: [f32; 2],
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

/// One captured scroll drag. Cleared when the pointer is released.
/// A press that starts on a [`Splitter`] does not capture.
#[derive(Clone, Copy, Debug, Default)]
pub struct ScrollDrag {
    pub entity: Option<Entity>,
    pub origin: [f32; 2],
    pub last: [f32; 2],
    pub active: bool,
}

impl Scroll {
    pub fn clamp_to(&mut self, port_w: f32, port_h: f32) {
        let min_x = (port_w - self.content[0]).min(0.0);
        let min_y = (port_h - self.content[1]).min(0.0);
        self.offset[0] = self.offset[0].clamp(min_x, 0.0);
        self.offset[1] = self.offset[1].clamp(min_y, 0.0);
    }
}

/// Parent-before-child placement after a bottom-up hug measure.
pub fn layout_propagate(world: &mut World) {
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
    for root in roots::<Layout>(world) {
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
}

/// Topmost (deepest, then later sibling) entity whose visible clip contains the point.
pub fn hit_test(world: &World, x: f32, y: f32) -> Option<Entity> {
    let mut hit = None;
    let floor = crate::overlay::modal_floor(world);
    for root in layered_roots(world) {
        if floor.is_some_and(|floor| crate::overlay::layer_of(world, root) < floor) {
            continue;
        }
        walk_paint_order(world, root, &mut |entity| {
            if let Some(clip) = world.get::<Clip>(entity).copied() {
                if clip.contains(x, y) {
                    hit = Some(entity);
                }
            } else if let Some(rect) = world.get::<Rect>(entity).copied() {
                if rect.contains(x, y) {
                    hit = Some(entity);
                }
            }
        });
    }
    hit
}

/// Apply the wheel, then a held contact.
/// The wheel goes to the deepest [`Scroll`] under the pointer.
/// A press that is not on a [`Splitter`] captures that scrollport. Once the
/// contact moves [`SCROLL_DRAG_SLOP`] points, further movement is added to its
/// offset. The press itself is left for focus.
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
    let target = hit_test(world, input.pointer.position[0], input.pointer.position[1])
        .and_then(|hit| ancestor_with::<Scroll>(world, hit))
        .or_else(|| roots::<Scroll>(world).into_iter().next());
    if let Some(entity) = target {
        if let Some(scroll) = world.get_mut::<Scroll>(entity) {
            scroll.offset[0] += input.scroll[0];
            scroll.offset[1] += input.scroll[1];
        }
    }
    if let Some(input) = world.resource_mut::<Input>() {
        input.scroll = [0.0, 0.0];
    }
}

fn apply_pointer_drag(world: &mut World, input: &Input) {
    if !input.pointer.down {
        clear_scroll_drag(world);
        return;
    }
    let position = input.pointer.position;
    if input.pointer.pressed {
        let entity = hit_test(world, position[0], position[1]).and_then(|hit| {
            if ancestor_with::<Splitter>(world, hit).is_some() {
                None
            } else {
                ancestor_with::<Scroll>(world, hit)
            }
        });
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
    if let Some(scroll) = world.get_mut::<Scroll>(entity) {
        scroll.offset[0] += position[0] - drag.last[0];
        scroll.offset[1] += position[1] - drag.last[1];
    }
    drag.last = position;
    world.insert_resource(drag);
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

fn walk_paint_order(world: &World, entity: Entity, visit: &mut impl FnMut(Entity)) {
    if !world.is_alive(entity) || world.has::<crate::tabs::Hidden>(entity) {
        return;
    }
    visit(entity);
    for child in children_of(world, entity) {
        walk_paint_order(world, child, visit);
    }
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
    for child in &kids {
        let size = measure(world, *child);
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
    if kids.len() > 1 {
        let extra = layout.gap * (kids.len() as f32 - 1.0);
        match layout.axis {
            Axis::Column => content_h += extra,
            Axis::Row => content_w += extra,
        }
    }
    let pad = layout.padding * 2.0;
    if world.has::<Scroll>(entity) {
        if let Some(scroll) = world.get_mut::<Scroll>(entity) {
            scroll.content = [content_w + pad, content_h + pad];
        }
    }
    let mut w = match layout.width {
        Size::Px(px) => px,
        Size::Hug => content_w + pad,
        Size::Fill => 0.0,
    };
    let mut h = match layout.height {
        Size::Px(px) => px,
        Size::Hug => content_h + pad,
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
    w = w.max(layout.min_width);
    h = h.max(layout.min_height);
    let rect = Rect {
        x: 0.0,
        y: 0.0,
        w,
        h,
    };
    world.insert(entity, rect);
    rect
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
    rect.w = rect.w.max(layout.min_width);
    rect.h = rect.h.max(layout.min_height);
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
            if matches!(layout.width, Size::Hug) {
                let w = tw.max(layout.min_width);
                if (w - avail_w).abs() > 0.01 {
                    rect.w = w;
                    stack_w = w;
                    (rect.x, rect.w) = snap_span(rect.x, rect.w, scale);
                }
            }
            if matches!(layout.height, Size::Hug) {
                let h = th.max(layout.min_height);
                if (h - avail_h).abs() > 0.01 {
                    rect.h = h;
                    stack_h = h;
                    (rect.y, rect.h) = snap_span(rect.y, rect.h, scale);
                }
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

    let inner_x = rect.x + layout.padding;
    let inner_y = rect.y + layout.padding;
    let inner_w = (rect.w - layout.padding * 2.0).max(0.0);
    let inner_h = (rect.h - layout.padding * 2.0).max(0.0);
    let offset = world
        .get::<Scroll>(entity)
        .map(|s| s.offset)
        .unwrap_or([0.0, 0.0]);

    let mut fixed = 0.0f32;
    let mut fill_count = 0usize;
    for child in &kids {
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
    let fill_share = if fill_count > 0 {
        leftover / fill_count as f32
    } else {
        0.0
    };

    let mut cursor_x = inner_x + offset[0];
    let mut cursor_y = inner_y + offset[1];
    let mut main_delta = 0.0f32;
    for child in kids {
        let child_layout = world.get::<Layout>(child).copied();
        let child_rect = world.get::<Rect>(child).copied().unwrap_or_default();
        let (cw, ch) = match layout.axis {
            Axis::Column => {
                let w = match child_layout.map(|l| l.width) {
                    Some(Size::Fill) => inner_w,
                    _ => child_rect.w,
                };
                let h = match child_layout.map(|l| l.height) {
                    Some(Size::Fill) => fill_share,
                    _ => child_rect.h,
                };
                (
                    w.max(child_layout.map(|l| l.min_width).unwrap_or(0.0)),
                    h.max(child_layout.map(|l| l.min_height).unwrap_or(0.0)),
                )
            }
            Axis::Row => {
                let w = match child_layout.map(|l| l.width) {
                    Some(Size::Fill) => fill_share,
                    _ => child_rect.w,
                };
                let h = match child_layout.map(|l| l.height) {
                    Some(Size::Fill) => inner_h,
                    _ => child_rect.h,
                };
                (
                    w.max(child_layout.map(|l| l.min_width).unwrap_or(0.0)),
                    h.max(child_layout.map(|l| l.min_height).unwrap_or(0.0)),
                )
            }
        };
        let (used_w, used_h) = place(world, child, cursor_x, cursor_y, cw, ch, clip);
        match layout.axis {
            Axis::Column => {
                main_delta += used_h - ch;
                cursor_y += used_h + layout.gap;
            }
            Axis::Row => {
                main_delta += used_w - cw;
                cursor_x += used_w + layout.gap;
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
    (stack_w, stack_h)
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
        Size::Hug => f32::INFINITY,
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
        Size::Hug => f32::INFINITY,
        Size::Fill => field_w,
    };
    label_hug_width(world, entity, layout, wrap_w)
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
    let style = style_of(world);
    let engine = text_engine(world);
    let max_width = if field_w.is_finite() {
        field_text_width(field_w, engine.unit_advance(&style))
    } else {
        f32::INFINITY
    };
    let (lines, width) = hug_lines(world, entity, max_width)?;
    let line_h = engine.line_height(&style);
    let h = (TEXT_ORIGIN_Y + lines as f32 * line_h + TEXT_ORIGIN_Y).max(36.0);
    let w = width + 2.0 * TEXT_ORIGIN_X;
    Some((w, h))
}

fn hug_lines(world: &World, entity: Entity, max_width: f32) -> Option<(usize, f32)> {
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
    let style = style_of(world);
    let measured = text_engine(world).measure(&text, &style, &spans, max_width);
    Some((measured.line_count.max(1), measured.width))
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
