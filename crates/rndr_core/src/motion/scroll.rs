//! Scroll motion: glides to an offset, fling and rubber-banding.
//!
//! The presented scroll position is the clamped [`Scroll::offset`] plus a
//! private overscroll that layout adds to the child origin. Glides and
//! flings run before input each frame; any other write to the offset
//! (wheel, focus reveal, app code) stops them.

use super::curve::{Anim, Curve, Spring};
use super::track::{Eps, Lane};
use crate::entity::Entity;
use crate::layout::{Rect, Scroll};
use crate::world::{Component, World};

const EPS: Eps = Eps {
    pos: 0.05,
    vel: 1.0,
};
/// Below this release speed, in points per second, a drag doesn't fling.
const MIN_FLING: f32 = 20.0;
const SAMPLES: usize = 20;
const HORIZON: f64 = 0.100;
const STOP_GAP: f64 = 0.040;

/// Fling and rubber-banding for a [`Scroll`] port dragged by a pointer.
///
/// Releasing a drag keeps the content moving at the release velocity and
/// decelerates it like UIKit; a press stops it where it is. Dragging past an
/// edge moves the content less the further it goes, and on release, or when
/// a fling reaches an edge, it springs back. Wheel scrolling is unchanged.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollPhysics {
    /// Fraction of velocity kept per millisecond while coasting, in `(0, 1)`.
    /// [`Self::NORMAL`] or [`Self::FAST`].
    pub deceleration: f32,
    /// Let a drag or fling pass the edges and spring back.
    pub rubber_band: bool,
    /// Spring that returns overscrolled content to the edge.
    pub bounce: Spring,
}

impl ScrollPhysics {
    /// UIKit's normal deceleration rate.
    pub const NORMAL: f32 = 0.998;
    /// UIKit's fast deceleration rate.
    pub const FAST: f32 = 0.99;
}

impl Default for ScrollPhysics {
    fn default() -> Self {
        Self {
            deceleration: Self::NORMAL,
            rubber_band: true,
            bounce: Spring::new(0.45, 0.0),
        }
    }
}

impl Component for ScrollPhysics {}
impl Component for ScrollMotion {}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Run {
    /// The lane holds the value and any glide or spring.
    Lane,
    /// Exponential decay from `x0` at `v0`, switching to a bounce at `hit`.
    Decay {
        t0: f64,
        x0: f32,
        v0: f32,
        k: f32,
        hit: Option<(f64, f32, f32)>,
    },
}

/// Scroll animation state for one port. Inserted by the functions here.
#[derive(Clone, Debug)]
pub(crate) struct ScrollMotion {
    lanes: [Lane; 2],
    runs: [Run; 2],
    overscroll: [f32; 2],
    written: [f32; 2],
    active: bool,
    held: Option<Held>,
}

#[derive(Clone, Debug)]
struct Held {
    raw: [f32; 2],
    samples: Vec<(f64, [f32; 2])>,
    moved_at: f64,
}

impl ScrollMotion {
    fn new(offset: [f32; 2]) -> Self {
        Self {
            lanes: offset.map(|v| Lane::new(v, EPS)),
            runs: [Run::Lane; 2],
            overscroll: [0.0; 2],
            written: offset,
            active: false,
            held: None,
        }
    }

    fn sample(&self, axis: usize, now: f64) -> (f32, f32) {
        match self.runs[axis] {
            Run::Lane => self.lanes[axis].sample(now),
            Run::Decay { t0, x0, v0, k, .. } => {
                let e = (-(k as f64) * (now - t0).max(0.0)).exp() as f32;
                (x0 + v0 / k * (1.0 - e), v0 * e)
            }
        }
    }

    fn start(&mut self, axis: usize, now: f64, target: f32, anim: Anim) {
        let (x, v) = self.sample(axis, now);
        self.lanes[axis].set(x);
        self.lanes[axis].step_to(
            now,
            target,
            Anim {
                velocity: anim.velocity.or(Some(v)),
                ..anim
            },
        );
        self.runs[axis] = Run::Lane;
    }

    fn stop(&mut self, now: f64) {
        for axis in 0..2 {
            let (x, _) = self.sample(axis, now);
            self.lanes[axis].set(x);
            self.runs[axis] = Run::Lane;
        }
        self.active = false;
    }
}

