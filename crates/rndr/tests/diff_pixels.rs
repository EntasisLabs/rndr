//! DiffView reference fixture. Pixels prove only this offscreen native-shaper path.
#![cfg(feature = "parley")]
use rndr_core::*;
use rndr_render::{encode_png, Exporter};
use rndr_ui::{
    collections::{ItemId, VirtualViewHandle},
    diff::{DiffAnnotation, DiffMarker, DiffProps, DiffRow, DiffView, HunkId},
    Appearance, UiId, UiPlugin, UiRecipe, UiStyle, UiTheme,
};
use std::sync::Arc;

fn fixture(width: f32, scale: f32, light: bool) -> (App, rndr_ui::diff::DiffHandle) {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    let mut viewport = Viewport::new(width, 520.);
    viewport.scale = scale;
    app.world_mut().insert_resource(viewport);
    let engine: Arc<dyn TextEngine> = Arc::new(rndr_render::ParleyText::bundled());
    app.world_mut().insert_resource(engine);
    let mut theme = UiTheme::default();
    theme.appearance = if light {
        Appearance::Light
    } else {
        Appearance::Dark
    };
    let background = theme.color(rndr_ui::Token::Content);
    app.world_mut().insert_resource(theme);
    let parent = app.world_mut().spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::column()
        },
        background,
    ));
    let heading = app.world_mut().spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(42.),
            ..Layout::column()
        },
        Label::new("context_usage.rs   src/agent_runtime                      +624  −152"),
        UiRecipe(UiStyle {
            foreground: Some(rndr_ui::Token::Text.into()),
            ..UiStyle::default()
        }),
        LabelBox {
            inset_x: 16.,
            inset_y: 10.,
            min_height: 0.,
        },
        TextStyle {
            family: "Inter".into(),
            size: 13.,
            ..TextStyle::default()
        },
    ));
    set_parent(app.world_mut(), heading, parent);
    let source = [
        (DiffMarker::Header, "9 unmodified lines", None, None),
        (DiffMarker::Unchanged, "pub const ESTIMATOR_LABEL: &str = \"chars_div_4\";", Some(10), Some(10)),
        (DiffMarker::Unchanged, "", Some(11), Some(11)),
        (DiffMarker::Unchanged, "pub fn chars_to_tokens(chars: usize) -> u32 {", Some(12), Some(12)),
        (DiffMarker::Removed, "    u32::try_from(chars.div_ceil(4)).unwrap_or(u32::MAX)", Some(13), None),
        (DiffMarker::Added, "    medousa_runtime::context::ContextCost::text(chars).tokens_estimate()", None, Some(13)),
        (DiffMarker::Unchanged, "}", Some(14), Some(14)),
        (DiffMarker::Unchanged, "", Some(15), Some(15)),
        (DiffMarker::Unchanged, "fn layer(name: &str, tokens: u32) -> ContextUsageLayer {", Some(16), Some(16)),
        (DiffMarker::Header, "7 unmodified lines", None, None),
        (DiffMarker::Unchanged, "pub struct ContextUsageInput<'a> {", Some(26), Some(26)),
        (DiffMarker::Added, "    pub session_id: &'a str,", None, Some(27)),
        (DiffMarker::Unchanged, "    pub system_prompt_chars: usize,", Some(27), Some(28)),
        (DiffMarker::Removed, "    pub user_prompt_chars: usize,", Some(28), None),
        (DiffMarker::Removed, "    pub resolved_prompt_chars: usize,", Some(29), None),
        (DiffMarker::Removed, "    pub prompt_request_chars: usize,", Some(30), None),
        (DiffMarker::Added, "    pub prompt_ledger: &'a super::prompt_ledger::PromptLedger,", None, Some(29)),
        (DiffMarker::Added, "    pub resolved_prompt: &'a str,", None, Some(30)),
        (DiffMarker::Added, "    pub current_user_message: &'a agent_runtime::messages::UserMessage,", None, Some(31)),
        (DiffMarker::Added, "    // Initial refresh snapshots the protected lane; preserve the complete line through horizontal scrolling; this additional source text verifies that the entire long-line tail is reachable. END", None, Some(32)),
        (DiffMarker::Unchanged, "    pub sticky_chars: usize,", Some(32), Some(33)),
        (DiffMarker::Unchanged, "}", Some(33), Some(34)),
        (DiffMarker::Header, "End of change", None, None),
    ];
    let rows: Vec<_> = source
        .iter()
        .enumerate()
        .map(|(i, (marker, text, _, _))| DiffRow {
            id: ItemId(i as u64 + 1),
            hunk: HunkId(if i < 9 { 1 } else { 2 }),
            text: (*text).into(),
            marker: *marker,
            height: 20.,
        })
        .collect();
    let annotations: Vec<_> = source
        .iter()
        .enumerate()
        .map(|(i, (_, text, old, new))| {
            let mut spans = Vec::new();
            for keyword in ["pub", "const", "fn", "struct"] {
                if let Some(start) = text.find(keyword) {
                    spans.push(TextSpan::new(
                        start,
                        start + keyword.len(),
                        if light {
                            Color::rgb(0.6, 0.1, 0.4)
                        } else {
                            Color::rgb(0.95, 0.4, 0.75)
                        },
                    ));
                }
            }
            for ty in ["usize", "u32", "&str"] {
                if let Some(start) = text.find(ty) {
                    spans.push(TextSpan::new(
                        start,
                        start + ty.len(),
                        if light {
                            Color::rgb(0.05, 0.4, 0.5)
                        } else {
                            Color::rgb(0.4, 0.8, 0.9)
                        },
                    ));
                }
            }
            if let Some(start) = text.find("chars_div_4") {
                spans.push(TextSpan::new(
                    start - 1,
                    start + 12,
                    if light {
                        Color::rgb(0.35, 0.4, 0.05)
                    } else {
                        Color::rgb(0.8, 0.85, 0.4)
                    },
                ));
            }
            if text.trim_start().starts_with("//") {
                spans.push(TextSpan::new(0, text.len(), Color::rgb(0.45, 0.58, 0.7)).italic());
            }
            if i == 4 || i == 5 {
                spans.push(
                    TextSpan::new(
                        4,
                        text.len(),
                        if light {
                            Color::rgb(0.15, 0.2, 0.25)
                        } else {
                            Color::rgb(0.8, 0.85, 0.9)
                        },
                    )
                    .highlight(Color::rgba(0.2, 0.5, 0.4, 0.2)),
                );
            }
            DiffAnnotation {
                id: ItemId(i as u64 + 1),
                old_line: *old,
                new_line: *new,
                spans,
            }
        })
        .collect();
    let handle = DiffView::mount_annotated(
        app.world_mut(),
        parent,
        UiId(1),
        DiffProps::new("Changes"),
        &rows,
        &annotations,
    )
    .unwrap();
    (app, handle)
}

