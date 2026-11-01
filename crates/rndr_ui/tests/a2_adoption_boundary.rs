use rndr_core::*;
use rndr_ui::{
    controls::{self, *},
    *,
};
#[test]
fn passive_adoption_rejects_all_operating_ownership_and_plain_transfer_still_works() {
    let mut a = App::new();
    a.add_plugin(UiPlugin);
    let w = a.world_mut();
    let root = w.spawn((Layout {
        width: Size::Px(320.0),
        ..Layout::column()
    },));
    let card = Card::mount(w, root, UiId(1), "Owner").unwrap();
    let button = controls::Button::mount(
        w,
        root,
        UiId(2),
        ButtonProps::new("Command", HostIntent::Command(1)),
    )
    .unwrap();
    let source = SourceAction::mount(
        w,
        root,
        UiId(3),
        SourceLocator {
            document: 1,
            revision: 2,
            anchor: 3,
        },
        true,
    )
    .unwrap();
    let mode = ModeSelector::mount(
        w,
        root,
        UiId(4),
        "Mode",
        &["Source", "Preview"],
        TabOrientation::Horizontal,
        0,
    )
    .unwrap();
    let toolbar = Toolbar::mount(w, root, UiId(5), 320.0).unwrap();
    for e in [
        button.root,
        button.label,
        source.root,
        source.label,
        mode.root,
        mode.group,
        mode.tabs[0].root,
        mode.tabs[0].label,
        mode.panels[0],
        toolbar.root,
    ] {
        let parent = w.get::<Parent>(e).map(|p| p.entity);
        assert_eq!(
            card.adopt(w, e),
            Err(UiError::InvalidAdoption),
            "cannot adopt existing operating ownership {e:?}"
        );
        assert_eq!(w.get::<Parent>(e).map(|p| p.entity), parent);
    }
    let wrapper = w.spawn((Layout::column(),));
    set_parent(w, wrapper, root);
    let nested = controls::Button::mount(
        w,
        wrapper,
        UiId(1),
        ButtonProps::new("Nested", HostIntent::Command(2)),
    )
    .unwrap();
    assert_eq!(card.adopt(w, wrapper), Err(UiError::InvalidAdoption));
    assert_eq!(w.get::<Parent>(wrapper).unwrap().entity, root);
    nested.validate(w).unwrap();
    let plain = w.spawn((Layout::column(), Label::new("Explicit plain transfer")));
    card.adopt(w, plain).unwrap();
    set_parent(w, button.root, card.content);
    card.destroy(w).unwrap();
    assert!(!w.is_alive(plain));
    assert!(w.get::<Parent>(button.root).is_none());
    button.validate(w).unwrap();
    source.validate(w).unwrap();
    mode.validate(w).unwrap();
    toolbar.validate(w).unwrap();
}
