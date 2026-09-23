use std::path::PathBuf;

use rndr_core::{
    detail_pane, set_parent, shell, spawn_fs_node, splitter, tree_column, App, Color, Schedule,
    Shell, World,
};
use rndr_render::run;
use rndr_std::StdFsPlugin;

fn setup(world: &mut World) {
    let root = world.spawn((shell(), Color::rgb(0.08, 0.09, 0.10)));

    let (tree_layout, tree_scroll) = tree_column();
    let tree = world.spawn((tree_layout, tree_scroll, Color::rgb(0.10, 0.11, 0.13)));
    set_parent(world, tree, root);

    let node = spawn_fs_node(world, demo_root());
    set_parent(world, node, tree);

    let (split_layout, split) = splitter();
    let seam = world.spawn((split_layout, split, Color::rgb(0.06, 0.07, 0.08)));
    set_parent(world, seam, root);

    let (pane_layout, pane_scroll, pane) = detail_pane();
    let detail = world.spawn((pane_layout, pane_scroll, pane, Color::rgb(0.12, 0.13, 0.15)));
    set_parent(world, detail, root);
}

fn demo_root() -> PathBuf {
    if let Some(arg) = std::env::args().nth(1) {
        let path = PathBuf::from(arg);
        if path.is_dir() {
            return path;
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixture")
}

fn main() {
    let mut app = App::new();
    app.add_plugin(StdFsPlugin);
    app.add_plugin(Shell);
    app.add_system(Schedule::Startup, setup);
    run(app);
}
