//! Motion runtime: animated [`Opacity`] and [`Transform`] for any entity.
//!
//! Application code states targets with [`animate`], [`enter`], and [`exit`].
//! Every frame, after layout, the runtime samples each running animation at
//! the frame time and writes the entity's [`Opacity`] and [`Transform`], which
//! paint composites and hit testing follows. While an entity carries
//! [`Motion`], those two components are owned by the runtime; write targets
//! through this module instead.
//!
//! Curves are evaluated in closed form, so a value at a given time does not
//! depend on the frame rate, and a test can step a [`crate::ManualClock`] to
//! any time. Retargeting mid-flight keeps the presented value and velocity.
//! Nothing animates on its own on first mount, and an unchanged target keeps
//! the motion already running.
//!
//! [`LayoutMotion`] makes an entity glide when layout moves it within its
//! parent, and grow or shrink when layout resizes it. [`exit`] keeps an
//! entity drawn, and in layout, until its exit settles, then despawns it.
//!
//! [`animate_color`] blends fills, border colours and label colours in
//! OKLab. [`play`] runs a [`Keyframes`] track on a property. [`PressScale`]
//! shrinks an element while a press on it is held.
//!
//! [`MotionSettings`] turns on reduced motion (offsets, scale, and rotation
//! jump; opacity takes a short fade) and scales motion time.
//!
//! ```
//! use std::time::Duration;
//! use rndr_core::motion::{self, Prop, Spring};
//! use rndr_core::{Layout, Opacity, Sim};
//!
//! let mut sim = Sim::new();
//! let card = sim.world().spawn((Layout::column(),));
//! motion::animate(sim.world(), card, Prop::Opacity, 0.0, Spring::SMOOTH);
//! sim.update();
//! sim.advance(Duration::from_millis(100));
//! sim.update();
//! let mid = sim.world().get::<Opacity>(card).unwrap().0;
//! assert!(mid > 0.0 && mid < 1.0);
//! sim.advance(Duration::from_secs(2));
//! sim.update();
//! assert_eq!(sim.world().get::<Opacity>(card).unwrap().0, 0.0);
//! assert!(!motion::animating(sim.world()));
//! ```

mod color;
mod curve;
mod keyframes;
mod track;

use std::time::Duration;

pub use color::ColorProp;
pub use curve::{Anim, Curve, Easing, Spring};
use keyframes::Script;
pub use keyframes::{Keyframes, Repeat};
use track::{Eps, Lane};

use crate::compose::{Opacity, Transform};
use crate::entity::Entity;
use crate::event::Event;
use crate::hierarchy::Parent;
use crate::layout::{Clip, Rect, Scroll};
use crate::style::{Border, Color, Label};
use crate::world::{Component, World};

/// An animatable property.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Prop {
    /// Group opacity, `0.0..=1.0`.
    Opacity,
    /// Horizontal offset in logical points.
    X,
    /// Vertical offset in logical points.
    Y,
    /// Both scale factors. Reading it returns [`Prop::ScaleX`].
    Scale,
    /// Horizontal scale factor.
    ScaleX,
    /// Vertical scale factor.
    ScaleY,
    /// Rotation in radians, clockwise on screen.
    Rotate,
}

const OPACITY: usize = 0;
const X: usize = 1;
const Y: usize = 2;
const SCALE_X: usize = 3;
const SCALE_Y: usize = 4;
const ROTATE: usize = 5;
const FLOW_X: usize = 6;
const FLOW_Y: usize = 7;
const FLOW_W: usize = 8;
const FLOW_H: usize = 9;
const PRESS: usize = 10;
const LANES: usize = 11;

/// Values the enter and exit transitions return to.
const REST: [f32; 6] = [1.0, 0.0, 0.0, 1.0, 1.0, 0.0];

impl Prop {
    fn lanes(self) -> &'static [usize] {
        match self {
            Self::Opacity => &[OPACITY],
            Self::X => &[X],
            Self::Y => &[Y],
            Self::Scale => &[SCALE_X, SCALE_Y],
            Self::ScaleX => &[SCALE_X],
            Self::ScaleY => &[SCALE_Y],
            Self::Rotate => &[ROTATE],
        }
    }
}

