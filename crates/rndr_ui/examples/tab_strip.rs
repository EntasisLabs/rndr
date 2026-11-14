//! Two document panes, split by a sash, with tabs dragged between them.
//!
//! Run: `cargo run -p rndr_ui --example tab_strip`
//!
//! The host owns the order. A drop on the other strip moves the tab.
//! A drop on a pane edge is a split request the host can turn into another
//! `SplitSash`. This run applies the move and records the split. It does not
//! open a window.
use rndr_core::*;
use rndr_ui::drag::DropPlace;
use rndr_ui::split::{SplitSash, SplitSashProps};
use rndr_ui::tabs::*;
use rndr_ui::{text, Typography, UiId, UiPlugin};

struct Pane {
    tabs: Vec<TabSpec>,
    selected: TabId,
    active: bool,
}

fn main() {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.world_mut().insert_resource(Viewport::new(800.0, 480.0));
    let root = app.world_mut().spawn((Layout {
        width: Size::Px(800.0),
        height: Size::Px(480.0),
        ..Layout::column()
    },));
    let mut left = Pane {
        tabs: vec![
            TabSpec::new(1, "Notes").icon(Icon::File),
            TabSpec::new(2, "Plan").dirty(true),
        ],
        selected: TabId(1),
        active: true,
    };
    let mut right = Pane {
        tabs: vec![
            TabSpec::new(4, "Review").pinned(true),
            TabSpec::new(3, "Log"),
        ],
        selected: TabId(3),
        active: false,
    };
    let sash = SplitSash::mount(
        app.world_mut(),
        root,
        UiId(1),
        SplitSashProps::new("Documents"),
        0.5,
    )
    .unwrap();
    let mut left_strip = mount_pane(&mut app, sash.leading, UiId(2), &left);
    let mut right_strip = mount_pane(&mut app, sash.trailing, UiId(3), &right);
    app.update();
    app.update();

    let from = left_strip.tab(app.world(), TabId(2)).unwrap();
    let onto = right_strip.tab(app.world(), TabId(3)).unwrap();
    let start = point(app.world(), from, 0.25);
    let end = point(app.world(), onto, 0.75);
    pointer(&mut app, start, Stroke::Press);
    pointer(&mut app, end, Stroke::Move);
    pointer(&mut app, end, Stroke::Release);
    let moved = events::<TabStripEvent>(app.world())
        .iter()
        .find_map(|event| match event.kind {
            TabStripEventKind::Moved(transfer) => Some(transfer),
            _ => None,
        })
        .expect("cross-strip move");
    assert_eq!(moved.id, TabId(2));
    assert_eq!(moved.target, Some(TabId(3)));
    assert_eq!(moved.place, DropPlace::After);
    let (source, dest) = transfer_tab(&left.tabs, &right.tabs, moved.id, moved.target, moved.place)
        .expect("host accepts the move");
    left.tabs = source;
    right.tabs = dest;
    left.active = false;
    right.active = true;
    left_strip = mount_pane(&mut app, sash.leading, UiId(2), &left);
    right_strip = mount_pane(&mut app, sash.trailing, UiId(3), &right);
    app.update();
    app.update();
    assert!(left.tabs.iter().all(|tab| tab.id != TabId(2)));
    assert_eq!(right.tabs.last().map(|tab| tab.id), Some(TabId(2)));
    assert!(left_strip.tab(app.world(), TabId(2)).is_none());
    assert!(right_strip.tab(app.world(), TabId(2)).is_some());

    let notes = left_strip.tab(app.world(), TabId(1)).unwrap();
    let body = *app.world().get::<Rect>(right_strip.body).unwrap();
    let edge = [body.x + 12.0, body.y + body.h * 0.5];
    let start = point(app.world(), notes, 0.25);
    pointer(&mut app, start, Stroke::Press);
    pointer(&mut app, edge, Stroke::Move);
    pointer(&mut app, edge, Stroke::Release);
    let split = events::<TabStripEvent>(app.world())
        .iter()
        .find_map(|event| match event.kind {
            TabStripEventKind::SplitRequested(request) => Some(request),
            _ => None,
        })
        .expect("split request");
    assert_eq!(split.id, TabId(1));
    assert_eq!(split.direction, SplitDirection::Left);
    assert_eq!(split.direction.axis(), Axis::Row);
    println!(
        "moved Plan onto Log; split Notes {}",
        direction(split.direction)
    );
}

fn mount_pane(app: &mut App, parent: Entity, id: UiId, pane: &Pane) -> TabStripHandle {
    let props =
        TabStripProps::new(if pane.active { "Active" } else { "Inactive" }).active(pane.active);
    let strip = TabStrip::mount(
        app.world_mut(),
        parent,
        id,
        props,
        &pane.tabs,
        pane.selected,
    )
    .unwrap();
    for tab in &pane.tabs {
        let panel = strip.panel(app.world(), tab.id).unwrap();
        if children_of(app.world(), panel).is_empty() {
            text(app.world_mut(), panel, &tab.label, Typography::Reading).unwrap();
        }
    }
    strip
}

fn point(world: &World, entity: Entity, fraction: f32) -> [f32; 2] {
    let rect = world.get::<Rect>(entity).copied().unwrap();
    [rect.x + rect.w * fraction, rect.y + rect.h * 0.5]
}

enum Stroke {
    Press,
    Move,
    Release,
}

fn pointer(app: &mut App, at: [f32; 2], stroke: Stroke) {
    let pointer = match stroke {
        Stroke::Press => Pointer {
            position: at,
            press_at: Some(at),
            pressed: true,
            down: true,
            button: PointerButton::Left,
            ..Pointer::default()
        },
        Stroke::Move => Pointer {
            position: at,
            down: true,
            button: PointerButton::Left,
            ..Pointer::default()
        },
        Stroke::Release => Pointer {
            position: at,
            released: true,
            button: PointerButton::Left,
            ..Pointer::default()
        },
    };
    app.world_mut().resource_mut::<Input>().unwrap().pointer = pointer;
    app.update();
}

fn direction(direction: SplitDirection) -> &'static str {
    match direction {
        SplitDirection::Left => "left",
        SplitDirection::Right => "right",
        SplitDirection::Above => "above",
        SplitDirection::Below => "below",
    }
}
