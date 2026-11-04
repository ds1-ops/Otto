//! Prism-style analysis dialogs: "Analyze Data" (method + data sets), the parameter
//! dialogs for t tests and one-way ANOVA with Prism's tabs, the curve-fit popup (model
//! families with 1PL…5PL variants, constraints) and the statistics engine dialog.

use iced::widget::{button, checkbox, column, container, mouse_area, opaque, pick_list, radio, row, scrollable, space, svg, text, text_input, Column};
use iced::{Alignment, Color, Fill, Length};

use otto_core::analysis::{
    AnovaSpec, AucSpec, Comparisons, CorrMethod, CorrelationSpec, Correction, DiffDirection, Distribution, FitFamily, FitModel, FitSpec, FixedParam,
    ContingencySpec, LinRegSpec, Matching, NormalitySpec, NormalityTests, OutlierMethod, OutlierSpec, PStyle, Pairing, SurvivalSpec, TTestSpec,
    TwoWayCompare,
    TwoWaySpec,
};
use otto_core::import::{Decimal, Encoding, ImportOptions, ImportPreview, RowRole, Separator};
use otto_core::{AnalysisKind, ColumnKind, ColumnType, DataShape};
use otto_engine::{
    AnovaArgs, AucArgs, Command, CorrelationArgs, DescriptiveArgs, EditAnovaArgs, EditAucArgs, EditCorrelationArgs, EditFitArgs, EditLinRegArgs,
    ContingencyArgs, EditContingencyArgs, EditNormalityArgs, EditOutlierArgs, EditSurvivalArgs, EditTTestArgs, EditTwoWayArgs, FitArgs, LinRegArgs,
    NormalityArgs, OutlierArgs,
    ImportFileArgs, RecalculateArgs, SurvivalArgs, TTestArgs, TwoWayArgs, View,
};

use crate::app::{App, Message, Pending};
use crate::icons::{icon, Icon};
use crate::style;
use crate::view::{caption, elide, El};

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    TTest,
    OneWayAnova,
    Descriptive,
    CurveFit,
    LinearRegression,
    Correlation,
    TwoWayAnova,
    Auc,
    Survival,
    Normality,
    Outliers,
    Contingency,
}

impl Method {
    fn label(self) -> &'static str {
        match self {
            Method::TTest => "t tests (and nonparametric tests)",
            Method::OneWayAnova => "One-way ANOVA (and nonparametric or mixed)",
            Method::Descriptive => "Descriptive statistics",
            Method::CurveFit => "Nonlinear regression (curve fit)",
            Method::LinearRegression => "Simple linear regression",
            Method::Correlation => "Correlation",
            Method::TwoWayAnova => "Two-way ANOVA",
            Method::Auc => "Area under curve",
            Method::Survival => "Survival analysis (Kaplan-Meier)",
            Method::Normality => "Normality and lognormality tests",
            Method::Outliers => "Identify outliers",
            Method::Contingency => "Contingency table (chi-square, Fisher's exact)",
        }
    }

    /// The graphs the method analyses (descriptive statistics take any graph's table).
    fn shape(self) -> DataShape {
        match self {
            Method::CurveFit | Method::LinearRegression | Method::Correlation | Method::Auc => DataShape::Xy,
            Method::Survival => DataShape::Survival,
            Method::Contingency => DataShape::Counts,
            _ => DataShape::Bars,
        }
    }

    /// Analyses of the data sets of an XY graph.
    fn xy(self) -> bool {
        self.shape() == DataShape::Xy
    }
}

#[derive(Clone, Debug)]
pub struct AnalyzeState {
    pub search: String,
    pub method: Method,
    pub graph: Option<String>,
    /// Ticked data sets (bar labels).
    pub checked: Vec<String>,
    pub error: Option<String>,
    /// Setting up an existing (out-of-date) analysis again.
    pub replacing: Option<String>,
}

/// Statistics that a graph change has made out of date.
#[derive(Clone, Debug)]
pub struct StaleState {
    pub graph: String,
    pub ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Model,
    Constraints,
    Design,
    RepeatedMeasures,
    Comparisons,
    Options,
    Residuals,
}

#[derive(Clone, Debug)]
pub enum Params {
    TTest(TTestSpec),
    Anova(AnovaSpec),
    Fit(FitSpec),
    LinReg(LinRegSpec),
    Corr(CorrelationSpec),
    TwoWay(TwoWaySpec),
    Auc(AucSpec),
    Survival(SurvivalSpec),
    Normality(NormalitySpec),
    Outliers(OutlierSpec),
    Contingency(ContingencySpec),
}

#[derive(Clone, Debug)]
pub struct ParamsState {
    /// Analysis id when changing an existing analysis.
    pub editing: Option<String>,
    pub tab: Tab,
    pub params: Params,
    pub pairs_open: bool,
    pub error: Option<String>,
    /// An analysis of another kind that this new one replaces (deleted on OK).
    pub replaces: Option<String>,
    /// Curve fit: the model family shown.
    pub family: FitFamily,
    /// Curve fit: constrained parameters and the value typed for each.
    pub fixed_text: Vec<(String, String)>,
    /// Area under curve: the baseline and minimum peak height as typed.
    pub baseline_text: String,
    pub min_peak_text: String,
}

impl Params {
    /// The graph whose data sets these parameters work on.
    pub fn graph_id(&self) -> &str {
        match self {
            Params::TTest(s) => &s.graph_id,
            Params::Anova(s) => &s.graph_id,
            Params::Fit(s) => &s.graph_id,
            Params::LinReg(s) => &s.graph_id,
            Params::Corr(s) => &s.graph_id,
            Params::TwoWay(s) => &s.graph_id,
            Params::Auc(s) => &s.graph_id,
            Params::Survival(s) => &s.graph_id,
            Params::Contingency(s) => &s.graph_id,
            Params::Normality(s) => &s.graph_id,
            Params::Outliers(s) => &s.graph_id,
        }
    }
}

impl Dialog {
    /// Graphs this dialog lists data sets for, so they can be worked out once per change
    /// rather than once per frame.
    pub fn graphs(&self) -> Vec<String> {
        match self {
            Dialog::Analyze(a) => a.graph.iter().cloned().collect(),
            Dialog::Params(p) => vec![p.params.graph_id().to_string()],
            Dialog::Stale(_) | Dialog::ConfirmDeleteTable(_) | Dialog::ConfirmDiscard(_) | Dialog::Engine(_) | Dialog::Import(_) => Vec::new(),
        }
    }
}

impl ParamsState {
    pub fn new(editing: Option<String>, params: Params, replaces: Option<String>) -> Self {
        let (tab, family, fixed_text) = match &params {
            Params::Fit(f) => (Tab::Model, f.model.family(), f.fixed.iter().map(|p| (p.name.clone(), otto_core::table::format_number(p.value))).collect()),
            _ => (Tab::Design, FitFamily::Stimulation, Vec::new()),
        };
        let (baseline_text, min_peak_text) = match &params {
            Params::Auc(a) => (otto_core::table::format_number(a.baseline), otto_core::table::format_number(a.min_peak_percent)),
            _ => ("0".into(), "10".into()),
        };
        ParamsState { editing, tab, params, pairs_open: false, error: None, replaces, family, fixed_text, baseline_text, min_peak_text }
    }
}

/// One labelled pick-list of the import wizard's settings.
fn setting<'a, T: Clone + PartialEq + 'static>(
    label: &'a str,
    options: impl Iterator<Item = (T, String)>,
    selected: T,
    on: impl Fn(T) -> DialogMsg + 'static,
) -> El<'a> {
    let options: Vec<Choice<T>> = options.map(|(value, label)| Choice { value, label }).collect();
    let current = options.iter().find(|c| c.value == selected).cloned();
    column![
        caption(label).size(12),
        pick_list(options, current, move |c: Choice<T>| Message::Dialog(on(c.value))).text_size(13).width(Fill)
    ]
    .spacing(3)
    .into()
}

/// The import wizard: how to read the file on the left, and what it would produce
/// underneath, so a title line, a two-row header or a wrong separator can be sorted out
/// before anything is imported.
fn import_view(s: &ImportState) -> El<'_> {
    let text_file = s.sheets.is_empty();
    let mut settings = Column::new().spacing(10);
    if text_file {
        settings = settings
            .push(setting("Separator", Separator::ALL.iter().map(|&v| (v, v.label())), s.options.separator, DialogMsg::ImportSeparator))
            .push(setting(
                "Encoding",
                Encoding::ALL.iter().map(|&v| (v, v.label().to_string())),
                s.options.encoding,
                DialogMsg::ImportEncoding,
            ));
    } else {
        let current = s.sheet.clone().unwrap_or_default();
        settings = settings.push(setting("Sheet", s.sheets.iter().map(|n| (n.clone(), n.clone())), current, DialogMsg::ImportSheet));
    }
    settings = settings.push(setting(
        "Decimal separator",
        Decimal::ALL.iter().map(|&v| (v, v.label().to_string())),
        s.options.decimal,
        DialogMsg::ImportDecimal,
    ));

    let p = &s.preview;
    let summary = match &s.error {
        Some(_) => String::new(),
        None => format!("{} columns of {} imported, {} data rows", p.include.iter().filter(|&&on| on).count(), p.names.len(), p.data_rows),
    };

    // Every column is one fixed-width strip: the name it would get, the type, whether to
    // import it, then its cells. Rows are labelled on the left with what each one is.
    // The settings at the top are spaced out; the data underneath is a real table, cells
    // edge to edge and alternating rows, so it reads the way the imported table will.
    const CELL: f32 = 132.0;
    const ROLE: f32 = 112.0;
    const ROW_H: f32 = 26.0;
    const HEAD_H: f32 = 28.0;
    const TYPE_H: f32 = 30.0;
    const PICK_H: f32 = 24.0;
    let kinds: Vec<Choice<Option<ColumnType>>> = std::iter::once(Choice { value: None, label: "Detect".into() })
        .chain(ColumnType::ALL.iter().map(|&k| Choice { value: Some(k), label: k.label().into() }))
        .collect();

    let roles = Column::with_children(p.rows.iter().enumerate().map(|(i, (role, _))| {
        let options: Vec<Choice<RowRole>> = RowRole::ALL.iter().map(|&r| Choice { value: r, label: r.label().into() }).collect();
        let current = Choice { value: *role, label: role.label().into() };
        container(
            pick_list(options, Some(current), move |c: Choice<RowRole>| Message::Dialog(DialogMsg::ImportRole(i, c.value)))
                .text_size(11.5)
                .padding([2, 6])
                .width(ROLE),
        )
        .height(ROW_H)
        .align_y(Alignment::Center)
        .padding([0, 2])
        .style(style::row_header)
        .into()
    }));
    let head_label = |s: &'static str, h: f32| -> El<'static> {
        container(caption(s).size(11)).height(h).width(ROLE).padding([0, 4]).align_y(Alignment::Center).into()
    };
    let labels = column![
        column![head_label("Column", HEAD_H), head_label("Type", TYPE_H), head_label("Import", PICK_H)].spacing(2),
        roles,
    ]
    .spacing(6)
    .width(ROLE);

    let no_wrap = iced::widget::text::Wrapping::None;
    let columns = row(p.names.iter().enumerate().map(|(c, name)| {
        let included = p.include[c];
        let chosen = s.options.column(c).kind;
        let current = Choice { value: chosen, label: chosen.map_or("Detect".to_string(), |k| k.label().to_string()) };
        let cells = Column::with_children(p.rows.iter().enumerate().map(|(i, (role, row_cells))| {
            let value = row_cells.get(c).cloned().unwrap_or_default();
            let skipped = *role == RowRole::Skip;
            let faded = !included || skipped;
            let is_name = *role == RowRole::Name;
            container(text(value).size(11.5).wrapping(no_wrap).color(if faded { style::MUTED } else { style::TEXT }))
                .padding([4, 6])
                .width(CELL)
                .height(ROW_H)
                .align_y(Alignment::Center)
                .clip(true)
                .style(move |t: &iced::Theme| if is_name { style::header_cell(false)(t) } else { style::cell(false, i % 2 == 1)(t) })
                .into()
        }));
        column![
            column![
                container(text(elide(name, 20)).size(12).wrapping(no_wrap))
                    .padding([4, 6])
                    .width(CELL)
                    .height(HEAD_H)
                    .align_y(Alignment::Center)
                    .clip(true)
                    .style(style::header_cell(false)),
                container(
                    pick_list(kinds.clone(), Some(current), move |k: Choice<Option<ColumnType>>| Message::Dialog(DialogMsg::ImportColumnKind(
                        c, k.value
                    )))
                    .text_size(11.5)
                    .padding([2, 6])
                    .width(CELL)
                )
                .height(TYPE_H),
                container(checkbox(included).size(14).on_toggle(move |on| Message::Dialog(DialogMsg::ImportColumnInclude(c, on))))
                    .height(PICK_H)
                    .width(CELL)
                    .padding([0, 6]),
            ]
            .spacing(2),
            cells,
        ]
        .spacing(6)
        .into()
    }))
    .spacing(0);

    let grid: El = match &s.error {
        Some(e) => yellow_box(column![text(e.clone()).size(13)].spacing(6).into()),
        None if p.rows.is_empty() => caption("Nothing to show: the file has no rows.").into(),
        None => scrollable(row![labels, columns].spacing(4))
            .direction(scrollable::Direction::Both { vertical: Default::default(), horizontal: Default::default() })
            .height(Length::Fixed(340.0))
            .into(),
    };

    let note: El = if p.total_rows > p.rows.len() {
        caption(format!("Showing the first {} of {} rows in the file.", p.rows.len(), p.total_rows)).size(11.5).into()
    } else {
        space::horizontal().into()
    };

    let body = column![
        row![
            container(settings).width(Length::Fixed(240.0)),
            column![
                desc(
                    "Say what each row is on the left of the preview. Several name rows join with \" | \", so a header \
                     spread over two rows becomes one name per column, and a title line above the table can be skipped."
                ),
                caption(summary).size(12),
            ]
            .spacing(6)
            .width(Fill),
        ]
        .spacing(16),
        grid,
        note,
        footer(Message::Dialog(DialogMsg::ImportOk), None),
    ]
    .spacing(12);
    let title = s.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Import".into());
    frame(format!("Import {title}"), body.into(), 900.0)
}

/// The statistics engine dialog: R, Otto's packages and their setup.
#[derive(Clone, Debug)]
pub struct EngineState {
    pub status: otto_r::Status,
    /// R path typed into the override field.
    pub path: String,
    /// The status was refreshed after the last setup finished.
    pub refreshed: bool,
}

/// The import wizard: the file as it reads right now, and the settings it reads with.
/// The grid is re-read when the separator, encoding or sheet changes; the preview is
/// rebuilt from the grid whenever any setting changes, so nothing is worked out in `view`.
#[derive(Clone, Debug)]
pub struct ImportState {
    pub path: std::path::PathBuf,
    /// Sheets of a workbook, empty for a text file.
    pub sheets: Vec<String>,
    pub sheet: Option<String>,
    pub options: ImportOptions,
    grid: Vec<RawCell2>,
    pub preview: ImportPreview,
    /// Whether the file reads with a decimal comma, as settled when it was read.
    pub decimal_comma: bool,
    pub error: Option<String>,
}

/// One row of the raw grid (an alias, so the field above stays readable).
type RawCell2 = Vec<otto_core::import::RawCell>;

impl ImportState {
    /// Reads the file under the current settings and rebuilds the preview.
    pub fn reread(&mut self) {
        match otto_core::import::read_file(&self.path, self.sheet.as_deref(), &self.options) {
            Ok((grid, _, fmt)) => {
                self.grid = grid;
                self.decimal_comma = fmt.decimal_comma;
                self.error = None;
            }
            Err(e) => {
                self.grid = Vec::new();
                self.error = Some(e.to_string());
            }
        }
        self.repreview();
    }

    /// Rebuilds the preview from the grid already read.
    pub fn repreview(&mut self) {
        self.preview = otto_core::import::preview(&self.grid, &self.options, self.decimal_comma);
    }

    pub fn new(path: std::path::PathBuf) -> Self {
        let sheets = otto_core::import::sheet_names(&path).unwrap_or_default();
        let mut st = ImportState {
            sheet: sheets.first().cloned(),
            sheets,
            path,
            options: ImportOptions::default(),
            grid: Vec::new(),
            preview: otto_core::import::preview(&[], &ImportOptions::default(), false),
            decimal_comma: false,
            error: None,
        };
        st.reread();
        st
    }
}

