use serde_json::json;

use super::*;

/// Tests that run statistics need the R engine; without it they are skipped with a note.
macro_rules! require_r {
    () => {
        if !otto_r::available() {
            eprintln!("skipped: the statistics engine (R with Otto's packages) is not set up");
            return;
        }
    };
}

fn sample_path() -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/data.xlsx").display().to_string()
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("otto-engine-test-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn engine_with_data() -> Engine {
    let mut e = Engine::new();
    e.call("table_import_file", json!({ "path": sample_path() })).unwrap();
    e
}

#[test]
fn import_create_graph_and_preview() {
    let mut e = engine_with_data();
    assert_eq!(e.view, View::Table("T1".into()));
    let out = e
        .call("graph_create", json!({ "kind": "grouped_interleaved", "x": "condition", "y": ["IL2"], "group": "Additive" }))
        .unwrap();
    assert!(out.message.contains("G1"), "{}", out.message);
    assert_eq!(e.view, View::Graph("G1".into()));
    // Column names are normalized to their real spelling.
    assert_eq!(e.project.graphs[0].x, vec!["Condition"]);
    let prev = e.call("graph_render_preview", json!({})).unwrap();
    assert!(prev.image_png.unwrap().starts_with(&[0x89, b'P', b'N', b'G']));
}

#[test]
fn auto_mapping_picks_sensible_columns() {
    let mut e = engine_with_data();
    e.call("graph_create", json!({ "kind": "column_bar" })).unwrap();
    let g = &e.project.graphs[0];
    assert_eq!(g.x, vec!["Condition"]);
    assert_eq!(g.y, vec!["IL2"]);
    e.call("graph_create", json!({ "kind": "grouped_separated", "y": ["IL7"] })).unwrap();
    assert_eq!(e.project.graphs[1].group, vec!["Additive"]);
}

#[test]
fn undo_redo_and_failed_commands() {
    let mut e = engine_with_data();
    e.call("graph_create", json!({ "kind": "column_bar", "x": "Condition", "y": ["IL2"] })).unwrap();
    e.call("graph_set_format", json!({ "format": "wide" })).unwrap();
    assert_eq!(e.project.graphs[0].format, otto_core::Format::Wide);
    e.call("undo", json!({})).unwrap();
    assert_eq!(e.project.graphs[0].format, otto_core::Format::Square);
    e.call("redo", json!({})).unwrap();
    assert_eq!(e.project.graphs[0].format, otto_core::Format::Wide);
    // A failing command changes nothing.
    let before = e.project.clone();
    assert!(e.call("graph_set_mapping", json!({ "x": "Nope" })).is_err());
    assert_eq!(e.project, before);
    assert!(e.call("graph_set_axis", json!({ "axis": "y", "min": 5, "max": 1 })).is_err());
    assert_eq!(e.project, before);
}

#[test]
fn rename_column_and_level_follow_through() {
    let mut e = engine_with_data();
    e.call(
        "graph_create",
        json!({ "kind": "grouped_interleaved", "x": "Condition", "y": ["IL2"], "group": "Additive",
                "filters": [{ "column": "Additive", "include": ["(none)", "IL12"] }] }),
    )
    .unwrap();
    e.call("table_rename_column", json!({ "column": "Additive", "new_name": "Cytokine" })).unwrap();
    e.call("table_rename_level", json!({ "column": "Cytokine", "level": "(none)", "new_name": "None" })).unwrap();
    let g = &e.project.graphs[0];
    assert_eq!(g.group, vec!["Cytokine"]);
    assert_eq!(g.filters[0].include, vec!["None", "IL12"]);
    let r = e.render_graph("G1").unwrap();
    assert_eq!(r.data.sets, vec!["None", "IL12"]);
}

