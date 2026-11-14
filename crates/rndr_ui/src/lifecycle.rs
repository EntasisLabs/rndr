//! Shared retained identity mechanics, not family-specific admission or ownership.
use crate::{UiError, UiId};
use rndr_core::{children_of, Entity, Layout, World};

/// Check only the parent boundary. A successful check grants no mutation policy
/// and says nothing about a retained child's internal slots.
pub(crate) fn require_layout_parent(world: &World, parent: Entity) -> Result<(), UiError> {
    if world.is_alive(parent) && world.has::<Layout>(parent) {
        Ok(())
    } else {
        Err(UiError::Stale)
    }
}

/// Existing interactive/data identity precedence. Passive adoption must query
/// this separately from passive identity so it does not conflict with itself.
/// The public capture adapter additionally recognizes structured surfaces; that
/// observation contract is deliberately not broadened into mount admission.
pub(crate) fn interactive_key_at(world: &World, entity: Entity) -> Option<UiId> {
    crate::selection::key_at(world, entity)
        .or_else(|| crate::data::key_at(world, entity))
        .or_else(|| crate::collections::key_at(world, entity))
        .or_else(|| crate::editing::key_at(world, entity))
        .or_else(|| crate::controls::local_key_at(world, entity))
        .or_else(|| crate::inputs::key_at(world, entity))
        .or_else(|| crate::palette::key_at(world, entity))
}

/// First matching direct child, in retained child order. No registry, recursive
/// search, repair, family validation, or side effect. Callers retain their own
/// refusal type and must validate the family/slots before reuse.
pub(crate) fn find_keyed_child(world: &World, parent: Entity, key: UiId) -> Option<Entity> {
    children_of(world, parent).into_iter().find(|entity| {
        interactive_key_at(world, *entity).or_else(|| crate::surfaces::key_at(world, *entity))
            == Some(key)
    })
}