#[derive(Clone, Debug)]
pub enum Dialog {
    Analyze(AnalyzeState),
    Params(ParamsState),
    Stale(StaleState),
    /// "Delete this data table and everything made from it?"
    ConfirmDeleteTable(String),
    /// "Save changes before starting a new project / opening another one?"
    ConfirmDiscard(Pending),
    Engine(EngineState),
    Import(ImportState),
}

/// Follow-up test choice (radio values need Copy + Eq).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompMode {
    None,
    AllPairs,
    VsControl,
    Selected,
}

#[derive(Clone, Debug)]
pub enum Change {
    Paired(bool),
    MatchBy(String),
    Dist(Distribution),
    Welch(bool),
    OneTailed(bool),
    Difference(DiffDirection),
    Confidence(u8),
    Descriptive(bool),
    Aicc(bool),
    Effect(bool),
    Digits(u8),
    PStyle(PStyle),
    ShowOnGraph(bool),
    ShowNs(bool),
    Matched(bool),
    EqualSd(bool),
    Sphericity(bool),
    Mode(CompMode),
    Control(String),
    TogglePair(String, String, bool),
    PairsOpen(bool),
    Correction(Correction),
    Alpha(u16),
    Swap(bool),
    /// A normality test of the residuals, by R name.
    Normality(&'static str, bool),
    Family(FitFamily),
    Model(FitModel),
    /// Constrain a parameter to a constant (or not).
    Fix(String, bool),
    FixValue(String, String),
    ShowCurve(bool),
    CorrMethod(CorrMethod),
    Interaction(bool),
    TwoWayCompare(TwoWayCompare),
    Baseline(String),
    MinPeak(String),
    PeaksBelow(bool),
    Proportions(bool),
    Expected(bool),
    Lognormal(bool),
    OutlierMethod(OutlierMethod),
    MaxOutliers(usize),
}

#[derive(Clone, Debug)]
pub enum DialogMsg {
    OpenAnalyze,
    Search(String),
    PickMethod(Method),
    PickGraph(String),
    Toggle(String, bool),
    All(bool),
    AnalyzeOk,
    EditAnalysis(String),
    Tab(Tab),
    Change(Change),
    ParamsOk,
    Cancel,
    /// Recalculate one out-of-date analysis (or, with `None`, all of the graph in the warning).
    Recalculate(Option<String>),
    /// Go through Analyze → parameters again for an out-of-date analysis.
    SetUpAgain(String),
    /// Undo the change that made statistics out of date.
    UndoChange,
    /// Answers to "save changes before …?": save the project first, or give the changes up.
    SaveThenDiscard(Pending),
    Discard(Pending),
    /// Confirmed: delete the data table and everything made from it.
    DeleteTable(String),
    /// Ticks a data set of an XY analysis.
    FitSet(String, bool),
    OpenEngine,
    EngineSetup,
    EnginePath(String),
    /// Saves the typed R path (empty: find R automatically).
    EngineSavePath,
    /// While a setup runs: refreshes the log, and the status when it finishes.
    EngineTick,
    /// Import wizard: a setting that changes how the file is read.
    ImportSeparator(Separator),
    ImportEncoding(Encoding),
    ImportDecimal(Decimal),
    ImportSheet(String),
    /// Import wizard: what row `i` of the file is.
    ImportRole(usize, RowRole),
    /// Import wizard: the type of column `i` (`None` infers it).
    ImportColumnKind(usize, Option<ColumnType>),
    /// Import wizard: whether column `i` is imported.
    ImportColumnInclude(usize, bool),
    ImportOk,
}

// ---------------------------------------------------------------------------
// Update
// ---------------------------------------------------------------------------

/// Graphs an analysis of this shape can use: (id, label). Statistics on a quick graph
/// make its final graph, so final graphs come first and quick ones say what OK will do.
fn analysis_graphs(app: &App, shape: DataShape) -> Vec<(String, String)> {
    let gs = app.engine.project.graphs.iter().filter(|g| g.kind.shape() == shape);
    let finals = gs.clone().filter(|g| g.is_final()).map(|g| (g.id.clone(), format!("{} (final)", g.name.trim_end_matches(" (final)"))));
    let quick = gs.filter(|g| !g.is_final()).map(|g| (g.id.clone(), format!("{} → new final graph", g.name)));
    finals.chain(quick).collect()
}

/// Data sets of a graph, as the dialog lists them. Message handlers read them straight
/// from the project; view code must use [], which is cached.
/// Data sets of a graph, straight from the project. Only for message handlers: view code
/// must go through `App::data_sets`, which is cached, because this distills the table.
fn data_set_labels(app: &App, graph: &Option<String>) -> Vec<(String, usize)> {
    graph
        .as_deref()
        .and_then(|g| otto_engine::column_tests::list_data_sets(&app.engine.project, g).ok())
        .unwrap_or_default()
}

/// "A", "B", … "Z", "AA" — Prism-style data set letters.
fn letter(i: usize) -> String {
    let mut n = i;
    let mut s = String::new();
    loop {
        s.insert(0, (b'A' + (n % 26) as u8) as char);
        if n < 26 {
            break;
        }
        n = n / 26 - 1;
    }
    s
}

impl App {
    /// Changes one import setting. `reread` is for the settings that decide how the bytes
    /// are read at all (separator, encoding, decimal mark); the rest only re-preview.
    fn import_setting(&mut self, f: impl FnOnce(&mut ImportOptions), reread: bool) {
        let Some(Dialog::Import(s)) = &mut self.dialog else { return };
        f(&mut s.options);
        if reread {
            s.reread();
        } else {
            s.repreview();
        }
    }

    pub fn update_dialog(&mut self, msg: DialogMsg) {
        match msg {
            DialogMsg::OpenAnalyze => {
                // Start with the analyses of the graph on screen.
                let shape = match &self.engine.view {
                    View::Graph(id) | View::Plotted(id) => self.engine.project.graph(id).map(|g| g.kind.shape()).unwrap_or(DataShape::Bars),
                    _ => DataShape::Bars,
                };
                let graphs = analysis_graphs(self, shape);
                let graph = match &self.engine.view {
                    View::Graph(id) | View::Plotted(id) if graphs.iter().any(|(g, _)| g == id) => Some(id.clone()),
                    _ => graphs.first().map(|(g, _)| g.clone()),
                };
                let checked = data_set_labels(self, &graph).into_iter().map(|(l, _)| l).collect();
                self.dialog = Some(Dialog::Analyze(AnalyzeState {
                    search: String::new(),
                    method: match shape {
                        DataShape::Xy => Method::CurveFit,
                        DataShape::Survival => Method::Survival,
                        DataShape::Counts => Method::Contingency,
                        DataShape::Bars | DataShape::Grid | DataShape::Slices => Method::TTest,
                    },
                    graph,
                    checked,
                    error: None,
                    replacing: None,
                }));
            }
            DialogMsg::Recalculate(id) => {
                let graph = match &self.dialog {
                    Some(Dialog::Stale(s)) => Some(s.graph.clone()),
                    _ => None,
                };
                let ok = self.run(Command::AnalysisRecalculate(RecalculateArgs { analysis: id.clone(), graph }));
                // The warning closes once nothing in it is out of date any more.
                if let Some(Dialog::Stale(s)) = &mut self.dialog {
                    s.ids.retain(|i| self.stale.contains_key(i));
                    if ok && (id.is_none() || s.ids.is_empty()) {
                        self.dialog = None;
                    }
                }
            }
            DialogMsg::SetUpAgain(id) => self.set_up_again(&id),
            DialogMsg::UndoChange => {
                self.dialog = None;
                self.run(Command::Undo(otto_engine::NoArgs {}));
            }
            // Both are dealt with in `Message::Dialog`, which can return a task.
            DialogMsg::SaveThenDiscard(_) | DialogMsg::Discard(_) => self.dialog = None,
            DialogMsg::DeleteTable(id) => {
                self.dialog = None;
                self.run(Command::TableDelete(otto_engine::TableRef { table: Some(id) }));
            }
            DialogMsg::Search(s) => {
                if let Some(Dialog::Analyze(a)) = &mut self.dialog {
                    a.search = s;
                }
            }
            DialogMsg::PickMethod(m) => {
                // XY analyses need an XY graph, the others a bar graph.
                let switch = match &self.dialog {
                    Some(Dialog::Analyze(a)) => {
                        let fits = a.graph.as_ref().and_then(|g| self.engine.project.graph(g)).is_some_and(|g| g.kind.shape() == m.shape());
                        (!fits && m != Method::Descriptive).then(|| analysis_graphs(self, m.shape()).first().map(|(g, _)| g.clone()))
                    }
                    _ => None,
                };
                let labels: Option<Vec<String>> = switch.as_ref().map(|g| data_set_labels(self, g).into_iter().map(|(l, _)| l).collect());
                if let Some(Dialog::Analyze(a)) = &mut self.dialog {
                    a.method = m;
                    a.error = None;
                    if let (Some(g), Some(l)) = (switch, labels) {
                        a.graph = g;
                        a.checked = l;
                    }
                }
            }
            DialogMsg::PickGraph(g) => {
                let labels: Vec<String> = data_set_labels(self, &Some(g.clone())).into_iter().map(|(l, _)| l).collect();
                if let Some(Dialog::Analyze(a)) = &mut self.dialog {
                    a.graph = Some(g);
                    a.checked = labels;
                    a.error = None;
                }
            }
            DialogMsg::Toggle(label, on) => {
                if let Some(Dialog::Analyze(a)) = &mut self.dialog {
                    a.checked.retain(|l| l != &label);
                    if on {
                        a.checked.push(label);
                    }
                    a.error = None;
                }
            }
            DialogMsg::All(on) => {
                let labels: Vec<String> = match &self.dialog {
                    Some(Dialog::Analyze(a)) => data_set_labels(self, &a.graph).into_iter().map(|(l, _)| l).collect(),
                    _ => Vec::new(),
                };
                if let Some(Dialog::Analyze(a)) = &mut self.dialog {
                    a.checked = if on { labels } else { Vec::new() };
                }
            }
            DialogMsg::AnalyzeOk => self.analyze_ok(),
            DialogMsg::EditAnalysis(id) => {
                let Some(a) = self.engine.project.analyses.iter().find(|a| a.id == id) else { return };
                let params = match &a.kind {
                    AnalysisKind::TTest(t) => Params::TTest(t.clone()),
                    AnalysisKind::OneWayAnova(x) => Params::Anova(x.clone()),
                    AnalysisKind::CurveFit(f) => Params::Fit(f.clone()),
                    AnalysisKind::LinearRegression(l) => Params::LinReg(l.clone()),
                    AnalysisKind::Correlation(c) => Params::Corr(c.clone()),
                    AnalysisKind::TwoWayAnova(t) => Params::TwoWay(t.clone()),
                    AnalysisKind::Auc(x) => Params::Auc(x.clone()),
                    AnalysisKind::Survival(x) => Params::Survival(x.clone()),
                    AnalysisKind::Contingency(x) => Params::Contingency(x.clone()),
                    AnalysisKind::Normality(n) => Params::Normality(n.clone()),
                    AnalysisKind::Outliers(o) => Params::Outliers(o.clone()),
                    AnalysisKind::Descriptive { .. } => return,
                };
                self.dialog = Some(Dialog::Params(ParamsState::new(Some(id), params, None)));
            }


            DialogMsg::FitSet(label, on) => {
                let graph = match &self.dialog {
                    Some(Dialog::Params(p)) => match &p.params {
                        Params::Fit(f) => Some(f.graph_id.clone()),
                        Params::LinReg(l) => Some(l.graph_id.clone()),
                        Params::Corr(c) => Some(c.graph_id.clone()),
                        Params::TwoWay(t) => Some(t.graph_id.clone()),
                        Params::Auc(x) => Some(x.graph_id.clone()),
                        Params::Survival(x) => Some(x.graph_id.clone()),
                        Params::Contingency(x) => Some(x.graph_id.clone()),
                        Params::Normality(x) => Some(x.graph_id.clone()),
                        Params::Outliers(x) => Some(x.graph_id.clone()),
                        _ => None,
                    },
                    _ => None,
                };
                let all: Vec<String> = data_set_labels(self, &graph).into_iter().map(|(l, _)| l).collect();
                if let Some(Dialog::Params(p)) = &mut self.dialog {
                    let sets = match &mut p.params {
                        Params::Fit(f) => &mut f.data_sets,
                        Params::LinReg(l) => &mut l.data_sets,
                        Params::Corr(c) => &mut c.data_sets,
                        Params::TwoWay(t) => &mut t.data_sets,
                        Params::Auc(x) => &mut x.data_sets,
                        Params::Survival(x) => &mut x.data_sets,
                        Params::Contingency(x) => &mut x.data_sets,
                        Params::Normality(x) => &mut x.data_sets,
                        Params::Outliers(x) => &mut x.data_sets,
                        _ => return,
                    };
                    // Empty means every data set; keep the graph's order.
                    let mut chosen: Vec<String> = if sets.is_empty() { all.clone() } else { sets.clone() };
                    chosen.retain(|l| l != &label);
                    if on {
                        chosen.push(label);
                    }
                    let ordered: Vec<String> = all.iter().filter(|l| chosen.contains(l)).cloned().collect();
                    *sets = if ordered.len() == all.len() { Vec::new() } else { ordered };
                    p.error = None;
                }
            }
            DialogMsg::OpenEngine => {
                otto_r::retry();
                let status = otto_r::status();
                let path = otto_r::locate::configured().map(|p| p.display().to_string()).unwrap_or_default();
                self.dialog = Some(Dialog::Engine(EngineState { status, path, refreshed: true }));
            }
            DialogMsg::EngineSetup => {
                if let Err(e) = otto_r::start_setup() {
                    self.set_status(e.to_string(), true);
                }
                if let Some(Dialog::Engine(s)) = &mut self.dialog {
                    s.refreshed = false;
                }
            }
            DialogMsg::EnginePath(text) => {
                if let Some(Dialog::Engine(s)) = &mut self.dialog {
                    s.path = text;
                }
            }
            DialogMsg::EngineSavePath => {
                let Some(Dialog::Engine(s)) = &self.dialog else { return };
                let path = s.path.trim().to_string();
                self.settings.r_path = (!path.is_empty()).then_some(path);
                if let Some(file) = &self.settings_file {
                    if let Err(e) = self.settings.save(file) {
                        self.set_status(format!("Could not save settings: {e}"), true);
                    }
                }
                otto_r::restart();
                let status = otto_r::status();
                if let Some(Dialog::Engine(s)) = &mut self.dialog {
                    s.status = status;
                }
            }
            DialogMsg::EngineTick => {
                let finished = !otto_r::setup_progress().running;
                if let Some(Dialog::Engine(s)) = &mut self.dialog {
                    if finished && !s.refreshed {
                        s.status = otto_r::status();
                        s.refreshed = true;
                    }
                }
            }
            DialogMsg::ImportSeparator(v) => self.import_setting(|o| o.separator = v, true),
            DialogMsg::ImportEncoding(v) => self.import_setting(|o| o.encoding = v, true),
            DialogMsg::ImportDecimal(v) => self.import_setting(|o| o.decimal = v, true),
            DialogMsg::ImportSheet(name) => {
                if let Some(Dialog::Import(s)) = &mut self.dialog {
                    s.sheet = Some(name);
                    s.reread();
                }
            }
            DialogMsg::ImportRole(i, role) => self.import_setting(move |o| o.set_role(i, role), false),
            DialogMsg::ImportColumnKind(i, kind) => self.import_setting(move |o| o.set_column(i, |c| c.kind = kind), false),
            DialogMsg::ImportColumnInclude(i, on) => self.import_setting(move |o| o.set_column(i, |c| c.include = on), false),
            DialogMsg::ImportOk => {
                let Some(Dialog::Import(s)) = &self.dialog else { return };
                let cmd = Command::TableImportFile(ImportFileArgs {
                    path: s.path.display().to_string(),
                    sheet: s.sheet.clone(),
                    name: None,
                    options: Some(s.options.clone()),
                });
                // A file that cannot be read keeps the wizard open with the reason.
                if self.run(cmd) {
                    self.dialog = None;
                } else {
                    let why = self.status.0.clone();
                    if let Some(Dialog::Import(s)) = &mut self.dialog {
                        s.error = Some(why);
                    }
                }
            }
            DialogMsg::Tab(t) => {
                if let Some(Dialog::Params(p)) = &mut self.dialog {
                    p.tab = t;
                }
            }
            DialogMsg::Change(c) => {
                if let Some(Dialog::Params(p)) = &mut self.dialog {
                    apply_change(p, c);
                    p.error = None;
                }
            }
            DialogMsg::ParamsOk => self.params_ok(),
            DialogMsg::Cancel => self.dialog = None,
        }
    }

