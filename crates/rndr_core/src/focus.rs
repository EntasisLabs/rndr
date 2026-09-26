use crate::caret::{is_editing, set_editing, set_editing_field, Field};
use crate::disclosure::{toggle_disclosure, Disclosure, Focusable};
use crate::entity::Entity;
use crate::fs::FsPath;
use crate::hierarchy::{children_of, roots, Parent};
use crate::input::{Focus, Input, Key};
use crate::layout::{ancestor_with, hit_test, Clip, Rect, Scroll};
use crate::pane::{set_selection, Selection};
use crate::style::Label;
use crate::world::World;

/// Click and keys update [`Focus`], toggle disclosures, and keep the row on screen.
pub fn apply_focus(world: &mut World) {
    snap_stale_focus(world);

    let Some(input) = world.resource::<Input>().cloned() else {
        return;
    };

    if input.pointer.pressed {
        if let Some(hit) = hit_test(world, input.pointer.position[0], input.pointer.position[1]) {
            if let Some(target) = ancestor_with::<Focusable>(world, hit) {
                set_focus(world, Some(target));
                set_selection(world, Some(target));
                set_editing(world, None);
            } else if let Some(field_entity) = ancestor_with::<Field>(world, hit) {
                if let Some(source) = world
                    .resource::<Selection>()
                    .and_then(|selection| selection.entity)
                    .filter(|entity| world.is_alive(*entity))
                {
                    let kind = world.get::<Field>(field_entity).unwrap().kind;
                    set_editing_field(world, source, kind);
                }
            }
        }
    }

    if let Some(key) = input.key {
        let leave_for_fs =
            key == Key::Save || (matches!(key, Key::New | Key::Delete) && !is_editing(world));
        if captures_keys(world) {
            // A text field or editor owns this key.
        } else if leave_for_fs {
            // Left for apply_fs_save / apply_fs_edit.
        } else if is_editing(world) {
            match key {
                Key::Escape => {
                    set_editing(world, None);
                    if let Some(input) = world.resource_mut::<Input>() {
                        input.key = None;
                    }
                }
                Key::New | Key::Delete => {
                    if let Some(input) = world.resource_mut::<Input>() {
                        input.key = None;
                    }
                }
                Key::Left
                | Key::Right
                | Key::Backspace
                | Key::Enter
                | Key::Tab
                | Key::Up
                | Key::Down
                | Key::Save
                | Key::Copy
                | Key::Cut
                | Key::Paste => {}
            }
        } else {
            handle_key(world, key, input.shift);
            if let Some(input) = world.resource_mut::<Input>() {
                input.key = None;
            }
        }
    }

    ensure_focus_visible(world);
}

fn captures_keys(world: &World) -> bool {
    world
        .resource::<Focus>()
        .and_then(|focus| focus.entity)
        .is_some_and(|entity| world.has::<crate::field::CapturesKeys>(entity))
}

fn handle_key(world: &mut World, key: Key, shift: bool) {
    match key {
        Key::Down => move_down(world),
        Key::Up => move_up(world),
        Key::Right => move_right(world),
        Key::Left => move_left(world),
        Key::Tab if shift => move_tab(world, true),
        Key::Tab => move_tab(world, false),
        Key::Enter => {
            if let Some(target) = current_focus(world) {
                set_selection(world, Some(target));
                set_editing(world, Some(target));
            }
        }
        Key::Escape => handle_escape(world),
        Key::Backspace | Key::Save | Key::New | Key::Delete | Key::Copy | Key::Cut | Key::Paste => {
        }
    }
}

