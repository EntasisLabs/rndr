use super::*;
use rndr_core::{set_parent, Color, Component, Layout, Schedule, Size};

#[test]
fn markdown_embedded_controls_use_unchanged_whole_frame_privacy_gate() {
    use rndr_core::{
        markdown_view, mount_markdown_slot, Button, Label, MarkdownPlugin, MarkdownSlotId,
        MarkdownView,
    };
    let mut app = App::new();
    app.add_plugin(MarkdownPlugin);
    app.world_mut().insert_resource(Viewport::new(320., 240.));
    app.world_mut()
        .insert_resource(observe::ObservationContent::Public);
    let view = markdown_view(app.world_mut(), "Public fallback");
    app.update();
    let block = app.world().get::<MarkdownView>(view).unwrap().blocks()[0].clone();
    let slot =
        mount_markdown_slot(app.world_mut(), view, 0, &block, MarkdownSlotId(1), 40.).unwrap();
    let button = app.world_mut().spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(32.),
            ..Layout::column()
        },
        Label::new("Private control value"),
        Button::new("private_command"),
        Key(91),
    ));
    set_parent(app.world_mut(), button, slot.root);
    app.update();
    let m = metadata(app.world(), Target::Entity(view), &ids).unwrap();
    assert!(m
        .nodes
        .iter()
        .any(|node| node.entity == button && node.ui_id == Some(91)));
    assert!(!m.to_json().to_string().contains("Private control value"));
    assert!(!m.to_json().to_string().contains("private_command"));
    // All hierarchy positions, hidden nodes and unrelated targets deny before
    // identity lookup. No masking/partial subtree metadata is released.
    for entity in [slot.root, button] {
        app.world_mut()
            .insert(entity, observe::ObservationSensitive);
        for hidden in [false, true] {
            if hidden {
                app.world_mut().insert(entity, Hidden);
            }
            for target in [view, button] {
                assert_eq!(
                    metadata(app.world(), Target::Entity(target), &|_, _| panic!(
                        "identity must not run"
                    )),
                    Err(Error::Sensitive)
                );
            }
        }
        app.world_mut()
            .remove::<observe::ObservationSensitive>(entity);
        app.world_mut().remove::<Hidden>(entity);
    }
    app.world_mut()
        .insert_resource(observe::ObservationContent::Deny);
    assert_eq!(
        metadata(app.world(), Target::Entity(button), &ids),
        Err(Error::Sensitive)
    );
    assert!(!permitted(app.world()).is_ok());
}

#[derive(Clone, Copy)]
struct Key(u64);
impl Component for Key {}
fn ids(w: &World, e: Entity) -> Option<u64> {
    w.get::<Key>(e).map(|k| k.0)
}
fn fixture() -> (App, Entity, Entity) {
    let mut app = App::new();
    app.world_mut()
        .insert_resource(observe::ObservationContent::Public);
    app.world_mut().insert_resource(Viewport::new(64.0, 48.0));
    let root = app.world_mut().spawn((
        Layout {
            width: Size::Px(40.0),
            height: Size::Px(32.0),
            padding: 4.0,
            gap: 0.0,
            ..Layout::column()
        },
        Color::rgb(1.0, 0.0, 0.0),
        Key(7),
    ));
    let child = app.world_mut().spawn((
        Layout {
            width: Size::Px(16.0),
            height: Size::Px(12.0),
            padding: 0.0,
            ..Layout::column()
        },
        Color::rgb(0.0, 1.0, 0.0),
        Key(8),
    ));
    set_parent(app.world_mut(), child, root);
    (app, root, child)
}
fn key(id: u64, parent: Option<Entity>) -> Target {
    Target::UiId {
        id,
        parent,
        expected: None,
    }
}

