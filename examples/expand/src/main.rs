use std::path::PathBuf;
use std::sync::Arc;

use rndr_core::{
    detail_pane, set_parent, shell, spawn_fs_node, splitter, theme_of, tree_column, App, Radius,
    Schedule, Shell, TextFace, Theme, World,
};
use rndr_paint::LatinFace;
use rndr_render::run;
use rndr_std::StdFsPlugin;

fn setup(world: &mut World) {
    let theme = theme_of(world);
    let root = world.spawn((shell(), theme.canvas));

    let (tree_layout, tree_scroll) = tree_column();
    let tree = world.spawn((tree_layout, tree_scroll, theme.tree, Radius(theme.radius)));
    set_parent(world, tree, root);

    let node = spawn_fs_node(world, demo_root());
    set_parent(world, node, tree);

    let (split_layout, split) = splitter();
    let seam = world.spawn((split_layout, split, theme.seam));
    set_parent(world, seam, root);

    let (pane_layout, pane_scroll, pane) = detail_pane();
    let detail = world.spawn((
        pane_layout,
        pane_scroll,
        pane,
        theme.pane,
        Radius(theme.radius),
    ));
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
    app.world_mut()
        .insert_resource(TextFace(Arc::new(LatinFace)));
    app.world_mut().insert_resource(Theme::default());
    app.add_system(Schedule::Startup, setup);
    run(app);
}
