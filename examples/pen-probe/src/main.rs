//! Pen and touch pressure spike.
//!
//! On macOS the runner installs a local `NSEvent` monitor. Mouse and tablet
//! events whose subtype is tablet-point copy pressure, tilt, and twist onto
//! the pen contact ([`rndr_core::PEN_ID`]). This file prints that contact when
//! pressure, tilt, or kind changes. The monitor is compiled into `rndr_render`;
//! it is not verified against a tablet in CI. An Apple Pencil through Sidecar,
//! or a Wacom tablet, is the check that still needs hardware.
//!
//! On Windows, winit 0.30 reports a pen as a touch. `Touch.force` is stored as
//! pressure on that touch contact. The kind stays [`rndr_core::PointerKind::Touch`]
//! because the event does not say pen. Force is unverified without a touch
//! digitizer. Trackpad force-touch is not treated as a pen.
//!
//! Linux does not install the monitor. File drop on Wayland is also absent
//! from winit 0.30. Pinch and rotation gestures are delivered on macOS.

use rndr_core::{App, PointerKind, Pointers, Schedule};
use rndr_render::run;

fn main() {
    let mut app = App::new();
    app.add_system(Schedule::Update, log_contacts);
    run(app);
}

fn log_contacts(world: &mut rndr_core::World) {
    let Some(pointers) = world.resource::<Pointers>() else {
        return;
    };
    for contact in pointers.iter() {
        let interesting = contact.kind != PointerKind::Mouse || contact.pressure.is_some();
        if !interesting {
            continue;
        }
        println!(
            "id={} kind={:?} down={} pos=({:.1}, {:.1}) pressure={:?} tilt={:?} twist={:?} samples={}",
            contact.id,
            contact.kind,
            contact.down,
            contact.position[0],
            contact.position[1],
            contact.pressure,
            contact.tilt,
            contact.twist,
            contact.samples.len()
        );
    }
}
