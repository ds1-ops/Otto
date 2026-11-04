//! A project bundles tables, graphs and analyses, saved as one readable JSON file (`.otto`).

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::spec::{AnalysisSpec, GraphSpec};
use crate::table::DataTable;
use crate::CoreError;

pub const FORMAT_VERSION: u32 = 1;
pub const FILE_EXTENSION: &str = "otto";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct IdCounters {
    pub table: u32,
    pub graph: u32,
    pub analysis: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub format_version: u32,
    pub name: String,
    #[serde(default)]
    pub tables: Vec<DataTable>,
    #[serde(default)]
    pub graphs: Vec<GraphSpec>,
    #[serde(default)]
    pub analyses: Vec<AnalysisSpec>,
    #[serde(default)]
    pub ids: IdCounters,
}

impl Default for Project {
    fn default() -> Self {
        Project::new("Untitled")
    }
}

/// Finds an item by id, then by exact name, then by case-insensitive name.
fn find_index<T>(items: &[T], key: &str, id: impl Fn(&T) -> &str, name: impl Fn(&T) -> &str) -> Option<usize> {
    let key = key.trim();
    items
        .iter()
        .position(|t| id(t).eq_ignore_ascii_case(key))
        .or_else(|| items.iter().position(|t| name(t) == key))
        .or_else(|| items.iter().position(|t| name(t).eq_ignore_ascii_case(key)))
}

impl Project {
    pub fn new(name: impl Into<String>) -> Self {
        Project {
            format_version: FORMAT_VERSION,
            name: name.into(),
            tables: Vec::new(),
            graphs: Vec::new(),
            analyses: Vec::new(),
            ids: IdCounters::default(),
        }
    }

    pub fn next_table_id(&mut self) -> String {
        self.ids.table += 1;
        format!("T{}", self.ids.table)
    }

    pub fn next_graph_id(&mut self) -> String {
        self.ids.graph += 1;
        format!("G{}", self.ids.graph)
    }

    pub fn next_analysis_id(&mut self) -> String {
        self.ids.analysis += 1;
        format!("R{}", self.ids.analysis)
    }

    pub fn table_index(&self, key: &str) -> Option<usize> {
        find_index(&self.tables, key, |t| &t.id, |t| &t.name)
    }

    pub fn graph_index(&self, key: &str) -> Option<usize> {
        find_index(&self.graphs, key, |g| &g.id, |g| &g.name)
    }

    pub fn analysis_index(&self, key: &str) -> Option<usize> {
        find_index(&self.analyses, key, |a| &a.id, |a| &a.name)
    }

    pub fn table(&self, key: &str) -> Option<&DataTable> {
        self.table_index(key).map(|i| &self.tables[i])
    }

    pub fn table_by_id(&self, id: &str) -> Option<&DataTable> {
        self.tables.iter().find(|t| t.id == id)
    }

    pub fn graph(&self, key: &str) -> Option<&GraphSpec> {
        self.graph_index(key).map(|i| &self.graphs[i])
    }

    /// A name not yet used by any item in `existing`, e.g. "Graph 3".
    pub fn unique_name<'a>(existing: impl Iterator<Item = &'a str> + Clone, base: &str) -> String {
        let taken = |n: &str| existing.clone().any(|e| e.eq_ignore_ascii_case(n));
        if !taken(base) {
            return base.to_string();
        }
        // "(2)" can't be mistaken for part of the name ("t test of Graph 1 (2)").
        (2..).map(|i| format!("{base} ({i})")).find(|n| !taken(n)).unwrap()
    }

    /// The first free default name "<stem> 1", "<stem> 2", … (e.g. "Graph 3").
    pub fn numbered_name<'a>(existing: impl Iterator<Item = &'a str> + Clone, stem: &str) -> String {
        (1..).map(|i| format!("{stem} {i}")).find(|n| !existing.clone().any(|e| e.eq_ignore_ascii_case(n))).unwrap()
    }

    pub fn unique_table_name(&self, base: &str) -> String {
        Self::unique_name(self.tables.iter().map(|t| t.name.as_str()), base)
    }

    pub fn unique_graph_name(&self, base: &str) -> String {
        Self::unique_name(self.graphs.iter().map(|g| g.name.as_str()), base)
    }

    pub fn unique_analysis_name(&self, base: &str) -> String {
        Self::unique_name(self.analyses.iter().map(|a| a.name.as_str()), base)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("project serializes")
    }

    pub fn from_json(text: &str) -> Result<Self, CoreError> {
        let p: Project = serde_json::from_str(text).map_err(|e| CoreError::Invalid(format!("not a valid Otto project: {e}")))?;
        if p.format_version > FORMAT_VERSION {
            return Err(CoreError::Invalid(format!(
                "project was saved by a newer Otto (format {}, this version reads {FORMAT_VERSION})",
                p.format_version
            )));
        }
        Ok(p)
    }

    pub fn save(&self, path: &Path) -> Result<(), CoreError> {
        std::fs::write(path, self.to_json()).map_err(|e| CoreError::Io(format!("{}: {e}", path.display())))
    }

    pub fn load(path: &Path) -> Result<Self, CoreError> {
        let text = std::fs::read_to_string(path).map_err(|e| CoreError::Io(format!("{}: {e}", path.display())))?;
        Self::from_json(&text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{GraphKind, GraphSpec};

    #[test]
    fn default_names_count_up() {
        let names = ["Graph 1", "Graph 2", "Data 1", "t test of Graph 1", "Graph 1 (final)"];
        let n = |stem: &str| Project::numbered_name(names.iter().copied(), stem);
        assert_eq!(n("Graph"), "Graph 3");
        assert_eq!(n("Data"), "Data 2");
        let u = |base: &str| Project::unique_name(names.iter().copied(), base);
        assert_eq!(u("Graph 1 (final)"), "Graph 1 (final) (2)");
        assert_eq!(u("t test of Graph 1"), "t test of Graph 1 (2)");
        assert_eq!(u("Fresh"), "Fresh");
    }
    use crate::table::Column;

    #[test]
    fn json_roundtrip_and_lookup() {
        let mut p = Project::new("p");
        let id = p.next_table_id();
        let mut t = DataTable::new(id.clone(), "Cytokines");
        t.add_column(Column::numeric("IL2", vec![Some(1.0), None]), None).unwrap();
        t.add_column(Column::categorical("Cond", &[Some("WT".into()), Some("KO".into())]), None).unwrap();
        p.tables.push(t);
        let gid = p.next_graph_id();
        p.graphs.push(GraphSpec::new(gid, "Graph 1", id, GraphKind::ColumnBar));
        let back = Project::from_json(&p.to_json()).unwrap();
        assert_eq!(back, p);
        assert_eq!(back.table_index("t1"), Some(0));
        assert_eq!(back.table_index("cytokines"), Some(0));
        assert_eq!(back.graph_index("Graph 1"), Some(0));
        assert_eq!(p.unique_graph_name("Graph 1"), "Graph 1 (2)");
    }
}
