//! Parameters of the column analyses (t tests, one-way ANOVA), mirroring Prism's
//! parameter dialogs, plus P value styles. Data sets are the bars of a graph.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

fn yes() -> bool {
    true
}

fn conf95() -> f64 {
    0.95
}

fn alpha05() -> f64 {
    0.05
}

fn digits4() -> u8 {
    4
}

/// Experimental design of a t test.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Pairing {
    #[default]
    Unpaired,
    /// Values are matched by the `match_by` column (e.g. donor, mouse).
    Paired,
}

/// Experimental design of a one-way ANOVA.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Matching {
    /// No matching or pairing.
    #[default]
    Unmatched,
    /// Repeated measures: values are matched by the `match_by` column.
    Matched,
}

/// Distribution assumption.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Distribution {
    /// Normal (Gaussian): compare means.
    #[default]
    Normal,
    /// Lognormal: compare geometric means (tests run on log values).
    Lognormal,
    /// No assumption: rank-based (nonparametric) tests.
    Nonparametric,
}

/// Direction of reported differences.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DiffDirection {
    /// First data set minus second.
    #[default]
    AMinusB,
    /// Second data set minus first.
    BMinusA,
}

/// How P values and their stars are written (Prism's P value styles).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PStyle {
    /// GP: 0.1234 (ns), 0.0332 (*), 0.0021 (**), 0.0002 (***), <0.0001 (****).
    #[default]
    Gp,
    /// APA: .123 (ns), .033 (*), .002 (**), <.001 (***).
    Apa,
    /// NEJM: 0.12 (ns), 0.033 (*), 0.002 (**), <0.001 (***).
    Nejm,
    /// Plain decimals with the chosen number of digits.
    Decimal,
}

impl PStyle {
    pub const ALL: [PStyle; 4] = [PStyle::Gp, PStyle::Apa, PStyle::Nejm, PStyle::Decimal];

    pub fn label(self) -> &'static str {
        match self {
            PStyle::Gp => "GP: 0.1234 (ns), 0.0332 (*), 0.0021 (**), 0.0002 (***), <0.0001 (****)",
            PStyle::Apa => "APA: .123 (ns), .033 (*), .002 (**), <.001 (***)",
            PStyle::Nejm => "NEJM: 0.12 (ns), 0.033 (*), 0.002 (**), <0.001 (***)",
            PStyle::Decimal => "Decimal: 0.123456",
        }
    }

    /// Writes a P value in this style.
    pub fn format(self, p: f64, digits: u8) -> String {
        if !p.is_finite() {
            return String::new();
        }
        match self {
            PStyle::Gp => if p < 0.0001 { "<0.0001".into() } else { format!("{p:.4}") },
            PStyle::Apa => {
                if p < 0.001 {
                    "<.001".into()
                } else {
                    format!("{p:.3}").trim_start_matches('0').to_string()
                }
            }
            PStyle::Nejm => {
                if p < 0.001 {
                    "<0.001".into()
                } else if p >= 0.1 {
                    format!("{p:.2}")
                } else {
                    format!("{p:.3}")
                }
            }
            PStyle::Decimal => {
                let d = digits.clamp(2, 15) as usize;
                if p < 10f64.powi(-(d as i32)) {
                    format!("<{:.*}", d, 10f64.powi(-(d as i32)))
                } else {
                    format!("{p:.d$}")
                }
            }
        }
    }

    /// Asterisks for a P value ("ns" when not significant at 0.05).
    pub fn stars(self, p: f64) -> &'static str {
        let max4 = matches!(self, PStyle::Gp | PStyle::Decimal);
        if p < 0.0001 && max4 {
            "****"
        } else if p < 0.001 {
            "***"
        } else if p < 0.01 {
            "**"
        } else if p < 0.05 {
            "*"
        } else {
            "ns"
        }
    }
}

impl std::fmt::Display for PStyle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Writes a number with `digits` significant digits (Prism's output setting).
pub fn format_sig(v: f64, digits: u8) -> String {
    if !v.is_finite() {
        return String::new();
    }
    if v == 0.0 {
        return "0".into();
    }
    let digits = digits.clamp(1, 15) as i32;
    let mag = v.abs().log10().floor() as i32;
    // Very small or large numbers (e.g. an EC50 of 7.1e-7 M) in scientific notation.
    if !(-4..7).contains(&mag) {
        let s = format!("{:.*e}", (digits - 1) as usize, v);
        let (m, e) = s.split_once('e').unwrap_or((&s, "0"));
        let m = if m.contains('.') { m.trim_end_matches('0').trim_end_matches('.') } else { m };
        return format!("{m}e{e}");
    }
    let decimals = (digits - 1 - mag).max(0) as usize;
    let s = format!("{v:.decimals$}");
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

/// Optional extra results.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ExtraResults {
    /// Descriptive statistics for each data set.
    #[serde(default)]
    pub descriptive: bool,
    /// Compare models using AICc.
    #[serde(default)]
    pub aicc: bool,
    /// Standardized effect sizes (Cohen's d, Hedges' g, Glass's Δ; η² and ω² for ANOVA).
    #[serde(default)]
    pub effect_sizes: bool,
}

/// Normality tests of the residuals (Prism's Residuals tab). For t tests: of the values
/// minus their group mean (paired: of the differences); for ANOVA: of the residuals.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct NormalityTests {
    /// D'Agostino-Pearson omnibus normality test (n ≥ 8).
    #[serde(default)]
    pub dagostino: bool,
    /// Anderson-Darling test (n ≥ 8).
    #[serde(default)]
    pub anderson: bool,
    /// Shapiro-Wilk normality test (3 ≤ n ≤ 5000).
    #[serde(default)]
    pub shapiro: bool,
    /// Kolmogorov-Smirnov test with Lilliefors' correction (n ≥ 5).
    #[serde(default)]
    pub ks: bool,
}

impl NormalityTests {
    /// The chosen tests as R names them, in Prism's order.
    pub fn names(&self) -> Vec<&'static str> {
        [(self.dagostino, "dagostino"), (self.anderson, "anderson"), (self.shapiro, "shapiro"), (self.ks, "ks")]
            .into_iter()
            .filter_map(|(on, n)| on.then_some(n))
            .collect()
    }

    pub fn any(&self) -> bool {
        !self.names().is_empty()
    }

    /// Display name of a test by its R name.
    pub fn label(name: &str) -> &'static str {
        match name {
            "dagostino" => "D'Agostino-Pearson omnibus (K2)",
            "anderson" => "Anderson-Darling (A2*)",
            "shapiro" => "Shapiro-Wilk (W)",
            "ks" => "Kolmogorov-Smirnov (distance)",
            _ => "Normality test",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OutputFormat {
    /// Significant digits for everything except P values.
    #[serde(default = "digits4")]
    pub digits: u8,
    #[serde(default)]
    pub p_style: PStyle,
}

impl Default for OutputFormat {
    fn default() -> Self {
        OutputFormat { digits: 4, p_style: PStyle::Gp }
    }
}

/// Significance brackets on the source graph.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GraphAnnotation {
    /// Draw brackets with stars on the graph.
    #[serde(default = "yes")]
    pub show: bool,
    /// Also draw non-significant comparisons ("ns").
    #[serde(default)]
    pub show_ns: bool,
}

