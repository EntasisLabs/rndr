use rndr_core::{
    events, request_file_dialog, resolve_ime, take_file_dialog, text_frame, App, Axis, Button,
    CapturesKeys, Chrome, Color, CountingWake, CursorImage, Desktop, FileDialog, Focus, ImeEvent,
    Input, KeyEvent, Label, Layout, List, ListKeys, Modifiers, NamedKey, PointerButton, Rect, Size,
    TextField, Viewport, WindowId, WindowMsg,
};
use std::sync::Arc;

fn key_text(text: &str) -> KeyEvent {
    let mut event = KeyEvent::character(text.chars().next().unwrap_or('a'), Modifiers::new());
    event.text = Some(text.to_string());
    event
}

#[test]
fn resolve_ime_strips_the_keys_that_built_a_commit() {
    let resolved = resolve_ime(&[
        ImeEvent::Enabled,
        ImeEvent::Key(key_text("a")),
        ImeEvent::Preedit("a".into(), None),
        ImeEvent::Preedit(String::new(), None),
        ImeEvent::Commit("あ".into()),
        ImeEvent::Key(key_text("あ")),
    ]);
    assert!(resolved.enabled);
    assert_eq!(resolved.commit, "あ");
    assert!(resolved.preedit.is_empty());
    assert!(resolved.keys.iter().all(|event| event.text.is_none()));
}

#[test]
fn text_frame_drops_composition_keys_and_a_duplicate_commit() {
    let mut input = Input {
        ime_enabled: true,
        ime_preedit: "ka".into(),
        chars: "a".into(),
        key: Some(rndr_core::Key::Enter),
        ..Input::default()
    };
    input.keys.push(key_text("a"));
    let frame = text_frame(&input);
    assert!(frame.keys.is_empty() || frame.keys.iter().all(|event| event.text.is_none()));
    assert!(frame.chars.is_empty());
    assert!(frame.legacy_key.is_none());

    input.ime_preedit.clear();
    input.ime_commit = "か".into();
    input.keys = vec![key_text("か")];
    input.chars = "か".into();
    let frame = text_frame(&input);
    assert_eq!(frame.commit, "か");
    assert!(frame.chars.is_empty());
    assert!(frame.keys.iter().all(|event| event.text.is_none()));
}

#[test]
fn a_field_inserts_a_commit_once() {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(240.0, 80.0));
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
        Label::new("hi"),
        TextField,
        CapturesKeys,
        rndr_core::Focusable,
    ));
    app.world_mut().insert_resource(Focus {
        entity: Some(entity),
    });
    app.update();
    {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.ime_enabled = true;
        input.ime_preedit = "a".into();
        input.keys.push(key_text("a"));
        input.chars = "a".into();
    }
    app.update();
    assert_eq!(app.world().get::<Label>(entity).unwrap().text, "hi");
    {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.ime_preedit.clear();
        input.ime_enabled = true;
        input.ime_commit = "あ".into();
        input.keys.push(key_text("あ"));
        input.chars = "あ".into();
    }
    app.update();
    assert_eq!(app.world().get::<Label>(entity).unwrap().text, "hiあ");
}

#[test]
fn windows_exchange_a_saved_message() {
    let (left, right) = Desktop::pair();
    let mut sender = App::new();
    let mut receiver = App::new();
    sender.world_mut().insert_resource(left.clone());
    receiver.world_mut().insert_resource(right.clone());
    let wake = CountingWake::new();
    rndr_core::install_wake(sender.world(), Arc::new(wake.clone()));
    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Saved(String);
    impl rndr_core::Event for Saved {}
    left.broadcast(Saved("notes".into()));
    assert_eq!(wake.hits(), 1);
    rndr_core::deliver(receiver.world_mut(), right.id());
    receiver.update();
    let mail = events::<WindowMsg<Saved>>(receiver.world());
    assert_eq!(mail.len(), 1);
    assert_eq!(mail[0].from, WindowId(1));
    assert_eq!(mail[0].body, Saved("notes".into()));
    rndr_core::deliver(sender.world_mut(), left.id());
    sender.update();
    assert!(events::<WindowMsg<Saved>>(sender.world()).is_empty());
}

