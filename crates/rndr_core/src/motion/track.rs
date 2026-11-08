//! One animated scalar: a target plus an analytic segment that decays to it.
//!
//! The presented value is `target + offset(t)`. Retargeting samples the
//! current value and velocity and starts a new segment from there, so both
//! stay continuous. Every segment is evaluated in closed form at the frame
//! time; nothing is integrated step by step.

use super::curve::{Anim, Curve, Easing, Spring};

/// Below these the remaining offset and speed are invisible.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Eps {
    pub pos: f32,
    pub vel: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Rest,
    /// Holds `c[0]` until `t0`, then jumps to the target.
    Hold,
    /// ζ < 1: e^{−στ}(A cos ωd τ + B sin ωd τ). c = [A, B, C, D, σ, ωd].
    Under,
    /// ζ = 1: (A + Bτ) e^{−ωτ}. c = [A, B, C, D, ω, _].
    Critical,
    /// ζ > 1: c1 e^{r1 τ} + c2 e^{r2 τ}. c = [c1, c2, r1, r2, _, _].
    Over,
    /// Eased from Δ to 0 with carried velocity fading out. c = [Δ, d, 1/d, v, _, _].
    Tween,
}

/// Longest any segment runs before it counts as settled, in seconds.
const MAX_SETTLE: f64 = 120.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Segment {
    t0: f64,
    end: f64,
    c: [f32; 6],
    kind: Kind,
    easing: Easing,
}

impl Segment {
    pub const REST: Self = Self {
        t0: 0.0,
        end: f64::NEG_INFINITY,
        c: [0.0; 6],
        kind: Kind::Rest,
        easing: Easing::Linear,
    };

    pub fn start(curve: Curve, t0: f64, offset: f32, velocity: f32, eps: Eps) -> Self {
        match curve {
            Curve::Spring(spring) => Self::spring(spring, t0, offset, velocity, Some(eps)),
            Curve::Tween { duration, easing } => {
                Self::tween(duration, easing, t0, offset, velocity)
            }
            Curve::Instant => Self::REST,
        }
    }

    /// Hold `offset` until `t0`.
    pub fn hold(t0: f64, offset: f32) -> Self {
        if offset == 0.0 {
            return Self::REST;
        }
        Self {
            t0,
            end: t0,
            c: [offset, 0.0, 0.0, 0.0, 0.0, 0.0],
            kind: Kind::Hold,
            easing: Easing::Linear,
        }
    }

    /// Spring segment. `eps` bounds the settle time; `None` never settles.
    pub fn spring(s: Spring, t0: f64, offset: f32, v0: f32, eps: Option<Eps>) -> Self {
        if offset == 0.0 && v0 == 0.0 {
            return Self::REST;
        }
        let omega = s.omega();
        let zeta = s.zeta();
        let a = offset;
        let (kind, c) = if (zeta - 1.0).abs() < 1.0e-4 {
            let b = v0 + omega * a;
            (
                Kind::Critical,
                [a, b, b - omega * a, -omega * b, omega, 0.0],
            )
        } else if zeta < 1.0 {
            let sigma = zeta * omega;
            let wd = omega * (1.0 - zeta * zeta).sqrt();
            let b = (v0 + sigma * a) / wd;
            (
                Kind::Under,
                [a, b, b * wd - sigma * a, -a * wd - sigma * b, sigma, wd],
            )
        } else {
            let root = (zeta * zeta - 1.0).sqrt();
            let r1 = -omega * (zeta - root);
            let r2 = -omega * (zeta + root);
            let c1 = (v0 - r2 * a) / (r1 - r2);
            (Kind::Over, [c1, a - c1, r1, r2, 0.0, 0.0])
        };
        let mut segment = Self {
            t0,
            end: f64::INFINITY,
            c,
            kind,
            easing: Easing::Linear,
        };
        if let Some(eps) = eps {
            segment.end = t0 + segment.settle_after(eps);
        }
        segment
    }

    fn tween(duration: f32, easing: Easing, t0: f64, offset: f32, velocity: f32) -> Self {
        if duration.is_nan() || duration <= 0.0 || (offset == 0.0 && velocity == 0.0) {
            return Self::REST;
        }
        Self {
            t0,
            end: t0 + f64::from(duration),
            c: [offset, duration, 1.0 / duration, velocity, 0.0, 0.0],
            kind: Kind::Tween,
            easing,
        }
    }

