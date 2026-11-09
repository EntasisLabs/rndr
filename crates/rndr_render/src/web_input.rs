//! Browser text input through a hidden `<textarea>`.
//!
//! winit's web backend has no IME and cancels every key press on the canvas,
//! so the browser never composes text or fires `paste` there. While a
//! [`rndr_core::CapturesKeys`] entity has focus, this bridge focuses an
//! invisible text area at the caret instead:
//!
//! - Ordinary key presses are cancelled on the text area and re-dispatched to
//!   the canvas, so winit maps them exactly as before.
//! - Composition (IME, dead keys) stays in the text area and becomes
//!   [`ImeEvent::Preedit`] and [`ImeEvent::Commit`].
//! - Text inserted without a key press (emoji picker, dictation) commits.
//! - Primary+V and Shift+Insert let the browser fire `paste`. Its plain text
//!   and HTML reach [`rndr_core::Clipboard`] before the held key reaches the
//!   canvas, so paste reads them without a permission prompt.
//!
//! Focus moving between the canvas and the text area is hidden from winit;
//! focus leaving the page is reported as the canvas losing focus.

#[cfg(target_arch = "wasm32")]
pub(crate) use bridge::WebTextInput;

#[cfg(any(target_arch = "wasm32", test))]
/// Text the browser's `paste` event carried.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Pasted {
    pub(crate) text: String,
    pub(crate) html: String,
}

#[cfg(any(target_arch = "wasm32", test))]
/// Put pasted text and HTML on the clipboard, as a desktop pull would.
pub(crate) fn receive_paste(clip: &mut rndr_core::Clipboard, pasted: Pasted) {
    clip.html = pasted.html;
    clip.html_text.clone_from(&pasted.text);
    clip.text = pasted.text;
}

#[cfg(any(target_arch = "wasm32", test))]
/// Whether this key press asks the browser to paste.
pub(crate) fn is_paste_chord(key: &str, ctrl: bool, meta: bool, alt: bool, shift: bool) -> bool {
    let primary_v = (ctrl || meta) && !alt && key.eq_ignore_ascii_case("v");
    let shift_insert = shift && !ctrl && !meta && !alt && key == "Insert";
    primary_v || shift_insert
}

#[cfg(any(target_arch = "wasm32", test))]
/// Whether the input method owns this key press, so it must reach the text
/// area untouched and not the canvas. Chrome reports `Process` or key code 229
/// while composing; a dead key starts a composition.
pub(crate) fn belongs_to_ime(key: &str, key_code: u32, composing: bool) -> bool {
    composing || key == "Process" || key == "Dead" || key_code == 229
}

#[cfg(any(target_arch = "wasm32", test))]
/// Preedit with the caret after its last character; the browser does not
/// report a caret inside the composition.
pub(crate) fn preedit(text: String) -> rndr_core::ImeEvent {
    let end = text.len();
    let cursor = (!text.is_empty()).then_some((end, end));
    rndr_core::ImeEvent::Preedit(text, cursor)
}

#[cfg(target_arch = "wasm32")]
mod bridge {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Arc;

    use rndr_core::ImeEvent;
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::{JsCast, JsValue};
    use web_sys::{
        AddEventListenerOptions, ClipboardEvent, CompositionEvent, Event, EventTarget, FocusEvent,
        FocusEventInit, HtmlCanvasElement, HtmlTextAreaElement, InputEvent, KeyboardEvent,
        KeyboardEventInit,
    };
    use winit::platform::web::WindowExtWeb;
    use winit::window::Window;

    use super::{belongs_to_ime, is_paste_chord, preedit, Pasted};

    #[derive(Default)]
    struct Shared {
        events: Vec<ImeEvent>,
        paste: Option<Pasted>,
        /// The paste key press, held until the browser's `paste` event.
        held_paste: Option<KeyboardEvent>,
    }

    type Listener = Closure<dyn FnMut(Event)>;

    pub(crate) struct WebTextInput {
        area: HtmlTextAreaElement,
        canvas: HtmlCanvasElement,
        shared: Rc<RefCell<Shared>>,
        enabled: bool,
        listeners: Vec<(EventTarget, &'static str, bool, Listener)>,
    }