fn eps(lane: usize) -> Eps {
    match lane {
        OPACITY => Eps {
            pos: 1.0e-3,
            vel: 1.0e-2,
        },
        X | Y | FLOW_X | FLOW_Y | FLOW_W | FLOW_H => Eps {
            pos: 0.01,
            vel: 0.1,
        },
        _ => Eps {
            pos: 1.0e-4,
            vel: 1.0e-3,
        },
    }
}

impl Component for Motion {}
impl Component for LayoutMotion {}
impl Component for PressScale {}

/// Animation state for one entity. Inserted by the functions in this module.
#[derive(Clone, Debug)]
pub struct Motion {
    lanes: [Lane; LANES],
    exiting: bool,
    active: bool,
    flow: Option<[f32; 2]>,
    size: Option<[f32; 2]>,
    /// Layout's own rect while [`Rect`] holds the presented size.
    resting: Option<Rect>,
    colors: [Option<[Lane; 4]>; 3],
    scripts: Vec<Script>,
    pressed: bool,
}

impl Motion {
    fn from_world(world: &World, entity: Entity) -> Self {
        let opacity = world.get::<Opacity>(entity).map_or(1.0, |o| o.0);
        let t = world.get::<Transform>(entity).copied().unwrap_or_default();
        let values = [
            opacity,
            t.translate[0],
            t.translate[1],
            t.scale[0],
            t.scale[1],
            t.rotate,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
        ];
        Self {
            lanes: std::array::from_fn(|lane| Lane::new(values[lane], eps(lane))),
            exiting: false,
            active: false,
            flow: None,
            size: None,
            resting: None,
            colors: [None; 3],
            scripts: Vec::new(),
            pressed: false,
        }
    }

    /// An [`exit`] is running; the entity despawns when it settles.
    pub fn is_exiting(&self) -> bool {
        self.exiting
    }

    fn cancel(&mut self, lanes: &[usize]) {
        self.scripts.retain(|s| !lanes.contains(&s.lane));
    }
}

/// Glide when layout moves this entity within its parent, and grow or
/// shrink when layout resizes it.
///
/// A jump in position or size is absorbed into an offset that animates back
/// to zero with the [`Anim`]. The first layout never animates and a scroll
/// offset on the parent is ignored. While the size moves, the entity's own
/// fill, border and clip are drawn at the presented size, anchored at the
/// top-left corner; children are laid out at the final size and the clip
/// reveals or hides them. Layout, input and hit testing see the final size.
/// A recycled virtual-list row would glide between items; leave it off there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutMotion(
    /// Curve for the glide.
    pub Anim,
);

/// Shrink this entity and its children to the factor while a primary press
/// that started on it is held and the pointer is over it, then spring back.
///
/// `PressScale(0.97)` suits buttons and cards. Moving out of the entity
/// while held springs it back; moving in again presses it again. The scale
/// multiplies [`Prop::Scale`], layout is untouched, and reduced motion turns
/// it off.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PressScale(
    /// Scale while pressed.
    pub f32,
);

impl Default for PressScale {
    fn default() -> Self {
        Self(0.97)
    }
}

impl Default for LayoutMotion {
    fn default() -> Self {
        Self(Anim::new(Curve::Spring(Spring::new(0.35, 0.0))))
    }
}

/// App-wide motion policy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionSettings {
    /// Offsets, scale, rotation, and layout glides change instantly. Opacity
    /// changes take a 0.18 s ease-in-out fade. The OS setting in
    /// [`crate::SystemAppearance::reduced_motion`] also turns this on.
    pub reduced_motion: bool,
    /// Multiplier on motion time. `0.25` is slow motion; `0.0` pauses.
    pub time_scale: f32,
}

impl Default for MotionSettings {
    fn default() -> Self {
        Self {
            reduced_motion: false,
            time_scale: 1.0,
        }
    }
}

/// Every running animation on `entity` has settled. `removed` means an
/// [`exit`] finished and the entity was despawned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MotionSettled {
    /// The entity whose motion settled.
    pub entity: Entity,
    /// The entity was despawned after its exit.
    pub removed: bool,
}

impl Event for MotionSettled {}

/// Motion time in seconds and the number of moving entities.
#[derive(Clone, Copy, Debug, Default)]
struct MotionClock {
    now: f64,
    seen: Duration,
    active: usize,
}

