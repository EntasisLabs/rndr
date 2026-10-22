//! Double-buffered typed event queues.
//!
//! [`send`] appends to the writer buffer. Readers see the reader buffer via
//! [`events`]. Each [`crate::App::update`] drops the previous reader buffer
//! and promotes queued sends, then appends anything emitted while tasks and
//! timers were pumped so systems on that frame can read them. Sends from
//! systems stay queued until the next frame.

use std::any::TypeId;

use crate::world::World;

/// A value that can be written into an [`Events`] queue.
pub trait Event: Send + Sync + 'static {}

/// Reader and writer buffers for `T`.
#[derive(Debug)]
pub struct Events<T> {
    read: Vec<T>,
    write: Vec<T>,
}

impl<T> Default for Events<T> {
    fn default() -> Self {
        Self {
            read: Vec::new(),
            write: Vec::new(),
        }
    }
}

impl<T> Events<T> {
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.read.iter()
    }

    pub fn as_slice(&self) -> &[T] {
        &self.read
    }

    pub fn len(&self) -> usize {
        self.read.len()
    }

    pub fn is_empty(&self) -> bool {
        self.read.is_empty()
    }

    pub fn send(&mut self, event: T) {
        self.write.push(event);
    }

    pub(crate) fn flip(&mut self) {
        self.read.clear();
        std::mem::swap(&mut self.read, &mut self.write);
    }

    pub(crate) fn merge(&mut self) {
        if !self.write.is_empty() {
            self.read.append(&mut self.write);
        }
    }
}

#[derive(Default)]
pub(crate) struct EventHub {
    flips: Vec<fn(&mut World)>,
    merges: Vec<fn(&mut World)>,
    types: Vec<TypeId>,
}

fn ensure<T: Event>(world: &mut World) {
    if world.has_resource::<Events<T>>() {
        return;
    }
    world.insert_resource(Events::<T>::default());
    let hub = world
        .resource_mut::<EventHub>()
        .expect("event hub installed by App");
    let id = TypeId::of::<T>();
    if hub.types.contains(&id) {
        return;
    }
    hub.types.push(id);
    hub.flips.push(|world| {
        if let Some(events) = world.resource_mut::<Events<T>>() {
            events.flip();
        }
    });
    hub.merges.push(|world| {
        if let Some(events) = world.resource_mut::<Events<T>>() {
            events.merge();
        }
    });
}

/// Queue `event` for readers. Visible to systems on the next commit
/// (the current frame's pump, or the next frame when called from a system).
pub fn send<T: Event>(world: &mut World, event: T) {
    ensure::<T>(world);
    world
        .resource_mut::<Events<T>>()
        .expect("event queue")
        .send(event);
}

/// Events readable on this frame. Empty when the queue was never written.
pub fn events<T: Event>(world: &World) -> &[T] {
    world
        .resource::<Events<T>>()
        .map(Events::as_slice)
        .unwrap_or(&[])
}

pub(crate) fn flip_events(world: &mut World) {
    let flips = world
        .resource::<EventHub>()
        .map(|hub| hub.flips.clone())
        .unwrap_or_default();
    for flip in flips {
        flip(world);
    }
}

pub(crate) fn merge_events(world: &mut World) {
    let merges = world
        .resource::<EventHub>()
        .map(|hub| hub.merges.clone())
        .unwrap_or_default();
    for merge in merges {
        merge(world);
    }
}
