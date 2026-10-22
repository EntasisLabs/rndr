//! Carbon `RegisterEventHotKey`. No Accessibility permission.
//!
//! Presses are delivered to the application event target, which winit's Cocoa
//! run loop already pumps. The handler posts into the press queue and wakes
//! winit; it does not take the backend lock.

use std::collections::HashMap;
use std::ffi::c_void;
use std::os::raw::{c_ulong, c_void as RawVoid};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use rndr_core::hotkey::{HotkeyBackend, HotkeyFailure, HotkeyId, PlatformAccel};

use super::press::PressQueue;

const SIGNATURE: u32 = 0x726e_6472; // 'rndr'
const EVENT_HOT_KEY_EXISTS: i32 = -9878;
const PARAM_ERR: i32 = -50;

type UInt32 = u32;
type OSStatus = i32;
type ByteCount = c_ulong;
type ItemCount = c_ulong;
type EventRef = *mut c_void;
type EventHandlerRef = *mut c_void;
type EventHandlerCallRef = *mut c_void;
type EventTargetRef = *mut c_void;
type EventHotKeyRef = *mut c_void;
type EventHandlerUPP =
    Option<unsafe extern "C" fn(EventHandlerCallRef, EventRef, *mut c_void) -> OSStatus>;

#[repr(C, packed(2))]
#[derive(Clone, Copy)]
struct EventHotKeyID {
    signature: UInt32,
    id: UInt32,
}

#[repr(C, packed(2))]
#[derive(Clone, Copy)]
struct EventTypeSpec {
    event_class: UInt32,
    event_kind: UInt32,
}

#[link(name = "Carbon", kind = "framework")]
extern "C" {
    fn GetApplicationEventTarget() -> EventTargetRef;
    fn RegisterEventHotKey(
        key_code: UInt32,
        modifiers: UInt32,
        hotkey_id: EventHotKeyID,
        target: EventTargetRef,
        options: UInt32,
        out_ref: *mut EventHotKeyRef,
    ) -> OSStatus;
    fn UnregisterEventHotKey(hotkey: EventHotKeyRef) -> OSStatus;
    fn InstallEventHandler(
        target: EventTargetRef,
        handler: EventHandlerUPP,
        num_types: ItemCount,
        types: *const EventTypeSpec,
        user_data: *mut c_void,
        out_ref: *mut EventHandlerRef,
    ) -> OSStatus;
    fn RemoveEventHandler(handler: EventHandlerRef) -> OSStatus;
    fn GetEventParameter(
        event: EventRef,
        name: UInt32,
        desired_type: UInt32,
        actual_type: *mut UInt32,
        buffer_size: ByteCount,
        actual_size: *mut ByteCount,
        data: *mut RawVoid,
    ) -> OSStatus;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRunLoopGetMain() -> *mut c_void;
    fn CFRunLoopGetCurrent() -> *mut c_void;
}

#[link(name = "System")]
extern "C" {
    // `dispatch_get_main_queue()` is a macro for the address of this object.
    static _dispatch_main_q: u8;
    fn dispatch_sync_f(
        queue: *mut c_void,
        context: *mut c_void,
        work: Option<unsafe extern "C" fn(*mut c_void)>,
    );
}

fn main_queue() -> *mut c_void {
    std::ptr::addr_of!(_dispatch_main_q) as *mut c_void
}

static PRESSES: Mutex<Option<Arc<PressQueue>>> = Mutex::new(None);
static HANDLER: Mutex<Option<usize>> = Mutex::new(None);

pub(super) struct MacHotkeys {
    presses: Arc<PressQueue>,
    keys: HashMap<u32, usize>,
}

// EventHotKeyRef is only passed back to Carbon on the main thread.
unsafe impl Send for MacHotkeys {}

impl MacHotkeys {
    pub(super) fn open(wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let presses = Arc::new(PressQueue::new(wake));
        *PRESSES.lock().unwrap_or_else(|err| err.into_inner()) = Some(Arc::clone(&presses));
        on_main(install_handler);
        Self {
            presses,
            keys: HashMap::new(),
        }
    }
}

