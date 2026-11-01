//! Contacts for one frame: mouse, touch, pen, and eraser.
//!
//! [`Input::pointer`](crate::Input::pointer) stays the primary contact, so a
//! widget that only reads that field does not change. Every other contact
//! lives on [`Pointers`]. A contact keeps the samples that arrived since the
//! previous frame, not only the newest one.
//!
//! Pen-priority over palm touches is left to the app. Call
//! [`Pointers::set_primary`] when a pen should drive widgets that still read
//! [`Input::pointer`](crate::Input::pointer). Until then the mouse contact
//! stays primary when it exists.
//!
//! # Identifiers
//!
//! * [`MOUSE_ID`](crate::MOUSE_ID) is the mouse.
//! * [`PEN_ID`] is the tablet contact written by the macOS shim.
//! * [`touch_id`] maps a platform finger id so it cannot collide with the mouse.
//!
//! [`Gesture`] is one pinch, rotation, and pan. The desktop runner emits one
//! from a platform gesture, and [`GestureState`] emits more from two touch
//! contacts.

use std::f32::consts::PI;
use std::time::Duration;

use crate::event::Event;
use crate::input::{Pointer, PointerButton, MOUSE_ID};

/// Tablet contact written by the macOS pen shim.
///
/// The bit does not overlap [`MOUSE_ID`](crate::MOUSE_ID) or [`touch_id`].
pub const PEN_ID: u64 = 1 << 62;

/// High bit set on ids from [`touch_id`].
pub const TOUCH_ID_BIT: u64 = 1 << 63;

/// Platform finger id with [`TOUCH_ID_BIT`] set, so it is never [`MOUSE_ID`](crate::MOUSE_ID).
pub const fn touch_id(platform_id: u64) -> u64 {
    platform_id | TOUCH_ID_BIT
}

/// What produced a contact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerKind {
    /// A mouse, or a contact the platform reported as a mouse.
    Mouse,
    /// A finger. winit 0.30 reports Windows pen input as touch.
    Touch,
    /// A stylus tip.
    Pen,
    /// The eraser end of a stylus.
    Eraser,
}

/// Buttons held on a contact. Bits match [`PointerButton::Other`] numbers:
/// left is 0, right is 1, middle is 2, back is 8, forward is 9.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PointerButtons {
    bits: u16,
}

impl PointerButtons {
    /// No buttons.
    pub const fn new() -> Self {
        Self { bits: 0 }
    }

    /// The bit for `button`, or `None` when the number does not fit in 16 bits.
    pub const fn bit(button: PointerButton) -> Option<u16> {
        let index = match button {
            PointerButton::Left => 0,
            PointerButton::Right => 1,
            PointerButton::Middle => 2,
            PointerButton::Other(code) => code,
        };
        if index < 16 {
            Some(1u16 << index)
        } else {
            None
        }
    }

    /// Set or clear `button`. A number that does not fit is ignored.
    pub fn set(&mut self, button: PointerButton, down: bool) {
        let Some(bit) = Self::bit(button) else {
            return;
        };
        if down {
            self.bits |= bit;
        } else {
            self.bits &= !bit;
        }
    }

    /// `true` when `button` is held. An unrepresentable number is never held.
    pub const fn contains(self, button: PointerButton) -> bool {
        match Self::bit(button) {
            Some(bit) => self.bits & bit != 0,
            None => false,
        }
    }

    /// `true` when no stored button is held.
    pub const fn is_empty(self) -> bool {
        self.bits == 0
    }
}

/// One sample inside a contact, in arrival order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerSample {
    /// Position in logical points, origin top-left.
    pub position: [f32; 2],
    /// Pressure in `0.0..=1.0`, or `None` when the device did not report it.
    pub pressure: Option<f32>,
    /// Tilt along X and Y, in degrees. `None` when the device did not report it.
    ///
    /// Positive X tilts toward the right. Positive Y tilts toward the user.
    pub tilt: Option<[f32; 2]>,
    /// Rotation around the contact axis, in degrees. `None` when unreported.
    pub twist: Option<f32>,
    /// Time since an arbitrary runner epoch. Tests may use any origin.
    pub time: Duration,
}

/// How this update relates to the previous one for the same id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContactPhase {
    /// The contact went down, or a finger id was reused.
    Start,
    /// The contact moved, or a button changed while it stayed down.
    Move,
    /// The contact lifted.
    End,
    /// The platform stopped tracking the contact.
    Cancel,
}

