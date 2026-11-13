use rndr_core::{access_id, access_tree, AccessRole, App, Icon, Layout, Size, Viewport};
use rndr_paint::frame;
use rndr_ui::{
    controls::{Button, ButtonProps, HostIntent, IconButton},
    UiId, UiPlugin,
};

fn mount() -> (App, rndr_core::Entity) {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.world_mut().insert_resource(Viewport::new(320.0, 200.0));
    let root = app.world_mut().spawn((Layout {
        width: Size::Px(320.0),
        ..Layout::column()
    },));
    (app, root)
}

#[test]
fn icon_button_draws_a_glyph_and_announces_its_name() {
    let (mut app, root) = mount();
    let handle = IconButton::icon(
        app.world_mut(),
        root,
        UiId(1),
        Icon::Search,
        "Search documents",
        ButtonProps::new("ignored", HostIntent::Command(1)),
    )
    .unwrap();
    assert!(IconButton::icon(
        app.world_mut(),
        root,
        UiId(2),
        Icon::X,
        "   ",
        ButtonProps::new("close", HostIntent::Command(2)),
    )
    .is_err());
    assert!(Button::mount(
        app.world_mut(),
        root,
        UiId(3),
        ButtonProps::new(Icon::X.scalar().to_string(), HostIntent::Command(3)),
    )
    .is_err());
    let painted = frame(&mut app);
    assert!(
        painted
            .quads
            .iter()
            .any(|quad| quad.atlas == 2 && quad.rect.w > 8.0),
        "icon glyph was not painted"
    );
    let tree = access_tree(app.world());
    let button = tree
        .nodes
        .iter()
        .find(|node| node.role == AccessRole::Button)
        .expect("button");
    assert_eq!(button.name, "Search documents");
    assert!(!button.name.chars().any(rndr_core::is_icon_scalar));
    let label = access_id(handle.label);
    assert!(tree.nodes.iter().all(|node| node.id != label));
}
