use rndr_core::{
    detail_pane, set_parent, shell, spawn_node, tree_column, App, Body, Color, NodeSpec, Schedule,
    World,
};
use rndr_render::run;

fn setup(world: &mut World) {
    let root = world.spawn((shell(), Color::rgb(0.08, 0.09, 0.10)));

    let (tree_layout, tree_scroll) = tree_column();
    let tree = world.spawn((tree_layout, tree_scroll, Color::rgb(0.10, 0.11, 0.13)));
    set_parent(world, tree, root);
    for spec in demo_tree() {
        let label = spec.label.clone();
        let node = spawn_node(world, spec);
        if label == "Alpha" {
            if let Some(body) = world.get_mut::<Body>(node) {
                body.text = "a node you can write in".to_string();
            }
        }
        if label == "Beta" {
            if let Some(body) = world.get_mut::<Body>(node) {
                body.text = "another document".to_string();
            }
        }
        set_parent(world, node, tree);
    }

    let (pane_layout, pane_scroll, pane) = detail_pane();
    let detail = world.spawn((pane_layout, pane_scroll, pane, Color::rgb(0.12, 0.13, 0.15)));
    set_parent(world, detail, root);
}

fn demo_tree() -> Vec<NodeSpec> {
    let root = Color::rgb(0.20, 0.36, 0.52);
    let child = Color::rgb(0.22, 0.44, 0.40);
    let leaf = Color::rgb(0.46, 0.34, 0.24);
    let mut tree = vec![
        NodeSpec::branch(
            "Alpha",
            root,
            vec![
                NodeSpec::branch(
                    "Alpha / 1",
                    child,
                    vec![NodeSpec::leaf("Alpha / 1 / a", leaf)],
                ),
                NodeSpec::leaf("Alpha / 2", child),
            ],
        ),
        NodeSpec::branch(
            "Beta",
            root,
            vec![
                NodeSpec::leaf("Beta / 1", child),
                NodeSpec::leaf("Beta / 2", child),
            ],
        ),
        NodeSpec::branch(
            "Gamma",
            root,
            vec![
                NodeSpec::leaf("Gamma / 1", child),
                NodeSpec::leaf("Gamma / 2", child),
            ],
        ),
    ];
    for name in [
        "Delta", "Epsilon", "Zeta", "Eta", "Theta", "Iota", "Kappa", "Lambda", "Mu", "Nu", "Xi",
        "Omicron",
    ] {
        tree.push(NodeSpec::branch(
            name,
            root,
            vec![
                NodeSpec::leaf(format!("{name} / 1"), child),
                NodeSpec::leaf(format!("{name} / 2"), child),
            ],
        ));
    }
    tree
}

fn main() {
    let mut app = App::new();
    app.add_system(Schedule::Startup, setup);
    run(app);
}
