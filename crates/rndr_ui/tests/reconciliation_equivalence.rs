//! Optional reproducible headless paint/access equivalence capture. Not native
//! presentation, font-raster, OS accessibility or physical-input certification.
#[path = "../examples/support/data_editors_gallery.rs"]
mod data_gallery;
#[path = "../examples/support/selection_gallery.rs"]
mod selection_gallery;
#[path = "../examples/support/virtual_gallery.rs"]
mod virtual_gallery;
use rndr_core::App;
use rndr_ui::{Appearance, Contrast, Density, UiTheme};

fn capture(app: &mut App, name: &str) {
    rndr_paint::frame(app);
    let frame = rndr_paint::frame(app);
    assert!(!frame.quads.is_empty(), "{name}");
    if let Ok(directory) = std::env::var("RNDR_RECONCILIATION_CAPTURE_DIR") {
        std::fs::create_dir_all(&directory).unwrap();
        let snapshot = format!(
            "{}\n{:#?}",
            rndr_paint::quads_snapshot(&frame),
            canonical_access(app.world())
        );
        std::fs::write(
            std::path::Path::new(&directory).join(format!("{name}.txt")),
            snapshot,
        )
        .unwrap();
    }
}
// ECS component-store iteration can enumerate a detached popup's root before
// or after the main document even on two runs of unchanged HEAD. Normalize only
// that root-group enumeration and node storage order, never sibling order inside
// a retained hierarchy. All IDs, links, semantics, focus and geometry remain.
fn canonical_access(world: &rndr_core::World) -> rndr_core::AccessTree {
    let mut tree = rndr_core::access_tree(world);
    let root_id = tree.root;
    if let Some(window) = tree.nodes.iter_mut().find(|node| node.id == root_id) {
        window.children.sort_by_key(|id| {
            let mut entity = rndr_core::entity_from_access_id(*id).unwrap();
            for _ in 0..64 {
                match world.get::<rndr_core::Parent>(entity) {
                    Some(parent) if world.is_alive(parent.entity) => entity = parent.entity,
                    _ => break,
                }
            }
            rndr_core::access_id(entity)
        });
    }
    tree.nodes.sort_by_key(|node| node.id);
    tree
}
#[test]
fn retained_gallery_paint_and_access_matrix() {
    for (name, theme, width) in [
        ("light", UiTheme::default(), 1180.),
        (
            "narrow-dark",
            UiTheme {
                appearance: Appearance::Dark,
                ..Default::default()
            },
            380.,
        ),
        (
            "compact-hc-200",
            UiTheme {
                contrast: Contrast::High,
                density: Density::Compact,
                text_scale: 2.,
                reading_scale: 2.,
                ..Default::default()
            },
            1180.,
        ),
    ] {
        for preset in [
            "",
            "--single-open",
            "--multi-open",
            "--inspector-open",
            "--empty",
            "--readonly",
            "--disabled",
            "--invalid",
        ] {
            let mut app = selection_gallery::create(width, 960., theme.clone(), &[preset.into()]);
            if preset.ends_with("-open") {
                for _ in 0..2 {
                    rndr_paint::frame(&mut app);
                }
                let gallery = app
                    .world()
                    .resource::<selection_gallery::Gallery>()
                    .unwrap();
                let target = match preset {
                    "--single-open" => gallery.single.unwrap().root,
                    "--multi-open" => gallery.multi.unwrap().root,
                    _ => {
                        gallery
                            .inspector
                            .unwrap()
                            .slot(app.world(), rndr_ui::data::PropertyId(3))
                            .unwrap()
                            .editor
                    }
                };
                rndr_core::request_focus(app.world_mut(), target);
                rndr_core::reveal_entity(app.world_mut(), target);
                rndr_paint::frame(&mut app);
                assert_eq!(
                    rndr_core::try_activate(app.world_mut(), target),
                    rndr_core::ActivationResult::Accepted
                );
                rndr_paint::frame(&mut app);
            }
            capture(
                &mut app,
                &format!("selection-{name}-{}", preset.trim_start_matches('-')),
            );
        }
        let mut app = data_gallery::create(width, 960., theme.clone());
        capture(&mut app, &format!("data-{name}"));
        let mut app = virtual_gallery::create(100_000, width, 760., theme);
        capture(&mut app, &format!("virtual-{name}"));
    }
}
