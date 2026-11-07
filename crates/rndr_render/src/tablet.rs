//! Windows and other winit backends report a pen as a tablet tool.
//!
//! `PenFeed` is a macOS event-monitor and does not run on Windows. This
//! mapping is the contact those `TabletTool` pointer and button events become.
//! It is not a claim that a physical pen was used.

use std::time::Duration;

use rndr_core::{ContactPhase, ContactUpdate, PointerButton, PointerButtons, PointerKind, PEN_ID};
use winit::dpi::PhysicalPosition;
use winit::event::{TabletToolButton, TabletToolData, TabletToolKind};

use crate::positive_scale;

/// One tablet sample in rndr's pointer model.
#[allow(clippy::too_many_arguments)]
pub(crate) fn tablet_contact(
    kind: TabletToolKind,
    data: &TabletToolData,
    phase: ContactPhase,
    button: PointerButton,
    held: bool,
    position: PhysicalPosition<f64>,
    scale: f64,
    time: Duration,
) -> ContactUpdate {
    let scale = positive_scale(scale);
    let mut buttons = PointerButtons::new();
    if held {
        buttons.set(button, true);
    }
    ContactUpdate {
        id: PEN_ID,
        kind: tool_kind(kind),
        position: [position.x as f32 / scale, position.y as f32 / scale],
        pressure: tool_pressure(data),
        tilt: tool_tilt(data),
        twist: data.twist.map(f32::from),
        buttons,
        button,
        phase,
        time,
    }
}

/// Stylus tip, or the eraser end. Other tool names stay a pen so the contact
/// is not dropped.
pub(crate) fn tool_kind(kind: TabletToolKind) -> PointerKind {
    match kind {
        TabletToolKind::Eraser => PointerKind::Eraser,
        TabletToolKind::Pen
        | TabletToolKind::Brush
        | TabletToolKind::Pencil
        | TabletToolKind::Airbrush
        | TabletToolKind::Finger
        | TabletToolKind::Mouse
        | TabletToolKind::Lens
        | _ => PointerKind::Pen,
    }
}

pub(crate) fn tool_button(button: TabletToolButton) -> PointerButton {
    match button {
        TabletToolButton::Contact => PointerButton::Left,
        TabletToolButton::Barrel => PointerButton::Right,
        TabletToolButton::Other(1) => PointerButton::Middle,
        TabletToolButton::Other(3) => PointerButton::Other(8),
        TabletToolButton::Other(4) => PointerButton::Other(9),
        TabletToolButton::Other(code) => PointerButton::Other(code),
    }
}

/// The tip is down when the tool reports a positive finite pressure.
pub(crate) fn tip_down(data: &TabletToolData) -> bool {
    tool_pressure(data).is_some_and(|pressure| pressure > 0.0)
}

fn tool_pressure(data: &TabletToolData) -> Option<f32> {
    let angle = data
        .angle
        .or_else(|| data.tilt.map(winit::event::TabletToolTilt::angle));
    data.force.as_ref().and_then(|force| {
        let normalized = force.normalized(angle);
        normalized.is_finite().then_some(normalized as f32)
    })
}

fn tool_tilt(data: &TabletToolData) -> Option<[f32; 2]> {
    let tilt = data
        .tilt
        .or_else(|| data.angle.map(winit::event::TabletToolAngle::tilt))?;
    Some([f32::from(tilt.x), f32::from(tilt.y)])
}

/// Contact phase and whether the named tool button is held.
pub(crate) fn tool_phase(button: TabletToolButton, pressed: bool) -> (ContactPhase, bool) {
    match (button, pressed) {
        (TabletToolButton::Contact, true) => (ContactPhase::Start, true),
        (TabletToolButton::Contact, false) => (ContactPhase::End, false),
        (_, true) => (ContactPhase::Move, true),
        (_, false) => (ContactPhase::Move, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::event::{Force, TabletToolAngle, TabletToolTilt};

    fn bits(value: [f32; 2]) -> [u32; 2] {
        [value[0].to_bits(), value[1].to_bits()]
    }

    #[test]
    fn a_pen_move_keeps_pressure_tilt_twist_and_logical_position() {
        let data = TabletToolData {
            force: Some(Force::Normalized(0.5)),
            tangential_force: None,
            twist: Some(90),
            tilt: Some(TabletToolTilt { x: 10, y: -5 }),
            angle: None,
        };
        let update = tablet_contact(
            TabletToolKind::Pen,
            &data,
            ContactPhase::Move,
            PointerButton::Left,
            true,
            PhysicalPosition::new(20.0, 40.0),
            2.0,
            Duration::from_millis(5),
        );
        assert_eq!(update.id, PEN_ID);
        assert_eq!(update.kind, PointerKind::Pen);
        assert_eq!(bits(update.position), bits([10.0, 20.0]));
        assert_eq!(update.pressure.map(f32::to_bits), Some(0.5f32.to_bits()));
        assert_eq!(update.tilt.map(bits), Some(bits([10.0, -5.0])));
        assert_eq!(update.twist.map(f32::to_bits), Some(90.0f32.to_bits()));
        assert!(update.buttons.contains(PointerButton::Left));
        assert_eq!(update.phase, ContactPhase::Move);
        assert!(tip_down(&data));
    }

    #[test]
    fn eraser_contact_press_and_release_are_start_and_end() {
        let data = TabletToolData {
            force: Some(Force::Calibrated {
                force: 1.0,
                max_possible_force: 2.0,
            }),
            angle: Some(TabletToolAngle {
                altitude: std::f64::consts::FRAC_PI_2,
                azimuth: 0.0,
            }),
            ..TabletToolData::default()
        };
        let (phase, held) = tool_phase(TabletToolButton::Contact, true);
        let start = tablet_contact(
            TabletToolKind::Eraser,
            &data,
            phase,
            tool_button(TabletToolButton::Contact),
            held,
            PhysicalPosition::new(0.0, 0.0),
            1.0,
            Duration::ZERO,
        );
        assert_eq!(start.kind, PointerKind::Eraser);
        assert_eq!(start.phase, ContactPhase::Start);
        assert!(start.buttons.contains(PointerButton::Left));
        assert_eq!(start.pressure.map(f32::to_bits), Some(0.5f32.to_bits()));
        assert_eq!(bits(start.position), bits([0.0, 0.0]));

        let (phase, held) = tool_phase(TabletToolButton::Contact, false);
        let end = tablet_contact(
            TabletToolKind::Eraser,
            &data,
            phase,
            tool_button(TabletToolButton::Contact),
            held,
            PhysicalPosition::new(4.0, 6.0),
            1.0,
            Duration::ZERO,
        );
        assert_eq!(end.phase, ContactPhase::End);
        assert!(end.buttons.is_empty());
        assert_eq!(tool_button(TabletToolButton::Barrel), PointerButton::Right);
    }

    #[test]
    fn a_missing_force_does_not_invent_pressure() {
        let update = tablet_contact(
            TabletToolKind::Brush,
            &TabletToolData::default(),
            ContactPhase::Move,
            PointerButton::Left,
            false,
            PhysicalPosition::new(8.0, 2.0),
            0.0,
            Duration::ZERO,
        );
        assert_eq!(update.kind, PointerKind::Pen);
        assert!(update.pressure.is_none());
        assert!(!tip_down(&TabletToolData::default()));
        assert!(update.tilt.is_none());
        assert!(update.twist.is_none());
        assert_eq!(bits(update.position), bits([8.0, 2.0]));
    }
}