impl HotkeyBackend for MacHotkeys {
    fn register(&mut self, id: HotkeyId, accel: &PlatformAccel) -> Result<(), HotkeyFailure> {
        let Some(mac) = accel.macos else {
            return Err(HotkeyFailure::Invalid);
        };
        let raw = id.get();
        let key_code = mac.key_code;
        let modifiers = mac.modifiers;
        let ptr = on_main(move || {
            carbon_register(raw, key_code, modifiers).map(|hotkey| hotkey as usize)
        })?;
        self.keys.insert(raw, ptr);
        Ok(())
    }

    fn unregister(&mut self, id: HotkeyId) {
        if let Some(ptr) = self.keys.remove(&id.get()) {
            on_main(move || unsafe {
                UnregisterEventHotKey(ptr as EventHotKeyRef);
            });
        }
    }

    fn poll(&mut self) -> Vec<HotkeyId> {
        self.presses.drain()
    }
}

impl Drop for MacHotkeys {
    fn drop(&mut self) {
        let keys = std::mem::take(&mut self.keys);
        on_main(move || unsafe {
            for ptr in keys.into_values() {
                UnregisterEventHotKey(ptr as EventHotKeyRef);
            }
            if let Some(handler) = HANDLER.lock().unwrap_or_else(|err| err.into_inner()).take() {
                RemoveEventHandler(handler as EventHandlerRef);
            }
        });
    }
}

fn install_handler() {
    let mut slot = HANDLER.lock().unwrap_or_else(|err| err.into_inner());
    if slot.is_some() {
        return;
    }
    unsafe {
        let spec = EventTypeSpec {
            event_class: 0x6b65_7962, // 'keyb'
            event_kind: 5,            // kEventHotKeyPressed
        };
        let mut handler: EventHandlerRef = std::ptr::null_mut();
        let status = InstallEventHandler(
            GetApplicationEventTarget(),
            Some(on_hotkey),
            1,
            &spec,
            std::ptr::null_mut(),
            &mut handler,
        );
        if status == 0 && !handler.is_null() {
            *slot = Some(handler as usize);
        }
    }
}

unsafe extern "C" fn on_hotkey(
    _call: EventHandlerCallRef,
    event: EventRef,
    _user: *mut c_void,
) -> OSStatus {
    let mut hotkey = EventHotKeyID {
        signature: 0,
        id: 0,
    };
    let status = GetEventParameter(
        event,
        0x2d2d_2d2d, // '----'
        0x686b_6964, // 'hkid'
        std::ptr::null_mut(),
        std::mem::size_of::<EventHotKeyID>() as ByteCount,
        std::ptr::null_mut(),
        &mut hotkey as *mut EventHotKeyID as *mut RawVoid,
    );
    if status == 0 && hotkey.signature == SIGNATURE {
        if let Some(presses) = PRESSES
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .as_ref()
        {
            presses.push(HotkeyId::from_raw(hotkey.id));
        }
    }
    0
}

fn carbon_register(
    id: u32,
    key_code: u32,
    modifiers: u32,
) -> Result<EventHotKeyRef, HotkeyFailure> {
    unsafe {
        let mut hotkey: EventHotKeyRef = std::ptr::null_mut();
        let status = RegisterEventHotKey(
            key_code,
            modifiers,
            EventHotKeyID {
                signature: SIGNATURE,
                id,
            },
            GetApplicationEventTarget(),
            0,
            &mut hotkey,
        );
        match status {
            0 if !hotkey.is_null() => Ok(hotkey),
            EVENT_HOT_KEY_EXISTS => Err(HotkeyFailure::Taken),
            PARAM_ERR => Err(HotkeyFailure::Invalid),
            _ => Err(HotkeyFailure::Unsupported),
        }
    }
}

fn is_main_thread() -> bool {
    unsafe { CFRunLoopGetMain() == CFRunLoopGetCurrent() }
}

struct Job {
    work: Box<dyn FnOnce() + Send>,
}

fn on_main<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    if is_main_thread() {
        return work();
    }
    let (tx, rx) = mpsc::sync_channel(1);
    let job = Box::new(Job {
        work: Box::new(move || {
            let _ = tx.send(work());
        }),
    });
    unsafe {
        dispatch_sync_f(
            main_queue(),
            Box::into_raw(job) as *mut c_void,
            Some(trampoline),
        );
    }
    rx.recv().expect("main queue ran the hotkey job")
}

unsafe extern "C" fn trampoline(context: *mut c_void) {
    let job = Box::from_raw(context as *mut Job);
    (job.work)();
}
