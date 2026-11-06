use rndr_app::{button, column, field, FormNavigation};
use rndr_core::*;

fn form() -> (App, Entity, Entity, Entity) {
    let mut app = App::new();
    app.add_plugin(Shell)
        .add_plugin(Chrome)
        .add_plugin(FormNavigation);
    let w = app.world_mut();
    let root = w.spawn(column(Size::Fill));
    let a = field(w, root, "a", "A");
    let b = field(w, root, "b", "B");
    let save = button(w, root, "save", "Save");
    app.update();
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(a);
    (app, a, b, save)
}
fn queue(app: &mut App, keys: impl IntoIterator<Item = KeyEvent>) {
    let platform = platform_of(app.world());
    for key in keys {
        app.world_mut()
            .resource_mut::<Input>()
            .unwrap()
            .push_key(key, platform);
    }
    app.update();
}
fn typed(ch: char) -> KeyEvent {
    KeyEvent::character(ch, Modifiers::new())
}
fn tab(reverse: bool) -> KeyEvent {
    let mut modifiers = Modifiers::new();
    modifiers.shift = reverse;
    KeyEvent::named(NamedKey::Tab, modifiers)
}

#[test]
fn queued_text_before_tab_stays_in_source_field() {
    let (mut app, a, b, _) = form();
    queue(&mut app, [typed('x'), tab(false)]);
    assert_eq!(app.world().get::<Label>(a).unwrap().text, "Ax");
    assert_eq!(app.world().get::<Label>(b).unwrap().text, "B");
    assert_eq!(app.world().resource::<Focus>().unwrap().entity, Some(b));
    assert!(events::<TextChanged>(app.world())
        .iter()
        .any(|e| e.entity == a && e.text == "Ax"));
}

#[test]
fn mixed_queue_routes_each_segment_including_reverse_and_save() {
    let (mut app, a, b, save) = form();
    queue(
        &mut app,
        [
            typed('x'),
            tab(false),
            typed('y'),
            tab(true),
            typed('z'),
            tab(false),
            typed('!'),
            tab(false),
            KeyEvent::named(NamedKey::Enter, Modifiers::new()),
        ],
    );
    assert_eq!(app.world().get::<Label>(a).unwrap().text, "Axz");
    assert_eq!(app.world().get::<Label>(b).unwrap().text, "By!");
    assert_eq!(app.world().resource::<Focus>().unwrap().entity, Some(save));
    assert_eq!(
        app.world()
            .resource::<ActionIntents>()
            .unwrap()
            .pending()
            .back()
            .map(|i| i.name.as_str()),
        Some("save")
    );
}

#[test]
fn queued_suffix_after_tab_is_edited_once() {
    let (mut app, a, b, _) = form();
    queue(&mut app, [tab(false), typed('y')]);
    assert_eq!(app.world().get::<Label>(a).unwrap().text, "A");
    assert_eq!(app.world().get::<Label>(b).unwrap().text, "By");
    app.update();
    assert_eq!(app.world().get::<Label>(b).unwrap().text, "By");
}

#[test]
fn legacy_chars_before_tab_are_not_redirected() {
    let (mut app, a, b, _) = form();
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.chars = "x".into();
    input.key = Some(Key::Tab);
    app.update();
    assert_eq!(app.world().get::<Label>(a).unwrap().text, "Ax");
    assert_eq!(app.world().get::<Label>(b).unwrap().text, "B");
}

#[test]
fn released_synthetic_and_modified_tabs_do_not_move_focus() {
    let (mut app, a, _, _) = form();
    let mut released = tab(false);
    released.released = true;
    let mut synthetic = tab(false);
    synthetic.synthetic = true;
    let mut modified = tab(false);
    modified.modifiers.ctrl = true;
    queue(&mut app, [released, synthetic, modified]);
    assert_eq!(app.world().resource::<Focus>().unwrap().entity, Some(a));
}

#[test]
fn form_tab_wraps_skips_hidden_disabled_and_preserves_ime() {
    let mut app = App::new();
    app.add_plugin(Shell)
        .add_plugin(Chrome)
        .add_plugin(FormNavigation);
    let w = app.world_mut();
    let root = w.spawn(column(Size::Fill));
    let first = button(w, root, "first", "First");
    let disabled = button(w, root, "disabled", "Disabled");
    w.get_mut::<Button>(disabled).unwrap().enabled = false;
    let hidden_page = w.spawn((column(Size::Fill), Hidden));
    set_parent(w, hidden_page, root);
    let _hidden = field(w, hidden_page, "Hidden", "never focus");
    let last = field(w, root, "Last", "日本語");
    app.update();
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(first);
    app.world_mut().resource_mut::<Input>().unwrap().key = Some(Key::Tab);
    app.update();
    assert_eq!(app.world().resource::<Focus>().unwrap().entity, Some(last));
    assert_eq!(app.world().get::<Label>(last).unwrap().text, "日本語");
    let tab = KeyEvent::named(NamedKey::Tab, Modifiers::new());
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(tab);
    app.world_mut().resource_mut::<Input>().unwrap().key = Some(Key::Tab);
    app.update();
    assert_eq!(app.world().resource::<Focus>().unwrap().entity, Some(first));
    let mut modifiers = Modifiers::new();
    modifiers.shift = true;
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(KeyEvent::named(NamedKey::Tab, modifiers));
    app.update();
    assert_eq!(app.world().resource::<Focus>().unwrap().entity, Some(last));
    app.world_mut().resource_mut::<Input>().unwrap().ime_preedit = "に".into();
    app.world_mut().resource_mut::<Input>().unwrap().key = Some(Key::Tab);
    app.update();
    assert_eq!(app.world().resource::<Focus>().unwrap().entity, Some(last));
}
