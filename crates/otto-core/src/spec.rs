//! Graph and analysis specifications. A spec only describes *what* to show;
//! `otto-plot` draws it from the linked table and R (`otto-r`) computes its statistics.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::table::ColumnKind;

/// The graph types. X/Y/Group always refer to columns of one multivariable table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GraphKind {
    /// XY graph, points only. Numeric X and Y; replicates at the same X are shown as mean ± error (or individually).
    XyPoints,
    /// XY graph, points connected by a line through the means (sorted by X).
    XyPointsLine,
    /// XY graph, a line through the means (sorted by X) without symbols; error bars as chosen.
    XyLine,
    /// Column bar graph: categorical X (or one bar per Y column when X is empty); bar = mean, error bar, every point shown on the bar.
    ColumnBar,
    /// Grouped bar graph, interleaved: one cluster per X level with one bar (plus points) per data set.
    GroupedInterleaved,
    /// Grouped bar graph, separated: one block per data set with one bar (plus points) per X level.
    GroupedSeparated,
    /// Stacked bar graph: one bar per X level, the data sets laid on top of each other.
    /// Each segment is that data set's mean (or median) at that X, as on any bar graph;
    /// stacking leaves no room for error bars or individual points, so neither is drawn.
    StackedBar,
    /// Stacked bar graph scaled to 100%: every X level's bar fills the axis and each data
    /// set takes its share of it.
    StackedPercent,
    /// Scatter plot of numeric X vs numeric Y, one dot per row, no averaging.
    XyScatter,
    /// Kaplan-Meier survival curves: X = time, Y = event (1 = event such as death, 0 =
    /// censored), one row per subject; Group by makes one curve per group.
    Survival,
    /// Heatmap: columns = categorical X, rows = data sets (Group by, or several Y columns);
    /// each cell's color is the mean (or median) of its values.
    Heatmap,
    /// Box-and-whisker plot: categorical X, one box per category (median, quartiles, whiskers).
    Box,
    /// Violin plot: categorical X, one kernel-density violin per category with median and quartiles.
    Violin,
    /// Volcano plot: X = log2 fold change, Y = P value (plotted as -log10), one dot per row,
    /// coloured by which side of the cutoffs it falls on.
    Volcano,
    /// Normal QQ plot: theoretical normal quantiles against the sorted values of each data set.
    Qq,
    /// Pie chart: one slice per X category, sized by the sum of its Y values.
    Pie,
    /// Donut chart: a pie with its centre cut out.
    Donut,
    /// Contingency table: rows = X categories, columns = data sets, each cell the sum of
    /// its Y values (the count). Drawn as a laid-out table rather than axes, and analysed
    /// with chi-square, Fisher's exact test and the rest.
    Contingency,
}

/// What the analyses of a graph work on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DataShape {
    /// The bars of a column or grouped graph (t tests, ANOVA).
    Bars,
    /// The data sets of an XY graph (curve fits, regression, correlation, AUC).
    Xy,
    /// The curves of a survival graph (Kaplan-Meier comparisons).
    Survival,
    /// The cells of a heatmap (no statistics).
    Grid,
    /// The slices of a pie or donut chart (no statistics).
    Slices,
    /// The cells of a contingency table (chi-square, Fisher's exact and the rest).
    Counts,
}

impl DataShape {
    /// What a graph of this shape is called, for messages.
    pub fn graph_name(self) -> &'static str {
        match self {
            DataShape::Bars => "a column or grouped graph",
            DataShape::Xy => "an XY graph",
            DataShape::Survival => "a survival graph",
            DataShape::Grid => "a heatmap",
            DataShape::Slices => "a pie or donut chart",
            DataShape::Counts => "a contingency table",
        }
    }
}

impl GraphKind {
    pub const ALL: [GraphKind; 18] = [
        GraphKind::XyPoints,
        GraphKind::XyPointsLine,
        GraphKind::XyLine,
        GraphKind::ColumnBar,
        GraphKind::Box,
        GraphKind::Violin,
        GraphKind::GroupedInterleaved,
        GraphKind::GroupedSeparated,
        GraphKind::StackedBar,
        GraphKind::StackedPercent,
        GraphKind::XyScatter,
        GraphKind::Volcano,
        GraphKind::Qq,
        GraphKind::Survival,
        GraphKind::Heatmap,
        GraphKind::Pie,
        GraphKind::Donut,
        GraphKind::Contingency,
    ];

