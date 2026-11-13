use rndr_core::{text_engine, Color, Font, Icon, Label, Rect, TextStyle, World, ICON_SCALAR_START};
use rndr_paint::{
    icon_atlas_size, icon_coverage, install_icons, paint, quads_snapshot, IconFace, ICON_CELL,
};

fn label(text: &str, size: f32, color: Color) -> World {
    let mut world = World::new();
    let style = TextStyle {
        size,
        ..TextStyle::default()
    };
    world.spawn((
        Rect {
            x: 10.0,
            y: 20.0,
            w: 200.0,
            h: 64.0,
        },
        style,
        Label::new(text).with_color(color),
    ));
    world
}

fn icon_quads(world: &World) -> Vec<rndr_paint::Quad> {
    paint(world)
        .into_iter()
        .filter(|quad| quad.atlas == 2)
        .collect()
}

#[test]
fn every_icon_has_ink_and_a_private_use_scalar() {
    let (width, height) = icon_atlas_size();
    let atlas = IconFace.color_atlas().expect("icon atlas");
    assert_eq!(atlas.width, width);
    assert_eq!(atlas.height, height);
    assert_eq!(atlas.rgba.len(), (width * height * 4) as usize);
    assert!(Icon::from_name("settings").is_some());
    assert!(Icon::from_name("trash").is_some());
    assert!(Icon::from_name("chevron-right").is_some());
    assert_eq!(
        Icon::Check.scalar() as u32,
        ICON_SCALAR_START + Icon::Check.index() as u32
    );
    assert_eq!(Icon::from_name("home"), Some(Icon::House));
    assert_eq!(Icon::from_name(" Map_Pin "), Some(Icon::MapPin));
    assert!(Icon::from_name("skull").is_none());
    for index in 0..Icon::COUNT {
        let icon = Icon::from_index(index).unwrap();
        let coverage = icon_coverage(icon.scalar(), ICON_CELL).unwrap();
        assert!(
            coverage.iter().any(|sample| *sample > 32),
            "{} is blank",
            icon.name()
        );
    }
    let check = icon_coverage(Icon::Check.scalar(), ICON_CELL).unwrap();
    let close = icon_coverage(Icon::X.scalar(), ICON_CELL).unwrap();
    assert_ne!(check, close);
}

#[test]
fn icons_tint_with_the_text_color_and_scale_with_the_font() {
    let color = Color::rgb(0.2, 0.4, 0.8);
    let small = label(&Icon::Search.scalar().to_string(), 16.0, color);
    let small_quads = icon_quads(&small);
    assert_eq!(small_quads.len(), 1);
    assert_eq!(small_quads[0].color, color);
    assert!((small_quads[0].rect.w - 16.0).abs() < 0.6);
    assert!((small_quads[0].rect.h - 16.0).abs() < 0.6);
    let face = IconFace;
    assert_eq!(small_quads[0].uv, face.glyph_uv(Icon::Search.scalar()));

    let big = label(&Icon::Search.scalar().to_string(), 32.0, color);
    let big_quads = icon_quads(&big);
    assert_eq!(big_quads.len(), 1);
    assert!(
        (big_quads[0].rect.w - 32.0).abs() < 0.6,
        "{}",
        big_quads[0].rect.w
    );
    assert!((big_quads[0].rect.h / small_quads[0].rect.h - 2.0).abs() < 0.05);
}

#[test]
fn an_icon_sits_inline_with_text() {
    install_icons();
    let world = label(
        &format!("A{}", Icon::Check.scalar()),
        16.0,
        Color::rgb(0.94, 0.95, 0.97),
    );
    let engine = text_engine(&world);
    let style = TextStyle::default();
    let layout = engine.layout(
        &format!("A{}", Icon::Check.scalar()),
        &style,
        &[],
        f32::INFINITY,
    );
    let icon = layout
        .glyphs
        .iter()
        .find(|glyph| glyph.ch == Icon::Check.scalar())
        .expect("icon glyph");
    let letter = layout
        .glyphs
        .iter()
        .find(|glyph| glyph.ch == 'A')
        .expect("A");
    assert!(icon.x >= letter.x + letter.w - 0.1);
    assert_eq!(icon.atlas, 2);
    let painted = rndr_paint::Frame {
        quads: paint(&world),
        slots: Vec::new(),
        draws: Vec::new(),
    };
    insta::assert_snapshot!(quads_snapshot(&painted));
}
