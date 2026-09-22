use rndr_core::{
    scroll_page, set_parent, spawn_node, toggle_disclosures, App, Color, NodeSpec, Schedule, World,
};
use rndr_render::run;

fn setup(world: &mut World) {
    let (layout, scroll) = scroll_page();
    let page = world.spawn((layout, scroll, Color::rgb(0.10, 0.11, 0.13)));
    for spec in demo_tree() {
        let node = spawn_node(world, spec);
        set_parent(world, node, page);
    }
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
    app.add_system(Schedule::Startup, setup)
        .add_system(Schedule::Update, toggle_disclosures);
    run(app);
}