    pub fn shape(self) -> DataShape {
        match self {
            GraphKind::ColumnBar
            | GraphKind::Box
            | GraphKind::Violin
            | GraphKind::GroupedInterleaved
            | GraphKind::GroupedSeparated
            | GraphKind::StackedBar
            | GraphKind::StackedPercent => DataShape::Bars,
            GraphKind::Survival => DataShape::Survival,
            GraphKind::Heatmap => DataShape::Grid,
            GraphKind::Pie | GraphKind::Donut => DataShape::Slices,
            GraphKind::Contingency => DataShape::Counts,
            _ => DataShape::Xy,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            GraphKind::XyPoints => "XY points",
            GraphKind::XyPointsLine => "XY points + line",
            GraphKind::XyLine => "XY line only",
            GraphKind::ColumnBar => "Column bar",
            GraphKind::GroupedInterleaved => "Grouped (interleaved)",
            GraphKind::GroupedSeparated => "Grouped (separated)",
            GraphKind::StackedBar => "Stacked bars",
            GraphKind::StackedPercent => "Stacked bars (100%)",
            GraphKind::XyScatter => "Scatter",
            GraphKind::Survival => "Survival (Kaplan-Meier)",
            GraphKind::Heatmap => "Heatmap",
            GraphKind::Box => "Box and whiskers",
            GraphKind::Violin => "Violin",
            GraphKind::Volcano => "Volcano",
            GraphKind::Qq => "QQ (normality)",
            GraphKind::Pie => "Pie",
            GraphKind::Donut => "Donut",
            GraphKind::Contingency => "Contingency table",
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            GraphKind::XyPoints => "xy_points",
            GraphKind::XyPointsLine => "xy_points_line",
            GraphKind::XyLine => "xy_line",
            GraphKind::ColumnBar => "column_bar",
            GraphKind::GroupedInterleaved => "grouped_interleaved",
            GraphKind::GroupedSeparated => "grouped_separated",
            GraphKind::StackedBar => "stacked_bar",
            GraphKind::StackedPercent => "stacked_percent",
            GraphKind::XyScatter => "xy_scatter",
            GraphKind::Survival => "survival",
            GraphKind::Heatmap => "heatmap",
            GraphKind::Box => "box",
            GraphKind::Violin => "violin",
            GraphKind::Volcano => "volcano",
            GraphKind::Qq => "qq",
            GraphKind::Pie => "pie",
            GraphKind::Donut => "donut",
            GraphKind::Contingency => "contingency",
        }
    }

    /// Whether X is numeric (XY family) or categorical (bar family).
    pub fn x_kind(self) -> ColumnKind {
        match self {
            GraphKind::XyPoints | GraphKind::XyPointsLine | GraphKind::XyLine | GraphKind::XyScatter | GraphKind::Survival | GraphKind::Volcano | GraphKind::Qq => {
                ColumnKind::Numeric
            }
            _ => ColumnKind::Categorical,
        }
    }

    pub fn is_xy(self) -> bool {
        self.x_kind() == ColumnKind::Numeric
    }

    /// X categories crossed with data sets: grouped and stacked bars.
    pub fn is_grouped(self) -> bool {
        matches!(self, GraphKind::GroupedInterleaved | GraphKind::GroupedSeparated | GraphKind::StackedBar | GraphKind::StackedPercent)
    }

    /// Bars laid on top of each other rather than side by side.
    pub fn is_stacked(self) -> bool {
        matches!(self, GraphKind::StackedBar | GraphKind::StackedPercent)
    }

    /// XY graphs drawn with lines (through the means, or one per replicate).
    pub fn has_line(self) -> bool {
        matches!(self, GraphKind::XyPointsLine | GraphKind::XyLine)
    }

    pub fn has_bars(self) -> bool {
        matches!(self, GraphKind::ColumnBar | GraphKind::GroupedInterleaved | GraphKind::GroupedSeparated) || self.is_stacked()
    }

    /// One slot per X category with no data sets: column bars, boxes, violins and the
    /// slices of a pie.
    pub fn is_column_like(self) -> bool {
        matches!(self, GraphKind::ColumnBar | GraphKind::Box | GraphKind::Violin) || self.is_pie()
    }

    /// Pie or donut: parts of a whole, drawn without axes.
    pub fn is_pie(self) -> bool {
        matches!(self, GraphKind::Pie | GraphKind::Donut)
    }

    /// Box-and-whisker or violin: a distribution per category rather than a bar.
    pub fn is_distribution(self) -> bool {
        matches!(self, GraphKind::Box | GraphKind::Violin)
    }

    /// Graphs whose X values Otto computes rather than reading from a column.
    pub fn x_is_derived(self) -> bool {
        self == GraphKind::Qq
    }

    /// Whether the graph draws point symbols that a symbol shape applies to. Bars, boxes and
    /// violins draw one per replicate, so the choice reaches them too.
    pub fn has_symbols(self) -> bool {
        !matches!(self, GraphKind::XyLine | GraphKind::Heatmap | GraphKind::Survival | GraphKind::Contingency) && !self.is_stacked() && !self.is_pie()
    }
}

impl std::fmt::Display for GraphKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Fixed publication formats. Sizes are axis lengths (plot area); labels sit outside.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    /// 15 mm wide × 30 mm tall (X:Y = 1:2), e.g. two or three bars.
    Slim,
    /// 30 × 30 mm (X:Y = 1:1).
    #[default]
    Square,
    /// 45 mm wide × 30 mm tall (X:Y = 1.5:1).
    Wide,
    /// 60 mm wide × 30 mm tall (X:Y = 2:1).
    UltraWide,
}