#[test]
fn project_roundtrip_and_exports() {
    let dir = temp_dir("roundtrip");
    let mut e = engine_with_data();
    e.call("graph_create", json!({ "kind": "column_bar", "x": "Condition", "y": ["IL2"] })).unwrap();
    let proj = dir.join("exp1.otto");
    e.call("project_save_as", json!({ "path": proj.display().to_string() })).unwrap();
    assert!(!e.dirty);
    for ext in ["svg", "pdf", "png"] {
        let p = dir.join(format!("g.{ext}"));
        e.call("graph_export_file", json!({ "path": p.display().to_string() })).unwrap();
        assert!(std::fs::metadata(&p).unwrap().len() > 100, "{ext}");
    }
    #[cfg(windows)]
    {
        let p = dir.join("g.emf");
        e.call("graph_export_file", json!({ "path": p.display().to_string() })).unwrap();
        assert!(p.exists());
    }
    let csv = dir.join("data.csv");
    e.call("graph_export_data", json!({ "path": csv.display().to_string(), "layout": "wide" })).unwrap();
    assert!(std::fs::read_to_string(&csv).unwrap().starts_with("Construct A"));
    let code = e.call("graph_export_r_code", json!({ "path": dir.join("g").display().to_string() })).unwrap();
    assert!(code.text.unwrap().contains("ggplot("));
    assert!(dir.join("g.txt").exists());

    let mut e2 = Engine::new();
    e2.call("project_open", json!({ "path": proj.display().to_string() })).unwrap();
    assert_eq!(e2.project, e.project);
    assert_eq!(e2.view, View::Graph("G1".into()));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn descriptive_results() {
    let mut e = engine_with_data();
    let out = e.call("analysis_create_descriptive", json!({ "y": ["IL2"], "group": "Condition" })).unwrap();
    let text = out.text.unwrap();
    let header = text.lines().position(|l| l.starts_with("Column,Condition,N,Mean")).expect("table header");
    assert_eq!(text.lines().skip(header + 1).filter(|l| !l.is_empty()).count(), 6);
    assert_eq!(e.view, View::Result("R1".into()));
}

#[test]
fn table_editing() {
    let mut e = Engine::new();
    e.call("table_create", json!({ "name": "Manual", "columns": [{ "name": "Group", "kind": "categorical" }, { "name": "Value" }], "rows": 2 }))
        .unwrap();
    e.call("table_set_cells", json!({ "row": 0, "column": "Group", "values": [["WT", 1.5], ["KO", 2], ["KO", 3]] })).unwrap();
    let t = &e.project.tables[0];
    assert_eq!(t.nrows, 3);
    assert_eq!(t.columns[1].number(2), Some(3.0));
    e.call("table_set_category_order", json!({ "column": "Group", "levels": ["KO", "WT"] })).unwrap();
    e.call("graph_create", json!({ "kind": "column_bar" })).unwrap();
    assert_eq!(e.render_graph("G1").unwrap().data.x_levels, vec!["KO", "WT"]);
    let txt = e.call("table_get", json!({ "format": "csv" })).unwrap().text.unwrap();
    assert!(txt.starts_with("Group,Value"));
    e.call("table_delete", json!({})).unwrap();
    assert!(e.project.graphs.is_empty());
}

#[test]
fn helpful_errors_for_ambiguous_targets() {
    let mut e = engine_with_data();
    e.call("table_import_text", json!({ "text": "a,b\n1,2\n" })).unwrap();
    e.view = View::Welcome;
    let err = e.call("table_describe", json!({})).unwrap_err().to_string();
    assert!(err.contains("which table") && err.contains("T1") && err.contains("T2"), "{err}");
    let err = e.call("graph_set_format", json!({ "graph": "G9", "format": "wide" })).unwrap_err().to_string();
    assert!(err.contains("G9"), "{err}");
}

#[test]
fn several_columns_for_x_and_group() {
    let mut e = engine_with_data();
    e.call("graph_create", json!({ "kind": "column_bar", "x": ["condition", "Additive"], "y": "IL2" })).unwrap_err();
    e.call("graph_create", json!({ "kind": "column_bar", "x": ["condition", "Additive"], "y": ["IL2"] })).unwrap();
    assert_eq!(e.project.graphs[0].x, vec!["Condition", "Additive"]);
    let r = e.render_graph("G1").unwrap();
    assert!(r.data.x_levels.contains(&"Construct A | IL12".to_string()));

    e.call("graph_create", json!({ "kind": "grouped_interleaved", "x": "Condition", "y": ["IL2"], "group": ["Additive", "Well"] })).unwrap();
    assert_eq!(e.project.graphs[1].group, vec!["Additive", "Well"]);
    // Moving a column to X removes it from Group by (and vice versa).
    e.call("graph_set_mapping", json!({ "x": ["Condition", "Additive"] })).unwrap();
    assert_eq!(e.project.graphs[1].group, vec!["Well"]);
    e.call("graph_set_mapping", json!({ "group": "Condition" })).unwrap();
    assert_eq!(e.project.graphs[1].x, vec!["Additive"]);
    e.call("graph_set_mapping", json!({ "group": [] })).unwrap();
    assert!(e.project.graphs[1].group.is_empty());
    e.call("graph_set_format", json!({ "format": "ultra_wide" })).unwrap();
    assert_eq!(e.project.graphs[1].format, otto_core::Format::UltraWide);
}

#[test]
fn switching_graph_type_drops_incompatible_x() {
    require_r!();
    let mut e = engine_with_data();
    e.call("graph_create", json!({ "kind": "xy_points", "x": "IL2", "y": ["IL7"] })).unwrap();
    assert_eq!(e.project.graphs[0].x, vec!["IL2"]);

    // XY -> column bar: the numeric X goes, a categorical one takes its place.
    let out = e.call("graph_set_kind", json!({ "kind": "column_bar" })).unwrap();
    assert_eq!(e.project.graphs[0].x, vec!["Condition"]);
    assert!(out.message.contains("replaced X 'IL2'"), "{}", out.message);
    assert!(e.render_graph("G1").is_ok());

    // Bars defined by two factors -> grouped keeps them.
    e.call("graph_set_mapping", json!({ "x": ["Condition", "Additive"] })).unwrap();
    e.call("graph_set_kind", json!({ "kind": "grouped_interleaved" })).unwrap();
    assert_eq!(e.project.graphs[0].x, vec!["Condition", "Additive"]);

    // Grouped -> XY: categorical X columns go, a numeric X (not a Y column) is picked.
    e.call("graph_set_kind", json!({ "kind": "xy_scatter" })).unwrap();
    assert_eq!(e.project.graphs[0].x, vec!["IL2"]);
    assert!(e.render_graph("G1").is_ok());

    // Column bar without X (one bar per Y column) stays that way when nothing was removed.
    e.call("graph_create", json!({ "kind": "column_bar", "x": "", "y": ["IL2", "IL7"] })).unwrap();
    e.call("graph_set_kind", json!({ "graph": "G2", "kind": "column_bar" })).unwrap();
    assert!(e.project.graphs[1].x.is_empty());
    // ...but a grouped graph needs an X, so one is picked.
    e.call("graph_set_kind", json!({ "graph": "G2", "kind": "grouped_separated" })).unwrap();
    assert_eq!(e.project.graphs[1].x, vec!["Condition"]);
    // Undo restores the previous mapping in one step.
    e.call("undo", json!({})).unwrap();
    assert!(e.project.graphs[1].x.is_empty());
}

/// Three conditions measured in the same four donors.
fn stats_engine() -> Engine {
    let mut e = Engine::new();
    let text = "Condition,Donor,Value\n\
        Ctrl,d1,10\nCtrl,d2,12\nCtrl,d3,11\nCtrl,d4,13\n\
        DrugA,d1,15\nDrugA,d2,18\nDrugA,d3,16\nDrugA,d4,19\n\
        DrugB,d1,30\nDrugB,d2,33\nDrugB,d3,31\nDrugB,d4,35\n";
    e.call("table_import_text", json!({ "text": text, "name": "Assay" })).unwrap();
    e.call("graph_create", json!({ "kind": "column_bar", "x": "Condition", "y": ["Value"] })).unwrap();
    e
}

/// Brackets drawn on the final graph G2 (made by the first analysis of quick graph G1).
fn significance_brackets(e: &Engine) -> usize {
    let r = e.render_graph("G2").unwrap();
    r.scene
        .items
        .iter()
        .filter_map(|i| match i {
            otto_plot::scene::Item::Group { id, items } if id == "significance" => Some(items.len() / 2),
            _ => None,
        })
        .sum()
}

#[test]
fn t_tests_run_and_draw_brackets() {
    require_r!();
    let mut e = stats_engine();
    let sets = e.call("analysis_list_data_sets", json!({})).unwrap();
    assert!(sets.message.contains("3 data sets"), "{}", sets.message);

    // Statistics on a quick graph make its final graph (G2, on a locked copy T2 of the data).
    let out = e.call("analysis_create_t_test", json!({ "data_sets": ["Ctrl", "DrugA"] })).unwrap();
    assert!(out.message.contains("Welch") && out.message.contains("Finalized \"Graph 1\" as G2"), "{}", out.message);
    let text = out.text.unwrap();
    assert!(text.contains("P value") && text.contains("Difference between means (Ctrl - DrugA)"), "{text}");
    // Welch's t test: t = -4.919, df = 5.4, P = 0.003565 (reference values from SciPy).
    assert!(text.contains("P value: 0.0036") && text.contains("Welch-corrected t, df: t=4.919, df=5.4"), "{text}");
    assert!(text.contains("Computed with R "), "{text}");
    assert_eq!(e.view, View::Graph("G2".into()));
    assert_eq!(e.project.analyses[0].name, "t test of Graph 1");
    assert_eq!(significance_brackets(&e), 1);
    // The quick graph stays free of statistics.
    assert!(e.render_graph("G1").unwrap().brackets.is_empty());

    // Paired needs a "Matched by" column.
    let err = e.call("analysis_edit_t_test", json!({ "data_sets": ["Ctrl", "DrugA"], "design": "paired" })).unwrap_err();
    assert!(err.to_string().contains("Matched by"), "{err}");
    let out = e
        .call("analysis_edit_t_test", json!({ "data_sets": ["Ctrl", "DrugA"], "design": "paired", "match_by": "donor" }))
        .unwrap();
    assert!(out.message.contains("Paired t test"), "{}", out.message);
    assert!(out.text.unwrap().contains("Number of pairs: 4"));

    // Nonparametric.
    let out = e.call("analysis_edit_t_test", json!({ "design": "unpaired", "distribution": "nonparametric" })).unwrap();
    assert!(out.message.contains("Mann-Whitney"));
    // Exactly two data sets are required.
    assert!(e.call("analysis_create_t_test", json!({ "data_sets": ["Ctrl"] })).is_err());
}

#[test]
fn one_way_anova_variants() {
    require_r!();
    let mut e = stats_engine();
    let out = e.call("analysis_create_one_way_anova", json!({})).unwrap();
    let text = out.text.unwrap();
    assert!(text.contains("Ordinary one-way ANOVA") && text.contains("Tukey's multiple comparisons test"), "{text}");
    assert!(text.contains("Ctrl vs. DrugB"), "{text}");
    // All three pairs differ clearly.
    assert_eq!(significance_brackets(&e), 3);

    // Dunnett vs. control.
    let out = e
        .call("analysis_edit_one_way_anova", json!({ "comparisons": { "mode": "vs_control", "control": "Ctrl" } }))
        .unwrap();
    assert!(out.text.unwrap().contains("Dunnett's multiple comparisons test"));
    assert_eq!(significance_brackets(&e), 2);

    // Brown-Forsythe and Welch -> Dunnett T3.
    let out = e.call("analysis_edit_one_way_anova", json!({ "equal_sd": false })).unwrap();
    let text = out.text.unwrap();
    assert!(text.contains("Welch's ANOVA test") && text.contains("Dunnett's T3"), "{text}");

    // Kruskal-Wallis with Dunn's test; FDR; repeated measures with Geisser-Greenhouse; Friedman.
    let out = e.call("analysis_edit_one_way_anova", json!({ "distribution": "nonparametric" })).unwrap();
    let text = out.text.unwrap();
    assert!(text.contains("Kruskal-Wallis test") && text.contains("Dunn's multiple comparisons test"), "{text}");
    let out = e.call("analysis_edit_one_way_anova", json!({ "correction": "fdr" })).unwrap();
    assert!(out.text.unwrap().contains("q value"));
    let out = e.call("analysis_edit_one_way_anova", json!({ "design": "matched", "match_by": "Donor", "distribution": "normal", "correction": "hypothesis" })).unwrap();
    let text = out.text.unwrap();
    assert!(text.contains("Geisser-Greenhouse's epsilon") && text.contains("Was the matching effective?"), "{text}");
    let out = e
        .call("analysis_edit_one_way_anova", json!({ "distribution": "nonparametric" }))
        .unwrap();
    assert!(out.text.unwrap().contains("Friedman test"));

    // Hide brackets / show ns.
    e.call("analysis_edit_one_way_anova", json!({ "graph": { "show": false } })).unwrap();
    assert_eq!(significance_brackets(&e), 0);
}

#[test]
fn a_graph_draws_one_analysis_at_a_time() {
    require_r!();
    let mut e = stats_engine();
    e.call("analysis_create_one_way_anova", json!({ "data_sets": ["Ctrl", "DrugA", "DrugB"] })).unwrap();
    assert_eq!(e.project.analyses[0].kind.drawn(), Some(true));
    // A second analysis on the same final graph takes the first one off the graph.
    e.call("analysis_create_t_test", json!({ "graph_id": "G2", "data_sets": ["Ctrl", "DrugA"] })).unwrap();
    assert_eq!(e.project.analyses[0].kind.drawn(), Some(false));
    assert_eq!(e.project.analyses[1].kind.drawn(), Some(true));
    assert_eq!(significance_brackets(&e), 1);
    // Showing the ANOVA again takes the t test off.
    e.call("analysis_edit_one_way_anova", json!({ "analysis": "R1", "graph": { "show": true } })).unwrap();
    assert_eq!(e.project.analyses[0].kind.drawn(), Some(true));
    assert_eq!(e.project.analyses[1].kind.drawn(), Some(false));
    assert_eq!(significance_brackets(&e), 3);
}

#[test]
fn analyses_follow_graph_and_level_changes() {
    require_r!();
    let mut e = stats_engine();
    e.call("analysis_create_one_way_anova", json!({ "data_sets": ["Ctrl", "DrugA", "DrugB"] })).unwrap();
    // Renaming a level in the data table doesn't reach the final graph's locked copy...
    e.call("table_rename_level", json!({ "table": "T1", "column": "Condition", "level": "Ctrl", "new_name": "Control" })).unwrap();
    let otto_core::AnalysisKind::OneWayAnova(a) = &e.project.analyses[0].kind else { panic!() };
    assert_eq!(a.data_sets[0], "Ctrl");
    // ...but relabelling the locked copy (a label for publication) is allowed and followed.
    e.call("table_rename_level", json!({ "table": "T2", "column": "Condition", "level": "Ctrl", "new_name": "Vehicle" })).unwrap();
    let otto_core::AnalysisKind::OneWayAnova(a) = &e.project.analyses[0].kind else { panic!() };
    assert_eq!(a.data_sets[0], "Vehicle");
    assert!(e.call("analysis_get_results", json!({})).unwrap().text.unwrap().contains("Vehicle vs. DrugB"));
    // Deleting the quick graph keeps the final graph and its statistics.
    e.call("graph_delete", json!({ "graph": "G1" })).unwrap();
    assert_eq!(e.project.analyses.len(), 1);
    // Deleting the final graph removes its statistics and its locked data.
    let out = e.call("graph_delete", json!({ "graph": "G2" })).unwrap();
    assert!(out.message.contains("1 analysis"), "{}", out.message);
    assert!(e.project.analyses.is_empty());
    assert!(e.project.table_by_id("T2").is_none());
}

#[test]
fn final_graphs_keep_a_locked_copy_of_the_data() {
    require_r!();
    let mut e = stats_engine();
    let out = e.call("graph_finalize", json!({ "graph": "G1" })).unwrap();
    assert!(out.message.contains("G2"), "{}", out.message);
    let g = e.project.graph("G2").unwrap();
    assert!(g.is_final() && g.table_id == "T2" && g.name == "Graph 1 (final)");
    let lock = g.locked.as_ref().unwrap();
    assert!(lock.from_graph == "G1" && lock.from_table == "Assay" && lock.at.ends_with(" UTC"), "{lock:?}");
    assert_eq!(e.view, View::Graph("G2".into()));

    // Edits to the data table change the quick graph, not the final graph.
    e.call("table_set_cells", json!({ "table": "T1", "row": 0, "column": "Value", "values": [[100]] })).unwrap();
    let top = |e: &Engine, g: &str| e.plot_data(g).unwrap().points.iter().map(|p| p.y).fold(f64::MIN, f64::max);
    assert_eq!(top(&e, "G1"), 100.0);
    assert_eq!(top(&e, "G2"), 35.0);

    // The locked copy refuses value, row and column edits; labels and bar order may change.
    for (tool, args) in [
        ("table_set_cells", json!({ "table": "T2", "row": 0, "column": "Value", "values": [[1]] })),
        ("table_add_rows", json!({ "table": "T2", "count": 1 })),
        ("table_delete_columns", json!({ "table": "T2", "columns": ["Value"] })),
        ("table_delete", json!({ "table": "T2" })),
    ] {
        let err = e.call(tool, args).unwrap_err();
        assert!(err.to_string().contains("locked data copy"), "{tool}: {err}");
    }
    e.call("table_set_category_order", json!({ "table": "T2", "column": "Condition", "levels": ["DrugB", "DrugA", "Ctrl"] })).unwrap();
    assert_eq!(e.plot_data("G2").unwrap().x_levels[0], "DrugB");

    // It isn't a data table: no quick graphs from it, final graphs can't switch table,
    // but a duplicate is an ordinary editable table.
    assert!(e.call("graph_create", json!({ "table": "T2", "kind": "column_bar" })).is_err());
    assert!(e.call("graph_set_mapping", json!({ "graph": "G2", "table": "T1" })).is_err());
    e.call("table_duplicate", json!({ "table": "T2" })).unwrap();
    assert!(e.project.table_by_id("T3").is_some_and(|t| t.locked_by.is_none()));
    // Data tables only count when guessing "the" table.
    e.call("table_delete", json!({ "table": "T3" })).unwrap();
    e.call("ui_navigate", json!({ "section": "tables" })).unwrap();
    assert_eq!(e.view, View::Table("T1".into()));

    // A duplicate of a final graph gets its own locked copy.
    e.call("graph_duplicate", json!({ "graph": "G2" })).unwrap();
    let copy = e.project.graphs.iter().find(|g| g.name == "Graph 1 (final) copy").unwrap();
    assert!(copy.is_final() && copy.table_id != "T2");
    assert_eq!(e.project.table_by_id(&copy.table_id).unwrap().locked_by.as_deref(), Some(copy.id.as_str()));
}

#[test]
fn deleting_tables_graphs_and_statistics() {
    require_r!();
    let mut e = two_readouts();
    e.call("analysis_create_t_test", json!({ "data_sets": ["Ctrl", "DrugA"] })).unwrap(); // G2 (final), T2, R1
    e.call("analysis_create_one_way_anova", json!({ "graph_id": "G2" })).unwrap(); // R2
    e.call("analysis_create_descriptive", json!({ "table": "T1" })).unwrap(); // R3

    // Duplicating a final graph copies its locked data and its statistics.
    let out = e.call("graph_duplicate", json!({ "graph": "G2" })).unwrap();
    assert!(out.message.contains("2 statistics result(s)"), "{}", out.message);
    let copy = e.project.graphs.last().unwrap().id.clone();
    assert_eq!(e.graph_analyses(&copy).len(), 2);
    assert!(e.out_of_date().is_empty());
    assert_eq!(e.render_graph(&copy).unwrap().brackets.len(), e.render_graph("G2").unwrap().brackets.len());

    // One statistics result can go on its own.
    e.call("analysis_delete", json!({ "analysis": "R1" })).unwrap();
    assert_eq!(e.graph_analyses("G2").len(), 1);

    // Deleting a final graph takes its final table and statistics; the quick graph and data stay.
    e.call("graph_delete", json!({ "graph": copy })).unwrap();
    assert!(e.project.tables.iter().all(|t| t.locked_by.as_deref() != Some(copy.as_str())));
    assert!(e.project.graph("G1").is_some() && e.project.table_by_id("T1").is_some());

    // Deleting a quick graph leaves its final graphs and the data table.
    e.call("graph_delete", json!({ "graph": "G1" })).unwrap();
    assert!(e.project.graph("G2").is_some() && e.project.table_by_id("T1").is_some());

    // Deleting the data table takes every graph made from it, quick and final, and their results.
    e.call("graph_create", json!({ "table": "T1", "kind": "column_bar", "x": "Condition", "y": ["Value"] })).unwrap();
    let t1 = e.project.table_by_id("T1").unwrap().clone();
    let gone = e.table_deletion(&t1);
    assert_eq!((gone.quick.len(), gone.finals.len(), gone.results), (1, 1, 2));
    let out = e.call("table_delete", json!({ "table": "T1" })).unwrap();
    assert!(out.message.contains("1 quick graph, 1 final graph and 2 results"), "{}", out.message);
    assert!(e.project.graphs.is_empty() && e.project.analyses.is_empty() && e.project.tables.is_empty());
    // Undo brings it all back.
    e.call("undo", json!({})).unwrap();
    assert_eq!((e.project.graphs.len(), e.project.analyses.len()), (2, 2));
}

#[test]
fn older_projects_get_final_graphs_for_their_statistics() {
    require_r!();
    // An older project: a t test attached directly to quick graph G1.
    let mut e = stats_engine();
    e.call("analysis_create_t_test", json!({ "data_sets": ["Ctrl", "DrugA"] })).unwrap();
    let mut old = e.project.clone();
    old.graphs.retain(|g| g.id == "G1");
    old.tables.retain(|t| t.id == "T1");
    for a in &mut old.analyses {
        a.table_id = "T1".into();
        a.basis.as_mut().unwrap().table_id = "T1".into();
        if let otto_core::AnalysisKind::TTest(t) = &mut a.kind {
            t.graph_id = "G1".into();
        }
    }
    let path = temp_dir("legacy").join("old.otto");
    old.save(&path).unwrap();

    let mut e = Engine::new();
    let out = e.call("project_open", json!({ "path": path.display().to_string() })).unwrap();
    assert!(out.message.contains("moved to new final graphs"), "{}", out.message);
    let final_graph = e.project.graphs.iter().find(|g| g.is_final()).unwrap();
    assert_eq!(e.analysis_graph("R1"), Some(final_graph.id.clone()));
    assert!(e.out_of_date().is_empty(), "{:?}", e.out_of_date());
    assert_eq!(e.render_graph(&final_graph.id).unwrap().brackets.len(), 1);
    assert!(e.render_graph("G1").unwrap().brackets.is_empty());
    assert!(e.dirty);
}

#[test]
fn pasting_a_table_with_its_header_row() {
    let mut e = Engine::new();
    e.call("table_create", json!({})).unwrap();
    let block = json!([["Condition", "Donor", "IL2", "IL7"], ["Ctrl", "d1", 1.5, 3], ["Ctrl", "d2", "2", 4], ["Drug", "d1", 3, 5], ["Drug", "d2", "4,5", 6]]);
    let out = e.call("table_set_cells", json!({ "row": 0, "column": 0, "values": block, "header": true })).unwrap();
    assert!(out.message.contains("column names"), "{}", out.message);
    let t = &e.project.tables[0];
    let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["Condition", "Donor", "IL2", "IL7"]);
    // Empty columns take the type of what was pasted, as when importing.
    let kinds: Vec<otto_core::ColumnKind> = t.columns.iter().map(|c| c.kind()).collect();
    { use otto_core::ColumnKind::*; assert_eq!(kinds, vec![Categorical, Categorical, Numeric, Numeric]); }
    assert_eq!(t.columns[2].number(3), Some(4.5));
    // A header pasted over named columns renames them, and graphs follow.
    e.call("graph_create", json!({ "kind": "column_bar", "x": "Condition", "y": ["IL2"] })).unwrap();
    e.call("table_set_cells", json!({ "table": "T1", "row": 0, "column": 2, "values": [["IL-2"]], "header": true })).unwrap();
    assert_eq!(e.project.graphs[0].y, vec!["IL-2"]);
    // A clash with another column's name gets a unique name instead.
    e.call("table_set_cells", json!({ "table": "T1", "row": 0, "column": 3, "values": [["Donor"]], "header": true })).unwrap();
    assert_eq!(e.project.tables[0].columns[3].name, "Donor (2)");
}

