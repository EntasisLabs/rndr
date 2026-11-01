use rndr_core::*;
use rndr_ui::*;

#[test]
fn malformed_title_is_rejected_before_any_recipe_update() {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    let parent = app.world_mut().spawn((Layout::column(),));
    let handle =
        Callout::mount(app.world_mut(), parent, UiId(1), Feedback::Info, "before").unwrap();
    let title = children_of(app.world(), handle.root)
        .into_iter()
        .find(|e| app.world().has::<Label>(*e))
        .unwrap();
    let before = app.world().get::<UiRecipe>(handle.root).unwrap().0.clone();
    app.world_mut().remove::<Label>(title);
    assert_eq!(handle.validate(app.world()), Err(UiError::Stale));
    assert_eq!(
        Callout::mount(app.world_mut(), parent, UiId(1), Feedback::Danger, "after"),
        Err(UiError::Stale)
    );
    assert_eq!(app.world().get::<UiRecipe>(handle.root).unwrap().0, before);
}
