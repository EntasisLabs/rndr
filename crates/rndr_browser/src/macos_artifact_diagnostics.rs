//! Opt-in, bounded native-only artifact probe. No global event tap, guest
//! bridge, synchronous per-event I/O, or change to event delivery.
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::{mpsc, Arc, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{msg_send, ClassType};
use objc2_app_kit::{NSEvent, NSEventMask, NSEventModifierFlags, NSEventType, NSView};
use objc2_web_kit::WKWebView;

const LIMIT: u64 = 20_000;
const LIFETIME: Duration = Duration::from_secs(600);
const QUEUE: usize = 256;
static SESSION: OnceLock<Option<Trace>> = OnceLock::new();
static NEXT_PREVIEW: AtomicU64 = AtomicU64::new(1);

struct Shared {
    sender: mpsc::SyncSender<Record>,
    started: Instant,
    dropped: AtomicU64,
    attempted: AtomicU64,
    active: AtomicBool,
}
#[derive(Clone)]
pub(crate) struct Trace {
    shared: Arc<Shared>,
    preview: u64,
}
struct Record {
    seconds: f64,
    preview: u64,
    event: Event,
}
// Fixed-size values only. No text from the guest, URL, key text or HTML.
enum Event {
    Lifecycle(&'static str),
    Wheel {
        timestamp: f64,
        delta: [f64; 3],
        precise: bool,
        phase: usize,
        momentum: usize,
        inverted: bool,
        position: [f64; 2],
        window: isize,
    },
    Geometry {
        rect: [f32; 4],
        visible: bool,
        scale: f64,
        window: isize,
    },
}
impl Trace {
    pub(crate) fn for_preview() -> Option<Self> {
        let base = SESSION.get_or_init(|| {
            let dir = std::env::var_os("RNDR_ARTIFACT_DIAGNOSTICS")?;
            match start(PathBuf::from(dir)) {
                Ok(trace) => Some(trace),
                Err(error) => {
                    eprintln!("RNDR artifact diagnostics disabled: {error}");
                    None
                }
            }
        });
        base.as_ref().map(|trace| Self {
            shared: trace.shared.clone(),
            preview: NEXT_PREVIEW.fetch_add(1, Relaxed),
        })
    }
    fn emit(&self, event: Event) {
        if !self.shared.active.load(Relaxed) {
            return;
        }
        let n = self.shared.attempted.fetch_add(1, Relaxed);
        if n >= LIMIT || self.shared.started.elapsed() >= LIFETIME {
            return;
        }
        let record = Record {
            seconds: self.shared.started.elapsed().as_secs_f64(),
            preview: self.preview,
            event,
        };
        if self.shared.sender.try_send(record).is_err() {
            self.shared.dropped.fetch_add(1, Relaxed);
        }
    }
    pub(crate) fn lifecycle(&self, name: &'static str) {
        self.emit(Event::Lifecycle(name));
    }
    pub(crate) fn geometry(
        &self,
        rect: crate::ClientRect,
        visible: bool,
        scale: f64,
        window: isize,
    ) {
        self.emit(Event::Geometry {
            rect: [rect.x, rect.y, rect.width, rect.height],
            visible,
            scale,
            window,
        });
    }
}
fn start(dir: PathBuf) -> std::io::Result<Trace> {
    if !dir.is_absolute() || !dir.is_dir() {
        return Err(std::io::Error::other(
            "RNDR_ARTIFACT_DIAGNOSTICS must name an existing absolute private directory",
        ));
    }
    if std::fs::metadata(&dir)?.permissions().mode() & 0o077 != 0 {
        return Err(std::io::Error::other(
            "diagnostic directory must be private (chmod 700)",
        ));
    }
    let path = dir.join(format!("artifact-{}.jsonl", std::process::id()));
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)?;
    let (sender, receiver) = mpsc::sync_channel(QUEUE);
    let shared = Arc::new(Shared {
        sender,
        started: Instant::now(),
        dropped: AtomicU64::new(0),
        attempted: AtomicU64::new(0),
        active: AtomicBool::new(true),
    });
    let writer_shared = shared.clone();
    std::thread::Builder::new()
        .name("rndr-artifact-probe".into())
        .spawn(move || {
            if let Err(error) = write_session(file, receiver, &writer_shared, dir, LIMIT) {
                eprintln!("RNDR artifact diagnostics stopped after I/O failure: {error}");
            }
            writer_shared.active.store(false, Relaxed);
        })?;
    eprintln!(
        "RNDR artifact diagnostics: {} (600s / 20000 events maximum)",
        path.display()
    );
    Ok(Trace { shared, preview: 0 })
}
fn write_session(
    file: File,
    receiver: mpsc::Receiver<Record>,
    shared: &Shared,
    dir: PathBuf,
    record_limit: u64,
) -> std::io::Result<()> {
    let mut out = BufWriter::new(file);
    let unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    writeln!(out, "{{\"kind\":\"session\",\"schema\":1,\"pid\":{},\"unix_seconds\":{unix},\"limit\":{record_limit},\"duration_seconds\":600,\"queue_capacity\":{QUEUE},\"guest_telemetry\":false,\"delivery\":\"appkit_local_pre_dispatch_not_webkit_consumption\"}}", std::process::id())?;
    out.flush()?;
    let mut last_flush = Instant::now();
    let mut marker = None;
    let mut written = 0u64;
    let reason = loop {
        if written >= record_limit {
            break "record_limit";
        }
        if shared.started.elapsed() >= LIFETIME {
            break "deadline";
        }
        let stamp = std::fs::metadata(dir.join("diagnostic.marker"))
            .and_then(|m| m.modified())
            .ok();
        if stamp.is_some() && stamp != marker {
            writeln!(
                out,
                "{{\"kind\":\"external_marker\",\"seconds\":{},\"preview\":null}}",
                shared.started.elapsed().as_secs_f64()
            )?;
            marker = stamp;
            written += 1;
        }
        if written >= record_limit {
            break "record_limit";
        }
        if dir.join("diagnostic.stop").exists() {
            break "stop_file";
        }
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(record) => {
                write_record(&mut out, record)?;
                written += 1;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break "disconnected",
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if shared.attempted.load(Relaxed) >= LIMIT {
                    break "attempt_limit";
                }
            }
        }
        if last_flush.elapsed() >= Duration::from_millis(250) {
            out.flush()?;
            last_flush = Instant::now();
        }
    };
    shared.active.store(false, Relaxed);
    // Stop is a flush barrier for already queued records. Never exceed the
    // hard record bound; a racing producer can add at most one final record
    // per monitor, accounted for as pending_discarded if it misses this drain.
    let mut discarded = 0usize;
    for record in receiver.try_iter().take(QUEUE) {
        if written < record_limit {
            write_record(&mut out, record)?;
            written += 1;
        } else {
            discarded += 1;
        }
    }
    writeln!(out, "{{\"kind\":\"summary\",\"seconds\":{},\"reason\":\"{reason}\",\"written\":{written},\"attempted\":{},\"queue_dropped\":{},\"pending_discarded\":{}}}", shared.started.elapsed().as_secs_f64(), shared.attempted.load(Relaxed), shared.dropped.load(Relaxed), discarded + receiver.try_iter().count())?;
    out.flush()
}
fn finite(value: f64) -> f64 {
    if value.is_finite() {
        value
    } else {
        0.0
    }
}
fn write_record(out: &mut impl Write, record: Record) -> std::io::Result<()> {
    write!(
        out,
        "{{\"seconds\":{},\"preview\":{},",
        record.seconds, record.preview
    )?;
    match record.event {
        Event::Lifecycle(name) => write!(out, "\"kind\":\"{name}\"")?,
        Event::Geometry { rect, visible, scale, window } => write!(out, "\"kind\":\"geometry\",\"rect_points\":[{},{},{},{}],\"visible\":{visible},\"scale\":{},\"window\":{window}", rect[0], rect[1], rect[2], rect[3], finite(scale))?,
        Event::Wheel { timestamp, delta, precise, phase, momentum, inverted, position, window } => write!(out, "\"kind\":\"wheel\",\"event_uptime\":{},\"delta\":[{},{}],\"legacy_delta_z\":{},\"precise\":{precise},\"units\":\"{}\",\"phase\":{phase},\"momentum\":{momentum},\"inverted\":{inverted},\"location_window_points\":[{},{}],\"window\":{window}", finite(timestamp), finite(delta[0]), finite(delta[1]), finite(delta[2]), if precise { "points" } else { "lines" }, finite(position[0]), finite(position[1]))?,
    }
    writeln!(out, "}}")
}

