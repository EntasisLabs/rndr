use std::time::Duration;

use rndr_core::{
    touch_id, ContactPhase, ContactUpdate, GesturePhase, GestureState, PointerButton,
    PointerButtons, PointerKind, Pointers, MOUSE_ID, PEN_ID,
};

fn update(
    id: u64,
    kind: PointerKind,
    position: [f32; 2],
    phase: ContactPhase,
    buttons: PointerButtons,
) -> ContactUpdate {
    ContactUpdate {
        id,
        kind,
        position,
        pressure: None,
        tilt: None,
        twist: None,
        buttons,
        button: PointerButton::Left,
        phase,
        time: Duration::from_millis(1),
    }
}

#[test]
fn mouse_hover_is_not_down_and_samples_accumulate() {
    let mut pointers = Pointers::new();
    pointers.apply(update(
        MOUSE_ID,
        PointerKind::Mouse,
        [1.0, 2.0],
        ContactPhase::Move,
        PointerButtons::new(),
    ));
    pointers.apply(update(
        MOUSE_ID,
        PointerKind::Mouse,
        [3.0, 4.0],
        ContactPhase::Move,
        PointerButtons::new(),
    ));
    let mouse = pointers.get(MOUSE_ID).unwrap();
    assert!(!mouse.down);
    assert_eq!(mouse.samples.len(), 2);
    assert_eq!(mouse.samples[1].position, [3.0, 4.0]);
    pointers.end_frame();
    let mouse = pointers.get(MOUSE_ID).unwrap();
    assert!(mouse.samples.is_empty());
    assert!(!mouse.pressed);
}

#[test]
fn a_press_mirrors_into_the_primary_pointer() {
    let mut pointers = Pointers::new();
    let mut buttons = PointerButtons::new();
    buttons.set(PointerButton::Left, true);
    pointers.apply(ContactUpdate {
        id: MOUSE_ID,
        kind: PointerKind::Mouse,
        position: [8.0, 9.0],
        pressure: Some(2.0),
        tilt: Some([f32::NAN, 1.0]),
        twist: Some(f32::INFINITY),
        buttons,
        button: PointerButton::Left,
        phase: ContactPhase::Start,
        time: Duration::from_millis(4),
    });
    let contact = pointers.primary().unwrap();
    assert!(contact.down);
    assert!(contact.pressed);
    assert_eq!(contact.pressure, Some(1.0));
    assert!(contact.tilt.is_none());
    assert!(contact.twist.is_none());
    let mirrored = pointers.mirror().unwrap();
    assert_eq!(mirrored.id, MOUSE_ID);
    assert_eq!(mirrored.press_at, Some([8.0, 9.0]));
    assert!(mirrored.down);
}

#[test]
fn a_press_and_release_in_one_frame_keeps_both_edges() {
    let mut pointers = Pointers::new();
    let mut held = PointerButtons::new();
    held.set(PointerButton::Left, true);
    pointers.apply(update(
        MOUSE_ID,
        PointerKind::Mouse,
        [12.0, 18.0],
        ContactPhase::Start,
        held,
    ));
    pointers.apply(update(
        MOUSE_ID,
        PointerKind::Mouse,
        [14.0, 19.0],
        ContactPhase::Move,
        held,
    ));
    pointers.apply(update(
        MOUSE_ID,
        PointerKind::Mouse,
        [14.0, 19.0],
        ContactPhase::End,
        PointerButtons::new(),
    ));
    let mouse = pointers.get(MOUSE_ID).unwrap();
    assert!(mouse.pressed);
    assert!(mouse.released);
    assert!(!mouse.down);
    assert_eq!(mouse.press_at, Some([12.0, 18.0]));
    assert_eq!(mouse.position, [14.0, 19.0]);
    let mirrored = pointers.mirror().unwrap();
    assert!(mirrored.pressed && mirrored.released && !mirrored.down);
    assert_eq!(mirrored.press_at, Some([12.0, 18.0]));
}

#[test]
fn touch_ids_do_not_collide_and_end_frame_drops_them() {
    let mut pointers = Pointers::new();
    pointers.apply(update(
        touch_id(0),
        PointerKind::Touch,
        [0.0, 0.0],
        ContactPhase::Move,
        PointerButtons::new(),
    ));
    let touch = pointers.get(touch_id(0)).unwrap();
    assert!(touch.down);
    assert_ne!(touch.id, MOUSE_ID);
    pointers.apply(update(
        touch_id(0),
        PointerKind::Touch,
        [0.0, 0.0],
        ContactPhase::End,
        PointerButtons::new(),
    ));
    assert!(pointers.get(touch_id(0)).unwrap().released);
    pointers.end_frame();
    assert!(pointers.get(touch_id(0)).is_none());
}

