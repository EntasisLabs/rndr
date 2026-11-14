//! Split sash: keyboard, double-click, clamp, nesting, and the access tree.
use std::time::Duration;

use rndr_core::*;
use rndr_ui::inputs::{Slider, SliderProps};
use rndr_ui::split::*;
use rndr_ui::{UiError, UiId, UiPlugin};

fn sim() -> (Sim, Entity) {
    let mut sim = Sim::new();
    sim.app.add_plugin(UiPlugin);
    sim.world().insert_resource(Viewport::new(800.0, 600.0));
    let root = sim.world().spawn((Layout {
        width: Size::Px(800.0),
        height: Size::Px(600.0),
        ..Layout::column()
    },));
    (sim, root)
}

fn settle(sim: &mut Sim) {
    sim.update();
    sim.update();
}

fn key(sim: &mut Sim, named: NamedKey) {
    sim.world().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(named, Modifiers::default())];
    sim.update();
    sim.world().resource_mut::<Input>().unwrap().keys.clear();
}

fn ratios(sim: &mut Sim) -> Vec<f64> {
    events::<SplitSashChange>(sim.world())
        .iter()
        .map(|event| event.ratio)
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
    assert!((value - expected).abs() < 1e-3, "{value} != {expected}");
}

fn main_size(world: &World, entity: Entity, axis: Axis) -> f32 {
    let rect = world.get::<Rect>(entity).copied().unwrap();
    match axis {
        Axis::Row => rect.w,
        Axis::Column => rect.h,
    }
}

fn press(sim: &mut Sim, at: [f32; 2]) {
    let input = sim.world().resource_mut::<Input>().unwrap();
    input.pointer.position = at;
    input.pointer.press_at = Some(at);
    input.pointer.pressed = true;
    input.pointer.down = true;
    input.pointer.button = PointerButton::Left;
    sim.update();
}

fn drag(sim: &mut Sim, at: [f32; 2]) {
    let input = sim.world().resource_mut::<Input>().unwrap();
    input.pointer.position = at;
    input.pointer.press_at = None;
    input.pointer.pressed = false;
    input.pointer.down = true;
    input.pointer.button = PointerButton::Left;
    sim.update();
}

fn release(sim: &mut Sim, at: [f32; 2]) {
    let input = sim.world().resource_mut::<Input>().unwrap();
    input.pointer.position = at;
    input.pointer.press_at = None;
    input.pointer.pressed = false;
    input.pointer.down = false;
    sim.update();
}

fn center(world: &World, entity: Entity) -> [f32; 2] {
    let rect = world.get::<Rect>(entity).copied().unwrap();
    [rect.x + rect.w / 2.0, rect.y + rect.h / 2.0]
}

fn props(label: &str) -> SplitSashProps {
    let mut props = SplitSashProps::new(label);
    props.min_leading = 48.0;
    props.min_trailing = 48.0;
    props.step = 0.25;
    props.page = 0.2;
    props.default_ratio = 0.5;
    props
}

#[test]
fn keys_home_end_and_slider_access() {
    let (mut sim, root) = sim();
    let mut props = props("Editors");
    props.description = "Between the panes".into();
    let handle = SplitSash::mount(sim.world(), root, UiId(1), props, 0.5).unwrap();
    settle(&mut sim);
    assert_eq!(main_size(sim.world(), handle.leading, Axis::Row), 396.0);
    request_focus(sim.world(), handle.sash);
    key(&mut sim, NamedKey::ArrowRight);
    assert_eq!(main_size(sim.world(), handle.leading, Axis::Row), 594.0);
    approx(ratios(&mut sim)[0], 0.75);
    key(&mut sim, NamedKey::PageDown);
    approx(handle.ratio(sim.world()).unwrap(), 0.55);
    key(&mut sim, NamedKey::Home);
    assert_eq!(main_size(sim.world(), handle.leading, Axis::Row), 48.0);
    key(&mut sim, NamedKey::End);
    assert_eq!(main_size(sim.world(), handle.leading, Axis::Row), 744.0);
    key(&mut sim, NamedKey::ArrowRight);
    assert_eq!(main_size(sim.world(), handle.leading, Axis::Row), 744.0);
    assert!(ratios(&mut sim).is_empty());

    let published = node(sim.world(), handle.sash);
    assert_eq!(published.role, AccessRole::Slider);
    assert_eq!(published.name, "Editors");
    assert_eq!(
        published.value,
        NumericAccess::text(handle.ratio(sim.world()).unwrap())
    );
    assert!(published.focusable);
    assert!(published.focused);
    assert!(published.enabled);
    let details = access_details(sim.world(), handle.sash);
    assert_eq!(details.description, "Between the panes");
    assert!(details.set_value);
    assert!(
        sim.world()
            .get::<NumericAccess>(handle.sash)
            .unwrap()
            .horizontal
    );
}

