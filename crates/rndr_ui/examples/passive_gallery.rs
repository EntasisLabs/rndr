//! Generate reviewable *headless display-list* evidence, not native screenshots.
#[path = "../tests/support/gallery.rs"]
mod gallery;
use rndr_core::{Label, Rect, World};
use rndr_ui::*;
use std::path::Path;

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("output directory inside the isolated worktree");
    let path = Path::new(&path);
    std::fs::create_dir_all(path).unwrap();
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
        let snapshot = rndr_paint::quads_snapshot(&frame);
        let layout = layout_report(app.world());
        let report = format!("HEADLESS A0; not native/GPU/AT certification\n{name}; width={width}; text_scale={scale}\n{layout}\n{snapshot}");
        std::fs::write(path.join(format!("{name}.txt")), report).unwrap();
        println!(
            "{name}: quads={} glyphs={} labels={}",
            frame.quads.len(),
            frame
                .quads
                .iter()
                .filter(|q| q.uv != rndr_paint::solid_uv())
                .count(),
            app.world().entities_with::<Label>().len()
        );
    }
}
fn layout_report(world: &World) -> String {
    let mut entities = world.entities_with::<Rect>();
    entities.sort_by_key(|e| (e.index(), e.generation()));
    entities
        .into_iter()
        .map(|e| {
            format!(
                "layout {}:{} {:?}\n",
                e.index(),
                e.generation(),
                world.get::<Rect>(e).unwrap()
            )
        })
        .collect()
}
