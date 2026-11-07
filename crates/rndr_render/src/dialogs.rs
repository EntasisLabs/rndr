//! Native open and save dialogs. The `dialog` feature calls these.

use std::path::PathBuf;

use rndr_core::{FileDialog, FileDialogKind};
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window::Window;

pub(crate) fn show(parent: Option<&dyn Window>, dialog: &FileDialog) -> Option<PathBuf> {
    let mut builder = rfd::FileDialog::new();
    if !dialog.title.is_empty() {
        builder = builder.set_title(dialog.title.clone());
    }
    for filter in &dialog.filters {
        builder = builder.add_filter(filter.name.clone(), &filter.extensions);
    }
    if let Some(name) = &dialog.file_name {
        builder = builder.set_file_name(name.clone());
    }
    if let Some(directory) = &dialog.directory {
        builder = builder.set_directory(directory);
    }
    if let Some(parent) = parent {
        builder = builder.set_parent(&DialogParent(parent));
    }
    match dialog.kind {
        FileDialogKind::Open => builder.pick_file(),
        FileDialogKind::Save => builder.save_file(),
    }
}

/// Sized parent so `rfd` can borrow a `dyn Window`.
struct DialogParent<'a>(&'a dyn Window);

impl HasWindowHandle for DialogParent<'_> {
    fn window_handle(
        &self,
    ) -> Result<winit::raw_window_handle::WindowHandle<'_>, winit::raw_window_handle::HandleError>
    {
        HasWindowHandle::window_handle(self.0)
    }
}

impl HasDisplayHandle for DialogParent<'_> {
    fn display_handle(
        &self,
    ) -> Result<winit::raw_window_handle::DisplayHandle<'_>, winit::raw_window_handle::HandleError>
    {
        HasDisplayHandle::display_handle(self.0)
    }
}
