//! Otto engine: owns the project and applies `Command`s with undo/redo.
//! The GUI and the MCP server both drive Otto exclusively through [`Engine::apply`].

pub mod clipboard;
pub mod column_tests;
mod contingency;
pub mod diagnostics;
pub mod commands;
mod exec;
pub mod results;
pub mod survival;
pub mod two_way;
pub mod xy_analyses;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use otto_core::{CoreError, Project};
use otto_plot::{PlotError, Rendered};

pub use commands::*;

const UNDO_LIMIT: usize = 200;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EngineError {
    #[error("unknown tool '{0}'")]
    UnknownTool(String),
    #[error("invalid arguments for {tool}: {message}")]
    BadArgs { tool: String, message: String },
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error(transparent)]
    Plot(#[from] PlotError),
    #[error("{0}")]
    Io(String),
    #[error("clipboard: {0}")]
    Clipboard(String),
    /// The statistics engine (R) is missing, not set up or failed.
    #[error("{0}")]
    Engine(String),
    /// R is still calculating the result in the background (GUI only).
    #[error("Calculating in R…")]
    Calculating,
}

impl From<otto_r::RError> for EngineError {
    fn from(e: otto_r::RError) -> Self {
        if e == otto_r::RError::Pending {
            EngineError::Calculating
        } else if e.is_engine_problem() {
            EngineError::Engine(e.to_string())
        } else {
            EngineError::Invalid(e.to_string())
        }
    }
}

/// What the app's centre area shows.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "section", content = "id", rename_all = "snake_case")]
pub enum View {
    #[default]
    Welcome,
    Table(String),
    /// The plotted table of a graph (by graph id).
    Plotted(String),
    Result(String),
    Graph(String),
}

/// The result of a command.
#[derive(Clone, Debug, Default)]
pub struct Outcome {
    /// One-line human-readable summary.
    pub message: String,
    /// Structured result (ids, summaries, rows…).
    pub data: Option<serde_json::Value>,
    /// A rendered graph preview.
    pub image_png: Option<Vec<u8>>,
    /// Text payload (CSV, R code, SVG…).
    pub text: Option<String>,
    /// Analyses this command made out of date (their graph changed under them).
    pub out_of_date: Vec<String>,
    /// Set when the command went through but the graph cannot be drawn as it now stands,
    /// e.g. a mapping only half changed. Why, so that a caller sees it as something other
    /// than a turn of phrase in `message`. The change itself stands: graph settings are
    /// made one call at a time, and a half-finished mapping has to be allowed to exist.
    pub needs_attention: Option<String>,
}

impl Outcome {
    pub fn msg(message: impl Into<String>) -> Self {
        Outcome { message: message.into(), ..Default::default() }
    }

    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }

    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }
}

/// Everything deleting a data table removes along with it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TableDeletion {
    /// Quick graph ids.
    pub quick: Vec<String>,
    /// Final graph ids (each with its locked copy and statistics).
    pub finals: Vec<String>,
    /// Statistics and descriptive results.
    pub results: usize,
}

impl TableDeletion {
    /// ", 2 quick graphs, 1 final graph and 3 results", or "" when nothing else goes.
    pub fn describe(&self) -> String {
        let n = |k: usize, one: &str, many: &str| format!("{k} {}", if k == 1 { one } else { many });
        let mut parts = Vec::new();
        if !self.quick.is_empty() {
            parts.push(n(self.quick.len(), "quick graph", "quick graphs"));
        }
        if !self.finals.is_empty() {
            parts.push(n(self.finals.len(), "final graph", "final graphs"));
        }
        if self.results > 0 {
            parts.push(n(self.results, "result", "results"));
        }
        match parts.len() {
            0 => String::new(),
            1 => format!(" and {}", parts[0]),
            _ => {
                let last = parts.pop().unwrap_or_default();
                format!(", {} and {last}", parts.join(", "))
            }
        }
    }
}

#[derive(Clone, Debug)]
struct Snapshot {
    project: Project,
    view: View,
}

