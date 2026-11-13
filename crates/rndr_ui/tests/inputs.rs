//! Slider, range, stepper, tooltip, and toast.
use std::time::Duration;

use rndr_core::motion::MotionSettings;
use rndr_core::*;
use rndr_ui::controls::{Button, ButtonProps, HostIntent};
use rndr_ui::inputs::*;
use rndr_ui::{identity, Appearance, UiId, UiPlugin, UiTheme};

fn sim() -> (Sim, Entity) {
    let mut sim = Sim::new();
    sim.app.add_plugin(UiPlugin);
    sim.world().insert_resource(Viewport::new(720.0, 560.0));
    let root = sim.world().spawn((Layout {
        width: Size::Px(420.0),
        height: Size::Px(520.0),
        padding: 12.0,
        gap: 8.0,
        ..Layout::column()
    },));
    (sim, root)
}

fn key(sim: &mut Sim, named: NamedKey) {
    sim.world().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(named, Modifiers::default())];
    sim.update();
    sim.world().resource_mut::<Input>().unwrap().keys.clear();
}

fn keys(sim: &mut Sim, named: &[NamedKey]) {
    sim.world().resource_mut::<Input>().unwrap().keys = named
        .iter()
        .copied()
        .map(|named| KeyEvent::named(named, Modifiers::default()))
        .collect();
    sim.update();
    sim.world().resource_mut::<Input>().unwrap().keys.clear();
}

fn type_text(sim: &mut Sim, text: &str) {
    sim.world().resource_mut::<Input>().unwrap().keys = text
        .chars()
        .map(|ch| KeyEvent::character(ch, Modifiers::default()))
        .collect();
    sim.update();
    sim.world().resource_mut::<Input>().unwrap().keys.clear();
}

fn slider_values(sim: &Sim) -> Vec<f64> {
    events::<SliderChange>(sim.app.world())
        .iter()
        .map(|event| event.value)
        .collect()
}

fn node(world: &World, entity: Entity) -> AccessNode {
    let id = access_id(entity);
    access_tree(world)
        .nodes
        .into_iter()
        .find(|node| node.id == id)
        .expect("access node")
}

fn approx(value: f64, expected: f64) {
    assert!((value - expected).abs() < 1e-6, "{value} != {expected}");
}

#[test]
fn slider_keys_page_home_end_keep_published_value_until_remount() {
    let (mut sim, root) = sim();
    let mut props = SliderProps::new("Volume");
    props.description = "Output level".into();
    let handle = Slider::mount(sim.world(), root, UiId(1), props.clone(), 40.0).unwrap();
    sim.update();
    request_focus(sim.world(), handle.root);
    keys(
        &mut sim,
        &[
            NamedKey::ArrowRight,
            NamedKey::ArrowRight,
            NamedKey::ArrowUp,
            NamedKey::ArrowLeft,
        ],
    );
    assert_eq!(slider_values(&sim), vec![41.0, 42.0, 43.0, 42.0]);
    key(&mut sim, NamedKey::PageUp);
    approx(slider_values(&sim)[0], 52.0);
    key(&mut sim, NamedKey::PageDown);
    approx(slider_values(&sim)[0], 42.0);
    key(&mut sim, NamedKey::Home);
    approx(slider_values(&sim)[0], 0.0);
    key(&mut sim, NamedKey::End);
    approx(slider_values(&sim)[0], 100.0);
    assert_eq!(handle.value(sim.world()), Some(40.0));
    let published = node(sim.world(), handle.root);
    assert_eq!(published.role, AccessRole::Slider);
    assert_eq!(published.name, "Volume");
    assert_eq!(published.value, "40");
    assert!(published.focusable);
    assert_eq!(
        sim.world().get::<NumericAccess>(handle.root).unwrap().value,
        40.0
    );
    Slider::mount(sim.world(), root, UiId(1), props, 100.0).unwrap();
    sim.update();
    assert_eq!(handle.value(sim.world()), Some(100.0));
    assert_eq!(node(sim.world(), handle.root).value, "100");
}