impl Format {
    pub const ALL: [Format; 4] = [Format::Slim, Format::Square, Format::Wide, Format::UltraWide];

    /// (X axis length, Y axis length) in millimetres.
    pub fn axis_mm(self) -> (f64, f64) {
        match self {
            Format::Slim => (15.0, 30.0),
            Format::Square => (30.0, 30.0),
            Format::Wide => (45.0, 30.0),
            Format::UltraWide => (60.0, 30.0),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Format::Slim => "Slim 15×30 mm",
            Format::Square => "Square 30×30 mm",
            Format::Wide => "Wide 45×30 mm",
            Format::UltraWide => "Ultra-wide 60×30 mm",
        }
    }
}

impl std::fmt::Display for Format {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Bar height / symbol position.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Center {
    #[default]
    Mean,
    Median,
}

/// Error bar type.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorBars {
    /// Standard deviation.
    #[default]
    Sd,
    /// Standard error of the mean.
    Sem,
    /// 95% confidence interval of the mean.
    Ci95,
    /// Minimum to maximum.
    Range,
    /// No error bars.
    None,
}

/// XY graphs: how replicate Y values at the same X are drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Replicates {
    /// One symbol at the mean (or median) with error bars.
    #[default]
    Summary,
    /// Every replicate as its own symbol.
    Individual,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct Summary {
    #[serde(default)]
    pub center: Center,
    #[serde(default)]
    pub error: ErrorBars,
    #[serde(default)]
    pub replicates: Replicates,
}

macro_rules! simple_display {
    ($t:ty { $($v:ident => $s:expr),* $(,)? }) => {
        impl $t {
            pub const ALL: &'static [$t] = &[$(<$t>::$v),*];
            pub fn label(self) -> &'static str { match self { $(<$t>::$v => $s),* } }
        }
        impl std::fmt::Display for $t {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(self.label()) }
        }
    };
}

pub(crate) use simple_display;

simple_display!(Center { Mean => "Mean", Median => "Median" });
simple_display!(ErrorBars { Sd => "SD", Sem => "SEM", Ci95 => "95% CI", Range => "Range", None => "None" });
simple_display!(Replicates { Summary => "Mean ± error", Individual => "Each replicate" });

/// Color scale of a heatmap.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HeatScale {
    /// Viridis (perceptually uniform, colour-blind safe), dark purple → yellow.
    #[default]
    Viridis,
    /// White → dark blue.
    Blues,
    /// Diverging blue → white → red, centred on 0 (or the middle of the range).
    BlueRed,
    /// Light → dark grey.
    Grayscale,
}

/// Rescaling of heatmap values before colouring.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HeatNormalize {
    /// The cell values as they are.
    #[default]
    None,
    /// Z-score within each row (each data set): (value − row mean) / row SD.
    Rows,
    /// Z-score within each column (each X category).
    Columns,
}

simple_display!(HeatScale { Viridis => "Viridis", Blues => "Blues", BlueRed => "Blue – white – red", Grayscale => "Grayscale" });
simple_display!(HeatNormalize { None => "Values", Rows => "Z-score per row", Columns => "Z-score per column" });

/// How a heatmap is coloured.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct HeatmapOptions {
    #[serde(default)]
    pub scale: HeatScale,
    #[serde(default)]
    pub normalize: HeatNormalize,
    /// Print each cell's value in the cell.
    #[serde(default)]
    pub show_values: bool,
    /// Decimal places of the printed cell values and colour-bar labels (default 2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decimals: Option<usize>,
}

/// Decimal places a heatmap prints (2 unless set).
pub const HEATMAP_DECIMALS: usize = 2;

impl HeatmapOptions {
    pub fn digits(&self) -> usize {
        self.decimals.unwrap_or(HEATMAP_DECIMALS).min(10)
    }
}

/// Color scheme for data sets. Journal-inspired (the house palettes of the ggsci package,
/// which approximate each journal's figures; they are not official). Every scheme starts
/// with its red and its blue, the two colors the focused data sets take.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Palette {
    /// Nature Publishing Group.
    #[default]
    #[serde(alias = "colors")]
    Npg,
    /// Science / AAAS.
    Aaas,
    /// New England Journal of Medicine.
    Nejm,
    /// The Lancet.
    Lancet,
    /// Journal of the American Medical Association.
    Jama,
    /// Okabe–Ito, colorblind-safe.
    OkabeIto,
    /// Shades of gray.
    Grayscale,
    /// Prism classic: white, gray and black fills.
    Classic,
}

simple_display!(Palette {
    Npg => "Nature",
    Aaas => "Science",
    Nejm => "NEJM",
    Lancet => "Lancet",
    Jama => "JAMA",
    OkabeIto => "Colorblind safe",
    Grayscale => "Grayscale",
    Classic => "Prism classic",
});

