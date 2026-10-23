//! PTY, parser, and the terminal state machine.
//!
//! The reader thread appends bytes and sets an end-of-file flag. It stops
//! reading once [`INCOMING_CAP`] bytes are waiting, so the kernel buffer
//! applies backpressure and a flood cannot grow without bound. The UI thread
//! drains at most one frame budget, advances the parser, and drops the session
//! to kill the child and join the reader.

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::Side;
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::{Config as TermOptions, Term, TermMode};
use alacritty_terminal::vte::ansi::{Processor, Rgb};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use rndr_core::Wake;

use crate::screen;

const FRAME_BUDGET: usize = 256 * 1024;
/// Bytes kept ahead of the parser. A full queue blocks the reader so the PTY
/// buffer fills and the child blocks in `write`. One frame drains the cap.
const INCOMING_CAP: usize = 64 * 1024;

pub struct Listener {
    shared: Arc<Shared>,
    reply: Arc<Mutex<Reply>>,
}

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        match event {
            Event::PtyWrite(text) => note(&self.shared, Note::Write(text.into_bytes())),
            Event::Title(title) => note(&self.shared, Note::Title(title)),
            Event::ResetTitle => note(&self.shared, Note::Title(String::new())),
            Event::ClipboardStore(_, text) => note(&self.shared, Note::Copy(text)),
            Event::ClipboardLoad(_, reply) => {
                let clip = lock(&self.reply).clipboard.clone();
                note(&self.shared, Note::Write(reply(&clip).into_bytes()));
            }
            Event::ColorRequest(index, reply) => {
                let rgb = lock(&self.reply).color(index);
                note(&self.shared, Note::Write(reply(rgb).into_bytes()));
            }
            Event::TextAreaSizeRequest(reply) => {
                let size = lock(&self.reply).size;
                note(&self.shared, Note::Write(reply(size).into_bytes()));
            }
            Event::Bell => note(&self.shared, Note::Bell),
            Event::Wakeup | Event::MouseCursorDirty | Event::CursorBlinkingChange => {
                self.shared.wake.wake();
            }
            Event::Exit | Event::ChildExit(_) => {
                self.shared.eof.store(true, Ordering::Release);
                self.shared.wake.wake();
            }
        }
    }
}

struct Reply {
    size: WindowSize,
    clipboard: String,
    colors: Vec<Option<Rgb>>,
}

impl Reply {
    fn color(&self, index: usize) -> Rgb {
        self.colors
            .get(index)
            .and_then(|color| *color)
            .unwrap_or(Rgb { r: 0, g: 0, b: 0 })
    }
}

enum Note {
    Write(Vec<u8>),
    Title(String),
    Copy(String),
    Bell,
}

struct Shared {
    incoming: Mutex<VecDeque<u8>>,
    /// Signaled when the queue has room or the session is shutting down.
    space: Condvar,
    notes: Mutex<Vec<Note>>,
    eof: AtomicBool,
    shutdown: AtomicBool,
    wake: Arc<dyn Wake>,
}

fn note(shared: &Shared, item: Note) {
    lock(&shared.notes).push(item);
    shared.wake.wake();
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|err| err.into_inner())
}

pub struct TermSize {
    columns: usize,
    screen_lines: usize,
}

impl Dimensions for TermSize {
    fn total_lines(&self) -> usize {
        self.screen_lines
    }

    fn screen_lines(&self) -> usize {
        self.screen_lines
    }

    fn columns(&self) -> usize {
        self.columns
    }
}

pub struct OpenOptions {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<std::path::PathBuf>,
    pub scrollback: usize,
    pub cols: u16,
    pub rows: u16,
    pub offline: bool,
    pub wake: Arc<dyn Wake>,
}

pub struct Session {
    shared: Arc<Shared>,
    reply: Arc<Mutex<Reply>>,
    writer: Option<Box<dyn Write + Send>>,
    master: Option<Box<dyn MasterPty + Send>>,
    child: Option<Box<dyn Child + Send + Sync>>,
    pub term: Term<Listener>,
    parser: Processor,
    thread: Option<JoinHandle<()>>,
    pub cols: u16,
    pub rows: u16,
    pub cell_w: f32,
    pub cell_h: f32,
    reaped: bool,
    pub selecting: bool,
    pub cursor_blinks: bool,
    /// When set, the cell pitch stays at the value passed to [`Session::open`].
    pub pin_cell: bool,
    /// Bytes written while no PTY is attached.
    pub offline_out: Vec<u8>,
}

pub struct Pump {
    pub title: Option<String>,
    pub bell: bool,
    pub copy: Option<String>,
    pub exit: Option<Option<i32>>,
}

