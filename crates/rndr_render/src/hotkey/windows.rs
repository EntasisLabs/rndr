//! `RegisterHotKey` on a thread that blocks in `GetMessage`.
//!
//! The thread is created once. Register and unregister are posted to it.
//! `MOD_NOREPEAT` makes a held key one event. Idle is a blocked message wait.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use rndr_core::hotkey::{HotkeyBackend, HotkeyEdge, HotkeyFailure, HotkeyId, PlatformAccel};
use windows_sys::Win32::Foundation::{
    GetLastError, ERROR_HOTKEY_ALREADY_REGISTERED, LPARAM, WPARAM,
};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, MOD_NOREPEAT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetMessageW, PeekMessageW, PostQuitMessage, PostThreadMessageW, MSG, PM_NOREMOVE, WM_APP,
    WM_HOTKEY,
};

use super::press::PressQueue;

const WAKE: u32 = WM_APP;

enum Cmd {
    Register {
        id: i32,
        modifiers: u32,
        vk: u32,
        reply: Sender<Result<(), HotkeyFailure>>,
    },
    Unregister {
        id: i32,
        reply: Sender<()>,
    },
    Stop,
}

pub(super) struct WinHotkeys {
    tx: Sender<Cmd>,
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
    presses: Arc<PressQueue>,
}

impl WinHotkeys {
    pub(super) fn open(wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let presses = Arc::new(PressQueue::new(wake));
        let (tx, rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let presses_thread = Arc::clone(&presses);
        let thread = thread::spawn(move || worker(rx, presses_thread, ready_tx));
        let thread_id = ready_rx.recv().unwrap_or(0);
        Self {
            tx,
            thread_id,
            thread: Some(thread),
            presses,
        }
    }

    fn wake_thread(&self) {
        unsafe {
            PostThreadMessageW(self.thread_id, WAKE, 0, 0);
        }
    }
}

impl HotkeyBackend for WinHotkeys {
    fn register(&mut self, id: HotkeyId, accel: &PlatformAccel) -> Result<(), HotkeyFailure> {
        let Some(win) = accel.windows else {
            return Err(HotkeyFailure::Invalid);
        };
        let Ok(hotkey_id) = i32::try_from(id.get()) else {
            return Err(HotkeyFailure::Invalid);
        };
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Cmd::Register {
                id: hotkey_id,
                modifiers: win.modifiers | MOD_NOREPEAT,
                vk: win.vk,
                reply: reply_tx,
            })
            .map_err(|_| HotkeyFailure::Unsupported)?;
        self.wake_thread();
        reply_rx.recv().unwrap_or(Err(HotkeyFailure::Unsupported))
    }

    fn unregister(&mut self, id: HotkeyId) {
        let Ok(hotkey_id) = i32::try_from(id.get()) else {
            return;
        };
        let (reply_tx, reply_rx) = mpsc::channel();
        if self
            .tx
            .send(Cmd::Unregister {
                id: hotkey_id,
                reply: reply_tx,
            })
            .is_ok()
        {
            self.wake_thread();
            let _ = reply_rx.recv();
        }
    }

    fn poll(&mut self) -> Vec<HotkeyId> {
        // `RegisterHotKey` has no key-up. A later non-repeat of the base key
        // clears the text guard when this queue never reports a release.
        self.presses
            .drain()
            .into_iter()
            .filter_map(|edge| match edge {
                HotkeyEdge::Press(id) => Some(id),
                HotkeyEdge::Release(_) => None,
            })
            .collect()
    }
}

impl Drop for WinHotkeys {
    fn drop(&mut self) {
        let _ = self.tx.send(Cmd::Stop);
        self.wake_thread();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn worker(rx: Receiver<Cmd>, presses: Arc<PressQueue>, ready: Sender<u32>) {
    unsafe {
        let mut msg = zeroed_msg();
        PeekMessageW(&mut msg, std::ptr::null_mut(), WM_APP, WM_APP, PM_NOREMOVE);
        let _ = ready.send(GetCurrentThreadId());
        loop {
            let status = GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0);
            if status == 0 || status == -1 {
                break;
            }
            if msg.message == WM_HOTKEY {
                presses.push(HotkeyId::from_raw(msg.wParam as u32));
                continue;
            }
            if msg.message == WAKE {
                while let Ok(cmd) = rx.try_recv() {
                    if !handle_cmd(cmd) {
                        PostQuitMessage(0);
                    }
                }
            }
        }
    }
}

fn handle_cmd(cmd: Cmd) -> bool {
    match cmd {
        Cmd::Register {
            id,
            modifiers,
            vk,
            reply,
        } => {
            let result = unsafe { RegisterHotKey(std::ptr::null_mut(), id, modifiers, vk) };
            let mapped = if result != 0 {
                Ok(())
            } else if unsafe { GetLastError() } == ERROR_HOTKEY_ALREADY_REGISTERED {
                Err(HotkeyFailure::Taken)
            } else {
                Err(HotkeyFailure::Unsupported)
            };
            let _ = reply.send(mapped);
            true
        }
        Cmd::Unregister { id, reply } => {
            unsafe {
                UnregisterHotKey(std::ptr::null_mut(), id);
            }
            let _ = reply.send(());
            true
        }
        Cmd::Stop => false,
    }
}

fn zeroed_msg() -> MSG {
    MSG {
        hwnd: std::ptr::null_mut(),
        message: 0,
        wParam: 0 as WPARAM,
        lParam: 0 as LPARAM,
        time: 0,
        pt: windows_sys::Win32::Foundation::POINT { x: 0, y: 0 },
    }
}
