//! Exports the distilled plot data (exactly what is drawn) as long, wide or summary tables.

use otto_core::table::format_number;
use otto_core::DataLayout;

use std::collections::HashMap;

use crate::distill::{group_key, PlotData, XValue};

/// A rectangular text table with a header row.
#[derive(Clone, Debug, PartialEq)]
pub struct TextTable {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl TextTable {
    pub fn to_delimited(&self, delimiter: u8) -> String {
        let mut w = csv::WriterBuilder::new().delimiter(delimiter).flexible(true).from_writer(Vec::new());
        let _ = w.write_record(&self.headers);
        for r in &self.rows {
            let _ = w.write_record(r);
        }
        String::from_utf8(w.into_inner().unwrap_or_default()).unwrap_or_default()
    }

    pub fn to_csv(&self) -> String {
        self.to_delimited(b',')
    }

    /// Tab-separated: pastes straight into Excel or Prism cells.
    pub fn to_tsv(&self) -> String {
        self.to_delimited(b'\t')
    }
}

fn num(v: f64) -> String {
    if v.is_finite() {
        format_number((v * 1e10).round() / 1e10)
    } else {
        String::new()
    }
}

pub fn table(data: &PlotData, layout: DataLayout) -> TextTable {
    match layout {
        DataLayout::Long => long(data),
        DataLayout::Wide => wide(data),
        DataLayout::Summary => summary(data),
    }
}

/// Header and per-point values that identify X: numeric X is one column; a categorical
/// X has one column per combined factor column.
fn x_headers(data: &PlotData) -> Vec<String> {
    if data.kind.is_xy() || data.x_columns.is_empty() {
        vec![if data.x_name.is_empty() { "X".into() } else { data.x_name.clone() }]
    } else {
        data.x_columns.clone()
    }
}

fn x_values(data: &PlotData, x: XValue) -> Vec<String> {
    match x {
        XValue::Cat(i) if !data.x_columns.is_empty() && data.x_parts.len() > i => data.x_parts[i].clone(),
        XValue::Num(v) => vec![num(v)],
        other => vec![data.x_label(other)],
    }
}

/// Whether a graph draws one line per replicate (so exports say which replicate a value is).
pub fn replicate_lines(data: &PlotData) -> bool {
    crate::distill::replicate_lines(data.kind, data.summary)
}

/// Which subcolumn of its data set's block each replicate owns: one per replicate the set
/// has, in replicate order. A value missing at some X then leaves that subcolumn blank
/// instead of letting the values below it move up a column, so a subcolumn stays one
/// subject from the first row to the last - what a Prism XY table does with its
/// replicate subcolumns.
struct RepSlots {
    /// Per data set: replicate index -> its first subcolumn.
    slot: Vec<HashMap<usize, usize>>,
    /// Per data set: the subcolumn titles, e.g. "Mouse 2".
    labels: Vec<Vec<String>>,
}

impl RepSlots {
    /// `None` unless the replicates are a column of the table: without one there is no
    /// identity to keep, and the k-th value at each X is all a subcolumn ever means.
    fn of(data: &PlotData) -> Option<RepSlots> {
        data.replicate_name.as_ref()?;
        // How many values a replicate has at the X where it has the most: measured twice
        // on one day, a subject owns two subcolumns, so that neither value is dropped.
        let mut counts: HashMap<(usize, usize, u64), usize> = HashMap::new();
        for p in &data.points {
            *counts.entry((p.set, p.replicate, p.x.num().to_bits())).or_insert(0) += 1;
        }
        let mut most: HashMap<(usize, usize), usize> = HashMap::new();
        for ((set, rep, _), n) in counts {
            let m = most.entry((set, rep)).or_insert(0);
            *m = (*m).max(n);
        }
        let mut per_set: Vec<Vec<usize>> = vec![Vec::new(); data.sets.len()];
        for p in &data.points {
            if let Some(reps) = per_set.get_mut(p.set) {
                reps.push(p.replicate);
            }
        }
        let mut slots = RepSlots { slot: Vec::new(), labels: Vec::new() };
        for (si, reps) in per_set.iter_mut().enumerate() {
            // usize::MAX (the replicate column is empty on that row) sorts last, into
            // subcolumns of its own: those values are in the data, they are simply not on
            // any subject's line.
            reps.sort_unstable();
            reps.dedup();
            let mut slot = HashMap::new();
            let mut labels = Vec::new();
            for &r in reps.iter() {
                slot.insert(r, labels.len());
                let name = data.replicates.get(r).cloned().unwrap_or_else(|| "(none)".into());
                let n = most.get(&(si, r)).copied().unwrap_or(1);
                labels.push(name.clone());
                labels.extend((2..=n).map(|k| format!("{name} ({k})")));
            }
            slots.slot.push(slot);
            slots.labels.push(labels);
        }
        Some(slots)
    }