#[test]
fn slider_vertical_up_increases_and_disabled_readonly_refuse_input() {
    let (mut sim, root) = sim();
    let mut props = SliderProps::new("Level");
    props.vertical = true;
    let handle = Slider::mount(sim.world(), root, UiId(2), props.clone(), 10.0).unwrap();
    sim.update();
    request_focus(sim.world(), handle.root);
    key(&mut sim, NamedKey::ArrowUp);
    assert_eq!(slider_values(&sim), vec![11.0]);
    key(&mut sim, NamedKey::ArrowDown);
    assert_eq!(slider_values(&sim), vec![10.0]);
    props.enabled = false;
    Slider::mount(sim.world(), root, UiId(2), props.clone(), 10.0).unwrap();
    sim.update();
    assert!(!request_focus(sim.world(), handle.root));
    key(&mut sim, NamedKey::ArrowUp);
    assert!(slider_values(&sim).is_empty());
    assert!(!node(sim.world(), handle.root).focusable);
    props.enabled = true;
    props.readonly = true;
    Slider::mount(sim.world(), root, UiId(2), props, 10.0).unwrap();
    sim.update();
    assert!(request_focus(sim.world(), handle.root));
    key(&mut sim, NamedKey::ArrowUp);
    assert!(slider_values(&sim).is_empty());
    assert!(node(sim.world(), handle.root).focusable);
}

#[test]
fn slider_clamps_snaps_and_pointer_drag_requests_a_position() {
    let (mut sim, root) = sim();
    let mut props = SliderProps::new("Gain");
    props.range.step = 5.0;
    props.range.page = 20.0;
    let handle = Slider::mount(sim.world(), root, UiId(3), props.clone(), 1_000.0).unwrap();
    assert_eq!(handle.value(sim.world()), Some(100.0));
    assert!(Slider::mount(sim.world(), root, UiId(9), SliderProps::new("  "), 1.0).is_err());
    sim.update();
    sim.update();
    let track = handle.track(sim.world()).unwrap();
    let rect = *sim.world().get::<Rect>(track).unwrap();
    let at = [rect.x + rect.w * 0.5, rect.y + rect.h * 0.5];
    let input = sim.world().resource_mut::<Input>().unwrap();
    input.pointer.position = at;
    input.pointer.press_at = Some(at);
    input.pointer.pressed = true;
    input.pointer.down = true;
    input.pointer.button = PointerButton::Left;
    sim.update();
    let value = slider_values(&sim)[0];
    assert!((40.0..=60.0).contains(&value), "{value}");
    assert_eq!(value % 5.0, 0.0);
    assert_eq!(handle.value(sim.world()), Some(100.0));
    props.range.min = 10.0;
    props.range.max = 10.0;
    assert!(Slider::mount(sim.world(), root, UiId(3), props, 10.0).is_err());
}

#[test]
fn range_thumbs_do_not_cross_and_each_is_a_slider() {
    let (mut sim, root) = sim();
    let props = SliderProps::new("Window");
    let handle = Range::mount(sim.world(), root, UiId(4), props, 20.0, 80.0).unwrap();
    sim.update();
    request_focus(sim.world(), handle.start);
    keys(
        &mut sim,
        &[NamedKey::ArrowLeft, NamedKey::End, NamedKey::ArrowRight],
    );
    let changes: Vec<_> = events::<RangeChange>(sim.world())
        .iter()
        .map(|event| (event.start, event.end))
        .collect();
    assert_eq!(changes, vec![(19.0, 80.0), (80.0, 80.0)]);
    key(&mut sim, NamedKey::Tab);
    assert_eq!(
        sim.world().resource::<Focus>().unwrap().entity,
        Some(handle.end)
    );
    key(&mut sim, NamedKey::ArrowRight);
    assert_eq!(
        events::<RangeChange>(sim.world())
            .iter()
            .map(|event| (event.start, event.end))
            .collect::<Vec<_>>(),
        vec![(80.0, 81.0)]
    );
    let start = node(sim.world(), handle.start);
    let end = node(sim.world(), handle.end);
    assert_eq!(start.role, AccessRole::Slider);
    assert_eq!(end.role, AccessRole::Slider);
    assert_eq!(start.name, "Window start");
    assert_eq!(end.name, "Window end");
    assert_eq!(start.value, "20");
    assert_eq!(end.value, "80");
    assert_eq!(handle.value(sim.world()), Some((20.0, 80.0)));
}

