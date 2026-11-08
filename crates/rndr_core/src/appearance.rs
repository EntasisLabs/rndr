//! The desktop's appearance and accessibility preferences.
//!
//! The desktop runner writes [`SystemAppearance`] when a window opens, when
//! it gains focus and whenever the OS reports a change, and sends
//! [`SystemAppearanceChanged`] when a value differs. Headless runs keep
//! [`SystemAppearance::default`] unless a test publishes one.
//!
//! The motion runtime reduces motion when either
//! [`crate::motion::MotionSettings::reduced_motion`] or
//! [`SystemAppearance::reduced_motion`] is set.

use crate::event::{send, Event};
use crate::style::Color;
use crate::world::World;

/// What the desktop prefers, as far as this platform reports it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SystemAppearance {
    /// `Some(true)` for a dark appearance, `Some(false)` for light, `None`
    /// when the desktop states no preference.
    pub dark: Option<bool>,
    /// The user's accent colour, in sRGB, when the platform exposes one.
    pub accent: Option<Color>,
    /// The user asked the OS to reduce or turn off animation.
    pub reduced_motion: bool,
}

/// The desktop's preferences changed. Carries the new values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SystemAppearanceChanged(pub SystemAppearance);

impl Event for SystemAppearanceChanged {}

/// The last published preferences, or the default before any.
pub fn system_appearance(world: &World) -> SystemAppearance {
    world
        .resource::<SystemAppearance>()
        .copied()
        .unwrap_or_default()
}

/// Store `appearance`. Sends [`SystemAppearanceChanged`] and returns `true`
/// when it differs from the stored value.
pub fn publish_system_appearance(world: &mut World, appearance: SystemAppearance) -> bool {
    if world.resource::<SystemAppearance>() == Some(&appearance) {
        return false;
    }
    world.insert_resource(appearance);
    send(world, SystemAppearanceChanged(appearance));
    true
}
