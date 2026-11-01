//! A request for a native open or save dialog.
//!
//! Core only stores the request. The desktop runner's `dialog` feature shows
//! it and sends [`FileChosen`]. Without that feature the request stays in the
//! world until something calls [`take_file_dialog`].

use std::path::PathBuf;

use crate::event::Event;
use crate::world::World;

/// Which files the dialog may pick.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileFilter {
    /// Label shown beside the extensions.
    pub name: String,
    /// Extensions without a leading dot, such as `"png"`.
    pub extensions: Vec<String>,
}

impl FileFilter {
    /// A filter named `name` whose extensions are `extensions`.
    pub fn new(
        name: impl Into<String>,
        extensions: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            name: name.into(),
            extensions: extensions.into_iter().map(Into::into).collect(),
        }
    }
}

/// Open an existing file, or choose a path to write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileDialogKind {
    /// The user picks a file that should already exist.
    Open,
    /// The user picks a path to write. The platform may ask before replacing.
    Save,
}

/// What to ask the platform to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDialog {
    /// Open or save.
    pub kind: FileDialogKind,
    /// Title-bar text. Empty leaves the platform title.
    pub title: String,
    /// Filters, in the order they should appear. Empty allows any file.
    pub filters: Vec<FileFilter>,
    /// Suggested file name for a save dialog.
    pub file_name: Option<String>,
    /// Directory the dialog opens in.
    pub directory: Option<PathBuf>,
}

impl FileDialog {
    /// An open dialog with no filters and the platform title.
    pub fn open() -> Self {
        Self {
            kind: FileDialogKind::Open,
            title: String::new(),
            filters: Vec::new(),
            file_name: None,
            directory: None,
        }
    }

    /// A save dialog with no filters and the platform title.
    pub fn save() -> Self {
        Self {
            kind: FileDialogKind::Save,
            title: String::new(),
            filters: Vec::new(),
            file_name: None,
            directory: None,
        }
    }

    /// Set the title and return the dialog.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Append one filter.
    pub fn filter(mut self, filter: FileFilter) -> Self {
        self.filters.push(filter);
        self
    }

    /// Suggest `name` as the file name.
    pub fn file_name(mut self, name: impl Into<String>) -> Self {
        self.file_name = Some(name.into());
        self
    }

    /// Open in `directory`.
    pub fn directory(mut self, directory: impl Into<PathBuf>) -> Self {
        self.directory = Some(directory.into());
        self
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PendingDialog {
    pub dialog: Option<FileDialog>,
}

/// Replace any pending dialog with `dialog`.
///
/// The runner shows one dialog at a time. A second request replaces the first
/// if the first has not been taken yet.
pub fn request_file_dialog(world: &mut World, dialog: FileDialog) {
    world.insert_resource(PendingDialog {
        dialog: Some(dialog),
    });
}

/// Take the pending dialog, if there is one.
///
/// The desktop `dialog` feature does this and then sends [`FileChosen`].
pub fn take_file_dialog(world: &mut World) -> Option<FileDialog> {
    world.resource_mut::<PendingDialog>()?.dialog.take()
}

/// The path the user picked, or `None` when they cancelled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileChosen {
    /// `Some` when the user confirmed a path.
    pub path: Option<PathBuf>,
}

impl Event for FileChosen {}
