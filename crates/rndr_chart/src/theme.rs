//! Palette and surface tokens.

use rndr_core::Color;

/// A named swatch from the shared palette.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Swatch {
    /// Red.
    Red,
    /// Orange.
    Orange,
    /// Yellow.
    Yellow,
    /// Green.
    Green,
    /// Blue.
    Blue,
    /// Purple.
    Purple,
    /// Pink.
    Pink,
    /// Neutral gray.
    Gray,
}

impl Swatch {
    /// Parse `red` … `pink`, plus `gray`/`grey`/`neutral`/`ink`.
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name.trim().to_ascii_lowercase().as_str() {
            "red" => Self::Red,
            "orange" => Self::Orange,
            "yellow" => Self::Yellow,
            "green" => Self::Green,
            "blue" => Self::Blue,
            "purple" => Self::Purple,
            "pink" => Self::Pink,
            "gray" | "grey" | "neutral" | "ink" => Self::Gray,
            _ => return None,
        })
    }
}

/// A color as written by an author: a swatch that follows the theme, or a
/// fixed color.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChartColor {
    /// Theme swatch.
    Swatch(Swatch),
    /// Fixed sRGB color.
    Rgba(Color),
}

impl ChartColor {
    /// Parse a swatch name or `#RGB`, `#RRGGBB`, `#RRGGBBAA`.
    pub fn parse(raw: &str) -> Option<Self> {
        let raw = raw.trim();
        if let Some(swatch) = Swatch::parse(raw) {
            return Some(Self::Swatch(swatch));
        }
        parse_hex(raw).map(Self::Rgba)
    }
}

/// `#RGB`, `#RRGGBB` or `#RRGGBBAA`.
pub fn parse_hex(raw: &str) -> Option<Color> {
    let hex = raw.trim().strip_prefix('#')?;
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let digits: Vec<u8> = match hex.len() {
        3 => hex.bytes().flat_map(|b| [b, b]).collect(),
        6 | 8 => hex.bytes().collect(),
        _ => return None,
    };
    let byte = |i: usize| {
        let s = std::str::from_utf8(&digits[i * 2..i * 2 + 2]).ok()?;
        u8::from_str_radix(s, 16).ok().map(|v| v as f32 / 255.0)
    };
    let a = if digits.len() == 8 { byte(3)? } else { 1.0 };
    Some(Color::rgba(byte(0)?, byte(1)?, byte(2)?, a))
}

/// Plot wash behind radar plates, polar tracks and heatmaps.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Surface {
    /// No plate on polar charts; the theme's soft wash elsewhere.
    #[default]
    Default,
    /// Theme ink at an alpha.
    Ink(f32),
    /// The theme's stronger wash.
    Muted,
    /// No wash anywhere.
    Transparent,
    /// A color at an alpha.
    Tint(ChartColor, f32),
}

impl Surface {
    /// Parse `soft`, `muted`, `none`, a swatch or hex, each optionally with
    /// `/alpha` or `@ alpha` (0–1, or a percent above 1).
    pub fn parse(raw: &str) -> Self {
        let raw = raw.trim();
        if raw.is_empty() {
            return Self::Default;
        }
        let (color, alpha) = split_alpha(raw);
        let lower = color.to_ascii_lowercase();
        match lower.as_str() {
            "soft" | "default" => alpha.map_or(Self::Default, Self::Ink),
            "none" | "transparent" | "off" => Self::Transparent,
            "muted" | "strong" => alpha.map_or(Self::Muted, Self::Ink),
            _ => match ChartColor::parse(color) {
                Some(c @ ChartColor::Swatch(Swatch::Gray)) => Self::Tint(c, alpha.unwrap_or(0.12)),
                Some(c) => Self::Tint(c, alpha.unwrap_or(0.16)),
                None => Self::Default,
            },
        }
    }

    /// Polar charts draw a plate only for an explicit wash.
    pub fn is_explicit(self) -> bool {
        !matches!(self, Self::Default | Self::Transparent)
    }
}

fn split_alpha(raw: &str) -> (&str, Option<f32>) {
    for sep in ['/', '@'] {
        if let Some((color, alpha)) = raw.rsplit_once(sep) {
            let alpha = alpha.trim();
            let digits = alpha.split('.').next().unwrap_or("");
            if !alpha.is_empty()
                && digits.len() <= 3
                && alpha.bytes().all(|b| b.is_ascii_digit() || b == b'.')
            {
                if let Ok(n) = alpha.parse::<f32>() {
                    let a = if n > 1.0 { n / 100.0 } else { n };
                    return (color.trim(), Some(a.clamp(0.0, 1.0)));
                }
            }
        }
    }
    (raw, None)
}

