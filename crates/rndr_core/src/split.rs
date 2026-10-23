use crate::entity::Entity;
use crate::hierarchy::{children_of, Parent};
use crate::input::Input;
use crate::layout::{ancestor_with, hit_test, Axis, Layout, Rect, Size};
use crate::world::{Component, World};

impl Component for Splitter {}

/// Drag handle between two siblings. Not [`crate::Focusable`].
///
/// Defaults are 120 points of leading pane and 160 of trailing pane.
/// [`apply_split`] writes the leading sibling's size and then clamps it.
#[derive(Clone, Copy, Debug)]
pub struct Splitter {
    /// Minimum size of the previous sibling, in logical points.
    pub min_leading: f32,
    /// Minimum size of the next sibling, in logical points.
    pub min_trailing: f32,
}

impl Default for Splitter {
    fn default() -> Self {
        Self {
            min_leading: 120.0,
            min_trailing: 160.0,
        }
    }
}

/// One captured splitter drag. Cleared when the pointer is released.
///
/// [`crate::Shell`] inserts this resource. [`apply_split`] ignores the
/// pointer when it is absent and still reclamps.
#[derive(Clone, Copy, Debug, Default)]
pub struct Drag {
    /// Handle being dragged. `None` while the pointer is up.
    pub entity: Option<Entity>,
    /// Pointer position from the previous move, in logical points.
    pub last: [f32; 2],
}

/// Resize the sibling before a [`Splitter`] from pointer drag, then reclamp.
///
/// A press on the handle, or on a descendant of it, stores that entity in
/// [`Drag`] and clears the press. Movement along the parent [`Axis`] is
/// added to the leading sibling: [`Layout`] width on a row, height on a
/// column, stored as [`Size::Px`]. The handle has to sit after that sibling.
/// Release clears [`Drag::entity`].
///
/// Every handle is then clamped to at least [`Splitter::min_leading`]. The
/// trailing sibling keeps [`Splitter::min_trailing`] when the parent is
/// large enough. Gaps and padding come out of the parent's span. A root
/// whose size on that axis is [`Size::Fill`] uses the viewport content
/// span. When the parent is smaller than both minimums, the leading side
/// stays at `min_leading`. A missing [`crate::Input`] only reclamps.
pub fn apply_split(world: &mut World) {
    let Some(input) = world.resource::<Input>().cloned() else {
        reclamp_all(world);
        return;
    };

    if !input.pointer.down {
        if let Some(drag) = world.resource_mut::<Drag>() {
            drag.entity = None;
        }
        reclamp_all(world);
        return;
    }

    if input.pointer.pressed {
        if let Some(hit) = hit_test(world, input.pointer.position[0], input.pointer.position[1]) {
            if let Some(splitter) = ancestor_with::<Splitter>(world, hit) {
                if let Some(drag) = world.resource_mut::<Drag>() {
                    drag.entity = Some(splitter);
                    drag.last = input.pointer.position;
                }
                if let Some(input) = world.resource_mut::<Input>() {
                    input.pointer.pressed = false;
                }
            }
        }
    }

    let drag = world.resource::<Drag>().copied();
    if let Some(drag) = drag {
        if let Some(entity) = drag.entity.filter(|entity| world.is_alive(*entity)) {
            apply_drag(world, entity, input.pointer.position, drag.last);
            if let Some(current) = world.resource_mut::<Drag>() {
                current.last = input.pointer.position;
            }
        }
    }

    reclamp_all(world);
}

fn apply_drag(world: &mut World, splitter: Entity, cursor: [f32; 2], last: [f32; 2]) {
    let Some(axis) = parent_axis(world, splitter) else {
        return;
    };
    let current = leading_size(world, splitter, axis);
    let delta = match axis {
        Axis::Row => cursor[0] - last[0],
        Axis::Column => cursor[1] - last[1],
    };
    write_leading(world, splitter, axis, current + delta);
}

fn reclamp_all(world: &mut World) {
    for splitter in world.entities_with::<Splitter>() {
        let Some(axis) = parent_axis(world, splitter) else {
            continue;
        };
        let current = leading_size(world, splitter, axis);
        write_leading(world, splitter, axis, current);
    }
}

