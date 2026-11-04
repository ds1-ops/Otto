//! Builds Otto's example project from the CSV files in `data/examples`: one data table,
//! quick graph and final graph with its statistics per assay. Needs the R engine.
//!
//!   cargo run -p otto-engine --example example_project -- <data/examples> <out.otto> [preview dir]
//!
//! With a preview directory, also writes each final graph as PNG and its results as text.

use serde_json::json;

use otto_engine::Engine;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = std::path::PathBuf::from(args.get(1).map(String::as_str).unwrap_or("data/examples"));
    let out = args.get(2).cloned().unwrap_or_else(|| "Otto examples.otto".into());
    let file = |name: &str| std::path::absolute(dir.join(name)).unwrap().display().to_string();
    let mut e = Engine::new();
    let mut call = |tool: &str, args: serde_json::Value| match e.call(tool, args.clone()) {
        Ok(o) => println!("{tool}: {}", o.message),
        Err(err) => panic!("{tool} {args}: {err}"),
    };

    // 1. Column bar graph, one-way ANOVA with Dunnett's test against the vehicle.
    call("table_import_file", json!({ "path": file("1 Cytokine release (bar).csv"), "name": "Cytokine release" }));
    call("graph_create", json!({ "kind": "column_bar", "x": "Treatment", "y": ["IL-6 (pg/mL)"], "name": "IL-6 release" }));
    call("graph_set_summary", json!({ "error": "sd" }));
    call("analysis_create_one_way_anova", json!({ "comparisons": { "mode": "vs_control", "control": "Vehicle" } }));

    // 2. Grouped bar graph, two-way ANOVA: WT vs KO within each treatment.
    call("table_import_file", json!({ "path": file("2 TNF knockout (grouped bar).csv"), "name": "TNF knockout" }));
    call("graph_create", json!({ "kind": "grouped_interleaved", "x": "Treatment", "y": ["TNF-alpha (pg/mL)"], "group": "Genotype", "name": "TNF-alpha" }));
    call("graph_set_summary", json!({ "error": "sem" }));
    call("analysis_create_two_way_anova", json!({ "compare": "within_rows" }));

    // 3. Dose-response, 4PL inhibition fit (IC50) per compound.
    call("table_import_file", json!({ "path": file("3 Inhibitor potency (dose-response).csv"), "name": "Inhibitor potency" }));
    call("graph_create", json!({ "kind": "xy_points", "x": "log[Inhibitor] (M)", "y": ["Activity (% of control)"], "group": "Compound", "name": "Dose-response" }));
    call("graph_set_summary", json!({ "error": "sd" }));
    call("analysis_create_curve_fit", json!({ "model": "inhibitor_4pl" }));

    // 4. Tumor growth curves (mean ± SEM), area under the curve per group.
    call("table_import_file", json!({ "path": file("4 Tumor growth (xenograft).csv"), "name": "Tumor growth" }));
    call("graph_create", json!({ "kind": "xy_points_line", "x": "Day", "y": ["Tumor volume (mm3)"], "group": "Group", "name": "Tumor growth" }));
    call("graph_set_summary", json!({ "error": "sem" }));
    call("graph_set_axis", json!({ "axis": "y", "title": "Tumor volume (mm³)" }));
    call("analysis_create_auc", json!({}));

    // 5. Kaplan-Meier survival curves, log-rank and Gehan-Breslow-Wilcoxon tests.
    call("table_import_file", json!({ "path": file("5 Survival (Kaplan-Meier).csv"), "name": "Survival" }));
    call("graph_create", json!({ "kind": "survival", "x": "Days", "y": ["Death"], "group": "Group", "name": "Survival" }));
    call("analysis_create_survival", json!({}));

    call("ui_navigate", json!({ "section": "final_graphs" }));
    call("project_save_as", json!({ "path": std::path::absolute(&out).unwrap().display().to_string() }));

    if let Some(preview) = args.get(3) {
        std::fs::create_dir_all(preview).unwrap();
        for g in e.project.graphs.iter().filter(|g| g.is_final()) {
            let png = e.render_graph(&g.id).unwrap().png(300.0).unwrap();
            std::fs::write(std::path::Path::new(preview).join(format!("{}.png", g.name)), png).unwrap();
        }
        for a in &e.project.analyses {
            let report = otto_engine::results::compute(&e.project, a).unwrap();
            std::fs::write(std::path::Path::new(preview).join(format!("{}.txt", a.name)), report.to_text()).unwrap();
        }
    }
}
