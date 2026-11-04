//! Statistics computed by the R engine, checked against reference values computed
//! independently (SciPy 1.18 / textbook values). Skipped when R with Otto's packages is not
//! set up. Otto's R code is additionally checked against established R implementations
//! (TukeyHSD, multcomp, drc) by `otto-r/tests/cross_check.R`.

use serde_json::json;

use otto_engine::Engine;

macro_rules! require_r {
    () => {
        if !otto_r::available() {
            eprintln!("skipped: the statistics engine (R with Otto's packages) is not set up");
            return;
        }
    };
}

/// The value after "key: " in a results text (first match).
fn value(text: &str, key: &str) -> String {
    let prefix = format!("{key}: ");
    text.lines()
        .find_map(|l| l.strip_prefix(&prefix))
        .unwrap_or_else(|| panic!("no '{key}' in:\n{text}"))
        .to_string()
}

/// A CSV row of a results table that starts with `label` (first match), split into cells.
fn csv_row(text: &str, label: &str) -> Vec<String> {
    let line = text.lines().find(|l| l.trim_start_matches('"').trim_start().starts_with(label)).unwrap_or_else(|| panic!("no row '{label}' in:\n{text}"));
    line.split(',').map(|c| c.trim().trim_matches('"').trim().to_string()).collect()
}

fn close(actual: &str, expected: f64, tol: f64) {
    let v: f64 = actual.trim_start_matches(['<', '=', 'P']).parse().unwrap_or_else(|_| panic!("not a number: {actual}"));
    assert!((v - expected).abs() <= tol, "{actual} vs. expected {expected} (± {tol})");
}

fn engine(csv: &str, kind: &str, x: &str, y: &str, group: Option<&str>) -> Engine {
    let mut e = Engine::new();
    e.call("table_import_text", json!({ "text": csv, "name": "Data" })).unwrap();
    let mut args = json!({ "kind": kind, "x": x, "y": [y] });
    if let Some(g) = group {
        args["group"] = json!(g);
    }
    e.call("graph_create", args).unwrap();
    e
}

const TWO_GROUPS: &str = "Group,Subject,Value\nA,s1,1\nA,s2,2\nA,s3,3\nA,s4,4\nA,s5,5\nB,s1,2\nB,s2,4\nB,s3,6\nB,s4,8\nB,s5,10\n";

#[test]
fn t_tests_match_reference_values() {
    require_r!();
    let mut e = engine(TWO_GROUPS, "column_bar", "Group", "Value", None);
    // Student's t: t = -1.8974, df = 8, P = 0.09435.
    let out = e.call("analysis_create_t_test", json!({ "data_sets": ["A", "B"], "welch": false, "results": { "effect_sizes": true } })).unwrap();
    let t = out.text.unwrap();
    assert_eq!(value(&t, "t, df"), "t=1.897, df=8");
    close(&value(&t, "P value"), 0.0943, 1e-4);
    assert!(t.contains("Computed with R "), "{t}");
    // Welch: df = 5.882.
    let t = e.call("analysis_edit_t_test", json!({ "welch": true })).unwrap().text.unwrap();
    assert_eq!(value(&t, "Welch-corrected t, df"), "t=1.897, df=5.882");
    // Paired (B - A = 1..5): t = 4.2426, df = 4, P = 0.01324.
    let t = e.call("analysis_edit_t_test", json!({ "design": "paired", "match_by": "Subject" })).unwrap().text.unwrap();
    assert_eq!(value(&t, "t, df"), "t=4.243, df=4");
    close(&value(&t, "P value"), 0.0132, 1e-4);
    // Wilcoxon matched pairs: exact P = 0.0625.
    let t = e.call("analysis_edit_t_test", json!({ "distribution": "nonparametric" })).unwrap().text.unwrap();
    close(&value(&t, "P value"), 0.0625, 1e-4);
    assert_eq!(value(&t, "Exact or approximate P value?"), "Exact");
    // Mann-Whitney on [1, 2, 3] vs [4, 5, 6] is checked in the ANOVA data below.
}

const THREE_GROUPS: &str = "Group,Value\nA,1\nA,2\nA,3\nB,2\nB,3\nB,4\nC,5\nC,6\nC,7\n";

