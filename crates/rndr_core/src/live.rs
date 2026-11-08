//! Values another thread updates and the next frame draws, without running
//! Input, Update or Layout.
//!
//! A [`Live`] handle is shared between a producer and one or more bindings.
//! [`Live::set`] stores the latest value and wakes each visible window that
//! draws it once until that window's next frame. Bindings write the value into
//! [`Opacity`], [`Transform`] or [`Color`] when the frame runs, so layout never
//! sees it: text does not rewrap and siblings do not move.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};

use crate::compose::{Opacity, Transform};
use crate::motion::Spring;
use crate::runtime::{frame_wake, Wake};
use crate::world::{Component, World};
use crate::{Color, Rect};

/// Types a [`Live`] can carry.
pub trait LiveValue: Copy + PartialEq + Send + Sync + 'static {}
impl LiveValue for f32 {}
impl LiveValue for [f32; 2] {}
impl LiveValue for [f32; 4] {}
impl LiveValue for Color {}

/// Cloneable, thread-safe, latest-wins value.
///
/// Clones share one value. Setting an equal value does nothing.
pub struct Live<T: LiveValue>(Arc<Shared<T>>);

struct Shared<T> {
    value: Mutex<T>,
    version: AtomicU64,
    watchers: Watchers,
}

/// Windows that draw one handle. Touching them wakes each once per frame.
#[derive(Default)]
pub(crate) struct Watchers(Mutex<Vec<Weak<Hub>>>);

impl Watchers {
    pub(crate) fn touch(&self) {
        let mut hubs = lock(&self.0);
        hubs.retain(|hub| match hub.upgrade() {
            Some(hub) => {
                hub.touch();
                true
            }
            None => false,
        });
    }

    fn subscribe(&self, hub: &Arc<Hub>) {
        let mut hubs = lock(&self.0);
        if !hubs
            .iter()
            .any(|known| std::ptr::eq(known.as_ptr(), Arc::as_ptr(hub)))
        {
            hubs.push(Arc::downgrade(hub));
        }
    }
}

impl<T: LiveValue> Clone for Live<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<T: LiveValue + std::fmt::Debug> std::fmt::Debug for Live<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Live").field(&self.get()).finish()
    }
}

impl<T: LiveValue> Live<T> {
    /// Handle holding `value`.
    pub fn new(value: T) -> Self {
        Self(Arc::new(Shared {
            value: Mutex::new(value),
            version: AtomicU64::new(0),
            watchers: Watchers::default(),
        }))
    }

    /// Latest value.
    pub fn get(&self) -> T {
        *lock(&self.0.value)
    }

    /// Store `value`. Wakes each visible window that draws this handle, once
    /// per frame. Returns `false` when `value` equals the stored value.
    pub fn set(&self, value: T) -> bool {
        {
            let mut stored = lock(&self.0.value);
            if *stored == value {
                return false;
            }
            *stored = value;
            self.0.version.fetch_add(1, Ordering::Release);
        }
        self.0.watchers.touch();
        true
    }

    /// Count of accepted [`Self::set`] calls.
    pub fn version(&self) -> u64 {
        self.0.version.load(Ordering::Acquire)
    }

    fn subscribe(&self, hub: &Arc<Hub>) {
        self.0.watchers.subscribe(hub);
    }
}

/// One world's wake state, shared with every handle it draws.
pub(crate) struct Hub {
    wake: Mutex<Arc<dyn Wake>>,
    visible: AtomicBool,
    /// The OS reports the window minimized or fully covered.
    occluded: AtomicBool,
    dirty: AtomicBool,
}

impl Hub {
    fn touch(&self) {
        let was = self.dirty.swap(true, Ordering::AcqRel);
        if !was && self.visible.load(Ordering::Acquire) && !self.occluded.load(Ordering::Acquire) {
            lock(&self.wake).wake();
        }
    }
}

struct LiveHub(Arc<Hub>);

fn hub(world: &mut World) -> Arc<Hub> {
    if let Some(hub) = world.resource::<LiveHub>() {
        return Arc::clone(&hub.0);
    }
    let hub = Arc::new(Hub {
        wake: Mutex::new(frame_wake(world)),
        visible: AtomicBool::new(crate::window::is_visible(world)),
        occluded: AtomicBool::new(false),
        dirty: AtomicBool::new(false),
    });
    world.insert_resource(LiveHub(Arc::clone(&hub)));
    hub
}

