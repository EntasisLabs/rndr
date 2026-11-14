//! Tab strip: keyboard, close, overflow, reorder, and split requests.
//! Headless. Not a pointer, keyboard, or screen-reader certificate.
use rndr_core::*;
use rndr_ui::drag::{announcement, DropPlace};
use rndr_ui::tabs::*;
use rndr_ui::{text, Token, Typography, UiError, UiId, UiPlugin, UiTheme};

fn boot(width: f32, height: f32) -> (Sim, Entity) {
    let mut sim = Sim::new();
    sim.app.add_plugin(UiPlugin);
    sim.world().insert_resource(Viewport::new(width, height));
    let root = sim.world().spawn((Layout {
        width: Size::Px(width),
        height: Size::Px(height),
        ..Layout::column()
    },));
    (sim, root)
}

fn settle(sim: &mut Sim) {
    sim.update();
    sim.update();
}

fn key(sim: &mut Sim, event: KeyEvent) {
    sim.world().resource_mut::<Input>().unwrap().keys = vec![event];
    sim.update();
    sim.world().resource_mut::<Input>().unwrap().keys.clear();
}

fn press(sim: &mut Sim, at: [f32; 2], button: PointerButton) {
    sim.world().resource_mut::<Input>().unwrap().pointer = Pointer {
        position: at,
        press_at: Some(at),
        pressed: true,
        down: true,
        button,
        ..Pointer::default()
    };
    sim.update();
}

fn drag(sim: &mut Sim, at: [f32; 2]) {
    sim.world().resource_mut::<Input>().unwrap().pointer = Pointer {
        position: at,
        down: true,
        button: PointerButton::Left,
        ..Pointer::default()
    };
    sim.update();
}

fn release(sim: &mut Sim, at: [f32; 2]) {
    sim.world().resource_mut::<Input>().unwrap().pointer = Pointer {
        position: at,
        released: true,
        button: PointerButton::Left,
        ..Pointer::default()
    };
    sim.update();
}

fn reset_pointer(sim: &mut Sim) {
    sim.world().resource_mut::<Input>().unwrap().pointer = Pointer::default();
}

fn center(world: &World, entity: Entity) -> [f32; 2] {
    let rect = world.get::<Rect>(entity).copied().unwrap();
    [rect.x + rect.w * 0.5, rect.y + rect.h * 0.5]
}

fn at_x(world: &World, entity: Entity, fraction: f32) -> [f32; 2] {
    let rect = world.get::<Rect>(entity).copied().unwrap();
    [rect.x + rect.w * fraction, rect.y + rect.h * 0.5]
}

fn press_at(sim: &mut Sim, entity: Entity, fraction: f32, button: PointerButton) {
    let at = at_x(sim.world(), entity, fraction);
    press(sim, at, button);
}

fn drag_at(sim: &mut Sim, entity: Entity, fraction: f32) {
    let at = at_x(sim.world(), entity, fraction);
    drag(sim, at);
}

fn release_at(sim: &mut Sim, entity: Entity, fraction: f32) {
    let at = at_x(sim.world(), entity, fraction);
    release(sim, at);
}

fn press_center(sim: &mut Sim, entity: Entity, button: PointerButton) {
    let at = center(sim.world(), entity);
    press(sim, at, button);
}

fn release_center(sim: &mut Sim, entity: Entity) {
    let at = center(sim.world(), entity);
    release(sim, at);
}

fn kinds(sim: &mut Sim, strip: Entity) -> Vec<TabStripEventKind> {
    events::<TabStripEvent>(sim.world())
        .iter()
        .filter(|event| event.strip == strip)
        .map(|event| event.kind)
        .collect()
}

fn focus_of(sim: &mut Sim) -> Option<Entity> {
    sim.world()
        .resource::<Focus>()
        .and_then(|focus| focus.entity)
}

fn node(world: &World, entity: Entity) -> AccessNode {
    let id = access_id(entity);
    access_tree(world)
        .nodes
        .into_iter()
        .find(|node| node.id == id)
        .expect("access node")
}

