//! GPU-free, opt-in A0/A1 foundations and operating A2 components above rndr_core.
//! Core contract acceptance is scoped; native platform acceptance remains separate.
//! See docs/design/A1-api-decisions.md for the bounded interaction contract.
#![warn(missing_docs)]
pub mod collections;
pub mod composition;
pub mod controls;
pub mod data;
pub mod style;
pub mod surfaces;
pub mod tokens;
pub use composition::{section_layout, WorkspaceLayout};
use rndr_core::{App, Layout, Plugin, Schedule, Size};
pub use style::{detach_style, resolve_styles, Corner, UiColor, UiRecipe, UiStyle};
pub use surfaces::{separator, text, Callout, Card, Feedback, Surface, UiError, UiHandle, UiId};
pub use tokens::{Appearance, Contrast, Density, Space, Token, Typography, UiTheme};

/// Sole installation owner for UI styling and reusable core interaction.
/// Uses idempotent Chrome through GenericInteraction; never Shell or a new runtime.
/// Resources and systems live for the App lifetime; plugin uninstall unsupported.
pub struct UiPlugin;
struct Installed;
impl Plugin for UiPlugin {
    fn build(self, app: &mut App) {
        if app.world().has_resource::<Installed>() {
            return;
        }
        app.add_plugin(rndr_core::GenericInteraction);
        if !app.world().has_resource::<UiTheme>() {
            app.world_mut().insert_resource(UiTheme::default());
        }
        app.world_mut().insert_resource(Installed);
        app.add_system(Schedule::Input, collections::requests);
        app.add_system(Schedule::PostUpdate, data::inspector_blur);
        app.add_system(Schedule::Layout, collections::styles);
        app.add_system(Schedule::Layout, data::styles);
        app.add_system(Schedule::Layout, controls::resolve_controls);
        app.add_system(Schedule::Layout, resolve_styles);
        app.add_system(Schedule::Layout, data::measure_cells);
        app.add_system(Schedule::Layout, controls::resolve_toolbars);
    }
}

/// Passive reading-column layout in logical points. Host supplies available
/// width on resize; no parallel layout engine or window lifecycle is installed.
/// Target measure ~65 characters using a conservative half-em approximation;
/// actual glyph widths are engine-dependent and verified through layout/paint.
/// Narrow windows remove excess margins rather than imposing a minimum width.
pub fn reading_layout(available: f32, theme: &UiTheme) -> Layout {
    reading_layout_at_measure(available, 65.0, theme)
}

/// Adjustable reading measure (55–75 nominal characters). This is a font-width
/// approximation, not a claim that every script has the same advance.
pub fn reading_layout_at_measure(available: f32, characters: f32, theme: &UiTheme) -> Layout {
    let characters = if characters.is_finite() {
        characters.clamp(55.0, 75.0)
    } else {
        65.0
    };
    let available = if available.is_finite() {
        available.max(0.0)
    } else {
        0.0
    };
    let type_size = theme.typography(Typography::Reading).size;
    let padding = if available < 360.0 {
        8.0
    } else {
        theme.space(Space::X24)
    };
    Layout {
        width: Size::Px(available.min(characters * type_size * 0.5 + padding * 2.0)),
        padding,
        gap: theme.space(Space::X16),
        ..Layout::column()
    }
}
