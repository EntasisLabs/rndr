//! Deterministic full-document display-list evidence, NOT native screenshots.
#[path = "support/operating.rs"]
mod operating;
use rndr_core::*;
use rndr_ui::*;
use std::{fs, path::PathBuf};
fn main() {
    let out = PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("new, nonexistent output directory"),
    );
    fs::create_dir(&out).expect("refuse existing output; preserve old evidence");
    let mut inventory = String::new();
    for (name, appearance, contrast, density, width, scale, long, preview) in [
        (
            "light-wide",
            Appearance::Light,
            Contrast::Standard,
            Density::Comfortable,
            1000.0,
            1.0,
            false,
            false,
        ),
        (
            "dark-wide-preview",
            Appearance::Dark,
            Contrast::Standard,
            Density::Comfortable,
            1000.0,
            1.0,
            false,
            true,
        ),
        (
            "light-narrow-200-long",
            Appearance::Light,
            Contrast::Standard,
            Density::Comfortable,
            320.0,
            2.0,
            true,
            false,
        ),
        (
            "dark-compact-200-long",
            Appearance::Dark,
            Contrast::Standard,
            Density::Compact,
            320.0,
            2.0,
            true,
            true,
        ),
        (
            "high-light-200",
            Appearance::Light,
            Contrast::High,
            Density::Comfortable,
            800.0,
            2.0,
            false,
            false,
        ),
        (
            "high-dark-compact",
            Appearance::Dark,
            Contrast::High,
            Density::Compact,
            800.0,
            1.0,
            false,
            true,
        ),
        (
            "reflow-400-equivalent",
            Appearance::Light,
            Contrast::Standard,
            Density::Comfortable,
            320.0,
            1.0,
            true,
            false,
        ),
        (
            "wide-long",
            Appearance::Dark,
            Contrast::Standard,
            Density::Comfortable,
            1000.0,
            1.0,
            true,
            false,
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
        let mut app = operating::create(width, 30000.0, theme, long);
        let demo = app.world().resource::<operating::Demo>().unwrap().clone();
        request_focus(app.world_mut(), demo.commands[0].root);
        app.world_mut().resource_mut::<Input>().unwrap().push_key(
            KeyEvent::named(NamedKey::Enter, Modifiers::new()),
            Platform::Mac,
        );
        app.update();
        app.world_mut().insert_resource(Input::default());
        assert_eq!(
            app.world().resource::<operating::Demo>().unwrap().accepted,
            1
        );
        if preview {
            assert_eq!(
                try_activate(app.world_mut(), demo.selector.tabs[1].root),
                ActivationResult::Accepted
            );
        }
        request_focus(app.world_mut(), demo.nav[0].root);
        let f = rndr_paint::frame(&mut app);
        let snapshot = rndr_paint::quads_snapshot(&f);
        let again = rndr_paint::frame(&mut app);
        assert_eq!(snapshot, rndr_paint::quads_snapshot(&again));
        fs::write(out.join(format!("{name}.display-list.txt")), snapshot).unwrap();
        let mut meta=format!("HEADLESS actual paint; full document height=30000 logical points; width={width} type={scale}; 320=1280/4 reflow (not physical DPI).\n");
        for node in access_tree(app.world()).nodes {
            meta.push_str(&format!("access {:?}\n", node));
        }
        // ECS component storage is hash-backed; evidence order is not input order.
        let mut labels = app.world().entities_with::<Label>();
        labels.sort_by_key(|e| (e.index(), e.generation()));
        for e in labels {
            if interaction_visible(app.world(), e) {
                meta.push_str(&format!(
                    "label {:?} {:?} {:?}\n",
                    e,
                    app.world().get::<Rect>(e),
                    app.world().get::<Label>(e).unwrap().text
                ));
            }
        }
        fs::write(out.join(format!("{name}.semantics-geometry.txt")), meta).unwrap();
        inventory.push_str(&format!(
            "{name}: quads={} glyphs={} accepted=1 preview={preview}\n",
            f.quads.len(),
            f.quads
                .iter()
                .filter(|q| q.uv != rndr_paint::solid_uv())
                .count()
        ));
    }
    fs::write(out.join("inventory.txt"), inventory).unwrap();
}