#[test]
fn resolves_scopes_duplicates_and_generation_pins() {
    let (mut app, root, child) = fixture();
    app.update();
    assert_eq!(
        metadata(app.world(), key(8, Some(root)), &ids)
            .unwrap()
            .target,
        child
    );
    assert_eq!(
        metadata(app.world(), key(999, None), &ids),
        Err(Error::NotFound)
    );
    let other = app.world_mut().spawn((Layout::column(), Key(8)));
    app.update();
    assert_eq!(
        metadata(app.world(), key(8, None), &ids),
        Err(Error::Ambiguous)
    );
    assert_eq!(
        metadata(app.world(), key(8, Some(root)), &ids)
            .unwrap()
            .target,
        child
    );
    assert_eq!(
        metadata(
            app.world(),
            Target::UiId {
                id: 8,
                parent: Some(root),
                expected: Some(other)
            },
            &ids
        ),
        Err(Error::Stale)
    );
    rndr_core::despawn(app.world_mut(), child);
    let _replacement = app.world_mut().spawn((Layout::column(), Key(8)));
    assert_eq!(
        metadata(app.world(), Target::Entity(child), &ids),
        Err(Error::Stale)
    );
    assert_eq!(
        metadata(app.world(), key(8, Some(child)), &ids),
        Err(Error::Stale)
    );
}
#[test]
fn geometry_hidden_empty_outside_invalid_and_fractional_scale() {
    let (mut app, root, child) = fixture();
    app.update();
    app.world_mut().insert(
        child,
        Rect {
            x: -1.0,
            y: 2.25,
            w: 4.0,
            h: 3.0,
        },
    );
    app.world_mut().insert(
        child,
        Clip {
            x: -1.0,
            y: 2.25,
            w: 4.0,
            h: 3.0,
        },
    );
    app.world_mut().resource_mut::<Viewport>().unwrap().scale = 1.5;
    assert_eq!(
        metadata(app.world(), Target::Entity(child), &ids)
            .unwrap()
            .pixel_rect,
        [0, 3, 5, 5]
    );
    app.world_mut().insert(root, Hidden);
    assert_eq!(
        metadata(app.world(), Target::Entity(child), &ids),
        Err(Error::Empty)
    );
    app.world_mut().remove::<Hidden>(root);
    for rect in [
        Rect {
            x: 100.0,
            y: 0.0,
            w: 4.0,
            h: 4.0,
        },
        Rect {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 4.0,
        },
    ] {
        app.world_mut().insert(child, rect);
        app.world_mut().remove::<Clip>(child);
        assert_eq!(
            metadata(app.world(), Target::Entity(child), &ids),
            Err(Error::Empty)
        );
    }
    for rect in [
        Rect {
            x: f32::NAN,
            y: 0.0,
            w: 4.0,
            h: 4.0,
        },
        Rect {
            x: 0.0,
            y: 0.0,
            w: -1.0,
            h: 4.0,
        },
    ] {
        app.world_mut().insert(child, rect);
        assert_eq!(
            metadata(app.world(), Target::Entity(child), &ids),
            Err(Error::Geometry)
        );
    }
    app.world_mut().resource_mut::<Viewport>().unwrap().scale = f32::INFINITY;
    assert_eq!(
        metadata(app.world(), Target::Entity(root), &ids),
        Err(Error::Geometry)
    );
}
#[test]
fn privacy_denies_before_identity_or_geometry_and_foreign_surfaces() {
    let (mut app, root, _) = fixture();
    app.update();
    app.world_mut()
        .insert_resource(observe::ObservationContent::Deny);
    assert_eq!(
        metadata(app.world(), key(999, None), &ids),
        Err(Error::Sensitive)
    );
    app.world_mut()
        .insert_resource(observe::ObservationContent::Public);
    let secret = app
        .world_mut()
        .spawn((observe::ObservationSensitive, Hidden));
    assert_eq!(
        metadata(app.world(), Target::Entity(root), &ids),
        Err(Error::Sensitive)
    );
    rndr_core::despawn(app.world_mut(), secret);
    app.world_mut().insert(root, SurfaceSlot { id: 1 });
    assert_eq!(
        metadata(app.world(), Target::Entity(root), &ids),
        Err(Error::ForeignSurface)
    );
}
#[test]
fn geometry_metadata_never_serializes_labels_or_access_values() {
    let (mut app, root, _) = fixture();
    app.world_mut()
        .insert(root, rndr_core::Label::new("DO_NOT_EXPORT_TEXT"));
    app.update();
    let m = metadata(app.world(), Target::Entity(root), &ids).unwrap();
    let json = m.to_json().to_string();
    assert!(!json.contains("DO_NOT_EXPORT_TEXT"));
    assert!(json.contains("\"ui_id\":\"7\""));
    assert_eq!(m.nodes.len(), 2);
}
#[test]
fn caps_entire_offscreen_viewport_even_for_tiny_targets() {
    let (mut app, root, _) = fixture();
    app.update();
    app.world_mut().resource_mut::<Viewport>().unwrap().width = 5000.0;
    assert_eq!(
        metadata(app.world(), Target::Entity(root), &ids),
        Err(Error::Limit)
    );
}

