#[path = "../examples/support/selection_gallery.rs"]
mod gallery;
use rndr_core::*;
use rndr_ui::{data::*, Appearance, Contrast, Density, UiTheme};
fn frame(a: &mut App) {
    rndr_paint::frame(a);
}
fn batch(a: &mut App, keys: Vec<NamedKey>) {
    a.world_mut().resource_mut::<Input>().unwrap().keys = keys
        .into_iter()
        .map(|k| KeyEvent::named(k, Modifiers::default()))
        .collect();
    frame(a);
}
#[test]
fn gallery_host_applies_standalone_but_keeps_inspector_drafts_until_commit() {
    let mut a = gallery::create(1180., 960., UiTheme::default(), &[]);
    frame(&mut a);
    let g = a.world().resource::<gallery::Gallery>().unwrap().clone();
    request_focus(a.world_mut(), g.checkbox.unwrap().root);
    batch(&mut a, vec![NamedKey::Enter]);
    assert!(!a.world().resource::<gallery::Gallery>().unwrap().checked);
    assert_eq!(
        a.world().resource::<gallery::Gallery>().unwrap().committed[0].value,
        PropertyValue::Boolean(true)
    );
    let h = g.inspector.unwrap();
    let editor = h.slot(a.world(), PropertyId(1)).unwrap().editor;
    request_focus(a.world_mut(), editor);
    batch(&mut a, vec![NamedKey::Enter]);
    assert_eq!(
        h.draft(a.world(), PropertyId(1)).unwrap().value,
        Some(PropertyValue::Boolean(false))
    );
    assert_eq!(
        a.world().resource::<gallery::Gallery>().unwrap().committed[0].value,
        PropertyValue::Boolean(true)
    );
    batch(&mut a, vec![NamedKey::Tab, NamedKey::Enter]);
    assert_eq!(
        a.world().resource::<gallery::Gallery>().unwrap().committed[0].value,
        PropertyValue::Boolean(false)
    );
}
#[test]
fn gallery_invalid_multi_selection_shows_empty_draft_and_rejects_commit() {
    let mut a = gallery::create(1180., 960., UiTheme::default(), &[]);
    frame(&mut a);
    let h = a
        .world()
        .resource::<gallery::Gallery>()
        .unwrap()
        .inspector
        .unwrap();
    let editor = h.slot(a.world(), PropertyId(3)).unwrap().editor;
    request_focus(a.world_mut(), editor);
    batch(&mut a, vec![NamedKey::Enter, NamedKey::Enter]);
    let d = h.draft(a.world(), PropertyId(3)).unwrap();
    assert!(d.error.is_some());
    assert_eq!(d.raw, "");
    assert!(!h.commit(a.world_mut(), PropertyId(3)).unwrap());
    let popup = a
        .world()
        .entities_with::<AnchoredPopup>()
        .into_iter()
        .find(|e| a.world().get::<AnchoredPopup>(*e).unwrap().anchor == editor)
        .unwrap();
    for e in children_of(a.world(), popup) {
        if a.world().has::<SelectedState>(e) {
            assert!(!a.world().get::<SelectedState>(e).unwrap().0);
        }
    }
    assert_eq!(
        a.world().resource::<gallery::Gallery>().unwrap().committed[2].value,
        PropertyValue::Choices(vec![1])
    );
}
#[test]
fn gallery_matrix_keeps_focus_popup_clipping_and_readable_named_roles() {
    for width in [380., 1180.] {
        for scale in [1., 2.] {
            for preset in [
                "",
                "--empty",
                "--many",
                "--readonly",
                "--disabled",
                "--invalid",
            ] {
                let theme = UiTheme {
                    appearance: Appearance::Dark,
                    density: Density::Compact,
                    contrast: Contrast::High,
                    text_scale: scale,
                    reading_scale: scale,
                    ..Default::default()
                };
                let mut a = gallery::create(width, 960., theme, &[preset.into()]);
                frame(&mut a);
                frame(&mut a);
                let g = a.world().resource::<gallery::Gallery>().unwrap().clone();
                let control = g.multi.unwrap().root;
                request_focus(a.world_mut(), control);
                reveal_entity(a.world_mut(), control);
                frame(&mut a);
                if preset != "--readonly" && preset != "--disabled" {
                    batch(&mut a, vec![NamedKey::Enter]);
                    let p = g.multi.unwrap().popup(a.world()).unwrap();
                    assert!(!a.world().has::<Hidden>(p));
                    let r = a.world().get::<Rect>(p).unwrap();
                    assert!(
                        r.x >= 0. && r.y >= 0. && r.x + r.w <= width + 0.1 && r.y + r.h <= 960.1,
                        "width={width} scale={scale} preset={preset} popup={r:?}"
                    );
                    assert!(access_tree(a.world())
                        .nodes
                        .iter()
                        .any(|n| n.role == AccessRole::ListBox));
                } else {
                    assert!(!access_details(a.world(), control).activate);
                }
                assert!(access_tree(a.world())
                    .nodes
                    .iter()
                    .any(|n| n.role == AccessRole::CheckBox && n.name == "Receive updates"));
            }
        }
    }
}
