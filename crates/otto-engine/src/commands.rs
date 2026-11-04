//! Every user action in Otto as a serializable `Command`.
//!
//! The `commands!` macro below is the single source of truth: it defines the enum, the
//! name → arguments parser and the registry the MCP server publishes as tools. GUI buttons
//! emit these same commands, so every button automatically has an MCP tool.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use otto_core::analysis::{
    AnovaSpec, AucSpec, Comparisons, ContingencySpec, CorrMethod, CorrelationSpec, Correction, CurveDisplay, DiffDirection, Distribution, ExtraResults,
    FitModel,
    FitSpec, FixedParam, GraphAnnotation, LinRegSpec, Matching, NormalityTests, OutputFormat, Pairing, SurvivalSpec, TTestSpec, TwoWayCompare,
    TwoWaySpec,
};
use otto_core::analysis::{NormalitySpec, OutlierMethod, OutlierSpec};
use otto_core::import::ImportOptions;
use otto_core::{AxisNumbers, AxisScale, Symbol, Whiskers};
use otto_core::{Center, ColumnKind, DataLayout, ErrorBars, ExportFormat, Filter, Format, GraphKind, HeatNormalize, HeatScale, OneOrMany, Palette, Replicates};

use crate::EngineError;

/// Metadata for one command / MCP tool.
#[derive(Clone, Copy)]
pub struct CommandMeta {
    pub name: &'static str,
    pub description: &'static str,
    /// Whether the command changes the project (and therefore the undo history).
    pub mutates: bool,
    /// Whether the command only reads state.
    pub read_only: bool,
    pub schema: fn() -> serde_json::Value,
}

impl std::fmt::Debug for CommandMeta {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommandMeta").field("name", &self.name).field("mutates", &self.mutates).finish()
    }
}

/// JSON schema for an argument struct, inlined and always an object (as MCP requires).
pub fn schema_of<T: JsonSchema>() -> serde_json::Value {
    let settings = schemars::generate::SchemaSettings::draft2020_12().with(|s| {
        s.inline_subschemas = true;
    });
    let schema = settings.into_generator().into_root_schema_for::<T>();
    let mut v = serde_json::to_value(schema).unwrap_or_default();
    if let Some(obj) = v.as_object_mut() {
        obj.remove("$schema");
        obj.remove("title");
        obj.insert("type".into(), "object".into());
        obj.entry("properties").or_insert_with(|| serde_json::json!({}));
    }
    v
}

