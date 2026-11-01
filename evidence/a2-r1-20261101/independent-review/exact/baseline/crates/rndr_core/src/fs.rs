use std::any::Any;
use std::path::{Path, PathBuf};

use crate::caret::set_editing;
use crate::disclosure::{toggle_disclosure, Disclosure, Focusable};
use crate::entity::Entity;
use crate::hierarchy::{children_of, despawn, set_parent, Parent};
use crate::input::{Focus, Input, Key};
use crate::layout::{Layout, Size};
use crate::pane::{set_selection, Selection};
use crate::style::{Body, Color, Label};
use crate::world::{Component, World};

impl Component for FsPath {}
impl Component for Dirty {}
impl Component for DirtyMark {}

/// One child returned by [`Fs::list`]. Core skips hidden names and symlinks.
#[derive(Clone, Debug)]
pub struct DirEntry {
    /// Full path of the child.
    pub path: PathBuf,
    /// This child is a directory.
    pub dir: bool,
    /// This child is a symlink. Core skips those when building the tree.
    pub symlink: bool,
}

/// Byte store behind [`FsPath`]. A plugin inserts this as [`Files`].
pub trait Fs: Any + Send + Sync {
    /// True when `path` is a directory. Files and missing paths are false.
    fn is_dir(&self, path: &Path) -> bool;
    /// True when `path` names a file or directory.
    fn exists(&self, path: &Path) -> bool;
    /// Size in bytes. `None` when `path` is missing or the size cannot be read.
    fn len(&self, path: &Path) -> Option<u64>;
    /// Immediate children of `path`. Empty when `path` is not a directory.
    /// Include hidden names and symlinks; the tree builder skips both.
    fn list(&self, path: &Path) -> Vec<DirEntry>;
    /// File bytes. `None` for a directory, a missing path, or a read error.
    fn read(&self, path: &Path) -> Option<Vec<u8>>;
    /// Store `bytes` at `path`, replacing a file. False when `path` is a
    /// directory or the write fails.
    fn write(&mut self, path: &Path, bytes: &[u8]) -> bool;
    /// Create one directory and not its parents. False when it already exists
    /// or the create fails.
    fn create_dir(&mut self, path: &Path) -> bool;
    /// Move `from` onto `to`. False when the move fails. Core does not call
    /// this when `to` already exists.
    fn rename(&mut self, from: &Path, to: &Path) -> bool;
    /// Delete a file. False for a directory or a missing path.
    fn remove_file(&mut self, path: &Path) -> bool;
    /// Delete an empty directory. False when `path` is not an empty directory.
    fn remove_dir(&mut self, path: &Path) -> bool;
    /// True when `path` is a directory with no children. A file is false.
    fn is_empty_dir(&self, path: &Path) -> bool;
}

/// The [`Fs`] for this app. Missing means file commands no-op.
pub struct Files {
    inner: Box<dyn Fs>,
}

impl Files {
    /// Box `fs` as the app file store.
    pub fn new(fs: impl Fs + 'static) -> Self {
        Self {
            inner: Box::new(fs),
        }
    }

    /// [`Fs::is_dir`] on the inner store.
    pub fn is_dir(&self, path: &Path) -> bool {
        self.inner.is_dir(path)
    }

    /// [`Fs::exists`] on the inner store.
    pub fn exists(&self, path: &Path) -> bool {
        self.inner.exists(path)
    }

    /// [`Fs::len`] on the inner store.
    pub fn len(&self, path: &Path) -> Option<u64> {
        self.inner.len(path)
    }

    /// [`Fs::list`] on the inner store.
    pub fn list(&self, path: &Path) -> Vec<DirEntry> {
        self.inner.list(path)
    }

    /// [`Fs::read`] on the inner store.
    pub fn read(&self, path: &Path) -> Option<Vec<u8>> {
        self.inner.read(path)
    }

    /// [`Fs::write`] on the inner store.
    pub fn write(&mut self, path: &Path, bytes: &[u8]) -> bool {
        self.inner.write(path, bytes)
    }

