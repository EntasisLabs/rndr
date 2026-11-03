//! Executable acceptance probes for the next milestone's canonical editor gap.
//! These are intentionally ignored on the unchanged baseline, NOT passing product
//! regressions. Run with `--ignored --nocapture` to reproduce the blocker. See
//! evidence/data-components-next/blockers/REPORT.md. No inspector is implemented.
use rndr_core::*;
use rndr_ui::UiPlugin;

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
#[ignore = "known baseline blocker: Enter reports end-of-batch text"]
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
#[ignore = "known baseline blocker: distinct Enter edges collapse"]
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
#[ignore = "known baseline blocker: repeated Enter submits"]
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
#[ignore = "known baseline blocker: deferred field editing crosses Tab focus"]
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
fn escape_hook(world: &mut World, _: Entity, event: &KeyEvent) -> bool {
    if event.logical == LogicalKey::Named(NamedKey::Escape) {
        world.resource_mut::<Cancels>().unwrap().0 += 1;
        return true;
    }
    false
}
#[test]
#[ignore = "known baseline blocker: CapturesKeys bypasses composite cancel hook"]
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
