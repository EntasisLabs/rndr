//! [`std::fs`] backend for [`rndr_core::Fs`].
//!
//! [`StdFsPlugin`] inserts [`StdFs`] as the [`rndr_core::Files`] resource.
//! Headless tests that must not touch the disk use [`rndr_core::MemoryFs`]
//! instead.
//!
//! # Example
//!
//! ```
//! use rndr_core::{App, Files};
//! use rndr_std::StdFsPlugin;
//!
//! let mut app = App::new();
//! app.add_plugin(StdFsPlugin);
//! assert!(app.world().has_resource::<Files>());
//! ```

#![deny(missing_docs)]

use std::fs;
use std::path::Path;

use rndr_core::{App, DirEntry, Files, Fs, Plugin};

/// [`std::fs`] backend for [`Fs`].
///
/// Insert it with [`StdFsPlugin`] or [`Files::new`]. Calls use the process
/// filesystem and keep no state. A missing path, a permission error, or a
/// failed write comes back as `false`, `None`, or an empty list.
/// [`Fs::is_dir`], [`Fs::exists`], and [`Fs::len`] follow symlinks.
/// [`Fs::list`] records each directory entry itself: a symlink sets
/// [`DirEntry::symlink`] and leaves [`DirEntry::dir`] false, and an entry
/// whose file type cannot be read is skipped. [`Fs::create_dir`] creates
/// one directory. [`Fs::remove_dir`] removes an empty directory. This
/// does not panic.
#[derive(Clone, Copy, Debug, Default)]
pub struct StdFs;

impl Fs for StdFs {
    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn len(&self, path: &Path) -> Option<u64> {
        fs::metadata(path).ok().map(|meta| meta.len())
    }

    fn list(&self, path: &Path) -> Vec<DirEntry> {
        let Ok(read) = fs::read_dir(path) else {
            return Vec::new();
        };
        let mut entries = Vec::new();
        for entry in read.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            entries.push(DirEntry {
                path: entry.path(),
                dir: file_type.is_dir(),
                symlink: file_type.is_symlink(),
            });
        }
        entries
    }

    fn read(&self, path: &Path) -> Option<Vec<u8>> {
        fs::read(path).ok()
    }

    fn write(&mut self, path: &Path, bytes: &[u8]) -> bool {
        fs::write(path, bytes).is_ok()
    }

    fn create_dir(&mut self, path: &Path) -> bool {
        fs::create_dir(path).is_ok()
    }

    fn rename(&mut self, from: &Path, to: &Path) -> bool {
        fs::rename(from, to).is_ok()
    }

    fn remove_file(&mut self, path: &Path) -> bool {
        fs::remove_file(path).is_ok()
    }

    fn remove_dir(&mut self, path: &Path) -> bool {
        fs::remove_dir(path).is_ok()
    }

    fn is_empty_dir(&self, path: &Path) -> bool {
        fs::read_dir(path)
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(false)
    }
}

/// Inserts [`StdFs`] as the [`Files`] resource.
///
/// Pass it to [`App::add_plugin`] before a frame reads the disk. An
/// existing [`Files`] resource is replaced. The plugin stores the backend
/// and creates no directories.
pub struct StdFsPlugin;

impl Plugin for StdFsPlugin {
    fn build(self, app: &mut App) {
        app.world_mut().insert_resource(Files::new(StdFs));
    }
}
