use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use rndr_core::{
    children_of, detail_pane, set_parent, shell, spawn_fs_node, spawn_node, tree_column, App, Body,
    Color, Dirty, Disclosure, EditField, Field, Focusable, FsPath, Input, Key, Label, NodeSpec,
    Pane, Parent, Schedule, World,
};

struct TempTree {
    path: PathBuf,
}

impl TempTree {
    fn new(name: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("rndr-fs-{name}-{nanos}"));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn write(&self, rel: &str, contents: impl AsRef<[u8]>) {
        let dest = self.path.join(rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(dest, contents).unwrap();
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn setup_into(world: &mut World, root: PathBuf) {
    let shell_root = world.spawn((shell(), Color::rgb(0.08, 0.09, 0.10)));
    let (tree_layout, tree_scroll) = tree_column();
    let column = world.spawn((tree_layout, tree_scroll, Color::rgb(0.10, 0.11, 0.13)));
    set_parent(world, column, shell_root);
    let node = spawn_fs_node(world, root);
    set_parent(world, node, column);
    let (pane_layout, pane_scroll, pane) = detail_pane();
    let detail = world.spawn((pane_layout, pane_scroll, pane, Color::rgb(0.12, 0.13, 0.15)));
    set_parent(world, detail, shell_root);
}

fn boot_fs(path: PathBuf) -> App {
    let mut app = App::new();
    setup_into(app.world_mut(), path);
    app.update();
    app
}

fn press(app: &mut App, key: Key) {
    app.world_mut().resource_mut::<Input>().unwrap().key = Some(key);
    app.update();
}

fn type_char(app: &mut App, ch: char) {
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .chars
        .push(ch);
    app.update();
}

fn fs_root(world: &World) -> rndr_core::Entity {
    for entity in world.entities_with::<FsPath>() {
        let parent = world.get::<Parent>(entity).map(|parent| parent.entity);
        if parent.is_none_or(|parent| !world.has::<FsPath>(parent)) {
            return entity;
        }
    }
    panic!("no fs root");
}

fn node_named(world: &World, name: &str) -> rndr_core::Entity {
    for (header, label) in world.query::<Label>() {
        if label.text != name {
            continue;
        }
        let Some(parent) = world.get::<Parent>(header) else {
            continue;
        };
        if world.has::<Focusable>(parent.entity) {
            return parent.entity;
        }
    }
    panic!("no tree node labeled {name}");
}

fn pane_field(world: &World, kind: EditField) -> rndr_core::Entity {
    let pane = world
        .entities_with::<Pane>()
        .into_iter()
        .next()
        .expect("pane");
    for entity in world.get::<Pane>(pane).unwrap().spawned.iter().copied() {
        if world
            .get::<Field>(entity)
            .is_some_and(|field| field.kind == kind)
        {
            return entity;
        }
    }
    panic!("no pane field {kind:?}");
}

fn child_names(world: &World, parent: rndr_core::Entity) -> Vec<String> {
    let mut names = Vec::new();
    for child in children_of(world, parent) {
        if !world.has::<Focusable>(child) {
            continue;
        }
        for header in children_of(world, child) {
            if let Some(label) = world.get::<Label>(header) {
                names.push(label.text.clone());
            }
        }
    }
    names
}

#[test]
fn expand_lists_dir_and_skips_hidden() {
    let temp = TempTree::new("list");
    temp.write("hello.txt", "hi");
    temp.write("sub/n.txt", "n");
    temp.write(".hidden", "nope");

    let mut app = boot_fs(temp.path.clone());
    press(&mut app, Key::Down);
    press(&mut app, Key::Right);

    let root = fs_root(app.world());
    let names = child_names(app.world(), root);
    assert!(names.contains(&"hello.txt".to_string()));
    assert!(names.contains(&"sub".to_string()));
    assert!(!names.contains(&".hidden".to_string()));
    assert!(app.world().get::<Disclosure>(root).unwrap().open);
}

#[test]
fn enter_file_loads_body() {
    let temp = TempTree::new("load");
    temp.write("hello.txt", "hi");
    temp.write("sub/n.txt", "n");

    let mut app = boot_fs(temp.path.clone());
    press(&mut app, Key::Down);
    press(&mut app, Key::Right);
    press(&mut app, Key::Down);
    press(&mut app, Key::Down);
    press(&mut app, Key::Enter);

    let hello = node_named(app.world(), "hello.txt");
    assert_eq!(app.world().get::<Body>(hello).unwrap().text, "hi");
    assert!(app.world().get::<FsPath>(hello).unwrap().loaded);
    assert!(app.world().get::<FsPath>(hello).unwrap().readable);
    assert_eq!(
        app.world()
            .get::<Label>(pane_field(app.world(), EditField::Body))
            .unwrap()
            .text,
        "hi"
    );
}

#[test]
fn save_writes_disk_and_clears_dirty() {
    let temp = TempTree::new("save");
    temp.write("hello.txt", "hi");
    temp.write("sub/n.txt", "n");

    let mut app = boot_fs(temp.path.clone());
    press(&mut app, Key::Down);
    press(&mut app, Key::Right);
    press(&mut app, Key::Down);
    press(&mut app, Key::Down);
    press(&mut app, Key::Enter);
    press(&mut app, Key::Tab);
    type_char(&mut app, 'x');

    let hello = node_named(app.world(), "hello.txt");
    assert!(app.world().has::<Dirty>(hello));
    press(&mut app, Key::Save);

    assert!(!app.world().has::<Dirty>(hello));
    assert_eq!(
        fs::read_to_string(temp.path.join("hello.txt")).unwrap(),
        "hix"
    );
}

#[test]
fn collapse_and_reopen_reads_saved_text() {
    let temp = TempTree::new("reload");
    temp.write("hello.txt", "hi");
    temp.write("sub/n.txt", "n");

    let mut app = boot_fs(temp.path.clone());
    press(&mut app, Key::Down);
    press(&mut app, Key::Right);
    press(&mut app, Key::Down);
    press(&mut app, Key::Down);
    press(&mut app, Key::Enter);
    press(&mut app, Key::Tab);
    type_char(&mut app, 'x');
    press(&mut app, Key::Save);
    press(&mut app, Key::Escape);
    press(&mut app, Key::Escape);
    press(&mut app, Key::Left);
    press(&mut app, Key::Left);
    press(&mut app, Key::Right);
    press(&mut app, Key::Down);
    press(&mut app, Key::Down);
    press(&mut app, Key::Enter);

    let hello = node_named(app.world(), "hello.txt");
    assert_eq!(app.world().get::<Body>(hello).unwrap().text, "hix");
    assert_eq!(
        fs::read_to_string(temp.path.join("hello.txt")).unwrap(),
        "hix"
    );
}

#[test]
fn save_renames_the_file() {
    let temp = TempTree::new("rename");
    temp.write("hello.txt", "hi");
    temp.write("sub/n.txt", "n");

    let mut app = boot_fs(temp.path.clone());
    press(&mut app, Key::Down);
    press(&mut app, Key::Right);
    press(&mut app, Key::Down);
    press(&mut app, Key::Down);
    press(&mut app, Key::Enter);
    for _ in 0.."hello.txt".len() {
        press(&mut app, Key::Backspace);
    }
    for ch in "hello2.txt".chars() {
        type_char(&mut app, ch);
    }
    press(&mut app, Key::Save);

    assert!(!temp.path.join("hello.txt").exists());
    assert_eq!(
        fs::read_to_string(temp.path.join("hello2.txt")).unwrap(),
        "hi"
    );
    let hello = node_named(app.world(), "hello2.txt");
    assert_eq!(
        app.world().get::<FsPath>(hello).unwrap().path,
        temp.path.join("hello2.txt")
    );
    assert!(!app.world().has::<Dirty>(hello));
}

#[test]
fn binary_file_does_not_save() {
    let temp = TempTree::new("binary");
    temp.write("bin.dat", [0u8]);

    let mut app = boot_fs(temp.path.clone());
    press(&mut app, Key::Down);
    press(&mut app, Key::Right);
    press(&mut app, Key::Down);
    press(&mut app, Key::Enter);

    let bin = node_named(app.world(), "bin.dat");
    assert_eq!(app.world().get::<Body>(bin).unwrap().text, "");
    assert!(!app.world().get::<FsPath>(bin).unwrap().readable);
    if let Some(body) = app.world_mut().get_mut::<Body>(bin) {
        body.text = "nope".to_string();
    }
    press(&mut app, Key::Save);
    assert_eq!(fs::read(temp.path.join("bin.dat")).unwrap(), vec![0]);
}

#[test]
fn nodespec_tree_ignores_save() {
    let mut app = App::new();
    app.add_system(Schedule::Startup, |world: &mut World| {
        let (layout, scroll) = rndr_core::scroll_page();
        let page = world.spawn((layout, scroll, Color::rgb(0.1, 0.1, 0.1)));
        let node = spawn_node(
            world,
            NodeSpec::branch(
                "Alpha",
                Color::rgb(0.2, 0.3, 0.4),
                vec![NodeSpec::leaf("Alpha / 1", Color::rgb(0.3, 0.4, 0.3))],
            ),
        );
        set_parent(world, node, page);
    });
    app.update();

    press(&mut app, Key::Down);
    press(&mut app, Key::Save);
    press(&mut app, Key::Right);
    let alpha = node_named(app.world(), "Alpha");
    assert!(app.world().get::<Disclosure>(alpha).unwrap().open);
    assert!(!app.world().has::<FsPath>(alpha));
    assert!(!app.world().has::<Dirty>(alpha));
}
