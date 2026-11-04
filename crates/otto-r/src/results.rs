//! R's results, as the R code returns them (see `r/*.R`). Numbers R reports as missing
//! (not finite) arrive as NaN.

use serde::Deserialize;

use crate::de;

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct FTest {
    #[serde(deserialize_with = "de::num")]
    pub f: f64,
    #[serde(deserialize_with = "de::num")]
    pub dfn: f64,
    #[serde(deserialize_with = "de::num")]
    pub dfd: f64,
    #[serde(deserialize_with = "de::num")]
    pub p: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct EffectSizes {
    #[serde(deserialize_with = "de::num")]
    pub cohens_d: f64,
    #[serde(deserialize_with = "de::num")]
    pub hedges_g: f64,
    #[serde(default)]
    pub glass_delta: Option<f64>,
}

/// AICc of the simpler model (one mean / no difference) and the more complex one.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct Aicc {
    #[serde(deserialize_with = "de::num")]
    pub simpler: f64,
    #[serde(deserialize_with = "de::num")]
    pub complex: f64,
    #[serde(deserialize_with = "de::num")]
    pub prob_complex: f64,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Extra {
    pub label: String,
    #[serde(deserialize_with = "de::num")]
    pub value: f64,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct HodgesLehmann {
    #[serde(deserialize_with = "de::num")]
    pub estimate: f64,
    #[serde(default, deserialize_with = "de::interval")]
    pub ci: Option<(f64, f64)>,
}

/// One normality test of a set of values (or residuals).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Normality {
    /// "dagostino", "anderson", "shapiro" or "ks".
    pub test: String,
    #[serde(default = "de::nan", deserialize_with = "de::num")]
    pub statistic: f64,
    #[serde(default = "de::nan", deserialize_with = "de::num")]
    pub p: f64,
    /// Why the test could not run (e.g. too few values).
    #[serde(default)]
    pub note: Option<String>,
}

/// One candidate outlier of a data set, most extreme first.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct OutlierCandidate {
    #[serde(default = "de::nan", deserialize_with = "de::num")]
    pub value: f64,
    /// Grubbs' statistic (NaN for Tukey's rule).
    #[serde(default = "de::nan", deserialize_with = "de::num")]
    pub g: f64,
    #[serde(default = "de::nan", deserialize_with = "de::num")]
    pub critical: f64,
    #[serde(default = "de::nan", deserialize_with = "de::num")]
    pub p: f64,
    #[serde(default)]
    pub significant: Option<bool>,
}

/// Outliers found in one data set.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Outliers {
    pub test: String,
    pub n: usize,
    /// How many of `found` are outliers (Grubbs / ESD); Tukey marks all of them.
    #[serde(default)]
    pub outliers: Option<usize>,
    /// Tukey's fences.
    #[serde(default = "de::nan", deserialize_with = "de::num")]
    pub lower: f64,
    #[serde(default = "de::nan", deserialize_with = "de::num")]
    pub upper: f64,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub found: Vec<OutlierCandidate>,
}