fn fully_inside(tab: Rect, port: Rect) -> bool {
    tab.w > 0.0 && tab.x >= port.x - 1.0 && tab.x + tab.w <= port.x + port.w + 1.0
}

fn close_button(world: &World, tab: Entity) -> Entity {
    children_of(world, tab)
        .into_iter()
        .find(|child| world.has::<Button>(*child) && !world.has::<Hidden>(*child))
        .expect("close button")
}

#[test]
fn helpers_reorder_and_transfer_keep_pins_together() {
    let pinned = TabSpec::new(1, "Pinned").pinned(true);
    let mut tabs = vec![pinned, TabSpec::new(2, "Beta"), TabSpec::new(3, "Gamma")];
    assert!(reorder_tabs(&tabs, TabId(1), TabId(2), DropPlace::After).is_none());
    let moved = reorder_tabs(&tabs, TabId(2), TabId(3), DropPlace::After).unwrap();
    assert_eq!(
        moved.iter().map(|tab| tab.id).collect::<Vec<_>>(),
        vec![TabId(1), TabId(3), TabId(2)]
    );
    assert!(reorder_tabs(&tabs, TabId(2), TabId(3), DropPlace::Before).is_none());

    let incoming = TabSpec::new(4, "Delta").pinned(true);
    tabs.push(incoming.clone());
    let (source, dest) = transfer_tab(
        &[incoming],
        &tabs[..3],
        TabId(4),
        Some(TabId(2)),
        DropPlace::Before,
    )
    .unwrap();
    assert!(source.is_empty());
    assert_eq!(
        dest.iter().map(|tab| tab.id).collect::<Vec<_>>(),
        vec![TabId(1), TabId(4), TabId(2), TabId(3)]
    );
    assert!(transfer_tab(
        &[TabSpec::new(4, "Delta").pinned(true)],
        &tabs[..3],
        TabId(4),
        Some(TabId(3)),
        DropPlace::After,
    )
    .is_none());
    let (_, appended) = transfer_tab(
        &[TabSpec::new(9, "Tail")],
        &tabs[..3],
        TabId(9),
        None,
        DropPlace::Inside,
    )
    .unwrap();
    assert_eq!(appended.last().unwrap().id, TabId(9));
}

