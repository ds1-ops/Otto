//! t tests and one-way ANOVA on the bars of a graph: gathers the data sets, has R run the
//! tests and lays the results out like Prism's results sheets.

use std::collections::HashMap;

use serde_json::json;

use otto_core::analysis::{
    format_sig, AnovaSpec, Comparisons, Correction, DiffDirection, Distribution, GraphBasis, Matching, NormalityTests, Pairing, PStyle,
    TTestSpec,
};
use otto_core::{AnalysisKind, AnalysisSpec, DataShape, DataTable, GraphSpec, Project};
use otto_plot::describe::{describe, percentile_sorted};
use otto_plot::{Bracket, TextTable};
use otto_r::results as rr;
use otto_r::Request;

use crate::results::{Report, Section};
use crate::EngineError;

/// One data set: a bar of a column graph, or a data set (legend entry) of an XY graph.
#[derive(Clone, Debug)]
pub struct DataSet {
    pub label: String,
    pub values: Vec<f64>,
    /// Source table rows of the values (for matching).
    pub rows: Vec<usize>,
    /// X values (XY data sets only).
    pub x: Vec<f64>,
}

/// Runs a request in R and reads the result.
pub(crate) fn r_call<T: serde::de::DeserializeOwned>(req: &Request) -> Result<(T, String), EngineError> {
    otto_r::call_as(req).map_err(EngineError::from)
}

/// All data sets of a graph, in the order its bars appear.
pub fn graph_data_sets<'a>(project: &'a Project, graph_id: &str) -> Result<(Vec<DataSet>, &'a GraphSpec, &'a DataTable), EngineError> {
    let (g, table) = graph_and_table(project, graph_id)?;
    Ok((spec_data_sets(g, table)?, g, table))
}

pub(crate) fn graph_and_table<'a>(project: &'a Project, graph_id: &str) -> Result<(&'a GraphSpec, &'a DataTable), EngineError> {
    let g = project
        .graph(graph_id)
        .ok_or_else(|| EngineError::NotFound(format!("graph '{graph_id}' (the analysis uses its data sets)")))?;
    let table = project.table_by_id(&g.table_id).ok_or_else(|| EngineError::NotFound(format!("table of graph \"{}\"", g.name)))?;
    Ok((g, table))
}

/// The data sets an analysis of this shape uses: bars, XY data sets or survival curves.
pub(crate) fn analysed_sets(g: &GraphSpec, table: &DataTable, shape: DataShape) -> Result<Vec<DataSet>, EngineError> {
    match shape {
        DataShape::Bars => spec_data_sets(g, table),
        DataShape::Xy => crate::xy_analyses::xy_data_sets(g, table),
        DataShape::Survival => crate::survival::survival_sets(g, table),
        DataShape::Grid => Err(EngineError::Invalid(format!("\"{}\" is a heatmap; heatmaps have no statistics", g.name))),
        DataShape::Counts => Err(EngineError::Invalid(format!(
            "\"{}\" is a contingency table; its statistics are chi-square and Fisher's exact test (analysis_create_contingency), not comparisons of bars",
            g.name
        ))),
        DataShape::Slices => Err(EngineError::Invalid(format!(
            "\"{}\" is a {}; pie and donut charts have no statistics",
            g.name,
            g.kind.label().to_lowercase()
        ))),
    }
}

/// The bars of a graph spec drawn from `table`, in the order they appear.
fn spec_data_sets(g: &GraphSpec, table: &DataTable) -> Result<Vec<DataSet>, EngineError> {
    if g.kind.shape() != DataShape::Bars {
        return Err(EngineError::Invalid(format!(
            "\"{}\" is an XY graph; t tests and one-way ANOVA compare the bars of a column, box, violin or grouped graph",
            g.name
        )));
    }
    let data = otto_plot::distill::distill(g, table)?;
    Ok(data
        .bar_order()
        .into_iter()
        .map(|i| {
            let grp = &data.groups[i];
            let pts: Vec<_> = data.points_of(grp).collect();
            DataSet {
                label: data.group_label(grp),
                values: pts.iter().map(|p| p.y).collect(),
                rows: pts.iter().map(|p| p.row).collect(),
                x: Vec::new(),
            }
        })
        .collect())
}

/// Graph and data sets of an analysis of a graph.
fn analysis_target(kind: &AnalysisKind) -> Option<(&str, &[String])> {
    kind.graph_id().map(|g| (g, kind.data_sets()))
}

/// Why a t test / ANOVA no longer matches its graph, or `None` while it does.
///
/// The analysed data sets are compared as they come out of the graph now and as they
/// would with the choices it was set up on, both from the current table. Data edits
/// therefore keep the analysis current (it simply recalculates, as in Prism), and so do
/// graph changes that leave the analysed bars alone (e.g. the graph type, or a filter
/// that only removes other bars). A change that alters them makes it out of date.
pub fn out_of_date(project: &Project, a: &AnalysisSpec) -> Option<Vec<String>> {
    let basis = a.basis.as_ref()?;
    let (graph_id, wanted) = analysis_target(&a.kind)?;
    let g = project.graph(graph_id)?;
    let now = GraphBasis::of(g);
    if *basis == now {
        return None;
    }
    let mut reasons = basis.changes(&now);
    let shape = a.kind.shape()?;
    if g.kind.shape() != shape {
        reasons.push(format!("the graph is no longer {}", shape.graph_name()));
        return Some(reasons);
    }
    let then_g = basis.apply_to(g);
    let sets_then = project.table_by_id(&basis.table_id).and_then(|t| analysed_sets(&then_g, t, shape).ok());
    let sets_now = project.table_by_id(&g.table_id).and_then(|t| analysed_sets(g, t, shape).ok());
    let (Some(then), Some(now)) = (sets_then, sets_now) else {
        reasons.push("its data sets can no longer be drawn".into());
        return Some(reasons);
    };
    let labels: Vec<&str> = if wanted.is_empty() { then.iter().map(|d| d.label.as_str()).collect() } else { wanted.iter().map(String::as_str).collect() };
    // Compared field by field, and each side looked up once: cloning the rows and values
    // of every bar to compare them was the most expensive thing about this check.
    fn find<'a>(sets: &'a [DataSet], label: &str) -> Option<&'a DataSet> {
        sets.iter().find(|d| d.label == label)
    }
    let same = |label: &str| match (find(&then, label), find(&now, label)) {
        (Some(a), Some(b)) => a.rows == b.rows && a.values == b.values && a.x == b.x,
        _ => false,
    };
    // Using all bars: a bar that appears or disappears changes the analysis too.
    let same_bars = !wanted.is_empty() || then.iter().map(|d| &d.label).eq(now.iter().map(|d| &d.label));
    if same_bars && labels.iter().all(|l| same(l)) {
        return None;
    }
    let missing: Vec<&str> = labels.iter().copied().filter(|l| !now.iter().any(|d| d.label == *l)).collect();
    if !missing.is_empty() {
        reasons.push(format!("no longer on the graph: {}", missing.join(", ")));
    }
    Some(reasons)
}

