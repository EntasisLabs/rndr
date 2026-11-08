//! Chart description: what to plot and how, already resolved to enums.

use crate::theme::{ChartColor, Surface};

/// Chart family.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ChartKind {
    /// Grouped or stacked bars.
    #[default]
    Bar,
    /// Lines with a faint wash.
    Line,
    /// Filled lines.
    Area,
    /// Pie of the first series.
    Pie,
    /// Pie with a hole and optional center text.
    Donut,
    /// Spider plot; three or more categories.
    Radar,
    /// One progress ring, or one ring per category when there are three or more.
    Radial,
    /// X/Y points, optionally grouped.
    Scatter,
    /// Bars and lines per series, with a right axis for the lines.
    Combo,
    /// Row × column matrix.
    Heatmap,
}

impl ChartKind {
    /// Every family.
    pub const ALL: [ChartKind; 10] = [
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

    /// Lowercase name.
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

    /// Parse a name, ignoring case and surrounding space.
    pub fn parse(name: &str) -> Option<Self> {
        let name = name.trim().to_ascii_lowercase();
        Self::ALL.into_iter().find(|kind| kind.as_str() == name)
    }

    /// Bars, lines, areas and combos share a category × value frame.
    pub fn is_cartesian(self) -> bool {
        matches!(self, Self::Bar | Self::Line | Self::Area | Self::Combo)
    }
}

/// Bar orientation. Only bars read it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Orientation {
    /// Categories along x.
    #[default]
    Vertical,
    /// Categories along y.
    Horizontal,
}

/// Line interpolation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Curve {
    /// Monotone cubic: smooth without overshooting a point.
    #[default]
    Smooth,
    /// Straight segments.
    Linear,
    /// Step after each point.
    Step,
}

/// Text drawn on marks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ValueLabels {
    /// No labels.
    #[default]
    None,
    /// The number.
    Value,
    /// The category.
    Category,
    /// Category, then number.
    Both,
}

/// Where pie and donut labels go. Other marks label at their end.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LabelPosition {
    /// Outside, except a donut without center text puts them inside.
    #[default]
    Auto,
    /// At the slice centroid.
    Inside,
    /// On a leader line.
    Outside,
}

/// Legend placement.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Legend {
    /// Bottom when there is more than one legend entry, otherwise hidden.
    #[default]
    Auto,
    /// Hidden.
    Hidden,
    /// Above the plot.
    Top,
    /// Below the plot.
    Bottom,
}

/// Combo mark for one series.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeriesMark {
    /// Bars against the left axis.
    Bar,
    /// A line against the right axis.
    Line,
}

/// Arrow beside the trend text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrendDirection {
    /// Rising.
    Up,
    /// Falling.
    Down,
    /// Level.
    Flat,
}

/// One named row of values, one per category.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Series {
    /// Stable key, used by [`Chart::active_key`] and hover.
    pub key: String,
    /// Legend and tooltip text.
    pub label: String,
    /// Values by category index. Missing values read as zero.
    pub values: Vec<f64>,
}

impl Series {
    /// Series with `key` and `label`.
    pub fn new(key: impl Into<String>, label: impl Into<String>, values: Vec<f64>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            values,
        }
    }

    pub(crate) fn value(&self, index: usize) -> f64 {
        self.values.get(index).copied().unwrap_or(0.0)
    }
}

/// One scatter point.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Point {
    /// X.
    pub x: f64,
    /// Y.
    pub y: f64,
    /// Group; each group gets a color and a legend entry.
    pub group: Option<String>,
}

/// Heatmap cells.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Matrix {
    /// Row labels.
    pub rows: Vec<String>,
    /// Column labels.
    pub cols: Vec<String>,
    /// `values[row][col]`.
    pub values: Vec<Vec<f64>>,
}

/// Why [`Chart::validate`] refused a chart.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChartError {
    /// Fewer categories than the family needs.
    Categories {
        /// Needed.
        needed: usize,
        /// Given.
        found: usize,
    },
    /// No series with at least one finite value.
    NoSeries,
    /// Scatter needs two finite points.
    Points,
    /// Heatmap needs a non-empty rectangular finite matrix.
    Matrix,
}

impl std::fmt::Display for ChartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Categories { needed, found } => {
                write!(f, "needs {needed} categories, found {found}")
            }
            Self::NoSeries => f.write_str("needs a series with values"),
            Self::Points => f.write_str("scatter needs at least two points"),
            Self::Matrix => f.write_str("heatmap needs a rectangular matrix"),
        }
    }
}

impl std::error::Error for ChartError {}

