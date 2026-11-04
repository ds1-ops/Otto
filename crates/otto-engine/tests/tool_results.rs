//! What a tool call hands back: a graph change that leaves the graph undrawable has to say
//! so as more than a turn of phrase, and a plotted table far too big for a tool result has
//! to go to a file instead of into the answer.

use serde_json::json;

use otto_engine::Engine;

fn engine(rows: usize) -> Engine {
    let mut e = Engine::new();
    let mut csv = String::from("Condition,Dose,Response\n");
    for r in 0..rows {
        csv.push_str(&format!("{},{},{}\n", ["Ctrl", "DrugA"][r % 2], r % 50, r % 37));
    }
    e.call("table_import_text", json!({ "text": csv, "name": "Doses" })).unwrap();
    e
}

/// A graph that cannot be drawn is reported, not refused: the change stands so the mapping
/// can be fixed one call at a time, and `needs_attention` carries why.
#[test]
fn an_undrawable_graph_says_so_and_keeps_the_change() {
    let mut e = engine(20);
    e.call("graph_create", json!({ "kind": "column_bar", "x": "Condition", "y": ["Response"] })).unwrap();

    // Otto fixes up what it can: switching to an XY graph swaps the categorical X for a
    // numeric one by itself, so that alone leaves nothing to report.
    let out = e.call("graph_set_kind", json!({ "graph": "G1", "kind": "xy_points" })).unwrap();
    assert!(out.needs_attention.is_none(), "{:?}", out.needs_attention);

    // Taking the X column away leaves a graph that cannot be drawn. The change still
    // stands — settings are made one call at a time — and the reason comes back with it.
    let out = e.call("graph_set_mapping", json!({ "graph": "G1", "x": "" })).unwrap();
    assert!(e.project.graph("G1").unwrap().x.is_empty(), "the change stands");
    let reason = out.needs_attention.as_deref().expect("an undrawable graph reports why");
    assert!(reason.contains("needs a numeric X column"), "{reason}");
    assert!(out.message.contains("needs attention"), "{}", out.message);

    // Giving it an X again clears the problem, with no rollback in between.
    let out = e.call("graph_set_mapping", json!({ "graph": "G1", "x": "Dose" })).unwrap();
    assert!(out.needs_attention.is_none(), "{:?}", out.needs_attention);
    assert!(out.message.contains("XY"), "{}", out.message);
}

/// The long layout is one row per plotted point. Past the cap the answer carries the head
/// and the path of a file with all of it, rather than the whole table.
#[test]
fn a_big_plotted_table_goes_to_a_file() {
    let mut e = engine(500);
    e.call("graph_create", json!({ "kind": "xy_scatter", "x": "Dose", "y": ["Response"] })).unwrap();

    let small = e.call("graph_get_plot_data", json!({ "graph": "G1", "layout": "summary" })).unwrap();
    let text = small.text.as_deref().unwrap();
    assert!(!text.contains("more rows in"), "a small table comes back whole: {}", small.message);

    let big = e.call("graph_get_plot_data", json!({ "graph": "G1", "layout": "long" })).unwrap();
    let text = big.text.as_deref().expect("the head of the table");
    assert!(big.message.contains("500 rows"), "{}", big.message);
    assert!(text.contains("more rows in"), "{text}");
    // 200 rows plus the header, plus the line naming the file.
    assert_eq!(text.lines().count(), 202, "only the head is inline");

    let path = std::env::temp_dir().join("otto-g1-plot-data.csv");
    let whole = std::fs::read_to_string(&path).expect("the file named in the message");
    assert_eq!(whole.lines().count(), 501, "the file has every row");
    let _ = std::fs::remove_file(&path);
}

