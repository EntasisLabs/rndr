//! GPU-free semantic components, styling and application-owned data editors.
//!
//! Install [`UiPlugin`] for core interaction, scrolling and semantic styling.
//! No window, GPU, filesystem runtime or parallel focus dispatcher is installed.
//! Collections offer retained bounded rows and virtual ListView rows, uniform
//! or variable height. Native screen-reader, IME and cross-platform visual
//! certification remain separate from headless tests.
#![deny(missing_docs)]
pub mod collections;
pub mod completion;
pub mod composition;
pub mod controls;
pub mod data;
pub mod editing;
pub mod editing_recipes;
mod lifecycle;
pub mod selection;
pub mod source_tools;
pub mod structured;
pub mod structured_embeds;
pub mod style;
pub mod surfaces;
pub mod theme_bridge;
pub mod tokens;
pub use composition::{section_layout, WorkspaceLayout};
use rndr_core::{App, Layout, Plugin, Schedule, Size};
pub use style::{detach_style, resolve_styles, Corner, UiColor, UiRecipe, UiStyle};
pub use surfaces::{separator, text, Callout, Card, Feedback, Surface, UiError, UiHandle, UiId};
pub use theme_bridge::apply_theme_bridge;
pub use tokens::{Appearance, Contrast, Density, Space, Token, Typography, UiTheme};

/// Read the caller-stable key of a mounted component root. Internal decoration,
/// row children and borrowed nodes have no component key. Keys are unique only
/// under their direct parent, not globally. This read-only adapter can be passed
/// to headless capture without introducing a renderer dependency into UI/core.
pub fn identity(world: &rndr_core::World, entity: rndr_core::Entity) -> Option<UiId> {
    controls::key_at(world, entity)
        .or_else(|| surfaces::key_at(world, entity))
        .or_else(|| editing::key_at(world, entity))
        .or_else(|| structured::key_at(world, entity))
}

/// Sole installation owner for UI styling and reusable core interaction.
/// Uses idempotent Chrome through GenericInteraction; never Shell or a new runtime.
/// Scrolling runs at the front of Input, before focus/activation can reveal rows.
/// Existing host `apply_scroll` registrations are safe: wheel deltas are consumed
/// and drag movement is measured from the last applied position, not accumulated twice.
/// Resources and systems live for the App lifetime; plugin uninstall unsupported.
pub struct UiPlugin;
struct Installed;
impl Plugin for UiPlugin {
    fn build(self, app: &mut App) {
        if app.world().has_resource::<Installed>() {
            return;
        }
        app.add_plugin(rndr_core::GenericInteraction);
        app.add_system_front(Schedule::Input, rndr_core::apply_scroll);
        // Installed after the front scroll registration, so a track claims its
        // input before parent scrolling regardless of Shell installation order.
        app.add_system_front(Schedule::Input, rndr_core::apply_scrollbars);
        if !app.world().has_resource::<UiTheme>() {
            app.world_mut().insert_resource(UiTheme::default());
        }
        app.world_mut().insert_resource(Installed);
        app.add_system(Schedule::Input, collections::requests);
        app.add_system(Schedule::PostUpdate, data::inspector_blur);
        app.add_system(Schedule::Layout, collections::styles);
        app.add_system(Schedule::Layout, data::styles);
        app.add_system(Schedule::Layout, controls::resolve_controls);
        app.add_system(Schedule::Layout, selection::styles);
        app.add_system(Schedule::Layout, resolve_styles);
        app.add_system(Schedule::Layout, structured::prepare_projection);
        app.add_system_front(Schedule::Input, structured_embeds::capture_exit);
        app.add_system(Schedule::PostUpdate, structured_embeds::prepare_input);
        app.add_system(Schedule::Layout, structured_embeds::reconcile);
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