    fn analyze_ok(&mut self) {
        let Some(Dialog::Analyze(a)) = &self.dialog else { return };
        let Some(graph) = a.graph.clone() else {
            if let Some(Dialog::Analyze(a)) = &mut self.dialog {
                a.error = Some("Create a column or grouped graph first: its bars are the data sets.".into());
            }
            return;
        };
        // Keep the graph's bar order, whatever order the boxes were ticked in.
        let order: Vec<String> = data_set_labels(self, &Some(graph.clone())).into_iter().map(|(l, _)| l).collect();
        let checked: Vec<String> = order.into_iter().filter(|l| a.checked.contains(l)).collect();
        let error = match a.method {
            Method::TTest if checked.len() != 2 => Some(format!("A t test compares exactly two data sets; {} are ticked.", checked.len())),
            Method::OneWayAnova if checked.len() < 2 => Some("One-way ANOVA needs at least two data sets.".to_string()),
            m if m != Method::Descriptive && checked.is_empty() => Some("Tick at least one data set.".to_string()),
            _ => None,
        };
        if let Some(e) = error {
            if let Some(Dialog::Analyze(a)) = &mut self.dialog {
                a.error = Some(e);
            }
            return;
        }
        // Setting up an existing analysis again keeps its parameters when the method is the
        // same; another method replaces it.
        let previous = a.replacing.as_ref().and_then(|id| self.engine.project.analyses.iter().find(|x| &x.id == id).cloned());
        let same = |p: &otto_core::AnalysisSpec| {
            matches!(
                (&p.kind, a.method),
                (AnalysisKind::TTest(_), Method::TTest)
                    | (AnalysisKind::OneWayAnova(_), Method::OneWayAnova)
                    | (AnalysisKind::CurveFit(_), Method::CurveFit)
                    | (AnalysisKind::LinearRegression(_), Method::LinearRegression)
                    | (AnalysisKind::Correlation(_), Method::Correlation)
                    | (AnalysisKind::TwoWayAnova(_), Method::TwoWayAnova)
                    | (AnalysisKind::Auc(_), Method::Auc)
                    | (AnalysisKind::Survival(_), Method::Survival)
                    | (AnalysisKind::Contingency(_), Method::Contingency)
                    | (AnalysisKind::Normality(_), Method::Normality)
                    | (AnalysisKind::Outliers(_), Method::Outliers)
            )
        };
        let (editing, replaces) = match &previous {
            Some(p) if same(p) => (Some(p.id.clone()), None),
            Some(p) if a.method != Method::Descriptive => (None, Some(p.id.clone())),
            _ => (None, None),
        };
        let params_dialog = |params: Params| Dialog::Params(ParamsState::new(editing.clone(), params, replaces.clone()));
        // XY analyses: every data set ticked means "all" (so new ones are included too).
        let all_ticked = data_set_labels(self, &Some(graph.clone())).len() == checked.len();
        let xy_sets = if all_ticked { Vec::new() } else { checked.clone() };
        match a.method {
            Method::Descriptive => {
                let table = self.engine.project.graph(&graph).map(|g| g.table_id.clone());
                self.dialog = None;
                self.run(Command::AnalysisCreateDescriptive(DescriptiveArgs { table, ..Default::default() }));
            }
            Method::TTest => {
                let spec = match previous.map(|p| p.kind) {
                    Some(otto_core::AnalysisKind::TTest(t)) if editing.is_some() => TTestSpec { graph_id: graph, data_sets: checked, ..t },
                    _ => TTestSpec::new(graph, checked),
                };
                self.dialog = Some(params_dialog(Params::TTest(spec)));
            }
            Method::OneWayAnova => {
                let spec = match previous.map(|p| p.kind) {
                    Some(otto_core::AnalysisKind::OneWayAnova(x)) if editing.is_some() => {
                        // Follow-up choices that refer to bars no longer picked fall back to defaults.
                        let comparisons = match x.comparisons.clone() {
                            Comparisons::VsControl { control } if !checked.contains(&control) => {
                                Comparisons::VsControl { control: checked[0].clone() }
                            }
                            Comparisons::Selected { pairs } => {
                                Comparisons::Selected { pairs: pairs.into_iter().filter(|[a, b]| checked.contains(a) && checked.contains(b)).collect() }
                            }
                            other => other,
                        };
                        AnovaSpec { graph_id: graph, data_sets: checked, comparisons, ..x }
                    }
                    _ => AnovaSpec::new(graph, checked),
                };
                self.dialog = Some(params_dialog(Params::Anova(spec)));
            }
            Method::CurveFit => {
                let spec = match previous.map(|p| p.kind) {
                    Some(AnalysisKind::CurveFit(f)) if editing.is_some() => FitSpec { graph_id: graph, data_sets: xy_sets, ..f },
                    _ => FitSpec { data_sets: xy_sets, ..FitSpec::new(graph, FitModel::Agonist4pl) },
                };
                self.dialog = Some(params_dialog(Params::Fit(spec)));
            }
            Method::LinearRegression => {
                let spec = match previous.map(|p| p.kind) {
                    Some(AnalysisKind::LinearRegression(l)) if editing.is_some() => LinRegSpec { graph_id: graph, data_sets: xy_sets, ..l },
                    _ => LinRegSpec { data_sets: xy_sets, ..LinRegSpec::new(graph) },
                };
                self.dialog = Some(params_dialog(Params::LinReg(spec)));
            }
            Method::Correlation => {
                let spec = match previous.map(|p| p.kind) {
                    Some(AnalysisKind::Correlation(c)) if editing.is_some() => CorrelationSpec { graph_id: graph, data_sets: xy_sets, ..c },
                    _ => CorrelationSpec { data_sets: xy_sets, ..CorrelationSpec::new(graph) },
                };
                self.dialog = Some(params_dialog(Params::Corr(spec)));
            }
            Method::TwoWayAnova => {
                let spec = match previous.map(|p| p.kind) {
                    Some(AnalysisKind::TwoWayAnova(t)) if editing.is_some() => TwoWaySpec { graph_id: graph, data_sets: xy_sets, ..t },
                    _ => TwoWaySpec { data_sets: xy_sets, ..TwoWaySpec::new(graph) },
                };
                self.dialog = Some(params_dialog(Params::TwoWay(spec)));
            }
            Method::Auc => {
                let spec = match previous.map(|p| p.kind) {
                    Some(AnalysisKind::Auc(x)) if editing.is_some() => AucSpec { graph_id: graph, data_sets: xy_sets, ..x },
                    _ => AucSpec { data_sets: xy_sets, ..AucSpec::new(graph) },
                };
                self.dialog = Some(params_dialog(Params::Auc(spec)));
            }
            Method::Survival => {
                let spec = match previous.map(|p| p.kind) {
                    Some(AnalysisKind::Survival(x)) if editing.is_some() => SurvivalSpec { graph_id: graph, data_sets: xy_sets, ..x },
                    _ => SurvivalSpec { data_sets: xy_sets, ..SurvivalSpec::new(graph) },
                };
                self.dialog = Some(params_dialog(Params::Survival(spec)));
            }
            Method::Contingency => {
                let spec = match previous.map(|p| p.kind) {
                    Some(AnalysisKind::Contingency(x)) if editing.is_some() => ContingencySpec { graph_id: graph, data_sets: xy_sets, ..x },
                    _ => ContingencySpec { data_sets: xy_sets, ..ContingencySpec::new(graph) },
                };
                self.dialog = Some(params_dialog(Params::Contingency(spec)));
            }
            Method::Normality => {
                let spec = match previous.map(|p| p.kind) {
                    Some(AnalysisKind::Normality(x)) if editing.is_some() => NormalitySpec { graph_id: graph, data_sets: xy_sets, ..x },
                    _ => NormalitySpec { data_sets: xy_sets, ..NormalitySpec::new(graph) },
                };
                self.dialog = Some(params_dialog(Params::Normality(spec)));
            }
            Method::Outliers => {
                let spec = match previous.map(|p| p.kind) {
                    Some(AnalysisKind::Outliers(x)) if editing.is_some() => OutlierSpec { graph_id: graph, data_sets: xy_sets, ..x },
                    _ => OutlierSpec { data_sets: xy_sets, ..OutlierSpec::new(graph) },
                };
                self.dialog = Some(params_dialog(Params::Outliers(spec)));
            }
        }
    }

    fn params_ok(&mut self) {
        let Some(Dialog::Params(mut p)) = self.dialog.clone() else { return };
        // Constraints as typed: each must be a number.
        if let Params::Fit(f) = &mut p.params {
            let mut fixed = Vec::new();
            for (name, text) in p.fixed_text.iter().filter(|(n, _)| f.model.params().contains(&n.as_str())) {
                match text.trim().replace(',', ".").parse::<f64>() {
                    Ok(v) if v.is_finite() => fixed.push(FixedParam { name: name.clone(), value: v }),
                    _ => {
                        if let Some(Dialog::Params(d)) = &mut self.dialog {
                            d.error = Some(format!("Constraints: type a number for {name} (or untick it)."));
                            d.tab = Tab::Constraints;
                        }
                        return;
                    }
                }
            }
            f.fixed = fixed;
        }
        if let Params::Auc(a) = &mut p.params {
            let num = |t: &str| t.trim().replace(',', ".").parse::<f64>().ok().filter(|v| v.is_finite());
            match (num(&p.baseline_text), num(&p.min_peak_text)) {
                (Some(b), Some(m)) if (0.0..=100.0).contains(&m) => {
                    a.baseline = b;
                    a.min_peak_percent = m;
                }
                _ => {
                    if let Some(Dialog::Params(d)) = &mut self.dialog {
                        d.error = Some("Type a number for the baseline, and a percentage (0–100) for the smallest peak.".into());
                    }
                    return;
                }
            }
        }
        let cmd = match (&p.params, &p.editing) {
            (Params::TTest(s), None) => Command::AnalysisCreateTTest(TTestArgs { name: None, spec: s.clone() }),
            (Params::TTest(s), Some(id)) => Command::AnalysisEditTTest(EditTTestArgs::full(id.clone(), s)),
            (Params::Anova(s), None) => Command::AnalysisCreateOneWayAnova(AnovaArgs { name: None, spec: s.clone() }),
            (Params::Anova(s), Some(id)) => Command::AnalysisEditOneWayAnova(EditAnovaArgs::full(id.clone(), s)),
            (Params::Fit(s), None) => Command::AnalysisCreateCurveFit(FitArgs { name: None, spec: s.clone() }),
            (Params::Fit(s), Some(id)) => Command::AnalysisEditCurveFit(EditFitArgs::full(id.clone(), s)),
            (Params::LinReg(s), None) => Command::AnalysisCreateLinearRegression(LinRegArgs { name: None, spec: s.clone() }),
            (Params::LinReg(s), Some(id)) => Command::AnalysisEditLinearRegression(EditLinRegArgs {
                analysis: Some(id.clone()),
                graph_id: Some(s.graph_id.clone()),
                data_sets: Some(s.data_sets.clone()),
                confidence: Some(s.confidence),
                output: Some(s.output),
                graph: Some(s.graph),
            }),
            (Params::Corr(s), None) => Command::AnalysisCreateCorrelation(CorrelationArgs { name: None, spec: s.clone() }),
            (Params::Corr(s), Some(id)) => Command::AnalysisEditCorrelation(EditCorrelationArgs {
                analysis: Some(id.clone()),
                graph_id: Some(s.graph_id.clone()),
                data_sets: Some(s.data_sets.clone()),
                method: Some(s.method),
                one_tailed: Some(s.one_tailed),
                confidence: Some(s.confidence),
                output: Some(s.output),
            }),
            (Params::TwoWay(s), None) => Command::AnalysisCreateTwoWayAnova(TwoWayArgs { name: None, spec: s.clone() }),
            (Params::TwoWay(s), Some(id)) => Command::AnalysisEditTwoWayAnova(EditTwoWayArgs {
                analysis: Some(id.clone()),
                data_sets: Some(s.data_sets.clone()),
                interaction: Some(s.interaction),
                compare: Some(s.compare),
                correction: Some(s.correction),
                alpha: Some(s.alpha),
                results: Some(s.results),
                output: Some(s.output),
                graph: Some(s.graph),
            }),
            (Params::Auc(s), None) => Command::AnalysisCreateAuc(AucArgs { name: None, spec: s.clone() }),
            (Params::Auc(s), Some(id)) => Command::AnalysisEditAuc(EditAucArgs {
                analysis: Some(id.clone()),
                graph_id: Some(s.graph_id.clone()),
                data_sets: Some(s.data_sets.clone()),
                baseline: Some(s.baseline),
                min_peak_percent: Some(s.min_peak_percent),
                peaks_below: Some(s.peaks_below),
                confidence: Some(s.confidence),
                output: Some(s.output),
            }),
            (Params::Survival(s), None) => Command::AnalysisCreateSurvival(SurvivalArgs { name: None, spec: s.clone() }),
            (Params::Survival(s), Some(id)) => Command::AnalysisEditSurvival(EditSurvivalArgs {
                analysis: Some(id.clone()),
                graph_id: Some(s.graph_id.clone()),
                data_sets: Some(s.data_sets.clone()),
                confidence: Some(s.confidence),
                proportions: Some(s.proportions),
                output: Some(s.output),
            }),
            (Params::Contingency(s), None) => Command::AnalysisCreateContingency(ContingencyArgs { name: None, spec: s.clone() }),
            (Params::Contingency(s), Some(id)) => Command::AnalysisEditContingency(EditContingencyArgs {
                analysis: Some(id.clone()),
                graph_id: Some(s.graph_id.clone()),
                data_sets: Some(s.data_sets.clone()),
                paired: Some(s.paired),
                confidence: Some(s.confidence),
                expected: Some(s.expected),
                output: Some(s.output),
            }),
            (Params::Normality(s), None) => Command::AnalysisCreateNormality(NormalityArgs { name: None, spec: s.clone() }),
            (Params::Normality(s), Some(id)) => Command::AnalysisEditNormality(EditNormalityArgs {
                analysis: Some(id.clone()),
                data_sets: Some(s.data_sets.clone()),
                tests: Some(s.tests),
                lognormal: Some(s.lognormal),
                output: Some(s.output),
            }),
            (Params::Outliers(s), None) => Command::AnalysisCreateOutliers(OutlierArgs { name: None, spec: s.clone() }),
            (Params::Outliers(s), Some(id)) => Command::AnalysisEditOutliers(EditOutlierArgs {
                analysis: Some(id.clone()),
                data_sets: Some(s.data_sets.clone()),
                method: Some(s.method),
                alpha: Some(s.alpha),
                max_outliers: Some(s.max_outliers),
                output: Some(s.output),
            }),
        };
        if self.run(cmd) {
            self.dialog = None;
            if let Some(old) = p.replaces {
                self.run(Command::AnalysisDelete(otto_engine::AnalysisRef { analysis: Some(old) }));
            }
        } else if let Some(Dialog::Params(p)) = &mut self.dialog {
            p.error = Some(self.status.0.clone());
        }
    }

