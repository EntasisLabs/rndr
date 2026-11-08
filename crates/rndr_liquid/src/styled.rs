//! ` ```block `: styled prose with an allowlisted font, size, alignment and
//! line spacing.

use crate::kv::{lf, re, split_header};
use crate::props::StyledBlockProps;

re!(TRAILING_WS, r"\s+$");
re!(UNITLESS, r"^\d+(\.\d+)?$");
re!(LENGTH, r"(?i)^\d+(\.\d+)?(px|rem|em)$");

/// Font family token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockFont {
    /// System sans-serif.
    Sans,
    /// System serif.
    Serif,
    /// System monospace.
    Mono,
}

impl BlockFont {
    /// Parses `sans`, `serif` or `mono` (case-insensitive).
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_lowercase().as_str() {
            "sans" => Some(Self::Sans),
            "serif" => Some(Self::Serif),
            "mono" => Some(Self::Mono),
            _ => None,
        }
    }
}

/// Font size in logical pixels for a size token (`sm`..`xl`), a unitless
/// number (pixels) or a `px`/`rem`/`em` length, given the base text size.
///
/// ```
/// use rndr_liquid::block_font_size;
/// assert_eq!(block_font_size("lg", 16.0), Some(18.0));
/// assert_eq!(block_font_size("14", 16.0), Some(14.0));
/// assert_eq!(block_font_size("1.5rem", 16.0), Some(24.0));
/// assert_eq!(block_font_size("huge", 16.0), None);
/// ```
pub fn block_font_size(size: &str, base: f32) -> Option<f32> {
    let t = size.trim().to_lowercase();
    let rem = match t.as_str() {
        "sm" => Some(0.875),
        "md" => Some(1.0),
        "lg" => Some(1.125),
        "xl" => Some(1.35),
        _ => None,
    };
    if let Some(rem) = rem {
        return Some(rem * base);
    }
    if UNITLESS.is_match(&t) {
        return t.parse().ok();
    }
    if !LENGTH.is_match(&t) {
        return None;
    }
    let (num, scale) = if let Some(n) = t.strip_suffix("px") {
        (n, 1.0)
    } else if let Some(n) = t.strip_suffix("rem") {
        (n, base)
    } else {
        (t.strip_suffix("em")?, base)
    };
    num.parse::<f32>().ok().map(|n| n * scale)
}

/// Line-height multiplier for `tight` / `normal` / `relaxed` or a number in 1..=3.
///
/// ```
/// assert_eq!(rndr_liquid::block_line_height("relaxed"), Some(1.8));
/// assert_eq!(rndr_liquid::block_line_height("2"), Some(2.0));
/// assert_eq!(rndr_liquid::block_line_height("9"), None);
/// ```
pub fn block_line_height(spacing: &str) -> Option<f32> {
    match spacing.trim().to_lowercase().as_str() {
        "tight" => Some(1.25),
        "normal" => Some(1.55),
        "relaxed" => Some(1.8),
        t => t.parse::<f32>().ok().filter(|n| (1.0..=3.0).contains(n)),
    }
}

fn kv_lines(header: &str) -> crate::kv::Fields {
    let mut out = crate::kv::Fields::new();
    for raw in lf(header).split('\n') {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = crate::kv::split_kv(line) {
            if !k.is_empty() && !v.is_empty() {
                out.insert(k, v);
            }
        }
    }
    out
}

pub(crate) fn styled_block(source: &str) -> Option<StyledBlockProps> {
    let normalized = lf(source);
    let (header, body) = match split_header(&normalized) {
        (h, Some(rest)) => (h, TRAILING_WS.replace(rest, "").into_owned()),
        (h, None) => (h, String::new()),
    };
    let fields = kv_lines(header);
    let prose = if body.is_empty() {
        fields
            .get("body")
            .map(|b| b.trim().to_string())
            .unwrap_or_default()
    } else {
        body
    };
    let has_style = ["id", "font", "size", "align", "spacing"]
        .iter()
        .any(|k| fields.contains_key(*k));
    if prose.is_empty() && !has_style {
        return None;
    }
    let prose = if prose.is_empty() {
        " ".to_string()
    } else {
        prose
    };
    let trimmed = |k: &str| {
        fields
            .get(k)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    Some(StyledBlockProps {
        body: prose.trim().to_string(),
        id: trimmed("id"),
        font: crate::parse::one_of(fields.get("font"), &["sans", "serif", "mono"]),
        size: trimmed("size"),
        align: crate::parse::one_of(fields.get("align"), &["left", "center", "right", "justify"]),
        spacing: trimmed("spacing"),
    })
}
