//! Pixels for `SurfaceView`: formats, colour standards, fit, mirror, groups.

use rndr_core::{
    set_parent, App, Color, FrameSurface, Layout, Opacity, Size, SurfaceFit, SurfaceView,
    VideoFrame, Viewport, YuvMatrix, YuvRange,
};
use rndr_render::{ExportError, Exporter, RgbaImage};

fn exporter() -> Exporter {
    match Exporter::new() {
        Ok(exporter) => exporter,
        Err(ExportError::NoAdapter) => panic!("no graphics adapter"),
        Err(err) => panic!("{err}"),
    }
}

/// A black 40x40 root holding a 40x40 view of `surface` that has its own
/// black fill, so the frame must draw above the fill.
fn app_with(view: SurfaceView, opacity: Option<f32>) -> App {
    let mut app = App::new();
    app.world_mut().insert_resource(Viewport::new(40.0, 40.0));
    let fixed = || Layout {
        width: Size::Px(40.0),
        height: Size::Px(40.0),
        ..Layout::row()
    };
    let root = app.world_mut().spawn((fixed(), Color::rgb(0.0, 0.0, 0.0)));
    let child = app
        .world_mut()
        .spawn((fixed(), Color::rgb(0.0, 0.0, 0.0), view));
    if let Some(opacity) = opacity {
        app.world_mut().insert(child, Opacity(opacity));
    }
    set_parent(app.world_mut(), child, root);
    app
}

fn draw(exporter: &mut Exporter, app: &mut App) -> RgbaImage {
    let frame = rndr_paint::frame(app);
    exporter
        .render_frame(Some(app.world()), &frame, 40, 40, 1.0)
        .expect("render")
}

fn pixel(image: &RgbaImage, x: u32, y: u32) -> [u8; 4] {
    let index = ((y * image.width + x) * 4) as usize;
    image.rgba[index..index + 4].try_into().unwrap()
}

fn near(actual: [u8; 4], expected: [u8; 3], tolerance: u8) {
    for channel in 0..3 {
        assert!(
            actual[channel].abs_diff(expected[channel]) <= tolerance,
            "{actual:?} is not within {tolerance} of {expected:?}"
        );
    }
}

fn solid(surface: &FrameSurface, frame: VideoFrame) -> [u8; 4] {
    surface.push(frame).unwrap();
    let mut app = app_with(
        SurfaceView::new(surface).with_fit(SurfaceFit::Stretch),
        None,
    );
    pixel(&draw(&mut exporter(), &mut app), 20, 20)
}

fn nv12(y: u8, cb: u8, cr: u8) -> VideoFrame {
    VideoFrame::nv12(4, 4, vec![y; 16], [cb, cr].repeat(4))
}

#[test]
fn rgba_and_bgra_frames_draw_their_colours() {
    let surface = FrameSurface::new();
    near(
        solid(
            &surface,
            VideoFrame::rgba(4, 4, [200, 100, 30, 255].repeat(16)),
        ),
        [200, 100, 30],
        1,
    );
    near(
        solid(
            &surface,
            VideoFrame::bgra(4, 4, [30, 100, 200, 255].repeat(16)),
        ),
        [200, 100, 30],
        1,
    );
}

#[test]
fn nv12_follows_matrix_and_range() {
    let surface = FrameSurface::new();
    // BT.709 video-range red, and mid grey: luma 126 expands to 128.
    near(solid(&surface, nv12(63, 102, 240)), [255, 0, 0], 4);
    near(solid(&surface, nv12(126, 128, 128)), [128, 128, 128], 1);
    // BT.601 full-range red, as JPEG and many cameras produce.
    let full = nv12(76, 85, 255).with_yuv(YuvMatrix::Bt601, YuvRange::Full);
    near(solid(&surface, full), [255, 0, 0], 4);
    // Luma 16 is black in video range and dark grey in full range.
    near(solid(&surface, nv12(16, 128, 128)), [0, 0, 0], 1);
    let full = nv12(16, 128, 128).with_yuv(YuvMatrix::Bt709, YuvRange::Full);
    near(solid(&surface, full), [16, 16, 16], 1);
}

#[test]
fn contain_leaves_bars_and_mirror_flips() {
    let surface = FrameSurface::new();
    // Left half red, right half blue, twice as wide as tall.
    let mut bytes = Vec::new();
    for _ in 0..2 {
        for x in 0..4 {
            bytes.extend(if x < 2 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            });
        }
    }
    surface.push(VideoFrame::rgba(4, 2, bytes)).unwrap();
    let mut gpu = exporter();
    let mut app = app_with(SurfaceView::new(&surface), None);
    let image = draw(&mut gpu, &mut app);
    near(pixel(&image, 20, 4), [0, 0, 0], 1);
    near(pixel(&image, 5, 20), [255, 0, 0], 1);
    near(pixel(&image, 35, 20), [0, 0, 255], 1);
    let mut app = app_with(SurfaceView::new(&surface).with_mirrored(true), None);
    let image = draw(&mut gpu, &mut app);
    near(pixel(&image, 5, 20), [0, 0, 255], 1);
    near(pixel(&image, 35, 20), [255, 0, 0], 1);
}

#[test]
fn a_new_frame_replaces_the_drawn_one_and_clear_hides_it() {
    let surface = FrameSurface::new();
    surface
        .push(VideoFrame::rgba(2, 2, [255, 0, 0, 255].repeat(4)))
        .unwrap();
    let mut gpu = exporter();
    let mut app = app_with(SurfaceView::new(&surface), None);
    near(pixel(&draw(&mut gpu, &mut app), 20, 20), [255, 0, 0], 1);
    surface.push(nv12(126, 128, 128)).unwrap();
    near(pixel(&draw(&mut gpu, &mut app), 20, 20), [128, 128, 128], 1);
    surface.clear();
    near(pixel(&draw(&mut gpu, &mut app), 20, 20), [0, 0, 0], 1);
    assert_eq!(surface.stats().shown, 2);
}

#[test]
fn group_opacity_fades_the_frame() {
    let surface = FrameSurface::new();
    surface
        .push(VideoFrame::rgba(2, 2, [255, 255, 255, 255].repeat(4)))
        .unwrap();
    let mut app = app_with(SurfaceView::new(&surface), Some(0.5));
    let faded = pixel(&draw(&mut exporter(), &mut app), 20, 20);
    assert!((100..230).contains(&faded[0]), "{faded:?}");
}

#[test]
fn rounded_corners_cut_the_frame() {
    let surface = FrameSurface::new();
    surface
        .push(VideoFrame::rgba(2, 2, [255, 255, 255, 255].repeat(4)))
        .unwrap();
    let mut app = app_with(SurfaceView::new(&surface).with_radius(12.0), None);
    let image = draw(&mut exporter(), &mut app);
    near(pixel(&image, 0, 0), [0, 0, 0], 1);
    near(pixel(&image, 20, 20), [255, 255, 255], 1);
}
