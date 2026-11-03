//! Read-only key adapter: no renderer dependency and no second identity tree.
use rndr_core::{App, Layout};
use rndr_ui::{
    controls::{Button, ButtonProps, HostIntent},
    identity, Card, UiId, UiPlugin,
};

#[test]
fn surfaces_controls_internal_nodes_scopes_remount_and_dead_generations() {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    let parent = app.world_mut().spawn((Layout::column(),));
    let other = app.world_mut().spawn((Layout::column(),));
    let card = Card::mount(app.world_mut(), parent, UiId(42), "Public card").unwrap();
    let button = Button::mount(
        app.world_mut(),
        card.content,
        UiId(43),
        ButtonProps::new("Public button", HostIntent::Command(1)),
    )
    .unwrap();
    let second = Card::mount(app.world_mut(), other, UiId(42), "Other scope").unwrap();
    assert_eq!(identity(app.world(), parent), None);
    assert_eq!(identity(app.world(), card.root), Some(UiId(42)));
    assert_eq!(identity(app.world(), second.root), Some(UiId(42)));
    assert_eq!(identity(app.world(), card.content), None);
    assert_eq!(identity(app.world(), button.root), Some(UiId(43)));
    assert_eq!(identity(app.world(), button.label), None);
    let updated = Card::mount(app.world_mut(), parent, UiId(42), "Updated").unwrap();
    assert_eq!(updated.root, card.root);
    assert_eq!(identity(app.world(), updated.root), Some(UiId(42)));
    let old = button.root;
    rndr_core::despawn(app.world_mut(), old);
    let _replacement = app.world_mut().spawn((Layout::column(),));
    assert_eq!(identity(app.world(), old), None);
}
