//! The secondary gesture: right-click, macOS Control-click, the Menu key and Shift+F10.
//!
//! Elements opt in with [`ContextTarget`]. The gesture sends [`SecondaryPress`] for the
//! nearest enabled target at the pointer, or around keyboard focus. A Control-click on
//! macOS is reported to every later Input system as a right button, so it never also
//! focuses, activates or starts a text selection.
use crate::*;

/// Marks an element that answers the secondary gesture, usually with a context menu.
#[derive(Clone, Copy, Debug, Default)]
pub struct ContextTarget;
impl Component for ContextTarget {}

/// A secondary gesture on a [`ContextTarget`]. Sent during Input, readable in Update.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SecondaryPress {
    /// Nearest enabled [`ContextTarget`] at the pointer or the focused element.
    pub entity: Entity,
    /// Where a menu should open, in logical points: the press point, or the
    /// target's bottom-left corner for a key.
    pub at: [f32; 2],
    /// True for the Menu key and Shift+F10.
    pub keyboard: bool,
}
impl Event for SecondaryPress {}

/// Control held at a macOS left press; the gesture stays secondary until it ends.
#[derive(Default)]
struct ControlClick(bool);

fn target(world: &World, entity: Entity) -> Option<Entity> {
    let target = crate::layout::ancestor_with::<ContextTarget>(world, entity)?;
    crate::interaction::policy(world, target)
        .0
        .then_some(target)
}

fn opens_menu(event: &KeyEvent) -> bool {
    let m = event.modifiers;
    if event.released || event.repeat || m.ctrl || m.alt || m.super_key {
        return false;
    }
    match event.logical {
        LogicalKey::Named(NamedKey::ContextMenu) => true,
        LogicalKey::Named(NamedKey::F(10)) => m.shift,
        _ => false,
    }
}

/// Front of Input: map macOS Control-click to the right button, then send
/// [`SecondaryPress`] for presses and consume the keys that produced one.
pub(crate) fn apply_secondary(world: &mut World) {
    let mac = matches!(crate::keys::platform_of(world), Platform::Mac);
    let Some(input) = world.resource_mut::<Input>() else {
        return;
    };
    let pointer = input.pointer;
    let control = input.ctrl;
    let mut held = world.resource::<ControlClick>().is_some_and(|c| c.0);
    if pointer.pressed {
        held = mac && control && pointer.button == PointerButton::Left;
    }
    if held {
        world.resource_mut::<Input>().unwrap().pointer.button = PointerButton::Right;
    }
    let ended = pointer.released || !pointer.down;
    world.insert_resource(ControlClick(held && !ended));

    if pointer.pressed && (held || pointer.button == PointerButton::Right) {
        let at = pointer.press_at.unwrap_or(pointer.position);
        if let Some(entity) =
            crate::layout::hit_test(world, at[0], at[1]).and_then(|hit| target(world, hit))
        {
            send(
                world,
                SecondaryPress {
                    entity,
                    at,
                    keyboard: false,
                },
            );
        }
    }

    let keys = std::mem::take(&mut world.resource_mut::<Input>().unwrap().keys);
    let mut kept = Vec::with_capacity(keys.len());
    for event in keys {
        let focused = world.resource::<Focus>().and_then(|f| f.entity);
        let entity = opens_menu(&event)
            .then_some(focused)
            .flatten()
            .and_then(|e| target(world, e));
        match entity {
            Some(entity) => {
                let r = world.get::<Rect>(entity).copied().unwrap_or_default();
                let at = [r.x, r.y + r.h];
                send(
                    world,
                    SecondaryPress {
                        entity,
                        at,
                        keyboard: true,
                    },
                );
            }
            None => kept.push(event),
        }
    }
    world.resource_mut::<Input>().unwrap().keys = kept;
}