#[test]
fn edits_only_change_given_fields() {
    require_r!();
    let mut e = stats_engine();
    e.call("analysis_create_t_test", json!({ "data_sets": ["Ctrl", "DrugB"], "confidence": 0.99, "output": { "digits": 3, "p_style": "apa" } }))
        .unwrap();
    e.call("analysis_edit_t_test", json!({ "one_tailed": true })).unwrap();
    let otto_core::AnalysisKind::TTest(t) = &e.project.analyses[0].kind else { panic!() };
    assert!(t.one_tailed);
    assert_eq!(t.confidence, 0.99);
    assert_eq!(t.data_sets, vec!["Ctrl", "DrugB"]);
    assert_eq!(t.output.p_style, otto_core::analysis::PStyle::Apa);
    // Undo restores the previous parameters.
    e.call("undo", json!({})).unwrap();
    let otto_core::AnalysisKind::TTest(t) = &e.project.analyses[0].kind else { panic!() };
    assert!(!t.one_tailed);
}

/// Like `stats_engine`, with a second readout.
fn two_readouts() -> Engine {
    let mut e = Engine::new();
    let text = "Condition,Donor,Value,Other\n\
        Ctrl,d1,10,1\nCtrl,d2,12,2\nCtrl,d3,11,1\nCtrl,d4,13,2\n\
        DrugA,d1,15,5\nDrugA,d2,18,6\nDrugA,d3,16,5\nDrugA,d4,19,7\n\
        DrugB,d1,30,9\nDrugB,d2,33,8\nDrugB,d3,31,9\nDrugB,d4,35,10\n";
    e.call("table_import_text", json!({ "text": text, "name": "Assay" })).unwrap();
    e.call("graph_create", json!({ "kind": "column_bar", "x": "Condition", "y": ["Value"] })).unwrap();
    e
}

