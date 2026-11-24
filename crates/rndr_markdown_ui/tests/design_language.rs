//! Document paint contracts, independent of native input.

#[test]
fn mounted_reading_uses_fourteen_twenty_two_independent_of_density() {
    let mut app = App::new();
    app.add_plugin(rndr_markdown_ui::MarkdownPlugin);
    app.world_mut().insert_resource(Viewport::new(800.0, 600.0));
    let document = app
        .world_mut()
        .resource_mut::<MarkdownDocuments>()
        .unwrap()
        .open("Reading body\n\n");
    let root = app.world_mut().spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    MarkdownSurface::mount(
        app.world_mut(),
        root,
        UiId(2),
        MarkdownSurfaceProps::new(document, "Reading"),
    )
    .unwrap();
    for density in [rndr_ui::Density::Comfortable, rndr_ui::Density::Compact] {
        for scale in [1.0, 2.0, 1.0] {
            let theme = app.world_mut().resource_mut::<UiTheme>().unwrap();
            theme.density = density;
            theme.reading_scale = scale;
            app.update();
            app.update();
            let label = app
                .world()
                .entities_with::<Label>()
                .into_iter()
                .find(|entity| app.world().get::<Label>(*entity).unwrap().text == "Reading body")
                .expect("mounted reading label");
            let style = app.world().get::<TextStyle>(label).unwrap();
            assert_eq!(style.size, 14.0 * scale);
            assert!((style.line_height.unwrap() * style.size - 22.0 * scale).abs() < 0.001);
        }
    }
}

use rndr_core::*;
use rndr_markdown_ui::*;
use rndr_ui::{Appearance, Contrast, Token, UiId, UiTheme};

fn luminance(c: Color) -> f32 {
    let linear = |v: f32| {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(c.r) + 0.7152 * linear(c.g) + 0.0722 * linear(c.b)
}
fn ratio(a: Color, b: Color) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
#[test]
fn document_pairs_pass_in_all_palettes() {
    use Token::*;
    for appearance in [Appearance::Light, Appearance::Dark] {
        for contrast in [Contrast::Standard, Contrast::High] {
            let theme = UiTheme {
                appearance,
                contrast,
                ..Default::default()
            };
            for background in [Content, Canvas, CodeFill, Highlight] {
                assert!(
                    ratio(theme.color(Text), theme.color(background)) >= 4.5,
                    "{appearance:?} {contrast:?} Text/{background:?}"
                );
            }
            for foreground in [
                HueRed, HueOrange, HueYellow, HueGreen, HueBlue, HuePurple, HuePink, Accent,
            ] {
                for background in [Content, Canvas] {
                    assert!(
                        ratio(theme.color(foreground), theme.color(background)) >= 4.5,
                        "{appearance:?} {contrast:?} {foreground:?}/{background:?}"
                    );
                }
            }
        }
    }
}
#[test]
fn mounted_spans_repaint_after_theme_change_without_editing_source() {
    let source = "Plain **bold** `code` ==highlight== {{red|red}} [link](https://example.com)\n\n";
    let mut app = App::new();
    app.add_plugin(rndr_markdown_ui::MarkdownPlugin);
    app.world_mut().insert_resource(Viewport::new(800.0, 600.0));
    let document = app
        .world_mut()
        .resource_mut::<MarkdownDocuments>()
        .unwrap()
        .open(source);
    app.world_mut()
        .resource_mut::<MarkdownDocuments>()
        .unwrap()
        .get_mut(document)
        .unwrap()
        .move_caret(source.len(), source.len())
        .unwrap();
    let root = app.world_mut().spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    MarkdownSurface::mount(
        app.world_mut(),
        root,
        UiId(1),
        MarkdownSurfaceProps::new(document, "Fixture"),
    )
    .unwrap();
    for appearance in [Appearance::Light, Appearance::Dark, Appearance::Light] {
        app.world_mut()
            .resource_mut::<UiTheme>()
            .unwrap()
            .appearance = appearance;
        app.update();
        app.update();
        let theme = app.world().resource::<UiTheme>().unwrap();
        let runs: Vec<_> = app
            .world()
            .entities_with::<Spans>()
            .into_iter()
            .flat_map(|e| app.world().get::<Spans>(e).unwrap().runs.iter())
            .collect();
        for color in [
            theme.color(Token::Text),
            theme.color(Token::HueRed),
            theme.color(Token::Accent),
        ] {
            assert!(
                runs.iter().any(|span| span.color == color),
                "missing {color:?} in {appearance:?} actual spans"
            );
        }
        for fill in [Token::CodeFill, Token::Highlight] {
            assert!(runs
                .iter()
                .any(|span| span.highlight == Some(theme.color(fill))));
        }
    }
    assert_eq!(
        app.world()
            .resource::<MarkdownDocuments>()
            .unwrap()
            .get(document)
            .unwrap()
            .bytes(),
        source
    );
}