/// A file with a title line above the table and a header spread over two rows: the sort of
/// thing that used to import as one text column per field, with no way to say otherwise.
#[test]
fn a_messy_file_previews_and_imports_with_options() {
    let dir = std::env::temp_dir().join("otto-import-test");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("plate.csv");
    std::fs::write(
        &path,
        "Plate 3 - run of 12 May,,\nWell,Treatment,Treatment\n,Low,High\nA1,1,5\nA2,2,7\nA3,3,9\n",
    )
    .unwrap();
    let mut e = Engine::new();
    let p = path.display().to_string();

    // As it stands, the title line is read as the column names.
    let out = e.call("table_import_preview", json!({ "path": p })).unwrap();
    let data = out.data.as_ref().unwrap();
    assert_eq!(data["columns"][0]["name"], "Plate 3 - run of 12 May");

    // Telling it what the first three rows are gives the names and types it should have.
    let options = json!({ "rows": ["skip", "name", "name"] });
    let out = e.call("table_import_preview", json!({ "path": p, "options": options })).unwrap();
    let data = out.data.as_ref().unwrap();
    let names: Vec<&str> = (0..3).map(|c| data["columns"][c]["name"].as_str().unwrap()).collect();
    assert_eq!(names, vec!["Well", "Treatment | Low", "Treatment | High"]);
    assert_eq!(data["columns"][1]["kind"], "numeric");
    assert_eq!(data["data_rows"], 3);
    assert_eq!(data["rows"][0]["role"], "skip");

    // Importing with the same options gives the table the preview promised.
    e.call("table_import_file", json!({ "path": p, "options": options, "name": "Plate 3" })).unwrap();
    let t = &e.project.tables[0];
    let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["Well", "Treatment | Low", "Treatment | High"]);
    assert_eq!(t.nrows, 3);
    assert_eq!(t.columns[2].number(2), Some(9.0));
    let _ = std::fs::remove_file(&path);
}

/// Dropping a column and forcing a type, through the tool rather than the dialog.
#[test]
fn import_options_can_drop_a_column_and_set_a_type() {
    let mut e = Engine::new();
    let dir = std::env::temp_dir().join("otto-import-test");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("codes.csv");
    std::fs::write(&path, "id,note,value\n1,ok,10\n2,ok,20\n").unwrap();
    let options = json!({ "columns": [{ "kind": "text" }, { "include": false }, {}] });
    e.call("table_import_file", json!({ "path": path.display().to_string(), "options": options })).unwrap();
    let t = &e.project.tables[0];
    let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["id", "value"]);
    assert_eq!(t.columns[0].kind(), otto_core::ColumnKind::Text);
    let _ = std::fs::remove_file(&path);
}

/// A range on a numeric column, which used to need a categorical column and an exact list.
#[test]
fn a_numeric_filter_keeps_a_range() {
    let mut e = Engine::new();
    let csv = "Condition,Dose,Response\nCtrl,1,5\nCtrl,10,8\nCtrl,100,20\nDrugA,1,6\nDrugA,10,9\nDrugA,100,25\n";
    e.call("table_import_text", json!({ "text": csv, "name": "Doses" })).unwrap();
    e.call("graph_create", json!({ "kind": "column_bar", "x": "Condition", "y": ["Response"] })).unwrap();
    let rows = |e: &Engine| e.plot_data("G1").unwrap().points.len();
    assert_eq!(rows(&e), 6);

    e.call("graph_set_filters", json!({ "graph": "G1", "filters": [{ "column": "Dose", "min": 5, "max": 50 }] })).unwrap();
    assert_eq!(rows(&e), 2, "only the two rows at dose 10");

    // One open end.
    e.call("graph_set_filters", json!({ "graph": "G1", "filters": [{ "column": "Dose", "min": 10 }] })).unwrap();
    assert_eq!(rows(&e), 4);

    // A range and a value list together.
    e.call(
        "graph_set_filters",
        json!({ "graph": "G1", "filters": [{ "column": "Dose", "min": 10 }, { "column": "Condition", "include": ["DrugA"] }] }),
    )
    .unwrap();
    assert_eq!(rows(&e), 2);

    // An empty list of filters puts every row back.
    e.call("graph_set_filters", json!({ "graph": "G1", "filters": [] })).unwrap();
    assert_eq!(rows(&e), 6);
}