/// Symbol shape of a data set. Circles everywhere is the default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Symbol {
    #[default]
    Circle,
    Square,
    TriangleUp,
    TriangleDown,
    Diamond,
    Hexagon,
}

simple_display!(Symbol {
    Circle => "Circle",
    Square => "Square",
    TriangleUp => "Triangle up",
    TriangleDown => "Triangle down",
    Diamond => "Diamond",
    Hexagon => "Hexagon",
});

/// The order symbols are handed out in when a graph varies them per data set.
pub const SYMBOL_CYCLE: [Symbol; 6] = [
    Symbol::Circle,
    Symbol::Square,
    Symbol::TriangleUp,
    Symbol::Diamond,
    Symbol::TriangleDown,
    Symbol::Hexagon,
];

/// Whiskers of a box plot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Whiskers {
    /// Tukey: 1.5 × IQR past the quartiles; values beyond are drawn as outliers.
    #[default]
    Tukey,
    /// Whiskers down to the smallest and up to the largest value.
    MinMax,
    /// 10th to 90th percentile; values beyond are drawn as outliers.
    Pct10To90,
}

simple_display!(Whiskers { Tukey => "Tukey (1.5 × IQR)", MinMax => "Min to max", Pct10To90 => "10 – 90 percentile" });

/// Volcano plot cutoffs and labels.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VolcanoOptions {
    /// Y already holds -log10(P); otherwise Y is a raw P value and Otto transforms it.
    #[serde(default)]
    pub y_is_log: bool,
    /// |log2 fold change| above which a point counts as changed.
    #[serde(default = "one")]
    pub fc_cutoff: f64,
    /// P value below which a point counts as significant (on the raw P scale).
    #[serde(default = "p05")]
    pub p_cutoff: f64,
    /// Column whose values label the most significant points.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// How many of the most significant changed points to label (0 = none).
    #[serde(default)]
    pub label_top: usize,
}

fn one() -> f64 {
    1.0
}

fn p05() -> f64 {
    0.05
}

impl Default for VolcanoOptions {
    fn default() -> Self {
        VolcanoOptions { y_is_log: false, fc_cutoff: 1.0, p_cutoff: 0.05, label: None, label_top: 0 }
    }
}

/// How an axis maps values to positions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AxisScale {
    /// Evenly spaced values.
    #[default]
    Linear,
    /// Base-10 logarithmic; ticks at decades, labelled in the data's own units.
    Log10,
    /// Base-2 logarithmic (fold changes).
    Log2,
    /// Natural logarithm.
    Ln,
}

simple_display!(AxisScale { Linear => "Linear", Log10 => "Log10", Log2 => "Log2", Ln => "Ln (natural log)" });

impl AxisScale {
    pub fn is_log(self) -> bool {
        self != AxisScale::Linear
    }

    /// The log base, or `None` for a linear axis.
    pub fn base(self) -> Option<f64> {
        match self {
            AxisScale::Linear => None,
            AxisScale::Log10 => Some(10.0),
            AxisScale::Log2 => Some(2.0),
            AxisScale::Ln => Some(std::f64::consts::E),
        }
    }

    /// Value → axis position (identity on a linear axis; NaN for values a log axis cannot show).
    pub fn forward(self, v: f64) -> f64 {
        match self.base() {
            None => v,
            Some(b) => {
                if v > 0.0 {
                    v.ln() / b.ln()
                } else {
                    f64::NAN
                }
            }
        }
    }

    /// Axis position → value.
    pub fn inverse(self, t: f64) -> f64 {
        match self.base() {
            None => t,
            Some(b) => b.powf(t),
        }
    }
}

/// How tick labels are written.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AxisNumbers {
    /// 0.5, 10, 1000 (1e5 notation where plain digits would get long).
    #[default]
    Plain,
    /// Thousands separated by thin spaces: 1 000, 25 000.
    Grouped,
    /// A digit and a power of ten: 1.5×10³.
    Scientific,
    /// Powers of ten alone: 10³ (values that are not whole powers keep their digits).
    Power10,
    /// The exponent itself, so a log axis reads 0, 1, 2 in place of 1, 10, 100.
    LogValue,
}

simple_display!(AxisNumbers {
    Plain => "Normal",
    Grouped => "Grouped (1 000)",
    Scientific => "Scientific (1.5×10³)",
    Power10 => "Power of 10 (10³)",
    LogValue => "Log value (3)",
});

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AxisSpec {
    /// Axis title; None uses the column name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Fixed axis minimum; None = automatic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    /// Fixed axis maximum; None = automatic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    /// Linear (default) or a logarithmic scale. Limits are given in the data's own units.
    #[serde(default, skip_serializing_if = "crate::spec::is_linear")]
    pub scale: AxisScale,
    /// How the tick labels are written (plain numbers by default).
    #[serde(default, skip_serializing_if = "crate::spec::is_plain_numbers")]
    pub numbers: AxisNumbers,
}