fn now(world: &mut World) -> f64 {
    if world.resource::<MotionClock>().is_none() {
        world.insert_resource(MotionClock::default());
    }
    world
        .resource::<MotionClock>()
        .map_or(0.0, |clock| clock.now)
}

fn settings(world: &World) -> MotionSettings {
    let mut settings = world
        .resource::<MotionSettings>()
        .copied()
        .unwrap_or_default();
    settings.reduced_motion |= crate::appearance::system_appearance(world).reduced_motion;
    settings
}

fn motion_mut(world: &mut World, entity: Entity) -> Option<&mut Motion> {
    if !world.is_alive(entity) {
        return None;
    }
    if !world.has::<Motion>(entity) {
        let motion = Motion::from_world(world, entity);
        world.insert(entity, motion);
    }
    world.get_mut::<Motion>(entity)
}

fn policy(world: &World, lanes: &[usize], anim: Anim) -> Anim {
    if !settings(world).reduced_motion {
        return anim;
    }
    if lanes == [OPACITY] {
        Anim {
            curve: Curve::tween(0.18, Easing::EaseInOut),
            velocity: None,
            ..anim
        }
    } else {
        Anim {
            curve: Curve::Instant,
            velocity: None,
            ..anim
        }
    }
}

/// Animate `prop` on `entity` to `target`. The motion starts from the value
/// and velocity presented now; [`Anim::velocity`] replaces the velocity.
pub fn animate(world: &mut World, entity: Entity, prop: Prop, target: f32, anim: impl Into<Anim>) {
    let anim = policy(world, prop.lanes(), anim.into());
    let now = now(world);
    if let Some(motion) = motion_mut(world, entity) {
        motion.cancel(prop.lanes());
        for &lane in prop.lanes() {
            motion.lanes[lane].animate(now, target, anim);
        }
        motion.active = true;
    }
}

/// Jump `prop` on `entity` to `value`, stopping its animation. Use this
/// while a gesture drives the value directly, then [`animate`] on release.
pub fn set(world: &mut World, entity: Entity, prop: Prop, value: f32) {
    if let Some(motion) = motion_mut(world, entity) {
        motion.cancel(prop.lanes());
        for &lane in prop.lanes() {
            motion.lanes[lane].set(value);
        }
        motion.active = true;
    }
}

/// Value of `prop` presented at the current motion time. `None` when the
/// entity has no [`Motion`].
pub fn value(world: &World, entity: Entity, prop: Prop) -> Option<f32> {
    let now = world
        .resource::<MotionClock>()
        .map_or(0.0, |clock| clock.now);
    let motion = world.get::<Motion>(entity)?;
    Some(motion.lanes[prop.lanes()[0]].sample(now).0)
}

/// Target of `prop`. `None` when the entity has no [`Motion`].
pub fn target(world: &World, entity: Entity, prop: Prop) -> Option<f32> {
    let motion = world.get::<Motion>(entity)?;
    Some(motion.lanes[prop.lanes()[0]].target)
}

/// Play `track` on `prop`, starting from the value presented now. Any
/// animation already on `prop` stops; a later [`animate`], [`set`] or
/// [`play`] on it stops the track. Tracks started on several properties in
/// the same frame share a start time, so their delays sequence them.
///
/// Under reduced motion a finite track goes straight to its final value
/// (opacity with a short fade) and an endless track holds still.
pub fn play(world: &mut World, entity: Entity, prop: Prop, track: &Keyframes) {
    let now = now(world);
    if settings(world).reduced_motion {
        let start = value(world, entity, prop)
            .unwrap_or_else(|| Motion::from_world(world, entity).lanes[prop.lanes()[0]].target);
        match track.final_value(start) {
            Some(end) => animate(world, entity, prop, end, Curve::Instant),
            None => {
                if let Some(motion) = motion_mut(world, entity) {
                    motion.cancel(prop.lanes());
                }
            }
        }
        return;
    }
    let Some(motion) = motion_mut(world, entity) else {
        return;
    };
    motion.cancel(prop.lanes());
    for &lane in prop.lanes() {
        let (start, _) = motion.lanes[lane].sample(now);
        if let Some(mut script) = track.script(lane, start, now) {
            script.advance(&mut motion.lanes[lane], now);
            motion.scripts.push(script);
        }
    }
    motion.active = true;
}

