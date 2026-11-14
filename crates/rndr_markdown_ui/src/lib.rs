//! Retained live markdown editing.
//!
//! The bytes and the concrete tree stay in [`rndr_markdown::Session`]. This
//! crate paints that tree and turns keystrokes, toolbar commands, and IME
//! commits into splices on the session. It does not attach a source session,
//! and [`rndr_ui::editing::EditorSurface`] is unchanged.
//!
//! Install [`MarkdownPlugin`] and [`rndr_ui::UiPlugin`] is installed with it.
//! Open a document in [`MarkdownDocuments`], then [`MarkdownSurface::mount`].
//!
//! Headless tests cover typing, structure, Liquid fields, undo, copy and
//! paste, composition, and the accessibility tree. They do not certify
//! VoiceOver, NVDA, Orca, or a platform IME.
#![deny(missing_docs)]

mod access;
mod document;
mod intent;
mod project;
mod surface;

pub use access::{access_snapshot, set_access_selection, MarkdownAccessSnapshot};
pub use document::{MarkdownDocuments, MarkdownId};
pub use project::{measure_block, project, update_projection, DisplayBlock, DisplayKind, Palette};
pub use surface::{
    document_of, MarkdownCommand, MarkdownSurface, MarkdownSurfaceError, MarkdownSurfaceHandle,
    MarkdownSurfaceProps,
};

use rndr_core::{App, Focus, Plugin, PointerDragOwner, Schedule};

/// Input, journal polling, and the press claim that keeps a drag inside the
/// surface a selection instead of a scroll.
pub struct MarkdownPlugin;

impl Plugin for MarkdownPlugin {
    fn build(self, app: &mut App) {
        app.add_plugin(rndr_ui::UiPlugin);
        if !app.world().has_resource::<MarkdownDocuments>() {
            app.world_mut()
                .insert_resource(MarkdownDocuments::default());
        }
        if !app.world().has_resource::<Focus>() {
            app.world_mut().insert_resource(Focus::default());
        }
        if !app.world().has_resource::<PointerDragOwner>() {
            app.world_mut()
                .insert_resource(PointerDragOwner(surface::owns_press));
        }
        app.add_system(Schedule::Input, surface::drive);
        app.add_system(Schedule::Update, surface::poll);
    }
}
