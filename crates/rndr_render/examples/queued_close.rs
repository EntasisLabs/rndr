//! Real event-loop queued-participant regression. Synthetic drafts only.
use rndr_core::{
    events, every, App, CloseGuard, CloseState, Event, Input, Schedule, WindowConfig, World,
};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
const TEXT: &str = "synthetic unsaved late window bytes α\n";
#[derive(Clone)]
struct Tick;
impl Event for Tick {}
#[derive(Default)]
struct Counts {
    built: usize,
    authorized: usize,
    dropped: usize,
    unsafe_drop: bool,
}
struct Draft {
    text: String,
    authorized: bool,
    counts: Arc<Mutex<Counts>>,
}
impl Drop for Draft {
    fn drop(&mut self) {
        let mut c = self.counts.lock().unwrap();
        c.dropped += 1;
        c.unsafe_drop |= !self.authorized;
        eprintln!(
            "DRAFT_DROP authorized={} exact={}",
            self.authorized,
            self.text == TEXT
        );
    }
}
#[derive(Clone)]
struct Config {
    counts: Arc<Mutex<Counts>>,
    root: PathBuf,
    mode: String,
    hidden: bool,
    discard: bool,
}
struct Probe {
    config: Config,
    failed_at: Option<Instant>,
    queued: bool,
}
fn hide(app: &mut App, hidden: bool) {
    if hidden {
        rndr_core::window::hide(app.world_mut());
    }
}
fn queue(world: &World, config: Config) {
    rndr_core::desktop(world)
        .unwrap()
        .open(WindowConfig::default(), move |new| {
            hide(new, config.hidden);
            if config.mode == "recursive-build" {
                // Unguarded builder queues a protected leaf. All admitted guards
                // may be Ready at this point; queue admission must prevent exit.
                queue(
                    new.world(),
                    Config {
                        mode: "leaf".into(),
                        ..config
                    },
                );
                return;
            }
            config.counts.lock().unwrap().built += 1;
            new.world_mut().insert_resource(Draft {
                text: TEXT.into(),
                authorized: false,
                counts: config.counts.clone(),
            });
            new.world_mut().insert_resource(CloseGuard::default());
            new.world_mut().insert_resource(Some(Probe {
                config,
                failed_at: None,
                queued: false,
            }));
            every(new.world_mut(), Duration::from_millis(40), Tick).detach();
            new.add_system(Schedule::Update, participant);
        });
}
fn participant(world: &mut World) {
    assert_eq!(world.resource::<Draft>().unwrap().text, TEXT);
    let mut p = world
        .resource_mut::<Option<Probe>>()
        .unwrap()
        .take()
        .unwrap();
    let state = world.resource::<CloseGuard>().unwrap().state().clone();
    match state {
        CloseState::Pending(r) => {
            if p.config.mode == "recursive-frame" && !p.queued {
                p.queued = true;
                queue(
                    world,
                    Config {
                        mode: "recursive-build".into(),
                        ..p.config.clone()
                    },
                );
            }
            let failure = std::fs::write(p.config.root.join("blocker/recovery"), TEXT);
            assert!(failure.is_err());
            assert!(world
                .resource_mut::<CloseGuard>()
                .unwrap()
                .complete(r, failure.map_err(|e| e.to_string())));
            p.failed_at = Some(Instant::now());
            eprintln!("FAILED_RETAINED exact=true hidden={}", p.config.hidden);
        }
        CloseState::Failed(r, _)
            if !events::<Tick>(world).is_empty()
                && p.failed_at.unwrap().elapsed() >= Duration::from_millis(600) =>
        {
            assert_eq!(world.resource::<Draft>().unwrap().text, TEXT);
            let guard = world.resource_mut::<CloseGuard>().unwrap();
            if p.config.discard {
                assert!(guard.discard(r));
            } else {
                let retry = guard.retry(r).unwrap();
                let path = p.config.root.join(format!(
                    "protected-{}",
                    p.config.counts.lock().unwrap().authorized
                ));
                std::fs::write(&path, TEXT).unwrap();
                std::fs::File::open(&path).unwrap().sync_all().unwrap();
                assert_eq!(std::fs::read(path).unwrap(), TEXT.as_bytes());
                assert!(guard.complete(retry, Ok(())));
            }
            world.resource_mut::<Draft>().unwrap().authorized = true;
            p.config.counts.lock().unwrap().authorized += 1;
            eprintln!("AUTHORIZED discard={} exact=true", p.config.discard);
        }
        _ => {}
    }
    *world.resource_mut::<Option<Probe>>().unwrap() = Some(p);
}
fn primary(world: &mut World) {
    if let CloseState::Pending(r) = world.resource::<CloseGuard>().unwrap().state().clone() {
        let mut p = world
            .resource_mut::<Option<Probe>>()
            .unwrap()
            .take()
            .unwrap();
        if !p.queued {
            p.queued = true;
            queue(world, p.config.clone());
            assert!(world
                .resource_mut::<CloseGuard>()
                .unwrap()
                .complete(r, Ok(())));
            eprintln!("FINAL_COMPLETION_QUEUED_OPEN hidden={}", p.config.hidden);
        }
        *world.resource_mut::<Option<Probe>>().unwrap() = Some(p);
    }
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let root = PathBuf::from(&args[1]);
    assert!(root.is_dir() && root.read_dir().unwrap().next().is_none());
    std::fs::write(root.join("blocker"), b"not a directory").unwrap();
    let hidden = args[2] == "hidden";
    let counts = Arc::new(Mutex::new(Counts::default()));
    let mut app = App::new();
    hide(&mut app, hidden);
    app.world_mut().insert_resource(CloseGuard::default());
    app.world_mut().resource_mut::<Input>().unwrap().quit = true;
    app.world_mut().insert_resource(Some(Probe {
        config: Config {
            counts: counts.clone(),
            root,
            mode: args[3].clone(),
            hidden,
            discard: args[4] == "discard",
        },
        failed_at: None,
        queued: false,
    }));
    app.add_system(Schedule::Update, primary);
    rndr_render::run(app);
    let c = counts.lock().unwrap();
    assert!(c.built > 0, "queued draft must be built");
    assert!(
        !c.unsafe_drop,
        "queued draft destroyed without authorization"
    );
    assert_eq!(c.built, c.authorized);
    assert_eq!(c.built, c.dropped);
    eprintln!(
        "PASS queued native built={} authorized={} dropped={}",
        c.built, c.authorized, c.dropped
    );
}
