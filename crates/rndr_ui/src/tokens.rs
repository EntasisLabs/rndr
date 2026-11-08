//! Logical-point design tokens. Explicit settings; [`UiTheme::follow_system`]
//! opts into the desktop's light/dark preference.
use rndr_core::{Color, SystemAppearance, TextStyle};

/// Explicit appearance. [`UiTheme::follow_system`] picks it from the desktop.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Appearance {
    /// Light surfaces.
    #[default]
    Light,
    /// Dark surfaces.
    Dark,
}
/// Increased contrast is a distinct palette, not an accent tweak.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Contrast {
    /// Standard palette.
    #[default]
    Standard,
    /// Opaque high-contrast palette.
    High,
}
/// Density affects spacing/targets, not font size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Density {
    /// 32-point minimum targets.
    #[default]
    Comfortable,
    /// Explicit 28-point minimum targets.
    Compact,
}
/// Explicit user settings. UI and reading text scales are independent.
#[derive(Clone, Debug, PartialEq)]
pub struct UiTheme {
    /// Explicit light/dark choice.
    pub appearance: Appearance,
    /// Explicit contrast mode.
    pub contrast: Contrast,
    /// Spacing/target density only.
    pub density: Density,
    /// UI text scale (finite values clamped to 0.5–4, otherwise 1).
    pub text_scale: f32,
    /// Reading/code text scale, independent of UI chrome.
    pub reading_scale: f32,
    /// UI family request: bundled Inter with the desktop Parley engine.
    /// Headless/no-Parley builds retain the deterministic bitmap fallback.
    pub ui_family: String,
    /// Reading font request; no native font verification is implied.
    pub reading_family: String,
    /// Monospace request. Actual fallback depends on the installed text engine.
    pub code_family: String,
}
impl Default for UiTheme {
    fn default() -> Self {
        Self {
            appearance: Appearance::Light,
            contrast: Contrast::Standard,
            density: Density::Comfortable,
            text_scale: 1.0,
            reading_scale: 1.0,
            ui_family: "Inter".into(),
            reading_family: "Inter".into(),
            code_family: "monospace".into(),
        }
    }
}
/// Semantic color keys. Only documented foreground/background pairs are guaranteed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Token {
    /// Window canvas.
    Canvas,
    /// Navigation plane.
    Navigation,
    /// Reading content.
    Content,
    /// Raised group.
    Raised,
    /// Overlay plane.
    Overlay,
    /// Main text.
    Text,
    /// Secondary text.
    Secondary,
    /// Inactive text (not used to convey available action).
    Disabled,
    /// Text on accent/hover/pressed.
    OnAccent,
    /// Decorative separator, not sole control boundary.
    SubtleStroke,
    /// Required boundary.
    StrongStroke,
    /// Independent focus indicator.
    Focus,
    /// Action fill.
    Accent,
    /// Action hover fill.
    Hover,
    /// Action pressed fill.
    Pressed,
    /// Neutral non-primary hover surface; pair with Text.
    ControlHover,
    /// Neutral non-primary pressed surface; pair with Text.
    ControlPressed,
    /// Selected surface; pair with Text.
    Selected,
    /// Informational foreground.
    InfoText,
    /// Informational surface.
    InfoFill,
    /// Informational boundary.
    InfoStroke,
    /// Success foreground.
    SuccessText,
    /// Success surface.
    SuccessFill,
    /// Success boundary.
    SuccessStroke,
    /// Warning foreground.
    WarningText,
    /// Warning surface.
    WarningFill,
    /// Warning boundary.
    WarningStroke,
    /// Danger foreground.
    DangerText,
    /// Danger surface.
    DangerFill,
    /// Danger boundary.
    DangerStroke,
}
fn hex(v: u32) -> Color {
    Color::rgb(
        ((v >> 16) & 255) as f32 / 255.0,
        ((v >> 8) & 255) as f32 / 255.0,
        (v & 255) as f32 / 255.0,
    )
}
impl UiTheme {
    /// This theme with the desktop's light/dark preference. A desktop with no
    /// preference keeps the current appearance.
    pub fn follow_system(mut self, system: &SystemAppearance) -> Self {
        if let Some(dark) = system.dark {
            self.appearance = if dark {
                Appearance::Dark
            } else {
                Appearance::Light
            };
        }
        self
    }
    /// Resolve to opaque color. Explicit transparent colors never pass through this resolver.
    pub fn color(&self, token: Token) -> Color {
        use Token::*;
        let dark = self.appearance == Appearance::Dark;
        if self.contrast == Contrast::High {
            return hex(match token {
                Canvas | Navigation | Content | Raised | Overlay | Selected | InfoFill
                | SuccessFill | WarningFill | DangerFill | ControlHover | ControlPressed => {
                    if dark {
                        0x000000
                    } else {
                        0xffffff
                    }
                }
                OnAccent => {
                    if dark {
                        0x000000
                    } else {
                        0xffffff
                    }
                }
                Accent | Hover | Pressed => {
                    if dark {
                        0xffff00
                    } else {
                        0x003d8f
                    }
                }
                Focus => {
                    if dark {
                        0xffff00
                    } else {
                        0x003d8f
                    }
                }
                _ => {
                    if dark {
                        0xffffff
                    } else {
                        0x000000
                    }
                }
            });
        }
        hex(match (dark, token) {
            (false, Canvas) => 0xf4f5f7,
            (true, Canvas) => 0x17191d,
            (false, Navigation) => 0xecedf0,
            (true, Navigation) => 0x1c1e23,
            (false, Content | Raised | Overlay) => 0xffffff,
            (true, Content) => 0x202227,
            (true, Raised) => 0x272a30,
            (true, Overlay) => 0x2d3037,
            (false, Text) => 0x24272e,
            (true, Text) => 0xedeef2,
            (false, Secondary | Disabled) => 0x60616a,
            (true, Secondary | Disabled) => 0xb5b7c1,
            (false, OnAccent) => 0xffffff,
            (true, OnAccent) => 0x111216,
            (false, SubtleStroke) => 0xdfe1e6,
            (true, SubtleStroke) => 0x393d45,
            (false, StrongStroke) => 0x858993,
            (true, StrongStroke) => 0x8c929e,
            (false, Focus) => 0x005fcc,
            (true, Focus) => 0x91caff,
            (false, Accent) => 0x345db3,
            (true, Accent) => 0x9ebcff,
            (false, Hover) => 0x294e9c,
            (true, Hover) => 0xb4cbff,
            (false, Pressed) => 0x203f82,
            (true, Pressed) => 0xc8d9ff,
            (false, ControlHover) => 0xe8ebf0,
            (true, ControlHover) => 0x30343d,
            (false, ControlPressed) => 0xdce0e8,
            (true, ControlPressed) => 0x3b414c,
            (false, Selected) => 0xe7edfa,
            (true, Selected) => 0x303c55,
            (false, InfoText) => 0x004a87,
            (false, InfoFill) => 0xe9f3ff,
            (false, InfoStroke) => 0x0065a9,
            (true, InfoText) => 0xb8ddff,
            (true, InfoFill) => 0x142b40,
            (true, InfoStroke) => 0x78b9ec,
            (false, SuccessText) => 0x175c31,
            (false, SuccessFill) => 0xeaf6ed,
            (false, SuccessStroke) => 0x327044,
            (true, SuccessText) => 0xb9ebc7,
            (true, SuccessFill) => 0x172e21,
            (true, SuccessStroke) => 0x7dcd96,
            (false, WarningText) => 0x6b3b00,
            (false, WarningFill) => 0xfff3d6,
            (false, WarningStroke) => 0xa15c00,
            (true, WarningText) => 0xffe0a3,
            (true, WarningFill) => 0x332613,
            (true, WarningStroke) => 0xd8a44d,
            (false, DangerText) => 0x91252d,
            (false, DangerFill) => 0xffedef,
            (false, DangerStroke) => 0xb33540,
            (true, DangerText) => 0xffc3c8,
            (true, DangerFill) => 0x391c23,
            (true, DangerStroke) => 0xf3949e,
        })
    }
    /// Density-adjusted spacing; text sizes are unaffected by density.
    pub fn space(&self, space: Space) -> f32 {
        let value = space.points();
        if self.density == Density::Compact && value >= 8.0 {
            value * 0.75
        } else {
            value
        }
    }
    /// Minimum target dimensions for future controls; grow further to fit content.
    pub fn target_min(&self) -> f32 {
        if self.density == Density::Compact {
            28.0
        } else {
            32.0
        }
    }
    /// Two-point focus indicator, independent of selection; A2 owns rendering.
    pub fn focus_width(&self) -> f32 {
        2.0
    }
    /// Contrasting separation/offset for a future two-color focus indicator.
    /// Accent fill alone is not a complete focus treatment.
    pub fn focus_gap(&self) -> f32 {
        2.0
    }
    /// Resolve a semantic type role using the current engine's family requests.
    pub fn typography(&self, role: Typography) -> TextStyle {
        let (size, line, weight, reading) = match role {
            Typography::Label => (14.0, 20.0, 400, false),
            Typography::Emphasis => (13.0, 18.0, 600, false),
            Typography::Secondary => (12.0, 16.0, 400, false),
            Typography::Reading => (16.0, 24.0, 400, true),
            Typography::Heading => (20.0, 28.0, 600, true),
            Typography::Title => (26.0, 34.0, 600, true),
            Typography::Code => (13.0, 20.0, 400, true),
        };
        let scale = if reading {
            self.reading_scale
        } else {
            self.text_scale
        };
        let scale = if scale.is_finite() {
            scale.clamp(0.5, 4.0)
        } else {
            1.0
        };
        TextStyle {
            size: size * scale,
            line_height: Some(line / size),
            weight,
            family: if role == Typography::Code {
                self.code_family.clone()
            } else if reading {
                self.reading_family.clone()
            } else {
                self.ui_family.clone()
            },
            ..TextStyle::default()
        }
    }
}
/// Spacing in logical points.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Space {
    /// No gap.
    Zero,
    /// Fine alignment.
    X2,
    /// Tight.
    X4,
    /// Small.
    X8,
    /// Medium-small.
    X12,
    /// Standard.
    X16,
    /// Group.
    X24,
    /// Section.
    X32,
    /// Large section.
    X48,
}
impl Space {
    /// Unscaled rhythm, before density.
    pub fn points(self) -> f32 {
        match self {
            Self::Zero => 0.0,
            Self::X2 => 2.0,
            Self::X4 => 4.0,
            Self::X8 => 8.0,
            Self::X12 => 12.0,
            Self::X16 => 16.0,
            Self::X24 => 24.0,
            Self::X32 => 32.0,
            Self::X48 => 48.0,
        }
    }
}
/// Semantic type roles, not claims of platform heading accessibility support.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Typography {
    /// Chrome label.
    Label,
    /// Compact semibold section/callout label.
    Emphasis,
    /// Secondary label.
    Secondary,
    /// Document reading.
    Reading,
    /// Subheading.
    Heading,
    /// Document title.
    Title,
    /// Monospace request.
    Code,
}