    /// Offset from the target and velocity at absolute time `now`.
    pub fn sample(&self, now: f64) -> (f32, f32) {
        match self.kind {
            Kind::Rest => return (0.0, 0.0),
            Kind::Hold => {
                return if now < self.t0 {
                    (self.c[0], 0.0)
                } else {
                    (0.0, 0.0)
                }
            }
            Kind::Tween => {}
            _ if now >= self.end => return (0.0, 0.0),
            _ => {}
        }
        let tau = (now - self.t0).max(0.0) as f32;
        let c = &self.c;
        match self.kind {
            Kind::Under => {
                let e = (-c[4] * tau).exp();
                let (s, co) = (c[5] * tau).sin_cos();
                (e * (c[0] * co + c[1] * s), e * (c[2] * co + c[3] * s))
            }
            Kind::Critical => {
                let e = (-c[4] * tau).exp();
                ((c[0] + c[1] * tau) * e, (c[2] + c[3] * tau) * e)
            }
            Kind::Over => {
                let e1 = (c[2] * tau).exp();
                let e2 = (c[3] * tau).exp();
                (c[0] * e1 + c[1] * e2, c[0] * c[2] * e1 + c[1] * c[3] * e2)
            }
            Kind::Tween => {
                let tau = tau.min(c[1]);
                let u = (tau * c[2]).min(1.0);
                let (y, dy) = self.easing.eval_with_slope(u);
                let w = 1.0 - u;
                let carry = tau * w * w;
                let dcarry = w * (1.0 - 3.0 * u);
                (
                    c[0] * (1.0 - y) + c[3] * carry,
                    -c[0] * dy * c[2] + c[3] * dcarry,
                )
            }
            Kind::Rest | Kind::Hold => (0.0, 0.0),
        }
    }

    /// No motion at or after `now`.
    pub fn settled(&self, now: f64) -> bool {
        self.kind == Kind::Rest || now >= self.end
    }

    /// Seconds after `t0` until offset and velocity stay below `eps`.
    fn settle_after(&self, eps: Eps) -> f64 {
        let c = self.c.map(f64::from);
        let (ep, ev) = (f64::from(eps.pos), f64::from(eps.vel));
        let t = match self.kind {
            Kind::Under => {
                let tp = (c[0].hypot(c[1]) / ep).ln();
                let tv = (c[2].hypot(c[3]) / ev).ln();
                tp.max(tv) / c[4]
            }
            Kind::Critical => {
                let omega = c[4];
                // (a + bτ) e^{−ωτ} has one extremum; the tail maximum is at τ or there.
                let tail = |a: f64, b: f64, t: f64| {
                    let at = |t: f64| ((a + b * t) * (-omega * t).exp()).abs();
                    let ext = if b == 0.0 { -1.0 } else { 1.0 / omega - a / b };
                    at(t).max(if ext > t { at(ext) } else { 0.0 })
                };
                bisect(
                    |t| tail(c[0], c[1], t) <= ep && tail(c[2], c[3], t) <= ev,
                    1.0 / omega,
                )
            }
            Kind::Over => {
                let env = |t: f64| c[0].abs() * (c[2] * t).exp() + c[1].abs() * (c[3] * t).exp();
                let envv = |t: f64| {
                    (c[0] * c[2]).abs() * (c[2] * t).exp() + (c[1] * c[3]).abs() * (c[3] * t).exp()
                };
                bisect(|t| env(t) <= ep && envv(t) <= ev, -1.0 / c[2])
            }
            _ => 0.0,
        };
        if t.is_finite() {
            t.clamp(0.0, MAX_SETTLE)
        } else {
            MAX_SETTLE
        }
    }
}

/// Smallest `t >= 0` for a predicate that stays true once true.
fn bisect(ok: impl Fn(f64) -> bool, scale: f64) -> f64 {
    if ok(0.0) {
        return 0.0;
    }
    let mut hi = scale.max(1.0e-3);
    while !ok(hi) {
        hi *= 2.0;
        if hi > MAX_SETTLE {
            return MAX_SETTLE;
        }
    }
    let mut lo = 0.0;
    for _ in 0..40 {
        let mid = 0.5 * (lo + hi);
        if ok(mid) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    hi
}

/// A target and the segment moving toward it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Lane {
    pub target: f32,
    segment: Segment,
    eps: Eps,
}

impl Lane {
    pub fn new(value: f32, eps: Eps) -> Self {
        Self {
            target: value,
            segment: Segment::REST,
            eps,
        }
    }

    /// Presented value and velocity at `now`.
    pub fn sample(&self, now: f64) -> (f32, f32) {
        let (offset, velocity) = self.segment.sample(now);
        (self.target + offset, velocity)
    }

    /// Jump to `value` and stop.
    pub fn set(&mut self, value: f32) {
        self.target = value;
        self.segment = Segment::REST;
    }

    /// Move to `target` from the value and velocity presented at `now`.
    /// An unchanged target with no new velocity keeps the running motion.
    pub fn animate(&mut self, now: f64, target: f32, anim: Anim) {
        if target == self.target && anim.velocity.is_none() {
            return;
        }
        self.start_from(now, self.sample(now), target, anim);
    }

    /// Add `by` to the presented value at `now`, then move back to the
    /// target. Used when layout moves an entity and its offset absorbs the jump.
    pub fn absorb(&mut self, now: f64, by: f32, anim: Anim) {
        let (value, velocity) = self.sample(now);
        self.start_from(now, (value + by, velocity), self.target, anim);
    }

    /// Move to `target` from the value and velocity presented at `t0`,
    /// even when the target is unchanged. Keyframe steps start here.
    pub fn step_to(&mut self, t0: f64, target: f32, anim: Anim) {
        self.start_from(t0, self.sample(t0), target, anim);
    }

