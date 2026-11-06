//! Opt-in read-only live observation. See `docs/native-observation.md`.
//!
//! This module never reconstructs a headless scene. Only the native runner can
//! complete a request, from the surface texture submitted in that redraw.
use rndr_core::{Component, World};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc, Arc, Mutex,
};
#[cfg_attr(not(unix), allow(unused_imports))]
use std::time::{Duration, Instant};
use std::{io, path::PathBuf};

/// Conservative image and semantic response limits (before encoding).
pub const MAX_PIXELS: u64 = 4_000_000;
/// Longest side, also bounds padded GPU readback rows on narrow/tall surfaces.
pub const MAX_SIDE: u32 = 4096;
/// Maximum UTF-8 metadata bytes in one response.
pub const MAX_JSON: usize = 1_048_576;
/// Mark any entity containing sensitive data. Denies the ENTIRE frame, even if
/// hidden or absent from accessibility. No attempt at unsafe pixel masking.
#[derive(Clone, Copy, Debug)]
pub struct ObservationSensitive;
impl Component for ObservationSensitive {}
/// Application attestation for *all* rendered content, including custom drawing,
/// images, titles and semantic descriptions. Default/missing means deny.
/// Applications must update this before rendering private content. This is not
/// automatic secret detection; use Deny for unaudited or mixed-content apps.
#[derive(Clone, Copy, Debug, Default)]
pub enum ObservationContent {
    /// Do not expose this frame. The default for every app/session.
    #[default]
    Deny,
    /// App has audited the entire frame as public, with no private content.
    Public,
}

