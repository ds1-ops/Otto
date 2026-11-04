//! Implementation of every command.

use std::path::{Path, PathBuf};

use serde_json::json;

use otto_core::analysis::GraphBasis;
use otto_core::import::{read_file, table_from_file_with, table_from_text_with, ImportOptions};
use otto_core::{AnalysisKind, AnalysisSpec, Column, ColumnData, ColumnKind, DataShape, DataTable, ExportFormat, GraphKind, GraphSpec, Project};

use crate::commands::*;
use crate::{clipboard, results, Engine, EngineError, Outcome, View};

fn not_found(what: &str, key: &str, options: impl Iterator<Item = String>) -> EngineError {
    let list: Vec<String> = options.collect();
    if list.is_empty() {
        EngineError::NotFound(format!("{what} '{key}' not found (there are none yet)"))
    } else {
        EngineError::NotFound(format!("{what} '{key}' not found (available: {})", list.join(", ")))
    }
}

fn key(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

/// How many rows of plotted data a tool result carries before the rest goes to a file.
/// The same cap `table_get` defaults to, for the same reason.
const INLINE_ROWS: usize = 200;

fn io_err(path: &Path, e: impl std::fmt::Display) -> EngineError {
    EngineError::Io(format!("{}: {e}", path.display()))
}

fn absolute(path: &str) -> PathBuf {
    let p = PathBuf::from(path);
    std::path::absolute(&p).unwrap_or(p)
}

impl Engine {
    // -- target resolution --------------------------------------------------

    fn tables_list(&self) -> impl Iterator<Item = String> + '_ {
        self.project.tables.iter().map(|t| format!("{} \"{}\"", t.id, t.name))
    }

    fn graphs_list(&self) -> impl Iterator<Item = String> + '_ {
        self.project.graphs.iter().map(|g| format!("{} \"{}\"", g.id, g.name))
    }

    fn analyses_list(&self) -> impl Iterator<Item = String> + '_ {
        self.project.analyses.iter().map(|a| format!("{} \"{}\"", a.id, a.name))
    }

    pub(crate) fn table_idx(&self, k: &Option<String>) -> Result<usize, EngineError> {
        if let Some(k) = key(k) {
            return self.project.table_index(k).ok_or_else(|| not_found("table", k, self.tables_list()));
        }
        let active = match &self.view {
            View::Table(id) => Some(id.clone()),
            View::Graph(id) | View::Plotted(id) => self.project.graphs.iter().find(|g| &g.id == id).map(|g| g.table_id.clone()),
            View::Result(id) => self.project.analyses.iter().find(|a| &a.id == id).map(|a| a.table_id.clone()),
            View::Welcome => None,
        };
        // Otherwise the only data table (locked copies of final graphs don't count).
        let data_tables: Vec<usize> = (0..self.project.tables.len()).filter(|&i| self.project.tables[i].locked_by.is_none()).collect();
        active
            .and_then(|id| self.project.tables.iter().position(|t| t.id == id))
            .or_else(|| (data_tables.len() == 1).then(|| data_tables[0]))
            .ok_or_else(|| {
                if self.project.tables.is_empty() {
                    EngineError::Invalid("there are no tables yet; import or create one first".into())
                } else {
                    EngineError::Invalid(format!("which table? pass `table` ({})", self.tables_list().collect::<Vec<_>>().join(", ")))
                }
            })
    }

    pub(crate) fn graph_idx(&self, k: &Option<String>) -> Result<usize, EngineError> {
        if let Some(k) = key(k) {
            return self.project.graph_index(k).ok_or_else(|| not_found("graph", k, self.graphs_list()));
        }
        let active = match &self.view {
            View::Graph(id) | View::Plotted(id) => self.project.graphs.iter().position(|g| &g.id == id),
            _ => None,
        };
        active.or_else(|| (self.project.graphs.len() == 1).then_some(0)).ok_or_else(|| {
            if self.project.graphs.is_empty() {
                EngineError::Invalid("there are no graphs yet; create one with graph_create".into())
            } else {
                EngineError::Invalid(format!("which graph? pass `graph` ({})", self.graphs_list().collect::<Vec<_>>().join(", ")))
            }
        })
    }

    fn analysis_idx(&self, k: &Option<String>) -> Result<usize, EngineError> {
        if let Some(k) = key(k) {
            return self.project.analysis_index(k).ok_or_else(|| not_found("analysis", k, self.analyses_list()));
        }
        let active = match &self.view {
            View::Result(id) => self.project.analyses.iter().position(|a| &a.id == id),
            _ => None,
        };
        active.or_else(|| (self.project.analyses.len() == 1).then_some(0)).ok_or_else(|| {
            EngineError::Invalid(format!("which analysis? pass `analysis` ({})", self.analyses_list().collect::<Vec<_>>().join(", ")))
        })
    }

    /// What deleting data table `t` takes with it: its quick graphs, the final graphs made
    /// from it (with their locked copies and statistics) and its results.
    pub fn table_deletion(&self, t: &DataTable) -> crate::TableDeletion {
        let quick: Vec<String> = self.project.graphs.iter().filter(|g| !g.is_final() && g.table_id == t.id).map(|g| g.id.clone()).collect();
        let finals: Vec<String> = self
            .project
            .graphs
            .iter()
            .filter(|g| {
                // Older final graphs only know the table's name.
                g.locked.as_ref().is_some_and(|l| l.table_id == t.id || (l.table_id.is_empty() && l.from_table == t.name))
            })
            .map(|g| g.id.clone())
            .collect();
        let results = self
            .project
            .analyses
            .iter()
            .filter(|a| a.table_id == t.id || analysis_graph(a).is_some_and(|g| finals.iter().any(|f| f == g)))
            .count();
        crate::TableDeletion { quick, finals, results }
    }

    /// A graph whose type, data, error bars and axes may change: a quick graph. Final graphs
    /// only change format, colors, legend and statistics, so their statistics always match.
    fn quick_graph_idx(&self, k: &Option<String>, what: &str) -> Result<usize, EngineError> {
        let gi = self.graph_idx(k)?;
        let g = &self.project.graphs[gi];
        if let Some(lock) = &g.locked {
            let from = self.project.graph(&lock.from_graph).map(|q| format!(" \"{}\"", q.name)).unwrap_or_default();
            return Err(EngineError::Invalid(format!(
                "\"{}\" is a final graph: its {what} can't change (only format, colors, legend, axis titles and statistics). \
                 Change it on the quick graph{from} and make a new final graph",
                g.name
            )));
        }
        Ok(gi)
    }

    /// A table whose values may be changed (not the locked data copy of a final graph).
    fn editable_table_idx(&self, k: &Option<String>) -> Result<usize, EngineError> {
        let ti = self.table_idx(k)?;
        let t = &self.project.tables[ti];
        match &t.locked_by {
            None => Ok(ti),
            Some(g) => {
                let graph = self.project.graph(g).map(|g| format!("{} \"{}\"", g.id, g.name)).unwrap_or_else(|| g.clone());
                Err(EngineError::Invalid(format!(
                    "{} is the locked data copy of final graph {graph}: its values, rows and columns can't change. \
                     Edit the data table it came from and finalize again, or duplicate {} for an editable copy",
                    t.id, t.id
                )))
            }
        }
    }

    /// Renames a column and every reference to it (graphs, analyses).
    fn rename_column_everywhere(&mut self, ti: usize, old: &str, new: &str) -> Result<(), EngineError> {
        let t = &mut self.project.tables[ti];
        t.rename_column(old, new)?;
        let (tid, new) = (t.id.clone(), new.trim().to_string());
        for g in self.project.graphs.iter_mut().filter(|g| g.table_id == tid) {
            g.rename_column(old, &new);
        }
        for an in self.project.analyses.iter_mut().filter(|x| x.table_id == tid) {
            match &mut an.kind {
                AnalysisKind::Descriptive { y, group } => {
                    y.iter_mut().chain(group.iter_mut()).filter(|c| *c == old).for_each(|c| *c = new.clone());
                }
                AnalysisKind::TTest(t) => t.match_by.iter_mut().filter(|c| *c == old).for_each(|c| *c = new.clone()),
                AnalysisKind::OneWayAnova(x) => x.match_by.iter_mut().filter(|c| *c == old).for_each(|c| *c = new.clone()),
                _ => {}
            }
        }
        for b in self.project.analyses.iter_mut().filter_map(|x| x.basis.as_mut()).filter(|b| b.table_id == tid) {
            b.rename_column(old, &new);
        }
        Ok(())
    }

    /// Canonical column name (exact or case-insensitive match).
    fn column_name(table: &DataTable, name: &str) -> Result<String, EngineError> {
        Ok(table.require_column(name)?.name.clone())
    }

    /// Canonical names for a column list ("" / [] = none), without duplicates.
    fn column_names(table: &DataTable, names: otto_core::OneOrMany) -> Result<Vec<String>, EngineError> {
        let mut out: Vec<String> = Vec::new();
        for n in names.into_vec() {
            let c = Self::column_name(table, &n)?;
            if !out.contains(&c) {
                out.push(c);
            }
        }
        Ok(out)
    }

    fn graph_mut(&mut self, k: &Option<String>) -> Result<&mut GraphSpec, EngineError> {
        let gi = self.graph_idx(k)?;
        Ok(&mut self.project.graphs[gi])
    }

    /// Render check after a graph change: problems are reported, not fatal, so the user
    /// (or Claude) can fix the mapping step by step. Returns the status line and, when the
    /// graph cannot be drawn, the reason — carried on the outcome as `needs_attention` so a
    /// caller does not have to read it out of the message.
    fn graph_status_full(&self, gi: usize) -> (String, Option<String>) {
        let g = &self.project.graphs[gi];
        match self.render_graph(&g.id) {
            Ok(r) => {
                let (w, h) = r.size_mm();
                (format!(
                    "{} \"{}\": {} of {} ({} points, {:.1} × {:.1} mm figure)",
                    g.id,
                    g.name,
                    g.kind.label(),
                    r.data.y_title,
                    r.data.points.len(),
                    w,
                    h
                ), None)
            }
            Err(e) => (format!("{} \"{}\" needs attention: {e}", g.id, g.name), Some(e.to_string())),
        }
    }

    fn graph_status(&self, gi: usize) -> String {
        self.graph_status_full(gi).0
    }

    fn graph_outcome(&self, gi: usize, verb: &str) -> Outcome {
        let g = &self.project.graphs[gi];
        let (status, attention) = self.graph_status_full(gi);
        let mut out = Outcome::msg(format!("{verb} {status}")).with_data(serde_json::to_value(g).unwrap_or_default());
        out.needs_attention = attention;
        out
    }

    // -- dispatcher ---------------------------------------------------------

    pub(crate) fn execute(&mut self, cmd: Command) -> Result<Outcome, EngineError> {
        use Command::*;
        match cmd {
            ProjectNew(a) => {
                self.project = Project::new(a.name.unwrap_or_else(|| "Untitled".into()));
                self.path = None;
                self.dirty = false;
                self.view = View::Welcome;
                Ok(Outcome::msg("New project"))
            }
            ProjectOpen(a) => {
                let path = absolute(&a.path);
                self.project = Project::load(&path)?;
                self.path = Some(path.clone());
                let migrated = self.finalize_legacy_analyses();
                self.dirty = migrated > 0;
                self.view = self
                    .project
                    .graphs
                    .first()
                    .map(|g| View::Graph(g.id.clone()))
                    .or_else(|| self.project.tables.first().map(|t| View::Table(t.id.clone())))
                    .unwrap_or_default();
                let note = if migrated > 0 {
                    format!("; statistics of {migrated} graph(s) moved to new final graphs (locked data copies)")
                } else {
                    String::new()
                };
                Ok(Outcome::msg(format!("Opened {}{note}", path.display())))
            }
            ProjectSave(_) => {
                let path = self.path.clone().ok_or_else(|| {
                    EngineError::Invalid("the project has no file yet; use project_save_as with a path".into())
                })?;
                self.save_to(&path)
            }
            ProjectSaveAs(a) => {
                let mut path = absolute(&a.path);
                if path.extension().is_none() {
                    path.set_extension(otto_core::project::FILE_EXTENSION);
                }
                self.save_to(&path)
            }
            ProjectSummary(_) => Ok(self.summary()),
            Undo(_) => self.undo_step(),
            Redo(_) => self.redo_step(),
            UiNavigate(a) => self.navigate(a),
            // The window itself is shown by whoever hosts the engine (the app, or the MCP bridge).
            UiShowWindow(_) => Ok(Outcome::msg("The Otto window is showing")),
            UiSetZoom(a) => {
                self.zoom = a.zoom;
                Ok(Outcome::msg(format!("Zoom {}", a.zoom.label())))
            }

            TableCreate(a) => {
                let id = self.project.next_table_id();
                let name = a.name.unwrap_or_else(|| Project::numbered_name(self.project.tables.iter().map(|t| t.name.as_str()), "Data"));
                let rows = a.rows.unwrap_or(12);
                let mut t = DataTable::new(id.clone(), name);
                t.nrows = rows;
                let cols = if a.columns.is_empty() {
                    vec![
                        ColumnDef { name: "Group".into(), kind: ColumnKind::Categorical },
                        ColumnDef { name: "X".into(), kind: ColumnKind::Numeric },
                        ColumnDef { name: "Y".into(), kind: ColumnKind::Numeric },
                    ]
                } else {
                    a.columns
                };
                for c in cols {
                    t.add_column(Column::new(c.name, c.kind, rows), None)?;
                }
                let msg = format!("Created table {} \"{}\" ({} columns × {rows} rows)", t.id, t.name, t.ncols());
                self.project.tables.push(t);
                self.view = View::Table(id.clone());
                Ok(Outcome::msg(msg).with_data(json!({ "table": id })))
            }
            TableImportFile(a) => {
                let path = absolute(&a.path);
                let id = self.project.next_table_id();
                let options = a.options.unwrap_or_default();
                let mut t = table_from_file_with(&id, &path, a.sheet.as_deref(), &options)?;
                t.name = self.project.unique_table_name(&a.name.unwrap_or(t.name));
                self.add_imported(t)
            }
            TableImportPreview(a) => {
                let path = absolute(&a.path);
                let options = a.options.unwrap_or_default();
                let (grid, name, fmt) = read_file(&path, a.sheet.as_deref(), &options)?;
                let p = otto_core::import::preview(&grid, &options, fmt.decimal_comma);
                let shown = a.rows.unwrap_or(20).min(p.rows.len());
                let columns: Vec<serde_json::Value> = (0..p.names.len())
                    .map(|c| json!({ "name": p.names[c], "kind": p.kinds[c], "included": p.include[c] }))
                    .collect();
                let rows: Vec<serde_json::Value> = p.rows.iter().take(shown).map(|(role, cells)| json!({ "role": role, "cells": cells })).collect();
                Ok(Outcome::msg(format!(
                    "{} \"{}\": {} columns × {} data rows of {} rows in the file (separator {:?}, decimal {}, showing {shown})",
                    path.display(),
                    name,
                    p.names.len(),
                    p.data_rows,
                    p.total_rows,
                    fmt.delimiter as char,
                    if fmt.decimal_comma { "comma" } else { "point" },
                ))
                .with_data(json!({ "name": name, "columns": columns, "rows": rows, "data_rows": p.data_rows, "total_rows": p.total_rows })))
            }
            TableImportText(a) => {
                let id = self.project.next_table_id();
                let name = a.name.unwrap_or_else(|| Project::numbered_name(self.project.tables.iter().map(|t| t.name.as_str()), "Data"));
                let options = a.options.unwrap_or_else(|| ImportOptions::with_header(a.has_header));
                let t = table_from_text_with(&id, &name, &a.text, &options)?;
                self.add_imported(t)
            }
            TableDescribe(a) => {
                let t = &self.project.tables[self.table_idx(&a.table)?];
                Ok(Outcome::msg(format!("{} \"{}\": {} rows × {} columns", t.id, t.name, t.nrows, t.ncols())).with_data(describe(t)))
            }
            TableGet(a) => {
                let t = &self.project.tables[self.table_idx(&a.table)?];
                let end = (a.offset + a.limit).min(t.nrows);
                let msg = format!("{} \"{}\" rows {}–{} of {}", t.id, t.name, a.offset.min(end), end, t.nrows);
                Ok(match a.format {
                    TextFormat::Json => Outcome::msg(msg).with_data(json!({ "rows": t.rows_json(a.offset, a.limit) })),
                    TextFormat::Csv => Outcome::msg(msg).with_text(t.rows_delimited(a.offset..end, b',')),
                })
            }
            TableSetCells(a) => {
                let ti = self.editable_table_idx(&a.table)?;
                let t = &mut self.project.tables[ti];
                let col = match &a.column {
                    ColumnRef::Index(i) => *i,
                    ColumnRef::Name(n) => t.require_column_index(n)?,
                };
                let (names, data) = match (a.header, a.values.split_first()) {
                    (true, Some((names, data))) => (Some(names.clone()), data),
                    _ => (None, &a.values[..]),
                };
                let mut renamed = 0;
                if let Some(names) = names {
                    // A pasted header row names the columns it lands on (new columns as needed).
                    let width = a.values.iter().map(Vec::len).max().unwrap_or(0);
                    t.ensure_size(t.nrows, col + width);
                    for (dc, v) in names.iter().enumerate() {
                        let name = match v {
                            serde_json::Value::String(s) => s.trim().to_string(),
                            serde_json::Value::Null => String::new(),
                            other => other.to_string(),
                        };
                        let old = self.project.tables[ti].columns[col + dc].name.clone();
                        if name.is_empty() || name == old {
                            continue;
                        }
                        let others = self.project.tables[ti].columns.iter().enumerate().any(|(i, c)| i != col + dc && c.name.eq_ignore_ascii_case(&name));
                        let name = if others { self.project.tables[ti].unique_column_name(&name) } else { name };
                        self.rename_column_everywhere(ti, &old, &name)?;
                        renamed += 1;
                    }
                }
                let t = &mut self.project.tables[ti];
                t.set_cells(a.row, col, data)?;
                let n: usize = data.iter().map(Vec::len).sum();
                let headers = if renamed > 0 { format!(" and {renamed} column names") } else { String::new() };
                Ok(Outcome::msg(format!("Set {n} cells{headers} in \"{}\"", t.name)))
            }
            TableAddRows(a) => {
                let ti = self.editable_table_idx(&a.table)?;
                let t = &mut self.project.tables[ti];
                let at = a.at.unwrap_or(t.nrows);
                t.insert_rows(at, a.count);
                Ok(Outcome::msg(format!("Added {} rows to \"{}\" ({} rows)", a.count, t.name, t.nrows)))
            }
            TableDeleteRows(a) => {
                let ti = self.editable_table_idx(&a.table)?;
                let t = &mut self.project.tables[ti];
                let before = t.nrows;
                t.delete_rows(&a.rows);
                Ok(Outcome::msg(format!("Deleted {} rows from \"{}\"", before - t.nrows, t.name)))
            }
            TableAddColumn(a) => {
                let ti = self.editable_table_idx(&a.table)?;
                let t = &mut self.project.tables[ti];
                let mut col = Column::new(a.name.trim(), a.kind, t.nrows.max(a.values.len()));
                for (r, v) in a.values.iter().enumerate() {
                    col.set_from_json(r, v)?;
                }
                t.add_column(col, a.at)?;
                Ok(Outcome::msg(format!("Added {} column \"{}\" to \"{}\"", a.kind.label().to_lowercase(), a.name.trim(), t.name)))
            }
            TableDeleteColumns(a) => {
                let ti = self.editable_table_idx(&a.table)?;
                let t = &mut self.project.tables[ti];
                let mut idx = Vec::new();
                for c in &a.columns {
                    idx.push(t.require_column_index(c)?);
                }
                idx.sort_unstable();
                idx.dedup();
                for i in idx.iter().rev() {
                    t.columns.remove(*i);
                }
                Ok(Outcome::msg(format!("Deleted {} columns from \"{}\"", idx.len(), t.name)))
            }
            TableRenameColumn(a) => {
                // Allowed on locked copies too: it changes a label, not the data.
                let ti = self.table_idx(&a.table)?;
                let old = Self::column_name(&self.project.tables[ti], &a.column)?;
                let new = a.new_name.trim().to_string();
                self.rename_column_everywhere(ti, &old, &new)?;
                Ok(Outcome::msg(format!("Renamed column \"{old}\" to \"{new}\"")))
            }
            TableSetColumnType(a) => {
                let ti = self.editable_table_idx(&a.table)?;
                let t = &mut self.project.tables[ti];
                let ci = t.require_column_index(&a.column)?;
                let ty = a.column_type();
                t.columns[ci] = t.columns[ci].convert_to(ty)?;
                Ok(Outcome::msg(format!("\"{}\" is now {}", t.columns[ci].name, ty.label().to_lowercase())))
            }
            TableSetCategoryOrder(a) => {
                let ti = self.table_idx(&a.table)?;
                let t = &mut self.project.tables[ti];
                let ci = t.require_column_index(&a.column)?;
                t.columns[ci].set_level_order(&a.levels)?;
                Ok(Outcome::msg(format!("Level order of \"{}\": {}", t.columns[ci].name, a.levels.join(", "))))
            }
            TableRenameLevel(a) => {
                let ti = self.table_idx(&a.table)?;
                let t = &mut self.project.tables[ti];
                let ci = t.require_column_index(&a.column)?;
                t.columns[ci].rename_level(&a.level, &a.new_name)?;
                let (tid, cname, new) = (t.id.clone(), t.columns[ci].name.clone(), a.new_name.trim().to_string());
                for g in self.project.graphs.iter_mut().filter(|g| g.table_id == tid) {
                    for f in g.filters.iter_mut().filter(|f| f.column == cname) {
                        for l in f.include.iter_mut().filter(|l| **l == a.level) {
                            *l = new.clone();
                        }
                        f.include.dedup();
                    }
                }
                // Data set labels of analyses contain level names ("A | B"): keep them in step.
                let old_level = a.level.clone();
                let relabel = |label: &mut String| {
                    let parts: Vec<String> = label.split(otto_core::COMBINE_SEP).map(|p| if p == old_level { new.clone() } else { p.to_string() }).collect();
                    *label = parts.join(otto_core::COMBINE_SEP);
                };
                for an in self.project.analyses.iter_mut().filter(|x| x.table_id == tid) {
                    match &mut an.kind {
                        AnalysisKind::OneWayAnova(x) => {
                            x.data_sets.iter_mut().for_each(relabel);
                            match &mut x.comparisons {
                                otto_core::analysis::Comparisons::VsControl { control } => relabel(control),
                                otto_core::analysis::Comparisons::Selected { pairs } => pairs.iter_mut().flatten().for_each(relabel),
                                _ => {}
                            }
                        }
                        other => {
                            if let Some(ds) = other.data_sets_mut() {
                                ds.iter_mut().for_each(relabel);
                            }
                        }
                    }
                    if let Some(b) = an.basis.as_mut().filter(|b| b.table_id == tid) {
                        b.rename_level(&cname, &a.level, &new);
                    }
                }
                Ok(Outcome::msg(format!("Renamed level \"{}\" to \"{new}\" in \"{cname}\"", a.level)))
            }
            TableRename(a) => {
                let ti = self.editable_table_idx(&a.table)?;
                let name = a.name.trim();
                if name.is_empty() {
                    return Err(EngineError::Invalid("name cannot be empty".into()));
                }
                self.project.tables[ti].name = name.to_string();
                Ok(Outcome::msg(format!("Renamed table to \"{name}\"")))
            }
            TableDuplicate(a) => {
                let ti = self.table_idx(&a.table)?;
                let mut t = self.project.tables[ti].clone();
                t.id = self.project.next_table_id();
                // A copy of a final graph's locked data is an ordinary, editable data table.
                t.locked_by = None;
                t.name = self.project.unique_table_name(&a.name.unwrap_or_else(|| format!("{} copy", t.name)));
                let msg = format!("Duplicated as {} \"{}\"", t.id, t.name);
                self.view = View::Table(t.id.clone());
                self.project.tables.insert(ti + 1, t);
                Ok(Outcome::msg(msg))
            }
            TableDelete(a) => {
                let ti = self.editable_table_idx(&a.table)?;
                let t = self.project.tables[ti].clone();
                let gone = self.table_deletion(&t);
                self.project.tables.remove(ti);
                self.project.graphs.retain(|g| !gone.quick.contains(&g.id) && !gone.finals.contains(&g.id));
                // The final graphs' locked copies, then every result whose table or graph is gone.
                self.project.tables.retain(|x| x.locked_by.as_ref().is_none_or(|g| !gone.finals.contains(g)));
                let (tables, graphs) = (&self.project.tables, &self.project.graphs);
                self.project.analyses.retain(|x| {
                    tables.iter().any(|t| t.id == x.table_id) && analysis_graph(x).is_none_or(|g| graphs.iter().any(|gr| gr.id == g))
                });
                self.fix_view();
                Ok(Outcome::msg(format!("Deleted table \"{}\"{}", t.name, gone.describe())))
            }

            GraphCreate(a) => self.graph_create(a),
            GraphSetKind(a) => {
                let gi = self.quick_graph_idx(&a.graph, "graph type")?;
                self.project.graphs[gi].kind = a.kind;
                let note = self.fit_mapping_to_kind(gi);
                let mut out = self.graph_outcome(gi, "Changed");
                if let Some(note) = note {
                    out.message = format!("{} ({note})", out.message);
                }
                Ok(out)
            }
            GraphSetMapping(a) => {
                let gi = self.quick_graph_idx(&a.graph, "data (table, X, Y, Group by)")?;
                let table_id = match key(&a.table) {
                    Some(k) => {
                        let ti = self.project.table_index(k).ok_or_else(|| not_found("table", k, self.tables_list()))?;
                        self.project.tables[ti].id.clone()
                    }
                    None => self.project.graphs[gi].table_id.clone(),
                };
                let g = &self.project.graphs[gi];
                let table = self
                    .project
                    .table_by_id(&table_id)
                    .ok_or_else(|| EngineError::NotFound(format!("table {table_id}")))?;
                if table.locked_by.is_some() && table_id != g.table_id {
                    return Err(EngineError::Invalid(format!("{table_id} is the locked data of a final graph; quick graphs use data tables")));
                }
                let x = a.x.clone().map(|v| Self::column_names(table, v)).transpose()?;
                let group = a.group.clone().map(|v| Self::column_names(table, v)).transpose()?;
                let y = match &a.y {
                    None => None,
                    Some(ys) => Some(ys.iter().map(|c| Self::column_name(table, c)).collect::<Result<Vec<_>, _>>()?),
                };
                let replicate_by = match a.replicate_by.as_deref().map(str::trim) {
                    None => None,
                    Some("") => Some(None),
                    Some(name) => Some(Some(Self::column_name(table, name)?)),
                };
                // Another table has other columns: a replicate column that is not among
                // them would only make the graph unplottable.
                let table_changed = table_id != self.project.graphs[gi].table_id;
                let keeps_replicate = self.project.graphs[gi].replicate_by.as_ref().is_some_and(|c| table.column(c).is_some());
                let g = &mut self.project.graphs[gi];
                g.table_id = table_id;
                if table_changed && !keeps_replicate {
                    g.replicate_by = None;
                }
                if let Some(c) = replicate_by {
                    g.replicate_by = c;
                }
                // A column is either X or Group: the side assigned now wins.
                if let Some(x) = x {
                    g.group.retain(|c| !x.contains(c));
                    g.x = x;
                }
                if let Some(y) = y {
                    g.y = y;
                }
                if let Some(group) = group {
                    g.x.retain(|c| !group.contains(c));
                    g.group = group;
                }
                Ok(self.graph_outcome(gi, "Mapped"))
            }
            GraphSetFilters(a) => {
                let gi = self.quick_graph_idx(&a.graph, "rows to include")?;
                let table = self
                    .project
                    .table_by_id(&self.project.graphs[gi].table_id)
                    .ok_or_else(|| EngineError::NotFound("graph table".into()))?;
                let mut filters = Vec::new();
                for mut f in a.filters {
                    f.column = Self::column_name(table, &f.column)?;
                    let col = table.require_column(&f.column)?;
                    if (f.min.is_some() || f.max.is_some()) && col.kind() != ColumnKind::Numeric {
                        return Err(EngineError::Invalid(format!(
                            "a range needs a numeric column, and \"{}\" is {}; use include to pick its values",
                            f.column,
                            col.kind().label().to_lowercase()
                        )));
                    }
                    if let (Some(lo), Some(hi)) = (f.min, f.max) {
                        if lo > hi {
                            return Err(EngineError::Invalid(format!("the range for \"{}\" is empty: min {lo} is above max {hi}", f.column)));
                        }
                    }
                    if !f.keeps_everything() {
                        filters.push(f);
                    }
                }
                self.project.graphs[gi].filters = filters;
                Ok(self.graph_outcome(gi, "Filtered"))
            }
            GraphSetFormat(a) => {
                self.graph_mut(&a.graph)?.format = a.format;
                let gi = self.graph_idx(&a.graph)?;
                Ok(self.graph_outcome(gi, "Formatted"))
            }
            GraphSetSummary(a) => {
                let gi = self.quick_graph_idx(&a.graph, "bars, symbols and error bars")?;
                let g = &mut self.project.graphs[gi];
                if let Some(c) = a.center {
                    g.summary.center = c;
                }
                if let Some(e) = a.error {
                    g.summary.error = e;
                }
                if let Some(r) = a.replicates {
                    g.summary.replicates = r;
                }
                if let Some(c) = &a.replicate_by {
                    let column = match c.trim() {
                        "" => None,
                        name => {
                            let t = self.project.table_by_id(&self.project.graphs[gi].table_id).ok_or_else(|| EngineError::NotFound("graph table".into()))?;
                            Some(Self::column_name(t, name)?)
                        }
                    };
                    self.project.graphs[gi].replicate_by = column;
                }
                if let Some(v) = a.whiskers {
                    let gi = self.quick_graph_idx(&a.graph, "whiskers")?;
                    self.project.graphs[gi].whiskers = v;
                }
                let gi = self.graph_idx(&a.graph)?;
                Ok(self.graph_outcome(gi, "Updated"))
            }
            GraphSetSymbols(a) => {
                // Symbols are appearance: allowed on final graphs too.
                let gi = self.graph_idx(&a.graph)?;
                let g = &self.project.graphs[gi];
                // Column, box and violin graphs hand their symbols out per bar; the rest
                // per data set.
                let n = self
                    .project
                    .table_by_id(&g.table_id)
                    .and_then(|t| otto_plot::distill::distill(g, t).ok())
                    .map_or(1, |d| if d.kind.is_column_like() { d.x_levels.len() } else { d.sets.len() });
                let g = &mut self.project.graphs[gi];
                g.symbols = if a.vary {
                    (0..n.max(1)).map(|i| otto_core::SYMBOL_CYCLE[i % otto_core::SYMBOL_CYCLE.len()]).collect()
                } else {
                    a.symbols.clone()
                };
                Ok(self.graph_outcome(gi, "Updated"))
            }
            GraphSetVolcano(a) => {
                let gi = self.quick_graph_idx(&a.graph, "volcano cutoffs")?;
                if self.project.graphs[gi].kind != GraphKind::Volcano {
                    return Err(EngineError::Invalid(format!("\"{}\" is not a volcano plot", self.project.graphs[gi].name)));
                }
                let label = match a.label.as_deref().map(str::trim) {
                    None => None,
                    Some("") => Some(None),
                    Some(name) => {
                        let t = self
                            .project
                            .table_by_id(&self.project.graphs[gi].table_id)
                            .ok_or_else(|| EngineError::NotFound("graph table".into()))?;
                        Some(Some(Self::column_name(t, name)?))
                    }
                };
                let v = &mut self.project.graphs[gi].volcano;
                if let Some(x) = a.y_is_log {
                    v.y_is_log = x;
                }
                if let Some(x) = a.fc_cutoff {
                    v.fc_cutoff = x.abs();
                }
                if let Some(x) = a.p_cutoff {
                    if !(0.0..1.0).contains(&x) {
                        return Err(EngineError::Invalid("the P cutoff must be between 0 and 1".into()));
                    }
                    v.p_cutoff = x;
                }
                if let Some(x) = label {
                    v.label = x;
                }
                if let Some(x) = a.label_top {
                    v.label_top = x.min(200);
                }
                Ok(self.graph_outcome(gi, "Updated"))
            }
            GraphSetHeatmap(a) => {
                // Colors are appearance: allowed on final graphs too.
                let gi = self.graph_idx(&a.graph)?;
                let g = &mut self.project.graphs[gi];
                if g.kind != GraphKind::Heatmap {
                    return Err(EngineError::Invalid(format!("\"{}\" is not a heatmap", g.name)));
                }
                if let Some(v) = a.scale {
                    g.heatmap.scale = v;
                }
                if let Some(v) = a.normalize {
                    g.heatmap.normalize = v;
                }
                if let Some(v) = a.show_values {
                    g.heatmap.show_values = v;
                }
                if let Some(v) = a.decimals {
                    if v > 10 {
                        return Err(EngineError::Invalid("at most 10 decimal places".into()));
                    }
                    g.heatmap.decimals = Some(v);
                }
                Ok(self.graph_outcome(gi, "Updated"))
            }
            GraphSetAxis(a) => {
                // Axis titles are labels and may change on final graphs too; ranges and the
                // scale decide what the statistics were set up on, so they may not.
                let gi = if a.min.is_some() || a.max.is_some() || a.auto_range || a.scale.is_some() {
                    self.quick_graph_idx(&a.graph, "axis ranges and scales")?
                } else {
                    self.graph_idx(&a.graph)?
                };
                let g = &mut self.project.graphs[gi];
                let axis = match a.axis {
                    Axis::X => &mut g.x_axis,
                    Axis::Y => &mut g.y_axis,
                };
                if let Some(t) = a.title {
                    axis.title = (!t.trim().is_empty()).then(|| t.trim().to_string());
                }
                if a.auto_range {
                    axis.min = None;
                    axis.max = None;
                }
                if a.min.is_some() {
                    axis.min = a.min;
                }
                if a.max.is_some() {
                    axis.max = a.max;
                }
                if let Some(v) = a.scale {
                    axis.scale = v;
                }
                if let Some(n) = a.numbers {
                    axis.numbers = n;
                }
                if axis.scale.is_log() && axis.min.is_some_and(|v| v <= 0.0) {
                    return Err(EngineError::Invalid("a logarithmic axis cannot start at zero or below".into()));
                }
                if let (Some(lo), Some(hi)) = (axis.min, axis.max) {
                    if lo >= hi {
                        return Err(EngineError::Invalid(format!("axis minimum ({lo}) must be below the maximum ({hi})")));
                    }
                }
                let gi = self.graph_idx(&a.graph)?;
                Ok(self.graph_outcome(gi, "Updated"))
            }
            GraphSetPalette(a) => {
                let g = self.graph_mut(&a.graph)?;
                g.palette = a.palette;
                if let Some(h) = a.highlight {
                    g.highlight = h;
                }
                let gi = self.graph_idx(&a.graph)?;
                Ok(self.graph_outcome(gi, "Recolored"))
            }
            GraphSetFocus(a) => {
                let gi = self.graph_idx(&a.graph)?;
                // Only data sets the graph actually has can be in focus, so a typo is caught
                // here rather than quietly colouring nothing.
                let known: Vec<String> = self.plot_data(&self.project.graphs[gi].id.clone())?.color_groups().to_vec();
                let focus: Vec<String> = a.focus.iter().map(|f| f.trim().to_string()).filter(|f| !f.is_empty()).collect();
                if let Some(bad) = focus.iter().find(|f| !known.contains(f)) {
                    return Err(EngineError::Invalid(format!("\"{bad}\" is not a data set of this graph; it has {}", known.join(", "))));
                }
                self.project.graphs[gi].focus = focus;
                Ok(self.graph_outcome(gi, "Recolored"))
            }
            GraphSetLegend(a) => {
                self.graph_mut(&a.graph)?.show_legend = a.show;
                let gi = self.graph_idx(&a.graph)?;
                Ok(self.graph_outcome(gi, "Updated"))
            }
            GraphRename(a) => {
                let name = a.name.trim().to_string();
                if name.is_empty() {
                    return Err(EngineError::Invalid("name cannot be empty".into()));
                }
                self.graph_mut(&a.graph)?.name = name.clone();
                Ok(Outcome::msg(format!("Renamed graph to \"{name}\"")))
            }
            GraphDuplicate(a) => {
                let gi = self.graph_idx(&a.graph)?;
                let mut g = self.project.graphs[gi].clone();
                g.id = self.project.next_graph_id();
                g.name = self.project.unique_graph_name(&a.name.unwrap_or_else(|| format!("{} copy", g.name)));
                let mut copied = 0;
                if g.is_final() {
                    // A final graph owns its locked data: the copy gets its own (identical) copy,
                    // and its statistics come along so the copy is the whole figure.
                    let (old_graph, old_table) = (self.project.graphs[gi].id.clone(), g.table_id.clone());
                    let mut data = self
                        .project
                        .table_by_id(&g.table_id)
                        .cloned()
                        .ok_or_else(|| EngineError::NotFound(format!("locked data of \"{}\"", g.name)))?;
                    data.id = self.project.next_table_id();
                    data.locked_by = Some(g.id.clone());
                    // The name says which graph the copy belongs to, so it follows the copy.
                    if let Some(l) = &g.locked {
                        data.name = format!("{} (locked for {})", l.from_table, g.id);
                    }
                    g.table_id = data.id.clone();
                    self.project.tables.push(data);
                    let stats: Vec<AnalysisSpec> =
                        self.project.analyses.iter().filter(|a| analysis_graph(a) == Some(old_graph.as_str())).cloned().collect();
                    for mut a in stats {
                        a.id = self.project.next_analysis_id();
                        a.name = self.project.unique_analysis_name(&a.name);
                        a.table_id = g.table_id.clone();
                        if let Some(gid) = a.kind.graph_id_mut() {
                            *gid = g.id.clone();
                        }
                        if let Some(b) = a.basis.as_mut().filter(|b| b.table_id == old_table) {
                            b.table_id = g.table_id.clone();
                        }
                        self.project.analyses.push(a);
                        copied += 1;
                    }
                }
                self.view = View::Graph(g.id.clone());
                self.project.graphs.insert(gi + 1, g);
                let mut out = self.graph_outcome(gi + 1, "Duplicated as");
                if copied > 0 {
                    out.message.push_str(&format!(" with {copied} statistics result(s)"));
                }
                Ok(out)
            }
            GraphFinalize(a) => {
                let gi = self.graph_idx(&a.graph)?;
                let fi = self.finalize(gi, a.name)?;
                let id = self.project.graphs[fi].id.clone();
                self.view = View::Graph(id);
                let mut out = self.graph_outcome(fi, "Finalized as");
                out.message.push_str(". Its data are a locked copy: later edits to the data table don't change it");
                Ok(out)
            }
            GraphUpdateFromQuick(a) => self.refresh_final(a),
            GraphDelete(a) => {
                let gi = self.graph_idx(&a.graph)?;
                let g = self.project.graphs.remove(gi);
                if self.view == View::Graph(g.id.clone()) {
                    self.view = View::Table(g.table_id.clone());
                }
                // Analyses of this graph's bars go with it, and so does a final graph's locked data.
                self.project.tables.retain(|t| t.locked_by.as_deref() != Some(g.id.as_str()));
                let before = self.project.analyses.len();
                self.project.analyses.retain(|an| analysis_graph(an) != Some(g.id.as_str()));
                let removed = before - self.project.analyses.len();
                self.fix_view();
                let extra = if removed > 0 { format!(" and {removed} analysis(es) of it") } else { String::new() };
                Ok(Outcome::msg(format!("Deleted graph \"{}\"{extra}", g.name)))
            }
            GraphRenderPreview(a) => {
                let gi = self.graph_idx(&a.graph)?;
                let r = self.render_graph(&self.project.graphs[gi].id.clone())?;
                let png = r.png(a.dpi.unwrap_or(300.0).clamp(50.0, 1200.0))?;
                let mut out = Outcome::msg(self.graph_status(gi));
                out.image_png = Some(png);
                if a.include_svg {
                    out.text = Some(r.svg());
                }
                Ok(out)
            }
            GraphGetPlotData(a) => {
                let gi = self.graph_idx(&a.graph)?;
                let id = self.project.graphs[gi].id.clone();
                let data = self.plot_data(&id)?;
                let t = otto_plot::data_export::table(&data, a.layout);
                let csv = t.to_csv();
                let label = a.layout.label();
                if t.rows.len() <= INLINE_ROWS {
                    return Ok(Outcome::msg(format!("{} rows ({label})", t.rows.len())).with_text(csv));
                }
                // The long layout is one row per plotted point, so a big graph's table is far
                // more than a tool result should carry. Past the cap it goes to a file and only
                // the head comes back, as the R script export does.
                let p = std::env::temp_dir().join(format!("otto-{}-plot-data.csv", id.to_lowercase()));
                std::fs::write(&p, &csv).map_err(|e| io_err(&p, e))?;
                let head: String = csv.lines().take(INLINE_ROWS + 1).collect::<Vec<_>>().join("\n");
                let rest = t.rows.len() - INLINE_ROWS;
                Ok(Outcome::msg(format!(
                    "{} rows ({label}): the first {INLINE_ROWS} are below, all of them are in {}. graph_export_data writes them where you want them.",
                    t.rows.len(),
                    p.display()
                ))
                .with_text(format!("{head}\n# … {rest} more rows in {}", p.display())))
            }
            GraphExportFile(a) => {
                let gi = self.graph_idx(&a.graph)?;
                let r = self.render_graph(&self.project.graphs[gi].id.clone())?;
                let mut path = absolute(&a.path);
                let format = match a.format {
                    Some(f) => f,
                    None => path
                        .extension()
                        .and_then(|e| e.to_str())
                        .and_then(ExportFormat::from_extension)
                        .unwrap_or(ExportFormat::Svg),
                };
                if path.extension().and_then(|e| e.to_str()).and_then(ExportFormat::from_extension) != Some(format) {
                    path.set_extension(format.extension());
                }
                let bytes = match format {
                    ExportFormat::Svg if a.outline_text => r.svg_outlined()?.into_bytes(),
                    ExportFormat::Svg if a.background => r.svg_with_background().into_bytes(),
                    ExportFormat::Svg => r.svg().into_bytes(),
                    ExportFormat::Pdf => r.pdf()?,
                    ExportFormat::Emf => r.emf()?,
                    ExportFormat::Png => r.png(600.0)?,
                };
                std::fs::write(&path, bytes).map_err(|e| io_err(&path, e))?;
                let (w, h) = r.size_mm();
                Ok(Outcome::msg(format!("Exported {} ({w:.1} × {h:.1} mm)", path.display()))
                    .with_data(json!({ "path": path.display().to_string() })))
            }
            GraphCopyVector(a) => {
                let gi = self.graph_idx(&a.graph)?;
                let r = self.render_graph(&self.project.graphs[gi].id.clone())?;
                clipboard::set_graph(&r)?;
                Ok(Outcome::msg(format!("Copied \"{}\" to the clipboard as vector graphics", self.project.graphs[gi].name)))
            }
            GraphExportData(a) => {
                let gi = self.graph_idx(&a.graph)?;
                let r = self.render_graph(&self.project.graphs[gi].id.clone())?;
                let mut path = absolute(&a.path);
                if path.extension().is_none() {
                    path.set_extension("csv");
                }
                let t = r.data_table(a.layout);
                std::fs::write(&path, t.to_csv()).map_err(|e| io_err(&path, e))?;
                Ok(Outcome::msg(format!("Saved {} rows to {}", t.rows.len(), path.display()))
                    .with_data(json!({ "path": path.display().to_string() })))
            }
            GraphCopyData(a) => {
                let gi = self.graph_idx(&a.graph)?;
                let r = self.render_graph(&self.project.graphs[gi].id.clone())?;
                let t = r.data_table(a.layout);
                clipboard::set_text(&t.to_tsv())?;
                Ok(Outcome::msg(format!("Copied {} rows of plotted data ({})", t.rows.len(), a.layout.label())))
            }
            GraphExportRCode(a) => {
                let gi = self.graph_idx(&a.graph)?;
                let spec = self.project.graphs[gi].clone();
                let code = self.render_graph(&spec.id)?.r_code(&spec);
                match a.path {
                    Some(p) => {
                        let mut path = absolute(&p);
                        if path.extension().is_none() {
                            path.set_extension("txt");
                        }
                        std::fs::write(&path, &code).map_err(|e| io_err(&path, e))?;
                        Ok(Outcome::msg(format!("Saved R code to {}", path.display())).with_text(code))
                    }
                    None => Ok(Outcome::msg("R code for the graph").with_text(code)),
                }
            }
            GraphCopyRCode(a) => {
                let gi = self.graph_idx(&a.graph)?;
                let spec = self.project.graphs[gi].clone();
                let code = self.render_graph(&spec.id)?.r_code(&spec);
                clipboard::set_text(&code)?;
                Ok(Outcome::msg("Copied R code to the clipboard"))
            }

            AnalysisCreateDescriptive(a) => {
                let ti = self.table_idx(&a.table)?;
                let t = &self.project.tables[ti];
                let y = if a.y.is_empty() {
                    t.columns.iter().filter(|c| c.kind() == ColumnKind::Numeric).map(|c| c.name.clone()).collect()
                } else {
                    a.y.iter().map(|c| Self::column_name(t, c)).collect::<Result<Vec<_>, _>>()?
                };
                if y.is_empty() {
                    return Err(EngineError::Invalid(format!("table \"{}\" has no numeric columns", t.name)));
                }
                let group = match key(&a.group) {
                    Some(g) => Some(Self::column_name(t, g)?),
                    None => None,
                };
                let table_id = t.id.clone();
                let id = self.project.next_analysis_id();
                let name = a.name.unwrap_or_else(|| self.project.unique_analysis_name("Descriptive statistics"));
                let spec = AnalysisSpec { id: id.clone(), name: name.clone(), table_id, kind: AnalysisKind::Descriptive { y, group }, basis: None };
                let res = results::compute(&self.project, &spec)?;
                self.project.analyses.push(spec);
                self.view = View::Result(id.clone());
                Ok(Outcome::msg(format!("Created {id} \"{name}\" ({} rows)", res.row_count())).with_text(res.to_text()))
            }
            AnalysisGetResults(a) => {
                let ai = self.analysis_idx(&a.analysis)?;
                let spec = &self.project.analyses[ai];
                if let Some(reasons) = crate::column_tests::out_of_date(&self.project, spec) {
                    return Err(EngineError::Invalid(format!(
                        "{} \"{}\" is out of date because its graph changed ({}); its results and brackets are hidden. \
                         Call analysis_recalculate to rerun it on the bars shown now, or analysis_edit_* with data_sets to set it up again",
                        spec.id,
                        spec.name,
                        reasons.join("; ")
                    )));
                }
                let res = otto_r::blocking(|| results::compute(&self.project, spec))?;
                Ok(Outcome::msg(format!("{} \"{}\": {}", spec.id, spec.name, res.title)).with_text(res.to_text()))
            }
            AnalysisCreateTTest(a) => {
                let mut spec = a.spec;
                spec.graph_id = self.resolve_graph_id(&spec.graph_id, None)?;
                let source = self.source_graph_name(&spec.graph_id);
                let name = a.name.unwrap_or_else(|| self.project.unique_analysis_name(&format!("t test of {source}")));
                let finalized = self.final_graph_for_statistics(&mut spec.graph_id, DataShape::Bars)?;
                spec.match_by = self.resolve_match_by(&spec.graph_id, &spec.match_by)?;
                self.add_column_analysis(name, AnalysisKind::TTest(spec), finalized)
            }
            AnalysisCreateOneWayAnova(a) => {
                let mut spec = a.spec;
                spec.graph_id = self.resolve_graph_id(&spec.graph_id, None)?;
                let source = self.source_graph_name(&spec.graph_id);
                let name = a.name.unwrap_or_else(|| self.project.unique_analysis_name(&format!("One-way ANOVA of {source}")));
                let finalized = self.final_graph_for_statistics(&mut spec.graph_id, DataShape::Bars)?;
                spec.match_by = self.resolve_match_by(&spec.graph_id, &spec.match_by)?;
                self.add_column_analysis(name, AnalysisKind::OneWayAnova(spec), finalized)
            }
            AnalysisEditTTest(a) => {
                let ai = self.analysis_idx(&a.analysis)?;
                let AnalysisKind::TTest(old) = &self.project.analyses[ai].kind else {
                    return Err(EngineError::Invalid(format!("\"{}\" is not a t test", self.project.analyses[ai].name)));
                };
                let mut spec = old.clone();
                a.apply(&mut spec);
                spec.graph_id = self.resolve_graph_id(&spec.graph_id, Some(&old.graph_id))?;
                spec.match_by = self.resolve_match_by(&spec.graph_id, &spec.match_by)?;
                let rebase = a.data_sets.is_some() || a.graph_id.is_some();
                self.replace_column_analysis(ai, AnalysisKind::TTest(spec), rebase)
            }
            AnalysisEditOneWayAnova(a) => {
                let ai = self.analysis_idx(&a.analysis)?;
                let AnalysisKind::OneWayAnova(old) = &self.project.analyses[ai].kind else {
                    return Err(EngineError::Invalid(format!("\"{}\" is not a one-way ANOVA", self.project.analyses[ai].name)));
                };
                let mut spec = old.clone();
                a.apply(&mut spec);
                spec.graph_id = self.resolve_graph_id(&spec.graph_id, Some(&old.graph_id))?;
                spec.match_by = self.resolve_match_by(&spec.graph_id, &spec.match_by)?;
                let rebase = a.data_sets.is_some() || a.graph_id.is_some();
                self.replace_column_analysis(ai, AnalysisKind::OneWayAnova(spec), rebase)
            }
            AnalysisCreateCurveFit(a) => {
                let mut spec = a.spec;
                spec.graph_id = self.resolve_graph_id(&spec.graph_id, None)?;
                self.check_fixed(&spec)?;
                let source = self.source_graph_name(&spec.graph_id);
                let name = a.name.unwrap_or_else(|| self.project.unique_analysis_name(&format!("{} fit of {source}", spec.model.label().split(" – ").next().unwrap_or("Curve"))));
                let finalized = self.final_graph_for_statistics(&mut spec.graph_id, DataShape::Xy)?;
                self.add_column_analysis(name, AnalysisKind::CurveFit(spec), finalized)
            }
            AnalysisEditCurveFit(a) => {
                let ai = self.analysis_idx(&a.analysis)?;
                let AnalysisKind::CurveFit(old) = &self.project.analyses[ai].kind else {
                    return Err(EngineError::Invalid(format!("\"{}\" is not a curve fit", self.project.analyses[ai].name)));
                };
                let mut spec = old.clone();
                a.apply(&mut spec);
                spec.graph_id = self.resolve_graph_id(&spec.graph_id, Some(&old.graph_id))?;
                self.check_fixed(&spec)?;
                let rebase = a.data_sets.is_some() || a.graph_id.is_some();
                self.replace_column_analysis(ai, AnalysisKind::CurveFit(spec), rebase)
            }
            AnalysisCreateLinearRegression(a) => {
                let mut spec = a.spec;
                spec.graph_id = self.resolve_graph_id(&spec.graph_id, None)?;
                let source = self.source_graph_name(&spec.graph_id);
                let name = a.name.unwrap_or_else(|| self.project.unique_analysis_name(&format!("Linear regression of {source}")));
                let finalized = self.final_graph_for_statistics(&mut spec.graph_id, DataShape::Xy)?;
                self.add_column_analysis(name, AnalysisKind::LinearRegression(spec), finalized)
            }
            AnalysisEditLinearRegression(a) => {
                let ai = self.analysis_idx(&a.analysis)?;
                let AnalysisKind::LinearRegression(old) = &self.project.analyses[ai].kind else {
                    return Err(EngineError::Invalid(format!("\"{}\" is not a linear regression", self.project.analyses[ai].name)));
                };
                let mut spec = old.clone();
                a.apply(&mut spec);
                let rebase = a.data_sets.is_some();
                self.replace_column_analysis(ai, AnalysisKind::LinearRegression(spec), rebase)
            }
            AnalysisCreateCorrelation(a) => {
                let mut spec = a.spec;
                spec.graph_id = self.resolve_graph_id(&spec.graph_id, None)?;
                let source = self.source_graph_name(&spec.graph_id);
                let name = a.name.unwrap_or_else(|| self.project.unique_analysis_name(&format!("Correlation of {source}")));
                let finalized = self.final_graph_for_statistics(&mut spec.graph_id, DataShape::Xy)?;
                self.add_column_analysis(name, AnalysisKind::Correlation(spec), finalized)
            }
            AnalysisEditCorrelation(a) => {
                let ai = self.analysis_idx(&a.analysis)?;
                let AnalysisKind::Correlation(old) = &self.project.analyses[ai].kind else {
                    return Err(EngineError::Invalid(format!("\"{}\" is not a correlation", self.project.analyses[ai].name)));
                };
                let mut spec = old.clone();
                a.apply(&mut spec);
                let rebase = a.data_sets.is_some();
                self.replace_column_analysis(ai, AnalysisKind::Correlation(spec), rebase)
            }
            AnalysisCreateTwoWayAnova(a) => {
                let mut spec = a.spec;
                spec.graph_id = self.resolve_graph_id(&spec.graph_id, None)?;
                let source = self.source_graph_name(&spec.graph_id);
                let name = a.name.unwrap_or_else(|| self.project.unique_analysis_name(&format!("Two-way ANOVA of {source}")));
                let finalized = self.final_graph_for_statistics(&mut spec.graph_id, DataShape::Bars)?;
                self.add_column_analysis(name, AnalysisKind::TwoWayAnova(spec), finalized)
            }
            AnalysisEditTwoWayAnova(a) => {
                let ai = self.analysis_idx(&a.analysis)?;
                let AnalysisKind::TwoWayAnova(old) = &self.project.analyses[ai].kind else {
                    return Err(EngineError::Invalid(format!("\"{}\" is not a two-way ANOVA", self.project.analyses[ai].name)));
                };
                let mut spec = old.clone();
                a.apply(&mut spec);
                let rebase = a.data_sets.is_some();
                self.replace_column_analysis(ai, AnalysisKind::TwoWayAnova(spec), rebase)
            }
            AnalysisCreateAuc(a) => {
                let mut spec = a.spec;
                spec.graph_id = self.resolve_graph_id(&spec.graph_id, None)?;
                let source = self.source_graph_name(&spec.graph_id);
                let name = a.name.unwrap_or_else(|| self.project.unique_analysis_name(&format!("Area under curve of {source}")));
                let finalized = self.final_graph_for_statistics(&mut spec.graph_id, DataShape::Xy)?;
                self.add_column_analysis(name, AnalysisKind::Auc(spec), finalized)
            }
            AnalysisEditAuc(a) => {
                let ai = self.analysis_idx(&a.analysis)?;
                let AnalysisKind::Auc(old) = &self.project.analyses[ai].kind else {
                    return Err(EngineError::Invalid(format!("\"{}\" is not an area under the curve", self.project.analyses[ai].name)));
                };
                let mut spec = old.clone();
                a.apply(&mut spec);
                let rebase = a.data_sets.is_some();
                self.replace_column_analysis(ai, AnalysisKind::Auc(spec), rebase)
            }
            AnalysisCreateSurvival(a) => {
                let mut spec = a.spec;
                spec.graph_id = self.resolve_graph_id(&spec.graph_id, None)?;
                let source = self.source_graph_name(&spec.graph_id);
                let name = a.name.unwrap_or_else(|| self.project.unique_analysis_name(&format!("Survival analysis of {source}")));
                let finalized = self.final_graph_for_statistics(&mut spec.graph_id, DataShape::Survival)?;
                self.add_column_analysis(name, AnalysisKind::Survival(spec), finalized)
            }
            AnalysisEditSurvival(a) => {
                let ai = self.analysis_idx(&a.analysis)?;
                let AnalysisKind::Survival(old) = &self.project.analyses[ai].kind else {
                    return Err(EngineError::Invalid(format!("\"{}\" is not a survival analysis", self.project.analyses[ai].name)));
                };
                let mut spec = old.clone();
                a.apply(&mut spec);
                let rebase = a.data_sets.is_some();
                self.replace_column_analysis(ai, AnalysisKind::Survival(spec), rebase)
            }
            AnalysisCreateContingency(a) => {
                let mut spec = a.spec;
                spec.graph_id = self.resolve_graph_id(&spec.graph_id, None)?;
                let source = self.source_graph_name(&spec.graph_id);
                let name = a.name.unwrap_or_else(|| self.project.unique_analysis_name(&format!("Contingency analysis of {source}")));
                let finalized = self.final_graph_for_statistics(&mut spec.graph_id, DataShape::Counts)?;
                self.add_column_analysis(name, AnalysisKind::Contingency(spec), finalized)
            }
            AnalysisEditContingency(a) => {
                let ai = self.analysis_idx(&a.analysis)?;
                let AnalysisKind::Contingency(old) = &self.project.analyses[ai].kind else {
                    return Err(EngineError::Invalid(format!("\"{}\" is not a contingency analysis", self.project.analyses[ai].name)));
                };
                let mut spec = old.clone();
                a.apply(&mut spec);
                let rebase = a.data_sets.is_some();
                self.replace_column_analysis(ai, AnalysisKind::Contingency(spec), rebase)
            }
            AnalysisCreateNormality(a) => {
                let mut spec = a.spec;
                spec.graph_id = self.resolve_graph_id(&spec.graph_id, None)?;
                let source = self.source_graph_name(&spec.graph_id);
                let name = a.name.unwrap_or_else(|| self.project.unique_analysis_name(&format!("Normality of {source}")));
                let finalized = self.final_graph_for_statistics(&mut spec.graph_id, DataShape::Bars)?;
                self.add_column_analysis(name, AnalysisKind::Normality(spec), finalized)
            }
            AnalysisEditNormality(a) => {
                let ai = self.analysis_idx(&a.analysis)?;
                let AnalysisKind::Normality(old) = &self.project.analyses[ai].kind else {
                    return Err(EngineError::Invalid(format!("\"{}\" is not a normality analysis", self.project.analyses[ai].name)));
                };
                let mut spec = old.clone();
                a.apply(&mut spec);
                let rebase = a.data_sets.is_some();
                self.replace_column_analysis(ai, AnalysisKind::Normality(spec), rebase)
            }
            AnalysisCreateOutliers(a) => {
                let mut spec = a.spec;
                spec.graph_id = self.resolve_graph_id(&spec.graph_id, None)?;
                let source = self.source_graph_name(&spec.graph_id);
                let name = a.name.unwrap_or_else(|| self.project.unique_analysis_name(&format!("Outliers of {source}")));
                let finalized = self.final_graph_for_statistics(&mut spec.graph_id, DataShape::Bars)?;
                self.add_column_analysis(name, AnalysisKind::Outliers(spec), finalized)
            }
            AnalysisEditOutliers(a) => {
                let ai = self.analysis_idx(&a.analysis)?;
                let AnalysisKind::Outliers(old) = &self.project.analyses[ai].kind else {
                    return Err(EngineError::Invalid(format!("\"{}\" is not an outlier analysis", self.project.analyses[ai].name)));
                };
                let mut spec = old.clone();
                a.apply(&mut spec);
                let rebase = a.data_sets.is_some();
                self.replace_column_analysis(ai, AnalysisKind::Outliers(spec), rebase)
            }
            AnalysisExportRScript(a) => {
                let ai = self.analysis_idx(&a.analysis)?;
                let spec = &self.project.analyses[ai];
                if spec.kind.graph_id().is_none() {
                    return Err(EngineError::Invalid("descriptive statistics are computed by Otto itself; there is no R script for them".into()));
                }
                let (report, requests) = otto_r::blocking(|| otto_r::recording(|| results::compute(&self.project, spec)));
                let report = report?;
                let script = otto_r::script(&format!("{} \"{}\": {} ({})", spec.id, spec.name, report.title, report.engine), &requests);
                // The script is thousands of lines (Otto's whole R source plus the data), so it is
                // always written to a file: returning it inline overruns the caller's limits.
                let p = match key(&a.path) {
                    Some(path) => absolute(path),
                    None => std::env::temp_dir().join(format!("otto-{}-r-script.R", spec.id.to_lowercase())),
                };
                std::fs::write(&p, &script).map_err(|e| io_err(&p, e))?;
                let lines = script.lines().count();
                let head: String = script.lines().take(25).collect::<Vec<_>>().join("
");
                let out = Outcome::msg(format!(
                    "Saved the R script for {} \"{}\" ({} R calls, {lines} lines) to {}. Run it with Rscript; it prints the same numbers Otto shows.",
                    spec.id,
                    spec.name,
                    requests.len(),
                    p.display()
                ));
                Ok(out.with_text(format!("{head}
# … {} more lines in {}", lines.saturating_sub(25), p.display())))
            }
            EngineStatus(_) => {
                let s = otto_r::status();
                let setup = otto_r::setup_progress();
                let mut msg = format!("Statistics engine: {}", s.summary());
                if setup.running {
                    msg.push_str(" (setup running)");
                }
                Ok(Outcome::msg(msg).with_data(json!({
                    "ready": s.ready(),
                    "rscript": s.rscript,
                    "r_version": s.r_version,
                    "library": s.library,
                    "packages": s.packages.iter().map(|(p, v)| json!({ "name": p, "version": v })).collect::<Vec<_>>(),
                    "error": s.error,
                    "setup": { "running": setup.running, "log_tail": setup.log.iter().rev().take(15).rev().collect::<Vec<_>>(),
                               "finished": setup.finished.as_ref().map(|r| match r { Ok(()) => "ok".to_string(), Err(e) => e.clone() }) },
                })))
            }
            EngineSetup(_) => {
                otto_r::start_setup()?;
                Ok(Outcome::msg(format!(
                    "Installing the statistics engine's R packages into {} (a few minutes); engine_status shows the progress",
                    otto_r::library_dir().display()
                )))
            }
            AnalysisListDataSets(a) => {
                let gi = self.graph_idx(&a.graph)?;
                let g = &self.project.graphs[gi];
                let sets = crate::column_tests::list_data_sets(&self.project, &g.id)?;
                let what = if g.kind.has_bars() { "data sets (bars)" } else { "data sets" };
                let msg = format!("{} {what} in {} \"{}\"", sets.len(), g.id, g.name);
                Ok(Outcome::msg(msg).with_data(json!(sets.iter().map(|(l, n)| json!({ "label": l, "n": n })).collect::<Vec<_>>())))
            }
            AnalysisRecalculate(a) => self.recalculate(a),
            AnalysisRename(a) => {
                let ai = self.analysis_idx(&a.analysis)?;
                let name = a.name.trim().to_string();
                if name.is_empty() {
                    return Err(EngineError::Invalid("name cannot be empty".into()));
                }
                self.project.analyses[ai].name = name.clone();
                Ok(Outcome::msg(format!("Renamed analysis to \"{name}\"")))
            }
            AnalysisDelete(a) => {
                let ai = self.analysis_idx(&a.analysis)?;
                let an = self.project.analyses.remove(ai);
                self.fix_view();
                Ok(Outcome::msg(format!("Deleted \"{}\"", an.name)))
            }
        }
    }

    // -- helpers ------------------------------------------------------------

    fn save_to(&mut self, path: &Path) -> Result<Outcome, EngineError> {
        if let Some(stem) = path.file_stem() {
            self.project.name = stem.to_string_lossy().to_string();
        }
        self.project.save(path)?;
        self.path = Some(path.to_path_buf());
        self.dirty = false;
        Ok(Outcome::msg(format!("Saved {}", path.display())).with_data(json!({ "path": path.display().to_string() })))
    }

    fn add_imported(&mut self, t: DataTable) -> Result<Outcome, EngineError> {
        let cols: Vec<String> = t.columns.iter().map(|c| format!("{} ({})", c.name, c.kind().label().to_lowercase())).collect();
        let msg = format!("Imported {} \"{}\": {} rows × {} columns: {}", t.id, t.name, t.nrows, t.ncols(), cols.join(", "));
        let data = json!({ "table": t.id, "describe": describe(&t) });
        self.view = View::Table(t.id.clone());
        self.project.tables.push(t);
        Ok(Outcome::msg(msg).with_data(data))
    }

    fn navigate(&mut self, a: NavigateArgs) -> Result<Outcome, EngineError> {
        // A graph by id/name, or the first one matching `pick`.
        let graph = |pick: fn(&GraphSpec) -> bool, what: &str| -> Result<String, EngineError> {
            match key(&a.item) {
                Some(k) => {
                    let gi = self.project.graph_index(k).ok_or_else(|| not_found("graph", k, self.graphs_list()))?;
                    Ok(self.project.graphs[gi].id.clone())
                }
                None => self.project.graphs.iter().find(|g| pick(g)).map(|g| g.id.clone()).ok_or_else(|| EngineError::Invalid(format!("no {what} yet"))),
            }
        };
        let view = match a.section {
            Section::Tables => {
                let ti = match key(&a.item) {
                    Some(k) => self.project.table_index(k).ok_or_else(|| not_found("table", k, self.tables_list()))?,
                    None => self.project.tables.iter().position(|t| t.locked_by.is_none()).unwrap_or(0),
                };
                View::Table(self.project.tables.get(ti).ok_or_else(|| EngineError::Invalid("no tables".into()))?.id.clone())
            }
            Section::PlottedTables => View::Plotted(graph(|_| true, "graphs")?),
            Section::Graphs => View::Graph(graph(|g| !g.is_final(), "quick graphs")?),
            Section::FinalGraphs => View::Graph(graph(|g| g.is_final(), "final graphs")?),
            Section::Results => {
                let ai = match key(&a.item) {
                    Some(k) => self.project.analysis_index(k).ok_or_else(|| not_found("analysis", k, self.analyses_list()))?,
                    None => 0,
                };
                View::Result(self.project.analyses.get(ai).ok_or_else(|| EngineError::Invalid("no results".into()))?.id.clone())
            }
        };
        self.view = view;
        let p = &self.project;
        let graph_name = |id: &str| p.graph(id).map(|g| format!("\"{}\"", g.name)).unwrap_or_else(|| id.to_string());
        let what = match &self.view {
            View::Welcome => "the start page".to_string(),
            View::Table(id) => format!("data table {}", p.table_by_id(id).map(|t| format!("\"{}\"", t.name)).unwrap_or_else(|| id.clone())),
            View::Plotted(id) => format!("the plotted table of {}", graph_name(id)),
            View::Graph(id) if p.graph(id).is_some_and(|g| g.is_final()) => format!("final graph {}", graph_name(id)),
            View::Graph(id) => format!("graph {}", graph_name(id)),
            View::Result(id) => format!("results {}", p.analyses.iter().find(|a| &a.id == id).map(|a| format!("\"{}\"", a.name)).unwrap_or_else(|| id.clone())),
        };
        Ok(Outcome::msg(format!("Showing {what}")))
    }

    /// Points the view at something that still exists.
    fn fix_view(&mut self) {
        let exists = match &self.view {
            View::Welcome => true,
            View::Table(id) => self.project.tables.iter().any(|t| &t.id == id),
            View::Graph(id) | View::Plotted(id) => self.project.graphs.iter().any(|g| &g.id == id),
            View::Result(id) => self.project.analyses.iter().any(|a| &a.id == id),
        };
        if !exists {
            self.view = self.project.tables.iter().find(|t| t.locked_by.is_none()).map(|t| View::Table(t.id.clone())).unwrap_or_default();
        }
    }

    /// Graph id from an id or name; empty means `fallback`, else the graph shown in the app.
    fn resolve_graph_id(&self, key: &str, fallback: Option<&str>) -> Result<String, EngineError> {
        let key = key.trim();
        if key.is_empty() {
            if let Some(f) = fallback {
                return Ok(f.to_string());
            }
            let gi = self.graph_idx(&None)?;
            return Ok(self.project.graphs[gi].id.clone());
        }
        let gi = self.project.graph_index(key).ok_or_else(|| not_found("graph", key, self.graphs_list()))?;
        Ok(self.project.graphs[gi].id.clone())
    }

    /// Canonical name of a "Matched by" column of the graph's table.
    fn resolve_match_by(&self, graph_id: &str, by: &Option<String>) -> Result<Option<String>, EngineError> {
        let Some(c) = key(by) else { return Ok(None) };
        let g = self.project.graph(graph_id).ok_or_else(|| EngineError::NotFound(format!("graph '{graph_id}'")))?;
        let t = self.project.table_by_id(&g.table_id).ok_or_else(|| EngineError::NotFound("graph table".into()))?;
        Ok(Some(Self::column_name(t, c)?))
    }

    fn table_of_graph(&self, graph_id: &str) -> Result<String, EngineError> {
        self.project
            .graph(graph_id)
            .map(|g| g.table_id.clone())
            .ok_or_else(|| EngineError::NotFound(format!("graph '{graph_id}'")))
    }

    /// Runs an analysis for its outcome message; in the app R may still be calculating it
    /// in the background, which is not an error (the results appear when ready).
    fn analysis_outcome(&self, verb: &str, spec: &AnalysisSpec) -> Result<Outcome, EngineError> {
        match results::compute(&self.project, spec) {
            Ok(report) => Ok(self.column_analysis_outcome(verb, spec, &report)),
            Err(EngineError::Calculating) => Ok(Outcome::msg(format!("{verb} {} \"{}\": calculating in R", spec.id, spec.name))),
            Err(e) => Err(e),
        }
    }

    fn column_analysis_outcome(&self, verb: &str, spec: &AnalysisSpec, report: &results::Report) -> Outcome {
        let brackets = match report.brackets.len() {
            0 => String::new(),
            1 => "; 1 significance bracket on the graph".to_string(),
            n => format!("; {n} significance brackets on the graph"),
        };
        Outcome::msg(format!("{verb} {} \"{}\": {}{brackets}", spec.id, spec.name, report.title)).with_text(report.to_text())
    }

    /// Adds a t test / ANOVA after checking that it runs.
    /// `finalized` notes the final graph just made for it (from a quick graph).
    fn add_column_analysis(&mut self, name: String, kind: AnalysisKind, finalized: Option<String>) -> Result<Outcome, EngineError> {
        let graph_id = kind_graph(&kind).unwrap_or_default().to_string();
        let table_id = self.table_of_graph(&graph_id)?;
        let id = self.project.next_analysis_id();
        let basis = self.project.graph(&graph_id).map(GraphBasis::of);
        let spec = AnalysisSpec { id: id.clone(), name, table_id, kind, basis };
        let mut out = self.analysis_outcome("Created", &spec)?;
        self.only_one_drawn(&spec);
        self.project.analyses.push(spec);
        // Show the final graph with its new statistics when it was just made or is on screen.
        if finalized.is_some() || self.view == View::Graph(graph_id.clone()) {
            self.view = View::Graph(graph_id);
        } else {
            self.view = View::Result(id);
        }
        if let Some(note) = finalized {
            out.message = format!("{note}. {}", out.message);
        }
        Ok(out)
    }

    /// The name analyses are called after: the quick graph a final graph came from (so
    /// "t test of Graph 1", whether started from the quick or the final graph).
    fn source_graph_name(&self, graph_id: &str) -> String {
        let Some(g) = self.project.graph(graph_id) else { return String::new() };
        match &g.locked {
            Some(lock) => self.project.graph(&lock.from_graph).map(|q| q.name.clone()).unwrap_or_else(|| g.name.trim_end_matches(" (final)").to_string()),
            None => g.name.clone(),
        }
    }

    /// Statistics belong to final graphs: for a quick graph, makes its final graph (locked
    /// data copy) and points `graph_id` at it. Returns a note when it did. `xy` analyses
    /// (curve fits, regression, correlation) need an XY graph, the others a bar graph.
    fn final_graph_for_statistics(&mut self, graph_id: &mut String, shape: DataShape) -> Result<Option<String>, EngineError> {
        let gi = self.project.graph_index(graph_id).ok_or_else(|| not_found("graph", graph_id, self.graphs_list()))?;
        let g = &self.project.graphs[gi];
        if g.kind.shape() != shape {
            let what = match shape {
                DataShape::Bars => "t tests and ANOVA compare the bars of a column or grouped graph",
                DataShape::Xy => "curve fits, regression, correlation and area under the curve analyse the data sets of an XY graph",
                DataShape::Survival => "Kaplan-Meier analysis compares the curves of a survival graph",
                DataShape::Grid => "heatmaps have no statistics",
                DataShape::Slices => "pie and donut charts have no statistics",
                DataShape::Counts => "chi-square and Fisher's exact test analyse the counts of a contingency table",
            };
            return Err(EngineError::Invalid(format!("\"{}\" is {}; {what}", g.name, g.kind.shape().graph_name())));
        }
        if g.is_final() {
            return Ok(None);
        }
        let quick = g.name.clone();
        let fi = self.finalize(gi, None)?;
        let f = &self.project.graphs[fi];
        let note = format!("Finalized \"{quick}\" as {} \"{}\" (statistics run on a locked copy of its data)", f.id, f.name);
        *graph_id = f.id.clone();
        Ok(Some(note))
    }

    /// Makes a final graph from quick graph `gi`: a copy of it whose data are a locked copy
    /// of its table, so later edits to the data table don't change it. Returns its index.
    fn finalize(&mut self, gi: usize, name: Option<String>) -> Result<usize, EngineError> {
        let quick = self.project.graphs[gi].clone();
        if quick.is_final() {
            return Err(EngineError::Invalid(format!("\"{}\" is already a final graph", quick.name)));
        }
        let source = self
            .project
            .table_by_id(&quick.table_id)
            .cloned()
            .ok_or_else(|| EngineError::NotFound(format!("table of graph \"{}\"", quick.name)))?;
        let gid = self.project.next_graph_id();
        let name = name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).unwrap_or_else(|| format!("{} (final)", quick.name));
        let name = self.project.unique_graph_name(&name);
        let mut data = source.clone();
        data.id = self.project.next_table_id();
        data.name = format!("{} (locked for {gid})", source.name);
        data.locked_by = Some(gid.clone());
        let mut g = quick.clone();
        g.id = gid;
        g.name = name;
        g.table_id = data.id.clone();
        g.locked = Some(otto_core::LockInfo {
            from_graph: quick.id.clone(),
            from_table: source.name.clone(),
            table_id: source.id.clone(),
            at: utc_now(),
        });
        self.project.tables.push(data);
        self.project.graphs.push(g);
        Ok(self.project.graphs.len() - 1)
    }

    /// Brings a final graph back in step with the quick graph it came from: a fresh locked
    /// copy of the data table, and the quick graph's current type, columns, filters, error
    /// bars and axis ranges. What belongs to the final graph alone - its name, format,
    /// colours, symbols, axis titles and statistics - stays, and the statistics are
    /// recalculated on the new data.
    fn refresh_final(&mut self, a: GraphRef) -> Result<Outcome, EngineError> {
        let gi = self.graph_idx(&a.graph)?;
        let final_graph = self.project.graphs[gi].clone();
        let Some(lock) = final_graph.locked.clone() else {
            return Err(EngineError::Invalid(format!(
                "\"{}\" is a quick graph; graph_finalize makes a final graph from it",
                final_graph.name
            )));
        };
        let quick = self.project.graph(&lock.from_graph).cloned().ok_or_else(|| {
            EngineError::NotFound(format!("the quick graph \"{}\" was made from (it has been deleted)", final_graph.name))
        })?;
        let source = self
            .project
            .table_by_id(&lock.table_id)
            .or_else(|| self.project.table_by_id(&quick.table_id))
            .cloned()
            .ok_or_else(|| EngineError::NotFound(format!("the data table \"{}\" was locked from", lock.from_table)))?;

        // A fresh locked copy, keeping the id the statistics already point at.
        let locked_id = final_graph.table_id.clone();
        let ti = self
            .project
            .tables
            .iter()
            .position(|t| t.id == locked_id)
            .ok_or_else(|| EngineError::NotFound(format!("the locked table of \"{}\"", final_graph.name)))?;
        let mut data = source.clone();
        data.id = locked_id.clone();
        data.name = format!("{} (locked for {})", source.name, final_graph.id);
        data.locked_by = Some(final_graph.id.clone());
        self.project.tables[ti] = data;

        // What the quick graph settles, taken over; what the final graph owns, left alone.
        let g = &mut self.project.graphs[gi];
        g.kind = quick.kind;
        g.x = quick.x.clone();
        g.y = quick.y.clone();
        g.group = quick.group.clone();
        g.filters = quick.filters.clone();
        g.summary = quick.summary;
        g.whiskers = quick.whiskers;
        g.palette = quick.palette;
        g.highlight = quick.highlight;
        g.focus = quick.focus.clone();
        g.volcano = quick.volcano.clone();
        g.replicate_by = quick.replicate_by.clone();
        g.x_axis.min = quick.x_axis.min;
        g.x_axis.max = quick.x_axis.max;
        g.x_axis.scale = quick.x_axis.scale;
        g.y_axis.min = quick.y_axis.min;
        g.y_axis.max = quick.y_axis.max;
        g.y_axis.scale = quick.y_axis.scale;
        g.locked = Some(otto_core::LockInfo { from_table: source.name.clone(), at: utc_now(), ..lock });
        let (gid, gname) = (g.id.clone(), g.name.clone());

        // The bars may be different ones now, so the statistics are run again on them.
        let view = self.view.clone();
        let again = self.recalculate(RecalculateArgs { graph: Some(gid), ..Default::default() });
        self.view = view;
        let mut out = self.graph_outcome(gi, "Updated");
        out.message = format!("Updated \"{gname}\" from \"{}\": fresh locked copy of \"{}\"", quick.name, source.name);
        match again {
            Ok(o) if o.message.starts_with("Recalculated") => out.message.push_str(&format!(". {}", o.message)),
            Err(e) => out.message.push_str(&format!(". Its statistics could not be recalculated: {e}")),
            _ => {}
        }
        Ok(out)
    }

    /// Older projects attached statistics to quick graphs: give each such graph a final
    /// graph and move its analyses there. Returns how many graphs were finalized.
    pub(crate) fn finalize_legacy_analyses(&mut self) -> usize {
        let mut graphs: Vec<String> = Vec::new();
        for a in &self.project.analyses {
            if let Some(g) = analysis_graph(a).and_then(|id| self.project.graph(id)) {
                if !g.is_final() && !graphs.contains(&g.id) {
                    graphs.push(g.id.clone());
                }
            }
        }
        for quick_id in &graphs {
            let Some(gi) = self.project.graph_index(quick_id) else { continue };
            let old_table = self.project.graphs[gi].table_id.clone();
            let Ok(fi) = self.finalize(gi, None) else { continue };
            let (fid, ftable) = (self.project.graphs[fi].id.clone(), self.project.graphs[fi].table_id.clone());
            for an in self.project.analyses.iter_mut().filter(|a| analysis_graph(a) == Some(quick_id.as_str())) {
                if let Some(gid) = an.kind.graph_id_mut() {
                    *gid = fid.clone();
                }
                an.table_id = ftable.clone();
                if let Some(b) = an.basis.as_mut().filter(|b| b.table_id == old_table) {
                    b.table_id = ftable.clone();
                }
            }
        }
        graphs.len()
    }

    /// Replaces the parameters of a t test / ANOVA after checking that they run. `rebase`
    /// records the graph as it is now as the analysis's basis (its data sets were chosen
    /// anew); otherwise an out-of-date analysis stays out of date.
    fn replace_column_analysis(&mut self, ai: usize, kind: AnalysisKind, rebase: bool) -> Result<Outcome, EngineError> {
        let mut spec = self.project.analyses[ai].clone();
        let graph_id = kind_graph(&kind).unwrap_or_default().to_string();
        if self.project.graph(&graph_id).is_some_and(|g| !g.is_final()) {
            return Err(EngineError::Invalid(
                "statistics belong to final graphs; analysis_create_* on a quick graph makes its final graph".into(),
            ));
        }
        spec.table_id = self.table_of_graph(&graph_id)?;
        spec.kind = kind;
        if rebase || spec.basis.is_none() {
            spec.basis = self.project.graph(&graph_id).map(GraphBasis::of);
        }
        let out = match crate::column_tests::out_of_date(&self.project, &spec) {
            // Still out of date (e.g. only the display options changed): nothing to run yet.
            Some(reasons) => Outcome::msg(format!(
                "Updated {} \"{}\" (still out of date: {}; use analysis_recalculate)",
                spec.id,
                spec.name,
                reasons.join("; ")
            )),
            None => self.analysis_outcome("Updated", &spec)?,
        };
        // Changes made while looking at the graph stay on the graph; otherwise show the results.
        if self.view != View::Graph(graph_id) {
            self.view = View::Result(spec.id.clone());
        }
        self.only_one_drawn(&spec);
        self.project.analyses[ai] = spec;
        Ok(out)
    }

    /// A graph shows one analysis at a time: when `spec` is drawn on it, the others are
    /// taken off, so brackets and curves never pile up on top of each other. They keep
    /// their results and can be put back from the graph's statistics list.
    fn only_one_drawn(&mut self, spec: &AnalysisSpec) {
        if spec.kind.drawn() != Some(true) {
            return;
        }
        let Some(graph_id) = spec.kind.graph_id() else { return };
        let graph_id = graph_id.to_string();
        for other in &mut self.project.analyses {
            if other.id != spec.id && other.kind.graph_id() == Some(graph_id.as_str()) {
                other.kind.set_drawn(false);
            }
        }
    }

    /// Constraints must name parameters of the model.
    fn check_fixed(&self, spec: &otto_core::analysis::FitSpec) -> Result<(), EngineError> {
        let params = spec.model.params();
        if let Some(bad) = spec.fixed.iter().find(|p| !params.contains(&p.name.as_str())) {
            return Err(EngineError::Invalid(format!(
                "'{}' is not a parameter of {} (parameters: {})",
                bad.name,
                spec.model.label(),
                params.join(", ")
            )));
        }
        if let Some(bad) = spec.fixed.iter().find(|p| !p.value.is_finite()) {
            return Err(EngineError::Invalid(format!("constraint {} needs a number", bad.name)));
        }
        Ok(())
    }

    /// Reruns out-of-date analyses on the bars their graph shows now.
    fn recalculate(&mut self, a: RecalculateArgs) -> Result<Outcome, EngineError> {
        let targets: Vec<usize> = match key(&a.analysis) {
            Some(_) => vec![self.analysis_idx(&a.analysis)?],
            None => {
                let gi = self.graph_idx(&a.graph)?;
                let gid = self.project.graphs[gi].id.clone();
                let list: Vec<usize> = (0..self.project.analyses.len())
                    .filter(|&i| {
                        let an = &self.project.analyses[i];
                        analysis_graph(an) == Some(gid.as_str()) && crate::column_tests::out_of_date(&self.project, an).is_some()
                    })
                    .collect();
                if list.is_empty() {
                    return Ok(Outcome::msg(format!("All analyses of \"{}\" are up to date", self.project.graphs[gi].name)));
                }
                list
            }
        };
        let view = self.view.clone();
        let mut done = Vec::new();
        let mut failed = Vec::new();
        for ai in targets {
            let an = &self.project.analyses[ai];
            let (id, name) = (an.id.clone(), an.name.clone());
            let rebound =
                crate::column_tests::rebind_to_graph(&self.project, &an.kind).and_then(|kind| self.replace_column_analysis(ai, kind, true));
            match rebound {
                Ok(_) => done.push(format!("{id} \"{name}\"")),
                Err(e) => failed.push(format!("{id} \"{name}\": {e}")),
            }
        }
        // Stay where the user is (replacing an analysis normally shows its results).
        self.view = view;
        if done.is_empty() {
            return Err(EngineError::Invalid(format!("could not recalculate {}", failed.join("; "))));
        }
        let mut msg = format!("Recalculated {} with the bars shown", done.join(", "));
        if !failed.is_empty() {
            msg.push_str(&format!("; not possible for {}", failed.join("; ")));
        }
        Ok(Outcome::msg(msg))
    }

    /// After a graph-type change, drops X columns the new type can't use and picks a
    /// replacement when the type needs an X: XY graphs take exactly one numeric X, bar
    /// graphs take categorical (or text) X columns. Returns a note about what changed.
    fn fit_mapping_to_kind(&mut self, gi: usize) -> Option<String> {
        let g = &self.project.graphs[gi];
        let table = self.project.table_by_id(&g.table_id)?;
        let is_numeric = |name: &str| table.column(name).is_some_and(|c| c.kind() == ColumnKind::Numeric);
        let (kind, old_x, mut y, group) = (g.kind, g.x.clone(), g.y.clone(), g.group.clone());
        // A survival graph needs an event column (1 / 0) as Y.
        let mut y_note = None;
        if kind == GraphKind::Survival && !(y.len() == 1 && is_binary(table, &y[0])) {
            if let (_, Some(event)) = survival_columns(table) {
                y_note = Some(format!("Y set to the event column {event}"));
                y = vec![event];
            }
        }

        if kind.x_is_derived() {
            // A QQ plot takes no X column.
            if !self.project.graphs[gi].x.is_empty() {
                self.project.graphs[gi].x.clear();
                return Some("X cleared: a QQ plot's X is the theoretical quantile".to_string());
            }
            return y_note;
        }
        let mut x: Vec<String> = if kind.is_xy() {
            old_x.iter().filter(|c| is_numeric(c)).take(1).cloned().collect()
        } else {
            old_x.iter().filter(|c| !is_numeric(c)).cloned().collect()
        };
        let removed: Vec<&String> = old_x.iter().filter(|c| !x.contains(c)).collect();
        // Pick a replacement when X was emptied, or when the new type requires one.
        let needs_x = kind.is_xy() || kind.is_grouped() || kind == GraphKind::Heatmap || !removed.is_empty();
        let needs_x = needs_x && !kind.x_is_derived();
        if x.is_empty() && needs_x {
            let pick = if kind.is_xy() {
                table.columns.iter().find(|c| c.kind() == ColumnKind::Numeric && !y.contains(&c.name))
            } else {
                table.columns.iter().find(|c| {
                    c.kind() == ColumnKind::Categorical && !group.contains(&c.name)
                }).or_else(|| table.columns.iter().find(|c| c.kind() == ColumnKind::Text && !group.contains(&c.name)))
            };
            x.extend(pick.map(|c| c.name.clone()));
        }
        let y_changed = y != self.project.graphs[gi].y;
        if x == old_x {
            if y_changed {
                self.project.graphs[gi].y = y;
            }
            return y_note;
        }
        let note = match (removed.is_empty(), x.is_empty()) {
            (true, _) => format!("X set to {}", x.join(otto_core::COMBINE_SEP)),
            (false, true) => format!("removed X {} (not usable for this graph type)", quoted(&removed)),
            (false, false) => format!(
                "replaced X {} with {} for this graph type",
                quoted(&removed),
                x.join(otto_core::COMBINE_SEP)
            ),
        };
        let g = &mut self.project.graphs[gi];
        g.group.retain(|c| !x.contains(c));
        g.x = x;
        if y_changed {
            g.y = y;
        }
        Some(match y_note {
            Some(n) => format!("{note}; {n}"),
            None => note,
        })
    }

    fn graph_create(&mut self, a: GraphCreateArgs) -> Result<Outcome, EngineError> {
        let ti = self.table_idx(&a.table)?;
        let t = &self.project.tables[ti];
        if t.locked_by.is_some() {
            return Err(EngineError::Invalid(format!(
                "{} is the locked data of a final graph; graphs are made from data tables (duplicate {} for an editable copy)",
                t.id, t.id
            )));
        }
        let numeric: Vec<&str> = t.columns.iter().filter(|c| c.kind() == ColumnKind::Numeric).map(|c| c.name.as_str()).collect();
        let categorical: Vec<&str> = t
            .columns
            .iter()
            .filter(|c| c.kind() == ColumnKind::Categorical)
            .chain(t.columns.iter().filter(|c| c.kind() == ColumnKind::Text))
            .map(|c| c.name.as_str())
            .collect();
        let mut y: Vec<String> = a.y.iter().map(|c| Self::column_name(t, c)).collect::<Result<_, _>>()?;
        // Survival graphs: a time column as X and an event (1 / 0) column as Y.
        let (mut surv_time, surv_event) = survival_columns(t);
        if a.kind == GraphKind::Survival && y.is_empty() {
            y.extend(surv_event.clone());
        }
        // Time and event columns picked together (e.g. both highlighted): the 1 / 0 one is
        // the event, the other the time.
        if a.kind == GraphKind::Survival && y.len() > 1 {
            if let Some(ev) = y.iter().find(|c| is_binary(t, c)).cloned() {
                surv_time = y.iter().find(|c| **c != ev).cloned().or(surv_time);
                y = vec![ev];
            }
        }
        let x: Vec<String> = match a.x {
            Some(v) => Self::column_names(t, v)?,
            None if a.kind == GraphKind::Survival => surv_time.into_iter().filter(|c| !y.contains(c)).collect(),
            // A QQ plot's X is the theoretical quantile Otto computes.
            None if a.kind.x_is_derived() => Vec::new(),
            None if a.kind.is_xy() => numeric.iter().find(|c| !y.iter().any(|y| y == *c)).map(|s| s.to_string()).into_iter().collect(),
            // Several Y columns on a column-like graph: one bar per column (like a Prism column table).
            None if a.kind.is_column_like() && y.len() > 1 => Vec::new(),
            None => categorical.first().map(|s| s.to_string()).into_iter().collect(),
        };
        if y.is_empty() {
            if let Some(c) = numeric.iter().find(|c| !x.iter().any(|x| x == *c)) {
                y.push(c.to_string());
            }
        }
        let mut group: Vec<String> = match a.group {
            Some(v) => Self::column_names(t, v)?,
            None if (a.kind.is_grouped() || matches!(a.kind, GraphKind::Survival | GraphKind::Heatmap)) && y.len() < 2 => {
                categorical.iter().find(|c| !x.iter().any(|x| x == *c)).map(|s| s.to_string()).into_iter().collect()
            }
            None => Vec::new(),
        };
        group.retain(|c| !x.contains(c));
        let mut filters = Vec::new();
        for mut f in a.filters {
            f.column = Self::column_name(t, &f.column)?;
            filters.push(f);
        }
        let table_id = t.id.clone();
        let id = self.project.next_graph_id();
        let name = a.name.unwrap_or_else(|| Project::numbered_name(self.project.graphs.iter().map(|g| g.name.as_str()), "Graph"));
        let mut g = GraphSpec::new(id.clone(), name, table_id, a.kind);
        g.x = x;
        g.y = y;
        g.group = group;
        g.filters = filters;
        if let Some(f) = a.format {
            g.format = f;
        } else if a.kind.is_grouped() || matches!(a.kind, GraphKind::XyPoints | GraphKind::XyPointsLine | GraphKind::XyLine | GraphKind::Heatmap) {
            g.format = otto_core::Format::Wide;
        }
        self.project.graphs.push(g);
        self.view = View::Graph(id);
        Ok(self.graph_outcome(self.project.graphs.len() - 1, "Created"))
    }

    fn summary(&self) -> Outcome {
        let tables: Vec<_> = self
            .project
            .tables
            .iter()
            .map(|t| {
                let mut v = json!({
                    "id": t.id, "name": t.name, "rows": t.nrows,
                    "columns": t.columns.iter().map(|c| json!({"name": c.name, "kind": c.kind()})).collect::<Vec<_>>()
                });
                if let Some(g) = &t.locked_by {
                    v["locked_data_of_final_graph"] = json!(g);
                }
                v
            })
            .collect();
        let graphs: Vec<_> = self
            .project
            .graphs
            .iter()
            .map(|g| {
                let mut v = serde_json::to_value(g).unwrap_or_default();
                if let Err(e) = self.render_graph(&g.id) {
                    v["problem"] = json!(e.to_string());
                }
                v
            })
            .collect();
        let analyses: Vec<_> = self
            .project
            .analyses
            .iter()
            .map(|a| {
                let mut v = serde_json::to_value(a).unwrap_or_default();
                if let Some(obj) = v.as_object_mut() {
                    obj.remove("basis");
                }
                if let Some(reasons) = crate::column_tests::out_of_date(&self.project, a) {
                    v["out_of_date"] = json!(reasons);
                }
                v
            })
            .collect();
        Outcome::msg(format!(
            "Project \"{}\": {} tables, {} graphs, {} results",
            self.project.name,
            tables.len(),
            graphs.len(),
            analyses.len()
        ))
        .with_data(json!({
            "name": self.project.name,
            "path": self.path.as_ref().map(|p| p.display().to_string()),
            "unsaved_changes": self.dirty,
            "showing": self.view,
            "tables": tables,
            "graphs": graphs,
            "analyses": analyses,
        }))
    }
}

/// Column-by-column description of a table.
pub fn describe(t: &DataTable) -> serde_json::Value {
    let cols: Vec<_> = t
        .columns
        .iter()
        .map(|c| {
            let mut v = json!({ "name": c.name, "kind": c.kind(), "missing": c.missing_count() });
            match &c.data {
                ColumnData::Numeric { values, .. } => {
                    let vals: Vec<f64> = values.iter().flatten().copied().collect();
                    if let Some(d) = otto_plot::describe::describe(&vals) {
                        v["n"] = json!(d.n);
                        v["min"] = json!(d.min);
                        v["max"] = json!(d.max);
                        v["mean"] = json!(d.mean);
                    }
                }
                ColumnData::Categorical { levels, codes } => {
                    let counts: Vec<_> = levels
                        .iter()
                        .enumerate()
                        .map(|(i, l)| json!({ "level": l, "n": codes.iter().filter(|c| **c == Some(i as u32)).count() }))
                        .collect();
                    v["levels"] = json!(counts);
                }
                ColumnData::Text { .. } => {
                    let levels = c.levels();
                    v["distinct"] = json!(levels.len());
                    v["examples"] = json!(levels.iter().take(5).collect::<Vec<_>>());
                }
            }
            v
        })
        .collect();
    json!({ "id": t.id, "name": t.name, "rows": t.nrows, "columns": cols })
}

/// A numeric column holding only 0 and 1 (an event column of a survival table).
fn is_binary(t: &DataTable, name: &str) -> bool {
    t.column(name).is_some_and(|c| {
        c.kind() == ColumnKind::Numeric && (0..t.nrows).filter_map(|r| c.number(r)).all(|v| v == 0.0 || v == 1.0) && (0..t.nrows).any(|r| c.number(r).is_some())
    })
}

/// Likely time and event columns of a survival table: the event column is numeric with
/// only 0 / 1 (preferring names like "event", "status", "death"); the time column is
/// another numeric column (preferring names like "time", "days", "months").
fn survival_columns(t: &DataTable) -> (Option<String>, Option<String>) {
    let named = |c: &Column, words: &[&str]| words.iter().any(|w| c.name.to_lowercase().contains(w));
    let binary: Vec<&Column> = t.columns.iter().filter(|c| is_binary(t, &c.name)).collect();
    let event = binary
        .iter()
        .find(|c| named(c, &["event", "status", "death", "dead", "died", "censor", "outcome"]))
        .or_else(|| binary.first())
        .map(|c| c.name.clone());
    let numeric: Vec<&Column> = t.columns.iter().filter(|c| c.kind() == ColumnKind::Numeric && Some(&c.name) != event.as_ref()).collect();
    let time = numeric
        .iter()
        .find(|c| named(c, &["time", "day", "week", "month", "year", "survival", "duration", "follow"]))
        .or_else(|| numeric.iter().find(|c| !binary.iter().any(|b| b.name == c.name)))
        .map(|c| c.name.clone());
    (time, event)
}

fn quoted(names: &[&String]) -> String {
    names.iter().map(|n| format!("'{n}'")).collect::<Vec<_>>().join(", ")
}

/// The graph an analysis uses (None for table-based analyses).
fn kind_graph(kind: &AnalysisKind) -> Option<&str> {
    kind.graph_id()
}

pub(crate) fn analysis_graph(a: &AnalysisSpec) -> Option<&str> {
    kind_graph(&a.kind)
}

/// The current time as "YYYY-MM-DD HH:MM UTC" (for final graph provenance).
fn utc_now() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02} {:02}:{:02} UTC", rem / 3600, rem % 3600 / 60)
}