/// Scroll bounds on one axis: `[min, 0]`, from the last layout.
fn bounds(world: &World, entity: Entity) -> Option<([f32; 2], [f32; 2])> {
    let rect = world.get::<Rect>(entity)?;
    let scroll = world.get::<Scroll>(entity)?;
    let min = [
        (rect.w - scroll.content[0]).min(0.0),
        (rect.h - scroll.content[1]).min(0.0),
    ];
    Some((min, [rect.w, rect.h]))
}

fn state(world: &mut World, entity: Entity) -> Option<&mut ScrollMotion> {
    let offset = world.get::<Scroll>(entity)?.offset;
    if !world.has::<ScrollMotion>(entity) {
        world.insert(entity, ScrollMotion::new(offset));
    }
    world.get_mut::<ScrollMotion>(entity)
}

/// How far content moves when dragged `over` points past an edge of a port
/// `dim` points long (UIKit: `(1 − 1 / (|x|·0.55/d + 1)) · d`).
pub fn rubber_band(over: f32, dim: f32) -> f32 {
    if dim <= 0.0 {
        return 0.0;
    }
    let y = (1.0 - 1.0 / (over.abs() * 0.55 / dim + 1.0)) * dim;
    y.copysign(over)
}

fn unrubber(shown: f32, dim: f32) -> f32 {
    if dim <= 0.0 {
        return 0.0;
    }
    let y = (shown.abs() / dim).min(0.999);
    (dim / 0.55 * (1.0 / (1.0 - y) - 1.0)).copysign(shown)
}

/// Glide `entity`'s [`Scroll`] to `offset` (clamped to the content), from
/// the position and velocity presented now. A wheel, a drag or any other
/// write to the offset stops it. Under reduced motion it jumps.
pub fn scroll_to(world: &mut World, entity: Entity, offset: [f32; 2], anim: impl Into<Anim>) {
    let anim = super::policy(world, &[super::X], anim.into());
    let now = super::now(world);
    let Some((min, _)) = bounds(world, entity) else {
        return;
    };
    let Some(motion) = state(world, entity) else {
        return;
    };
    motion.held = None;
    for axis in 0..2 {
        let target = offset[axis].clamp(min[axis], 0.0);
        motion.start(axis, now, target, anim);
    }
    motion.active = true;
}

/// [`scroll_to`] the current glide target, or offset, plus `delta`.
pub fn scroll_by(world: &mut World, entity: Entity, delta: [f32; 2], anim: impl Into<Anim>) {
    let Some(offset) = world.get::<Scroll>(entity).map(|s| s.offset) else {
        return;
    };
    let base = match world.get::<ScrollMotion>(entity) {
        Some(m) if m.active && m.runs == [Run::Lane; 2] => [m.lanes[0].target, m.lanes[1].target],
        _ => offset,
    };
    scroll_to(
        world,
        entity,
        [base[0] + delta[0], base[1] + delta[1]],
        anim,
    );
}

/// A glide, fling or bounce is moving `entity`'s scroll offset.
pub fn is_scrolling(world: &World, entity: Entity) -> bool {
    world
        .get::<ScrollMotion>(entity)
        .is_some_and(|m| m.active || m.overscroll != [0.0; 2])
}

/// Ports a glide, fling, bounce or held drag is moving. Counted again
/// after input so a glide started this frame keeps the window redrawing.
pub(crate) fn moving(world: &World) -> usize {
    world
        .entities_with::<ScrollMotion>()
        .into_iter()
        .filter(|&e| {
            world
                .get::<ScrollMotion>(e)
                .is_some_and(|m| m.active || m.held.is_some())
        })
        .count()
}

/// Offset plus overscroll: where layout places the content.
pub(crate) fn presented_offset(world: &World, entity: Entity) -> Option<[f32; 2]> {
    let offset = world.get::<Scroll>(entity)?.offset;
    Some(match world.get::<ScrollMotion>(entity) {
        Some(m) => [offset[0] + m.overscroll[0], offset[1] + m.overscroll[1]],
        None => offset,
    })
}