#[test]
fn keyboard_roves_and_enter_or_space_activates() {
    let (mut sim, root) = boot(640.0, 360.0);
    let tabs = [
        TabSpec::new(1, "Notes").icon(Icon::File).dirty(true),
        TabSpec::new(2, "Plan"),
        TabSpec::new(3, "Log"),
    ];
    let strip = TabStrip::mount(
        sim.world(),
        root,
        UiId(1),
        TabStripProps::new("Documents"),
        &tabs,
        TabId(1),
    )
    .unwrap();
    settle(&mut sim);
    let notes = strip.tab(sim.world(), TabId(1)).unwrap();
    let plan = strip.tab(sim.world(), TabId(2)).unwrap();
    let log = strip.tab(sim.world(), TabId(3)).unwrap();
    let list = node(sim.world(), strip.list);
    assert_eq!(list.role, AccessRole::TabList);
    assert_eq!(list.name, "Documents");
    let notes_node = node(sim.world(), notes);
    assert_eq!(notes_node.role, AccessRole::Tab);
    assert!(notes_node.selected);
    assert!(!node(sim.world(), plan).selected);
    assert_eq!(notes_node.name, "Notes, unsaved");
    let panel = strip.panel(sim.world(), TabId(1)).unwrap();
    let details = access_details(sim.world(), panel);
    assert!(details.panel);
    assert_eq!(details.labelled_by, Some(access_id(notes)));
    assert_eq!(
        access_details(sim.world(), notes).controls,
        Some(access_id(panel))
    );
    let hidden_plan = strip.panel(sim.world(), TabId(2)).unwrap();
    assert!(sim.world().has::<Hidden>(hidden_plan));
    let theme = UiTheme::default();
    let dirty = {
        let world = sim.world();
        children_of(world, notes).iter().any(|child| {
            !world.has::<Hidden>(*child)
                && world.get::<Color>(*child).copied() == Some(theme.color(Token::WarningFill))
        })
    };
    assert!(dirty);

    request_focus(sim.world(), notes);
    key(
        &mut sim,
        KeyEvent::named(NamedKey::ArrowRight, Modifiers::default()),
    );
    assert_eq!(focus_of(&mut sim), Some(plan));
    assert_eq!(strip.selected(sim.world()), Some(TabId(1)));
    assert!(kinds(&mut sim, strip.root).is_empty());
    key(
        &mut sim,
        KeyEvent::named(NamedKey::End, Modifiers::default()),
    );
    assert_eq!(focus_of(&mut sim), Some(log));
    key(
        &mut sim,
        KeyEvent::named(NamedKey::Home, Modifiers::default()),
    );
    assert_eq!(focus_of(&mut sim), Some(notes));
    key(
        &mut sim,
        KeyEvent::named(NamedKey::ArrowRight, Modifiers::default()),
    );
    key(
        &mut sim,
        KeyEvent::named(NamedKey::Enter, Modifiers::default()),
    );
    assert_eq!(strip.selected(sim.world()), Some(TabId(2)));
    assert_eq!(
        kinds(&mut sim, strip.root),
        vec![TabStripEventKind::Activated(TabId(2))]
    );
    let shown = strip.panel(sim.world(), TabId(2)).unwrap();
    assert!(!sim.world().has::<Hidden>(shown));
    request_focus(sim.world(), log);
    sim.world().resource_mut::<Input>().unwrap().keys = vec![
        KeyEvent::named(NamedKey::Space, Modifiers::default()),
        KeyEvent::named(NamedKey::Space, Modifiers::default()).released(),
    ];
    sim.update();
    sim.world().resource_mut::<Input>().unwrap().keys.clear();
    assert_eq!(
        kinds(&mut sim, strip.root),
        vec![TabStripEventKind::Activated(TabId(3))]
    );
}

#[test]
fn click_selects_and_the_active_strip_uses_the_selected_fill() {
    let (mut sim, root) = boot(800.0, 400.0);
    let row = sim.world().spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::row()
    },));
    set_parent(sim.world(), row, root);
    let tabs = [TabSpec::new(1, "One"), TabSpec::new(2, "Two")];
    let active = TabStrip::mount(
        sim.world(),
        row,
        UiId(1),
        TabStripProps::new("Active"),
        &tabs,
        TabId(1),
    )
    .unwrap();
    let quiet = TabStrip::mount(
        sim.world(),
        row,
        UiId(2),
        TabStripProps::new("Quiet").active(false),
        &tabs,
        TabId(1),
    )
    .unwrap();
    settle(&mut sim);
    let theme = UiTheme::default();
    let active_tab = active.tab(sim.world(), TabId(1)).unwrap();
    let quiet_tab = quiet.tab(sim.world(), TabId(1)).unwrap();
    assert_eq!(
        sim.world().get::<Color>(active_tab).copied(),
        Some(theme.color(Token::Selected))
    );
    assert_eq!(
        sim.world().get::<Color>(quiet_tab).copied(),
        Some(theme.color(Token::Raised))
    );
    let two = active.tab(sim.world(), TabId(2)).unwrap();
    press_at(&mut sim, two, 0.25, PointerButton::Left);
    release_at(&mut sim, two, 0.25);
    assert_eq!(active.selected(sim.world()), Some(TabId(2)));
    assert_eq!(
        kinds(&mut sim, active.root),
        vec![TabStripEventKind::Activated(TabId(2))]
    );
}