/// Rebinds a t test / ANOVA to the bars the graph shows now ("recalculate with the data
/// shown"): ANOVA uses every bar; a t test uses the two bars shown, or keeps its pair if
/// the graph shows more and both are still there. Follow-up choices are kept where they
/// still apply.
pub fn rebind_to_graph(project: &Project, kind: &AnalysisKind) -> Result<AnalysisKind, EngineError> {
    let Some((graph_id, _)) = analysis_target(kind) else {
        return Err(EngineError::Invalid("only analyses of a graph can be recalculated with the data it shows".into()));
    };
    let (g, table) = graph_and_table(project, graph_id)?;
    let shape = kind.shape().unwrap_or(DataShape::Bars);
    if g.kind.shape() != shape {
        return Err(EngineError::Invalid(format!("\"{}\" is no longer {}", g.name, shape.graph_name())));
    }
    let sets = analysed_sets(g, table, shape)?;
    let labels: Vec<String> = sets.into_iter().map(|d| d.label).collect();
    if !matches!(kind, AnalysisKind::TTest(_) | AnalysisKind::OneWayAnova(_)) {
        if labels.is_empty() {
            return Err(EngineError::Invalid(format!("\"{}\" shows no data sets", g.name)));
        }
        // Keep the data sets that are still there; when none are, use all of them.
        let mut k = kind.clone();
        if let Some(ds) = k.data_sets_mut() {
            ds.retain(|d| labels.contains(d));
        }
        return Ok(k);
    }
    match kind {
        AnalysisKind::TTest(t) => {
            let mut t = t.clone();
            if labels.len() == 2 {
                t.data_sets = labels;
            } else if t.data_sets.len() == 2 && t.data_sets.iter().all(|d| labels.contains(d)) {
                // Keep the pair, in the graph's order.
                t.data_sets = labels.into_iter().filter(|l| t.data_sets.contains(l)).collect();
            } else {
                return Err(EngineError::Invalid(format!(
                    "a t test compares two bars, but \"{}\" shows {}; set the t test up again to choose two",
                    g.name,
                    labels.len()
                )));
            }
            Ok(AnalysisKind::TTest(t))
        }
        AnalysisKind::OneWayAnova(x) => {
            if labels.len() < 2 {
                return Err(EngineError::Invalid(format!("one-way ANOVA needs at least two bars, but \"{}\" shows {}", g.name, labels.len())));
            }
            let mut x = x.clone();
            x.comparisons = match &x.comparisons {
                Comparisons::VsControl { control } if labels.contains(control) => Comparisons::VsControl { control: control.clone() },
                Comparisons::VsControl { .. } => Comparisons::VsControl { control: labels[0].clone() },
                Comparisons::Selected { pairs } => {
                    let kept: Vec<[String; 2]> = pairs.iter().filter(|[a, b]| labels.contains(a) && labels.contains(b)).cloned().collect();
                    if kept.is_empty() {
                        Comparisons::AllPairs
                    } else {
                        Comparisons::Selected { pairs: kept }
                    }
                }
                other => other.clone(),
            };
            x.data_sets = labels;
            Ok(AnalysisKind::OneWayAnova(x))
        }
        _ => unreachable!("XY analyses are handled above; descriptive statistics have no graph"),
    }
}

/// The wanted data sets (all when `wanted` is empty), in the requested order.
pub(crate) fn pick(all: Vec<DataSet>, wanted: &[String]) -> Result<Vec<DataSet>, EngineError> {
    if wanted.is_empty() {
        return Ok(all);
    }
    wanted
        .iter()
        .map(|w| {
            all.iter().find(|d| d.label == *w || d.label.eq_ignore_ascii_case(w.trim())).cloned().ok_or_else(|| {
                EngineError::NotFound(format!(
                    "data set '{w}' (available: {})",
                    all.iter().map(|d| d.label.as_str()).collect::<Vec<_>>().join(", ")
                ))
            })
        })
        .collect()
}

/// Matches values across data sets by the `match_by` column: rows = subjects present in
/// every data set. Returns the matrix and how many subjects were left out.
fn matched(sets: &[DataSet], table: &DataTable, by: &Option<String>) -> Result<(Vec<Vec<f64>>, usize), EngineError> {
    let by = by.as_deref().map(str::trim).filter(|s| !s.is_empty()).ok_or_else(|| {
        EngineError::Invalid(
            "a paired / repeated measures design needs a 'Matched by' column that identifies each subject (e.g. Donor, Mouse)".into(),
        )
    })?;
    let col = table.require_column(by)?;
    let mut maps: Vec<HashMap<String, f64>> = Vec::new();
    let mut order: Vec<String> = Vec::new();
    for s in sets {
        let mut m = HashMap::new();
        for (&row, &v) in s.rows.iter().zip(&s.values) {
            let key = col.display(row);
            if key.is_empty() {
                continue;
            }
            if m.insert(key.clone(), v).is_some() {
                return Err(EngineError::Invalid(format!(
                    "'{key}' appears more than once in data set \"{}\"; matched designs need one value per {by} and data set",
                    s.label
                )));
            }
            if !order.contains(&key) {
                order.push(key);
            }
        }
        maps.push(m);
    }
    let rows: Vec<Vec<f64>> = order
        .iter()
        .filter(|k| maps.iter().all(|m| m.contains_key(*k)))
        .map(|k| maps.iter().map(|m| m[k]).collect())
        .collect();
    let excluded = order.len() - rows.len();
    if rows.len() < 2 {
        return Err(EngineError::Invalid(format!(
            "fewer than two subjects ({by}) have values in every data set; check the 'Matched by' column"
        )));
    }
    Ok((rows, excluded))
}