/// One platform update applied by [`Pointers::apply`].
#[derive(Clone, Debug, PartialEq)]
pub struct ContactUpdate {
    /// Stable id. See the module docs for the reserved values.
    pub id: u64,
    /// Device kind. A later update may change it, for example pen to eraser.
    pub kind: PointerKind,
    /// Position in logical points.
    pub position: [f32; 2],
    /// Pressure in `0.0..=1.0`. Non-finite values are stored as `None`.
    pub pressure: Option<f32>,
    /// Tilt in degrees. Non-finite components are stored as `None`.
    pub tilt: Option<[f32; 2]>,
    /// Twist in degrees. A non-finite value is stored as `None`.
    pub twist: Option<f32>,
    /// Buttons held after this update.
    pub buttons: PointerButtons,
    /// Button that produced a press or release edge. While moving, the held button.
    pub button: PointerButton,
    /// Start, move, end, or cancel.
    pub phase: ContactPhase,
    /// Timestamp stored on the contact and on the appended sample.
    pub time: Duration,
}

/// One contact after [`Pointers::apply`].
#[derive(Clone, Debug, PartialEq)]
pub struct Contact {
    /// Id from the update that created it.
    pub id: u64,
    /// Latest kind.
    pub kind: PointerKind,
    /// Latest position in logical points.
    pub position: [f32; 2],
    /// Latest pressure, if the device reported one.
    pub pressure: Option<f32>,
    /// Latest tilt in degrees, if the device reported it.
    pub tilt: Option<[f32; 2]>,
    /// Latest twist in degrees, if the device reported it.
    pub twist: Option<f32>,
    /// Buttons held now.
    pub buttons: PointerButtons,
    /// Button of the latest edge, or the held button while moving.
    pub button: PointerButton,
    /// Timestamp of the latest update.
    pub time: Duration,
    /// Press edge for this frame. A later move or release in the same frame
    /// leaves it set. Cleared by [`Pointers::end_frame`].
    pub pressed: bool,
    /// `true` while the contact has not ended.
    pub down: bool,
    /// Release edge for this frame. A press earlier in the same frame leaves
    /// [`Self::pressed`] set. Cleared by [`Pointers::end_frame`].
    pub released: bool,
    /// `true` when this frame's end was [`ContactPhase::Cancel`].
    pub cancelled: bool,
    /// Position where the contact went down, on the press frame. `None` after
    /// [`Pointers::end_frame`].
    pub press_at: Option<[f32; 2]>,
    /// Samples since [`Pointers::end_frame`], including this frame's newest.
    pub samples: Vec<PointerSample>,
}

/// Every contact for the frame, plus which id is primary.
#[derive(Clone, Debug, PartialEq)]
pub struct Pointers {
    contacts: Vec<Contact>,
    primary: u64,
}

impl Default for Pointers {
    fn default() -> Self {
        Self {
            contacts: Vec::new(),
            primary: MOUSE_ID,
        }
    }
}

impl Pointers {
    /// Empty set. The primary id is [`MOUSE_ID`](crate::MOUSE_ID).
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or update the contact for `update.id` and append one sample.
    ///
    /// An end or cancel for an unknown id still inserts a released contact so
    /// the frame can observe it. Finite pressure is clamped to `0.0..=1.0`.
    pub fn apply(&mut self, update: ContactUpdate) {
        let sample = PointerSample {
            position: update.position,
            pressure: clamp_unit(update.pressure),
            tilt: finite_tilt(update.tilt),
            twist: finite_scalar(update.twist),
            time: update.time,
        };
        if let Some(contact) = self
            .contacts
            .iter_mut()
            .find(|contact| contact.id == update.id)
        {
            contact.kind = update.kind;
            contact.position = update.position;
            contact.pressure = sample.pressure;
            contact.tilt = sample.tilt;
            contact.twist = sample.twist;
            contact.buttons = update.buttons;
            contact.button = update.button;
            contact.time = update.time;
            contact.samples.push(sample);
            // Edges stick until `end_frame`. A press and release in one batch
            // used to keep only the release, so a click with no gap never
            // looked pressed.
            match update.phase {
                ContactPhase::Start => {
                    contact.pressed = true;
                    contact.down = true;
                    contact.cancelled = false;
                    contact.press_at = Some(update.position);
                }
                ContactPhase::Move => {
                    if contact.buttons.is_empty() && contact.kind == PointerKind::Mouse {
                        // A hover keeps `down` as it was. A touch move stays down.
                    }
                }
                ContactPhase::End | ContactPhase::Cancel => {
                    contact.down = false;
                    contact.released = true;
                    contact.cancelled = update.phase == ContactPhase::Cancel;
                    contact.buttons = PointerButtons::new();
                }
            }
            return;
        }
        let ended = matches!(update.phase, ContactPhase::End | ContactPhase::Cancel);
        let started = update.phase == ContactPhase::Start;
        self.contacts.push(Contact {
            id: update.id,
            kind: update.kind,
            position: update.position,
            pressure: sample.pressure,
            tilt: sample.tilt,
            twist: sample.twist,
            buttons: if ended {
                PointerButtons::new()
            } else {
                update.buttons
            },
            button: update.button,
            time: update.time,
            pressed: started,
            down: !ended
                && (started || !update.buttons.is_empty() || update.kind == PointerKind::Touch),
            released: ended,
            cancelled: update.phase == ContactPhase::Cancel,
            press_at: started.then_some(update.position),
            samples: vec![sample],
        });
    }

