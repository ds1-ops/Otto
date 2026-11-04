use std::sync::Arc;

use base64::Engine as _;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation, ListToolsResult, PaginatedRequestParams,
    ServerCapabilities, ServerConfig, Tool, ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler};

use otto_engine::{registry, EngineError, Outcome};

use crate::backend::Backend;

pub const INSTRUCTIONS: &str = "\
Otto is a GraphPad Prism-style plotting app. Data lives in multivariable tables (one row per \
sample, columns for factors and measurements); graphs pick X, Y and Group columns from a table.

The app is organised as a workflow: Data Tables -> Graphs (quick, each with its plotted table) -> Final Graphs (each with its locked table and statistics).

Typical workflow:
1. table_import_file (CSV/Excel) or table_import_text, then project_summary / table_describe to see columns and types. \
table_set_cells with header: true pastes a block whose first row names the columns. If a file does not come in \
cleanly - a title line above the table, a header spread over two rows, a separator or encoding guessed wrong - \
table_import_preview shows what a given set of options would produce, and table_import_file takes the same options.
2. graph_create with a kind: xy_points, xy_points_line, xy_line (line only), xy_scatter (numeric X), column_bar, heatmap (columns = X categories, rows = Group by or several Y, colored by the mean; graph_set_heatmap for colors and z-scores), \
grouped_interleaved, grouped_separated, stacked_bar and stacked_percent (the data sets stacked within each X rather than side by side; stacked_percent scales every bar to 100%), pie and donut (one slice per X category, sized by the sum of its Y values rather than the mean), contingency (a table of counts: rows = X, columns = Group by or several Y columns, each cell the sum of its Y values) (categorical X). x and group take one column or a list: several columns combine into one bar / data set per value combination, labelled 'A | B' (e.g. x: [\"Construct\", \"Additive\", \"Well\"]). Several Y columns become data sets. \
These are quick graphs for looking through the data; they never show statistics. graph_get_plot_data with layout wide is a graph's plotted table.
3. graph_render_preview to look at the result, then stop. Otto's defaults are already publication format: \
square 30 x 30 mm axes, 1 pt lines, Arial Bold 7 pt, the Nature color theme, circles, and a legend when there are \
several data sets. Getting the data right - the kind, and which columns are X, Y and Group by - is the job; a graph \
that plots the right numbers is finished unless the user asks for more. Do not walk through graph_set_format, \
graph_set_axis, graph_set_palette, graph_set_symbols, graph_set_legend and graph_set_focus to tidy up: each is a \
round trip and a redraw, and the user can do all of it in the panel. Change a setting when the user asks for it, or \
when the data needs it: graph_set_summary for error bars or for showing each replicate - one line per replicate also \
needs graph_set_mapping's replicate_by, the column that says which subject a value belongs to (e.g. Mouse), \
without which there is no graph at all: the place of a value among the ones at its X says nothing about whose \
it is, so one subject missing a timepoint would move every later one up and the lines would run from one \
subject into another - graph_set_axis for a log scale, graph_set_filters for the rows to plot \
(the values to keep for a categorical column, min and/or max for a numeric one), graph_set_heatmap, graph_set_volcano. \
graph_set_palette picks the journal color theme (npg, aaas, nejm, lancet, jama, okabe_ito, grayscale, classic) and \
switches highlight mode on; graph_set_focus names the data sets in focus: the first takes the theme's red, the second \
its blue, and in highlight mode every other data set is gray.
4. Statistics belong to final graphs. analysis_create_t_test / analysis_create_one_way_anova on a quick graph \
first makes its final graph (graph_finalize does this without statistics): a copy whose data are a locked copy of the table, so \
later data edits don't change it (its values can't be edited; labels and bar order can). Further analyses go on that final graph's id. \
analysis_list_data_sets lists the bars; analysis_create_t_test compares two bars, analysis_create_one_way_anova several (with Tukey / Dunnett / \
Sidak / Dunn / FDR comparisons). Paired or repeated measures designs need match_by (the subject column). \
Significant comparisons are drawn on the graph as brackets with stars (above the graph, which itself stays \
unchanged); analysis_get_results returns the Prism-style results; analysis_edit_* changes only the parameters you pass \
(graph: {show, show_ns} chooses whether an analysis is drawn; a graph draws one analysis at a time, so showing one takes the previous one off). A final graph's type, X / Y / Group by, rows, error bars and \
axis ranges are fixed (set them on the quick graph first); only graph_set_format, axis titles (graph_set_axis with a title), \
graph_set_palette, graph_set_focus, graph_set_legend, tick label format (graph_set_axis with numbers: plain / grouped / scientific / power10 / log_value) and its statistics change. If an analysis is ever reported out of date, analysis_recalculate reruns it on the bars shown.
   XY graphs: analysis_create_curve_fit fits a model to each data set (model: line, poly2, poly3; dose-response \
agonist_1pl … agonist_5pl and agonist_conc, inhibitor_1pl … inhibitor_5pl and inhibitor_conc (the log models expect X = log10 concentration); \
one_phase_decay, plateau_decay, two_phase_decay, one_phase_association, exponential_growth; one_site_specific, one_site_total, specific_hill, \
michaelis_menten, substrate_inhibition; gaussian), with fixed: [{name, value}] to constrain parameters (e.g. Bottom = 0). It reports best-fit \
values with SE and CI (EC50/IC50, half-life…), R squared and Sy.x, and draws the curves. analysis_create_linear_regression and \
analysis_create_correlation and analysis_create_auc (area under the curve, with baseline and peaks) work the same way. On a quick graph they first make its final graph.
   Contingency tables: analysis_create_contingency on a contingency graph runs the chi-square test (with and without Yates' correction), Fisher's exact test, McNemar's test (paired: true) and the chi-square test for trend; a 2 x 2 table also gets the relative risk, odds ratio, difference between fractions and the diagnostic ratios. Tick two columns with data_sets to pull a 2 x 2 out of a wider table.
   Grouped graphs: analysis_create_two_way_anova (X categories × data sets, Type III, with compare: within_rows / within_columns / row_means / column_means / all_cells). Survival: graph_create kind survival (X = time, Y = event column 1 = event / 0 = censored, one row per subject, Group by = curves), then analysis_create_survival (Kaplan-Meier: medians, log-rank, trend, Gehan-Breslow-Wilcoxon, hazard ratios).
   Every test and fit is computed by R (results say which R and package versions); analysis_export_r_script gives a standalone R script \
that reproduces an analysis. If R is not set up, analyses fail with a message saying so: engine_status shows what is missing and engine_setup \
installs Otto's R packages (R itself must be installed by the user).
Two conventions across the tools: a parameter you leave out is left as it is, while an empty string or empty list clears it (so match_by: \"\" unpairs an analysis, and graph_set_axis with title: \"\" goes back to the column name). And a column is named by a string; a bare number is a 0-based column index, so pass \"0\" for a column actually called 0.
5. graph_export_file (svg/pdf/emf/png), graph_copy_vector, graph_export_data / graph_copy_data \
(the plotted subset; layout wide is Prism's layout with replicate subcolumns side by side) and graph_export_r_code.

Graphs use fixed publication formats (slim 15×30, square 30×30, wide 45×30 or ultra-wide 60×30 mm axes, 1 pt lines, \
7 pt Arial Bold). Bar order follows the category order (table_set_category_order). \
Tables/graphs default to the one shown in the app, so ids are optional when there is no ambiguity. \
Every change can be undone with undo.
When a job (analysis, graphs) is done, call ui_show_window so the user sees it: if Otto was not open, it opens the window \
with the project (saved under Documents\\Otto unless it already has a file) and later changes appear there live.";

/// The tool list, generated from the engine's command registry.
pub fn tools() -> Vec<Tool> {
    registry()
        .into_iter()
        .map(|m| {
            let schema = match (m.schema)() {
                serde_json::Value::Object(o) => o,
                _ => serde_json::Map::new(),
            };
            let destructive = m.name.contains("delete") || matches!(m.name, "project_new" | "project_open");
            let mut tool = Tool::new(m.name, m.description.trim(), Arc::new(schema));
            tool.annotations = Some(ToolAnnotations::new().read_only(m.read_only).destructive(destructive));
            tool
        })
        .collect()
}

/// Converts an engine result into MCP content: a summary line, structured data,
/// text payloads (CSV, R code, SVG) and rendered images.
pub fn outcome_to_result(result: Result<Outcome, EngineError>) -> CallToolResult {
    match result {
        Ok(out) => {
            let mut content = vec![ContentBlock::text(out.message)];
            // The setting was applied but the graph cannot be drawn as it now stands. Said
            // as its own block, so it cannot be mistaken for part of the summary line.
            if let Some(reason) = &out.needs_attention {
                content.push(ContentBlock::text(format!(
                    "The change was applied, but this graph cannot be drawn yet: {reason}. \
                     Fix the mapping and call the same tool again; nothing has been rolled back."
                )));
            }
            if let Some(data) = out.data {
                content.push(ContentBlock::text(serde_json::to_string_pretty(&data).unwrap_or_default()));
            }
            if let Some(text) = out.text {
                content.push(ContentBlock::text(text));
            }
            if let Some(png) = out.image_png {
                content.push(ContentBlock::image(base64::engine::general_purpose::STANDARD.encode(png), "image/png"));
            }
            CallToolResult::success(content)
        }
        Err(e) => CallToolResult::error(vec![ContentBlock::text(e.to_string())]),
    }
}

/// MCP server over any backend.
#[derive(Clone)]
pub struct OttoServer<B: Backend> {
    backend: Arc<B>,
    tools: Arc<Vec<Tool>>,
}

impl<B: Backend> OttoServer<B> {
    pub fn new(backend: B) -> Self {
        OttoServer { backend: Arc::new(backend), tools: Arc::new(tools()) }
    }
}

impl<B: Backend> ServerHandler for OttoServer<B> {
    fn get_info(&self) -> ServerConfig {
        let mut info = Implementation::new("otto", env!("CARGO_PKG_VERSION"));
        info.title = Some("Otto".into());
        info.description = Some("Prism-style scientific graphs from multivariable tables".into());
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(info)
            .with_instructions(INSTRUCTIONS)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(self.tools.as_ref().clone()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        if !self.tools.iter().any(|t| t.name == request.name) {
            return Err(McpError::invalid_params(format!("unknown tool '{}'", request.name), None));
        }
        let args = request.arguments.map(serde_json::Value::Object).unwrap_or(serde_json::Value::Null);
        Ok(self.backend.call(request.name.to_string(), args).await.into())
    }
}
