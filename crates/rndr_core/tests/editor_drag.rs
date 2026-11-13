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
fn autoscroll_uses_elapsed_time_clamps_at_edges_and_stops_on_release() {
    let text = "line\n".repeat(20);
    let (mut app, e) = boot(&text);
    let wake = std::sync::Arc::new(rndr_core::CountingWake::new());
    app.set_wake(wake.clone());
    let clock = rndr_core::ManualClock::handle();
    app.set_clock(clock.clone());
    let full = rndr_core::layout_of(app.world(), e, &text);
    let left = point(&app, e, 0.0, 1.0);
    frame(&mut app, left, true, true, false);
    let below = point(&app, e, 0.0, 10000.0);
    frame(&mut app, below, false, true, false);
    assert!(
        app.next_deadline().is_some(),
        "stationary drag must wake the runner"
    );
    let wakes = wake.hits();
    for _ in 0..20 {
        app.update();
    }
    assert_eq!(
        wake.hits(),
        wakes,
        "idle redraws must not rearm native wakes"
    );
    assert_eq!(app.world().get::<Editor>(e).unwrap().offset, 0.0);
    for line in &full.lines[1..=5] {
        clock.advance(std::time::Duration::from_millis(50));
        assert!(app.wants_frame(), "due drag timer must request a frame");
        // No pointer motion: only the clock drives these frames.
        app.update();
        assert_eq!(app.world().get::<Editor>(e).unwrap().offset, line.y);
        assert_eq!(selection(&app, e).1, 0);
        assert!(selection(&app, e).0 < text.len());
    }
    for _ in 0..40 {
        clock.advance(std::time::Duration::from_millis(50));
        app.update();
    }
    assert!(
        app.next_deadline().is_none(),
        "no idle wake at document edge"
    );
    let bottom = full.lines.last().unwrap().y;
    assert_eq!(app.world().get::<Editor>(e).unwrap().offset, bottom);
    frame(&mut app, below, false, false, true);
    assert!(app.next_deadline().is_none());
    let above = point(&app, e, 0.0, -10000.0);
    frame(&mut app, above, false, true, false);
    assert_eq!(app.world().get::<Editor>(e).unwrap().offset, bottom);
    // New capture can scroll back to the document start, but never below zero.
    frame(&mut app, left, true, true, false);
    frame(&mut app, above, false, true, false);
    for _ in 0..40 {
        clock.advance(std::time::Duration::from_millis(50));
        app.update();
    }
    assert_eq!(app.world().get::<Editor>(e).unwrap().offset, 0.0);
    assert_eq!(selection(&app, e).0, 0);
}

#[test]
fn stationary_drag_rate_is_independent_of_frame_count_and_caps_stalls() {
    for cadence in [10, 50, 100] {
        let text = "line\n".repeat(100);
        let (mut app, e) = boot(&text);
        let clock = rndr_core::ManualClock::handle();
        app.set_clock(clock.clone());
        let full = rndr_core::layout_of(app.world(), e, &text);
        let left = point(&app, e, 0.0, 1.0);
        let below = point(&app, e, 0.0, 1000.0);
        frame(&mut app, left, true, true, false);
        frame(&mut app, below, false, true, false);
        for _ in 0..(500 / cadence) {
            clock.advance(std::time::Duration::from_millis(cadence));
            app.update();
        }
        assert_eq!(
            app.world().get::<Editor>(e).unwrap().offset,
            full.lines[10].y
        );
        clock.advance(std::time::Duration::from_secs(60));
        app.update();
        assert_eq!(
            app.world().get::<Editor>(e).unwrap().offset,
            full.lines[15].y
        );
        // Returning inside cancels the wake; elapsed inside time cannot accrue.
        frame(&mut app, left, false, true, false);
        assert!(app.next_deadline().is_none());
        clock.advance(std::time::Duration::from_secs(5));
        let above = point(&app, e, 0.0, -1000.0);
        frame(&mut app, above, false, true, false);
        assert_eq!(
            app.world().get::<Editor>(e).unwrap().offset,
            full.lines[15].y
        );
        clock.advance(std::time::Duration::from_millis(50));
        app.update();
        assert_eq!(
            app.world().get::<Editor>(e).unwrap().offset,
            full.lines[14].y
        );
    }
}

#[test]
fn stationary_drag_timer_cancels_on_release_blur_focus_loss_and_destruction() {
    for reason in 0..4 {
        let (mut app, e) = boot(&"line\n".repeat(40));
        rndr_core::window::platform_focused(app.world());
        app.update();
        let clock = rndr_core::ManualClock::handle();
        app.set_clock(clock.clone());
        let left = point(&app, e, 0.0, 1.0);
        let below = point(&app, e, 0.0, 1000.0);
        frame(&mut app, left, true, true, false);
        frame(&mut app, below, false, true, false);
        assert!(app.next_deadline().is_some());
        let before = selection(&app, e);
        match reason {
            0 => frame(&mut app, below, false, false, true),
            1 => {
                rndr_core::window::platform_blurred(app.world());
                app.update();
            }
            2 => {
                app.world_mut().resource_mut::<Focus>().unwrap().entity = None;
                app.update();
            }
            _ => app.world_mut().despawn_entity(e),
        }
        assert!(app.next_deadline().is_none(), "reason {reason}");
        clock.advance(std::time::Duration::from_secs(1));
        app.update();
        if reason < 3 {
            assert_eq!(selection(&app, e), before);
            assert_eq!(app.world().get::<Editor>(e).unwrap().offset, 0.0);
        }
    }
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
fn pointer_placement_preserves_snapped_caret_column_for_subsequent_vertical_navigation() {
    let text = "abcdef\nx\nabcdef";
    let (mut app, e) = boot(text);
    let full = rndr_core::layout_of(app.world(), e, text);
    let end_of_short_row = point(&app, e, 250.0, full.lines[1].y + 1.0);
    frame(&mut app, end_of_short_row, true, false, true);
    assert_eq!(selection(&app, e), (8, 8));
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.pressed = false;
    input.pointer.released = false;
    input.key = Some(rndr_core::Key::Down);
    app.update();
    assert_eq!(selection(&app, e), (10, 10));
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