pub(crate) fn is_linear(s: &AxisScale) -> bool {
    *s == AxisScale::Linear
}

pub(crate) fn is_plain_numbers(n: &AxisNumbers) -> bool {
    *n == AxisNumbers::Plain
}

/// Keeps only the rows that pass on `column`: for a categorical or text column, those
/// whose value is one of `include`; for a numeric one, those inside `min`..`max`. Both can
/// be given, and an empty filter keeps every row.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Filter {
    pub column: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub include: Vec<String>,
    /// Numeric columns: the smallest value to keep. A row with no value is left out of any
    /// range, since it cannot be shown to be inside one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    /// Numeric columns: the largest value to keep.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
}

impl Filter {
    /// Whether the filter keeps every row, which is the same as not having it.
    pub fn keeps_everything(&self) -> bool {
        self.include.is_empty() && self.min.is_none() && self.max.is_none()
    }

    /// Whether row `r` of `column` passes.
    pub fn keeps(&self, col: &crate::table::Column, r: usize) -> bool {
        if !self.include.is_empty() {
            let label = match col.kind() {
                crate::table::ColumnKind::Numeric => col.number(r).map(crate::table::format_number),
                _ => col.label(r).map(str::to_string),
            };
            if !label.is_some_and(|l| self.include.contains(&l)) {
                return false;
            }
        }
        if self.min.is_some() || self.max.is_some() {
            let Some(v) = col.number(r) else { return false };
            if self.min.is_some_and(|lo| v < lo) || self.max.is_some_and(|hi| v > hi) {
                return false;
            }
        }
        true
    }

    /// How the filter reads, e.g. "Dose 10–100" or "Condition: Ctrl, DrugA".
    pub fn describe(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if !self.include.is_empty() {
            parts.push(self.include.join(", "));
        }
        match (self.min, self.max) {
            (Some(lo), Some(hi)) => parts.push(format!("{}–{}", crate::table::format_number(lo), crate::table::format_number(hi))),
            (Some(lo), None) => parts.push(format!("from {}", crate::table::format_number(lo))),
            (None, Some(hi)) => parts.push(format!("up to {}", crate::table::format_number(hi))),
            (None, None) => {}
        }
        format!("{}: {}", self.column, parts.join(", "))
    }
}

fn yes() -> bool {
    true
}

/// Separator used when several columns define one category, e.g. "Construct A | IL12".
pub const COMBINE_SEP: &str = " | ";

/// One column name or a list of names (several columns combine into one category).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl OneOrMany {
    /// The names, with empty strings removed ("" or [] mean "none").
    pub fn into_vec(self) -> Vec<String> {
        let v = match self {
            OneOrMany::One(s) => vec![s],
            OneOrMany::Many(v) => v,
        };
        v.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
    }
}

/// Reads a column list that older project files stored as a single string (or null).
fn one_or_many<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    Ok(Option::<OneOrMany>::deserialize(d)?.map(OneOrMany::into_vec).unwrap_or_default())
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GraphSpec {
    pub id: String,
    pub name: String,
    pub table_id: String,
    pub kind: GraphKind,
    /// Categorical X columns (combined, "A | B") or the single numeric X of XY graphs.
    #[serde(default, deserialize_with = "one_or_many")]
    pub x: Vec<String>,
    #[serde(default)]
    pub y: Vec<String>,
    /// Columns whose value combinations form the data sets.
    #[serde(default, deserialize_with = "one_or_many")]
    pub group: Vec<String>,
    #[serde(default)]
    pub filters: Vec<Filter>,
    #[serde(default)]
    pub format: Format,
    #[serde(default)]
    pub summary: Summary,
    #[serde(default)]
    pub x_axis: AxisSpec,
    #[serde(default)]
    pub y_axis: AxisSpec,
    #[serde(default)]
    pub palette: Palette,
    /// Data sets "in focus": the first takes the palette's red, the second its blue, and
    /// the rest follow. Names are data sets (bars on a column graph).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub focus: Vec<String>,
    /// Highlight mode: the focused data sets keep their color, every other one turns gray.
    #[serde(default)]
    pub highlight: bool,
    #[serde(default = "yes")]
    pub show_legend: bool,
    /// XY graphs showing each replicate: the column that says which subject a value
    /// belongs to (e.g. "Mouse"); each one gets its own line. Drawing those lines needs
    /// it, since the place of a value among the ones at its X says nothing about whose it
    /// is. Graphs that only draw points fall back to "the k-th value at each X is
    /// replicate k", which names the replicate subcolumns of the plotted table and
    /// nothing else.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replicate_by: Option<String>,
    /// Heatmaps: colour scale, normalisation and cell values.
    #[serde(default)]
    pub heatmap: HeatmapOptions,
    /// Symbol shape per data set; empty (the default) draws circles everywhere.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub symbols: Vec<Symbol>,
    /// Box plots: what the whiskers span.
    #[serde(default)]
    pub whiskers: Whiskers,
    /// Volcano plots: cutoffs and labels.
    #[serde(default)]
    pub volcano: VolcanoOptions,
    /// Set on final graphs: where their locked data copy came from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked: Option<LockInfo>,
}

