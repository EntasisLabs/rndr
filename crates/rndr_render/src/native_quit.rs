//! macOS Quit (menu, Cmd-Q, Dock) is not a winit CloseRequested event.
//! Add only the optional termination decision to winit's existing delegate;
//! do not replace its launch, input, or window lifecycle implementation.
use std::sync::{Mutex, OnceLock};

use crate::RunnerBus;
use objc2::runtime::{AnyClass, AnyObject, Bool, Imp, Sel};
use objc2::{class, msg_send, sel};

static QUIT: Mutex<Option<RunnerBus>> = Mutex::new(None);
static INSTALLED: OnceLock<()> = OnceLock::new();

#[link(name = "objc")]
extern "C" {
    fn class_addMethod(
        class: *const AnyClass,
        name: Sel,
        imp: Imp,
        types: *const std::ffi::c_char,
    ) -> Bool;
}

extern "C" fn should_terminate(_delegate: &AnyObject, _sel: Sel, _app: &AnyObject) -> usize {
    // NSApplicationTerminateReply: Cancel=0, Now=1. No UI or app code is
    // run reentrantly inside AppKit's termination decision.
    let quit = QUIT.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(proxy) = &*quit {
        proxy.quit();
        0
    } else {
        1
    }
}

pub(crate) fn configure(protected: bool, proxy: &RunnerBus) {
    if protected {
        INSTALLED.get_or_init(|| {
            // Called on the event-loop/main thread after winit installed its delegate.
            // The optional selector is absent in winit 0.30.13. Fail closed rather
            // than silently breaking an unknown future delegate implementation.
            unsafe {
                let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
                let delegate: *mut AnyObject = msg_send![app, delegate];
                let class = delegate
                    .as_ref()
                    .expect("winit application delegate")
                    .class();
                let selector = sel!(applicationShouldTerminate:);
                assert!(
                    class.instance_method(selector).is_none(),
                    "native close protection: unexpected termination delegate"
                );
                let imp = std::mem::transmute::<
                    extern "C" fn(&AnyObject, Sel, &AnyObject) -> usize,
                    Imp,
                >(should_terminate);
                assert!(
                    class_addMethod(class, selector, imp, c"Q@:@".as_ptr()).as_bool(),
                    "install native termination protection"
                );
            }
        });
    }
    *QUIT.lock().unwrap_or_else(|e| e.into_inner()) = protected.then(|| proxy.clone());
}

pub(crate) fn clear() {
    *QUIT.lock().unwrap_or_else(|e| e.into_inner()) = None;
}
