use std::any::{Any, TypeId};
use std::collections::HashMap;

use crate::entity::Entity;

/// Marker for data that can live on an entity. Not blanket-implemented so tuples stay bundles.
///
/// Every component is `'static + Send + Sync`. [`World`] itself is not
/// `Send`. UI-thread state goes in [`World::insert_local`].
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
///
/// Implemented for a single [`Component`] and for tuples of up to six
/// components. There is no impl for `()`, so [`World::spawn`] always takes
/// at least one component.
pub trait Bundle {
    /// Write this bundle onto `entity`.
    ///
    /// [`World::spawn`] allocates a live id, then calls this. A component
    /// calls [`World::insert`]. A tuple inserts each element that way. A dead
    /// `entity` makes [`World::insert`] return without writing.
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
///
/// Each component type has its own map. [`Self::insert`] on a dead entity
/// returns without writing, and [`Self::get`] returns `None`. Resources
/// require `Send + Sync` and there is one per type. Locals do not require
/// `Send`: [`Self::insert_local`] keeps UI-thread state, which is why
/// [`World`] is not `Send`.
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
    /// An empty world: no entities, no resources, and no locals.
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

    /// Allocate an entity, insert `bundle` on it, and return the id.
    ///
    /// A free index from [`Self::despawn_entity`] is reused with that slot's
    /// current generation, so a stale id still fails [`Self::is_alive`]. The
    /// returned id is alive.
    pub fn spawn(&mut self, bundle: impl Bundle) -> Entity {
        let entity = self.alloc();
        bundle.insert(self, entity);
        entity
    }

    /// True when `entity` came from [`Self::spawn`] and has not been despawned.
    ///
    /// A recycled index with an old generation is dead. An index that was
    /// never allocated is dead.
    pub fn is_alive(&self, entity: Entity) -> bool {
        let i = entity.index() as usize;
        i < self.alive.len() && self.alive[i] && self.generations[i] == entity.generation()
    }

    /// Recycles the index and bumps the generation. Cascade despawn lives in hierarchy.
    ///
    /// A dead or unknown entity returns without changing the world. Components
    /// on this id are removed. Children stay until [`crate::despawn`]. The
    /// generation uses wrapping addition.
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

    /// Store `value` as the `T` on a live `entity`, replacing a previous `T`.
    ///
    /// A dead entity returns without writing, and `value` is dropped. That
    /// includes a stale generation and an id that was never spawned. `T` is
    /// [`Component`], so it is `'static + Send + Sync`.
    pub fn insert<T: Component>(&mut self, entity: Entity, value: T) {
        if !self.is_alive(entity) {
            return;
        }
        self.store_mut::<T>().map.insert(entity, value);
    }

    /// Take `T` off `entity`.
    ///
    /// Returns `None` when `T` is absent. Despawn already cleared every
    /// component, so a dead entity returns `None`.
    pub fn remove<T: Component>(&mut self, entity: Entity) -> Option<T> {
        self.store_mut::<T>().map.remove(&entity)
    }

    /// True when `entity` currently stores `T`.
    ///
    /// False for a dead entity, a stale generation, and a live entity that
    /// never received `T`.
    pub fn has<T: Component>(&self, entity: Entity) -> bool {
        self.store::<T>()
            .is_some_and(|store| store.map.contains_key(&entity))
    }

    /// Borrow the `T` stored on `entity`.
    ///
    /// Returns `None` when `entity` is dead, when the generation is stale, or
    /// when `T` was never inserted. Despawn drops stored components, and
    /// [`Self::insert`] on that dead id returns without writing one back.
    pub fn get<T: Component>(&self, entity: Entity) -> Option<&T> {
        self.store::<T>()?.map.get(&entity)
    }

    /// Mutably borrow the `T` stored on `entity`.
    ///
    /// Returns `None` in the same cases as [`Self::get`], including a dead
    /// entity.
    pub fn get_mut<T: Component>(&mut self, entity: Entity) -> Option<&mut T> {
        self.store_mut::<T>().map.get_mut(&entity)
    }

    /// Live entities that store `T`, in arbitrary order.
    ///
    /// Empty when no `T` has been inserted. Despawned entities are omitted.
    pub fn entities_with<T: Component>(&self) -> Vec<Entity> {
        self.store::<T>()
            .map(|store| store.entities())
            .unwrap_or_default()
    }

    /// Each live entity that stores `T`, paired with a borrow of that value.
    ///
    /// Order is arbitrary. Empty when no `T` has been inserted. Despawned
    /// entities are omitted.
    pub fn query<T: Component>(&self) -> Vec<(Entity, &T)> {
        match self.store::<T>() {
            Some(store) => store.map.iter().map(|(e, v)| (*e, v)).collect(),
            None => Vec::new(),
        }
    }

    /// Store `value` as the resource of type `T`, dropping any previous one.
    ///
    /// `T: Send + Sync`, so the value may move to another thread with a task.
    /// One resource exists per type. Values that are not `Send` belong in
    /// [`Self::insert_local`].
    pub fn insert_resource<T: 'static + Send + Sync>(&mut self, value: T) {
        self.resources.insert(TypeId::of::<T>(), Box::new(value));
    }

    /// True when [`Self::insert_resource`] has stored a `T`.
    ///
    /// Locals live in another map. A `T` stored with [`Self::insert_local`]
    /// stays invisible to this check.
    pub fn has_resource<T: 'static + Send + Sync>(&self) -> bool {
        self.resources.contains_key(&TypeId::of::<T>())
    }

    /// Borrow the resource of type `T`.
    ///
    /// Returns `None` when that resource was never inserted. Read a local of
    /// the same type with [`Self::local`].
    pub fn resource<T: 'static + Send + Sync>(&self) -> Option<&T> {
        self.resources
            .get(&TypeId::of::<T>())
            .and_then(|value| value.downcast_ref::<T>())
    }

    /// Mutably borrow the resource of type `T`.
    ///
    /// Returns `None` when that resource was never inserted.
    pub fn resource_mut<T: 'static + Send + Sync>(&mut self) -> Option<&mut T> {
        self.resources
            .get_mut(&TypeId::of::<T>())
            .and_then(|value| value.downcast_mut::<T>())
    }

    /// Insert a UI-thread value. Unlike [`insert_resource`](Self::insert_resource),
    /// `T` does not have to be `Send`.
    ///
    /// Use this for `Rc`, a raw window pointer, or other `!Send` state. A
    /// [`crate::TaskHandle`] is `Send + Sync` and belongs in a resource; the
    /// local slot is only for state that cannot cross a thread.
    pub fn insert_local<T: 'static>(&mut self, value: T) {
        self.locals.insert(TypeId::of::<T>(), Box::new(value));
    }

    /// True when [`Self::insert_local`] has stored a `T`.
    ///
    /// A resource of the same type lives in the other map and stays invisible
    /// to this check. `T` does not have to be `Send`.
    pub fn has_local<T: 'static>(&self) -> bool {
        self.locals.contains_key(&TypeId::of::<T>())
    }

    /// Borrow the UI-thread local of type `T`.
    ///
    /// Returns `None` when that local was never inserted. `T` does not have
    /// to be `Send`. See [`Self::insert_local`].
    pub fn local<T: 'static>(&self) -> Option<&T> {
        self.locals
            .get(&TypeId::of::<T>())
            .and_then(|value| value.downcast_ref::<T>())
    }

    /// Mutably borrow the UI-thread local of type `T`.
    ///
    /// Returns `None` when that local was never inserted.
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
