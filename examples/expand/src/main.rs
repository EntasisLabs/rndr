use std::path::PathBuf;

use rndr_core::{
    detail_pane, set_parent, shell, spawn_fs_node, tree_column, App, Color, Schedule, World,
};
use rndr_render::run;

fn setup(world: &mut World) {
    let root = world.spawn((shell(), Color::rgb(0.08, 0.09, 0.10)));

    let (tree_layout, tree_scroll) = tree_column();
    let tree = world.spawn((tree_layout, tree_scroll, Color::rgb(0.10, 0.11, 0.13)));
    set_parent(world, tree, root);

    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixture");
    let node = spawn_fs_node(world, fixture);
    set_parent(world, node, tree);

    let (pane_layout, pane_scroll, pane) = detail_pane();
    let detail = world.spawn((pane_layout, pane_scroll, pane, Color::rgb(0.12, 0.13, 0.15)));
    set_parent(world, detail, root);
}

fn main() {
    let mut app = App::new();
    app.add_system(Schedule::Startup, setup);
    run(app);
}