#[test]
fn hierarchy_cycles_and_node_limits_are_explicit_errors() {
    let (mut app, root, child) = fixture();
    app.update();
    app.world_mut().insert(root, Parent { entity: child });
    assert_eq!(
        metadata(app.world(), Target::Entity(child), &ids),
        Err(Error::Limit)
    );
    let (mut app, root, _) = fixture();
    app.update();
    for _ in 0..MAX_NODES {
        let e = app.world_mut().spawn((Layout::column(),));
        set_parent(app.world_mut(), e, root);
    }
    assert_eq!(
        metadata(app.world(), Target::Entity(root), &ids),
        Err(Error::Limit)
    );
}

#[test]
fn offscreen_composite_keeps_overlays_and_clips_to_viewport() {
    let (mut app, root, child) = fixture();
    let overlay = app.world_mut().spawn((
        Layout {
            width: Size::Px(64.0),
            height: Size::Px(48.0),
            padding: 0.0,
            ..Layout::column()
        },
        rndr_core::Layer(100),
        Color::rgb(0.0, 0.0, 1.0),
    ));
    let mut gpu = Exporter::new().expect("real headless adapter required");
    let c = capture(&mut app, &mut gpu, Target::Entity(child), ids).unwrap();
    assert_eq!(c.metadata.target, child);
    assert!(c.metadata.nodes.iter().all(|n| n.entity != overlay));
    assert!(c
        .image
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .all(|p| p[0] < 3 && p[1] < 3 && p[2] > 250 && p[3] == 255));
    rndr_core::despawn(app.world_mut(), overlay);
    app.world_mut().resource_mut::<Viewport>().unwrap().width = 12.0;
    let c = capture(&mut app, &mut gpu, Target::Entity(child), ids).unwrap();
    assert_eq!(c.image.width, 8);
    assert_eq!(c.metadata.pixel_rect[0], 4);
    assert_eq!(c.metadata.nodes[0].bounds.unwrap().w, 16.0); // Layout unchanged, visible pixels clipped.
    assert!(c
        .image
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .all(|p| p[1] > 250 && p[0] < 3));
    let mut layout = *app.world().get::<Layout>(root).unwrap();
    layout.padding = 100.0;
    app.world_mut().insert(root, layout);
    assert!(matches!(
        capture(&mut app, &mut gpu, Target::Entity(child), ids),
        Err(Error::Empty)
    ));
}

