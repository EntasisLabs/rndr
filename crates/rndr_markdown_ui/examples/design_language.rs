//! Actual editor surface, synthetic public content, offscreen pixels (not native input proof).
use rndr_core::*;
use rndr_markdown_ui::*;
use rndr_ui::{Appearance, Contrast, Density, Token, UiId, UiTheme};
use std::sync::Arc;

const FIELD_NOTES: &str = "# Field notes\n\nBody **bold**, *italic*, `inline code`, $x+y$, ==highlight== and [link](https://example.com).\n\n{{red|Red}} {{orange|Orange}} {{yellow|Yellow}} {{green|Green}} {{blue|Blue}} {{purple|Purple}} {{pink|Pink}}\n\n- [ ] Unchecked task\n- [x] Completed task\n- [ ] Long task label wraps onto the next line to test first-line checkbox alignment and document geometry in narrow windows.\n\n> [!info] Information\n> A neutral body with **bold** and `code`.\n\n> [!warning] Warning\n> Read the warning before continuing.\n\n> [!danger] Danger\n> Check before removing a file.\n\n> [!success] Success\n> All changes saved.\n\n> [!note] Note\n> Keep a local copy.\n\n```rust\nlet quiet = true;\n```\n\n| Name | Value |\n| --- | --- |\n| Reading | 14/22 |\n| Last row | Visible |\n";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let has = |flag: &str| args.iter().any(|arg| arg == flag);
    let prefix = args
        .iter()
        .position(|arg| arg == "--capture-prefix")
        .and_then(|i| args.get(i + 1))
        .ok_or("--capture-prefix required")?;
    let scale = if has("--200") { 2.0 } else { 1.0 };
    let theme = UiTheme {
        appearance: if has("--dark") {
            Appearance::Dark
        } else {
            Appearance::Light
        },
        contrast: if has("--hc") {
            Contrast::High
        } else {
            Contrast::Standard
        },
        density: if has("--compact") {
            Density::Compact
        } else {
            Density::Comfortable
        },
        text_scale: scale,
        reading_scale: scale,
        ..Default::default()
    };
    let width = if has("--narrow") { 380.0 } else { 900.0 };
    let mut app = App::new();
    if has("--reader") {
        app.add_plugin(rndr_core::MarkdownPlugin);
    } else {
        app.add_plugin(rndr_markdown_ui::MarkdownPlugin);
    }
    app.world_mut()
        .insert_resource(Arc::new(rndr_render::ParleyText::bundled()) as Arc<dyn TextEngine>);
    app.world_mut().insert_resource(Viewport::new(
        width,
        if has("--reader") || has("--parity") {
            1400.0
        } else if scale > 1.0 {
            1950.0
        } else {
            1000.0
        },
    ));
    app.world_mut().insert_resource(theme.clone());
    let root = app.world_mut().spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::column()
        },
        theme.color(Token::Content),
    ));
    let parity = "## Documentation conventions\n\nFor each machine, track:\n\n- Hostname and purpose\n- Management address\n- Backup and recovery plan\n\n> [!warning]\n> Record where a secret is managed, not the secret itself.\n\n*Last updated: October*\n";
    let document_text = if has("--parity") { parity } else { FIELD_NOTES };
    if has("--reader") {
        rndr_ui::apply_theme_bridge(app.world_mut(), theme.clone());
        let source = "# Project index\n\n- [[notes/index|Project notes]] — project overview\n- [[notes/reference|Reference]] — reference material\n- [[notes/archive|Archive]] — past decisions\n\n## Current projects\n\n| Name | Count | Status |\n|---|---:|:---:|\n| `Alpha` | 123 | Ready for review |\n| Beta | 7 | In progress |\n\n> [!info] Status\n> Review the project notes for details.\n";
        let view = markdown_view(
            app.world_mut(),
            if has("--parity") { parity } else { source },
        );
        set_parent(app.world_mut(), view, root);
        app.world_mut()
            .get_mut::<MarkdownView>(view)
            .unwrap()
            .set_follow(false);
    } else {
        let panel = if !has("--parity") {
            use rndr_ui::tabs::{TabId, TabSpec, TabStrip, TabStripProps};
            TabStrip::mount(
                app.world_mut(),
                root,
                UiId(2),
                TabStripProps::new("Documents"),
                &[
                    TabSpec::new(1, "Index").pinned(true),
                    TabSpec::new(2, "Field notes").dirty(true),
                ],
                TabId(2),
            )
            .expect("fixture tabs")
            .panel(app.world(), TabId(2))
            .expect("fixture panel")
        } else {
            root
        };
        let document = app
            .world_mut()
            .resource_mut::<MarkdownDocuments>()
            .unwrap()
            .open(document_text);
        // Keep caret outside styled content so source-reveal does not distort the fixture.
        app.world_mut()
            .resource_mut::<MarkdownDocuments>()
            .unwrap()
            .get_mut(document)
            .unwrap()
            .move_caret(document_text.len(), document_text.len())
            .expect("fixture caret");
        MarkdownSurface::mount(
            app.world_mut(),
            panel,
            UiId(1),
            MarkdownSurfaceProps::new(document, "Field notes"),
        )
        .expect("fixture surface");
    }
    for _ in 0..3 {
        rndr_paint::frame(&mut app);
    }
    use rndr_render::{
        capture::{capture, Target},
        encode_png,
        observe::ObservationContent,
        Exporter,
    };
    app.world_mut().insert_resource(ObservationContent::Public);
    let mut exporter = Exporter::new()?;
    let result = capture(&mut app, &mut exporter, Target::Entity(root), |_, _| None)?;
    std::fs::write(format!("{prefix}.png"), encode_png(&result.image)?)?;
    std::fs::write(
        format!("{prefix}.json"),
        result.metadata.to_json().to_string(),
    )?;
    if has("--probe") {
        verify_contrast(app.world(), root, &mut exporter, &result.image)?;
    }
    println!(
        "{}x{} pixels, {} same-frame nodes",
        result.image.width,
        result.image.height,
        result.metadata.nodes.len()
    );
    Ok(())
}