/// A [`play`] track is still running on `prop`.
pub fn is_playing(world: &World, entity: Entity, prop: Prop) -> bool {
    world
        .get::<Motion>(entity)
        .is_some_and(|m| m.scripts.iter().any(|s| prop.lanes().contains(&s.lane)))
}

fn current_color(world: &World, entity: Entity, prop: ColorProp) -> Option<Color> {
    match prop {
        ColorProp::Fill => world.get::<Color>(entity).copied(),
        ColorProp::Border => world.get::<Border>(entity).map(|b| b.color),
        ColorProp::Text => world.get::<Label>(entity).and_then(|l| {
            l.color
                .or_else(|| world.resource::<crate::Theme>().map(|t| t.text))
        }),
    }
}

fn color_eps(channel: usize) -> Eps {
    if channel == 3 {
        eps(OPACITY)
    } else {
        Eps {
            pos: 2.0e-4,
            vel: 2.0e-3,
        }
    }
}

/// Animate a colour on `entity` to `target`, blending in OKLab so hues
/// don't pass through grey. A missing fill starts from transparent `target`;
/// [`ColorProp::Border`] needs a [`Border`] and [`ColorProp::Text`] a
/// [`Label`]. Retargeting keeps the presented colour and velocity. The
/// component is written each frame while the colour moves, and left alone
/// once it settles. Under reduced motion the colour takes a short fade.
pub fn animate_color(
    world: &mut World,
    entity: Entity,
    prop: ColorProp,
    target: Color,
    anim: impl Into<Anim>,
) {
    let anim = policy(world, &[OPACITY], anim.into());
    let now = now(world);
    let current = match current_color(world, entity, prop) {
        Some(color) => color,
        None if prop == ColorProp::Fill => Color { a: 0.0, ..target },
        None => return,
    };
    let Some(motion) = motion_mut(world, entity) else {
        return;
    };
    let goal = color::to_lanes(target);
    let lanes = motion.colors[prop.index()].get_or_insert_with(|| {
        let from = color::to_lanes(current);
        std::array::from_fn(|c| Lane::new(from[c], color_eps(c)))
    });
    for (lane, goal) in lanes.iter_mut().zip(goal) {
        lane.animate(now, goal, anim);
    }
    motion.active = true;
}

/// Colour presented at the current motion time while it animates, or the
/// entity's colour otherwise.
pub fn color_value(world: &World, entity: Entity, prop: ColorProp) -> Option<Color> {
    let now = world
        .resource::<MotionClock>()
        .map_or(0.0, |clock| clock.now);
    match world
        .get::<Motion>(entity)
        .and_then(|m| m.colors[prop.index()])
    {
        Some(lanes) => Some(color::from_lanes(lanes.map(|l| l.sample(now).0))),
        None => current_color(world, entity, prop),
    }
}

/// Any animation on `entity` is still running after the last frame.
pub fn is_animating(world: &World, entity: Entity) -> bool {
    world.get::<Motion>(entity).is_some_and(|m| m.active)
}

/// Some entity is still moving and the window is visible. The desktop
/// runner keeps requesting frames while this holds.
pub fn animating(world: &World) -> bool {
    world
        .resource::<MotionClock>()
        .is_some_and(|clock| clock.active > 0)
        && crate::window::is_visible(world)
}

/// How an entity appears or leaves, relative to its resting values
/// (opacity 1, no offset, scale 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transition {
    /// Opacity at the hidden end.
    pub opacity: f32,
    /// Scale at the hidden end.
    pub scale: f32,
    /// Offset in logical points at the hidden end.
    pub offset: [f32; 2],
    /// Curve between the hidden end and rest.
    pub anim: Anim,
}

impl Transition {
    /// Fade only.
    pub const fn fade() -> Self {
        Self {
            opacity: 0.0,
            scale: 1.0,
            offset: [0.0, 0.0],
            anim: Anim::new(Curve::Spring(Spring::new(0.3, 0.0))),
        }
    }

    /// Fade and scale from `scale`.
    pub const fn scale(scale: f32) -> Self {
        Self {
            scale,
            ..Self::fade()
        }
    }

