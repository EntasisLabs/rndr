//! `XGrabKey` on every root window, through x11rb.
//!
//! The worker blocks in `poll` on the X socket and a wake pipe. Num Lock,
//! Caps Lock, and Mod5 are grabbed as extra masks so a lock state still
//! matches. Detectable auto-repeat is requested on this connection. Further
//! presses while the key is held are repeats, and so is a release followed by
//! a press that shares its timestamp when detectable auto-repeat is absent.

use std::collections::HashMap;
use std::os::fd::AsRawFd;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use rndr_core::hotkey::{HotkeyBackend, HotkeyFailure, HotkeyId, PlatformAccel};
use x11rb::connection::Connection;
use x11rb::errors::ReplyError;
use x11rb::protocol::xproto::{ConnectionExt as _, GrabMode, KeyButMask, Keycode, ModMask, Window};
use x11rb::protocol::{xkb, ErrorKind, Event};
use x11rb::rust_connection::RustConnection;

use super::press::PressQueue;
use super::repeat::RepeatFilter;

const SHIFT: u32 = 1 << 0;
const CONTROL: u32 = 1 << 2;
const ALT: u32 = 1 << 3;
const SUPER: u32 = 1 << 6;
const CARE: u32 = SHIFT | CONTROL | ALT | SUPER;

enum Cmd {
    Grab {
        id: u32,
        keysym: u32,
        modifiers: u32,
        masks: [u32; 8],
        reply: Sender<Result<(), HotkeyFailure>>,
    },
    Ungrab {
        id: u32,
        reply: Sender<()>,
    },
    Stop,
}

struct Grabbed {
    keycode: Keycode,
    modifiers: u32,
    masks: [u32; 8],
}

pub(super) struct X11Hotkeys {
    tx: Sender<Cmd>,
    wake: i32,
    thread: Option<JoinHandle<()>>,
    presses: Arc<PressQueue>,
}

impl X11Hotkeys {
    pub(super) fn open(wake: Arc<dyn Fn() + Send + Sync>) -> Result<Self, ()> {
        let presses = Arc::new(PressQueue::new(wake));
        let (tx, rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let presses_thread = Arc::clone(&presses);
        let thread = thread::spawn(move || worker(rx, presses_thread, ready_tx));
        let wake_fd = ready_rx.recv().map_err(|_| ())?;
        if wake_fd < 0 {
            let _ = thread.join();
            return Err(());
        }
        Ok(Self {
            tx,
            wake: wake_fd,
            thread: Some(thread),
            presses,
        })
    }

    fn poke(&self) {
        let byte = [1u8];
        unsafe {
            libc::write(self.wake, byte.as_ptr().cast(), 1);
        }
    }
}

impl HotkeyBackend for X11Hotkeys {
    fn register(&mut self, id: HotkeyId, accel: &PlatformAccel) -> Result<(), HotkeyFailure> {
        let Some(x11) = accel.x11 else {
            return Err(HotkeyFailure::Invalid);
        };
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Cmd::Grab {
                id: id.get(),
                keysym: x11.keysym,
                modifiers: x11.modifiers,
                masks: x11.grab_masks,
                reply: reply_tx,
            })
            .map_err(|_| HotkeyFailure::Unsupported)?;
        self.poke();
        reply_rx.recv().unwrap_or(Err(HotkeyFailure::Unsupported))
    }

    fn unregister(&mut self, id: HotkeyId) {
        let (reply_tx, reply_rx) = mpsc::channel();
        if self
            .tx
            .send(Cmd::Ungrab {
                id: id.get(),
                reply: reply_tx,
            })
            .is_ok()
        {
            self.poke();
            let _ = reply_rx.recv();
        }
    }

    fn poll(&mut self) -> Vec<HotkeyId> {
        self.presses.drain()
    }
}

