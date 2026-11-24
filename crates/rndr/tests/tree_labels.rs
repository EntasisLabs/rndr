//! The Notes tree uses 34-point rows with compact, ellipsized labels.
#![cfg(feature = "parley")]
use rndr_core::*;
use rndr_ui::tree::*;
use rndr_ui::{UiId, UiPlugin};
use std::sync::Arc;

#[test]
fn tree_labels_contain_descenders() {
    for scale in [1., 2.] {
        let mut app = App::new();
        app.add_plugin(UiPlugin);
        let world = app.world_mut();
        let mut viewport = Viewport::new(280., 140.);
        viewport.scale = scale;
        world.insert_resource(viewport);
        let engine = Arc::new(rndr_render::ParleyText::bundled()) as Arc<dyn TextEngine>;
        engine.set_pixels_per_point(scale);
        world.insert_resource(engine);
        let layout = Layout {
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::column()
        };
        let root = world.spawn((layout,));
        let tree = VirtualTree::mount(
            world,
            root,
            UiId(1),
            TreeProps::new("Notes library").row_height(34.),
            &[
                TreeNode::new(1, "groups").expandable(true),
                TreeNode::new(2, "guides")
                    .child(NodeId(1))
                    .secondary("groups/guides")
                    .badge("jpg"),
            ],
            &[NodeId(1)],
        )
        .unwrap();
        world.insert(tree.root, layout);
        rndr_paint::frame(&mut app);
        let frame = rndr_paint::frame(&mut app);
        for text in ["groups", "guides", "groups/guides", "jpg"] {
            let w = app.world();
            let e = w
                .entities_with::<Label>()
                .into_iter()
                .find(|e| w.get::<Label>(*e).unwrap().text == text)
                .unwrap();
            let rect = w.get::<Rect>(e).unwrap();
            let clip = w.get::<Clip>(e).unwrap();
            let shaped = w.get::<TextLayout>(e).unwrap();
            assert!(!shaped.glyphs.is_empty());
            for g in &shaped.glyphs {
                let y = rect.y + label_box(w, e).inset_y + g.y;
                assert!(
                    y >= clip.y && y + g.h <= clip.y + clip.h,
                    "{text} at {scale}x: {y}..{} vs {clip:?}, rect {rect:?}",
                    y + g.h
                );
            }
        }
        if let Some(dir) = std::env::var_os("RNDR_TREE_CAPTURE_DIR") {
            let img = rndr_render::Exporter::new()
                .unwrap()
                .render_frame(
                    Some(app.world()),
                    &frame,
                    (280. * scale) as u32,
                    (140. * scale) as u32,
                    scale,
                )
                .unwrap();
            std::fs::write(
                std::path::Path::new(&dir).join(format!("tree-{scale}.png")),
                rndr_render::encode_png(&img).unwrap(),
            )
            .unwrap();
        }
    }
}
