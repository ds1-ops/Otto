//! XY analyses of a graph's data sets: nonlinear regression (curve fits), linear
//! regression and correlation. R fits and tests; the results are laid out like Prism's,
//! one column per data set, and fitted curves are drawn on the graph.

use serde_json::json;

use otto_core::analysis::{format_sig, AucSpec, CorrMethod, CorrelationSpec, FitSpec, LinRegSpec, OutputFormat};
use otto_core::{DataTable, GraphSpec, Project};
use otto_plot::{Curve, TextTable};
use otto_r::results as rr;
use otto_r::Request;

use crate::column_tests::{graph_and_table, r_call, yes_no, DataSet, Fmt};
use crate::results::{Report, Section};
use crate::EngineError;

/// The data sets (legend entries) of an XY graph, each with its X and Y values.
pub fn xy_data_sets(g: &GraphSpec, table: &DataTable) -> Result<Vec<DataSet>, EngineError> {
    if g.kind.shape() != otto_core::DataShape::Xy {
        return Err(EngineError::Invalid(format!(
            "\"{}\" is a bar graph; curve fits, linear regression and correlation analyse the data sets of an XY graph",
            g.name
        )));
    }
    let data = otto_plot::distill::distill(g, table)?;
    Ok(data
        .sets
        .iter()
        .enumerate()
        .map(|(si, label)| {
            let pts: Vec<_> = data.points.iter().filter(|p| p.set == si).collect();
            DataSet {
                label: label.clone(),
                values: pts.iter().map(|p| p.y).collect(),
                rows: pts.iter().map(|p| p.row).collect(),
                x: pts.iter().map(|p| p.x.num()).collect(),
            }
        })
        .collect())
}

/// Data sets to analyse, their graph and table, and the graph's X axis range.
type Chosen<'a> = (Vec<DataSet>, &'a GraphSpec, &'a DataTable, Option<(f64, f64)>);

/// The chosen data sets of an XY graph (all when none are chosen), plus the X axis range.
fn chosen<'a>(project: &'a Project, graph_id: &str, wanted: &[String]) -> Result<Chosen<'a>, EngineError> {
    let (g, table) = graph_and_table(project, graph_id)?;
    let all = xy_data_sets(g, table)?;
    let sets = if wanted.is_empty() {
        all
    } else {
        wanted
            .iter()
            .map(|w| {
                all.iter().find(|d| d.label == *w || d.label.eq_ignore_ascii_case(w.trim())).cloned().ok_or_else(|| {
                    EngineError::NotFound(format!("data set '{w}' (available: {})", all.iter().map(|d| d.label.as_str()).collect::<Vec<_>>().join(", ")))
                })
            })
            .collect::<Result<Vec<_>, _>>()?
    };
    if sets.is_empty() {
        return Err(EngineError::Invalid(format!("\"{}\" has no data sets to analyse", g.name)));
    }
    let data = otto_plot::distill::distill(g, table)?;
    let range = otto_plot::draw::x_axis_range(&data, g, &otto_plot::Theme::default());
    Ok((sets, g, table, range))
}

/// One results table with a column per data set (Prism's layout for XY analyses).
struct Columns {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl Columns {
    fn new(corner: &str, sets: &[DataSet]) -> Self {
        let mut headers = vec![corner.to_string()];
        headers.extend(sets.iter().map(|s| s.label.clone()));
        Columns { headers, rows: Vec::new() }
    }

    fn heading(&mut self, title: &str) {
        let mut row = vec![title.to_string()];
        row.resize(self.headers.len(), String::new());
        self.rows.push(row);
    }

    fn row(&mut self, label: &str, values: Vec<String>) {
        let mut row = vec![format!("    {label}")];
        row.extend(values);
        row.resize(self.headers.len(), String::new());
        self.rows.push(row);
    }

