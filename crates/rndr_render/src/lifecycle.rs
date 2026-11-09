//! The OS reaching a running app: links it opens with the app, later
//! launches ([`single_instance`]), the app icon ([`set_app_icon`]) and, on
//! macOS, a Dock reopen.
//!
//! OS callbacks and the single-instance listener thread only post an
//! [`Incoming`] to the loop. The loop hands it to the primary window's world
//! as [`rndr_core::UrlOpened`] or [`rndr_core::Reopened`]; nothing runs app
//! code inside an OS callback. Before the loop runs, arrivals are kept and
//! sent in order once it starts.
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::decode::{decode_image, DecodeError, Raster};
use crate::{RunnerBus, RunnerEvent};

/// What reached the app from outside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Incoming {
    Url(String),
    Reopen,
}

struct Inbox {
    bus: Option<RunnerBus>,
    pending: Vec<Incoming>,
}

static INBOX: Mutex<Inbox> = Mutex::new(Inbox {
    bus: None,
    pending: Vec::new(),
});

/// Posts `incoming` to the loop, or keeps it until the loop starts. Returns
/// whether a running loop took it.
pub(crate) fn receive(incoming: Incoming) -> bool {
    let mut inbox = INBOX.lock().unwrap_or_else(|e| e.into_inner());
    match &inbox.bus {
        Some(bus) => {
            bus.post(RunnerEvent::Lifecycle(incoming));
            true
        }
        None => {
            inbox.pending.push(incoming);
            false
        }
    }
}

/// The loop runs: send what arrived so far, then everything after.
pub(crate) fn attach(bus: &RunnerBus) {
    let mut inbox = INBOX.lock().unwrap_or_else(|e| e.into_inner());
    for incoming in std::mem::take(&mut inbox.pending) {
        bus.post(RunnerEvent::Lifecycle(incoming));
    }
    inbox.bus = Some(bus.clone());
}

/// The loop ended: keep later arrivals for a later loop.
pub(crate) fn detach() {
    INBOX.lock().unwrap_or_else(|e| e.into_inner()).bus = None;
}

#[cfg(any(all(unix, not(target_os = "macos")), target_os = "windows"))]
/// A later launch's arguments: each link as a [`Incoming::Url`], or a reopen
/// when it carried none.
pub(crate) fn relaunched(args: &[String]) {
    let links = rndr_core::launch_links(args);
    if links.is_empty() {
        receive(Incoming::Reopen);
    }
    for link in links {
        receive(Incoming::Url(link));
    }
}

#[cfg(any(all(unix, not(target_os = "macos")), target_os = "windows", test))]
/// Opens every single-instance message, so a stray client's bytes are not
/// taken for arguments.
const MAGIC: &[u8] = b"rndr-instance 1\0";
#[cfg(any(all(unix, not(target_os = "macos")), target_os = "windows", test))]
/// The most one launch's arguments may take.
const MAX_MESSAGE: usize = 1 << 20;

#[cfg(any(all(unix, not(target_os = "macos")), target_os = "windows", test))]
/// This launch's arguments as a single-instance message.
fn encode(args: &[String]) -> Vec<u8> {
    let mut message = MAGIC.to_vec();
    message.extend(args.join("\0").into_bytes());
    message
}

#[cfg(any(all(unix, not(target_os = "macos")), target_os = "windows", test))]
/// A single-instance message's arguments, or `None` for anything else.
fn decode(message: &[u8]) -> Option<Vec<String>> {
    if message.len() > MAX_MESSAGE + MAGIC.len() {
        return None;
    }
    let body = message.strip_prefix(MAGIC)?;
    let text = std::str::from_utf8(body).ok()?;
    Some(
        text.split('\0')
            .filter(|a| !a.is_empty())
            .map(str::to_owned)
            .collect(),
    )
}