fn write_leading(world: &mut World, splitter: Entity, axis: Axis, desired: f32) {
    let Some(leading) = leading_of(world, splitter) else {
        return;
    };
    let clamped = clamp_leading(world, splitter, axis, desired);
    if let Some(layout) = world.get_mut::<Layout>(leading) {
        match axis {
            Axis::Row => layout.width = Size::Px(clamped),
            Axis::Column => layout.height = Size::Px(clamped),
        }
    }
}

fn clamp_leading(world: &World, splitter: Entity, axis: Axis, desired: f32) -> f32 {
    let spec = world.get::<Splitter>(splitter).copied().unwrap_or_default();
    let Some(parent) = world.get::<Parent>(splitter).map(|parent| parent.entity) else {
        return spec.min_leading;
    };
    let Some(parent_layout) = world.get::<Layout>(parent).copied() else {
        return spec.min_leading.max(desired);
    };
    let Some(span) = parent_span(world, parent, axis) else {
        return spec.min_leading.max(desired);
    };
    let inner = (span - parent_layout.padding * 2.0).max(0.0);
    let kids = children_of(world, parent);
    let gaps = if kids.len() > 1 {
        parent_layout.gap * (kids.len() as f32 - 1.0)
    } else {
        0.0
    };
    let leading = leading_of(world, splitter);
    let mut other = 0.0;
    for child in kids {
        if leading == Some(child) {
            continue;
        }
        let Some(rect) = world.get::<Rect>(child).copied() else {
            continue;
        };
        other += match axis {
            Axis::Row => rect.w,
            Axis::Column => rect.h,
        };
    }
    let trailing_len = trailing_of(world, splitter)
        .and_then(|entity| world.get::<Rect>(entity).copied())
        .map(|rect| match axis {
            Axis::Row => rect.w,
            Axis::Column => rect.h,
        })
        .unwrap_or(0.0);
    // Replace the trailing pane's current size with its minimum so leftover stays usable.
    let other_without_trailing = (other - trailing_len).max(0.0);
    let max = (inner - other_without_trailing - spec.min_trailing - gaps).max(spec.min_leading);
    desired.clamp(spec.min_leading, max)
}

fn parent_span(world: &World, parent: Entity, axis: Axis) -> Option<f32> {
    let layout = world.get::<Layout>(parent).copied()?;
    let fill = match axis {
        Axis::Row => matches!(layout.width, Size::Fill),
        Axis::Column => matches!(layout.height, Size::Fill),
    };
    if fill && world.get::<Parent>(parent).is_none() {
        let viewport = world
            .resource::<crate::input::Viewport>()
            .copied()
            .unwrap_or_default();
        return Some(match axis {
            Axis::Row => viewport.content_width(),
            Axis::Column => viewport.content_height(),
        });
    }
    world.get::<Rect>(parent).map(|rect| match axis {
        Axis::Row => rect.w,
        Axis::Column => rect.h,
    })
}

fn parent_axis(world: &World, splitter: Entity) -> Option<Axis> {
    let parent = world.get::<Parent>(splitter)?.entity;
    world.get::<Layout>(parent).map(|layout| layout.axis)
}

fn leading_of(world: &World, splitter: Entity) -> Option<Entity> {
    let parent = world.get::<Parent>(splitter)?.entity;
    let kids = children_of(world, parent);
    let i = kids.iter().position(|entity| *entity == splitter)?;
    if i == 0 {
        None
    } else {
        Some(kids[i - 1])
    }
}

fn trailing_of(world: &World, splitter: Entity) -> Option<Entity> {
    let parent = world.get::<Parent>(splitter)?.entity;
    let kids = children_of(world, parent);
    let i = kids.iter().position(|entity| *entity == splitter)?;
    kids.get(i + 1).copied()
}

fn leading_size(world: &World, splitter: Entity, axis: Axis) -> f32 {
    let Some(leading) = leading_of(world, splitter) else {
        return 0.0;
    };
    let layout = world.get::<Layout>(leading).copied();
    let rect = world.get::<Rect>(leading).copied();
    match axis {
        Axis::Row => match layout.map(|layout| layout.width) {
            Some(Size::Px(px)) => px,
            _ => rect.map(|rect| rect.w).unwrap_or(0.0),
        },
        Axis::Column => match layout.map(|layout| layout.height) {
            Some(Size::Px(px)) => px,
            _ => rect.map(|rect| rect.h).unwrap_or(0.0),
        },
    }
}
