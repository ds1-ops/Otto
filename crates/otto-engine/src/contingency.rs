//! Contingency table analysis: the counts of a contingency graph go to R, which runs the
//! chi-square test with and without Yates' correction, Fisher's exact test, McNemar's test
//! for a paired design, the chi-square test for trend, and — for a 2 x 2 table — the
//! relative risk, the odds ratio, the difference between proportions and the diagnostic
//! ratios, reported the way Prism reports them.

use serde_json::json;

use otto_core::analysis::{format_sig, ContingencySpec};
use otto_core::{DataTable, GraphKind, GraphSpec, Project};
use otto_plot::TextTable;
use otto_r::results as rr;
use otto_r::Request;

use crate::column_tests::{graph_and_table, kv, r, r_call, yes_no, Fmt};
use crate::results::{Report, Section};
use crate::EngineError;

/// Row labels, column labels and the count in each cell.
pub type CountTable = (Vec<String>, Vec<String>, Vec<Vec<f64>>);

/// The counts of a contingency graph. An empty cell counts as zero, which is what a
/// contingency table means by it.
pub fn count_table(g: &GraphSpec, table: &DataTable) -> Result<CountTable, EngineError> {
    if g.kind != GraphKind::Contingency {
        return Err(EngineError::Invalid(format!(
            "\"{}\" is not a contingency table; this analysis needs one (rows = X, columns = Group by or several Y columns, cells = counts)",
            g.name
        )));
    }
    let data = otto_plot::distill::distill(g, table)?;
    let m = otto_plot::contingency::counts(&data);
    let counts = m.iter().map(|row| row.iter().map(|v| v.unwrap_or(0.0)).collect()).collect();
    Ok((data.x_levels.clone(), data.sets.clone(), counts))
}