/// A two-group comparison. Differences are a - b (ratios a / b for lognormal tests).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct TwoGroup {
    pub test: String,
    /// "t", "U" or "W".
    pub statistic_name: String,
    #[serde(deserialize_with = "de::num")]
    pub statistic: f64,
    #[serde(default)]
    pub df: Option<f64>,
    #[serde(deserialize_with = "de::num")]
    pub p_two_tailed: f64,
    #[serde(default)]
    pub exact: Option<bool>,
    #[serde(deserialize_with = "de::num")]
    pub diff: f64,
    #[serde(default)]
    pub se: Option<f64>,
    #[serde(default, deserialize_with = "de::interval")]
    pub ci: Option<(f64, f64)>,
    #[serde(default)]
    pub ratio: bool,
    #[serde(default)]
    pub r_squared: Option<f64>,
    #[serde(default)]
    pub f_test: Option<FTest>,
    #[serde(default)]
    pub effect: Option<EffectSizes>,
    #[serde(default)]
    pub aicc: Option<Aicc>,
    #[serde(default)]
    pub hodges_lehmann: Option<HodgesLehmann>,
    #[serde(default)]
    pub extra: Vec<Extra>,
    #[serde(default)]
    pub normality: Vec<Normality>,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct FResult {
    #[serde(deserialize_with = "de::num")]
    pub f: f64,
    #[serde(deserialize_with = "de::num")]
    pub df1: f64,
    #[serde(deserialize_with = "de::num")]
    pub df2: f64,
    #[serde(deserialize_with = "de::num")]
    pub p: f64,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct AnovaRow {
    pub source: String,
    #[serde(deserialize_with = "de::num")]
    pub ss: f64,
    #[serde(deserialize_with = "de::num")]
    pub df: f64,
    #[serde(default)]
    pub ms: Option<f64>,
    #[serde(default)]
    pub f: Option<f64>,
    #[serde(default)]
    pub p: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Bartlett {
    #[serde(deserialize_with = "de::num")]
    pub statistic: f64,
    #[serde(deserialize_with = "de::num")]
    pub df: f64,
    #[serde(deserialize_with = "de::num")]
    pub p: f64,
}

/// One multiple comparison: data sets `a` and `b` (indices), difference a - b.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Comparison {
    pub a: usize,
    pub b: usize,
    #[serde(deserialize_with = "de::num")]
    pub diff: f64,
    #[serde(default, deserialize_with = "de::interval")]
    pub ci: Option<(f64, f64)>,
    #[serde(deserialize_with = "de::num")]
    pub p_adjusted: f64,
    #[serde(deserialize_with = "de::num")]
    pub p_raw: f64,
    pub significant: bool,
}

/// One-way ANOVA results; which fields are set depends on the design.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct Anova {
    // Ordinary.
    #[serde(default)]
    pub f: Option<FResult>,
    #[serde(default)]
    pub r_squared: Option<f64>,
    #[serde(default)]
    pub table: Vec<AnovaRow>,
    #[serde(default)]
    pub mse: Option<f64>,
    #[serde(default)]
    pub brown_forsythe_sd: Option<FResult>,
    #[serde(default)]
    pub bartlett: Option<Bartlett>,
    #[serde(default)]
    pub eta_squared: Option<f64>,
    #[serde(default)]
    pub omega_squared: Option<f64>,
    #[serde(default)]
    pub aicc: Option<Aicc>,
    // Brown-Forsythe and Welch.
    #[serde(default)]
    pub brown_forsythe: Option<FResult>,
    #[serde(default)]
    pub welch: Option<FResult>,
    // Kruskal-Wallis.
    #[serde(default)]
    pub h: Option<f64>,
    #[serde(default)]
    pub p: Option<f64>,
    #[serde(default, deserialize_with = "de::nums")]
    pub mean_ranks: Vec<f64>,
    #[serde(default, deserialize_with = "de::nums")]
    pub rank_sums: Vec<f64>,
    #[serde(default)]
    pub n_total: Option<f64>,
    #[serde(default)]
    pub epsilon_squared: Option<f64>,
    #[serde(default)]
    pub kendalls_w: Option<f64>,
    // Repeated measures.
    #[serde(default)]
    pub epsilon: Option<f64>,
    #[serde(default)]
    pub f_gg: Option<FResult>,
    #[serde(default)]
    pub subjects: Option<serde_json::Value>,
    // Friedman.
    #[serde(default)]
    pub q: Option<f64>,
    #[serde(default)]
    pub comparisons: Vec<Comparison>,
    #[serde(default)]
    pub normality: Vec<Normality>,
}