#[test]
fn anova_and_post_tests_match_reference_values() {
    require_r!();
    let mut e = engine(THREE_GROUPS, "column_bar", "Group", "Value", None);
    // F = 13 on (2, 6) df, P = 0.006592; Tukey adjusted P: 0.4827, 0.006494, 0.02423.
    let t = e.call("analysis_create_one_way_anova", json!({})).unwrap().text.unwrap();
    close(&value(&t, "F"), 13.0, 1e-9);
    close(&value(&t, "P value"), 0.0066, 1e-4);
    close(csv_row(&t, "A vs. B").last().unwrap(), 0.4827, 1e-4);
    close(csv_row(&t, "A vs. C").last().unwrap(), 0.0065, 1e-4);
    close(csv_row(&t, "B vs. C").last().unwrap(), 0.0242, 1e-4);
    // Mann-Whitney A vs C ([1,2,3] vs [5,6,7]): U = 0, exact P = 0.1.
    let t = e.call("analysis_create_t_test", json!({ "data_sets": ["A", "C"], "distribution": "nonparametric" })).unwrap().text.unwrap();
    close(&value(&t, "Mann-Whitney U"), 0.0, 1e-12);
    close(&value(&t, "P value"), 0.1, 1e-4);
}

#[test]
fn rank_anovas_match_reference_values() {
    require_r!();
    let csv = "Group,Value\nA,1\nA,2\nA,3\nB,4\nB,5\nB,6\nC,7\nC,8\nC,9\n";
    let mut e = engine(csv, "column_bar", "Group", "Value", None);
    // Kruskal-Wallis: H = 7.2, P = exp(-3.6) = 0.02732.
    let t = e.call("analysis_create_one_way_anova", json!({ "distribution": "nonparametric" })).unwrap().text.unwrap();
    close(&value(&t, "Kruskal-Wallis statistic"), 7.2, 1e-9);
    close(&value(&t, "P value"), 0.0273, 1e-4);
    // Friedman: Q = 8, P = exp(-4) = 0.01832.
    let csv = "Subject,Treatment,Value\ns1,A,1\ns1,B,2\ns1,C,3\ns2,A,2\ns2,B,5\ns2,C,9\ns3,A,0.5\ns3,B,0.7\ns3,C,4\ns4,A,3\ns4,B,3.5\ns4,C,3.6\n";
    let mut e = engine(csv, "column_bar", "Treatment", "Value", None);
    let t = e.call("analysis_create_one_way_anova", json!({ "design": "matched", "match_by": "Subject", "distribution": "nonparametric" })).unwrap().text.unwrap();
    close(&value(&t, "Friedman statistic"), 8.0, 1e-9);
    close(&value(&t, "P value"), 0.0183, 1e-4);
}

#[test]
fn normality_tests_match_reference_values() {
    require_r!();
    let v = [2.1, 3.4, 1.9, 5.6, 4.4, 3.3, 2.8, 4.1, 3.9, 2.2, 4.8, 3.0];
    let req = otto_r::Request::new("normality", json!({ "sets": [v] }), json!({ "tests": ["dagostino", "shapiro"] }));
    let reply = otto_r::call(&req).unwrap();
    let r = &reply.result[0];
    // D'Agostino-Pearson K2 = 0.4670, P = 0.7918 (SciPy normaltest implements the same test).
    assert!((r[0]["statistic"].as_f64().unwrap() - 0.466969).abs() < 1e-5, "{r}");
    assert!((r[0]["p"].as_f64().unwrap() - 0.791770).abs() < 1e-5, "{r}");
    // Shapiro-Wilk on the first ten values: W = 0.95248, P = 0.6979.
    let req = otto_r::Request::new("normality", json!({ "sets": [&v[..10]] }), json!({ "tests": ["shapiro"] }));
    let r = otto_r::call(&req).unwrap().result;
    assert!((r[0][0]["statistic"].as_f64().unwrap() - 0.952483).abs() < 1e-5, "{r}");
    assert!((r[0][0]["p"].as_f64().unwrap() - 0.697917).abs() < 1e-4, "{r}");
}

const XY: &str = "X,Y\n1,2.1\n2,3.9\n3,6.2\n4,7.8\n5,10.1\n6,12.2\n";