/// A chart to lay out and paint.
///
/// Build one with [`Chart::new`] and the field setters, or from Liquid
/// Markdown with the `liquid` feature. Defaults match Medousa's chart
/// organism: tooltips and hover on, pie separators on, smooth curves.
#[derive(Clone, Debug, PartialEq)]
pub struct Chart {
    /// Family.
    pub kind: ChartKind,
    /// Heading.
    pub title: String,
    /// Text under the heading.
    pub description: String,
    /// Category labels.
    pub categories: Vec<String>,
    /// Series, one value per category.
    pub series: Vec<Series>,
    /// Scatter points.
    pub points: Vec<Point>,
    /// Heatmap cells.
    pub matrix: Option<Matrix>,
    /// Combo marks by series index; missing entries are bar for the first
    /// series and line after it.
    pub series_marks: Vec<SeriesMark>,
    /// Bar orientation.
    pub orientation: Orientation,
    /// Stack bars.
    pub stacked: bool,
    /// Line interpolation.
    pub curve: Curve,
    /// Gaps between pie slices.
    pub separator: bool,
    /// Donut and radial center caption.
    pub center_label: String,
    /// Donut and radial center value. A donut shows the total when empty.
    pub center_value: String,
    /// Footer trend text.
    pub trend: String,
    /// Footer trend arrow.
    pub trend_direction: Option<TrendDirection>,
    /// Footer caption.
    pub caption: String,
    /// Mark labels.
    pub labels: ValueLabels,
    /// Pie and donut label placement.
    pub label_position: LabelPosition,
    /// Report tooltips from [`crate::ChartScene::hit`].
    pub tooltip: bool,
    /// Dim the rest of the chart while one item is hovered.
    pub interactive: bool,
    /// Series key, label or category to keep highlighted. Empty keeps all.
    pub active_key: String,
    /// Legend placement.
    pub legend: Legend,
    /// Per-series or per-category color overrides.
    pub colors: Vec<ChartColor>,
    /// Plot wash for radar plates, polar tracks and heatmaps.
    pub surface: Surface,
}

impl Default for Chart {
    fn default() -> Self {
        Self::new(ChartKind::Bar)
    }
}

impl Chart {
    /// Empty chart of `kind` with default options.
    pub fn new(kind: ChartKind) -> Self {
        Self {
            kind,
            title: String::new(),
            description: String::new(),
            categories: Vec::new(),
            series: Vec::new(),
            points: Vec::new(),
            matrix: None,
            series_marks: Vec::new(),
            orientation: Orientation::Vertical,
            stacked: false,
            curve: Curve::Smooth,
            separator: true,
            center_label: String::new(),
            center_value: String::new(),
            trend: String::new(),
            trend_direction: None,
            caption: String::new(),
            labels: ValueLabels::None,
            label_position: LabelPosition::Auto,
            tooltip: true,
            interactive: true,
            active_key: String::new(),
            legend: Legend::Auto,
            colors: Vec::new(),
            surface: Surface::Default,
        }
    }

    /// Set the categories.
    pub fn categories<S: Into<String>>(mut self, categories: impl IntoIterator<Item = S>) -> Self {
        self.categories = categories.into_iter().map(Into::into).collect();
        self
    }

    /// Append a series.
    pub fn series(mut self, series: Series) -> Self {
        self.series.push(series);
        self
    }

    /// Set the heading.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Check that the family has the data it needs.
    ///
    /// Series without finite values, non-finite points, and blank categories
    /// are dropped first, as Medousa does, so a chart that validates always
    /// lays out.
    pub fn validate(mut self) -> Result<Self, ChartError> {
        self.categories = self
            .categories
            .into_iter()
            .map(|c| c.trim().to_string())
            .filter(|c| !c.is_empty())
            .collect();
        self.series.retain_mut(|s| {
            s.values.retain(|v| v.is_finite());
            !s.values.is_empty()
        });
        for (i, s) in self.series.iter_mut().enumerate() {
            if s.key.trim().is_empty() {
                s.key = format!("series-{i}");
            }
            if s.label.trim().is_empty() {
                s.label = format!("Series {}", i + 1);
            }
        }
        self.points.retain(|p| p.x.is_finite() && p.y.is_finite());
        match self.kind {
            ChartKind::Scatter => {
                if self.points.len() < 2 {
                    return Err(ChartError::Points);
                }
            }
            ChartKind::Heatmap => {
                let ok = self.matrix.as_ref().is_some_and(|m| {
                    !m.rows.is_empty()
                        && !m.cols.is_empty()
                        && m.values.len() == m.rows.len()
                        && m.values.iter().all(|row| {
                            row.len() == m.cols.len() && row.iter().all(|v| v.is_finite())
                        })
                });
                if !ok {
                    return Err(ChartError::Matrix);
                }
            }
            kind => {
                let needed = match kind {
                    ChartKind::Radar => 3,
                    ChartKind::Radial => 1,
                    _ => 2,
                };
                if self.categories.len() < needed {
                    return Err(ChartError::Categories {
                        needed,
                        found: self.categories.len(),
                    });
                }
                if self.series.is_empty() {
                    return Err(ChartError::NoSeries);
                }
            }
        }
        Ok(self)
    }