impl Default for GraphAnnotation {
    fn default() -> Self {
        GraphAnnotation { show: true, show_ns: false }
    }
}

/// Parameters of a t test (and its nonparametric counterparts).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TTestSpec {
    /// Graph (id or name) whose bars are the data sets. Default: the graph shown in the app.
    #[serde(default)]
    pub graph_id: String,
    /// The two data sets (bar labels), in order a, b. Empty = the graph's bars (must be two).
    #[serde(default)]
    pub data_sets: Vec<String>,
    #[serde(default)]
    pub design: Pairing,
    /// Column that identifies matched values (paired design), e.g. "Donor".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_by: Option<String>,
    #[serde(default)]
    pub distribution: Distribution,
    /// Unpaired normal/lognormal: Welch's t test (does not assume equal SDs). Default true.
    #[serde(default = "yes")]
    pub welch: bool,
    /// One-tailed instead of two-tailed P value.
    #[serde(default)]
    pub one_tailed: bool,
    #[serde(default)]
    pub difference: DiffDirection,
    /// Confidence level (0.90, 0.95 or 0.99); significance is P < 1 - confidence.
    #[serde(default = "conf95")]
    pub confidence: f64,
    /// Normality tests of the residuals (normal and lognormal designs only).
    #[serde(default)]
    pub normality: NormalityTests,
    #[serde(default)]
    pub results: ExtraResults,
    #[serde(default)]
    pub output: OutputFormat,
    #[serde(default)]
    pub graph: GraphAnnotation,
}

impl TTestSpec {
    pub fn new(graph_id: impl Into<String>, data_sets: Vec<String>) -> Self {
        TTestSpec {
            graph_id: graph_id.into(),
            data_sets,
            design: Pairing::Unpaired,
            match_by: None,
            distribution: Distribution::Normal,
            welch: true,
            one_tailed: false,
            difference: DiffDirection::AMinusB,
            confidence: 0.95,
            normality: NormalityTests::default(),
            results: ExtraResults::default(),
            output: OutputFormat::default(),
            graph: GraphAnnotation::default(),
        }
    }

    /// The test Prism would run for these choices.
    pub fn test_name(&self) -> &'static str {
        match (self.design, self.distribution, self.welch) {
            (Pairing::Unpaired, Distribution::Normal, true) => "Unpaired t test with Welch's correction",
            (Pairing::Unpaired, Distribution::Normal, false) => "Unpaired t test",
            (Pairing::Unpaired, Distribution::Lognormal, true) => "Unpaired t test with Welch's correction on log-transformed data",
            (Pairing::Unpaired, Distribution::Lognormal, false) => "Unpaired t test on log-transformed data",
            (Pairing::Unpaired, Distribution::Nonparametric, _) => "Mann-Whitney test",
            (Pairing::Paired, Distribution::Normal, _) => "Paired t test",
            (Pairing::Paired, Distribution::Lognormal, _) => "Ratio paired t test",
            (Pairing::Paired, Distribution::Nonparametric, _) => "Wilcoxon matched-pairs signed rank test",
        }
    }
}

/// Which multiple comparisons follow the ANOVA.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Comparisons {
    /// No follow-up tests.
    None,
    /// Compare the mean of each data set with the mean of every other data set.
    #[default]
    AllPairs,
    /// Compare the mean of each data set with the mean of a control data set.
    VsControl { control: String },
    /// Compare the means of preselected pairs of data sets.
    Selected { pairs: Vec<[String; 2]> },
}

/// How the multiple comparisons are corrected.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Correction {
    /// Correct using statistical hypothesis testing (Tukey, Dunnett, Šidák, Dunn…). Recommended.
    #[default]
    Hypothesis,
    /// Control the False Discovery Rate (two-stage step-up, Benjamini, Krieger & Yekutieli).
    Fdr,
    /// Don't correct: each comparison stands alone (Fisher's LSD).
    None,
}

/// Parameters of a one-way ANOVA (and nonparametric or repeated measures variants).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AnovaSpec {
    /// Graph (id or name) whose bars are the data sets. Default: the graph shown in the app.
    #[serde(default)]
    pub graph_id: String,
    /// Data sets (bar labels) to compare; at least two. Empty = all bars of the graph.
    #[serde(default)]
    pub data_sets: Vec<String>,
    #[serde(default)]
    pub design: Matching,
    /// Column that identifies matched values (repeated measures), e.g. "Donor".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_by: Option<String>,
    #[serde(default)]
    pub distribution: Distribution,
    /// Unmatched normal/lognormal: assume equal SDs (ordinary ANOVA). false = Brown-Forsythe and Welch ANOVA.
    #[serde(default = "yes")]
    pub equal_sd: bool,
    /// Repeated measures: assume sphericity. false = Geisser-Greenhouse correction (recommended).
    #[serde(default)]
    pub sphericity: bool,
    #[serde(default)]
    pub comparisons: Comparisons,
    #[serde(default)]
    pub correction: Correction,
    /// Family-wise alpha (or Q for FDR).
    #[serde(default = "alpha05")]
    pub alpha: f64,
    /// Swap direction of comparisons (B - A instead of A - B).
    #[serde(default)]
    pub swap: bool,
    /// Normality tests of the residuals (normal and lognormal designs only).
    #[serde(default)]
    pub normality: NormalityTests,
    #[serde(default)]
    pub results: ExtraResults,
    #[serde(default)]
    pub output: OutputFormat,
    #[serde(default)]
    pub graph: GraphAnnotation,
}

impl AnovaSpec {
    pub fn new(graph_id: impl Into<String>, data_sets: Vec<String>) -> Self {
        AnovaSpec {
            graph_id: graph_id.into(),
            data_sets,
            design: Matching::Unmatched,
            match_by: None,
            distribution: Distribution::Normal,
            equal_sd: true,
            sphericity: false,
            comparisons: Comparisons::AllPairs,
            correction: Correction::Hypothesis,
            alpha: 0.05,
            swap: false,
            normality: NormalityTests::default(),
            results: ExtraResults::default(),
            output: OutputFormat::default(),
            graph: GraphAnnotation::default(),
        }
    }

