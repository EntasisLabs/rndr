use crate::entity::Entity;
use crate::hierarchy::{children_of, roots, Parent};
use crate::input::{Input, Viewport};
use crate::split::Splitter;
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
}

impl Layout {
    pub fn column() -> Self {
        Self {
            axis: Axis::Column,
            gap: 0.0,
            padding: 0.0,
            width: Size::Hug,
            height: Size::Hug,
        }
    }

    pub fn row() -> Self {
        Self {
            axis: Axis::Row,
            gap: 0.0,
            padding: 0.0,
            width: Size::Hug,
            height: Size::Hug,
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
    for root in roots::<Rect>(world) {
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

fn walk_paint_order(world: &World, entity: Entity, visit: &mut impl FnMut(Entity)) {
    if !world.is_alive(entity) {
        return;
    }
    visit(entity);
    for child in children_of(world, entity) {
        walk_paint_order(world, child, visit);
    }
}

fn measure(world: &mut World, entity: Entity) -> Rect {
    let layout = match world.get::<Layout>(entity).copied() {
        Some(layout) => layout,
        None => {
            return world.get::<Rect>(entity).copied().unwrap_or_default();
        }
    };
    let kids = children_of(world, entity);
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
    let w = match layout.width {
        Size::Px(px) => px,
        Size::Hug => content_w + pad,
        Size::Fill => 0.0,
    };
    let h = match layout.height {
        Size::Px(px) => px,
        Size::Hug => content_h + pad,
        Size::Fill => 0.0,
    };
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
) {
    let Some(layout) = world.get::<Layout>(entity).copied() else {
        return;
    };
    let mut rect = world.get::<Rect>(entity).copied().unwrap_or_default();
    if matches!(layout.width, Size::Fill) {
        rect.w = avail_w;
    }
    if matches!(layout.height, Size::Fill) {
        rect.h = avail_h;
    }
    rect.x = x;
    rect.y = y;
    world.insert(entity, rect);

    if world.has::<Scroll>(entity) {
        if let Some(scroll) = world.get_mut::<Scroll>(entity) {
            scroll.clamp_to(rect.w, rect.h);
        }
    }

    let clip = parent_clip.intersect(Clip::from_rect(rect));
    world.insert(entity, clip);

    let kids = children_of(world, entity);
    if kids.is_empty() {
        return;
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
                (w, h)
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
                (w, h)
            }
        };
        place(world, child, cursor_x, cursor_y, cw, ch, clip);
        match layout.axis {
            Axis::Column => cursor_y += ch + layout.gap,
            Axis::Row => cursor_x += cw + layout.gap,
        }
    }
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
        match world.get::<Parent>(entity) {
            Some(parent) => entity = parent.entity,
            None => return None,
        }
    }
}