/// Route live wakes to `wake` instead of the frame wake.
///
/// A runner uses this to tell a live-only wake from one that needs a full
/// frame.
pub fn set_live_wake(world: &mut World, wake: Arc<dyn Wake>) {
    *lock(&hub(world).wake) = wake;
}

/// Record whether the OS shows the window. While it is minimized or fully
/// covered, live values do not wake it and [`pending`] stays false; changes
/// are kept. Returns `true` when this reveals a change not yet drawn, so the
/// runner should request a frame.
pub fn set_occluded(world: &mut World, occluded: bool) -> bool {
    let hub = hub(world);
    let was = hub.occluded.swap(occluded, Ordering::AcqRel);
    was && !occluded && hub.dirty.load(Ordering::Acquire)
}

/// Whether [`set_occluded`] last recorded the window as not shown.
pub fn occluded(world: &World) -> bool {
    world
        .resource::<LiveHub>()
        .is_some_and(|hub| hub.0.occluded.load(Ordering::Acquire))
}

/// A bound handle changed since the last frame and the window is shown.
pub fn pending(world: &World) -> bool {
    world.resource::<LiveHub>().is_some_and(|hub| {
        hub.0.dirty.load(Ordering::Acquire)
            && !hub.0.occluded.load(Ordering::Acquire)
            && crate::window::is_visible(world)
    })
}

/// Optional smoothing for a binding.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Smooth {
    /// Draw the latest value as is.
    None,
    /// Follow the latest value with a spring, retargeted on each change.
    Spring(Spring),
}

/// Per-channel spring state for a smoothed binding.
#[derive(Clone, Copy, Debug, Default)]
struct Follow {
    started: bool,
    value: [f32; 4],
    velocity: [f32; 4],
}

impl Follow {
    /// Step toward `target` by `dt` seconds. Returns whether it is still moving.
    fn step(&mut self, smooth: Smooth, target: [f32; 4], dt: f32, lanes: usize) -> bool {
        let Smooth::Spring(spring) = smooth else {
            self.value = target;
            self.started = true;
            return false;
        };
        if !self.started {
            self.value = target;
            self.started = true;
            return false;
        }
        let mut moving = false;
        let lanes = self
            .value
            .iter_mut()
            .zip(self.velocity.iter_mut())
            .zip(target)
            .take(lanes);
        for ((value, velocity), goal) in lanes {
            let (next, speed) = spring.sample(*value, *velocity, goal, dt);
            let scale = goal.abs().max(1.0);
            if (next - goal).abs() < 1.0e-3 * scale && speed.abs() < 1.0e-2 * scale {
                *value = goal;
                *velocity = 0.0;
            } else {
                *value = next;
                *velocity = speed;
                moving = true;
            }
        }
        moving
    }
}