#[cfg(any(all(unix, not(target_os = "macos")), target_os = "windows", test))]
/// `id` as part of a socket or pipe name: letters, digits, `.`, `-` and `_`.
fn clean_id(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '-'
            }
        })
        .take(64)
        .collect()
}

/// Makes this process the app's only running one. Call it in `main` before
/// opening a window.
///
/// Returns `false` when another process of the app already runs: this
/// launch's arguments went to it, so return from `main` without opening a
/// window. The running process gets a [`rndr_core::UrlOpened`] for each link
/// among them, or a [`rndr_core::Reopened`] without links, and brings its
/// primary window forward. Returns `true` for the first process, which
/// receives the launches that follow, and also when the running one can't be
/// reached, so a launch still opens a window.
///
/// `id` names the app (a reverse-DNS identifier works); builds that should
/// run side by side use different ids.
///
/// - **Linux and other Unix:** a socket in `$XDG_RUNTIME_DIR`, one per user.
/// - **Windows:** a named pipe per user and session.
/// - **macOS and the browser:** always `true`. macOS already keeps one
///   process per app and hands it links and reopens.
pub fn single_instance(id: &str) -> bool {
    #[cfg(all(unix, not(target_os = "macos"), not(target_arch = "wasm32")))]
    {
        unix::claim(id, &std::env::args().skip(1).collect::<Vec<_>>())
    }
    #[cfg(target_os = "windows")]
    {
        windows::claim(id)
    }
    #[cfg(not(any(
        all(unix, not(target_os = "macos"), not(target_arch = "wasm32")),
        target_os = "windows"
    )))]
    {
        let _ = id;
        true
    }
}

#[cfg(all(unix, not(target_os = "macos"), not(target_arch = "wasm32")))]
mod unix {
    use std::io::{ErrorKind, Read, Write};
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::PathBuf;
    use std::time::Duration;

    /// How long either side waits on the other.
    const TIMEOUT: Duration = Duration::from_secs(5);

    pub(super) fn claim(id: &str, args: &[String]) -> bool {
        let Some(path) = socket_path(id) else {
            return true;
        };
        claim_at(&path, args)
    }

    pub(super) fn claim_at(path: &std::path::Path, args: &[String]) -> bool {
        // A second pass after removing a socket its process left behind.
        for _ in 0..2 {
            match UnixListener::bind(path) {
                Ok(listener) => {
                    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
                    listen(listener);
                    return true;
                }
                Err(e) if e.kind() == ErrorKind::AddrInUse => match UnixStream::connect(path) {
                    Ok(stream) => return !send(stream, args),
                    Err(e) if e.kind() == ErrorKind::ConnectionRefused => {
                        let _ = std::fs::remove_file(path);
                    }
                    Err(_) => return true,
                },
                Err(_) => return true,
            }
        }
        true
    }