impl Session {
    pub fn open(options: OpenOptions) -> Result<Self, String> {
        let cols = options.cols.max(2);
        let rows = options.rows.max(1);
        let shared = Arc::new(Shared {
            incoming: Mutex::new(VecDeque::new()),
            space: Condvar::new(),
            notes: Mutex::new(Vec::new()),
            eof: AtomicBool::new(false),
            shutdown: AtomicBool::new(false),
            wake: options.wake,
        });
        let reply = Arc::new(Mutex::new(Reply {
            size: window_size(cols, rows, 16.0, 16.0),
            clipboard: String::new(),
            colors: vec![None; 269],
        }));
        let listener = Listener {
            shared: Arc::clone(&shared),
            reply: Arc::clone(&reply),
        };
        let term_options = TermOptions {
            scrolling_history: options.scrollback,
            ..TermOptions::default()
        };
        let term = Term::new(
            term_options,
            &TermSize {
                columns: cols as usize,
                screen_lines: rows as usize,
            },
            listener,
        );
        let parser = Processor::new();
        if options.offline {
            return Ok(Self {
                shared,
                reply,
                writer: None,
                master: None,
                child: None,
                term,
                parser,
                thread: None,
                cols,
                rows,
                cell_w: 16.0,
                cell_h: 16.0,
                reaped: true,
                selecting: false,
                cursor_blinks: false,
                pin_cell: false,
                offline_out: Vec::new(),
            });
        }

        let pty = native_pty_system();
        let size = pty_size(cols, rows, 16.0, 16.0);
        let pair = pty.openpty(size).map_err(|err| err.to_string())?;
        let mut command = CommandBuilder::new(options.program);
        for arg in &options.args {
            command.arg(arg);
        }
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        if let Some(cwd) = &options.cwd {
            command.cwd(cwd);
        }
        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|err| err.to_string())?;
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|err| err.to_string())?;
        let writer = pair.master.take_writer().map_err(|err| err.to_string())?;
        let thread_shared = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("rndr-term".into())
            .spawn(move || read_loop(reader, thread_shared))
            .map_err(|err| err.to_string())?;
        Ok(Self {
            shared,
            reply,
            writer: Some(writer),
            master: Some(pair.master),
            child: Some(child),
            term,
            parser,
            thread: Some(thread),
            cols,
            rows,
            cell_w: 16.0,
            cell_h: 16.0,
            reaped: false,
            selecting: false,
            cursor_blinks: false,
            pin_cell: false,
            offline_out: Vec::new(),
        })
    }

    pub fn resize(&mut self, cols: u16, rows: u16, cell_w: f32, cell_h: f32) {
        let cols = cols.max(2);
        let rows = rows.max(1);
        if cols == self.cols && rows == self.rows {
            let pitch = (cell_w - self.cell_w).abs() > 0.5 || (cell_h - self.cell_h).abs() > 0.5;
            self.cell_w = cell_w;
            self.cell_h = cell_h;
            if pitch {
                if let Some(master) = &self.master {
                    let _ = master.resize(pty_size(cols, rows, cell_w, cell_h));
                }
                lock(&self.reply).size = window_size(cols, rows, cell_w, cell_h);
            }
            return;
        }
        self.cols = cols;
        self.rows = rows;
        self.cell_w = cell_w;
        self.cell_h = cell_h;
        self.term.resize(TermSize {
            columns: cols as usize,
            screen_lines: rows as usize,
        });
        if let Some(master) = &self.master {
            let _ = master.resize(pty_size(cols, rows, cell_w, cell_h));
        }
        lock(&self.reply).size = window_size(cols, rows, cell_w, cell_h);
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        self.term.scroll_display(Scroll::Bottom);
        self.clear_selection();
        self.write_raw(bytes);
    }

    fn write_raw(&mut self, bytes: &[u8]) {
        let Some(writer) = self.writer.as_mut() else {
            self.offline_out.extend_from_slice(bytes);
            return;
        };
        if writer.write_all(bytes).is_ok() {
            let _ = writer.flush();
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        self.parser.advance(&mut self.term, bytes);
    }

    /// Bytes still waiting. Only the Unix flood test reads this; Windows
    /// clippy treats a test-only helper with no caller as dead code.
    #[cfg(all(test, unix))]
    fn queued_len(&self) -> usize {
        lock(&self.shared.incoming).len()
    }

    pub fn pump(&mut self, clipboard: &str) -> Pump {
        let bytes = drain(&self.shared);
        self.prepare_reply(clipboard);
        if !bytes.is_empty() {
            self.parser.advance(&mut self.term, &bytes);
        }
        let (title, bell, copy) = self.flush_notes();
        let exit = self.poll_exit();
        self.cursor_blinks = self.term.cursor_style().blinking;
        Pump {
            title,
            bell,
            copy,
            exit,
        }
    }

    pub fn modes(&self) -> TermMode {
        *self.term.mode()
    }

    pub fn writer_open(&self) -> bool {
        self.writer.is_some()
    }

    pub fn alt_scroll(&self) -> bool {
        let mode = self.modes();
        mode.contains(TermMode::ALT_SCREEN) && mode.contains(TermMode::ALTERNATE_SCROLL)
    }

    pub fn scroll_by(&mut self, lines: i32) {
        if lines != 0 {
            self.term.scroll_display(Scroll::Delta(lines));
        }
    }

    pub fn scroll_page(&mut self, up: bool) {
        let lines = (self.rows.saturating_sub(1) as i32).max(1);
        self.scroll_by(if up { lines } else { -lines });
    }

    pub fn selection_text(&self) -> Option<String> {
        self.term
            .selection_to_string()
            .filter(|text| !text.is_empty())
    }

    pub fn start_selection(&mut self, col: usize, row: usize, extend: bool) {
        let point = screen::cell_to_point(&self.term, col, row);
        if extend {
            if let Some(selection) = self.term.selection.as_mut() {
                selection.update(point, Side::Right);
                return;
            }
        }
        self.term.selection = Some(Selection::new(SelectionType::Simple, point, Side::Left));
        self.selecting = true;
    }

    pub fn drag_selection(&mut self, col: usize, row: usize) {
        let point = screen::cell_to_point(&self.term, col, row);
        if let Some(selection) = self.term.selection.as_mut() {
            selection.update(point, Side::Right);
        }
    }

    pub fn clear_selection(&mut self) {
        self.term.selection = None;
        self.selecting = false;
    }

    fn prepare_reply(&mut self, clipboard: &str) {
        let mut reply = lock(&self.reply);
        reply.clipboard = clipboard.to_string();
        reply.size = window_size(self.cols, self.rows, self.cell_w, self.cell_h);
        let colors = self.term.colors();
        for (index, slot) in reply.colors.iter_mut().enumerate() {
            *slot = screen::color_slot(colors, index);
        }
    }

    fn flush_notes(&mut self) -> (Option<String>, bool, Option<String>) {
        let notes = std::mem::take(&mut *lock(&self.shared.notes));
        let mut title = None;
        let mut bell = false;
        let mut copy = None;
        let mut pending = Vec::new();
        for note in notes {
            match note {
                Note::Title(text) => title = Some(text),
                Note::Bell => bell = true,
                Note::Copy(text) => copy = Some(text),
                Note::Write(bytes) => pending.extend(bytes),
            }
        }
        if !pending.is_empty() {
            self.write_raw(&pending);
        }
        (title, bell, copy)
    }

    fn poll_exit(&mut self) -> Option<Option<i32>> {
        if self.reaped {
            return None;
        }
        let child = self.child.as_mut()?;
        match child.try_wait() {
            Ok(Some(status)) => {
                self.reaped = true;
                let code = if status.signal().is_some() {
                    None
                } else {
                    Some(status.exit_code() as i32)
                };
                Some(code)
            }
            Ok(None) => None,
            Err(_) => {
                self.reaped = true;
                Some(None)
            }
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.shared.shutdown.store(true, Ordering::Release);
        self.shared.space.notify_one();
        if !self.reaped {
            #[cfg(unix)]
            if let Some(master) = &self.master {
                if let Some(pid) = master.process_group_leader() {
                    unsafe {
                        libc::kill(-pid, libc::SIGHUP);
                    }
                }
            }
            if let Some(child) = self.child.as_mut() {
                let _ = child.kill();
                let _ = child.wait();
                self.reaped = true;
            }
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn read_loop(mut reader: Box<dyn Read + Send>, shared: Arc<Shared>) {
    let mut buf = [0u8; 32 * 1024];
    loop {
        if shared.shutdown.load(Ordering::Acquire) {
            break;
        }
        let room = {
            let mut incoming = lock(&shared.incoming);
            loop {
                if shared.shutdown.load(Ordering::Acquire) {
                    finish_read(&shared);
                    return;
                }
                let room = INCOMING_CAP.saturating_sub(incoming.len());
                if room > 0 {
                    break room;
                }
                incoming = shared
                    .space
                    .wait(incoming)
                    .unwrap_or_else(|err| err.into_inner());
            }
        };
        let max = room.min(buf.len());
        match reader.read(&mut buf[..max]) {
            Ok(0) => break,
            Ok(n) => {
                lock(&shared.incoming).extend(buf[..n].iter().copied());
                shared.wake.wake();
            }
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    finish_read(&shared);
}

fn finish_read(shared: &Shared) {
    shared.eof.store(true, Ordering::Release);
    shared.wake.wake();
}

fn drain(shared: &Shared) -> Vec<u8> {
    let mut incoming = lock(&shared.incoming);
    if incoming.is_empty() {
        return Vec::new();
    }
    let n = incoming.len().min(FRAME_BUDGET);
    let out: Vec<u8> = incoming.drain(..n).collect();
    drop(incoming);
    shared.space.notify_one();
    out
}

fn pty_size(cols: u16, rows: u16, cell_w: f32, cell_h: f32) -> PtySize {
    PtySize {
        rows,
        cols,
        pixel_width: pixels(cell_w, cols),
        pixel_height: pixels(cell_h, rows),
    }
}

fn pixels(cell: f32, count: u16) -> u16 {
    let value = cell.max(1.0) * f32::from(count);
    if value.is_finite() {
        (value as u16).max(1)
    } else {
        1
    }
}

fn window_size(cols: u16, rows: u16, cell_w: f32, cell_h: f32) -> WindowSize {
    WindowSize {
        num_cols: cols,
        num_lines: rows,
        cell_width: pixels(cell_w, 1),
        cell_height: pixels(cell_h, 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    struct Flood;

    impl Read for Flood {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            buf.fill(b'x');
            Ok(buf.len())
        }
    }

    fn shared() -> Arc<Shared> {
        Arc::new(Shared {
            incoming: Mutex::new(VecDeque::new()),
            space: Condvar::new(),
            notes: Mutex::new(Vec::new()),
            eof: AtomicBool::new(false),
            shutdown: AtomicBool::new(false),
            wake: Arc::new(rndr_core::NopWake),
        })
    }

    #[test]
    fn a_flood_stops_at_the_cap_until_the_queue_is_drained() {
        let shared = shared();
        let reader = Arc::clone(&shared);
        let handle = std::thread::spawn(move || read_loop(Box::new(Flood), reader));
        let start = Instant::now();
        while lock(&shared.incoming).len() < INCOMING_CAP
            && start.elapsed() < Duration::from_secs(2)
        {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(lock(&shared.incoming).len(), INCOMING_CAP);
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(
            lock(&shared.incoming).len(),
            INCOMING_CAP,
            "the reader kept appending after the cap"
        );
        let drained = drain(&shared);
        assert_eq!(drained.len(), INCOMING_CAP);
        let start = Instant::now();
        while lock(&shared.incoming).is_empty() && start.elapsed() < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!lock(&shared.incoming).is_empty());
        assert!(lock(&shared.incoming).len() <= INCOMING_CAP);
        shared.shutdown.store(true, Ordering::Release);
        shared.space.notify_one();
        handle.join().expect("reader");
    }

    #[cfg(unix)]
    #[test]
    fn interrupt_during_a_flood_drains_within_a_second() {
        let wake: Arc<dyn Wake> = Arc::new(rndr_core::NopWake);
        let mut session = Session::open(OpenOptions {
            program: "sh".into(),
            args: vec![
                "-c".into(),
                "while true; do printf 'flood line with some text ABCDEFGHIJ\\n'; done".into(),
            ],
            cwd: None,
            scrollback: 1_000,
            cols: 80,
            rows: 24,
            offline: false,
            wake,
        })
        .expect("pty");
        let start = Instant::now();
        while session.queued_len() < INCOMING_CAP / 2 && start.elapsed() < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(session.queued_len() > 0, "flood produced no output");
        assert!(session.queued_len() <= INCOMING_CAP);
        // `Child::kill` reaps the process, which would hide the exit from `pump`.
        let pid = session
            .child
            .as_ref()
            .and_then(|child| child.process_id())
            .expect("pid");
        unsafe {
            libc::kill(pid as i32, libc::SIGKILL);
        }
        let recover = Instant::now();
        // `pump` reports the exit once. Bytes read after that still have to drain.
        let mut saw_exit = false;
        loop {
            let pump = session.pump("");
            saw_exit |= pump.exit.is_some();
            if saw_exit && session.queued_len() == 0 {
                break;
            }
            assert!(
                recover.elapsed() < Duration::from_secs(1),
                "queued {} after {:?}",
                session.queued_len(),
                recover.elapsed()
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
