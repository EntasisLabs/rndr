//! Opt-in canonical field transactions, using real queued keys and paint frames.
use rndr_core::*;
use rndr_ui::UiPlugin;

fn transaction(world: &mut World, _: Entity, event: FieldTransaction) {
    if event == FieldTransaction::Cancel {
        if let Some(c) = world.resource_mut::<Cancels>() {
            c.0 += 1;
        }
    }
}
fn setup(captures: bool) -> (App, Entity, Entity) {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.world_mut().insert_resource(Viewport::new(400.0, 300.0));
    let root = app.world_mut().spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    let mut fields = Vec::new();
    for name in ["First property", "Second property"] {
        let e = app.world_mut().spawn((
            Layout {
                width: Size::Fill,
                height: Size::Px(40.0),
                ..Layout::column()
            },
            Label::new(""),
            TextField,
            Focusable,
            Semantics {
                name: Some(name.into()),
                ..Semantics::default()
            },
        ));
        set_parent(app.world_mut(), e, root);
        app.world_mut().insert(e, FieldTransactions(transaction));
        if captures {
            app.world_mut().insert(e, CapturesKeys);
        }
        fields.push(e);
    }
    rndr_paint::frame(&mut app);
    assert!(request_focus(app.world_mut(), fields[0]));
    (app, fields[0], fields[1])
}
fn character(c: char) -> KeyEvent {
    KeyEvent::character(c, Modifiers::default())
}
fn named(k: NamedKey) -> KeyEvent {
    KeyEvent::named(k, Modifiers::default())
}
fn batch(app: &mut App, keys: Vec<KeyEvent>) {
    app.world_mut().resource_mut::<Input>().unwrap().keys = keys;
    rndr_paint::frame(app);
}
fn submitted(app: &App) -> Vec<String> {
    events::<Submitted>(app.world())
        .iter()
        .map(|e| e.text.clone())
        .collect()
}

#[test]
fn enter_snapshot_precedes_later_typing_in_same_native_batch() {
    let (mut app, field, _) = setup(true);
    batch(
        &mut app,
        vec![character('1'), named(NamedKey::Enter), character('2')],
    );
    println!(
        "draft={:?}, submitted={:?}",
        app.world().get::<Label>(field).unwrap().text,
        submitted(&app)
    );
    assert_eq!(
        submitted(&app),
        vec!["1"],
        "commit must not include text typed after Enter"
    );
}

#[test]
fn two_enter_boundaries_keep_distinct_snapshots() {
    let (mut app, _, _) = setup(true);
    batch(
        &mut app,
        vec![
            character('1'),
            named(NamedKey::Enter),
            character('2'),
            named(NamedKey::Enter),
        ],
    );
    println!("submitted={:?}", submitted(&app));
    assert_eq!(
        submitted(&app),
        vec!["1", "12"],
        "one snapshot per non-repeat submit boundary"
    );
}

#[test]
fn repeated_enter_is_not_a_second_commit_request() {
    let (mut app, _, _) = setup(true);
    batch(&mut app, vec![character('1'), named(NamedKey::Enter)]);
    assert_eq!(submitted(&app), vec!["1"]);
    let mut repeat = named(NamedKey::Enter);
    repeat.repeat = true;
    batch(&mut app, vec![repeat]);
    println!("repeat submitted={:?}", submitted(&app));
    assert!(
        submitted(&app).is_empty(),
        "held Enter must not produce repeat transactions"
    );
}

#[test]
fn text_on_each_side_of_tab_goes_only_to_its_chronological_target() {
    let (mut app, first, second) = setup(false);
    batch(
        &mut app,
        vec![character('a'), named(NamedKey::Tab), character('b')],
    );
    let a = &app.world().get::<Label>(first).unwrap().text;
    let b = &app.world().get::<Label>(second).unwrap().text;
    println!(
        "first={a:?}, second={b:?}, focus={:?}",
        app.world().resource::<Focus>().unwrap().entity
    );
    assert_eq!(
        (a.as_str(), b.as_str()),
        ("a", "b"),
        "text before Tab must not migrate into the next property"
    );
}