/// Provenance of a final graph's locked data copy.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LockInfo {
    /// The quick graph it was finalized from.
    pub from_graph: String,
    /// The data table the copy was taken from (name at that time).
    pub from_table: String,
    /// Its id: deleting that data table deletes this final graph too.
    #[serde(default)]
    pub table_id: String,
    /// When the copy was taken (UTC, "YYYY-MM-DD HH:MM UTC").
    pub at: String,
}

impl GraphSpec {
    /// A final graph (statistics, locked data) rather than a quick graph.
    pub fn is_final(&self) -> bool {
        self.locked.is_some()
    }

    /// The symbol of data set `i`: what was chosen for it, else a circle.
    pub fn symbol(&self, i: usize) -> Symbol {
        self.symbols.get(i).copied().unwrap_or_default()
    }

    pub fn new(id: impl Into<String>, name: impl Into<String>, table_id: impl Into<String>, kind: GraphKind) -> Self {
        GraphSpec {
            id: id.into(),
            name: name.into(),
            table_id: table_id.into(),
            kind,
            x: Vec::new(),
            y: Vec::new(),
            group: Vec::new(),
            filters: Vec::new(),
            format: Format::default(),
            summary: Summary::default(),
            x_axis: AxisSpec::default(),
            y_axis: AxisSpec::default(),
            palette: Palette::default(),
            focus: Vec::new(),
            highlight: false,
            show_legend: true,
            replicate_by: None,
            heatmap: HeatmapOptions::default(),
            symbols: Vec::new(),
            whiskers: Whiskers::default(),
            volcano: VolcanoOptions::default(),
            locked: None,
        }
    }

    /// Every column this graph reads (for rename/delete bookkeeping).
    pub fn referenced_columns(&self) -> Vec<&str> {
        let mut cols: Vec<&str> = self.x.iter().map(String::as_str).collect();
        cols.extend(self.y.iter().map(String::as_str));
        cols.extend(self.group.iter().map(String::as_str));
        cols.extend(self.filters.iter().map(|f| f.column.as_str()));
        cols.extend(self.replicate_by.as_deref());
        cols.extend(self.volcano.label.as_deref());
        cols
    }

    pub fn rename_column(&mut self, old: &str, new: &str) {
        let fix = |s: &mut String| {
            if s == old {
                *s = new.to_string();
            }
        };
        self.x.iter_mut().for_each(fix);
        self.y.iter_mut().for_each(fix);
        self.group.iter_mut().for_each(fix);
        self.filters.iter_mut().for_each(|f| fix(&mut f.column));
        self.replicate_by.iter_mut().for_each(fix);
        self.volcano.label.iter_mut().for_each(fix);
    }
}

/// Layout of exported plot data.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DataLayout {
    /// Tidy: one row per plotted point (Row, X, data set, Y).
    #[default]
    Long,
    /// Prism table: each bar / data set is a block of side-by-side replicate subcolumns
    /// (title row, "replicate n" row, then the values: one row for bar graphs, one per X
    /// value for XY graphs). Pastes into Prism as is.
    Wide,
    /// One row per bar / symbol: n, mean, SD, SEM, median, min, max, 95% CI.
    Summary,
}

simple_display!(DataLayout { Long => "Long (tidy)", Wide => "Prism (replicates side by side)", Summary => "Summary statistics" });

/// Vector/bitmap export formats for graphs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    /// Scalable Vector Graphics (editable text, named layers).
    Svg,
    /// PDF with embedded fonts.
    Pdf,
    /// Enhanced Metafile (Windows; pastes as vectors into Office).
    Emf,
    /// 600 dpi PNG bitmap.
    Png,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Svg => "svg",
            ExportFormat::Pdf => "pdf",
            ExportFormat::Emf => "emf",
            ExportFormat::Png => "png",
        }
    }

    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "svg" => Some(ExportFormat::Svg),
            "pdf" => Some(ExportFormat::Pdf),
            "emf" => Some(ExportFormat::Emf),
            "png" => Some(ExportFormat::Png),
            _ => None,
        }
    }
}

