//! Ordinary two-way ANOVA on the bars of a grouped graph: its X categories are the row
//! factor and its data sets the column factor (Prism's grouped table). R fits the model
//! and runs the comparisons; significant comparisons between bars are drawn as brackets.

use serde_json::json;

use otto_core::analysis::{format_sig, Correction, TwoWayCompare, TwoWaySpec};
use otto_core::{Project, COMBINE_SEP};
use otto_plot::{Bracket, TextTable};
use otto_r::results as rr;
use otto_r::Request;

use crate::column_tests::{graph_and_table, kv, r, r_call, yes_no, Fmt};
use crate::results::{Report, Section};
use crate::EngineError;

pub fn two_way_anova(project: &Project, spec: &TwoWaySpec) -> Result<Report, EngineError> {
    let (g, table) = graph_and_table(project, &spec.graph_id)?;
    if !g.kind.is_grouped() {
        return Err(EngineError::Invalid(format!(
            "\"{}\" is not a grouped graph; two-way ANOVA needs X categories (one factor) and data sets from Group by or several Y columns (the other factor)",
            g.name
        )));
    }
    let data = otto_plot::distill::distill(g, table)?;
    if data.sets.len() < 2 {
        return Err(EngineError::Invalid(format!(
            "\"{}\" has one data set; two-way ANOVA needs a second factor (Group by, or several Y columns)",
            g.name
        )));
    }
    // The bars to analyse, then the levels of both factors that they use (in graph order).
    let bars: Vec<_> = data
        .bar_order()
        .into_iter()
        .map(|i| &data.groups[i])
        .filter(|b| spec.data_sets.is_empty() || spec.data_sets.iter().any(|d| *d == data.group_label(b)))
        .collect();
    if bars.is_empty() {
        return Err(EngineError::NotFound(format!("none of the chosen bars ({}) is on the graph", spec.data_sets.join(", "))));
    }
    let mut a_idx: Vec<usize> = bars.iter().map(|b| b.x.num() as usize).collect();
    a_idx.sort_unstable();
    a_idx.dedup();
    let mut b_idx: Vec<usize> = bars.iter().map(|b| b.set).collect();
    b_idx.sort_unstable();
    b_idx.dedup();
    let a_levels: Vec<String> = a_idx.iter().map(|&i| data.x_levels[i].clone()).collect();
    let b_levels: Vec<String> = b_idx.iter().map(|&i| data.sets[i].clone()).collect();
    let (mut y, mut a, mut b) = (Vec::new(), Vec::new(), Vec::new());
    for bar in &bars {
        let ai = a_idx.iter().position(|&i| i == bar.x.num() as usize).unwrap_or(0);
        let bi = b_idx.iter().position(|&i| i == bar.set).unwrap_or(0);
        for p in data.points_of(bar) {
            y.push(p.y);
            a.push(ai);
            b.push(bi);
        }
    }
    let (na, nb) = (a_levels.len(), b_levels.len());
    let method = match spec.correction {
        Correction::None => "uncorrected",
        Correction::Fdr => "fdr",
        Correction::Hypothesis => match spec.compare {
            // One comparison per family: Šídák over all of them, as Prism does.
            TwoWayCompare::WithinRows if nb == 2 => "sidak",
            TwoWayCompare::WithinColumns if na == 2 => "sidak",
            _ => "tukey",
        },
    };
    let req = Request::new(
        "two_way",
        json!({ "y": y, "a": a, "b": b, "a_levels": a_levels, "b_levels": b_levels }),
        json!({ "interaction": spec.interaction, "compare": spec.compare.id(), "method": method, "alpha": spec.alpha }),
    );
    let (res, engine): (rr::TwoWay, String) = r_call(&req)?;

    let f = Fmt { digits: spec.output.digits, style: spec.output.p_style };
    let alpha = spec.alpha;
    let row_name = if data.x_name.is_empty() { "Row factor".to_string() } else { data.x_name.clone() };
    let col_name = data.set_name.clone().unwrap_or_else(|| "Column factor".to_string());
    let source = |s: &str| match s {
        "interaction" => "Interaction".to_string(),
        "rows" => row_name.clone(),
        "columns" => col_name.clone(),
        _ => "Residual".to_string(),
    };
    let sig = |p: f64| if p < alpha { f.stars(p) } else { "ns".to_string() };
    let info = vec![
        r("Table analyzed", format!("{} (bars of graph \"{}\")", table.name, g.name)),
        r("Two-way ANOVA", if spec.interaction { "Ordinary, full model (with interaction)" } else { "Ordinary, main effects only" }),
        r("Rows (X)", format!("{row_name}: {}", a_levels.join(", "))),
        r("Columns (data sets)", format!("{col_name}: {}", b_levels.join(", "))),
        r("Sums of squares", "Type III"),
        r("Alpha", format_sig(alpha, 3)),
    ];
    let mut sections = vec![kv("Two-way ANOVA", info)];
    sections.push(Section {
        title: "Source of Variation".into(),
        table: TextTable {
            headers: ["Source of Variation", "% of total variation", "P value", "P value summary", "Significant?"].map(String::from).to_vec(),
            rows: res
                .table
                .iter()
                .filter(|t| t.source != "residual")
                .map(|t| {
                    let p = t.p.unwrap_or(f64::NAN);
                    vec![source(&t.source), f.n(t.percent), f.p(p), sig(p), yes_no(p < alpha)]
                })
                .collect(),
        },
    });
    let resid_df = res.df_residual;
    sections.push(Section {
        title: "ANOVA table".into(),
        table: TextTable {
            headers: ["ANOVA table", "SS (Type III)", "DF", "MS", "F (DFn, DFd)", "P value"].map(String::from).to_vec(),
            rows: res
                .table
                .iter()
                .map(|t| {
                    let fcell = t.f.map(|fv| format!("F ({}, {}) = {}", format_sig(t.df, 6), format_sig(resid_df, 6), f.n(fv))).unwrap_or_default();
                    let pcell = t.p.map(|p| { let s = f.p(p); if s.starts_with('<') { format!("P{s}") } else { format!("P={s}") } }).unwrap_or_default();
                    vec![source(&t.source), f.n(t.ss), format_sig(t.df, 6), f.n(t.ms), fcell, pcell]
                })
                .collect(),
        },
    });

    // Multiple comparisons, grouped into families as Prism lists them.
    let mut brackets = Vec::new();
    if !res.comparisons.is_empty() {
        let label = |c: &[usize], kind: &str| -> String {
            match (kind, c) {
                ("rows", [i]) => a_levels[*i].clone(),
                ("columns", [j]) => b_levels[*j].clone(),
                (_, [i, j]) => match spec.compare {
                    TwoWayCompare::WithinRows => b_levels[*j].clone(),
                    TwoWayCompare::WithinColumns => a_levels[*i].clone(),
                    _ => format!("{}{COMBINE_SEP}{}", a_levels[*i], b_levels[*j]),
                },
                _ => String::new(),
            }
        };
        let fdr = method == "fdr";
        let test = match method {
            "tukey" => "Tukey's multiple comparisons test",
            "sidak" => "Šídák's multiple comparisons test",
            "fdr" => "Two-stage linear step-up procedure of Benjamini, Krieger and Yekutieli",
            _ => "Uncorrected Fisher's LSD",
        };
        let conf_pct = format!("{}%", format_sig((1.0 - alpha) * 100.0, 4));
        let mut headers = vec![test.to_string(), "Mean Diff.".into()];
        if !fdr {
            headers.push(format!("{conf_pct} CI of diff."));
        }
        headers.extend([if fdr { "Discovery?" } else { "Below threshold?" }, "Summary", if fdr { "q value" } else { "Adjusted P Value" }].map(String::from));
        let family_title = |c: &rr::TwoWayComparison| -> Option<String> {
            match spec.compare {
                TwoWayCompare::WithinRows => c.a.first().map(|&i| format!("{row_name}: {}", a_levels[i])),
                TwoWayCompare::WithinColumns => c.a.get(1).map(|&j| format!("{col_name}: {}", b_levels[j])),
                TwoWayCompare::RowMeans => Some(format!("{row_name} (main effect, averaged over {col_name})")),
                TwoWayCompare::ColumnMeans => Some(format!("{col_name} (main effect, averaged over {row_name})")),
                _ => None,
            }
        };
        let mut rows = Vec::new();
        let mut last_family: Option<String> = None;
        for c in &res.comparisons {
            if let Some(title) = family_title(c) {
                if last_family.as_ref() != Some(&title) {
                    let mut h = vec![title.clone()];
                    h.resize(headers.len(), String::new());
                    rows.push(h);
                    last_family = Some(title);
                }
            }
            let summary = if c.significant { f.stars(c.p_adjusted) } else { "ns".to_string() };
            let mut row = vec![format!("    {} vs. {}", label(&c.a, &c.kind), label(&c.b, &c.kind)), f.n(c.diff)];
            if !fdr {
                row.push(c.ci.map(|ci| f.ci(ci)).unwrap_or_default());
            }
            row.extend([yes_no(c.significant), summary.clone(), f.p(c.p_adjusted)]);
            rows.push(row);
            // Comparisons between two bars are drawn on the graph.
            if c.kind == "cells" && spec.graph.show && (c.significant || spec.graph.show_ns) {
                if let ([i1, j1], [i2, j2]) = (c.a.as_slice(), c.b.as_slice()) {
                    let bar = |i: usize, j: usize| format!("{}{COMBINE_SEP}{}", a_levels[i], b_levels[j]);
                    brackets.push(Bracket { a: bar(*i1, *j1), b: bar(*i2, *j2), text: summary });
                }
            }
        }
        sections.push(Section { title: "Multiple comparisons".into(), table: TextTable { headers, rows } });
        let families = {
            let mut fs: Vec<i64> = res.comparisons.iter().map(|c| c.family as i64).collect();
            fs.dedup();
            fs.len()
        };
        sections.push(kv(
            "Multiple comparisons details",
            vec![
                r("Number of families", families.to_string()),
                r("Number of comparisons per family", (res.comparisons.len() / families.max(1)).to_string()),
                r(if fdr { "Q (false discovery rate)" } else { "Alpha" }, format_sig(alpha, 3)),
                r("Residual mean square (from the ANOVA)", f.n(res.mse)),
                r("Residual DF", format_sig(resid_df, 6)),
            ],
        ));
    }
    sections.push(kv(
        "Data summary",
        vec![
            r("Number of rows (X categories)", na.to_string()),
            r("Number of columns (data sets)", nb.to_string()),
            r("Number of values", res.n.to_string()),
        ],
    ));
    if spec.results.descriptive {
        sections.push(Section {
            title: "Descriptive statistics".into(),
            table: TextTable {
                headers: vec![row_name.clone(), col_name.clone(), "N".into(), "Mean".into(), "SD".into()],
                rows: res
                    .cells
                    .iter()
                    .filter_map(|c| Some(vec![a_levels.get(c.a)?.clone(), b_levels.get(c.b)?.clone(), c.n.to_string(), f.n(c.mean), f.n(c.sd)]))
                    .collect(),
            },
        });
    }
    let title = if spec.interaction { "Two-way ANOVA (full model)" } else { "Two-way ANOVA (main effects only)" };
    Ok(Report { title: title.into(), sections, brackets, curves: Vec::new(), engine })
}