// Render the same settled scene without glyphs to sample the actual backdrop,
// including inline code/highlight fills. Measure full-coverage ink, not the
// antialiasing fringe (which approaches the background color by definition).
fn verify_contrast(
    world: &World,
    root: Entity,
    exporter: &mut rndr_render::Exporter,
    image: &rndr_render::RgbaImage,
) -> Result<(), Box<dyn std::error::Error>> {
    let (mut quads, draws) = rndr_paint::paint_subtree(world, root);
    let solid = text_engine(world).solid_uv(&style_of(world));
    let mut glyphs = Vec::new();
    for q in &mut quads {
        if q.uv != solid {
            glyphs.push(*q);
            q.color.a = 0.;
        }
    }
    let frame = rndr_paint::Frame {
        quads,
        draws,
        slots: Vec::new(),
    };
    let backdrop = exporter.render_frame(Some(world), &frame, image.width, image.height, 1.)?;
    assert!(glyphs.len() > 100, "expected painted document glyphs");
    let disabled: Vec<_> = world
        .query::<Button>()
        .into_iter()
        .filter(|(_, b)| !b.enabled)
        .filter_map(|(e, _)| world.get::<Rect>(e))
        .collect();
    for q in glyphs {
        let x = q.rect.x + q.rect.w * 0.5;
        let y = q.rect.y + q.rect.h * 0.5;
        if q.clip.contains(x, y) && !disabled.iter().any(|r| r.contains(x, y)) {
            let bg = pixel(&backdrop, x, y);
            let ratio = contrast(q.color, bg);
            assert!(
                ratio >= 4.5,
                "ink at {x},{y}: {ratio:.2}:1, foreground {:?} on {bg:?}",
                q.color
            );
        }
    }
    // Layout rectangles can agree with their clips even when wrapped text
    // paints below the last table row. Check the shaped run as well.
    for (e, label) in world.query::<Label>() {
        let (Some(r), Some(run)) = (world.get::<Rect>(e), world.get::<TextLayout>(e)) else {
            continue;
        };
        if r.w > 0. && r.h > 0. && label.overflow == TextOverflow::Wrap && !label.text.is_empty() {
            let inset = world.get::<LabelBox>(e).map_or(0., |b| b.inset_y);
            assert!(
                run.height + inset <= r.h + 1.,
                "wrapped label {:?}: shaped height {} + inset {inset}, bounds {r:?}",
                label.text,
                run.height
            );
        }
    }
    // Inspect real, square checkbox faces. Sample the top straight edge (not
    // a rounded corner), its outside neighbor, and the check against its fill.
    let mut faces = 0;
    for (entity, _) in world.query::<Border>() {
        let Some(r) = world.get::<Rect>(entity) else {
            continue;
        };
        if r.w != r.h || !world.has::<Radius>(entity) || r.w < 2. {
            continue;
        }
        let x = r.x + r.w * 0.5;
        let ratio = contrast(pixel(image, x, r.y), pixel(image, x, r.y - 1.));
        assert!(ratio >= 3., "box edge {r:?}: {ratio:.2}:1");
        if world.has::<CheckMark>(entity) {
            let ratio = contrast(
                pixel(image, r.x + r.w * 0.43, r.y + r.h * 0.70),
                pixel(image, x, r.y + r.h * 0.2),
            );
            assert!(ratio >= 3., "check {r:?}: {ratio:.2}:1");
        }
        faces += 1;
    }
    assert_eq!(faces, 3, "fixture task faces");
    Ok(())
}

fn pixel(image: &rndr_render::RgbaImage, x: f32, y: f32) -> Color {
    assert!(x >= 0. && y >= 0. && x < image.width as f32 && y < image.height as f32);
    let i = (y as usize * image.width as usize + x as usize) * 4;
    let p = &image.rgba[i..i + 3];
    Color::rgb(p[0] as f32 / 255., p[1] as f32 / 255., p[2] as f32 / 255.)
}

fn contrast(fg: Color, bg: Color) -> f32 {
    let lum = |c: Color| {
        let linear = |v: f32| {
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        linear(c.r) * 0.2126 + linear(c.g) * 0.7152 + linear(c.b) * 0.0722
    };
    let a = lum(fg) * fg.a + lum(bg) * (1. - fg.a);
    let b = lum(bg);
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