fn write(world: &mut World, entity: Entity, shown: [f32; 2], min: [f32; 2], rubber: bool) {
    let offset = [0, 1].map(|a| shown[a].clamp(min[a], 0.0));
    let over = if rubber {
        [shown[0] - offset[0], shown[1] - offset[1]]
    } else {
        [0.0; 2]
    };
    if let Some(scroll) = world.get_mut::<Scroll>(entity) {
        scroll.offset = offset;
    }
    if let Some(motion) = world.get_mut::<ScrollMotion>(entity) {
        motion.overscroll = over;
        motion.written = offset;
    }
}

/// Advance glides and flings. Runs before input, so input and layout this
/// frame see the new offset. Returns how many ports are still moving.
pub(crate) fn drive(world: &mut World) -> usize {
    let now = super::now(world);
    let mut moving = 0;
    for entity in world.entities_with::<ScrollMotion>() {
        let Some(offset) = world.get::<Scroll>(entity).map(|s| s.offset) else {
            world.remove::<ScrollMotion>(entity);
            continue;
        };
        let Some((min, _)) = bounds(world, entity) else {
            continue;
        };
        let physics = world.get::<ScrollPhysics>(entity).copied();
        let rubber = physics.is_some_and(|p| p.rubber_band);
        let Some(motion) = world.get_mut::<ScrollMotion>(entity) else {
            continue;
        };
        if motion.held.is_some() {
            moving += 1;
            continue;
        }
        if !motion.active {
            continue;
        }
        if offset != motion.written {
            motion.stop(now);
            motion.overscroll = [0.0; 2];
            for (lane, value) in motion.lanes.iter_mut().zip(offset) {
                lane.set(value);
            }
            continue;
        }
        let mut still = false;
        for axis in 0..2 {
            if let Run::Decay { hit, .. } = motion.runs[axis] {
                match hit {
                    Some((at, bound, v)) if now >= at => {
                        motion.lanes[axis].set(bound);
                        motion.runs[axis] = Run::Lane;
                        if rubber {
                            let spring = physics.map_or(Spring::new(0.45, 0.0), |p| p.bounce);
                            motion.lanes[axis].step_to(
                                at,
                                bound,
                                Anim::new(Curve::Spring(spring)).velocity(v),
                            );
                        }
                    }
                    None if motion.sample(axis, now).1.abs() < EPS.vel => {
                        let rest = motion.sample(axis, now).0;
                        motion.lanes[axis].set(rest);
                        motion.runs[axis] = Run::Lane;
                    }
                    _ => {}
                }
            }
            still |= match motion.runs[axis] {
                Run::Lane => motion.lanes[axis].settle(now),
                Run::Decay { .. } => true,
            };
        }
        let shown = [motion.sample(0, now).0, motion.sample(1, now).0];
        motion.active = still;
        if still {
            moving += 1;
        }
        let shown = if still {
            shown
        } else {
            [0, 1].map(|a| shown[a].clamp(min[a], 0.0))
        };
        write(world, entity, shown, min, rubber);
    }
    moving
}

/// A press captured `entity`: stop any glide or fling where it is drawn.
pub(crate) fn catch(world: &mut World, entity: Entity) {
    if !world.has::<ScrollPhysics>(entity) {
        return;
    }
    let now = super::now(world);
    let Some((min, dim)) = bounds(world, entity) else {
        return;
    };
    let Some(shown) = presented_offset(world, entity) else {
        return;
    };
    let Some(motion) = state(world, entity) else {
        return;
    };
    motion.stop(now);
    let raw = [0, 1].map(|a| {
        let edge = shown[a].clamp(min[a], 0.0);
        edge + unrubber(shown[a] - edge, dim[a])
    });
    motion.held = Some(Held {
        raw,
        samples: vec![(now, raw)],
        moved_at: now,
    });
}