    /// Fade and slide from `dx`, `dy` points away.
    pub const fn slide(dx: f32, dy: f32) -> Self {
        Self {
            offset: [dx, dy],
            ..Self::fade()
        }
    }

    /// Same transition with another curve.
    pub const fn with_anim(mut self, anim: Anim) -> Self {
        self.anim = anim;
        self
    }

    fn hidden(&self) -> [(usize, f32); 5] {
        [
            (OPACITY, self.opacity),
            (X, self.offset[0]),
            (Y, self.offset[1]),
            (SCALE_X, self.scale),
            (SCALE_Y, self.scale),
        ]
    }
}

/// Animate `entity` in from `transition`'s hidden values. An entity in the
/// middle of an [`exit`] turns around from where it is instead.
pub fn enter(world: &mut World, entity: Entity, transition: Transition) {
    let now = now(world);
    let reduced = settings(world).reduced_motion;
    let Some(motion) = motion_mut(world, entity) else {
        return;
    };
    let revive = std::mem::take(&mut motion.exiting);
    motion.cancel(&transition.hidden().map(|(lane, _)| lane));
    for (lane, hidden) in transition.hidden() {
        let rest = REST[lane];
        if hidden == rest && !revive {
            continue;
        }
        if !revive {
            motion.lanes[lane].set(hidden);
        }
        let anim = enter_exit_anim(reduced, lane, transition.anim);
        motion.lanes[lane].animate(now, rest, anim);
    }
    motion.active = true;
}

/// Animate `entity` out to `transition`'s hidden values, then despawn it.
/// It stays in layout and hit testing until then; [`enter`] revives it.
pub fn exit(world: &mut World, entity: Entity, transition: Transition) {
    let now = now(world);
    let reduced = settings(world).reduced_motion;
    let Some(motion) = motion_mut(world, entity) else {
        return;
    };
    motion.exiting = true;
    motion.cancel(&transition.hidden().map(|(lane, _)| lane));
    for (lane, hidden) in transition.hidden() {
        let anim = enter_exit_anim(reduced, lane, transition.anim);
        motion.lanes[lane].animate(now, hidden, anim);
    }
    motion.active = true;
}

fn enter_exit_anim(reduced: bool, lane: usize, anim: Anim) -> Anim {
    if !reduced {
        anim
    } else if lane == OPACITY {
        Anim {
            curve: Curve::tween(0.18, Easing::EaseInOut),
            velocity: None,
            ..anim
        }
    } else {
        Anim {
            curve: Curve::Instant,
            velocity: None,
            ..anim
        }
    }
}

/// Advance motion time from [`crate::Time`]. Runs at the start of a frame,
/// so targets set during the frame start at this frame's time.
pub(crate) fn tick(world: &mut World) {
    let Some(time) = world.resource::<crate::Time>().copied() else {
        return;
    };
    let scale = settings(world).time_scale;
    let scale = if scale.is_finite() {
        f64::from(scale.max(0.0))
    } else {
        1.0
    };
    if world.resource::<MotionClock>().is_none() {
        world.insert_resource(MotionClock {
            seen: time.now,
            ..MotionClock::default()
        });
        return;
    }
    if let Some(clock) = world.resource_mut::<MotionClock>() {
        let delta = time.now.saturating_sub(clock.seen);
        clock.seen = time.now;
        clock.now += delta.as_secs_f64() * scale;
    }
}

