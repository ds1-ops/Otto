//! Kaplan-Meier survival analysis of a survival graph: each curve (group) is one data set
//! of subjects with a time and an event (1) or censoring (0). R's survival package
//! computes the survival proportions, medians and the comparisons of the curves.

use serde_json::json;

use otto_core::analysis::{format_sig, SurvivalSpec};
use otto_core::{DataTable, GraphKind, GraphSpec, Project};
use otto_plot::TextTable;
use otto_r::results as rr;
use otto_r::Request;

use crate::column_tests::{graph_and_table, kv, r, r_call, yes_no, DataSet, Fmt};
use crate::results::{Report, Section};
use crate::EngineError;

/// The curves of a survival graph: times in `x`, events (1 / 0) in `values`.
pub fn survival_sets(g: &GraphSpec, table: &DataTable) -> Result<Vec<DataSet>, EngineError> {
    if g.kind != GraphKind::Survival {
        return Err(EngineError::Invalid(format!(
            "\"{}\" is not a survival graph; Kaplan-Meier analysis needs a survival graph (X = time, Y = event 1 / censored 0, Group by = curves)",
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

pub fn survival(project: &Project, spec: &SurvivalSpec) -> Result<Report, EngineError> {
    let (g, table) = graph_and_table(project, &spec.graph_id)?;
    let all = survival_sets(g, table)?;
    let sets: Vec<DataSet> = if spec.data_sets.is_empty() {
        all
    } else {
        spec.data_sets
            .iter()
            .map(|w| {
                all.iter().find(|d| d.label == *w || d.label.eq_ignore_ascii_case(w.trim())).cloned().ok_or_else(|| {
                    EngineError::NotFound(format!("curve '{w}' (available: {})", all.iter().map(|d| d.label.as_str()).collect::<Vec<_>>().join(", ")))
                })
            })
            .collect::<Result<_, _>>()?
    };
    let (mut time, mut event, mut group): (Vec<f64>, Vec<f64>, Vec<usize>) = (Vec::new(), Vec::new(), Vec::new());
    for (i, s) in sets.iter().enumerate() {
        time.extend(&s.x);
        event.extend(&s.values);
        group.extend(std::iter::repeat_n(i, s.x.len()));
    }
    let labels: Vec<&String> = sets.iter().map(|s| &s.label).collect();
    let req = Request::new("survival", json!({ "time": time, "event": event, "group": group, "labels": labels }), json!({ "confidence": spec.confidence }));
    let (res, engine): (rr::Survival, String) = r_call(&req)?;

    let f = Fmt { digits: spec.output.digits, style: spec.output.p_style };
    let conf_pct = format!("{}%", format_sig(spec.confidence * 100.0, 3));
    let sig = |p: f64| if p < 0.05 { f.stars(p) } else { "ns".to_string() };
    let mut sections = vec![kv(
        "Kaplan-Meier survival analysis",
        vec![
            r("Table analyzed", format!("{} (curves of graph \"{}\")", table.name, g.name)),
            r("Curves", labels.iter().map(|l| l.as_str()).collect::<Vec<_>>().join(", ")),
            r("Confidence intervals", format!("{conf_pct}, asymmetrical (log-log transformation)")),
        ],
    )];
    if res.curves.len() > 1 {
        let mut rows = Vec::new();
        let test = |rows: &mut Vec<(String, String)>, title: &str, t: &rr::ChiTest| {
            rows.extend([
                r(title, ""),
                r("Chi square", f.n(t.chisq)),
                r("df", format_sig(t.df, 3)),
                r("P value", f.p(t.p)),
                r("P value summary", sig(t.p)),
                r("Are the survival curves sig different?", yes_no(t.p < 0.05)),
                r("", ""),
            ]);
        };
        if let Some(t) = &res.logrank {
            test(&mut rows, "Log-rank (Mantel-Cox) test", t);
        }
        if let Some(t) = &res.trend {
            test(&mut rows, "Logrank test for trend", t);
        }
        if let Some(t) = &res.gehan_breslow_wilcoxon {
            test(&mut rows, "Gehan-Breslow-Wilcoxon test", t);
        }
        rows.pop();
        sections.push(kv("Comparison of survival curves", rows));
    }
    let median = |c: &rr::SurvivalCurve| if c.median.is_finite() { f.n(c.median) } else { "Undefined".to_string() };
    let median_ci = |c: &rr::SurvivalCurve| match c.median_ci.as_slice() {
        [lo, hi] => format!("{} to {}", if lo.is_finite() { f.n(*lo) } else { "?".into() }, if hi.is_finite() { f.n(*hi) } else { "undefined".into() }),
        _ => String::new(),
    };
    sections.push(Section {
        title: "Median survival".into(),
        table: TextTable {
            headers: vec!["Curve".into(), "Median survival".into(), format!("{conf_pct} CI")],
            rows: res.curves.iter().map(|c| vec![c.label.clone(), median(c), median_ci(c)]).collect(),
        },
    });
    if let (Some(hr), [a, b]) = (&res.hazard_ratio, res.curves.as_slice()) {
        let pair = format!("{} / {}", a.label, b.label);
        let back = format!("{} / {}", b.label, a.label);
        let inv_ci = |ci: Option<(f64, f64)>| ci.map(|(lo, hi)| f.ci((1.0 / hi, 1.0 / lo))).unwrap_or_default();
        let mut rows = Vec::new();
        if a.median.is_finite() && b.median.is_finite() {
            rows.push(r(format!("Ratio of median survival ({pair})"), f.n(a.median / b.median)));
            rows.push(r("", ""));
        }
        rows.extend([
            r("Hazard Ratio (Mantel-Haenszel)", ""),
            r(format!("Ratio ({pair})"), f.n(hr.mantel_haenszel)),
            r(format!("{conf_pct} CI of ratio"), hr.mantel_haenszel_ci.map(|ci| f.ci(ci)).unwrap_or_default()),
            r(format!("Reciprocal ({back})"), f.n(1.0 / hr.mantel_haenszel)),
            r(format!("{conf_pct} CI of reciprocal"), inv_ci(hr.mantel_haenszel_ci)),
            r("", ""),
            r("Hazard Ratio (logrank)", ""),
            r(format!("Ratio ({pair})"), f.n(hr.logrank)),
            r(format!("{conf_pct} CI of ratio"), hr.logrank_ci.map(|ci| f.ci(ci)).unwrap_or_default()),
            r(format!("Reciprocal ({back})"), f.n(1.0 / hr.logrank)),
            r(format!("{conf_pct} CI of reciprocal"), inv_ci(hr.logrank_ci)),
        ]);
        sections.push(kv("Hazard ratio", rows));
    }
    sections.push(Section {
        title: "Data analyzed".into(),
        table: TextTable {
            headers: ["Curve", "Number of subjects", "Number of events", "Number censored"].map(String::from).to_vec(),
            rows: res.curves.iter().map(|c| vec![c.label.clone(), c.subjects.to_string(), c.events.to_string(), (c.subjects - c.events.min(c.subjects)).to_string()]).collect(),
        },
    });
    if spec.proportions {
        let mut rows = Vec::new();
        for c in &res.curves {
            let at = |v: &[f64], i: usize| v.get(i).copied().unwrap_or(f64::NAN);
            for i in 0..c.time.len() {
                rows.push(vec![
                    c.label.clone(),
                    f.n(at(&c.time, i)),
                    format_sig(at(&c.at_risk, i), 6),
                    format_sig(at(&c.n_event, i), 6),
                    format_sig(at(&c.n_censor, i), 6),
                    f.n(100.0 * at(&c.survival, i)),
                    f.n(100.0 * at(&c.se, i)),
                    f.n(100.0 * at(&c.lower, i)),
                    f.n(100.0 * at(&c.upper, i)),
                ]);
            }
        }
        sections.push(Section {
            title: "Survival proportions".into(),
            table: TextTable {
                headers: ["Curve", "Time", "At risk", "Events", "Censored", "Percent survival", "Standard error", "Lower CI", "Upper CI"].map(String::from).to_vec(),
                rows,
            },
        });
    }
    Ok(Report { title: "Kaplan-Meier survival analysis".into(), sections, brackets: Vec::new(), curves: Vec::new(), engine })
}
