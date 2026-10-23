use std::sync::Arc;
use std::time::Duration;

use rndr_core::{
    events, App, CapturesKeys, Caret, Chrome, Clipboard, Color, Focus, ImePreview, Input, KeyEvent,
    Label, Layout, ManualClock, Modifiers, NamedKey, Placeholder, Platform, PointerButton, Prompt,
    PromptHistory, Rect, Size, Submitted, TextLayout, TextOverflow, Viewport, CELL_H, CELL_W,
    TEXT_ORIGIN_X, TEXT_ORIGIN_Y,
};

fn boot(text: &str) -> (App, rndr_core::Entity) {
    boot_sized(text, 200.0, Prompt::new())
}

fn boot_sized(text: &str, width: f32, prompt: Prompt) -> (App, rndr_core::Entity) {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Platform::Windows);
    app.world_mut().insert_resource(Viewport::new(480.0, 320.0));
    let entity = app.world_mut().spawn((
        Layout {
            width: Size::Px(width),
            height: Size::Hug,
            ..Layout::column()
        },
        Color::rgb(0.15, 0.16, 0.18),
        Label::new(text),
        prompt,
        CapturesKeys,
    ));
    app.world_mut().insert_resource(Focus {
        entity: Some(entity),
    });
    app.update();
    (app, entity)
}

fn ctrl() -> Modifiers {
    let mut modifiers = Modifiers::new();
    modifiers.ctrl = true;
    modifiers
}

fn shift() -> Modifiers {
    let mut modifiers = Modifiers::new();
    modifiers.shift = true;
    modifiers
}

fn keys(app: &mut App, events: Vec<KeyEvent>) {
    app.world_mut().resource_mut::<Input>().unwrap().keys = events;
    app.update();
}

fn type_text(app: &mut App, text: &str) {
    keys(
        app,
        text.chars()
            .map(|ch| KeyEvent::character(ch, Modifiers::new()))
            .collect(),
    );
}

fn label(app: &App, entity: rndr_core::Entity) -> String {
    app.world().get::<Label>(entity).unwrap().text.clone()
}

fn caret(app: &App, entity: rndr_core::Entity) -> Caret {
    *app.world().get::<Caret>(entity).unwrap()
}

fn submitted(app: &App) -> Vec<String> {
    events::<Submitted>(app.world())
        .iter()
        .map(|event| event.text.clone())
        .collect()
}

#[test]
fn enter_submits_and_shift_enter_inserts_a_newline() {
    let (mut app, entity) = boot("hello");
    assert_eq!(
        app.world().get::<Label>(entity).unwrap().overflow,
        TextOverflow::Wrap
    );
    keys(
        &mut app,
        vec![KeyEvent::named(NamedKey::Enter, Modifiers::new())],
    );
    assert_eq!(label(&app, entity), "hello");
    assert_eq!(submitted(&app), vec!["hello".to_string()]);
    assert_eq!(
        app.world().get::<PromptHistory>(entity).unwrap().entries(),
        &["hello".to_string()]
    );

    keys(&mut app, vec![KeyEvent::named(NamedKey::Enter, shift())]);
    assert_eq!(label(&app, entity), "hello\n");
    assert!(submitted(&app).is_empty());
}

#[test]
fn repeated_enter_does_not_submit_again_and_a_chord_is_ignored() {
    let (mut app, entity) = boot("hello");
    keys(
        &mut app,
        vec![KeyEvent::named(NamedKey::Enter, Modifiers::new()).repeating()],
    );
    assert!(submitted(&app).is_empty());
    assert!(app
        .world()
        .get::<PromptHistory>(entity)
        .unwrap()
        .entries()
        .is_empty());

    keys(&mut app, vec![KeyEvent::named(NamedKey::Enter, ctrl())]);
    assert_eq!(label(&app, entity), "hello");
    assert!(submitted(&app).is_empty());
}

#[test]
fn enter_inserts_a_newline_when_submit_is_off() {
    let (mut app, entity) = boot_sized("ab", 200.0, Prompt::new().submit_on_enter(false));
    keys(
        &mut app,
        vec![KeyEvent::named(NamedKey::Enter, Modifiers::new())],
    );
    assert_eq!(label(&app, entity), "ab\n");
    assert!(submitted(&app).is_empty());
    assert_eq!(Prompt::new().max_lines(0).max_lines, 1);
}