    impl WebTextInput {
        pub(crate) fn new(window: Arc<dyn Window>) -> Option<Self> {
            let canvas = window.canvas()?.clone();
            let document = canvas.owner_document()?;
            let area: HtmlTextAreaElement =
                document.create_element("textarea").ok()?.dyn_into().ok()?;
            for (name, value) in [
                ("autocomplete", "off"),
                ("autocorrect", "off"),
                ("autocapitalize", "off"),
                ("spellcheck", "false"),
                ("tabindex", "-1"),
                ("aria-hidden", "true"),
                ("data-rndr-input", ""),
            ] {
                let _ = area.set_attribute(name, value);
            }
            let style = area.style();
            for (name, value) in [
                ("position", "fixed"),
                ("left", "0px"),
                ("top", "0px"),
                ("width", "2px"),
                ("height", "16px"),
                ("padding", "0"),
                ("border", "0"),
                ("margin", "0"),
                ("outline", "none"),
                ("resize", "none"),
                ("overflow", "hidden"),
                ("opacity", "0"),
                ("pointer-events", "none"),
                ("font-size", "16px"),
                ("caret-color", "transparent"),
                ("background", "transparent"),
                ("color", "transparent"),
            ] {
                let _ = style.set_property(name, value);
            }
            document.body()?.append_child(&area).ok()?;
            let mut input = Self {
                area,
                canvas,
                shared: Rc::new(RefCell::new(Shared::default())),
                enabled: false,
                listeners: Vec::new(),
            };
            input.listen(window);
            Some(input)
        }

        fn add(
            &mut self,
            target: &EventTarget,
            kind: &'static str,
            capture: bool,
            listener: Listener,
        ) {
            let options = AddEventListenerOptions::new();
            options.set_capture(capture);
            let _ = target.add_event_listener_with_callback_and_add_event_listener_options(
                kind,
                listener.as_ref().unchecked_ref(),
                &options,
            );
            self.listeners
                .push((target.clone(), kind, capture, listener));
        }

        fn listen(&mut self, window: Arc<dyn Window>) {
            let area: EventTarget = self.area.clone().into();
            for kind in ["keydown", "keyup"] {
                let canvas = self.canvas.clone();
                let shared = Rc::clone(&self.shared);
                let listener = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
                    let Some(key) = event.dyn_ref::<KeyboardEvent>() else {
                        return;
                    };
                    if belongs_to_ime(&key.key(), key.key_code(), key.is_composing()) {
                        return;
                    }
                    let paste = is_paste_chord(
                        &key.key(),
                        key.ctrl_key(),
                        key.meta_key(),
                        key.alt_key(),
                        key.shift_key(),
                    );
                    if paste && key.type_() == "keydown" {
                        shared.borrow_mut().held_paste = Some(key.clone());
                        return;
                    }
                    key.prevent_default();
                    forward_key(&canvas, key);
                });
                self.add(&area, kind, false, listener);
            }

