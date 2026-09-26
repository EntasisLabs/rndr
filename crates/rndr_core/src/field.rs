use crate::caret::Caret;
use crate::edit::edit_string;
use crate::input::{Focus, Input, Key};
use crate::layout::{ancestor_with, hit_test, Rect};
use crate::style::{Label, TEXT_ORIGIN_X, TEXT_ORIGIN_Y};
use crate::world::{Component, World};
use crate::wrap::layout_of;

impl Component for TextField {}
impl Component for CapturesKeys {}

/// Single-line editor on this entity's [`Label`]. No file binding.
#[derive(Clone, Copy, Debug, Default)]
pub struct TextField;

/// Focused entity keeps key edges for itself. Tree navigation does not take them.
#[derive(Clone, Copy, Debug, Default)]
pub struct CapturesKeys;

pub fn apply_fields(world: &mut World) {
    let focus = world.resource::<Focus>().and_then(|focus| focus.entity);
    let fields = world.entities_with::<TextField>();
    for entity in fields {
        if focus != Some(entity) {
            world.remove::<Caret>(entity);
            continue;
        }
        let existed = world.has::<Caret>(entity);
        if !existed {
            let len = world
                .get::<Label>(entity)
                .map(|label| label.text.len())
                .unwrap_or(0);
            world.insert(
                entity,
                Caret {
                    index: len,
                    anchor: len,
                },
            );
        }
        place_from_pointer(world, entity, existed);
        edit_label(world, entity);
    }
}

fn place_from_pointer(world: &mut World, entity: crate::entity::Entity, existed: bool) {
    let Some(input) = world.resource::<Input>().cloned() else {
        return;
    };
    if !input.pointer.pressed {
        return;
    }
    let Some(hit) = hit_test(world, input.pointer.position[0], input.pointer.position[1]) else {
        return;
    };
    if ancestor_with::<TextField>(world, hit) != Some(entity) {
        return;
    }
    let Some(rect) = world.get::<Rect>(entity).copied() else {
        return;
    };
    let text = world
        .get::<Label>(entity)
        .map(|label| label.text.clone())
        .unwrap_or_default();
    let layout = layout_of(world, entity, &text);
    let index = layout.index_at(
        input.pointer.position[0] - rect.x - TEXT_ORIGIN_X,
        input.pointer.position[1] - rect.y - TEXT_ORIGIN_Y,
    );
    if let Some(caret) = world.get_mut::<Caret>(entity) {
        caret.index = index;
        if !(input.shift && existed) {
            caret.anchor = index;
        }
    }
}

fn edit_label(world: &mut World, entity: crate::entity::Entity) {
    let (key, shift, chars) = match world.resource::<Input>() {
        Some(input) => (input.key, input.shift, input.chars.clone()),
        None => return,
    };
    let mut text = world
        .get::<Label>(entity)
        .map(|label| label.text.clone())
        .unwrap_or_default();
    let (index, anchor) = world
        .get::<Caret>(entity)
        .map(|caret| (caret.index, caret.anchor))
        .unwrap_or((text.len(), text.len()));
    let (index, anchor) = edit_string(&mut text, index, anchor, key, shift, &chars, false);
    if let Some(label) = world.get_mut::<Label>(entity) {
        label.text = text;
    }
    if let Some(caret) = world.get_mut::<Caret>(entity) {
        caret.index = index;
        caret.anchor = anchor;
    }
    if let Some(input) = world.resource_mut::<Input>() {
        input.chars.clear();
        if matches!(input.key, Some(Key::Left | Key::Right | Key::Backspace)) {
            input.key = None;
        }
    }
}
