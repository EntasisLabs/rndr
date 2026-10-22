use rndr_core::{
    set_parent, standard_action, App, Axis, Buffer, CapturesKeys, Caret, Chrome, Clipboard, Color,
    Context, EditAction, Editor, Focus, Focusable, Input, Key, KeyBinding, KeyCode, KeyEvent,
    Keymap, KeymapFired, Keystroke, Label, Layout, LogicalKey, Modifiers, NamedKey, Platform, Size,
    TextField, Viewport,
};

fn mods(shift: bool, ctrl: bool, alt: bool, super_key: bool) -> Modifiers {
    Modifiers {
        shift,
        ctrl,
        alt,
        super_key,
    }
}

fn primary(platform: Platform, shift: bool) -> Modifiers {
    if platform.primary_is_super() {
        mods(shift, false, false, true)
    } else {
        mods(shift, true, false, false)
    }
}

fn letter(platform: Platform, ch: char, shift: bool) -> KeyEvent {
    KeyEvent::character(ch, primary(platform, shift))
}

fn named(key: NamedKey, modifiers: Modifiers) -> KeyEvent {
    KeyEvent::named(key, modifiers)
}

fn chord(platform: Platform, action: EditAction) -> KeyEvent {
    let mac = platform.primary_is_super();
    match action {
        EditAction::SelectAll => letter(platform, 'a', false),
        EditAction::Undo => letter(platform, 'z', false),
        EditAction::Redo => {
            if mac {
                letter(platform, 'z', true)
            } else {
                letter(platform, 'y', false)
            }
        }
        EditAction::Copy => letter(platform, 'c', false),
        EditAction::Cut => letter(platform, 'x', false),
        EditAction::Paste => letter(platform, 'v', false),
        EditAction::WordLeft => {
            if mac {
                named(NamedKey::ArrowLeft, mods(false, false, true, false))
            } else {
                named(NamedKey::ArrowLeft, primary(platform, false))
            }
        }
        EditAction::WordRight => {
            if mac {
                named(NamedKey::ArrowRight, mods(false, false, true, false))
            } else {
                named(NamedKey::ArrowRight, primary(platform, false))
            }
        }
        EditAction::LineStart => {
            if mac {
                named(NamedKey::ArrowLeft, primary(platform, false))
            } else {
                named(NamedKey::Home, Modifiers::new())
            }
        }
        EditAction::LineEnd => {
            if mac {
                named(NamedKey::ArrowRight, primary(platform, false))
            } else {
                named(NamedKey::End, Modifiers::new())
            }
        }
        EditAction::DocStart => {
            if mac {
                named(NamedKey::ArrowUp, primary(platform, false))
            } else {
                named(NamedKey::Home, primary(platform, false))
            }
        }
        EditAction::DocEnd => {
            if mac {
                named(NamedKey::ArrowDown, primary(platform, false))
            } else {
                named(NamedKey::End, primary(platform, false))
            }
        }
        EditAction::DeleteWordBackward => {
            if mac {
                named(NamedKey::Backspace, mods(false, false, true, false))
            } else {
                named(NamedKey::Backspace, primary(platform, false))
            }
        }
    }
}

#[test]
fn standard_actions_resolve_on_mac_and_windows() {
    for platform in [
        Platform::Mac,
        Platform::Windows,
        Platform::X11,
        Platform::Wayland,
    ] {
        for action in EditAction::ALL {
            let event = chord(platform, action);
            assert_eq!(
                standard_action(platform, &event),
                Some(action),
                "{action:?} on {platform:?}"
            );
        }
    }
}

#[test]
fn shift_selects_without_changing_a_motion() {
    let event = named(NamedKey::ArrowLeft, mods(true, false, true, false));
    assert_eq!(
        standard_action(Platform::Mac, &event),
        Some(EditAction::WordLeft)
    );
    let event = named(NamedKey::Home, mods(true, false, false, false));
    assert_eq!(
        standard_action(Platform::Windows, &event),
        Some(EditAction::LineStart)
    );
}

#[test]
fn copy_matches_the_physical_c_key_on_a_cyrillic_layout() {
    let event = KeyEvent {
        logical: LogicalKey::Character('с'),
        text: None,
        code: Some(KeyCode::Letter('c')),
        modifiers: mods(false, false, false, true),
        repeat: false,
    };
    assert_eq!(
        standard_action(Platform::Mac, &event),
        Some(EditAction::Copy)
    );
    assert_eq!(standard_action(Platform::Windows, &event), None);

    let ctrl = KeyEvent {
        logical: LogicalKey::Character('с'),
        text: None,
        code: Some(KeyCode::Letter('c')),
        modifiers: mods(false, true, false, false),
        repeat: false,
    };
    assert_eq!(
        standard_action(Platform::Windows, &ctrl),
        Some(EditAction::Copy)
    );
    assert_eq!(standard_action(Platform::Mac, &ctrl), None);
}