#[test]
fn linear_regression_and_correlation_match_reference_values() {
    require_r!();
    let mut e = engine(XY, "xy_points", "X", "Y", None);
    // Slope 2.02 (SE 0.04276), intercept -0.02 (SE 0.1665), r = 0.99910, P = 1.2e-6.
    let t = e.call("analysis_create_linear_regression", json!({})).unwrap().text.unwrap();
    close(&csv_row(&t, "Slope")[1], 2.02, 1e-9);
    close(&csv_row(&t, "Y-intercept")[1], -0.02, 1e-9);
    // Std. Error rows come second.
    let se: Vec<&str> = t.lines().filter(|l| l.trim_start_matches('"').trim_start().starts_with("Slope")).collect();
    assert!(se[1].contains("0.04276"), "{t}");
    // Regression on a quick graph made its final graph, which draws the line.
    let fin = e.project.graphs.iter().find(|g| g.is_final()).unwrap().id.clone();
    assert_eq!(e.render_graph(&fin).unwrap().curves.len(), 1);
    let t = e.call("analysis_create_correlation", json!({ "graph_id": fin })).unwrap().text.unwrap();
    close(&csv_row(&t, "r")[1], 0.9991, 1e-4);
    assert!(csv_row(&t, "P (two-tailed)")[1].starts_with('<'), "{t}");
}

/// Noise-free 4PL data, then the same with noise (reference fit: SciPy curve_fit).
#[test]
fn four_parameter_fit_matches_reference_values() {
    require_r!();
    let x: Vec<f64> = (0..13).map(|i| -9.0 + 0.5 * i as f64).collect();
    let noisy = [11.057, 12.545, 11.309, 7.339, 17.464, 27.4, 48.389, 75.683, 86.346, 89.634, 89.768, 91.56, 87.771];
    let mut csv = String::from("LogDose,Response\n");
    for (a, b) in x.iter().zip(noisy) {
        csv.push_str(&format!("{a},{b}\n"));
    }
    let mut e = engine(&csv, "xy_points", "LogDose", "Response", None);
    let out = e.call("analysis_create_curve_fit", json!({ "model": "agonist_4pl", "output": { "digits": 6 } })).unwrap();
    let t = out.text.unwrap();
    // Bottom 10.8418 (SE 1.1231), Top 90.2513 (1.1262), LogEC50 -5.99471 (0.03483), Hill 1.23184 (0.10723).
    close(&csv_row(&t, "Bottom")[1], 10.84184, 1e-3);
    close(&csv_row(&t, "Top")[1], 90.25131, 1e-3);
    close(&csv_row(&t, "LogEC50")[1], -5.994707, 1e-4);
    close(&csv_row(&t, "HillSlope")[1], 1.231836, 1e-4);
    close(&csv_row(&t, "Sy.x")[1], 2.210432, 1e-4);
    let se: Vec<&str> = t.lines().filter(|l| l.trim_start_matches('"').trim_start().starts_with("LogEC50")).collect();
    assert!(se[1].contains("0.0348"), "{t}");
    // EC50 = 10^LogEC50.
    close(&csv_row(&t, "EC50")[1], 10f64.powf(-5.994707), 1e-11);
    let fin = e.project.graphs.iter().find(|g| g.is_final()).unwrap().id.clone();
    let r = e.render_graph(&fin).unwrap();
    assert_eq!(r.curves.len(), 1);
    assert!(r.curves[0].points.len() > 100);

    // Constraining Bottom = 0 and Top = 100 leaves two fitted parameters.
    let t = e.call("analysis_edit_curve_fit", json!({ "fixed": [{ "name": "Bottom", "value": 0 }, { "name": "Top", "value": 100 }] })).unwrap().text.unwrap();
    assert_eq!(csv_row(&t, "Degrees of Freedom")[1], "11");
    assert!(csv_row(&t, "Bottom")[1].starts_with("= 0"), "{t}");
    // An unknown parameter is refused.
    assert!(e.call("analysis_edit_curve_fit", json!({ "fixed": [{ "name": "Plateau", "value": 1 }] })).is_err());
    // The R script reproduces it with the data embedded; it is written to a file, not returned.
    let path = std::env::temp_dir().join("otto-test-curve-fit.R");
    let out = e.call("analysis_export_r_script", json!({ "path": path.to_string_lossy() })).unwrap();
    assert!(out.message.contains("Saved the R script"), "{}", out.message);
    let script = std::fs::read_to_string(&path).unwrap();
    assert!(script.contains("op_curve_fit <- function") && script.contains("\"agonist_4pl\"") && script.contains("48.389"));
    let _ = std::fs::remove_file(&path);
}

