//! Pixels for live text and live colours drawn by live-only frames.

use rndr_core::{
    Border, Color, Entity, Gradient, GradientStop, Label, Layout, Live, LiveBorder, LiveGradient,
    LiveLabel, LiveShadow, LiveText, LiveTextColor, Shadow, Shadows, Sim, Size, Viewport,
};
use rndr_render::{ExportError, Exporter, RgbaImage};

const WHITE: Color = Color::rgb(1.0, 1.0, 1.0);
const BLACK: Color = Color::rgb(0.0, 0.0, 0.0);
const RED: Color = Color::rgb(1.0, 0.0, 0.0);
const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);
const GREEN: Color = Color::rgb(0.0, 1.0, 0.0);

fn sim() -> Sim {
    let mut sim = Sim::new();
    sim.world().insert_resource(Viewport::new(120.0, 120.0));
    sim
}

fn card(sim: &mut Sim, w: f32, h: f32, color: Color) -> Entity {
    sim.world().spawn((
        Layout {
            width: Size::Px(w),
            height: Size::Px(h),
            padding: 10.0,
            ..Layout::column()
        },
        color,
    ))
}

fn exporter() -> Exporter {
    match Exporter::new() {
        Ok(exporter) => exporter,
        Err(ExportError::NoAdapter) => panic!("no graphics adapter"),
        Err(err) => panic!("{err}"),
    }
}

fn shot(sim: &mut Sim, exporter: &mut Exporter, live: bool) -> RgbaImage {
    let frame = if live {
        rndr_paint::frame_live(&mut sim.app)
    } else {
        rndr_paint::frame(&mut sim.app)
    };
    exporter
        .render_frame(Some(sim.world()), &frame, 120, 120, 1.0)
        .expect("render")
}

fn pixel(image: &RgbaImage, x: u32, y: u32) -> [u8; 3] {
    let i = ((y * image.width + x) * 4) as usize;
    [image.rgba[i], image.rgba[i + 1], image.rgba[i + 2]]
}

/// Pixels in the top 40 rows, where the cards are, that pass `test`.
fn count(image: &RgbaImage, test: impl Fn([u8; 3]) -> bool) -> usize {
    (0..40)
        .flat_map(|y| (0..image.width).map(move |x| (x, y)))
        .filter(|&(x, y)| test(pixel(image, x, y)))
        .count()
}

#[test]
fn live_text_redraws_in_a_live_only_frame() {
    let mut sim = sim();
    let e = card(&mut sim, 120.0, 40.0, WHITE);
    sim.world().insert(e, Label::new("1").with_color(BLACK));
    let text = LiveText::new("1");
    sim.world().insert(e, LiveLabel::new(&text));
    let mut exporter = exporter();
    let dark = |p: [u8; 3]| p[0] < 100;
    let before = count(&shot(&mut sim, &mut exporter, false), dark);
    text.set("1111111");
    let after = count(&shot(&mut sim, &mut exporter, true), dark);
    assert!(before > 0 && after > before * 4, "{before} -> {after}");
}

#[test]
fn live_text_colour_recolours_the_glyphs() {
    let mut sim = sim();
    let e = card(&mut sim, 120.0, 40.0, WHITE);
    sim.world().insert(e, Label::new("MMMM"));
    let ink = Live::new(RED);
    sim.world().insert(e, LiveTextColor::new(&ink));
    let mut exporter = exporter();
    let red = |p: [u8; 3]| p[0] > 200 && p[1] < 80 && p[2] < 80;
    let blue = |p: [u8; 3]| p[2] > 200 && p[0] < 80 && p[1] < 80;
    let first = shot(&mut sim, &mut exporter, false);
    assert!(count(&first, red) > 10 && count(&first, blue) == 0);
    ink.set(BLUE);
    let second = shot(&mut sim, &mut exporter, true);
    assert!(count(&second, blue) > 10 && count(&second, red) == 0);
}

#[test]
fn live_border_shadow_and_gradient_repaint() {
    let mut sim = sim();
    let page = card(&mut sim, 120.0, 120.0, WHITE);
    let e = card(&mut sim, 60.0, 60.0, WHITE);
    rndr_core::set_parent(sim.world(), e, page);
    sim.world().insert(
        e,
        Border {
            color: BLACK,
            width: 4.0,
        },
    );
    sim.world().insert(
        e,
        Shadows(vec![Shadow {
            offset: [20.0, 0.0],
            blur: 0.0,
            spread: 0.0,
            color: BLACK,
            inset: false,
        }]),
    );
    sim.world().insert(
        e,
        Gradient::Linear {
            angle: 0.0,
            stops: vec![GradientStop::new(0.0, BLACK), GradientStop::new(1.0, BLACK)],
        },
    );
    let (edge, glow, top, bottom) = (
        Live::new(BLACK),
        Live::new(BLACK),
        Live::new(BLACK),
        Live::new(BLACK),
    );
    sim.world().insert(e, LiveBorder::new(&edge));
    sim.world().insert(e, LiveShadow::new(&glow));
    sim.world().insert(e, LiveGradient::new(&top, &bottom));
    let mut exporter = exporter();
    shot(&mut sim, &mut exporter, false);
    edge.set(RED);
    glow.set(BLUE);
    top.set(GREEN);
    bottom.set(BLUE);
    let image = shot(&mut sim, &mut exporter, true);
    // The card sits at (10, 10) to (70, 70).
    assert_eq!(pixel(&image, 11, 40), [255, 0, 0], "border");
    assert_eq!(
        pixel(&image, 80, 40),
        [0, 0, 255],
        "shadow past the right edge"
    );
    let near_top = pixel(&image, 40, 16);
    let near_bottom = pixel(&image, 40, 64);
    // Stops blend in linear light, so blue shows early.
    assert!(
        near_top[1] > 220 && near_top[1] > near_top[2] * 2,
        "{near_top:?}"
    );
    assert!(
        near_bottom[2] > 220 && near_bottom[2] > near_bottom[1] * 2,
        "{near_bottom:?}"
    );
}
