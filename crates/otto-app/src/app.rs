//! Application state and update logic. Everything that changes the project goes
//! through `otto_engine::Command`, whether it comes from a button or from Claude (MCP).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use iced::futures::SinkExt;
use iced::keyboard::{self, key::Named, Key};
use iced::widget::{operation, scrollable, svg};
use iced::{window, Subscription, Task};

use otto_core::{ColumnKind, DataLayout, GraphKind};
use otto_engine::*;
use otto_mcp::McpRequest;

pub const CELL_EDITOR: &str = "cell-editor";
pub const HEADER_EDITOR: &str = "header-editor";
pub const GRID_SCROLL: &str = "grid-scroll";
pub const ROW_HEIGHT: f32 = 26.0;

/// An MCP request travelling through the message queue (taken exactly once).
#[derive(Clone, Debug)]
pub struct McpEnvelope(Arc<Mutex<Option<McpRequest>>>);

impl McpEnvelope {
    fn take(&self) -> Option<McpRequest> {
        self.0.lock().ok().and_then(|mut r| r.take())
    }
}

/// Text fields whose edits are applied on Enter.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Draft {
    TableName(String),
    ColumnName(String, usize),
    LevelName(String, usize, usize),
    GraphName(String),
    AxisTitle(String, Axis),
    AxisMin(String, Axis),
    AxisMax(String, Axis),
    AnalysisName(String),
    VolcanoFc(String),
    VolcanoP(String),
    /// A numeric row filter's bounds: (graph, column).
    FilterMin(String, String),
    FilterMax(String, String),
}

/// What New or Open wants to do once the current project has been saved or given up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pending {
    New,
    Open,
    /// Close the window, once its unsaved changes have been dealt with.
    Close(window::Id),
}

#[derive(Clone, Debug)]
pub enum Message {
    Cmd(Command),
    /// Folds a settings-panel section open or shut.
    TogglePanelSection(&'static str, bool),
    Mcp(McpEnvelope),
    McpStarted(Result<String, String>),
    /// Switch Claude's MCP access to the app on or off (top right of the window).
    SetMcp(bool),

    /// Start a new project, or open one: both throw the current project away, so they
    /// go through `guard_unsaved` first.
    New,
    Open,
    OpenPicked(Option<PathBuf>),
    Save,
    SaveAs,
    SavePicked(Option<PathBuf>),
    Import,
    ImportPicked(Option<PathBuf>),
    PasteNewTable,
    PasteNewTableText(Option<String>),
    ExportGraph,
    ExportGraphPicked(Option<PathBuf>),
    ExportData,
    ExportDataPicked(Option<PathBuf>),
    ExportCode,
    ExportCodePicked(Option<PathBuf>),
    /// Save an analysis as a standalone R script (analysis id).
    ExportRScript(String),
    ExportRScriptPicked(String, Option<PathBuf>),
    SetDataLayout(DataLayout),
    NewGraph(GraphKind),
    NewDescriptive,
    /// Header click: highlights the column (Ctrl adds or removes it, Shift extends to it).
    SelectColumn(usize),
    HeaderDoubleClicked(usize),
    HeaderInput(String),
    HeaderSubmit,
    /// Clipboard text pasted onto a highlighted header: a table including its header row.
    HeaderPaste(Option<String>),

    /// Row-header click: selects the row (Ctrl adds or removes it, Shift extends to it).
    SelectRow(usize),
    /// The pointer entered a row or column header while the button is still down.
    DragOverRow(usize),
    DragOverColumn(usize),
    /// The mouse button came back up anywhere: any drag-select is over.
    DragEnd,

    CellClicked(usize, usize),
    CellDoubleClicked(usize, usize),
    CellInput(String),
    CellSubmit,
    GridScrolled(scrollable::Viewport),
    GridPaste(Option<String>),

    Draft(Draft, String),
    Commit(Draft),
    ToggleSection(usize),
    /// Opens (or closes, if already open) the dropdown with this id.
    ToggleDropdown(String),
    /// Asks before deleting a data table (and every graph made from it).
    AskDeleteTable(String),
    CloseDropdown,
    /// Analysis dialogs (Analyze Data, parameters).
    Dialog(crate::dialogs::DialogMsg),
    Noop,
    Key(keyboard::Event),
    CloseRequested(window::Id),
    /// The main window opened: fit it to the screen above the taskbar, then show it.
    WindowOpened(window::Id),
    /// Checks for statistics finished in the background.
    StatsTick,
    /// The window was resized: its new width in logical pixels.
    Resized(f32),
    /// Show or hide the navigator (`Side::Left`) or the settings panel (`Side::Right`).
    TogglePanel(Side),
}

/// Which edge panel a toggle acts on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

/// Below this width the navigator is folded away, and below the narrower one the settings
/// panel goes too: on a narrow window the graph or table is what matters.
pub const NAV_MIN_WIDTH: f32 = 1240.0;
pub const PANEL_MIN_WIDTH: f32 = 980.0;

#[derive(Default)]
pub struct McpStatus {
    pub url: Option<String>,
    pub error: Option<String>,
    pub actions: u64,
    pub last: Option<String>,
}

#[derive(Default)]
pub struct Grid {
    /// Selected (row, column).
    pub sel: Option<(usize, usize)>,
    /// Column shown in the inspector.
    pub column: Option<usize>,
    /// Edit buffer while a cell is being edited.
    pub editing: Option<String>,
    pub scroll_y: f32,
    pub view_h: f32,
    /// Highlighted columns, in the order they were picked ("Graph this table" plots them).
    pub highlight: Vec<usize>,
    /// Column a Shift-click extends from.
    pub anchor: Option<usize>,
    /// Selected whole rows, ascending.
    pub rows: Vec<usize>,
    /// Row a Shift-click extends from.
    pub row_anchor: Option<usize>,
    /// A header drag in progress, so the pointer moving onto a header extends the
    /// selection rather than doing nothing.
    pub drag: Option<Drag>,
    /// The table's (rows, columns) when the selection was made. Rows and columns are
    /// selected by position, so adding or deleting any of them means the selection no
    /// longer picks out what the user picked: it is dropped rather than left pointing
    /// at whatever moved into those places.
    pub shape: (usize, usize),
    /// Column header being renamed in place: (column, text).
    pub header_edit: Option<(usize, String)>,
}

/// What a drag that started on a header is selecting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drag {
    Rows,
    Columns,
}

impl Grid {
    /// Nothing is selected in the grid at all.
    pub fn nothing_selected(&self) -> bool {
        self.sel.is_none() && self.highlight.is_empty() && self.rows.is_empty()
    }
}

/// The plotted table on screen, built only when its view is opened.
pub struct Plotted {
    pub graph_id: String,
    pub revision: u64,
    pub result: Result<otto_plot::TextTable, String>,
}

pub const GRAPH_SCROLL: &str = "graph-scroll";

/// A rendered graph: SVG and figure size in points.
pub struct Sheet {
    pub handle: svg::Handle,
    pub width: f32,
    pub height: f32,
    /// Part of `height` added at the top for significance brackets.
    pub stats_height: f32,
}