#[test]
fn word_line_and_document_motion_follows_the_platform() {
    let (mut app, entity) = boot("hello world\nnext");
    let end = label(&app, entity).len();
    assert_eq!(caret(&app, entity).index, end);

    keys(&mut app, vec![KeyEvent::named(NamedKey::ArrowLeft, ctrl())]);
    assert_eq!(caret(&app, entity).index, "hello world\n".len());

    keys(
        &mut app,
        vec![KeyEvent::named(NamedKey::Home, Modifiers::new())],
    );
    assert_eq!(caret(&app, entity).index, "hello world\n".len());

    keys(
        &mut app,
        vec![KeyEvent::named(NamedKey::End, Modifiers::new())],
    );
    assert_eq!(caret(&app, entity).index, end);

    keys(&mut app, vec![KeyEvent::named(NamedKey::Home, ctrl())]);
    assert_eq!(caret(&app, entity).index, 0);

    keys(&mut app, vec![KeyEvent::named(NamedKey::End, ctrl())]);
    assert_eq!(caret(&app, entity).index, end);

    app.world_mut().insert_resource(Platform::Mac);
    let mut alt = Modifiers::new();
    alt.alt = true;
    keys(&mut app, vec![KeyEvent::named(NamedKey::ArrowLeft, alt)]);
    assert_eq!(caret(&app, entity).index, "hello world\n".len());

    let mut command = Modifiers::new();
    command.super_key = true;
    keys(
        &mut app,
        vec![KeyEvent::named(NamedKey::ArrowLeft, command)],
    );
    assert_eq!(caret(&app, entity).index, "hello world\n".len());

    keys(&mut app, vec![KeyEvent::named(NamedKey::ArrowUp, command)]);
    assert_eq!(caret(&app, entity).index, 0);
}

#[test]
fn shift_and_drag_select() {
    let (mut app, entity) = boot("hello");
    keys(
        &mut app,
        vec![KeyEvent::named(NamedKey::ArrowLeft, shift())],
    );
    let selected = caret(&app, entity);
    assert_eq!(selected.index, 4);
    assert_eq!(selected.anchor, 5);

    app.world_mut().insert(
        entity,
        Caret {
            index: 0,
            anchor: 0,
        },
    );
    let rect = *app.world().get::<Rect>(entity).unwrap();
    {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.shift = true;
        input.pointer.pressed = true;
        input.pointer.down = true;
        input.pointer.position = [
            rect.x + TEXT_ORIGIN_X + CELL_W * 3.0 + 1.0,
            rect.y + TEXT_ORIGIN_Y + 1.0,
        ];
    }
    app.update();
    let selected = caret(&app, entity);
    assert_eq!(selected.anchor, 0);
    assert_eq!(selected.index, 3);

    {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.shift = false;
        input.pointer.pressed = true;
        input.pointer.down = true;
        input.pointer.position = [rect.x + TEXT_ORIGIN_X + 1.0, rect.y + TEXT_ORIGIN_Y + 1.0];
    }
    app.update();
    {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.pointer.pressed = false;
        input.pointer.down = true;
        input.pointer.position = [
            rect.x + TEXT_ORIGIN_X + CELL_W * 4.0 + 1.0,
            rect.y + TEXT_ORIGIN_Y + 1.0,
        ];
    }
    app.update();
    let selected = caret(&app, entity);
    assert_eq!(selected.anchor, 0);
    assert_eq!(selected.index, 4);
}

#[test]
fn undo_redo_and_clipboard_round_trip() {
    let (mut app, entity) = boot("hello");
    type_text(&mut app, "x");
    assert_eq!(label(&app, entity), "hellox");
    keys(&mut app, vec![KeyEvent::character('z', ctrl())]);
    assert_eq!(label(&app, entity), "hello");
    keys(&mut app, vec![KeyEvent::character('y', ctrl())]);
    assert_eq!(label(&app, entity), "hellox");

    keys(&mut app, vec![KeyEvent::character('z', ctrl())]);
    app.world_mut().insert(
        entity,
        Caret {
            index: 4,
            anchor: 1,
        },
    );
    keys(&mut app, vec![KeyEvent::character('c', ctrl())]);
    assert_eq!(app.world().resource::<Clipboard>().unwrap().text, "ell");
    assert!(app.world().resource::<Clipboard>().unwrap().dirty);
    assert_eq!(label(&app, entity), "hello");

    keys(&mut app, vec![KeyEvent::character('x', ctrl())]);
    assert_eq!(label(&app, entity), "ho");
    app.world_mut().resource_mut::<Clipboard>().unwrap().text = "Z".into();
    keys(&mut app, vec![KeyEvent::character('v', ctrl())]);
    assert_eq!(label(&app, entity), "hZo");
    assert_eq!(caret(&app, entity).index, 2);
}