#[test]
fn close_from_the_button_middle_click_delete_and_ctrl_w() {
    let (mut sim, root) = boot(640.0, 360.0);
    let tabs = [TabSpec::new(1, "Notes"), TabSpec::new(2, "Plan")];
    let strip = TabStrip::mount(
        sim.world(),
        root,
        UiId(1),
        TabStripProps::new("Documents"),
        &tabs,
        TabId(1),
    )
    .unwrap();
    settle(&mut sim);
    let notes = strip.tab(sim.world(), TabId(1)).unwrap();
    let close = close_button(sim.world(), notes);
    assert_eq!(
        sim.world()
            .get::<Semantics>(close)
            .and_then(|semantics| semantics.name.clone())
            .as_deref(),
        Some("Close Notes")
    );
    press_center(&mut sim, close, PointerButton::Left);
    release_center(&mut sim, close);
    assert_eq!(
        kinds(&mut sim, strip.root),
        vec![TabStripEventKind::CloseRequested(TabId(1))]
    );
    assert_eq!(strip.selected(sim.world()), Some(TabId(1)));
    reset_pointer(&mut sim);

    press_at(&mut sim, notes, 0.2, PointerButton::Middle);
    assert_eq!(
        kinds(&mut sim, strip.root),
        vec![TabStripEventKind::CloseRequested(TabId(1))]
    );
    reset_pointer(&mut sim);

    let editor = sim.world().spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(40.0),
            ..Layout::column()
        },
        Focusable,
        CapturesKeys,
        Label::new("body"),
    ));
    let notes_panel = strip.panel(sim.world(), TabId(1)).unwrap();
    set_parent(sim.world(), editor, notes_panel);
    request_focus(sim.world(), editor);
    key(
        &mut sim,
        KeyEvent::character(
            'w',
            Modifiers {
                ctrl: true,
                ..Modifiers::default()
            },
        ),
    );
    assert!(kinds(&mut sim, strip.root).is_empty());

    request_focus(sim.world(), notes);
    key(
        &mut sim,
        KeyEvent::named(NamedKey::Delete, Modifiers::default()),
    );
    assert_eq!(
        kinds(&mut sim, strip.root),
        vec![TabStripEventKind::CloseRequested(TabId(1))]
    );
    let plan = strip.tab(sim.world(), TabId(2)).unwrap();
    request_focus(sim.world(), plan);
    key(
        &mut sim,
        KeyEvent::character(
            'w',
            Modifiers {
                ctrl: true,
                ..Modifiers::default()
            },
        ),
    );
    assert_eq!(
        kinds(&mut sim, strip.root),
        vec![TabStripEventKind::CloseRequested(TabId(2))]
    );
}

#[test]
fn pinned_and_disallowed_close_emit_nothing() {
    let (mut sim, root) = boot(640.0, 320.0);
    let tabs = [
        TabSpec::new(1, "Pinned").pinned(true),
        TabSpec::new(2, "Open").closable(false),
    ];
    let strip = TabStrip::mount(
        sim.world(),
        root,
        UiId(1),
        TabStripProps::new("Documents"),
        &tabs,
        TabId(1),
    )
    .unwrap();
    settle(&mut sim);
    let pinned = strip.tab(sim.world(), TabId(1)).unwrap();
    assert_eq!(node(sim.world(), pinned).name, "Pinned, pinned");
    let pinned_closed = {
        let world = sim.world();
        children_of(world, pinned)
            .into_iter()
            .filter(|child| world.has::<Button>(*child))
            .all(|child| world.has::<Hidden>(child))
    };
    assert!(pinned_closed);
    press_at(&mut sim, pinned, 0.3, PointerButton::Middle);
    assert!(kinds(&mut sim, strip.root).is_empty());
    reset_pointer(&mut sim);
    request_focus(sim.world(), pinned);
    key(
        &mut sim,
        KeyEvent::named(NamedKey::Delete, Modifiers::default()),
    );
    key(
        &mut sim,
        KeyEvent::named(
            NamedKey::ArrowRight,
            Modifiers {
                alt: true,
                ..Modifiers::default()
            },
        ),
    );
    assert!(kinds(&mut sim, strip.root).is_empty());
    assert_eq!(announcement(sim.world()), "Cannot move Pinned.");

    let open = strip.tab(sim.world(), TabId(2)).unwrap();
    request_focus(sim.world(), open);
    key(
        &mut sim,
        KeyEvent::named(NamedKey::Delete, Modifiers::default()),
    );
    assert!(kinds(&mut sim, strip.root).is_empty());

    assert_eq!(
        TabStrip::mount(
            sim.world(),
            root,
            UiId(2),
            TabStripProps::new(" "),
            &tabs,
            TabId(1),
        ),
        Err(UiError::InvalidProps)
    );
}