    fn start_from(&mut self, now: f64, (value, velocity): (f32, f32), target: f32, anim: Anim) {
        let velocity = anim.velocity.unwrap_or(velocity);
        let delay = if anim.delay.is_finite() {
            f64::from(anim.delay.max(0.0))
        } else {
            0.0
        };
        let t0 = now + delay;
        self.target = target;
        self.segment = match anim.curve {
            Curve::Instant if delay > 0.0 => Segment::hold(t0, value - target),
            curve => Segment::start(curve, t0, value - target, velocity, self.eps),
        };
    }

    /// Still moving at `now`. A settled segment is dropped.
    pub fn settle(&mut self, now: f64) -> bool {
        if self.segment.settled(now) {
            self.segment = Segment::REST;
            false
        } else {
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: Eps = Eps {
        pos: 1.0e-3,
        vel: 1.0e-2,
    };

    // Semi-implicit reference integration of x'' = -k (x - target) - c x'.
    fn integrate(s: Spring, x0: f32, v0: f32, target: f32, t: f32) -> f32 {
        let (k, c) = (s.omega() * s.omega(), 2.0 * s.zeta() * s.omega());
        let steps = 200_000;
        let dt = f64::from(t) / f64::from(steps);
        let (mut x, mut v) = (f64::from(x0), f64::from(v0));
        for _ in 0..steps {
            let a = -f64::from(k) * (x - f64::from(target)) - f64::from(c) * v;
            v += a * dt;
            x += v * dt;
        }
        x as f32
    }

    #[test]
    fn closed_form_matches_integration_in_every_regime() {
        for s in [
            Spring::new(0.4, 0.3),
            Spring::new(0.4, 0.0),
            Spring::new(0.4, -0.4),
        ] {
            for t in [0.05, 0.2, 0.6] {
                let (x, _) = s.sample(0.0, 3.0, 100.0, t);
                let reference = integrate(s, 0.0, 3.0, 100.0, t);
                assert!(
                    (x - reference).abs() < 0.05,
                    "{s:?} at {t}: {x} vs {reference}"
                );
            }
        }
    }

    #[test]
    fn settles_to_the_target_within_the_bound() {
        let mut lane = Lane::new(0.0, EPS);
        lane.animate(0.0, 100.0, Spring::SNAPPY.into());
        assert!(lane.settle(0.1));
        let mut t = 0.0;
        while lane.settle(t) {
            t += 1.0 / 120.0;
            assert!(t < 10.0);
        }
        assert_eq!(lane.sample(t), (100.0, 0.0));
    }

    #[test]
    fn retarget_keeps_value_and_velocity() {
        let mut lane = Lane::new(0.0, EPS);
        lane.animate(0.0, 100.0, Spring::BOUNCY.into());
        let before = lane.sample(0.12);
        lane.animate(0.12, -40.0, Curve::spring(0.3, 0.0).into());
        let after = lane.sample(0.12);
        assert!((before.0 - after.0).abs() < 1.0e-3);
        assert!((before.1 - after.1).abs() < 1.0e-2);
        let mut tween = Lane::new(0.0, EPS);
        tween.animate(0.0, 10.0, Spring::SMOOTH.into());
        let before = tween.sample(0.1);
        tween.animate(0.1, 0.0, Curve::tween(0.25, Easing::EaseInOut).into());
        let after = tween.sample(0.1);
        assert!((before.0 - after.0).abs() < 1.0e-3);
        assert!((before.1 - after.1).abs() < 1.0e-2);
        assert_eq!(tween.sample(0.35), (0.0, 0.0));
    }

    #[test]
    fn delay_holds_then_moves() {
        let mut lane = Lane::new(1.0, EPS);
        lane.animate(
            0.0,
            0.0,
            Anim::new(Curve::tween(0.2, Easing::Linear)).delay(0.1),
        );
        assert_eq!(lane.sample(0.05).0, 1.0);
        assert!((lane.sample(0.2).0 - 0.5).abs() < 1.0e-4);
        let mut instant = Lane::new(1.0, EPS);
        instant.animate(0.0, 0.0, Anim::new(Curve::Instant).delay(0.1));
        assert_eq!(instant.sample(0.05).0, 1.0);
        assert_eq!(instant.sample(0.11).0, 0.0);
    }

    #[test]
    fn release_velocity_seeds_the_spring() {
        let mut lane = Lane::new(0.0, EPS);
        lane.animate(0.0, 0.0, Anim::new(Spring::SMOOTH.into()).velocity(500.0));
        assert!(lane.sample(0.05).0 > 5.0);
        assert!(lane.settle(0.05));
    }

    #[test]
    fn easing_endpoints_and_monotonic() {
        for easing in [Easing::EaseIn, Easing::EaseOut, Easing::EaseInOut] {
            assert!(easing.eval(0.0).abs() < 1.0e-5);
            assert!((easing.eval(1.0) - 1.0).abs() < 1.0e-5);
            let mut last = 0.0;
            for i in 1..=20 {
                let y = easing.eval(i as f32 / 20.0);
                assert!(y >= last - 1.0e-6);
                last = y;
            }
        }
        assert!((Easing::EaseInOut.eval(0.5) - 0.5).abs() < 1.0e-4);
    }
}
