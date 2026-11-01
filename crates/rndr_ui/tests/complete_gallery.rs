use rndr_core::*;
use rndr_ui::*;
#[path = "support/gallery.rs"]
mod gallery;
#[test]
fn full_gallery_has_no_unintentional_nested_hug_clipping_in_any_a0_state() {
    for appearance in [Appearance::Light, Appearance::Dark] {
        for contrast in [Contrast::Standard, Contrast::High] {
            for density in [Density::Comfortable, Density::Compact] {
                for text_scale in [1.0, 2.0] {
                    for width in [280.0, 800.0] {
                        let theme = UiTheme {
                            appearance,
                            contrast,
                            density,
                            text_scale,
                            reading_scale: text_scale,
                            ..UiTheme::default()
                        };
                        let (mut app, _, _) = gallery::scene(theme, width);
                        let frame = rndr_paint::frame(&mut app);
                        for parent in app.world().entities_with::<Layout>() {
                            if !interaction_visible(app.world(), parent) {
                                continue;
                            }
                            if app.world().get::<Layout>(parent).unwrap().height != Size::Hug {
                                continue;
                            }
                            let parent_rect = *app.world().get::<Rect>(parent).unwrap();
                            for child in children_of(app.world(), parent) {
                                if app.world().has::<Hidden>(child) {
                                    continue;
                                }
                                let rect = *app.world().get::<Rect>(child).unwrap();
                                assert!(rect.y>=parent_rect.y-0.01&&rect.y+rect.h<=parent_rect.y+parent_rect.h+0.01,"{appearance:?}/{contrast:?}/{density:?}/{text_scale}/{width}: {parent_rect:?} clips {rect:?}");
                            }
                        }
                        let expected_ascii = app
                            .world()
                            .entities_with::<Label>()
                            .into_iter()
                            .filter(|e| interaction_visible(app.world(), *e))
                            .map(|e| {
                                app.world()
                                    .get::<Label>(e)
                                    .unwrap()
                                    .text
                                    .chars()
                                    .filter(|c| c.is_ascii_graphic())
                                    .count()
                            })
                            .sum::<usize>();
                        let glyphs = frame
                            .quads
                            .iter()
                            .filter(|q| q.uv != rndr_paint::solid_uv())
                            .count();
                        assert!(
                            glyphs >= expected_ascii,
                            "missing visible ASCII glyphs: {glyphs} < {expected_ascii}"
                        );
                        assert!(access_tree(app.world())
                            .nodes
                            .iter()
                            .any(|node| node.name.contains("日本語 / العربية")));
                    }
                }
            }
        }
    }
}
