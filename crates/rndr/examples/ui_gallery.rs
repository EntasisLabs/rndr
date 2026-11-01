//! Native launcher for the same synthetic, headless-tested operating gallery.
//! This does not adopt UI components in a legacy/default application.
#[path = "../../rndr_ui/examples/support/operating.rs"]
mod operating;
use rndr_core::{LogicalSize, WindowConfig};
use rndr_ui::*;
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let has = |s: &str| args.iter().any(|a| a == s);
    let width = if has("--narrow") { 320.0 } else { 1000.0 };
    let scale = if has("--200") { 2.0 } else { 1.0 };
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
        text_scale: scale,
        reading_scale: scale,
        ..UiTheme::default()
    };
    let mut app = operating::create(width, 700.0, theme, has("--long"));
    app.world_mut().insert_resource(WindowConfig {
        title: "rndr A2 — synthetic host (not Blooey)".into(),
        size: LogicalSize::new(width, 700.0),
        min_size: Some(LogicalSize::new(280.0, 240.0)),
        ..WindowConfig::default()
    });
    rndr::run(app);
}
