use crate::entity::Entity;
use crate::hierarchy::{children_of, roots, Parent};
use crate::input::Viewport;
use crate::world::{Component, World};

impl Component for Layout {}
impl Component for Rect {}

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

/// Placed rectangle in physical pixels, origin top-left.
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

/// Parent-before-child placement after a bottom-up hug measure.
pub fn layout_propagate(world: &mut World) {
    let viewport = world.resource::<Viewport>().copied().unwrap_or_default();
    let root_ids: Vec<Entity> = roots::<Layout>(world);
    for root in root_ids {
        measure(world, root);
    }
    for root in roots::<Layout>(world) {
        place(world, root, 0.0, 0.0, viewport.width, viewport.height);
    }
}

/// Topmost (deepest, then later sibling) entity whose rect contains the point.
pub fn hit_test(world: &World, x: f32, y: f32) -> Option<Entity> {
    let mut hit = None;
    for root in roots::<Rect>(world) {
        walk_paint_order(world, root, &mut |entity| {
            if let Some(rect) = world.get::<Rect>(entity) {
                if rect.contains(x, y) {
                    hit = Some(entity);
                }
            }
        });
    }
    hit
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
    let mut child_sizes = Vec::new();
    for child in &kids {
        let size = measure(world, *child);
        child_sizes.push(size);
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
    let _ = child_sizes;
    rect
}

fn place(world: &mut World, entity: Entity, x: f32, y: f32, avail_w: f32, avail_h: f32) {
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

    let kids = children_of(world, entity);
    if kids.is_empty() {
        return;
    }

    let inner_x = rect.x + layout.padding;
    let inner_y = rect.y + layout.padding;
    let inner_w = (rect.w - layout.padding * 2.0).max(0.0);
    let inner_h = (rect.h - layout.padding * 2.0).max(0.0);

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

    let mut cursor_x = inner_x;
    let mut cursor_y = inner_y;
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
        place(world, child, cursor_x, cursor_y, cw, ch);
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
