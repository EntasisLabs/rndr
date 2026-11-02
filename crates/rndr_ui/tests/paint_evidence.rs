use rndr_core::*;
use rndr_ui::*;
#[path = "support/gallery.rs"]
mod gallery;

#[test]
fn actual_glyph_geometry_scales_and_device_scale_keeps_logical_layout() {
    let mut geometry = vec![];
    for scale in [1.0, 2.0] {
        let mut app = App::new();
        app.add_plugin(UiPlugin);
        app.world_mut().insert_resource(UiTheme {
            reading_scale: scale,
            ..UiTheme::default()
        });
        let parent = app.world_mut().spawn((Layout {
            width: Size::Px(300.0),
            ..Layout::column()
        },));
        let label = text(app.world_mut(), parent, "MMMM", Typography::Reading).unwrap();
        let frame = rndr_paint::frame(&mut app);
        let glyph = frame
            .quads
            .iter()
            .find(|q| q.uv != rndr_paint::solid_uv())
            .unwrap();
        geometry.push((glyph.rect.w, glyph.rect.h));
        let before = frame.quads.clone();
        let logical = app.world().get::<TextLayout>(label).unwrap().clone();
        for device_scale in [1.25, 2.0] {
            app.world_mut().insert_resource(Viewport {
                scale: device_scale,
                ..Viewport::default()
            });
            let frame = rndr_paint::frame(&mut app);
            // Device-pixel snapping is visible at non-integral 14pt metrics;
            // the former 16pt fixture happened to divide both tested grids.
            // Shaping stays logical, while painted edges may round by <1pt.
            assert_eq!(app.world().get::<TextLayout>(label).unwrap(), &logical);
            assert_eq!(before.len(), frame.quads.len());
            for (a, b) in before.iter().zip(&frame.quads) {
                assert_eq!(a.uv, b.uv);
                assert_eq!(a.color, b.color);
                for (x, y) in [
                    (a.rect.x, b.rect.x),
                    (a.rect.y, b.rect.y),
                    (a.rect.x + a.rect.w, b.rect.x + b.rect.w),
                    (a.rect.y + a.rect.h, b.rect.y + b.rect.h),
                ] {
                    assert!(
                        (x - y).abs() <= 1.0,
                        "device snap moved logical edge: {x} -> {y}"
                    );
                }
            }
        }
    }
    assert!((geometry[1].0 - geometry[0].0 * 2.0).abs() < 0.01);
    assert!((geometry[1].1 - geometry[0].1 * 2.0).abs() < 0.01);
}
#[test]
fn nested_scroll_uses_real_paint_scissors_and_owned_updates_preserve_scroll() {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.world_mut().insert_resource(Viewport::new(240.0, 160.0));
    let root = app.world_mut().spawn((
        Layout {
            width: Size::Px(240.0),
            height: Size::Px(160.0),
            ..Layout::column()
        },
        Scroll::default(),
    ));
    let port = app.world_mut().spawn((
        Layout {
            width: Size::Px(220.0),
            height: Size::Px(90.0),
            ..Layout::column()
        },
        Scroll::default(),
    ));
    set_parent(app.world_mut(), port, root);
    let card = Card::mount(app.world_mut(), port, UiId(1), "Long nested document").unwrap();
    let body = text(
        app.world_mut(),
        card.content,
        &"Long content wraps in the nested scroll port. ".repeat(30),
        Typography::Reading,
    )
    .unwrap();
    card.adopt(app.world_mut(), body).unwrap();
    let before = rndr_paint::frame(&mut app);
    let glyphs: Vec<_> = before
        .quads
        .iter()
        .filter(|q| q.uv != rndr_paint::solid_uv())
        .collect();
    assert!(!glyphs.is_empty());
    for glyph in glyphs {
        assert!(glyph.clip.w <= 220.0 && glyph.clip.h <= 90.0);
        assert!(glyph.clip.y + glyph.clip.h <= 90.01);
    }
    app.world_mut().get_mut::<Scroll>(port).unwrap().offset = [0.0, -30.0];
    let scrolled = rndr_paint::frame(&mut app);
    assert_ne!(
        rndr_paint::quads_snapshot(&before),
        rndr_paint::quads_snapshot(&scrolled)
    );
    Card::mount(app.world_mut(), port, UiId(1), "Updated title").unwrap();
    app.world_mut()
        .resource_mut::<UiTheme>()
        .unwrap()
        .appearance = Appearance::Dark;
    let _ = rndr_paint::frame(&mut app);
    assert_eq!(
        app.world().get::<Scroll>(port).unwrap().offset,
        [0.0, -30.0]
    );
    assert!(app
        .world()
        .get::<Label>(body)
        .unwrap()
        .text
        .starts_with("Long content"));
}
#[test]
fn passive_title_toggle_separator_and_adjustable_measure() {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    let root = app.world_mut().spawn((Layout {
        width: Size::Px(800.0),
        ..Layout::column()
    },));
    let card = Card::mount(app.world_mut(), root, UiId(1), "").unwrap();
    app.update();
    card.set_title(app.world_mut(), "Visible title").unwrap();
    let sep = separator(app.world_mut(), card.content).unwrap();
    let frame = rndr_paint::frame(&mut app);
    assert!(access_tree(app.world())
        .nodes
        .iter()
        .any(|n| n.name == "Visible title"));
    let rect = *app.world().get::<Rect>(sep).unwrap();
    assert_eq!(rect.h, 1.0);
    assert!(frame
        .quads
        .iter()
        .any(|q| q.rect == rect && q.uv == rndr_paint::solid_uv()));
    assert!(!access_details(app.world(), sep).focus);
    card.set_title(app.world_mut(), "").unwrap();
    app.update();
    assert!(!access_tree(app.world())
        .nodes
        .iter()
        .any(|n| n.name == "Visible title"));
    assert_ne!(
        reading_layout_at_measure(900.0, 55.0, &UiTheme::default()).width,
        reading_layout_at_measure(900.0, 75.0, &UiTheme::default()).width
    );
    assert_eq!(UiTheme::default().focus_gap(), 2.0);
}
#[test]
fn deterministic_actual_display_list_matches_saved_headless_artifacts() {
    for (name, appearance, contrast, density, width, scale) in [
        (
            "light-wide",
            Appearance::Light,
            Contrast::Standard,
            Density::Comfortable,
            800.0,
            1.0,
        ),
        (
            "dark-wide",
            Appearance::Dark,
            Contrast::Standard,
            Density::Comfortable,
            800.0,
            1.0,
        ),
        (
            "light-narrow-200",
            Appearance::Light,
            Contrast::Standard,
            Density::Comfortable,
            280.0,
            2.0,
        ),
        (
            "dark-compact-200",
            Appearance::Dark,
            Contrast::Standard,
            Density::Compact,
            320.0,
            2.0,
        ),
        (
            "high-light-200",
            Appearance::Light,
            Contrast::High,
            Density::Comfortable,
            360.0,
            2.0,
        ),
        (
            "high-dark-200",
            Appearance::Dark,
            Contrast::High,
            Density::Comfortable,
            360.0,
            2.0,
        ),
    ] {
        let theme = UiTheme {
            appearance,
            contrast,
            density,
            text_scale: scale,
            reading_scale: scale,
            ..UiTheme::default()
        };
        let (mut app, _, _) = gallery::scene(theme, width);
        let frame = rndr_paint::frame(&mut app);
        let actual = rndr_paint::quads_snapshot(&frame);
        let expected = std::fs::read_to_string(format!(
            // The historical ui-gallery evidence is immutable. These active
            // snapshots cover the refined text boxes/type roles, not visual sign-off.
            "{}/../../evidence/refinement-1/passive-gallery/{name}.txt",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        assert!(expected.ends_with(&actual), "golden mismatch: {name}");
    }
}
