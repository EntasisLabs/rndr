//! ` ```chart ` grammar: KV preamble plus a GFM table of numbers.

use serde::{Deserialize, Serialize};

use crate::kv::{first, lf, parse_kv_block, split_table, Fields, Ids};
use crate::parse::one_of;

/// Chart family.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChartType {
    /// Grouped or stacked bars.
    #[default]
    Bar,
    /// Lines.
    Line,
    /// Filled lines.
    Area,
    /// Pie.
    Pie,
    /// Pie with a hole.
    Donut,
    /// Radar / spider (three or more categories).
    Radar,
    /// Radial bars.
    Radial,
    /// X/Y points, optionally grouped.
    Scatter,
    /// Bars and lines per series.
    Combo,
    /// Row × column matrix.
    Heatmap,
}

impl ChartType {
    /// Every chart type.
    pub const ALL: [ChartType; 10] = [
        Self::Bar,
        Self::Line,
        Self::Area,
        Self::Pie,
        Self::Donut,
        Self::Radar,
        Self::Radial,
        Self::Scatter,
        Self::Combo,
        Self::Heatmap,
    ];

    /// Lowercase fence name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bar => "bar",
            Self::Line => "line",
            Self::Area => "area",
            Self::Pie => "pie",
            Self::Donut => "donut",
            Self::Radar => "radar",
            Self::Radial => "radial",
            Self::Scatter => "scatter",
            Self::Combo => "combo",
            Self::Heatmap => "heatmap",
        }
    }

    /// Parses a lowercase fence name.
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.as_str() == name)
    }
}

/// `legend:` is either a boolean or a placement.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ChartLegend {
    /// Show or hide with default placement.
    Show(bool),
    /// none | top | bottom.
    Placement(String),
}

/// One numeric series.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ChartSeries {
    /// Stable key.
    pub key: String,
    /// Legend label.
    pub label: String,
    /// One value per category (scatter: per point in the group).
    pub values: Vec<f64>,
}

/// Scatter point.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ChartPoint {
    /// X.
    pub x: f64,
    /// Y.
    pub y: f64,
    /// Group label; groups become series.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

/// Heatmap cells.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ChartMatrix {
    /// Row labels.
    pub rows: Vec<String>,
    /// Column labels.
    pub cols: Vec<String>,
    /// `values[row][col]`.
    pub values: Vec<Vec<f64>>,
}

