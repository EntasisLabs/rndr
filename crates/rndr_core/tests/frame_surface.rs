//! `FrameSurface`: validation, placement, wakes and stats.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use rndr_core::{
    FrameError, FrameSurface, Layout, Rect, Sim, Size, SurfaceFit, SurfaceView, VideoFrame,
    Viewport, Wake, WindowPhase,
};

#[derive(Default)]
struct CountWake(AtomicUsize);

impl Wake for CountWake {
    fn wake(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

fn red(w: u32, h: u32) -> VideoFrame {
    VideoFrame::rgba(w, h, [255, 0, 0, 255].repeat((w * h) as usize))
}

fn sim_with_view(surface: &FrameSurface) -> (Sim, Arc<CountWake>) {
    let mut sim = Sim::new();
    sim.world().insert_resource(Viewport::new(200.0, 100.0));
    let wake = Arc::new(CountWake::default());
    sim.app.set_live_wake(wake.clone());
    sim.world().spawn((
        Layout {
            width: Size::Px(80.0),
            height: Size::Px(40.0),
            ..Layout::column()
        },
        SurfaceView::new(surface),
    ));
    sim.update();
    (sim, wake)
}

#[test]
fn malformed_frames_are_refused_and_keep_the_old_frame() {
    let surface = FrameSurface::new();
    assert_eq!(
        surface.push(VideoFrame::rgba(0, 4, Vec::new())),
        Err(FrameError::Empty)
    );
    surface.push(red(2, 2)).unwrap();
    let short = VideoFrame::nv12(4, 3, vec![0; 12], vec![0; 7]);
    assert_eq!(
        surface.push(short),
        Err(FrameError::PlaneSize {
            plane: 1,
            expected: 8,
            actual: 7
        })
    );
    assert_eq!(surface.size(), Some([2, 2]));
    assert_eq!(surface.stats().pushed, 1);
}

#[test]
fn odd_nv12_sizes_round_chroma_up() {
    let frame = VideoFrame::nv12(5, 3, vec![0; 15], vec![0; 12]);
    assert_eq!(frame.plane_sizes(), [15, 12]);
    assert!(frame.validate().is_ok());
}

#[test]
fn placement_contains_covers_and_stretches() {
    let surface = FrameSurface::new();
    surface.push(red(8, 4)).unwrap();
    let rect = Rect {
        x: 10.0,
        y: 0.0,
        w: 40.0,
        h: 40.0,
    };
    let view = SurfaceView::new(&surface);
    let (dest, uv) = view.placement(rect).unwrap();
    assert_eq!((dest.x, dest.y, dest.w, dest.h), (10.0, 10.0, 40.0, 20.0));
    assert_eq!(uv, [0.0, 0.0, 1.0, 1.0]);
    let (dest, uv) = view
        .clone()
        .with_fit(SurfaceFit::Cover)
        .placement(rect)
        .unwrap();
    assert_eq!(dest, rect);
    assert_eq!(uv, [0.25, 0.0, 0.75, 1.0]);
    let (dest, uv) = view.with_fit(SurfaceFit::Stretch).placement(rect).unwrap();
    assert_eq!((dest, uv), (rect, [0.0, 0.0, 1.0, 1.0]));
    assert!(SurfaceView::new(&FrameSurface::new())
        .placement(rect)
        .is_none());
}

#[test]
fn pushes_wake_a_drawing_window_once_per_frame() {
    let surface = FrameSurface::new();
    let (mut sim, wake) = sim_with_view(&surface);
    let producer = {
        let surface = surface.clone();
        std::thread::spawn(move || {
            for _ in 0..50 {
                surface.push(red(2, 2)).unwrap();
            }
        })
    };
    producer.join().unwrap();
    assert_eq!(wake.0.load(Ordering::SeqCst), 1);
    assert!(rndr_core::live::pending(sim.world()));
    sim.app.update_live();
    assert!(!rndr_core::live::pending(sim.world()));
    surface.clear();
    assert_eq!(wake.0.load(Ordering::SeqCst), 2);
    surface.clear();
    assert_eq!(
        wake.0.load(Ordering::SeqCst),
        2,
        "clearing an empty surface"
    );
}

#[test]
fn hidden_windows_are_not_woken() {
    let surface = FrameSurface::new();
    let (mut sim, wake) = sim_with_view(&surface);
    sim.world().insert_resource(WindowPhase {
        visible: false,
        ..WindowPhase::default()
    });
    sim.update();
    surface.push(red(2, 2)).unwrap();
    assert_eq!(wake.0.load(Ordering::SeqCst), 0);
    assert!(!rndr_core::live::pending(sim.world()));
}

#[test]
fn renderers_count_shown_frames() {
    let surface = FrameSurface::new();
    assert!(surface.latest_after(0).is_none());
    surface.push(red(2, 2)).unwrap();
    surface.push(red(4, 4)).unwrap();
    let (version, frame) = surface.latest_after(0).unwrap();
    assert_eq!((version, frame.unwrap().width), (2, 4));
    assert!(surface.latest_after(version).is_none());
    assert_eq!(surface.stats().pushed, 2);
    assert_eq!(surface.stats().shown, 1);
    surface.clear();
    let (_, frame) = surface.latest_after(version).unwrap();
    assert!(frame.is_none());
    assert_eq!(surface.stats().shown, 1);
}
