//! The reading column is a half-em estimate, including for CJK and Arabic.

use rndr_core::Size;
use rndr_ui::{reading_layout_at_measure, Space, Typography, UiTheme};

#[test]
fn the_column_is_half_an_em_times_the_character_count() {
    let theme = UiTheme::default();
    let size = theme.typography(Typography::Reading).size;
    let padding = theme.space(Space::X24);
    let layout = reading_layout_at_measure(900.0, 65.0, &theme);
    let Size::Px(width) = layout.width else {
        panic!("reading width is a fixed column");
    };
    let expected = 65.0 * size * 0.5 + padding * 2.0;
    assert!((width - expected).abs() < 0.001);
    // CJK and Arabic are not half an em wide. The formula does not consult
    // the script, the font, or the string. It is a column estimate only.
    assert!((width - (65.0 * size + padding * 2.0)).abs() > 1.0);
}