#[test]
fn overflow_scrolls_and_the_menu_selects_a_hidden_tab() {
    let (mut sim, root) = boot(800.0, 600.0);
    let narrow = sim.world().spawn((Layout {
        width: Size::Px(220.0),
        height: Size::Px(280.0),
        ..Layout::column()
    },));
    set_parent(sim.world(), narrow, root);
    let tabs = [
        TabSpec::new(1, "One"),
        TabSpec::new(2, "Two"),
        TabSpec::new(3, "Three"),
    ];
    let strip = TabStrip::mount(
        sim.world(),
        narrow,
        UiId(1),
        TabStripProps::new("Documents"),
        &tabs,
        TabId(3),
    )
    .unwrap();
    settle(&mut sim);
    let port = *sim.world().get::<Rect>(strip.scroll).unwrap();
    let three_tab = strip.tab(sim.world(), TabId(3)).unwrap();
    let three = *sim.world().get::<Rect>(three_tab).unwrap();
    assert!(
        fully_inside(three, port),
        "selected tab {three:?} port {port:?}"
    );
    let strip = TabStrip::mount(
        sim.world(),
        narrow,
        UiId(1),
        TabStripProps::new("Documents"),
        &tabs,
        TabId(1),
    )
    .unwrap();
    settle(&mut sim);
    let forward = strip.scroll_forward(sim.world()).unwrap();
    let back = strip.scroll_back(sim.world()).unwrap();
    assert!(!sim.world().get::<Button>(back).unwrap().enabled);
    assert!(sim.world().get::<Button>(forward).unwrap().enabled);

    let wide = sim.world().spawn((Layout {
        width: Size::Px(700.0),
        height: Size::Px(200.0),
        ..Layout::column()
    },));
    set_parent(sim.world(), wide, root);
    let fitted = TabStrip::mount(
        sim.world(),
        wide,
        UiId(2),
        TabStripProps::new("Fitted"),
        &tabs[..2],
        TabId(1),
    )
    .unwrap();
    settle(&mut sim);
    let overflow = fitted.overflow_button(sim.world()).unwrap();
    assert!(!sim.world().get::<Button>(overflow).unwrap().enabled);

    let menu_button = strip.overflow_button(sim.world()).unwrap();
    assert!(sim.world().get::<Button>(menu_button).unwrap().enabled);
    assert_eq!(node(sim.world(), menu_button).name, "More tabs");
    press_center(&mut sim, forward, PointerButton::Left);
    release_center(&mut sim, forward);
    let offset = sim.world().get::<Scroll>(strip.scroll).unwrap().offset[0];
    assert!((offset + 120.0).abs() < 1.0, "offset {offset}");
    assert!(sim.world().get::<Button>(back).unwrap().enabled);

    press_center(&mut sim, menu_button, PointerButton::Left);
    release_center(&mut sim, menu_button);
    let row = menu_row(sim.world(), "Three");
    press_center(&mut sim, row, PointerButton::Left);
    release_center(&mut sim, row);
    assert_eq!(strip.selected(sim.world()), Some(TabId(3)));
    sim.update();
    assert_eq!(
        kinds(&mut sim, strip.root),
        vec![TabStripEventKind::Activated(TabId(3))]
    );
    let port = *sim.world().get::<Rect>(strip.scroll).unwrap();
    let three_tab = strip.tab(sim.world(), TabId(3)).unwrap();
    let three = *sim.world().get::<Rect>(three_tab).unwrap();
    assert!(
        fully_inside(three, port),
        "revealed {three:?} port {port:?}"
    );
}

