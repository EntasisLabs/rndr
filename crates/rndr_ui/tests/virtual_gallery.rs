#[path = "../examples/support/virtual_gallery.rs"]
mod gallery;
use rndr_core::*;
use rndr_ui::{collections::ItemId, selection::*, *};
#[test]
fn actual_gallery_large_model_controls_and_theme_matrix_remain_bounded() {
    for theme in [
        UiTheme::default(),
        UiTheme {
            appearance: Appearance::Dark,
            ..Default::default()
        },
        UiTheme {
            contrast: Contrast::High,
            text_scale: 2.,
            ..Default::default()
        },
    ] {
        let mut a = gallery::create(100_000, 1100., 760., theme.clone());
        let frame = rndr_paint::frame(&mut a);
        assert!(!frame.quads.is_empty());
        let h = a.world().resource::<gallery::Gallery>().unwrap().list;
        assert_eq!(h.len(a.world()), 100_000);
        let count = a.world().entities_with::<Layout>().len();
        assert!(count < 220, "{count}");
        h.focus(a.world_mut(), ItemId(99_999)).unwrap();
        rndr_paint::frame(&mut a);
        let row = h.row(a.world(), ItemId(99_999)).unwrap();
        let r = a.world().get::<Rect>(row.root).unwrap();
        let port = a.world().get::<Rect>(h.root).unwrap();
        assert!(
            r.y >= port.y - 0.1 && r.y + r.h <= port.y + port.h + 0.1,
            "{r:?} {port:?}"
        );
        // Explicit host acknowledgement, retained through recycling by stable key.
        let checkbox = children_of(a.world(), row.content)
            .into_iter()
            .find(|e| {
                a.world()
                    .get::<SelectionAccess>(*e)
                    .is_some_and(|s| s.role == AccessRole::CheckBox)
            })
            .unwrap();
        send(
            a.world_mut(),
            SelectionChange {
                control: checkbox,
                value: SelectionValue::Checked(true),
            },
        );
        rndr_paint::frame(&mut a);
        assert!(a
            .world()
            .resource::<gallery::Gallery>()
            .unwrap()
            .checked
            .contains(&ItemId(99_999)));
        h.reveal(a.world_mut(), ItemId(0)).unwrap();
        rndr_paint::frame(&mut a);
        assert!(!a.world().is_alive(checkbox));
        h.reveal(a.world_mut(), ItemId(99_999)).unwrap();
        rndr_paint::frame(&mut a);
        let row = h.row(a.world(), ItemId(99_999)).unwrap();
        let checked = children_of(a.world(), row.content).iter().any(|e| {
            a.world()
                .get::<SelectionAccess>(*e)
                .is_some_and(|s| s.checked == Some(true))
        });
        assert!(checked);
        assert_eq!(h.selected(a.world()), None);
    }
}