    /// Opens Analyze Data for an out-of-date analysis: same method, its graph, and the
    /// data sets it used that are still on the graph (all bars if none are).
    fn set_up_again(&mut self, id: &str) {
        let Some(a) = self.engine.project.analyses.iter().find(|a| a.id == id) else { return };
        let method = match &a.kind {
            AnalysisKind::TTest(_) => Method::TTest,
            AnalysisKind::OneWayAnova(_) => Method::OneWayAnova,
            AnalysisKind::CurveFit(_) => Method::CurveFit,
            AnalysisKind::LinearRegression(_) => Method::LinearRegression,
            AnalysisKind::Correlation(_) => Method::Correlation,
            AnalysisKind::TwoWayAnova(_) => Method::TwoWayAnova,
            AnalysisKind::Auc(_) => Method::Auc,
            AnalysisKind::Survival(_) => Method::Survival,
            AnalysisKind::Contingency(_) => Method::Contingency,
            AnalysisKind::Normality(_) => Method::Normality,
            AnalysisKind::Outliers(_) => Method::Outliers,
            AnalysisKind::Descriptive { .. } => return,
        };
        let (graph, used) = (a.kind.graph_id().unwrap_or_default().to_string(), a.kind.data_sets().to_vec());
        let labels: Vec<String> = data_set_labels(self, &Some(graph.clone())).into_iter().map(|(l, _)| l).collect();
        let mut checked: Vec<String> = labels.iter().filter(|l| used.contains(l)).cloned().collect();
        if checked.is_empty() || (method == Method::OneWayAnova && checked.len() < 2) {
            checked = labels;
        }
        self.dialog = Some(Dialog::Analyze(AnalyzeState {
            search: String::new(),
            method,
            graph: Some(graph),
            checked,
            error: None,
            replacing: Some(id.to_string()),
        }));
    }
}

fn apply_change(p: &mut ParamsState, c: Change) {
    match &mut p.params {
        Params::TTest(s) => match c {
            Change::Paired(v) => s.design = if v { Pairing::Paired } else { Pairing::Unpaired },
            Change::MatchBy(v) => s.match_by = (!v.is_empty()).then_some(v),
            Change::Dist(v) => s.distribution = v,
            Change::Welch(v) => s.welch = v,
            Change::OneTailed(v) => s.one_tailed = v,
            Change::Difference(v) => s.difference = v,
            Change::Confidence(v) => s.confidence = v as f64 / 100.0,
            Change::Descriptive(v) => s.results.descriptive = v,
            Change::Aicc(v) => s.results.aicc = v,
            Change::Effect(v) => s.results.effect_sizes = v,
            Change::Digits(v) => s.output.digits = v,
            Change::PStyle(v) => s.output.p_style = v,
            Change::ShowOnGraph(v) => s.graph.show = v,
            Change::ShowNs(v) => s.graph.show_ns = v,
            Change::Normality(t, v) => set_normality(&mut s.normality, t, v),
            _ => {}
        },
        Params::Anova(s) => match c {
            Change::Matched(v) => s.design = if v { Matching::Matched } else { Matching::Unmatched },
            Change::MatchBy(v) => s.match_by = (!v.is_empty()).then_some(v),
            Change::Dist(v) => s.distribution = v,
            Change::EqualSd(v) => s.equal_sd = v,
            Change::Sphericity(v) => s.sphericity = v,
            Change::Mode(m) => {
                s.comparisons = match m {
                    CompMode::None => Comparisons::None,
                    CompMode::AllPairs => Comparisons::AllPairs,
                    CompMode::VsControl => Comparisons::VsControl { control: s.data_sets.first().cloned().unwrap_or_default() },
                    CompMode::Selected => Comparisons::Selected { pairs: Vec::new() },
                };
                p.pairs_open = m == CompMode::Selected;
            }
            Change::Control(v) => s.comparisons = Comparisons::VsControl { control: v },
            Change::TogglePair(a, b, on) => {
                if let Comparisons::Selected { pairs } = &mut s.comparisons {
                    pairs.retain(|[x, y]| !((x == &a && y == &b) || (x == &b && y == &a)));
                    if on {
                        pairs.push([a, b]);
                    }
                }
            }
            Change::PairsOpen(v) => p.pairs_open = v,
            Change::Correction(v) => s.correction = v,
            Change::Alpha(v) => s.alpha = v as f64 / 1000.0,
            Change::Swap(v) => s.swap = v,
            Change::Descriptive(v) => s.results.descriptive = v,
            Change::Aicc(v) => s.results.aicc = v,
            Change::Effect(v) => s.results.effect_sizes = v,
            Change::Digits(v) => s.output.digits = v,
            Change::PStyle(v) => s.output.p_style = v,
            Change::ShowOnGraph(v) => s.graph.show = v,
            Change::ShowNs(v) => s.graph.show_ns = v,
            Change::Normality(t, v) => set_normality(&mut s.normality, t, v),
            _ => {}
        },
        Params::Fit(s) => match c {
            Change::Family(f) => {
                p.family = f;
                // Show the family's most common model.
                if s.model.family() != f {
                    let models = f.models();
                    let pick = models.iter().copied().find(|m| matches!(m, FitModel::Agonist4pl | FitModel::Inhibitor4pl)).or_else(|| models.first().copied());
                    if let Some(m) = pick {
                        s.model = m;
                    }
                }
            }
            Change::Model(m) => s.model = m,
            Change::Fix(name, on) => {
                p.fixed_text.retain(|(n, _)| n != &name);
                if on {
                    let default = match name.as_str() {
                        "Bottom" => "0",
                        "Top" => "100",
                        "HillSlope" | "h" | "S" => "1",
                        _ => "0",
                    };
                    p.fixed_text.push((name, default.to_string()));
                }
            }
            Change::FixValue(name, text) => {
                if let Some(v) = p.fixed_text.iter_mut().find(|(n, _)| n == &name) {
                    v.1 = text;
                }
            }
            Change::Confidence(v) => s.confidence = v as f64 / 100.0,
            Change::ShowCurve(v) => s.graph.show = v,
            Change::Digits(v) => s.output.digits = v,
            Change::PStyle(v) => s.output.p_style = v,
            _ => {}
        },
        Params::LinReg(s) => match c {
            Change::Confidence(v) => s.confidence = v as f64 / 100.0,
            Change::ShowCurve(v) => s.graph.show = v,
            Change::Digits(v) => s.output.digits = v,
            Change::PStyle(v) => s.output.p_style = v,
            _ => {}
        },
        Params::TwoWay(s) => match c {
            Change::Interaction(v) => s.interaction = v,
            Change::TwoWayCompare(v) => s.compare = v,
            Change::Correction(v) => s.correction = v,
            Change::Alpha(v) => s.alpha = v as f64 / 1000.0,
            Change::Descriptive(v) => s.results.descriptive = v,
            Change::Digits(v) => s.output.digits = v,
            Change::PStyle(v) => s.output.p_style = v,
            Change::ShowOnGraph(v) => s.graph.show = v,
            Change::ShowNs(v) => s.graph.show_ns = v,
            _ => {}
        },
        Params::Auc(s) => match c {
            Change::Baseline(v) => p.baseline_text = v,
            Change::MinPeak(v) => p.min_peak_text = v,
            Change::PeaksBelow(v) => s.peaks_below = v,
            Change::Confidence(v) => s.confidence = v as f64 / 100.0,
            Change::Digits(v) => s.output.digits = v,
            Change::PStyle(v) => s.output.p_style = v,
            _ => {}
        },
        Params::Survival(s) => match c {
            Change::Confidence(v) => s.confidence = v as f64 / 100.0,
            Change::Proportions(v) => s.proportions = v,
            Change::Digits(v) => s.output.digits = v,
            Change::PStyle(v) => s.output.p_style = v,
            _ => {}
        },
        Params::Contingency(s) => match c {
            Change::Paired(v) => s.paired = v,
            Change::Confidence(v) => s.confidence = v as f64 / 100.0,
            Change::Expected(v) => s.expected = v,
            Change::Digits(v) => s.output.digits = v,
            Change::PStyle(v) => s.output.p_style = v,
            _ => {}
        },
        Params::Normality(s) => match c {
            Change::Normality(name, v) => set_normality(&mut s.tests, name, v),
            Change::Lognormal(v) => s.lognormal = v,
            Change::Digits(v) => s.output.digits = v,
            Change::PStyle(v) => s.output.p_style = v,
            _ => {}
        },
        Params::Outliers(s) => match c {
            Change::OutlierMethod(m) => s.method = m,
            Change::Alpha(v) => s.alpha = v as f64 / 1000.0,
            Change::MaxOutliers(v) => s.max_outliers = v,
            Change::Digits(v) => s.output.digits = v,
            Change::PStyle(v) => s.output.p_style = v,
            _ => {}
        },
        Params::Corr(s) => match c {
            Change::CorrMethod(m) => s.method = m,
            Change::OneTailed(v) => s.one_tailed = v,
            Change::Confidence(v) => s.confidence = v as f64 / 100.0,
            Change::Digits(v) => s.output.digits = v,
            Change::PStyle(v) => s.output.p_style = v,
            _ => {}
        },
    }
}

fn set_normality(n: &mut NormalityTests, test: &str, on: bool) {
    match test {
        "dagostino" => n.dagostino = on,
        "anderson" => n.anderson = on,
        "shapiro" => n.shapiro = on,
        "ks" => n.ks = on,
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Views
// ---------------------------------------------------------------------------

fn msg(c: Change) -> Message {
    Message::Dialog(DialogMsg::Change(c))
}

fn heading<'a>(s: &'a str) -> El<'a> {
    text(s).size(13.5).font(iced::Font { weight: iced::font::Weight::Bold, ..Default::default() }).into()
}

fn desc<'a>(s: &'a str) -> El<'a> {
    container(caption(s).size(12)).padding(iced::Padding { left: 26.0, ..Default::default() }).into()
}

fn yellow_box<'a>(content: El<'a>) -> El<'a> {
    container(content)
        .padding([10, 12])
        .width(Fill)
        .style(|_| container::Style {
            background: Some(Color::from_rgba8(0xD2, 0x99, 0x22, 0.14).into()),
            border: iced::Border { color: Color::from_rgba8(0xD2, 0x99, 0x22, 0.55), width: 1.0, radius: 4.0.into() },
            text_color: Some(Color::from_rgb8(0xF0, 0xD8, 0x9A)),
            ..Default::default()
        })
        .into()
}

fn footer<'a>(ok: Message, error: Option<&'a str>) -> El<'a> {
    let err: El = match error {
        Some(e) => text(e).size(12.5).color(style::DANGER).into(),
        None => space().into(),
    };
    column![
        err,
        row![
            button(text("Learn").size(13)).padding([5, 18]).style(style::secondary),
            space::horizontal(),
            button(text("Cancel").size(13)).padding([5, 18]).style(style::secondary).on_press(Message::Dialog(DialogMsg::Cancel)),
            button(text("OK").size(13)).padding([5, 26]).style(style::primary).on_press(ok),
        ]
        .spacing(10)
    ]
    .spacing(8)
    .into()
}

fn frame<'a>(title: String, body: El<'a>, width: f32) -> El<'a> {
    let title_bar = row![
        text(title).size(14),
        space::horizontal(),
        crate::view::close_x(Message::Dialog(DialogMsg::Cancel))
    ]
    .align_y(Alignment::Center);
    container(column![title_bar, body].spacing(12))
        .padding(16)
        .width(width)
        .max_height(760.0)
        .style(|_| container::Style {
            background: Some(style::PANEL.into()),
            border: iced::Border { color: style::BORDER, width: 1.0, radius: 8.0.into() },
            shadow: iced::Shadow { color: Color::from_rgba8(0, 0, 0, 0.6), offset: iced::Vector::new(0.0, 8.0), blur_radius: 30.0 },
            text_color: Some(style::TEXT),
            ..Default::default()
        })
        .into()
}

/// The dialog on top of the window with a dimmed, click-blocking backdrop.
pub fn overlay<'a>(app: &'a App, base: El<'a>) -> El<'a> {
    let Some(dialog) = &app.dialog else { return base };
    let dlg = match dialog {
        Dialog::Analyze(a) => analyze_view(app, a),
        Dialog::Params(p) => params_view(app, p),
        Dialog::Stale(s) => stale_view(app, s),
        Dialog::ConfirmDeleteTable(id) => delete_table_view(app, id),
        Dialog::ConfirmDiscard(what) => discard_view(app, *what),
        Dialog::Engine(s) => engine_view(s),
        Dialog::Import(s) => import_view(s),
    };
    let backdrop = opaque(mouse_area(container(space()).width(Fill).height(Fill).style(|_| container::Style {
        background: Some(Color::from_rgba8(0, 0, 0, 0.55).into()),
        ..Default::default()
    })));
    iced::widget::stack![base, backdrop, opaque(container(dlg).center(Fill))].into()
}

/// What an analysis computes, in one line (test, follow-up test).
pub fn analysis_summary(a: &otto_core::AnalysisSpec) -> String {
    match &a.kind {
        otto_core::AnalysisKind::TTest(t) => format!("{} · {}", t.test_name(), t.data_sets.join(" vs. ")),
        otto_core::AnalysisKind::OneWayAnova(x) if x.comparisons != Comparisons::None => {
            format!("{} · {}", x.test_name(), otto_engine::column_tests::followup_name(x))
        }
        otto_core::AnalysisKind::OneWayAnova(x) => x.test_name().to_string(),
        otto_core::AnalysisKind::Descriptive { .. } => "Descriptive statistics".into(),
        otto_core::AnalysisKind::Normality(n) => {
            let names: Vec<&str> = n.chosen().names().into_iter().map(NormalityTests::label).collect();
            format!("Normality · {}", names.join(", "))
        }
        otto_core::AnalysisKind::Outliers(o) => format!("Outliers · {}", o.method.label()),
        otto_core::AnalysisKind::CurveFit(f) => {
            let n = f.fixed.len();
            let constraints = if n == 0 { String::new() } else { format!(" · {n} constraint{}", if n == 1 { "" } else { "s" }) };
            format!("Nonlinear regression · {}{constraints}", f.model.label())
        }
        otto_core::AnalysisKind::LinearRegression(_) => "Simple linear regression".into(),
        otto_core::AnalysisKind::Correlation(c) => match c.method {
            CorrMethod::Pearson => "Correlation · Pearson r".into(),
            CorrMethod::Spearman => "Correlation · Spearman r".into(),
        },
        otto_core::AnalysisKind::TwoWayAnova(t) => {
            let model = if t.interaction { "full model" } else { "main effects only" };
            format!("Two-way ANOVA ({model}) · {}", two_way_compare_label(t.compare))
        }
        otto_core::AnalysisKind::Auc(_) => "Area under curve".into(),
        otto_core::AnalysisKind::Survival(_) => "Kaplan-Meier · log-rank and Gehan-Breslow-Wilcoxon".into(),
        otto_core::AnalysisKind::Contingency(c) => {
            let paired = if c.paired { " · paired (McNemar)" } else { "" };
            format!("Contingency · chi-square and Fisher's exact test{paired}")
        }
    }
}

fn two_way_compare_label(c: TwoWayCompare) -> &'static str {
    match c {
        TwoWayCompare::None => "no multiple comparisons",
        TwoWayCompare::WithinRows => "compare data sets within each X category",
        TwoWayCompare::WithinColumns => "compare X categories within each data set",
        TwoWayCompare::RowMeans => "compare X category means (main effect)",
        TwoWayCompare::ColumnMeans => "compare data set means (main effect)",
        TwoWayCompare::AllCells => "compare every bar with every other",
    }
}

/// Whether "recalculate with the data shown" can work for this analysis.
pub fn can_recalculate(app: &App, a: &otto_core::AnalysisSpec) -> Result<(), String> {
    app.can_recalculate(&a.id)
}

/// Asks before deleting a data table, listing everything that goes with it.
fn delete_table_view<'a>(app: &'a App, id: &'a str) -> El<'a> {
    let p = &app.engine.project;
    let Some(t) = p.table_by_id(id) else { return space().into() };
    let gone = app.engine.table_deletion(t);
    let names = |ids: &[String]| ids.iter().filter_map(|g| p.graph(g)).map(|g| format!("\"{}\"", g.name)).collect::<Vec<_>>().join(", ");
    let mut body = Column::new().spacing(10).push(text(format!("Delete the data table \"{}\" ({} rows × {} columns)?", t.name, t.nrows, t.ncols())).size(13.5));
    let mut also = Column::new().spacing(4);
    if !gone.quick.is_empty() {
        also = also.push(text(format!("• quick graphs (and their plotted tables): {}", names(&gone.quick))).size(12.5));
    }
    if !gone.finals.is_empty() {
        also = also.push(text(format!("• final graphs, with their final tables and statistics: {}", names(&gone.finals))).size(12.5));
    }
    if gone.results > 0 {
        also = also.push(text(format!("• {} statistics / descriptive result(s)", gone.results)).size(12.5));
    }
    if gone.quick.is_empty() && gone.finals.is_empty() && gone.results == 0 {
        body = body.push(caption("No graphs or results use it.").size(12.5));
    } else {
        body = body.push(yellow_box(column![text("This also deletes everything made from it:").size(13), also].spacing(6).into()));
    }
    body = body.push(caption("You can undo this with Ctrl+Z.").size(12));
    body = body.push(
        row![
            space::horizontal(),
            button(text("Cancel").size(13)).padding([5, 18]).style(style::secondary).on_press(Message::Dialog(DialogMsg::Cancel)),
            button(text("Delete").size(13)).padding([5, 18]).style(style::danger).on_press(Message::Dialog(DialogMsg::DeleteTable(t.id.clone()))),
        ]
        .spacing(10),
    );
    frame("Delete data table".into(), body.into(), 520.0)
}

