//! Pixels for `Blur` and `BackdropBlur`.

use rndr_core::{
    set_parent, Absolute, BackdropBlur, Blur, Color, Corners, Entity, Layout, Opacity, Sim, Size,
    Transform, Viewport,
};
use rndr_render::{ExportError, Exporter, RgbaImage};

const BLACK: Color = Color::rgb(0.0, 0.0, 0.0);
const WHITE: Color = Color::rgb(1.0, 1.0, 1.0);

fn sim() -> Sim {
    let mut sim = Sim::new();
    sim.world().insert_resource(Viewport::new(120.0, 120.0));
    sim
}

fn boxed(sim: &mut Sim, w: f32, h: f32, layout: Layout) -> Entity {
    sim.world().spawn((Layout {
        width: Size::Px(w),
        height: Size::Px(h),
        ..layout
    },))
}

fn filled(sim: &mut Sim, w: f32, h: f32, color: Color) -> Entity {
    let e = boxed(sim, w, h, Layout::column());
    sim.world().insert(e, color);
    e
}

fn at(sim: &mut Sim, e: Entity, left: f32, top: f32) {
    sim.world().insert(
        e,
        Absolute {
            left: Some(left),
            top: Some(top),
            ..Absolute::default()
        },
    );
}

fn red(image: &RgbaImage, x: u32, y: u32) -> u8 {
    image.rgba[((y * image.width + x) * 4) as usize]
}

fn render(sim: &mut Sim) -> RgbaImage {
    let mut exporter = match Exporter::new() {
        Ok(exporter) => exporter,
        Err(ExportError::NoAdapter) => panic!("no graphics adapter"),
        Err(err) => panic!("{err}"),
    };
    let frame = rndr_paint::frame(&mut sim.app);
    exporter
        .render_frame(Some(sim.world()), &frame, 120, 120, 1.0)
        .expect("render")
}

/// White page with a black 40×40 square at (40, 40), blurred by `sigma`.
fn square(sigma: f32) -> RgbaImage {
    let mut sim = sim();
    let page = filled(&mut sim, 120.0, 120.0, WHITE);
    let square = filled(&mut sim, 40.0, 40.0, BLACK);
    at(&mut sim, square, 40.0, 40.0);
    sim.world().insert(square, Blur(sigma));
    set_parent(sim.world(), square, page);
    render(&mut sim)
}

#[test]
fn blur_spreads_the_subtree_past_its_edges() {
    let sharp = square(0.0);
    assert_eq!(red(&sharp, 36, 60), 255);
    assert_eq!(red(&sharp, 44, 60), 0);
    let soft = square(6.0);
    let (outside, inside) = (red(&soft, 36, 60), red(&soft, 44, 60));
    assert!(outside > 120 && outside < 250, "{outside}");
    assert!(inside > 40 && inside < outside, "{inside}");
    assert!(red(&soft, 60, 60) < 10, "the middle stays dark");
    assert_eq!(red(&soft, 10, 60), 255, "far away is untouched");
    // Symmetric about the square's centre.
    let right = red(&soft, 83, 60);
    assert!(right.abs_diff(outside) <= 2, "{right} vs {outside}");
}

#[test]
fn the_parent_clip_cuts_the_blur() {
    let mut sim = sim();
    let page = filled(&mut sim, 120.0, 120.0, WHITE);
    let panel = filled(&mut sim, 60.0, 60.0, WHITE);
    at(&mut sim, panel, 0.0, 0.0);
    let bar = filled(&mut sim, 60.0, 20.0, BLACK);
    sim.world().insert(bar, Blur(4.0));
    set_parent(sim.world(), panel, page);
    set_parent(sim.world(), bar, panel);
    let image = render(&mut sim);
    assert!(red(&image, 30, 24) < 250, "spreads inside the panel");
    assert_eq!(red(&image, 64, 10), 255, "cut at the panel's edge");
}

/// Black left half, white right half, and a 40×40 panel across the seam at
/// (50, 40) with corner radius `radius`. `group` fades the page a hair so
/// everything draws inside a layer.
fn frosted(radius: f32, group: bool) -> RgbaImage {
    frosted_with(radius, group, |_, _| {})
}

fn frosted_with(radius: f32, group: bool, tweak: impl Fn(&mut Sim, Entity)) -> RgbaImage {
    let mut sim = sim();
    let page = boxed(&mut sim, 120.0, 120.0, Layout::row());
    if group {
        sim.world().insert(page, Opacity(0.999));
    }
    let left = filled(&mut sim, 60.0, 120.0, BLACK);
    let right = filled(&mut sim, 60.0, 120.0, WHITE);
    let panel = boxed(&mut sim, 40.0, 40.0, Layout::column());
    at(&mut sim, panel, 50.0, 40.0);
    sim.world().insert(panel, BackdropBlur(6.0));
    sim.world().insert(panel, Corners([radius; 4]));
    for child in [left, right, panel] {
        set_parent(sim.world(), child, page);
    }
    tweak(&mut sim, panel);
    render(&mut sim)
}

fn assert_frosted(image: &RgbaImage) {
    assert!(red(image, 45, 60) < 5, "left of the panel stays black");
    assert!(red(image, 95, 60) > 250, "right of the panel stays white");
    let near_black = red(image, 52, 60);
    let near_white = red(image, 66, 60);
    assert!(near_black > 15 && near_black < 140, "{near_black}");
    assert!(near_white > near_black && near_white < 250, "{near_white}");
    assert!(red(image, 88, 60) > 250, "far inside on the white side");
}

#[test]
fn backdrop_blur_frosts_what_is_behind_the_panel() {
    let square = frosted(0.0, false);
    assert_frosted(&square);
    assert!(red(&square, 51, 41) > 5, "a square corner blurs too");
    let round = frosted(20.0, false);
    assert_frosted(&round);
    assert!(red(&round, 51, 41) < 5, "outside the round corner is sharp");
}

#[test]
fn backdrop_blur_inside_a_layer_blurs_what_the_layer_drew() {
    let image = frosted(20.0, true);
    assert_frosted(&image);
    assert!(red(&image, 51, 41) < 5);
}

#[test]
fn a_moved_panel_blurs_where_it_is_drawn_and_fades_with_it() {
    let moved = frosted_with(20.0, false, |sim, panel| {
        at(sim, panel, 30.0, 40.0);
        sim.world().insert(panel, Transform::translate(20.0, 0.0));
    });
    assert_frosted(&moved);
    assert!(red(&moved, 51, 41) < 5);
    let still = frosted(20.0, false);
    assert!(red(&moved, 52, 60).abs_diff(red(&still, 52, 60)) <= 2);
    let faded = frosted_with(0.0, false, |sim, panel| {
        sim.world().insert(panel, Opacity(0.5));
    });
    let (half, full) = (red(&faded, 52, 60), red(&still, 52, 60));
    assert!(half > 0 && half < full, "{half} vs {full}");
}