#[test]
fn primary_chord_is_not_copy_on_the_other_platform() {
    assert_eq!(
        standard_action(Platform::Mac, &letter(Platform::Windows, 'c', false)),
        None
    );
    assert_eq!(
        standard_action(Platform::Windows, &letter(Platform::Mac, 'c', false)),
        None
    );
}

#[test]
fn shim_keeps_only_the_first_key() {
    let mut input = Input::default();
    let backspace = named(NamedKey::Backspace, Modifiers::new());
    input.push_key(backspace.clone(), Platform::Windows);
    input.push_key(backspace, Platform::Windows);
    assert_eq!(input.keys.len(), 2);
    assert_eq!(input.key, Some(Key::Backspace));
}

fn field() -> (App, rndr_core::Entity) {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(240.0, 80.0));
    app.world_mut().insert_resource(Focus::default());
    let entity = app.world_mut().spawn((
        Layout {
            axis: Axis::Column,
            gap: 0.0,
            padding: 0.0,
            width: Size::Px(200.0),
            height: Size::Px(36.0),
            min_width: 0.0,
            min_height: 0.0,
        },
        Color::rgb(0.15, 0.16, 0.18),
        Label::new("ab"),
        TextField,
        Focusable,
        CapturesKeys,
    ));
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(entity);
    app.update();
    (app, entity)
}

fn editor(text: &str) -> (App, rndr_core::Entity) {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(240.0, 80.0));
    app.world_mut().insert_resource(Focus::default());
    let entity = app.world_mut().spawn((
        Layout {
            axis: Axis::Column,
            gap: 0.0,
            padding: 0.0,
            width: Size::Px(220.0),
            height: Size::Px(80.0),
            min_width: 0.0,
            min_height: 0.0,
        },
        Color::rgb(0.14, 0.15, 0.16),
        Label::new(""),
        Buffer::from_str(text),
        Editor::default(),
        CapturesKeys,
    ));
    app.update();
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(entity);
    (app, entity)
}

fn typed(text: &str) -> KeyEvent {
    KeyEvent {
        logical: text
            .chars()
            .next()
            .map(LogicalKey::Character)
            .unwrap_or(LogicalKey::Unidentified),
        text: Some(text.to_string()),
        code: None,
        modifiers: Modifiers::new(),
        repeat: false,
    }
}

#[test]
fn two_backspaces_in_one_frame_delete_two_chars() {
    let (mut app, entity) = field();
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input
        .keys
        .push(named(NamedKey::Backspace, Modifiers::new()));
    input
        .keys
        .push(named(NamedKey::Backspace, Modifiers::new()));
    app.update();
    assert_eq!(app.world().get::<Label>(entity).unwrap().text, "");
    assert_eq!(app.world().get::<Caret>(entity).unwrap().index, 0);
    app.update();
    assert_eq!(app.world().get::<Label>(entity).unwrap().text, "");

    let (mut app, entity) = editor("abcd");
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input
        .keys
        .push(named(NamedKey::Backspace, Modifiers::new()));
    input
        .keys
        .push(named(NamedKey::Backspace, Modifiers::new()));
    app.update();
    assert_eq!(app.world().get::<Buffer>(entity).unwrap().text(), "ab");
    assert_eq!(app.world().get::<Buffer>(entity).unwrap().caret, 2);
}

#[test]
fn unicode_text_is_inserted() {
    let (mut app, entity) = field();
    {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.keys.push(typed("é"));
        input.keys.push(typed("ß"));
        input.keys.push(typed("日本"));
    }
    app.update();
    assert_eq!(app.world().get::<Label>(entity).unwrap().text, "abéß日本");

    let (mut app, entity) = editor("");
    {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.keys.push(typed("é"));
        input.keys.push(typed("ß"));
        input.keys.push(typed("日本"));
    }
    app.update();
    assert_eq!(app.world().get::<Buffer>(entity).unwrap().text(), "éß日本");
}

#[test]
fn shortcut_modifier_does_not_insert_text() {
    let (mut app, entity) = field();
    app.world_mut().insert_resource(Platform::Windows);
    let mut event = KeyEvent::character('c', mods(false, true, false, false));
    event.text = Some("c".to_string());
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(event);
    app.update();
    assert_eq!(app.world().get::<Label>(entity).unwrap().text, "ab");

    let (mut app, entity) = field();
    app.world_mut().insert_resource(Platform::Mac);
    let mut event = KeyEvent::character('c', mods(false, false, false, true));
    event.text = Some("c".to_string());
    event.logical = LogicalKey::Character('с');
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(event);
    app.world_mut().insert(
        entity,
        Caret {
            index: 2,
            anchor: 0,
        },
    );
    app.update();
    assert_eq!(app.world().get::<Label>(entity).unwrap().text, "ab");
    assert_eq!(app.world().resource::<Clipboard>().unwrap().text, "ab");
}