/// "Save changes first?", asked before New or Open throws the current project away.
/// The app's own dialog rather than the system one, so it looks and behaves like the
/// other questions it asks.
fn discard_view<'a>(app: &'a App, what: Pending) -> El<'a> {
    let name = app.engine.project.name.clone();
    let line = match what {
        Pending::New => format!("Starting a new project closes \"{name}\", which has unsaved changes."),
        Pending::Open => format!("Opening another project closes \"{name}\", which has unsaved changes."),
        Pending::Close(_) => format!("\"{name}\" has unsaved changes."),
    };
    let body = column![
        text(line).size(13.5),
        caption("\"Don't save\" throws those changes away: there is no undo for it.").size(12),
        row![
            button(text("Cancel").size(13)).padding([5, 18]).style(style::secondary).on_press(Message::Dialog(DialogMsg::Cancel)),
            space::horizontal(),
            button(text("Don't save").size(13)).padding([5, 18]).style(style::danger).on_press(Message::Dialog(DialogMsg::Discard(what))),
            button(text("Save").size(13)).padding([5, 18]).style(style::primary).on_press(Message::Dialog(DialogMsg::SaveThenDiscard(what))),
        ]
        .spacing(10),
    ]
    .spacing(12);
    frame("Unsaved changes".into(), body.into(), 480.0)
}

/// The warning shown right after a graph change made statistics out of date.
fn stale_view<'a>(app: &'a App, s: &'a StaleState) -> El<'a> {
    let analyses: Vec<&otto_core::AnalysisSpec> =
        s.ids.iter().filter_map(|id| app.engine.project.analyses.iter().find(|a| &a.id == id)).collect();
    let mut reasons: Vec<String> = Vec::new();
    for r in s.ids.iter().filter_map(|id| app.stale.get(id)).flatten() {
        if !reasons.contains(r) {
            reasons.push(r.clone());
        }
    }
    let what = if reasons.is_empty() { "The graph changed".to_string() } else { format!("The graph changed: {}", reasons.join("; ")) };
    let mut list = Column::new().spacing(8);
    for a in &analyses {
        let recalc = can_recalculate(app, a);
        let note: El = match &recalc {
            Ok(()) => space().into(),
            Err(e) => caption(e.clone()).size(11.5).into(),
        };
        list = list.push(
            container(
                row![
                    icon(Icon::Warning, 16.0, style::WARNING),
                    column![text(a.name.clone()).size(13), caption(analysis_summary(a)).size(11.5), note].spacing(2).width(Fill),
                    button(text("Recalculate").size(12))
                        .padding([4, 10])
                        .style(style::secondary)
                        .on_press_maybe(recalc.is_ok().then(|| Message::Dialog(DialogMsg::Recalculate(Some(a.id.clone()))))),
                    button(text("Set up again…").size(12)).padding([4, 10]).style(style::secondary).on_press(Message::Dialog(DialogMsg::SetUpAgain(a.id.clone()))),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            )
            .padding([8, 10])
            .style(style::card),
        );
    }
    let any_recalc = analyses.iter().any(|a| can_recalculate(app, a).is_ok());
    let body = column![
        yellow_box(text(what).size(13).into()),
        text(format!(
            "{} set up on what the graph showed before. Until you decide, {} hidden from the graph and the results.",
            if analyses.len() == 1 { "This analysis was" } else { "These analyses were" },
            if analyses.len() == 1 { "it is" } else { "they are" }
        ))
        .size(13),
        list,
        caption("Recalculate reruns the same test on the bars shown now (every bar for ANOVA, the two bars shown for a t test). Set up again lets you pick the data sets and the test.").size(12),
        row![
            button(text("Undo the change").size(13)).padding([5, 14]).style(style::secondary).on_press(Message::Dialog(DialogMsg::UndoChange)),
            space::horizontal(),
            button(text("Not now").size(13)).padding([5, 14]).style(style::secondary).on_press(Message::Dialog(DialogMsg::Cancel)),
            button(text(if analyses.len() == 1 { "Recalculate" } else { "Recalculate all" }).size(13))
                .padding([5, 18])
                .style(style::primary)
                .on_press_maybe(any_recalc.then_some(Message::Dialog(DialogMsg::Recalculate(None)))),
        ]
        .spacing(10),
    ]
    .spacing(12);
    frame("Statistics no longer match the graph".into(), body.into(), 600.0)
}

fn analyze_view<'a>(app: &'a App, a: &'a AnalyzeState) -> El<'a> {
    // Left: which analysis (searchable tree).
    let q = a.search.to_lowercase();
    let matches = |l: &str| q.is_empty() || l.to_lowercase().contains(&q);
    let item = |m: Method| -> El<'a> {
        let selected = a.method == m;
        button(text(m.label()).size(13))
            .width(Fill)
            .padding([3, 8])
            .style(style::nav_item(selected))
            .on_press(Message::Dialog(DialogMsg::PickMethod(m)))
            .into()
    };
    let category = |name: &'a str| -> El<'a> {
        row![icon(Icon::ChevronDown, 11.0, style::MUTED), heading(name)].spacing(6).align_y(Alignment::Center).into()
    };
    let mut tree = Column::new().spacing(3);
    let column_items: Vec<Method> = [Method::TTest, Method::OneWayAnova, Method::Descriptive].into_iter().filter(|m| matches(m.label())).collect();
    if !column_items.is_empty() {
        tree = tree.push(category("Column analyses"));
        for m in column_items {
            tree = tree.push(container(item(m)).padding(iced::Padding { left: 18.0, ..Default::default() }));
        }
    }
    let groups: [(&str, Vec<Method>); 5] = [
        ("XY analyses", vec![Method::CurveFit, Method::LinearRegression, Method::Correlation, Method::Auc]),
        ("Grouped analyses", vec![Method::TwoWayAnova]),
        ("Survival", vec![Method::Survival]),
        ("Contingency tables", vec![Method::Contingency]),
        ("Diagnostics", vec![Method::Normality, Method::Outliers]),
    ];
    for (title, methods) in groups {
        let items: Vec<Method> = methods.into_iter().filter(|m| matches(m.label())).collect();
        if !items.is_empty() {
            tree = tree.push(category(title));
            for m in items {
                tree = tree.push(container(item(m)).padding(iced::Padding { left: 18.0, ..Default::default() }));
            }
        }
    }
    let later = ["Two-way repeated measures / mixed model", "Three-way ANOVA (or mixed model)", "Nested one-way ANOVA"];
    let later: Vec<&str> = later.into_iter().filter(|l| matches(l)).collect();
    if !later.is_empty() {
        tree = tree.push(category("Coming later"));
        for l in later {
            tree = tree.push(container(caption(l).size(13)).padding(iced::Padding { left: 26.0, top: 3.0, bottom: 3.0, ..Default::default() }));
        }
    }
    let search = row![
        text_input("Search analyses", &a.search).on_input(|s| Message::Dialog(DialogMsg::Search(s))).size(13).padding([5, 8]),
        crate::view::close_x(Message::Dialog(DialogMsg::Search(String::new()))),
    ]
    .spacing(4)
    .align_y(Alignment::Center);
    let left = column![
        text("Which analysis?").size(13),
        search,
        container(scrollable(tree).height(Fill)).padding(8).height(360).width(Fill).style(style::card),
    ]
    .spacing(8)
    .width(Fill);

    // Right: which data sets (bars of a graph, or the data sets of an XY graph).
    let graphs = analysis_graphs(app, a.method.shape());
    let choices: Vec<GraphChoice> = graphs.iter().map(|(id, name)| GraphChoice { id: id.clone(), name: name.clone() }).collect();
    let selected = choices.iter().find(|c| Some(&c.id) == a.graph.as_ref()).cloned();
    let graph_pick = pick_list(choices, selected, |c: GraphChoice| Message::Dialog(DialogMsg::PickGraph(c.id))).text_size(13).width(Fill);
    let mut list = Column::new().spacing(5);
    let sets = app.data_sets(a.graph.as_deref().unwrap_or_default());
    if graphs.is_empty() {
        list = list.push(caption(if a.method.xy() {
            "No XY graph yet. Create one first: its data sets are analysed."
        } else if a.method == Method::Survival {
            "No survival graph yet. Create one first (X = time, Y = event 1 / censored 0, Group by = curves)."
        } else if a.method == Method::Contingency {
            "No contingency table yet. Create one first (rows = X, columns = Group by or several Y columns, cells = counts)."
        } else {
            "No column or grouped graph yet. Create one first: its bars are the data sets."
        }));
    }
    for (i, (label, n)) in sets.iter().enumerate() {
        let checked = a.checked.contains(label);
        let l = label.clone();
        list = list.push(
            checkbox(checked)
                .label(format!("{}: {label}   (n={n})", letter(i)))
                .size(15)
                .text_size(13)
                .on_toggle(move |on| Message::Dialog(DialogMsg::Toggle(l.clone(), on))),
        );
    }
    let quick = a.graph.as_ref().and_then(|g| app.engine.project.graph(g)).is_some_and(|g| !g.is_final()) && a.method != Method::Descriptive;
    let where_note: El = if quick {
        caption("OK makes a final graph from this quick graph (on a locked copy of its data) and runs the statistics there.").size(12).into()
    } else {
        space().into()
    };
    let right = column![
        text("Analyze which data sets?").size(13),
        row![caption("Graph:").size(13), graph_pick].spacing(8).align_y(Alignment::Center),
        where_note,
        container(scrollable(list).height(Fill)).padding(8).height(if quick { 300 } else { 324 }).width(Fill).style(style::card),
        row![
            button(text("Select All").size(12.5)).width(Fill).padding([5, 8]).style(style::secondary).on_press(Message::Dialog(DialogMsg::All(true))),
            button(text("Deselect All").size(12.5)).width(Fill).padding([5, 8]).style(style::secondary).on_press(Message::Dialog(DialogMsg::All(false))),
        ]
        .spacing(8),
    ]
    .spacing(8)
    .width(Fill);
    let use_row = row![
        space::horizontal(),
        caption("Use:").size(13),
        container(text("Built-in analysis").size(13)).padding([4, 10]).width(220).style(style::card),
        space::horizontal()
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    let mut body = Column::new().spacing(14);
    if let Some(name) = a.replacing.as_ref().and_then(|id| app.engine.project.analyses.iter().find(|x| &x.id == id)).map(|x| x.name.clone()) {
        body = body.push(yellow_box(
            text(format!("Setting up \"{name}\" again for what the graph shows now. Its other parameters are kept unless you pick another analysis."))
                .size(13)
                .into(),
        ));
    }
    let body = body.push(use_row).push(row![left, right].spacing(16)).push(footer(Message::Dialog(DialogMsg::AnalyzeOk), a.error.as_deref()));
    frame("Analyze Data".into(), body.into(), 700.0)
}

#[derive(Clone, Debug, PartialEq)]
struct GraphChoice {
    id: String,
    name: String,
}

impl std::fmt::Display for GraphChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

/// A labelled pick-list value.
#[derive(Clone, Debug, PartialEq)]
struct Choice<T> {
    value: T,
    label: String,
}

impl<T> std::fmt::Display for Choice<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

fn tabs<'a>(current: Tab, list: &[(Tab, &'a str, bool)]) -> El<'a> {
    row(list.iter().map(|&(t, label, enabled)| {
        let selected = t == current;
        button(text(label).size(12.5))
            .padding([5, 10])
            .style(move |th, st| {
                let mut s = style::tile(selected)(th, st);
                if !enabled {
                    s.text_color = style::MUTED;
                }
                s
            })
            .on_press_maybe(enabled.then_some(Message::Dialog(DialogMsg::Tab(t))))
            .into()
    }))
    .spacing(4)
    .into()
}

/// Columns that can identify matched subjects (factors and text IDs).
fn match_columns(app: &App, graph_id: &str) -> Vec<Choice<String>> {
    let Some(g) = app.engine.project.graph(graph_id) else { return Vec::new() };
    let Some(t) = app.engine.project.table_by_id(&g.table_id) else { return Vec::new() };
    t.columns.iter().filter(|c| c.kind() != ColumnKind::Numeric).map(|c| Choice { value: c.name.clone(), label: c.name.clone() }).collect()
}

fn match_by_row<'a>(app: &'a App, graph_id: &str, current: &Option<String>, enabled: bool) -> El<'a> {
    let options = match_columns(app, graph_id);
    let selected = options.iter().find(|c| Some(&c.value) == current.as_ref()).cloned();
    let pick: El = if enabled {
        pick_list(options, selected, |c: Choice<String>| msg(Change::MatchBy(c.value))).placeholder("choose a column").text_size(12.5).into()
    } else {
        caption("—").into()
    };
    container(row![caption("Matched by:").size(12.5), pick].spacing(8).align_y(Alignment::Center))
        .padding(iced::Padding { left: 26.0, ..Default::default() })
        .into()
}

fn illustration<'a>(matched: bool) -> El<'a> {
    let src = if matched { ILLUSTRATION_MATCHED } else { ILLUSTRATION_UNMATCHED };
    container(svg(svg::Handle::from_memory(src.as_bytes())).width(330).height(118))
        .padding(iced::Padding { left: 26.0, ..Default::default() })
        .into()
}

fn distribution_section<'a>(dist: Distribution, means: &'a str) -> El<'a> {
    column![
        heading("Distribution assumption"),
        radio("Normal (Gaussian)", Distribution::Normal, Some(dist), |d| msg(Change::Dist(d))).size(15).text_size(13),
        desc(if means == "means" { "Assume sampling from normal distributions. Compare means." } else { "Assume sampling from normal distribution. Compare means." }),
        radio("Lognormal", Distribution::Lognormal, Some(dist), |d| msg(Change::Dist(d))).size(15).text_size(13),
        desc("Assume sampling from lognormal distributions. Compare geometric means."),
        radio("Nonparametric", Distribution::Nonparametric, Some(dist), |d| msg(Change::Dist(d))).size(15).text_size(13),
        desc("No assumption. Use a nonparametric test. Compare ranks."),
    ]
    .spacing(5)
    .into()
}

fn output_section<'a>(digits: u8, style_: PStyle) -> El<'a> {
    let digit_opts: Vec<u8> = (2..=8).collect();
    column![
        heading("Output"),
        row![
            caption("Show this many significant digits (for everything except P values):").size(12.5),
            pick_list(digit_opts, Some(digits), |d| msg(Change::Digits(d))).text_size(12.5).width(64)
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        row![
            caption("P value style:").size(12.5),
            pick_list(PStyle::ALL.to_vec(), Some(style_), |s| msg(Change::PStyle(s))).text_size(12).width(Fill)
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    ]
    .spacing(6)
    .into()
}

fn graphing_section<'a>(show: bool, show_ns: bool, extra: Vec<&'a str>) -> El<'a> {
    let mut col = column![
        heading("Graphing options"),
        checkbox(show).label("Show significance on the graph (brackets with stars)").size(15).text_size(13).on_toggle(|v| msg(Change::ShowOnGraph(v))),
        checkbox(show_ns)
            .label("Also show non-significant comparisons (ns)")
            .size(15)
            .text_size(13)
            .on_toggle_maybe(show.then_some(|v| msg(Change::ShowNs(v)))),
    ]
    .spacing(5);
    for e in extra {
        col = col.push(checkbox(false).label(e).size(15).text_size(13));
    }
    col.into()
}

fn params_view<'a>(app: &'a App, p: &'a ParamsState) -> El<'a> {
    match &p.params {
        Params::TTest(s) => ttest_view(app, p, s),
        Params::Anova(s) => anova_view(app, p, s),
        Params::Fit(s) => fit_view(app, p, s),
        Params::LinReg(s) => linreg_view(app, p, s),
        Params::Corr(s) => corr_view(app, p, s),
        Params::TwoWay(s) => two_way_view(app, p, s),
        Params::Auc(s) => auc_view(app, p, s),
        Params::Survival(s) => survival_view(app, p, s),
        Params::Contingency(s) => contingency_view(app, p, s),
        Params::Normality(s) => normality_view(app, p, s),
        Params::Outliers(s) => outliers_view(app, p, s),
    }
}

