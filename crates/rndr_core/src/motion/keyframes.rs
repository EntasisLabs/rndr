//! Keyframe tracks: a list of steps one property plays in order.

use std::sync::Arc;

use super::curve::{Anim, Curve, Easing, Spring};
use super::track::Lane;

/// One step of a [`Keyframes`] track.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Key {
    pub value: f32,
    pub curve: Curve,
    /// Seconds before the next step starts. A spring step hands its value
    /// and velocity to the next step when this runs out.
    pub slot: f32,
}

/// How many times a [`Keyframes`] track plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Repeat {
    /// Play this many passes. `Times(1)` plays once.
    Times(u32),
    /// Play until another animation on the property replaces it.
    Forever,
}

/// Steps for one property, played with [`super::play`].
///
/// Each step moves from wherever the previous step left the value. A spring
/// step runs for its [`Spring::duration`] (or the time given to
/// [`Self::spring_for`]) before the next step starts from its value and
/// velocity, so a chain of springs stays smooth.
///
/// ```
/// use rndr_core::motion::{Easing, Keyframes, Spring};
///
/// let pulse = Keyframes::new()
///     .eased(1.2, 0.15, Easing::EaseOut)
///     .spring(1.0, Spring::BOUNCY)
///     .delay(0.1)
///     .repeat(3);
/// assert_eq!(pulse.pass_duration(), 0.15 + Spring::BOUNCY.duration);
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct Keyframes {
    keys: Vec<Key>,
    delay: f32,
    repeat: Repeat,
    alternate: bool,
}

impl Default for Keyframes {
    fn default() -> Self {
        Self::new()
    }
}

fn seconds(s: f32) -> f32 {
    if s.is_finite() {
        s.max(0.0)
    } else {
        0.0
    }
}

impl Keyframes {
    /// An empty track that plays once with no delay.
    pub fn new() -> Self {
        Self {
            keys: Vec::new(),
            delay: 0.0,
            repeat: Repeat::Times(1),
            alternate: false,
        }
    }

    /// Move to `value` at constant speed over `duration` seconds.
    pub fn linear(self, value: f32, duration: f32) -> Self {
        self.eased(value, duration, Easing::Linear)
    }

    /// Move to `value` over `duration` seconds along `easing`.
    pub fn eased(mut self, value: f32, duration: f32, easing: Easing) -> Self {
        let duration = seconds(duration);
        self.keys.push(Key {
            value,
            curve: Curve::tween(duration, easing),
            slot: duration,
        });
        self
    }

    /// Spring to `value`; the next step starts after [`Spring::duration`].
    pub fn spring(self, value: f32, spring: Spring) -> Self {
        let slot = spring.duration;
        self.spring_for(value, slot, spring)
    }

    /// Spring to `value`; the next step starts after `slot` seconds.
    pub fn spring_for(mut self, value: f32, slot: f32, spring: Spring) -> Self {
        self.keys.push(Key {
            value,
            curve: Curve::Spring(spring),
            slot: seconds(slot),
        });
        self
    }

    /// Jump to `value` with no motion.
    pub fn jump(mut self, value: f32) -> Self {
        self.keys.push(Key {
            value,
            curve: Curve::Instant,
            slot: 0.0,
        });
        self
    }

    /// Wait `seconds` before the first step.
    pub fn delay(mut self, seconds: f32) -> Self {
        self.delay = self::seconds(seconds);
        self
    }

    /// Play `passes` times in total. Zero is treated as one.
    pub fn repeat(mut self, passes: u32) -> Self {
        self.repeat = Repeat::Times(passes.max(1));
        self
    }

    /// Play until replaced. A track whose steps take no time plays once.
    pub fn forever(mut self) -> Self {
        self.repeat = Repeat::Forever;
        self
    }

    /// Play every second pass backwards, back to where the track started.
    pub fn alternate(mut self) -> Self {
        self.alternate = true;
        self
    }

    /// Seconds one pass takes, not counting the delay.
    pub fn pass_duration(&self) -> f32 {
        self.keys.iter().map(|k| k.slot).sum()
    }

    /// How many passes this track plays.
    pub fn repeats(&self) -> Repeat {
        self.repeat
    }

    /// Value the track rests at after its last pass, given the value it
    /// started from. `None` for an endless track.
    pub fn final_value(&self, start: f32) -> Option<f32> {
        let Repeat::Times(passes) = self.repeat else {
            return None;
        };
        let last = self.keys.last().map_or(start, |k| k.value);
        Some(if self.alternate && passes % 2 == 0 {
            start
        } else {
            last
        })
    }

    pub(super) fn script(&self, lane: usize, start: f32, now: f64) -> Option<Script> {
        if self.keys.is_empty() {
            return None;
        }
        let forward: Arc<[Key]> = self.keys.clone().into();
        // Backwards pass: undo each step with its own curve, ending at `start`.
        let backward: Arc<[Key]> = (0..self.keys.len())
            .rev()
            .map(|i| Key {
                value: if i == 0 {
                    start
                } else {
                    self.keys[i - 1].value
                },
                ..self.keys[i]
            })
            .collect();
        let passes = match self.repeat {
            Repeat::Times(n) => Some(n.max(1)),
            Repeat::Forever if self.pass_duration() > 0.0 => None,
            Repeat::Forever => Some(1),
        };
        Some(Script {
            lane,
            forward,
            backward,
            alternate: self.alternate,
            reverse: false,
            next: 0,
            passes,
            at: now + f64::from(self.delay),
        })
    }
}

/// A playing [`Keyframes`] track on one lane of a [`super::Motion`].
#[derive(Clone, Debug)]
pub(super) struct Script {
    pub lane: usize,
    forward: Arc<[Key]>,
    backward: Arc<[Key]>,
    alternate: bool,
    reverse: bool,
    next: usize,
    /// Passes left including this one; `None` plays forever.
    passes: Option<u32>,
    /// Motion time the next step starts.
    at: f64,
}

impl Script {
    /// Start every step due by `now` at its own start time, so the value
    /// at a given time does not depend on the frame rate. `false` once the
    /// last step has started; the lane finishes that step by itself.
    pub fn advance(&mut self, lane: &mut Lane, now: f64) -> bool {
        while self.at <= now {
            let keys = if self.reverse {
                &self.backward
            } else {
                &self.forward
            };
            let Some(&key) = keys.get(self.next) else {
                match &mut self.passes {
                    Some(1) => return false,
                    Some(n) => *n -= 1,
                    None => {}
                }
                self.next = 0;
                self.reverse = self.alternate && !self.reverse;
                continue;
            };
            match key.curve {
                Curve::Instant => lane.set(key.value),
                // A tween step starts at rest; a spring takes over the velocity.
                curve @ Curve::Tween { .. } => {
                    lane.step_to(self.at, key.value, Anim::new(curve).velocity(0.0));
                }
                curve => lane.step_to(self.at, key.value, Anim::new(curve)),
            }
            self.next += 1;
            self.at += f64::from(key.slot);
        }
        true
    }
}
