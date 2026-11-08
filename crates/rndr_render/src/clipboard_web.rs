//! Browser clipboard through `navigator.clipboard`.
//!
//! Copies are written as text, or as HTML plus a plain-text fallback, by a
//! promise the runner does not wait on. Reading needs an async permission
//! prompt, so paste keeps reading the in-app clipboard.

use rndr_core::Clipboard;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;

pub(crate) struct OsClipboard;

impl OsClipboard {
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) fn pull(&mut self, clip: &mut Clipboard) {
        clip.want_image = false;
    }

    pub(crate) fn push(&mut self, clip: &Clipboard) {
        if !clip.dirty {
            return;
        }
        let Some(window) = web_sys::window() else {
            return;
        };
        let board = window.navigator().clipboard();
        let promise = match clip
            .rich_html()
            .and_then(|html| rich_items(html, &clip.text))
        {
            Some(items) => board.write(&items),
            None => board.write_text(&clip.text),
        };
        wasm_bindgen_futures::spawn_local(async move {
            let _ = JsFuture::from(promise).await;
        });
    }
}

fn rich_items(html: &str, text: &str) -> Option<JsValue> {
    let record = js_sys::Object::new();
    for (mime, body) in [("text/html", html), ("text/plain", text)] {
        let parts = js_sys::Array::of1(&JsValue::from_str(body));
        let options = web_sys::BlobPropertyBag::new();
        options.set_type(mime);
        let blob = web_sys::Blob::new_with_str_sequence_and_options(&parts, &options).ok()?;
        js_sys::Reflect::set(&record, &JsValue::from_str(mime), &blob).ok()?;
    }
    let item = web_sys::ClipboardItem::new_with_record_from_str_to_blob_promise(&record).ok()?;
    Some(js_sys::Array::of1(&item).into())
}
