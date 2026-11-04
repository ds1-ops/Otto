//! Normality diagnostics and outlier identification for the data sets of a graph
//! (Prism's "Normality and Lognormality Tests" and "Identify outliers"). Both are
//! computed by R; Otto only lays the results out.

use otto_core::analysis::{format_sig, NormalitySpec, NormalityTests, OutlierMethod, OutlierSpec};
use otto_core::Project;
use otto_plot::TextTable;
use otto_r as rr;
use otto_r::Request;
use serde_json::json;

use crate::column_tests::{graph_data_sets, kv, pick, r, r_call, source_rows, yes_no, Fmt};
use crate::results::{Report, Section};
use crate::EngineError;

/// Normality (and optionally lognormality) tests of each data set.
pub fn normality(project: &Project, spec: &NormalitySpec) -> Result<Report, EngineError> {
    let (all, graph, table) = graph_data_sets(project, &spec.graph_id)?;
    let sets = pick(all, &spec.data_sets)?;
    if sets.is_empty() {
        return Err(EngineError::Invalid("choose at least one data set".into()));
    }
    let f = Fmt { digits: spec.output.digits, style: spec.output.p_style };
    let tests = spec.chosen();
    let names = tests.names();
    let mut sections = Vec::new();
    let mut engine = String::new();

    let mut run = |values: Vec<Vec<f64>>| -> Result<Vec<Vec<rr::results::Normality>>, EngineError> {
        let req = Request::new("normality", json!({ "sets": values }), json!({ "tests": names }));
        let (res, eng): (Vec<Vec<rr::results::Normality>>, String) = r_call(&req)?;
        engine = eng;
        Ok(res)
    };

    let raw = run(sets.iter().map(|s| s.values.clone()).collect())?;
    sections.push(kv("Data analyzed", source_rows(table, graph)));

    let alpha = 0.05;
    for name in &names {
        let mut rows = Vec::new();
        for (set, results) in sets.iter().zip(&raw) {
            let r = results.iter().find(|r| r.test == *name);
            rows.push(match r {
                Some(r) if r.note.is_none() => vec![
                    set.label.clone(),
                    set.values.len().to_string(),
                    f.n(r.statistic),
                    f.p(r.p),
                    if r.p < alpha { f.stars(r.p) } else { "ns".into() },
                    yes_no(r.p >= alpha).to_string(),
                ],
                Some(r) => vec![set.label.clone(), set.values.len().to_string(), r.note.clone().unwrap_or_default(), String::new(), String::new(), String::new()],
                None => vec![set.label.clone(), set.values.len().to_string(), "not run".into(), String::new(), String::new(), String::new()],
            });
        }
        sections.push(Section {
            title: NormalityTests::label(name).to_string(),
            table: TextTable {
                headers: ["Data set", "N", "Statistic", "P value", "P value summary", format!("Passed normality (alpha = {})?", format_sig(alpha, 3)).as_str()]
                    .map(String::from)
                    .to_vec(),
                rows,
            },
        });
    }

    if spec.lognormal {
        // Lognormality: the same tests on log10 of the values (positive values only).
        let logged: Vec<Vec<f64>> = sets.iter().map(|s| s.values.iter().filter(|v| **v > 0.0).map(|v| v.log10()).collect()).collect();
        let dropped: usize = sets.iter().zip(&logged).map(|(s, l)| s.values.len() - l.len()).sum();
        let res = run(logged.clone())?;
        for name in &names {
            let mut rows = Vec::new();
            for ((set, values), results) in sets.iter().zip(&logged).zip(&res) {
                let r = results.iter().find(|r| r.test == *name);
                rows.push(match r {
                    Some(r) if r.note.is_none() => vec![
                        set.label.clone(),
                        values.len().to_string(),
                        f.n(r.statistic),
                        f.p(r.p),
                        yes_no(r.p >= alpha).to_string(),
                    ],
                    Some(r) => vec![set.label.clone(), values.len().to_string(), r.note.clone().unwrap_or_default(), String::new(), String::new()],
                    None => vec![set.label.clone(), values.len().to_string(), "not run".into(), String::new(), String::new()],
                });
            }
            let mut title = format!("{} on log10 values (lognormality)", NormalityTests::label(name));
            if dropped > 0 {
                title = format!("{title} — {dropped} value(s) of 0 or less left out");
            }
            sections.push(Section {
                title,
                table: TextTable {
                    headers: ["Data set", "N", "Statistic", "P value", "Passed lognormality?"].map(String::from).to_vec(),
                    rows,
                },
            });
        }
    }

    Ok(Report { title: "Normality tests".into(), sections, brackets: Vec::new(), curves: Vec::new(), engine })
}