#[test]
fn final_graphs_only_change_format_colors_and_statistics() {
    require_r!();
    let mut e = two_readouts();
    e.call("analysis_create_t_test", json!({ "data_sets": ["Ctrl", "DrugA"] })).unwrap();
    // Type, data, rows, error bars and axes were settled on the quick graph.
    for (tool, args) in [
        ("graph_set_kind", json!({ "graph": "G2", "kind": "grouped_separated" })),
        ("graph_set_mapping", json!({ "graph": "G2", "y": ["Other"] })),
        ("graph_set_filters", json!({ "graph": "G2", "filters": [{ "column": "Condition", "include": ["Ctrl"] }] })),
        ("graph_set_summary", json!({ "graph": "G2", "error": "sem" })),
        ("graph_set_axis", json!({ "graph": "G2", "axis": "y", "max": 50 })),
    ] {
        let err = e.call(tool, args).unwrap_err().to_string();
        assert!(err.contains("is a final graph") && err.contains("quick graph \"Graph 1\""), "{tool}: {err}");
    }
    // Format, colors, legend, axis titles and statistics can change.
    e.call("graph_set_axis", json!({ "graph": "G2", "axis": "y", "title": "IL-15 (pg/ml)" })).unwrap();
    e.call("graph_set_axis", json!({ "graph": "G2", "axis": "x", "title": "Treatment" })).unwrap();
    let g = e.project.graph("G2").unwrap();
    assert_eq!((g.x_axis.title.as_deref(), g.y_axis.title.as_deref()), (Some("Treatment"), Some("IL-15 (pg/ml)")));
    assert!(e.render_graph("G2").unwrap().data.y_title == "IL-15 (pg/ml)");
    e.call("graph_set_format", json!({ "graph": "G2", "format": "ultra_wide" })).unwrap();
    e.call("graph_set_palette", json!({ "graph": "G2", "palette": "grayscale" })).unwrap();
    e.call("graph_set_legend", json!({ "graph": "G2", "show": false })).unwrap();
    // Focus: only data sets the graph has, and highlight mode rides along with the palette.
    let sets = crate::column_tests::list_data_sets(&e.project, "G2").unwrap();
    let first = sets[0].0.clone();
    e.call("graph_set_focus", json!({ "graph": "G2", "focus": [&first] })).unwrap();
    e.call("graph_set_palette", json!({ "graph": "G2", "palette": "npg", "highlight": true })).unwrap();
    let g = e.project.graph("G2").unwrap();
    assert!(g.focus == vec![first] && g.highlight && g.palette == otto_core::Palette::Npg);
    let err = e.call("graph_set_focus", json!({ "graph": "G2", "focus": ["Nonsense"] })).unwrap_err().to_string();
    assert!(err.contains("not a data set"), "{err}");
    e.call("analysis_create_one_way_anova", json!({ "graph_id": "G2" })).unwrap();
    assert_eq!(e.graph_analyses("G2").len(), 2);
    // The quick graph stays fully editable.
    e.call("graph_set_mapping", json!({ "graph": "G1", "y": ["Other"] })).unwrap();
    assert!(e.out_of_date().is_empty());
}