#[test]
fn retained_images_render_but_missing_or_native_gpu_only_images_do_not_fake_success() {
    let (mut app, _, child) = fixture();
    app.world_mut().remove::<Color>(child); // Images paint before this entity's solid quad.
    app.world_mut().insert(child, rndr_core::ImageView::new(99));
    let mut gpu = Exporter::new().expect("real headless adapter required");
    assert!(matches!(
        capture(&mut app, &mut gpu, Target::Entity(child), ids),
        Err(Error::ImageUnavailable)
    ));
    crate::load_rgba(app.world_mut(), 99, 1, 1, vec![0, 0, 255, 255]).unwrap();
    let c = capture(&mut app, &mut gpu, Target::Entity(child), ids).unwrap();
    assert!(c
        .image
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .all(|p| p[2] > 250 && p[0] < 3 && p[1] < 3 && p[3] == 255));
    crate::images::release_cpu(app.world_mut(), &[99]);
    assert!(matches!(
        capture(&mut app, &mut gpu, Target::Entity(child), ids),
        Err(Error::ImageUnavailable)
    ));
}

#[test]
fn post_paint_engine_replacement_and_epoch_mutations_are_rejected() {
    let mut gpu = Exporter::new().expect("real headless adapter required");
    for mutation in 0..3 {
        let (mut app, root, _) = fixture();
        app.world_mut().insert_resource(mutation);
        app.add_system(Schedule::Render, |w| match *w.resource::<i32>().unwrap() {
            0 => w.insert_resource(std::sync::Arc::new(rndr_core::BuiltinText::new())
                as std::sync::Arc<dyn rndr_core::TextEngine>),
            1 => rndr_core::text_engine(w).set_fallbacks("bitmap", &["different".into()]),
            _ => w.insert_resource(rndr_core::TextFace(std::sync::Arc::new(
                rndr_core::BitmapFont,
            ))),
        });
        assert!(matches!(
            capture(&mut app, &mut gpu, Target::Entity(root), ids),
            Err(Error::Changed)
        ));
    }
}

#[test]
fn native_semantic_scroll_theme_and_scale_guards_also_apply_headlessly() {
    let mut gpu = Exporter::new().expect("real headless adapter required");
    for mutation in 0..4 {
        let (mut app, root, _) = fixture();
        app.world_mut()
            .insert(root, rndr_core::Button::new("Audited button"));
        app.world_mut()
            .insert(root, rndr_core::Semantics::default());
        app.world_mut().insert(root, rndr_core::Scroll::default());
        app.world_mut().insert_resource(mutation);
        app.add_system(Schedule::Render, |w| {
            let root = w.entities_with::<rndr_core::Button>()[0];
            match *w.resource::<i32>().unwrap() {
                0 => {
                    w.get_mut::<rndr_core::Semantics>(root).unwrap().description = "new help".into()
                }
                1 => w.get_mut::<rndr_core::Scroll>(root).unwrap().offset[1] = -1.0,
                2 => w.insert_resource(observe::ObservationTheme("changed".into())),
                _ => w.resource_mut::<Viewport>().unwrap().scale = 2.0,
            }
        });
        assert!(matches!(
            capture(&mut app, &mut gpu, Target::Entity(root), ids),
            Err(Error::Changed)
        ));
    }
}

#[test]
fn update_can_revoke_capture_before_submission() {
    let (mut app, root, _) = fixture();
    let mut gpu = Exporter::new().expect("real headless adapter required");
    app.add_system(Schedule::Update, |w| {
        w.insert_resource(observe::ObservationContent::Deny);
    });
    assert!(matches!(
        capture(&mut app, &mut gpu, Target::Entity(root), ids),
        Err(Error::Sensitive)
    ));
}