pub(crate) struct Request {
    after: u64,
    deadline: Instant,
    reply: mpsc::SyncSender<(Value, Vec<u8>)>,
}
/// One app/window's session. Drop/removal stops the worker and removes only its
/// own socket/descriptor/directory. Not cloned or inherited by other windows.
pub struct ObservationSession {
    requests: Mutex<mpsc::Receiver<Request>>,
    stop: Arc<AtomicBool>,
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
    directory: PathBuf,
    revision: AtomicU64,
    theme: String,
}
impl ObservationSession {
    /// Create a private session in the OS temp directory. Returns the descriptor
    /// path for a local client. Does not grant permission to expose any content:
    /// `ObservationContent::Public` is also required for each captured frame.
    /// Available on Unix; no TCP fallback. Call before starting the native app.
    #[cfg(unix)]
    pub fn enable(world: &mut World, theme: &str) -> io::Result<PathBuf> {
        use std::io::Write;
        use std::os::unix::{
            fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
            net::UnixListener,
        };
        if world.resource::<Self>().is_some() || theme.len() > 4096 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "session exists or theme too large",
            ));
        }
        let mut random = [0u8; 44];
        getrandom::fill(&mut random)
            .map_err(|_| io::Error::other("session entropy unavailable"))?;
        let token: String = random[..32].iter().map(|b| format!("{b:02x}")).collect();
        // The public locator reveals no bits of the independent capability.
        let nonce: String = random[32..].iter().map(|b| format!("{b:02x}")).collect();
        let directory = std::env::temp_dir().join(format!("rndr-observe-{nonce}"));
        std::fs::DirBuilder::new().mode(0o700).create(&directory)?;
        let descriptor = directory.join("session.json");
        let setup = (|| {
            let socket = directory.join("observe.sock");
            let listener = UnixListener::bind(&socket)?;
            std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
            listener.set_nonblocking(true)?;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&descriptor)?;
            file.write_all(
                serde_json::to_vec(
                    &json!({"protocol":"rndr-observe-1", "socket":socket, "capability":token}),
                )
                .unwrap()
                .as_slice(),
            )?;
            let (send, recv) = mpsc::sync_channel(1);
            let stop = Arc::new(AtomicBool::new(false));
            let stopping = stop.clone();
            let wake = rndr_core::frame_wake(world);
            let worker = std::thread::Builder::new()
                .name("rndr-observe".into())
                .spawn(move || {
                    while !stopping.load(Ordering::Acquire) {
                        match listener.accept() {
                            Ok((mut stream, _)) => {
                                // A timeout-configuration failure must not turn
                                // a local slow client into an unbounded read.
                                if stream
                                    .set_read_timeout(Some(Duration::from_millis(50)))
                                    .is_err()
                                {
                                    continue;
                                }
                                let result = read_request(&mut stream)
                                    .and_then(|line| authenticate(&line, &token));
                                let packet = match result {
                                    Ok(after) => {
                                        let (reply, receive) = mpsc::sync_channel(1);
                                        let deadline = Instant::now() + Duration::from_secs(2);
                                        if send
                                            .try_send(Request {
                                                after,
                                                deadline,
                                                reply,
                                            })
                                            .is_ok()
                                        {
                                            wake.wake();
                                            receive
                                                .recv_timeout(Duration::from_secs(2))
                                                .unwrap_or_else(|_| error("unavailable"))
                                        } else {
                                            error("busy")
                                        }
                                    }
                                    Err(_) => error("invalid_request"),
                                };
                                let _ = write_packet(&mut stream, packet);
                                // At most ten serviced connections per second per
                                // session, including invalid/denied requests.
                                std::thread::sleep(Duration::from_millis(100));
                            }
                            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                                std::thread::sleep(Duration::from_millis(10))
                            }
                            Err(_) => break,
                        }
                    }
                })?;
            Ok(Self {
                requests: Mutex::new(recv),
                stop,
                worker: Mutex::new(Some(worker)),
                directory: directory.clone(),
                revision: AtomicU64::new(0),
                theme: theme.into(),
            })
        })();
        match setup {
            Ok(session) => {
                world.insert_resource(session);
                Ok(descriptor)
            }
            Err(e) => {
                let _ = std::fs::remove_file(&descriptor);
                let _ = std::fs::remove_file(directory.join("observe.sock"));
                let _ = std::fs::remove_dir(directory);
                Err(e)
            }
        }
    }
    /// This transport is unavailable on non-Unix hosts and returns Unsupported;
    /// it never falls back to a network or unauthenticated transport.
    #[cfg(not(unix))]
    pub fn enable(_: &mut World, _: &str) -> io::Result<PathBuf> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Unix local transport required",
        ))
    }
}
impl ObservationSession {
    /// Revoke this session, stop its worker and remove its exact private files.
    /// Safe to call repeatedly. The resource remains disabled until world drop;
    /// this v1 API does not silently rotate or restart a revoked session.
    pub fn shutdown(&self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.lock().unwrap().take() {
            let _ = worker.join();
        }
        let _ = std::fs::remove_file(self.directory.join("observe.sock"));
        let _ = std::fs::remove_file(self.directory.join("session.json"));
        let _ = std::fs::remove_dir(&self.directory);
    }
}
impl Drop for ObservationSession {
    fn drop(&mut self) {
        self.shutdown();
    }
}
#[cfg(test)]
#[path = "observe_tests.rs"]
mod tests;