pub(crate) struct Probe {
    monitor: Option<Retained<AnyObject>>,
    pub(crate) trace: Option<Trace>,
}
impl Probe {
    pub(crate) fn new(web: Retained<WKWebView>) -> Self {
        let trace = Trace::for_preview();
        let mut probe = Self {
            monitor: None,
            trace,
        };
        let Some(trace) = probe.trace.clone() else {
            return probe;
        };
        trace.lifecycle("attach");
        let Some(mtm) = objc2_foundation::MainThreadMarker::new() else {
            return probe;
        };
        let callback_trace = trace.clone();
        let handler = RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
            let key = unsafe { event.as_ref() };
            if !callback_trace.shared.active.load(Relaxed) {
                return event.as_ptr();
            }
            // Hit-test the actual native subtree, not merely the webview bounds.
            // Host overlays and other previews in the same window are excluded.
            let Some(window) = (unsafe { key.window(mtm) }) else {
                return event.as_ptr();
            };
            if web.window().as_ref() != Some(&window) {
                return event.as_ptr();
            }
            let is_wheel = unsafe { key.r#type() } == NSEventType::ScrollWheel;
            let target = if is_wheel {
                window.contentView().and_then(|content| {
                    let point = unsafe { content.superview() }
                        .map(|parent| unsafe {
                            parent.convertPoint_fromView(key.locationInWindow(), None)
                        })
                        .unwrap_or_else(|| unsafe { key.locationInWindow() });
                    unsafe { content.hitTest(point) }
                })
            } else {
                window.firstResponder().and_then(|responder| {
                    let is_view: bool =
                        unsafe { msg_send![&responder, isKindOfClass: NSView::class()] };
                    if is_view {
                        unsafe {
                            Retained::retain(
                                Retained::as_ptr(&responder).cast_mut().cast::<NSView>(),
                            )
                        }
                    } else {
                        None
                    }
                })
            };
            if !target.is_some_and(|target| unsafe { target.isDescendantOf(&web) }) {
                return event.as_ptr();
            }
            if is_wheel {
                let point = unsafe { key.locationInWindow() };
                callback_trace.emit(Event::Wheel {
                    timestamp: unsafe { key.timestamp() },
                    delta: [
                        unsafe { key.scrollingDeltaX() },
                        unsafe { key.scrollingDeltaY() },
                        unsafe { key.deltaZ() },
                    ],
                    precise: unsafe { key.hasPreciseScrollingDeltas() },
                    phase: unsafe { key.phase() }.0,
                    momentum: unsafe { key.momentumPhase() }.0,
                    inverted: unsafe { key.isDirectionInvertedFromDevice() },
                    position: [point.x, point.y],
                    window: unsafe { key.windowNumber() },
                });
            } else {
                // Physical M key (ANSI keycode 46), exact Cmd+Shift. Never read
                // characters or log arbitrary keys; event is returned untouched.
                let flags = unsafe { key.modifierFlags() };
                let relevant = NSEventModifierFlags::NSEventModifierFlagCommand
                    | NSEventModifierFlags::NSEventModifierFlagShift
                    | NSEventModifierFlags::NSEventModifierFlagControl
                    | NSEventModifierFlags::NSEventModifierFlagOption;
                let chord = NSEventModifierFlags::NSEventModifierFlagCommand
                    | NSEventModifierFlags::NSEventModifierFlagShift;
                if unsafe { key.keyCode() } == 46
                    && flags & relevant == chord
                    && !unsafe { key.isARepeat() }
                {
                    callback_trace.lifecycle("flash_marker");
                }
            }
            event.as_ptr()
        });
        probe.monitor = unsafe {
            NSEvent::addLocalMonitorForEventsMatchingMask_handler(
                NSEventMask::ScrollWheel | NSEventMask::KeyDown,
                &handler,
            )
        };
        trace.lifecycle(if probe.monitor.is_some() {
            "local_monitor_ready"
        } else {
            "local_monitor_unavailable"
        });
        probe
    }
}
impl Drop for Probe {
    fn drop(&mut self) {
        if let Some(monitor) = &self.monitor {
            unsafe { NSEvent::removeMonitor(monitor) };
        }
        if let Some(trace) = &self.trace {
            trace.lifecycle("detach");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn channel(capacity: usize) -> (Trace, mpsc::Receiver<Record>) {
        let (sender, receiver) = mpsc::sync_channel(capacity);
        (
            Trace {
                shared: Arc::new(Shared {
                    sender,
                    started: Instant::now(),
                    dropped: AtomicU64::new(0),
                    attempted: AtomicU64::new(0),
                    active: AtomicBool::new(true),
                }),
                preview: 7,
            },
            receiver,
        )
    }
    #[test]
    fn queue_never_blocks_and_counts_dropped_records() {
        let (trace, receiver) = channel(1);
        trace.lifecycle("attach");
        trace.lifecycle("ready");
        assert_eq!(trace.shared.dropped.load(Relaxed), 1);
        assert_eq!(receiver.try_iter().count(), 1);
        trace.shared.active.store(false, Relaxed);
        trace.lifecycle("detach");
        assert_eq!(trace.shared.attempted.load(Relaxed), 2);
    }
    #[test]
    fn event_limit_is_hard_even_without_writer() {
        let (trace, receiver) = channel(1);
        trace.shared.attempted.store(LIMIT, Relaxed);
        trace.lifecycle("ready");
        assert!(receiver.try_recv().is_err());
    }
    #[test]
    fn serialization_is_one_line_finite_and_no_guest_content() {
        let mut output = Vec::new();
        write_record(
            &mut output,
            Record {
                seconds: 0.1,
                preview: 2,
                event: Event::Wheel {
                    timestamp: 100.0,
                    delta: [1.0, -2.0, f64::NAN],
                    precise: false,
                    phase: 0,
                    momentum: 0,
                    inverted: false,
                    position: [2.0, 3.0],
                    window: 10,
                },
            },
        )
        .unwrap();
        let output = String::from_utf8(output).unwrap();
        assert_eq!(output.lines().count(), 1);
        assert!(output.contains("\"units\":\"lines\""));
        assert!(output.contains("\"legacy_delta_z\":0"));
        assert!(!output.contains("NaN"));
    }
    #[test]
    fn explicit_stop_drains_queue_and_flushes_summary_and_marker() {
        let dir = std::env::temp_dir().join(format!(
            "rndr-diagnostic-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("test.jsonl");
        let file = File::create(&path).unwrap();
        let (trace, receiver) = channel(4);
        trace.lifecycle("attach");
        trace.lifecycle("detach");
        std::fs::write(dir.join("diagnostic.marker"), []).unwrap();
        std::fs::write(dir.join("diagnostic.stop"), []).unwrap();
        write_session(file, receiver, &trace.shared, dir.clone(), LIMIT).unwrap();
        let log = std::fs::read_to_string(path).unwrap();
        assert!(log.contains("external_marker"));
        assert!(log.contains("\"kind\":\"detach\""));
        assert!(log.contains("\"reason\":\"stop_file\""));
        assert!(log.contains("\"queue_dropped\":0"));
        assert!(log.contains("\"pending_discarded\":0"));
        assert!(!trace.shared.active.load(Relaxed));
        assert_eq!(log.lines().count(), 5);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn record_limit_accounts_for_every_discarded_queued_event() {
        let dir =
            std::env::temp_dir().join(format!("rndr-diagnostic-cap-test-{}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("test.jsonl");
        let (trace, receiver) = channel(4);
        for _ in 0..4 {
            trace.lifecycle("ready");
        }
        let file = File::create(&path).unwrap();
        write_session(file, receiver, &trace.shared, dir.clone(), 2).unwrap();
        let log = std::fs::read_to_string(path).unwrap();
        assert_eq!(log.lines().count(), 4); // header + 2 records + summary
        assert!(log.contains("\"written\":2"));
        assert!(log.contains("\"attempted\":4"));
        assert!(log.contains("\"reason\":\"record_limit\""));
        assert!(log.contains("\"pending_discarded\":2"));
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn refuses_relative_missing_or_nonprivate_directory() {
        assert!(start(PathBuf::from("relative")).is_err());
        let dir = std::env::temp_dir().join(format!(
            "rndr-public-diagnostic-test-{}",
            std::process::id()
        ));
        std::fs::create_dir(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(start(dir.clone()).is_err());
        std::fs::remove_dir(dir).unwrap();
    }
}