#[test]
fn vertical_down_grows_the_leading_pane_and_up_shrinks_it() {
    let (mut sim, root) = sim();
    let mut props = props("Stack");
    props.axis = Axis::Column;
    props.step = 0.1;
    let handle = SplitSash::mount(sim.world(), root, UiId(2), props, 0.5).unwrap();
    settle(&mut sim);
    // 600 points of height, minus the 8-point bar.
    assert_eq!(main_size(sim.world(), handle.leading, Axis::Column), 296.0);
    request_focus(sim.world(), handle.sash);
    key(&mut sim, NamedKey::ArrowDown);
    approx(handle.ratio(sim.world()).unwrap(), 0.6);
    key(&mut sim, NamedKey::ArrowUp);
    approx(handle.ratio(sim.world()).unwrap(), 0.5);
    key(&mut sim, NamedKey::ArrowRight);
    approx(handle.ratio(sim.world()).unwrap(), 0.6);
    let access = sim.world().get::<NumericAccess>(handle.sash).unwrap();
    assert!(!access.horizontal);
    assert_eq!(node(sim.world(), handle.sash).role, AccessRole::Slider);
}

#[test]
fn double_click_resets_and_a_late_second_press_does_not() {
    let (mut sim, root) = sim();
    let handle = SplitSash::mount(sim.world(), root, UiId(3), props("Panes"), 0.75).unwrap();
    settle(&mut sim);
    approx(handle.ratio(sim.world()).unwrap(), 0.75);
    let at = center(sim.world(), handle.sash);
    press(&mut sim, at);
    release(&mut sim, at);
    sim.advance(Duration::from_millis(100));
    press(&mut sim, at);
    approx(handle.ratio(sim.world()).unwrap(), 0.5);
    assert_eq!(main_size(sim.world(), handle.leading, Axis::Row), 396.0);
    release(&mut sim, at);

    SplitSash::mount(sim.world(), root, UiId(3), props("Panes"), 0.75).unwrap();
    sim.update();
    approx(handle.ratio(sim.world()).unwrap(), 0.75);
    let at = center(sim.world(), handle.sash);
    press(&mut sim, at);
    release(&mut sim, at);
    sim.advance(Duration::from_millis(500));
    press(&mut sim, at);
    approx(handle.ratio(sim.world()).unwrap(), 0.75);
}

#[test]
fn drag_clamps_to_both_minimums_once_with_shell() {
    let (mut sim, root) = sim();
    sim.app.add_plugin(Shell);
    let mut props = props("Editors");
    props.min_leading = 120.0;
    props.min_trailing = 160.0;
    let handle = SplitSash::mount(sim.world(), root, UiId(4), props, 0.5).unwrap();
    settle(&mut sim);
    assert_eq!(main_size(sim.world(), handle.leading, Axis::Row), 396.0);
    let at = center(sim.world(), handle.sash);
    press(&mut sim, at);
    assert_eq!(
        sim.world().resource::<Drag>().unwrap().entity,
        Some(handle.sash)
    );
    drag(&mut sim, [at[0] + 40.0, at[1]]);
    assert_eq!(main_size(sim.world(), handle.leading, Axis::Row), 436.0);
    approx(ratios(&mut sim)[0], 436.0 / 792.0);

    drag(&mut sim, [4_000.0, at[1]]);
    assert_eq!(main_size(sim.world(), handle.leading, Axis::Row), 632.0);
    drag(&mut sim, [-1_000.0, at[1]]);
    assert_eq!(main_size(sim.world(), handle.leading, Axis::Row), 120.0);
    release(&mut sim, [0.0, at[1]]);
    assert!(sim.world().resource::<Drag>().unwrap().entity.is_none());
    assert_eq!(main_size(sim.world(), handle.leading, Axis::Row), 120.0);
}

