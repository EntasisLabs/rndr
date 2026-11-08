//! Number and label text.

use crate::model::ValueLabels;

/// Group thousands with commas and keep at most two decimals, like
/// `Intl.NumberFormat("en", { maximumFractionDigits: 2 })`.
///
/// ```
/// assert_eq!(rndr_chart::format_number(1234.567), "1,234.57");
/// assert_eq!(rndr_chart::format_number(-0.5), "-0.5");
/// assert_eq!(rndr_chart::format_number(f64::NAN), "");
/// ```
pub fn format_number(value: f64) -> String {
    if !value.is_finite() {
        return String::new();
    }
    let rounded = (value * 100.0).round() / 100.0;
    let negative = rounded < 0.0;
    let fixed = format!("{:.2}", rounded.abs());
    let (int, frac) = fixed.split_once('.').unwrap_or((&fixed, ""));
    let mut out = String::new();
    if negative {
        out.push('-');
    }
    for (i, ch) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    let frac = frac.trim_end_matches('0');
    if !frac.is_empty() {
        out.push('.');
        out.push_str(frac);
    }
    out
}

/// Text for a mark label, or empty when labels are off.
pub fn format_label(labels: ValueLabels, category: &str, value: f64) -> String {
    match labels {
        ValueLabels::None => String::new(),
        ValueLabels::Category => category.to_string(),
        ValueLabels::Value => format_number(value),
        ValueLabels::Both => format!("{category} {}", format_number(value)),
    }
}