#[test]
fn a_direct_message_reaches_the_named_window() {
    let (left, right) = Desktop::pair();
    let mut receiver = App::new();
    receiver.world_mut().insert_resource(right.clone());
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Ping;
    impl rndr_core::Event for Ping {}
    left.send(right.id(), Ping);
    rndr_core::deliver(receiver.world_mut(), right.id());
    receiver.update();
    assert_eq!(events::<WindowMsg<Ping>>(receiver.world()).len(), 1);
}

#[test]
fn file_dialog_request_is_replaced_until_taken() {
    let mut app = App::new();
    request_file_dialog(app.world_mut(), FileDialog::open().title("first"));
    request_file_dialog(app.world_mut(), FileDialog::save().title("second"));
    let taken = take_file_dialog(app.world_mut()).unwrap();
    assert_eq!(taken.title, "second");
    assert!(take_file_dialog(app.world_mut()).is_none());
}

#[test]
fn rotate_zero_is_identity_and_a_quarter_turn_swaps_extents() {
    let rgba = vec![255u8; 10 * 2 * 4];
    let image = CursorImage::new(10, 2, 0, 0, rgba).unwrap();
    let same = rndr_core::rotate_cursor(&image, 0.0);
    assert_eq!(same, image);
    let turned = rndr_core::rotate_cursor(&image, std::f32::consts::FRAC_PI_2);
    assert!(turned.height > turned.width);
    assert!(turned.hotspot_x < turned.width);
    assert!(turned.hotspot_y < turned.height);
}

#[test]
fn a_list_without_a_context_moves_from_the_keyboard() {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(200.0, 200.0));
    let list = app.world_mut().spawn((
        Layout {
            axis: Axis::Column,
            gap: 0.0,
            padding: 0.0,
            width: Size::Px(120.0),
            height: Size::Px(80.0),
            min_width: 0.0,
            min_height: 0.0,
        },
        List::new(5, 20.0),
        ListKeys,
    ));
    app.world_mut()
        .insert_resource(Focus { entity: Some(list) });
    app.update();
    assert!(app.world().has::<rndr_core::Focusable>(list));
    assert_eq!(app.world().get::<List>(list).unwrap().selected, Some(0));
    let platform = rndr_core::platform_of(app.world());
    app.world_mut().resource_mut::<Input>().unwrap().push_key(
        KeyEvent::named(NamedKey::ArrowDown, Modifiers::new()),
        platform,
    );
    app.update();
    assert_eq!(app.world().get::<List>(list).unwrap().selected, Some(1));
}

#[test]
fn access_tree_names_a_button_and_activation_fires_it() {
    let mut app = App::new();
    app.add_plugin(Chrome);
    let button = app.world_mut().spawn((
        Button::new("save"),
        Label::new("Save"),
        Rect {
            x: 1.0,
            y: 2.0,
            w: 40.0,
            h: 16.0,
        },
    ));
    let tree = rndr_core::access_tree(app.world());
    let node = tree
        .nodes
        .iter()
        .find(|node| node.role == rndr_core::AccessRole::Button)
        .unwrap();
    assert_eq!(node.name, "Save");
    assert!(node.focusable);
    rndr_core::apply_access(app.world_mut(), rndr_core::AccessCommand::Activate(button));
    assert_eq!(
        app.world()
            .resource::<rndr_core::Fired>()
            .unwrap()
            .name
            .as_deref(),
        Some("save")
    );
}

#[test]
fn pointer_button_other_codes_fit_in_sixteen_bits() {
    let mut buttons = rndr_core::PointerButtons::new();
    buttons.set(PointerButton::Other(8), true);
    assert!(buttons.contains(PointerButton::Other(8)));
    buttons.set(PointerButton::Other(40), true);
    assert!(!buttons.contains(PointerButton::Other(40)));
}