/// Number and P value formatting following the output settings.
pub(crate) struct Fmt {
    pub digits: u8,
    pub style: PStyle,
}

impl Fmt {
    pub fn n(&self, v: f64) -> String {
        format_sig(v, self.digits)
    }
    pub fn p(&self, p: f64) -> String {
        self.style.format(p, self.digits)
    }
    pub fn stars(&self, p: f64) -> String {
        self.style.stars(p).to_string()
    }
    pub fn ci(&self, (lo, hi): (f64, f64)) -> String {
        format!("{} to {}", self.n(lo), self.n(hi))
    }
}

pub(crate) fn yes_no(b: bool) -> String {
    if b { "Yes" } else { "No" }.to_string()
}

pub(crate) fn kv(title: &str, rows: Vec<(String, String)>) -> Section {
    Section { title: title.to_string(), table: TextTable { headers: vec![String::new(), String::new()], rows: rows.into_iter().map(|(k, v)| vec![k, v]).collect() } }
}

pub(crate) fn r(k: impl Into<String>, v: impl Into<String>) -> (String, String) {
    (k.into(), v.into())
}

fn descriptive_section(sets: &[DataSet], f: &Fmt) -> Section {
    let headers = ["Data set", "Number of values", "Mean", "Std. Deviation", "Std. Error of Mean", "Median", "Minimum", "Maximum", "Lower 95% CI", "Upper 95% CI"]
        .map(String::from)
        .to_vec();
    let rows = sets
        .iter()
        .map(|s| match describe(&s.values) {
            Some(d) => vec![
                s.label.clone(),
                d.n.to_string(),
                f.n(d.mean),
                f.n(d.sd),
                f.n(d.sem),
                f.n(d.median),
                f.n(d.min),
                f.n(d.max),
                f.n(d.ci95_low),
                f.n(d.ci95_high),
            ],
            None => vec![s.label.clone(), "0".into()],
        })
        .collect();
    Section { title: "Descriptive statistics".into(), table: TextTable { headers, rows } }
}

pub(crate) fn source_rows(table: &DataTable, graph: &GraphSpec) -> Vec<(String, String)> {
    vec![r("Table analyzed", format!("{} (bars of graph \"{}\")", table.name, graph.name))]
}

// ---------------------------------------------------------------------------
// t tests
// ---------------------------------------------------------------------------

