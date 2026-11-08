//! Strikethrough spans and read-only selection highlights.

use rndr_core::{
    theme_of, App, Chrome, Color, Entity, Label, Layout, Rect, Selectable, Size, Spans, TextLayout,
    TextSpan, Viewport, World, TEXT_ORIGIN_X, TEXT_ORIGIN_Y,
};
use rndr_paint::{paint, solid_uv};

fn boot(text: &str) -> (App, Entity) {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(300.0, 100.0));
    let entity = app.world_mut().spawn((
        Layout {
            width: Size::Px(280.0),
            height: Size::Px(60.0),
            ..Layout::column()
        },
        Label::new(text),
    ));
    app.update();
    (app, entity)
}

fn baseline(world: &World, entity: Entity) -> f32 {
    world.get::<TextLayout>(entity).unwrap().lines[0].baseline
}

#[test]
fn strike_crosses_the_range_above_the_baseline() {
    let (mut app, e) = boot("keep gone keep");
    let tint = Color::rgb(0.9, 0.2, 0.1);
    let under = Color::rgb(0.1, 0.8, 0.2);
    app.world_mut().insert(
        e,
        Spans {
            runs: vec![
                TextSpan::new(5, 9, tint).strike(),
                TextSpan::new(10, 14, under).underline(),
            ],
        },
    );
    app.update();
    let rect = *app.world().get::<Rect>(e).unwrap();
    let base = rect.y + TEXT_ORIGIN_Y + baseline(app.world(), e);
    let quads = paint(app.world());
    let strike = quads
        .iter()
        .find(|quad| quad.uv == solid_uv() && quad.color == tint)
        .expect("strike quad");
    let layout = app.world().get::<TextLayout>(e).unwrap();
    let span = layout.selection_boxes(5, 9)[0];
    assert!((strike.rect.x - (rect.x + TEXT_ORIGIN_X + span.x)).abs() <= 1.0);
    assert!((strike.rect.w - span.w).abs() <= 1.0);
    assert!(
        strike.rect.y + strike.rect.h < base - 2.0,
        "{strike:?} base {base}"
    );
    assert!(strike.rect.y > rect.y + TEXT_ORIGIN_Y + 2.0);
    let underline = quads
        .iter()
        .find(|quad| quad.uv == solid_uv() && quad.color == under)
        .expect("underline quad");
    assert!(underline.rect.y >= base - 1.0);
}

#[test]
fn selectable_selection_paints_the_theme_highlight() {
    let (mut app, e) = boot("pick some words");
    let selection = theme_of(app.world()).selection;
    let highlighted = |app: &App| {
        paint(app.world())
            .iter()
            .filter(|quad| quad.uv == solid_uv() && quad.color == selection)
            .count()
    };
    app.world_mut().insert(e, Selectable::default());
    app.update();
    assert_eq!(highlighted(&app), 0);
    app.world_mut().insert(e, Selectable { anchor: 5, head: 9 });
    app.update();
    assert_eq!(highlighted(&app), 1);
    let rect = *app.world().get::<Rect>(e).unwrap();
    assert_eq!(rect, *app.world().get::<Rect>(e).unwrap());
}
