use rndr_core::*;
fn append(world: &mut World, parent: Entity, bundle: impl Bundle) -> Entity {
    let e = world.spawn(bundle);
    set_parent(world, e, parent);
    e
}
fn contains_hug_children(world: &World) {
    for parent in world.entities_with::<Layout>() {
        let layout = world.get::<Layout>(parent).unwrap();
        if layout.height != Size::Hug {
            continue;
        }
        let rect = *world.get::<Rect>(parent).unwrap();
        for child in children_of(world, parent) {
            if world.has::<Hidden>(child) {
                continue;
            }
            let child_rect = *world.get::<Rect>(child).unwrap();
            assert!(
                child_rect.y >= rect.y - 0.01
                    && child_rect.y + child_rect.h <= rect.y + rect.h + 0.01,
                "{parent:?} {rect:?} clips {child:?} {child_rect:?}"
            );
        }
    }
}
#[test]
fn nested_wrapped_hug_heights_are_correct_on_first_frame_resize_and_scale() {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut()
        .insert_resource(Viewport::new(900.0, 20000.0));
    let root = app.world_mut().spawn((Layout {
        width: Size::Px(280.0),
        ..Layout::column()
    },));
    let card = append(
        app.world_mut(),
        root,
        (Layout {
            width: Size::Fill,
            padding: 16.0,
            gap: 8.0,
            ..Layout::column()
        },),
    );
    append(
        app.world_mut(),
        card,
        (
            Layout {
                width: Size::Fill,
                ..Layout::column()
            },
            Label::new("A very long translated-style title that must wrap")
                .overflow(TextOverflow::Wrap),
            TextStyle {
                size: 24.0,
                ..TextStyle::default()
            },
        ),
    );
    let slot = append(
        app.world_mut(),
        card,
        (Layout {
            width: Size::Fill,
            ..Layout::column()
        },),
    );
    let body = append(
        app.world_mut(),
        slot,
        (
            Layout {
                width: Size::Fill,
                ..Layout::column()
            },
            Label::new("Nested reading content wraps without clipping. ".repeat(8))
                .overflow(TextOverflow::Wrap),
            TextStyle {
                size: 16.0,
                ..TextStyle::default()
            },
        ),
    );
    for width in [280.0, 160.0, 600.0, 240.0] {
        for size in [16.0, 32.0] {
            app.world_mut().get_mut::<Layout>(root).unwrap().width = Size::Px(width);
            app.world_mut().get_mut::<TextStyle>(body).unwrap().size = size;
            app.update();
            contains_hug_children(app.world());
        }
    }
}
#[test]
fn row_fill_widths_determine_hug_cross_height_before_clipping() {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut()
        .insert_resource(Viewport::new(800.0, 20000.0));
    let row = app.world_mut().spawn((Layout {
        width: Size::Px(240.0),
        gap: 8.0,
        ..Layout::row()
    },));
    for _ in 0..2 {
        let column = append(
            app.world_mut(),
            row,
            (Layout {
                width: Size::Fill,
                padding: 8.0,
                ..Layout::column()
            },),
        );
        append(
            app.world_mut(),
            column,
            (
                Layout {
                    width: Size::Fill,
                    ..Layout::column()
                },
                Label::new("Long wrapping row content. ".repeat(10)).overflow(TextOverflow::Wrap),
            ),
        );
    }
    app.update();
    contains_hug_children(app.world());
}

/// Shaping engines report fractional advances; the builtin one does not.
struct Fractional(BuiltinText);
impl TextEngine for Fractional {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn layout(&self, text: &str, style: &TextStyle, runs: &[TextSpan], max: f32) -> TextLayout {
        self.0.layout(text, style, runs, max)
    }
    fn measure(&self, text: &str, style: &TextStyle, runs: &[TextSpan], max: f32) -> TextMeasure {
        let mut m = self.0.measure(text, style, runs, max);
        if !text.is_empty() {
            m.width += 0.23;
        }
        m
    }
    fn line_height(&self, style: &TextStyle) -> f32 {
        self.0.line_height(style)
    }
    fn unit_advance(&self, style: &TextStyle) -> f32 {
        self.0.unit_advance(style)
    }
    fn solid_uv(&self, style: &TextStyle) -> [f32; 4] {
        self.0.solid_uv(style)
    }
    fn coverage_atlas(&self) -> Option<std::sync::Arc<FontAtlas>> {
        self.0.coverage_atlas()
    }
    fn color_atlas(&self) -> Option<std::sync::Arc<ColorAtlas>> {
        self.0.color_atlas()
    }
    fn fallbacks(&self, family: &str) -> Vec<String> {
        self.0.fallbacks(family)
    }
    fn set_fallbacks(&self, family: &str, faces: &[String]) {
        self.0.set_fallbacks(family, faces);
    }
    fn register_font(&self, name: &str, font: std::sync::Arc<dyn Font>) {
        self.0.register_font(name, font);
    }
    fn begin_frame(&self) {
        self.0.begin_frame();
    }
    fn shape_count(&self) -> u64 {
        self.0.shape_count()
    }
}

/// Pixel snapping must never make a hugging label narrower than its own
/// text, or an ellipsizing label truncates text that fits.
#[test]
fn hug_labels_are_never_snapped_narrower_than_their_text() {
    for scale in [1.0, 1.5, 2.0] {
        for offset in [0.0, 13.3, 13.7] {
            let mut app = App::new();
            let engine: std::sync::Arc<dyn TextEngine> =
                std::sync::Arc::new(Fractional(BuiltinText::new()));
            app.world_mut().insert_resource(engine);
            app.add_plugin(Chrome);
            let mut viewport = Viewport::new(600.0, 200.0);
            viewport.scale = scale;
            app.world_mut().insert_resource(viewport);
            // A tree row: padding, gap, a Fill title that must give way, then
            // hugging metadata.
            let row = app.world_mut().spawn((Layout {
                width: Size::Px(328.0 + offset),
                padding: 4.0,
                gap: 4.0,
                ..Layout::row()
            },));
            append(
                app.world_mut(),
                row,
                (
                    Layout {
                        width: Size::Fill,
                        min_width: 0.0,
                        ..Layout::column()
                    },
                    Label::new("A long note title that cannot fit inside this row at all")
                        .overflow(TextOverflow::Ellipsis),
                    LabelBox::CONTENT,
                ),
            );
            let meta = append(
                app.world_mut(),
                row,
                (
                    Layout::column(),
                    Label::new("Jan 1 · note").overflow(TextOverflow::Ellipsis),
                    LabelBox::CONTENT,
                ),
            );
            app.update();
            app.update();
            let world = app.world();
            let text = text_engine(world)
                .measure("Jan 1 · note", &style_for(world, meta), &[], f32::INFINITY)
                .width;
            let rect = *world.get::<Rect>(meta).unwrap();
            assert!(
                rect.w + 0.001 >= text,
                "scale {scale} offset {offset}: {rect:?} narrower than {text}"
            );
        }
    }
}