#[derive(Default)]
struct Cancels(usize);
fn escape_hook(_world: &mut World, _: Entity, event: &KeyEvent) -> bool {
    if event.logical == LogicalKey::Named(NamedKey::Escape) {
        panic!("legacy composite hook must not be used as an editor dispatcher");
    }
    false
}
#[test]
fn capture_field_can_reach_canonical_escape_hook() {
    let (mut app, field, _) = setup(true);
    app.world_mut().insert_resource(Cancels::default());
    app.world_mut().insert(field, KeyNavigation(escape_hook));
    batch(&mut app, vec![character('x'), named(NamedKey::Escape)]);
    let count = app.world().resource::<Cancels>().unwrap().0;
    println!(
        "cancel_hook_calls={count}, remaining_keys={:?}",
        app.world().resource::<Input>().unwrap().keys
    );
    assert_eq!(
        count, 1,
        "Escape must be visible at the editor's ordered transaction boundary"
    );
}
#[test]
fn ime_preedit_and_commit_do_not_submit_or_duplicate_text() {
    let (mut app, e, _) = setup(true);
    {
        let i = app.world_mut().resource_mut::<Input>().unwrap();
        i.ime_enabled = true;
        i.ime_preedit = "に".into();
        i.keys = vec![character('n'), named(NamedKey::Enter)];
    }
    rndr_paint::frame(&mut app);
    assert_eq!(app.world().get::<Label>(e).unwrap().text, "");
    assert!(submitted(&app).is_empty());
    {
        let i = app.world_mut().resource_mut::<Input>().unwrap();
        i.ime_preedit.clear();
        i.ime_commit = "日".into();
        i.keys = vec![character('日'), named(NamedKey::Enter)];
    }
    rndr_paint::frame(&mut app);
    assert_eq!(app.world().get::<Label>(e).unwrap().text, "日");
    assert!(submitted(&app).is_empty());
    batch(&mut app, vec![named(NamedKey::Enter)]);
    assert_eq!(submitted(&app), vec!["日"]);
}
#[test]
fn readonly_allows_select_copy_but_not_paste_delete_or_submit() {
    let (mut app, e, _) = setup(true);
    reset_field_draft(app.world_mut(), e, "Read");
    app.world_mut().get_mut::<Semantics>(e).unwrap().readonly = true;
    let mut m = Modifiers::default();
    m.super_key = Platform::host() == Platform::Mac;
    m.ctrl = !m.super_key;
    batch(
        &mut app,
        vec![
            KeyEvent::character('a', m),
            KeyEvent::character('c', m),
            character('x'),
            named(NamedKey::Backspace),
            named(NamedKey::Enter),
        ],
    );
    assert_eq!(app.world().get::<Label>(e).unwrap().text, "Read");
    assert_eq!(app.world().resource::<Clipboard>().unwrap().text, "Read");
    assert!(submitted(&app).is_empty());
}
#[test]
fn reset_discards_local_undo_history_and_rejects_other_entities() {
    let (mut app, e, other) = setup(true);
    batch(&mut app, vec![character('a'), character('b')]);
    assert!(reset_field_draft(app.world_mut(), e, "New"));
    let mut m = Modifiers::default();
    m.super_key = Platform::host() == Platform::Mac;
    m.ctrl = !m.super_key;
    batch(&mut app, vec![KeyEvent::character('z', m)]);
    assert_eq!(app.world().get::<Label>(e).unwrap().text, "New");
    app.world_mut().remove::<FieldTransactions>(other);
    assert!(!reset_field_draft(app.world_mut(), other, "bad"));
}
#[test]
fn released_synthetic_and_repeat_cancel_never_transact() {
    let (mut app, _, _) = setup(true);
    app.world_mut().insert_resource(Cancels::default());
    let mut release = named(NamedKey::Enter);
    release.released = true;
    let mut synthetic = named(NamedKey::Enter);
    synthetic.synthetic = true;
    let mut cancel = named(NamedKey::Escape);
    cancel.repeat = true;
    batch(&mut app, vec![release, synthetic, cancel]);
    assert!(submitted(&app).is_empty());
    assert_eq!(app.world().resource::<Cancels>().unwrap().0, 0);
}

#[test]
fn reviewer_ime_enabled_commit_strips_different_key_text() {
    let (mut app, e, _) = setup(true);
    {
        let i = app.world_mut().resource_mut::<Input>().unwrap();
        i.ime_enabled = true;
        i.ime_commit = "日".into();
        i.keys = vec![character('n')];
    }
    rndr_paint::frame(&mut app);
    assert_eq!(app.world().get::<Label>(e).unwrap().text, "日");
}