pub struct Engine {
    pub project: Project,
    /// File the project was opened from / saved to.
    pub path: Option<PathBuf>,
    /// Unsaved changes.
    pub dirty: bool,
    pub view: View,
    pub zoom: Zoom,
    /// Increments after every successful command, so views know when to refresh.
    pub revision: u64,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// Out-of-date analyses as of the last command; see [`Engine::out_of_date`].
    stale: Vec<(String, Vec<String>)>,
}

impl Default for Engine {
    fn default() -> Self {
        Engine::new()
    }
}

impl Engine {
    pub fn new() -> Self {
        Engine {
            project: Project::default(),
            path: None,
            dirty: false,
            view: View::Welcome,
            zoom: Zoom::Fit,
            revision: 0,
            undo: Vec::new(),
            redo: Vec::new(),
            stale: Vec::new(),
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Applies a command. Failed commands leave the project unchanged.
    pub fn apply(&mut self, cmd: Command) -> Result<Outcome, EngineError> {
        let snapshot = cmd.mutates().then(|| Snapshot { project: self.project.clone(), view: self.view.clone() });
        let resets_history = matches!(cmd, Command::ProjectNew(_) | Command::ProjectOpen(_));
        // Commands that do not change the project leave the stored answer standing; undo
        // and redo do change it without taking a snapshot, so they are named here.
        let restates = snapshot.is_some() || matches!(cmd, Command::Undo(_) | Command::Redo(_));
        let stale_before: Vec<String> = if snapshot.is_some() && !resets_history {
            self.out_of_date().into_iter().map(|(id, _)| id).collect()
        } else {
            Vec::new()
        };
        let mut result = self.execute(cmd);
        if result.is_ok() && restates {
            self.stale = self.out_of_date();
        }
        if let (Ok(out), true) = (&mut result, snapshot.is_some() && !resets_history) {
            let newly: Vec<(String, Vec<String>)> = self.stale.iter().filter(|(id, _)| !stale_before.contains(id)).cloned().collect();
            if !newly.is_empty() {
                let names: Vec<String> = newly
                    .iter()
                    .filter_map(|(id, _)| self.project.analyses.iter().find(|a| &a.id == id))
                    .map(|a| format!("{} \"{}\"", a.id, a.name))
                    .collect();
                out.message = format!(
                    "{}. Statistics out of date ({}): {} no longer match the graph and are hidden; \
                     analysis_recalculate reruns them on the bars shown, analysis_edit_* with data_sets sets them up again",
                    out.message,
                    newly[0].1.join("; "),
                    names.join(", ")
                );
                out.out_of_date = newly.into_iter().map(|(id, _)| id).collect();
            }
        }
        match (&result, snapshot) {
            (Ok(_), Some(snap)) => {
                if resets_history {
                    self.undo.clear();
                    self.redo.clear();
                } else if snap.project != self.project {
                    self.undo.push(snap);
                    if self.undo.len() > UNDO_LIMIT {
                        self.undo.remove(0);
                    }
                    self.redo.clear();
                    self.dirty = true;
                }
            }
            (Err(_), Some(snap)) => {
                self.project = snap.project;
                self.view = snap.view;
            }
            _ => {}
        }
        if result.is_ok() {
            self.revision += 1;
        }
        result
    }

    /// Parses and applies an MCP-style tool call.
    pub fn call(&mut self, tool: &str, args: serde_json::Value) -> Result<Outcome, EngineError> {
        let cmd = Command::parse(tool, args)?;
        self.apply(cmd)
    }

    fn undo_step(&mut self) -> Result<Outcome, EngineError> {
        let snap = self.undo.pop().ok_or_else(|| EngineError::Invalid("nothing to undo".into()))?;
        let current = Snapshot { project: std::mem::replace(&mut self.project, snap.project), view: self.view.clone() };
        self.view = snap.view;
        self.redo.push(current);
        self.dirty = true;
        Ok(Outcome::msg("Undone"))
    }

    fn redo_step(&mut self) -> Result<Outcome, EngineError> {
        let snap = self.redo.pop().ok_or_else(|| EngineError::Invalid("nothing to redo".into()))?;
        let current = Snapshot { project: std::mem::replace(&mut self.project, snap.project), view: self.view.clone() };
        self.view = snap.view;
        self.undo.push(current);
        self.dirty = true;
        Ok(Outcome::msg("Redone"))
    }

    /// The data a graph plots (filtered, summarised), without drawing it or running its
    /// statistics: what plotted tables and data exports are made from.
    pub fn plot_data(&self, key: &str) -> Result<otto_plot::PlotData, EngineError> {
        let spec = self.project.graph(key).ok_or_else(|| EngineError::NotFound(format!("graph '{key}'")))?;
        let table = self
            .project
            .table_by_id(&spec.table_id)
            .ok_or_else(|| EngineError::NotFound(format!("table {} of graph '{}'", spec.table_id, spec.name)))?;
        Ok(otto_plot::distill::distill(spec, table)?)
    }

    /// Renders a graph by id or name.
    pub fn render_graph(&self, key: &str) -> Result<Rendered, EngineError> {
        let gi = self.project.graph_index(key).ok_or_else(|| EngineError::NotFound(format!("graph '{key}'")))?;
        let spec = &self.project.graphs[gi];
        let table = self
            .project
            .table_by_id(&spec.table_id)
            .ok_or_else(|| EngineError::NotFound(format!("table {} of graph '{}'", spec.table_id, spec.name)))?;
        // Significance brackets from the t tests / ANOVAs of this graph (not from those
        // that are out of date: they no longer describe what the graph shows).
        let reports: Vec<results::Report> = self
            .project
            .analyses
            .iter()
            .filter(|a| exec::analysis_graph(a) == Some(spec.id.as_str()))
            .filter(|a| column_tests::out_of_date(&self.project, a).is_none())
            .filter_map(|a| results::compute(&self.project, a).ok())
            .collect();
        let curves: Vec<otto_plot::Curve> = reports.iter().flat_map(|r| r.curves.iter().cloned()).collect();
        let brackets: Vec<otto_plot::Bracket> = reports.into_iter().flat_map(|r| r.brackets).collect();
        // One bracket per pair of bars: a later analysis replaces an earlier one.
        let mut unique: Vec<otto_plot::Bracket> = Vec::new();
        for b in brackets.into_iter().rev() {
            if !unique.iter().any(|u| (u.a == b.a && u.b == b.b) || (u.a == b.b && u.b == b.a)) {
                unique.push(b);
            }
        }
        unique.reverse();
        Ok(otto_plot::render_full(spec, table, &unique, &curves)?)
    }

    /// Out-of-date t tests / ANOVAs with the reasons, in project order.
    /// Out-of-date analyses (id → reasons), worked out from the project as it is now.
    /// Each analysed graph costs two distills of the data table, so anything that only
    /// needs the answer as of the last command should read [`Engine::stale`] instead.
    pub fn out_of_date(&self) -> Vec<(String, Vec<String>)> {
        self.project
            .analyses
            .iter()
            .filter_map(|a| column_tests::out_of_date(&self.project, a).map(|r| (a.id.clone(), r)))
            .collect()
    }

    /// Out-of-date analyses as [`Engine::apply`] last worked them out. The same answer as
    /// [`Engine::out_of_date`] for anything that changes the project through commands,
    /// which is everything except a test reaching into `project` directly.
    pub fn stale(&self) -> &[(String, Vec<String>)] {
        &self.stale
    }

    /// Graph whose bars an analysis (t test / ANOVA) compares.
    pub fn analysis_graph(&self, analysis_id: &str) -> Option<String> {
        self.project.analyses.iter().find(|a| a.id == analysis_id).and_then(exec::analysis_graph).map(str::to_string)
    }

    /// Analyses (t tests / ANOVA) that use the bars of a graph.
    pub fn graph_analyses(&self, graph_id: &str) -> Vec<&otto_core::AnalysisSpec> {
        self.project.analyses.iter().filter(|a| exec::analysis_graph(a) == Some(graph_id)).collect()
    }

    /// Display title, e.g. "Experiment 3.otto*".
    pub fn title(&self) -> String {
        let name = self
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| self.project.name.clone());
        format!("{name}{}", if self.dirty { " •" } else { "" })
    }
}

#[cfg(test)]
mod tests;
