//! Native open and save dialogs. The `dialog` feature calls these.

use std::path::PathBuf;

use rndr_core::{FileDialog, FileDialogKind};
use winit::window::Window;

pub(crate) fn show(parent: Option<&Window>, dialog: &FileDialog) -> Option<PathBuf> {
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
        builder = builder.set_parent(parent);
    }
    match dialog.kind {
        FileDialogKind::Open => builder.pick_file(),
        FileDialogKind::Save => builder.save_file(),
    }
}
