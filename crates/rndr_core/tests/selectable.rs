//! Pointer selection, multi-press units and copy on read-only labels.

use std::sync::Arc;
use std::time::Duration;

use rndr_core::{
    label_box, App, Clipboard, Entity, Input, KeyEvent, Label, Layout, ManualClock, Modifiers,
    Platform, PointerButton, Rect, Selectable, SelectablePlugin, Size, TextLayout, Viewport, World,
};

const TEXT: &str = "alpha beta gamma\nsecond line here";

fn boot() -> (App, Entity, Arc<ManualClock>) {
    let mut app = App::new();
    let clock = Arc::new(ManualClock::new());
    app.set_clock(clock.clone());
    app.add_plugin(SelectablePlugin);
    app.world_mut().insert_resource(Viewport::new(480.0, 200.0));
    app.world_mut().insert_resource(Platform::X11);
    let other = app.world_mut().spawn((Layout {
        width: Size::Px(480.0),
        height: Size::Px(200.0),
        ..Layout::column()
    },));
    let label = app.world_mut().spawn((
        Layout {
            width: Size::Px(460.0),
            height: Size::Hug,
            ..Layout::column()
        },
        Label::new(TEXT),
        Selectable::default(),
    ));
    rndr_core::set_parent(app.world_mut(), label, other);
    app.update();
    (app, label, clock)
}

/// Window point of the caret edge before `byte` on the label's first line or
/// the line holding `byte`.
fn at(world: &World, label: Entity, byte: usize) -> [f32; 2] {
    let rect = *world.get::<Rect>(label).unwrap();
    let inset = label_box(world, label);
    let layout = world.get::<TextLayout>(label).unwrap();
    let line = layout
        .lines
        .iter()
        .find(|line| byte >= line.start && byte <= line.end)
        .unwrap();
    let x = if byte == line.start {
        0.0
    } else {
        let boxes = layout.selection_boxes(line.start, byte);
        let last = boxes.last().unwrap();
        last.x + last.w
    };
    [
        rect.x + inset.inset_x + x,
        rect.y + inset.inset_y + line.y + line.h * 0.5,
    ]
}

fn pointer(app: &mut App, [x, y]: [f32; 2], pressed: bool, down: bool, released: bool) {
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.position = [x, y];
    input.pointer.pressed = pressed;
    input.pointer.down = down;
    input.pointer.released = released;
    input.pointer.button = PointerButton::Left;
    app.update();
}

fn click(app: &mut App, point: [f32; 2]) {
    pointer(app, point, true, true, false);
    pointer(app, point, false, false, true);
}

fn selected(app: &App, label: Entity) -> String {
    app.world()
        .get::<Selectable>(label)
        .unwrap()
        .selected(TEXT)
        .to_string()
}

#[test]
fn drag_selects_and_release_publishes_primary() {
    let (mut app, label, clock) = boot();
    let from = at(app.world(), label, 6);
    let to = at(app.world(), label, 16);
    pointer(&mut app, from, true, true, false);
    pointer(&mut app, to, false, true, false);
    assert_eq!(selected(&app, label), "beta gamma");
    assert!(app
        .world()
        .resource::<Clipboard>()
        .is_none_or(|clipboard| clipboard.primary.is_empty()));
    pointer(&mut app, to, false, false, true);
    assert_eq!(
        app.world().resource::<Clipboard>().unwrap().primary,
        "beta gamma"
    );
    let backwards = at(app.world(), label, 2);
    clock.advance(Duration::from_secs(1));
    pointer(&mut app, from, true, true, false);
    pointer(&mut app, backwards, false, true, false);
    let selection = *app.world().get::<Selectable>(label).unwrap();
    assert_eq!((selection.anchor, selection.head), (6, 2));
    assert_eq!(selected(&app, label), "pha ");
}