#[test]
fn assistive_set_value_is_a_request_and_does_not_publish() {
    let (mut sim, root) = sim();
    let handle =
        Slider::mount(sim.world(), root, UiId(5), SliderProps::new("Volume"), 40.0).unwrap();
    sim.update();
    let command = AccessCommand::SetValue {
        entity: handle.root,
        value: "55.2".into(),
    };
    assert_eq!(
        try_apply_access(sim.world(), command),
        AccessCommandResult::Applied
    );
    assert_eq!(handle.value(sim.world()), Some(40.0));
    sim.update();
    approx(slider_values(&sim)[0], 55.0);
    assert_eq!(
        sim.world().get::<NumericAccess>(handle.root).unwrap().value,
        40.0
    );
    let rejected = AccessCommand::SetValue {
        entity: handle.root,
        value: "nope".into(),
    };
    assert_eq!(
        try_apply_access(sim.world(), rejected),
        AccessCommandResult::Rejected
    );
}

#[test]
fn stepper_arrows_typing_buttons_clamp_and_reject() {
    let (mut sim, root) = sim();
    let mut props = NumberProps::new("Count");
    props.range = NumberRange::new(0.0, 10.0);
    props.range.step = 1.0;
    props.range.page = 4.0;
    let handle = NumberField::mount(sim.world(), root, UiId(6), props.clone(), 4.0).unwrap();
    sim.update();
    request_focus(sim.world(), handle.field);
    keys(
        &mut sim,
        &[NamedKey::ArrowUp, NamedKey::PageUp, NamedKey::End],
    );
    assert_eq!(
        events::<NumberChange>(sim.world())
            .iter()
            .map(|event| event.value)
            .collect::<Vec<_>>(),
        vec![5.0, 9.0, 10.0]
    );
    assert_eq!(handle.draft(sim.world()).as_deref(), Some("10"));
    assert_eq!(handle.value(sim.world()), Some(4.0));
    assert_eq!(
        sim.world()
            .get::<NumericAccess>(handle.field)
            .unwrap()
            .value,
        4.0
    );
    assert_eq!(node(sim.world(), handle.field).role, AccessRole::SpinButton);
    assert_eq!(node(sim.world(), handle.field).name, "Count");
    assert_eq!(node(sim.world(), handle.field).value, "10");
    keys(&mut sim, &[NamedKey::Backspace, NamedKey::Backspace]);
    type_text(&mut sim, "2");
    key(&mut sim, NamedKey::Enter);
    approx(events::<NumberChange>(sim.world())[0].value, 2.0);
    type_text(&mut sim, "nope");
    key(&mut sim, NamedKey::Enter);
    let rejected = events::<NumberRejected>(sim.world());
    assert_eq!(rejected.len(), 1);
    assert!(rejected[0].draft.contains("nope") || rejected[0].draft.contains('n'));
    assert_eq!(handle.draft(sim.world()).as_deref(), Some("2"));
    type_text(&mut sim, "99");
    key(&mut sim, NamedKey::Enter);
    approx(events::<NumberChange>(sim.world())[0].value, 10.0);
    let increase = step_buttons(sim.world(), handle.root)[1];
    request_focus(sim.world(), increase);
    key(&mut sim, NamedKey::Enter);
    assert!(events::<NumberChange>(sim.world()).is_empty());
    NumberField::mount(sim.world(), root, UiId(6), props.clone(), 2.0).unwrap();
    sim.update();
    request_focus(sim.world(), increase);
    key(&mut sim, NamedKey::Enter);
    approx(events::<NumberChange>(sim.world())[0].value, 3.0);
    props.readonly = true;
    NumberField::mount(sim.world(), root, UiId(6), props, 2.0).unwrap();
    sim.update();
    request_focus(sim.world(), handle.field);
    type_text(&mut sim, "8");
    key(&mut sim, NamedKey::Enter);
    assert!(events::<NumberChange>(sim.world()).is_empty());
}

