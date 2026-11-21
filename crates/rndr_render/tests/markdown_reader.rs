//! Read-mode labels must contain the native shaped ink, including marker punctuation.
#![cfg(feature = "parley")]
use rndr_core::*;
use rndr_render::{encode_png, Exporter, ParleyText};
use std::sync::Arc;

#[test]
fn reader_contains_native_code_and_marker_ink() {
    let mut app = App::new();
    app.add_plugin(MarkdownPlugin);
    let engine: Arc<dyn TextEngine> = Arc::new(ParleyText::bundled());
    app.world_mut().insert_resource(engine);
    app.world_mut().insert_resource(Viewport::new(440., 460.));
    let body = "User sends message\n    │\n    └── PREMIUM → fallback chain: V3.2 → Sonnet 4 → 503\n\ngeneral_settings:\n  max_request_size_mb: 12";
    let view = markdown_view(
        app.world_mut(),
        format!("1. Deadwood first\n2. Create a clear trunk line\n\nA separate list:\n\n9. first item\n10. second item\n\n```yaml\n{body}\n```\n\n### Following heading"),
    );
    app.world_mut().insert(
        view,
        TextStyle {
            size: 17.,
            ..Default::default()
        },
    );
    let _ = rndr_paint::frame(&mut app);
    let frame = rndr_paint::frame(&mut app);
    let mut single_digit_origin = None;
    for text in ["1.", "2.", "9.", "10.", body] {
        let world = app.world();
        let entity = world
            .entities_with::<Label>()
            .into_iter()
            .find(|e| world.get::<Label>(*e).unwrap().text == text)
            .unwrap();
        let rect = world.get::<Rect>(entity).unwrap();
        let layout = world.get::<TextLayout>(entity).unwrap();
        let clip = world.get::<Clip>(entity).unwrap();
        if text.ends_with('.') {
            if text.len() == 2 {
                let expected = *single_digit_origin.get_or_insert(rect.x);
                assert_eq!(rect.x, expected, "single-digit marker {text} shifted");
            }
            assert_eq!(layout.lines.len(), 1, "marker {text} must not wrap");
            assert!(layout.glyphs.len() >= text.len());
        }
        for g in &layout.glyphs {
            let right = rect.x + TEXT_ORIGIN_X + g.x + g.w;
            assert!(
                right <= clip.x + clip.w + 0.5,
                "{text}: glyph right {right} exceeds clip {clip:?}"
            );
            assert!(
                TEXT_ORIGIN_X + g.x + g.w <= rect.w + 1.
                    && TEXT_ORIGIN_Y + g.y + g.h <= rect.h + 1.,
                "{text}: glyph at {},{} size {}x{} outside {rect:?}",
                g.x,
                g.y,
                g.w,
                g.h
            );
        }
    }
    if let Some(path) = std::env::var_os("RNDR_READER_CAPTURE") {
        let image = Exporter::new()
            .unwrap()
            .render_frame(Some(app.world()), &frame, 440, 460, 1.)
            .unwrap();
        std::fs::write(path, encode_png(&image).unwrap()).unwrap();
    }
}

#[test]
fn scrolling_rebinds_each_item_to_exactly_its_own_marker() {
    let mut app = App::new();
    app.add_plugin(MarkdownPlugin);
    app.world_mut()
        .insert_resource(Arc::new(ParleyText::bundled()) as Arc<dyn TextEngine>);
    app.world_mut().insert_resource(Viewport::new(420., 180.));
    let source = (0..24).map(|i| format!("Paragraph {i}\n\n- Bullet {i}\n\n9. Nine {i}\n10. Ten {i}\n\n- [x] Checked {i}\n- [ ] Unchecked {i}\n\n")).collect::<String>();
    let view = markdown_view(app.world_mut(), source);
    rndr_paint::frame(&mut app);
    let mut offsets = std::collections::HashSet::new();
    for delta in [
        0., 100000., -160., -490., 260., -100000., 100000., -800., 800.,
    ] {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.pointer.position = [100., 80.];
        input.scroll = [0., delta];
        rndr_paint::frame(&mut app);
        let world = app.world();
        offsets.insert(world.get::<Scroll>(view).unwrap().offset[1] as i32);
        let mut items = 0;
        for row in children_of(world, view) {
            let mut nodes = vec![row];
            let mut labels = vec![];
            let mut dots = 0;
            let mut checks = vec![];
            while let Some(node) = nodes.pop() {
                nodes.extend(children_of(world, node));
                if world.has::<Border>(node) {
                    checks.push(!children_of(world, node).is_empty());
                } else if world.has::<Radius>(node) {
                    dots += 1;
                }
                if let Some(label) = world.get::<Label>(node) {
                    labels.push((label.text.as_str(), node));
                }
            }
            let Some((body, _)) = labels.iter().find(|(text, _)| text.contains(' ')) else {
                continue;
            };
            items += 1;
            let kind = body.split_once(' ').unwrap().0;
            assert_eq!(dots, usize::from(kind == "Bullet"), "{body} at {delta}");
            let expected_checks: &[bool] = match kind {
                "Checked" => &[true],
                "Unchecked" => &[false],
                _ => &[],
            };
            assert_eq!(checks, expected_checks, "{body} at {delta}");
            let numbers: Vec<_> = labels.iter().filter(|(t, _)| t.ends_with('.')).collect();
            let expected = match kind {
                "Nine" => Some("9."),
                "Ten" => Some("10."),
                _ => None,
            };
            assert_eq!(
                numbers.iter().map(|(t, _)| *t).collect::<Vec<_>>(),
                expected.into_iter().collect::<Vec<_>>(),
                "{body} at {delta}"
            );
            for (_, entity) in numbers {
                let rect = world.get::<Rect>(*entity).unwrap();
                let clip = world.get::<Clip>(*entity).unwrap();
                if clip.h > 0. && rect.y >= 0. && rect.y + rect.h <= 180. {
                    let layout = world.get::<TextLayout>(*entity).unwrap();
                    assert_eq!(layout.lines.len(), 1);
                    assert!(
                        layout
                            .glyphs
                            .iter()
                            .all(|g| rect.x + TEXT_ORIGIN_X + g.x + g.w <= clip.x + clip.w + 0.5),
                        "clipped {body}"
                    );
                }
            }
        }
        assert!(items > 0);
    }
    assert!(
        offsets.len() >= 4,
        "scroll fixture did not move: {offsets:?}"
    );
}
