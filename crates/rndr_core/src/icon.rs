//! Built-in icon names and Private Use scalars.
//!
//! The coverage atlas lives in `rndr_paint` (ISC Lucide subset). This module
//! is the typed id those glyphs are addressed by, plus the fallback-face slot
//! the builtin shaper consults. Liquid block sentinels stay at U+E000 and
//! U+E001; icons start at U+E100.

include!("icon_data.rs");

use std::sync::{Arc, OnceLock};

use crate::font::Font;

/// `ch` is a baked icon scalar.
pub fn is_icon_scalar(ch: char) -> bool {
    Icon::from_scalar(ch).is_some()
}

/// Text with icon scalars removed.
///
/// A scalar becomes a space when it sits between words, so `Save ◆ file`
/// reads as `Save file`. Text that contains no icon scalar is returned
/// unchanged, including its spacing.
pub fn strip_icon_scalars(text: &str) -> String {
    if !text.chars().any(is_icon_scalar) {
        return text.to_string();
    }
    let mut out = String::new();
    let mut gap = false;
    for ch in text.chars() {
        if is_icon_scalar(ch) || ch.is_whitespace() {
            gap = true;
            continue;
        }
        if gap && !out.is_empty() {
            out.push(' ');
        }
        gap = false;
        out.push(ch);
    }
    out
}

static FALLBACK_FONT: OnceLock<Arc<dyn Font>> = OnceLock::new();

/// Register the face the builtin shaper appends after every family.
///
/// The first registration wins. Paint installs the Lucide coverage face so a
/// Private Use scalar in any label resolves without replacing the primary
/// atlas. A later call with a different face is ignored.
pub fn install_fallback_font(font: Arc<dyn Font>) {
    let _ = FALLBACK_FONT.set(font);
}

pub(crate) fn fallback_font() -> Option<Arc<dyn Font>> {
    FALLBACK_FONT.get().map(Arc::clone)
}

pub(crate) fn fallback_stamp() -> u64 {
    fallback_font()
        .map(|font| Arc::as_ptr(&font) as *const () as usize as u64)
        .unwrap_or(0)
}