/// Analyses shown in the Results section.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnalysisKind {
    /// n, mean, SD, SEM, median, quartiles, min/max and 95% CI per column and group.
    Descriptive {
        y: Vec<String>,
        #[serde(default)]
        group: Option<String>,
    },
    /// t test (or Mann-Whitney / Wilcoxon) between two bars of a graph.
    TTest(crate::analysis::TTestSpec),
    /// One-way ANOVA (or Kruskal-Wallis / Friedman) across bars of a graph, with multiple comparisons.
    OneWayAnova(crate::analysis::AnovaSpec),
    /// Nonlinear regression (curve fit) of the data sets of an XY graph.
    CurveFit(crate::analysis::FitSpec),
    /// Linear regression of the data sets of an XY graph.
    LinearRegression(crate::analysis::LinRegSpec),
    /// Correlation of X and Y in each data set of an XY graph.
    Correlation(crate::analysis::CorrelationSpec),
    /// Two-way ANOVA of a grouped graph: X categories × data sets.
    TwoWayAnova(crate::analysis::TwoWaySpec),
    /// Area under the curve of each data set of an XY graph.
    Auc(crate::analysis::AucSpec),
    /// Kaplan-Meier survival analysis of a survival graph: curves, medians, log-rank and Gehan-Breslow-Wilcoxon tests.
    Survival(crate::analysis::SurvivalSpec),
    /// Normality (and lognormality) tests of the data sets of a graph.
    Normality(crate::analysis::NormalitySpec),
    /// Outlier identification in the data sets of a graph.
    Outliers(crate::analysis::OutlierSpec),
    /// Contingency table: chi-square, Fisher's exact test, McNemar, the test for trend,
    /// relative risk, odds ratio and the diagnostic ratios.
    Contingency(crate::analysis::ContingencySpec),
}

impl AnalysisKind {
    /// The graph whose data sets the analysis uses (None for table-based analyses).
    pub fn graph_id(&self) -> Option<&str> {
        match self {
            AnalysisKind::Descriptive { .. } => None,
            AnalysisKind::TTest(t) => Some(&t.graph_id),
            AnalysisKind::OneWayAnova(a) => Some(&a.graph_id),
            AnalysisKind::CurveFit(f) => Some(&f.graph_id),
            AnalysisKind::LinearRegression(l) => Some(&l.graph_id),
            AnalysisKind::Correlation(c) => Some(&c.graph_id),
            AnalysisKind::TwoWayAnova(t) => Some(&t.graph_id),
            AnalysisKind::Auc(a) => Some(&a.graph_id),
            AnalysisKind::Survival(s) => Some(&s.graph_id),
            AnalysisKind::Normality(n) => Some(&n.graph_id),
            AnalysisKind::Outliers(o) => Some(&o.graph_id),
            AnalysisKind::Contingency(c) => Some(&c.graph_id),
        }
    }

    pub fn graph_id_mut(&mut self) -> Option<&mut String> {
        match self {
            AnalysisKind::Descriptive { .. } => None,
            AnalysisKind::TTest(t) => Some(&mut t.graph_id),
            AnalysisKind::OneWayAnova(a) => Some(&mut a.graph_id),
            AnalysisKind::CurveFit(f) => Some(&mut f.graph_id),
            AnalysisKind::LinearRegression(l) => Some(&mut l.graph_id),
            AnalysisKind::Correlation(c) => Some(&mut c.graph_id),
            AnalysisKind::TwoWayAnova(t) => Some(&mut t.graph_id),
            AnalysisKind::Auc(a) => Some(&mut a.graph_id),
            AnalysisKind::Survival(s) => Some(&mut s.graph_id),
            AnalysisKind::Normality(n) => Some(&mut n.graph_id),
            AnalysisKind::Outliers(o) => Some(&mut o.graph_id),
            AnalysisKind::Contingency(c) => Some(&mut c.graph_id),
        }
    }

    /// The data sets the analysis uses (empty = all of the graph's).
    pub fn data_sets(&self) -> &[String] {
        match self {
            AnalysisKind::Descriptive { .. } => &[],
            AnalysisKind::TTest(t) => &t.data_sets,
            AnalysisKind::OneWayAnova(a) => &a.data_sets,
            AnalysisKind::CurveFit(f) => &f.data_sets,
            AnalysisKind::LinearRegression(l) => &l.data_sets,
            AnalysisKind::Correlation(c) => &c.data_sets,
            AnalysisKind::TwoWayAnova(t) => &t.data_sets,
            AnalysisKind::Auc(a) => &a.data_sets,
            AnalysisKind::Survival(s) => &s.data_sets,
            AnalysisKind::Normality(n) => &n.data_sets,
            AnalysisKind::Outliers(o) => &o.data_sets,
            AnalysisKind::Contingency(c) => &c.data_sets,
        }
    }

    pub fn data_sets_mut(&mut self) -> Option<&mut Vec<String>> {
        match self {
            AnalysisKind::Descriptive { .. } => None,
            AnalysisKind::TTest(t) => Some(&mut t.data_sets),
            AnalysisKind::OneWayAnova(a) => Some(&mut a.data_sets),
            AnalysisKind::CurveFit(f) => Some(&mut f.data_sets),
            AnalysisKind::LinearRegression(l) => Some(&mut l.data_sets),
            AnalysisKind::Correlation(c) => Some(&mut c.data_sets),
            AnalysisKind::TwoWayAnova(t) => Some(&mut t.data_sets),
            AnalysisKind::Auc(a) => Some(&mut a.data_sets),
            AnalysisKind::Survival(s) => Some(&mut s.data_sets),
            AnalysisKind::Normality(n) => Some(&mut n.data_sets),
            AnalysisKind::Outliers(o) => Some(&mut o.data_sets),
            AnalysisKind::Contingency(c) => Some(&mut c.data_sets),
        }
    }