    /// Contact with `id`, if one was applied and not yet dropped.
    pub fn get(&self, id: u64) -> Option<&Contact> {
        self.contacts.iter().find(|contact| contact.id == id)
    }

    /// Contacts in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &Contact> {
        self.contacts.iter()
    }

    /// Id mirrored into [`Input::pointer`](crate::Input::pointer) when that
    /// contact exists. The default is [`MOUSE_ID`](crate::MOUSE_ID).
    pub fn primary_id(&self) -> u64 {
        self.primary
    }

    /// Prefer `id` in [`Self::primary`]. Stored even when the contact is absent,
    /// so a pen that appears later can take over without another call.
    pub fn set_primary(&mut self, id: u64) {
        self.primary = id;
    }

    /// The contact widgets should treat as [`Input::pointer`](crate::Input::pointer).
    ///
    /// The stored primary id wins when that contact exists. Otherwise the mouse
    /// wins, then any contact that is down, then the oldest contact.
    pub fn primary(&self) -> Option<&Contact> {
        if let Some(contact) = self.get(self.primary) {
            return Some(contact);
        }
        self.contacts
            .iter()
            .find(|contact| contact.kind == PointerKind::Mouse)
            .or_else(|| self.contacts.iter().find(|contact| contact.down))
            .or_else(|| self.contacts.first())
    }

    /// [`Self::primary`] copied into a [`Pointer`]. `None` when there is no contact.
    pub fn mirror(&self) -> Option<Pointer> {
        let contact = self.primary()?;
        Some(Pointer {
            id: contact.id,
            position: contact.position,
            pressed: contact.pressed,
            down: contact.down,
            released: contact.released,
            button: contact.button,
            press_at: contact.press_at,
        })
    }

    /// Drop ended touch, pen, and eraser contacts. Keep the mouse so hover
    /// survives the lift. Clear edges, coalesced samples, and `press_at`.
    pub fn end_frame(&mut self) {
        self.contacts
            .retain(|contact| contact.down || contact.kind == PointerKind::Mouse);
        for contact in &mut self.contacts {
            contact.pressed = false;
            contact.released = false;
            contact.cancelled = false;
            contact.press_at = None;
            contact.samples.clear();
        }
    }
}

fn clamp_unit(value: Option<f32>) -> Option<f32> {
    let value = finite_scalar(value)?;
    Some(value.clamp(0.0, 1.0))
}

fn finite_scalar(value: Option<f32>) -> Option<f32> {
    value.filter(|value| value.is_finite())
}

fn finite_tilt(value: Option<[f32; 2]>) -> Option<[f32; 2]> {
    let value = value?;
    if value[0].is_finite() && value[1].is_finite() {
        Some(value)
    } else {
        None
    }
}

/// Phase of a [`Gesture`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GesturePhase {
    /// The gesture began. Scale is 1 and the deltas are 0 for a synthesized pair.
    Started,
    /// The gesture changed.
    Changed,
    /// A finger lifted, or the platform ended the gesture.
    Ended,
    /// The platform cancelled tracking.
    Cancelled,
}

/// Pinch, rotation, and pan for one update.
///
/// `pinch` is a scale factor for this update. `1.0` means the distance did not
/// change. A platform magnification whose delta is `0` at rest is stored as
/// `1 + delta`.
///
/// `rotation` is radians, positive counterclockwise, matching the sign of the
/// macOS rotation gesture after its degrees are converted.
///
/// `pan` is the centroid movement in logical points. Positive X is right and
/// positive Y is down.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gesture {
    /// Started, changed, ended, or cancelled.
    pub phase: GesturePhase,
    /// Scale factor for this update. `1.0` is unchanged.
    pub pinch: f32,
    /// Radians, positive counterclockwise.
    pub rotation: f32,
    /// Centroid delta in logical points.
    pub pan: [f32; 2],
}