pub fn t_test(project: &Project, spec: &TTestSpec) -> Result<Report, EngineError> {
    let (all, graph, table) = graph_data_sets(project, &spec.graph_id)?;
    let available = all.len();
    let sets = pick(all, &spec.data_sets)?;
    if sets.len() != 2 {
        return Err(EngineError::Invalid(format!(
            "a t test compares exactly two data sets; {} chosen (the graph has {available})",
            sets.len()
        )));
    }
    let f = Fmt { digits: spec.output.digits, style: spec.output.p_style };
    let (a, b) = (&sets[0], &sets[1]);
    let alpha = 1.0 - spec.confidence;
    let lognormal = spec.distribution == Distribution::Lognormal;
    let rank_based = spec.distribution == Distribution::Nonparametric;
    let mut excluded = 0;
    let (xa, xb): (Vec<f64>, Vec<f64>) = match spec.design {
        Pairing::Unpaired => (a.values.clone(), b.values.clone()),
        Pairing::Paired => {
            let (rows, ex) = matched(&sets, table, &spec.match_by)?;
            excluded = ex;
            rows.iter().map(|r| (r[0], r[1])).unzip()
        }
    };
    let test = match (spec.design, rank_based) {
        (Pairing::Unpaired, true) => "mann_whitney",
        (Pairing::Unpaired, false) => "unpaired",
        (Pairing::Paired, true) => "wilcoxon",
        (Pairing::Paired, false) => "paired",
    };
    let normality = if rank_based { Vec::new() } else { spec.normality.names() };
    let req = Request::new(
        "t_test",
        json!({ "a": xa, "b": xb }),
        json!({ "test": test, "welch": spec.welch, "log": lognormal, "confidence": spec.confidence, "normality": normality }),
    );
    let (res, engine): (rr::TwoGroup, String) = r_call(&req)?;

    // Report differences as a - b (default) or b - a.
    let swap = spec.difference == DiffDirection::BMinusA;
    let (first, second) = if swap { (b, a) } else { (a, b) };
    let flip = |v: f64| if res.ratio { 1.0 / v } else { -v };
    let diff = if swap { flip(res.diff) } else { res.diff };
    let ci = res.ci.map(|(lo, hi)| if swap { (flip(hi), flip(lo)) } else { (lo, hi) });
    let p = if spec.one_tailed { res.p_two_tailed / 2.0 } else { res.p_two_tailed };
    let significant = p < alpha;
    let conf_pct = format!("{}%", format_sig(spec.confidence * 100.0, 3));

    let mut rows = source_rows(table, graph);
    rows.extend([r("Column A", &a.label), r("vs.", "vs."), r("Column B", &b.label), r("", ""), r(res.test.clone(), "")]);
    rows.push(r("P value", f.p(p)));
    if let Some(exact) = res.exact {
        rows.push(r("Exact or approximate P value?", if exact { "Exact" } else { "Approximate" }));
    }
    rows.push(r("P value summary", if significant { f.stars(p) } else { "ns".into() }));
    rows.push(r(format!("Significantly different (P < {})?", format_sig(alpha, 3)), yes_no(significant)));
    rows.push(r("One- or two-tailed P value?", if spec.one_tailed { "One-tailed" } else { "Two-tailed" }));
    match res.statistic_name.as_str() {
        "t" => {
            let label = if spec.design == Pairing::Unpaired && spec.welch { "Welch-corrected t, df" } else { "t, df" };
            rows.push(r(label, format!("t={}, df={}", f.n(res.statistic.abs()), f.n(res.df.unwrap_or(f64::NAN)))));
        }
        "U" => {
            for e in &res.extra {
                let k = e.label.replace("first data set", &a.label).replace("second data set", &b.label);
                rows.push(r(k, f.n(e.value)));
            }
            rows.push(r("Mann-Whitney U", f.n(res.statistic)));
        }
        _ => {
            for e in &res.extra {
                rows.push(r(e.label.clone(), f.n(e.value)));
            }
            rows.push(r("Sum of signed ranks (W)", f.n(res.statistic)));
        }
    }
    let (fa, fb) = (&first.label, &second.label);
    rows.extend([r("", ""), r("How big is the difference?", "")]);
    match (spec.design, spec.distribution) {
        (Pairing::Unpaired, Distribution::Normal) => {
            rows.push(r(format!("Mean of {}", a.label), f.n(mean(&a.values))));
            rows.push(r(format!("Mean of {}", b.label), f.n(mean(&b.values))));
            rows.push(r(format!("Difference between means ({fa} - {fb}) ± SEM"), format!("{} ± {}", f.n(diff), f.n(res.se.unwrap_or(f64::NAN)))));
        }
        (Pairing::Unpaired, Distribution::Lognormal) => {
            rows.push(r(format!("Geometric mean of {}", a.label), f.n(geomean(&a.values))));
            rows.push(r(format!("Geometric mean of {}", b.label), f.n(geomean(&b.values))));
            rows.push(r(format!("Ratio of geometric means ({fa} / {fb})"), f.n(diff)));
        }
        (Pairing::Unpaired, Distribution::Nonparametric) => {
            rows.push(r(format!("Median of {}", a.label), f.n(median(&a.values))));
            rows.push(r(format!("Median of {}", b.label), f.n(median(&b.values))));
            rows.push(r(format!("Difference: Actual ({fa} - {fb})"), f.n(diff)));
            hodges_lehmann_rows(&mut rows, &res, swap, fa, fb, &conf_pct, &f);
        }
        (Pairing::Paired, Distribution::Normal) => {
            rows.push(r(format!("Mean of differences ({fa} - {fb})"), f.n(diff)));
            rows.push(r("SEM of differences", f.n(res.se.unwrap_or(f64::NAN))));
        }
        (Pairing::Paired, Distribution::Lognormal) => rows.push(r(format!("Geometric mean of ratios ({fa} / {fb})"), f.n(diff))),
        (Pairing::Paired, Distribution::Nonparametric) => {
            rows.push(r(format!("Median of differences ({fa} - {fb})"), f.n(diff)));
            hodges_lehmann_rows(&mut rows, &res, swap, fa, fb, &conf_pct, &f);
        }
    }
    if let Some(ci) = ci {
        rows.push(r(format!("{conf_pct} confidence interval"), f.ci(ci)));
    }
    if let Some(r2) = res.r_squared {
        let label = if spec.design == Pairing::Paired { "R squared (partial eta squared)" } else { "R squared (eta squared)" };
        rows.push(r(label, f.n(r2)));
    }
    if spec.design == Pairing::Paired && res.statistic_name == "t" {
        rows.extend([r("", ""), r("How effective was the pairing?", "")]);
        if let [corr, pair] = res.extra.as_slice() {
            let p_pair = pair.value;
            rows.push(r("Correlation coefficient (r)", f.n(corr.value)));
            rows.push(r("P value (one tailed)", f.p(p_pair)));
            rows.push(r("P value summary", if p_pair < alpha { f.stars(p_pair) } else { "ns".into() }));
            rows.push(r(format!("Was the pairing significantly effective? (P < {})", format_sig(alpha, 3)), yes_no(p_pair < alpha)));
        }
    }
    if let Some(ft) = &res.f_test {
        rows.extend([r("", ""), r("F test to compare variances", "")]);
        rows.push(r("F, DFn, Dfd", format!("{}, {}, {}", f.n(ft.f), ft.dfn, ft.dfd)));
        rows.push(r("P value", f.p(ft.p)));
        rows.push(r("P value summary", if ft.p < alpha { f.stars(ft.p) } else { "ns".into() }));
        rows.push(r(format!("Significantly different (P < {})?", format_sig(alpha, 3)), yes_no(ft.p < alpha)));
    }
    rows.extend([r("", ""), r("Data analyzed", "")]);
    if spec.design == Pairing::Paired {
        let pairs = a.values.len().min(b.values.len()) - excluded.min(a.values.len().min(b.values.len()));
        rows.push(r("Number of pairs", pairs.to_string()));
        if excluded > 0 {
            rows.push(r("Subjects without a value in both data sets (excluded)", excluded.to_string()));
        }
    } else {
        rows.push(r(format!("Number of values in {}", a.label), a.values.len().to_string()));
        rows.push(r(format!("Number of values in {}", b.label), b.values.len().to_string()));
    }

    let mut sections = vec![kv("Table of results", rows)];
    if !res.normality.is_empty() {
        let what = if spec.design == Pairing::Paired { "Normality of differences" } else { "Normality of residuals" };
        sections.push(normality_section(what, &res.normality, &f));
    }
    if spec.results.effect_sizes {
        if let Some(e) = &res.effect {
            let mut er = vec![r("Cohen's d", f.n(if swap { -e.cohens_d } else { e.cohens_d })), r("Hedges' g", f.n(if swap { -e.hedges_g } else { e.hedges_g }))];
            if let Some(g) = e.glass_delta {
                er.push(r(format!("Glass's Δ (SD of {})", b.label), f.n(if swap { -g } else { g })));
            }
            sections.push(kv("Standardized effect sizes", er));
        } else {
            sections.push(kv("Standardized effect sizes", vec![r("Not computed for rank-based tests", "")]));
        }
    }
    if spec.results.aicc {
        if let Some(m) = &res.aicc {
            sections.push(kv(
                "Compare models using AICc",
                vec![
                    r("Simpler model", if spec.design == Pairing::Paired { "Mean difference = 0" } else { "One shared mean" }),
                    r("AICc (simpler model)", f.n(m.simpler)),
                    r("AICc (separate means)", f.n(m.complex)),
                    r("Probability that the means differ", format!("{}%", format_sig(m.prob_complex * 100.0, 3))),
                ],
            ));
        }
    }
    if spec.results.descriptive {
        sections.push(descriptive_section(&sets, &f));
    }

    let mut brackets = Vec::new();
    if spec.graph.show && (significant || spec.graph.show_ns) {
        brackets.push(Bracket { a: a.label.clone(), b: b.label.clone(), text: if significant { f.stars(p) } else { "ns".into() } });
    }
    Ok(Report { title: res.test.clone(), sections, brackets, curves: Vec::new(), engine })
}