#[test]
fn set_primary_prefers_the_pen_when_it_exists() {
    let mut pointers = Pointers::new();
    pointers.apply(update(
        MOUSE_ID,
        PointerKind::Mouse,
        [1.0, 1.0],
        ContactPhase::Move,
        PointerButtons::new(),
    ));
    pointers.apply(update(
        PEN_ID,
        PointerKind::Pen,
        [4.0, 5.0],
        ContactPhase::Start,
        PointerButtons::new(),
    ));
    assert_eq!(pointers.mirror().unwrap().id, MOUSE_ID);
    pointers.set_primary(PEN_ID);
    assert_eq!(pointers.primary_id(), PEN_ID);
    assert_eq!(pointers.mirror().unwrap().position, [4.0, 5.0]);
}

fn touch(id: u64, position: [f32; 2], phase: ContactPhase) -> ContactUpdate {
    let mut buttons = PointerButtons::new();
    if !matches!(phase, ContactPhase::End | ContactPhase::Cancel) {
        buttons.set(PointerButton::Left, true);
    }
    update(touch_id(id), PointerKind::Touch, position, phase, buttons)
}

#[test]
fn two_touches_pinch_and_pan() {
    let mut pointers = Pointers::new();
    let mut gestures = GestureState::new();
    pointers.apply(touch(1, [0.0, 0.0], ContactPhase::Start));
    pointers.apply(touch(2, [10.0, 0.0], ContactPhase::Start));
    let started = gestures.update(&pointers);
    assert_eq!(started.len(), 1);
    assert_eq!(started[0].phase, GesturePhase::Started);
    assert_eq!(started[0].pinch, 1.0);
    pointers.end_frame();
    pointers.apply(touch(2, [20.0, 0.0], ContactPhase::Move));
    let changed = gestures.update(&pointers);
    assert_eq!(changed[0].phase, GesturePhase::Changed);
    assert!((changed[0].pinch - 2.0).abs() < 1e-4);
    assert!((changed[0].pan[0] - 5.0).abs() < 1e-4);
    assert!(changed[0].pan[1].abs() < 1e-4);
    assert!(changed[0].rotation.abs() < 1e-4);
}

#[test]
fn two_touches_translate_without_scaling() {
    let mut pointers = Pointers::new();
    let mut gestures = GestureState::new();
    pointers.apply(touch(1, [0.0, 0.0], ContactPhase::Start));
    pointers.apply(touch(2, [10.0, 0.0], ContactPhase::Start));
    let _ = gestures.update(&pointers);
    pointers.end_frame();
    pointers.apply(touch(1, [3.0, 4.0], ContactPhase::Move));
    pointers.apply(touch(2, [13.0, 4.0], ContactPhase::Move));
    let changed = gestures.update(&pointers);
    assert!((changed[0].pinch - 1.0).abs() < 1e-4);
    assert!((changed[0].pan[0] - 3.0).abs() < 1e-4);
    assert!((changed[0].pan[1] - 4.0).abs() < 1e-4);
}

#[test]
fn rotation_is_counterclockwise_positive() {
    let mut pointers = Pointers::new();
    let mut gestures = GestureState::new();
    pointers.apply(touch(1, [0.0, 0.0], ContactPhase::Start));
    pointers.apply(touch(2, [10.0, 0.0], ContactPhase::Start));
    let _ = gestures.update(&pointers);
    pointers.end_frame();
    pointers.apply(touch(2, [0.0, 10.0], ContactPhase::Move));
    let changed = gestures.update(&pointers);
    assert!((changed[0].rotation + std::f32::consts::FRAC_PI_2).abs() < 1e-3);
}

#[test]
fn lifting_a_finger_ends_and_a_cancel_cancels() {
    let mut pointers = Pointers::new();
    let mut gestures = GestureState::new();
    pointers.apply(touch(1, [0.0, 0.0], ContactPhase::Start));
    pointers.apply(touch(2, [10.0, 0.0], ContactPhase::Start));
    let _ = gestures.update(&pointers);
    pointers.apply(touch(2, [10.0, 0.0], ContactPhase::End));
    let ended = gestures.update(&pointers);
    assert_eq!(ended[0].phase, GesturePhase::Ended);

    let mut pointers = Pointers::new();
    let mut gestures = GestureState::new();
    pointers.apply(touch(1, [0.0, 0.0], ContactPhase::Start));
    pointers.apply(touch(2, [10.0, 0.0], ContactPhase::Start));
    let _ = gestures.update(&pointers);
    pointers.apply(touch(1, [0.0, 0.0], ContactPhase::Cancel));
    let cancelled = gestures.update(&pointers);
    assert_eq!(cancelled[0].phase, GesturePhase::Cancelled);
}

#[test]
fn platform_pinch_and_rotation_convert_units() {
    let pinch = rndr_core::Gesture::pinch(GesturePhase::Changed, 0.5);
    assert!((pinch.pinch - 1.5).abs() < 1e-4);
    let turn = rndr_core::Gesture::rotate(GesturePhase::Changed, 180.0);
    assert!((turn.rotation - std::f32::consts::PI).abs() < 1e-4);
}
