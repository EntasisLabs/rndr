//! Conversion from Liquid Markdown ` ```chart ` fences.

use rndr_liquid::chart::{ChartLegend, ChartProps, ChartType};

use crate::model::{
    Chart, ChartError, ChartKind, Curve, LabelPosition, Legend, Matrix, Orientation, Point, Series,
    SeriesMark, TrendDirection, ValueLabels,
};
use crate::theme::{ChartColor, Surface, Swatch};

fn kind(t: ChartType) -> ChartKind {
    match t {
        ChartType::Bar => ChartKind::Bar,
        ChartType::Line => ChartKind::Line,
        ChartType::Area => ChartKind::Area,
        ChartType::Pie => ChartKind::Pie,
        ChartType::Donut => ChartKind::Donut,
        ChartType::Radar => ChartKind::Radar,
        ChartType::Radial => ChartKind::Radial,
        ChartType::Scatter => ChartKind::Scatter,
        ChartType::Combo => ChartKind::Combo,
        ChartType::Heatmap => ChartKind::Heatmap,
    }
}

fn lower(s: &Option<String>) -> String {
    s.as_deref().unwrap_or("").trim().to_ascii_lowercase()
}

impl Chart {
    /// Build and [`validate`](Chart::validate) a chart from a parsed fence.
    ///
    /// Unknown option words fall back to defaults, and an unreadable color
    /// keeps the palette color for its index.
    pub fn from_liquid(props: &ChartProps) -> Result<Chart, ChartError> {
        const FALLBACK: [Swatch; 5] = [
            Swatch::Blue,
            Swatch::Purple,
            Swatch::Green,
            Swatch::Orange,
            Swatch::Pink,
        ];
        let mut chart = Chart::new(kind(props.r#type));
        chart.categories = props.categories.clone();
        chart.series = props
            .series
            .iter()
            .map(|s| Series::new(&s.key, &s.label, s.values.clone()))
            .collect();
        chart.title = props.title.clone().unwrap_or_default();
        chart.description = props.description.clone().unwrap_or_default();
        chart.points = props
            .points
            .iter()
            .flatten()
            .map(|p| Point {
                x: p.x,
                y: p.y,
                group: p.group.clone(),
            })
            .collect();
        chart.matrix = props.matrix.as_ref().map(|m| Matrix {
            rows: m.rows.clone(),
            cols: m.cols.clone(),
            values: m.values.clone(),
        });
        chart.series_marks = props
            .series_marks
            .iter()
            .flatten()
            .map(|m| match m.trim().to_ascii_lowercase().as_str() {
                "line" => SeriesMark::Line,
                _ => SeriesMark::Bar,
            })
            .collect();
        if lower(&props.layout) == "horizontal" {
            chart.orientation = Orientation::Horizontal;
        }
        chart.stacked = props.stacked.unwrap_or(false);
        chart.curve = match lower(&props.curve).as_str() {
            "linear" => Curve::Linear,
            "step" => Curve::Step,
            _ => Curve::Smooth,
        };
        chart.separator = props.separator.unwrap_or(true);
        chart.center_label = props.center_label.clone().unwrap_or_default();
        chart.center_value = props.center_value.clone().unwrap_or_default();
        chart.trend = props.trend.clone().unwrap_or_default();
        chart.trend_direction = match lower(&props.trend_direction).as_str() {
            "up" => Some(TrendDirection::Up),
            "down" => Some(TrendDirection::Down),
            "flat" => Some(TrendDirection::Flat),
            _ => None,
        };
        chart.caption = props.caption.clone().unwrap_or_default();
        chart.labels = match lower(&props.labels).as_str() {
            "value" => ValueLabels::Value,
            "category" => ValueLabels::Category,
            "both" => ValueLabels::Both,
            _ => ValueLabels::None,
        };
        chart.label_position = match lower(&props.label_position).as_str() {
            "inside" => LabelPosition::Inside,
            "outside" => LabelPosition::Outside,
            _ => LabelPosition::Auto,
        };
        chart.tooltip = props.tooltip.unwrap_or(true);
        chart.interactive = props.interactive.unwrap_or(true);
        chart.active_key = props.active_key.clone().unwrap_or_default();
        chart.legend = match &props.legend {
            None => Legend::Auto,
            Some(ChartLegend::Show(true)) => Legend::Bottom,
            Some(ChartLegend::Show(false)) => Legend::Hidden,
            Some(ChartLegend::Placement(p)) => match p.trim().to_ascii_lowercase().as_str() {
                "none" | "hidden" | "off" | "false" => Legend::Hidden,
                "top" => Legend::Top,
                "bottom" | "true" => Legend::Bottom,
                _ => Legend::Auto,
            },
        };
        chart.colors = props
            .colors
            .iter()
            .flatten()
            .enumerate()
            .map(|(i, c)| {
                ChartColor::parse(c).unwrap_or(ChartColor::Swatch(FALLBACK[i % FALLBACK.len()]))
            })
            .collect();
        chart.surface = Surface::parse(props.surface.as_deref().unwrap_or(""));
        chart.validate()
    }
}