/// A final graph changed outside Otto (e.g. an edited project file): the safety net
/// still hides statistics that no longer match and can recalculate them.
#[test]
fn out_of_date_statistics_are_hidden_and_recalculated() {
    require_r!();
    let mut e = two_readouts();
    e.call("analysis_create_t_test", json!({ "data_sets": ["Ctrl", "DrugA"] })).unwrap();
    e.call("analysis_create_one_way_anova", json!({ "graph_id": "G2" })).unwrap();
    assert!(significance_brackets(&e) > 0);
    let gi = e.project.graph_index("G2").unwrap();
    e.project.graphs[gi].y = vec!["Other".into()];
    let stale = e.out_of_date();
    assert_eq!(stale.len(), 2);
    assert!(stale[0].1.iter().any(|r| r == "Y Value → Other"), "{stale:?}");
    assert_eq!(significance_brackets(&e), 0);
    assert!(e.call("analysis_get_results", json!({ "analysis": "R1" })).unwrap_err().to_string().contains("out of date"));
    let out = e.call("analysis_recalculate", json!({ "graph": "G2" })).unwrap();
    assert!(out.message.contains("R1") && out.message.contains("R2"), "{}", out.message);
    assert!(e.out_of_date().is_empty() && significance_brackets(&e) > 0);

    // When the analysed bars are gone, a t test can't be recalculated, only set up again.
    let mut e = two_readouts();
    e.call("analysis_create_t_test", json!({ "data_sets": ["Ctrl", "DrugA"] })).unwrap();
    let gi = e.project.graph_index("G2").unwrap();
    e.project.graphs[gi].x = vec!["Donor".into()];
    let stale = e.out_of_date();
    assert!(stale[0].1.iter().any(|r| r.contains("no longer on the graph: Ctrl, DrugA")), "{stale:?}");
    let err = e.call("analysis_recalculate", json!({ "graph": "G2" })).unwrap_err();
    assert!(err.to_string().contains("set the t test up again"), "{err}");
    e.call("analysis_edit_t_test", json!({ "analysis": "R1", "data_sets": ["d1", "d2"] })).unwrap();
    assert!(e.out_of_date().is_empty());
    assert!(e.call("analysis_get_results", json!({ "analysis": "R1" })).unwrap().text.unwrap().contains("d1 - d2"));
}

