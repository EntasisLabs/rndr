use rndr_core::{
    App, Axis, Buffer, CapturesKeys, Chrome, Editor, Entity, Focus, Input, Label, Layout,
    PointerButton, Rect, Size, Viewport, TEXT_ORIGIN_X, TEXT_ORIGIN_Y,
};

fn boot(text: &str) -> (App, Entity) {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(300.0, 100.0));
    app.world_mut().insert_resource(Focus::default());
    let e = app.world_mut().spawn((
        Layout {
            axis: Axis::Column,
            gap: 0.0,
            padding: 0.0,
            width: Size::Px(280.0),
            height: Size::Px(80.0),
            min_width: 0.0,
            min_height: 0.0,
        },
        Label::new(""),
        Buffer::from_str(text),
        Editor::default(),
        CapturesKeys,
    ));
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(e);
    app.update();
    app.update();
    (app, e)
}

fn point(app: &App, e: Entity, x: f32, y: f32) -> [f32; 2] {
    let r = app.world().get::<Rect>(e).unwrap();
    [r.x + TEXT_ORIGIN_X + x, r.y + TEXT_ORIGIN_Y + y]
}

fn frame(app: &mut App, p: [f32; 2], pressed: bool, down: bool, released: bool) {
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.position = p;
    input.pointer.pressed = pressed;
    input.pointer.down = down;
    input.pointer.released = released;
    app.update();
}

fn selection(app: &App, e: Entity) -> (usize, usize) {
    let b = app.world().get::<Buffer>(e).unwrap();
    (b.caret, b.anchor)
}

#[test]
fn held_motion_reverses_across_stable_anchor_and_release_applies_final_position() {
    let (mut app, e) = boot("hello world");
    let start = point(&app, e, 40.0, 1.0);
    frame(&mut app, start, true, true, false);
    let anchor = selection(&app, e).1;
    assert!(anchor > 0 && anchor < 11);
    let right = point(&app, e, 250.0, 1.0);
    frame(&mut app, right, false, true, false);
    assert_eq!(selection(&app, e), (11, anchor));
    app.world_mut().resource_mut::<Input>().unwrap().shift = true;
    let left = point(&app, e, 0.0, 1.0);
    frame(&mut app, left, false, true, false);
    assert_eq!(selection(&app, e), (0, anchor));
    frame(&mut app, right, false, false, true);
    assert_eq!(selection(&app, e), (11, anchor));
    frame(&mut app, left, false, false, false);
    assert_eq!(selection(&app, e), (11, anchor));
    // A held state alone must not resurrect released capture.
    frame(&mut app, left, false, true, false);
    assert_eq!(selection(&app, e), (11, anchor));
}

#[test]
fn shift_press_preserves_old_anchor_and_new_unshifted_press_resets_it() {
    let (mut app, e) = boot("hello world");
    app.world_mut().resource_mut::<Input>().unwrap().shift = true;
    let left = point(&app, e, 0.0, 1.0);
    frame(&mut app, left, true, true, false);
    assert_eq!(selection(&app, e), (0, 11));
    app.world_mut().resource_mut::<Input>().unwrap().shift = false;
    let right = point(&app, e, 250.0, 1.0);
    frame(&mut app, right, false, true, false);
    assert_eq!(selection(&app, e), (11, 11));
    frame(&mut app, left, true, true, false);
    assert_eq!(selection(&app, e), (0, 0));
}

#[test]
fn capture_tracks_off_editor_and_ignores_presses_that_started_elsewhere() {
    let (mut app, e) = boot("hello world");
    let left = point(&app, e, 0.0, 1.0);
    frame(&mut app, left, true, true, false);
    let outside = point(&app, e, 1000.0, 1.0);
    frame(&mut app, outside, false, true, false);
    assert_eq!(selection(&app, e), (11, 0));
    frame(&mut app, outside, false, false, true);
    frame(&mut app, outside, true, true, false);
    frame(&mut app, left, false, true, false);
    assert_eq!(selection(&app, e), (11, 0));
}

#[test]
fn press_at_keeps_start_anchor_when_motion_and_release_are_coalesced() {
    let (mut app, e) = boot("hello world");
    let left = point(&app, e, 0.0, 1.0);
    let right = point(&app, e, 250.0, 1.0);
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .pointer
        .press_at = Some(left);
    frame(&mut app, right, true, false, true);
    assert_eq!(selection(&app, e), (11, 0));
    frame(&mut app, left, false, true, false);
    assert_eq!(selection(&app, e), (11, 0));
}

#[test]
fn capture_cancels_on_focus_loss_contact_change_button_change_or_missing_down() {
    for reason in 0..4 {
        let (mut app, e) = boot("hello world");
        let left = point(&app, e, 0.0, 1.0);
        let right = point(&app, e, 250.0, 1.0);
        frame(&mut app, left, true, true, false);
        match reason {
            0 => app.world_mut().resource_mut::<Focus>().unwrap().entity = None,
            1 => app.world_mut().resource_mut::<Input>().unwrap().pointer.id = 8,
            2 => {
                app.world_mut()
                    .resource_mut::<Input>()
                    .unwrap()
                    .pointer
                    .button = PointerButton::Right
            }
            _ => (),
        }
        frame(&mut app, right, false, reason != 3, false);
        assert_eq!(selection(&app, e), (0, 0));
        app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(e);
        let p = &mut app.world_mut().resource_mut::<Input>().unwrap().pointer;
        p.id = 0;
        p.button = PointerButton::Left;
        frame(&mut app, right, false, true, false);
        assert_eq!(selection(&app, e), (0, 0), "reason {reason}");
    }
}