/// The rendered graph currently on screen.
pub struct Preview {
    pub graph_id: String,
    /// The graph's colored data sets (bars, for a column graph): what the panel offers to
    /// put in focus.
    pub sets: Vec<String>,
    pub revision: u64,
    /// Background statistics generation it was drawn with (redrawn when results arrive).
    pub stats: u64,
    pub result: Result<Sheet, String>,
}

/// The analysis results the views on screen need, computed once per change instead of
/// once per frame. `results::compute` distills the whole data table and formats every
/// number, which is far too much to do while drawing: only the analyses of the graph or
/// result being shown are kept, and they are rebuilt when the project changes or R
/// answers.
#[derive(Default)]
pub struct Reports {
    revision: u64,
    stats: u64,
    by_analysis: HashMap<String, Result<otto_engine::results::Report, otto_engine::EngineError>>,
}

pub struct App {
    pub engine: Engine,
    pub status: (String, bool),
    pub mcp: McpStatus,
    pub grid: Grid,
    pub drafts: HashMap<Draft, String>,
    /// Navigator sections: data tables, graphs, final graphs.
    pub collapsed: [bool; 3],
    /// Settings-panel sections the user has folded open or shut, by key. A key that is not
    /// here is at its default, so only what was changed is remembered.
    pub sections: HashMap<&'static str, bool>,
    pub plotted: Option<Plotted>,
    pub modifiers: keyboard::Modifiers,
    /// Id of the open dropdown picker, if any.
    pub open_dropdown: Option<String>,
    /// The open modal dialog, if any.
    pub dialog: Option<crate::dialogs::Dialog>,
    pub preview: Option<Preview>,
    pub data_layout: DataLayout,
    pub settings: crate::settings::Settings,
    /// Where settings are saved (`None`: not saved, e.g. in tests).
    pub settings_file: Option<PathBuf>,
    /// Out-of-date analyses (id → reasons), refreshed after every change.
    pub stale: HashMap<String, Vec<String>>,
    /// Results of the analyses on screen; see [`Reports`].
    reports: Reports,
    /// Data sets (bar label and n) of the graphs the open dialog lists, and whether each
    /// out-of-date analysis could be recalculated. Both distill the data table, so they
    /// are worked out once per change instead of once per frame.
    sets: HashMap<String, Vec<(String, usize)>>,
    recalc: HashMap<String, Result<(), String>>,
    /// Analyses the last command made out of date.
    newly_stale: Vec<String>,
    /// Scroll the graph sheet to its bottom (the graph itself) after the next change.
    snap_graph: bool,
    last_view: View,
    /// New or Open, waiting on a Save As the user asked for first.
    after_save: Option<Pending>,
    /// The window's width, which decides whether the edge panels fit.
    pub window_width: f32,
    /// Set only where the user overruled what the width decided; cleared as soon as the
    /// width crosses a threshold again.
    pub nav_open: Option<bool>,
    pub panel_open: Option<bool>,
}

/// What the window's width alone says about the two edge panels.
fn panels_fit(width: f32) -> (bool, bool) {
    (width >= NAV_MIN_WIDTH, width >= PANEL_MIN_WIDTH)
}