    /// The main test Prism would run for these choices.
    pub fn test_name(&self) -> &'static str {
        match (self.design, self.distribution) {
            (Matching::Unmatched, Distribution::Nonparametric) => "Kruskal-Wallis test",
            (Matching::Matched, Distribution::Nonparametric) => "Friedman test",
            (Matching::Unmatched, _) if self.equal_sd => "Ordinary one-way ANOVA",
            (Matching::Unmatched, _) => "Brown-Forsythe and Welch ANOVA tests",
            (Matching::Matched, _) if self.sphericity => "RM one-way ANOVA",
            (Matching::Matched, _) => "RM one-way ANOVA, with the Geisser-Greenhouse correction",
        }
    }
}

// ---------------------------------------------------------------------------
// XY analyses: curve fits, linear regression, correlation
// ---------------------------------------------------------------------------

/// Groups of curve-fit models, as in Prism's equation list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FitFamily {
    Lines,
    Stimulation,
    Inhibition,
    Exponential,
    Binding,
    Other,
}

impl FitFamily {
    pub const ALL: [FitFamily; 6] =
        [FitFamily::Lines, FitFamily::Stimulation, FitFamily::Inhibition, FitFamily::Exponential, FitFamily::Binding, FitFamily::Other];

    pub fn label(self) -> &'static str {
        match self {
            FitFamily::Lines => "Lines",
            FitFamily::Stimulation => "Dose-response – Stimulation",
            FitFamily::Inhibition => "Dose-response – Inhibition",
            FitFamily::Exponential => "Exponential",
            FitFamily::Binding => "Binding – Saturation",
            FitFamily::Other => "Other",
        }
    }

    pub fn models(self) -> Vec<FitModel> {
        FitModel::ALL.into_iter().filter(|m| m.family() == self).collect()
    }
}

/// A curve-fit model (Prism's equation names and parameters). Dose-response models
/// marked "log" expect X = log10 of the concentration.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FitModel {
    /// Straight line: Y = YIntercept + Slope*X.
    Line,
    /// Second order polynomial (quadratic): Y = B0 + B1*X + B2*X^2.
    Poly2,
    /// Third order polynomial (cubic): Y = B0 + B1*X + B2*X^2 + B3*X^3.
    Poly3,
    /// log(agonist) vs. normalized response (1 parameter, Hill slope 1): Y = 100/(1+10^(LogEC50-X)).
    #[serde(rename = "agonist_1pl")]
    Agonist1pl,
    /// log(agonist) vs. normalized response, variable slope (2 parameters): Y = 100/(1+10^((LogEC50-X)*HillSlope)).
    #[serde(rename = "agonist_2pl")]
    Agonist2pl,
    /// log(agonist) vs. response, standard slope (3 parameters, Hill slope 1): Y = Bottom + (Top-Bottom)/(1+10^(LogEC50-X)).
    #[serde(rename = "agonist_3pl")]
    Agonist3pl,
    /// log(agonist) vs. response, variable slope (4 parameters): Y = Bottom + (Top-Bottom)/(1+10^((LogEC50-X)*HillSlope)).
    #[default]
    #[serde(rename = "agonist_4pl")]
    Agonist4pl,
    /// Asymmetric sigmoidal (5 parameters), X is log(concentration).
    #[serde(rename = "agonist_5pl")]
    Agonist5pl,
    /// [Agonist] vs. response, variable slope (X is concentration): Y = Bottom + (Top-Bottom)*X^HillSlope/(EC50^HillSlope + X^HillSlope).
    AgonistConc,
    /// log(inhibitor) vs. normalized response (1 parameter): Y = 100/(1+10^(X-LogIC50)).
    #[serde(rename = "inhibitor_1pl")]
    Inhibitor1pl,
    /// log(inhibitor) vs. normalized response, variable slope (2 parameters): Y = 100/(1+10^((LogIC50-X)*HillSlope)).
    #[serde(rename = "inhibitor_2pl")]
    Inhibitor2pl,
    /// log(inhibitor) vs. response (3 parameters, Hill slope -1): Y = Bottom + (Top-Bottom)/(1+10^(X-LogIC50)).
    #[serde(rename = "inhibitor_3pl")]
    Inhibitor3pl,
    /// log(inhibitor) vs. response, variable slope (4 parameters): Y = Bottom + (Top-Bottom)/(1+10^((LogIC50-X)*HillSlope)).
    #[serde(rename = "inhibitor_4pl")]
    Inhibitor4pl,
    /// Asymmetric sigmoidal inhibition (5 parameters), X is log(concentration).
    #[serde(rename = "inhibitor_5pl")]
    Inhibitor5pl,
    /// [Inhibitor] vs. response, variable slope (X is concentration): Y = Bottom + (Top-Bottom)/(1+(X/IC50)^HillSlope).
    InhibitorConc,
    /// One phase decay: Y = (Y0-Plateau)*exp(-K*X) + Plateau.
    OnePhaseDecay,
    /// Plateau followed by one phase decay (decay starts at X0).
    PlateauDecay,
    /// Two phase decay (fast and slow components).
    TwoPhaseDecay,
    /// One phase association: Y = Y0 + (Plateau-Y0)*(1-exp(-K*X)).
    OnePhaseAssociation,
    /// Exponential (Malthusian) growth: Y = Y0*exp(K*X).
    ExponentialGrowth,
    /// One site, specific binding: Y = Bmax*X/(Kd + X).
    OneSiteSpecific,
    /// One site, total binding: Y = Bmax*X/(Kd + X) + NS*X + Background.
    OneSiteTotal,
    /// Specific binding with Hill slope: Y = Bmax*X^h/(Kd^h + X^h).
    SpecificHill,
    /// Michaelis-Menten enzyme kinetics: Y = Vmax*X/(Km + X).
    MichaelisMenten,
    /// Substrate inhibition: Y = Vmax*X/(Km + X*(1+X/Ki)).
    SubstrateInhibition,
    /// Gaussian: Y = Amplitude*exp(-0.5*((X-Mean)/SD)^2).
    Gaussian,
}

