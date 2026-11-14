//! Headless gallery for a nested split sash.
//!
//! Run: `cargo run -p rndr_ui --example split_gallery`
//!
//! Arrow keys resize the focused sash. The host applies [`SplitSashChange`]
//! by mounting that ratio again, which is how a saved layout is restored.
use rndr_core::*;
use rndr_ui::split::*;
use rndr_ui::{UiId, UiPlugin};

fn main() {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.world_mut().insert_resource(Viewport::new(800.0, 600.0));
    let root = app.world_mut().spawn((Layout {
        width: Size::Px(800.0),
        height: Size::Px(600.0),
        ..Layout::column()
    },));
    let mut editors = SplitSashProps::new("Editors");
    editors.step = 0.05;
    editors.min_leading = 120.0;
    editors.min_trailing = 160.0;
    let mut notes = SplitSashProps::new("Notes");
    notes.axis = Axis::Column;
    notes.step = 0.1;
    let outer = SplitSash::mount(app.world_mut(), root, UiId(1), editors.clone(), 0.4).unwrap();
    let inner =
        SplitSash::mount(app.world_mut(), outer.leading, UiId(2), notes.clone(), 0.5).unwrap();
    app.update();
    app.update();
    request_focus(app.world_mut(), outer.sash);
    app.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(NamedKey::ArrowRight, Modifiers::default())];
    app.update();
    let mut editor_ratio = outer.ratio(app.world()).unwrap();
    for change in events::<SplitSashChange>(app.world()) {
        if change.sash == outer.sash {
            editor_ratio = change.ratio;
        }
    }
    SplitSash::mount(app.world_mut(), root, UiId(1), editors, editor_ratio).unwrap();
    app.update();
    let shown = outer.ratio(app.world()).unwrap();
    let nested = inner.ratio(app.world()).unwrap();
    assert!((shown - editor_ratio).abs() < 1e-3);
    assert!((nested - 0.5).abs() < 1e-2);
    let role = access_tree(app.world())
        .nodes
        .into_iter()
        .find(|node| node.id == access_id(outer.sash))
        .map(|node| node.role);
    println!("editors {shown:.3}");
    println!("notes {nested:.3}");
    println!("role {role:?}");
}
