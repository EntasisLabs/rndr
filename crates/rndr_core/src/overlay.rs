use crate::action::Fired;
use crate::hierarchy::{despawn, roots};
use crate::input::{Input, Key};
use crate::world::{Component, World};

impl Component for Layer {}
impl Component for Modal {}

/// Paint and hit order. Higher values are on top. Missing means 0.
#[derive(Clone, Copy, Debug)]
pub struct Layer(
    /// Index compared by paint and hit order. Higher is on top.
    pub i32,
);

/// Drops hits to every layer below this one.
///
/// Marker with no data. Hit testing ignores roots whose [`Layer`] is below
/// the highest modal. A modal with no [`Layer`] reads as 0.
#[derive(Clone, Copy, Debug, Default)]
pub struct Modal;

/// [`crate::Fired`] name written when Escape removes the top [`Layer`] root.
pub const DISMISS: &str = "dismiss";

/// Escape removes the top [`Layer`] root and records [`DISMISS`].
pub fn apply_overlay(world: &mut World) {
    let escape = world
        .resource::<Input>()
        .is_some_and(|input| input.key == Some(Key::Escape));
    if !escape {
        return;
    }
    let Some(root) = top_layer(world) else {
        return;
    };
    if let Some(fired) = world.resource_mut::<Fired>() {
        fired.name = Some(DISMISS.to_string());
    }
    despawn(world, root);
    if let Some(input) = world.resource_mut::<Input>() {
        input.key = None;
    }
}

fn top_layer(world: &World) -> Option<crate::entity::Entity> {
    let mut best: Option<(i32, crate::entity::Entity)> = None;
    for root in roots::<Layer>(world) {
        let layer = world.get::<Layer>(root).map(|layer| layer.0).unwrap_or(0);
        if best.map(|(current, _)| layer >= current).unwrap_or(true) {
            best = Some((layer, root));
        }
    }
    best.map(|(_, entity)| entity)
}

pub fn layer_of(world: &World, mut entity: crate::entity::Entity) -> i32 {
    loop {
        if let Some(layer) = world.get::<Layer>(entity) {
            return layer.0;
        }
        match world.get::<crate::hierarchy::Parent>(entity) {
            Some(parent) if world.is_alive(parent.entity) => entity = parent.entity,
            _ => return 0,
        }
    }
}

pub fn modal_floor(world: &World) -> Option<i32> {
    let mut floor = None;
    for entity in world.entities_with::<Modal>() {
        let layer = layer_of(world, entity);
        floor = Some(floor.map(|value: i32| value.max(layer)).unwrap_or(layer));
    }
    floor
}
