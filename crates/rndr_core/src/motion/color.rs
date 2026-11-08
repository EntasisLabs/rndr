//! Colour channels animated in premultiplied OKLab.

use crate::style::Color;

/// A colour an entity draws, animated with [`super::animate_color`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ColorProp {
    /// The entity's fill [`Color`] component.
    Fill,
    /// The colour of the entity's [`crate::Border`]. Ignored without one.
    Border,
    /// The entity's [`crate::Label`] text colour.
    Text,
}

impl ColorProp {
    pub(super) fn index(self) -> usize {
        match self {
            Self::Fill => 0,
            Self::Border => 1,
            Self::Text => 2,
        }
    }

    pub(super) const ALL: [Self; 3] = [Self::Fill, Self::Border, Self::Text];
}

fn decode(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn encode(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// Premultiplied OKLab lanes `[L·a, a·a, b·a, alpha]`. Premultiplying keeps
/// a fade from transparent from passing through the transparent hue.
pub(super) fn to_lanes(color: Color) -> [f32; 4] {
    let (r, g, b) = (decode(color.r), decode(color.g), decode(color.b));
    let l = (0.412_221_47 * r + 0.536_332_55 * g + 0.051_445_99 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    let lab = [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ];
    let a = color.a.clamp(0.0, 1.0);
    [lab[0] * a, lab[1] * a, lab[2] * a, a]
}

pub(super) fn from_lanes([pl, pa, pb, alpha]: [f32; 4]) -> Color {
    let alpha = alpha.clamp(0.0, 1.0);
    if alpha <= 1.0e-6 {
        return Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        };
    }
    let (ll, a, b) = (pl / alpha, pa / alpha, pb / alpha);
    let l = (ll + 0.396_337_78 * a + 0.215_803_76 * b).powi(3);
    let m = (ll - 0.105_561_346 * a - 0.063_854_17 * b).powi(3);
    let s = (ll - 0.089_484_18 * a - 1.291_485_5 * b).powi(3);
    let r = 4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s;
    let g = -1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s;
    let bl = -0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s;
    Color {
        r: encode(r.clamp(0.0, 1.0)),
        g: encode(g.clamp(0.0, 1.0)),
        b: encode(bl.clamp(0.0, 1.0)),
        a: alpha,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_srgb() {
        for c in [
            Color {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
            Color {
                r: 0.2,
                g: 0.6,
                b: 0.9,
                a: 0.5,
            },
            Color {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            },
            Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
        ] {
            let back = from_lanes(to_lanes(c));
            for (x, y) in [(c.r, back.r), (c.g, back.g), (c.b, back.b), (c.a, back.a)] {
                assert!((x - y).abs() < 2.0e-3, "{c:?} -> {back:?}");
            }
        }
    }
}
