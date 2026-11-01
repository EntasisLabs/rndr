use std::path::{Path, PathBuf};

use rndr_core::{
    children_of, detail_pane, set_parent, shell, spawn_fs_node, spawn_node, tree_column, App, Body,
    Color, Dirty, DirtyMark, Disclosure, EditField, Editing, Field, Files, Focus, Focusable,
    FsPath, Input, Key, Label, MemoryFs, MemoryFsPlugin, NodeSpec, Pane, Parent, Schedule, Shell,
    World,
};

struct MemTree {
    path: PathBuf,
    fs: MemoryFs,
}

impl MemTree {
    fn new(name: &str) -> Self {
        let path = PathBuf::from("/mem").join(name);
        let mut fs = MemoryFs::new();
        fs.insert_dir_all(&path);
        Self { path, fs }
    }

    fn write(&mut self, rel: &str, contents: impl AsRef<[u8]>) {
        self.fs.insert_file(&self.path.join(rel), contents);
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

fn boot_fs(tree: &MemTree) -> App {
    let mut app = App::new();
    app.add_plugin(MemoryFsPlugin {
        fs: tree.fs.clone(),
    });
    app.add_plugin(Shell);
    setup_into(app.world_mut(), tree.path.clone());
    app.update();
    app
}

fn stored(app: &App, path: &Path) -> Option<Vec<u8>> {
    app.world().resource::<Files>().unwrap().read(path)
}

fn stored_text(app: &App, path: &Path) -> String {
    String::from_utf8(stored(app, path).expect("file")).unwrap()
}

fn stored_exists(app: &App, path: &Path) -> bool {
    app.world().resource::<Files>().unwrap().exists(path)
}

fn stored_dir(app: &App, path: &Path) -> bool {
    app.world().resource::<Files>().unwrap().is_dir(path)
}

fn press(app: &mut App, key: Key) {
    press_mods(app, key, false);
}

fn press_mods(app: &mut App, key: Key, shift: bool) {
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.key = Some(key);
    input.shift = shift;
    app.update();
}

fn focused(world: &World) -> rndr_core::Entity {
    world
        .resource::<Focus>()
        .and_then(|focus| focus.entity)
        .expect("focus")
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
    let mut temp = MemTree::new("list");
    temp.write("hello.txt", "hi");
    temp.write("sub/n.txt", "n");
    temp.write(".hidden", "nope");

    let mut app = boot_fs(&temp);
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
    let mut temp = MemTree::new("load");
    temp.write("hello.txt", "hi");
    temp.write("sub/n.txt", "n");

    let mut app = boot_fs(&temp);
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
    let mut temp = MemTree::new("save");
    temp.write("hello.txt", "hi");
    temp.write("sub/n.txt", "n");

    let mut app = boot_fs(&temp);
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
    assert_eq!(stored_text(&app, &temp.path.join("hello.txt")), "hix");
}

#[test]
fn edit_marks_the_tree_row_until_save() {
    let mut temp = MemTree::new("mark");
    temp.write("hello.txt", "hi");
    temp.write("sub/n.txt", "n");

    let mut app = boot_fs(&temp);
    press(&mut app, Key::Down);
    press(&mut app, Key::Right);
    press(&mut app, Key::Down);
    press(&mut app, Key::Down);
    press(&mut app, Key::Enter);
    press(&mut app, Key::Tab);
    type_char(&mut app, 'x');

    let hello = node_named(app.world(), "hello.txt");
    let header = children_of(app.world(), hello)
        .into_iter()
        .find(|child| app.world().has::<Label>(*child))
        .unwrap();
    assert!(app.world().has::<Dirty>(hello));
    assert!(app.world().has::<DirtyMark>(header));

    press(&mut app, Key::Save);
    assert!(!app.world().has::<Dirty>(hello));
    assert!(!app.world().has::<DirtyMark>(header));
}

#[test]
fn collapse_and_reopen_reads_saved_text() {
    let mut temp = MemTree::new("reload");
    temp.write("hello.txt", "hi");
    temp.write("sub/n.txt", "n");

    let mut app = boot_fs(&temp);
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
    assert_eq!(stored_text(&app, &temp.path.join("hello.txt")), "hix");
}

#[test]
fn save_renames_the_file() {
    let mut temp = MemTree::new("rename");
    temp.write("hello.txt", "hi");
    temp.write("sub/n.txt", "n");

    let mut app = boot_fs(&temp);
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

    assert!(!stored_exists(&app, &temp.path.join("hello.txt")));
    assert_eq!(stored_text(&app, &temp.path.join("hello2.txt")), "hi");
    let hello = node_named(app.world(), "hello2.txt");
    assert_eq!(
        app.world().get::<FsPath>(hello).unwrap().path,
        temp.path.join("hello2.txt")
    );
    assert!(!app.world().has::<Dirty>(hello));
}

#[test]
fn binary_file_does_not_save() {
    let mut temp = MemTree::new("binary");
    temp.write("bin.dat", [0u8]);

    let mut app = boot_fs(&temp);
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
    assert_eq!(stored(&app, &temp.path.join("bin.dat")).unwrap(), vec![0]);
}

#[test]
fn nodespec_tree_ignores_save() {
    let mut app = App::new();
    app.add_plugin(Shell);
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

#[test]
fn new_creates_untitled_file() {
    let mut temp = MemTree::new("new-file");
    temp.write("hello.txt", "hi");

    let mut app = boot_fs(&temp);
    press(&mut app, Key::Down);
    press(&mut app, Key::Right);
    press(&mut app, Key::New);

    assert!(stored_exists(&app, &temp.path.join("untitled.txt")));
    assert_eq!(stored_text(&app, &temp.path.join("untitled.txt")), "");
    let untitled = node_named(app.world(), "untitled.txt");
    assert_eq!(focused(app.world()), untitled);
    let editing = app.world().resource::<Editing>().unwrap();
    assert_eq!(editing.source, Some(untitled));
    assert_eq!(editing.field, EditField::Title);
    assert!(app.world().get::<FsPath>(untitled).unwrap().loaded);
    assert!(app.world().get::<FsPath>(untitled).unwrap().readable);
    assert!(!app.world().get::<FsPath>(untitled).unwrap().dir);
}

#[test]
fn shift_new_creates_untitled_dir() {
    let mut temp = MemTree::new("new-dir");
    temp.write("hello.txt", "hi");

    let mut app = boot_fs(&temp);
    press(&mut app, Key::Down);
    press(&mut app, Key::Right);
    press_mods(&mut app, Key::New, true);

    assert!(stored_dir(&app, &temp.path.join("untitled")));
    let untitled = node_named(app.world(), "untitled");
    assert_eq!(focused(app.world()), untitled);
    assert!(app.world().get::<FsPath>(untitled).unwrap().dir);
    let editing = app.world().resource::<Editing>().unwrap();
    assert_eq!(editing.source, Some(untitled));
    assert_eq!(editing.field, EditField::Title);
}

#[test]
fn delete_clean_file() {
    let mut temp = MemTree::new("delete-file");
    temp.write("hello.txt", "hi");
    temp.write("sub/n.txt", "n");

    let mut app = boot_fs(&temp);
    press(&mut app, Key::Down);
    press(&mut app, Key::Right);
    press(&mut app, Key::Down);
    press(&mut app, Key::Down);
    press(&mut app, Key::Delete);

    assert!(!stored_exists(&app, &temp.path.join("hello.txt")));
    let root = fs_root(app.world());
    assert!(!child_names(app.world(), root).contains(&"hello.txt".to_string()));
    assert_eq!(focused(app.world()), root);
}

#[test]
fn delete_saves_dirty_then_unlinks() {
    let mut temp = MemTree::new("delete-dirty");
    temp.write("hello.txt", "hi");
    temp.write("sub/n.txt", "n");

    let mut app = boot_fs(&temp);
    press(&mut app, Key::Down);
    press(&mut app, Key::Right);
    press(&mut app, Key::Down);
    press(&mut app, Key::Down);
    press(&mut app, Key::Enter);
    press(&mut app, Key::Tab);
    type_char(&mut app, 'x');

    let hello = node_named(app.world(), "hello.txt");
    assert!(app.world().has::<Dirty>(hello));
    press(&mut app, Key::Escape);
    press(&mut app, Key::Delete);

    assert!(!stored_exists(&app, &temp.path.join("hello.txt")));
    let root = fs_root(app.world());
    assert!(!child_names(app.world(), root).contains(&"hello.txt".to_string()));
    assert_eq!(focused(app.world()), root);
}

#[test]
fn delete_nonempty_dir_is_noop() {
    let mut temp = MemTree::new("delete-dir");
    temp.write("hello.txt", "hi");
    temp.write("sub/n.txt", "n");

    let mut app = boot_fs(&temp);
    press(&mut app, Key::Down);
    press(&mut app, Key::Right);
    press(&mut app, Key::Down);
    press(&mut app, Key::Delete);

    assert!(stored_dir(&app, &temp.path.join("sub")));
    assert!(stored_exists(&app, &temp.path.join("sub/n.txt")));
    let sub = node_named(app.world(), "sub");
    assert!(app.world().is_alive(sub));
    assert_eq!(focused(app.world()), sub);
}

#[test]
fn delete_fs_root_is_noop() {
    let mut temp = MemTree::new("delete-root");
    temp.write("hello.txt", "hi");

    let mut app = boot_fs(&temp);
    press(&mut app, Key::Down);
    press(&mut app, Key::Delete);

    assert!(stored_dir(&app, &temp.path));
    let root = fs_root(app.world());
    assert!(app.world().is_alive(root));
    assert_eq!(focused(app.world()), root);
    assert_eq!(app.world().get::<FsPath>(root).unwrap().path, temp.path);
}

#[test]
fn collapse_flushes_dirty_child() {
    let mut temp = MemTree::new("collapse-flush");
    temp.write("hello.txt", "hi");
    temp.write("sub/n.txt", "n");

    let mut app = boot_fs(&temp);
    press(&mut app, Key::Down);
    press(&mut app, Key::Right);
    press(&mut app, Key::Down);
    press(&mut app, Key::Down);
    press(&mut app, Key::Enter);
    press(&mut app, Key::Tab);
    type_char(&mut app, 'x');
    press(&mut app, Key::Escape);
    press(&mut app, Key::Left);
    press(&mut app, Key::Left);

    assert_eq!(stored_text(&app, &temp.path.join("hello.txt")), "hix");
    let root = fs_root(app.world());
    assert!(!app.world().get::<Disclosure>(root).unwrap().open);
}

#[test]
fn nodespec_tree_ignores_new_and_delete() {
    let mut app = App::new();
    app.add_plugin(Shell);
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
    press(&mut app, Key::New);
    press(&mut app, Key::Delete);
    press(&mut app, Key::Right);
    let alpha = node_named(app.world(), "Alpha");
    assert!(app.world().get::<Disclosure>(alpha).unwrap().open);
    assert!(!app.world().has::<FsPath>(alpha));
    assert_eq!(
        child_names(app.world(), alpha),
        vec!["Alpha / 1".to_string()]
    );
}