#[test]
fn plain_splitter_does_not_drag_until_a_sash_is_mounted() {
    let (mut sim, root) = sim();
    let row = sim.world().spawn((Layout {
        width: Size::Px(800.0),
        height: Size::Px(600.0),
        ..Layout::row()
    },));
    set_parent(sim.world(), row, root);
    let leading = sim.world().spawn((Layout {
        width: Size::Px(280.0),
        height: Size::Fill,
        ..Layout::column()
    },));
    set_parent(sim.world(), leading, row);
    let (bar, splitter) = splitter();
    let seam = sim.world().spawn((bar, splitter));
    set_parent(sim.world(), seam, row);
    let trailing = sim.world().spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    set_parent(sim.world(), trailing, row);
    settle(&mut sim);
    let at = center(sim.world(), seam);
    press(&mut sim, at);
    drag(&mut sim, [at[0] + 40.0, at[1]]);
    assert_eq!(main_size(sim.world(), leading, Axis::Row), 280.0);
    assert!(sim.world().resource::<Focus>().unwrap().entity.is_none());
}

#[test]
fn nested_splits_keep_their_ratios_and_both_are_sliders() {
    let (mut sim, root) = sim();
    let mut outer_props = props("Editors");
    outer_props.step = 0.25;
    let outer = SplitSash::mount(sim.world(), root, UiId(5), outer_props, 0.5).unwrap();
    let mut inner_props = props("Notes");
    inner_props.step = 0.25;
    let inner = SplitSash::mount(sim.world(), outer.leading, UiId(6), inner_props, 0.5).unwrap();
    settle(&mut sim);
    approx(outer.ratio(sim.world()).unwrap(), 0.5);
    approx(inner.ratio(sim.world()).unwrap(), 0.5);

    request_focus(sim.world(), outer.sash);
    key(&mut sim, NamedKey::ArrowRight);
    sim.update();
    approx(outer.ratio(sim.world()).unwrap(), 0.75);
    approx(inner.ratio(sim.world()).unwrap(), 0.5);
    let outer_width = main_size(sim.world(), outer.leading, Axis::Row);
    let inner_width = main_size(sim.world(), inner.leading, Axis::Row);
    assert!((inner_width - (outer_width - 8.0) * 0.5).abs() < 1.0);

    request_focus(sim.world(), inner.sash);
    key(&mut sim, NamedKey::ArrowRight);
    approx(inner.ratio(sim.world()).unwrap(), 0.75);
    approx(outer.ratio(sim.world()).unwrap(), 0.75);

    let outer_node = node(sim.world(), outer.sash);
    let inner_node = node(sim.world(), inner.sash);
    assert_eq!(outer_node.role, AccessRole::Slider);
    assert_eq!(inner_node.role, AccessRole::Slider);
    assert_eq!(outer_node.name, "Editors");
    assert_eq!(inner_node.name, "Notes");
    assert!(outer_node.focusable && inner_node.focusable);
    assert!(inner_node.focused);
    assert!(!outer_node.focused);
}

#[test]
fn readonly_and_disabled_refuse_keys_and_drags() {
    let (mut sim, root) = sim();
    let mut props = props("Locked");
    props.readonly = true;
    let handle = SplitSash::mount(sim.world(), root, UiId(7), props.clone(), 0.5).unwrap();
    settle(&mut sim);
    assert!(request_focus(sim.world(), handle.sash));
    key(&mut sim, NamedKey::ArrowRight);
    approx(handle.ratio(sim.world()).unwrap(), 0.5);
    assert!(ratios(&mut sim).is_empty());
    let at = center(sim.world(), handle.sash);
    press(&mut sim, at);
    drag(&mut sim, [at[0] + 80.0, at[1]]);
    approx(handle.ratio(sim.world()).unwrap(), 0.5);
    assert_eq!(main_size(sim.world(), handle.leading, Axis::Row), 396.0);
    release(&mut sim, at);
    assert!(node(sim.world(), handle.sash).focusable);
    assert!(!access_details(sim.world(), handle.sash).set_value);
    assert_eq!(
        try_apply_access(
            sim.world(),
            AccessCommand::SetValue {
                entity: handle.sash,
                value: "0.8".into(),
            }
        ),
        AccessCommandResult::Rejected
    );

    props.readonly = false;
    props.enabled = false;
    SplitSash::mount(sim.world(), root, UiId(7), props, 0.5).unwrap();
    sim.update();
    assert!(!request_focus(sim.world(), handle.sash));
    let published = node(sim.world(), handle.sash);
    assert!(!published.focusable);
    assert!(!published.enabled);
    let at = center(sim.world(), handle.sash);
    press(&mut sim, at);
    drag(&mut sim, [at[0] + 80.0, at[1]]);
    assert_eq!(main_size(sim.world(), handle.leading, Axis::Row), 396.0);
}