    /// [`Fs::create_dir`] on the inner store.
    pub fn create_dir(&mut self, path: &Path) -> bool {
        self.inner.create_dir(path)
    }

    /// [`Fs::rename`] on the inner store.
    pub fn rename(&mut self, from: &Path, to: &Path) -> bool {
        self.inner.rename(from, to)
    }

    /// [`Fs::remove_file`] on the inner store.
    pub fn remove_file(&mut self, path: &Path) -> bool {
        self.inner.remove_file(path)
    }

    /// [`Fs::remove_dir`] on the inner store.
    pub fn remove_dir(&mut self, path: &Path) -> bool {
        self.inner.remove_dir(path)
    }

    /// [`Fs::is_empty_dir`] on the inner store.
    pub fn is_empty_dir(&self, path: &Path) -> bool {
        self.inner.is_empty_dir(path)
    }

    /// Borrow the inner store as `T` when it was inserted as that type.
    pub fn downcast_ref<T: Fs + 'static>(&self) -> Option<&T> {
        (self.inner.as_ref() as &dyn Any).downcast_ref()
    }
}

const MAX_FILE_BYTES: u64 = 256 * 1024;

const DIR_COLOR: Color = Color {
    r: 0.20,
    g: 0.36,
    b: 0.52,
    a: 1.0,
};
const FILE_COLOR: Color = Color {
    r: 0.46,
    g: 0.34,
    b: 0.24,
    a: 1.0,
};

/// Disk path for a tree node. Nodes without this stay in-memory [`crate::NodeSpec`] trees.
#[derive(Clone, Debug)]
pub struct FsPath {
    /// Path this node edits.
    pub path: PathBuf,
    /// Directory node. A file is false.
    pub dir: bool,
    /// The body has been read, or the node was just created empty.
    /// [`apply_fs_load`] skips a node once this is set.
    pub loaded: bool,
    /// The body may be written back. False after a missing file, a read
    /// error, a NUL byte, text that is not UTF-8, or more than 256 KiB.
    /// A node just created is true.
    pub readable: bool,
}

/// Title or body changed on a node that has [`FsPath`].
#[derive(Clone, Copy, Debug, Default)]
pub struct Dirty;

/// Painted on the tree header while its node has [`Dirty`].
#[derive(Clone, Copy, Debug, Default)]
pub struct DirtyMark;

/// Keep [`DirtyMark`] on the header of each [`Dirty`] node.
pub fn apply_dirty_marks(world: &mut World) {
    let mut headers = Vec::new();
    for entity in world.entities_with::<Dirty>() {
        if let Some(header) = header_entity(world, entity) {
            headers.push(header);
        }
    }
    for header in world.entities_with::<DirtyMark>() {
        if !headers.contains(&header) {
            world.remove::<DirtyMark>(header);
        }
    }
    for header in headers {
        world.insert(header, DirtyMark);
    }
}

/// Spawn a file or directory node. Directories always get an empty [`Disclosure`].
pub fn spawn_fs_node(world: &mut World, path: impl Into<PathBuf>) -> Entity {
    let path = path.into();
    let dir = world
        .resource::<Files>()
        .is_some_and(|files| files.is_dir(&path));
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());

    let mut layout = Layout::column();
    layout.gap = 4.0;
    layout.width = Size::Fill;
    layout.height = Size::Hug;

    let fs = FsPath {
        path,
        dir,
        loaded: false,
        readable: true,
    };
    let color = if dir { DIR_COLOR } else { FILE_COLOR };
    let root = if dir {
        world.spawn((
            layout,
            Disclosure::new(Vec::new()),
            Focusable,
            Body::default(),
            fs,
        ))
    } else {
        world.spawn((layout, Focusable, Body::default(), fs))
    };

    let mut header = Layout::row();
    header.padding = 8.0;
    header.width = Size::Fill;
    header.height = Size::Px(36.0);
    let header_entity = world.spawn((header, color, Label::new(name)));
    set_parent(world, header_entity, root);
    root
}