impl Drop for X11Hotkeys {
    fn drop(&mut self) {
        let _ = self.tx.send(Cmd::Stop);
        self.poke();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn worker(rx: Receiver<Cmd>, presses: Arc<PressQueue>, ready: Sender<i32>) {
    let connection = match RustConnection::connect(None) {
        Ok((conn, _screen)) => conn,
        Err(_) => {
            let _ = ready.send(-1);
            return;
        }
    };
    request_detectable_repeat(&connection);
    let roots: Vec<Window> = connection
        .setup()
        .roots
        .iter()
        .map(|screen| screen.root)
        .collect();
    let xfd = connection.stream().as_raw_fd();
    let mut pipes = [0; 2];
    let pipe_ok = unsafe { libc::pipe(pipes.as_mut_ptr()) } == 0;
    if !pipe_ok {
        let _ = ready.send(-1);
        return;
    }
    set_cloexec_nonblock(pipes[0]);
    set_cloexec_nonblock(pipes[1]);
    let _ = ready.send(pipes[1]);

    let mut grabbed: HashMap<u32, Grabbed> = HashMap::new();
    let mut repeats = RepeatFilter::default();
    loop {
        let mut fds = [
            libc::pollfd {
                fd: xfd,
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: pipes[0],
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        let polled = unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) };
        if polled < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            break;
        }
        if fds[1].revents & libc::POLLIN != 0 {
            drain_pipe(pipes[0]);
            let mut stop = false;
            while let Ok(cmd) = rx.try_recv() {
                match cmd {
                    Cmd::Grab {
                        id,
                        keysym,
                        modifiers,
                        masks,
                        reply,
                    } => {
                        let result = grab(
                            &connection,
                            &roots,
                            &mut grabbed,
                            id,
                            keysym,
                            modifiers,
                            masks,
                        );
                        let _ = reply.send(result);
                    }
                    Cmd::Ungrab { id, reply } => {
                        ungrab(&connection, &roots, &mut grabbed, id);
                        let _ = reply.send(());
                    }
                    Cmd::Stop => {
                        stop = true;
                        break;
                    }
                }
            }
            if stop {
                break;
            }
        }
        if fds[0].revents & (libc::POLLIN | libc::POLLERR | libc::POLLHUP) != 0 {
            loop {
                match connection.poll_for_event() {
                    Ok(Some(event)) => handle_event(event, &grabbed, &mut repeats, &presses),
                    Ok(None) => break,
                    Err(_) => return,
                }
            }
        }
    }
    unsafe {
        libc::close(pipes[0]);
        libc::close(pipes[1]);
    }
}

/// Ask the server to send real holds: extra presses, no synthetic release.
/// A server that declines still produces the same-timestamp pair [`RepeatFilter`] drops.
fn request_detectable_repeat(conn: &RustConnection) {
    let Ok(cookie) = xkb::ConnectionExt::xkb_use_extension(conn, 1, 0) else {
        return;
    };
    if cookie.reply().is_err() {
        return;
    }
    let Ok(cookie) = xkb::ConnectionExt::xkb_per_client_flags(
        conn,
        u16::from(xkb::ID::USE_CORE_KBD),
        xkb::PerClientFlag::DETECTABLE_AUTO_REPEAT,
        xkb::PerClientFlag::DETECTABLE_AUTO_REPEAT,
        xkb::BoolCtrl::default(),
        xkb::BoolCtrl::default(),
        xkb::BoolCtrl::default(),
    ) else {
        return;
    };
    let _ = cookie.reply();
}

fn grab(
    conn: &RustConnection,
    roots: &[Window],
    grabbed: &mut HashMap<u32, Grabbed>,
    id: u32,
    keysym: u32,
    modifiers: u32,
    masks: [u32; 8],
) -> Result<(), HotkeyFailure> {
    let Some(keycode) = keycode_for(conn, keysym)? else {
        return Err(HotkeyFailure::Invalid);
    };
    for root in roots {
        for mask in masks {
            let cookie = conn
                .grab_key(
                    false,
                    *root,
                    mod_mask(mask),
                    keycode,
                    GrabMode::ASYNC,
                    GrabMode::ASYNC,
                )
                .map_err(|_| HotkeyFailure::Unsupported)?;
            if let Err(err) = cookie.check() {
                ungrab_codes(conn, roots, keycode, &masks);
                return Err(map_grab_error(err));
            }
        }
    }
    grabbed.insert(
        id,
        Grabbed {
            keycode,
            modifiers,
            masks,
        },
    );
    Ok(())
}

fn ungrab(conn: &RustConnection, roots: &[Window], grabbed: &mut HashMap<u32, Grabbed>, id: u32) {
    if let Some(grab) = grabbed.remove(&id) {
        ungrab_codes(conn, roots, grab.keycode, &grab.masks);
    }
}

fn ungrab_codes(conn: &RustConnection, roots: &[Window], keycode: Keycode, masks: &[u32]) {
    for root in roots {
        for mask in masks {
            if let Ok(cookie) = conn.ungrab_key(keycode, *root, mod_mask(*mask)) {
                let _ = cookie.check();
            }
        }
    }
}

fn map_grab_error(err: ReplyError) -> HotkeyFailure {
    match err {
        ReplyError::X11Error(error) if error.error_kind == ErrorKind::Access => {
            HotkeyFailure::Taken
        }
        _ => HotkeyFailure::Unsupported,
    }
}

fn keycode_for(conn: &RustConnection, keysym: u32) -> Result<Option<Keycode>, HotkeyFailure> {
    let setup = conn.setup();
    let min = setup.min_keycode;
    let max = setup.max_keycode;
    let count = max.wrapping_sub(min).wrapping_add(1);
    let mapping = conn
        .get_keyboard_mapping(min, count)
        .map_err(|_| HotkeyFailure::Unsupported)?
        .reply()
        .map_err(|_| HotkeyFailure::Unsupported)?;
    let per = mapping.keysyms_per_keycode as usize;
    if per == 0 {
        return Ok(None);
    }
    for (index, keysyms) in mapping.keysyms.chunks(per).enumerate() {
        if keysyms.contains(&keysym) {
            return Ok(Some(min.wrapping_add(index as u8)));
        }
    }
    Ok(None)
}

fn handle_event(
    event: Event,
    grabbed: &HashMap<u32, Grabbed>,
    repeats: &mut RepeatFilter,
    presses: &PressQueue,
) {
    match event {
        Event::KeyRelease(event) => {
            repeats.release(u32::from(event.detail), u64::from(event.time));
        }
        Event::KeyPress(event) => {
            if !repeats.press(u32::from(event.detail), u64::from(event.time)) {
                return;
            }
            let state = key_but_mask_bits(event.state) & CARE;
            for (id, grab) in grabbed {
                if grab.keycode == event.detail && grab.modifiers == state {
                    presses.push(HotkeyId::from_raw(*id));
                }
            }
        }
        _ => {}
    }
}

fn mod_mask(bits: u32) -> ModMask {
    ModMask::from(bits as u16)
}

fn key_but_mask_bits(mask: KeyButMask) -> u32 {
    u32::from(u16::from(mask))
}

fn set_cloexec_nonblock(fd: i32) {
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL);
        if flags >= 0 {
            libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
        }
        let flags = libc::fcntl(fd, libc::F_GETFD);
        if flags >= 0 {
            libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC);
        }
    }
}

fn drain_pipe(fd: i32) {
    let mut buf = [0u8; 64];
    loop {
        let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
        if n <= 0 {
            break;
        }
    }
}