// These must execute the real adapter/readback, not skip on NoAdapter. Machines
// without a graphics backend should run only the deterministic tests above and
// explicitly report that the GPU track could not run.
#[test]
fn actual_headless_pixels_same_frame_layout_identity_and_scale() {
    let (mut app, root, child) = fixture();
    let mut gpu = Exporter::new().expect("real headless adapter required");
    for scale in [1.0, 1.5, 2.0] {
        app.world_mut().resource_mut::<Viewport>().unwrap().scale = scale;
        let c = capture(&mut app, &mut gpu, key(8, Some(root)), ids).unwrap();
        assert_eq!(c.metadata.target, child);
        assert_eq!(c.metadata.nodes[0].ui_id, Some(8));
        assert_eq!(c.metadata.nodes[0].parent, Some(root));
        let rect = c.metadata.nodes[0].bounds.unwrap();
        assert_eq!(
            [c.image.width, c.image.height],
            [(rect.w * scale) as u32, (rect.h * scale) as u32]
        );
        assert!(c
            .image
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| p[0] < 3 && p[1] > 250 && p[2] < 3 && p[3] == 255));
        assert!(crate::encode_png(&c.image)
            .unwrap()
            .starts_with(b"\x89PNG\r\n\x1a\n"));
    }
    app.add_system(Schedule::Update, |w| {
        for (e, _) in w
            .query::<Key>()
            .into_iter()
            .map(|(e, k)| (e, k.0))
            .collect::<Vec<_>>()
        {
            if ids(w, e) == Some(8) {
                w.insert(e, Key(9));
                w.insert(e, Color::rgb(0.0, 0.0, 1.0));
            }
        }
    });
    let c = capture(&mut app, &mut gpu, key(9, Some(root)), ids).unwrap();
    assert_eq!(c.metadata.nodes[0].ui_id, Some(9));
    assert!(c
        .image
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .all(|p| p[2] > 250 && p[1] < 3));
}
#[test]
fn render_mutation_and_new_sensitive_markers_drop_all_output() {
    let (mut app, root, _) = fixture();
    let mut gpu = Exporter::new().expect("real headless adapter required");
    app.add_system(Schedule::Render, |w| {
        let e = w.entities_with::<Key>()[0];
        w.insert(e, Key(100));
    });
    assert!(matches!(
        capture(&mut app, &mut gpu, Target::Entity(root), ids),
        Err(Error::Changed)
    ));
    let (mut app, root, _) = fixture();
    app.add_system(Schedule::Render, |w| {
        w.spawn((observe::ObservationSensitive, Hidden));
    });
    assert!(matches!(
        capture(&mut app, &mut gpu, Target::Entity(root), ids),
        Err(Error::Sensitive)
    ));
}

#[test]
fn independent_reused_exporter_observes_replaced_and_cross_world_images() {
    let mut gpu = Exporter::new().expect("real adapter");
    for blue in [false, true, false] {
        let (mut app, _, child) = fixture();
        app.world_mut().remove::<Color>(child);
        app.world_mut().insert(child, rndr_core::ImageView::new(99));
        for channel in [if blue { 2 } else { 1 }, 0] {
            let mut pixel = vec![0, 0, 0, 255];
            pixel[channel] = 255;
            crate::load_rgba(app.world_mut(), 99, 1, 1, pixel).unwrap();
            let c = capture(&mut app, &mut gpu, Target::Entity(child), ids).unwrap();
            assert!(
                c.image
                    .rgba
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .all(|p| p[channel] > 250 && p[(channel + 1) % 3] < 3),
                "stale image for channel {channel}"
            );
        }
    }
}

#[test]
fn independent_image_budget_failure_is_not_blank_success() {
    let (mut app, root, _) = fixture();
    for id in 100..105 {
        let image = app.world_mut().spawn((
            Layout {
                width: Size::Px(2.0),
                height: Size::Px(2.0),
                ..Layout::column()
            },
            rndr_core::ImageView::new(id),
        ));
        set_parent(app.world_mut(), image, root);
        crate::load_rgba(app.world_mut(), id, 128, 128, vec![255; 128 * 128 * 4]).unwrap();
    }
    // Minimum budget is 64 KiB. Five downsampled mip chains cannot coexist.
    crate::set_image_budget(app.world_mut(), 64 * 1024);
    let mut gpu = Exporter::new().expect("real adapter");
    assert!(matches!(
        capture(&mut app, &mut gpu, Target::Entity(root), ids),
        Err(Error::ImageUnavailable)
    ));
}