macro_rules! binding {
    ($(#[$doc:meta])* $name:ident, $ty:ty) => {
        $(#[$doc])*
        #[derive(Clone, Debug)]
        pub struct $name {
            /// Handle the binding reads.
            pub live: Live<$ty>,
            /// Smoothing toward the latest value.
            pub smooth: Smooth,
            follow: Follow,
        }

        impl $name {
            /// Draw `live` as is.
            pub fn new(live: &Live<$ty>) -> Self {
                Self { live: live.clone(), smooth: Smooth::None, follow: Follow::default() }
            }

            /// Follow `live` with `spring`.
            pub fn spring(live: &Live<$ty>, spring: Spring) -> Self {
                Self { smooth: Smooth::Spring(spring), ..Self::new(live) }
            }
        }

        impl Component for $name {}
    };
}

binding!(
    /// Binds a [`Live<f32>`] to [`Opacity`].
    LiveOpacity,
    f32
);
binding!(
    /// Binds a [`Live<[f32; 2]>`](Live) to [`Transform::translate`], in points.
    LiveOffset,
    [f32; 2]
);
binding!(
    /// Binds a [`Live<[f32; 2]>`](Live) to [`Transform::scale`].
    LiveScale,
    [f32; 2]
);
binding!(
    /// Binds a [`Live<Color>`] to the entity's fill [`Color`].
    LiveFill,
    Color
);
binding!(
    /// Binds a [`Live<[f32; 4]>`](Live) `[x, y, w, h]` in window points to
    /// where the entity is drawn and hit. Layout keeps its own rect; the
    /// entity is moved and scaled from it through [`Transform`], so its text
    /// scales rather than rewrapping.
    LiveRect,
    [f32; 4]
);

fn lanes_f32(value: f32) -> [f32; 4] {
    [value, 0.0, 0.0, 0.0]
}

fn lanes_2(value: [f32; 2]) -> [f32; 4] {
    [value[0], value[1], 0.0, 0.0]
}

fn lanes_color(color: Color) -> [f32; 4] {
    [color.r, color.g, color.b, color.a]
}

/// Copy every bound value into its component. Runs on every frame, full or
/// live-only, after motion, so a binding wins over an animation of the same
/// property. A smoothed binding still settling keeps [`pending`] set.
pub(crate) fn apply(world: &mut World) {
    let hub = hub(world);
    hub.visible
        .store(crate::window::is_visible(world), Ordering::Release);
    hub.dirty.store(false, Ordering::Release);
    let dt = world
        .resource::<crate::Time>()
        .map_or(0.0, |time| time.delta.as_secs_f32())
        .min(0.1);
    let mut moving = false;

    for entity in world.entities_with::<crate::SurfaceView>() {
        if let Some(view) = world.get::<crate::SurfaceView>(entity) {
            view.surface.watchers().subscribe(&hub);
        }
    }
    for entity in world.entities_with::<LiveOpacity>() {
        let Some(mut bind) = world.get::<LiveOpacity>(entity).cloned() else {
            continue;
        };
        bind.live.subscribe(&hub);
        moving |= bind
            .follow
            .step(bind.smooth, lanes_f32(bind.live.get()), dt, 1);
        let value = bind.follow.value[0];
        world.insert(entity, bind);
        world.insert(entity, Opacity(value));
    }
    for entity in world.entities_with::<LiveFill>() {
        let Some(mut bind) = world.get::<LiveFill>(entity).cloned() else {
            continue;
        };
        bind.live.subscribe(&hub);
        moving |= bind
            .follow
            .step(bind.smooth, lanes_color(bind.live.get()), dt, 4);
        let [r, g, b, a] = bind.follow.value;
        world.insert(entity, bind);
        world.insert(entity, Color { r, g, b, a });
    }
    let mut moved = world.entities_with::<LiveOffset>();
    moved.extend(world.entities_with::<LiveScale>());
    moved.extend(world.entities_with::<LiveRect>());
    let mut seen = std::collections::HashSet::new();
    moved.retain(|entity| seen.insert(*entity));
    for entity in moved {
        let mut transform = world.get::<Transform>(entity).copied().unwrap_or_default();
        if let Some(mut bind) = world.get::<LiveOffset>(entity).cloned() {
            bind.live.subscribe(&hub);
            moving |= bind
                .follow
                .step(bind.smooth, lanes_2(bind.live.get()), dt, 2);
            transform.translate = [bind.follow.value[0], bind.follow.value[1]];
            world.insert(entity, bind);
        }
        if let Some(mut bind) = world.get::<LiveScale>(entity).cloned() {
            bind.live.subscribe(&hub);
            moving |= bind
                .follow
                .step(bind.smooth, lanes_2(bind.live.get()), dt, 2);
            transform.scale = [bind.follow.value[0], bind.follow.value[1]];
            world.insert(entity, bind);
        }
        if let Some(mut bind) = world.get::<LiveRect>(entity).cloned() {
            bind.live.subscribe(&hub);
            moving |= bind.follow.step(bind.smooth, bind.live.get(), dt, 4);
            if let Some(rect) = world.get::<Rect>(entity).copied() {
                let [x, y, w, h] = bind.follow.value;
                transform.origin = [0.0, 0.0];
                transform.translate = [x - rect.x, y - rect.y];
                transform.scale = [
                    if rect.w > 0.0 { w / rect.w } else { 1.0 },
                    if rect.h > 0.0 { h / rect.h } else { 1.0 },
                ];
            }
            world.insert(entity, bind);
        }
        world.insert(entity, transform);
    }
    // A smoothed binding still settling needs another frame.
    if moving {
        hub.dirty.store(true, Ordering::Release);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poison| poison.into_inner())
}