/// Absorb layout moves, sample every animation, and write [`Opacity`] and
/// [`Transform`]. Runs after layout, before paint.
pub(crate) fn step(world: &mut World) {
    if world.resource::<MotionClock>().is_none() {
        return;
    }
    let now = now(world);
    let reduced = settings(world).reduced_motion;
    for (entity, LayoutMotion(anim)) in world
        .query::<LayoutMotion>()
        .into_iter()
        .map(|(e, m)| (e, *m))
        .collect::<Vec<_>>()
    {
        let position = flow_position(world, entity);
        let size = if world.has::<crate::tabs::Hidden>(entity) {
            None
        } else {
            world.get::<Rect>(entity).map(|r| [r.w, r.h])
        };
        let Some(motion) = motion_mut(world, entity) else {
            continue;
        };
        if let (Some(last), Some(size)) = (motion.size, size) {
            let delta = [last[0] - size[0], last[1] - size[1]];
            if !reduced && (delta[0] != 0.0 || delta[1] != 0.0) {
                motion.lanes[FLOW_W].absorb(now, delta[0], anim);
                motion.lanes[FLOW_H].absorb(now, delta[1], anim);
                motion.active = true;
            }
        }
        motion.size = size;
        if let (Some(last), Some(position)) = (motion.flow, position) {
            let delta = [last[0] - position[0], last[1] - position[1]];
            if !reduced && (delta[0] != 0.0 || delta[1] != 0.0) {
                motion.lanes[FLOW_X].absorb(now, delta[0], anim);
                motion.lanes[FLOW_Y].absorb(now, delta[1], anim);
                motion.active = true;
            }
        }
        motion.flow = position;
    }
    press(world, now, reduced);
    let mut active = 0;
    let mut settled = Vec::new();
    let mut removed = Vec::new();
    let mut colors = Vec::new();
    let mut sized = Vec::new();
    for entity in world.entities_with::<Motion>() {
        let origin = world
            .get::<Transform>(entity)
            .map_or(Transform::IDENTITY.origin, |t| t.origin);
        let Some(motion) = world.get_mut::<Motion>(entity) else {
            continue;
        };
        let mut scripts = std::mem::take(&mut motion.scripts);
        scripts.retain_mut(|s| s.advance(&mut motion.lanes[s.lane], now));
        let scripted = !scripts.is_empty();
        motion.scripts = scripts;
        let values: [f32; LANES] = std::array::from_fn(|lane| motion.lanes[lane].sample(now).0);
        let mut moving = motion
            .lanes
            .iter_mut()
            .fold(scripted, |any, lane| lane.settle(now) | any);
        for prop in ColorProp::ALL {
            let Some(lanes) = &mut motion.colors[prop.index()] else {
                continue;
            };
            let color = color::from_lanes(lanes.map(|l| l.sample(now).0));
            let still = lanes.iter_mut().fold(false, |any, l| l.settle(now) | any);
            colors.push((entity, prop, color));
            if still {
                moving = true;
            } else {
                motion.colors[prop.index()] = None;
            }
        }
        let grow = [values[FLOW_W], values[FLOW_H]];
        if grow[0].abs() > 1.0e-3 || grow[1].abs() > 1.0e-3 {
            sized.push((entity, grow));
        }
        let was = std::mem::replace(&mut motion.active, moving);
        let exiting = motion.exiting;
        if moving {
            active += 1;
        } else if exiting {
            removed.push(entity);
        } else if was {
            settled.push(entity);
        }
        world.insert(entity, Opacity(values[OPACITY]));
        world.insert(
            entity,
            Transform {
                translate: [values[X] + values[FLOW_X], values[Y] + values[FLOW_Y]],
                scale: [
                    values[SCALE_X] * values[PRESS],
                    values[SCALE_Y] * values[PRESS],
                ],
                rotate: values[ROTATE],
                origin,
            },
        );
    }
    for (entity, prop, color) in colors {
        match prop {
            ColorProp::Fill => world.insert(entity, color),
            ColorProp::Border => {
                if let Some(border) = world.get_mut::<Border>(entity) {
                    border.color = color;
                }
            }
            ColorProp::Text => {
                if let Some(label) = world.get_mut::<Label>(entity) {
                    label.color = Some(color);
                }
            }
        }
    }
    for (entity, grow) in sized {
        present_size(world, entity, grow);
    }
    if let Some(clock) = world.resource_mut::<MotionClock>() {
        clock.active = active;
    }
    for entity in settled {
        crate::event::send(
            world,
            MotionSettled {
                entity,
                removed: false,
            },
        );
    }
    for entity in removed {
        crate::hierarchy::despawn(world, entity);
        crate::event::send(
            world,
            MotionSettled {
                entity,
                removed: true,
            },
        );
    }
}

/// Put layout's own rects back before the frame's systems run, so input,
/// app code and layout never see a presented size.
pub(crate) fn restore(world: &mut World) {
    for entity in world.entities_with::<Motion>() {
        if let Some(rect) = world
            .get_mut::<Motion>(entity)
            .and_then(|m| m.resting.take())
        {
            world.insert(entity, rect);
        }
    }
}