#[test]
fn remount_rejects_a_drag_or_keeps_the_echoed_ratio() {
    let (mut sim, root) = sim();
    let props = props("Editors");
    let handle = SplitSash::mount(sim.world(), root, UiId(8), props.clone(), 0.5).unwrap();
    settle(&mut sim);
    let at = center(sim.world(), handle.sash);
    press(&mut sim, at);
    drag(&mut sim, [at[0] + 80.0, at[1]]);
    release(&mut sim, at);
    let moved = handle.ratio(sim.world()).unwrap();
    assert!(moved > 0.55);
    SplitSash::mount(sim.world(), root, UiId(8), props.clone(), moved).unwrap();
    sim.update();
    approx(handle.ratio(sim.world()).unwrap(), moved);

    SplitSash::mount(sim.world(), root, UiId(8), props, 0.5).unwrap();
    sim.update();
    approx(handle.ratio(sim.world()).unwrap(), 0.5);
    assert_eq!(main_size(sim.world(), handle.leading, Axis::Row), 396.0);
}

#[test]
fn assistive_set_value_clamps_to_the_legal_ratio() {
    let (mut sim, root) = sim();
    let handle = SplitSash::mount(sim.world(), root, UiId(9), props("Editors"), 0.5).unwrap();
    settle(&mut sim);
    let max = sim.world().get::<NumericAccess>(handle.sash).unwrap().max;
    assert_eq!(
        try_apply_access(
            sim.world(),
            AccessCommand::SetValue {
                entity: handle.sash,
                value: NumericAccess::text(max),
            }
        ),
        AccessCommandResult::Applied
    );
    sim.update();
    approx(handle.ratio(sim.world()).unwrap(), max);
    assert_eq!(main_size(sim.world(), handle.leading, Axis::Row), 744.0);
    assert_eq!(
        try_apply_access(
            sim.world(),
            AccessCommand::SetValue {
                entity: handle.sash,
                value: "nope".into(),
            }
        ),
        AccessCommandResult::Rejected
    );
    assert_eq!(main_size(sim.world(), handle.leading, Axis::Row), 744.0);
}

#[test]
fn invalid_props_axis_changes_and_unmount() {
    let (mut sim, root) = sim();
    assert!(SplitSash::mount(sim.world(), root, UiId(1), SplitSashProps::new("  "), 0.5).is_err());
    let mut bad = props("Editors");
    bad.min_leading = -1.0;
    assert!(SplitSash::mount(sim.world(), root, UiId(1), bad.clone(), 0.5).is_err());
    bad.min_leading = 48.0;
    bad.step = 0.0;
    assert!(SplitSash::mount(sim.world(), root, UiId(1), bad, 0.5).is_err());
    assert!(SplitSash::mount(sim.world(), root, UiId(1), props("Editors"), f64::NAN).is_err());

    let handle = SplitSash::mount(sim.world(), root, UiId(2), props("Editors"), 0.5).unwrap();
    let mut turned = props("Editors");
    turned.axis = Axis::Column;
    assert_eq!(
        SplitSash::mount(sim.world(), root, UiId(2), turned, 0.5),
        Err(UiError::InvalidProps)
    );
    assert!(sim.world().is_alive(handle.sash));

    Slider::mount(sim.world(), root, UiId(3), SliderProps::new("Volume"), 1.0).unwrap();
    assert_eq!(
        SplitSash::mount(sim.world(), root, UiId(3), props("Editors"), 0.5),
        Err(UiError::IdentityConflict)
    );

    let parent = sim.world().spawn((Color::rgb(0.2, 0.2, 0.2),));
    assert_eq!(
        SplitSash::mount(sim.world(), parent, UiId(4), props("Editors"), 0.5),
        Err(UiError::Stale)
    );

    settle(&mut sim);
    handle.unmount(sim.world()).unwrap();
    assert!(handle.ratio(sim.world()).is_none());
    assert_eq!(handle.unmount(sim.world()), Err(UiError::Stale));
    assert!(node_missing(sim.world(), handle.sash));
}

fn node_missing(world: &World, entity: Entity) -> bool {
    let id = access_id(entity);
    !access_tree(world).nodes.iter().any(|node| node.id == id)
}
