//! Synthetic native close probe. No live notes. See docs/safe-close.md.
//! Run with an EMPTY scratch directory and drive `command` with `SEQ:retry`,
//! `SEQ:cancel`, `SEQ:discard`, `SEQ:native-close`, or `SEQ:native-quit`.
use rndr_core::{
    events, every, spawn, App, CloseGuard, CloseRequest, CloseState, Color, Event, Label, Schedule,
    WindowConfig, World,
};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const TEXT: &str = "Synthetic unsaved draft α\nexact bytes, no live notes\n";
#[derive(Clone)]
struct Tick;
impl Event for Tick {}
#[derive(Clone)]
struct Completed(CloseRequest, Result<(), String>);
impl Event for Completed {}
struct Probe {
    root: PathBuf,
    command: String,
    started: Option<CloseRequest>,
    succeed: bool,
    since: Instant,
    last: String,
}

#[cfg(target_os = "macos")]
fn native(action: &str) {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    // The probe runs this on the UI thread. These call AppKit's actual native
    // delegate paths, not core close helpers or synthesized CloseRequested.
    unsafe {
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        if action == "native-quit" {
            let _: () = msg_send![app, terminate: std::ptr::null::<AnyObject>()];
        } else {
            let windows: *mut AnyObject = msg_send![app, windows];
            let window: *mut AnyObject = msg_send![windows, firstObject];
            assert!(!window.is_null(), "native window exists");
            let _: () = msg_send![window, performClose: std::ptr::null::<AnyObject>()];
        }
    }
}
#[cfg(not(target_os = "macos"))]
fn native(_: &str) {
    panic!("native probe requires macOS");
}

fn update(world: &mut World) {
    let completed = events::<Completed>(world).to_vec();
    for Completed(request, result) in completed {
        let accepted = world
            .resource_mut::<CloseGuard>()
            .unwrap()
            .complete(request, result);
        eprintln!("completion {request:?} accepted={accepted}");
    }
    let tick = !events::<Tick>(world).is_empty();
    if !tick {
        return;
    }
    let mut p = world
        .resource_mut::<Option<Probe>>()
        .unwrap()
        .take()
        .unwrap();
    assert!(
        p.since.elapsed() < Duration::from_secs(30),
        "native probe timed out"
    );
    assert_eq!(
        world.resource::<String>().unwrap(),
        TEXT,
        "in-memory bytes retained"
    );
    let command = std::fs::read_to_string(p.root.join("command")).unwrap_or_default();
    if command != p.command {
        p.command = command.clone();
        let action = command.trim().split_once(':').map(|(_, a)| a).unwrap_or("");
        eprintln!("command {action}");
        let guard = world.resource_mut::<CloseGuard>().unwrap();
        match (action, guard.state().clone()) {
            ("retry", CloseState::Failed(r, _)) => {
                p.succeed = true;
                guard.retry(r).unwrap();
            }
            ("cancel", _) => {
                let old = p.started;
                guard.cancel();
                if let Some(old) = old {
                    assert!(!guard.complete(old, Ok(())), "cancel fences old result");
                }
            }
            ("discard", CloseState::Failed(r, _)) => {
                assert!(guard.discard(r));
            }
            ("hide", _) => {
                rndr_core::window::hide(world);
            }
            ("quit", _) => {
                world.resource_mut::<rndr_core::Input>().unwrap().quit = true;
            }
            ("native-close" | "native-quit", _) => {
                native(action);
            }
            _ => {}
        }
    }
    let state = world.resource::<CloseGuard>().unwrap().state().clone();
    if let CloseState::Pending(r) = state {
        if p.started != Some(r) {
            p.started = Some(r);
            let root = p.root.clone();
            let succeed = p.succeed;
            spawn(world, async move {
                // Real background work, long enough to exercise pending double-close.
                std::thread::sleep(Duration::from_millis(600));
                let result = (|| -> std::io::Result<()> {
                    use std::io::Write;
                    let path = if succeed {
                        root.join("recovery")
                    } else {
                        root.join("blocker").join("recovery")
                    };
                    let mut file = std::fs::File::create(path)?;
                    file.write_all(TEXT.as_bytes())?;
                    file.sync_all()
                })()
                .map_err(|e| e.to_string());
                Completed(r, result)
            })
            .detach();
        }
    }
    let shown = format!("{state:?}");
    if shown != p.last {
        eprintln!("state={shown} draft_exact=true");
        p.last = shown.clone();
        std::fs::write(p.root.join("state"), &shown).unwrap();
    }
    *world.resource_mut::<Option<Probe>>().unwrap() = Some(p);
}

fn main() {
    let root = PathBuf::from(std::env::args_os().nth(1).expect("empty scratch directory"));
    assert!(
        root.is_dir() && root.read_dir().unwrap().next().is_none(),
        "scratch must be empty"
    );
    std::fs::write(root.join("blocker"), b"not a directory").unwrap();
    let ordinary = std::env::args().any(|a| a == "--ordinary");
    let mut app = App::new();
    app.world_mut().insert_resource(WindowConfig {
        title: "rndr synthetic safe-close probe".into(),
        ..Default::default()
    });
    app.world_mut()
        .spawn((Label::new(TEXT), Color::rgb(0.2, 0.7, 0.8)));
    if ordinary {
        // Negative control: the legacy opt-out behavior still exits after a
        // recovery error. Only synthetic text is deliberately lost here.
        let failure = std::fs::write(root.join("blocker").join("recovery"), TEXT.as_bytes());
        assert!(failure.is_err());
        eprintln!(
            "unguarded control: recovery failed; draft in memory only; native close is unprotected"
        );
        // No guard is installed at all for ordinary apps.
        app.add_system(Schedule::Update, |world| {
            let p = world.resource::<Option<Probe>>().unwrap().as_ref().unwrap();
            if let Ok(c) = std::fs::read_to_string(p.root.join("command")) {
                if c.contains("native-close") {
                    native("native-close");
                }
            }
        });
    } else {
        app.add_system(Schedule::Update, update);
    }
    if !ordinary {
        app.world_mut().insert_resource(CloseGuard::default());
    }
    app.world_mut().insert_resource(TEXT.to_string());
    app.world_mut().insert_resource(Some(Probe {
        root: root.clone(),
        command: String::new(),
        started: None,
        succeed: false,
        since: Instant::now(),
        last: String::new(),
    }));
    let _tick = every(app.world_mut(), Duration::from_millis(100), Tick);
    eprintln!("probe ready ordinary={ordinary}");
    rndr_render::run(app);
    eprintln!("run returned");
    if let Ok(recovered) = std::fs::read(root.join("recovery")) {
        assert_eq!(recovered, TEXT.as_bytes());
    }
}