    /// Per user: `$XDG_RUNTIME_DIR` is the user's own, else a private
    /// directory in the temp dir.
    fn socket_path(id: &str) -> Option<PathBuf> {
        let name = format!("rndr-instance-{}.sock", super::clean_id(id));
        if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR").filter(|d| !d.is_empty()) {
            return Some(PathBuf::from(dir).join(name));
        }
        // SAFETY: `getuid` has no preconditions and cannot fail.
        let uid = unsafe { libc::getuid() };
        let dir = std::env::temp_dir().join(format!("rndr-{uid}"));
        let _ = std::fs::DirBuilder::new().mode(0o700).create(&dir);
        let meta = std::fs::symlink_metadata(&dir).ok()?;
        use std::os::unix::fs::MetadataExt;
        // Someone else's directory, or a link to one, is not ours to use.
        (meta.is_dir() && meta.uid() == uid && meta.mode() & 0o077 == 0).then(|| dir.join(name))
    }

    fn send(mut stream: UnixStream, args: &[String]) -> bool {
        let _ = stream.set_write_timeout(Some(TIMEOUT));
        stream.write_all(&super::encode(args)).is_ok()
    }

    fn listen(listener: UnixListener) {
        let _ = std::thread::Builder::new()
            .name("rndr-instance".into())
            .spawn(move || {
                for stream in listener.incoming() {
                    let Ok(stream) = stream else { continue };
                    if let Some(args) = read(stream) {
                        super::relaunched(&args);
                    }
                }
            });
    }

    fn read(stream: UnixStream) -> Option<Vec<String>> {
        let _ = stream.set_read_timeout(Some(TIMEOUT));
        let mut message = Vec::new();
        stream
            .take((super::MAX_MESSAGE + super::MAGIC.len() + 1) as u64)
            .read_to_end(&mut message)
            .ok()?;
        super::decode(&message)
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use std::time::{Duration, Instant};

    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, ERROR_ACCESS_DENIED, ERROR_BROKEN_PIPE, ERROR_FILE_NOT_FOUND,
        ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, ReadFile, WriteFile, FILE_FLAG_FIRST_PIPE_INSTANCE, OPEN_EXISTING,
        PIPE_ACCESS_INBOUND,
    };
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeServerProcessId,
        WaitNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
        PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
    };
    use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::System::WindowsProgramming::GetUserNameW;
    use windows_sys::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow;

    /// How long a later launch keeps trying to reach the first process.
    const REACH_FOR: Duration = Duration::from_secs(5);

    enum Sent {
        Delivered,
        /// No process has the pipe now.
        Gone,
        /// One has, but didn't take the message.
        Unreachable,
    }

    pub(super) fn claim(id: &str) -> bool {
        let name = wide(&pipe_name(id));
        let deadline = Instant::now() + REACH_FOR;
        loop {
            match create(&name) {
                Ok(pipe) => {
                    listen(pipe);
                    return true;
                }
                Err(ERROR_ACCESS_DENIED) => {}
                Err(_) => return true,
            }
            match send(&name) {
                Sent::Delivered => return false,
                // The first process went between the two calls: claim again.
                Sent::Gone if Instant::now() < deadline => continue,
                Sent::Gone | Sent::Unreachable => return true,
            }
        }
    }

    /// Per user and session, so another account's app never takes this
    /// one's launches.
    fn pipe_name(id: &str) -> String {
        let mut session = 0;
        // SAFETY: writes the session id into `session`.
        unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session) };
        let mut buf = [0u16; 257];
        let mut len = buf.len() as u32;
        // SAFETY: `len` is the buffer's length in characters; it comes back
        // as the name's, counting the terminating NUL.
        let user = if unsafe { GetUserNameW(buf.as_mut_ptr(), &mut len) } != 0 {
            String::from_utf16_lossy(&buf[..(len as usize).saturating_sub(1)])
        } else {
            String::new()
        };
        let name = format!(
            r"\\.\pipe\rndr-instance-{session}-{}-{}",
            super::clean_id(&user),
            super::clean_id(id)
        );
        name.chars().take(256).collect()
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    /// The pipe's first instance, or why not (`ERROR_ACCESS_DENIED`: another
    /// process has it).
    fn create(name: &[u16]) -> Result<HANDLE, u32> {
        // SAFETY: `name` is NUL-terminated and outlives the call. No security
        // attributes means the default ACL, which lets only this user (and
        // administrators) write to it.
        let pipe = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                PIPE_ACCESS_INBOUND | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_UNLIMITED_INSTANCES,
                0,
                64 * 1024,
                0,
                std::ptr::null(),
            )
        };
        if pipe == INVALID_HANDLE_VALUE {
            // SAFETY: reads this thread's last error.
            Err(unsafe { GetLastError() })
        } else {
            Ok(pipe)
        }
    }

    /// Reads one launch at a time for the rest of the process. The handle
    /// stays open, so the name stays this process's.
    fn listen(pipe: HANDLE) {
        let raw = pipe as usize;
        let spawned = std::thread::Builder::new()
            .name("rndr-instance".into())
            .spawn(move || {
                let pipe = raw as HANDLE;
                loop {
                    // SAFETY: `pipe` is the open server handle; a client that
                    // connected first answers ERROR_PIPE_CONNECTED.
                    let connected = unsafe { ConnectNamedPipe(pipe, std::ptr::null_mut()) } != 0
                        || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
                    if connected {
                        if let Some(args) = read(pipe) {
                            super::relaunched(&args);
                        }
                    }
                    // SAFETY: ends this client's connection so the next can connect.
                    unsafe { DisconnectNamedPipe(pipe) };
                }
            });
        if spawned.is_err() {
            // SAFETY: the handle no thread took.
            unsafe { CloseHandle(raw as HANDLE) };
        }
    }

    fn read(pipe: HANDLE) -> Option<Vec<String>> {
        let mut message = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            let mut got = 0;
            // SAFETY: reads at most `buf.len()` bytes into `buf`.
            let ok = unsafe {
                ReadFile(
                    pipe,
                    buf.as_mut_ptr(),
                    buf.len() as u32,
                    &mut got,
                    std::ptr::null_mut(),
                )
            } != 0;
            message.extend_from_slice(&buf[..got as usize]);
            if message.len() > super::MAX_MESSAGE + super::MAGIC.len() {
                return None;
            }
            if !ok {
                // SAFETY: reads this thread's last error.
                if unsafe { GetLastError() } == ERROR_BROKEN_PIPE {
                    break;
                }
                return None;
            }
        }
        super::decode(&message)
    }

    fn send(name: &[u16]) -> Sent {
        let deadline = Instant::now() + REACH_FOR;
        let pipe = loop {
            // SAFETY: `name` is NUL-terminated; opens the client end for writing.
            let pipe = unsafe {
                CreateFileW(
                    name.as_ptr(),
                    GENERIC_WRITE,
                    0,
                    std::ptr::null(),
                    OPEN_EXISTING,
                    0,
                    std::ptr::null_mut(),
                )
            };
            if pipe != INVALID_HANDLE_VALUE {
                break pipe;
            }
            // SAFETY: reads this thread's last error.
            match unsafe { GetLastError() } {
                ERROR_FILE_NOT_FOUND => return Sent::Gone,
                // SAFETY: `name` is NUL-terminated.
                ERROR_PIPE_BUSY if Instant::now() < deadline => unsafe {
                    WaitNamedPipeW(name.as_ptr(), 1000);
                },
                _ => return Sent::Unreachable,
            }
        };
        let mut server = 0;
        // SAFETY: `pipe` is the open client handle. Windows gives the
        // foreground only to the process the user just started, so let the
        // first process take it for the window it brings forward.
        unsafe {
            if GetNamedPipeServerProcessId(pipe, &mut server) != 0 {
                AllowSetForegroundWindow(server);
            }
        }
        let args: Vec<String> = std::env::args().skip(1).collect();
        let message = super::encode(&args);
        let mut written = 0;
        // SAFETY: writes `message` to the open pipe, then closes it once.
        let ok = unsafe {
            let ok = WriteFile(
                pipe,
                message.as_ptr(),
                message.len() as u32,
                &mut written,
                std::ptr::null_mut(),
            ) != 0;
            CloseHandle(pipe);
            ok
        };
        if ok && written as usize == message.len() {
            Sent::Delivered
        } else {
            Sent::Unreachable
        }
    }
}