    fn width(&self) -> usize {
        self.labels.iter().map(Vec::len).max().unwrap_or(0)
    }
}

fn long(data: &PlotData) -> TextTable {
    let with_rep = replicate_lines(data);
    let mut headers = vec!["Row".to_string()];
    headers.extend(x_headers(data));
    headers.extend(data.set_columns.iter().cloned());
    if with_rep {
        headers.push(data.replicate_name.clone().unwrap_or_else(|| "Replicate".into()));
    }
    headers.push(data.y_name.clone());
    let rows = data
        .points
        .iter()
        .map(|p| {
            let mut r = vec![(p.row + 1).to_string()];
            r.extend(x_values(data, p.x));
            r.extend(data.set_parts.get(p.set).cloned().unwrap_or_default());
            if with_rep {
                r.push(match &data.replicate_name {
                    Some(_) => data.replicates.get(p.replicate).cloned().unwrap_or_default(),
                    None => (p.replicate + 1).to_string(),
                });
            }
            r.push(num(p.y));
            r
        })
        .collect();
    TextTable { headers, rows }
}

/// Header cells for blocks of replicate subcolumns: the block's title over its first
/// subcolumn (the rest blank), and "replicate 1…n" underneath, as in a Prism table.
fn replicate_headers(titles: &[String], reps: usize) -> (Vec<String>, Vec<String>) {
    let mut top = Vec::new();
    let mut sub = Vec::new();
    for t in titles {
        for k in 0..reps {
            top.push(if k == 0 { t.clone() } else { String::new() });
            sub.push(format!("replicate {}", k + 1));
        }
    }
    (top, sub)
}

/// The same, with each subcolumn named after the subject it holds (e.g. "Mouse 2") and
/// padded to `reps` where a data set has fewer of them.
fn named_replicate_headers(titles: &[String], reps: usize, labels: &[Vec<String>]) -> (Vec<String>, Vec<String>) {
    let mut top = Vec::new();
    let mut sub = Vec::new();
    for (si, t) in titles.iter().enumerate() {
        let names = labels.get(si).map(Vec::as_slice).unwrap_or_default();
        for k in 0..reps {
            top.push(if k == 0 { t.clone() } else { String::new() });
            sub.push(names.get(k).cloned().unwrap_or_default());
        }
    }
    (top, sub)
}

/// Prism layout: blocks of side-by-side replicate subcolumns, so the table pastes into
/// Prism as is. Every block has as many subcolumns as the largest group (Prism uses one
/// replicate count per table); missing values are blank. The first row of `rows` is the
/// "replicate n" subheader.
/// - Bar graphs with one data set: a block per bar, one row of values.
/// - Bar graphs with several data sets (several Y columns or Group by): a block per X
///   category, one row per data set, titled in the first column.
/// - XY graphs: an X column, a block per data set, one row per X value.
fn wide(data: &PlotData) -> TextTable {
    if data.kind == otto_core::GraphKind::Heatmap {
        // The matrix as drawn: a row per data set, a column per X category, cell = mean
        // (or median) of its values, before any normalisation.
        let m = crate::heatmap::cells(data, &otto_core::HeatmapOptions::default());
        let corner = data.set_name.clone().unwrap_or_default();
        let rows = data
            .sets
            .iter()
            .zip(m)
            .map(|(s, row)| std::iter::once(s.clone()).chain(row.into_iter().map(|v| v.map(num).unwrap_or_default())).collect())
            .collect();
        return TextTable { headers: std::iter::once(corner).chain(data.x_levels.iter().cloned()).collect(), rows };
    }
    if data.kind == otto_core::GraphKind::Survival {
        // Prism's survival table: one row per subject, its time in X and 1 / 0 in its group's column.
        let x_title = if data.x_name.is_empty() { "Time".to_string() } else { data.x_name.clone() };
        let mut pts: Vec<_> = data.points.iter().collect();
        pts.sort_by(|a, b| (a.set, a.x.num()).partial_cmp(&(b.set, b.x.num())).unwrap_or(std::cmp::Ordering::Equal));
        let rows = pts
            .into_iter()
            .map(|p| {
                let mut r = vec![num(p.x.num())];
                r.extend((0..data.sets.len()).map(|si| if si == p.set { num(p.y) } else { String::new() }));
                r
            })
            .collect();
        return TextTable { headers: std::iter::once(x_title).chain(data.sets.iter().cloned()).collect(), rows };
    }
    if data.kind.is_xy() {
        let mut xs: Vec<f64> = data.points.iter().map(|p| p.x.num()).collect();
        xs.sort_by(f64::total_cmp);
        xs.dedup();
        let slots = RepSlots::of(data);
        let cells = cells(data, slots.as_ref());
        let cell = |si: usize, x: f64| -> Vec<String> { cell_of(&cells, si, XValue::Num(x)) };
        let widest = (0..data.sets.len()).flat_map(|si| xs.iter().map(move |&x| (si, x))).map(|(si, x)| cell(si, x).len()).max().unwrap_or(0);
        let reps = widest.max(slots.as_ref().map_or(0, RepSlots::width)).max(1);
        let (top, sub) = match &slots {
            Some(s) => named_replicate_headers(&data.sets, reps, &s.labels),
            None => replicate_headers(&data.sets, reps),
        };
        let x_title = if data.x_name.is_empty() { "X".to_string() } else { data.x_name.clone() };
        let mut rows = vec![std::iter::once(String::new()).chain(sub).collect::<Vec<_>>()];
        for &x in &xs {
            let mut r = vec![num(x)];
            for si in 0..data.sets.len() {
                let mut vals = cell(si, x);
                vals.resize(reps, String::new());
                r.extend(vals);
            }
            rows.push(r);
        }
        return TextTable { headers: std::iter::once(x_title).chain(top).collect(), rows };
    }
    if data.sets.len() > 1 {
        let cells = cells(data, None);
        let values = |xi: usize, si: usize| -> Vec<String> { cell_of(&cells, si, XValue::Cat(xi)) };
        let nx = data.x_levels.len();
        let reps = (0..nx).flat_map(|xi| (0..data.sets.len()).map(move |si| (xi, si))).map(|(xi, si)| values(xi, si).len()).max().unwrap_or(0).max(1);
        let titles: Vec<String> = (0..nx).map(|xi| data.x_label(XValue::Cat(xi))).collect();
        let (top, sub) = replicate_headers(&titles, reps);
        let mut rows = vec![std::iter::once(String::new()).chain(sub).collect::<Vec<_>>()];
        for (si, set) in data.sets.iter().enumerate() {
            let mut r = vec![set.clone()];
            for xi in 0..nx {
                let mut v = values(xi, si);
                v.resize(reps, String::new());
                r.extend(v);
            }
            rows.push(r);
        }
        return TextTable { headers: std::iter::once(String::new()).chain(top).collect(), rows };
    }
    let cells = cells(data, None);
    let bars: Vec<(String, Vec<String>)> = data
        .bar_order()
        .into_iter()
        .map(|i| {
            let g = &data.groups[i];
            (data.group_label(g), cell_of(&cells, g.set, g.x))
        })
        .collect();
    let reps = bars.iter().map(|(_, v)| v.len()).max().unwrap_or(0).max(1);
    let titles: Vec<String> = bars.iter().map(|(t, _)| t.clone()).collect();
    let (headers, sub) = replicate_headers(&titles, reps);
    let values = bars
        .into_iter()
        .flat_map(|(_, mut v)| {
            v.resize(reps, String::new());
            v
        })
        .collect();
    TextTable { headers, rows: vec![sub, values] }
}

/// The plotted Y values of every (data set, X) cell: at each replicate's own subcolumn
/// when `slots` says which one that is, otherwise in the order the points come in.
/// Each wide layout used to look a cell up by filtering all the points, once to find the
/// widest cell and again for every row: O(data sets × distinct X × points), twice over.
/// One pass builds them all.
fn cells(data: &PlotData, slots: Option<&RepSlots>) -> HashMap<(usize, crate::distill::GroupKey), Vec<String>> {
    let mut out: HashMap<(usize, crate::distill::GroupKey), Vec<String>> = HashMap::new();
    // How many values of this replicate already sit in this cell, for the rare subject
    // measured more than once at the same X.
    let mut taken: HashMap<(usize, crate::distill::GroupKey, usize), usize> = HashMap::new();
    for p in &data.points {
        let Some(key) = group_key(p.x) else { continue };
        let at = slots.and_then(|s| s.slot.get(p.set)).and_then(|m| m.get(&p.replicate)).map(|&base| {
            let k = taken.entry((p.set, key, p.replicate)).or_insert(0);
            *k += 1;
            base + *k - 1
        });
        let cell = out.entry((p.set, key)).or_default();
        match at {
            Some(i) => {
                if cell.len() <= i {
                    cell.resize(i + 1, String::new());
                }
                cell[i] = num(p.y);
            }
            None => cell.push(num(p.y)),
        }
    }
    out
}

fn cell_of(cells: &HashMap<(usize, crate::distill::GroupKey), Vec<String>>, set: usize, x: XValue) -> Vec<String> {
    group_key(x).and_then(|k| cells.get(&(set, k))).cloned().unwrap_or_default()
}

fn summary(data: &PlotData) -> TextTable {
    let mut headers = x_headers(data);
    headers.extend(data.set_columns.iter().cloned());
    headers.extend(
        ["N", "Mean", "SD", "SEM", "Median", "Q1", "Q3", "Min", "Max", "Lower 95% CI", "Upper 95% CI", "Plotted center", "Error low", "Error high"]
            .map(String::from),
    );
    let rows = data
        .groups
        .iter()
        .map(|g| {
            let s = &g.stats;
            let mut r = x_values(data, g.x);
            r.extend(data.set_parts.get(g.set).cloned().unwrap_or_default());
            r.push(s.n.to_string());
            r.extend([s.mean, s.sd, s.sem, s.median, s.q1, s.q3, s.min, s.max, s.ci95_low, s.ci95_high, g.center, g.low, g.high].map(num));
            r
        })
        .collect();
    TextTable { headers, rows }
}