#[test]
fn secondary_press_and_hover_do_not_select() {
    let (mut app, e) = boot("hello world");
    let left = point(&app, e, 0.0, 1.0);
    frame(&mut app, left, false, true, false);
    assert_eq!(selection(&app, e), (11, 11));
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .pointer
        .button = PointerButton::Right;
    frame(&mut app, left, true, true, false);
    assert_eq!(selection(&app, e), (11, 11));
}

#[test]
fn autoscroll_is_one_row_per_update_clamps_at_edges_and_stops_on_release() {
    let text = "line\n".repeat(20);
    let (mut app, e) = boot(&text);
    let full = rndr_core::layout_of(app.world(), e, &text);
    let left = point(&app, e, 0.0, 1.0);
    frame(&mut app, left, true, true, false);
    let below = point(&app, e, 0.0, 10000.0);
    for expected in 1..=5 {
        frame(&mut app, below, false, true, false);
        assert_eq!(
            app.world().get::<Editor>(e).unwrap().offset,
            full.lines[expected].y
        );
        assert_eq!(selection(&app, e).1, 0);
        assert!(selection(&app, e).0 < text.len());
    }
    for _ in 0..40 {
        frame(&mut app, below, false, true, false);
    }
    let bottom = full.lines.last().unwrap().y;
    assert_eq!(app.world().get::<Editor>(e).unwrap().offset, bottom);
    frame(&mut app, below, false, false, true);
    let above = point(&app, e, 0.0, -10000.0);
    frame(&mut app, above, false, true, false);
    assert_eq!(app.world().get::<Editor>(e).unwrap().offset, bottom);
    // New capture can scroll back to the document start, but never below zero.
    frame(&mut app, left, true, true, false);
    for _ in 0..40 {
        frame(&mut app, above, false, true, false);
    }
    assert_eq!(app.world().get::<Editor>(e).unwrap().offset, 0.0);
    assert_eq!(selection(&app, e).0, 0);
}

#[test]
fn scrolled_soft_wrap_drag_maps_document_graphemes_not_label_bytes() {
    use unicode_segmentation::UnicodeSegmentation;
    let text = "🇺🇸e\u{301}漢🙂 ".repeat(40);
    let (mut app, e) = boot(&text);
    let full = rndr_core::layout_of(app.world(), e, &text);
    app.world_mut().get_mut::<Editor>(e).unwrap().offset = full.lines[4].y;
    app.update();
    let left = point(&app, e, 0.0, 1.0);
    frame(&mut app, left, true, true, false);
    let anchor = selection(&app, e).1;
    assert!(anchor > 0);
    let right = point(&app, e, 250.0, 35.0);
    frame(&mut app, right, false, true, false);
    let (caret, a) = selection(&app, e);
    assert_eq!(a, anchor);
    assert!(caret > anchor);
    let boundaries: Vec<_> = text
        .grapheme_indices(true)
        .map(|(i, _)| i)
        .chain(std::iter::once(text.len()))
        .collect();
    assert!(boundaries.contains(&caret));
    assert!(boundaries.contains(&a));
    assert_eq!(app.world().get::<Buffer>(e).unwrap().text(), text);
}

#[test]
fn capture_does_not_transfer_to_another_editor_and_coalesced_press_focus_uses_origin() {
    let (mut app, e) = boot("hello world");
    let other = app.world_mut().spawn((
        Rect {
            x: 400.0,
            y: 0.0,
            w: 280.0,
            h: 80.0,
        },
        Label::new(""),
        Buffer::from_str("other"),
        Editor::default(),
        CapturesKeys,
    ));
    app.update();
    let left = point(&app, e, 0.0, 1.0);
    let over_other = point(&app, other, 1.0, 1.0);
    assert_eq!(
        rndr_core::hit_test(app.world(), over_other[0], over_other[1]),
        Some(other)
    );
    frame(&mut app, left, true, true, false);
    frame(&mut app, over_other, false, true, false);
    assert_eq!(selection(&app, e), (11, 0));
    assert_eq!(selection(&app, other), (5, 5));
    assert_eq!(app.world().resource::<Focus>().unwrap().entity, Some(e));
    frame(&mut app, over_other, false, false, true);
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(other);
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .pointer
        .press_at = Some(left);
    frame(&mut app, over_other, true, false, true);
    assert_eq!(app.world().resource::<Focus>().unwrap().entity, Some(e));
    assert_eq!(selection(&app, e), (11, 0));
    assert_eq!(selection(&app, other), (5, 5));
}

#[test]
fn rejected_press_does_not_reveal_an_unrelated_offscreen_caret() {
    let (mut app, e) = boot(&"line\n".repeat(30));
    let left = point(&app, e, 0.0, 1.0);
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .pointer
        .button = PointerButton::Right;
    frame(&mut app, left, true, true, false);
    assert_eq!(app.world().get::<Editor>(e).unwrap().offset, 0.0);
    assert!(app.world().get::<rndr_core::Caret>(e).is_none());
}

#[test]
fn empty_document_and_nonfinite_motion_are_safe_and_cancel_capture() {
    let (mut app, e) = boot("");
    let left = point(&app, e, 0.0, 1.0);
    frame(&mut app, left, true, true, false);
    frame(&mut app, [f32::NAN, f32::INFINITY], false, true, false);
    frame(&mut app, left, false, true, false);
    assert_eq!(selection(&app, e), (0, 0));
}