/// Outlier identification in each data set.
pub fn outliers(project: &Project, spec: &OutlierSpec) -> Result<Report, EngineError> {
    let (all, graph, table) = graph_data_sets(project, &spec.graph_id)?;
    let sets = pick(all, &spec.data_sets)?;
    if sets.is_empty() {
        return Err(EngineError::Invalid("choose at least one data set".into()));
    }
    if !(0.0001..0.5).contains(&spec.alpha) {
        return Err(EngineError::Invalid("the significance level must be between 0.0001 and 0.5".into()));
    }
    let f = Fmt { digits: spec.output.digits, style: spec.output.p_style };
    let method = match spec.method {
        OutlierMethod::Esd => "esd",
        OutlierMethod::Grubbs => "grubbs",
        OutlierMethod::Tukey => "tukey",
    };
    let values: Vec<Vec<f64>> = sets.iter().map(|s| s.values.clone()).collect();
    let req = Request::new(
        "outliers",
        json!({ "sets": values }),
        json!({ "method": method, "alpha": spec.alpha, "max_outliers": spec.max_outliers }),
    );
    let (res, engine): (Vec<rr::results::Outliers>, String) = r_call(&req)?;

    let mut rows = source_rows(table, graph);
    rows.push(r("Method", spec.method.label()));
    if spec.method != OutlierMethod::Tukey {
        rows.push(r("Significance level", format_sig(spec.alpha, 3)));
    }
    let mut sections = vec![kv("How it was done", rows)];

    // One row per data set: how many outliers it has.
    let mut summary = Vec::new();
    let mut detail = Vec::new();
    for (set, r) in sets.iter().zip(&res) {
        let count = r.outliers.unwrap_or(r.found.len());
        summary.push(vec![
            set.label.clone(),
            r.n.to_string(),
            if r.note.is_some() { "—".to_string() } else { count.to_string() },
            r.note.clone().unwrap_or_default(),
        ]);
        for (i, c) in r.found.iter().enumerate() {
            let is_outlier = match spec.method {
                OutlierMethod::Tukey => true,
                _ => i < count,
            };
            if !is_outlier && spec.method != OutlierMethod::Tukey {
                // Candidates past the last significant one are reported too, so the
                // borderline cases are visible rather than silently dropped.
            }
            detail.push(vec![
                set.label.clone(),
                f.n(c.value),
                if c.g.is_finite() { f.n(c.g) } else { String::new() },
                if c.critical.is_finite() { f.n(c.critical) } else { String::new() },
                if c.p.is_finite() { f.p(c.p) } else { String::new() },
                yes_no(is_outlier).to_string(),
            ]);
        }
    }
    sections.push(Section {
        title: "Outliers per data set".into(),
        table: TextTable { headers: ["Data set", "N", "Outliers", "Note"].map(String::from).to_vec(), rows: summary },
    });
    if !detail.is_empty() {
        sections.push(Section {
            title: if spec.method == OutlierMethod::Tukey { "Values outside the fences".into() } else { "Most extreme values, tested in turn".to_string() },
            table: TextTable {
                headers: ["Data set", "Value", "Grubbs' G", "Critical G", "P value", "Outlier?"].map(String::from).to_vec(),
                rows: detail,
            },
        });
    }
    Ok(Report { title: "Outlier identification".into(), sections, brackets: Vec::new(), curves: Vec::new(), engine })
}