    /// Combo mark for series `index`.
    pub fn series_mark(&self, index: usize) -> SeriesMark {
        self.series_marks
            .get(index)
            .copied()
            .unwrap_or(if index == 0 {
                SeriesMark::Bar
            } else {
                SeriesMark::Line
            })
    }

    /// Series drawn as `mark`. Every series outside a combo.
    pub(crate) fn marked(&self, mark: SeriesMark) -> Vec<(usize, &Series)> {
        self.series
            .iter()
            .enumerate()
            .filter(|(i, _)| self.kind != ChartKind::Combo || self.series_mark(*i) == mark)
            .collect()
    }

    /// Largest value, or the largest category sum when stacked. Never zero.
    pub fn value_max(&self) -> f64 {
        Self::max_of(
            &self.series.iter().collect::<Vec<_>>(),
            self.stacked,
            self.categories.len(),
        )
    }

    /// [`Self::value_max`] over the series drawn as `mark`; zero when none are.
    pub fn value_max_for(&self, mark: SeriesMark) -> f64 {
        if self.kind != ChartKind::Combo {
            return self.value_max();
        }
        let marked: Vec<&Series> = self.marked(mark).into_iter().map(|(_, s)| s).collect();
        if marked.is_empty() {
            return 0.0;
        }
        Self::max_of(
            &marked,
            self.stacked && mark == SeriesMark::Bar,
            self.categories.len(),
        )
    }

    fn max_of(series: &[&Series], stacked: bool, categories: usize) -> f64 {
        let max = if stacked {
            (0..categories)
                .map(|i| series.iter().map(|s| s.value(i)).sum::<f64>())
                .fold(0.0, f64::max)
        } else {
            series
                .iter()
                .flat_map(|s| s.values.iter().copied())
                .fold(0.0, f64::max)
        };
        if max == 0.0 {
            1.0
        } else {
            max
        }
    }

    /// Where pie and donut labels land, or `None` when labels are off.
    pub fn resolved_label_position(&self) -> Option<LabelPosition> {
        if self.labels == ValueLabels::None {
            return None;
        }
        Some(match self.label_position {
            LabelPosition::Auto => {
                if self.kind == ChartKind::Donut
                    && self.center_label.is_empty()
                    && self.center_value.is_empty()
                {
                    LabelPosition::Inside
                } else {
                    LabelPosition::Outside
                }
            }
            other => other,
        })
    }

    /// Legend entries: categories for pie, donut and multi-ring radial; scatter
    /// groups; nothing for heatmaps; series otherwise.
    pub fn legend_items(&self) -> Vec<String> {
        match self.kind {
            ChartKind::Pie | ChartKind::Donut => self.categories.clone(),
            ChartKind::Radial if self.categories.len() >= 3 => self.categories.clone(),
            ChartKind::Heatmap => Vec::new(),
            ChartKind::Scatter => self.scatter_groups(),
            _ => self.series.iter().map(|s| s.label.clone()).collect(),
        }
    }

    /// Resolved legend placement; `None` when hidden.
    pub fn legend_placement(&self) -> Option<Legend> {
        let count = match self.kind {
            ChartKind::Heatmap => 0,
            ChartKind::Scatter => self.scatter_groups().len(),
            _ => self.series.len(),
        };
        match self.legend {
            Legend::Hidden => None,
            Legend::Auto if count > 1 => Some(Legend::Bottom),
            Legend::Auto => None,
            placement => Some(placement),
        }
    }

    /// Distinct scatter groups in first-seen order, or `["Points"]`.
    pub fn scatter_groups(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for p in &self.points {
            if let Some(group) = p.group.as_deref().filter(|g| !g.is_empty()) {
                if !out.iter().any(|g| g == group) {
                    out.push(group.to_string());
                }
            }
        }
        if out.is_empty() {
            out.push("Points".into());
        }
        out
    }

    /// `active_key` matches a key, label or category, ignoring case. An empty
    /// key matches everything.
    pub fn is_active(&self, candidates: &[&str]) -> bool {
        let needle = self.active_key.trim().to_lowercase();
        needle.is_empty() || candidates.iter().any(|c| c.trim().to_lowercase() == needle)
    }
}