impl Anova {
    /// RM ANOVA: the "was the matching effective" F test.
    pub fn subjects_f(&self) -> Option<FResult> {
        self.subjects.as_ref().and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Friedman: the number of subjects.
    pub fn subject_count(&self) -> Option<usize> {
        self.subjects.as_ref().and_then(|v| v.as_u64()).map(|n| n as usize)
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Estimate {
    #[serde(deserialize_with = "de::num")]
    pub value: f64,
    #[serde(default = "de::nan", deserialize_with = "de::num")]
    pub se: f64,
    #[serde(default, deserialize_with = "de::interval")]
    pub ci: Option<(f64, f64)>,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct Curve {
    #[serde(default, deserialize_with = "de::nums")]
    pub x: Vec<f64>,
    #[serde(default, deserialize_with = "de::nums")]
    pub y: Vec<f64>,
}

impl Curve {
    pub fn points(&self) -> Vec<(f64, f64)> {
        self.x.iter().copied().zip(self.y.iter().copied()).filter(|(x, y)| x.is_finite() && y.is_finite()).collect()
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct LinearRegression {
    pub slope: Estimate,
    pub intercept: Estimate,
    #[serde(deserialize_with = "de::num")]
    pub x_intercept: f64,
    #[serde(deserialize_with = "de::num")]
    pub inv_slope: f64,
    #[serde(deserialize_with = "de::num")]
    pub r_squared: f64,
    #[serde(deserialize_with = "de::num")]
    pub syx: f64,
    #[serde(deserialize_with = "de::num")]
    pub f: f64,
    #[serde(deserialize_with = "de::num")]
    pub dfn: f64,
    #[serde(deserialize_with = "de::num")]
    pub dfd: f64,
    #[serde(deserialize_with = "de::num")]
    pub p: f64,
    pub n: usize,
    #[serde(deserialize_with = "de::num")]
    pub ss: f64,
    #[serde(default)]
    pub curve: Curve,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Correlation {
    pub method: String,
    #[serde(deserialize_with = "de::num")]
    pub r: f64,
    #[serde(default, deserialize_with = "de::interval")]
    pub ci: Option<(f64, f64)>,
    #[serde(default)]
    pub r_squared: Option<f64>,
    #[serde(deserialize_with = "de::num")]
    pub p: f64,
    pub n: usize,
    #[serde(default)]
    pub exact: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct FitParam {
    pub name: String,
    #[serde(deserialize_with = "de::num")]
    pub value: f64,
    #[serde(default = "de::nan", deserialize_with = "de::num")]
    pub se: f64,
    #[serde(default, deserialize_with = "de::interval")]
    pub ci: Option<(f64, f64)>,
    /// Constrained to a constant (not fitted).
    #[serde(default)]
    pub fixed: bool,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Derived {
    pub name: String,
    #[serde(deserialize_with = "de::num")]
    pub value: f64,
    #[serde(default, deserialize_with = "de::interval")]
    pub ci: Option<(f64, f64)>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct CurveFit {
    pub model: String,
    pub params: Vec<FitParam>,
    #[serde(default)]
    pub derived: Vec<Derived>,
    #[serde(deserialize_with = "de::num")]
    pub r_squared: f64,
    #[serde(deserialize_with = "de::num")]
    pub syx: f64,
    #[serde(deserialize_with = "de::num")]
    pub ss: f64,
    #[serde(deserialize_with = "de::num")]
    pub df: f64,
    pub n: usize,
    #[serde(default)]
    pub converged: bool,
    #[serde(default)]
    pub curve: Curve,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct TwoWayRow {
    /// "interaction", "rows", "columns" or "residual".
    pub source: String,
    #[serde(deserialize_with = "de::num")]
    pub ss: f64,
    #[serde(deserialize_with = "de::num")]
    pub df: f64,
    #[serde(deserialize_with = "de::num")]
    pub ms: f64,
    #[serde(default)]
    pub f: Option<f64>,
    #[serde(default)]
    pub p: Option<f64>,
    #[serde(deserialize_with = "de::num")]
    pub percent: f64,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct TwoWayCell {
    pub a: usize,
    pub b: usize,
    pub n: usize,
    #[serde(deserialize_with = "de::num")]
    pub mean: f64,
    #[serde(default = "de::nan", deserialize_with = "de::num")]
    pub sd: f64,
}

/// One comparison after a two-way ANOVA. `kind` "cells": `a` and `b` are [row, column]
/// indices of two bars; "rows" / "columns": a single row or column index each.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct TwoWayComparison {
    #[serde(deserialize_with = "de::num")]
    pub family: f64,
    pub kind: String,
    pub a: Vec<usize>,
    pub b: Vec<usize>,
    #[serde(deserialize_with = "de::num")]
    pub diff: f64,
    #[serde(default, deserialize_with = "de::interval")]
    pub ci: Option<(f64, f64)>,
    #[serde(deserialize_with = "de::num")]
    pub p_adjusted: f64,
    #[serde(deserialize_with = "de::num")]
    pub p_raw: f64,
    pub significant: bool,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct TwoWay {
    pub table: Vec<TwoWayRow>,
    #[serde(deserialize_with = "de::num")]
    pub mse: f64,
    #[serde(deserialize_with = "de::num")]
    pub df_residual: f64,
    pub n: usize,
    #[serde(default)]
    pub cells: Vec<TwoWayCell>,
    #[serde(default)]
    pub comparisons: Vec<TwoWayComparison>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Peak {
    #[serde(deserialize_with = "de::num")]
    pub start: f64,
    #[serde(deserialize_with = "de::num")]
    pub end: f64,
    #[serde(deserialize_with = "de::num")]
    pub peak_x: f64,
    #[serde(deserialize_with = "de::num")]
    pub peak_y: f64,
    #[serde(deserialize_with = "de::num")]
    pub area: f64,
    #[serde(default = "de::nan", deserialize_with = "de::num")]
    pub percent: f64,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Auc {
    #[serde(deserialize_with = "de::num")]
    pub baseline: f64,
    #[serde(deserialize_with = "de::num")]
    pub total_area: f64,
    #[serde(default = "de::nan", deserialize_with = "de::num")]
    pub se: f64,
    #[serde(default, deserialize_with = "de::interval")]
    pub ci: Option<(f64, f64)>,
    #[serde(deserialize_with = "de::num")]
    pub df: f64,
    pub n_x: usize,
    pub n_values: usize,
    #[serde(deserialize_with = "de::num")]
    pub total_peak_area: f64,
    #[serde(default)]
    pub peaks: Vec<Peak>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct ChiTest {
    #[serde(deserialize_with = "de::num")]
    pub chisq: f64,
    #[serde(deserialize_with = "de::num")]
    pub df: f64,
    #[serde(deserialize_with = "de::num")]
    pub p: f64,
}

/// Fisher's exact test. `simulated` says the P value was estimated by simulation, which
/// happens only when the exact computation is beyond the memory R will give it.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct FisherExact {
    #[serde(deserialize_with = "de::num")]
    pub p: f64,
    #[serde(default)]
    pub simulated: bool,
    #[serde(default)]
    pub odds_ratio: Option<f64>,
    #[serde(default, deserialize_with = "de::interval")]
    pub odds_ratio_ci: Option<(f64, f64)>,
}

/// How the two rows of a 2 x 2 table differ: the proportion in each, and the three ways
/// of comparing them.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Effect {
    #[serde(deserialize_with = "de::num")]
    pub p1: f64,
    #[serde(default, deserialize_with = "de::interval")]
    pub p1_ci: Option<(f64, f64)>,
    #[serde(deserialize_with = "de::num")]
    pub p2: f64,
    #[serde(default, deserialize_with = "de::interval")]
    pub p2_ci: Option<(f64, f64)>,
    #[serde(deserialize_with = "de::num")]
    pub relative_risk: f64,
    #[serde(default, deserialize_with = "de::interval")]
    pub relative_risk_ci: Option<(f64, f64)>,
    #[serde(deserialize_with = "de::num")]
    pub odds_ratio: f64,
    #[serde(default, deserialize_with = "de::interval")]
    pub odds_ratio_ci: Option<(f64, f64)>,
    #[serde(deserialize_with = "de::num")]
    pub difference: f64,
    #[serde(default, deserialize_with = "de::interval")]
    pub difference_ci: Option<(f64, f64)>,
}

/// The diagnostic ratios of a 2 x 2 table, on the convention that the first column is the
/// condition being present and the first row is the test calling it present.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Diagnostic {
    #[serde(deserialize_with = "de::num")]
    pub sensitivity: f64,
    #[serde(default, deserialize_with = "de::interval")]
    pub sensitivity_ci: Option<(f64, f64)>,
    #[serde(deserialize_with = "de::num")]
    pub specificity: f64,
    #[serde(default, deserialize_with = "de::interval")]
    pub specificity_ci: Option<(f64, f64)>,
    #[serde(deserialize_with = "de::num")]
    pub ppv: f64,
    #[serde(default, deserialize_with = "de::interval")]
    pub ppv_ci: Option<(f64, f64)>,
    #[serde(deserialize_with = "de::num")]
    pub npv: f64,
    #[serde(default, deserialize_with = "de::interval")]
    pub npv_ci: Option<(f64, f64)>,
    #[serde(deserialize_with = "de::num")]
    pub likelihood_positive: f64,
    #[serde(deserialize_with = "de::num")]
    pub likelihood_negative: f64,
}

/// A contingency table analysis: the tests that apply to the table's shape, plus the
/// effect sizes a 2 x 2 table allows.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Contingency {
    pub rows: Vec<String>,
    pub cols: Vec<String>,
    pub nrow: usize,
    pub ncol: usize,
    pub n: f64,
    #[serde(deserialize_with = "de::num")]
    pub confidence: f64,
    #[serde(default, deserialize_with = "de::nums")]
    pub row_totals: Vec<f64>,
    #[serde(default, deserialize_with = "de::nums")]
    pub col_totals: Vec<f64>,
    pub chi_square: ChiTest,
    #[serde(default)]
    pub chi_square_yates: Option<ChiTest>,
    /// Expected counts under independence, row by row.
    #[serde(default, deserialize_with = "de::nums")]
    pub expected: Vec<f64>,
    #[serde(default, deserialize_with = "de::num")]
    pub min_expected: f64,
    #[serde(default)]
    pub fisher: Option<FisherExact>,
    #[serde(default)]
    pub mcnemar: Option<ChiTest>,
    #[serde(default)]
    pub trend: Option<ChiTest>,
    /// Which side the trend runs along: "rows" or "columns".
    #[serde(default)]
    pub trend_along: Option<String>,
    #[serde(default)]
    pub effect: Option<Effect>,
    #[serde(default)]
    pub diagnostic: Option<Diagnostic>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct SurvivalCurve {
    pub label: String,
    pub subjects: usize,
    pub events: usize,
    /// Median survival (NaN when the curve never falls to 50%).
    #[serde(default = "de::nan", deserialize_with = "de::num")]
    pub median: f64,
    #[serde(default, deserialize_with = "de::nums")]
    pub median_ci: Vec<f64>,
    #[serde(default, deserialize_with = "de::nums")]
    pub time: Vec<f64>,
    #[serde(default, deserialize_with = "de::nums")]
    pub at_risk: Vec<f64>,
    #[serde(default, deserialize_with = "de::nums")]
    pub n_event: Vec<f64>,
    #[serde(default, deserialize_with = "de::nums")]
    pub n_censor: Vec<f64>,
    /// Fraction surviving (0–1) after each time.
    #[serde(default, deserialize_with = "de::nums")]
    pub survival: Vec<f64>,
    #[serde(default, deserialize_with = "de::nums")]
    pub se: Vec<f64>,
    #[serde(default, deserialize_with = "de::nums")]
    pub lower: Vec<f64>,
    #[serde(default, deserialize_with = "de::nums")]
    pub upper: Vec<f64>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct HazardRatio {
    #[serde(deserialize_with = "de::num")]
    pub logrank: f64,
    #[serde(default, deserialize_with = "de::interval")]
    pub logrank_ci: Option<(f64, f64)>,
    #[serde(deserialize_with = "de::num")]
    pub mantel_haenszel: f64,
    #[serde(default, deserialize_with = "de::interval")]
    pub mantel_haenszel_ci: Option<(f64, f64)>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Survival {
    pub curves: Vec<SurvivalCurve>,
    #[serde(default)]
    pub logrank: Option<ChiTest>,
    #[serde(default)]
    pub gehan_breslow_wilcoxon: Option<ChiTest>,
    #[serde(default)]
    pub trend: Option<ChiTest>,
    #[serde(default)]
    pub hazard_ratio: Option<HazardRatio>,
}