#[test]
fn double_and_triple_press_select_word_and_paragraph() {
    let (mut app, label, clock) = boot();
    let inside_beta = at(app.world(), label, 8);
    click(&mut app, inside_beta);
    clock.advance(Duration::from_millis(80));
    click(&mut app, inside_beta);
    assert_eq!(selected(&app, label), "beta");
    clock.advance(Duration::from_millis(80));
    click(&mut app, inside_beta);
    assert_eq!(selected(&app, label), "alpha beta gamma");

    clock.advance(Duration::from_millis(80));
    let second = at(app.world(), label, 20);
    click(&mut app, second);
    assert_eq!(selected(&app, label), "");
    clock.advance(Duration::from_millis(80));
    click(&mut app, second);
    assert_eq!(selected(&app, label), "second");
}

#[test]
fn slow_or_distant_presses_start_over() {
    let (mut app, label, clock) = boot();
    let beta = at(app.world(), label, 8);
    click(&mut app, beta);
    clock.advance(Duration::from_millis(600));
    click(&mut app, beta);
    assert_eq!(selected(&app, label), "");
    clock.advance(Duration::from_millis(50));
    let point = at(app.world(), label, 13);
    click(&mut app, point);
    assert_eq!(selected(&app, label), "");
}

#[test]
fn word_drag_extends_by_whole_words() {
    let (mut app, label, clock) = boot();
    let beta = at(app.world(), label, 8);
    click(&mut app, beta);
    clock.advance(Duration::from_millis(60));
    pointer(&mut app, beta, true, true, false);
    let point = at(app.world(), label, 13);
    pointer(&mut app, point, false, true, false);
    assert_eq!(selected(&app, label), "beta gamma");
    let point = at(app.world(), label, 2);
    pointer(&mut app, point, false, true, false);
    assert_eq!(selected(&app, label), "alpha beta");
}

#[test]
fn shift_press_extends_the_selection() {
    let (mut app, label, _) = boot();
    let point = at(app.world(), label, 6);
    click(&mut app, point);
    app.world_mut().resource_mut::<Input>().unwrap().shift = true;
    let point = at(app.world(), label, 10);
    click(&mut app, point);
    app.world_mut().resource_mut::<Input>().unwrap().shift = false;
    assert_eq!(selected(&app, label), "beta");
}

#[test]
fn press_elsewhere_clears_and_disengages() {
    let (mut app, label, _) = boot();
    let from = at(app.world(), label, 0);
    pointer(&mut app, from, true, true, false);
    let point = at(app.world(), label, 5);
    pointer(&mut app, point, false, false, true);
    assert_eq!(selected(&app, label), "alpha");
    click(&mut app, [470.0, 190.0]);
    assert!(app.world().get::<Selectable>(label).unwrap().is_empty());
    keys(&mut app, &['a', 'c']);
    assert!(app.world().get::<Selectable>(label).unwrap().is_empty());
}

fn keys(app: &mut App, chars: &[char]) {
    let platform = Platform::X11;
    let mut mods = Modifiers::new();
    mods.ctrl = true;
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.keys.clear();
    for ch in chars {
        input.push_key(KeyEvent::character(*ch, mods), platform);
    }
    app.update();
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.keys.clear();
    input.key = None;
}

#[test]
fn select_all_and_copy_write_the_clipboard() {
    let (mut app, label, _) = boot();
    let point = at(app.world(), label, 3);
    click(&mut app, point);
    keys(&mut app, &['c']);
    assert!(app
        .world()
        .resource::<Clipboard>()
        .is_none_or(|clipboard| clipboard.text.is_empty()));
    keys(&mut app, &['a', 'c']);
    assert_eq!(selected(&app, label), TEXT);
    let clipboard = app.world().resource::<Clipboard>().unwrap();
    assert_eq!(clipboard.text, TEXT);
    assert_eq!(clipboard.primary, TEXT);
}