#[test]
fn diff_reference_pixels_and_horizontal_tail() {
    let mut gpu = Exporter::new().expect("a real headless graphics adapter");
    for (width, scale, light, name) in [
        (900., 2., false, "wide-dark"),
        (440., 2., false, "narrow-dark"),
        (900., 1., true, "wide-light"),
    ] {
        let (mut app, handle) = fixture(width, scale, light);
        for scrolled in [false, true] {
            if scrolled {
                app.world_mut()
                    .get_mut::<Scroll>(handle.root)
                    .unwrap()
                    .offset[0] = -10000.;
            }
            let frame = rndr_paint::frame(&mut app);
            let scroll = *app.world().get::<Scroll>(handle.root).unwrap();
            if scrolled {
                assert!(scroll.offset[0] < 0.);
            }
            let image = gpu
                .render_frame(
                    Some(app.world()),
                    &frame,
                    (width * scale) as u32,
                    (520. * scale) as u32,
                    scale,
                )
                .unwrap();
            // Full-row fills, including beyond the text and the marker gutter.
            if !scrolled {
                let slot = (VirtualViewHandle { root: handle.root })
                    .row(app.world(), ItemId(6))
                    .unwrap();
                let r = *app.world().get::<Rect>(slot.root).unwrap();
                let x = (width - 16.) * scale;
                let y = (r.y + r.h - 2.) * scale;
                let i = ((y as u32 * image.width + x as u32) * 4) as usize;
                let pixel = &image.rgba[i..i + 3];
                assert!(
                    pixel[1] > pixel[0] && pixel[1] > pixel[2],
                    "full width added fill {pixel:?}"
                );
                let text = *children_of(app.world(), slot.content).last().unwrap();
                let rect = app.world().get::<Rect>(text).unwrap();
                assert!(
                    app.world().get::<TextLayout>(text).unwrap().height <= rect.h + 0.01,
                    "shaped={} row={rect:?} style={:?}",
                    app.world().get::<TextLayout>(text).unwrap().height,
                    app.world().get::<TextStyle>(text)
                );
            } else {
                let slot = (VirtualViewHandle { root: handle.root })
                    .row(app.world(), ItemId(20))
                    .unwrap();
                let text = *children_of(app.world(), slot.content).last().unwrap();
                let rect = app.world().get::<Rect>(text).unwrap();
                let layout = app.world().get::<TextLayout>(text).unwrap();
                assert!(
                    rect.x + layout.width < width,
                    "tail is reachable: {rect:?}, {}",
                    layout.width
                );
            }
            if let Some(dir) = std::env::var_os("RNDR_DIFF_CAPTURE_DIR") {
                let dir = std::path::PathBuf::from(dir);
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(
                    dir.join(format!(
                        "diff-{name}{}.png",
                        if scrolled { "-tail" } else { "" }
                    )),
                    encode_png(&image).unwrap(),
                )
                .unwrap();
            }
        }
    }
}