struct AppIcon {
    raster: Arc<Raster>,
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    bytes: Arc<Vec<u8>>,
}

static ICON: Mutex<Option<AppIcon>> = Mutex::new(None);
/// Counts [`set_app_icon`] calls, so each window and the Dock apply a new
/// icon once.
static ICON_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Sets the app's icon from PNG or JPEG bytes, in place of the bundle's or
/// executable's, from any thread. Best square and 256 px or larger.
///
/// - **macOS:** the Dock and app-switcher icon.
/// - **Windows:** each window's title bar, taskbar and Alt+Tab icon.
/// - **Linux X11:** each window's `_NET_WM_ICON`, which docks and window
///   switchers show. Wayland compositors take the icon from the app's
///   `.desktop` file instead.
///
/// Windows that are already open switch to it on their next frame. A
/// headless or browser run keeps it without showing it.
///
/// # Errors
///
/// Fails when the bytes are not a PNG or JPEG.
pub fn set_app_icon(bytes: impl Into<Vec<u8>>) -> Result<(), DecodeError> {
    let bytes = bytes.into();
    let raster = decode_image(&bytes)?;
    *ICON.lock().unwrap_or_else(|e| e.into_inner()) = Some(AppIcon {
        raster: Arc::new(raster),
        bytes: Arc::new(bytes),
    });
    ICON_GENERATION.fetch_add(1, Ordering::AcqRel);
    Ok(())
}

