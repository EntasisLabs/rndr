//! Curves: springs, eased tweens, and instant changes.

/// A spring with mass 1, tuned by perceived duration and bounce.
///
/// `ω = 2π / duration`. A bounce of 0 is critically damped, above 0
/// overshoots, and below 0 is overdamped (`ζ = 1 / (1 + bounce)`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spring {
    /// Perceived duration in seconds (the undamped period). Positive.
    pub duration: f32,
    /// Overshoot, in `-1.0 < bounce < 1.0`. Zero does not overshoot.
    pub bounce: f32,
}

impl Spring {
    /// Smooth settle with no overshoot.
    pub const SMOOTH: Self = Self::new(0.5, 0.0);
    /// Quick, with a little overshoot.
    pub const SNAPPY: Self = Self::new(0.3, 0.15);
    /// Visible overshoot.
    pub const BOUNCY: Self = Self::new(0.5, 0.3);

    /// Spring with `duration` seconds and `bounce`.
    ///
    /// # Panics
    ///
    /// Panics when `duration` is not positive or `bounce` is outside `(-1, 1)`.
    pub const fn new(duration: f32, bounce: f32) -> Self {
        assert!(
            duration > 0.0 && duration < 1.0e4,
            "spring duration must be positive seconds"
        );
        assert!(
            bounce > -1.0 && bounce < 1.0,
            "spring bounce must be in (-1, 1)"
        );
        Self { duration, bounce }
    }

    /// Angular frequency in radians per second.
    pub fn omega(self) -> f32 {
        std::f32::consts::TAU / self.duration
    }

    /// Damping ratio.
    pub fn zeta(self) -> f32 {
        if self.bounce >= 0.0 {
            1.0 - self.bounce
        } else {
            1.0 / (1.0 + self.bounce)
        }
    }

    /// Position and velocity `t` seconds after leaving `from` with
    /// `velocity` toward `target`. Closed form in every damping regime.
    pub fn sample(self, from: f32, velocity: f32, target: f32, t: f32) -> (f32, f32) {
        let segment = super::track::Segment::spring(self, 0.0, from - target, velocity, None);
        let (offset, v) = segment.sample(f64::from(t));
        (target + offset, v)
    }
}

/// Tween timing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Easing {
    /// Constant speed.
    Linear,
    /// CSS `ease-in`.
    EaseIn,
    /// CSS `ease-out`.
    EaseOut,
    /// CSS `ease-in-out`.
    EaseInOut,
    /// CSS `cubic-bezier(x1, y1, x2, y2)`. `x1` and `x2` are clamped to `0..=1`.
    Cubic(f32, f32, f32, f32),
}

impl Easing {
    fn control(self) -> Option<[f32; 4]> {
        match self {
            Self::Linear => None,
            Self::EaseIn => Some([0.42, 0.0, 1.0, 1.0]),
            Self::EaseOut => Some([0.0, 0.0, 0.58, 1.0]),
            Self::EaseInOut => Some([0.42, 0.0, 0.58, 1.0]),
            Self::Cubic(x1, y1, x2, y2) => Some([x1.clamp(0.0, 1.0), y1, x2.clamp(0.0, 1.0), y2]),
        }
    }

    /// Eased progress at `u` in `0..=1`.
    pub fn eval(self, u: f32) -> f32 {
        self.eval_with_slope(u).0
    }

    /// Eased progress and its slope with respect to `u`.
    pub(crate) fn eval_with_slope(self, u: f32) -> (f32, f32) {
        let u = u.clamp(0.0, 1.0);
        let Some([x1, y1, x2, y2]) = self.control() else {
            return (u, 1.0);
        };
        let bez = |a: f32, b: f32, s: f32| {
            let m = 1.0 - s;
            3.0 * m * m * s * a + 3.0 * m * s * s * b + s * s * s
        };
        let dbez = |a: f32, b: f32, s: f32| {
            let m = 1.0 - s;
            3.0 * m * m * a + 6.0 * m * s * (b - a) + 3.0 * s * s * (1.0 - b)
        };
        // Solve x(s) = u: Newton from u, then bisection if it stalls.
        let mut s = u;
        let mut solved = false;
        for _ in 0..8 {
            let err = bez(x1, x2, s) - u;
            if err.abs() < 1.0e-6 {
                solved = true;
                break;
            }
            let d = dbez(x1, x2, s);
            if d.abs() < 1.0e-6 {
                break;
            }
            s = (s - err / d).clamp(0.0, 1.0);
        }
        if !solved {
            let (mut lo, mut hi) = (0.0f32, 1.0f32);
            for _ in 0..32 {
                s = 0.5 * (lo + hi);
                if bez(x1, x2, s) < u {
                    lo = s;
                } else {
                    hi = s;
                }
            }
        }
        let dx = dbez(x1, x2, s);
        let dy = dbez(y1, y2, s);
        let slope = if dx.abs() < 1.0e-6 { 0.0 } else { dy / dx };
        (bez(y1, y2, s), slope)
    }
}

/// How a value moves to its target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Curve {
    /// Jump to the target.
    Instant,
    /// Spring physics. Retargeting keeps position and velocity.
    Spring(Spring),
    /// Timed curve. Velocity carried in from an interrupted motion fades out
    /// over the tween.
    Tween {
        /// Seconds. A non-positive duration is instant.
        duration: f32,
        /// Timing function.
        easing: Easing,
    },
}

impl Curve {
    /// [`Curve::Spring`] with `duration` seconds and `bounce`.
    pub const fn spring(duration: f32, bounce: f32) -> Self {
        Self::Spring(Spring::new(duration, bounce))
    }

    /// [`Curve::Tween`] over `duration` seconds.
    pub const fn tween(duration: f32, easing: Easing) -> Self {
        Self::Tween { duration, easing }
    }
}

impl From<Spring> for Curve {
    fn from(spring: Spring) -> Self {
        Self::Spring(spring)
    }
}

/// A [`Curve`] plus when it starts and the velocity it starts with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anim {
    /// Shape of the motion.
    pub curve: Curve,
    /// Seconds before the motion starts. The value holds until then.
    pub delay: f32,
    /// Starting velocity in units per second, replacing the current one.
    /// A gesture release passes its measured velocity here.
    pub velocity: Option<f32>,
}

impl Anim {
    /// Start `curve` now with the current velocity.
    pub const fn new(curve: Curve) -> Self {
        Self {
            curve,
            delay: 0.0,
            velocity: None,
        }
    }

    /// Same animation, starting `seconds` later.
    pub const fn delay(mut self, seconds: f32) -> Self {
        self.delay = seconds;
        self
    }

    /// Same animation, starting with `velocity` units per second.
    pub const fn velocity(mut self, velocity: f32) -> Self {
        self.velocity = Some(velocity);
        self
    }
}

impl From<Curve> for Anim {
    fn from(curve: Curve) -> Self {
        Self::new(curve)
    }
}

impl From<Spring> for Anim {
    fn from(spring: Spring) -> Self {
        Self::new(Curve::Spring(spring))
    }
}