#[test]
fn placeholder_is_drawn_and_is_not_part_of_the_buffer() {
    let (mut app, entity) = boot("");
    app.world_mut().insert(entity, Placeholder::new("Ask"));
    app.update();
    assert_eq!(label(&app, entity), "");
    assert!(app
        .world()
        .get::<TextLayout>(entity)
        .unwrap()
        .glyphs
        .iter()
        .any(|glyph| glyph.ch == 'A'));

    let rect = *app.world().get::<Rect>(entity).unwrap();
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.pressed = true;
    input.pointer.down = true;
    input.pointer.position = [
        rect.x + TEXT_ORIGIN_X + CELL_W * 2.0,
        rect.y + TEXT_ORIGIN_Y + 1.0,
    ];
    app.update();
    assert_eq!(label(&app, entity), "");
    assert_eq!(caret(&app, entity).index, 0);
    assert_eq!(caret(&app, entity).anchor, 0);
}

#[test]
fn history_recalls_submissions_and_editing_abandons_the_browse() {
    let (mut app, entity) = boot("");
    type_text(&mut app, "one");
    keys(
        &mut app,
        vec![KeyEvent::named(NamedKey::Enter, Modifiers::new())],
    );
    keys(
        &mut app,
        vec![
            KeyEvent::character('a', ctrl()),
            KeyEvent::named(NamedKey::Backspace, Modifiers::new()),
        ],
    );
    type_text(&mut app, "two");
    keys(
        &mut app,
        vec![KeyEvent::named(NamedKey::Enter, Modifiers::new())],
    );
    keys(
        &mut app,
        vec![
            KeyEvent::character('a', ctrl()),
            KeyEvent::named(NamedKey::Backspace, Modifiers::new()),
        ],
    );
    assert_eq!(label(&app, entity), "");
    assert_eq!(
        app.world().get::<PromptHistory>(entity).unwrap().entries(),
        &["one".to_string(), "two".to_string()]
    );

    keys(
        &mut app,
        vec![KeyEvent::named(NamedKey::ArrowUp, Modifiers::new())],
    );
    assert_eq!(label(&app, entity), "two");
    keys(
        &mut app,
        vec![KeyEvent::named(NamedKey::ArrowUp, Modifiers::new())],
    );
    assert_eq!(label(&app, entity), "one");
    keys(
        &mut app,
        vec![KeyEvent::named(NamedKey::ArrowDown, Modifiers::new())],
    );
    assert_eq!(label(&app, entity), "two");
    keys(
        &mut app,
        vec![KeyEvent::named(NamedKey::ArrowDown, Modifiers::new())],
    );
    assert_eq!(label(&app, entity), "");

    keys(
        &mut app,
        vec![KeyEvent::named(NamedKey::ArrowUp, Modifiers::new())],
    );
    assert_eq!(label(&app, entity), "two");
    keys(
        &mut app,
        vec![KeyEvent::named(NamedKey::ArrowUp, Modifiers::new())],
    );
    type_text(&mut app, "x");
    assert_eq!(label(&app, entity), "onex");
    keys(
        &mut app,
        vec![KeyEvent::named(NamedKey::ArrowUp, Modifiers::new())],
    );
    assert_eq!(label(&app, entity), "two");

    keys(
        &mut app,
        vec![KeyEvent::named(NamedKey::Enter, Modifiers::new())],
    );
    assert_eq!(
        app.world().get::<PromptHistory>(entity).unwrap().entries(),
        &["one".to_string(), "two".to_string()]
    );
}

