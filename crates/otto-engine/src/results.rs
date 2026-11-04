//! Computes analysis results (the Results section): descriptive statistics, t tests,
//! one-way ANOVA and XY analyses. Every analysis produces a [`Report`] of titled tables,
//! plus the significance brackets or fitted curves it contributes to its graph.
//! Every test, fit and P value is computed by R (see `otto-r`); only descriptive
//! statistics (means, SDs) are computed here.

use otto_core::{AnalysisKind, AnalysisSpec, ColumnKind, Project};
use otto_plot::{Bracket, Curve, TextTable};

use crate::EngineError;

/// One titled table of a results sheet.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub title: String,
    pub table: TextTable,
}

/// A full results sheet.
#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    /// What was computed, e.g. "Ordinary one-way ANOVA".
    pub title: String,
    pub sections: Vec<Section>,
    /// Brackets to draw on the analysed graph.
    pub brackets: Vec<Bracket>,
    /// Fitted curves / regression lines to draw on the analysed (XY) graph.
    pub curves: Vec<Curve>,
    /// What computed the results, e.g. "R 4.6.1 · stats · minpack.lm 1.2-4" (empty for
    /// descriptive statistics, which Otto computes itself).
    pub engine: String,
}

impl Report {
    /// Plain-text version (sections as CSV) for MCP and the clipboard.
    pub fn to_text(&self) -> String {
        let mut out = format!("{}\n", self.title);
        if !self.engine.is_empty() {
            out.push_str(&format!("Computed with {}\n", self.engine));
        }
        for s in &self.sections {
            out.push_str(&format!("\n## {}\n", s.title));
            let keyed = s.table.headers.iter().all(|h| h.is_empty());
            if keyed {
                for row in &s.table.rows {
                    match (row.first(), row.get(1)) {
                        (Some(k), Some(v)) if !k.is_empty() && !v.is_empty() => out.push_str(&format!("{k}: {v}\n")),
                        (Some(k), _) if !k.is_empty() => out.push_str(&format!("{k}\n")),
                        _ => out.push('\n'),
                    }
                }
            } else {
                out.push_str(&s.table.to_csv());
            }
        }
        out
    }

    /// Number of data rows over all sections.
    pub fn row_count(&self) -> usize {
        self.sections.iter().map(|s| s.table.rows.len()).sum()
    }
}

fn num(v: f64) -> String {
    if v.is_finite() {
        let r = (v * 1e6).round() / 1e6;
        otto_core::table::format_number(r)
    } else {
        String::new()
    }
}

pub fn compute(project: &Project, spec: &AnalysisSpec) -> Result<Report, EngineError> {
    match &spec.kind {
        AnalysisKind::TTest(t) => crate::column_tests::t_test(project, t),
        AnalysisKind::OneWayAnova(a) => crate::column_tests::one_way_anova(project, a),
        AnalysisKind::CurveFit(f) => crate::xy_analyses::curve_fit(project, f),
        AnalysisKind::LinearRegression(l) => crate::xy_analyses::linear_regression(project, l),
        AnalysisKind::Correlation(c) => crate::xy_analyses::correlation(project, c),
        AnalysisKind::TwoWayAnova(t) => crate::two_way::two_way_anova(project, t),
        AnalysisKind::Auc(a) => crate::xy_analyses::area_under_curve(project, a),
        AnalysisKind::Survival(s) => crate::survival::survival(project, s),
        AnalysisKind::Contingency(c) => crate::contingency::contingency(project, c),
        AnalysisKind::Normality(n) => crate::diagnostics::normality(project, n),
        AnalysisKind::Outliers(o) => crate::diagnostics::outliers(project, o),
        AnalysisKind::Descriptive { y, group } => {
            let table = project
                .table_by_id(&spec.table_id)
                .ok_or_else(|| EngineError::NotFound(format!("table {} of analysis '{}'", spec.table_id, spec.name)))?;
            let gcol = match group {
                Some(g) => Some(table.require_column(g)?),
                None => None,
            };
            let levels = gcol.map(|g| g.levels()).unwrap_or_else(|| vec![String::new()]);
            let mut headers = vec!["Column".to_string()];
            if let Some(g) = gcol {
                headers.push(g.name.clone());
            }
            headers.extend(
                ["N", "Mean", "SD", "SEM", "Median", "25th percentile", "75th percentile", "Min", "Max", "Lower 95% CI", "Upper 95% CI"]
                    .map(String::from),
            );
            let mut rows = Vec::new();
            for name in y {
                let col = table.require_column(name)?;
                if col.kind() != ColumnKind::Numeric {
                    return Err(EngineError::Invalid(format!("column '{}' is not numeric", col.name)));
                }
                for level in &levels {
                    let values: Vec<f64> = (0..table.nrows)
                        .filter(|&r| match gcol {
                            Some(g) => {
                                let label = match g.kind() {
                                    ColumnKind::Numeric => g.number(r).map(otto_core::table::format_number),
                                    _ => g.label(r).map(str::to_string),
                                };
                                label.as_deref() == Some(level.as_str())
                            }
                            None => true,
                        })
                        .filter_map(|r| col.number(r))
                        .collect();
                    let mut row = vec![col.name.clone()];
                    if gcol.is_some() {
                        row.push(level.clone());
                    }
                    match otto_plot::describe::describe(&values) {
                        Some(d) => {
                            row.push(d.n.to_string());
                            row.extend([d.mean, d.sd, d.sem, d.median, d.q1, d.q3, d.min, d.max, d.ci95_low, d.ci95_high].map(num));
                        }
                        None => {
                            row.push("0".into());
                            row.extend(std::iter::repeat_n(String::new(), 10));
                        }
                    }
                    rows.push(row);
                }
            }
            Ok(Report {
                title: "Descriptive statistics".into(),
                sections: vec![Section { title: "Descriptive statistics".into(), table: TextTable { headers, rows } }],
                brackets: Vec::new(),
                curves: Vec::new(),
                engine: String::new(),
            })
        }
    }
}