#[test]
fn tooltip_follows_hover_and_focus_and_links_described_by() {
    let (mut sim, root) = sim();
    let button = Button::mount(
        sim.world(),
        root,
        UiId(7),
        ButtonProps::new("Save", HostIntent::Command(1)),
    )
    .unwrap();
    let tip = Tooltip::attach(sim.world(), button.root, "Write the draft").unwrap();
    sim.update();
    sim.update();
    assert!(access_tree(sim.world())
        .nodes
        .iter()
        .all(|node| node.role != AccessRole::Tooltip));
    assert_eq!(
        sim.world().get::<DescribedBy>(button.root).unwrap().0,
        vec![tip.root]
    );
    request_focus(sim.world(), button.root);
    sim.update();
    let shown = node(sim.world(), tip.root);
    assert_eq!(shown.role, AccessRole::Tooltip);
    assert_eq!(shown.name, "Write the draft");
    key(&mut sim, NamedKey::Escape);
    assert!(sim.world().has::<Hidden>(tip.root));
    let elsewhere = sim.world().spawn((Layout::column(), Focusable));
    request_focus(sim.world(), elsewhere);
    sim.update();
    request_focus(sim.world(), button.root);
    sim.update();
    assert!(!sim.world().has::<Hidden>(tip.root));
    request_focus(sim.world(), elsewhere);
    sim.update();
    let rect = *sim.world().get::<Rect>(button.root).unwrap();
    let at = [rect.x + rect.w * 0.5, rect.y + rect.h * 0.5];
    sim.world()
        .resource_mut::<Input>()
        .unwrap()
        .pointer
        .position = at;
    sim.update();
    assert!(!sim.world().has::<Hidden>(tip.root));
    sim.world()
        .resource_mut::<Input>()
        .unwrap()
        .pointer
        .position = [-10.0, -10.0];
    sim.update();
    assert!(sim.world().has::<Hidden>(tip.root));
    tip.detach(sim.world()).unwrap();
    assert!(sim.world().get::<DescribedBy>(button.root).is_none());
    assert!(Tooltip::attach(sim.world(), button.root, "  ").is_err());
}

#[test]
fn toast_announces_status_undo_and_dismiss_respects_reduced_motion() {
    let (mut sim, root) = sim();
    sim.world().insert_resource(SystemAppearance {
        reduced_motion: true,
        ..SystemAppearance::default()
    });
    let mut props = ToastProps::timed("Saved", Duration::from_millis(50));
    props.undo = Some("Undo".into());
    let toast = Toast::mount(sim.world(), root, UiId(8), props).unwrap();
    sim.update();
    let status = node(sim.world(), toast.root);
    assert_eq!(status.role, AccessRole::Status);
    assert_eq!(status.name, "Saved");
    assert_eq!(
        sim.world().get::<LiveRegion>(toast.root).unwrap().0,
        LivePoliteness::Polite
    );
    let rect = *sim.world().get::<Rect>(toast.root).unwrap();
    sim.world()
        .resource_mut::<Input>()
        .unwrap()
        .pointer
        .position = [rect.x + 8.0, rect.y + 8.0];
    sim.update();
    sim.advance(Duration::from_millis(500));
    sim.update();
    assert!(
        events::<ToastEvent>(sim.world()).is_empty(),
        "hover pauses auto-dismiss"
    );
    sim.world()
        .resource_mut::<Input>()
        .unwrap()
        .pointer
        .position = [-20.0, -20.0];
    sim.update();
    assert!(events::<ToastEvent>(sim.world()).is_empty());
    sim.advance(Duration::from_millis(50));
    sim.update();
    assert_eq!(
        events::<ToastEvent>(sim.world())
            .iter()
            .map(|event| event.notice)
            .collect::<Vec<_>>(),
        vec![ToastNotice::Expired]
    );
    assert!(sim.world().has::<Hidden>(toast.root));
}