/// A held drag on a port with [`ScrollPhysics`] moved by `delta` this frame.
/// Returns false when the port has no physics and the caller scrolls it.
pub(crate) fn drag(world: &mut World, entity: Entity, delta: [f32; 2]) -> bool {
    let Some(physics) = world.get::<ScrollPhysics>(entity).copied() else {
        return false;
    };
    let now = super::now(world);
    let Some((min, dim)) = bounds(world, entity) else {
        return true;
    };
    if world
        .get::<ScrollMotion>(entity)
        .is_none_or(|m| m.held.is_none())
    {
        catch(world, entity);
    }
    let Some(held) = world
        .get_mut::<ScrollMotion>(entity)
        .and_then(|m| m.held.as_mut())
    else {
        return true;
    };
    held.raw[0] += delta[0];
    held.raw[1] += delta[1];
    if delta != [0.0; 2] {
        held.moved_at = now;
    }
    if held.samples.len() == SAMPLES {
        held.samples.remove(0);
    }
    held.samples.push((now, held.raw));
    let raw = held.raw;
    let shown = [0, 1].map(|a| {
        let edge = raw[a].clamp(min[a], 0.0);
        if physics.rubber_band {
            edge + rubber_band(raw[a] - edge, dim[a])
        } else {
            edge
        }
    });
    write(world, entity, shown, min, physics.rubber_band);
    true
}

// Least-squares velocity over the newest samples within the horizon (at
// least two frames, since positions arrive once per frame). Zero when the
// pointer stood still for longer than the stop gap.
fn velocity(held: &Held) -> [f32; 2] {
    let Some(&(newest, _)) = held.samples.last() else {
        return [0.0; 2];
    };
    if newest - held.moved_at > STOP_GAP {
        return [0.0; 2];
    }
    let n = held.samples.len();
    let take = held
        .samples
        .iter()
        .rev()
        .enumerate()
        .take_while(|(i, (t, _))| *i < 2 || newest - t <= HORIZON)
        .count();
    let points = &held.samples[n - take..];
    if points.len() < 2 {
        return [0.0; 2];
    }
    let mean_t = points.iter().map(|(t, _)| t - newest).sum::<f64>() / points.len() as f64;
    let var: f64 = points
        .iter()
        .map(|(t, _)| (t - newest - mean_t).powi(2))
        .sum();
    if var <= 1.0e-12 {
        return [0.0; 2];
    }
    [0, 1].map(|a| {
        let mean_x = points.iter().map(|(_, x)| f64::from(x[a])).sum::<f64>() / points.len() as f64;
        let cov: f64 = points
            .iter()
            .map(|(t, x)| (t - newest - mean_t) * (f64::from(x[a]) - mean_x))
            .sum();
        (cov / var) as f32
    })
}

/// The press holding `entity` ended: fling, bounce back, or rest.
pub(crate) fn release(world: &mut World, entity: Entity) {
    let now = super::now(world);
    let Some(physics) = world.get::<ScrollPhysics>(entity).copied() else {
        return;
    };
    let Some((min, _)) = bounds(world, entity) else {
        return;
    };
    let Some(shown) = presented_offset(world, entity) else {
        return;
    };
    let Some(motion) = world.get_mut::<ScrollMotion>(entity) else {
        return;
    };
    let Some(held) = motion.held.take() else {
        return;
    };
    let velocity = velocity(&held);
    let rate = physics.deceleration.clamp(0.5, 0.99999);
    let k = -(rate.ln()) * 1000.0;
    let bounce = Anim::new(Curve::Spring(physics.bounce));
    for axis in 0..2 {
        let (x, v) = (shown[axis], velocity[axis]);
        let (lo, hi) = (min[axis], 0.0);
        motion.lanes[axis].set(x);
        motion.runs[axis] = Run::Lane;
        if x < lo || x > hi {
            motion.lanes[axis].step_to(now, x.clamp(lo, hi), bounce.velocity(v));
            continue;
        }
        if v.abs() < MIN_FLING {
            continue;
        }
        let rest = x + v / k;
        let bound = if rest < lo {
            Some(lo)
        } else if rest > hi {
            Some(hi)
        } else {
            None
        };
        let hit = bound.map(|b| {
            let left = (1.0 - (b - x) * k / v).max(1.0e-6);
            (now + f64::from(-left.ln() / k), b, v * left)
        });
        motion.runs[axis] = Run::Decay {
            t0: now,
            x0: x,
            v0: v,
            k,
            hit,
        };
    }
    motion.active = true;
}