macro_rules! opt_fields {
    ($(#[$m:meta])* pub struct $name:ident { $($(#[$fm:meta])* $f:ident : $t:ty,)* } { $($(#[$om:meta])* $o:ident : $ot:ty,)* }) => {
        $(#[$m])*
        #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
        #[serde(rename_all = "camelCase")]
        pub struct $name {
            $($(#[$fm])* pub $f: $t,)*
            $(
                $(#[$om])*
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub $o: Option<$ot>,
            )*
        }
    };
}

opt_fields! {
    /// ` ```chart ` payload.
    pub struct ChartProps {
        /// Chart family.
        r#type: ChartType,
        /// Category axis labels.
        categories: Vec<String>,
        /// Series.
        series: Vec<ChartSeries>,
    } {
        /// Title.
        title: String,
        /// Description.
        description: String,
        /// Scatter points.
        points: Vec<ChartPoint>,
        /// Heatmap matrix.
        matrix: ChartMatrix,
        /// Per-series bar | line for combo charts.
        series_marks: Vec<String>,
        /// vertical | horizontal.
        layout: String,
        /// Stack series.
        stacked: bool,
        /// smooth | linear | step.
        curve: String,
        /// Gaps between pie slices.
        separator: bool,
        /// Donut center label.
        center_label: String,
        /// Donut center value.
        center_value: String,
        /// Trend text.
        trend: String,
        /// up | down | flat.
        trend_direction: String,
        /// Caption.
        caption: String,
        /// none | value | category | both.
        labels: String,
        /// inside | outside | auto.
        label_position: String,
        /// Show tooltips.
        tooltip: bool,
        /// Legend.
        legend: ChartLegend,
        /// Allow series toggling.
        interactive: bool,
        /// Initially highlighted series key.
        active_key: String,
        /// Series colors.
        colors: Vec<String>,
        /// sm | md | lg | full | length.
        width: String,
        /// sm | md | lg | length.
        height: String,
        /// soft | muted | none | color.
        surface: String,
    }
}

/// Commas and a trailing `%` are ignored; non-finite values are rejected.
pub(crate) fn chart_number(raw: &str) -> Option<f64> {
    let cleaned = raw.replace(',', "");
    let cleaned = cleaned.strip_suffix('%').unwrap_or(&cleaned).trim();
    if cleaned.is_empty() {
        return None;
    }
    chart_number_strict(cleaned)
}

/// JavaScript `Number()` for a trimmed, non-empty string, finite values only.
pub(crate) fn chart_number_strict(cleaned: &str) -> Option<f64> {
    let lower = cleaned.to_ascii_lowercase();
    let (sign, digits) = match lower.strip_prefix('-') {
        Some(d) => (-1.0, d),
        None => (1.0, lower.strip_prefix('+').unwrap_or(&lower)),
    };
    let radix = |p: &str, r: u32| {
        digits
            .strip_prefix(p)
            .and_then(|d| u64::from_str_radix(d, r).ok())
            .map(|v| v as f64)
    };
    // JS `Number()` accepts unsigned 0x/0o/0b literals only.
    if sign > 0.0 && cleaned.as_bytes()[0] != b'+' {
        if let Some(v) = radix("0x", 16)
            .or_else(|| radix("0o", 8))
            .or_else(|| radix("0b", 2))
        {
            return Some(v);
        }
    }
    if lower.contains("inf") || lower.contains("nan") || lower.contains('_') {
        return None;
    }
    let n: f64 = cleaned.parse().ok()?;
    n.is_finite().then_some(n)
}

fn chart_bool(raw: Option<&String>) -> Option<bool> {
    match raw?.trim().to_lowercase().as_str() {
        "true" | "yes" | "1" | "on" => Some(true),
        "false" | "no" | "0" | "off" => Some(false),
        _ => None,
    }
}

fn row_numbers(cells: &[String], count: usize) -> Option<Vec<f64>> {
    (0..count)
        .map(|i| chart_number(cells.get(i + 1).map_or("", String::as_str)))
        .collect()
}

fn header_labels(rows: &[Vec<String>]) -> Option<Vec<String>> {
    let header = &rows[0];
    if header.len() < 2 {
        return None;
    }
    let labels: Vec<String> = header[1..]
        .iter()
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
        .collect();
    (!labels.is_empty()).then_some(labels)
}

fn category_series(rows: &[Vec<String>], ty: ChartType) -> Option<ChartProps> {
    let mut ids = Ids::default();
    let headers = header_labels(rows)?;
    let mut categories = Vec::new();
    let mut values: Vec<Vec<f64>> = vec![Vec::new(); headers.len()];
    for cells in &rows[1..] {
        let category = cells.first().map_or("", |c| c.trim());
        if category.is_empty() {
            continue;
        }
        let Some(nums) = row_numbers(cells, headers.len()) else {
            continue;
        };
        categories.push(category.to_string());
        for (s, n) in nums.into_iter().enumerate() {
            values[s].push(n);
        }
    }
    let min = match ty {
        ChartType::Radar => 3,
        ChartType::Radial => 1,
        _ => 2,
    };
    if categories.len() < min {
        return None;
    }
    let series = headers
        .into_iter()
        .zip(values)
        .enumerate()
        .map(|(i, (label, values))| ChartSeries {
            key: ids.slug(&label, "series", i),
            label,
            values,
        })
        .collect();
    Some(ChartProps {
        r#type: ty,
        categories,
        series,
        ..Default::default()
    })
}

fn scatter(rows: &[Vec<String>], ty: ChartType) -> Option<ChartProps> {
    let mut ids = Ids::default();
    let header: Vec<&str> = rows[0].iter().map(|c| c.trim()).collect();
    if header.len() < 2 {
        return None;
    }
    let has_group = header.len() >= 3;
    let mut points = Vec::new();
    for cells in &rows[1..] {
        let cell = |i: usize| cells.get(i).map_or("", String::as_str);
        let (Some(x), Some(y)) = (chart_number(cell(0)), chart_number(cell(1))) else {
            continue;
        };
        let group = has_group
            .then(|| cell(2).trim().to_string())
            .filter(|g| !g.is_empty());
        points.push(ChartPoint { x, y, group });
    }
    if points.len() < 2 {
        return None;
    }
    let mut groups: Vec<String> = Vec::new();
    for g in points.iter().filter_map(|p| p.group.as_ref()) {
        if !groups.contains(g) {
            groups.push(g.clone());
        }
    }
    let series = if groups.is_empty() {
        vec![ChartSeries {
            key: "points".into(),
            label: if header[1].is_empty() { "Y" } else { header[1] }.into(),
            values: points.iter().map(|p| p.y).collect(),
        }]
    } else {
        groups
            .iter()
            .enumerate()
            .map(|(i, label)| ChartSeries {
                key: ids.slug(label, "series", i),
                label: label.clone(),
                values: points
                    .iter()
                    .filter(|p| p.group.as_ref() == Some(label))
                    .map(|p| p.y)
                    .collect(),
            })
            .collect()
    };
    Some(ChartProps {
        r#type: ty,
        categories: if groups.is_empty() {
            vec!["Points".into()]
        } else {
            groups
        },
        series,
        points: Some(points),
        ..Default::default()
    })
}

fn heatmap(rows: &[Vec<String>], ty: ChartType) -> Option<ChartProps> {
    let mut ids = Ids::default();
    let cols = header_labels(rows)?;
    let mut labels = Vec::new();
    let mut values = Vec::new();
    for cells in &rows[1..] {
        let label = cells.first().map_or("", |c| c.trim());
        if label.is_empty() {
            continue;
        }
        let Some(nums) = row_numbers(cells, cols.len()) else {
            continue;
        };
        labels.push(label.to_string());
        values.push(nums);
    }
    if labels.is_empty() {
        return None;
    }
    let series = labels
        .iter()
        .zip(&values)
        .enumerate()
        .map(|(i, (label, v))| ChartSeries {
            key: ids.slug(label, "row", i),
            label: label.clone(),
            values: v.clone(),
        })
        .collect();
    Some(ChartProps {
        r#type: ty,
        categories: cols.clone(),
        series,
        matrix: Some(ChartMatrix {
            rows: labels,
            cols,
            values,
        }),
        ..Default::default()
    })
}

fn list(raw: Option<String>) -> Vec<String> {
    raw.map(|r| {
        r.split([',', '|'])
            .map(|c| c.trim().to_string())
            .filter(|c| !c.is_empty())
            .collect()
    })
    .unwrap_or_default()
}

fn apply_fields(chart: &mut ChartProps, f: &Fields) {
    let nonempty = |keys: &[&str]| first(f, keys).filter(|v| !v.is_empty());
    chart.title = f.get("title").cloned();
    chart.description = nonempty(&["description", "subtitle"]);
    chart.layout = one_of(f.get("layout"), &["vertical", "horizontal"]);
    chart.stacked = chart_bool(f.get("stacked"));
    chart.curve = one_of(f.get("curve"), &["smooth", "linear", "step"]);
    chart.separator = chart_bool(f.get("separator"));
    chart.center_label = f.get("centerlabel").or(f.get("center_label")).cloned();
    chart.center_value = f.get("centervalue").or(f.get("center_value")).cloned();
    chart.trend = f.get("trend").cloned();
    chart.trend_direction = one_of(
        first(f, &["trenddirection", "trend_direction"]).as_ref(),
        &["up", "down", "flat"],
    );
    chart.caption = f.get("caption").cloned();
    chart.labels = one_of(f.get("labels"), &["none", "value", "category", "both"]);
    chart.label_position = one_of(
        first(f, &["labelposition", "label_position"]).as_ref(),
        &["inside", "outside", "auto"],
    );
    chart.tooltip = chart_bool(f.get("tooltip"));
    if let Some(legend) = f.get("legend") {
        let v = legend.trim().to_lowercase();
        chart.legend = match v.as_str() {
            "none" | "top" | "bottom" => Some(ChartLegend::Placement(v)),
            _ => chart_bool(Some(&v))
                .filter(|_| ["true", "false", "yes", "no", "1", "0"].contains(&v.as_str()))
                .map(ChartLegend::Show),
        };
    }
    chart.interactive = chart_bool(f.get("interactive"));
    chart.active_key = nonempty(&["activekey", "active_key"]);
    let colors = list(nonempty(&["colors"]));
    chart.colors = (!colors.is_empty()).then_some(colors);
    chart.width = nonempty(&["width"]);
    chart.height = nonempty(&["height"]);
    chart.surface = nonempty(&["surface", "plot"]);
    if chart.r#type == ChartType::Combo {
        let marks: Vec<String> = list(nonempty(&["seriesmarks", "series_marks"]))
            .into_iter()
            .map(|m| m.to_lowercase())
            .filter(|m| m == "bar" || m == "line")
            .collect();
        chart.series_marks = (!marks.is_empty()).then_some(marks);
    }
}

pub(crate) fn chart(body: &str) -> Option<ChartProps> {
    let (preamble, rows) = split_table(&lf(body));
    if rows.len() < 2 {
        return None;
    }
    let fields = parse_kv_block(&preamble);
    let ty = first(&fields, &["type", "chart"]).unwrap_or_else(|| "bar".into());
    let ty = ChartType::parse(&ty.to_lowercase())?;
    let mut chart = match ty {
        ChartType::Scatter => scatter(&rows, ty),
        ChartType::Heatmap => heatmap(&rows, ty),
        _ => category_series(&rows, ty),
    }?;
    apply_fields(&mut chart, &fields);
    Some(chart)
}