#[test]
fn toast_undo_and_escape_dismiss_without_waiting_for_motion() {
    let (mut sim, root) = sim();
    sim.world().insert_resource(MotionSettings {
        reduced_motion: true,
        ..MotionSettings::default()
    });
    let toast = Toast::mount(
        sim.world(),
        root,
        UiId(8),
        ToastProps {
            message: "Moved".into(),
            undo: Some("Undo".into()),
            duration: Duration::from_secs(30),
        },
    )
    .unwrap();
    sim.update();
    let undo = buttons(sim.world(), toast.root)
        .into_iter()
        .find(|entity| {
            sim.world()
                .get::<Semantics>(*entity)
                .and_then(|semantics| semantics.name.as_deref())
                == Some("Undo")
        })
        .unwrap();
    request_focus(sim.world(), undo);
    key(&mut sim, NamedKey::Enter);
    assert_eq!(
        events::<ToastEvent>(sim.world())[0].notice,
        ToastNotice::Undo
    );
    assert!(sim.world().has::<Hidden>(toast.root));
    let again = Toast::mount(
        sim.world(),
        root,
        UiId(8),
        ToastProps::timed("Moved again", Duration::from_secs(30)),
    )
    .unwrap();
    sim.update();
    assert!(!sim.world().has::<Hidden>(again.root));
    let dismiss = buttons(sim.world(), again.root)
        .into_iter()
        .find(|entity| {
            sim.world()
                .get::<Semantics>(*entity)
                .and_then(|semantics| semantics.name.as_deref())
                == Some("Dismiss")
        })
        .unwrap();
    request_focus(sim.world(), dismiss);
    key(&mut sim, NamedKey::Escape);
    assert_eq!(
        events::<ToastEvent>(sim.world())[0].notice,
        ToastNotice::Dismissed
    );
    assert_eq!(identity(sim.world(), again.root), Some(UiId(8)));
    assert!(Slider::mount(sim.world(), root, UiId(8), SliderProps::new("Nope"), 1.0).is_err());
}

#[test]
fn motion_fade_delays_hide_until_the_opacity_settles() {
    let (mut sim, root) = sim();
    let toast = Toast::mount(
        sim.world(),
        root,
        UiId(11),
        ToastProps::timed("Fading", Duration::from_millis(30)),
    )
    .unwrap();
    sim.update();
    sim.advance(Duration::from_millis(30));
    sim.update();
    assert_eq!(
        events::<ToastEvent>(sim.world())[0].notice,
        ToastNotice::Expired
    );
    assert!(
        !sim.world().has::<Hidden>(toast.root),
        "fade keeps the toast in the tree on the dismiss frame"
    );
    sim.advance(Duration::from_millis(250));
    sim.update();
    assert!(sim.world().has::<Hidden>(toast.root));
}

#[test]
fn paint_snapshot_is_stable_theme_aware_and_moves_the_thumb() {
    let (mut sim, root) = sim();
    let props = SliderProps::new("Volume");
    let handle = Slider::mount(sim.world(), root, UiId(1), props.clone(), 0.0).unwrap();
    NumberField::mount(sim.world(), root, UiId(2), NumberProps::new("Count"), 3.0).unwrap();
    let toast = Toast::mount(sim.world(), root, UiId(3), ToastProps::new("Saved")).unwrap();
    sim.update();
    sim.update();
    let thumb = handle.thumb(sim.world()).unwrap();
    let low = *sim.world().get::<Rect>(thumb).unwrap();
    Slider::mount(sim.world(), root, UiId(1), props, 100.0).unwrap();
    sim.update();
    let high = *sim.world().get::<Rect>(thumb).unwrap();
    assert!(high.x > low.x + 20.0, "thumb {low:?} -> {high:?}");
    let first = rndr_paint::frame(&mut sim.app);
    let again = rndr_paint::frame(&mut sim.app);
    let snap = rndr_paint::quads_snapshot(&first);
    assert_eq!(snap, rndr_paint::quads_snapshot(&again));
    assert!(snap.contains("0.204,0.365,0.702"), "{snap}");
    assert!(first.quads.len() > 8);
    sim.world().insert_resource(UiTheme {
        appearance: Appearance::Dark,
        ..UiTheme::default()
    });
    let dark = rndr_paint::frame(&mut sim.app);
    assert_ne!(
        rndr_paint::quads_snapshot(&first),
        rndr_paint::quads_snapshot(&dark)
    );
    assert!(sim.world().is_alive(toast.root));
}

fn step_buttons(world: &World, root: Entity) -> Vec<Entity> {
    buttons(world, root)
}

fn buttons(world: &World, root: Entity) -> Vec<Entity> {
    let mut found = Vec::new();
    fn walk(world: &World, entity: Entity, found: &mut Vec<Entity>) {
        if world.has::<rndr_core::Button>(entity) {
            found.push(entity);
        }
        for child in children_of(world, entity) {
            walk(world, child, found);
        }
    }
    walk(world, root, &mut found);
    found
}