pub fn contingency(project: &Project, spec: &ContingencySpec) -> Result<Report, EngineError> {
    let (g, table) = graph_and_table(project, &spec.graph_id)?;
    let (rows, all_cols, counts) = count_table(g, table)?;

    // Keeping only some columns is how a 2 x 2 is picked out of a wider table.
    let keep: Vec<usize> = if spec.data_sets.is_empty() {
        (0..all_cols.len()).collect()
    } else {
        spec.data_sets
            .iter()
            .map(|w| {
                all_cols
                    .iter()
                    .position(|c| c == w || c.eq_ignore_ascii_case(w.trim()))
                    .ok_or_else(|| EngineError::NotFound(format!("column '{w}' (available: {})", all_cols.join(", "))))
            })
            .collect::<Result<_, _>>()?
    };
    let cols: Vec<String> = keep.iter().map(|&c| all_cols[c].clone()).collect();
    // Column-major, as R's matrix() reads it.
    let mut flat: Vec<f64> = Vec::with_capacity(rows.len() * keep.len());
    for &c in &keep {
        for row in &counts {
            flat.push(row[c]);
        }
    }

    let req = Request::new(
        "contingency",
        json!({ "counts": flat, "nrow": rows.len(), "ncol": cols.len(), "rows": rows, "cols": cols }),
        json!({ "paired": spec.paired, "confidence": spec.confidence }),
    );
    let (res, engine): (rr::Contingency, String) = r_call(&req)?;

    let f = Fmt { digits: spec.output.digits, style: spec.output.p_style };
    let conf_pct = format!("{}%", format_sig(spec.confidence * 100.0, 3));
    let sig = |p: f64| if p < 0.05 { f.stars(p) } else { "ns".to_string() };
    let ci = |c: Option<(f64, f64)>| c.map(|c| f.ci(c)).unwrap_or_default();
    let pct = |v: f64| if v.is_finite() { format!("{}%", f.n(100.0 * v)) } else { String::new() };
    // A proportion and its interval both read as percentages, so both carry the sign.
    let pct_ci = |c: Option<(f64, f64)>| match c {
        Some((lo, hi)) => format!("{}% to {}%", f.n(100.0 * lo), f.n(100.0 * hi)),
        None => String::new(),
    };

    let shape = format!("{} x {}", res.nrow, res.ncol);
    let mut sections = vec![kv(
        "Contingency table",
        vec![
            r("Table analyzed", format!("{} (counts of graph \"{}\")", table.name, g.name)),
            r("Table size", format!("{shape} ({} rows x {} columns)", res.nrow, res.ncol)),
            r("Rows", res.rows.join(", ")),
            r("Columns", res.cols.join(", ")),
            r("Total subjects", format_sig(res.n, 9)),
            r("Design", if spec.paired { "Paired (the same subjects twice)" } else { "Unpaired (independent subjects)" }),
        ],
    )];

    // The counts themselves, with their totals, so the results stand on their own.
    let mut headers = vec![String::new()];
    headers.extend(res.cols.iter().cloned());
    headers.push("Total".into());
    let whole = |v: f64| format_sig(v, 9);
    let mut count_rows: Vec<Vec<String>> = rows
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let mut row = vec![label.clone()];
            row.extend(keep.iter().map(|&c| whole(counts[i][c])));
            row.push(whole(res.row_totals.get(i).copied().unwrap_or(0.0)));
            row
        })
        .collect();
    let mut totals = vec!["Total".to_string()];
    totals.extend(res.col_totals.iter().map(|&v| whole(v)));
    totals.push(whole(res.n));
    count_rows.push(totals);
    sections.push(Section { title: "Counts".into(), table: TextTable { headers: headers.clone(), rows: count_rows } });

    // The tests that apply to this table's shape.
    let test = |rows: &mut Vec<(String, String)>, title: &str, t: &rr::ChiTest, note: &str| {
        rows.push(r(title, ""));
        rows.push(r("Chi square", f.n(t.chisq)));
        rows.push(r("df", format_sig(t.df, 3)));
        rows.push(r("P value", f.p(t.p)));
        rows.push(r("P value summary", sig(t.p)));
        if !note.is_empty() {
            rows.push(r("Assumption", note.to_string()));
        }
        rows.push(r("", ""));
    };
    let mut rows_out = Vec::new();
    test(&mut rows_out, "Chi-square test", &res.chi_square, "");
    if let Some(t) = &res.chi_square_yates {
        test(&mut rows_out, "Chi-square test with Yates' correction", t, "");
    }
    if let Some(fi) = &res.fisher {
        rows_out.push(r("Fisher's exact test", ""));
        rows_out.push(r("P value", f.p(fi.p)));
        rows_out.push(r("P value summary", sig(fi.p)));
        if fi.simulated {
            // Said out loud: an exact test that was not exact must not read like one.
            rows_out.push(r("Note", "P value estimated by simulation: the table is too large for the exact computation".to_string()));
        }
        rows_out.push(r("", ""));
    }
    if let Some(t) = &res.mcnemar {
        test(&mut rows_out, "McNemar's test (paired)", t, "");
    }
    if let Some(t) = &res.trend {
        let along = res.trend_along.as_deref().unwrap_or("columns");
        test(&mut rows_out, "Chi-square test for trend", t, &format!("The {along} are ordered and equally spaced"));
    }
    // Prism's own caution, with the number behind it.
    rows_out.push(r("Smallest expected count", f.n(res.min_expected)));
    if res.min_expected < 5.0 {
        rows_out.push(r(
            "Caution",
            "An expected count below 5 makes the chi-square approximation unreliable; use Fisher's exact test".to_string(),
        ));
    }
    rows_out.push(r("Are the rows and columns associated?", yes_no(res.chi_square.p < 0.05)));
    sections.push(kv("Statistical tests", rows_out));

    if let Some(e) = &res.effect {
        let (r1, r2) = (rows.first().cloned().unwrap_or_default(), rows.get(1).cloned().unwrap_or_default());
        let c1 = res.cols.first().cloned().unwrap_or_default();
        sections.push(kv(
            "Effect size",
            vec![
                r(format!("Fraction of \"{c1}\" in {r1}"), format!("{} ({conf_pct} CI {})", pct(e.p1), pct_ci(e.p1_ci))),
                r(format!("Fraction of \"{c1}\" in {r2}"), format!("{} ({conf_pct} CI {})", pct(e.p2), pct_ci(e.p2_ci))),
                r("", ""),
                r(format!("Relative risk ({r1} / {r2})"), f.n(e.relative_risk)),
                r(format!("{conf_pct} CI of relative risk"), ci(e.relative_risk_ci)),
                r("", ""),
                r(format!("Odds ratio ({r1} / {r2})"), f.n(e.odds_ratio)),
                r(format!("{conf_pct} CI of odds ratio"), ci(e.odds_ratio_ci)),
                r("", ""),
                r("Difference between fractions", pct(e.difference)),
                r(format!("{conf_pct} CI of difference"), pct_ci(e.difference_ci)),
            ],
        ));
    }

    if let Some(d) = &res.diagnostic {
        let c1 = res.cols.first().cloned().unwrap_or_default();
        let r1 = rows.first().cloned().unwrap_or_default();
        let as_pct = |v: f64, c: Option<(f64, f64)>| format!("{} ({conf_pct} CI {})", pct(v), pct_ci(c));
        sections.push(kv(
            "Diagnostic ratios",
            vec![
                // The convention is stated, because these four swap round if it is read the other way.
                r("Convention", format!("\"{c1}\" is the condition being present and \"{r1}\" is the test calling it present")),
                r("", ""),
                r("Sensitivity", as_pct(d.sensitivity, d.sensitivity_ci)),
                r("Specificity", as_pct(d.specificity, d.specificity_ci)),
                r("Positive predictive value", as_pct(d.ppv, d.ppv_ci)),
                r("Negative predictive value", as_pct(d.npv, d.npv_ci)),
                r("Likelihood ratio (positive test)", f.n(d.likelihood_positive)),
                r("Likelihood ratio (negative test)", f.n(d.likelihood_negative)),
            ],
        ));
    }

    if spec.expected && res.expected.len() == res.nrow * res.ncol {
        let rows_e: Vec<Vec<String>> = rows
            .iter()
            .enumerate()
            .map(|(i, label)| {
                let mut row = vec![label.clone()];
                row.extend((0..res.ncol).map(|c| f.n(res.expected[i * res.ncol + c])));
                row
            })
            .collect();
        let mut h = vec![String::new()];
        h.extend(res.cols.iter().cloned());
        sections.push(Section { title: "Expected counts (if the rows and columns were independent)".into(), table: TextTable { headers: h, rows: rows_e } });
    }

    Ok(Report { title: "Contingency table analysis".into(), sections, brackets: Vec::new(), curves: Vec::new(), engine })
}
