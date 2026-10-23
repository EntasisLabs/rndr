use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::app::{App, Plugin};
use crate::fs::{DirEntry, Files, Fs};

#[derive(Clone, Debug)]
enum Node {
    Dir,
    File(Vec<u8>),
}

/// In-memory [`Fs`]. Headless tests use this instead of the disk.
#[derive(Clone, Debug, Default)]
pub struct MemoryFs {
    nodes: HashMap<PathBuf, Node>,
}

impl MemoryFs {
    /// Empty store. Seed it with [`Self::insert_dir_all`] and
    /// [`Self::insert_file`] before a test reads it.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create `path` and any missing parents as directories.
    ///
    /// An existing file or directory on that chain is left unchanged, so a
    /// file in the middle stays a file.
    pub fn insert_dir_all(&mut self, path: &Path) {
        let mut acc = PathBuf::new();
        for component in path.components() {
            acc.push(component);
            self.nodes.entry(acc.clone()).or_insert(Node::Dir);
        }
    }

    /// Store `bytes` at `path`, creating any missing parent directories.
    ///
    /// Replaces a node already at `path`, including a directory. Descendants
    /// of a replaced directory stay stored, but listings of that path are
    /// empty while it is a file. Parents that already exist are left as they
    /// are, same as [`Self::insert_dir_all`].
    pub fn insert_file(&mut self, path: &Path, bytes: impl AsRef<[u8]>) {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                self.insert_dir_all(parent);
            }
        }
        self.nodes
            .insert(path.to_path_buf(), Node::File(bytes.as_ref().to_vec()));
    }
}

impl Fs for MemoryFs {
    fn is_dir(&self, path: &Path) -> bool {
        matches!(self.nodes.get(path), Some(Node::Dir))
    }

    fn exists(&self, path: &Path) -> bool {
        self.nodes.contains_key(path)
    }

    fn len(&self, path: &Path) -> Option<u64> {
        match self.nodes.get(path) {
            Some(Node::File(bytes)) => Some(bytes.len() as u64),
            Some(Node::Dir) => Some(0),
            None => None,
        }
    }

    fn list(&self, path: &Path) -> Vec<DirEntry> {
        if !self.is_dir(path) {
            return Vec::new();
        }
        self.nodes
            .iter()
            .filter_map(|(key, node)| {
                if key.parent() != Some(path) {
                    return None;
                }
                Some(DirEntry {
                    path: key.clone(),
                    dir: matches!(node, Node::Dir),
                    symlink: false,
                })
            })
            .collect()
    }

    fn read(&self, path: &Path) -> Option<Vec<u8>> {
        match self.nodes.get(path) {
            Some(Node::File(bytes)) => Some(bytes.clone()),
            _ => None,
        }
    }

    fn write(&mut self, path: &Path, bytes: &[u8]) -> bool {
        if self.is_dir(path) || !parent_ready(self, path) {
            return false;
        }
        self.nodes
            .insert(path.to_path_buf(), Node::File(bytes.to_vec()));
        true
    }

    fn create_dir(&mut self, path: &Path) -> bool {
        if self.exists(path) || !parent_ready(self, path) {
            return false;
        }
        self.nodes.insert(path.to_path_buf(), Node::Dir);
        true
    }

    fn rename(&mut self, from: &Path, to: &Path) -> bool {
        if !self.exists(from) || self.exists(to) || to.starts_with(from) {
            return false;
        }
        let keys: Vec<PathBuf> = self
            .nodes
            .keys()
            .filter(|key| *key == from || key.starts_with(from))
            .cloned()
            .collect();
        let mut moved = Vec::new();
        for key in keys {
            let rest = key
                .strip_prefix(from)
                .unwrap_or(Path::new(""))
                .to_path_buf();
            moved.push((key, to.join(rest)));
        }
        for (old, new) in moved {
            if let Some(node) = self.nodes.remove(&old) {
                self.nodes.insert(new, node);
            }
        }
        true
    }

    fn remove_file(&mut self, path: &Path) -> bool {
        match self.nodes.get(path) {
            Some(Node::File(_)) => {
                self.nodes.remove(path);
                true
            }
            _ => false,
        }
    }

    fn remove_dir(&mut self, path: &Path) -> bool {
        if !self.is_dir(path) || !self.is_empty_dir(path) {
            return false;
        }
        self.nodes.remove(path);
        true
    }

    fn is_empty_dir(&self, path: &Path) -> bool {
        self.is_dir(path) && self.list(path).is_empty()
    }
}

fn parent_ready(fs: &MemoryFs, path: &Path) -> bool {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => fs.is_dir(parent),
        _ => true,
    }
}

/// Inserts a [`MemoryFs`] as the [`Files`] resource.
///
/// [`crate::Plugin::build`] moves [`MemoryFsPlugin::fs`] into the world and
/// replaces a [`Files`] resource that was already there.
pub struct MemoryFsPlugin {
    /// Store installed as [`Files`].
    pub fs: MemoryFs,
}

impl Plugin for MemoryFsPlugin {
    fn build(self, app: &mut App) {
        app.world_mut().insert_resource(Files::new(self.fs));
    }
}