/// Prism's "Normality and Lognormality Tests" on the bars of a graph.
fn normality_view<'a>(app: &'a App, p: &'a ParamsState, s: &'a NormalitySpec) -> El<'a> {
    let item = |name: &'static str, on: bool, label: &'a str| -> El<'a> {
        checkbox(on).label(label).size(15).text_size(13).on_toggle(move |v| msg(Change::Normality(name, v))).into()
    };
    let content = column![
        heading("Data sets to test"),
        container(xy_sets_list(app, &s.graph_id, &s.data_sets)).padding(8).width(Fill).style(style::card),
        heading("Tests"),
        item("dagostino", s.tests.dagostino, "D'Agostino-Pearson omnibus K2 (n >= 8)"),
        item("shapiro", s.tests.shapiro, "Shapiro-Wilk (3 <= n <= 5000)"),
        item("anderson", s.tests.anderson, "Anderson-Darling (n >= 8)"),
        item("ks", s.tests.ks, "Kolmogorov-Smirnov with Lilliefors' correction (n >= 5)"),
        caption("With none ticked, Otto runs D'Agostino-Pearson and Shapiro-Wilk.").size(12),
        heading("Lognormality"),
        checkbox(s.lognormal)
            .label("Also test log10 of the values (lognormality)")
            .size(15)
            .text_size(13)
            .on_toggle(|v| msg(Change::Lognormal(v))),
        caption("Values of zero or less are left out of the lognormality test.").size(12),
        output_section(s.output.digits, s.output.p_style),
    ]
    .spacing(8);
    let body = column![
        graph_note(app, &s.graph_id),
        container(scrollable(container(content).padding([4, 4])).height(Length::Shrink)).max_height(520.0),
        yellow_box(text("Otto will perform:\n   - Normality tests of each data set in R (stats, nortest).").size(13).into()),
        footer(Message::Dialog(DialogMsg::ParamsOk), p.error.as_deref())
    ]
    .spacing(12);
    frame("Parameters: Normality and Lognormality Tests".into(), body.into(), 600.0)
}

/// Prism's "Identify outliers" on the bars of a graph.
fn outliers_view<'a>(app: &'a App, p: &'a ParamsState, s: &'a OutlierSpec) -> El<'a> {
    let method = pick_list(OutlierMethod::ALL, Some(s.method), |m| msg(Change::OutlierMethod(m))).text_size(13).width(Fill);
    let mut content = column![
        heading("Data sets to examine"),
        container(xy_sets_list(app, &s.graph_id, &s.data_sets)).padding(8).width(Fill).style(style::card),
        heading("Method"),
        method,
    ]
    .spacing(8);
    if s.method != OutlierMethod::Tukey {
        let alphas: Vec<Choice<u16>> =
            [(10u16, "0.01"), (50, "0.05"), (100, "0.10")].iter().map(|&(v, l)| Choice { value: v, label: l.to_string() }).collect();
        let sel = alphas.iter().find(|c| c.value as f64 == (s.alpha * 1000.0).round()).cloned();
        content = content
            .push(heading("Significance level"))
            .push(pick_list(alphas, sel, |c: Choice<u16>| msg(Change::Alpha(c.value))).text_size(13).width(Fill));
    }
    if s.method == OutlierMethod::Esd {
        let counts: Vec<usize> = (1..=10).collect();
        content = content
            .push(heading("At most how many outliers per data set?"))
            .push(pick_list(counts, Some(s.max_outliers), |v| msg(Change::MaxOutliers(v))).text_size(13).width(Fill));
    }
    content = content
        .push(caption("Otto only reports outliers; it never removes them from the data. Decide yourself whether a flagged value is a mistake or real biology.").size(12))
        .push(output_section(s.output.digits, s.output.p_style));
    let body = column![
        graph_note(app, &s.graph_id),
        container(scrollable(container(content).padding([4, 4])).height(Length::Shrink)).max_height(520.0),
        yellow_box(text("Otto will perform:\n   - Outlier identification in each data set in R (Grubbs / ESD or Tukey's rule).").size(13).into()),
        footer(Message::Dialog(DialogMsg::ParamsOk), p.error.as_deref())
    ]
    .spacing(12);
    frame("Parameters: Identify Outliers".into(), body.into(), 600.0)
}

/// "Graph: ..." above a parameters dialog, saying when OK will make the final graph.
fn graph_note<'a>(app: &'a App, graph_id: &str) -> El<'a> {
    let name = app.engine.project.graph(graph_id).map(|g| g.name.clone()).unwrap_or_default();
    if app.engine.project.graph(graph_id).is_some_and(|g| !g.is_final()) {
        caption(format!("OK makes a final graph from \"{name}\" (on a locked copy of its data) with the analysis.")).size(12).into()
    } else {
        caption(format!("Graph: \"{name}\"")).size(12).into()
    }
}

fn two_way_view<'a>(app: &'a App, p: &'a ParamsState, s: &'a TwoWaySpec) -> El<'a> {
    let tab_bar = tabs(p.tab, &[(Tab::Design, "Model", true), (Tab::Comparisons, "Multiple Comparisons", true), (Tab::Options, "Options", true)]);
    let content: El = match p.tab {
        Tab::Comparisons => {
            let mut col = column![heading("Which comparisons?")].spacing(6);
            for c in TwoWayCompare::ALL {
                col = col.push(radio(two_way_compare_label(c), c, Some(s.compare), |v| msg(Change::TwoWayCompare(v))).size(15).text_size(13));
            }
            col.push(caption("Comparisons use the residual mean square of the ANOVA. Comparisons between two bars are drawn on the graph as brackets.").size(12)).into()
        }
        Tab::Options => {
            let alphas: Vec<Choice<u16>> = [(50u16, "0.05 (95% confidence interval)"), (10, "0.01 (99% confidence interval)"), (100, "0.10 (90% confidence interval)")]
                .iter()
                .map(|&(v, l)| Choice { value: v, label: l.to_string() })
                .collect();
            let alpha_sel = alphas.iter().find(|c| c.value as f64 == (s.alpha * 1000.0).round()).cloned();
            let corr = |c: Correction, label: &'a str| -> El<'a> { radio(label, c, Some(s.correction), |c| msg(Change::Correction(c))).size(15).text_size(13).into() };
            column![
                heading("Multiple comparisons test"),
                corr(Correction::Hypothesis, "Correct for multiple comparisons using statistical hypothesis testing (Tukey; Šídák when one comparison per family). Recommended."),
                corr(Correction::Fdr, "Correct for multiple comparisons by controlling the False Discovery Rate."),
                corr(Correction::None, "Don't correct for multiple comparisons (Fisher's LSD)."),
                row![caption("Family-wise alpha threshold and confidence level").size(12.5), pick_list(alphas, alpha_sel, |c: Choice<u16>| msg(Change::Alpha(c.value))).text_size(12.5).width(Fill)]
                    .spacing(8)
                    .align_y(Alignment::Center),
                graphing_section(s.graph.show, s.graph.show_ns, Vec::new()),
                heading("Additional results"),
                checkbox(s.results.descriptive).label("Descriptive statistics for each bar").size(15).text_size(13).on_toggle(|v| msg(Change::Descriptive(v))),
                output_section(s.output.digits, s.output.p_style),
            ]
            .spacing(6)
            .into()
        }
        _ => column![
            heading("Experimental design"),
            caption("Rows: the X categories of the grouped graph. Columns: its data sets (Group by, or several Y columns). Each bar is one combination; its values are the replicates.").size(13),
            heading("Model"),
            radio("Full model, with the interaction (recommended)", true, Some(s.interaction), |v| msg(Change::Interaction(v))).size(15).text_size(13),
            radio("Main effects only (no interaction)", false, Some(s.interaction), |v| msg(Change::Interaction(v))).size(15).text_size(13),
            caption("Type III sums of squares (car::Anova), so unequal numbers of replicates are handled as in Prism. Repeated measures / mixed models will come later.").size(12),
            heading("Bars to include"),
            container(xy_sets_list(app, &s.graph_id, &s.data_sets)).padding(8).width(Fill).style(style::card),
        ]
        .spacing(6)
        .into(),
    };
    let perform = format!(
        "Otto will perform:\n   - Ordinary two-way ANOVA ({}).\n   - {}.",
        if s.interaction { "full model" } else { "main effects only" },
        two_way_compare_label(s.compare)
    );
    let body = column![
        tab_bar,
        container(scrollable(container(content).padding([4, 4])).height(Length::Shrink)).max_height(470.0),
        yellow_box(text(perform).size(13).into()),
        footer(Message::Dialog(DialogMsg::ParamsOk), p.error.as_deref())
    ]
    .spacing(12);
    frame("Parameters: Two-way ANOVA".into(), body.into(), 640.0)
}

fn auc_view<'a>(app: &'a App, p: &'a ParamsState, s: &'a AucSpec) -> El<'a> {
    let content = column![
        heading("Data sets"),
        container(xy_sets_list(app, &s.graph_id, &s.data_sets)).padding(8).width(Fill).style(style::card),
        heading("Baseline"),
        row![
            caption("Baseline Y =").size(13),
            text_input("0", &p.baseline_text).on_input(|t| msg(Change::Baseline(t))).size(13).padding([4, 8]).width(120)
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        heading("Peaks"),
        row![
            caption("Ignore peaks that are less than").size(13),
            text_input("10", &p.min_peak_text).on_input(|t| msg(Change::MinPeak(t))).size(13).padding([4, 8]).width(70),
            caption("% of the distance from minimum to maximum Y").size(13),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        checkbox(s.peaks_below).label("Also list peaks below the baseline").size(15).text_size(13).on_toggle(|v| msg(Change::PeaksBelow(v))),
        heading("Confidence interval of the total area"),
        confidence_pick(s.confidence),
        caption("From the SEM of the replicates at each X (needs replicates at every X).").size(12),
        output_section(s.output.digits, s.output.p_style),
    ]
    .spacing(8);
    let body = column![
        container(scrollable(container(content).padding([4, 4])).height(Length::Shrink)).max_height(520.0),
        yellow_box(text("Otto will compute:\n   - The area under each curve (trapezoid rule on the mean Y at each X), its SE and CI, and the peaks (R).").size(13).into()),
        footer(Message::Dialog(DialogMsg::ParamsOk), p.error.as_deref())
    ]
    .spacing(12);
    frame("Parameters: Area Under Curve".into(), body.into(), 600.0)
}

fn contingency_view<'a>(app: &'a App, p: &'a ParamsState, s: &'a ContingencySpec) -> El<'a> {
    let graph_name = app.engine.project.graph(&s.graph_id).map(|g| g.name.clone()).unwrap_or_default();
    let quick = app.engine.project.graph(&s.graph_id).is_some_and(|g| !g.is_final());
    let note: El = if quick {
        caption(format!("OK makes a final graph from \"{graph_name}\" (on a locked copy of its data) with the analysis.")).size(12).into()
    } else {
        caption(format!("Graph: \"{graph_name}\"")).size(12).into()
    };
    let content = column![
        heading("Columns to analyse"),
        container(xy_sets_list(app, &s.graph_id, &s.data_sets)).padding(8).width(Fill).style(style::card),
        caption("Tick two of them to pull a 2 x 2 table out of a wider one: that is what the relative risk and odds ratio need.").size(12),
        heading("Experimental design"),
        checkbox(s.paired).label("Paired: the same subjects counted twice (adds McNemar's test)").size(15).text_size(13).on_toggle(|v| msg(Change::Paired(v))),
        heading("Confidence intervals"),
        confidence_pick(s.confidence),
        caption("For the relative risk, odds ratio, difference between fractions and the diagnostic ratios.").size(12),
        heading("Results"),
        checkbox(s.expected).label("Report the expected counts the chi-square test assumes").size(15).text_size(13).on_toggle(|v| msg(Change::Expected(v))),
        output_section(s.output.digits, s.output.p_style),
    ]
    .spacing(8);
    let body = column![
        note,
        container(scrollable(container(content).padding([4, 4])).height(Length::Shrink)).max_height(520.0),
        yellow_box(text("Otto will perform:\n   - Chi-square test, with Yates' correction on a 2 x 2 table.\n   - Fisher's exact test.\n   - The chi-square test for trend, where one side has three or more ordered categories.\n   - On a 2 x 2 table: relative risk, odds ratio, difference between fractions and the diagnostic ratios.").size(13).into()),
        footer(Message::Dialog(DialogMsg::ParamsOk), p.error.as_deref())
    ]
    .spacing(12);
    frame("Parameters: Contingency Table".into(), body.into(), 600.0)
}

fn survival_view<'a>(app: &'a App, p: &'a ParamsState, s: &'a SurvivalSpec) -> El<'a> {
    let graph_name = app.engine.project.graph(&s.graph_id).map(|g| g.name.clone()).unwrap_or_default();
    let quick = app.engine.project.graph(&s.graph_id).is_some_and(|g| !g.is_final());
    let note: El = if quick {
        caption(format!("OK makes a final graph from \"{graph_name}\" (on a locked copy of its data) with the analysis.")).size(12).into()
    } else {
        caption(format!("Graph: \"{graph_name}\"")).size(12).into()
    };
    let content = column![
        heading("Curves to compare"),
        container(xy_sets_list(app, &s.graph_id, &s.data_sets)).padding(8).width(Fill).style(style::card),
        heading("Comparisons"),
        caption("Log-rank (Mantel-Cox) test and Gehan-Breslow-Wilcoxon test (more weight to early events); the log-rank test for trend with three or more curves; hazard ratios with two curves.").size(12.5),
        heading("Confidence intervals"),
        confidence_pick(s.confidence),
        caption("Asymmetrical intervals (log-log transformation) for survival proportions and medians.").size(12),
        heading("Results"),
        checkbox(s.proportions).label("Report the survival proportions at every time (with SE and CI)").size(15).text_size(13).on_toggle(|v| msg(Change::Proportions(v))),
        output_section(s.output.digits, s.output.p_style),
    ]
    .spacing(8);
    let body = column![
        note,
        container(scrollable(container(content).padding([4, 4])).height(Length::Shrink)).max_height(520.0),
        yellow_box(text("Otto will perform:\n   - Kaplan-Meier survival analysis with R's survival package (survfit, survdiff).").size(13).into()),
        footer(Message::Dialog(DialogMsg::ParamsOk), p.error.as_deref())
    ]
    .spacing(12);
    frame("Parameters: Survival Analysis (Kaplan-Meier)".into(), body.into(), 600.0)
}

/// Prism's Residuals tab: normality tests of the residuals.
fn residuals_tab<'a>(n: &NormalityTests, rank_based: bool, what: &'a str) -> El<'a> {
    if rank_based {
        return column![heading("Normality of residuals"), caption("Nonparametric tests make no assumption about the distribution, so there are no residuals to test.").size(13)]
            .spacing(8)
            .into();
    }
    let item = |name: &'static str, on: bool, label: &'a str| -> El<'a> {
        checkbox(on).label(label).size(15).text_size(13).on_toggle(move |v| msg(Change::Normality(name, v))).into()
    };
    column![
        heading("Diagnostics for residuals"),
        caption(format!("Is the distribution of {what} consistent with a Gaussian distribution?")).size(13),
        item("dagostino", n.dagostino, "D'Agostino-Pearson omnibus normality test (n ≥ 8)"),
        item("anderson", n.anderson, "Anderson-Darling test (n ≥ 8)"),
        item("shapiro", n.shapiro, "Shapiro-Wilk normality test"),
        item("ks", n.ks, "Kolmogorov-Smirnov normality test with Dallal-Wilkinson-Lilliefors P value (n ≥ 5)"),
        caption("Computed by R (stats::shapiro.test, nortest::ad.test, nortest::lillie.test; D'Agostino-Pearson K² after D'Agostino et al. 1990).").size(12),
    ]
    .spacing(6)
    .into()
}

/// The data sets of an XY analysis, as checkboxes (none chosen = all).
fn xy_sets_list<'a>(app: &'a App, graph: &str, chosen: &[String]) -> El<'a> {
    let sets = app.data_sets(graph);
    let mut list = Column::new().spacing(5);
    if sets.is_empty() {
        list = list.push(caption("This graph shows no data sets.").size(12.5));
    }
    for (i, (label, n)) in sets.iter().enumerate() {
        let on = chosen.is_empty() || chosen.contains(label);
        let l = label.clone();
        list = list.push(
            checkbox(on)
                .label(format!("{}: {label}  (n={n})", letter(i)))
                .size(14)
                .text_size(12.5)
                .on_toggle(move |v| Message::Dialog(DialogMsg::FitSet(l.clone(), v))),
        );
    }
    list.into()
}