// Draw `entity` at its layout size plus `grow`, top-left anchored, and clip
// it and its subtree to that size inside the parent's clip.
fn present_size(world: &mut World, entity: Entity, grow: [f32; 2]) {
    let Some(rect) = world.get::<Rect>(entity).copied() else {
        return;
    };
    let presented = Rect {
        w: (rect.w + grow[0]).max(0.0),
        h: (rect.h + grow[1]).max(0.0),
        ..rect
    };
    let outer = world
        .get::<Parent>(entity)
        .and_then(|p| world.get::<Clip>(p.entity).copied())
        .unwrap_or_else(|| {
            let viewport = world
                .resource::<crate::Viewport>()
                .copied()
                .unwrap_or_default();
            Clip {
                x: 0.0,
                y: 0.0,
                w: viewport.width,
                h: viewport.height,
            }
        });
    let clip = outer.intersect(Clip::from_rect(presented));
    if let Some(motion) = world.get_mut::<Motion>(entity) {
        motion.resting = Some(rect);
    }
    world.insert(entity, presented);
    world.insert(entity, clip);
    let mut stack = crate::hierarchy::children_of(world, entity);
    while let Some(child) = stack.pop() {
        if let Some(own) = world.get::<Clip>(child).copied() {
            world.insert(child, own.intersect(clip));
        }
        stack.extend(crate::hierarchy::children_of(world, child));
    }
}

// Drive the press lane of every `PressScale` entity from the pointer.
fn press(world: &mut World, now: f64, reduced: bool) {
    let Some(pointer) = world.resource::<crate::Input>().map(|i| i.pointer) else {
        return;
    };
    for (entity, PressScale(factor)) in world
        .query::<PressScale>()
        .into_iter()
        .map(|(e, p)| (e, *p))
        .collect::<Vec<_>>()
    {
        let starts = pointer.pressed
            && pointer.button == crate::PointerButton::Left
            && pointer
                .press_at
                .and_then(|[x, y]| crate::layout::hit_test(world, x, y))
                .is_some_and(|hit| within(world, hit, entity));
        let was = world.get::<Motion>(entity).is_some_and(|m| m.pressed);
        if !starts && !was {
            continue;
        }
        let held = pointer.down;
        let [x, y] = pointer.position;
        let inside = world
            .get::<Rect>(entity)
            .is_some_and(|r| x >= r.x && y >= r.y && x < r.x + r.w && y < r.y + r.h);
        let target = if held && inside && factor.is_finite() {
            factor.max(0.0)
        } else {
            1.0
        };
        let Some(motion) = motion_mut(world, entity) else {
            continue;
        };
        motion.pressed = held;
        let lane = &mut motion.lanes[PRESS];
        if reduced {
            if lane.target != 1.0 || lane.sample(now).0 != 1.0 {
                lane.set(1.0);
                motion.active = true;
            }
        } else if lane.target != target {
            let spring = if target < 1.0 {
                Spring::new(0.15, 0.0)
            } else {
                Spring::new(0.4, 0.35)
            };
            lane.animate(now, target, Anim::new(Curve::Spring(spring)));
            motion.active = true;
        }
    }
}

fn within(world: &World, mut entity: Entity, ancestor: Entity) -> bool {
    loop {
        if entity == ancestor {
            return true;
        }
        match world.get::<Parent>(entity) {
            Some(parent) => entity = parent.entity,
            None => return false,
        }
    }
}

// Layout position inside the parent's content, ignoring the parent's scroll.
fn flow_position(world: &World, entity: Entity) -> Option<[f32; 2]> {
    if world.has::<crate::tabs::Hidden>(entity) {
        return None;
    }
    let rect = world.get::<Rect>(entity)?;
    let Some(parent) = world.get::<Parent>(entity).map(|p| p.entity) else {
        return Some([rect.x, rect.y]);
    };
    let origin = world.get::<Rect>(parent).map_or([0.0, 0.0], |r| [r.x, r.y]);
    let scroll = world.get::<Scroll>(parent).map_or([0.0, 0.0], |s| s.offset);
    Some([
        rect.x - origin[0] - scroll[0],
        rect.y - origin[1] - scroll[1],
    ])
}
