#![cfg(feature = "parley")]
//! CPU shaping/layout/paint-command characterization. No native event loop or GPU presentation.
#[path = "../../rndr_core/tests/editor_scale_cost.rs"]
mod scale;

#[test]
#[ignore = "opt-in release shaped scale characterization"]
fn shaped_editor_scale_costs() {
    scale::measure(
        Some(std::sync::Arc::new(rndr_render::ParleyText::bundled())),
        |app| {
            let _ = rndr_paint::frame(app);
        },
    );
}