pub(crate) fn spawn_fs_children(world: &mut World, parent: Entity) -> Vec<Entity> {
    let Some(path) = world.get::<FsPath>(parent).map(|fs| fs.path.clone()) else {
        return Vec::new();
    };
    let paths = list_dir(world, &path);
    let mut spawned = Vec::new();
    for child_path in paths {
        let child = spawn_fs_node(world, child_path);
        set_parent(world, child, parent);
        spawned.push(child);
    }
    spawned
}

/// Load the selected file into [`Body`] before the pane spawns.
///
/// Directories and nodes with [`FsPath::loaded`] set are left alone. A missing
/// file, a read error, a NUL byte, text that is not UTF-8, or more than 256 KiB
/// stores an empty [`Body`] and sets [`FsPath::readable`] false.
pub fn apply_fs_load(world: &mut World) {
    let Some(selected) = world
        .resource::<Selection>()
        .and_then(|selection| selection.entity)
        .filter(|entity| world.is_alive(*entity))
    else {
        return;
    };
    let Some(fs) = world.get::<FsPath>(selected).cloned() else {
        return;
    };
    if fs.dir || fs.loaded {
        return;
    }

    let (text, readable) = read_text(world, &fs.path);
    if let Some(body) = world.get_mut::<Body>(selected) {
        body.text = text;
    }
    if let Some(fs) = world.get_mut::<FsPath>(selected) {
        fs.loaded = true;
        fs.readable = readable;
    }
    world.remove::<Dirty>(selected);
}

/// Write or rename the selected path on [`Key::Save`].
///
/// The key is consumed either way. A header that differs from the file name
/// is renamed first. An empty name, a slash, a backslash, a NUL, an existing
/// destination, or a failed rename restores the header and leaves the file
/// untouched.
/// The body is written only when [`FsPath::readable`] is set and the node is
/// not a directory. [`Dirty`] is cleared after success.
pub fn apply_fs_save(world: &mut World) {
    let save = world
        .resource::<Input>()
        .is_some_and(|input| input.key == Some(Key::Save));
    if !save {
        return;
    }
    if let Some(input) = world.resource_mut::<Input>() {
        input.key = None;
    }

    let Some(selected) = world
        .resource::<Selection>()
        .and_then(|selection| selection.entity)
        .filter(|entity| world.is_alive(*entity))
    else {
        return;
    };
    let _ = save_entity(world, selected);
}

/// Create or unlink on [`Key::New`] / [`Key::Delete`]. Shift + New makes a directory.
pub fn apply_fs_edit(world: &mut World) {
    let (key, shift) = match world.resource::<Input>() {
        Some(input) => (input.key, input.shift),
        None => return,
    };
    match key {
        Some(Key::New) => {
            if let Some(input) = world.resource_mut::<Input>() {
                input.key = None;
            }
            create_untitled(world, shift);
        }
        Some(Key::Delete) => {
            if let Some(input) = world.resource_mut::<Input>() {
                input.key = None;
            }
            delete_focused(world);
        }
        _ => {}
    }
}

pub(crate) fn save_entity(world: &mut World, entity: Entity) -> bool {
    if !world.has::<FsPath>(entity) {
        return false;
    }
    if !rename_if_needed(world, entity) {
        return false;
    }
    let Some(fs) = world.get::<FsPath>(entity).cloned() else {
        return false;
    };
    if !fs.dir && fs.readable {
        let text = world
            .get::<Body>(entity)
            .map(|body| body.text.clone())
            .unwrap_or_default();
        let path = fs.path.clone();
        let wrote = world
            .resource_mut::<Files>()
            .is_some_and(|files| files.write(&path, text.as_bytes()));
        if !wrote {
            return false;
        }
    }
    world.remove::<Dirty>(entity);
    true
}

pub(crate) fn flush_dirty_subtree(world: &mut World, entity: Entity) {
    let mut stack = children_of(world, entity);
    while let Some(child) = stack.pop() {
        stack.extend(children_of(world, child));
        if world.has::<Dirty>(child) && world.has::<FsPath>(child) {
            let _ = save_entity(world, child);
        }
    }
}

