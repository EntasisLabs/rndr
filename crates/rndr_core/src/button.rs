use crate::input::{Focus, Input, Key};
use crate::layout::{ancestor_with, hit_test};
use crate::world::{Component, World};

impl Component for Button {}

/// Click or Enter target. `action` is the name written into [`crate::Fired`].
///
/// [`crate::Chrome`] does this on Update. A pointer press on this entity, or
/// on a descendant, activates it. Enter activates it when focus is here or
/// on a descendant, and that key is then cleared. An `enabled` value of
/// `false` skips both, and Enter stays in [`Input`].
#[derive(Clone, Debug)]
pub struct Button {
    /// Name copied into [`crate::Fired`] when this button activates.
    pub action: String,
    /// When `false`, press and Enter leave [`crate::Fired`] and the key unchanged.
    pub enabled: bool,
}

impl Button {
    /// Enabled button that writes `action` into [`crate::Fired`] when activated.
    pub fn new(action: impl Into<String>) -> Self {
        Self {
            action: action.into(),
            enabled: true,
        }
    }
}

/// Fire an enabled button under the pointer, or the focused button on Enter.
pub fn apply_buttons(world: &mut World) {
    let Some(input) = world.resource::<Input>().cloned() else {
        return;
    };

    if input.pointer.pressed {
        if let Some(hit) = hit_test(world, input.pointer.position[0], input.pointer.position[1]) {
            if let Some(entity) = ancestor_with::<Button>(world, hit) {
                let _ = fire(world, entity);
            }
        }
    }

    if input.key == Some(Key::Enter) {
        if let Some(entity) = focused_button(world) {
            if fire(world, entity) {
                if let Some(input) = world.resource_mut::<Input>() {
                    input.key = None;
                }
            }
        }
    }
}

fn focused_button(world: &World) -> Option<crate::entity::Entity> {
    let entity = world.resource::<Focus>().and_then(|focus| focus.entity)?;
    if world.has::<Button>(entity) {
        Some(entity)
    } else {
        ancestor_with::<Button>(world, entity)
    }
}

fn fire(world: &mut World, entity: crate::entity::Entity) -> bool {
    if world.has::<crate::ReleaseActivation>(entity) {
        return false;
    }
    crate::try_activate(world, entity) == crate::ActivationResult::Accepted
}