/// Labels of a final graph's locked copy may change; its statistics follow.
#[test]
fn relabelling_a_final_graph_keeps_its_statistics_current() {
    require_r!();
    let mut e = two_readouts();
    e.call("analysis_create_t_test", json!({ "data_sets": ["Ctrl", "DrugA"] })).unwrap();
    e.call("table_rename_column", json!({ "table": "T2", "column": "Condition", "new_name": "Treatment" })).unwrap();
    e.call("table_rename_level", json!({ "table": "T2", "column": "Treatment", "level": "DrugA", "new_name": "A" })).unwrap();
    assert!(e.out_of_date().is_empty(), "{:?}", e.out_of_date());
    assert!(e.call("analysis_get_results", json!({ "analysis": "R1" })).unwrap().text.unwrap().contains("Ctrl - A"));
}

#[test]
fn one_bracket_per_pair_across_analyses() {
    require_r!();
    let mut e = stats_engine();
    e.call("analysis_create_t_test", json!({ "data_sets": ["Ctrl", "DrugA"] })).unwrap();
    e.call("analysis_create_one_way_anova", json!({})).unwrap();
    // A graph draws one analysis at a time, so only a project that already had two shown
    // (an older file) gets here: the pair they share is drawn once.
    e.project.analyses[0].kind.set_drawn(true);
    // t test: Ctrl–DrugA; ANOVA: three pairs including Ctrl–DrugA again -> 3 brackets.
    assert_eq!(significance_brackets(&e), 3);
}