/// How many times the icon was set; a window compares it with the last it
/// applied.
pub(crate) fn icon_generation() -> u64 {
    ICON_GENERATION.load(Ordering::Acquire)
}

/// The icon as a winit window icon, once one is set.
pub(crate) fn window_icon() -> Option<winit::icon::Icon> {
    let icon = ICON.lock().unwrap_or_else(|e| e.into_inner());
    let raster = Arc::clone(&icon.as_ref()?.raster);
    drop(icon);
    winit::icon::RgbaIcon::new(raster.rgba.clone(), raster.width, raster.height)
        .ok()
        .map(Into::into)
}

/// macOS: shows the icon in the Dock. Main thread, after launch (AppKit
/// would put the bundle's icon over an earlier one).
#[cfg(target_os = "macos")]
pub(crate) fn apply_dock_icon() {
    let bytes = {
        let icon = ICON.lock().unwrap_or_else(|e| e.into_inner());
        match icon.as_ref() {
            Some(icon) => Arc::clone(&icon.bytes),
            None => return,
        }
    };
    mac::set_dock_icon(&bytes);
}

#[cfg(target_os = "macos")]
pub(crate) use mac::{install_open_url, install_reopen};

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::{c_char, c_void};
    use std::sync::OnceLock;

    use objc2::rc::autoreleasepool;
    use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Imp, NSObject, Sel};
    use objc2::{class, msg_send, sel, ClassType};
    use objc2_foundation::{MainThreadMarker, NSString};

    use super::Incoming;

    /// `'GURL'`: `kInternetEventClass` and `kAEGetURL`.
    const GET_URL: u32 = u32::from_be_bytes(*b"GURL");
    /// `'----'`: `keyDirectObject`, the event's URL string.
    const DIRECT_OBJECT: u32 = u32::from_be_bytes(*b"----");

    #[link(name = "objc")]
    extern "C" {
        fn class_addMethod(
            class: *const AnyClass,
            name: Sel,
            imp: Imp,
            types: *const c_char,
        ) -> Bool;
    }

    fn url_of(event: &AnyObject) -> Option<String> {
        // SAFETY: `event` is an NSAppleEventDescriptor; both messages are
        // nil-safe and return autoreleased objects, released with the pool.
        autoreleasepool(|_| unsafe {
            let param: *mut AnyObject = msg_send![event, paramDescriptorForKeyword: DIRECT_OBJECT];
            let string: *mut NSString = msg_send![param.as_ref()?, stringValue];
            Some(string.as_ref()?.to_string())
        })
    }

    extern "C" fn get_url(_: &AnyObject, _: Sel, event: *mut AnyObject, _reply: *mut AnyObject) {
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // SAFETY: AppKit passes the event, or nil.
            if let Some(url) = unsafe { event.as_ref() }.and_then(url_of) {
                super::receive(Incoming::Url(url));
            }
        }));
        if caught.is_err() {
            eprintln!("rndr: open-url handler panicked");
        }
    }

    /// The handler object, made once and kept for the process (the event
    /// manager does not retain it).
    fn handler() -> Option<*mut AnyObject> {
        static HANDLER: OnceLock<usize> = OnceLock::new();
        let made = *HANDLER.get_or_init(|| {
            const NAME: &str = "RndrOpenUrlHandler";
            let class = AnyClass::get(NAME).or_else(|| {
                let mut builder = ClassBuilder::new(NAME, NSObject::class())?;
                // SAFETY: the function matches
                // `handleGetURLEvent:withReplyEvent:` (two objects, void).
                unsafe {
                    builder.add_method(
                        sel!(handleGetURLEvent:withReplyEvent:),
                        get_url as extern "C" fn(_, _, _, _),
                    );
                }
                Some(builder.register())
            });
            let Some(class) = class else { return 0 };
            // SAFETY: `new` on an NSObject subclass; the +1 reference is kept.
            let made: *mut AnyObject = unsafe { msg_send![class, new] };
            made as usize
        });
        (made != 0).then_some(made as *mut AnyObject)
    }

    /// Makes rndr's object the get-URL Apple event handler, before the loop
    /// runs so the link that launched the app arrives too. Main thread only.
    pub(crate) fn install_open_url() {
        if MainThreadMarker::new().is_none() {
            return;
        }
        let Some(handler) = handler() else { return };
        // SAFETY: Foundation calls on the main thread; the handler lives for
        // the process.
        autoreleasepool(|_| unsafe {
            let manager: *mut AnyObject =
                msg_send![class!(NSAppleEventManager), sharedAppleEventManager];
            if let Some(manager) = manager.as_ref() {
                let _: () = msg_send![manager, setEventHandler: handler,
                    andSelector: sel!(handleGetURLEvent:withReplyEvent:),
                    forEventClass: GET_URL, andEventID: GET_URL];
            }
        });
    }

    extern "C" fn should_handle_reopen(
        _: *mut AnyObject,
        _: Sel,
        _: *mut AnyObject,
        visible: Bool,
    ) -> Bool {
        let sent =
            std::panic::catch_unwind(|| super::receive(Incoming::Reopen)).unwrap_or_else(|_| {
                eprintln!("rndr: reopen handler panicked");
                false
            });
        // With a visible window AppKit's handling changes nothing and the
        // activation already brought the app forward. Otherwise the loop
        // shows the primary window, unless it is gone and AppKit should
        // restore what it can.
        Bool::new(!sent || visible.as_bool())
    }

    /// Adds `applicationShouldHandleReopen:hasVisibleWindows:` to winit's app
    /// delegate, which lacks it. Leaves a delegate that already answers it
    /// alone. Main thread, once the delegate exists.
    pub(crate) fn install_reopen() {
        static INSTALLED: OnceLock<()> = OnceLock::new();
        if MainThreadMarker::new().is_none() {
            return;
        }
        INSTALLED.get_or_init(|| {
            let selector = sel!(applicationShouldHandleReopen:hasVisibleWindows:);
            // SAFETY: runtime calls on the main thread. The function has the
            // selector's ABI (BOOL is `bool` on Apple silicon and `signed
            // char` on Intel), and adding a missing method leaves the class's
            // other methods and instances alone.
            unsafe {
                let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
                let delegate: *mut AnyObject = msg_send![app, delegate];
                let delegate = match delegate.as_ref() {
                    Some(delegate) => delegate,
                    None => {
                        // winit sets no application delegate. AppKit holds
                        // the delegate weakly, so this one is kept for the
                        // process.
                        let Some(made) = app_delegate().as_ref() else {
                            return;
                        };
                        made
                    }
                };
                let answers: bool = msg_send![delegate, respondsToSelector: selector];
                if answers {
                    return;
                }
                let types = if cfg!(target_arch = "aarch64") {
                    c"B@:@B"
                } else {
                    c"c@:@c"
                };
                let imp = std::mem::transmute::<
                    extern "C" fn(*mut AnyObject, Sel, *mut AnyObject, Bool) -> Bool,
                    Imp,
                >(should_handle_reopen);
                if !class_addMethod(delegate.class(), selector, imp, types.as_ptr()).as_bool() {
                    eprintln!("rndr: could not add the reopen method to the app delegate");
                    return;
                }
                // AppKit caches which optional methods its delegate answers.
                let _: () = msg_send![app, setDelegate: delegate];
            }
        });
    }

    /// An empty application delegate, made when there is none to extend.
    fn app_delegate() -> *mut AnyObject {
        const NAME: &str = "RndrAppDelegate";
        let class = AnyClass::get(NAME).or_else(|| {
            ClassBuilder::new(NAME, NSObject::class()).map(|builder| builder.register())
        });
        let Some(class) = class else {
            return std::ptr::null_mut();
        };
        // SAFETY: `new` on an NSObject subclass; the +1 reference is kept.
        unsafe { msg_send![class, new] }
    }

    pub(super) fn set_dock_icon(bytes: &[u8]) {
        if MainThreadMarker::new().is_none() {
            return;
        }
        // SAFETY: AppKit calls on the main thread. `alloc`/`initWithData:`
        // returns a +1 image (or nil), released after AppKit retains it.
        autoreleasepool(|_| unsafe {
            let data: *mut AnyObject = msg_send![class!(NSData),
                dataWithBytes: bytes.as_ptr().cast::<c_void>(), length: bytes.len()];
            if data.is_null() {
                return;
            }
            let image: *mut AnyObject = msg_send![class!(NSImage), alloc];
            let image: *mut AnyObject = msg_send![image, initWithData: data];
            let Some(image) = image.as_ref() else { return };
            let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
            let _: () = msg_send![app, setApplicationIconImage: image];
            let _: () = msg_send![image, release];
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_round_trip_and_strangers_are_refused() {
        let args = vec!["--flag".to_owned(), "myapp://open?id=1".to_owned()];
        assert_eq!(decode(&encode(&args)), Some(args));
        assert_eq!(decode(&encode(&[])), Some(Vec::new()));
        assert_eq!(decode(b"GET / HTTP/1.1\r\n"), None);
        let mut invalid = MAGIC.to_vec();
        invalid.extend([0xff, 0xfe]);
        assert_eq!(decode(&invalid), None);
    }

    #[test]
    fn ids_become_safe_names() {
        assert_eq!(clean_id("com.example.App"), "com.example.App");
        assert_eq!(clean_id("../a b\\c"), "..-a-b-c");
        assert_eq!(clean_id(&"x".repeat(100)).len(), 64);
    }

    #[test]
    fn an_icon_must_be_an_image() {
        assert!(set_app_icon(b"not an image".to_vec()).is_err());
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn a_second_claim_forwards_its_arguments_to_the_first() {
        let dir = std::env::temp_dir().join(format!("rndr-instance-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("app.sock");
        let _ = std::fs::remove_file(&path);
        assert!(
            unix::claim_at(&path, &[]),
            "the first claim owns the socket"
        );
        let args = vec!["myapp://second".to_owned()];
        assert!(!unix::claim_at(&path, &args), "a later claim hands over");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let pending = INBOX.lock().unwrap().pending.clone();
            if pending.contains(&Incoming::Url("myapp://second".into())) {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the first process got the link"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn a_socket_left_behind_is_reclaimed() {
        let dir = std::env::temp_dir().join(format!("rndr-instance-stale-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("app.sock");
        let _ = std::fs::remove_file(&path);
        drop(std::os::unix::net::UnixListener::bind(&path).unwrap());
        assert!(path.exists(), "a dead process's socket file stays behind");
        assert!(unix::claim_at(&path, &[]), "it is claimed again");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