    fn section(self, title: &str) -> Section {
        Section { title: title.to_string(), table: TextTable { headers: self.headers, rows: self.rows } }
    }
}

fn fmt(o: &OutputFormat) -> Fmt {
    Fmt { digits: o.digits, style: o.p_style }
}

fn conf_label(conf: f64) -> String {
    format!("{}%", format_sig(conf * 100.0, 3))
}

fn distinct(v: &[f64]) -> usize {
    let mut s: Vec<f64> = v.to_vec();
    s.sort_by(|a, b| a.total_cmp(b));
    s.dedup();
    s.len()
}

/// Runs one request per data set. A data set R can't analyse gets its message instead of
/// results; when none can be analysed the first message is the error.
fn per_set<T: serde::de::DeserializeOwned>(sets: &[DataSet], req: impl Fn(&DataSet) -> Request) -> Result<(Vec<Result<T, String>>, String), EngineError> {
    let mut out = Vec::new();
    let mut engine = String::new();
    for s in sets {
        match r_call::<T>(&req(s)) {
            Ok((v, e)) => {
                engine = e;
                out.push(Ok(v));
            }
            Err(EngineError::Invalid(msg)) => out.push(Err(msg)),
            Err(e) => return Err(e),
        }
    }
    if out.iter().all(|r| r.is_err()) {
        let first = out.iter().find_map(|r| r.as_ref().err().cloned()).unwrap_or_default();
        return Err(EngineError::Invalid(if sets.len() == 1 { first } else { format!("no data set could be analysed: {first}") }));
    }
    Ok((out, engine))
}

/// A value per data set (blank where the analysis failed).
fn each<T>(res: &[Result<T, String>], f: impl Fn(&T) -> String) -> Vec<String> {
    res.iter().map(|r| r.as_ref().map(&f).unwrap_or_default()).collect()
}

fn failures<T>(c: &mut Columns, res: &[Result<T, String>]) {
    if res.iter().any(|r| r.is_err()) {
        c.row("Could not be analysed", res.iter().map(|r| r.as_ref().err().cloned().unwrap_or_default()).collect());
    }
}

// ---------------------------------------------------------------------------
// Nonlinear regression
// ---------------------------------------------------------------------------

pub fn curve_fit(project: &Project, spec: &FitSpec) -> Result<Report, EngineError> {
    let (sets, graph, table, range) = chosen(project, &spec.graph_id, &spec.data_sets)?;
    let f = fmt(&spec.output);
    let params = spec.model.params();
    let fixed: serde_json::Map<String, serde_json::Value> =
        spec.fixed.iter().filter(|p| params.contains(&p.name.as_str())).map(|p| (p.name.clone(), json!(p.value))).collect();
    let (res, engine) = per_set::<rr::CurveFit>(&sets, |s| {
        Request::new(
            "curve_fit",
            json!({ "x": s.x, "y": s.values }),
            json!({ "model": spec.model.id(), "fixed": fixed, "confidence": spec.confidence, "xrange": range.map(|(a, b)| vec![a, b]) }),
        )
    })?;

    let mut c = Columns::new(spec.model.label(), &sets);
    c.heading("Best-fit values");
    for name in params {
        c.row(
            name,
            each(&res, |r| match r.params.iter().find(|p| p.name == *name) {
                Some(p) if p.fixed => format!("= {}", f.n(p.value)),
                Some(p) => f.n(p.value),
                None => String::new(),
            }),
        );
    }
    let derived: Vec<String> = res.iter().flatten().flat_map(|r| r.derived.iter().map(|d| d.name.clone())).fold(Vec::new(), |mut v, n| {
        if !v.contains(&n) {
            v.push(n);
        }
        v
    });
    for name in &derived {
        c.row(name, each(&res, |r| r.derived.iter().find(|d| d.name == *name).map(|d| f.n(d.value)).unwrap_or_default()));
    }
    let free: Vec<&str> = params.iter().copied().filter(|p| !fixed.contains_key(*p)).collect();
    c.heading("Std. Error");
    for name in &free {
        c.row(name, each(&res, |r| r.params.iter().find(|p| p.name == *name).map(|p| f.n(p.se)).unwrap_or_default()));
    }
    c.heading(&format!("{} CI (asymptotic)", conf_label(spec.confidence)));
    for name in &free {
        c.row(name, each(&res, |r| r.params.iter().find(|p| p.name == *name).and_then(|p| p.ci).map(|ci| f.ci(ci)).unwrap_or_default()));
    }
    for name in &derived {
        c.row(name, each(&res, |r| r.derived.iter().find(|d| d.name == *name).and_then(|d| d.ci).map(|ci| f.ci(ci)).unwrap_or_default()));
    }
    c.heading("Goodness of Fit");
    c.row("Degrees of Freedom", each(&res, |r| format_sig(r.df, 6)));
    c.row("R squared", each(&res, |r| f.n(r.r_squared)));
    c.row("Sum of Squares", each(&res, |r| f.n(r.ss)));
    c.row("Sy.x", each(&res, |r| f.n(r.syx)));
    if res.iter().flatten().any(|r| !r.converged) {
        c.row("Converged?", each(&res, |r| yes_no(r.converged)));
    }
    if !fixed.is_empty() {
        c.heading("Constraints");
        for p in spec.fixed.iter().filter(|p| fixed.contains_key(&p.name)) {
            c.row(&p.name, vec![format!("{} = {}", p.name, f.n(p.value)); sets.len()]);
        }
    }
    c.heading("Number of points");
    c.row("# of X values", sets.iter().map(|s| distinct(&s.x).to_string()).collect());
    c.row("# Y values analyzed", each(&res, |r| r.n.to_string()));
    failures(&mut c, &res);

    let info = vec![
        crate::column_tests::r("Table analyzed", format!("{} (data sets of graph \"{}\")", table.name, graph.name)),
        crate::column_tests::r("Model", spec.model.label()),
        crate::column_tests::r("Equation", spec.model.equation()),
        crate::column_tests::r("Fitting method", "Least squares (Levenberg-Marquardt, minpack.lm)"),
    ];
    let curves = if spec.graph.show { fitted_curves(&sets, &res, |r| &r.curve) } else { Vec::new() };
    Ok(Report {
        title: format!("Nonlinear regression: {}", spec.model.label()),
        sections: vec![crate::column_tests::kv("Nonlinear fit", info), c.section("Table of results")],
        brackets: Vec::new(),
        curves,
        engine,
    })
}

fn fitted_curves<T>(sets: &[DataSet], res: &[Result<T, String>], curve: impl Fn(&T) -> &rr::Curve) -> Vec<Curve> {
    sets.iter()
        .zip(res)
        .filter_map(|(s, r)| r.as_ref().ok().map(|r| Curve { set: s.label.clone(), points: curve(r).points() }))
        .filter(|c| c.points.len() >= 2)
        .collect()
}

// ---------------------------------------------------------------------------
// Linear regression
// ---------------------------------------------------------------------------

pub fn linear_regression(project: &Project, spec: &LinRegSpec) -> Result<Report, EngineError> {
    let (sets, graph, table, range) = chosen(project, &spec.graph_id, &spec.data_sets)?;
    let f = fmt(&spec.output);
    let (res, engine) = per_set::<rr::LinearRegression>(&sets, |s| {
        Request::new(
            "linear_regression",
            json!({ "x": s.x, "y": s.values }),
            json!({ "confidence": spec.confidence, "xrange": range.map(|(a, b)| vec![a, b]) }),
        )
    })?;
    let mut c = Columns::new("Simple linear regression", &sets);
    c.heading("Best-fit values");
    c.row("Slope", each(&res, |r| f.n(r.slope.value)));
    c.row("Y-intercept", each(&res, |r| f.n(r.intercept.value)));
    c.row("X-intercept", each(&res, |r| f.n(r.x_intercept)));
    c.row("1/slope", each(&res, |r| f.n(r.inv_slope)));
    c.heading("Std. Error");
    c.row("Slope", each(&res, |r| f.n(r.slope.se)));
    c.row("Y-intercept", each(&res, |r| f.n(r.intercept.se)));
    c.heading(&format!("{} Confidence Intervals", conf_label(spec.confidence)));
    c.row("Slope", each(&res, |r| r.slope.ci.map(|ci| f.ci(ci)).unwrap_or_default()));
    c.row("Y-intercept", each(&res, |r| r.intercept.ci.map(|ci| f.ci(ci)).unwrap_or_default()));
    c.heading("Goodness of Fit");
    c.row("R squared", each(&res, |r| f.n(r.r_squared)));
    c.row("Sy.x", each(&res, |r| f.n(r.syx)));
    c.heading("Is slope significantly non-zero?");
    c.row("F", each(&res, |r| f.n(r.f)));
    c.row("DFn, DFd", each(&res, |r| format!("{}, {}", format_sig(r.dfn, 6), format_sig(r.dfd, 6))));
    c.row("P value", each(&res, |r| f.p(r.p)));
    c.row("Deviation from zero?", each(&res, |r| if r.p < 0.05 { "Significant" } else { "Not significant" }.to_string()));
    c.heading("Equation");
    c.row(
        "",
        each(&res, |r| {
            let b = r.intercept.value;
            format!("Y = {}*X {} {}", f.n(r.slope.value), if b < 0.0 { "-" } else { "+" }, f.n(b.abs()))
        }),
    );
    c.heading("Data");
    c.row("Number of X values", sets.iter().map(|s| distinct(&s.x).to_string()).collect());
    c.row("Total number of values", each(&res, |r| r.n.to_string()));
    failures(&mut c, &res);
    let info = vec![crate::column_tests::r("Table analyzed", format!("{} (data sets of graph \"{}\")", table.name, graph.name))];
    let curves = if spec.graph.show { fitted_curves(&sets, &res, |r| &r.curve) } else { Vec::new() };
    Ok(Report {
        title: "Simple linear regression".into(),
        sections: vec![crate::column_tests::kv("Linear regression", info), c.section("Table of results")],
        brackets: Vec::new(),
        curves,
        engine,
    })
}

// ---------------------------------------------------------------------------
// Correlation
// ---------------------------------------------------------------------------

pub fn correlation(project: &Project, spec: &CorrelationSpec) -> Result<Report, EngineError> {
    let (sets, graph, table, _) = chosen(project, &spec.graph_id, &spec.data_sets)?;
    let f = fmt(&spec.output);
    let method = match spec.method {
        CorrMethod::Pearson => "pearson",
        CorrMethod::Spearman => "spearman",
    };
    let (res, engine) = per_set::<rr::Correlation>(&sets, |s| {
        Request::new(
            "correlation",
            json!({ "x": s.x, "y": s.values }),
            json!({ "method": method, "one_tailed": spec.one_tailed, "confidence": spec.confidence }),
        )
    })?;
    let pearson = spec.method == CorrMethod::Pearson;
    let mut c = Columns::new(if pearson { "Pearson correlation" } else { "Spearman correlation" }, &sets);
    c.heading(if pearson { "Pearson r" } else { "Spearman r" });
    c.row("r", each(&res, |r| f.n(r.r)));
    if pearson {
        c.row(&format!("{} confidence interval", conf_label(spec.confidence)), each(&res, |r| r.ci.map(|ci| f.ci(ci)).unwrap_or_default()));
        c.row("R squared", each(&res, |r| f.n(r.r_squared.unwrap_or(f64::NAN))));
    }
    c.heading("P value");
    c.row(if spec.one_tailed { "P (one-tailed)" } else { "P (two-tailed)" }, each(&res, |r| f.p(r.p)));
    if !pearson {
        c.row("Exact or approximate P value?", each(&res, |r| if r.exact == Some(true) { "Exact" } else { "Approximate" }.to_string()));
    }
    c.row("P value summary", each(&res, |r| if r.p < 0.05 { f.stars(r.p) } else { "ns".into() }));
    c.row("Significant? (alpha = 0.05)", each(&res, |r| if r.p < 0.05 { "Yes" } else { "No" }.to_string()));
    c.heading("Data");
    c.row("Number of XY Pairs", each(&res, |r| r.n.to_string()));
    failures(&mut c, &res);
    let info = vec![crate::column_tests::r("Table analyzed", format!("{} (X and Y of graph \"{}\")", table.name, graph.name))];
    Ok(Report {
        title: if pearson { "Correlation (Pearson)" } else { "Correlation (Spearman)" }.into(),
        sections: vec![crate::column_tests::kv("Correlation", info), c.section("Table of results")],
        brackets: Vec::new(),
        curves: Vec::new(),
        engine,
    })
}

// ---------------------------------------------------------------------------
// Area under the curve
// ---------------------------------------------------------------------------

pub fn area_under_curve(project: &Project, spec: &AucSpec) -> Result<Report, EngineError> {
    let (sets, graph, table, _) = chosen(project, &spec.graph_id, &spec.data_sets)?;
    let f = fmt(&spec.output);
    let (res, engine) = per_set::<rr::Auc>(&sets, |s| {
        Request::new(
            "auc",
            json!({ "x": s.x, "y": s.values }),
            json!({ "baseline": spec.baseline, "min_peak_percent": spec.min_peak_percent, "peaks_below": spec.peaks_below, "confidence": spec.confidence }),
        )
    })?;
    let mut c = Columns::new("Area under curve", &sets);
    c.row("Baseline", each(&res, |r| f.n(r.baseline)));
    c.row("Total Area", each(&res, |r| f.n(r.total_area)));
    c.row("Std. Error", each(&res, |r| if r.se.is_finite() { f.n(r.se) } else { "(needs replicates at every X)".into() }));
    c.row(&format!("{} Confidence Interval", conf_label(spec.confidence)), each(&res, |r| r.ci.map(|ci| f.ci(ci)).unwrap_or_default()));
    c.row("Degrees of freedom of CI", each(&res, |r| if r.se.is_finite() { format_sig(r.df, 6) } else { String::new() }));
    c.row("Total Peak Area", each(&res, |r| f.n(r.total_peak_area)));
    c.row("Number of Peaks", each(&res, |r| r.peaks.len().to_string()));
    c.row("# of X values", each(&res, |r| r.n_x.to_string()));
    c.row("# Y values analyzed", each(&res, |r| r.n_values.to_string()));
    failures(&mut c, &res);
    let info = vec![
        crate::column_tests::r("Table analyzed", format!("{} (data sets of graph \"{}\")", table.name, graph.name)),
        crate::column_tests::r("Method", "Trapezoid rule on the mean Y at each X; SE from the replicates (Gagnon & Peterson 1998)"),
        crate::column_tests::r("Baseline", f.n(spec.baseline)),
        crate::column_tests::r(
            "Peaks",
            format!(
                "regions between baseline crossings, ignoring peaks lower than {}% of the Y range{}",
                format_sig(spec.min_peak_percent, 3),
                if spec.peaks_below { "; peaks below the baseline included" } else { "; peaks above the baseline only" }
            ),
        ),
    ];
    let mut sections = vec![crate::column_tests::kv("Area under curve", info), c.section("Table of results")];
    for (s, r) in sets.iter().zip(&res) {
        let Ok(r) = r else { continue };
        if r.peaks.is_empty() {
            continue;
        }
        sections.push(Section {
            title: format!("Peaks of {}", s.label),
            table: TextTable {
                headers: ["Peak", "First X", "Last X", "Peak X", "Peak Y", "Area", "Percentage of total peak area"].map(String::from).to_vec(),
                rows: r
                    .peaks
                    .iter()
                    .enumerate()
                    .map(|(i, p)| vec![(i + 1).to_string(), f.n(p.start), f.n(p.end), f.n(p.peak_x), f.n(p.peak_y), f.n(p.area), format!("{}%", f.n(p.percent))])
                    .collect(),
            },
        });
    }
    Ok(Report { title: "Area under curve".into(), sections, brackets: Vec::new(), curves: Vec::new(), engine })
}
