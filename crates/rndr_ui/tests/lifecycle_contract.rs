//! Characterization of existing family-specific admission, not unified errors.
use rndr_core::*;
use rndr_ui::{controls, selection::*, Card, UiError, UiId, UiPlugin};

fn setup() -> (App, Entity) {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    let parent = app.world_mut().spawn((Layout::column(),));
    (app, parent)
}
fn button(label: &str) -> controls::ButtonProps {
    controls::ButtonProps::new(label, controls::HostIntent::Command(7))
}
#[test]
fn parent_and_props_precede_identity_without_allocating_or_stealing_focus() {
    let (mut app, parent) = setup();
    let control = controls::Button::mount(app.world_mut(), parent, UiId(1), button("Old")).unwrap();
    request_focus(app.world_mut(), control.root);
    let entities = app.world().entities_with::<Layout>();
    let children = children_of(app.world(), parent);
    // Same collision, deliberately distinct historical family error contracts.
    assert_eq!(
        Checkbox::mount(
            app.world_mut(),
            parent,
            UiId(1),
            SelectionProps::new("Check"),
            false
        ),
        Err(UiError::InvalidProps)
    );
    assert_eq!(
        Card::mount(app.world_mut(), parent, UiId(1), "Card"),
        Err(UiError::IdentityConflict)
    );
    assert_eq!(
        controls::IconButton::mount(
            app.world_mut(),
            parent,
            UiId(1),
            "x",
            "Icon",
            button("Icon")
        ),
        Err(UiError::IdentityConflict)
    );
    app.world_mut().remove::<Layout>(parent);
    assert_eq!(
        controls::Button::mount(app.world_mut(), parent, UiId(1), button("")),
        Err(UiError::Stale)
    );
    assert_eq!(
        Checkbox::mount(
            app.world_mut(),
            parent,
            UiId(1),
            SelectionProps::new(""),
            true
        ),
        Err(UiError::Stale)
    );
    app.world_mut().insert(parent, Layout::column());
    assert_eq!(entities, app.world().entities_with::<Layout>());
    assert_eq!(children, children_of(app.world(), parent));
    assert_eq!(app.world().get::<Label>(control.label).unwrap().text, "Old");
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(control.root)
    );
    assert!(events::<SelectionChange>(app.world()).is_empty());
}
#[test]
fn retained_keys_are_parent_scoped_and_dead_generations_never_rebind() {
    let (mut app, parent) = setup();
    let other = app.world_mut().spawn((Layout::column(),));
    let first = controls::Button::mount(app.world_mut(), parent, UiId(4), button("One")).unwrap();
    let second = controls::Button::mount(app.world_mut(), other, UiId(4), button("Two")).unwrap();
    let updated =
        controls::Button::mount(app.world_mut(), parent, UiId(4), button("Changed")).unwrap();
    assert_eq!(first, updated);
    assert_ne!(first.root, second.root);
    app.world_mut().remove::<Label>(first.label);
    assert_eq!(
        controls::Button::mount(app.world_mut(), parent, UiId(4), button("Refused")),
        Err(UiError::Stale)
    );
    despawn(app.world_mut(), parent);
    let replacement = app.world_mut().spawn((Layout::column(),));
    assert_ne!(parent, replacement);
    assert_eq!(
        Checkbox::mount(
            app.world_mut(),
            parent,
            UiId(4),
            SelectionProps::new("Check"),
            true
        ),
        Err(UiError::Stale)
    );
    assert_eq!(rndr_ui::identity(app.world(), first.root), None);
    assert_eq!(rndr_ui::identity(app.world(), second.root), Some(UiId(4)));
}
#[test]
fn selector_reorders_by_id_and_invalid_update_preserves_open_popup_and_focus() {
    let (mut app, parent) = setup();
    let options = [
        SelectOption::new(10, "Alpha"),
        SelectOption::new(20, "Beta"),
    ];
    let handle = MultiSelect::mount(
        app.world_mut(),
        parent,
        UiId(1),
        SelectionProps::new("Tags"),
        &options,
        &[OptionId(20)],
    )
    .unwrap();
    request_focus(app.world_mut(), handle.root);
    app.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(NamedKey::Enter, Modifiers::default())];
    app.update();
    let popup = handle.popup(app.world()).unwrap();
    let alpha = handle.option(app.world(), OptionId(10)).unwrap();
    let beta = handle.option(app.world(), OptionId(20)).unwrap();
    assert!(!app.world().has::<Hidden>(popup));
    request_focus(app.world_mut(), beta);
    let before = app.world().entities_with::<Layout>();
    let duplicate = [options[0].clone(), options[0].clone()];
    assert_eq!(
        MultiSelect::mount(
            app.world_mut(),
            parent,
            UiId(1),
            SelectionProps::new("Refused"),
            &duplicate,
            &[]
        ),
        Err(UiError::InvalidProps)
    );
    assert_eq!(before, app.world().entities_with::<Layout>());
    assert_eq!(app.world().resource::<Focus>().unwrap().entity, Some(beta));
    assert!(!app.world().has::<Hidden>(popup));
    let reordered = [options[1].clone(), options[0].clone()];
    assert_eq!(
        MultiSelect::mount(
            app.world_mut(),
            parent,
            UiId(1),
            SelectionProps::new("Tags"),
            &reordered,
            &[OptionId(20), OptionId(20), OptionId(999)]
        )
        .unwrap(),
        handle
    );
    assert_eq!(children_of(app.world(), popup), vec![beta, alpha]);
    assert_eq!(handle.option(app.world(), OptionId(10)), Some(alpha));
    assert_eq!(app.world().resource::<Focus>().unwrap().entity, Some(beta));
    assert!(app.world().get::<SelectedState>(beta).unwrap().0);
    assert!(!app.world().get::<SelectedState>(alpha).unwrap().0);
    assert!(events::<SelectionChange>(app.world()).is_empty());
    handle.unmount(app.world_mut()).unwrap();
    assert!(!app.world().is_alive(popup));
    assert!(!app.world().is_alive(alpha));
}