#[test]
fn drag_reorder_and_alt_arrow_emit_moves() {
    let (mut sim, root) = boot(800.0, 360.0);
    let tabs = [
        TabSpec::new(1, "Alpha"),
        TabSpec::new(2, "Beta"),
        TabSpec::new(3, "Gamma"),
    ];
    let strip = TabStrip::mount(
        sim.world(),
        root,
        UiId(1),
        TabStripProps::new("Documents"),
        &tabs,
        TabId(1),
    )
    .unwrap();
    settle(&mut sim);
    let alpha = strip.tab(sim.world(), TabId(1)).unwrap();
    let beta = strip.tab(sim.world(), TabId(2)).unwrap();
    let panel = strip.panel(sim.world(), TabId(1)).unwrap();
    text(sim.world(), panel, "kept", Typography::Label).unwrap();
    press_at(&mut sim, alpha, 0.25, PointerButton::Left);
    drag_at(&mut sim, beta, 0.8);
    release_at(&mut sim, beta, 0.8);
    assert_eq!(
        kinds(&mut sim, strip.root),
        vec![TabStripEventKind::Reordered(TabMove {
            id: TabId(1),
            target: TabId(2),
            place: DropPlace::After,
        })]
    );
    assert_eq!(announcement(sim.world()), "Moved Alpha after Beta.");
    let reordered = reorder_tabs(&tabs, TabId(1), TabId(2), DropPlace::After).unwrap();
    let strip = TabStrip::mount(
        sim.world(),
        root,
        UiId(1),
        TabStripProps::new("Documents"),
        &reordered,
        TabId(1),
    )
    .unwrap();
    settle(&mut sim);
    assert_eq!(strip.panel(sim.world(), TabId(1)), Some(panel));
    let kept = children_of(sim.world(), panel)[0];
    assert!(sim.world().is_alive(kept));
    let left = strip.tab(sim.world(), TabId(2)).unwrap();
    let right = strip.tab(sim.world(), TabId(1)).unwrap();
    let left_x = sim.world().get::<Rect>(left).unwrap().x;
    let right_x = sim.world().get::<Rect>(right).unwrap().x;
    assert!(left_x < right_x);

    request_focus(sim.world(), right);
    key(
        &mut sim,
        KeyEvent::named(
            NamedKey::ArrowRight,
            Modifiers {
                alt: true,
                ..Modifiers::default()
            },
        ),
    );
    assert_eq!(
        kinds(&mut sim, strip.root),
        vec![TabStripEventKind::Reordered(TabMove {
            id: TabId(1),
            target: TabId(3),
            place: DropPlace::After,
        })]
    );
    assert_eq!(announcement(sim.world()), "Moved Alpha after Gamma.");
    let gamma = strip.tab(sim.world(), TabId(3)).unwrap();
    request_focus(sim.world(), gamma);
    key(
        &mut sim,
        KeyEvent::named(
            NamedKey::ArrowRight,
            Modifiers {
                alt: true,
                ..Modifiers::default()
            },
        ),
    );
    assert!(kinds(&mut sim, strip.root).is_empty());
    assert_eq!(announcement(sim.world()), "Cannot move Gamma.");
}

#[test]
fn pane_edge_requests_a_split() {
    let (mut sim, root) = boot(480.0, 420.0);
    let tabs = [TabSpec::new(1, "Notes"), TabSpec::new(2, "Plan")];
    let strip = TabStrip::mount(
        sim.world(),
        root,
        UiId(1),
        TabStripProps::new("Documents"),
        &tabs,
        TabId(1),
    )
    .unwrap();
    settle(&mut sim);
    let notes = strip.tab(sim.world(), TabId(1)).unwrap();
    let body = *sim.world().get::<Rect>(strip.body).unwrap();
    let edge = [body.x + body.w * 0.05, body.y + body.h * 0.5];
    press_at(&mut sim, notes, 0.25, PointerButton::Left);
    drag(&mut sim, edge);
    release(&mut sim, edge);
    assert_eq!(
        kinds(&mut sim, strip.root),
        vec![TabStripEventKind::SplitRequested(TabSplitRequest {
            id: TabId(1),
            strip: strip.root,
            direction: SplitDirection::Left,
        })]
    );
    assert_eq!(announcement(sim.world()), "Split Notes left.");
    assert_eq!(SplitDirection::Left.axis(), Axis::Row);
    assert!(SplitDirection::Left.leading());
    assert_eq!(SplitDirection::Below.axis(), Axis::Column);
}