macro_rules! commands {
    ($( $(#[doc = $doc:literal])* $variant:ident = $name:literal ($args:ty) [$mutates:literal, $read_only:literal]; )*) => {
        #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
        #[serde(tag = "tool", content = "args")]
        pub enum Command {
            $( #[serde(rename = $name)] $variant($args), )*
        }

        impl Command {
            pub fn name(&self) -> &'static str {
                match self { $( Command::$variant(_) => $name, )* }
            }

            pub fn mutates(&self) -> bool {
                match self { $( Command::$variant(_) => $mutates, )* }
            }

            /// Builds a command from an MCP tool call.
            pub fn parse(name: &str, args: serde_json::Value) -> Result<Command, EngineError> {
                let args = if args.is_null() { serde_json::json!({}) } else { args };
                match name {
                    $( $name => serde_json::from_value::<$args>(args)
                        .map(Command::$variant)
                        .map_err(|e| EngineError::BadArgs { tool: $name.into(), message: e.to_string() }), )*
                    other => Err(EngineError::UnknownTool(other.to_string())),
                }
            }

            /// Arguments as JSON (for forwarding a command to another process).
            pub fn args_json(&self) -> serde_json::Value {
                match self { $( Command::$variant(a) => serde_json::to_value(a).unwrap_or_default(), )* }
            }
        }

        /// All commands, in menu order.
        pub fn registry() -> Vec<CommandMeta> {
            vec![ $( CommandMeta {
                name: $name,
                description: concat!($($doc, "\n"),*),
                mutates: $mutates,
                read_only: $read_only,
                schema: schema_of::<$args>,
            }, )* ]
        }
    };
}

// ---------------------------------------------------------------------------
// Argument types
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoArgs {}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectNewArgs {
    /// Project name (default "Untitled").
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PathArgs {
    /// Absolute file path.
    pub path: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Section {
    /// Data tables (multivariable).
    Tables,
    /// The data each graph plots, Prism-style (one column per bar / data set). Item: a graph.
    PlottedTables,
    /// Quick graphs, for looking through the data.
    Graphs,
    /// Statistics results.
    Results,
    /// Final graphs: statistics on a locked copy of the data, refined for publication.
    FinalGraphs,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NavigateArgs {
    /// Which navigator section to show.
    pub section: Section,
    /// Item id or name (e.g. "T1", "G2", "Graph 1"); default: the first item in the section.
    #[serde(default)]
    pub item: Option<String>,
}

/// Graph zoom in the app.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Zoom {
    /// Fit the graph into the window.
    #[default]
    Fit,
    /// 100%: physical size on screen.
    Actual,
    /// 200%.
    Double,
    /// 400%.
    Quadruple,
}

impl Zoom {
    pub const ALL: [Zoom; 4] = [Zoom::Fit, Zoom::Actual, Zoom::Double, Zoom::Quadruple];
    pub fn label(self) -> &'static str {
        match self {
            Zoom::Fit => "Fit",
            Zoom::Actual => "100%",
            Zoom::Double => "200%",
            Zoom::Quadruple => "400%",
        }
    }
    pub fn factor(self) -> Option<f32> {
        match self {
            Zoom::Fit => None,
            Zoom::Actual => Some(1.0),
            Zoom::Double => Some(2.0),
            Zoom::Quadruple => Some(4.0),
        }
    }
}

impl std::fmt::Display for Zoom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ZoomArgs {
    pub zoom: Zoom,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ColumnDef {
    pub name: String,
    #[serde(default = "numeric")]
    pub kind: ColumnKind,
}

fn numeric() -> ColumnKind {
    ColumnKind::Numeric
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TableCreateArgs {
    /// Table name (default "Data N").
    #[serde(default)]
    pub name: Option<String>,
    /// Columns to create (default: three numeric columns).
    #[serde(default)]
    pub columns: Vec<ColumnDef>,
    /// Number of empty rows (default 12).
    #[serde(default)]
    pub rows: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImportFileArgs {
    /// Absolute path to a .csv, .tsv, .txt, .xlsx, .xls or .ods file. By default the first
    /// row holds the column names and everything else is worked out from the file.
    pub path: String,
    /// Worksheet name for workbooks (default: the first sheet).
    #[serde(default)]
    pub sheet: Option<String>,
    /// Table name (default: the file name).
    #[serde(default)]
    pub name: Option<String>,
    /// How to read the file, when the defaults get it wrong: rows lists what each leading
    /// row is ("name", "data" or "skip" — several name rows join with " | ", so a header
    /// spread over two rows becomes "Treatment | Low"), separator and encoding and decimal
    /// override what would be detected, and columns gives a type or drops a column.
    /// table_import_preview shows what a given set of options would produce.
    #[serde(default)]
    pub options: Option<ImportOptions>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImportPreviewArgs {
    /// Absolute path to the file to look at.
    pub path: String,
    /// Worksheet name for workbooks (default: the first sheet).
    #[serde(default)]
    pub sheet: Option<String>,
    /// The options to preview (default: what the file suggests).
    #[serde(default)]
    pub options: Option<ImportOptions>,
    /// How many of the file's rows to show (default 20).
    #[serde(default)]
    pub rows: Option<usize>,
}

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImportTextArgs {
    /// Comma-, semicolon- or tab-separated text (e.g. copied from Excel).
    pub text: String,
    /// Table name (default "Data N").
    #[serde(default)]
    pub name: Option<String>,
    /// Whether the first row holds column names (default true).
    #[serde(default = "yes")]
    pub has_header: bool,
    /// How to read the text, when the defaults get it wrong (see table_import_file).
    /// Given, it replaces has_header.
    #[serde(default)]
    pub options: Option<ImportOptions>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TableRef {
    /// Table id (e.g. "T1") or name. Default: the table shown in the app.
    #[serde(default)]
    pub table: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TextFormat {
    #[default]
    Json,
    Csv,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TableGetArgs {
    #[serde(default)]
    pub table: Option<String>,
    /// First row (0-based, default 0).
    #[serde(default)]
    pub offset: usize,
    /// Maximum rows (default 200).
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub format: TextFormat,
}

fn default_limit() -> usize {
    200
}

/// A column, either by name or by 0-based index. Which one it is comes from the JSON type,
/// so a number is an index and a string is a name: `0` is the first column, while `"0"` is
/// a column actually called "0". Prefer the name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ColumnRef {
    Index(usize),
    Name(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetCellsArgs {
    #[serde(default)]
    pub table: Option<String>,
    /// 0-based row of the top-left cell.
    pub row: usize,
    /// Column of the top-left cell (0-based index or name).
    pub column: ColumnRef,
    /// Rows of cell values (numbers, text or null to clear). The table grows as needed;
    /// empty columns take the type of the values written into them.
    pub values: Vec<Vec<serde_json::Value>>,
    /// The first row of `values` holds column names (a pasted header row): they rename the
    /// columns from `column` on, and the remaining rows are written from `row` down.
    #[serde(default)]
    pub header: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AddRowsArgs {
    #[serde(default)]
    pub table: Option<String>,
    /// How many rows to add.
    pub count: usize,
    /// Insert before this 0-based row (default: append).
    #[serde(default)]
    pub at: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeleteRowsArgs {
    #[serde(default)]
    pub table: Option<String>,
    /// 0-based row indices.
    pub rows: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AddColumnArgs {
    #[serde(default)]
    pub table: Option<String>,
    pub name: String,
    #[serde(default = "numeric")]
    pub kind: ColumnKind,
    /// Insert at this 0-based column position (default: append).
    #[serde(default)]
    pub at: Option<usize>,
    /// Optional initial values, top to bottom.
    #[serde(default)]
    pub values: Vec<serde_json::Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeleteColumnsArgs {
    #[serde(default)]
    pub table: Option<String>,
    pub columns: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RenameColumnArgs {
    #[serde(default)]
    pub table: Option<String>,
    pub column: String,
    pub new_name: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetColumnTypeArgs {
    #[serde(default)]
    pub table: Option<String>,
    pub column: String,
    pub kind: ColumnKind,
    /// How a numeric column's numbers are written: plain, `percent`, `date` (whole days
    /// since 1970-01-01) or `time` (seconds since midnight). Ignored for other kinds.
    #[serde(default)]
    pub format: otto_core::NumberFormat,
}

impl SetColumnTypeArgs {
    pub fn column_type(&self) -> otto_core::ColumnType {
        otto_core::ColumnType::of(self.kind, self.format)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CategoryOrderArgs {
    #[serde(default)]
    pub table: Option<String>,
    /// A categorical column.
    pub column: String,
    /// Every level of the column, in the desired order (sets bar order on graphs).
    pub levels: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RenameLevelArgs {
    #[serde(default)]
    pub table: Option<String>,
    pub column: String,
    pub level: String,
    /// New label; renaming onto an existing level merges the two.
    pub new_name: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RenameTableArgs {
    #[serde(default)]
    pub table: Option<String>,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DuplicateTableArgs {
    #[serde(default)]
    pub table: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphCreateArgs {
    /// Source table id or name (default: the active table).
    #[serde(default)]
    pub table: Option<String>,
    pub kind: GraphKind,
    /// X column(s): a name or a list. Several categorical columns combine into one bar per
    /// value combination, labelled "A | B" (e.g. ["Construct", "Additive"]). XY graphs take one
    /// numeric column. Omit to pick automatically; "" or [] for none (column bar: one bar per Y column).
    #[serde(default)]
    pub x: Option<OneOrMany>,
    /// One or more numeric Y columns. Several Y columns become data sets. Omit to pick automatically.
    #[serde(default)]
    pub y: Vec<String>,
    /// Column(s) that split the data into data sets (colors / grouped bars); several columns
    /// combine into one data set per value combination ("A | B").
    #[serde(default)]
    pub group: Option<OneOrMany>,
    /// Keep only rows matching these filters.
    #[serde(default)]
    pub filters: Vec<Filter>,
    #[serde(default)]
    pub format: Option<Format>,
    /// Graph name (default "Graph N").
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphRef {
    /// Graph id (e.g. "G1") or name. Default: the graph shown in the app.
    #[serde(default)]
    pub graph: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FinalizeArgs {
    /// Quick graph id (e.g. "G1") or name. Default: the graph shown in the app.
    #[serde(default)]
    pub graph: Option<String>,
    /// Name of the final graph (default "<graph> (final)").
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphKindArgs {
    #[serde(default)]
    pub graph: Option<String>,
    pub kind: GraphKind,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphMappingArgs {
    #[serde(default)]
    pub graph: Option<String>,
    /// Re-link the graph to another table.
    #[serde(default)]
    pub table: Option<String>,
    /// New X column(s), a name or a list ("" or [] clears it).
    #[serde(default)]
    pub x: Option<OneOrMany>,
    /// New Y columns.
    #[serde(default)]
    pub y: Option<Vec<String>>,
    /// New group column(s), a name or a list ("" or [] clears it).
    #[serde(default)]
    pub group: Option<OneOrMany>,
    /// The column that says which subject (e.g. "Mouse") a value belongs to. An XY graph
    /// drawing one line per replicate needs it: without it there is nothing that ties a
    /// value at one X to the same subject's value at the next ("" clears it).
    #[serde(default)]
    pub replicate_by: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphFiltersArgs {
    #[serde(default)]
    pub graph: Option<String>,
    /// Replaces all filters; an empty list shows every row.
    pub filters: Vec<Filter>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphFormatArgs {
    #[serde(default)]
    pub graph: Option<String>,
    pub format: Format,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphSummaryArgs {
    #[serde(default)]
    pub graph: Option<String>,
    #[serde(default)]
    pub center: Option<Center>,
    #[serde(default)]
    pub error: Option<ErrorBars>,
    /// XY graphs: show replicates as mean ± error or individually (each point; lines: one line per replicate).
    #[serde(default)]
    pub replicates: Option<Replicates>,
    /// The column that says which subject a value belongs to, e.g. "Mouse" (each gets its own line). Also settable with graph_set_mapping, where it belongs; "" clears it.
    #[serde(default)]
    pub replicate_by: Option<String>,
    /// Box plots: what the whiskers span (tukey, min_max or pct10_to_90).
    #[serde(default)]
    pub whiskers: Option<Whiskers>,
}

/// Heatmap appearance: only the fields given change.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphHeatmapArgs {
    #[serde(default)]
    pub graph: Option<String>,
    /// Color scale: viridis, blues, blue_red (diverging, centred on 0) or grayscale.
    #[serde(default)]
    pub scale: Option<HeatScale>,
    /// none (the cell means), rows or columns (z-scores within each row / column).
    #[serde(default)]
    pub normalize: Option<HeatNormalize>,
    /// Print each cell's value in the cell.
    #[serde(default)]
    pub show_values: Option<bool>,
    /// Decimal places of the cell values and the colour-bar labels (2 by default).
    #[serde(default)]
    pub decimals: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    X,
    Y,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphAxisArgs {
    #[serde(default)]
    pub graph: Option<String>,
    pub axis: Axis,
    /// Axis title; "" returns to the automatic title (the column name).
    #[serde(default)]
    pub title: Option<String>,
    /// Fixed minimum.
    #[serde(default)]
    pub min: Option<f64>,
    /// Fixed maximum.
    #[serde(default)]
    pub max: Option<f64>,
    /// true clears fixed limits.
    #[serde(default)]
    pub auto_range: bool,
    /// linear (default), log10, log2 or ln. Fixed limits stay in the data's own units.
    #[serde(default)]
    pub scale: Option<AxisScale>,
    /// How the tick labels are written: plain (default), grouped (1 000), scientific (1.5x10^3), power10 (10^3) or log_value (the exponent: 0, 1, 2 for 1, 10, 100).
    #[serde(default)]
    pub numbers: Option<AxisNumbers>,
}

/// Symbol shapes of a graph's data sets.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphSymbolsArgs {
    #[serde(default)]
    pub graph: Option<String>,
    /// One shape per data set, in legend order: circle, square, triangle_up, triangle_down,
    /// diamond or hexagon. A shorter list repeats; an empty list returns to circles.
    #[serde(default)]
    pub symbols: Vec<Symbol>,
    /// Instead of `symbols`: hand out a different shape per data set automatically.
    #[serde(default)]
    pub vary: bool,
}

/// Volcano plot cutoffs and labels: only the fields given change.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphVolcanoArgs {
    #[serde(default)]
    pub graph: Option<String>,
    /// true when Y already holds -log10(P); false (the default) when Y is a raw P value.
    #[serde(default)]
    pub y_is_log: Option<bool>,
    /// |log2 fold change| above which a point counts as changed (default 1).
    #[serde(default)]
    pub fc_cutoff: Option<f64>,
    /// P value below which a point counts as significant (default 0.05).
    #[serde(default)]
    pub p_cutoff: Option<f64>,
    /// Column whose values label the most significant changed points ("" = no labels).
    #[serde(default)]
    pub label: Option<String>,
    /// How many of the most significant changed points to label (0 = none).
    #[serde(default)]
    pub label_top: Option<usize>,
}

/// Normality tests of the data sets of a graph.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NormalityArgs {
    /// Result name (default "Normality of <graph>").
    #[serde(default)]
    pub name: Option<String>,
    #[serde(flatten)]
    pub spec: NormalitySpec,
}

/// Changes to an existing normality analysis: only the fields given change.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditNormalityArgs {
    #[serde(default)]
    pub analysis: Option<String>,
    #[serde(default)]
    pub data_sets: Option<Vec<String>>,
    #[serde(default)]
    pub tests: Option<NormalityTests>,
    #[serde(default)]
    pub lognormal: Option<bool>,
    #[serde(default)]
    pub output: Option<OutputFormat>,
}

impl EditNormalityArgs {
    pub fn apply(&self, s: &mut NormalitySpec) {
        if let Some(v) = &self.data_sets {
            s.data_sets = v.clone();
        }
        if let Some(v) = self.tests {
            s.tests = v;
        }
        if let Some(v) = self.lognormal {
            s.lognormal = v;
        }
        if let Some(v) = self.output {
            s.output = v;
        }
    }
}

/// Outlier identification in the data sets of a graph.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OutlierArgs {
    /// Result name (default "Outliers of <graph>").
    #[serde(default)]
    pub name: Option<String>,
    #[serde(flatten)]
    pub spec: OutlierSpec,
}

/// Changes to an existing outlier analysis: only the fields given change.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditOutlierArgs {
    #[serde(default)]
    pub analysis: Option<String>,
    #[serde(default)]
    pub data_sets: Option<Vec<String>>,
    #[serde(default)]
    pub method: Option<OutlierMethod>,
    #[serde(default)]
    pub alpha: Option<f64>,
    #[serde(default)]
    pub max_outliers: Option<usize>,
    #[serde(default)]
    pub output: Option<OutputFormat>,
}

impl EditOutlierArgs {
    pub fn apply(&self, s: &mut OutlierSpec) {
        if let Some(v) = &self.data_sets {
            s.data_sets = v.clone();
        }
        if let Some(v) = self.method {
            s.method = v;
        }
        if let Some(v) = self.alpha {
            s.alpha = v;
        }
        if let Some(v) = self.max_outliers {
            s.max_outliers = v;
        }
        if let Some(v) = self.output {
            s.output = v;
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphPaletteArgs {
    #[serde(default)]
    pub graph: Option<String>,
    pub palette: Palette,
    /// Highlight mode: focused data sets keep their color, the rest turn gray. Left alone
    /// when not given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highlight: Option<bool>,
}

/// Which data sets are "in focus": the first takes the palette's red, the second its blue.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphFocusArgs {
    #[serde(default)]
    pub graph: Option<String>,
    /// Data set names (bars on a column graph), in the order they take the palette's colors.
    /// An empty list clears the focus.
    #[serde(default)]
    pub focus: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphLegendArgs {
    #[serde(default)]
    pub graph: Option<String>,
    pub show: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RenameGraphArgs {
    #[serde(default)]
    pub graph: Option<String>,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DuplicateGraphArgs {
    #[serde(default)]
    pub graph: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreviewArgs {
    #[serde(default)]
    pub graph: Option<String>,
    /// Image resolution (default 300 dpi).
    #[serde(default)]
    pub dpi: Option<f32>,
    /// Also return the SVG source.
    #[serde(default)]
    pub include_svg: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlotDataArgs {
    #[serde(default)]
    pub graph: Option<String>,
    #[serde(default)]
    pub layout: DataLayout,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportFileArgs {
    #[serde(default)]
    pub graph: Option<String>,
    /// Absolute output path. The format follows the extension unless `format` is given.
    pub path: String,
    #[serde(default)]
    pub format: Option<ExportFormat>,
    /// SVG only: convert text to outlines.
    #[serde(default)]
    pub outline_text: bool,
    /// Paint a white background instead of transparent.
    #[serde(default)]
    pub background: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportDataArgs {
    #[serde(default)]
    pub graph: Option<String>,
    /// Absolute path of the .csv file to write.
    pub path: String,
    #[serde(default)]
    pub layout: DataLayout,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CopyDataArgs {
    #[serde(default)]
    pub graph: Option<String>,
    #[serde(default)]
    pub layout: DataLayout,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportCodeArgs {
    #[serde(default)]
    pub graph: Option<String>,
    /// Absolute path of the .txt/.R file to write; omit to just return the code.
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DescriptiveArgs {
    #[serde(default)]
    pub table: Option<String>,
    /// Numeric columns to describe (default: all numeric columns).
    #[serde(default)]
    pub y: Vec<String>,
    /// Optional categorical column to describe each group separately.
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AnalysisRef {
    /// Analysis id (e.g. "R1") or name. Default: the analysis shown in the app.
    #[serde(default)]
    pub analysis: Option<String>,
}

/// Which out-of-date analyses to recalculate.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecalculateArgs {
    /// Analysis id (e.g. "R2") or name. Default: every out-of-date analysis of `graph`.
    #[serde(default)]
    pub analysis: Option<String>,
    /// Graph id or name whose out-of-date analyses to recalculate. Default: the graph shown in the app.
    #[serde(default)]
    pub graph: Option<String>,
}

/// A t test between two bars (data sets) of a graph.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TTestArgs {
    /// Result name (default "t test of <graph>").
    #[serde(default)]
    pub name: Option<String>,
    #[serde(flatten)]
    pub spec: TTestSpec,
}


/// A one-way ANOVA across bars (data sets) of a graph.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AnovaArgs {
    /// Result name (default "One-way ANOVA of <graph>").
    #[serde(default)]
    pub name: Option<String>,
    #[serde(flatten)]
    pub spec: AnovaSpec,
}

/// Changes to an existing t test: only the fields given change.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditTTestArgs {
    /// Analysis id (e.g. "R2") or name. Default: the result shown in the app.
    #[serde(default)]
    pub analysis: Option<String>,
    #[serde(default)]
    pub graph_id: Option<String>,
    #[serde(default)]
    pub data_sets: Option<Vec<String>>,
    #[serde(default)]
    pub design: Option<Pairing>,
    /// "Matched by" column for paired data ("" clears it).
    #[serde(default)]
    pub match_by: Option<String>,
    #[serde(default)]
    pub distribution: Option<Distribution>,
    #[serde(default)]
    pub welch: Option<bool>,
    #[serde(default)]
    pub one_tailed: Option<bool>,
    #[serde(default)]
    pub difference: Option<DiffDirection>,
    #[serde(default)]
    pub confidence: Option<f64>,
    /// Normality tests of the residuals.
    #[serde(default)]
    pub normality: Option<NormalityTests>,
    #[serde(default)]
    pub results: Option<ExtraResults>,
    #[serde(default)]
    pub output: Option<OutputFormat>,
    #[serde(default)]
    pub graph: Option<GraphAnnotation>,
}

impl EditTTestArgs {
    pub fn apply(&self, s: &mut TTestSpec) {
        if let Some(v) = &self.graph_id {
            s.graph_id = v.clone();
        }
        if let Some(v) = &self.data_sets {
            s.data_sets = v.clone();
        }
        if let Some(v) = self.design {
            s.design = v;
        }
        if let Some(v) = &self.match_by {
            s.match_by = (!v.trim().is_empty()).then(|| v.clone());
        }
        if let Some(v) = self.distribution {
            s.distribution = v;
        }
        if let Some(v) = self.welch {
            s.welch = v;
        }
        if let Some(v) = self.one_tailed {
            s.one_tailed = v;
        }
        if let Some(v) = self.difference {
            s.difference = v;
        }
        if let Some(v) = self.confidence {
            s.confidence = v;
        }
        if let Some(v) = self.normality {
            s.normality = v;
        }
        if let Some(v) = self.results {
            s.results = v;
        }
        if let Some(v) = self.output {
            s.output = v;
        }
        if let Some(v) = self.graph {
            s.graph = v;
        }
    }

    /// An edit that sets every field to `s` (used by the parameters dialog).
    pub fn full(analysis: String, s: &TTestSpec) -> Self {
        EditTTestArgs {
            analysis: Some(analysis),
            graph_id: Some(s.graph_id.clone()),
            data_sets: Some(s.data_sets.clone()),
            design: Some(s.design),
            match_by: Some(s.match_by.clone().unwrap_or_default()),
            distribution: Some(s.distribution),
            welch: Some(s.welch),
            one_tailed: Some(s.one_tailed),
            difference: Some(s.difference),
            confidence: Some(s.confidence),
            normality: Some(s.normality),
            results: Some(s.results),
            output: Some(s.output),
            graph: Some(s.graph),
        }
    }
}

/// Changes to an existing one-way ANOVA: only the fields given change.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditAnovaArgs {
    /// Analysis id (e.g. "R2") or name. Default: the result shown in the app.
    #[serde(default)]
    pub analysis: Option<String>,
    #[serde(default)]
    pub graph_id: Option<String>,
    #[serde(default)]
    pub data_sets: Option<Vec<String>>,
    #[serde(default)]
    pub design: Option<Matching>,
    /// "Matched by" column for repeated measures ("" clears it).
    #[serde(default)]
    pub match_by: Option<String>,
    #[serde(default)]
    pub distribution: Option<Distribution>,
    #[serde(default)]
    pub equal_sd: Option<bool>,
    #[serde(default)]
    pub sphericity: Option<bool>,
    #[serde(default)]
    pub comparisons: Option<Comparisons>,
    #[serde(default)]
    pub correction: Option<Correction>,
    #[serde(default)]
    pub alpha: Option<f64>,
    #[serde(default)]
    pub swap: Option<bool>,
    /// Normality tests of the residuals.
    #[serde(default)]
    pub normality: Option<NormalityTests>,
    #[serde(default)]
    pub results: Option<ExtraResults>,
    #[serde(default)]
    pub output: Option<OutputFormat>,
    #[serde(default)]
    pub graph: Option<GraphAnnotation>,
}

impl EditAnovaArgs {
    pub fn apply(&self, s: &mut AnovaSpec) {
        if let Some(v) = &self.graph_id {
            s.graph_id = v.clone();
        }
        if let Some(v) = &self.data_sets {
            s.data_sets = v.clone();
        }
        if let Some(v) = self.design {
            s.design = v;
        }
        if let Some(v) = &self.match_by {
            s.match_by = (!v.trim().is_empty()).then(|| v.clone());
        }
        if let Some(v) = self.distribution {
            s.distribution = v;
        }
        if let Some(v) = self.equal_sd {
            s.equal_sd = v;
        }
        if let Some(v) = self.sphericity {
            s.sphericity = v;
        }
        if let Some(v) = &self.comparisons {
            s.comparisons = v.clone();
        }
        if let Some(v) = self.correction {
            s.correction = v;
        }
        if let Some(v) = self.alpha {
            s.alpha = v;
        }
        if let Some(v) = self.swap {
            s.swap = v;
        }
        if let Some(v) = self.normality {
            s.normality = v;
        }
        if let Some(v) = self.results {
            s.results = v;
        }
        if let Some(v) = self.output {
            s.output = v;
        }
        if let Some(v) = self.graph {
            s.graph = v;
        }
    }

    /// An edit that sets every field to `s` (used by the parameters dialog).
    pub fn full(analysis: String, s: &AnovaSpec) -> Self {
        EditAnovaArgs {
            analysis: Some(analysis),
            graph_id: Some(s.graph_id.clone()),
            data_sets: Some(s.data_sets.clone()),
            design: Some(s.design),
            match_by: Some(s.match_by.clone().unwrap_or_default()),
            distribution: Some(s.distribution),
            equal_sd: Some(s.equal_sd),
            sphericity: Some(s.sphericity),
            comparisons: Some(s.comparisons.clone()),
            correction: Some(s.correction),
            alpha: Some(s.alpha),
            swap: Some(s.swap),
            normality: Some(s.normality),
            results: Some(s.results),
            output: Some(s.output),
            graph: Some(s.graph),
        }
    }
}

/// A curve fit (nonlinear regression) of the data sets of an XY graph.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FitArgs {
    /// Result name (default "<model> fit of <graph>").
    #[serde(default)]
    pub name: Option<String>,
    #[serde(flatten)]
    pub spec: FitSpec,
}

/// Changes to an existing curve fit: only the fields given change.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditFitArgs {
    /// Analysis id (e.g. "R2") or name. Default: the result shown in the app.
    #[serde(default)]
    pub analysis: Option<String>,
    #[serde(default)]
    pub graph_id: Option<String>,
    #[serde(default)]
    pub data_sets: Option<Vec<String>>,
    #[serde(default)]
    pub model: Option<FitModel>,
    /// Constrained parameters (replaces the list; [] removes every constraint).
    #[serde(default)]
    pub fixed: Option<Vec<FixedParam>>,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub output: Option<OutputFormat>,
    #[serde(default)]
    pub graph: Option<CurveDisplay>,
}

impl EditFitArgs {
    pub fn apply(&self, s: &mut FitSpec) {
        if let Some(v) = &self.graph_id {
            s.graph_id = v.clone();
        }
        if let Some(v) = &self.data_sets {
            s.data_sets = v.clone();
        }
        if let Some(v) = self.model {
            if v != s.model && self.fixed.is_none() {
                // Keep constraints the new model still has.
                s.fixed.retain(|p| v.params().contains(&p.name.as_str()));
            }
            s.model = v;
        }
        if let Some(v) = &self.fixed {
            s.fixed = v.clone();
        }
        if let Some(v) = self.confidence {
            s.confidence = v;
        }
        if let Some(v) = self.output {
            s.output = v;
        }
        if let Some(v) = self.graph {
            s.graph = v;
        }
    }

    /// An edit that sets every field to `s` (used by the parameters dialog).
    pub fn full(analysis: String, s: &FitSpec) -> Self {
        EditFitArgs {
            analysis: Some(analysis),
            graph_id: Some(s.graph_id.clone()),
            data_sets: Some(s.data_sets.clone()),
            model: Some(s.model),
            fixed: Some(s.fixed.clone()),
            confidence: Some(s.confidence),
            output: Some(s.output),
            graph: Some(s.graph),
        }
    }
}

/// Linear regression of the data sets of an XY graph.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LinRegArgs {
    /// Result name (default "Linear regression of <graph>").
    #[serde(default)]
    pub name: Option<String>,
    #[serde(flatten)]
    pub spec: LinRegSpec,
}

/// Changes to an existing linear regression: only the fields given change.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditLinRegArgs {
    /// Analysis id (e.g. "R2") or name. Default: the result shown in the app.
    #[serde(default)]
    pub analysis: Option<String>,
    /// Move the analysis to another graph of the same kind (default: leave it where it is).
    #[serde(default)]
    pub graph_id: Option<String>,
    #[serde(default)]
    pub data_sets: Option<Vec<String>>,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub output: Option<OutputFormat>,
    #[serde(default)]
    pub graph: Option<CurveDisplay>,
}

impl EditLinRegArgs {
    pub fn apply(&self, s: &mut LinRegSpec) {
        if let Some(v) = &self.graph_id {
            s.graph_id = v.clone();
        }
        if let Some(v) = &self.data_sets {
            s.data_sets = v.clone();
        }
        if let Some(v) = self.confidence {
            s.confidence = v;
        }
        if let Some(v) = self.output {
            s.output = v;
        }
        if let Some(v) = self.graph {
            s.graph = v;
        }
    }
}

/// Correlation of X and Y in each data set of an XY graph.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CorrelationArgs {
    /// Result name (default "Correlation of <graph>").
    #[serde(default)]
    pub name: Option<String>,
    #[serde(flatten)]
    pub spec: CorrelationSpec,
}

/// Changes to an existing correlation: only the fields given change.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditCorrelationArgs {
    #[serde(default)]
    pub analysis: Option<String>,
    /// Move the analysis to another graph of the same kind (default: leave it where it is).
    #[serde(default)]
    pub graph_id: Option<String>,
    #[serde(default)]
    pub data_sets: Option<Vec<String>>,
    #[serde(default)]
    pub method: Option<CorrMethod>,
    #[serde(default)]
    pub one_tailed: Option<bool>,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub output: Option<OutputFormat>,
}

impl EditCorrelationArgs {
    pub fn apply(&self, s: &mut CorrelationSpec) {
        if let Some(v) = &self.graph_id {
            s.graph_id = v.clone();
        }
        if let Some(v) = &self.data_sets {
            s.data_sets = v.clone();
        }
        if let Some(v) = self.method {
            s.method = v;
        }
        if let Some(v) = self.one_tailed {
            s.one_tailed = v;
        }
        if let Some(v) = self.confidence {
            s.confidence = v;
        }
        if let Some(v) = self.output {
            s.output = v;
        }
    }
}

/// An ordinary two-way ANOVA of a grouped graph (X categories × data sets).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TwoWayArgs {
    /// Result name (default "Two-way ANOVA of <graph>").
    #[serde(default)]
    pub name: Option<String>,
    #[serde(flatten)]
    pub spec: TwoWaySpec,
}

/// Changes to an existing two-way ANOVA: only the fields given change.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditTwoWayArgs {
    /// Analysis id (e.g. "R2") or name. Default: the result shown in the app.
    #[serde(default)]
    pub analysis: Option<String>,
    #[serde(default)]
    pub data_sets: Option<Vec<String>>,
    #[serde(default)]
    pub interaction: Option<bool>,
    #[serde(default)]
    pub compare: Option<TwoWayCompare>,
    #[serde(default)]
    pub correction: Option<Correction>,
    #[serde(default)]
    pub alpha: Option<f64>,
    #[serde(default)]
    pub results: Option<ExtraResults>,
    #[serde(default)]
    pub output: Option<OutputFormat>,
    #[serde(default)]
    pub graph: Option<GraphAnnotation>,
}

impl EditTwoWayArgs {
    pub fn apply(&self, s: &mut TwoWaySpec) {
        if let Some(v) = &self.data_sets {
            s.data_sets = v.clone();
        }
        if let Some(v) = self.interaction {
            s.interaction = v;
        }
        if let Some(v) = self.compare {
            s.compare = v;
        }
        if let Some(v) = self.correction {
            s.correction = v;
        }
        if let Some(v) = self.alpha {
            s.alpha = v;
        }
        if let Some(v) = self.results {
            s.results = v;
        }
        if let Some(v) = self.output {
            s.output = v;
        }
        if let Some(v) = self.graph {
            s.graph = v;
        }
    }
}

/// Area under the curve of the data sets of an XY graph.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AucArgs {
    /// Result name (default "Area under curve of <graph>").
    #[serde(default)]
    pub name: Option<String>,
    #[serde(flatten)]
    pub spec: AucSpec,
}

/// Changes to an existing area under the curve: only the fields given change.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditAucArgs {
    /// Analysis id (e.g. "R2") or name. Default: the result shown in the app.
    #[serde(default)]
    pub analysis: Option<String>,
    /// Move the analysis to another graph of the same kind (default: leave it where it is).
    #[serde(default)]
    pub graph_id: Option<String>,
    #[serde(default)]
    pub data_sets: Option<Vec<String>>,
    #[serde(default)]
    pub baseline: Option<f64>,
    #[serde(default)]
    pub min_peak_percent: Option<f64>,
    #[serde(default)]
    pub peaks_below: Option<bool>,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub output: Option<OutputFormat>,
}

impl EditAucArgs {
    pub fn apply(&self, s: &mut AucSpec) {
        if let Some(v) = &self.graph_id {
            s.graph_id = v.clone();
        }
        if let Some(v) = &self.data_sets {
            s.data_sets = v.clone();
        }
        if let Some(v) = self.baseline {
            s.baseline = v;
        }
        if let Some(v) = self.min_peak_percent {
            s.min_peak_percent = v;
        }
        if let Some(v) = self.peaks_below {
            s.peaks_below = v;
        }
        if let Some(v) = self.confidence {
            s.confidence = v;
        }
        if let Some(v) = self.output {
            s.output = v;
        }
    }
}

/// Kaplan-Meier survival analysis of a survival graph.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SurvivalArgs {
    /// Result name (default "Survival analysis of <graph>").
    #[serde(default)]
    pub name: Option<String>,
    #[serde(flatten)]
    pub spec: SurvivalSpec,
}

/// Changes to an existing survival analysis: only the fields given change.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditSurvivalArgs {
    /// Analysis id (e.g. "R2") or name. Default: the result shown in the app.
    #[serde(default)]
    pub analysis: Option<String>,
    /// Move the analysis to another graph of the same kind (default: leave it where it is).
    #[serde(default)]
    pub graph_id: Option<String>,
    #[serde(default)]
    pub data_sets: Option<Vec<String>>,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub proportions: Option<bool>,
    #[serde(default)]
    pub output: Option<OutputFormat>,
}

impl EditSurvivalArgs {
    pub fn apply(&self, s: &mut SurvivalSpec) {
        if let Some(v) = &self.graph_id {
            s.graph_id = v.clone();
        }
        if let Some(v) = &self.data_sets {
            s.data_sets = v.clone();
        }
        if let Some(v) = self.confidence {
            s.confidence = v;
        }
        if let Some(v) = self.proportions {
            s.proportions = v;
        }
        if let Some(v) = self.output {
            s.output = v;
        }
    }
}

/// Contingency table analysis: chi-square, Fisher's exact test and the rest.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ContingencyArgs {
    /// Result name (default "Contingency analysis of <graph>").
    #[serde(default)]
    pub name: Option<String>,
    #[serde(flatten)]
    pub spec: ContingencySpec,
}

/// Changes to an existing contingency analysis: only the fields given change.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditContingencyArgs {
    /// Analysis id (e.g. "R2") or name. Default: the result shown in the app.
    #[serde(default)]
    pub analysis: Option<String>,
    /// Move the analysis to another graph of the same kind (default: leave it where it is).
    #[serde(default)]
    pub graph_id: Option<String>,
    #[serde(default)]
    pub data_sets: Option<Vec<String>>,
    #[serde(default)]
    pub paired: Option<bool>,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub expected: Option<bool>,
    #[serde(default)]
    pub output: Option<OutputFormat>,
}

impl EditContingencyArgs {
    pub fn apply(&self, s: &mut ContingencySpec) {
        if let Some(v) = &self.graph_id {
            s.graph_id = v.clone();
        }
        if let Some(v) = &self.data_sets {
            s.data_sets = v.clone();
        }
        if let Some(v) = self.paired {
            s.paired = v;
        }
        if let Some(v) = self.confidence {
            s.confidence = v;
        }
        if let Some(v) = self.expected {
            s.expected = v;
        }
        if let Some(v) = self.output {
            s.output = v;
        }
    }
}

/// Which analysis to write as an R script, and where.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportScriptArgs {
    /// Analysis id (e.g. "R2") or name. Default: the result shown in the app.
    #[serde(default)]
    pub analysis: Option<String>,
    /// Save to this .R file (absolute path). Without it the script goes to a temporary file.
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RenameAnalysisArgs {
    #[serde(default)]
    pub analysis: Option<String>,
    pub name: String,
}

// ---------------------------------------------------------------------------
// The command list (name, args, [mutates, read_only])
// ---------------------------------------------------------------------------

commands! {
    /// Start a new, empty project (the current one is discarded unless saved).
    ProjectNew = "project_new" (ProjectNewArgs) [true, false];
    /// Open an Otto project (.otto).
    ProjectOpen = "project_open" (PathArgs) [true, false];
    /// Save the project to its current file.
    ProjectSave = "project_save" (NoArgs) [false, false];
    /// Save the project to a new .otto file.
    ProjectSaveAs = "project_save_as" (PathArgs) [false, false];
    /// Overview of the project: tables (with columns and types), graphs, analyses and what the app shows.
    ProjectSummary = "project_summary" (NoArgs) [false, true];
    /// Undo the last change.
    Undo = "undo" (NoArgs) [false, false];
    /// Redo the last undone change.
    Redo = "redo" (NoArgs) [false, false];
    /// Show a data table, plotted table, graph, result or final graph in the app (like clicking it in the left navigator).
    UiNavigate = "ui_navigate" (NavigateArgs) [false, false];
    /// Set the graph zoom in the app.
    UiSetZoom = "ui_set_zoom" (ZoomArgs) [false, false];
    /// Show the work to the user in the Otto window. Call it when an analysis or plotting job is done. When Otto runs as a background copy (it was not open when Claude started), this saves the project, opens an Otto window with it and continues there, so later changes appear live. In an open Otto window it brings the window to the front.
    UiShowWindow = "ui_show_window" (NoArgs) [false, false];

    /// Create an empty multivariable table.
    TableCreate = "table_create" (TableCreateArgs) [true, false];
    /// Import a CSV/TSV/Excel file as a new multivariable table (first row = column names; types are inferred).
    TableImportFile = "table_import_file" (ImportFileArgs) [true, false];
    /// Create a table from pasted CSV/TSV text (first row = column names; types are inferred).
    TableImportText = "table_import_text" (ImportTextArgs) [true, false];
    /// Look at what importing a file would produce, without importing it: the name and type each column would get, whether it would be included, and the first rows with what each one is (name row / data row / skipped). Use it when a file does not import cleanly - a title line above the table, a header spread over two rows, an encoding or separator Otto guessed wrong - then pass the same options to table_import_file.
    TableImportPreview = "table_import_preview" (ImportPreviewArgs) [false, true];
    /// Describe a table: columns with type, missing values, levels (categorical) or range (numeric).
    TableDescribe = "table_describe" (TableRef) [false, true];
    /// Read table rows as JSON objects or CSV.
    TableGet = "table_get" (TableGetArgs) [false, true];
    /// Write a block of cells starting at (row, column); the table grows as needed.
    TableSetCells = "table_set_cells" (SetCellsArgs) [true, false];
    /// Insert empty rows.
    TableAddRows = "table_add_rows" (AddRowsArgs) [true, false];
    /// Delete rows by 0-based index.
    TableDeleteRows = "table_delete_rows" (DeleteRowsArgs) [true, false];
    /// Add a column (numeric, categorical or text).
    TableAddColumn = "table_add_column" (AddColumnArgs) [true, false];
    /// Delete columns.
    TableDeleteColumns = "table_delete_columns" (DeleteColumnsArgs) [true, false];
    /// Rename a column (graphs and analyses follow the new name).
    TableRenameColumn = "table_rename_column" (RenameColumnArgs) [true, false];
    /// Change a column's type (numeric / categorical / text).
    TableSetColumnType = "table_set_column_type" (SetColumnTypeArgs) [true, false];
    /// Reorder the levels of a categorical column; this sets the order of bars on graphs.
    TableSetCategoryOrder = "table_set_category_order" (CategoryOrderArgs) [true, false];
    /// Rename a level of a categorical column (e.g. "(none)" → "No additive").
    TableRenameLevel = "table_rename_level" (RenameLevelArgs) [true, false];
    /// Rename a table.
    TableRename = "table_rename" (RenameTableArgs) [true, false];
    /// Duplicate a table.
    TableDuplicate = "table_duplicate" (DuplicateTableArgs) [true, false];
    /// Delete a data table together with everything made from it: its quick graphs, the final graphs finalized from it (with their locked copies and statistics) and its results. The app asks the user first.
    TableDelete = "table_delete" (TableRef) [true, false];

    /// Create a graph from a table. Kinds: xy_points, xy_points_line, xy_line (line only) (numeric X); column_bar, box, violin, grouped_interleaved, grouped_separated, stacked_bar, stacked_percent (categorical X; the stacked kinds lay the data sets on top of each other within each X, and stacked_percent scales every bar to 100%); heatmap (columns = categorical X, rows = Group by or several Y columns, cell = mean); pie and donut (one slice per categorical X, sized by the SUM of its Y values, not the mean; no statistics); xy_scatter (numeric X, one dot per row); volcano (X = log2 fold change, Y = P value, one dot per row; see graph_set_volcano); qq (normal QQ plot of each data set: no X column, Y = the values, Group by = data sets); survival (Kaplan-Meier curves: X = numeric time, Y = event column with 1 = event / 0 = censored, one row per subject, Group by = one curve per group).
    GraphCreate = "graph_create" (GraphCreateArgs) [true, false];
    /// Change the graph type.
    GraphSetKind = "graph_set_kind" (GraphKindArgs) [true, false];
    /// Change which columns are X, Y, Group and the replicate (or re-link to another table). An XY graph drawing one line per replicate needs replicate_by: the column that says which subject a value belongs to.
    GraphSetMapping = "graph_set_mapping" (GraphMappingArgs) [true, false];
    /// Set row filters, which pick the rows a graph plots. A categorical or text column takes include, the list of values to keep: [{"column": "Additive", "include": ["(none)", "IL12"]}]. A numeric column takes min and/or max, a range in its own units: [{"column": "Dose", "min": 10, "max": 100}] keeps rows from 10 to 100 inclusive, and a row with no value is left out of any range. Replaces all filters; an empty list plots every row.
    GraphSetFilters = "graph_set_filters" (GraphFiltersArgs) [true, false];
    /// Set the publication format: square (30×30 mm axes), wide (45×30 mm) or ultra_wide (60×30 mm).
    GraphSetFormat = "graph_set_format" (GraphFormatArgs) [true, false];
    /// Set bar/symbol center (mean/median), error bars (sd/sem/ci95/range/none), box-plot whiskers (tukey/min_max/pct10_to_90) and XY replicate display (summary = mean ± error, individual = each replicate). Drawing one line per replicate needs the replicate column: set it here or, better, with graph_set_mapping.
    GraphSetSummary = "graph_set_summary" (GraphSummaryArgs) [true, false];
    /// Set an axis title, fixed range, scale (linear, log10, log2 or ln) and/or how its tick labels are written (plain, grouped, scientific, power10, log_value).
    GraphSetAxis = "graph_set_axis" (GraphAxisArgs) [true, false];
    /// Set the color scheme: npg (Nature), aaas (Science), nejm, lancet, jama, okabe_ito (colorblind safe), grayscale or classic; highlight turns highlight mode on or off. Colors follow each category's place in its column's level order, so a group keeps its color when rows are filtered out.
    GraphSetPalette = "graph_set_palette" (GraphPaletteArgs) [true, false];
    /// Put data sets in focus: the first named takes the palette's red, the second its blue, and the rest follow behind them. In highlight mode every other data set turns gray. Names are data sets as analysis_list_data_sets reports them (the bars of a column graph).
    GraphSetFocus = "graph_set_focus" (GraphFocusArgs) [true, false];
    /// Set the symbol shape of each data set (circle, square, triangle_up, triangle_down, diamond, hexagon), or vary them automatically. Circles everywhere is the default.
    GraphSetSymbols = "graph_set_symbols" (GraphSymbolsArgs) [true, false];
    /// Volcano plot settings: the fold-change and P cutoffs, whether Y is already -log10(P), and which column labels the most significant points.
    GraphSetVolcano = "graph_set_volcano" (GraphVolcanoArgs) [true, false];
    /// Show or hide the legend (the color bar of a heatmap).
    GraphSetLegend = "graph_set_legend" (GraphLegendArgs) [true, false];
    /// Heatmap colors: color scale, z-score normalisation per row or column, values printed in the cells, and how many decimals they and the colour bar show (2 by default).
    GraphSetHeatmap = "graph_set_heatmap" (GraphHeatmapArgs) [true, false];
    /// Rename a graph.
    GraphRename = "graph_rename" (RenameGraphArgs) [true, false];
    /// Duplicate a graph (e.g. to show another Y column with the same settings). A final graph is copied with its locked data and its statistics.
    GraphDuplicate = "graph_duplicate" (DuplicateGraphArgs) [true, false];
    /// Make a final graph from a quick graph: a copy whose data are a locked copy of its table (later edits to the data table don't change it). Statistics belong to final graphs; analysis_create_* on a quick graph does this automatically.
    GraphFinalize = "graph_finalize" (FinalizeArgs) [true, false];
    /// Take a fresh locked copy of the data table into an existing final graph and bring over the quick graph's current type, columns, filters, error bars, axis ranges, color theme, highlight mode and data sets in focus. Its name, format, axis titles and statistics stay; the statistics are recalculated on the new data.
    GraphUpdateFromQuick = "graph_update_from_quick" (GraphRef) [true, false];
    /// Delete a graph. A final graph takes its final table and statistics with it; deleting a quick graph leaves the final graphs made from it and the data table.
    GraphDelete = "graph_delete" (GraphRef) [true, false];
    /// Render the graph and return it as a PNG image, so you can see it.
    GraphRenderPreview = "graph_render_preview" (PreviewArgs) [false, true];
    /// The exact data shown in the graph (filtered subset), as CSV: long (tidy), wide (Prism columns) or summary statistics.
    GraphGetPlotData = "graph_get_plot_data" (PlotDataArgs) [false, true];
    /// Export the graph as a vector (SVG, PDF, EMF) or PNG file at its physical size.
    GraphExportFile = "graph_export_file" (ExportFileArgs) [false, false];
    /// Copy the graph to the clipboard as vectors (EMF + SVG, plus a PNG fallback) for pasting into Office, Illustrator or Inkscape.
    GraphCopyVector = "graph_copy_vector" (GraphRef) [false, false];
    /// Save the plotted data as a CSV file.
    GraphExportData = "graph_export_data" (ExportDataArgs) [false, false];
    /// Copy the plotted data to the clipboard as tab-separated text (pastes into Excel or Prism).
    GraphCopyData = "graph_copy_data" (CopyDataArgs) [false, false];
    /// Get R/ggplot2 code that reproduces the graph; optionally save it as a .txt file.
    GraphExportRCode = "graph_export_r_code" (ExportCodeArgs) [false, false];
    /// Copy the R/ggplot2 code for the graph to the clipboard.
    GraphCopyRCode = "graph_copy_r_code" (GraphRef) [false, false];

    /// Create a descriptive statistics result (n, mean, SD, SEM, median, quartiles, min/max, 95% CI), optionally per group.
    AnalysisCreateDescriptive = "analysis_create_descriptive" (DescriptiveArgs) [true, false];
    /// Read the results of an analysis (all result tables as text).
    AnalysisGetResults = "analysis_get_results" (AnalysisRef) [false, true];
    /// List the data sets of a GRAPH (the argument is a graph id such as "G2", not an analysis id) with their number of values: the bars of a bar graph (what t tests and ANOVA compare) or the data sets of an XY graph (what curve fits, regression and correlation analyse).
    AnalysisListDataSets = "analysis_list_data_sets" (GraphRef) [false, true];
    /// t test between two bars of a graph (Prism's "t tests and nonparametric tests"): unpaired Welch or Student, paired, ratio paired, Mann-Whitney or Wilcoxon. Significance brackets with stars are drawn on the graph.
    AnalysisCreateTTest = "analysis_create_t_test" (TTestArgs) [true, false];
    /// One-way ANOVA across bars of a graph (ordinary, Brown-Forsythe and Welch, repeated measures, Kruskal-Wallis or Friedman) with multiple comparisons (Tukey, Dunnett, Sidak, Dunn, FDR...). Brackets with stars are drawn on the graph.
    AnalysisCreateOneWayAnova = "analysis_create_one_way_anova" (AnovaArgs) [true, false];
    /// Change the parameters of an existing t test (like Prism's analysis parameters button).
    AnalysisEditTTest = "analysis_edit_t_test" (EditTTestArgs) [true, false];
    /// Change the parameters of an existing one-way ANOVA.
    AnalysisEditOneWayAnova = "analysis_edit_one_way_anova" (EditAnovaArgs) [true, false];
    /// Fit a curve (nonlinear regression) to each data set of an XY graph, like Prism: lines, dose-response stimulation / inhibition (1PL, 2PL, 3PL, 4PL, 5PL; log or concentration X), exponential decay / association / growth, saturation binding, Michaelis-Menten, Gaussian. Parameters can be constrained to constants (fixed, e.g. Bottom = 0). Reports best-fit values with SE and confidence intervals (EC50/IC50, half-life...), R squared and Sy.x; the curves are drawn on the graph. On a quick graph this first makes its final graph. Computed by R (minpack.lm).
    AnalysisCreateCurveFit = "analysis_create_curve_fit" (FitArgs) [true, false];
    /// Change an existing curve fit (model, constraints, data sets, confidence, whether the curve is drawn).
    AnalysisEditCurveFit = "analysis_edit_curve_fit" (EditFitArgs) [true, false];
    /// Simple linear regression of each data set of an XY graph: slope and intercept with SE and CI, R squared, Sy.x, test of slope = 0. The line is drawn on the graph. Computed by R (lm).
    AnalysisCreateLinearRegression = "analysis_create_linear_regression" (LinRegArgs) [true, false];
    /// Change an existing linear regression.
    AnalysisEditLinearRegression = "analysis_edit_linear_regression" (EditLinRegArgs) [true, false];
    /// Correlation between X and Y of each data set of an XY graph: Pearson r with CI, or Spearman's rank correlation. Computed by R (cor.test).
    AnalysisCreateCorrelation = "analysis_create_correlation" (CorrelationArgs) [true, false];
    /// Change an existing correlation.
    AnalysisEditCorrelation = "analysis_edit_correlation" (EditCorrelationArgs) [true, false];
    /// Ordinary two-way ANOVA of a grouped graph: its X categories (rows) × its data sets (columns, from Group by or several Y columns), Type III sums of squares, with or without interaction. Follow-up comparisons: within_rows (data sets within each X category, the bars of each cluster), within_columns, row_means / column_means (main effects), all_cells; Tukey / Sidak, FDR or uncorrected. Significant comparisons between bars are drawn as brackets. Computed by R (car::Anova). On a quick graph this first makes its final graph.
    AnalysisCreateTwoWayAnova = "analysis_create_two_way_anova" (TwoWayArgs) [true, false];
    /// Change an existing two-way ANOVA.
    AnalysisEditTwoWayAnova = "analysis_edit_two_way_anova" (EditTwoWayArgs) [true, false];
    /// Area under the curve of each data set of an XY graph (trapezoid rule on the mean Y at each X, from a baseline): total area with SE and CI from the replicates, total peak area and the peaks (start, end, peak X / Y, area). Computed by R.
    AnalysisCreateAuc = "analysis_create_auc" (AucArgs) [true, false];
    /// Change an existing area under the curve.
    AnalysisEditAuc = "analysis_edit_auc" (EditAucArgs) [true, false];
    /// Kaplan-Meier survival analysis of a survival graph (graph kind survival: X = time, Y = event 1 / censored 0, Group by = curves): survival proportions, median survival, log-rank (Mantel-Cox), log-rank test for trend (3+ curves), Gehan-Breslow-Wilcoxon test and hazard ratios (2 curves). Computed by R's survival package.
    AnalysisCreateSurvival = "analysis_create_survival" (SurvivalArgs) [true, false];
    /// Change an existing survival analysis.
    AnalysisEditSurvival = "analysis_edit_survival" (EditSurvivalArgs) [true, false];
    /// Contingency table analysis (graph kind contingency: rows = X, columns = Group by or several Y columns, cells = counts): the chi-square test with and without Yates' correction, Fisher's exact test, McNemar's test for a paired design, the chi-square test for trend, and for a 2 x 2 table the relative risk, odds ratio, difference between proportions and the diagnostic ratios (sensitivity, specificity, predictive values). Computed by R. On a quick graph this first makes its final graph.
    AnalysisCreateContingency = "analysis_create_contingency" (ContingencyArgs) [true, false];
    /// Change an existing contingency analysis.
    AnalysisEditContingency = "analysis_edit_contingency" (EditContingencyArgs) [true, false];
    /// Normality (and lognormality) tests of the data sets of a graph: D'Agostino-Pearson, Anderson-Darling, Shapiro-Wilk and Kolmogorov-Smirnov (Lilliefors), as in Prism. Pair it with a qq graph to see the same thing. Computed by R. On a quick graph this first makes its final graph.
    AnalysisCreateNormality = "analysis_create_normality" (NormalityArgs) [true, false];
    /// Change an existing normality analysis.
    AnalysisEditNormality = "analysis_edit_normality" (EditNormalityArgs) [true, false];
    /// Identify outliers in the data sets of a graph: iterative Grubbs (generalized ESD, the default), a single Grubbs' test, or Tukey's 1.5 x IQR rule. Reports each flagged value with its G statistic and P value; nothing is removed from the data. Computed by R. On a quick graph this first makes its final graph.
    AnalysisCreateOutliers = "analysis_create_outliers" (OutlierArgs) [true, false];
    /// Change an existing outlier analysis.
    AnalysisEditOutliers = "analysis_edit_outliers" (EditOutlierArgs) [true, false];
    /// Write a standalone R script that reproduces an analysis (Otto's exact R functions plus the analysed data) to a file and return its path with the first lines. The script is thousands of lines, so it is never returned in full; read the file if you need more of it.
    AnalysisExportRScript = "analysis_export_r_script" (ExportScriptArgs) [false, true];
    /// The statistics engine: whether R and Otto's R packages are installed (with versions), and the progress of a running setup.
    EngineStatus = "engine_status" (NoArgs) [false, true];
    /// Install the statistics engine's R packages into Otto's own library from a dated CRAN snapshot (R itself must be installed). Runs in the background for a few minutes; engine_status shows the progress.
    EngineSetup = "engine_setup" (NoArgs) [false, false];
    /// Recalculate t tests / ANOVAs that are out of date because their graph changed (X, Y, Group by or rows), using the bars the graph shows now: ANOVA compares every bar, a t test the two bars shown (or its old pair if still there). Other parameters are kept. To choose data sets or tests differently, use analysis_edit_* with data_sets instead.
    AnalysisRecalculate = "analysis_recalculate" (RecalculateArgs) [true, false];
    /// Rename an analysis.
    AnalysisRename = "analysis_rename" (RenameAnalysisArgs) [true, false];
    /// Delete an analysis (a statistics or descriptive result); its graph stays.
    AnalysisDelete = "analysis_delete" (AnalysisRef) [true, false];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_is_consistent() {
        let reg = registry();
        let mut names: Vec<_> = reg.iter().map(|m| m.name).collect();
        names.sort();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "duplicate tool names");
        for m in &reg {
            let schema = (m.schema)();
            assert_eq!(schema["type"], "object", "{} schema must be an object", m.name);
            assert!(!m.description.trim().is_empty(), "{} needs a description", m.name);
            assert!(m.name.chars().all(|c| c.is_ascii_lowercase() || c == '_'), "{}", m.name);
            assert!(!(m.mutates && m.read_only));
        }
    }

    #[test]
    fn parse_roundtrip() {
        let cmd = Command::parse("graph_create", serde_json::json!({"kind": "column_bar", "x": "Condition", "y": ["IL2"]})).unwrap();
        assert_eq!(cmd.name(), "graph_create");
        let again = Command::parse(cmd.name(), cmd.args_json()).unwrap();
        assert_eq!(cmd, again);
        assert!(Command::parse("project_summary", serde_json::Value::Null).is_ok());
        assert!(matches!(Command::parse("nope", serde_json::json!({})), Err(EngineError::UnknownTool(_))));
        let bad = Command::parse("graph_set_format", serde_json::json!({"format": "huge"})).unwrap_err();
        assert!(bad.to_string().contains("graph_set_format"));
    }
}