impl Event for Gesture {}

impl Gesture {
    /// A platform pinch. `delta` is the magnification where `0` is unchanged,
    /// so the stored scale is `1 + delta`. A non-finite delta becomes `1.0`
    /// on a change and is left at `1.0` when the phase is not a change.
    pub fn pinch(phase: GesturePhase, delta: f64) -> Self {
        let pinch = if delta.is_finite() {
            1.0 + delta as f32
        } else {
            1.0
        };
        Self {
            phase,
            pinch,
            rotation: 0.0,
            pan: [0.0, 0.0],
        }
    }

    /// A platform rotation. `degrees` is converted to radians. Positive stays
    /// counterclockwise. A non-finite value becomes `0.0`.
    pub fn rotate(phase: GesturePhase, degrees: f32) -> Self {
        let rotation = if degrees.is_finite() {
            degrees.to_radians()
        } else {
            0.0
        };
        Self {
            phase,
            pinch: 1.0,
            rotation,
            pan: [0.0, 0.0],
        }
    }
}

/// Synthesizes [`Gesture`] values from the two oldest touch contacts that are down.
///
/// Extra fingers are ignored. A new pair emits [`GesturePhase::Started`]. Later
/// updates emit [`GesturePhase::Changed`]. Dropping below two contacts emits
/// [`GesturePhase::Ended`], or [`GesturePhase::Cancelled`] when either contact
/// was cancelled this frame.
#[derive(Clone, Debug, Default)]
pub struct GestureState {
    pair: Option<TouchPair>,
}

#[derive(Clone, Copy, Debug)]
struct TouchPair {
    a: u64,
    b: u64,
    span: f32,
    angle: f32,
    centroid: [f32; 2],
}

impl GestureState {
    /// No pair in progress.
    pub fn new() -> Self {
        Self::default()
    }

    /// Gestures implied by `pointers` since the previous call.
    ///
    /// Only [`PointerKind::Touch`] contacts with [`Contact::down`] participate.
    /// The two lowest ids form the pair.
    pub fn update(&mut self, pointers: &Pointers) -> Vec<Gesture> {
        let mut down: Vec<&Contact> = pointers
            .iter()
            .filter(|contact| contact.kind == PointerKind::Touch && contact.down)
            .collect();
        down.sort_by_key(|contact| contact.id);
        if down.len() >= 2 {
            let (first, second) = (down[0], down[1]);
            let span = distance(first.position, second.position);
            let angle = angle_of(first.position, second.position);
            let centroid = midpoint(first.position, second.position);
            let next = TouchPair {
                a: first.id,
                b: second.id,
                span,
                angle,
                centroid,
            };
            if self
                .pair
                .is_some_and(|pair| pair.a == next.a && pair.b == next.b)
            {
                let prev = self.pair.replace(next).expect("pair");
                let pinch = if prev.span > f32::EPSILON {
                    (span / prev.span).clamp(0.01, 100.0)
                } else {
                    1.0
                };
                return vec![Gesture {
                    phase: GesturePhase::Changed,
                    pinch,
                    rotation: wrap_angle(-(angle - prev.angle)),
                    pan: [
                        centroid[0] - prev.centroid[0],
                        centroid[1] - prev.centroid[1],
                    ],
                }];
            }
            self.pair = Some(next);
            return vec![Gesture {
                phase: GesturePhase::Started,
                pinch: 1.0,
                rotation: 0.0,
                pan: [0.0, 0.0],
            }];
        }
        let Some(pair) = self.pair.take() else {
            return Vec::new();
        };
        let cancelled = pointers
            .get(pair.a)
            .is_some_and(|contact| contact.cancelled)
            || pointers
                .get(pair.b)
                .is_some_and(|contact| contact.cancelled);
        vec![Gesture {
            phase: if cancelled {
                GesturePhase::Cancelled
            } else {
                GesturePhase::Ended
            },
            pinch: 1.0,
            rotation: 0.0,
            pan: [0.0, 0.0],
        }]
    }
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    (dx * dx + dy * dy).sqrt()
}

fn midpoint(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5]
}

fn angle_of(a: [f32; 2], b: [f32; 2]) -> f32 {
    (b[1] - a[1]).atan2(b[0] - a[0])
}

fn wrap_angle(mut radians: f32) -> f32 {
    if !radians.is_finite() {
        return 0.0;
    }
    while radians > PI {
        radians -= 2.0 * PI;
    }
    while radians < -PI {
        radians += 2.0 * PI;
    }
    radians
}