/// The Hodges-Lehmann estimate of the difference and its confidence interval (rank tests).
fn hodges_lehmann_rows(rows: &mut Vec<(String, String)>, res: &rr::TwoGroup, swap: bool, fa: &str, fb: &str, conf_pct: &str, f: &Fmt) {
    let Some(hl) = &res.hodges_lehmann else { return };
    let est = if swap { -hl.estimate } else { hl.estimate };
    rows.push(r(format!("Difference: Hodges-Lehmann ({fa} - {fb})"), f.n(est)));
    if let Some((lo, hi)) = hl.ci {
        let ci = if swap { (-hi, -lo) } else { (lo, hi) };
        rows.push(r(format!("{conf_pct} CI of the Hodges-Lehmann difference"), f.ci(ci)));
    }
}

/// Normality tests of residuals (or differences), as in Prism's results.
pub(crate) fn normality_section(title: &str, tests: &[rr::Normality], f: &Fmt) -> Section {
    let headers = ["Test", "Statistic", "P value", "Passed normality test (alpha=0.05)?", "P value summary"].map(String::from).to_vec();
    let rows = tests
        .iter()
        .map(|t| {
            let label = NormalityTests::label(&t.test).to_string();
            match &t.note {
                Some(note) => vec![label, format!("Not computed: {note}"), String::new(), String::new(), String::new()],
                None => vec![label, f.n(t.statistic), f.p(t.p), yes_no(t.p >= 0.05), if t.p < 0.05 { f.stars(t.p) } else { "ns".into() }],
            }
        })
        .collect();
    Section { title: title.to_string(), table: TextTable { headers, rows } }
}

fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len() as f64
}

fn geomean(v: &[f64]) -> f64 {
    10f64.powf(v.iter().map(|x| x.log10()).sum::<f64>() / v.len() as f64)
}

fn median(v: &[f64]) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.total_cmp(b));
    percentile_sorted(&s, 0.5)
}

// ---------------------------------------------------------------------------
// One-way ANOVA
// ---------------------------------------------------------------------------

/// The multiple comparisons test the choices lead to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Method {
    Tukey,
    Dunnett,
    Sidak,
    GamesHowell,
    DunnettT3,
    Dunn,
    Uncorrected,
    TwoStageFdr,
}

impl Method {
    fn label(self) -> &'static str {
        match self {
            Method::Tukey => "Tukey's multiple comparisons test",
            Method::Dunnett => "Dunnett's multiple comparisons test",
            Method::Sidak => "Šídák's multiple comparisons test",
            Method::GamesHowell => "Games-Howell's multiple comparisons test",
            Method::DunnettT3 => "Dunnett's T3 multiple comparisons test",
            Method::Dunn => "Dunn's multiple comparisons test",
            Method::Uncorrected => "Uncorrected multiple comparisons (Fisher's LSD)",
            Method::TwoStageFdr => "Two-stage linear step-up procedure of Benjamini, Krieger and Yekutieli",
        }
    }

    /// The name R's code uses.
    fn r_name(self) -> &'static str {
        match self {
            Method::Tukey => "tukey",
            Method::Dunnett => "dunnett",
            Method::Sidak => "sidak",
            Method::GamesHowell => "games_howell",
            Method::DunnettT3 => "dunnett_t3",
            Method::Dunn => "dunn",
            Method::Uncorrected => "uncorrected",
            Method::TwoStageFdr => "fdr",
        }
    }

    /// Whether the method reports confidence intervals.
    fn has_ci(self) -> bool {
        !matches!(self, Method::Dunn | Method::TwoStageFdr)
    }
}

/// All pairs i < j of `k` data sets.
fn all_pairs(k: usize) -> Vec<(usize, usize)> {
    (0..k).flat_map(|i| (i + 1..k).map(move |j| (i, j))).collect()
}

