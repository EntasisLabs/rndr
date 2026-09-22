use crate::entity::Entity;
use crate::hierarchy::children_of;
use crate::input::{Input, Key};
use crate::pane::Pane;
use crate::style::Label;
use crate::world::{Component, World};

impl Component for Caret {}

/// Tree node whose pane title is being edited.
#[derive(Clone, Copy, Debug, Default)]
pub struct Editing {
    pub source: Option<Entity>,
}

/// Insertion point on a pane title [`Label`]. Index is a byte offset.
#[derive(Clone, Copy, Debug, Default)]
pub struct Caret {
    pub index: usize,
}

pub fn set_editing(world: &mut World, source: Option<Entity>) {
    if let Some(editing) = world.resource_mut::<Editing>() {
        editing.source = source;
    }
}

pub fn is_editing(world: &World) -> bool {
    world
        .resource::<Editing>()
        .is_some_and(|editing| editing.source.is_some())
}

/// Bind or move the pane-title caret from [`Editing`] and [`Input`].
pub fn apply_caret(world: &mut World) {
    let source = world
        .resource::<Editing>()
        .and_then(|editing| editing.source)
        .filter(|entity| world.is_alive(*entity));
    let Some(source) = source else {
        clear_carets(world);
        if let Some(editing) = world.resource_mut::<Editing>() {
            editing.source = None;
        }
        clear_text_input(world);
        return;
    };

    let Some(title) = pane_title(world) else {
        return;
    };

    if !world.has::<Caret>(title) {
        let len = world
            .get::<Label>(title)
            .map(|label| label.text.len())
            .unwrap_or(0);
        clear_carets(world);
        world.insert(title, Caret { index: len });
    }

    let chars = world
        .resource::<Input>()
        .map(|input| input.chars.clone())
        .unwrap_or_default();
    let key = world.resource::<Input>().and_then(|input| input.key);

    let mut text = world
        .get::<Label>(title)
        .map(|label| label.text.clone())
        .unwrap_or_default();
    let mut index = world
        .get::<Caret>(title)
        .map(|caret| caret.index.min(text.len()))
        .unwrap_or(text.len());

    for ch in chars.chars() {
        if is_printable_ascii(ch) {
            text.insert(index, ch);
            index += ch.len_utf8();
        }
    }
    if key == Some(Key::Backspace) && index > 0 {
        index -= 1;
        text.remove(index);
    }
    if key == Some(Key::Left) {
        index = index.saturating_sub(1);
    }
    if key == Some(Key::Right) {
        index = (index + 1).min(text.len());
    }

    if let Some(label) = world.get_mut::<Label>(title) {
        label.text.clone_from(&text);
    }
    if let Some(caret) = world.get_mut::<Caret>(title) {
        caret.index = index;
    }
    if let Some(header) = header_label_entity(world, source) {
        if let Some(label) = world.get_mut::<Label>(header) {
            label.text = text;
        }
    }

    clear_text_input(world);
}

fn pane_title(world: &World) -> Option<Entity> {
    let pane = world.entities_with::<Pane>().into_iter().next()?;
    world.get::<Pane>(pane)?.spawned.first().copied()
}

fn header_label_entity(world: &World, entity: Entity) -> Option<Entity> {
    for child in children_of(world, entity) {
        if world.has::<Label>(child) {
            return Some(child);
        }
    }
    world.has::<Label>(entity).then_some(entity)
}

fn clear_carets(world: &mut World) {
    for entity in world.entities_with::<Caret>() {
        world.remove::<Caret>(entity);
    }
}

fn clear_text_input(world: &mut World) {
    if let Some(input) = world.resource_mut::<Input>() {
        input.chars.clear();
        if matches!(input.key, Some(Key::Left | Key::Right | Key::Backspace)) {
            input.key = None;
        }
    }
}

fn is_printable_ascii(ch: char) -> bool {
    matches!(ch, ' '..='~')
}
