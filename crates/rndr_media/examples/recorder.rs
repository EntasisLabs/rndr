//! Records the default input (or a test tone with `--tone`) with a live level
//! meter. Click the window to start and stop; each take plays back once.
//!
//! `cargo run -p rndr_media --example recorder -- [--tone] [out.ogg]`
//! (`out.m4a` or `out.wav` on macOS)
use rndr_core::{
    set_parent, App, Color, Input, Label, Layout, Live, LiveLabel, LiveScale, LiveText, Size,
    Transform, WindowConfig, World,
};
use rndr_media::{Player, RecordSource, Recorder, RecorderState};

struct Session {
    rec: Recorder,
    status: LiveText,
    last: Option<Player>,
}

fn toggle(world: &mut World) {
    let pressed = world
        .resource::<Input>()
        .is_some_and(|input| input.pointer.pressed && input.pointer.press_at.is_some());
    if !pressed {
        return;
    }
    let Some(session) = world.resource_mut::<Session>() else {
        return;
    };
    if session.rec.is_recording() {
        match session.rec.stop() {
            Ok(take) => {
                session.status.set(format!(
                    "Saved {:.1} s to {} - click to record again",
                    take.duration,
                    take.path.display()
                ));
                let clip = Player::audio(take.path);
                clip.play();
                session.last = Some(clip);
            }
            Err(error) => {
                session.status.set(format!("Couldn't save: {error}"));
            }
        }
    } else {
        session.rec.start();
        session.status.set("Recording - click to stop");
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let tone = args.iter().any(|a| a == "--tone");
    let path = args
        .iter()
        .find(|a| !a.starts_with("--"))
        .cloned()
        .unwrap_or_else(|| {
            if cfg!(target_os = "macos") {
                "take.m4a".into()
            } else {
                "take.ogg".into()
            }
        });
    let source = if tone {
        RecordSource::TestTone(330.0)
    } else {
        RecordSource::Microphone
    };
    let rec = Recorder::with_source(path, source);
    let meter = Live::new([0.0_f32, 1.0]);
    {
        let level = rec.live_level();
        let meter = meter.clone();
        let rec = rec.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(16));
            meter.set([level.get().max(0.01), 1.0]);
            if let RecorderState::Failed(error) = rec.state() {
                eprintln!("recorder failed: {error}");
            }
        });
    }
    let status = LiveText::new("Click to record");

    let mut app = App::new();
    app.world_mut().insert_resource(WindowConfig {
        title: "rndr recorder".into(),
        ..Default::default()
    });
    let world = app.world_mut();
    let root = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            padding: 40.0,
            gap: 24.0,
            ..Layout::column()
        },
        Color::rgb(0.1, 0.1, 0.12),
    ));
    let line = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(28.0),
            ..Layout::column()
        },
        Label::new("Click to record").with_color(Color::rgb(0.95, 0.95, 0.97)),
    ));
    world.insert(line, LiveLabel::new(&status));
    let track = world.spawn((
        Layout {
            width: Size::Px(400.0),
            height: Size::Px(24.0),
            ..Layout::row()
        },
        Color::rgb(0.2, 0.2, 0.24),
    ));
    let bar = world.spawn((
        Layout {
            width: Size::Px(400.0),
            height: Size::Px(24.0),
            ..Layout::row()
        },
        Color::rgb(0.3, 0.85, 0.5),
        Transform::IDENTITY.with_origin(0.0, 0.5),
    ));
    world.insert(bar, LiveScale::new(&meter));
    set_parent(world, line, root);
    set_parent(world, track, root);
    set_parent(world, bar, track);
    world.insert_resource(Session {
        rec,
        status,
        last: None,
    });
    app.add_system(rndr_core::Schedule::Update, toggle);
    rndr_render::run(app);
}