fn parent_dir(world: &World, entity: Entity) -> Option<Entity> {
    if world.get::<FsPath>(entity).is_some_and(|fs| fs.dir) {
        return Some(entity);
    }
    let mut current = entity;
    loop {
        let parent = world.get::<Parent>(current)?.entity;
        if !world.is_alive(parent) {
            return None;
        }
        if world.get::<FsPath>(parent).is_some_and(|fs| fs.dir) {
            return Some(parent);
        }
        current = parent;
    }
}

fn unique_untitled(files: &Files, parent: &Path, dir: bool) -> String {
    let first = if dir {
        "untitled".to_string()
    } else {
        "untitled.txt".to_string()
    };
    if !files.exists(&parent.join(&first)) {
        return first;
    }
    for n in 2usize.. {
        let name = if dir {
            format!("untitled-{n}")
        } else {
            format!("untitled-{n}.txt")
        };
        if !files.exists(&parent.join(&name)) {
            return name;
        }
    }
    unreachable!()
}

fn create_untitled(world: &mut World, dir: bool) {
    let Some(focused) = world
        .resource::<Focus>()
        .and_then(|focus| focus.entity)
        .filter(|entity| world.is_alive(*entity))
    else {
        return;
    };
    let Some(parent) = parent_dir(world, focused) else {
        return;
    };
    let Some(parent_path) = world.get::<FsPath>(parent).map(|fs| fs.path.clone()) else {
        return;
    };
    let Some(name) = world
        .resource::<Files>()
        .map(|files| unique_untitled(files, &parent_path, dir))
    else {
        return;
    };
    let dest = parent_path.join(name);
    let created = if dir {
        world
            .resource_mut::<Files>()
            .is_some_and(|files| files.create_dir(&dest))
    } else {
        world
            .resource_mut::<Files>()
            .is_some_and(|files| files.write(&dest, b""))
    };
    if !created {
        return;
    }

    let open = world
        .get::<Disclosure>(parent)
        .is_some_and(|disclosure| disclosure.open);
    let child = if open {
        let child = spawn_fs_node(world, dest);
        set_parent(world, child, parent);
        if let Some(disclosure) = world.get_mut::<Disclosure>(parent) {
            disclosure.spawned.push(child);
        }
        child
    } else {
        toggle_disclosure(world, parent);
        let spawned = world
            .get::<Disclosure>(parent)
            .map(|disclosure| disclosure.spawned.clone())
            .unwrap_or_default();
        let Some(child) = spawned.into_iter().find(|entity| {
            world
                .get::<FsPath>(*entity)
                .is_some_and(|fs| fs.path == dest)
        }) else {
            return;
        };
        child
    };

    if let Some(fs) = world.get_mut::<FsPath>(child) {
        fs.loaded = true;
        fs.readable = true;
    }
    if let Some(focus) = world.resource_mut::<Focus>() {
        focus.entity = Some(child);
    }
    set_selection(world, Some(child));
    set_editing(world, Some(child));
}

fn delete_focused(world: &mut World) {
    let Some(entity) = world
        .resource::<Focus>()
        .and_then(|focus| focus.entity)
        .filter(|entity| world.is_alive(*entity))
    else {
        return;
    };
    if !world.has::<FsPath>(entity) {
        return;
    }
    let Some(parent) = world.get::<Parent>(entity).map(|parent| parent.entity) else {
        return;
    };
    if !world.is_alive(parent) || !world.has::<FsPath>(parent) {
        return;
    }
    if world.has::<Dirty>(entity) && !save_entity(world, entity) {
        return;
    }
    let Some(fs) = world.get::<FsPath>(entity).cloned() else {
        return;
    };
    let path = fs.path.clone();
    let removed = if fs.dir {
        let empty = world
            .resource::<Files>()
            .is_some_and(|files| files.is_empty_dir(&path));
        if !empty {
            return;
        }
        world
            .resource_mut::<Files>()
            .is_some_and(|files| files.remove_dir(&path))
    } else {
        world
            .resource_mut::<Files>()
            .is_some_and(|files| files.remove_file(&path))
    };
    if !removed {
        return;
    }

    if let Some(disclosure) = world.get_mut::<Disclosure>(parent) {
        disclosure.spawned.retain(|spawned| *spawned != entity);
    }
    despawn(world, entity);
    if let Some(focus) = world.resource_mut::<Focus>() {
        focus.entity = Some(parent);
    }
    set_selection(world, Some(parent));
    set_editing(world, None);
}