    /// What the analysis works on (None for table-based analyses).
    pub fn shape(&self) -> Option<DataShape> {
        match self {
            AnalysisKind::Descriptive { .. } => None,
            AnalysisKind::TTest(_) | AnalysisKind::OneWayAnova(_) | AnalysisKind::TwoWayAnova(_) | AnalysisKind::Normality(_) | AnalysisKind::Outliers(_) => {
                Some(DataShape::Bars)
            }
            AnalysisKind::CurveFit(_) | AnalysisKind::LinearRegression(_) | AnalysisKind::Correlation(_) | AnalysisKind::Auc(_) => Some(DataShape::Xy),
            AnalysisKind::Survival(_) => Some(DataShape::Survival),
            AnalysisKind::Contingency(_) => Some(DataShape::Counts),
        }
    }

    /// Analyses of the data sets of an XY graph (rather than the bars of a column graph).
    pub fn is_xy(&self) -> bool {
        self.shape() == Some(DataShape::Xy)
    }

    /// Whether the analysis is drawn on its graph (brackets or a fitted curve), or `None`
    /// for the ones that draw nothing.
    pub fn drawn(&self) -> Option<bool> {
        match self {
            AnalysisKind::TTest(t) => Some(t.graph.show),
            AnalysisKind::OneWayAnova(a) => Some(a.graph.show),
            AnalysisKind::TwoWayAnova(t) => Some(t.graph.show),
            AnalysisKind::CurveFit(f) => Some(f.graph.show),
            AnalysisKind::LinearRegression(l) => Some(l.graph.show),
            AnalysisKind::Descriptive { .. }
            | AnalysisKind::Correlation(_)
            | AnalysisKind::Auc(_)
            | AnalysisKind::Survival(_)
            | AnalysisKind::Normality(_)
            | AnalysisKind::Outliers(_)
            | AnalysisKind::Contingency(_) => None,
        }
    }

    /// Draw the analysis on its graph, or take it off.
    pub fn set_drawn(&mut self, show: bool) {
        match self {
            AnalysisKind::TTest(t) => t.graph.show = show,
            AnalysisKind::OneWayAnova(a) => a.graph.show = show,
            AnalysisKind::TwoWayAnova(t) => t.graph.show = show,
            AnalysisKind::CurveFit(f) => f.graph.show = show,
            AnalysisKind::LinearRegression(l) => l.graph.show = show,
            AnalysisKind::Descriptive { .. }
            | AnalysisKind::Correlation(_)
            | AnalysisKind::Auc(_)
            | AnalysisKind::Survival(_)
            | AnalysisKind::Normality(_)
            | AnalysisKind::Outliers(_)
            | AnalysisKind::Contingency(_) => {}
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnalysisSpec {
    pub id: String,
    pub name: String,
    pub table_id: String,
    pub kind: AnalysisKind,
    /// For t tests / ANOVA: the graph choices the analysis was set up on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub basis: Option<crate::analysis::GraphBasis>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_lists_accept_old_and_new_forms() {
        let old = r#"{"id":"G1","name":"g","table_id":"T1","kind":"column_bar","x":"Condition","y":["IL2"],"group":null}"#;
        let g: GraphSpec = serde_json::from_str(old).unwrap();
        assert_eq!(g.x, vec!["Condition"]);
        assert!(g.group.is_empty());
        let new = r#"{"id":"G1","name":"g","table_id":"T1","kind":"column_bar","x":["Construct","Additive"],"group":""}"#;
        let g: GraphSpec = serde_json::from_str(new).unwrap();
        assert_eq!(g.x, vec!["Construct", "Additive"]);
        assert!(g.group.is_empty());
        let back: GraphSpec = serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
        assert_eq!(back, g);
    }

    #[test]
    fn a_numeric_filter_leaves_out_rows_with_no_value() {
        let col = crate::table::Column::numeric("Dose", vec![Some(1.0), Some(10.0), None, Some(100.0)]);
        let f = Filter { column: "Dose".into(), min: Some(5.0), max: Some(50.0), ..Default::default() };
        let kept: Vec<bool> = (0..4).map(|r| f.keeps(&col, r)).collect();
        assert_eq!(kept, vec![false, true, false, false]);
        assert_eq!(f.describe(), "Dose: 5-50".replace('-', "\u{2013}"));
        assert!(!f.keeps_everything());
        assert!(Filter { column: "Dose".into(), ..Default::default() }.keeps_everything());
    }
}