#[test]
fn independent_privacy_checkpoints_precede_identity_and_never_leak_text() {
    let mut gpu = Exporter::new().expect("real adapter");
    for mode in 0..7 {
        let (mut app, root, _) = fixture();
        app.world_mut().insert_resource(0u32);
        app.add_system(Schedule::Update, |w| *w.resource_mut::<u32>().unwrap() += 1);
        match mode {
            0 => {
                app.world_mut()
                    .insert_resource(observe::ObservationContent::Deny);
            }
            1 => {
                app.world_mut()
                    .spawn((observe::ObservationSensitive, Hidden));
            }
            2 => {
                app.add_system(Schedule::Update, |w| {
                    w.spawn((observe::ObservationSensitive, Hidden));
                });
            }
            3 => {
                app.add_system(Schedule::Render, |w| {
                    w.insert_resource(observe::ObservationContent::Deny)
                });
            }
            4 => {
                app.add_system(Schedule::Render, |w| {
                    w.spawn((observe::ObservationSensitive, Hidden));
                });
            }
            5 => {
                app.add_system(Schedule::Layout, |w| {
                    w.insert_resource(observe::ObservationContent::Deny);
                });
            }
            _ => {
                app.add_system(Schedule::Layout, |w| {
                    w.spawn((observe::ObservationSensitive, Hidden));
                });
            }
        }
        let result = capture(&mut app, &mut gpu, Target::Entity(root), ids);
        assert!(matches!(result, Err(Error::Sensitive)));
        assert_eq!(
            *app.world().resource::<u32>().unwrap(),
            if mode < 2 { 0 } else { 1 }
        );
    }
}

#[test]
fn independent_same_parent_duplicates_and_live_remount_pin_are_rejected() {
    let (mut app, root, child) = fixture();
    let other = app.world_mut().spawn((Layout::column(), Key(8)));
    set_parent(app.world_mut(), other, root);
    app.update();
    assert_eq!(
        metadata(app.world(), key(8, Some(root)), &ids),
        Err(Error::Ambiguous)
    );
    app.world_mut().insert(child, Key(88)); // old handle still alive, logical key remounted
    assert_eq!(
        metadata(
            app.world(),
            Target::UiId {
                id: 8,
                parent: Some(root),
                expected: Some(child)
            },
            &ids
        ),
        Err(Error::Stale)
    );
}

#[test]
fn independent_missing_attestation_denies_before_update_or_target_lookup() {
    let mut app = App::new();
    let root = app.world_mut().spawn((Layout::column(),));
    app.world_mut().insert_resource(0u32);
    app.add_system(Schedule::Update, |w| *w.resource_mut::<u32>().unwrap() += 1);
    let mut gpu = Exporter::new().expect("real adapter");
    assert!(matches!(
        capture(&mut app, &mut gpu, Target::Entity(root), ids),
        Err(Error::Sensitive)
    ));
    assert_eq!(*app.world().resource::<u32>().unwrap(), 0);
}

#[test]
fn independent_post_render_geometry_hidden_and_foreign_surface_checks() {
    let mut gpu = Exporter::new().expect("real adapter");
    for mode in 0..3 {
        let (mut app, root, _) = fixture();
        app.world_mut().insert_resource(mode);
        app.add_system(Schedule::Render, |w| {
            let root = w
                .query::<Key>()
                .into_iter()
                .find(|(_, k)| k.0 == 7)
                .unwrap()
                .0;
            match *w.resource::<i32>().unwrap() {
                0 => w.get_mut::<Rect>(root).unwrap().x += 1.0,
                1 => {
                    w.insert(root, Hidden);
                }
                _ => {
                    w.spawn((SurfaceSlot { id: 123 }, Hidden));
                }
            }
        });
        let result = capture(&mut app, &mut gpu, Target::Entity(root), ids);
        assert!(
            matches!(
                (&result, mode),
                (Err(Error::Changed), 0 | 1) | (Err(Error::ForeignSurface), 2)
            ),
            "{result:?}"
        );
    }
}