#[test]
fn new_graph_settings() {
    let mut e = Engine::new();
    e.call("table_import_text", json!({ "text": "Gene,Group,log2FC,P,Value\nG1,A,2.5,0.001,3\nG2,A,-2.0,0.002,4\nG3,B,0.1,0.8,5\nG4,B,0.2,0.6,6\nG5,A,3.1,0.0001,7\nG6,B,-0.3,0.4,8\n", "name": "D" })).unwrap();

    // A log Y axis, and limits that a log axis cannot show.
    e.call("graph_create", json!({ "kind": "xy_points", "x": "log2FC", "y": ["Value"] })).unwrap();
    e.call("graph_set_axis", json!({ "axis": "y", "scale": "log10" })).unwrap();
    assert_eq!(e.project.graphs[0].y_axis.scale, otto_core::AxisScale::Log10);
    assert!(e.call("graph_set_axis", json!({ "axis": "y", "min": 0 })).is_err());

    // Symbols per data set, and back to circles.
    e.call("graph_set_symbols", json!({ "symbols": ["square"] })).unwrap();
    assert_eq!(e.project.graphs[0].symbol(0), otto_core::Symbol::Square);
    e.call("graph_set_symbols", json!({})).unwrap();
    assert!(e.project.graphs[0].symbols.is_empty());

    // A volcano plot with its cutoffs and a label column.
    e.call("graph_create", json!({ "kind": "volcano", "x": "log2FC", "y": ["P"] })).unwrap();
    let v = e.project.graphs[1].id.clone();
    e.call("graph_set_volcano", json!({ "graph": v, "fc_cutoff": 1.5, "p_cutoff": 0.01, "label": "Gene", "label_top": 3 })).unwrap();
    let o = &e.project.graphs[1].volcano;
    assert_eq!((o.fc_cutoff, o.p_cutoff, o.label.as_deref(), o.label_top), (1.5, 0.01, Some("Gene"), 3));
    assert!(e.call("graph_set_volcano", json!({ "graph": &v, "p_cutoff": 2 })).is_err());
    // Three data sets, the middle one unchanged.
    let data = otto_plot::distill::distill(&e.project.graphs[1], e.project.table_by_id(&e.project.graphs[1].table_id).unwrap()).unwrap();
    assert_eq!(data.sets, vec!["Down", "Not significant", "Up"]);
    assert_eq!(data.set_colors, vec![0, 1, 2]);

    // A QQ plot takes no X column.
    e.call("graph_create", json!({ "kind": "qq", "y": ["Value"], "group": "Group" })).unwrap();
    assert!(e.project.graphs[2].x.is_empty());

    // Box plots: whiskers, and the statistics a column graph has.
    e.call("graph_create", json!({ "kind": "box", "x": "Group", "y": ["Value"] })).unwrap();
    let b = e.project.graphs[3].id.clone();
    e.call("graph_set_summary", json!({ "graph": &b, "whiskers": "min_max" })).unwrap();
    assert_eq!(e.project.graphs[3].whiskers, otto_core::Whiskers::MinMax);
    let out = e.call("analysis_list_data_sets", json!({ "graph": &b })).unwrap();
    let sets = out.text.unwrap_or(out.message);
    assert!(sets.contains("2 data sets"), "a box plot has bars to compare: {sets}");

    // Heatmap decimals.
    e.call("graph_create", json!({ "kind": "heatmap", "x": "Group", "y": ["Value"] })).unwrap();
    let h = e.project.graphs[4].id.clone();
    assert_eq!(e.project.graphs[4].heatmap.digits(), 2);
    e.call("graph_set_heatmap", json!({ "graph": &h, "decimals": 3 })).unwrap();
    assert_eq!(e.project.graphs[4].heatmap.digits(), 3);
    assert!(e.call("graph_set_heatmap", json!({ "graph": &h, "decimals": 99 })).is_err());
}

