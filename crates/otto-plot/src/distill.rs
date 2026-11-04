//! Distills the plotted subset out of a multivariable table: applies filters, drops
//! missing values, combines several factor columns into one category ("A | B"), melts
//! multiple Y columns into data sets and computes summaries.
//! The same `PlotData` drives drawing and every data/code export, so they always agree.

use std::collections::HashMap;

use otto_core::{Center, Column, ColumnKind, DataTable, ErrorBars, GraphKind, GraphSpec, Summary, COMBINE_SEP};
use crate::describe::{describe, Descriptive};

use crate::PlotError;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum XValue {
    Num(f64),
    /// Index into `PlotData::x_levels`.
    Cat(usize),
}

impl XValue {
    pub fn num(self) -> f64 {
        match self {
            XValue::Num(v) => v,
            XValue::Cat(i) => i as f64,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlotPoint {
    /// 0-based row in the source table.
    pub row: usize,
    pub x: XValue,
    /// Index into `PlotData::sets`.
    pub set: usize,
    pub y: f64,
    /// Which replicate the value belongs to (index into `PlotData::replicates` when the
    /// graph names a replicate column; otherwise its order among the values at this X).
    pub replicate: usize,
}

/// Summary of the points sharing one X position and data set (one bar or one symbol).
#[derive(Clone, Debug, PartialEq)]
pub struct GroupStats {
    pub x: XValue,
    pub set: usize,
    pub stats: Descriptive,
    /// Bar height / symbol position (mean or median).
    pub center: f64,
    /// Error bar ends (equal to `center` when there is no error bar).
    pub low: f64,
    pub high: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlotData {
    pub kind: GraphKind,
    pub summary: Summary,
    pub x_title: String,
    pub y_title: String,
    /// Display name of the X factor ("Condition | Additive") or the numeric X column.
    pub x_name: String,
    /// Columns that make up X (one per part; "Variable" when Y columns are the bars).
    pub x_columns: Vec<String>,
    /// Column header for Y values in exports.
    pub y_name: String,
    /// Display name of the data-set factor, if the data are split into data sets.
    pub set_name: Option<String>,
    /// Columns that make up the data sets ("Variable" stands for the Y column).
    pub set_columns: Vec<String>,
    /// Categories along a categorical X axis (bar graphs), labelled "A | B".
    pub x_levels: Vec<String>,
    /// The parts of each X level, one value per `x_columns` entry.
    pub x_parts: Vec<Vec<String>>,
    /// Data sets (legend entries), labelled "A | B".
    pub sets: Vec<String>,
    /// Palette slot of each data set: its place in the table's own level order, so a group
    /// keeps its colour when rows are filtered out or the graph is split up.
    pub set_colors: Vec<usize>,
    /// Palette slot of each X category (column bar, box and violin graphs).
    pub x_colors: Vec<usize>,
    /// Per-point labels (volcano plots); empty when the graph has none.
    pub point_labels: Vec<String>,
    /// The column that identifies replicates (XY graphs showing each replicate), if any.
    pub replicate_name: Option<String>,
    /// Replicate labels (values of that column).
    pub replicates: Vec<String>,
    /// The parts of each data set, one value per `set_columns` entry.
    pub set_parts: Vec<Vec<String>>,
    pub points: Vec<PlotPoint>,
    pub groups: Vec<GroupStats>,
}

impl PlotData {
    /// Palette slot of data set `i`.
    pub fn set_color(&self, i: usize) -> usize {
        self.set_colors.get(i).copied().unwrap_or(i)
    }

    /// The names that carry a color, and so can be put in focus: the X categories of a
    /// column-like graph (its bars), the data sets everywhere else.
    pub fn color_groups(&self) -> &[String] {
        if self.kind.is_column_like() {
            &self.x_levels
        } else {
            &self.sets
        }
    }

    /// Palette slot of X category `i`.
    pub fn x_color(&self, i: usize) -> usize {
        self.x_colors.get(i).copied().unwrap_or(i)
    }

    pub fn x_label(&self, x: XValue) -> String {
        match x {
            XValue::Num(v) => otto_core::table::format_number(v),
            XValue::Cat(i) => self.x_levels[i].clone(),
        }
    }

    /// Label of a bar (data set for analyses): the X category, plus " | data set"
    /// when the graph has several data sets.
    pub fn group_label(&self, g: &GroupStats) -> String {
        let x = self.x_label(g.x);
        if self.sets.len() > 1 {
            format!("{x}{COMBINE_SEP}{}", self.sets[g.set])
        } else {
            x
        }
    }

    /// Indices into `groups` in the order the bars appear on the graph.
    pub fn bar_order(&self) -> Vec<usize> {
        let mut idx: Vec<usize> = (0..self.groups.len()).collect();
        let key = |g: &GroupStats| (g.x.num(), g.set);
        if self.kind == GraphKind::GroupedSeparated {
            idx.sort_by(|&a, &b| (self.groups[a].set, key(&self.groups[a]).0).partial_cmp(&(self.groups[b].set, key(&self.groups[b]).0)).unwrap());
        } else {
            idx.sort_by(|&a, &b| key(&self.groups[a]).partial_cmp(&key(&self.groups[b])).unwrap());
        }
        idx
    }

    /// Points belonging to one bar/symbol group.
    pub fn points_of<'a>(&'a self, g: &'a GroupStats) -> impl Iterator<Item = &'a PlotPoint> + 'a {
        self.points.iter().filter(move |p| p.set == g.set && p.x == g.x)
    }
}

fn mapping(msg: impl Into<String>) -> PlotError {
    PlotError::Mapping(msg.into())
}

/// Whether the graph draws one line per replicate, so that every value has to be traced
/// to the subject it belongs to (and exports say which subject that is).
pub fn replicate_lines(kind: GraphKind, summary: Summary) -> bool {
    kind.has_line() && summary.replicates == otto_core::Replicates::Individual
}

fn names_of(table: &DataTable, kind: Option<ColumnKind>) -> String {
    table
        .columns
        .iter()
        .filter(|c| kind.is_none_or(|k| c.kind() == k))
        .map(|c| c.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Category label of a cell; numbers are used as labels too.
fn level_label(col: &Column, row: usize) -> Option<String> {
    match col.kind() {
        ColumnKind::Numeric => col.number(row).map(otto_core::table::format_number),
        _ => col.label(row).map(str::to_string),
    }
}

fn columns<'a>(table: &'a DataTable, names: &[String]) -> Result<Vec<&'a Column>, PlotError> {
    names.iter().map(|n| table.require_column(n).map_err(|e| mapping(e.to_string()))).collect()
}

/// One or more columns combined into a single factor. Levels are the value combinations
/// that occur, ordered by each column's level order (first column first).
struct Factor<'a> {
    cols: Vec<&'a Column>,
    maps: Vec<HashMap<String, usize>>,
    levels: Vec<Vec<usize>>,
    labels: Vec<Vec<String>>,
}

impl<'a> Factor<'a> {
    fn new(cols: Vec<&'a Column>, rows: &[usize]) -> Self {
        let labels: Vec<Vec<String>> = cols.iter().map(|c| c.levels()).collect();
        let maps = labels.iter().map(|ls| ls.iter().enumerate().map(|(i, l)| (l.clone(), i)).collect()).collect();
        let mut f = Factor { cols, maps, levels: Vec::new(), labels };
        let mut levels: Vec<Vec<usize>> = rows.iter().filter_map(|&r| f.key(r)).collect();
        levels.sort();
        levels.dedup();
        f.levels = levels;
        f
    }

    fn key(&self, row: usize) -> Option<Vec<usize>> {
        self.cols.iter().zip(&self.maps).map(|(c, m)| level_label(c, row).and_then(|l| m.get(&l).copied())).collect()
    }

    fn index(&self, row: usize) -> Option<usize> {
        self.levels.binary_search(&self.key(row)?).ok()
    }

    fn len(&self) -> usize {
        self.levels.len()
    }

    fn parts(&self, i: usize) -> Vec<String> {
        self.levels[i].iter().enumerate().map(|(c, &l)| self.labels[c][l].clone()).collect()
    }

    fn label(&self, i: usize) -> String {
        self.parts(i).join(COMBINE_SEP)
    }

    fn column_names(&self) -> Vec<String> {
        self.cols.iter().map(|c| c.name.clone()).collect()
    }

    fn name(&self) -> String {
        self.column_names().join(COMBINE_SEP)
    }
}

pub fn distill(spec: &GraphSpec, table: &DataTable) -> Result<PlotData, PlotError> {
    let kind = spec.kind;
    if spec.y.is_empty() {
        return Err(mapping(format!(
            "choose at least one numeric Y column (numeric columns: {})",
            names_of(table, Some(ColumnKind::Numeric))
        )));
    }
    let ycols = columns(table, &spec.y)?;
    for c in &ycols {
        if c.kind() != ColumnKind::Numeric {
            return Err(mapping(format!(
                "Y column '{}' is {}, but Y must be numeric (numeric columns: {})",
                c.name,
                c.kind().label().to_lowercase(),
                names_of(table, Some(ColumnKind::Numeric))
            )));
        }
    }
    let xcols = columns(table, &spec.x)?;
    let gcols = columns(table, &spec.group)?;
    if let Some(c) = xcols.iter().find(|c| gcols.iter().any(|g| g.name == c.name)) {
        return Err(mapping(format!("'{}' is used for both X and Group by; use it in one place only", c.name)));
    }
    // A column-like graph has no data sets: Group by columns become extra parts of each bar.
    let (xcols, gcols) = if kind.is_column_like() && !gcols.is_empty() {
        (xcols.into_iter().chain(gcols).collect::<Vec<_>>(), Vec::new())
    } else {
        (xcols, gcols)
    };

    // Row filters.
    let mut keep = vec![true; table.nrows];
    for f in spec.filters.iter().filter(|f| !f.keeps_everything()) {
        let col = table.require_column(&f.column).map_err(|e| mapping(e.to_string()))?;
        for (r, k) in keep.iter_mut().enumerate() {
            *k &= f.keeps(col, r);
        }
    }
    let rows: Vec<usize> = (0..table.nrows).filter(|&r| keep[r]).collect();

    let y_single = ycols.len() == 1;
    let default_y_name = if y_single { ycols[0].name.clone() } else { "Value".to_string() };
    let gcols_for_color = gcols.clone();
    let group = (!gcols.is_empty()).then(|| Factor::new(gcols, &rows));

    // Data sets from multiple Y columns and/or the group factor.
    let mut sets: Vec<String> = Vec::new();
    let mut set_parts: Vec<Vec<String>> = Vec::new();
    let mut set_columns: Vec<String> = Vec::new();
    let mut set_name: Option<String> = None;
    match (&group, y_single) {
        (Some(g), true) => {
            sets = (0..g.len()).map(|i| g.label(i)).collect();
            set_parts = (0..g.len()).map(|i| g.parts(i)).collect();
            set_columns = g.column_names();
            set_name = Some(g.name());
        }
        (Some(g), false) => {
            for y in &ycols {
                for i in 0..g.len() {
                    let mut parts = vec![y.name.clone()];
                    parts.extend(g.parts(i));
                    sets.push(parts.join(COMBINE_SEP));
                    set_parts.push(parts);
                }
            }
            set_columns = std::iter::once("Variable".to_string()).chain(g.column_names()).collect();
            set_name = Some(format!("Variable{COMBINE_SEP}{}", g.name()));
        }
        (None, true) => {
            sets = vec![ycols[0].name.clone()];
            set_parts = vec![Vec::new()];
        }
        (None, false) => {
            sets = ycols.iter().map(|c| c.name.clone()).collect();
            set_parts = ycols.iter().map(|c| vec![c.name.clone()]).collect();
            set_columns = vec!["Variable".to_string()];
            set_name = Some("Variable".to_string());
        }
    }
    let set_of = |yi: usize, row: usize| -> Option<usize> {
        match &group {
            Some(g) => {
                let gi = g.index(row)?;
                Some(if y_single { gi } else { yi * g.len() + gi })
            }
            None => Some(if y_single { 0 } else { yi }),
        }
    };

    let mut x_levels: Vec<String> = Vec::new();
    let mut x_parts: Vec<Vec<String>> = Vec::new();
    let mut x_columns: Vec<String> = xcols.iter().map(|c| c.name.clone()).collect();
    let mut x_name = x_columns.join(COMBINE_SEP);
    let mut points: Vec<PlotPoint> = Vec::new();
    let mut point_labels: Vec<String> = Vec::new();

    match kind {
        GraphKind::XyPoints | GraphKind::XyPointsLine | GraphKind::XyLine | GraphKind::XyScatter => {
            let numeric = names_of(table, Some(ColumnKind::Numeric));
            let x = match xcols.as_slice() {
                [x] => *x,
                [] => return Err(mapping(format!("{} needs a numeric X column (numeric columns: {numeric})", kind.label()))),
                _ => return Err(mapping(format!("{} takes exactly one numeric X column (numeric columns: {numeric})", kind.label()))),
            };
            if x.kind() != ColumnKind::Numeric {
                return Err(mapping(format!(
                    "{} needs a numeric X column, but '{}' is {}. Use a column bar or grouped graph for categorical X (numeric columns: {numeric})",
                    kind.label(),
                    x.name,
                    x.kind().label().to_lowercase(),
                )));
            }
            for (yi, yc) in ycols.iter().enumerate() {
                for &r in &rows {
                    let (Some(xv), Some(yv), Some(set)) = (x.number(r), yc.number(r), set_of(yi, r)) else { continue };
                    points.push(PlotPoint { row: r, x: XValue::Num(xv), set, y: yv, replicate: 0 });
                }
            }
        }
        GraphKind::Survival => {
            // One row per subject: X = time, Y = event (1) or censored (0).
            let numeric = names_of(table, Some(ColumnKind::Numeric));
            let time = match xcols.as_slice() {
                [x] if x.kind() == ColumnKind::Numeric => *x,
                _ => return Err(mapping(format!("a survival graph needs one numeric time column as X (numeric columns: {numeric})"))),
            };
            if !y_single {
                return Err(mapping("a survival graph takes one event column as Y (1 = event, 0 = censored); use Group by for several curves"));
            }
            let event = ycols[0];
            for &r in &rows {
                let (Some(t), Some(e), Some(set)) = (time.number(r), event.number(r), set_of(0, r)) else { continue };
                if e != 0.0 && e != 1.0 {
                    return Err(mapping(format!(
                        "the event column '{}' must hold 1 (event, e.g. death) or 0 (censored); row {} has {}",
                        event.name,
                        r + 1,
                        otto_core::table::format_number(e)
                    )));
                }
                if t < 0.0 {
                    return Err(mapping(format!("the time column '{}' has a negative time in row {}", time.name, r + 1)));
                }
                points.push(PlotPoint { row: r, x: XValue::Num(t), set, y: e, replicate: 0 });
            }
        }
        GraphKind::ColumnBar | GraphKind::Box | GraphKind::Violin | GraphKind::Pie | GraphKind::Donut => {
            sets = vec![default_y_name.clone()];
            set_parts = vec![Vec::new()];
            set_columns.clear();
            set_name = None;
            if xcols.is_empty() {
                // Each Y column is one bar, like a Prism column table.
                x_levels = ycols.iter().map(|c| c.name.clone()).collect();
                x_parts = ycols.iter().map(|c| vec![c.name.clone()]).collect();
                x_columns = vec!["Variable".to_string()];
                x_name = "Variable".to_string();
                for (yi, yc) in ycols.iter().enumerate() {
                    for &r in &rows {
                        if let Some(yv) = yc.number(r) {
                            points.push(PlotPoint { row: r, x: XValue::Cat(yi), set: 0, y: yv, replicate: 0 });
                        }
                    }
                }
            } else {
                if !y_single {
                    return Err(mapping(format!(
                        "{} with X columns takes one Y column; clear X to show one per Y column, or use a grouped graph",
                        kind.label()
                    )));
                }
                let xf = Factor::new(xcols.clone(), &rows);
                x_levels = (0..xf.len()).map(|i| xf.label(i)).collect();
                x_parts = (0..xf.len()).map(|i| xf.parts(i)).collect();
                for &r in &rows {
                    let (Some(xi), Some(yv)) = (xf.index(r), ycols[0].number(r)) else { continue };
                    points.push(PlotPoint { row: r, x: XValue::Cat(xi), set: 0, y: yv, replicate: 0 });
                }
            }
        }
        GraphKind::Volcano => {
            let numeric = names_of(table, Some(ColumnKind::Numeric));
            let x = match xcols.as_slice() {
                [x] if x.kind() == ColumnKind::Numeric => *x,
                _ => return Err(mapping(format!("a volcano plot needs the log2 fold change as its numeric X column (numeric columns: {numeric})"))),
            };
            if !y_single {
                return Err(mapping("a volcano plot takes one Y column: the P value (or -log10 P)"));
            }
            let o = &spec.volcano;
            let p_of = |v: f64| if o.y_is_log { 10f64.powf(-v) } else { v };
            // Fixed data sets, so a point keeps its colour whatever the cutoffs are.
            sets = vec!["Down".into(), "Not significant".into(), "Up".into()];
            set_parts = sets.iter().map(|s| vec![s.clone()]).collect();
            set_columns = vec!["Change".into()];
            set_name = Some("Change".into());
            let labels = o.label.as_deref().map(|n| table.require_column(n).map_err(|e| mapping(e.to_string()))).transpose()?;
            let mut rows_out: Vec<(usize, f64, f64, usize)> = Vec::new();
            for &r in &rows {
                let (Some(fc), Some(raw)) = (x.number(r), ycols[0].number(r)) else { continue };
                let p = p_of(raw);
                let y = if o.y_is_log { raw } else if p > 0.0 { -p.log10() } else { f64::INFINITY };
                if !y.is_finite() {
                    continue;
                }
                let set = if p < o.p_cutoff && fc >= o.fc_cutoff {
                    2
                } else if p < o.p_cutoff && fc <= -o.fc_cutoff {
                    0
                } else {
                    1
                };
                rows_out.push((r, fc, y, set));
            }
            // Label the most significant changed points.
            let mut ranked: Vec<usize> = (0..rows_out.len()).filter(|&i| rows_out[i].3 != 1).collect();
            ranked.sort_by(|&a, &b| rows_out[b].2.total_cmp(&rows_out[a].2));
            ranked.truncate(o.label_top);
            for (i, &(r, fc, y, set)) in rows_out.iter().enumerate() {
                points.push(PlotPoint { row: r, x: XValue::Num(fc), set, y, replicate: 0 });
                point_labels.push(match (&labels, ranked.contains(&i)) {
                    (Some(c), true) => level_label(c, r).unwrap_or_default(),
                    _ => String::new(),
                });
            }
            x_columns = vec![x.name.clone()];
            x_name = x.name.clone();
        }
        GraphKind::Qq => {
            if !xcols.is_empty() {
                return Err(mapping("a QQ plot takes no X column: X is the theoretical normal quantile Otto computes"));
            }
            // Y values per data set, sorted, against Blom's theoretical normal quantiles.
            let mut by_set: Vec<Vec<(usize, f64)>> = vec![Vec::new(); sets.len().max(1)];
            for (yi, yc) in ycols.iter().enumerate() {
                for &r in &rows {
                    let (Some(yv), Some(set)) = (yc.number(r), set_of(yi, r)) else { continue };
                    by_set[set].push((r, yv));
                }
            }
            for (set, vals) in by_set.iter_mut().enumerate() {
                vals.sort_by(|a, b| a.1.total_cmp(&b.1));
                let n = vals.len();
                for (i, &(r, v)) in vals.iter().enumerate() {
                    let p = (i as f64 + 1.0 - 0.375) / (n as f64 + 0.25);
                    points.push(PlotPoint { row: r, x: XValue::Num(normal_quantile(p)), set, y: v, replicate: 0 });
                }
            }
            x_columns = vec!["Theoretical quantile".into()];
            x_name = "Theoretical quantile".into();
        }
        GraphKind::GroupedInterleaved
        | GraphKind::GroupedSeparated
        | GraphKind::Heatmap
        | GraphKind::StackedBar
        | GraphKind::StackedPercent
        | GraphKind::Contingency => {
            if xcols.is_empty() {
                return Err(mapping(format!(
                    "{} graphs need at least one categorical X column (categorical columns: {})",
                    match kind {
                        GraphKind::Heatmap => "heatmap",
                        GraphKind::Contingency => "contingency",
                        k if k.is_stacked() => "stacked bar",
                        _ => "grouped",
                    },
                    names_of(table, Some(ColumnKind::Categorical))
                )));
            }
            let xf = Factor::new(xcols.clone(), &rows);
            x_levels = (0..xf.len()).map(|i| xf.label(i)).collect();
            x_parts = (0..xf.len()).map(|i| xf.parts(i)).collect();
            for (yi, yc) in ycols.iter().enumerate() {
                for &r in &rows {
                    let (Some(xi), Some(yv), Some(set)) = (xf.index(r), yc.number(r), set_of(yi, r)) else { continue };
                    points.push(PlotPoint { row: r, x: XValue::Cat(xi), set, y: yv, replicate: 0 });
                }
            }
        }
    }

    if points.is_empty() {
        return Err(PlotError::NoData(
            "no rows left to plot (check filters and missing values in the X/Y/Group columns)".into(),
        ));
    }

    // Keep only categories and data sets that have points, preserving order.
    compact(&mut x_levels, &mut x_parts, &mut points, |p| match &mut p.x {
        XValue::Cat(i) => Some(i),
        XValue::Num(_) => None,
    });
    compact(&mut sets, &mut set_parts, &mut points, |p| Some(&mut p.set));

    // Replicates: by the named column. A graph that draws one line per replicate needs
    // it, because the position of a value among the ones at its X says nothing about
    // which subject it belongs to: the moment one subject is missing a timepoint, every
    // later one moves up a place and the lines jump from subject to subject. Graphs that
    // only draw points (or bars) claim no such identity, so there the k-th value at each
    // X is merely the k-th replicate subcolumn of the exported table.
    let mut replicates: Vec<String> = Vec::new();
    let replicate_name = if kind.is_xy() { spec.replicate_by.clone().filter(|c| !c.trim().is_empty()) } else { None };
    if replicate_name.is_none() && replicate_lines(kind, spec.summary) {
        return Err(mapping(format!(
            "one line per replicate needs the column that says which subject a value belongs to \
             (e.g. Mouse): set \"Replicate (one line each)\" under Data on table, or show the mean \
             instead of each replicate (columns: {})",
            names_of(table, None)
        )));
    }
    match &replicate_name {
        Some(name) => {
            let col = table.require_column(name).map_err(|e| mapping(format!("replicate column: {e}")))?;
            let f = Factor::new(vec![col], &rows);
            replicates = (0..f.len()).map(|i| f.label(i)).collect();
            for p in points.iter_mut() {
                p.replicate = f.index(p.row).unwrap_or(usize::MAX);
            }
        }
        None => {
            let mut seen: HashMap<(usize, u64), usize> = HashMap::new();
            for p in points.iter_mut() {
                let k = seen.entry((p.set, p.x.num().to_bits())).or_insert(0);
                p.replicate = *k;
                *k += 1;
            }
        }
    }

    // Palette slots from the table's own level order, computed over *all* rows: filtering
    // rows out, or splitting a figure into panels, then never moves a group's colour.
    let all_rows: Vec<usize> = (0..table.nrows).collect();
    let group_all = (!gcols_for_color.is_empty()).then(|| Factor::new(gcols_for_color.clone(), &all_rows));
    let mut set_slot: HashMap<String, usize> = HashMap::new();
    match (&group_all, y_single) {
        (Some(g), true) => {
            for i in 0..g.len() {
                set_slot.insert(g.label(i), i);
            }
        }
        (Some(g), false) => {
            for (yi, y) in spec.y.iter().enumerate() {
                for i in 0..g.len() {
                    set_slot.insert(format!("{y}{COMBINE_SEP}{}", g.label(i)), yi * g.len() + i);
                }
            }
        }
        (None, false) => {
            for (yi, y) in spec.y.iter().enumerate() {
                set_slot.insert(y.clone(), yi);
            }
        }
        (None, true) => {}
    }
    if kind == GraphKind::Volcano {
        set_slot = [("Down".to_string(), 0), ("Not significant".to_string(), 1), ("Up".to_string(), 2)].into_iter().collect();
    }
    let set_colors: Vec<usize> = sets.iter().enumerate().map(|(i, s)| set_slot.get(s).copied().unwrap_or(i)).collect();
    let x_slot: HashMap<String, usize> = if kind.is_column_like() && !xcols.is_empty() {
        let f = Factor::new(xcols.clone(), &all_rows);
        (0..f.len()).map(|i| (f.label(i), i)).collect()
    } else if kind.is_column_like() {
        spec.y.iter().enumerate().map(|(i, y)| (y.clone(), i)).collect()
    } else {
        HashMap::new()
    };
    let mut x_colors: Vec<usize> = x_levels.iter().enumerate().map(|(i, l)| x_slot.get(l).copied().unwrap_or(i)).collect();

    // Data sets in focus take the palette's first slots (red, then blue); in highlight mode
    // everything else turns gray. Volcano points and heatmap cells are colored by value, so
    // focus leaves them alone.
    let mut set_colors = set_colors;
    if !matches!(kind, GraphKind::Volcano | GraphKind::Heatmap) {
        focus_slots(&sets, &mut set_colors, spec);
        focus_slots(&x_levels, &mut x_colors, spec);
    }

    // Survival curves, volcano points and QQ points have no means or error bars.
    let groups = if matches!(kind, GraphKind::Survival | GraphKind::Volcano | GraphKind::Qq) {
        Vec::new()
    } else {
        summarize(&points, spec.summary)
    };
    let x_title = spec.x_axis.title.clone().unwrap_or_else(|| match (&set_name, kind) {
        (_, k) if k.is_column_like() && xcols.is_empty() => String::new(),
        // Separated bars are labelled "X | data set", so the title says so too.
        (Some(s), GraphKind::GroupedSeparated) if sets.len() > 1 => format!("{x_name}{COMBINE_SEP}{s}"),
        _ => x_name.clone(),
    });
    let y_title = spec.y_axis.title.clone().unwrap_or_else(|| {
        if kind == GraphKind::Survival {
            "Percent survival".to_string()
        } else if kind == GraphKind::StackedPercent {
            "Percent of total".to_string()
        } else if kind == GraphKind::Volcano {
            "-log10 P".to_string()
        } else if y_single || ycols.len() > 3 {
            default_y_name.clone()
        } else {
            ycols.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", ")
        }
    });

    Ok(PlotData {
        kind,
        summary: spec.summary,
        x_title,
        y_title,
        x_name,
        x_columns,
        y_name: default_y_name,
        set_name,
        set_columns,
        x_levels,
        x_parts,
        sets,
        set_colors,
        x_colors,
        point_labels,
        set_parts,
        replicate_name,
        replicates,
        points,
        groups,
    })
}

/// Removes unused entries of `names` (and their `parts`) and renumbers the indices returned by `index`.
fn compact(
    names: &mut Vec<String>,
    parts: &mut Vec<Vec<String>>,
    points: &mut [PlotPoint],
    index: impl Fn(&mut PlotPoint) -> Option<&mut usize>,
) {
    if names.is_empty() {
        return;
    }
    let mut used = vec![false; names.len()];
    for p in points.iter_mut() {
        if let Some(i) = index(p) {
            used[*i] = true;
        }
    }
    let mut remap = vec![usize::MAX; names.len()];
    let mut next = 0;
    for (i, u) in used.iter().enumerate() {
        if *u {
            remap[i] = next;
            next += 1;
        }
    }
    for p in points.iter_mut() {
        if let Some(i) = index(p) {
            *i = remap[*i];
        }
    }
    let mut i = 0;
    names.retain(|_| {
        i += 1;
        used[i - 1]
    });
    if parts.len() == used.len() {
        let mut i = 0;
        parts.retain(|_| {
            i += 1;
            used[i - 1]
        });
    }
}

/// A hashable stand-in for an [`XValue`], so points can be bucketed by it.
pub type GroupKey = (u8, u64);

/// `None` for an X that is not a number: comparing such an X for equality never matched,
/// so it formed no group, and it still doesn't. `-0.0` folds onto `0.0`, which it equals.
pub fn group_key(x: XValue) -> Option<GroupKey> {
    match x {
        XValue::Num(v) => v.is_finite().then(|| (0, if v == 0.0 { 0.0f64 } else { v }.to_bits())),
        XValue::Cat(i) => Some((1, i as u64)),
    }
}

/// One summary per (X position, data set), ordered by data set then X.
fn summarize(points: &[PlotPoint], summary: Summary) -> Vec<GroupStats> {
    // Grouped in one pass. Finding each group by scanning the groups so far and then
    // collecting its values by scanning every point was quadratic as soon as the X values
    // were mostly distinct, which is every XY graph of any size.
    let mut keys: Vec<(usize, XValue)> = Vec::new();
    let mut values: Vec<Vec<f64>> = Vec::new();
    let mut at: HashMap<(usize, GroupKey), usize> = HashMap::new();
    for p in points {
        let Some(key) = group_key(p.x) else { continue };
        let i = *at.entry((p.set, key)).or_insert_with(|| {
            keys.push((p.set, p.x));
            values.push(Vec::new());
            keys.len() - 1
        });
        values[i].push(p.y);
    }
    let mut order: Vec<usize> = (0..keys.len()).collect();
    order.sort_by(|&a, &b| keys[a].0.cmp(&keys[b].0).then(keys[a].1.num().total_cmp(&keys[b].1.num())));
    order
        .into_iter()
        .filter_map(|i| {
            let (set, x) = keys[i];
            let stats = describe(&values[i])?;
            let center = match summary.center {
                Center::Mean => stats.mean,
                Center::Median => stats.median,
            };
            let (low, high) = match summary.error {
                ErrorBars::Sd => (center - stats.sd, center + stats.sd),
                ErrorBars::Sem => (center - stats.sem, center + stats.sem),
                ErrorBars::Ci95 => (stats.ci95_low, stats.ci95_high),
                ErrorBars::Range => (stats.min, stats.max),
                ErrorBars::None => (center, center),
            };
            let (low, high) = if low.is_finite() && high.is_finite() { (low, high) } else { (center, center) };
            Some(GroupStats { x, set, stats, center, low, high })
        })
        .collect()
}

/// Quantile of the standard normal distribution (Acklam's rational approximation,
/// accurate to about 1e-9 — plenty for drawing a QQ plot).
pub fn normal_quantile(p: f64) -> f64 {
    use statrs::distribution::{ContinuousCDF, Normal};
    match Normal::new(0.0, 1.0) {
        Ok(n) => n.inverse_cdf(p.clamp(1e-12, 1.0 - 1e-12)),
        Err(_) => f64::NAN,
    }
}

/// Moves the graph's focused data sets to the palette's first slots, in the order they were
/// picked, and pushes the rest behind them (or grays them out in highlight mode). A graph
/// with nothing in focus keeps its colors, highlight mode or not: there is nothing to
/// highlight yet.
fn focus_slots(names: &[String], slots: &mut [usize], spec: &GraphSpec) {
    let focus: Vec<&String> = spec.focus.iter().filter(|f| names.contains(f)).collect();
    if focus.is_empty() {
        return;
    }
    // The unfocused sets keep their order among themselves, behind the focused ones.
    let mut rest: Vec<usize> = (0..names.len()).filter(|&i| !focus.contains(&&names[i])).collect();
    rest.sort_by_key(|&i| slots[i]);
    for (rank, i) in rest.into_iter().enumerate() {
        slots[i] = if spec.highlight { crate::theme::MUTED_SLOT } else { focus.len() + rank };
    }
    for (rank, name) in focus.iter().enumerate() {
        if let Some(i) = names.iter().position(|n| &n == name) {
            slots[i] = rank;
        }
    }
}