impl FitModel {
    pub const ALL: [FitModel; 26] = [
        FitModel::Line,
        FitModel::Poly2,
        FitModel::Poly3,
        FitModel::Agonist1pl,
        FitModel::Agonist2pl,
        FitModel::Agonist3pl,
        FitModel::Agonist4pl,
        FitModel::Agonist5pl,
        FitModel::AgonistConc,
        FitModel::Inhibitor1pl,
        FitModel::Inhibitor2pl,
        FitModel::Inhibitor3pl,
        FitModel::Inhibitor4pl,
        FitModel::Inhibitor5pl,
        FitModel::InhibitorConc,
        FitModel::OnePhaseDecay,
        FitModel::PlateauDecay,
        FitModel::TwoPhaseDecay,
        FitModel::OnePhaseAssociation,
        FitModel::ExponentialGrowth,
        FitModel::OneSiteSpecific,
        FitModel::OneSiteTotal,
        FitModel::SpecificHill,
        FitModel::MichaelisMenten,
        FitModel::SubstrateInhibition,
        FitModel::Gaussian,
    ];

    /// The name R uses for the model.
    pub fn id(self) -> &'static str {
        match self {
            FitModel::Line => "line",
            FitModel::Poly2 => "poly2",
            FitModel::Poly3 => "poly3",
            FitModel::Agonist1pl => "agonist_1pl",
            FitModel::Agonist2pl => "agonist_2pl",
            FitModel::Agonist3pl => "agonist_3pl",
            FitModel::Agonist4pl => "agonist_4pl",
            FitModel::Agonist5pl => "agonist_5pl",
            FitModel::AgonistConc => "agonist_conc",
            FitModel::Inhibitor1pl => "inhibitor_1pl",
            FitModel::Inhibitor2pl => "inhibitor_2pl",
            FitModel::Inhibitor3pl => "inhibitor_3pl",
            FitModel::Inhibitor4pl => "inhibitor_4pl",
            FitModel::Inhibitor5pl => "inhibitor_5pl",
            FitModel::InhibitorConc => "inhibitor_conc",
            FitModel::OnePhaseDecay => "one_phase_decay",
            FitModel::PlateauDecay => "plateau_decay",
            FitModel::TwoPhaseDecay => "two_phase_decay",
            FitModel::OnePhaseAssociation => "one_phase_association",
            FitModel::ExponentialGrowth => "exponential_growth",
            FitModel::OneSiteSpecific => "one_site_specific",
            FitModel::OneSiteTotal => "one_site_total",
            FitModel::SpecificHill => "specific_hill",
            FitModel::MichaelisMenten => "michaelis_menten",
            FitModel::SubstrateInhibition => "substrate_inhibition",
            FitModel::Gaussian => "gaussian",
        }
    }

    pub fn family(self) -> FitFamily {
        use FitModel::*;
        match self {
            Line | Poly2 | Poly3 => FitFamily::Lines,
            Agonist1pl | Agonist2pl | Agonist3pl | Agonist4pl | Agonist5pl | AgonistConc => FitFamily::Stimulation,
            Inhibitor1pl | Inhibitor2pl | Inhibitor3pl | Inhibitor4pl | Inhibitor5pl | InhibitorConc => FitFamily::Inhibition,
            OnePhaseDecay | PlateauDecay | TwoPhaseDecay | OnePhaseAssociation | ExponentialGrowth => FitFamily::Exponential,
            OneSiteSpecific | OneSiteTotal | SpecificHill | MichaelisMenten | SubstrateInhibition => FitFamily::Binding,
            Gaussian => FitFamily::Other,
        }
    }

    /// Short name for lists ("4PL – variable slope").
    pub fn label(self) -> &'static str {
        use FitModel::*;
        match self {
            Line => "Straight line",
            Poly2 => "Quadratic (2nd order polynomial)",
            Poly3 => "Cubic (3rd order polynomial)",
            Agonist1pl => "1PL – log(agonist) vs. normalized response",
            Agonist2pl => "2PL – log(agonist) vs. normalized response, variable slope",
            Agonist3pl => "3PL – log(agonist) vs. response, standard slope",
            Agonist4pl => "4PL – log(agonist) vs. response, variable slope",
            Agonist5pl => "5PL – asymmetric sigmoidal, X is log(concentration)",
            AgonistConc => "4PL – [agonist] vs. response, variable slope",
            Inhibitor1pl => "1PL – log(inhibitor) vs. normalized response",
            Inhibitor2pl => "2PL – log(inhibitor) vs. normalized response, variable slope",
            Inhibitor3pl => "3PL – log(inhibitor) vs. response, standard slope",
            Inhibitor4pl => "4PL – log(inhibitor) vs. response, variable slope",
            Inhibitor5pl => "5PL – asymmetric sigmoidal inhibition, X is log(concentration)",
            InhibitorConc => "4PL – [inhibitor] vs. response, variable slope",
            OnePhaseDecay => "One phase decay",
            PlateauDecay => "Plateau followed by one phase decay",
            TwoPhaseDecay => "Two phase decay",
            OnePhaseAssociation => "One phase association",
            ExponentialGrowth => "Exponential growth",
            OneSiteSpecific => "One site – specific binding",
            OneSiteTotal => "One site – total binding",
            SpecificHill => "Specific binding with Hill slope",
            MichaelisMenten => "Michaelis-Menten",
            SubstrateInhibition => "Substrate inhibition",
            Gaussian => "Gaussian",
        }
    }

    /// Parameter names, as reported (and constrained).
    pub fn params(self) -> &'static [&'static str] {
        use FitModel::*;
        match self {
            Line => &["YIntercept", "Slope"],
            Poly2 => &["B0", "B1", "B2"],
            Poly3 => &["B0", "B1", "B2", "B3"],
            Agonist1pl => &["LogEC50"],
            Agonist2pl => &["LogEC50", "HillSlope"],
            Agonist3pl => &["Bottom", "Top", "LogEC50"],
            Agonist4pl => &["Bottom", "Top", "LogEC50", "HillSlope"],
            Agonist5pl => &["Bottom", "Top", "LogEC50", "HillSlope", "S"],
            AgonistConc => &["Bottom", "Top", "EC50", "HillSlope"],
            Inhibitor1pl => &["LogIC50"],
            Inhibitor2pl => &["LogIC50", "HillSlope"],
            Inhibitor3pl => &["Bottom", "Top", "LogIC50"],
            Inhibitor4pl => &["Bottom", "Top", "LogIC50", "HillSlope"],
            Inhibitor5pl => &["Bottom", "Top", "LogIC50", "HillSlope", "S"],
            InhibitorConc => &["Bottom", "Top", "IC50", "HillSlope"],
            OnePhaseDecay => &["Y0", "Plateau", "K"],
            PlateauDecay => &["X0", "Y0", "Plateau", "K"],
            TwoPhaseDecay => &["Y0", "Plateau", "PercentFast", "KFast", "KSlow"],
            OnePhaseAssociation => &["Y0", "Plateau", "K"],
            ExponentialGrowth => &["Y0", "K"],
            OneSiteSpecific => &["Bmax", "Kd"],
            OneSiteTotal => &["Bmax", "Kd", "NS", "Background"],
            SpecificHill => &["Bmax", "Kd", "h"],
            MichaelisMenten => &["Vmax", "Km"],
            SubstrateInhibition => &["Vmax", "Km", "Ki"],
            Gaussian => &["Amplitude", "Mean", "SD"],
        }
    }

    /// The equation, written as in Prism.
    pub fn equation(self) -> &'static str {
        use FitModel::*;
        match self {
            Line => "Y = YIntercept + Slope*X",
            Poly2 => "Y = B0 + B1*X + B2*X^2",
            Poly3 => "Y = B0 + B1*X + B2*X^2 + B3*X^3",
            Agonist1pl => "Y = 100/(1+10^(LogEC50-X))",
            Agonist2pl => "Y = 100/(1+10^((LogEC50-X)*HillSlope))",
            Agonist3pl => "Y = Bottom + (Top-Bottom)/(1+10^(LogEC50-X))",
            Agonist4pl => "Y = Bottom + (Top-Bottom)/(1+10^((LogEC50-X)*HillSlope))",
            Agonist5pl => "LogXb = LogEC50 + (1/HillSlope)*log(2^(1/S)-1); Y = Bottom + (Top-Bottom)/(1+10^((LogXb-X)*HillSlope))^S",
            AgonistConc => "Y = Bottom + (Top-Bottom)*X^HillSlope/(EC50^HillSlope + X^HillSlope)",
            Inhibitor1pl => "Y = 100/(1+10^(X-LogIC50))",
            Inhibitor2pl => "Y = 100/(1+10^((LogIC50-X)*HillSlope))",
            Inhibitor3pl => "Y = Bottom + (Top-Bottom)/(1+10^(X-LogIC50))",
            Inhibitor4pl => "Y = Bottom + (Top-Bottom)/(1+10^((LogIC50-X)*HillSlope))",
            Inhibitor5pl => "LogXb = LogIC50 + (1/HillSlope)*log(2^(1/S)-1); Y = Bottom + (Top-Bottom)/(1+10^((LogXb-X)*HillSlope))^S",
            InhibitorConc => "Y = Bottom + (Top-Bottom)/(1+(X/IC50)^HillSlope)",
            OnePhaseDecay => "Y = (Y0-Plateau)*exp(-K*X) + Plateau",
            PlateauDecay => "Y = IF(X<X0, Y0, Plateau + (Y0-Plateau)*exp(-K*(X-X0)))",
            TwoPhaseDecay => "Y = Plateau + SpanFast*exp(-KFast*X) + SpanSlow*exp(-KSlow*X)",
            OnePhaseAssociation => "Y = Y0 + (Plateau-Y0)*(1-exp(-K*X))",
            ExponentialGrowth => "Y = Y0*exp(K*X)",
            OneSiteSpecific => "Y = Bmax*X/(Kd + X)",
            OneSiteTotal => "Y = Bmax*X/(Kd + X) + NS*X + Background",
            SpecificHill => "Y = Bmax*X^h/(Kd^h + X^h)",
            MichaelisMenten => "Y = Vmax*X/(Km + X)",
            SubstrateInhibition => "Y = Vmax*X/(Km + X*(1+X/Ki))",
            Gaussian => "Y = Amplitude*exp(-0.5*((X-Mean)/SD)^2)",
        }
    }

    /// The shape of the curve on a unit square, for the little drawings in the fit dialog.
    pub fn shape(self, t: f64) -> f64 {
        use FitModel::*;
        let sig = |t: f64, k: f64| 1.0 / (1.0 + (-(t - 0.5) * k).exp());
        match self {
            Line => 0.15 + 0.7 * t,
            Poly2 => 0.85 - 2.8 * (t - 0.5).powi(2),
            Poly3 => 0.5 + 2.6 * (t - 0.5).powi(3) - 0.35 * (t - 0.5),
            Agonist1pl | Agonist2pl => 0.05 + 0.9 * sig(t, if self == Agonist1pl { 9.0 } else { 14.0 }),
            Agonist3pl | Agonist4pl => 0.15 + 0.7 * sig(t, if self == Agonist3pl { 9.0 } else { 14.0 }),
            Agonist5pl => 0.15 + 0.7 * sig(t, 14.0).powf(2.5),
            AgonistConc | SpecificHill => 0.1 + 0.8 * t.powi(2) / (0.09 + t.powi(2)),
            Inhibitor1pl | Inhibitor2pl => 0.95 - 0.9 * sig(t, if self == Inhibitor1pl { 9.0 } else { 14.0 }),
            Inhibitor3pl | Inhibitor4pl => 0.85 - 0.7 * sig(t, if self == Inhibitor3pl { 9.0 } else { 14.0 }),
            Inhibitor5pl => 0.85 - 0.7 * sig(t, 14.0).powf(2.5),
            InhibitorConc => 0.9 - 0.8 * t.powi(2) / (0.09 + t.powi(2)),
            OnePhaseDecay => 0.15 + 0.75 * (-4.0 * t).exp(),
            PlateauDecay => if t < 0.3 { 0.9 } else { 0.15 + 0.75 * (-6.0 * (t - 0.3)).exp() },
            TwoPhaseDecay => 0.1 + 0.45 * (-12.0 * t).exp() + 0.35 * (-2.0 * t).exp(),
            OnePhaseAssociation => 0.1 + 0.75 * (1.0 - (-4.0 * t).exp()),
            ExponentialGrowth => 0.08 + 0.82 * ((3.0 * t).exp() - 1.0) / (3f64.exp() - 1.0),
            OneSiteSpecific | MichaelisMenten => 0.05 + 0.85 * t / (0.2 + t),
            OneSiteTotal => 0.05 + 0.55 * t / (0.15 + t) + 0.3 * t,
            SubstrateInhibition => 0.05 + 1.6 * t / (0.12 + t * (1.0 + t / 0.35)),
            Gaussian => 0.1 + 0.8 * (-0.5 * ((t - 0.5) / 0.15).powi(2)).exp(),
        }
    }
}