fn confidence_pick<'a>(confidence: f64) -> El<'a> {
    let confs: Vec<Choice<u8>> = [90u8, 95, 99].iter().map(|&c| Choice { value: c, label: format!("{c}%") }).collect();
    let sel = confs.iter().find(|c| c.value as f64 == (confidence * 100.0).round()).cloned();
    row![caption("Confidence level:").size(13), pick_list(confs, sel, |c: Choice<u8>| msg(Change::Confidence(c.value))).text_size(13).width(90)]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
}

/// A small drawing of a model's curve shape (like the pictures in Prism's equation list).
fn model_picture<'a>(m: FitModel, selected: bool) -> El<'a> {
    let (w, h) = (54.0, 34.0);
    let pts: Vec<String> = (0..=48)
        .map(|i| {
            let t = i as f64 / 48.0;
            format!("{:.1},{:.1}", 2.0 + t * (w - 4.0), h - 2.0 - m.shape(t).clamp(0.0, 1.0) * (h - 4.0))
        })
        .collect();
    let stroke = if selected { "#7AB0FF" } else { "#C9CCD2" };
    let svg_src = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}"><path d="M1.5 1V{b}H{r}" fill="none" stroke="#6B6F76" stroke-width="1"/><polyline points="{pts}" fill="none" stroke="{stroke}" stroke-width="1.8" stroke-linejoin="round"/></svg>"##,
        b = h - 1.5,
        r = w - 1.0,
        pts = pts.join(" ")
    );
    svg(svg::Handle::from_memory(svg_src.into_bytes())).width(w as f32).height(h as f32).into()
}

fn fit_view<'a>(app: &'a App, p: &'a ParamsState, s: &'a FitSpec) -> El<'a> {
    let tab_bar = tabs(p.tab, &[(Tab::Model, "Model", true), (Tab::Constraints, "Constraints", true), (Tab::Options, "Options", true)]);
    let graph_name = app.engine.project.graph(&s.graph_id).map(|g| g.name.clone()).unwrap_or_default();
    let quick = app.engine.project.graph(&s.graph_id).is_some_and(|g| !g.is_final());
    let content: El = match p.tab {
        Tab::Constraints => {
            let mut col = column![
                heading("Constrain parameters"),
                caption("Tick a parameter to hold it at a constant value instead of fitting it (e.g. Bottom = 0, Top = 100).").size(12.5),
            ]
            .spacing(8);
            for &name in s.model.params() {
                let fixed = p.fixed_text.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone());
                let on = fixed.is_some();
                let n = name.to_string();
                let value: El = match fixed {
                    Some(v) => {
                        let n2 = n.clone();
                        text_input("value", &v).on_input(move |t| msg(Change::FixValue(n2.clone(), t))).size(13).padding([4, 8]).width(120).into()
                    }
                    None => caption("fitted").size(12.5).into(),
                };
                col = col.push(
                    row![
                        container(text(name).size(13)).width(110),
                        checkbox(on).label("Constant equal to").size(15).text_size(13).on_toggle(move |v| msg(Change::Fix(n.clone(), v))),
                        value,
                    ]
                    .spacing(12)
                    .align_y(Alignment::Center),
                );
            }
            col.into()
        }
        Tab::Options => column![
            heading("Confidence intervals"),
            confidence_pick(s.confidence),
            caption("Asymptotic (Wald) intervals from the fit's covariance matrix, with Student's t on the residual degrees of freedom; EC50 / IC50 and half-lives are transformed from the intervals of their parameters.").size(12),
            heading("Graphing options"),
            checkbox(s.graph.show).label("Draw the fitted curve on the graph").size(15).text_size(13).on_toggle(|v| msg(Change::ShowCurve(v))),
            output_section(s.output.digits, s.output.p_style),
        ]
        .spacing(8)
        .into(),
        _ => {
            // Left: families. Middle: the family's models with a drawing each. Right: data sets.
            let mut families = Column::new().spacing(3);
            for f in FitFamily::ALL {
                families = families.push(
                    button(text(f.label()).size(12.5)).width(Fill).padding([4, 8]).style(style::nav_item(p.family == f)).on_press(msg(Change::Family(f))),
                );
            }
            let mut models = Column::new().spacing(4);
            for m in p.family.models() {
                let selected = s.model == m;
                models = models.push(
                    button(
                        row![model_picture(m, selected), column![text(m.label()).size(12.5), caption(m.params().join(", ")).size(11)].spacing(2)]
                            .spacing(10)
                            .align_y(Alignment::Center),
                    )
                    .width(Fill)
                    .padding([4, 8])
                    .style(style::tile(selected))
                    .on_press(msg(Change::Model(m))),
                );
            }
            let log_x = matches!(
                s.model,
                FitModel::Agonist1pl | FitModel::Agonist2pl | FitModel::Agonist3pl | FitModel::Agonist4pl | FitModel::Agonist5pl
                    | FitModel::Inhibitor1pl | FitModel::Inhibitor2pl | FitModel::Inhibitor3pl | FitModel::Inhibitor4pl | FitModel::Inhibitor5pl
            );
            let mut eq = column![caption(s.model.equation()).size(12)].spacing(4);
            if log_x {
                eq = eq.push(caption("X must be log10 of the concentration (use the [agonist] / [inhibitor] models for concentrations).").size(11.5));
            }
            column![
                row![
                    column![caption("Model family").size(12), container(scrollable(families)).padding(6).height(330).width(Fill).style(style::card)]
                        .spacing(6)
                        .width(200),
                    column![caption("Equation").size(12), container(scrollable(models)).padding(6).height(330).width(Fill).style(style::card)].spacing(6).width(Fill),
                    column![
                        caption("Fit which data sets?").size(12),
                        container(scrollable(xy_sets_list(app, &s.graph_id, &s.data_sets))).padding(8).height(330).width(Fill).style(style::card)
                    ]
                    .spacing(6)
                    .width(200),
                ]
                .spacing(12),
                eq,
            ]
            .spacing(8)
            .into()
        }
    };
    let constraints: Vec<String> = p.fixed_text.iter().filter(|(n, _)| s.model.params().contains(&n.as_str())).map(|(n, v)| format!("{n} = {v}")).collect();
    let mut perform = format!("Otto will fit:\n   - {} ({}).", s.model.label(), s.model.equation());
    if !constraints.is_empty() {
        perform.push_str(&format!("\n   - Constraints: {}.", constraints.join(", ")));
    }
    perform.push_str("\n   - Least squares in R (minpack.lm), one fit per data set.");
    let note: El = if quick {
        caption(format!("OK makes a final graph from \"{graph_name}\" (on a locked copy of its data) with the fitted curves.")).size(12).into()
    } else {
        caption(format!("Graph: \"{graph_name}\"")).size(12).into()
    };
    let body = column![
        note,
        tab_bar,
        container(scrollable(container(content).padding([4, 4])).height(Length::Shrink)).max_height(470.0),
        yellow_box(text(perform).size(13).into()),
        footer(Message::Dialog(DialogMsg::ParamsOk), p.error.as_deref())
    ]
    .spacing(12);
    frame("Fit curve: Nonlinear Regression".into(), body.into(), 900.0)
}

fn linreg_view<'a>(app: &'a App, p: &'a ParamsState, s: &'a LinRegSpec) -> El<'a> {
    let content = column![
        heading("Data sets"),
        container(xy_sets_list(app, &s.graph_id, &s.data_sets)).padding(8).width(Fill).style(style::card),
        heading("Calculations"),
        confidence_pick(s.confidence),
        heading("Graphing options"),
        checkbox(s.graph.show).label("Draw the regression line on the graph").size(15).text_size(13).on_toggle(|v| msg(Change::ShowCurve(v))),
        output_section(s.output.digits, s.output.p_style),
    ]
    .spacing(8);
    let body = column![
        container(scrollable(container(content).padding([4, 4])).height(Length::Shrink)).max_height(520.0),
        yellow_box(text("Otto will perform:\n   - Simple linear regression of Y on X for each data set (R: lm).").size(13).into()),
        footer(Message::Dialog(DialogMsg::ParamsOk), p.error.as_deref())
    ]
    .spacing(12);
    frame("Parameters: Simple Linear Regression".into(), body.into(), 560.0)
}

fn corr_view<'a>(app: &'a App, p: &'a ParamsState, s: &'a CorrelationSpec) -> El<'a> {
    let content = column![
        heading("Data sets"),
        container(xy_sets_list(app, &s.graph_id, &s.data_sets)).padding(8).width(Fill).style(style::card),
        heading("Correlation coefficient"),
        radio("Pearson correlation coefficients (assume data are sampled from Gaussian populations)", CorrMethod::Pearson, Some(s.method), |m| msg(Change::CorrMethod(m)))
            .size(15)
            .text_size(13),
        radio("Spearman nonparametric correlation", CorrMethod::Spearman, Some(s.method), |m| msg(Change::CorrMethod(m))).size(15).text_size(13),
        heading("P value"),
        row![
            radio("One-tailed", true, Some(s.one_tailed), |v| msg(Change::OneTailed(v))).size(15).text_size(13),
            radio("Two-tailed (recommended)", false, Some(s.one_tailed), |v| msg(Change::OneTailed(v))).size(15).text_size(13),
        ]
        .spacing(14),
        confidence_pick(s.confidence),
        output_section(s.output.digits, s.output.p_style),
    ]
    .spacing(8);
    let what = match s.method {
        CorrMethod::Pearson => "Pearson correlation of X and Y for each data set (R: cor.test)",
        CorrMethod::Spearman => "Spearman rank correlation of X and Y for each data set (R: cor.test)",
    };
    let body = column![
        container(scrollable(container(content).padding([4, 4])).height(Length::Shrink)).max_height(520.0),
        yellow_box(text(format!("Otto will perform:\n   - {what}.")).size(13).into()),
        footer(Message::Dialog(DialogMsg::ParamsOk), p.error.as_deref())
    ]
    .spacing(12);
    frame("Parameters: Correlation".into(), body.into(), 560.0)
}

/// R, Otto's R packages, their setup and an R path override.
fn engine_view<'a>(s: &'a EngineState) -> El<'a> {
    let st = &s.status;
    let setup = otto_r::setup_progress();
    let mut body = Column::new().spacing(12);
    body = body.push(
        caption("Otto runs every statistical test and curve fit in R and only lays out the results. Its packages are installed into Otto's own library from a dated CRAN snapshot, so everyone computes with the same versions; each result names the versions used.")
            .size(12.5),
    );
    let ready = st.ready();
    let (headline, color) = if ready {
        (format!("Ready: {}", st.summary()), style::SUCCESS)
    } else {
        (format!("Not ready: {}", st.summary()), style::WARNING)
    };
    body = body.push(text(headline).size(14).color(color));
    let r_line = match (&st.rscript, &st.r_version) {
        (Some(p), Some(v)) => format!("R {v} at {p}"),
        (Some(p), None) => format!("Rscript at {p} did not run"),
        (None, _) => "R was not found.".to_string(),
    };
    body = body.push(caption(r_line).size(12.5));
    if let Some(e) = &st.error {
        body = body.push(container(text(e.clone()).size(12.5)).padding(8).width(Fill).style(style::error_box));
    }
    if st.rscript.is_none() {
        body = body.push(yellow_box(
            column![
                text("Install R first (free, from the R Project). In a terminal:").size(13),
                text_input("", "winget install --id RProject.R -e --scope machine").size(13).padding([4, 8]),
                caption("or download it from https://cran.r-project.org. Then click Check again, and Set up.").size(12),
            ]
            .spacing(6)
            .into(),
        ));
    }
    if !st.packages.is_empty() {
        let mut pk = Column::new().spacing(2);
        for (name, version) in &st.packages {
            let need = if otto_r::PACKAGES.contains(&name.as_str()) { "" } else { "  (for cross-checks)" };
            let v = if version.is_empty() { "missing".to_string() } else { version.clone() };
            pk = pk.push(row![container(text(name.clone()).size(12.5)).width(120), caption(format!("{v}{need}")).size(12.5)].spacing(8));
        }
        body = body.push(column![heading("Packages"), container(pk).padding(8).width(Fill).style(style::card), caption(format!("Library: {}", st.library)).size(11.5)].spacing(6));
    }
    if setup.running || !setup.log.is_empty() {
        let tail: Vec<String> = setup.log.iter().rev().take(14).rev().cloned().collect();
        body = body.push(
            column![
                heading(if setup.running { "Installing… (a few minutes)" } else { "Last setup" }),
                container(scrollable(text(tail.join("\n")).size(11.5).font(iced::Font::MONOSPACE)).height(170)).padding(8).width(Fill).style(style::card),
            ]
            .spacing(6),
        );
    }
    body = body.push(
        column![
            heading("R location"),
            row![
                text_input("Found automatically (R's registry entry or C:\\Program Files\\R)", &s.path)
                    .on_input(|t| Message::Dialog(DialogMsg::EnginePath(t)))
                    .size(12.5)
                    .padding([4, 8]),
                button(text("Use").size(12.5)).padding([4, 12]).style(style::secondary).on_press(Message::Dialog(DialogMsg::EngineSavePath)),
            ]
            .spacing(8),
            caption("An R installation folder or Rscript.exe; leave empty to find R automatically.").size(11.5),
        ]
        .spacing(6),
    );
    body = body.push(
        row![
            button(text("Check again").size(13)).padding([5, 14]).style(style::secondary).on_press(Message::Dialog(DialogMsg::OpenEngine)),
            space::horizontal(),
            button(text("Close").size(13)).padding([5, 18]).style(style::secondary).on_press(Message::Dialog(DialogMsg::Cancel)),
            button(text(if ready { "Reinstall packages" } else { "Set up" }).size(13))
                .padding([5, 18])
                .style(style::primary)
                .on_press_maybe((st.rscript.is_some() && !setup.running).then_some(Message::Dialog(DialogMsg::EngineSetup))),
        ]
        .spacing(10),
    );
    frame("Statistics engine (R)".into(), scrollable(body).height(Length::Shrink).into(), 620.0)
}