fn handle_escape(world: &mut World) {
    if world
        .resource::<Selection>()
        .is_some_and(|selection| selection.entity.is_some())
    {
        set_selection(world, None);
        return;
    }
    let Some(target) = current_focus(world) else {
        if let Some(input) = world.resource_mut::<Input>() {
            input.quit = true;
        }
        return;
    };
    if world
        .get::<Disclosure>(target)
        .is_some_and(|d| d.open && can_expand(world, target, d))
    {
        toggle_disclosure(world, target);
        retarget_focus_after_toggle(world, target);
        return;
    }
    if let Some(parent) = parent_focusable(world, target) {
        set_focus(world, Some(parent));
        return;
    }
    if let Some(input) = world.resource_mut::<Input>() {
        input.quit = true;
    }
}

fn can_expand(world: &World, entity: Entity, disclosure: &Disclosure) -> bool {
    !disclosure.children.is_empty() || world.get::<FsPath>(entity).is_some_and(|path| path.dir)
}

fn move_down(world: &mut World) {
    let Some(current) = current_focus(world) else {
        set_focus(world, first_focusable(world));
        return;
    };
    if let Some(child) = focusable_children(world, current).into_iter().next() {
        set_focus(world, Some(child));
        return;
    }
    if let Some(next) = next_after_subtree(world, current) {
        set_focus(world, Some(next));
    }
}

fn move_up(world: &mut World) {
    let Some(current) = current_focus(world) else {
        set_focus(world, last_focusable(world));
        return;
    };
    if let Some(prev) = previous_sibling_focusable(world, current) {
        set_focus(world, Some(last_visible_descendant(world, prev)));
        return;
    }
    if let Some(parent) = parent_focusable(world, current) {
        set_focus(world, Some(parent));
    }
}

fn move_right(world: &mut World) {
    let Some(current) = current_focus(world) else {
        return;
    };
    if let Some(disclosure) = world.get::<Disclosure>(current).cloned() {
        if !disclosure.open && can_expand(world, current, &disclosure) {
            toggle_disclosure(world, current);
            return;
        }
        if disclosure.open {
            if let Some(child) = focusable_children(world, current).into_iter().next() {
                set_focus(world, Some(child));
            }
        }
    }
}

fn move_left(world: &mut World) {
    let Some(current) = current_focus(world) else {
        return;
    };
    if world.get::<Disclosure>(current).is_some_and(|d| d.open) {
        toggle_disclosure(world, current);
        retarget_focus_after_toggle(world, current);
        return;
    }
    if let Some(parent) = parent_focusable(world, current) {
        set_focus(world, Some(parent));
    }
}

fn move_tab(world: &mut World, reverse: bool) {
    let order = all_focusable(world);
    if order.is_empty() {
        return;
    }
    let next = match current_focus(world).and_then(|e| order.iter().position(|x| *x == e)) {
        Some(i) if reverse && i > 0 => order[i - 1],
        Some(i) if !reverse && i + 1 < order.len() => order[i + 1],
        None if reverse => *order.last().unwrap(),
        None => order[0],
        _ => return,
    };
    set_focus(world, Some(next));
}

fn snap_stale_focus(world: &mut World) {
    let Some(entity) = world.resource::<Focus>().and_then(|f| f.entity) else {
        return;
    };
    if world.is_alive(entity) && world.has::<Focusable>(entity) {
        return;
    }
    let snapped = if world.is_alive(entity) {
        ancestor_with::<Focusable>(world, entity)
    } else {
        None
    };
    set_focus(world, snapped.or_else(|| first_focusable(world)));
}

fn retarget_focus_after_toggle(world: &mut World, target: Entity) {
    let Some(focused) = world.resource::<Focus>().and_then(|f| f.entity) else {
        return;
    };
    if !world.is_alive(focused) || is_in_subtree(world, focused, target) {
        set_focus(world, Some(target));
    }
}