#[test]
fn up_inside_a_line_does_not_recall() {
    let (mut app, entity) = boot("");
    type_text(&mut app, "old");
    keys(
        &mut app,
        vec![KeyEvent::named(NamedKey::Enter, Modifiers::new())],
    );
    keys(
        &mut app,
        vec![
            KeyEvent::character('a', ctrl()),
            KeyEvent::named(NamedKey::Backspace, Modifiers::new()),
            KeyEvent::character('a', Modifiers::new()),
            KeyEvent::named(NamedKey::Enter, shift()),
            KeyEvent::character('b', Modifiers::new()),
        ],
    );
    assert_eq!(label(&app, entity), "a\nb");
    keys(
        &mut app,
        vec![KeyEvent::named(NamedKey::ArrowUp, Modifiers::new())],
    );
    assert_eq!(label(&app, entity), "a\nb");
    // The caret keeps the column of 'b', which is the end of the first line.
    assert_eq!(caret(&app, entity).index, 1);
    assert_eq!(
        app.world().get::<PromptHistory>(entity).unwrap().entries(),
        &["old".to_string()]
    );
}

#[test]
fn the_row_grows_then_scrolls_and_soft_wraps() {
    let (app, entity) = boot("");
    assert_eq!(app.world().get::<Rect>(entity).unwrap().h, 36.0);

    let (mut app, entity) = boot("");
    keys(&mut app, vec![KeyEvent::named(NamedKey::Enter, shift())]);
    assert_eq!(label(&app, entity), "\n");
    assert_eq!(
        app.world().get::<Rect>(entity).unwrap().h,
        8.0 + 2.0 * CELL_H + 8.0
    );

    let (app, entity) = boot_sized("a\nb\nc", 200.0, Prompt::new().max_lines(2));
    assert_eq!(
        app.world().get::<Rect>(entity).unwrap().h,
        8.0 + 2.0 * CELL_H + 8.0
    );
    assert_eq!(app.world().get::<Prompt>(entity).unwrap().scroll, CELL_H);

    let (mut app, entity) = boot_sized("abcdefghij", 88.0, Prompt::new());
    assert!(!label(&app, entity).contains('\n'));
    assert_eq!(
        app.world().get::<Rect>(entity).unwrap().h,
        8.0 + 3.0 * CELL_H + 8.0
    );
    let before = caret(&app, entity).index;
    keys(
        &mut app,
        vec![KeyEvent::named(NamedKey::ArrowUp, Modifiers::new())],
    );
    assert!(caret(&app, entity).index < before);
    assert!(!label(&app, entity).contains('\n'));
}

#[test]
fn ime_preedit_stays_out_of_the_label_until_commit() {
    let (mut app, entity) = boot("hi");
    app.world_mut().resource_mut::<Input>().unwrap().ime_preedit = "xy".into();
    app.update();
    assert_eq!(label(&app, entity), "hi");
    assert_eq!(app.world().get::<ImePreview>(entity).unwrap().text, "xy");
    assert!(app
        .world()
        .get::<TextLayout>(entity)
        .unwrap()
        .glyphs
        .iter()
        .any(|glyph| glyph.ch == 'x'));
    assert!(submitted(&app).is_empty());

    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.ime_preedit = "xy".into();
    input
        .keys
        .push(KeyEvent::named(NamedKey::Enter, Modifiers::new()));
    app.update();
    assert_eq!(label(&app, entity), "hi");
    assert!(submitted(&app).is_empty());

    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.ime_preedit.clear();
    input.ime_commit = "z".into();
    input
        .keys
        .push(KeyEvent::named(NamedKey::Enter, Modifiers::new()));
    app.update();
    assert_eq!(label(&app, entity), "hiz");
    assert!(app.world().get::<ImePreview>(entity).is_none());
    assert!(submitted(&app).is_empty());
}

#[test]
fn ctrl_shift_z_redoes_and_does_not_insert() {
    let (mut app, entity) = boot("");
    type_text(&mut app, "ab");
    keys(&mut app, vec![KeyEvent::character('z', ctrl())]);
    assert_eq!(label(&app, entity), "");
    let mut redo = KeyEvent::character('z', mods(true, true, false, false));
    redo.text = Some("Z".into());
    keys(&mut app, vec![redo]);
    assert_eq!(label(&app, entity), "ab");
}