            {
                let shared = Rc::clone(&self.shared);
                let window = Arc::clone(&window);
                let listener = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
                    if let Some(event) = event.dyn_ref::<CompositionEvent>() {
                        shared
                            .borrow_mut()
                            .events
                            .push(preedit(event.data().unwrap_or_default()));
                        window.request_redraw();
                    }
                });
                self.add(&area, "compositionupdate", false, listener);
            }
            {
                let shared = Rc::clone(&self.shared);
                let window = Arc::clone(&window);
                let text_area = self.area.clone();
                let listener = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
                    if let Some(event) = event.dyn_ref::<CompositionEvent>() {
                        let mut shared = shared.borrow_mut();
                        shared.events.push(preedit(String::new()));
                        shared
                            .events
                            .push(ImeEvent::Commit(event.data().unwrap_or_default()));
                        text_area.set_value("");
                        window.request_redraw();
                    }
                });
                self.add(&area, "compositionend", false, listener);
            }
            {
                let shared = Rc::clone(&self.shared);
                let window = Arc::clone(&window);
                let text_area = self.area.clone();
                let listener = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
                    let composing = event
                        .dyn_ref::<InputEvent>()
                        .is_some_and(InputEvent::is_composing);
                    if composing {
                        return;
                    }
                    let text = text_area.value();
                    text_area.set_value("");
                    if !text.is_empty() {
                        shared.borrow_mut().events.push(ImeEvent::Commit(text));
                        window.request_redraw();
                    }
                });
                self.add(&area, "input", false, listener);
            }
            {
                let shared = Rc::clone(&self.shared);
                let window = Arc::clone(&window);
                let canvas = self.canvas.clone();
                let listener = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
                    let Some(paste) = event.dyn_ref::<ClipboardEvent>() else {
                        return;
                    };
                    event.prevent_default();
                    let data = paste.clipboard_data();
                    let read = |mime: &str| {
                        data.as_ref()
                            .and_then(|data| data.get_data(mime).ok())
                            .unwrap_or_default()
                    };
                    let pasted = Pasted {
                        text: read("text/plain"),
                        html: read("text/html"),
                    };
                    let held = {
                        let mut shared = shared.borrow_mut();
                        shared.paste = Some(pasted);
                        shared.held_paste.take()
                    };
                    match held {
                        Some(key) => forward_key(&canvas, &key),
                        None => {
                            forward_paste_chord(&canvas, "keydown");
                            forward_paste_chord(&canvas, "keyup");
                        }
                    }
                    window.request_redraw();
                });
                self.add(&area, "paste", false, listener);
            }

            // Focus moving between the canvas and the text area is internal.
            if let Some(page) = web_sys::window() {
                let page: EventTarget = page.into();
                for kind in ["focus", "blur"] {
                    let canvas = self.canvas.clone();
                    let text_area = self.area.clone();
                    let listener = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
                        let Some(focus) = event.dyn_ref::<FocusEvent>() else {
                            return;
                        };
                        let on_canvas = event
                            .target()
                            .is_some_and(|target| JsValue::from(target) == JsValue::from(&canvas));
                        let from_area = focus
                            .related_target()
                            .is_some_and(|other| JsValue::from(other) == JsValue::from(&text_area));
                        if on_canvas && from_area {
                            event.stop_immediate_propagation();
                        }
                    });
                    self.add(&page, kind, true, listener);
                }
            }
            // Focus entering or leaving the text area from outside the canvas
            // is the canvas gaining or losing focus.
            for kind in ["focus", "blur"] {
                let canvas = self.canvas.clone();
                let listener = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
                    let Some(focus) = event.dyn_ref::<FocusEvent>() else {
                        return;
                    };
                    let with_canvas = focus
                        .related_target()
                        .is_some_and(|other| JsValue::from(other) == JsValue::from(&canvas));
                    if !with_canvas {
                        let init = FocusEventInit::new();
                        if let Ok(copy) = FocusEvent::new_with_focus_event_init_dict(kind, &init) {
                            let _ = canvas.dispatch_event(&copy);
                        }
                    }
                });
                self.add(&area, kind, false, listener);
            }
        }

        /// Focus the text area while a text widget has focus, and hand focus
        /// back to the canvas when it loses it. Focus elsewhere on the page is
        /// left alone.
        pub(crate) fn set_allowed(&mut self, allow: bool) {
            let active = self
                .canvas
                .owner_document()
                .and_then(|document| document.active_element())
                .map(JsValue::from);
            let canvas_active = active.as_ref() == Some(&JsValue::from(&self.canvas));
            let area_active = active.as_ref() == Some(&JsValue::from(&self.area));
            if allow {
                if !self.enabled {
                    self.enabled = true;
                    self.shared.borrow_mut().events.push(ImeEvent::Enabled);
                }
                if canvas_active {
                    let _ = self.area.focus();
                }
            } else if self.enabled {
                self.enabled = false;
                self.area.set_value("");
                self.shared.borrow_mut().events.push(ImeEvent::Disabled);
                if area_active {
                    let _ = self.canvas.focus();
                }
            }
        }

        /// Place the text area over the caret, in canvas points, so the
        /// candidate window opens next to it.
        pub(crate) fn set_area(&self, x: f64, y: f64, height: f64) {
            let rect = self.canvas.get_bounding_client_rect();
            let style = self.area.style();
            let _ = style.set_property("left", &format!("{}px", rect.left() + x));
            let _ = style.set_property("top", &format!("{}px", rect.top() + y));
            let _ = style.set_property("height", &format!("{}px", height.max(1.0)));
        }

        /// Events since the last call, in the order the browser sent them.
        pub(crate) fn drain(&self, into: &mut Vec<ImeEvent>) {
            into.append(&mut self.shared.borrow_mut().events);
        }

        pub(crate) fn take_paste(&self) -> Option<Pasted> {
            self.shared.borrow_mut().paste.take()
        }
    }

    impl Drop for WebTextInput {
        fn drop(&mut self) {
            for (target, kind, capture, listener) in self.listeners.drain(..) {
                let _ = target.remove_event_listener_with_callback_and_bool(
                    kind,
                    listener.as_ref().unchecked_ref(),
                    capture,
                );
            }
            self.area.remove();
        }
    }

    fn key_init(kind_source: &KeyboardEvent) -> KeyboardEventInit {
        let init = KeyboardEventInit::new();
        init.set_bubbles(true);
        init.set_cancelable(true);
        init.set_key(&kind_source.key());
        init.set_code(&kind_source.code());
        init.set_location(kind_source.location());
        init.set_repeat(kind_source.repeat());
        init.set_ctrl_key(kind_source.ctrl_key());
        init.set_shift_key(kind_source.shift_key());
        init.set_alt_key(kind_source.alt_key());
        init.set_meta_key(kind_source.meta_key());
        init
    }

    /// Re-dispatch a text-area key press to the canvas for winit to map.
    fn forward_key(canvas: &HtmlCanvasElement, key: &KeyboardEvent) {
        let init = key_init(key);
        if let Ok(copy) = KeyboardEvent::new_with_keyboard_event_init_dict(&key.type_(), &init) {
            let _ = canvas.dispatch_event(&copy);
        }
    }

    /// A paste with no key press (the browser's own menu) still needs the
    /// paste chord so the focused widget pastes.
    fn forward_paste_chord(canvas: &HtmlCanvasElement, kind: &str) {
        let apple = crate::host_platform() == rndr_core::Platform::Mac;
        let init = KeyboardEventInit::new();
        init.set_bubbles(true);
        init.set_cancelable(true);
        init.set_key("v");
        init.set_code("KeyV");
        init.set_ctrl_key(!apple);
        init.set_meta_key(apple);
        if let Ok(copy) = KeyboardEvent::new_with_keyboard_event_init_dict(kind, &init) {
            let _ = canvas.dispatch_event(&copy);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rndr_core::{Clipboard, ImeEvent};

    #[test]
    fn paste_chords() {
        assert!(is_paste_chord("v", true, false, false, false));
        assert!(is_paste_chord("V", false, true, false, true));
        assert!(is_paste_chord("Insert", false, false, false, true));
        assert!(!is_paste_chord("v", false, false, false, false));
        assert!(!is_paste_chord("v", true, false, true, false));
        assert!(!is_paste_chord("c", true, false, false, false));
        assert!(!is_paste_chord("Insert", true, false, false, true));
    }

    #[test]
    fn ime_keys_stay_in_the_text_area() {
        assert!(belongs_to_ime("Process", 229, false));
        assert!(belongs_to_ime("a", 229, false));
        assert!(belongs_to_ime("Dead", 0, false));
        assert!(belongs_to_ime("a", 65, true));
        assert!(!belongs_to_ime("a", 65, false));
        assert!(!belongs_to_ime("Enter", 13, false));
    }

    #[test]
    fn preedit_caret_sits_at_the_end() {
        assert_eq!(
            preedit("にほ".into()),
            ImeEvent::Preedit("にほ".into(), Some((6, 6)))
        );
        assert_eq!(
            preedit(String::new()),
            ImeEvent::Preedit(String::new(), None)
        );
    }

    #[test]
    fn paste_fills_text_and_html() {
        let mut clip = Clipboard {
            text: "old".into(),
            html: "<b>old</b>".into(),
            html_text: "old".into(),
            ..Clipboard::default()
        };
        receive_paste(
            &mut clip,
            Pasted {
                text: "new".into(),
                html: "<i>new</i>".into(),
            },
        );
        assert_eq!(clip.rich_html(), Some("<i>new</i>"));
        receive_paste(
            &mut clip,
            Pasted {
                text: "plain".into(),
                html: String::new(),
            },
        );
        assert_eq!(clip.text, "plain");
        assert_eq!(clip.rich_html(), None);
        assert!(!clip.dirty);
    }
}