/// A parameter constrained to a constant (Prism's Constraints tab), e.g. Bottom = 0.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FixedParam {
    /// Parameter name as the model lists it (e.g. "Bottom", "HillSlope").
    pub name: String,
    pub value: f64,
}

/// Whether a fit or regression line is drawn on its graph.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CurveDisplay {
    /// Draw the fitted curve / line on the graph.
    #[serde(default = "yes")]
    pub show: bool,
}

impl Default for CurveDisplay {
    fn default() -> Self {
        CurveDisplay { show: true }
    }
}

/// Nonlinear regression (curve fit) of the data sets of an XY graph.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FitSpec {
    /// XY graph (id or name) whose data sets are fitted. Default: the graph shown in the app.
    #[serde(default)]
    pub graph_id: String,
    /// Data sets (legend entries) to fit, each separately. Empty = all.
    #[serde(default)]
    pub data_sets: Vec<String>,
    #[serde(default)]
    pub model: FitModel,
    /// Parameters constrained to constants (e.g. [{"name": "Bottom", "value": 0}]).
    #[serde(default)]
    pub fixed: Vec<FixedParam>,
    /// Confidence level of the intervals.
    #[serde(default = "conf95")]
    pub confidence: f64,
    #[serde(default)]
    pub output: OutputFormat,
    #[serde(default)]
    pub graph: CurveDisplay,
}

