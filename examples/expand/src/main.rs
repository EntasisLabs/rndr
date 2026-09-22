use rndr_core::{
    page_layout, set_parent, spawn_node, toggle_disclosures, App, Color, Schedule, World,
};
use rndr_render::run;

fn setup(world: &mut World) {
    let page = world.spawn((page_layout(), Color::rgb(0.10, 0.11, 0.13)));
    for spec in demo_tree() {
        let node = spawn_node(world, spec);
        set_parent(world, node, page);
    }
}

fn demo_tree() -> Vec<rndr_core::NodeSpec> {
    let root = Color::rgb(0.20, 0.36, 0.52);
    let child = Color::rgb(0.22, 0.44, 0.40);
    let leaf = Color::rgb(0.46, 0.34, 0.24);
    vec![
        rndr_core::NodeSpec::branch(
            "Alpha",
            root,
            vec![
                rndr_core::NodeSpec::branch(
                    "Alpha / 1",
                    child,
                    vec![rndr_core::NodeSpec::leaf("Alpha / 1 / a", leaf)],
                ),
                rndr_core::NodeSpec::leaf("Alpha / 2", child),
            ],
        ),
        rndr_core::NodeSpec::branch(
            "Beta",
            root,
            vec![
                rndr_core::NodeSpec::leaf("Beta / 1", child),
                rndr_core::NodeSpec::leaf("Beta / 2", child),
            ],
        ),
        rndr_core::NodeSpec::branch(
            "Gamma",
            root,
            vec![
                rndr_core::NodeSpec::leaf("Gamma / 1", child),
                rndr_core::NodeSpec::leaf("Gamma / 2", child),
            ],
        ),
    ]
}

fn main() {
    let mut app = App::new();
    app.add_system(Schedule::Startup, setup)
        .add_system(Schedule::Update, toggle_disclosures);
    run(app);
}