pub fn one_way_anova(project: &Project, spec: &AnovaSpec) -> Result<Report, EngineError> {
    let (all, graph, table) = graph_data_sets(project, &spec.graph_id)?;
    let sets = pick(all, &spec.data_sets)?;
    if sets.len() < 2 {
        return Err(EngineError::Invalid("one-way ANOVA needs at least two data sets".into()));
    }
    let f = Fmt { digits: spec.output.digits, style: spec.output.p_style };
    let alpha = spec.alpha;
    let k = sets.len();
    let lognormal = spec.distribution == Distribution::Lognormal;
    let rank_based = spec.distribution == Distribution::Nonparametric;
    let alpha_txt = format_sig(alpha, 3);
    let sig = |p: f64| if p < alpha { f.stars(p) } else { "ns".to_string() };

    // Which comparisons, and with which test.
    let pairs: Vec<(usize, usize)> = match &spec.comparisons {
        Comparisons::None => Vec::new(),
        Comparisons::AllPairs => all_pairs(k),
        Comparisons::VsControl { control } => {
            let c = sets
                .iter()
                .position(|s| s.label == *control || s.label.eq_ignore_ascii_case(control.trim()))
                .ok_or_else(|| EngineError::NotFound(format!("control data set '{control}'")))?;
            (0..k).filter(|&i| i != c).map(|i| (c, i)).collect()
        }
        Comparisons::Selected { pairs } => pairs
            .iter()
            .map(|[x, y]| {
                let find = |l: &String| sets.iter().position(|s| s.label == *l || s.label.eq_ignore_ascii_case(l.trim()));
                match (find(x), find(y)) {
                    (Some(i), Some(j)) if i != j => Ok((i, j)),
                    _ => Err(EngineError::NotFound(format!("pair '{x}' vs. '{y}' among the chosen data sets"))),
                }
            })
            .collect::<Result<_, _>>()?,
    };
    let welch = spec.design == Matching::Unmatched && !spec.equal_sd && !rank_based;
    let method = match spec.correction {
        Correction::None => Method::Uncorrected,
        Correction::Fdr => Method::TwoStageFdr,
        Correction::Hypothesis if rank_based => Method::Dunn,
        Correction::Hypothesis if welch => match spec.comparisons {
            Comparisons::AllPairs if sets.iter().all(|s| s.values.len() >= 50) => Method::GamesHowell,
            _ => Method::DunnettT3,
        },
        Correction::Hypothesis => match spec.comparisons {
            Comparisons::VsControl { .. } => Method::Dunnett,
            Comparisons::Selected { .. } => Method::Sidak,
            _ => Method::Tukey,
        },
    };

    let design = match (spec.design, rank_based) {
        (Matching::Unmatched, true) => "kruskal",
        (Matching::Matched, true) => "friedman",
        (Matching::Unmatched, false) if spec.equal_sd => "ordinary",
        (Matching::Unmatched, false) => "welch",
        (Matching::Matched, false) => "rm",
    };
    let mut excluded = 0;
    let mut subjects = 0;
    let data = match spec.design {
        Matching::Unmatched => {
            json!({ "groups": sets.iter().map(|s| &s.values).collect::<Vec<_>>(), "labels": sets.iter().map(|s| &s.label).collect::<Vec<_>>(), "pairs": pairs })
        }
        Matching::Matched => {
            let (m, ex) = matched(&sets, table, &spec.match_by)?;
            excluded = ex;
            subjects = m.len();
            json!({ "rows": m, "pairs": pairs })
        }
    };
    let normality = if rank_based { Vec::new() } else { spec.normality.names() };
    let req = Request::new(
        "anova",
        data,
        json!({ "design": design, "log": lognormal, "method": method.r_name(), "alpha": alpha, "sphericity": spec.sphericity, "normality": normality }),
    );
    let (res, engine): (rr::Anova, String) = r_call(&req)?;

    let mut rows = source_rows(table, graph);
    rows.push(r("Data sets analyzed", sets.iter().map(|s| s.label.as_str()).collect::<Vec<_>>().join(", ")));
    if lognormal {
        rows.push(r("Values analyzed", "Logarithms (base 10); differences are ratios of geometric means"));
    }
    rows.push(r("", ""));
    let mut sections = Vec::new();
    let mut extra_sections = Vec::new();
    let mut effect_rows: Vec<(String, String)> = Vec::new();
    let mut aicc_section: Option<Section> = None;
    let missing = || EngineError::Invalid("R returned an incomplete ANOVA result".into());

    match design {
        "kruskal" => {
            let p = res.p.ok_or_else(missing)?;
            rows.extend([
                r("Kruskal-Wallis test", ""),
                r("P value", f.p(p)),
                r("Exact or approximate P value?", "Approximate"),
                r("P value summary", sig(p)),
                r(format!("Do the medians vary signif. (P < {alpha_txt})?"), yes_no(p < alpha)),
                r("Number of groups", k.to_string()),
                r("Kruskal-Wallis statistic", f.n(res.h.unwrap_or(f64::NAN))),
            ]);
            effect_rows.push(r("Epsilon squared (ε²)", f.n(res.epsilon_squared.unwrap_or(f64::NAN))));
            extra_sections.push(Section {
                title: "Mean ranks".into(),
                table: TextTable {
                    headers: vec!["Data set".into(), "Number of values".into(), "Sum of ranks".into(), "Mean rank".into()],
                    rows: sets
                        .iter()
                        .enumerate()
                        .map(|(i, s)| {
                            let at = |v: &[f64]| v.get(i).copied().unwrap_or(f64::NAN);
                            vec![s.label.clone(), s.values.len().to_string(), f.n(at(&res.rank_sums)), f.n(at(&res.mean_ranks))]
                        })
                        .collect(),
                },
            });
        }
        "friedman" => {
            let p = res.p.ok_or_else(missing)?;
            rows.extend([
                r("Friedman test", ""),
                r("P value", f.p(p)),
                r("Exact or approximate P value?", "Approximate"),
                r("P value summary", sig(p)),
                r(format!("Are the medians signif. different? (P < {alpha_txt})"), yes_no(p < alpha)),
                r("Number of treatments (columns)", k.to_string()),
                r("Number of subjects (matched)", res.subject_count().unwrap_or(subjects).to_string()),
                r("Friedman statistic", f.n(res.q.unwrap_or(f64::NAN))),
            ]);
            if excluded > 0 {
                rows.push(r("Subjects without values in every data set (excluded)", excluded.to_string()));
            }
            effect_rows.push(r("Kendall's W", f.n(res.kendalls_w.unwrap_or(f64::NAN))));
        }
        "rm" => {
            let full = res.f.clone().ok_or_else(missing)?;
            let gg = res.f_gg.clone().ok_or_else(missing)?;
            let used = if spec.sphericity { full.clone() } else { gg };
            let epsilon = res.epsilon.unwrap_or(1.0);
            let r2 = res.r_squared.unwrap_or(f64::NAN);
            rows.extend([
                r("Repeated measures ANOVA summary", ""),
                r("Assume sphericity (equal variability of differences)?", yes_no(spec.sphericity)),
                r("F", f.n(full.f)),
                r("P value", f.p(used.p)),
                r("P value summary", sig(used.p)),
                r(format!("Statistically significant (P < {alpha_txt})?"), yes_no(used.p < alpha)),
            ]);
            if !spec.sphericity {
                rows.push(r("Geisser-Greenhouse's epsilon", f.n(epsilon)));
            }
            rows.push(r("R squared", f.n(r2)));
            if let Some(s) = res.subjects_f() {
                rows.extend([
                    r("", ""),
                    r("Was the matching effective?", ""),
                    r("F", f.n(s.f)),
                    r("P value", f.p(s.p)),
                    r("P value summary", sig(s.p)),
                    r(format!("Is there significant matching (P < {alpha_txt})?"), yes_no(s.p < alpha)),
                ]);
            }
            effect_rows.push(r("Partial eta squared (η²p)", f.n(r2)));
            // The treatment P value in the table is the one used (corrected unless sphericity is assumed).
            let mut table_rows = res.table.clone();
            if let Some(first) = table_rows.first_mut() {
                first.p = Some(used.p);
            }
            sections.push(anova_table(&table_rows, &f, if spec.sphericity { None } else { Some(epsilon) }));
            let mut summary = vec![r("Number of treatments (columns)", k.to_string()), r("Number of subjects (matched)", subjects.to_string())];
            if excluded > 0 {
                summary.push(r("Subjects without values in every data set (excluded)", excluded.to_string()));
            }
            extra_sections.push(kv("Data summary", summary));
        }
        "ordinary" => {
            let ow = res.f.clone().ok_or_else(missing)?;
            effect_rows.push(r("Eta squared (η²)", f.n(res.eta_squared.unwrap_or(f64::NAN))));
            effect_rows.push(r("Omega squared (ω²)", f.n(res.omega_squared.unwrap_or(f64::NAN))));
            if let Some(m) = &res.aicc {
                aicc_section = Some(kv(
                    "Compare models using AICc",
                    vec![
                        r("AICc (one shared mean)", f.n(m.simpler)),
                        r("AICc (separate means)", f.n(m.complex)),
                        r("Probability that the means differ", format!("{}%", format_sig(m.prob_complex * 100.0, 3))),
                    ],
                ));
            }
            let n_total: usize = sets.iter().map(|s| s.values.len()).sum();
            extra_sections.push(kv("Data summary", vec![r("Number of treatments (columns)", k.to_string()), r("Number of values (total)", n_total.to_string())]));
            rows.extend([
                r("ANOVA summary", ""),
                r("F", f.n(ow.f)),
                r("P value", f.p(ow.p)),
                r("P value summary", sig(ow.p)),
                r(format!("Significant diff. among means (P < {alpha_txt})?"), yes_no(ow.p < alpha)),
                r("R squared", f.n(res.r_squared.unwrap_or(f64::NAN))),
            ]);
            if let Some(bf) = &res.brown_forsythe_sd {
                rows.extend([
                    r("", ""),
                    r("Brown-Forsythe test", ""),
                    r("F (DFn, DFd)", format!("{} ({}, {})", f.n(bf.f), bf.df1, bf.df2)),
                    r("P value", f.p(bf.p)),
                    r("P value summary", sig(bf.p)),
                    r(format!("Are SDs significantly different (P < {alpha_txt})?"), yes_no(bf.p < alpha)),
                ]);
            }
            if let Some(b) = &res.bartlett {
                rows.extend([
                    r("", ""),
                    r("Bartlett's test", ""),
                    r("Bartlett's statistic (corrected)", f.n(b.statistic)),
                    r("DF", format_sig(b.df, 3)),
                    r("P value", f.p(b.p)),
                    r("P value summary", sig(b.p)),
                    r(format!("Are SDs significantly different (P < {alpha_txt})?"), yes_no(b.p < alpha)),
                ]);
            }
            sections.push(anova_table(&res.table, &f, None));
        }
        _ => {
            let bf = res.brown_forsythe.clone().ok_or_else(missing)?;
            let w = res.welch.clone().ok_or_else(missing)?;
            rows.extend([
                r("Brown-Forsythe ANOVA test", ""),
                r("F* (DFn, DFd)", format!("{} ({}, {})", f.n(bf.f), format_sig(bf.df1, 4), format_sig(bf.df2, 4))),
                r("P value", f.p(bf.p)),
                r("P value summary", sig(bf.p)),
                r(format!("Significant diff. among means (P < {alpha_txt})?"), yes_no(bf.p < alpha)),
                r("", ""),
                r("Welch's ANOVA test", ""),
                r("W (DFn, DFd)", format!("{} ({}, {})", f.n(w.f), format_sig(w.df1, 4), format_sig(w.df2, 4))),
                r("P value", f.p(w.p)),
                r("P value summary", sig(w.p)),
                r(format!("Significant diff. among means (P < {alpha_txt})?"), yes_no(w.p < alpha)),
            ]);
        }
    }
    let mut all_sections = vec![kv(spec.test_name(), rows)];
    all_sections.extend(sections);

    // Multiple comparisons.
    let mut brackets = Vec::new();
    if !res.comparisons.is_empty() {
        let fdr = method == Method::TwoStageFdr;
        let show_ci = method.has_ci() && !rank_based;
        let diff_header = if rank_based {
            "Mean rank diff."
        } else if lognormal {
            "Ratio of geometric means"
        } else {
            "Mean Diff."
        };
        let conf_pct = format!("{}%", format_sig((1.0 - alpha) * 100.0, 4));
        let mut headers = vec![method.label().to_string(), diff_header.to_string()];
        if show_ci {
            headers.push(format!("{conf_pct} CI of diff."));
        }
        headers.extend([if fdr { "Discovery?" } else { "Below threshold?" }, "Summary", if fdr { "q value" } else { "Adjusted P Value" }].map(String::from));
        let mut comp_rows = Vec::new();
        for c in &res.comparisons {
            let (Some(sa), Some(sb)) = (sets.get(c.a), sets.get(c.b)) else { continue };
            let (mut diff, mut ci) = (c.diff, c.ci);
            let (mut la, mut lb) = (&sa.label, &sb.label);
            if spec.swap {
                diff = -diff;
                ci = ci.map(|(lo, hi)| (-hi, -lo));
                std::mem::swap(&mut la, &mut lb);
            }
            let (diff_txt, ci_txt) = if lognormal && !rank_based {
                (f.n(10f64.powf(diff)), ci.map(|(lo, hi)| f.ci((10f64.powf(lo), 10f64.powf(hi)))))
            } else {
                (f.n(diff), ci.map(|x| f.ci(x)))
            };
            let mut row = vec![format!("{la} vs. {lb}"), diff_txt];
            if show_ci {
                row.push(ci_txt.unwrap_or_default());
            }
            let summary = if c.significant { f.stars(c.p_adjusted) } else { "ns".into() };
            row.extend([yes_no(c.significant), summary.clone(), f.p(c.p_adjusted)]);
            comp_rows.push(row);
            if spec.graph.show && (c.significant || spec.graph.show_ns) {
                brackets.push(Bracket { a: sa.label.clone(), b: sb.label.clone(), text: summary });
            }
        }
        all_sections.push(Section { title: "Multiple comparisons".into(), table: TextTable { headers, rows: comp_rows } });
        let mut details = vec![
            r("Number of families", "1"),
            r("Number of comparisons per family", res.comparisons.len().to_string()),
            r(if fdr { "Q (false discovery rate)" } else { "Alpha" }, format_sig(alpha, 3)),
        ];
        if !rank_based && !fdr {
            details.push(r("Compare", match spec.comparisons {
                Comparisons::AllPairs => "Every data set with every other",
                Comparisons::VsControl { .. } => "Each data set with the control",
                _ => "Preselected pairs",
            }));
        }
        all_sections.push(kv("Multiple comparisons details", details));
    }
    if !res.normality.is_empty() {
        all_sections.push(normality_section("Normality of residuals", &res.normality, &f));
    }
    all_sections.extend(extra_sections);
    if spec.results.effect_sizes && !effect_rows.is_empty() {
        all_sections.push(kv("Standardized effect sizes", effect_rows));
    }
    if spec.results.aicc {
        if let Some(s) = aicc_section {
            all_sections.push(s);
        }
    }
    if spec.results.descriptive {
        all_sections.push(descriptive_section(&sets, &f));
    }
    Ok(Report { title: spec.test_name().to_string(), sections: all_sections, brackets, curves: Vec::new(), engine })
}