impl FitSpec {
    pub fn new(graph_id: impl Into<String>, model: FitModel) -> Self {
        FitSpec {
            graph_id: graph_id.into(),
            data_sets: Vec::new(),
            model,
            fixed: Vec::new(),
            confidence: 0.95,
            output: OutputFormat::default(),
            graph: CurveDisplay::default(),
        }
    }
}

/// Linear regression of the data sets of an XY graph (each separately).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LinRegSpec {
    /// XY graph (id or name). Default: the graph shown in the app.
    #[serde(default)]
    pub graph_id: String,
    /// Data sets (legend entries). Empty = all.
    #[serde(default)]
    pub data_sets: Vec<String>,
    #[serde(default = "conf95")]
    pub confidence: f64,
    #[serde(default)]
    pub output: OutputFormat,
    #[serde(default)]
    pub graph: CurveDisplay,
}

impl LinRegSpec {
    pub fn new(graph_id: impl Into<String>) -> Self {
        LinRegSpec { graph_id: graph_id.into(), data_sets: Vec::new(), confidence: 0.95, output: OutputFormat::default(), graph: CurveDisplay::default() }
    }
}

/// Correlation coefficient.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CorrMethod {
    /// Pearson r (assumes a bivariate Gaussian distribution).
    #[default]
    Pearson,
    /// Spearman's rank correlation (nonparametric).
    Spearman,
}

/// Correlation between X and Y of each data set of an XY graph.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CorrelationSpec {
    /// XY graph (id or name). Default: the graph shown in the app.
    #[serde(default)]
    pub graph_id: String,
    /// Data sets (legend entries). Empty = all.
    #[serde(default)]
    pub data_sets: Vec<String>,
    #[serde(default)]
    pub method: CorrMethod,
    /// One-tailed P value (in the direction of the observed correlation).
    #[serde(default)]
    pub one_tailed: bool,
    #[serde(default = "conf95")]
    pub confidence: f64,
    #[serde(default)]
    pub output: OutputFormat,
}

/// Normality diagnostics of the data sets of a graph (Prism's "Normality and Lognormality Tests").
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NormalitySpec {
    /// Graph (id or name) whose data sets are tested. Default: the graph shown in the app.
    #[serde(default)]
    pub graph_id: String,
    /// Data sets (bar labels). Empty = all.
    #[serde(default)]
    pub data_sets: Vec<String>,
    /// Which tests to run; none chosen runs Shapiro-Wilk and D'Agostino-Pearson.
    #[serde(default)]
    pub tests: NormalityTests,
    /// Also test log10 of the values (lognormality), as Prism does.
    #[serde(default)]
    pub lognormal: bool,
    #[serde(default)]
    pub output: OutputFormat,
}

impl NormalitySpec {
    pub fn new(graph_id: impl Into<String>) -> Self {
        NormalitySpec {
            graph_id: graph_id.into(),
            data_sets: Vec::new(),
            tests: NormalityTests { dagostino: true, anderson: false, shapiro: true, ks: false },
            lognormal: false,
            output: OutputFormat::default(),
        }
    }

    /// The tests to run, falling back to Prism's default pair.
    pub fn chosen(&self) -> NormalityTests {
        if self.tests.any() {
            self.tests
        } else {
            NormalityTests { dagostino: true, anderson: false, shapiro: true, ks: false }
        }
    }
}

/// How outliers are identified.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OutlierMethod {
    /// Generalized ESD (Rosner): Grubbs' test applied repeatedly, so several outliers in one
    /// data set are found without the masking that a single Grubbs' test suffers from.
    #[default]
    Esd,
    /// Grubbs' test: at most one outlier per data set.
    Grubbs,
    /// Tukey's rule: more than 1.5 × IQR outside the quartiles (descriptive, no P value).
    Tukey,
}

impl OutlierMethod {
    pub const ALL: [OutlierMethod; 3] = [OutlierMethod::Esd, OutlierMethod::Grubbs, OutlierMethod::Tukey];

    pub fn label(self) -> &'static str {
        match self {
            OutlierMethod::Esd => "Iterative Grubbs (ESD)",
            OutlierMethod::Grubbs => "Grubbs (one outlier)",
            OutlierMethod::Tukey => "Tukey (1.5 x IQR)",
        }
    }
}

impl std::fmt::Display for OutlierMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Outlier identification in the data sets of a graph.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutlierSpec {
    /// Graph (id or name) whose data sets are examined. Default: the graph shown in the app.
    #[serde(default)]
    pub graph_id: String,
    /// Data sets (bar labels). Empty = all.
    #[serde(default)]
    pub data_sets: Vec<String>,
    #[serde(default)]
    pub method: OutlierMethod,
    /// Significance level of the test (Grubbs / ESD).
    #[serde(default = "alpha05")]
    pub alpha: f64,
    /// At most this many outliers per data set (ESD).
    #[serde(default = "max_out")]
    pub max_outliers: usize,
    #[serde(default)]
    pub output: OutputFormat,
}

fn max_out() -> usize {
    5
}

impl OutlierSpec {
    pub fn new(graph_id: impl Into<String>) -> Self {
        OutlierSpec {
            graph_id: graph_id.into(),
            data_sets: Vec::new(),
            method: OutlierMethod::default(),
            alpha: 0.05,
            max_outliers: 5,
            output: OutputFormat::default(),
        }
    }
}