fn error(code: &str) -> (Value, Vec<u8>) {
    (
        json!({"protocol":"rndr-observe-1", "error":code, "png_bytes":0}),
        vec![],
    )
}
#[cfg_attr(not(unix), allow(dead_code))]
fn authenticate(line: &[u8], token: &str) -> io::Result<u64> {
    let invalid = || io::Error::new(io::ErrorKind::PermissionDenied, "invalid request");
    let text = std::str::from_utf8(line).map_err(|_| invalid())?;
    let text = text.strip_suffix('\n').ok_or_else(invalid)?;
    let parts: Vec<_> = text.split(' ').collect();
    if parts.len() != 3 || parts[0] != "RNDR1" || parts[1].len() != 64 {
        return Err(invalid());
    }
    let diff = parts[1]
        .bytes()
        .zip(token.bytes())
        .fold(0u8, |a, (x, y)| a | (x ^ y));
    if diff != 0 {
        return Err(invalid());
    }
    if parts[2].is_empty() || !parts[2].bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid());
    }
    parts[2].parse().map_err(|_| invalid())
}
#[cfg(unix)]
fn read_request(stream: &mut std::os::unix::net::UnixStream) -> io::Result<Vec<u8>> {
    use std::io::Read;
    let until = Instant::now() + Duration::from_millis(500);
    let mut line = Vec::new();
    while line.len() < 128 && Instant::now() < until {
        let mut byte = [0];
        match stream.read(&mut byte) {
            Ok(1) => {
                line.push(byte[0]);
                if byte[0] == b'\n' {
                    return Ok(line);
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                continue
            }
            _ => break,
        }
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "bounded request",
    ))
}
#[cfg(unix)]
fn write_packet(
    stream: &mut std::os::unix::net::UnixStream,
    packet: (Value, Vec<u8>),
) -> io::Result<()> {
    use std::io::Write;
    let metadata = serde_json::to_vec(&packet.0)?;
    if metadata.len() > MAX_JSON {
        return Err(io::Error::other("metadata limit"));
    }
    // A slow reader cannot extend this worker's lifetime indefinitely by
    // accepting one byte per socket timeout. One absolute response deadline.
    stream.set_nonblocking(true)?;
    let deadline = Instant::now() + Duration::from_millis(500);
    for bytes in [
        &(metadata.len() as u32).to_be_bytes()[..],
        &metadata,
        &packet.1,
    ] {
        let mut remaining = bytes;
        while !remaining.is_empty() {
            if Instant::now() >= deadline {
                return Err(io::Error::new(io::ErrorKind::TimedOut, "response deadline"));
            }
            match stream.write(remaining) {
                Ok(0) => return Err(io::Error::new(io::ErrorKind::WriteZero, "client closed")),
                Ok(n) => remaining = &remaining[n..],
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(2))
                }
                Err(e) => return Err(e),
            }
        }
    }
    Ok(())
}

/// Optional current theme descriptor. Update alongside an application's theme;
/// this is an app-supplied description, not a renderer-inferred style system.
#[derive(Clone, Debug)]
pub struct ObservationTheme(pub String);

