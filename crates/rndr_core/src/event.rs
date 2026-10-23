//! Double-buffered typed event queues.
//!
//! [`send`] appends to the writer buffer. Readers see the reader buffer via
//! [`events`]. Each [`crate::App::update`] drops the previous reader buffer
//! and promotes queued sends, then appends anything emitted while tasks and
//! timers were pumped. Focus and blur from the platform or a task are merged
//! before Input. Sends from the Input stage, including a dismiss blur, are
//! merged again before Update, so `TextChanged` and `ListActivated` are
//! visible to Update on the same frame. Scale and resize are published before
//! the frame, so the opening flip makes them readable in Input and Layout.
//! Sends from Update, PostUpdate, and Layout stay queued until the next frame.

use std::any::TypeId;

use crate::world::World;

/// A value that can be written into an [`Events`] queue.
///
/// Marker bound: `Send + Sync + 'static`, with no methods. Implement it, then
/// [`send`] and [`events`] queue that type. The first [`send`] of `T` inserts
/// the [`Events`] resource.
pub trait Event: Send + Sync + 'static {}

/// Reader and writer buffers for `T`.
///
/// [`Events::iter`], [`Events::as_slice`], [`Events::len`], and
/// [`Events::is_empty`] see only the reader side. [`Events::send`] appends to
/// the writer. The frame flip publishes the writer and drops the previous
/// reader. The module docs say which schedule sees a send.
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
    /// Reader-buffer items, in send order. Unpublished writes are omitted.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.read.iter()
    }

    /// Reader buffer. Unpublished writes are omitted.
    pub fn as_slice(&self) -> &[T] {
        &self.read
    }

    /// Number of events in the reader buffer, not the unpublished writer.
    pub fn len(&self) -> usize {
        self.read.len()
    }

    /// `true` when the reader buffer is empty. Unpublished writes do not count.
    pub fn is_empty(&self) -> bool {
        self.read.is_empty()
    }

    /// Append `event` to the writer. Readers miss it until the next flip or
    /// merge. Prefer [`send`], which inserts the resource on first use.
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
///
/// The first send of `T` inserts [`Events<T>`].
///
/// # Panics
///
/// Panics if this is the first send of `T` and the world has no event hub.
/// [`crate::App::new`] installs the hub.
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
