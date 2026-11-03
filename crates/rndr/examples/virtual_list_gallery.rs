//! Interactive bounded 100,000-record list. Synthetic data only.
#[path = "../../rndr_ui/examples/support/virtual_gallery.rs"]
mod gallery;
use rndr_core::*;
use rndr_ui::*;
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let has = |s: &str| args.iter().any(|a| a == s);
    let theme = UiTheme {
        appearance: if has("--dark") {
            Appearance::Dark
        } else {
            Appearance::Light
        },
        contrast: if has("--hc") {
            Contrast::High
        } else {
            Contrast::Standard
        },
        density: if has("--compact") {
            Density::Compact
        } else {
            Density::Comfortable
        },
        text_scale: if has("--200") { 2. } else { 1. },
        ..Default::default()
    };
    let mut app = gallery::create(100_000, 1100., 760., theme);
    app.world_mut().insert_resource(WindowConfig {
        title: "rndr / Virtual semantic list".into(),
        size: LogicalSize::new(1100., 760.),
        ..Default::default()
    });
    rndr::run(app);
}