pub(crate) struct Capture {
    request: Request,
    metadata: Value,
    tree: rndr_core::AccessTree,
    engine: Option<Arc<dyn rndr_core::TextEngine>>,
    engine_epoch: (u64, u64),
    engine_kind: &'static str,
    details: Vec<(u64, rndr_core::AccessDetails)>,
    scroll: Vec<(u64, rndr_core::Scroll, rndr_core::Rect)>,
}
impl Capture {
    pub(crate) fn text_engine_kind(&self) -> &'static str {
        self.engine_kind
    }
}
#[cfg(test)]
fn text_engine_kind(world: &World) -> &'static str {
    engine_kind(rndr_core::text_engine(world).as_ref())
}
fn engine_kind(engine: &dyn rndr_core::TextEngine) -> &'static str {
    if engine.as_any().is::<rndr_core::BuiltinText>() {
        return "builtin_atlas";
    }
    #[cfg(feature = "parley")]
    if engine.as_any().is::<crate::text::ParleyText>() {
        return "parley";
    }
    "custom"
}
pub(crate) fn semantic_details(
    world: &World,
    tree: &rndr_core::AccessTree,
) -> Vec<(u64, rndr_core::AccessDetails)> {
    tree.nodes
        .iter()
        .map(|node| {
            (
                node.id,
                rndr_core::entity_from_access_id(node.id)
                    .map(|e| rndr_core::access_details(world, e))
                    .unwrap_or_default(),
            )
        })
        .collect()
}
pub(crate) fn scroll_state(world: &World) -> Vec<(u64, rndr_core::Scroll, rndr_core::Rect)> {
    let mut ports: Vec<_> = world
        .query::<rndr_core::Scroll>()
        .into_iter()
        .map(|(e, s)| {
            (
                rndr_core::access_id(e),
                *s,
                world.get::<rndr_core::Rect>(e).copied().unwrap_or_default(),
            )
        })
        .collect();
    ports.sort_by_key(|p| p.0);
    ports
}
pub(crate) fn public(world: &World) -> bool {
    matches!(
        world.resource::<ObservationContent>(),
        Some(ObservationContent::Public)
    ) && world.entities_with::<ObservationSensitive>().is_empty()
}
pub(crate) fn theme(world: &World) -> &str {
    world
        .resource::<ObservationTheme>()
        .map(|t| t.0.as_str())
        .unwrap_or_else(|| {
            world
                .resource::<ObservationSession>()
                .map(|s| s.theme.as_str())
                .unwrap_or("unspecified")
        })
}
/// Refuse Render-stage changes rather than combine old paint with new semantics
/// or newly sensitive data. Snapshot and validation both run on the UI thread.
pub(crate) fn validate(world: &World, capture: Capture, scale: f32) -> Option<Capture> {
    let error_code = if world
        .resource::<ObservationSession>()
        .is_none_or(|s| s.stop.load(Ordering::Acquire))
    {
        Some("session_revoked")
    } else if capture.request.deadline <= Instant::now() {
        Some("expired")
    } else if !public(world) {
        Some("sensitive_frame")
    } else if !world.entities_with::<rndr_core::SurfaceSlot>().is_empty() {
        Some("foreign_surface_unsupported")
    } else if !engine_matches(world, &capture) {
        Some("post_paint_text_engine_mutation")
    } else if capture.tree != rndr_core::access_tree(world)
        || capture.details != semantic_details(world, &capture.tree)
        || capture.scroll != scroll_state(world)
        || capture.metadata["device_scale"] != json!(scale)
        || capture.metadata["theme"] != json!(theme(world))
    {
        Some("post_paint_mutation")
    } else {
        None
    };
    if let Some(code) = error_code {
        let _ = capture.request.reply.try_send(error(code));
        None
    } else {
        Some(capture)
    }
}
pub(crate) fn engine_epoch(engine: &dyn rndr_core::TextEngine) -> (u64, u64) {
    #[cfg(feature = "parley")]
    if let Some(parley) = engine.as_any().downcast_ref::<crate::text::ParleyText>() {
        return parley.paint_epoch();
    }
    (engine.cache_epoch(), 0)
}
fn engine_matches(world: &World, capture: &Capture) -> bool {
    // Synchronize a builtin TextFace change before checking its version. Keep
    // the original Arc alive so an address cannot be recycled into a match.
    let current = world.resource::<Arc<dyn rndr_core::TextEngine>>();
    match (&capture.engine, current) {
        (Some(old), Some(current)) if Arc::ptr_eq(old, current) => {
            let current = rndr_core::text_engine(world);
            engine_epoch(current.as_ref()) == capture.engine_epoch
        }
        (None, None) => true,
        _ => false,
    }
}
/// Called after the world's layout/paint pass, before submitting its surface.
pub(crate) fn begin(world: &World, width: u32, height: u32, scale: f32) -> Option<Capture> {
    let session = world.resource::<ObservationSession>()?;
    if session.stop.load(Ordering::Acquire) {
        return None;
    }
    let revision = session.revision.fetch_add(1, Ordering::Relaxed) + 1;
    let request = session.requests.lock().unwrap().try_recv().ok()?;
    let deny = if request.deadline <= Instant::now() {
        Some("expired")
    } else if request.after >= revision {
        Some("stale_revision")
    } else if !public(world) {
        Some("sensitive_frame")
    } else if !world.entities_with::<rndr_core::SurfaceSlot>().is_empty() {
        Some("foreign_surface_unsupported")
    } else if width == 0
        || height == 0
        || width > MAX_SIDE
        || height > MAX_SIDE
        || u64::from(width) * u64::from(height) > MAX_PIXELS
        || !scale.is_finite()
        || scale <= 0.0
    {
        Some("size_limit")
    } else {
        None
    };
    if let Some(code) = deny {
        let _ = request.reply.try_send(error(code));
        return None;
    }
    let tree = rndr_core::access_tree(world);
    if tree.nodes.len() > 8192 {
        let _ = request.reply.try_send(error("semantic_limit"));
        return None;
    }
    let binding_tree = tree.clone();
    let nodes: Vec<_> = tree.nodes.into_iter().map(|node| {
        let entity = rndr_core::entity_from_access_id(node.id);
        let d = entity.map(|e| rndr_core::access_details(world, e)).unwrap_or_default();
        let mut actions = Vec::new();
        if d.focus { actions.push("focus"); }
        if d.activate { actions.push("activate"); }
        if d.set_value { actions.push("set_value"); }
        json!({"id":node.id.to_string(), "role":format!("{:?}",node.role), "label":node.name, "value":node.value,
            "bounds":[node.bounds.x,node.bounds.y,node.bounds.w,node.bounds.h], "children":node.children.iter().map(u64::to_string).collect::<Vec<_>>(),
            "focusable":node.focusable,"focused":node.focused,"selected":node.selected,"enabled":node.enabled,"expanded":node.expanded,
            "checked":d.checked,"multiselect":d.multiselect,"readonly":d.readonly,"description":d.description,"actions":actions,"panel":d.panel,"controls":d.controls.map(|id|id.to_string()),"labelled_by":d.labelled_by.map(|id|id.to_string()),"orientation":d.orientation.map(|o|format!("{o:?}"))})
    }).collect();
    // Scroll ports can be flattened out of the accessibility tree; expose those
    // separately with generation-safe entity ids and actual logical geometry.
    let scroll: Vec<_> = world.query::<rndr_core::Scroll>().into_iter().filter(|(e,_)| rndr_core::interaction_visible(world,*e)).map(|(e,s)| {
        let r = world.get::<rndr_core::Rect>(e).copied().unwrap_or_default();
        json!({"id":rndr_core::access_id(e).to_string(),"bounds":[r.x,r.y,r.w,r.h],"offset":s.offset,"content":s.content})
    }).collect();
    let metadata = json!({"protocol":"rndr-observe-1", "frame_revision":revision.to_string(), "width":width,"height":height,"device_scale":scale,
        "theme":theme(world),"source":"live-surface-copy-before-present", "presented_confirmation":false,"bounds_units":"logical_points", "root":tree.root.to_string(),"nodes":nodes,"scroll":scroll});
    if serde_json::to_vec(&metadata).map_or(true, |v| v.len() > MAX_JSON - 4096) {
        let _ = request.reply.try_send(error("semantic_limit"));
        return None;
    }
    let engine = world.resource::<Arc<dyn rndr_core::TextEngine>>().cloned();
    let engine_epoch = engine.as_ref().map_or((0, 0), |e| engine_epoch(e.as_ref()));
    let engine_kind = engine
        .as_ref()
        .map_or("builtin_atlas", |e| engine_kind(e.as_ref()));
    Some(Capture {
        engine,
        engine_epoch,
        engine_kind,
        request,
        metadata,
        details: semantic_details(world, &binding_tree),
        tree: binding_tree,
        scroll: scroll_state(world),
    })
}
pub(crate) fn complete(
    capture: Capture,
    image: Result<crate::RgbaImage, &'static str>,
    submitted: bool,
    provenance: impl Into<Value>,
) {
    let packet = if capture.request.deadline <= Instant::now() {
        error("expired")
    } else if !submitted {
        error("surface_unavailable")
    } else {
        match image.and_then(|i| {
            if capture.metadata["width"] != json!(i.width)
                || capture.metadata["height"] != json!(i.height)
                || i.rgba.len() != i.width as usize * i.height as usize * 4
            {
                return Err("size_mismatch");
            }
            crate::encode_png(&i).map_err(|_| "png_failed")
        }) {
            Ok(png) => {
                let mut metadata = capture.metadata;
                metadata["png_bytes"] = json!(png.len());
                metadata["render_provenance"] = provenance.into();
                (metadata, png)
            }
            Err(code) => error(code),
        }
    };
    let _ = capture.request.reply.try_send(packet);
}
