//! Native paste ingress guard. Script clipboard access is separately denied by
//! the opaque sandbox and WebKit preferences; this prevents a user Cmd-V from
//! delivering file/image clipboard objects to the guest. Native context menus
//! are suppressed by the artifact UI delegate. The privileged embedding host
//! must not add its own paste/file actions targeting the native child.

use crate::ArtifactError;
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{msg_send, ClassType};
use objc2_app_kit::{NSEvent, NSEventMask, NSEventModifierFlags, NSView};
use objc2_web_kit::WKWebView;
use std::ptr::NonNull;

pub(crate) struct PasteGuard(Retained<AnyObject>);
impl PasteGuard {
    pub(crate) fn new(web: Retained<WKWebView>) -> Result<Self, ArtifactError> {
        let mtm = objc2_foundation::MainThreadMarker::new()
            .ok_or(ArtifactError::Browser(crate::BrowserError::Thread))?;
        let handler = RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
            // AppKit calls local monitors on the UI thread, while both the
            // event and first responder are alive. Only this preview's focused
            // subtree is affected, not ordinary browsing or host controls.
            let key = unsafe { event.as_ref() };
            if unsafe { key.modifierFlags() }
                .contains(NSEventModifierFlags::NSEventModifierFlagCommand)
                && unsafe { key.charactersIgnoringModifiers() }
                    .map(|c| c.to_string().eq_ignore_ascii_case("v"))
                    .unwrap_or(false)
            {
                if let Some(responder) = unsafe { key.window(mtm) }.and_then(|w| w.firstResponder())
                {
                    let is_view: bool =
                        unsafe { msg_send![&responder, isKindOfClass: NSView::class()] };
                    if is_view {
                        let view = unsafe { &*Retained::as_ptr(&responder).cast::<NSView>() };
                        if unsafe { view.isDescendantOf(&web) } {
                            return std::ptr::null_mut();
                        }
                    }
                }
            }
            event.as_ptr()
        });
        let monitor = unsafe {
            NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &handler)
        }
        .ok_or_else(|| ArtifactError::Policy("native paste ingress guard unavailable".into()))?;
        Ok(Self(monitor))
    }
}
impl Drop for PasteGuard {
    fn drop(&mut self) {
        unsafe { NSEvent::removeMonitor(&self.0) };
    }
}