fn anova_table(rows: &[rr::AnovaRow], f: &Fmt, epsilon: Option<f64>) -> Section {
    let headers = ["ANOVA table", "SS", "DF", "MS", "F (DFn, DFd)", "P value"].map(String::from).to_vec();
    let treat_df = rows.first().map_or(0.0, |r| r.df);
    let resid_df = rows.iter().find(|r| r.source.starts_with("Residual")).map_or(0.0, |r| r.df);
    let body = rows
        .iter()
        .map(|row| {
            let fcell = match (row.f, row.source.starts_with("Treatment"), epsilon) {
                (Some(fv), true, Some(e)) => format!("F ({}, {}) = {}", format_sig(e * treat_df, 4), format_sig(e * resid_df, 4), f.n(fv)),
                (Some(fv), _, _) => format!("F ({}, {}) = {}", row.df, resid_df, f.n(fv)),
                _ => String::new(),
            };
            vec![
                row.source.clone(),
                f.n(row.ss),
                format_sig(row.df, 6),
                row.ms.map(|m| f.n(m)).unwrap_or_default(),
                fcell,
                row.p.map(|p| { let s = f.p(p); if s.starts_with('<') { format!("P{s}") } else { format!("P={s}") } }).unwrap_or_default(),
            ]
        })
        .collect();
    Section { title: "ANOVA table".into(), table: TextTable { headers, rows: body } }
}

