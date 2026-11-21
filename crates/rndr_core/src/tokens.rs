//! Headless design defaults for consumers that do not depend on the semantic UI crate.
use crate::{Color, MarkdownTheme, Theme};

impl Default for MarkdownTheme {
    fn default() -> Self {
        Self {
            card_radius: 8.0,
            callouts: [
                ((0.16, 0.17, 0.21), (0.5, 0.55, 0.6)),
                ((0.12, 0.17, 0.24), (0.6, 0.75, 0.95)),
                ((0.10, 0.19, 0.15), (0.6, 0.85, 0.68)),
                ((0.22, 0.16, 0.08), (0.95, 0.78, 0.48)),
                ((0.23, 0.12, 0.12), (0.98, 0.65, 0.65)),
            ]
            .map(|((r, g, b), (x, y, z))| {
                (
                    Color::rgb(r, g, b),
                    Color::rgb(x, y, z),
                    Color::rgb(x, y, z),
                )
            }),
            surface: Color::rgb(0.10, 0.11, 0.13),
            text: Color::rgb(0.93, 0.94, 0.96),
            muted: Color::rgb(0.62, 0.66, 0.72),
            link: Color::rgb(0.45, 0.68, 0.95),
            code: Color::rgb(0.86, 0.89, 0.93),
            code_bg: Color::rgb(0.16, 0.17, 0.21),
            inline_bg: Color::rgb(0.20, 0.24, 0.30),
            quote: Color::rgb(0.45, 0.55, 0.68),
            rule: Color::rgb(0.30, 0.33, 0.38),
            keyword: Color::rgb(0.55, 0.62, 0.95),
            string: Color::rgb(0.62, 0.80, 0.55),
            number: Color::rgb(0.86, 0.70, 0.42),
            comment: Color::rgb(0.55, 0.58, 0.62),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            canvas: Color::rgb(0.08, 0.09, 0.10),
            tree: Color::rgb(0.10, 0.11, 0.13),
            seam: Color::rgb(0.06, 0.07, 0.08),
            pane: Color::rgb(0.12, 0.13, 0.15),
            title: Color::rgb(0.22, 0.34, 0.48),
            body: Color::rgb(0.14, 0.16, 0.20),
            status: Color::rgb(0.18, 0.22, 0.26),
            child: Color::rgb(0.20, 0.28, 0.30),
            empty: Color::rgb(0.16, 0.17, 0.19),
            text: Color::rgb(0.94, 0.95, 0.97),
            caret: Color::rgb(0.94, 0.95, 0.97),
            selection: Color::rgba(0.36, 0.62, 0.98, 0.88),
            dirty: Color::rgb(0.86, 0.62, 0.20),
            radius: 8.0,
        }
    }
}

/// Unscaled rhythm for the low-level layout defaults.
pub mod space {
    /// No separation.
    pub const ZERO: f32 = 0.;
    /// Fine alignment.
    pub const X2: f32 = 2.;
    /// Tight.
    pub const X4: f32 = 4.;
    /// Small.
    pub const X8: f32 = 8.;
    /// Page inset.
    pub const X16: f32 = 16.;
}
/// Default monospace terminal foreground.
pub const MONO_TEXT: Color = Color::rgb(0.83, 0.83, 0.84);
/// Default monospace terminal background.
pub const MONO_SURFACE: Color = Color::rgb(0.05, 0.05, 0.06);
/// Fallback diagnostic error ink.
pub const DIAGNOSTIC_ERROR: Color = Color::rgb(0.86, 0.28, 0.24);
/// Fallback diagnostic warning ink.
pub(crate) const DIAGNOSTIC_WARNING: Color = Color::rgb(0.85, 0.62, 0.18);
/// Fallback diagnostic information ink.
pub(crate) const DIAGNOSTIC_INFO: Color = Color::rgb(0.30, 0.60, 0.90);
/// Fallback diagnostic hint ink.
pub(crate) const DIAGNOSTIC_HINT: Color = Color::rgb(0.45, 0.65, 0.55);
/// Fallback source gutter marker ink.
pub(crate) const GUTTER_MARK: Color = Color::rgb(0.40, 0.65, 0.90);
/// Fallback source syntax: comment.
pub(crate) const SYNTAX_COMMENT: Color = Color::rgb(0.4, 0.6, 0.4);
/// Fallback source syntax: string.
pub(crate) const SYNTAX_STRING: Color = Color::rgb(0.7, 0.5, 0.3);
/// Fallback source syntax: number.
pub(crate) const SYNTAX_NUMBER: Color = Color::rgb(0.4, 0.5, 0.8);
/// Fallback window clear when no canvas is installed.
pub const CLEAR: Color = Color::rgb(0.06, 0.07, 0.09);
/// Corner radius of a square primitive.
pub const SQUARE: f32 = 0.;
/// Radius for the small headless task marker.
pub(crate) const TASK_CORNER: f32 = 2.;