#[test]
fn a_range_on_the_wrong_column_or_the_wrong_way_round_is_refused() {
    let mut e = Engine::new();
    e.call("table_import_text", json!({ "text": "Condition,Response\nCtrl,5\nDrugA,8\n", "name": "D" })).unwrap();
    e.call("graph_create", json!({ "kind": "column_bar", "x": "Condition", "y": ["Response"] })).unwrap();

    let err = e
        .call("graph_set_filters", json!({ "graph": "G1", "filters": [{ "column": "Condition", "min": 1 }] }))
        .unwrap_err()
        .to_string();
    assert!(err.contains("a range needs a numeric column"), "{err}");

    let err = e
        .call("graph_set_filters", json!({ "graph": "G1", "filters": [{ "column": "Response", "min": 9, "max": 2 }] }))
        .unwrap_err()
        .to_string();
    assert!(err.contains("range") && err.contains("empty"), "{err}");
}

/// Every analysis_edit_* takes graph_id, so an analysis can be moved to another graph of
/// the same kind. Four of them were missing it while their siblings had it.
#[test]
fn every_analysis_edit_can_move_to_another_graph() {
    if !otto_r::available() {
        eprintln!("skipped: the statistics engine (R with Otto's packages) is not set up");
        return;
    }
    let mut e = Engine::new();
    let csv = "Group,Dose,Response\nA,1,5\nA,10,8\nA,100,20\nB,1,6\nB,10,9\nB,100,25\n";
    e.call("table_import_text", json!({ "text": csv, "name": "Doses" })).unwrap();
    e.call("graph_create", json!({ "kind": "xy_points", "x": "Dose", "y": ["Response"], "group": "Group" })).unwrap();
    e.call("graph_duplicate", json!({ "graph": "G1", "name": "Second" })).unwrap();
    // Statistics live on final graphs, so the one to move to has to be finalized first.
    e.call("graph_finalize", json!({ "graph": "G2" })).unwrap();
    let target = e.project.graphs.last().expect("the final graph").id.clone();

    // Each of these makes its own final graph from G1; moving it to G2's final graph is
    // the same edit for every kind.
    for (create, edit) in [
        ("analysis_create_linear_regression", "analysis_edit_linear_regression"),
        ("analysis_create_correlation", "analysis_edit_correlation"),
        ("analysis_create_auc", "analysis_edit_auc"),
    ] {
        e.call(create, json!({ "graph_id": "G1" })).unwrap();
        let id = e.project.analyses.last().expect("the new analysis").id.clone();
        let moved = e.call(edit, json!({ "analysis": &id, "graph_id": &target }));
        assert!(moved.is_ok(), "{edit} should take graph_id: {:?}", moved.err());
        let kind = &e.project.analyses.iter().find(|a| a.id == id).unwrap().kind;
        assert_eq!(kind.graph_id(), Some(target.as_str()), "{edit} moved the analysis");
    }
}

/// The new graph kinds go through graph_create, render and export like any other.
#[test]
fn new_kinds_create_render_and_export() {
    let csv = "Treatment,Outcome,N\nDrug,Cured,23\nDrug,Not cured,7\nPlacebo,Cured,12\nPlacebo,Not cured,18\n";
    for kind in ["stacked_bar", "stacked_percent", "pie", "donut", "contingency"] {
        let mut e = Engine::new();
        e.call("table_import_text", json!({ "text": csv, "name": "T" })).unwrap();
        let out = e
            .call("graph_create", json!({ "kind": kind, "x": "Treatment", "y": ["N"], "group": "Outcome" }))
            .unwrap_or_else(|err| panic!("{kind}: {err}"));
        // A graph that cannot be drawn says so on the outcome rather than looking fine.
        assert!(out.needs_attention.is_none(), "{kind}: {:?}", out.needs_attention);
        e.call("graph_render_preview", json!({})).unwrap_or_else(|err| panic!("{kind} preview: {err}"));
        e.call("graph_export_r_code", json!({})).unwrap_or_else(|err| panic!("{kind} r code: {err}"));
        e.call("graph_get_plot_data", json!({ "layout": "wide" })).unwrap_or_else(|err| panic!("{kind} data: {err}"));
    }
}