/// Otto's model list and R's agree on every model's parameters (recovery of each model's
/// parameters is checked in otto-r/tests/cross_check.R).
#[test]
fn models_list_the_same_parameters_in_r_and_otto() {
    require_r!();
    use otto_core::analysis::FitModel;
    let models = otto_r::call(&otto_r::Request::new("models", json!(null), json!(null))).unwrap().result;
    for m in FitModel::ALL {
        // R and Otto list the same parameters for every model.
        let r_params: Vec<String> = serde_json::from_value(models[m.id()]["params"].clone()).unwrap_or_else(|_| panic!("{} missing in R", m.id()));
        assert_eq!(r_params, m.params(), "{}", m.id());
    }
}

/// Grouped data: 3 X categories × 2 data sets, 4 replicates (aov: F = 162.95, 275.27, 11.94).
fn grouped_csv() -> String {
    let y = [10.2, 11.1, 9.8, 10.6, 13.0, 12.4, 13.9, 12.8, 12.1, 11.7, 12.9, 12.4, 16.3, 15.2, 16.8, 15.9, 14.2, 13.8, 14.9, 14.1, 19.8, 20.4, 18.9, 19.6];
    let mut csv = String::from("Dose,Strain,Value\n");
    for (i, v) in y.iter().enumerate() {
        csv.push_str(&format!("x{},{},{v}\n", i / 8 + 1, if i % 8 < 4 { "WT" } else { "KO" }));
    }
    csv
}

#[test]
fn two_way_anova_matches_aov() {
    require_r!();
    let mut e = engine(&grouped_csv(), "grouped_interleaved", "Dose", "Value", Some("Strain"));
    let out = e.call("analysis_create_two_way_anova", json!({ "output": { "digits": 5 } })).unwrap();
    let t = out.text.unwrap();
    // From the ANOVA table rows ("Dose,109.9,2,54.95,\"F (2, 18) = 162.95\",P<0.0001").
    let f_of = |source: &str| -> f64 {
        let line = t.lines().find(|l| l.starts_with(source) && l.contains("F (")).unwrap_or_else(|| panic!("{t}"));
        line.split("= ").nth(1).unwrap().split('"').next().unwrap().parse().unwrap()
    };
    assert!(t.contains("Number of families: 1"), "{t}");
    assert!((f_of("Interaction") - 11.94).abs() < 0.01, "{t}");
    assert!((f_of("Dose") - 162.95).abs() < 0.01, "{t}");
    assert!((f_of("Strain") - 275.27).abs() < 0.01, "{t}");
    // WT vs KO within each dose: Sidak (one comparison per dose), all significant.
    assert!(t.contains("Šídák") && t.contains("WT vs. KO"), "{t}");
    let fin = e.project.graphs.iter().find(|g| g.is_final()).unwrap().id.clone();
    assert_eq!(e.render_graph(&fin).unwrap().brackets.len(), 3);
    // A bar graph without data sets can't be analysed this way.
    let mut e = engine(THREE_GROUPS, "column_bar", "Group", "Value", None);
    assert!(e.call("analysis_create_two_way_anova", json!({})).is_err());
}

const AML: &str = "9,13,13,18,23,28,31,34,45,48,161,5,5,8,8,12,16,23,27,30,33,43,45";
const AML_STATUS: &str = "1,1,0,1,1,0,1,1,0,1,0,1,1,1,1,1,0,1,1,1,1,1,1";

fn aml_csv() -> String {
    let mut csv = String::from("Weeks,Relapse,Group\n");
    for (i, (t, s)) in AML.split(',').zip(AML_STATUS.split(',')).enumerate() {
        csv.push_str(&format!("{t},{s},{}\n", if i < 11 { "Maintained" } else { "Nonmaintained" }));
    }
    csv
}