#[test]
fn modifier_chords_insert_only_platform_text() {
    let (mut app, entity) = field();
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(named(NamedKey::Space, Modifiers::new()));
    app.update();
    assert_eq!(app.world().get::<Label>(entity).unwrap().text, "ab ");

    let (mut app, entity) = field();
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(named(NamedKey::Space, mods(true, false, false, false)));
    app.update();
    assert_eq!(app.world().get::<Label>(entity).unwrap().text, "ab ");

    let (mut app, entity) = field();
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(named(NamedKey::Space, mods(false, false, true, false)));
    app.update();
    assert_eq!(app.world().get::<Label>(entity).unwrap().text, "ab");

    let (mut app, entity) = field();
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(named(NamedKey::Space, mods(false, true, false, false)));
    app.update();
    assert_eq!(app.world().get::<Label>(entity).unwrap().text, "ab");

    let (mut app, entity) = field();
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(KeyEvent::character('e', mods(false, false, true, false)).with_text("é"));
    app.update();
    assert_eq!(app.world().get::<Label>(entity).unwrap().text, "abé");

    let (mut app, entity) = field();
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(KeyEvent::character('e', mods(false, true, true, false)).with_text("€"));
    app.update();
    assert_eq!(app.world().get::<Label>(entity).unwrap().text, "ab€");

    let (mut app, entity) = field();
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(KeyEvent::character('a', mods(false, false, false, true)).with_text("a"));
    app.update();
    assert_eq!(app.world().get::<Label>(entity).unwrap().text, "ab");
}

#[test]
fn field_uses_platform_line_start_and_undo() {
    let (mut app, entity) = field();
    app.world_mut().insert_resource(Platform::Windows);
    app.world_mut().get_mut::<Label>(entity).unwrap().text = "ab\ncd".to_string();
    app.world_mut().insert(
        entity,
        Caret {
            index: 5,
            anchor: 5,
        },
    );
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(chord(Platform::Windows, EditAction::LineStart));
    app.update();
    let caret = app.world().get::<Caret>(entity).unwrap();
    assert_eq!((caret.anchor, caret.index), (3, 3));

    let (mut app, entity) = field();
    app.world_mut().insert_resource(Platform::Mac);
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(typed("é"));
    app.update();
    assert_eq!(app.world().get::<Label>(entity).unwrap().text, "abé");
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(chord(Platform::Mac, EditAction::Undo));
    app.update();
    assert_eq!(app.world().get::<Label>(entity).unwrap().text, "ab");
}

#[test]
fn editor_deletes_the_previous_word_on_windows() {
    let (mut app, entity) = editor("hello world");
    app.world_mut().insert_resource(Platform::Windows);
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(chord(Platform::Windows, EditAction::DeleteWordBackward));
    app.update();
    assert_eq!(app.world().get::<Buffer>(entity).unwrap().text(), "hello ");
}

fn press(map: &mut Keymap, stack: &[&str], event: KeyEvent, now: u64) -> Vec<String> {
    let stack: Vec<String> = stack.iter().map(|part| (*part).to_string()).collect();
    map.dispatch(&stack, &[event], Platform::Windows, now)
        .actions
}

#[test]
fn keymap_prefers_the_deepest_context_then_the_later_binding() {
    let mut map = Keymap::default();
    let key = Keystroke::character('p');
    map.bind(KeyBinding::new("global", [key]));
    map.bind(KeyBinding::new("palette", [key]).context("Palette"));
    map.bind(KeyBinding::new("list", [key]).context("Palette > List"));
    map.bind(KeyBinding::new("list-later", [key]).context("Palette > List"));

    let event = KeyEvent::character('p', Modifiers::new());
    assert_eq!(
        press(&mut map, &["Palette", "List"], event.clone(), 0),
        vec!["list-later".to_string()]
    );
    assert_eq!(
        press(&mut map, &["Palette"], event.clone(), 0),
        vec!["palette".to_string()]
    );
    assert_eq!(press(&mut map, &[], event, 0), vec!["global".to_string()]);

    let mut map = Keymap::default();
    map.bind(KeyBinding::new("editor", [Keystroke::character('k')]).context("Editor"));
    map.bind(KeyBinding::new("global", [Keystroke::character('k')]));
    let event = KeyEvent::character('k', Modifiers::new());
    assert_eq!(
        press(&mut map, &["Editor"], event, 0),
        vec!["editor".to_string()]
    );
}