#[test]
fn typed_runs_share_one_undo_step() {
    let (mut app, entity) = boot("");
    type_text(&mut app, "ab");
    type_text(&mut app, " ");
    type_text(&mut app, "c");
    keys(&mut app, vec![KeyEvent::character('z', ctrl())]);
    assert_eq!(label(&app, entity), "ab ");
    keys(&mut app, vec![KeyEvent::character('z', ctrl())]);
    assert_eq!(label(&app, entity), "ab");
    keys(&mut app, vec![KeyEvent::character('z', ctrl())]);
    assert_eq!(label(&app, entity), "");

    type_text(&mut app, "ab");
    keys(
        &mut app,
        vec![KeyEvent::named(NamedKey::ArrowLeft, Modifiers::new())],
    );
    keys(
        &mut app,
        vec![KeyEvent::named(NamedKey::ArrowRight, Modifiers::new())],
    );
    type_text(&mut app, "c");
    keys(&mut app, vec![KeyEvent::character('z', ctrl())]);
    assert_eq!(label(&app, entity), "ab");
}

#[test]
fn a_pause_starts_a_new_undo_step() {
    let (mut app, entity) = boot("");
    let clock = Arc::new(ManualClock::new());
    app.set_clock(clock.clone());
    type_text(&mut app, "a");
    clock.advance(Duration::from_millis(700));
    type_text(&mut app, "b");
    keys(&mut app, vec![KeyEvent::character('z', ctrl())]);
    assert_eq!(label(&app, entity), "a");
    keys(&mut app, vec![KeyEvent::character('z', ctrl())]);
    assert_eq!(label(&app, entity), "");
}

#[test]
fn mouse_selection_sets_the_primary_and_middle_click_pastes_it() {
    let (mut app, entity) = boot("hello");
    app.world_mut().insert_resource(Platform::X11);
    app.world_mut().insert(
        entity,
        Caret {
            index: 0,
            anchor: 0,
        },
    );
    let rect = *app.world().get::<Rect>(entity).unwrap();
    {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.shift = true;
        input.pointer.button = PointerButton::Left;
        input.pointer.pressed = true;
        input.pointer.down = true;
        input.pointer.released = false;
        input.pointer.position = [
            rect.x + TEXT_ORIGIN_X + CELL_W * 3.0 + 1.0,
            rect.y + TEXT_ORIGIN_Y + 1.0,
        ];
    }
    app.update();
    {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.shift = false;
        input.pointer.pressed = false;
        input.pointer.down = false;
        input.pointer.released = true;
    }
    app.update();
    let clipboard = app.world().resource::<Clipboard>().unwrap();
    assert_eq!(clipboard.primary, "hel");
    assert!(clipboard.primary_dirty);

    {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.pointer.button = PointerButton::Middle;
        input.pointer.pressed = true;
        input.pointer.down = true;
        input.pointer.released = false;
        input.pointer.position = [rect.x + TEXT_ORIGIN_X + 1.0, rect.y + TEXT_ORIGIN_Y + 1.0];
    }
    app.update();
    assert_eq!(label(&app, entity), "helhello");
    {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.pointer.pressed = false;
        input.pointer.down = false;
        input.pointer.released = false;
    }
    keys(&mut app, vec![KeyEvent::character('z', ctrl())]);
    assert_eq!(label(&app, entity), "hello");

    app.world_mut().insert_resource(Platform::Windows);
    app.world_mut()
        .resource_mut::<Clipboard>()
        .unwrap()
        .primary
        .clear();
    app.world_mut()
        .resource_mut::<Clipboard>()
        .unwrap()
        .primary_dirty = false;
    {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.pointer.button = PointerButton::Left;
        input.pointer.pressed = false;
        input.pointer.down = false;
        input.pointer.released = true;
        input.pointer.position = [
            rect.x + TEXT_ORIGIN_X + CELL_W * 3.0 + 1.0,
            rect.y + TEXT_ORIGIN_Y + 1.0,
        ];
    }
    app.update();
    let clipboard = app.world().resource::<Clipboard>().unwrap();
    assert!(clipboard.primary.is_empty());
    assert!(!clipboard.primary_dirty);
}

fn mods(shift: bool, ctrl: bool, alt: bool, super_key: bool) -> Modifiers {
    Modifiers {
        shift,
        ctrl,
        alt,
        super_key,
    }
}