/// A final graph can be brought back in step with its quick graph: fresh locked data and
/// the quick graph's current type, columns and ranges, while its own name and colours stay.
#[test]
fn updating_a_final_graph_from_its_quick_graph() {
    let mut e = Engine::new();
    e.call("table_import_text", json!({ "text": "Dose,Group,Value\n1,A,3\n1,B,4\n2,A,5\n2,B,6\n", "name": "D" })).unwrap();
    e.call("graph_create", json!({ "kind": "column_bar", "x": "Group", "y": ["Value"] })).unwrap();
    let quick = e.project.graphs[0].id.clone();
    e.call("graph_finalize", json!({ "graph": &quick })).unwrap();
    let fin = e.project.graphs[1].id.clone();
    let locked = e.project.graphs[1].table_id.clone();
    e.call("graph_rename", json!({ "graph": &fin, "name": "Figure 1A" })).unwrap();
    e.call("graph_set_palette", json!({ "graph": &fin, "palette": "grayscale" })).unwrap();

    // The data table and the quick graph both move on.
    e.call("table_set_cells", json!({ "table": "T1", "row": 0, "column": "Value", "values": [[42]] })).unwrap();
    e.call("graph_set_kind", json!({ "graph": &quick, "kind": "box" })).unwrap();
    e.call("graph_set_mapping", json!({ "graph": &quick, "x": ["Dose"] })).unwrap();
    e.call("graph_set_axis", json!({ "graph": &quick, "axis": "y", "scale": "log10" })).unwrap();
    e.call("graph_set_palette", json!({ "graph": &quick, "palette": "lancet", "highlight": true })).unwrap();
    let bar = crate::column_tests::list_data_sets(&e.project, &quick).unwrap()[0].0.clone();
    e.call("graph_set_focus", json!({ "graph": &quick, "focus": [&bar] })).unwrap();
    // The final graph is still as it was.
    assert_eq!(e.project.graphs[1].kind, otto_core::GraphKind::ColumnBar);
    assert_eq!(e.project.table_by_id(&locked).unwrap().columns.iter().find(|c| c.name == "Value").unwrap().number(0), Some(3.0));

    let out = e.call("graph_update_from_quick", json!({ "graph": &fin })).unwrap();
    assert!(out.message.contains("Updated \"Figure 1A\""), "{}", out.message);
    let g = &e.project.graphs[1];
    assert_eq!(g.kind, otto_core::GraphKind::Box);
    assert_eq!(g.x, vec!["Dose".to_string()]);
    assert_eq!(g.y_axis.scale, otto_core::AxisScale::Log10);
    // Colors come over with the rest: the theme, highlight mode and the data sets in focus.
    assert_eq!(g.palette, otto_core::Palette::Lancet);
    assert!(g.highlight && g.focus == vec![bar]);
    // What belongs to the final graph alone is untouched, and so is its locked table's id.
    assert_eq!(g.name, "Figure 1A");
    assert_eq!(g.table_id, locked);
    let t = e.project.table_by_id(&locked).unwrap();
    assert_eq!(t.locked_by.as_deref(), Some(fin.as_str()));
    assert_eq!(t.columns.iter().find(|c| c.name == "Value").unwrap().number(0), Some(42.0));

    // A quick graph has nothing to update from.
    assert!(e.call("graph_update_from_quick", json!({ "graph": &quick })).is_err());
}

/// A column graph hands its symbols out per bar: it has no data sets to tell apart.
#[test]
fn column_graphs_vary_symbols_per_bar() {
    let mut e = Engine::new();
    e.call("table_import_text", json!({ "text": "Group,Value\nA,3\nB,4\nC,5\n", "name": "D" })).unwrap();
    e.call("graph_create", json!({ "kind": "column_bar", "x": "Group", "y": ["Value"] })).unwrap();
    e.call("graph_set_symbols", json!({ "vary": true })).unwrap();
    assert_eq!(e.project.graphs[0].symbols.len(), 3, "one symbol per bar");
    assert_ne!(e.project.graphs[0].symbol(0), e.project.graphs[0].symbol(1));

    // Group by is kept on a column graph: it adds to each bar's combination.
    e.call("graph_set_mapping", json!({ "group": ["Group"], "x": [] })).unwrap();
    assert_eq!(e.project.graphs[0].group, vec!["Group".to_string()]);
    let g = &e.project.graphs[0];
    let data = otto_plot::distill::distill(g, e.project.table_by_id(&g.table_id).unwrap()).unwrap();
    assert_eq!(data.x_levels, vec!["A".to_string(), "B".into(), "C".into()]);
}
