use crate::entity::Entity;
use crate::world::{Component, World};

impl Component for Parent {}
impl Component for Children {}

/// Child → parent link. Canonical hierarchy edge.
#[derive(Clone, Copy, Debug)]
pub struct Parent {
    pub entity: Entity,
}

/// Parent → children list. Maintained by [`set_parent`].
#[derive(Clone, Debug, Default)]
pub struct Children {
    pub list: Vec<Entity>,
}

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
