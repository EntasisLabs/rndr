use std::fs;
use std::path::{Path, PathBuf};

use crate::disclosure::{Disclosure, Focusable};
use crate::entity::Entity;
use crate::hierarchy::{children_of, set_parent};
use crate::input::{Input, Key};
use crate::layout::{Layout, Size};
use crate::pane::Selection;
use crate::style::{Body, Color, Label};
use crate::world::{Component, World};

impl Component for FsPath {}
impl Component for Dirty {}

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
    pub path: PathBuf,
    pub dir: bool,
    pub loaded: bool,
    pub readable: bool,
}

/// Title or body changed on a node that has [`FsPath`].
#[derive(Clone, Copy, Debug, Default)]
pub struct Dirty;

/// Spawn a file or directory node. Directories always get an empty [`Disclosure`].
pub fn spawn_fs_node(world: &mut World, path: impl Into<PathBuf>) -> Entity {
    let path = path.into();
    let dir = path.is_dir();
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
    let mut spawned = Vec::new();
    for child_path in list_dir(&path) {
        let child = spawn_fs_node(world, child_path);
        set_parent(world, child, parent);
        spawned.push(child);
    }
    spawned
}

/// Load the selected file into [`Body`] before the pane spawns.
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

    let (text, readable) = read_text(&fs.path);
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
    if !world.has::<FsPath>(selected) {
        return;
    }

    if !rename_if_needed(world, selected) {
        return;
    }

    let Some(fs) = world.get::<FsPath>(selected).cloned() else {
        return;
    };
    if !fs.dir && fs.readable {
        let text = world
            .get::<Body>(selected)
            .map(|body| body.text.clone())
            .unwrap_or_default();
        if fs::write(&fs.path, text).is_err() {
            return;
        }
    }
    world.remove::<Dirty>(selected);
}

fn list_dir(path: &Path) -> Vec<PathBuf> {
    let Ok(read) = fs::read_dir(path) else {
        return Vec::new();
    };
    let mut entries: Vec<(bool, String, PathBuf)> = Vec::new();
    for entry in read.flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        entries.push((
            file_type.is_dir(),
            name.to_string_lossy().into_owned(),
            entry.path(),
        ));
    }
    entries.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    entries.into_iter().map(|(_, _, path)| path).collect()
}

fn read_text(path: &Path) -> (String, bool) {
    let Ok(meta) = fs::metadata(path) else {
        return (String::new(), false);
    };
    if meta.len() > MAX_FILE_BYTES {
        return (String::new(), false);
    }
    let Ok(bytes) = fs::read(path) else {
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
    if dest.exists() {
        set_header_text(world, entity, current);
        return false;
    }
    if fs::rename(&fs.path, &dest).is_err() {
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

fn header_text(world: &World, entity: Entity) -> Option<String> {
    for child in children_of(world, entity) {
        if let Some(label) = world.get::<Label>(child) {
            return Some(label.text.clone());
        }
    }
    world.get::<Label>(entity).map(|label| label.text.clone())
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
