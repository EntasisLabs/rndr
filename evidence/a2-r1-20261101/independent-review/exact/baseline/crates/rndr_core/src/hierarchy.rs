use crate::entity::Entity;
use crate::world::{Component, World};

impl Component for Parent {}
impl Component for Children {}

/// Child → parent link. Canonical hierarchy edge.
#[derive(Clone, Copy, Debug)]
pub struct Parent {
    /// Parent entity. A dead id means this node is a root until [`set_parent`]
    /// runs again. [`despawn`] of the parent removes the child as well.
    pub entity: Entity,
}

/// Parent → children list. Maintained by [`set_parent`].
#[derive(Clone, Debug, Default)]
pub struct Children {
    /// Children in insertion order. [`set_parent`] appends. Reparent and
    /// [`despawn`] remove. A dead id can linger here until the next edit;
    /// [`children_of`] skips those.
    pub list: Vec<Entity>,
}

/// Make `child` a child of `parent`, and keep both links in step.
///
/// Returns without writing when either entity is dead, when `child` is
/// `parent`, or when `child` is already under `parent`. An old parent loses
/// `child` from its [`Children`] list. Does not walk ancestors, so the caller
/// must not close a cycle.
pub fn set_parent(world: &mut World, child: Entity, parent: Entity) {
    if !world.is_alive(child) || !world.is_alive(parent) || child == parent {
        return;
    }
    if let Some(old) = world.get::<Parent>(child).map(|p| p.entity) {
        if old == parent {
            return;
        }
        detach_from_parent(world, child, old);
    }
    world.insert(child, Parent { entity: parent });
    match world.get_mut::<Children>(parent) {
        Some(children) => {
            if !children.list.contains(&child) {
                children.list.push(child);
            }
        }
        None => world.insert(parent, Children { list: vec![child] }),
    }
}

/// Cascade-despawn a subtree. Detached children are not created; children die with the parent.
///
/// A dead `entity` returns immediately. A live parent drops `entity` from its
/// [`Children`] list first, then every live descendant is despawned with it.
/// Dead ids already stored on a [`Children`] list are skipped.
pub fn despawn(world: &mut World, entity: Entity) {
    if !world.is_alive(entity) {
        return;
    }
    if let Some(parent) = world.get::<Parent>(entity).map(|p| p.entity) {
        detach_from_parent(world, entity, parent);
    }
    let mut stack = vec![entity];
    let mut kill = Vec::new();
    while let Some(current) = stack.pop() {
        if !world.is_alive(current) {
            continue;
        }
        kill.push(current);
        if let Some(children) = world.get::<Children>(current) {
            stack.extend(children.list.iter().copied());
        }
    }
    for dead in kill {
        world.despawn_entity(dead);
    }
}

/// Live children of `entity`, in [`Children`] order.
///
/// Dead ids stored on the parent are skipped. An entity with no [`Children`]
/// component yields an empty list. The list is owned; it is not a borrow of
/// the component.
pub fn children_of(world: &World, entity: Entity) -> Vec<Entity> {
    world
        .get::<Children>(entity)
        .map(|c| {
            c.list
                .iter()
                .copied()
                .filter(|child| world.is_alive(*child))
                .collect()
        })
        .unwrap_or_default()
}

/// Live entities with `T` that have no live [`Parent`].
///
/// A parent id that has already been despawned counts as no parent. The order
/// is the component store's, not spawn order.
pub fn roots<T: crate::world::Component>(world: &World) -> Vec<Entity> {
    world
        .entities_with::<T>()
        .into_iter()
        .filter(|entity| {
            world.is_alive(*entity)
                && world
                    .get::<Parent>(*entity)
                    .map(|p| !world.is_alive(p.entity))
                    .unwrap_or(true)
        })
        .collect()
}

fn detach_from_parent(world: &mut World, child: Entity, parent: Entity) {
    if let Some(children) = world.get_mut::<Children>(parent) {
        children.list.retain(|e| *e != child);
    }
    world.remove::<Parent>(child);
}