#[test]
fn dropping_on_another_strip_requests_a_move() {
    let (mut sim, root) = boot(800.0, 420.0);
    let row = sim.world().spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::row()
    },));
    set_parent(sim.world(), row, root);
    let left_tabs = [TabSpec::new(1, "Alpha"), TabSpec::new(2, "Beta")];
    let right_tabs = [TabSpec::new(3, "Gamma"), TabSpec::new(4, "Delta")];
    let left = TabStrip::mount(
        sim.world(),
        row,
        UiId(1),
        TabStripProps::new("Left"),
        &left_tabs,
        TabId(1),
    )
    .unwrap();
    let right = TabStrip::mount(
        sim.world(),
        row,
        UiId(2),
        TabStripProps::new("Right").active(false),
        &right_tabs,
        TabId(3),
    )
    .unwrap();
    settle(&mut sim);
    let beta = left.tab(sim.world(), TabId(2)).unwrap();
    let gamma = right.tab(sim.world(), TabId(3)).unwrap();
    press_at(&mut sim, beta, 0.25, PointerButton::Left);
    drag_at(&mut sim, gamma, 0.75);
    release_at(&mut sim, gamma, 0.75);
    assert_eq!(
        kinds(&mut sim, right.root),
        vec![TabStripEventKind::Moved(TabTransfer {
            id: TabId(2),
            from: left.root,
            to: right.root,
            target: Some(TabId(3)),
            place: DropPlace::After,
        })]
    );
    assert_eq!(announcement(sim.world()), "Moved Beta after Gamma.");

    let (source, dest) = transfer_tab(
        &left_tabs,
        &right_tabs,
        TabId(2),
        Some(TabId(3)),
        DropPlace::After,
    )
    .unwrap();
    TabStrip::mount(
        sim.world(),
        row,
        UiId(1),
        TabStripProps::new("Left"),
        &source,
        TabId(1),
    )
    .unwrap();
    let right = TabStrip::mount(
        sim.world(),
        row,
        UiId(2),
        TabStripProps::new("Right").active(false),
        &dest,
        TabId(3),
    )
    .unwrap();
    settle(&mut sim);
    let alpha = left.tab(sim.world(), TabId(1)).unwrap();
    let body = *sim.world().get::<Rect>(right.body).unwrap();
    let inside = [body.x + body.w * 0.5, body.y + body.h * 0.5];
    press_at(&mut sim, alpha, 0.25, PointerButton::Left);
    drag(&mut sim, inside);
    release(&mut sim, inside);
    assert_eq!(
        kinds(&mut sim, right.root),
        vec![TabStripEventKind::Moved(TabTransfer {
            id: TabId(1),
            from: left.root,
            to: right.root,
            target: None,
            place: DropPlace::Inside,
        })]
    );
    assert_eq!(announcement(sim.world()), "Moved Alpha to Right.");
}

fn menu_row(world: &World, label: &str) -> Entity {
    world
        .entities_with::<Label>()
        .into_iter()
        .find(|entity| {
            world
                .get::<Label>(*entity)
                .is_some_and(|text| text.text == label)
                && ancestor_with::<Tab>(world, *entity).is_none()
                && ancestor_with::<Button>(world, *entity).is_some()
        })
        .and_then(|entity| ancestor_with::<Button>(world, entity))
        .expect("menu row")
}