/// Data set labels and value counts of a graph (its bars, or the data sets of an XY
/// graph), for pickers and the MCP tool.
pub fn list_data_sets(project: &Project, graph_id: &str) -> Result<Vec<(String, usize)>, EngineError> {
    let (g, table) = graph_and_table(project, graph_id)?;
    let sets = analysed_sets(g, table, g.kind.shape())?;
    Ok(sets.into_iter().map(|s| (s.label, s.values.len())).collect())
}

/// Name of the multiple comparisons test the parameters lead to (for the dialogs).
pub fn followup_name(spec: &AnovaSpec) -> String {
    let rank = spec.distribution == Distribution::Nonparametric;
    let welch = spec.design == Matching::Unmatched && !spec.equal_sd && !rank;
    let name = match spec.correction {
        Correction::Fdr => "Two-stage step-up method of Benjamini, Krieger and Yekutieli (FDR)",
        Correction::None if rank => "Uncorrected Dunn's test",
        Correction::None if welch => "Welch t tests without correction for multiple comparisons",
        Correction::None => "Fisher's LSD test",
        Correction::Hypothesis if rank => "Dunn's multiple comparisons test",
        Correction::Hypothesis if welch => "Dunnett's T3 multiple comparisons test (Games-Howell when every n ≥ 50)",
        Correction::Hypothesis => match spec.comparisons {
            Comparisons::VsControl { .. } => "Dunnett's multiple comparisons test",
            Comparisons::Selected { .. } => "Šídák's multiple comparisons test",
            _ => "Tukey's multiple comparisons test",
        },
    };
    name.to_string()
}