fn ttest_view<'a>(app: &'a App, p: &'a ParamsState, s: &'a TTestSpec) -> El<'a> {
    let tab_bar = tabs(p.tab, &[(Tab::Design, "Experimental Design", true), (Tab::Residuals, "Residuals", true), (Tab::Options, "Options", true)]);
    let (a, b) = (s.data_sets.first().cloned().unwrap_or_default(), s.data_sets.get(1).cloned().unwrap_or_default());
    let content: El = match p.tab {
        Tab::Options => {
            let diffs = vec![
                Choice { value: DiffDirection::AMinusB, label: format!("{a} - {b}") },
                Choice { value: DiffDirection::BMinusA, label: format!("{b} - {a}") },
            ];
            let diff_sel = diffs.iter().find(|c| c.value == s.difference).cloned();
            let confs: Vec<Choice<u8>> = [90u8, 95, 99].iter().map(|&c| Choice { value: c, label: format!("{c}%") }).collect();
            let conf_sel = confs.iter().find(|c| c.value as f64 == (s.confidence * 100.0).round()).cloned();
            let alpha = format_alpha(1.0 - s.confidence);
            let paired = s.design == Pairing::Paired;
            column![
                heading("Calculations"),
                row![
                    caption("P value:").size(13),
                    radio("One-tailed", true, Some(s.one_tailed), |v| msg(Change::OneTailed(v))).size(15).text_size(13),
                    radio("Two-tailed (recommended)", false, Some(s.one_tailed), |v| msg(Change::OneTailed(v))).size(15).text_size(13),
                ]
                .spacing(14)
                .align_y(Alignment::Center),
                row![caption("Report differences as:").size(13), pick_list(diffs, diff_sel, |c: Choice<DiffDirection>| msg(Change::Difference(c.value))).text_size(13).width(Fill)]
                    .spacing(8)
                    .align_y(Alignment::Center),
                row![caption("Confidence level:").size(13), pick_list(confs, conf_sel, |c: Choice<u8>| msg(Change::Confidence(c.value))).text_size(13).width(90)]
                    .spacing(8)
                    .align_y(Alignment::Center),
                yellow_box(text(format!("Definition of statistical significance:  P < {alpha}")).size(13).into()),
                graphing_section(
                    s.graph.show,
                    s.graph.show_ns,
                    if paired { vec!["Graph differences (paired) — coming later", "Graph correlation (paired) — coming later"] } else { vec!["Graph CI of difference between means (estimation plot) — coming later"] }
                ),
                heading("Additional results"),
                checkbox(s.results.descriptive).label("Descriptive statistics for each data set").size(15).text_size(13).on_toggle(|v| msg(Change::Descriptive(v))),
                checkbox(s.results.aicc)
                    .label("t test: Also compare models using AICc")
                    .size(15)
                    .text_size(13)
                    .on_toggle_maybe((s.distribution != Distribution::Nonparametric).then_some(|v| msg(Change::Aicc(v)))),
                checkbox(s.results.effect_sizes)
                    .label("Report standardized effect sizes (Cohen's d, Hedges' g, Glass's Δ)")
                    .size(15)
                    .text_size(13)
                    .on_toggle_maybe((s.distribution != Distribution::Nonparametric).then_some(|v| msg(Change::Effect(v)))),
                output_section(s.output.digits, s.output.p_style),
            ]
            .spacing(8)
            .into()
        }
        Tab::Residuals => residuals_tab(
            &s.normality,
            s.distribution == Distribution::Nonparametric,
            if s.design == Pairing::Paired { "the differences between paired values" } else { "the residuals (each value minus its data set's mean)" },
        ),
        _ => {
            let paired = s.design == Pairing::Paired;
            let unpaired_param = !paired && s.distribution != Distribution::Nonparametric;
            let choose: El = if unpaired_param {
                column![
                    radio("Welch's t test. Do not assume equal SDs.", true, Some(s.welch), |v| msg(Change::Welch(v))).size(15).text_size(13),
                    radio("Unpaired t test. Assume both populations have the same SD.", false, Some(s.welch), |v| msg(Change::Welch(v))).size(15).text_size(13),
                ]
                .spacing(5)
                .into()
            } else {
                desc(s.test_name())
            };
            column![
                heading("Experimental design"),
                radio("Unpaired", false, Some(paired), |v| msg(Change::Paired(v))).size(15).text_size(13),
                radio("Paired", true, Some(paired), |v| msg(Change::Paired(v))).size(15).text_size(13),
                match_by_row(app, &s.graph_id, &s.match_by, paired),
                illustration(paired),
                distribution_section(s.distribution, "means"),
                heading("Choose test"),
                choose,
            ]
            .spacing(6)
            .into()
        }
    };
    let summary = yellow_box(text(format!("Based on your choices (on all tabs), Otto will perform:\n   - {}.", s.test_name())).size(13).into());
    let body = column![
        caption(format!("Data sets: {a}  vs.  {b}")).size(12.5),
        tab_bar,
        container(scrollable(container(content).padding([4, 4])).height(Length::Shrink)).max_height(470.0),
        summary,
        footer(Message::Dialog(DialogMsg::ParamsOk), p.error.as_deref())
    ]
    .spacing(12);
    frame("Parameters: t tests (and Nonparametric Tests)".into(), body.into(), 620.0)
}

fn format_alpha(a: f64) -> String {
    otto_core::analysis::format_sig(a, 3)
}

fn anova_view<'a>(app: &'a App, p: &'a ParamsState, s: &'a AnovaSpec) -> El<'a> {
    let matched = s.design == Matching::Matched;
    let tab_bar = tabs(
        p.tab,
        &[
            (Tab::Design, "Experimental Design", true),
            (Tab::RepeatedMeasures, "Repeated Measures", true),
            (Tab::Comparisons, "Multiple Comparisons", true),
            (Tab::Options, "Options", true),
            (Tab::Residuals, "Residuals", true),
        ],
    );
    let labeled: Vec<String> = s.data_sets.iter().enumerate().map(|(i, l)| format!("{}: {l}", letter(i))).collect();
    let content: El = match p.tab {
        Tab::Residuals => residuals_tab(&s.normality, s.distribution == Distribution::Nonparametric, "the residuals of the ANOVA"),
        Tab::Design | Tab::Model | Tab::Constraints => {
            let sd: El = match (matched, s.distribution) {
                (_, Distribution::Nonparametric) => space().into(),
                (false, _) => column![
                    heading("Assume equal SDs?"),
                    radio("Yes. Use ordinary ANOVA test.", true, Some(s.equal_sd), |v| msg(Change::EqualSd(v))).size(15).text_size(13),
                    radio("No. Use Brown-Forsythe and Welch ANOVA tests.", false, Some(s.equal_sd), |v| msg(Change::EqualSd(v))).size(15).text_size(13),
                ]
                .spacing(5)
                .into(),
                (true, _) => column![
                    heading("Assume sphericity (equal variability of differences)?"),
                    radio("Yes. No correction.", true, Some(s.sphericity), |v| msg(Change::Sphericity(v))).size(15).text_size(13),
                    radio("No. Use the Geisser-Greenhouse correction. Recommended.", false, Some(s.sphericity), |v| msg(Change::Sphericity(v))).size(15).text_size(13),
                ]
                .spacing(5)
                .into(),
            };
            column![
                heading("Experimental design"),
                radio("No matching or pairing", false, Some(matched), |v| msg(Change::Matched(v))).size(15).text_size(13),
                radio("Matched values (repeated measures): each subject has a value in every data set", true, Some(matched), |v| msg(Change::Matched(v)))
                    .size(15)
                    .text_size(13),
                match_by_row(app, &s.graph_id, &s.match_by, matched),
                illustration(matched),
                distribution_section(s.distribution, "mean"),
                sd,
            ]
            .spacing(6)
            .into()
        }
        Tab::RepeatedMeasures => {
            let txt = if matched {
                format!(
                    "Values are matched by the column \"{}\". Each subject needs a value in every data set; subjects with missing values are left out (a mixed-effects model for missing values will come later).",
                    s.match_by.clone().unwrap_or_else(|| "(choose on the Experimental Design tab)".into())
                )
            } else {
                "Choose \"Matched values (repeated measures)\" on the Experimental Design tab to analyse repeated measures.".into()
            };
            column![heading("Repeated measures"), caption(txt).size(13)].spacing(8).into()
        }
        Tab::Comparisons => {
            let mode = match s.comparisons {
                Comparisons::None => CompMode::None,
                Comparisons::AllPairs => CompMode::AllPairs,
                Comparisons::VsControl { .. } => CompMode::VsControl,
                Comparisons::Selected { .. } => CompMode::Selected,
            };
            let controls: Vec<Choice<String>> =
                s.data_sets.iter().zip(&labeled).map(|(v, l)| Choice { value: v.clone(), label: l.clone() }).collect();
            let control_sel = match &s.comparisons {
                Comparisons::VsControl { control } => controls.iter().find(|c| &c.value == control).cloned(),
                _ => None,
            };
            let control_pick: El = if mode == CompMode::VsControl {
                pick_list(controls, control_sel, |c: Choice<String>| msg(Change::Control(c.value))).text_size(12.5).width(Fill).into()
            } else {
                caption("—").into()
            };
            let mut pairs_list = Column::new().spacing(4);
            if mode == CompMode::Selected && p.pairs_open {
                let selected: &[[String; 2]] = match &s.comparisons {
                    Comparisons::Selected { pairs } => pairs,
                    _ => &[],
                };
                let n = s.data_sets.len();
                for (i, j) in (0..n).flat_map(|i| (i + 1..n).map(move |j| (i, j))) {
                    let (a, b) = (s.data_sets[i].clone(), s.data_sets[j].clone());
                    let on = selected.iter().any(|[x, y]| (x == &a && y == &b) || (x == &b && y == &a));
                    let label = format!("{} vs. {}", labeled[i], labeled[j]);
                    pairs_list = pairs_list.push(
                        checkbox(on).label(label).size(14).text_size(12.5).on_toggle(move |v| msg(Change::TogglePair(a.clone(), b.clone(), v))),
                    );
                }
            }
            let n_pairs = match &s.comparisons {
                Comparisons::Selected { pairs } => pairs.len(),
                _ => 0,
            };
            column![
                heading("Followup tests"),
                radio("None.", CompMode::None, Some(mode), |m| msg(Change::Mode(m))).size(15).text_size(13),
                radio("Compare the mean of each data set with the mean of every other data set.", CompMode::AllPairs, Some(mode), |m| msg(Change::Mode(m)))
                    .size(15)
                    .text_size(13),
                radio("Compare the mean of each data set with the mean of a control data set.", CompMode::VsControl, Some(mode), |m| msg(Change::Mode(m)))
                    .size(15)
                    .text_size(13),
                container(row![caption("Control data set:").size(12.5), control_pick].spacing(8).align_y(Alignment::Center))
                    .padding(iced::Padding { left: 26.0, ..Default::default() }),
                radio("Compare the means of preselected pairs of data sets.", CompMode::Selected, Some(mode), |m| msg(Change::Mode(m)))
                    .size(15)
                    .text_size(13),
                container(
                    row![
                        caption(format!("Selected pairs: {n_pairs}")).size(12.5),
                        button(text(if p.pairs_open { "Done" } else { "Select…" }).size(12.5))
                            .padding([3, 10])
                            .style(style::secondary)
                            .on_press_maybe((mode == CompMode::Selected).then_some(msg(Change::PairsOpen(!p.pairs_open))))
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center)
                )
                .padding(iced::Padding { left: 26.0, ..Default::default() }),
                container(pairs_list).padding(iced::Padding { left: 40.0, ..Default::default() }),
                radio("Test for linear trend between data set mean and left-to-right order (coming later)", CompMode::None, None, |m| msg(Change::Mode(m)))
                    .size(15)
                    .text_size(13),
                heading("Which test?"),
                caption("Use choices on the Options tab to choose the test.").size(13),
            ]
            .spacing(6)
            .into()
        }
        Tab::Options => {
            let none = s.comparisons == Comparisons::None;
            let followup = otto_engine::column_tests::followup_name(s);
            let alphas: Vec<Choice<u16>> = [(50u16, "0.05 (95% confidence interval)"), (10, "0.01 (99% confidence interval)"), (100, "0.10 (90% confidence interval)")]
                .iter()
                .map(|&(v, l)| Choice { value: v, label: l.to_string() })
                .collect();
            let alpha_sel = alphas.iter().find(|c| c.value as f64 == (s.alpha * 1000.0).round()).cloned();
            let corr = |c: Correction, label: &'a str| -> El<'a> {
                radio(label, c, Some(s.correction), |c| msg(Change::Correction(c))).size(15).text_size(13).into()
            };
            let test_line = |c: Correction| -> El<'a> {
                let t = if none {
                    "No post test".to_string()
                } else if s.correction == c {
                    followup.clone()
                } else {
                    otto_engine::column_tests::followup_name(&AnovaSpec { correction: c, ..s.clone() })
                };
                desc_owned(format!("Test: {t}"))
            };
            column![
                heading("Multiple comparisons test"),
                corr(Correction::Hypothesis, "Correct for multiple comparisons using statistical hypothesis testing. Recommended."),
                test_line(Correction::Hypothesis),
                corr(Correction::Fdr, "Correct for multiple comparisons by controlling the False Discovery Rate."),
                test_line(Correction::Fdr),
                corr(Correction::None, "Don't correct for multiple comparisons. Each comparison stands alone."),
                test_line(Correction::None),
                heading("Multiple comparisons options"),
                checkbox(s.swap).label("Swap direction of comparisons (A-B) vs. (B-A).").size(15).text_size(13).on_toggle(|v| msg(Change::Swap(v))),
                checkbox(true).label("Report multiplicity adjusted P value for each comparison.").size(15).text_size(13),
                row![
                    caption("Family-wise alpha threshold and confidence level").size(12.5),
                    pick_list(alphas, alpha_sel, |c: Choice<u16>| msg(Change::Alpha(c.value))).text_size(12.5).width(Fill)
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                graphing_section(s.graph.show, s.graph.show_ns, vec!["Graph confidence intervals — coming later"]),
                heading("Additional results"),
                checkbox(s.results.descriptive).label("Descriptive statistics for each data set.").size(15).text_size(13).on_toggle(|v| msg(Change::Descriptive(v))),
                checkbox(s.results.aicc)
                    .label("Report comparison of models using AICc.")
                    .size(15)
                    .text_size(13)
                    .on_toggle_maybe((!matched && s.distribution != Distribution::Nonparametric && s.equal_sd).then_some(|v| msg(Change::Aicc(v)))),
                checkbox(s.results.effect_sizes).label("Report standardized effect sizes").size(15).text_size(13).on_toggle(|v| msg(Change::Effect(v))),
                output_section(s.output.digits, s.output.p_style),
            ]
            .spacing(6)
            .into()
        }
    };
    let mut perform = format!("Based on your choices (on all tabs), Otto will perform:\n   - {}.", s.test_name());
    if s.comparisons != Comparisons::None {
        perform.push_str(&format!("\n   - {}.", otto_engine::column_tests::followup_name(s)));
    }
    let body = column![
        caption(format!("Data sets: {}", labeled.join(", "))).size(12.5),
        tab_bar,
        container(scrollable(container(content).padding([4, 4])).height(Length::Shrink)).max_height(470.0),
        yellow_box(text(perform).size(13).into()),
        footer(Message::Dialog(DialogMsg::ParamsOk), p.error.as_deref())
    ]
    .spacing(12);
    frame("Parameters: One-Way ANOVA (and Nonparametric or Mixed)".into(), body.into(), 640.0)
}

fn desc_owned<'a>(s: String) -> El<'a> {
    container(caption(s).size(12)).padding(iced::Padding { left: 26.0, ..Default::default() }).into()
}

// Small schematic tables like Prism's: unmatched values (ellipses) vs. matched rows (arrows).
const ILLUSTRATION_UNMATCHED: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 330 118">
<g font-family="Segoe UI, Arial" font-size="11" fill="#C9CCD2">
<rect x="0.5" y="0.5" width="329" height="117" fill="#26282B" stroke="#4A4D52"/>
<rect x="0.5" y="0.5" width="329" height="22" fill="#34363A" stroke="#4A4D52"/>
<text x="70" y="15">Group A</text><text x="160" y="15">Group B</text><text x="250" y="15">Group C</text>
<path d="M40 22.5V117 M130 22.5V117 M220 22.5V117 M0 46H330 M0 70H330 M0 94H330" stroke="#4A4D52"/>
<text x="16" y="38">1</text><text x="16" y="62">2</text><text x="16" y="86">3</text><text x="16" y="110">4</text>
<ellipse cx="85" cy="70" rx="30" ry="40" fill="#8BC34A" fill-opacity="0.18" stroke="#8BC34A" stroke-width="2"/>
<ellipse cx="175" cy="70" rx="30" ry="40" fill="#8BC34A" fill-opacity="0.18" stroke="#8BC34A" stroke-width="2"/>
<ellipse cx="265" cy="70" rx="30" ry="40" fill="#8BC34A" fill-opacity="0.18" stroke="#8BC34A" stroke-width="2"/>
<path d="M112 70H148 M202 70H238" stroke="#B388FF" stroke-width="2.5"/>
<path d="M112 70l7-4v8z M148 70l-7-4v8z M202 70l7-4v8z M238 70l-7-4v8z" fill="#B388FF"/>
</g></svg>"##;

const ILLUSTRATION_MATCHED: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 330 118">
<g font-family="Segoe UI, Arial" font-size="11" fill="#C9CCD2">
<rect x="0.5" y="0.5" width="329" height="117" fill="#26282B" stroke="#4A4D52"/>
<rect x="0.5" y="0.5" width="329" height="22" fill="#34363A" stroke="#4A4D52"/>
<text x="70" y="15">Group A</text><text x="160" y="15">Group B</text><text x="250" y="15">Group C</text>
<path d="M40 22.5V117 M130 22.5V117 M220 22.5V117 M0 46H330 M0 70H330 M0 94H330" stroke="#4A4D52"/>
<text x="6" y="38">Donor 1</text><text x="6" y="62">Donor 2</text>
<path d="M60 34H150 M160 34H250 M60 58H150 M160 58H250" stroke="#B388FF" stroke-width="2.5"/>
<path d="M60 34l7-4v8z M150 34l-7-4v8z M160 34l7-4v8z M250 34l-7-4v8z M60 58l7-4v8z M150 58l-7-4v8z M160 58l7-4v8z M250 58l-7-4v8z" fill="#B388FF"/>
</g></svg>"##;