fn list_dir(world: &World, path: &Path) -> Vec<PathBuf> {
    let Some(files) = world.resource::<Files>() else {
        return Vec::new();
    };
    let mut entries: Vec<(bool, String, PathBuf)> = Vec::new();
    for entry in files.list(path) {
        if entry.symlink {
            continue;
        }
        let Some(name) = entry.path.file_name() else {
            continue;
        };
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        entries.push((entry.dir, name.into_owned(), entry.path));
    }
    entries.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    entries.into_iter().map(|(_, _, path)| path).collect()
}

fn read_text(world: &World, path: &Path) -> (String, bool) {
    let Some(files) = world.resource::<Files>() else {
        return (String::new(), false);
    };
    let Some(len) = files.len(path) else {
        return (String::new(), false);
    };
    if len > MAX_FILE_BYTES {
        return (String::new(), false);
    }
    let Some(bytes) = files.read(path) else {
        return (String::new(), false);
    };
    if bytes.contains(&0) {
        return (String::new(), false);
    }
    match String::from_utf8(bytes) {
        Ok(text) => (text, true),
        Err(_) => (String::new(), false),
    }
}

fn rename_if_needed(world: &mut World, entity: Entity) -> bool {
    let Some(fs) = world.get::<FsPath>(entity).cloned() else {
        return true;
    };
    let Some(title) = header_text(world, entity) else {
        return true;
    };
    let current = fs
        .path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    if title == current {
        return true;
    }
    if !valid_name(&title) {
        set_header_text(world, entity, current);
        return false;
    }
    let Some(parent) = fs.path.parent() else {
        set_header_text(world, entity, current);
        return false;
    };
    let dest = parent.join(&title);
    let from = fs.path.clone();
    let renamed = world.resource_mut::<Files>().is_some_and(|files| {
        if files.exists(&dest) {
            return false;
        }
        files.rename(&from, &dest)
    });
    if !renamed {
        set_header_text(world, entity, current);
        return false;
    }
    rebase_paths(world, entity, &fs.path, &dest);
    true
}

fn valid_name(name: &str) -> bool {
    !name.is_empty() && !name.contains('/') && !name.contains('\\') && !name.contains('\0')
}

fn rebase_paths(world: &mut World, entity: Entity, old: &Path, new: &Path) {
    if let Some(fs) = world.get_mut::<FsPath>(entity) {
        if fs.path == old {
            fs.path = new.to_path_buf();
        } else if let Ok(rest) = fs.path.strip_prefix(old) {
            fs.path = new.join(rest);
        }
    }
    for child in children_of(world, entity) {
        rebase_paths(world, child, old, new);
    }
}

fn header_entity(world: &World, entity: Entity) -> Option<Entity> {
    for child in children_of(world, entity) {
        if world.has::<Label>(child) {
            return Some(child);
        }
    }
    world.has::<Label>(entity).then_some(entity)
}

fn header_text(world: &World, entity: Entity) -> Option<String> {
    let header = header_entity(world, entity)?;
    world.get::<Label>(header).map(|label| label.text.clone())
}

fn set_header_text(world: &mut World, entity: Entity, text: String) {
    for child in children_of(world, entity) {
        if world.has::<Label>(child) {
            if let Some(label) = world.get_mut::<Label>(child) {
                label.text = text;
            }
            return;
        }
    }
}