impl CorrelationSpec {
    pub fn new(graph_id: impl Into<String>) -> Self {
        CorrelationSpec {
            graph_id: graph_id.into(),
            data_sets: Vec::new(),
            method: CorrMethod::Pearson,
            one_tailed: false,
            confidence: 0.95,
            output: OutputFormat::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// Two-way ANOVA, area under the curve, survival
// ---------------------------------------------------------------------------

/// Which multiple comparisons follow a two-way ANOVA. "Rows" are the X categories of the
/// grouped graph, "columns" its data sets (Group by), as in Prism's grouped tables.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TwoWayCompare {
    /// No follow-up tests.
    None,
    /// Within each X category, compare the data sets (simple effects; the bars of each cluster).
    #[default]
    WithinRows,
    /// Within each data set, compare the X categories.
    WithinColumns,
    /// Main effect of X: compare the X category means (averaged over data sets).
    RowMeans,
    /// Main effect of the data sets: compare the data set means (averaged over X).
    ColumnMeans,
    /// Compare every bar with every other bar.
    AllCells,
}

impl TwoWayCompare {
    pub const ALL: [TwoWayCompare; 6] =
        [TwoWayCompare::None, TwoWayCompare::WithinRows, TwoWayCompare::WithinColumns, TwoWayCompare::RowMeans, TwoWayCompare::ColumnMeans, TwoWayCompare::AllCells];

    pub fn id(self) -> &'static str {
        match self {
            TwoWayCompare::None => "none",
            TwoWayCompare::WithinRows => "within_rows",
            TwoWayCompare::WithinColumns => "within_columns",
            TwoWayCompare::RowMeans => "row_means",
            TwoWayCompare::ColumnMeans => "column_means",
            TwoWayCompare::AllCells => "all_cells",
        }
    }
}

/// Parameters of an ordinary two-way ANOVA on the bars of a grouped graph.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TwoWaySpec {
    /// Grouped graph (id or name): its X categories are one factor, its data sets the other.
    #[serde(default)]
    pub graph_id: String,
    /// Bars to include ("X | data set" labels). Empty = all bars.
    #[serde(default)]
    pub data_sets: Vec<String>,
    /// Fit the full model with the interaction (recommended). false = main effects only.
    #[serde(default = "yes")]
    pub interaction: bool,
    #[serde(default)]
    pub compare: TwoWayCompare,
    #[serde(default)]
    pub correction: Correction,
    /// Family-wise alpha (or Q for FDR).
    #[serde(default = "alpha05")]
    pub alpha: f64,
    #[serde(default)]
    pub results: ExtraResults,
    #[serde(default)]
    pub output: OutputFormat,
    #[serde(default)]
    pub graph: GraphAnnotation,
}

impl TwoWaySpec {
    pub fn new(graph_id: impl Into<String>) -> Self {
        TwoWaySpec {
            graph_id: graph_id.into(),
            data_sets: Vec::new(),
            interaction: true,
            compare: TwoWayCompare::WithinRows,
            correction: Correction::Hypothesis,
            alpha: 0.05,
            results: ExtraResults::default(),
            output: OutputFormat::default(),
            graph: GraphAnnotation::default(),
        }
    }
}

/// Area under the curve of the data sets of an XY graph (trapezoid rule, on the mean Y
/// at each X).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AucSpec {
    /// XY graph (id or name). Default: the graph shown in the app.
    #[serde(default)]
    pub graph_id: String,
    /// Data sets (legend entries). Empty = all.
    #[serde(default)]
    pub data_sets: Vec<String>,
    /// Baseline Y: areas are measured from here.
    #[serde(default)]
    pub baseline: f64,
    /// Ignore peaks lower than this percentage of the Y range (default 10).
    #[serde(default = "ten")]
    pub min_peak_percent: f64,
    /// Also list peaks below the baseline (negative areas).
    #[serde(default)]
    pub peaks_below: bool,
    #[serde(default = "conf95")]
    pub confidence: f64,
    #[serde(default)]
    pub output: OutputFormat,
}

fn ten() -> f64 {
    10.0
}

impl AucSpec {
    pub fn new(graph_id: impl Into<String>) -> Self {
        AucSpec {
            graph_id: graph_id.into(),
            data_sets: Vec::new(),
            baseline: 0.0,
            min_peak_percent: 10.0,
            peaks_below: false,
            confidence: 0.95,
            output: OutputFormat::default(),
        }
    }
}

/// Kaplan-Meier survival analysis of the curves of a survival graph.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SurvivalSpec {
    /// Survival graph (id or name). Default: the graph shown in the app.
    #[serde(default)]
    pub graph_id: String,
    /// Curves (groups) to compare. Empty = all.
    #[serde(default)]
    pub data_sets: Vec<String>,
    /// Confidence level of survival proportions, medians and hazard ratios.
    #[serde(default = "conf95")]
    pub confidence: f64,
    /// Report the survival proportions at every event time (Prism's "Survival proportions" table).
    #[serde(default = "yes")]
    pub proportions: bool,
    #[serde(default)]
    pub output: OutputFormat,
}

impl SurvivalSpec {
    pub fn new(graph_id: impl Into<String>) -> Self {
        SurvivalSpec { graph_id: graph_id.into(), data_sets: Vec::new(), confidence: 0.95, proportions: true, output: OutputFormat::default() }
    }
}

/// Analysis of a contingency table: chi-square, Fisher's exact test and the rest.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ContingencySpec {
    /// Contingency table graph (id or name). Default: the graph shown in the app.
    #[serde(default)]
    pub graph_id: String,
    /// Columns of the table to analyse (the graph's data sets). Empty = all of them.
    #[serde(default)]
    pub data_sets: Vec<String>,
    /// Paired design (the same subjects before and after): adds McNemar's test, which
    /// needs a square table.
    #[serde(default)]
    pub paired: bool,
    /// Confidence level of the relative risk, odds ratio and the other estimates.
    #[serde(default = "conf95")]
    pub confidence: f64,
    /// Report the expected counts the chi-square test assumes.
    #[serde(default)]
    pub expected: bool,
    #[serde(default)]
    pub output: OutputFormat,
}

impl ContingencySpec {
    pub fn new(graph_id: impl Into<String>) -> Self {
        ContingencySpec {
            graph_id: graph_id.into(),
            data_sets: Vec::new(),
            paired: false,
            confidence: 0.95,
            expected: false,
            output: OutputFormat::default(),
        }
    }
}

/// The choices of a graph that define its bars, recorded when an analysis of those bars
/// is set up. If a later change to the graph alters the analysed data sets, the analysis
/// is out of date until it is recalculated or set up again.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GraphBasis {
    pub table_id: String,
    pub x: Vec<String>,
    pub y: Vec<String>,
    pub group: Vec<String>,
    /// Row filters, sorted by column with sorted values (so their order doesn't matter).
    pub filters: Vec<crate::spec::Filter>,
}