/// The acute myelogenous leukaemia data (Miller 1981; survival::aml): log-rank chi² 3.40,
/// P = 0.065; medians 31 and 23 weeks.
#[test]
fn kaplan_meier_matches_published_values() {
    require_r!();
    let mut e = Engine::new();
    e.call("table_import_text", json!({ "text": aml_csv(), "name": "AML" })).unwrap();
    // The time and event columns are found by themselves.
    e.call("graph_create", json!({ "kind": "survival" })).unwrap();
    let g = e.project.graphs[0].clone();
    assert_eq!((g.x.clone(), g.y.clone(), g.group.clone()), (vec!["Weeks".to_string()], vec!["Relapse".to_string()], vec!["Group".to_string()]));
    let r = e.render_graph(&g.id).unwrap();
    assert!(r.scene.items.iter().any(|i| matches!(i, otto_plot::scene::Item::Group { items, .. } if items.iter().any(|x| matches!(x, otto_plot::scene::Item::Group { id, .. } if id.starts_with("survival-"))))));
    let t = e.call("analysis_create_survival", json!({})).unwrap().text.unwrap();
    assert_eq!(value(&t, "Chi square"), "3.396");
    close(&value(&t, "P value"), 0.0653, 1e-4);
    assert!(t.contains("Gehan-Breslow-Wilcoxon test") && t.contains("Hazard Ratio (logrank)"), "{t}");
    assert!(csv_row(&t, "Maintained")[1] == "31" && csv_row(&t, "Nonmaintained")[1] == "23", "{t}");
    // The curve Otto draws is R's Kaplan-Meier estimate: 90.91, 81.82, 71.59, 61.36, 49.09 % for Maintained.
    let subjects: Vec<(f64, bool)> = AML.split(',').zip(AML_STATUS.split(',')).take(11).map(|(t, s)| (t.parse().unwrap(), s == "1")).collect();
    let steps: Vec<f64> = otto_plot::survival::kaplan_meier(&subjects).iter().filter(|s| s.events > 0).map(|s| s.survival).take(5).collect();
    for (a, b) in steps.iter().zip([0.9090909, 0.8181818, 0.7159091, 0.6136364, 0.4909091]) {
        assert!((a - b).abs() < 1e-7, "{steps:?}");
    }
    // An event column must hold 1 / 0.
    let mut e = Engine::new();
    e.call("table_import_text", json!({ "text": "T,E\n1,2\n3,0\n", "name": "Bad" })).unwrap();
    e.call("graph_create", json!({ "kind": "survival", "x": "T", "y": ["E"] })).unwrap();
    assert!(e.render_graph("G1").unwrap_err().to_string().contains("1 (event"));
}

#[test]
fn area_under_curve_with_replicates() {
    require_r!();
    // Two replicates at each X around 0, 2, 4, 2, 0: area 8; SE from the replicate SDs.
    let csv = "Time,Conc\n0,-0.5\n0,0.5\n1,1.5\n1,2.5\n2,3.5\n2,4.5\n3,1.5\n3,2.5\n4,-0.5\n4,0.5\n";
    let mut e = engine(csv, "xy_points", "Time", "Conc", None);
    let t = e.call("analysis_create_auc", json!({})).unwrap().text.unwrap();
    close(&csv_row(&t, "Total Area")[1], 8.0, 1e-9);
    // SD = 0.7071 at each X, n = 2: SEM 0.5; weights 0.5, 1, 1, 1, 0.5 -> SE = 0.5 * sqrt(3.5).
    close(&csv_row(&t, "Std. Error")[1], 0.5 * 3.5f64.sqrt(), 1e-3);
    assert_eq!(csv_row(&t, "Number of Peaks")[1], "1");
}

