//! Opt-in theme conversion at the UI -> core dependency boundary.
use crate::{Contrast, Token, Typography, UiTheme};
use rndr_core::{Color, MarkdownTheme, Theme, World};

impl UiTheme {
    /// Map semantic tokens to every legacy core chrome color. Pure conversion:
    /// does not mutate a world or opt an application into future theme updates.
    /// Core's single radius cannot express per-component UI corner recipes.
    /// High-contrast Selected equals the content surface in semantic UI, which
    /// adds an independent selection indicator. Core's text selection has no
    /// such indicator: use a 35%-alpha Focus overlay there to remain visible.
    pub fn core_theme(&self) -> Theme {
        let c = |token| self.color(token);
        Theme {
            canvas: c(Token::Canvas),
            tree: c(Token::Navigation),
            seam: c(Token::SubtleStroke),
            pane: c(Token::Content),
            title: c(Token::Raised),
            body: c(Token::Raised),
            status: c(Token::ControlHover),
            child: c(Token::ControlHover),
            empty: c(Token::Content),
            text: c(Token::Text),
            caret: c(Token::Focus),
            selection: if self.contrast == Contrast::High {
                let focus = c(Token::Focus);
                Color::rgba(focus.r, focus.g, focus.b, 0.35)
            } else {
                c(Token::Selected)
            },
            dirty: c(Token::WarningText),
            radius: 8.0,
        }
    }
    /// Map markdown accents as well as surfaces. Unlike the legacy default,
    /// links and code accents use the selected light/dark/high-contrast palette.
    pub fn markdown_theme(&self) -> MarkdownTheme {
        MarkdownTheme {
            surface: self.color(Token::Content),
            text: self.color(Token::Text),
            muted: self.color(Token::Secondary),
            link: self.color(Token::Accent),
            code: self.color(Token::Text),
            code_bg: self.color(Token::Raised),
            inline_bg: self.color(Token::Raised),
            quote: self.color(Token::StrongStroke),
            rule: self.color(Token::SubtleStroke),
            keyword: self.color(Token::InfoText),
            string: self.color(Token::SuccessText),
            number: self.color(Token::WarningText),
            comment: self.color(Token::Secondary),
        }
    }
}

/// Explicitly replace this world's `UiTheme`, core `Theme`, `MarkdownTheme`,
/// and default `TextStyle` (Reading role) in one host call. Call again whenever
/// changing the UI theme, before the next frame. There is no watcher, plugin,
/// process-global setting, reverse dependency or implicit resource precedence.
/// Installing `UiPlugin` alone does NOT install this bridge.
///
/// Markdown/editor and core widgets that read these resources see the update on
/// their normal sync/draw. Existing per-entity colors/TextStyles remain explicit
/// overrides. Static colors copied onto host-owned entities (e.g. root fills)
/// are not restyled: update those explicitly, or use `UiStyle` token recipes.
/// Core has one default font family/scale, so UI/reading/code typography cannot
/// all be represented there; this bridge chooses Reading. Semantic controls
/// retain their own UI typography. Markdown's legacy monospace request remains.
/// Host must not hold references into the replaced resources across this call.
pub fn apply_theme_bridge(world: &mut World, theme: UiTheme) {
    world.insert_resource(theme.core_theme());
    world.insert_resource(theme.markdown_theme());
    world.insert_resource(theme.typography(Typography::Reading));
    world.insert_resource(theme);
}