impl GraphBasis {
    pub fn of(g: &crate::spec::GraphSpec) -> Self {
        let mut filters: Vec<crate::spec::Filter> = g
            .filters
            .iter()
            .filter(|f| !f.keeps_everything())
            .map(|f| {
                // Sorted, so that picking the same values in another order is the same basis.
                let mut include = f.include.clone();
                include.sort();
                include.dedup();
                crate::spec::Filter { column: f.column.clone(), include, min: f.min, max: f.max }
            })
            .collect();
        filters.sort_by(|a, b| a.column.cmp(&b.column));
        GraphBasis { table_id: g.table_id.clone(), x: g.x.clone(), y: g.y.clone(), group: g.group.clone(), filters }
    }

    /// `g` with this basis's choices (to compute the bars the analysis was set up on).
    pub fn apply_to(&self, g: &crate::spec::GraphSpec) -> crate::spec::GraphSpec {
        let mut g = g.clone();
        g.table_id = self.table_id.clone();
        g.x = self.x.clone();
        g.y = self.y.clone();
        g.group = self.group.clone();
        g.filters = self.filters.clone();
        g
    }

    /// What changed between this basis and `now`, in words for the user.
    pub fn changes(&self, now: &GraphBasis) -> Vec<String> {
        let list = |v: &[String], sep: &str| if v.is_empty() { "(none)".to_string() } else { v.join(sep) };
        let mut out = Vec::new();
        if self.table_id != now.table_id {
            out.push(format!("data table {} → {}", self.table_id, now.table_id));
        }
        if self.x != now.x {
            out.push(format!("X {} → {}", list(&self.x, crate::spec::COMBINE_SEP), list(&now.x, crate::spec::COMBINE_SEP)));
        }
        if self.y != now.y {
            out.push(format!("Y {} → {}", list(&self.y, ", "), list(&now.y, ", ")));
        }
        if self.group != now.group {
            out.push(format!("Group by {} → {}", list(&self.group, crate::spec::COMBINE_SEP), list(&now.group, crate::spec::COMBINE_SEP)));
        }
        if self.filters != now.filters {
            let mut cols: Vec<&str> = self.filters.iter().chain(&now.filters).map(|f| f.column.as_str()).collect();
            cols.sort_unstable();
            cols.dedup();
            let changed: Vec<&str> = cols
                .into_iter()
                .filter(|c| self.filters.iter().find(|f| f.column == *c) != now.filters.iter().find(|f| f.column == *c))
                .collect();
            out.push(format!("rows to include ({})", changed.join(", ")));
        }
        out
    }

    /// Keeps the basis in step with a column rename.
    pub fn rename_column(&mut self, old: &str, new: &str) {
        let fix = |s: &mut String| {
            if s == old {
                *s = new.to_string();
            }
        };
        self.x.iter_mut().chain(self.y.iter_mut()).chain(self.group.iter_mut()).for_each(fix);
        self.filters.iter_mut().for_each(|f| fix(&mut f.column));
        self.filters.sort_by(|a, b| a.column.cmp(&b.column));
    }

    /// Keeps the basis in step with a level rename.
    pub fn rename_level(&mut self, column: &str, old: &str, new: &str) {
        for f in self.filters.iter_mut().filter(|f| f.column == column) {
            for l in f.include.iter_mut().filter(|l| *l == old) {
                *l = new.to_string();
            }
            f.include.sort();
            f.include.dedup();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_basis_ignores_filter_order_and_describes_changes() {
        use crate::spec::{Filter, GraphKind, GraphSpec};
        let mut g = GraphSpec::new("G1", "g", "T1", GraphKind::ColumnBar);
        g.x = vec!["Condition".into()];
        g.y = vec!["IL15".into()];
        g.filters = vec![
            Filter { column: "Well".into(), include: vec!["B".into(), "A".into()], ..Default::default() },
            Filter { column: "Additive".into(), include: vec!["IL2".into()], ..Default::default() },
        ];
        let before = GraphBasis::of(&g);
        g.filters.reverse();
        g.filters[1].include = vec!["A".into(), "B".into()];
        assert_eq!(GraphBasis::of(&g), before);
        g.y = vec!["IL2".into()];
        g.filters.pop();
        assert_eq!(before.changes(&GraphBasis::of(&g)), vec!["Y IL15 → IL2".to_string(), "rows to include (Well)".to_string()]);
        assert_eq!(before.apply_to(&g).y, vec!["IL15".to_string()]);
    }

    #[test]
    fn p_styles() {
        assert_eq!(PStyle::Gp.format(0.03321, 4), "0.0332");
        assert_eq!(PStyle::Gp.format(0.00001, 4), "<0.0001");
        assert_eq!(PStyle::Gp.stars(0.00001), "****");
        assert_eq!(PStyle::Apa.format(0.0332, 4), ".033");
        assert_eq!(PStyle::Apa.stars(0.00001), "***");
        assert_eq!(PStyle::Nejm.format(0.1234, 4), "0.12");
        assert_eq!(PStyle::Nejm.format(0.0332, 4), "0.033");
        assert_eq!(PStyle::Gp.stars(0.05), "ns");
        assert_eq!(PStyle::Gp.stars(0.0499), "*");
    }

    #[test]
    fn significant_digits() {
        assert_eq!(format_sig(1.23456, 4), "1.235");
        assert_eq!(format_sig(0.00123456, 4), "0.001235");
        assert_eq!(format_sig(12345.6, 4), "12346");
        assert_eq!(format_sig(-2.5, 4), "-2.5");
        assert_eq!(format_sig(7.1352e-7, 4), "7.135e-7");
        assert_eq!(format_sig(-1.5e9, 4), "-1.5e9");
    }

    #[test]
    fn fit_model_ids_are_their_json_names() {
        for m in FitModel::ALL {
            assert_eq!(serde_json::to_value(m).unwrap(), serde_json::json!(m.id()));
            assert!(m.shape(0.5).is_finite());
        }
        assert_eq!(FitFamily::ALL.iter().map(|f| f.models().len()).sum::<usize>(), FitModel::ALL.len());
    }

    #[test]
    fn specs_roundtrip_with_defaults() {
        let t: TTestSpec = serde_json::from_str(r#"{"graph_id":"G1","data_sets":["A","B"]}"#).unwrap();
        assert!(t.welch && t.confidence == 0.95 && t.graph.show && !t.graph.show_ns);
        assert_eq!(t.test_name(), "Unpaired t test with Welch's correction");
        let a: AnovaSpec = serde_json::from_str(r#"{"graph_id":"G1","data_sets":["A","B","C"],"comparisons":{"mode":"vs_control","control":"A"}}"#).unwrap();
        assert_eq!(a.comparisons, Comparisons::VsControl { control: "A".into() });
        assert_eq!(a.test_name(), "Ordinary one-way ANOVA");
    }
}