/// In the app, R works in the background: creating an analysis does not wait for it, and
/// the results appear once R is done.
#[test]
fn statistics_are_calculated_in_the_background() {
    require_r!();
    let mut e = engine(&grouped_csv(), "grouped_interleaved", "Dose", "Value", Some("Strain"));
    otto_r::set_background(true);
    let out = e.call("analysis_create_two_way_anova", json!({ "compare": "all_cells", "output": { "digits": 7 } })).unwrap();
    assert!(out.message.contains("calculating in R"), "{}", out.message);
    let spec = e.project.analyses[0].clone();
    assert!(matches!(otto_engine::results::compute(&e.project, &spec), Err(otto_engine::EngineError::Calculating)));
    let start = std::time::Instant::now();
    while otto_r::pending() > 0 && start.elapsed().as_secs() < 60 {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(otto_engine::results::compute(&e.project, &spec).is_ok());
    // Claude's tool calls wait for the results.
    let t = e.call("analysis_get_results", json!({})).unwrap().text.unwrap();
    assert!(t.contains("Tukey"), "{t}");
    otto_r::set_background(false);
}

/// Normality tests and outlier identification, against SciPy 1.18 reference values.
/// Group A holds one gross outlier (9.8 among values near 2.2); group B is clean.
#[test]
fn normality_and_outliers_match_reference_values() {
    require_r!();
    const CSV: &str = "Group,Value
A,2.1
A,2.4
A,2.0
A,2.3
A,2.2
A,2.5
A,2.1
A,9.8
                       B,5.1
B,5.4
B,4.8
B,5.6
B,5.0
B,5.3
B,4.9
B,5.2
";
    let mut e = engine(CSV, "box", "Group", "Value", None);

    // D'Agostino-Pearson: A K2 = 22.6583, P = 1.202e-05; B K2 = 0.26687, P = 0.8751.
    // Shapiro-Wilk: A W = 0.477628, P = 5.546e-06; B W = 0.983527, P = 0.97821.
    let t = e
        .call("analysis_create_normality", json!({ "tests": { "shapiro": true, "dagostino": true } }))
        .unwrap()
        .text
        .unwrap();
    let dagostino = t.split("## D'Agostino-Pearson").nth(1).expect("D'Agostino section");
    close(&csv_row(dagostino, "A")[2], 22.66, 0.01);
    assert_eq!(csv_row(dagostino, "A")[3], "<0.0001");
    close(&csv_row(dagostino, "B")[2], 0.2669, 1e-4);
    close(&csv_row(dagostino, "B")[3], 0.8751, 1e-4);
    let shapiro = t.split("## Shapiro-Wilk").nth(1).expect("Shapiro-Wilk section");
    close(&csv_row(shapiro, "A")[2], 0.4776, 1e-4);
    assert_eq!(csv_row(shapiro, "A")[5], "No", "A is not normal:
{t}");
    close(&csv_row(shapiro, "B")[2], 0.9835, 1e-4);
    close(&csv_row(shapiro, "B")[3], 0.9782, 1e-4);
    assert_eq!(csv_row(shapiro, "B")[5], "Yes", "B is normal:
{t}");

    // Grubbs on A: the 9.8 has G = 2.4701 against a critical value of 2.1266 (n = 8,
    // alpha = 0.05). B's most extreme value (5.6, G = 1.6390, P = 0.5885) is not an outlier.
    let t = e.call("analysis_create_outliers", json!({ "method": "esd", "alpha": 0.05 })).unwrap().text.unwrap();
    let row = csv_row(&t, "A,9.8");
    close(&row[2], 2.47, 0.01);
    close(&row[3], 2.127, 1e-3);
    assert_eq!(row[4], "<0.0001", "{t}");
    assert_eq!(row[5], "Yes", "{t}");
    let b = csv_row(&t, "B,5.6");
    close(&b[2], 1.639, 1e-3);
    close(&b[4], 0.5885, 1e-4);
    assert_eq!(b[5], "No", "{t}");

    // Tukey's rule finds the same value by a different route, with no P value.
    let t = e.call("analysis_create_outliers", json!({ "method": "tukey" })).unwrap().text.unwrap();
    assert_eq!(csv_row(&t, "A,9.8")[5], "Yes", "{t}");
}

/// A paired t test reports how effective the pairing was (Prism's section of that name):
/// the correlation between the two data sets with its one-tailed P value.
#[test]
fn paired_t_test_reports_the_pairing() {
    require_r!();
    let mut e = engine(TWO_GROUPS, "column_bar", "Group", "Value", None);
    let t = e
        .call("analysis_create_t_test", json!({ "data_sets": ["A", "B"], "design": "paired", "match_by": "Subject" }))
        .unwrap()
        .text
        .unwrap();
    assert!(t.contains("How effective was the pairing?"), "{t}");
    // r(A, B) = 1 exactly here (B = 2A), so the pairing is perfect and P is tiny.
    close(&value(&t, "Correlation coefficient (r)"), 1.0, 1e-6);
    assert_eq!(value(&t, "Was the pairing significantly effective? (P < 0.05)"), "Yes", "{t}");
}

/// Duplicating a final graph gives the copy its own locked table, named after the copy.
#[test]
fn duplicating_a_final_graph_renames_its_locked_table() {
    let mut e = engine(TWO_GROUPS, "column_bar", "Group", "Value", None);
    e.call("graph_finalize", json!({})).unwrap();
    let final_id = e.project.graphs.iter().find(|g| g.is_final()).unwrap().id.clone();
    e.call("graph_duplicate", json!({ "graph": final_id })).unwrap();
    for g in e.project.graphs.iter().filter(|g| g.is_final()) {
        let table = e.project.table_by_id(&g.table_id).unwrap();
        assert!(table.name.ends_with(&format!("(locked for {})", g.id)), "{} is named \"{}\"", g.id, table.name);
    }
}

/// A 2 x 2 table whose statistics are worked out by hand below, and a 2 x 3 one for the
/// test for trend.
const CURE: &str = "Treatment,Cured,Not cured\nDrug,23,7\nPlacebo,12,18\n";

#[test]
fn contingency_tables_match_reference_values() {
    require_r!();
    let mut e = Engine::new();
    e.call("table_import_text", json!({ "text": CURE, "name": "Cure" })).unwrap();
    e.call("graph_create", json!({ "kind": "contingency", "x": "Treatment", "y": ["Cured", "Not cured"] })).unwrap();
    let out = e.call("analysis_create_contingency", json!({ "results": { "digits": 4 } })).unwrap();
    let t = out.text.unwrap();

    // Expected counts are 17.5 / 12.5 in both rows, so
    // chi-square = 4 x 30.25 / (17.5 or 12.5) = 8.2971 on 1 df, P = 0.003969.
    close(&value(&t, "Chi square"), 8.2971, 1e-3);
    assert_eq!(value(&t, "df"), "1");
    close(&value(&t, "P value"), 0.003969, 1e-4);
    // Yates subtracts 0.5 from each |O - E|: chi-square = 4 x 25 / (17.5 or 12.5) = 6.8571.
    assert!(t.contains("Chi-square test with Yates' correction"));
    // Odds ratio (23 x 18) / (7 x 12) = 4.9286; relative risk (23/30) / (12/30) = 1.9167.
    close(&value(&t, "Odds ratio (Drug / Placebo)"), 4.9286, 1e-3);
    close(&value(&t, "Relative risk (Drug / Placebo)"), 1.9167, 1e-3);
    // Fisher's exact test is exact here, not simulated.
    assert!(t.contains("Fisher's exact test") && !t.contains("estimated by simulation"));
    // The diagnostic ratios, on the stated convention: 23 / (23 + 12) and 18 / (7 + 18).
    close(value(&t, "Sensitivity").split(' ').next().unwrap().trim_end_matches('%'), 65.71, 0.01);
    close(value(&t, "Specificity").split(' ').next().unwrap().trim_end_matches('%'), 72.0, 0.01);
    // Every count is well above 5, so no caution is raised.
    assert!(!t.contains("makes the chi-square approximation unreliable"), "{t}");
    assert_eq!(value(&t, "Are the rows and columns associated?"), "Yes");

    // McNemar needs the paired design to be asked for; it is off by default.
    assert!(!t.contains("McNemar"));
    let out = e.call("analysis_edit_contingency", json!({ "paired": true })).unwrap();
    assert!(out.text.unwrap().contains("McNemar's test (paired)"));
}

#[test]
fn contingency_refuses_what_is_not_a_count() {
    require_r!();
    let mut e = Engine::new();
    e.call("table_import_text", json!({ "text": "Dose,Alive,Dead\nLow,10.5,3\nHigh,4,9\n", "name": "T" })).unwrap();
    e.call("graph_create", json!({ "kind": "contingency", "x": "Dose", "y": ["Alive", "Dead"] })).unwrap();
    // A fraction is not a number of subjects, so the analysis says so instead of rounding.
    let err = e.call("analysis_create_contingency", json!({})).unwrap_err().to_string();
    assert!(err.contains("whole number"), "{err}");
}

#[test]
fn contingency_tests_for_trend_across_ordered_columns() {
    require_r!();
    let mut e = Engine::new();
    // Responders rise with the dose, so the trend test should see it.
    e.call("table_import_text", json!({ "text": "Outcome,Low,Medium,High\nResponded,5,12,22\nDid not,25,18,8\n", "name": "Dose" })).unwrap();
    e.call("graph_create", json!({ "kind": "contingency", "x": "Outcome", "y": ["Low", "Medium", "High"] })).unwrap();
    let t = e.call("analysis_create_contingency", json!({})).unwrap().text.unwrap();
    assert!(t.contains("Chi-square test for trend"), "{t}");
    assert_eq!(value(&t, "Assumption"), "The columns are ordered and equally spaced");
    // A 2 x 3 table has no odds ratio or relative risk: those need a 2 x 2.
    assert!(!t.contains("Odds ratio"), "{t}");
}