fn ensure_focus_visible(world: &mut World) {
    let Some(entity) = current_focus(world) else {
        return;
    };
    let Some(rect) = focus_rect(world, entity) else {
        return;
    };
    let Some(scroll_entity) = ancestor_with::<Scroll>(world, entity) else {
        return;
    };
    let Some(port) = world.get::<Clip>(scroll_entity).copied().or_else(|| {
        world
            .get::<Rect>(scroll_entity)
            .copied()
            .map(Clip::from_rect)
    }) else {
        return;
    };
    let mut dy = 0.0f32;
    if rect.y < port.y {
        dy = port.y - rect.y;
    } else if rect.y + rect.h > port.y + port.h {
        dy = (port.y + port.h) - (rect.y + rect.h);
    }
    let mut dx = 0.0f32;
    if rect.x < port.x {
        dx = port.x - rect.x;
    } else if rect.x + rect.w > port.x + port.w {
        dx = (port.x + port.w) - (rect.x + rect.w);
    }
    if dx == 0.0 && dy == 0.0 {
        return;
    }
    if let Some(scroll) = world.get_mut::<Scroll>(scroll_entity) {
        scroll.offset[0] += dx;
        scroll.offset[1] += dy;
    }
}

fn focus_rect(world: &World, entity: Entity) -> Option<Rect> {
    for child in children_of(world, entity) {
        if world.has::<Label>(child) {
            return world.get::<Rect>(child).copied();
        }
    }
    world.get::<Rect>(entity).copied()
}

fn current_focus(world: &World) -> Option<Entity> {
    let entity = world.resource::<Focus>()?.entity?;
    if world.is_alive(entity) && world.has::<Focusable>(entity) {
        Some(entity)
    } else {
        None
    }
}

fn set_focus(world: &mut World, entity: Option<Entity>) {
    if let Some(focus) = world.resource_mut::<Focus>() {
        focus.entity = entity;
    }
}

fn focusable_children(world: &World, entity: Entity) -> Vec<Entity> {
    children_of(world, entity)
        .into_iter()
        .filter(|child| world.has::<Focusable>(*child))
        .collect()
}

fn parent_focusable(world: &World, entity: Entity) -> Option<Entity> {
    let parent = world.get::<Parent>(entity)?.entity;
    ancestor_with::<Focusable>(world, parent)
}

fn previous_sibling_focusable(world: &World, entity: Entity) -> Option<Entity> {
    let parent = world.get::<Parent>(entity)?.entity;
    let sibs = focusable_children(world, parent);
    let i = sibs.iter().position(|e| *e == entity)?;
    if i == 0 {
        None
    } else {
        Some(sibs[i - 1])
    }
}

fn next_after_subtree(world: &World, mut entity: Entity) -> Option<Entity> {
    loop {
        let parent = world.get::<Parent>(entity)?.entity;
        let sibs = focusable_children(world, parent);
        if let Some(i) = sibs.iter().position(|e| *e == entity) {
            if i + 1 < sibs.len() {
                return Some(sibs[i + 1]);
            }
        }
        entity = parent;
        if !world.is_alive(entity) {
            return None;
        }
    }
}

fn last_visible_descendant(world: &World, entity: Entity) -> Entity {
    let mut current = entity;
    loop {
        match focusable_children(world, current).pop() {
            Some(last) => current = last,
            None => return current,
        }
    }
}

fn all_focusable(world: &World) -> Vec<Entity> {
    let mut out = Vec::new();
    for root in roots::<crate::layout::Layout>(world) {
        collect_focusable(world, root, &mut out);
    }
    out
}

fn collect_focusable(world: &World, entity: Entity, out: &mut Vec<Entity>) {
    if world.has::<Focusable>(entity) {
        out.push(entity);
    }
    for child in children_of(world, entity) {
        collect_focusable(world, child, out);
    }
}

fn first_focusable(world: &World) -> Option<Entity> {
    all_focusable(world).into_iter().next()
}

fn last_focusable(world: &World) -> Option<Entity> {
    all_focusable(world).pop()
}

fn is_in_subtree(world: &World, mut entity: Entity, ancestor: Entity) -> bool {
    loop {
        if entity == ancestor {
            return true;
        }
        match world.get::<Parent>(entity) {
            Some(parent) if world.is_alive(parent.entity) => entity = parent.entity,
            _ => return false,
        }
    }
}