#[test]
fn selecting_leaves_layout_unchanged() {
    let (mut app, label, _) = boot();
    let rect = *app.world().get::<Rect>(label).unwrap();
    let glyphs: Vec<[f32; 2]> = app
        .world()
        .get::<TextLayout>(label)
        .unwrap()
        .glyphs
        .iter()
        .map(|glyph| [glyph.x, glyph.y])
        .collect();
    let from = at(app.world(), label, 0);
    pointer(&mut app, from, true, true, false);
    let point = at(app.world(), label, 20);
    pointer(&mut app, point, false, false, true);
    assert!(!app.world().get::<Selectable>(label).unwrap().is_empty());
    assert_eq!(*app.world().get::<Rect>(label).unwrap(), rect);
    let after: Vec<[f32; 2]> = app
        .world()
        .get::<TextLayout>(label)
        .unwrap()
        .glyphs
        .iter()
        .map(|glyph| [glyph.x, glyph.y])
        .collect();
    assert_eq!(after, glyphs);
}

#[test]
fn hovering_selectable_text_shows_the_text_cursor() {
    let (mut app, label, _) = boot();
    let point = at(app.world(), label, 3);
    pointer(&mut app, point, false, false, false);
    assert_eq!(
        app.world().resource::<rndr_core::CursorStyle>().copied(),
        Some(rndr_core::CursorStyle::Text)
    );
    pointer(&mut app, [470.0, 190.0], false, false, false);
    assert_eq!(
        app.world().resource::<rndr_core::CursorStyle>().copied(),
        Some(rndr_core::CursorStyle::Arrow)
    );
}

#[test]
fn selections_follow_label_text_changes() {
    let (mut app, label, _) = boot();
    app.world_mut().insert(
        label,
        Selectable {
            anchor: 6,
            head: 10,
        },
    );
    app.world_mut().get_mut::<Label>(label).unwrap().text = "añb".into();
    app.update();
    assert_eq!(
        *app.world().get::<Selectable>(label).unwrap(),
        Selectable { anchor: 4, head: 4 }
    );
    app.world_mut()
        .insert(label, Selectable { anchor: 2, head: 9 });
    app.update();
    let selection = *app.world().get::<Selectable>(label).unwrap();
    assert_eq!(selection, Selectable { anchor: 1, head: 4 });
    assert_eq!(selection.selected("añb"), "ñb");
    assert_eq!(Selectable { anchor: 2, head: 9 }.selected("añb"), "ñb");
}

fn button_with_selectable_text(selectable_first: bool) -> (App, Entity, Entity) {
    use rndr_core::{
        apply_focus, Button, Focusable, GenericInteraction, ReleaseActivation, Schedule,
    };
    let mut app = App::new();
    if selectable_first {
        app.add_plugin(SelectablePlugin)
            .add_plugin(GenericInteraction);
    } else {
        app.add_plugin(GenericInteraction)
            .add_plugin(SelectablePlugin);
    }
    app.add_system(Schedule::Update, apply_focus);
    app.world_mut().insert_resource(Viewport::new(480.0, 200.0));
    app.world_mut().insert_resource(Platform::X11);
    let button = app.world_mut().spawn((
        Layout {
            width: Size::Px(460.0),
            height: Size::Px(80.0),
            ..Layout::column()
        },
        Button::new("go"),
        Focusable,
        ReleaseActivation,
    ));
    let label = app.world_mut().spawn((
        Layout {
            width: Size::Px(400.0),
            height: Size::Hug,
            ..Layout::column()
        },
        Label::new(TEXT),
        Selectable::default(),
    ));
    rndr_core::set_parent(app.world_mut(), label, button);
    app.update();
    (app, button, label)
}

#[test]
fn selectable_text_keeps_the_press_from_its_button() {
    for selectable_first in [true, false] {
        let (mut app, button, label) = button_with_selectable_text(selectable_first);
        let from = at(app.world(), label, 6);
        let to = at(app.world(), label, 10);
        pointer(&mut app, from, true, true, false);
        pointer(&mut app, to, false, true, false);
        pointer(&mut app, to, false, false, true);
        assert_eq!(selected(&app, label), "beta", "order {selectable_first}");
        let world = app.world();
        assert_ne!(
            world
                .resource::<rndr_core::Focus>()
                .and_then(|focus| focus.entity),
            Some(button),
            "order {selectable_first}"
        );
        assert!(
            world
                .resource::<rndr_core::ActionIntents>()
                .is_none_or(|queue| queue.pending().is_empty()),
            "order {selectable_first}"
        );
    }
}
