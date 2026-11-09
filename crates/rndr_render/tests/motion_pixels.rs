//! Pixels for colour and size motion at fixed clock times.

use std::time::Duration;

use rndr_core::motion::{self, ColorProp, Curve, Easing, LayoutMotion};
use rndr_core::{set_parent, Color, Entity, Layout, Sim, Size, Viewport};
use rndr_render::{ExportError, Exporter, RgbaImage};

fn boxed(sim: &mut Sim, w: f32, h: f32, color: Color) -> Entity {
    sim.world().spawn((
        Layout {
            width: Size::Px(w),
            height: Size::Px(h),
            ..Layout::column()
        },
        color,
    ))
}

fn pixel(image: &RgbaImage, x: u32, y: u32) -> [u8; 4] {
    let index = ((y * image.width + x) * 4) as usize;
    image.rgba[index..index + 4].try_into().unwrap()
}

fn render(sim: &mut Sim, exporter: &mut Exporter) -> RgbaImage {
    let frame = rndr_paint::frame(&mut sim.app);
    exporter
        .render_frame(Some(sim.world()), &frame, 120, 120, 1.0)
        .expect("render")
}

fn exporter() -> Exporter {
    match Exporter::new() {
        Ok(exporter) => exporter,
        Err(ExportError::NoAdapter) => panic!("no graphics adapter"),
        Err(err) => panic!("{err}"),
    }
}

#[test]
fn fill_blends_through_oklab_not_grey() {
    let mut sim = Sim::new();
    sim.world().insert_resource(Viewport::new(120.0, 120.0));
    let card = boxed(&mut sim, 120.0, 120.0, Color::rgb(0.0, 0.0, 1.0));
    rndr_paint::frame(&mut sim.app);
    motion::animate_color(
        sim.world(),
        card,
        ColorProp::Fill,
        Color::rgb(1.0, 1.0, 0.0),
        Curve::tween(1.0, Easing::Linear),
    );
    let mut exporter = exporter();
    rndr_paint::frame(&mut sim.app);
    sim.advance(Duration::from_millis(500));
    let mid = pixel(&render(&mut sim, &mut exporter), 60, 60);
    // An sRGB lerp would paint (128, 128, 128).
    let spread = mid[..3].iter().max().unwrap() - mid[..3].iter().min().unwrap();
    assert!(spread > 60, "{mid:?}");
    sim.advance(Duration::from_millis(600));
    let end = pixel(&render(&mut sim, &mut exporter), 60, 60);
    assert!(end[0] > 250 && end[1] > 250 && end[2] < 5, "{end:?}");
}

#[test]
fn growing_panel_reveals_children_through_its_clip() {
    let mut sim = Sim::new();
    sim.world().insert_resource(Viewport::new(120.0, 120.0));
    let white = Color::rgb(1.0, 1.0, 1.0);
    let root = boxed(&mut sim, 120.0, 120.0, white);
    let panel = boxed(&mut sim, 100.0, 20.0, Color::rgb(0.0, 0.0, 1.0));
    let body = boxed(&mut sim, 100.0, 100.0, Color::rgb(1.0, 0.0, 0.0));
    set_parent(sim.world(), panel, root);
    set_parent(sim.world(), body, panel);
    sim.world().insert(panel, LayoutMotion::default());
    let mut exporter = exporter();
    rndr_paint::frame(&mut sim.app);
    sim.advance(Duration::from_millis(16));
    let closed = render(&mut sim, &mut exporter);
    assert_eq!(pixel(&closed, 50, 10)[..3], [255, 0, 0]);
    assert_eq!(pixel(&closed, 50, 60)[..3], [255, 255, 255]);
    sim.world().get_mut::<Layout>(panel).unwrap().height = Size::Px(100.0);
    // The frame layout grows the panel still shows it closed.
    let start = render(&mut sim, &mut exporter);
    assert_eq!(pixel(&start, 50, 60)[..3], [255, 255, 255]);
    sim.advance(Duration::from_millis(120));
    let mid = render(&mut sim, &mut exporter);
    let edge = (0..120)
        .find(|&y| pixel(&mid, 50, y)[..3] == [255, 255, 255])
        .expect("panel still opening");
    assert!(edge > 22 && edge < 98, "{edge}");
    // Left of the panel stays the root's white.
    assert_eq!(pixel(&mid, 110, 10)[..3], [255, 255, 255]);
    sim.advance(Duration::from_secs(3));
    let open = render(&mut sim, &mut exporter);
    assert_eq!(pixel(&open, 50, 90)[..3], [255, 0, 0]);
    assert_eq!(pixel(&open, 50, 110)[..3], [255, 255, 255]);
}

