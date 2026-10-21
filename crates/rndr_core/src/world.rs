use std::any::{Any, TypeId};
use std::collections::HashMap;

use crate::entity::Entity;

/// Marker for data that can live on an entity. Not blanket-implemented so tuples stay bundles.
pub trait Component: 'static + Send + Sync {}

trait ErasedStore: Send + Sync {
    fn remove(&mut self, entity: Entity);
    fn entities(&self) -> Vec<Entity>;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

struct Store<T> {
    map: HashMap<Entity, T>,
}

impl<T: Component> ErasedStore for Store<T> {
    fn remove(&mut self, entity: Entity) {
        self.map.remove(&entity);
    }

    fn entities(&self) -> Vec<Entity> {
        self.map.keys().copied().collect()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Data that can be spawned onto a new entity.
pub trait Bundle {
    fn insert(self, world: &mut World, entity: Entity);
}

impl<T: Component> Bundle for T {
    fn insert(self, world: &mut World, entity: Entity) {
        world.insert(entity, self);
    }
}

macro_rules! impl_bundle_tuple {
    ($($name:ident),+) => {
        impl<$($name: Component),+> Bundle for ($($name,)+) {
            #[allow(non_snake_case)]
            fn insert(self, world: &mut World, entity: Entity) {
                let ($($name,)+) = self;
                $(world.insert(entity, $name);)+
            }
        }
    };
}

impl_bundle_tuple!(A);
impl_bundle_tuple!(A, B);
impl_bundle_tuple!(A, B, C);
impl_bundle_tuple!(A, B, C, D);
impl_bundle_tuple!(A, B, C, D, E);
impl_bundle_tuple!(A, B, C, D, E, F);

/// Sparse component maps plus generation-safe entity ids.
pub struct World {
    generations: Vec<u32>,
    alive: Vec<bool>,
    free: Vec<u32>,
    stores: HashMap<TypeId, Box<dyn ErasedStore>>,
    resources: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
    /// Main-thread values. Foreground task futures live here, so [`World`] is not `Send`.
    locals: HashMap<TypeId, Box<dyn Any>>,
}

impl World {
    pub fn new() -> Self {
        Self {
            generations: Vec::new(),
            alive: Vec::new(),
            free: Vec::new(),
            stores: HashMap::new(),
            resources: HashMap::new(),
            locals: HashMap::new(),
        }
    }

    pub fn spawn(&mut self, bundle: impl Bundle) -> Entity {
        let entity = self.alloc();
        bundle.insert(self, entity);
        entity
    }

    pub fn is_alive(&self, entity: Entity) -> bool {
        let i = entity.index() as usize;
        i < self.alive.len() && self.alive[i] && self.generations[i] == entity.generation()
    }

    /// Recycles the index and bumps the generation. Cascade despawn lives in hierarchy.
    pub fn despawn_entity(&mut self, entity: Entity) {
        if !self.is_alive(entity) {
            return;
        }
        let i = entity.index() as usize;
        self.alive[i] = false;
        self.generations[i] = self.generations[i].wrapping_add(1);
        self.free.push(entity.index());
        for store in self.stores.values_mut() {
            store.remove(entity);
        }
    }

    pub fn insert<T: Component>(&mut self, entity: Entity, value: T) {
        if !self.is_alive(entity) {
            return;
        }
        self.store_mut::<T>().map.insert(entity, value);
    }

    pub fn remove<T: Component>(&mut self, entity: Entity) -> Option<T> {
        self.store_mut::<T>().map.remove(&entity)
    }

    pub fn has<T: Component>(&self, entity: Entity) -> bool {
        self.store::<T>()
            .is_some_and(|store| store.map.contains_key(&entity))
    }

    pub fn get<T: Component>(&self, entity: Entity) -> Option<&T> {
        self.store::<T>()?.map.get(&entity)
    }

    pub fn get_mut<T: Component>(&mut self, entity: Entity) -> Option<&mut T> {
        self.store_mut::<T>().map.get_mut(&entity)
    }

    pub fn entities_with<T: Component>(&self) -> Vec<Entity> {
        self.store::<T>()
            .map(|store| store.entities())
            .unwrap_or_default()
    }

    pub fn query<T: Component>(&self) -> Vec<(Entity, &T)> {
        match self.store::<T>() {
            Some(store) => store.map.iter().map(|(e, v)| (*e, v)).collect(),
            None => Vec::new(),
        }
    }

    pub fn insert_resource<T: 'static + Send + Sync>(&mut self, value: T) {
        self.resources.insert(TypeId::of::<T>(), Box::new(value));
    }

    pub fn has_resource<T: 'static + Send + Sync>(&self) -> bool {
        self.resources.contains_key(&TypeId::of::<T>())
    }

    pub fn resource<T: 'static + Send + Sync>(&self) -> Option<&T> {
        self.resources
            .get(&TypeId::of::<T>())
            .and_then(|value| value.downcast_ref::<T>())
    }

    pub fn resource_mut<T: 'static + Send + Sync>(&mut self) -> Option<&mut T> {
        self.resources
            .get_mut(&TypeId::of::<T>())
            .and_then(|value| value.downcast_mut::<T>())
    }

    /// Insert a main-thread value. Unlike [`insert_resource`](Self::insert_resource),
    /// `T` does not have to be `Send`.
    pub fn insert_local<T: 'static>(&mut self, value: T) {
        self.locals.insert(TypeId::of::<T>(), Box::new(value));
    }

    pub fn has_local<T: 'static>(&self) -> bool {
        self.locals.contains_key(&TypeId::of::<T>())
    }

    pub fn local<T: 'static>(&self) -> Option<&T> {
        self.locals
            .get(&TypeId::of::<T>())
            .and_then(|value| value.downcast_ref::<T>())
    }

    pub fn local_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.locals
            .get_mut(&TypeId::of::<T>())
            .and_then(|value| value.downcast_mut::<T>())
    }

    fn alloc(&mut self) -> Entity {
        if let Some(index) = self.free.pop() {
            let i = index as usize;
            self.alive[i] = true;
            Entity::new(index, self.generations[i])
        } else {
            let index = self.generations.len() as u32;
            self.generations.push(0);
            self.alive.push(true);
            Entity::new(index, 0)
        }
    }

    fn store<T: Component>(&self) -> Option<&Store<T>> {
        self.stores
            .get(&TypeId::of::<T>())
            .and_then(|erased| erased.as_any().downcast_ref::<Store<T>>())
    }

    fn store_mut<T: Component>(&mut self) -> &mut Store<T> {
        self.stores.entry(TypeId::of::<T>()).or_insert_with(|| {
            Box::new(Store::<T> {
                map: HashMap::new(),
            })
        });
        self.stores
            .get_mut(&TypeId::of::<T>())
            .and_then(|erased| erased.as_any_mut().downcast_mut::<Store<T>>())
            .expect("component store type")
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}