#[test]
fn keymap_context_comes_from_the_focused_stack() {
    let mut app = App::new();
    app.world_mut().insert_resource(Focus::default());
    let palette = app.world_mut().spawn(Context::new("Palette"));
    let list = app.world_mut().spawn(Context::new("List"));
    set_parent(app.world_mut(), list, palette);
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(list);
    let mut keymap = Keymap::default();
    let key = Keystroke::character('p');
    keymap.bind(KeyBinding::new("palette", [key]).context("Palette"));
    keymap.bind(KeyBinding::new("list", [key]).context("Palette > List"));
    app.world_mut().insert_resource(keymap);
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(KeyEvent::character('p', Modifiers::new()));
    rndr_core::apply_keymap(app.world_mut());
    assert_eq!(
        app.world().resource::<KeymapFired>().unwrap().actions,
        vec!["list".to_string()]
    );
}

#[test]
fn multi_stroke_waits_and_times_out() {
    let mut map = Keymap::default();
    map.timeout_ms = 1000;
    map.bind(KeyBinding::new(
        "save-all",
        [
            Keystroke::character('k').ctrl(),
            Keystroke::character('s').ctrl(),
        ],
    ));
    map.bind(KeyBinding::new("open", [Keystroke::character('k').ctrl()]));
    let ctrl = mods(false, true, false, false);
    let k = KeyEvent::character('k', ctrl);
    let s = KeyEvent::character('s', ctrl);

    let first = map.dispatch(&[], std::slice::from_ref(&k), Platform::Windows, 0);
    assert!(first.actions.is_empty());
    assert_eq!(map.pending_len(), 1);
    assert_eq!(first.consumed, vec![0]);

    let done = map.dispatch(&[], std::slice::from_ref(&s), Platform::Windows, 500);
    assert_eq!(done.actions, vec!["save-all".to_string()]);
    assert_eq!(map.pending_len(), 0);

    let armed = map.dispatch(&[], std::slice::from_ref(&k), Platform::Windows, 0);
    assert!(armed.actions.is_empty());
    let expired = map.dispatch(&[], &[], Platform::Windows, 1001);
    assert_eq!(expired.actions, vec!["open".to_string()]);
    assert_eq!(map.pending_len(), 0);
    let late = map.dispatch(&[], std::slice::from_ref(&s), Platform::Windows, 1002);
    assert!(late.actions.is_empty());
}

#[test]
fn reverse_lookup_returns_the_winning_strokes() {
    let mut map = Keymap::default();
    let key = Keystroke::character('p');
    let sequence = vec![
        Keystroke::character('k').ctrl(),
        Keystroke::character('s').ctrl(),
    ];
    map.bind(KeyBinding::new("palette", [key]).context("Palette"));
    map.bind(KeyBinding::new("list", [key]).context("Palette > List"));
    map.bind(KeyBinding::new("save-all", sequence.clone()).context("Palette > List"));

    let stack = ["Palette".to_string(), "List".to_string()];
    assert_eq!(map.lookup("list", &stack), vec![vec![key]]);
    assert!(map.lookup("palette", &stack).is_empty());
    assert_eq!(map.lookup("save-all", &stack), vec![sequence.clone()]);
    assert_eq!(sequence[0].label(Platform::Windows), "Ctrl+K");
    assert_eq!(
        Keystroke::character('c').primary().label(Platform::Mac),
        "Cmd+C"
    );
}

#[test]
fn shortcut_text_uses_symbols_on_mac_and_words_elsewhere() {
    let open = KeyBinding::new("open", [Keystroke::character('k').primary()]);
    assert_eq!(open.shortcut(Platform::Mac), "⌘K");
    assert_eq!(open.shortcut(Platform::Windows), "Ctrl+K");

    let go = KeyBinding::new(
        "go",
        [
            Keystroke::character('g').primary(),
            Keystroke::character('m'),
        ],
    );
    assert_eq!(go.shortcut(Platform::Mac), "⌘G M");
    assert_eq!(go.shortcut(Platform::Windows), "Ctrl+G M");
    assert_eq!(go.shortcut(Platform::X11), "Ctrl+G M");

    let mut map = Keymap::default();
    map.bind(open);
    map.bind(go);
    assert_eq!(map.shortcut("open", &[], Platform::Mac), "⌘K");
    assert_eq!(map.shortcut("go", &[], Platform::Windows), "Ctrl+G M");
    assert_eq!(map.shortcut("missing", &[], Platform::Windows), "");
}