/// Colors a chart is painted with.
#[derive(Clone, Debug, PartialEq)]
pub struct ChartTheme {
    /// Swatches, in [`Swatch`] order: red, orange, yellow, green, blue,
    /// purple, pink, gray.
    pub swatches: [Color; 8],
    /// Default series order when no color is given.
    pub palette: [Swatch; 5],
    /// Titles, center values, hovered labels.
    pub fg: Color,
    /// Axis and value labels.
    pub fg_secondary: Color,
    /// Descriptions, captions, center captions.
    pub fg_muted: Color,
    /// Wash ink.
    pub plot_ink: Color,
    /// Soft wash alpha of [`Self::plot_ink`].
    pub plot_alpha: f32,
    /// Muted wash alpha of [`Self::plot_ink`].
    pub plot_muted_alpha: f32,
    /// Grid lines.
    pub grid: Color,
    /// Dot outlines and inside pie labels.
    pub halo: Color,
    /// Card background.
    pub card: Color,
    /// Card border and footer rule.
    pub border: Color,
    /// Tooltip background.
    pub tooltip: Color,
    /// Font family for every label. `None` uses the world's [`rndr_core::TextStyle`]
    /// resource, or the engine default.
    pub family: Option<String>,
}

const fn rgb8(r: u8, g: u8, b: u8) -> Color {
    Color::rgba(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0)
}

const fn rgba8(r: u8, g: u8, b: u8, a: f32) -> Color {
    Color::rgba(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a)
}

impl Default for ChartTheme {
    fn default() -> Self {
        Self::dark()
    }
}

impl ChartTheme {
    /// Ink-first dark theme.
    pub fn dark() -> Self {
        Self {
            swatches: [
                rgb8(248, 113, 113),
                rgb8(251, 146, 60),
                rgb8(250, 204, 21),
                rgb8(74, 222, 128),
                rgb8(96, 165, 250),
                rgb8(192, 132, 252),
                rgb8(244, 114, 182),
                rgb8(148, 163, 184),
            ],
            palette: DEFAULT_PALETTE,
            fg: rgb8(248, 250, 252),
            fg_secondary: rgb8(203, 213, 225),
            fg_muted: rgb8(148, 163, 184),
            plot_ink: rgb8(248, 250, 252),
            plot_alpha: 0.07,
            plot_muted_alpha: 0.14,
            grid: rgba8(148, 163, 184, 0.28),
            halo: rgba8(248, 250, 252, 0.7),
            card: rgba8(15, 23, 42, 0.48),
            border: rgba8(100, 116, 139, 0.28),
            tooltip: rgba8(15, 23, 42, 0.92),
            family: None,
        }
    }

    /// Paper light theme.
    pub fn light() -> Self {
        Self {
            swatches: [
                rgb8(220, 38, 38),
                rgb8(234, 88, 12),
                rgb8(202, 138, 4),
                rgb8(22, 163, 74),
                rgb8(37, 99, 235),
                rgb8(147, 51, 234),
                rgb8(219, 39, 119),
                rgb8(100, 116, 139),
            ],
            palette: DEFAULT_PALETTE,
            fg: rgb8(24, 24, 32),
            fg_secondary: rgb8(71, 85, 105),
            fg_muted: rgb8(100, 116, 139),
            plot_ink: rgb8(30, 41, 59),
            plot_alpha: 0.05,
            plot_muted_alpha: 0.10,
            grid: rgba8(100, 116, 139, 0.32),
            halo: rgba8(255, 255, 255, 0.85),
            card: rgba8(248, 250, 252, 0.55),
            border: rgba8(100, 116, 139, 0.22),
            tooltip: rgba8(255, 255, 255, 0.95),
            family: None,
        }
    }

    /// Color of `swatch`.
    pub fn swatch(&self, swatch: Swatch) -> Color {
        self.swatches[swatch as usize]
    }

    /// Resolve an author color.
    pub fn color(&self, color: ChartColor) -> Color {
        match color {
            ChartColor::Swatch(s) => self.swatch(s),
            ChartColor::Rgba(c) => c,
        }
    }

    /// Color for series or category `index`: the override at that index, or
    /// the palette cycled.
    pub fn series_color(&self, index: usize, overrides: &[ChartColor]) -> Color {
        match overrides.get(index) {
            Some(c) => self.color(*c),
            None => self.swatch(self.palette[index % self.palette.len()]),
        }
    }

    /// Wash color for `surface`; `None` when transparent.
    pub fn surface(&self, surface: Surface) -> Option<Color> {
        match surface {
            Surface::Default => Some(alpha(self.plot_ink, self.plot_alpha)),
            Surface::Muted => Some(alpha(self.plot_ink, self.plot_muted_alpha)),
            Surface::Ink(a) => Some(alpha(self.plot_ink, a)),
            Surface::Transparent => None,
            Surface::Tint(c, a) => {
                let c = self.color(c);
                Some(alpha(c, c.a * a))
            }
        }
    }
}

const DEFAULT_PALETTE: [Swatch; 5] = [
    Swatch::Blue,
    Swatch::Purple,
    Swatch::Green,
    Swatch::Orange,
    Swatch::Pink,
];

/// `color` with alpha replaced.
pub fn alpha(color: Color, a: f32) -> Color {
    Color::rgba(color.r, color.g, color.b, a)
}