impl App {
    pub fn new(open: Option<PathBuf>) -> (Self, Task<Message>) {
        let settings_file = crate::settings::default_path();
        let settings = crate::settings::Settings::load(settings_file.as_deref());
        let mut app = App {
            settings,
            settings_file,
            engine: Engine::new(),
            status: ("Ready".into(), false),
            mcp: McpStatus::default(),
            grid: Grid { view_h: 800.0, ..Default::default() },
            drafts: HashMap::new(),
            collapsed: [false; 3],
            plotted: None,
            modifiers: keyboard::Modifiers::default(),
            open_dropdown: None,
            dialog: None,
            preview: None,
            data_layout: DataLayout::Long,
            stale: HashMap::new(),
            sections: HashMap::new(),
            reports: Reports::default(),
            sets: HashMap::new(),
            recalc: HashMap::new(),
            newly_stale: Vec::new(),
            snap_graph: false,
            last_view: View::Welcome,
            after_save: None,
            window_width: f32::INFINITY,
            nav_open: None,
            panel_open: None,
        };
        // Start R in the background so the first analysis doesn't wait for it, and never let
        // the window wait for R: statistics that are not ready yet are calculated in the
        // background and appear when done.
        otto_r::warm_up();
        otto_r::set_background(true);
        if let Some(path) = open {
            let cmd = if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("otto")) {
                Command::ProjectOpen(PathArgs { path: path.display().to_string() })
            } else {
                Command::TableImportFile(ImportFileArgs { path: path.display().to_string(), sheet: None, name: None, options: None })
            };
            app.run(cmd);
        }
        (app, Task::none())
    }

    /// Whether the navigator and the settings panel are on screen: what the window's width
    /// allows, unless the user said otherwise.
    pub fn panels(&self) -> (bool, bool) {
        let (nav, panel) = panels_fit(self.window_width);
        (self.nav_open.unwrap_or(nav), self.panel_open.unwrap_or(panel))
    }

    pub fn title(&self) -> String {
        format!("Otto — {}", self.engine.title())
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![
            keyboard::listen().map(Message::Key),
            // A drag-select ends wherever the button comes up, not only over a header.
            iced::event::listen_with(|event, _, _| match event {
                iced::Event::Mouse(iced::mouse::Event::ButtonReleased(_)) => Some(Message::DragEnd),
                _ => None,
            }),
            window::close_requests().map(Message::CloseRequested),
            window::open_events().map(Message::WindowOpened),
            window::resize_events().map(|(_, size)| Message::Resized(size.width)),
        ];
        // The MCP endpoint only exists while switched on: removing the subscription stops
        // the server and deletes the discovery file, so Claude can't reach the app.
        if self.settings.mcp_enabled {
            subs.push(Subscription::run(mcp_worker));
        }
        // Background statistics: look for finished results while any are being calculated.
        if otto_r::pending() > 0 || self.preview.as_ref().is_some_and(|p| p.stats != otto_r::generation()) {
            subs.push(iced::time::every(std::time::Duration::from_millis(150)).map(|_| Message::StatsTick));
        }
        // The statistics engine dialog follows a running setup.
        if matches!(self.dialog, Some(crate::dialogs::Dialog::Engine(_))) {
            subs.push(iced::time::every(std::time::Duration::from_millis(600)).map(|_| Message::Dialog(crate::dialogs::DialogMsg::EngineTick)));
        }
        Subscription::batch(subs)
    }

    pub(crate) fn set_status(&mut self, text: impl Into<String>, error: bool) {
        self.status = (text.into(), error);
    }

    /// Applies a command and refreshes derived state.
    pub fn run(&mut self, cmd: Command) -> bool {
        self.newly_stale.clear();
        let ok = match self.engine.apply(cmd) {
            Ok(out) => {
                // The full note (with MCP tool names) is for Claude; the app shows its own dialog.
                let message = match out.message.split_once(". Statistics out of date") {
                    Some((head, _)) => format!("{head}. Statistics on this graph are out of date"),
                    None => out.message,
                };
                self.set_status(message, false);
                self.newly_stale = out.out_of_date;
                true
            }
            Err(e) => {
                self.set_status(e.to_string(), true);
                false
            }
        };
        self.after_change();
        ok
    }

    /// Runs a command from the GUI; if it made statistics out of date, asks what to do.
    pub fn run_from_ui(&mut self, cmd: Command) -> bool {
        let ok = self.run(cmd);
        if ok && !self.newly_stale.is_empty() && self.dialog.is_none() {
            let ids = std::mem::take(&mut self.newly_stale);
            if let Some(graph) = ids.first().and_then(|id| self.engine.analysis_graph(id)) {
                self.dialog = Some(crate::dialogs::Dialog::Stale(crate::dialogs::StaleState { graph, ids }));
            }
        }
        ok
    }

    fn after_change(&mut self) {
        if self.engine.view != self.last_view {
            self.drafts.clear();
            self.open_dropdown = None;
            self.grid.editing = None;
            let same_table = matches!((&self.engine.view, &self.last_view), (View::Table(a), View::Table(b)) if a == b);
            self.grid.header_edit = None;
            if !same_table {
                self.grid.sel = None;
                self.grid.column = None;
                self.grid.anchor = None;
                self.grid.highlight.clear();
                self.grid.rows.clear();
                self.grid.row_anchor = None;
                self.grid.scroll_y = 0.0;
            }
            self.last_view = self.engine.view.clone();
        }
        if let Some(t) = self.active_table() {
            let (rows, cols) = (t.nrows, t.ncols());
            if let Some((r, c)) = self.grid.sel {
                if r >= rows.max(1) || c >= cols.max(1) {
                    self.grid.sel = None;
                }
            }
            if self.grid.column.is_some_and(|c| c >= cols) {
                self.grid.column = None;
            }
            if self.grid.shape != (rows, cols) {
                if !self.grid.rows.is_empty() || self.grid.highlight.len() > 1 {
                    self.grid.rows.clear();
                    self.grid.row_anchor = None;
                    self.grid.highlight.retain(|&c| Some(c) == self.grid.column);
                }
                self.grid.shape = (rows, cols);
            }
            self.grid.highlight.retain(|&c| c < cols);
            self.grid.rows.retain(|&r| r < rows);
        }
        // Plotted tables are built only while one is on screen (graphs stay quick).
        if let View::Plotted(id) = &self.engine.view {
            if self.plotted.as_ref().is_none_or(|p| p.graph_id != *id || p.revision != self.engine.revision) {
                let result = self
                    .engine
                    .plot_data(id)
                    .map(|d| otto_plot::data_export::table(&d, DataLayout::Wide))
                    .map_err(|e| e.to_string());
                self.plotted = Some(Plotted { graph_id: id.clone(), revision: self.engine.revision, result });
            }
        }
        self.stale = self.engine.stale().iter().cloned().collect();
        let stats = otto_r::generation();
        self.refresh_reports(stats);
        self.refresh_sets();
        if let View::Graph(id) = &self.engine.view {
            let outdated = self.preview.as_ref().is_none_or(|p| p.graph_id != *id || p.revision != self.engine.revision || p.stats != stats);
            if outdated {
                let rendered = self.engine.render_graph(id);
                // The colored data sets come out of the same render: distilling the table
                // a second time just to list them was the third pass over the data.
                let sets = rendered.as_ref().map(|r| r.data.color_groups().to_vec()).unwrap_or_default();
                let result = rendered
                    .map(|r| Sheet {
                        handle: svg::Handle::from_memory(r.svg().into_bytes()),
                        width: r.scene.width as f32,
                        height: r.scene.height as f32,
                        stats_height: r.scene.stats_height as f32,
                    })
                    .map_err(|e| e.to_string());
                // Brackets grow the figure upward: keep the graph itself in view.
                let height = |p: &Preview| p.result.as_ref().map(|s| s.height).ok();
                let before = self.preview.as_ref().filter(|p| p.graph_id == *id).and_then(height);
                let new = Preview { graph_id: id.clone(), sets, revision: self.engine.revision, stats, result };
                if before != height(&new) {
                    self.snap_graph = true;
                }
                self.preview = Some(new);
            }
        }
    }

    /// Recomputes [`Reports`] for the analyses the current view shows, if the project has
    /// changed or R has answered since the last pass.
    fn refresh_reports(&mut self, stats: u64) {
        let wanted: Vec<String> = match &self.engine.view {
            View::Graph(id) => self.engine.graph_analyses(id).iter().map(|a| a.id.clone()).collect(),
            View::Result(id) => vec![id.clone()],
            _ => Vec::new(),
        };
        let current = self.reports.revision == self.engine.revision && self.reports.stats == stats;
        if current && wanted.iter().all(|id| self.reports.by_analysis.contains_key(id)) {
            return;
        }
        self.reports = Reports { revision: self.engine.revision, stats, by_analysis: HashMap::new() };
        for id in wanted {
            if let Some(a) = self.engine.project.analyses.iter().find(|a| a.id == id) {
                let report = otto_engine::results::compute(&self.engine.project, a);
                self.reports.by_analysis.insert(id, report);
            }
        }
    }

    /// Whether a settings-panel section is open: what the user last chose for it, else
    /// `default_open` (the type and the data are open, the rest folded away).
    pub fn section_open(&self, key: &'static str, default_open: bool) -> bool {
        self.sections.get(key).copied().unwrap_or(default_open)
    }

    /// Recomputes the data sets the open dialog lists and the recalculate check for every
    /// out-of-date analysis. Both call into `column_tests`, which distills the data table.
    /// Called after every command and after every dialog message, so `view()` never has to.
    pub(crate) fn refresh_sets(&mut self) {
        self.sets = self
            .dialog
            .as_ref()
            .map(|d| d.graphs())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|g| otto_engine::column_tests::list_data_sets(&self.engine.project, &g).ok().map(|s| (g, s)))
            .collect();
        // Only out-of-date analyses offer "recalculate", so only they need the check.
        self.recalc = self
            .stale
            .keys()
            .filter_map(|id| self.engine.project.analyses.iter().find(|a| &a.id == id))
            .map(|a| {
                let ok = otto_engine::column_tests::rebind_to_graph(&self.engine.project, &a.kind).map(|_| ()).map_err(|e| e.to_string());
                (a.id.clone(), ok)
            })
            .collect();
    }

    /// The data sets of a graph the open dialog lists (empty if it lists none).
    pub fn data_sets(&self, graph_id: &str) -> &[(String, usize)] {
        self.sets.get(graph_id).map(|v| v.as_slice()).unwrap_or_default()
    }

    /// Whether "recalculate with the data shown" can work for an out-of-date analysis.
    pub fn can_recalculate(&self, analysis_id: &str) -> Result<(), String> {
        self.recalc.get(analysis_id).cloned().unwrap_or(Ok(()))
    }

    /// The computed results of an analysis on screen. `None` means it is not one of the
    /// analyses this view asked for, so nothing was computed for it.
    pub fn report(&self, analysis_id: &str) -> Option<&Result<otto_engine::results::Report, otto_engine::EngineError>> {
        self.reports.by_analysis.get(analysis_id)
    }

    /// Whether any statistic the shown graph draws is still being calculated in R.
    pub fn calculating(&self, graph_id: &str) -> bool {
        self.engine
            .graph_analyses(graph_id)
            .into_iter()
            .any(|a| matches!(self.report(&a.id), Some(Err(otto_engine::EngineError::Calculating))))
    }

    pub fn active_table(&self) -> Option<&otto_core::DataTable> {
        match &self.engine.view {
            View::Table(id) => self.engine.project.table_by_id(id),
            View::Graph(id) | View::Plotted(id) => {
                let g = self.engine.project.graphs.iter().find(|g| &g.id == id)?;
                self.engine.project.table_by_id(&g.table_id)
            }
            View::Result(id) => {
                let a = self.engine.project.analyses.iter().find(|a| &a.id == id)?;
                self.engine.project.table_by_id(&a.table_id)
            }
            View::Welcome => None,
        }
    }

    /// The data table to graph or analyse: the one on screen, or the first data table
    /// (never a final graph's locked copy).
    fn active_table_id(&self) -> Option<String> {
        self.active_table()
            .filter(|t| t.locked_by.is_none())
            .map(|t| t.id.clone())
            .or_else(|| self.engine.project.tables.iter().find(|t| t.locked_by.is_none()).map(|t| t.id.clone()))
    }

    /// The graph on screen (also when its plotted table is shown).
    fn graph_id(&self) -> Option<String> {
        match &self.engine.view {
            View::Graph(id) | View::Plotted(id) => Some(id.clone()),
            _ => None,
        }
    }

    fn table_view_id(&self) -> Option<String> {
        match &self.engine.view {
            View::Table(id) => Some(id.clone()),
            _ => None,
        }
    }

    /// Applies every pending text-field edit (used before other actions so nothing typed is lost).
    fn commit_drafts(&mut self) {
        let keys: Vec<Draft> = self.drafts.keys().cloned().collect();
        for k in keys {
            self.commit(k);
        }
    }

    fn commit(&mut self, key: Draft) {
        let Some(value) = self.drafts.remove(&key) else { return };
        let value = value.trim().to_string();
        let project = &self.engine.project;
        let cmd = match &key {
            Draft::TableName(t) => Command::TableRename(RenameTableArgs { table: Some(t.clone()), name: value }),
            Draft::ColumnName(t, c) => {
                let Some(col) = project.table_by_id(t).and_then(|t| t.columns.get(*c)) else { return };
                if col.name == value {
                    return;
                }
                Command::TableRenameColumn(RenameColumnArgs { table: Some(t.clone()), column: col.name.clone(), new_name: value })
            }
            Draft::LevelName(t, c, l) => {
                let Some(col) = project.table_by_id(t).and_then(|t| t.columns.get(*c)) else { return };
                let Some(level) = col.levels().get(*l).cloned() else { return };
                if level == value {
                    return;
                }
                Command::TableRenameLevel(RenameLevelArgs { table: Some(t.clone()), column: col.name.clone(), level, new_name: value })
            }
            Draft::GraphName(g) => Command::GraphRename(RenameGraphArgs { graph: Some(g.clone()), name: value }),
            Draft::AnalysisName(a) => Command::AnalysisRename(RenameAnalysisArgs { analysis: Some(a.clone()), name: value }),
            Draft::AxisTitle(g, axis) => Command::GraphSetAxis(GraphAxisArgs {
                scale: None,
                numbers: None,
                graph: Some(g.clone()),
                axis: *axis,
                title: Some(value),
                min: None,
                max: None,
                auto_range: false,
            }),
            Draft::AxisMin(g, axis) | Draft::AxisMax(g, axis) => {
                let Some(spec) = project.graph(g) else { return };
                let current = if *axis == Axis::X { &spec.x_axis } else { &spec.y_axis };
                let parsed = if value.is_empty() {
                    None
                } else {
                    match otto_core::table::parse_number(&value, true) {
                        Some(v) => Some(v),
                        None => {
                            self.set_status(format!("'{value}' is not a number"), true);
                            return;
                        }
                    }
                };
                let (min, max) = if matches!(key, Draft::AxisMin(..)) { (parsed, current.max) } else { (current.min, parsed) };
                Command::GraphSetAxis(GraphAxisArgs { graph: Some(g.clone()), axis: *axis, title: None, min, max, auto_range: true, scale: None, numbers: None })
            }
            Draft::FilterMin(g, column) | Draft::FilterMax(g, column) => {
                let Some(spec) = project.graph(g) else { return };
                let parsed = if value.is_empty() {
                    None
                } else {
                    match otto_core::table::parse_number(&value, true) {
                        Some(v) => Some(v),
                        None => {
                            self.set_status(format!("'{value}' is not a number"), true);
                            return;
                        }
                    }
                };
                let mut filters = spec.filters.clone();
                let mut f = filters.iter().find(|f| &f.column == column).cloned().unwrap_or_else(|| otto_core::Filter { column: column.clone(), ..Default::default() });
                if matches!(key, Draft::FilterMin(..)) {
                    f.min = parsed;
                } else {
                    f.max = parsed;
                }
                filters.retain(|x| &x.column != column);
                if !f.keeps_everything() {
                    filters.push(f);
                }
                Command::GraphSetFilters(GraphFiltersArgs { graph: Some(g.clone()), filters })
            }
            Draft::VolcanoFc(g) | Draft::VolcanoP(g) => {
                let Some(v) = otto_core::table::parse_number(&value, true) else {
                    self.set_status(format!("'{value}' is not a number"), true);
                    return;
                };
                let fc = matches!(key, Draft::VolcanoFc(..));
                Command::GraphSetVolcano(GraphVolcanoArgs {
                    graph: Some(g.clone()),
                    fc_cutoff: fc.then_some(v),
                    p_cutoff: (!fc).then_some(v),
                    ..Default::default()
                })
            }
        };
        self.run(cmd);
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        let task = self.handle(message);
        if std::mem::take(&mut self.snap_graph) {
            // Pin the sheet to its bottom: the graph stays put while brackets extend upward.
            let snap = operation::snap_to(GRAPH_SCROLL, scrollable::RelativeOffset { x: None, y: Some(1.0) });
            Task::batch([task, snap])
        } else {
            task
        }
    }

    fn handle(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Cmd(cmd) => {
                self.commit_drafts();
                self.run_from_ui(cmd);
                Task::none()
            }
            Message::Mcp(env) => {
                if let Some(req) = env.take() {
                    if !self.settings.mcp_enabled {
                        // A call already on its way when access was switched off.
                        let _ = req.reply.send(Err(EngineError::Invalid(otto_mcp::SWITCHED_OFF.into())));
                        return Task::none();
                    }
                    let tool = req.tool.clone();
                    // Claude needs the results of its calls: wait for R here.
                    let result = otto_r::blocking(|| self.engine.call(&req.tool, req.args));
                    self.mcp.actions += 1;
                    match &result {
                        Ok(out) => self.set_status(format!("Claude · {tool}: {}", out.message), false),
                        Err(e) => self.set_status(format!("Claude · {tool}: {e}"), true),
                    }
                    let show = tool == "ui_show_window";
                    self.mcp.last = Some(tool);
                    let _ = req.reply.send(result);
                    self.after_change();
                    if show {
                        // Claude is done: bring the window to the front.
                        return window::latest().and_then(window::gain_focus);
                    }
                }
                Task::none()
            }
            Message::McpStarted(Ok(url)) => {
                if self.settings.mcp_enabled {
                    self.mcp.url = Some(url);
                }
                Task::none()
            }
            Message::McpStarted(Err(e)) => {
                self.set_status(format!("MCP server not started: {e}"), true);
                self.mcp.error = Some(e);
                Task::none()
            }
            Message::SetMcp(on) => {
                self.settings.mcp_enabled = on;
                if let Some(path) = &self.settings_file {
                    if let Err(e) = self.settings.save(path) {
                        self.set_status(format!("Could not save settings: {e}"), true);
                    }
                }
                self.mcp.url = None;
                self.mcp.error = None;
                if on {
                    self.set_status("Claude MCP switched on: Claude can see and change this window", false);
                } else {
                    self.mcp.actions = 0;
                    self.mcp.last = None;
                    self.set_status("Claude MCP switched off: Claude can no longer see or change this window", false);
                }
                Task::none()
            }

            Message::New => self.guard_unsaved(Pending::New),
            Message::Open => self.guard_unsaved(Pending::Open),
            Message::OpenPicked(Some(p)) => {
                self.run(Command::ProjectOpen(PathArgs { path: p.display().to_string() }));
                Task::none()
            }
            Message::Save => {
                self.commit_drafts();
                if self.engine.path.is_some() {
                    self.run(Command::ProjectSave(NoArgs {}));
                    Task::none()
                } else {
                    self.update(Message::SaveAs)
                }
            }
            Message::SaveAs => {
                let name = format!("{}.otto", self.engine.project.name);
                Task::perform(
                    async move {
                        rfd::AsyncFileDialog::new()
                            .set_title("Save Otto project")
                            .set_file_name(name)
                            .add_filter("Otto project", &["otto"])
                            .save_file()
                            .await
                            .map(|f| f.path().to_path_buf())
                    },
                    Message::SavePicked,
                )
            }
            Message::SavePicked(Some(p)) => {
                let ok = self.run(Command::ProjectSaveAs(PathArgs { path: p.display().to_string() }));
                if !ok {
                    self.after_save = None;
                    return Task::none();
                }
                match self.after_save.take() {
                    Some(what) => self.proceed(what),
                    None => Task::none(),
                }
            }
            Message::Import => Task::perform(
                async {
                    rfd::AsyncFileDialog::new()
                        .set_title("Import data table")
                        .add_filter("Data files", &["csv", "tsv", "txt", "xlsx", "xls", "xlsm", "ods"])
                        .pick_file()
                        .await
                        .map(|f| f.path().to_path_buf())
                },
                Message::ImportPicked,
            ),
            Message::ImportPicked(Some(p)) => {
                // The wizard opens on the file rather than importing it straight away, so a
                // title line, a two-row header or a guessed-wrong separator can be sorted out.
                self.dialog = Some(crate::dialogs::Dialog::Import(crate::dialogs::ImportState::new(p)));
                Task::none()
            }
            Message::PasteNewTable => iced::clipboard::read().map(Message::PasteNewTableText),
            Message::PasteNewTableText(Some(text)) => {
                self.run(Command::TableImportText(ImportTextArgs { text, name: None, has_header: true, options: None }));
                Task::none()
            }
            Message::ExportGraph => {
                let Some(gid) = self.graph_id() else { return Task::none() };
                let name = self.engine.project.graph(&gid).map(|g| g.name.clone()).unwrap_or_else(|| "graph".into());
                Task::perform(
                    async move {
                        rfd::AsyncFileDialog::new()
                            .set_title("Export graph")
                            .set_file_name(format!("{name}.svg"))
                            .add_filter("SVG vector", &["svg"])
                            .add_filter("PDF vector", &["pdf"])
                            .add_filter("EMF vector (Office)", &["emf"])
                            .add_filter("PNG 600 dpi", &["png"])
                            .save_file()
                            .await
                            .map(|f| f.path().to_path_buf())
                    },
                    Message::ExportGraphPicked,
                )
            }
            Message::ExportGraphPicked(Some(p)) => {
                self.run(Command::GraphExportFile(ExportFileArgs {
                    graph: self.graph_id(),
                    path: p.display().to_string(),
                    format: None,
                    outline_text: false,
                    background: false,
                }));
                Task::none()
            }
            Message::ExportData => {
                let Some(gid) = self.graph_id() else { return Task::none() };
                let name = self.engine.project.graph(&gid).map(|g| g.name.clone()).unwrap_or_else(|| "graph".into());
                Task::perform(
                    async move {
                        rfd::AsyncFileDialog::new()
                            .set_title("Export plotted data")
                            .set_file_name(format!("{name} data.csv"))
                            .add_filter("CSV", &["csv"])
                            .save_file()
                            .await
                            .map(|f| f.path().to_path_buf())
                    },
                    Message::ExportDataPicked,
                )
            }
            Message::ExportDataPicked(Some(p)) => {
                // From a plotted table: exactly what it shows (Prism columns).
                let layout = if matches!(self.engine.view, View::Plotted(_)) { DataLayout::Wide } else { self.data_layout };
                self.run(Command::GraphExportData(ExportDataArgs { graph: self.graph_id(), path: p.display().to_string(), layout }));
                Task::none()
            }
            Message::ExportCode => {
                let Some(gid) = self.graph_id() else { return Task::none() };
                let name = self.engine.project.graph(&gid).map(|g| g.name.clone()).unwrap_or_else(|| "graph".into());
                Task::perform(
                    async move {
                        rfd::AsyncFileDialog::new()
                            .set_title("Export R code")
                            .set_file_name(format!("{name}.R.txt"))
                            .add_filter("Text", &["txt", "R"])
                            .save_file()
                            .await
                            .map(|f| f.path().to_path_buf())
                    },
                    Message::ExportCodePicked,
                )
            }
            Message::ExportCodePicked(Some(p)) => {
                self.run(Command::GraphExportRCode(ExportCodeArgs { graph: self.graph_id(), path: Some(p.display().to_string()) }));
                Task::none()
            }
            Message::ExportRScript(id) => {
                let name = self.engine.project.analyses.iter().find(|a| a.id == id).map(|a| a.name.clone()).unwrap_or_else(|| "analysis".into());
                Task::perform(
                    async move {
                        rfd::AsyncFileDialog::new()
                            .set_title("Export R script")
                            .set_file_name(format!("{name}.R"))
                            .add_filter("R script", &["R"])
                            .save_file()
                            .await
                            .map(|f| f.path().to_path_buf())
                    },
                    move |p| Message::ExportRScriptPicked(id.clone(), p),
                )
            }
            Message::ExportRScriptPicked(id, Some(p)) => {
                self.run(Command::AnalysisExportRScript(ExportScriptArgs { analysis: Some(id), path: Some(p.display().to_string()) }));
                Task::none()
            }
            Message::ExportRScriptPicked(_, None) => Task::none(),
            Message::OpenPicked(None)
            | Message::SavePicked(None)
            | Message::ImportPicked(None)
            | Message::PasteNewTableText(None)
            | Message::ExportGraphPicked(None)
            | Message::ExportDataPicked(None)
            | Message::ExportCodePicked(None)
            | Message::GridPaste(None)
            | Message::HeaderPaste(None) => {
                self.after_save = None;
                Task::none()
            }
            Message::SetDataLayout(l) => {
                self.data_layout = l;
                Task::none()
            }
            Message::NewGraph(kind) => {
                self.commit_drafts();
                let Some(table) = self.active_table_id() else {
                    self.set_status("Import or create a data table first", true);
                    return Task::none();
                };
                // Highlighted numeric columns of the table on screen become the Y values.
                let y: Vec<String> = match (&self.engine.view, self.engine.project.table_by_id(&table)) {
                    (View::Table(id), Some(t)) if *id == table => self
                        .grid
                        .highlight
                        .iter()
                        .filter_map(|&c| t.columns.get(c))
                        .filter(|c| c.kind() == ColumnKind::Numeric)
                        .map(|c| c.name.clone())
                        .collect(),
                    _ => Vec::new(),
                };
                self.run(Command::GraphCreate(GraphCreateArgs {
                    table: Some(table),
                    kind,
                    x: None,
                    y,
                    group: None,
                    filters: Vec::new(),
                    format: None,
                    name: None,
                }));
                Task::none()
            }
            Message::NewDescriptive => {
                let Some(table) = self.active_table_id() else {
                    self.set_status("Import or create a data table first", true);
                    return Task::none();
                };
                self.run(Command::AnalysisCreateDescriptive(DescriptiveArgs { table: Some(table), ..Default::default() }));
                Task::none()
            }
            Message::SelectColumn(c) => {
                self.commit_drafts();
                self.commit_cell(None);
                self.commit_header();
                let h = &mut self.grid.highlight;
                if self.modifiers.shift() {
                    // Extend from the anchor, keeping what is already highlighted.
                    let a = self.grid.anchor.unwrap_or(c);
                    for i in a.min(c)..=a.max(c) {
                        if !h.contains(&i) {
                            h.push(i);
                        }
                    }
                } else if self.modifiers.command() {
                    if let Some(p) = h.iter().position(|&i| i == c) {
                        h.remove(p);
                    } else {
                        h.push(c);
                    }
                    self.grid.anchor = Some(c);
                } else {
                    *h = vec![c];
                    self.grid.anchor = Some(c);
                }
                // A header is now selected (not a cell): Ctrl+V pastes a table with its header row.
                self.grid.sel = None;
                self.grid.column = Some(c);
                self.grid.rows.clear();
                self.grid.row_anchor = None;
                self.grid.drag = Some(Drag::Columns);
                Task::none()
            }
            Message::HeaderDoubleClicked(c) => {
                self.commit_cell(None);
                let Some(name) = self.active_table().and_then(|t| t.columns.get(c)).map(|col| col.name.clone()) else { return Task::none() };
                self.grid.sel = None;
                self.grid.column = Some(c);
                self.grid.highlight = vec![c];
                self.grid.anchor = Some(c);
                self.grid.header_edit = Some((c, name));
                operation::focus(HEADER_EDITOR).chain(operation::select_all(HEADER_EDITOR))
            }
            Message::HeaderInput(s) => {
                if let Some((_, buf)) = &mut self.grid.header_edit {
                    *buf = s;
                }
                Task::none()
            }
            Message::HeaderSubmit => {
                self.commit_header();
                Task::none()
            }
            Message::HeaderPaste(Some(text)) => {
                let (Some(tid), Some(c)) = (self.table_view_id(), self.grid.column) else { return Task::none() };
                let block = tsv_block(&text);
                if !block.is_empty() {
                    self.run(Command::TableSetCells(SetCellsArgs { table: Some(tid), row: 0, column: ColumnRef::Index(c), values: block, header: true }));
                }
                Task::none()
            }

            Message::SelectRow(r) => {
                self.commit_drafts();
                self.commit_cell(None);
                self.commit_header();
                let rows = &mut self.grid.rows;
                if self.modifiers.shift() {
                    let a = self.grid.row_anchor.unwrap_or(r);
                    for i in a.min(r)..=a.max(r) {
                        if !rows.contains(&i) {
                            rows.push(i);
                        }
                    }
                } else if self.modifiers.command() {
                    match rows.iter().position(|&i| i == r) {
                        Some(p) => {
                            rows.remove(p);
                        }
                        None => rows.push(r),
                    }
                    self.grid.row_anchor = Some(r);
                } else {
                    *rows = vec![r];
                    self.grid.row_anchor = Some(r);
                }
                rows.sort_unstable();
                // A whole row is selected: no cell, no columns.
                self.grid.sel = None;
                self.grid.highlight.clear();
                self.grid.drag = Some(Drag::Rows);
                Task::none()
            }
            Message::DragOverRow(r) => {
                if self.grid.drag == Some(Drag::Rows) {
                    let a = self.grid.row_anchor.unwrap_or(r);
                    self.grid.rows = (a.min(r)..=a.max(r)).collect();
                }
                Task::none()
            }
            Message::DragOverColumn(c) => {
                if self.grid.drag == Some(Drag::Columns) {
                    let a = self.grid.anchor.unwrap_or(c);
                    self.grid.highlight = (a.min(c)..=a.max(c)).collect();
                    self.grid.column = Some(c);
                }
                Task::none()
            }
            Message::DragEnd => {
                self.grid.drag = None;
                Task::none()
            }

            Message::CellClicked(r, c) => {
                self.commit_cell(None);
                self.commit_header();
                self.grid.highlight.clear();
                self.grid.rows.clear();
                self.grid.row_anchor = None;
                self.grid.sel = Some((r, c));
                self.grid.column = Some(c);
                Task::none()
            }
            Message::CellDoubleClicked(r, c) => {
                self.commit_cell(None);
                self.commit_header();
                self.grid.highlight.clear();
                self.grid.rows.clear();
                self.grid.row_anchor = None;
                self.grid.sel = Some((r, c));
                self.grid.column = Some(c);
                self.start_edit(None)
            }
            Message::CellInput(s) => {
                self.grid.editing = Some(s);
                Task::none()
            }
            Message::CellSubmit => {
                self.commit_cell(Some((1, 0)));
                self.ensure_visible()
            }
            Message::GridScrolled(v) => {
                self.grid.scroll_y = v.absolute_offset().y;
                self.grid.view_h = v.bounds().height;
                Task::none()
            }
            Message::GridPaste(Some(text)) => {
                self.paste_into_grid(&text);
                Task::none()
            }

            Message::Draft(k, v) => {
                self.drafts.insert(k, v);
                Task::none()
            }
            Message::Commit(k) => {
                self.commit(k);
                Task::none()
            }
            Message::ToggleSection(i) => {
                self.collapsed[i] = !self.collapsed[i];
                Task::none()
            }
            Message::ToggleDropdown(id) => {
                self.open_dropdown = if self.open_dropdown.as_deref() == Some(id.as_str()) { None } else { Some(id) };
                Task::none()
            }
            Message::CloseDropdown => {
                self.open_dropdown = None;
                Task::none()
            }
            Message::AskDeleteTable(id) => {
                self.open_dropdown = None;
                self.commit_drafts();
                self.dialog = Some(crate::dialogs::Dialog::ConfirmDeleteTable(id));
                Task::none()
            }
            Message::Noop => Task::none(),
            Message::StatsTick => {
                // Redraws the graph once its statistics have arrived; views re-read results anyway.
                self.after_change();
                Task::none()
            }
            Message::TogglePanelSection(key, open) => {
                self.sections.insert(key, open);
                Task::none()
            }
            // Answering "save changes first?" carries on with the New or Open behind it,
            // which needs a task, so it is handled here rather than in `update_dialog`.
            Message::Dialog(crate::dialogs::DialogMsg::SaveThenDiscard(what)) => {
                self.dialog = None;
                self.save_then(what)
            }
            Message::Dialog(crate::dialogs::DialogMsg::Discard(what)) => {
                self.dialog = None;
                self.proceed(what)
            }
            Message::Dialog(m) => {
                self.commit_drafts();
                self.update_dialog(m);
                // A dialog message can change which graph is picked, and so which data
                // sets the dialog lists; `update_dialog` does not go through `run`.
                self.refresh_sets();
                Task::none()
            }
            Message::Key(event) => self.on_key(event),
            Message::Resized(width) => {
                // Crossing a threshold hands the decision back to the width, so widening the
                // window brings the panels back whatever was toggled while it was narrow.
                if panels_fit(self.window_width) != panels_fit(width) {
                    self.nav_open = None;
                    self.panel_open = None;
                }
                self.window_width = width;
                Task::none()
            }
            Message::TogglePanel(side) => {
                let (nav, panel) = panels_fit(self.window_width);
                match side {
                    Side::Left => self.nav_open = Some(!self.nav_open.unwrap_or(nav)),
                    Side::Right => self.panel_open = Some(!self.panel_open.unwrap_or(panel)),
                }
                Task::none()
            }
            Message::WindowOpened(id) => window::run(id, crate::placement::fit_to_work_area)
                .then(move |_| window::set_mode(id, window::Mode::Windowed))
                // The first width, so the edge panels are right before anything is dragged.
                .chain(window::size(id).map(|s: iced::Size| Message::Resized(s.width))),
            // The window is kept open (`exit_on_close_request(false)`) until unsaved
            // changes have been dealt with, the same question New and Open ask.
            Message::CloseRequested(id) => self.guard_unsaved(Pending::Close(id)),
        }
    }

    /// New and Open throw the current project away, so unsaved work is asked about first.
    /// The question is the app's own dialog, like the one for deleting a data table.
    fn guard_unsaved(&mut self, what: Pending) -> Task<Message> {
        self.open_dropdown = None;
        self.commit_drafts();
        if !self.engine.dirty {
            return self.proceed(what);
        }
        self.dialog = Some(crate::dialogs::Dialog::ConfirmDiscard(what));
        Task::none()
    }

    /// "Save" in that dialog: write the project out, then go on once it is on disk.
    pub(crate) fn save_then(&mut self, what: Pending) -> Task<Message> {
        if self.engine.path.is_some() {
            if self.run(Command::ProjectSave(NoArgs {})) {
                self.proceed(what)
            } else {
                Task::none()
            }
        } else {
            // No file yet: ask where to put it, and go on once it is written.
            self.after_save = Some(what);
            self.update(Message::SaveAs)
        }
    }

    /// New or Open, once the current project is no longer in the way.
    pub(crate) fn proceed(&mut self, what: Pending) -> Task<Message> {
        match what {
            Pending::New => {
                self.run(Command::ProjectNew(ProjectNewArgs::default()));
                Task::none()
            }
            Pending::Open => Task::perform(
                async {
                    rfd::AsyncFileDialog::new()
                        .set_title("Open Otto project")
                        .add_filter("Otto project", &["otto"])
                        .pick_file()
                        .await
                        .map(|f| f.path().to_path_buf())
                },
                Message::OpenPicked,
            ),
            Pending::Close(id) => window::close(id),
        }
    }

    // -- grid editing ---------------------------------------------------------

    fn start_edit(&mut self, initial: Option<String>) -> Task<Message> {
        let Some((r, c)) = self.grid.sel else { return Task::none() };
        let Some(t) = self.active_table() else { return Task::none() };
        let replace = initial.is_some();
        let current = t.columns.get(c).map(|col| col.display(r)).unwrap_or_default();
        self.grid.editing = Some(initial.unwrap_or(current));
        let focus = operation::focus(CELL_EDITOR);
        if replace {
            focus.chain(operation::move_cursor_to_end(CELL_EDITOR))
        } else {
            focus.chain(operation::select_all(CELL_EDITOR))
        }
    }

    /// Writes the edit buffer to the cell and optionally moves the selection by (rows, cols).
    fn commit_cell(&mut self, step: Option<(isize, isize)>) {
        let Some(value) = self.grid.editing.take() else { return };
        let (Some((r, c)), Some(tid)) = (self.grid.sel, self.table_view_id()) else { return };
        let unchanged = self.active_table().and_then(|t| t.columns.get(c)).is_some_and(|col| col.display(r) == value.trim());
        if !unchanged {
            self.run(Command::TableSetCells(SetCellsArgs {
                table: Some(tid),
                row: r,
                column: ColumnRef::Index(c),
                values: vec![vec![serde_json::Value::String(value)]],
                header: false,
            }));
        }
        if let Some(step) = step {
            self.move_sel(step);
        }
    }

    fn move_sel(&mut self, (dr, dc): (isize, isize)) {
        let Some(t) = self.active_table() else { return };
        let (rows, cols) = (t.nrows as isize, t.ncols() as isize);
        let (r, c) = self.grid.sel.map(|(r, c)| (r as isize, c as isize)).unwrap_or((0, 0));
        // Moving past the last row adds a row, like a spreadsheet.
        if r + dr >= rows && dr > 0 {
            if let Some(tid) = self.table_view_id() {
                self.run(Command::TableAddRows(AddRowsArgs { table: Some(tid), count: 1, at: None }));
            }
        }
        let rows = self.active_table().map_or(rows, |t| t.nrows as isize);
        let nr = (r + dr).clamp(0, (rows - 1).max(0));
        let nc = (c + dc).clamp(0, (cols - 1).max(0));
        self.grid.sel = Some((nr as usize, nc as usize));
        self.grid.column = Some(nc as usize);
    }

    fn ensure_visible(&self) -> Task<Message> {
        let Some((r, _)) = self.grid.sel else { return Task::none() };
        let top = r as f32 * ROW_HEIGHT;
        let bottom = top + ROW_HEIGHT;
        let y = if top < self.grid.scroll_y {
            top
        } else if bottom > self.grid.scroll_y + self.grid.view_h {
            bottom - self.grid.view_h
        } else {
            return Task::none();
        };
        operation::scroll_to(GRID_SCROLL, scrollable::AbsoluteOffset { x: None, y: Some(y.max(0.0)) })
    }

    fn paste_into_grid(&mut self, text: &str) {
        let (Some(tid), Some((r, c))) = (self.table_view_id(), self.grid.sel) else { return };
        let block = tsv_block(text);
        if block.is_empty() {
            return;
        }
        self.run(Command::TableSetCells(SetCellsArgs { table: Some(tid), row: r, column: ColumnRef::Index(c), values: block, header: false }));
    }

    /// Applies an in-place header rename.
    fn commit_header(&mut self) {
        let Some((c, name)) = self.grid.header_edit.take() else { return };
        let (Some(tid), Some(old)) = (self.table_view_id(), self.active_table().and_then(|t| t.columns.get(c)).map(|col| col.name.clone())) else {
            return;
        };
        let name = name.trim().to_string();
        if !name.is_empty() && name != old {
            self.run(Command::TableRenameColumn(RenameColumnArgs { table: Some(tid), column: old, new_name: name }));
        }
    }

    fn copy_cells(&self) -> Task<Message> {
        let (Some(t), Some((r, c))) = (self.active_table(), self.grid.sel) else { return Task::none() };
        let text = t.columns.get(c).map(|col| col.display(r)).unwrap_or_default();
        iced::clipboard::write(text)
    }

    fn on_key(&mut self, event: keyboard::Event) -> Task<Message> {
        if let keyboard::Event::ModifiersChanged(m) = event {
            // Header clicks read these (Ctrl / Shift to highlight several columns).
            self.modifiers = m;
            return Task::none();
        }
        let keyboard::Event::KeyPressed { key, modifiers, text, .. } = event else { return Task::none() };
        // While a dialog is open only Escape (cancel) is handled here.
        if self.dialog.is_some() {
            if key == Key::Named(Named::Escape) {
                self.dialog = None;
            }
            return Task::none();
        }
        // Renaming a header: the text field handles typing; Escape cancels.
        if self.grid.header_edit.is_some() {
            if key == Key::Named(Named::Escape) {
                self.grid.header_edit = None;
            }
            return Task::none();
        }
        let in_table = self.table_view_id().is_some();
        let in_graph = matches!(self.engine.view, View::Graph(_));
        let in_plotted = matches!(self.engine.view, View::Plotted(_));
        let editing = self.grid.editing.is_some();
        let header_selected = self.grid.sel.is_none() && self.grid.column.is_some() && !self.grid.highlight.is_empty();

        if modifiers.command() {
            let Key::Character(ch) = key.as_ref() else { return Task::none() };
            return match (ch.to_ascii_lowercase().as_str(), modifiers.shift()) {
                ("z", false) => self.update(Message::Cmd(Command::Undo(NoArgs {}))),
                ("z", true) | ("y", _) => self.update(Message::Cmd(Command::Redo(NoArgs {}))),
                ("s", false) => self.update(Message::Save),
                ("s", true) => self.update(Message::SaveAs),
                ("o", _) => self.update(Message::Open),
                ("i", _) => self.update(Message::Import),
                ("n", _) => self.update(Message::New),
                ("c", _) if in_graph => self.update(Message::Cmd(Command::GraphCopyVector(GraphRef { graph: self.graph_id() }))),
                ("c", _) if in_plotted => {
                    self.update(Message::Cmd(Command::GraphCopyData(CopyDataArgs { graph: self.graph_id(), layout: DataLayout::Wide })))
                }
                ("c", _) if in_table && !editing => self.copy_cells(),
                ("v", _) if in_table && !editing && self.grid.sel.is_some() => iced::clipboard::read().map(Message::GridPaste),
                ("v", _) if in_table && !editing && header_selected => iced::clipboard::read().map(Message::HeaderPaste),
                ("v", _) if !in_table => self.update(Message::PasteNewTable),
                ("a", _) if in_table && !editing => {
                    let Some(t) = self.active_table() else { return Task::none() };
                    let (rows, cols) = (t.nrows, t.ncols());
                    self.grid.sel = None;
                    self.grid.rows = (0..rows).collect();
                    self.grid.highlight = (0..cols).collect();
                    self.grid.row_anchor = Some(0);
                    self.grid.anchor = Some(0);
                    self.grid.column = (cols > 0).then_some(0);
                    Task::none()
                }
                _ => Task::none(),
            };
        }

        if !in_table {
            return Task::none();
        }
        if editing {
            return match key.as_ref() {
                Key::Named(Named::Escape) => {
                    self.grid.editing = None;
                    Task::none()
                }
                Key::Named(Named::Tab) => {
                    self.commit_cell(Some((0, if modifiers.shift() { -1 } else { 1 })));
                    Task::none()
                }
                _ => Task::none(),
            };
        }
        if self.grid.sel.is_none() {
            return Task::none();
        }
        match key.as_ref() {
            Key::Named(Named::ArrowUp) => self.step(-1, 0),
            Key::Named(Named::ArrowDown) => self.step(1, 0),
            Key::Named(Named::ArrowLeft) => self.step(0, -1),
            Key::Named(Named::ArrowRight) => self.step(0, 1),
            Key::Named(Named::Tab) => self.step(0, if modifiers.shift() { -1 } else { 1 }),
            Key::Named(Named::Enter) | Key::Named(Named::F2) => self.start_edit(None),
            Key::Named(Named::Delete) | Key::Named(Named::Backspace) => {
                let (Some(tid), Some((r, c))) = (self.table_view_id(), self.grid.sel) else { return Task::none() };
                self.run(Command::TableSetCells(SetCellsArgs {
                    table: Some(tid),
                    row: r,
                    column: ColumnRef::Index(c),
                    values: vec![vec![serde_json::Value::Null]],
                    header: false,
                }));
                Task::none()
            }
            _ => match text {
                Some(t) if !t.chars().any(char::is_control) && !modifiers.alt() => self.start_edit(Some(t.to_string())),
                _ => Task::none(),
            },
        }
    }

    fn step(&mut self, dr: isize, dc: isize) -> Task<Message> {
        self.move_sel((dr, dc));
        self.ensure_visible()
    }

    /// Columns of the active table usable as X / Y / Group for the active graph.
    pub fn numeric_columns(t: &otto_core::DataTable) -> Vec<String> {
        t.columns.iter().filter(|c| c.kind() == ColumnKind::Numeric).map(|c| c.name.clone()).collect()
    }
}

/// Tab-separated clipboard text (e.g. from Excel) as a block of cell values.
fn tsv_block(text: &str) -> Vec<Vec<serde_json::Value>> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.split('\t').map(|s| serde_json::Value::String(s.trim().to_string())).collect())
        .collect()
}

/// Starts the app's MCP endpoint and forwards Claude's tool calls into the update loop.
fn mcp_worker() -> impl iced::futures::Stream<Item = Message> {
    iced::stream::channel(64, async |mut output: iced::futures::channel::mpsc::Sender<Message>| {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<McpRequest>(32);
        match otto_mcp::serve_http(otto_mcp::ChannelBackend::new(tx), otto_mcp::DEFAULT_PORT, true).await {
            Ok(handle) => {
                let _ = output.send(Message::McpStarted(Ok(handle.url().to_string()))).await;
                while let Some(req) = rx.recv().await {
                    let env = McpEnvelope(Arc::new(Mutex::new(Some(req))));
                    if output.send(Message::Mcp(env)).await.is_err() {
                        break;
                    }
                }
                drop(handle);
            }
            Err(e) => {
                let _ = output.send(Message::McpStarted(Err(e.to_string()))).await;
            }
        }
        std::future::pending::<()>().await;
    })
}