#[test]
fn a_flight_paints_between_the_two_layouts() {
    use rndr_core::Corners;
    let mut sim = Sim::new();
    sim.world().insert_resource(Viewport::new(120.0, 120.0));
    let root = sim.world().spawn((
        Layout {
            width: Size::Px(120.0),
            height: Size::Px(120.0),
            ..Layout::row()
        },
        Color::rgb(1.0, 1.0, 1.0),
    ));
    let thumb = boxed(&mut sim, 20.0, 20.0, Color::rgb(1.0, 0.0, 0.0));
    let gap = boxed(&mut sim, 60.0, 20.0, Color::rgba(0.0, 0.0, 0.0, 0.0));
    let detail = boxed(&mut sim, 40.0, 120.0, Color::rgb(0.0, 0.0, 1.0));
    sim.world().insert(thumb, Corners::all(10.0));
    for e in [thumb, gap, detail] {
        set_parent(sim.world(), e, root);
    }
    let mut exporter = exporter();
    rndr_paint::frame(&mut sim.app);
    motion::fly(
        sim.world(),
        thumb,
        detail,
        Curve::tween(1.0, Easing::Linear),
    );
    sim.advance(Duration::from_millis(16));
    let start = render(&mut sim, &mut exporter);
    // The detail is drawn over the hidden thumbnail, round and red.
    assert!(
        pixel(&start, 10, 10)[0] > 240,
        "{:?}",
        pixel(&start, 10, 10)
    );
    assert!(
        pixel(&start, 1, 1)[1] > 240,
        "corner {:?}",
        pixel(&start, 1, 1)
    );
    assert_eq!(pixel(&start, 100, 60)[..3], [255, 255, 255]);
    sim.advance(Duration::from_millis(484));
    let mid = render(&mut sim, &mut exporter);
    // Halfway: x 40..70, y 0..70, purple-ish.
    let c = pixel(&mid, 55, 35);
    assert!(c[0] > 60 && c[2] > 60, "{c:?}");
    assert_eq!(pixel(&mid, 55, 100)[..3], [255, 255, 255]);
    assert_eq!(pixel(&mid, 5, 5)[..3], [255, 255, 255]);
    sim.advance(Duration::from_millis(600));
    let end = render(&mut sim, &mut exporter);
    assert!(pixel(&end, 100, 110)[2] > 250);
    assert_eq!(pixel(&end, 55, 35)[..3], [255, 255, 255]);
}

#[test]
fn scroll_to_paints_the_content_mid_glide() {
    use rndr_core::Scroll;
    let mut sim = Sim::new();
    sim.world().insert_resource(Viewport::new(120.0, 120.0));
    let port = sim.world().spawn((
        Layout {
            width: Size::Px(120.0),
            height: Size::Px(120.0),
            ..Layout::column()
        },
        Scroll::default(),
    ));
    let top = boxed(&mut sim, 120.0, 120.0, Color::rgb(1.0, 0.0, 0.0));
    let bottom = boxed(&mut sim, 120.0, 120.0, Color::rgb(0.0, 0.0, 1.0));
    set_parent(sim.world(), top, port);
    set_parent(sim.world(), bottom, port);
    let mut exporter = exporter();
    rndr_paint::frame(&mut sim.app);
    motion::scroll_to(
        sim.world(),
        port,
        [0.0, -120.0],
        Curve::tween(1.0, Easing::Linear),
    );
    sim.advance(Duration::from_millis(500));
    let mid = render(&mut sim, &mut exporter);
    assert_eq!(pixel(&mid, 60, 50)[..3], [255, 0, 0]);
    assert_eq!(pixel(&mid, 60, 70)[..3], [0, 0, 255]);
    sim.advance(Duration::from_millis(600));
    let end = render(&mut sim, &mut exporter);
    assert_eq!(pixel(&end, 60, 5)[..3], [0, 0, 255]);
}
