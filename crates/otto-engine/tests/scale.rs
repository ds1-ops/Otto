//! Otto on a table the size of a real screening run. Every command takes an undo snapshot
//! of the whole project, so editing one cell must copy one column's rows and share all the
//! rest: the cost of a command has to follow what it changed, not how big the table is.
//! Checked by comparing allocations rather than clocks, so these cannot go flaky; the
//! timings are printed for a look by hand (`cargo test --release --test scale -- --nocapture`).
//!
//! Drawing a graph of this many points is a separate matter — the beeswarm layout and the
//! SVG still carry every point — so these tests keep to the data side.

use std::time::Instant;

use otto_core::{ColumnData, DataTable};
use otto_engine::Engine;

const ROWS: usize = 100_000;

/// A project with one wide table: three measurements, a condition and a donor.
fn big_table(rows: usize) -> Engine {
    let mut e = Engine::new();
    let mut csv = String::from("Donor,Condition,IL2,IFNg,TNF\n");
    for r in 0..rows {
        let cond = ["Ctrl", "DrugA", "DrugB"][r % 3];
        csv.push_str(&format!("D{},{cond},{},{},{}\n", r % 17, r % 97, r % 53, r % 31));
    }
    e.call("table_import_text", serde_json::json!({ "text": csv, "name": "Big" })).unwrap();
    e
}

/// Which allocation each column's cells live in: two tables sharing a pointer share rows.
fn cell_ptrs(t: &DataTable) -> Vec<*const u8> {
    t.columns
        .iter()
        .map(|c| match &c.data {
            ColumnData::Numeric { values, .. } => std::sync::Arc::as_ptr(values) as *const u8,
            ColumnData::Categorical { codes, .. } => std::sync::Arc::as_ptr(codes) as *const u8,
            ColumnData::Text { values } => std::sync::Arc::as_ptr(values) as *const u8,
        })
        .collect()
}

#[test]
fn editing_one_cell_copies_only_that_column() {
    let mut e = big_table(ROWS);
    assert_eq!(e.project.tables[0].nrows, ROWS);
    let before = cell_ptrs(&e.project.tables[0]);

    let t = Instant::now();
    e.call("table_set_cells", serde_json::json!({ "row": 7, "column": "IL2", "values": [[42.0]] })).unwrap();
    let edit = t.elapsed();

    let after = cell_ptrs(&e.project.tables[0]);
    let moved: Vec<usize> = (0..before.len()).filter(|&i| before[i] != after[i]).collect();
    assert_eq!(moved, vec![2], "only IL2 (column 2) should have been copied, not {moved:?}");
    assert_eq!(e.project.tables[0].columns[2].number(7), Some(42.0));

    // Undo restores the old value, which proves the snapshot really kept the old rows.
    e.call("undo", serde_json::json!({})).unwrap();
    assert_eq!(e.project.tables[0].columns[2].number(7), Some(7.0), "the value the CSV gave row 7");
    eprintln!("{ROWS} rows: one cell edit took {edit:?}");
}

/// A hundred edits in a row: a hundred snapshots, none of which may hold a copy of the
/// columns it did not touch.
#[test]
fn a_hundred_edits_stay_flat() {
    let mut e = big_table(ROWS);
    let before = cell_ptrs(&e.project.tables[0]);
    let t = Instant::now();
    for r in 0..100 {
        e.call("table_set_cells", serde_json::json!({ "row": r, "column": "IFNg", "values": [[1.0]] })).unwrap();
    }
    let edits = t.elapsed();
    let after = cell_ptrs(&e.project.tables[0]);
    let moved: Vec<usize> = (0..before.len()).filter(|&i| before[i] != after[i]).collect();
    assert_eq!(moved, vec![3], "only IFNg (column 3) should have been copied, not {moved:?}");
    eprintln!("{ROWS} rows: 100 cell edits took {edits:?}");
}

/// A column of sample IDs: every value distinct, so it comes in as text. Listing a text
/// column's distinct values backs the inspector and `table_describe`, and it used to ask a
/// growing `Vec` whether it already held each value — a hundred thousand of those is ten
/// billion comparisons. It is now a set lookup, so describing the table is a moment's work.
#[test]
fn describing_a_table_of_distinct_text_is_quick() {
    let mut e = Engine::new();
    let mut csv = String::from("Sample,Reading\n");
    for r in 0..ROWS {
        csv.push_str(&format!("S{r:06},{}\n", r % 97));
    }
    e.call("table_import_text", serde_json::json!({ "text": csv, "name": "Runs" })).unwrap();
    assert_eq!(e.project.tables[0].columns[0].kind(), otto_core::ColumnKind::Text);

    let t = Instant::now();
    let out = e.call("table_describe", serde_json::json!({ "table": "T1" })).unwrap();
    let describe = t.elapsed();

    assert_eq!(out.data.as_ref().unwrap()["columns"][0]["distinct"], ROWS);
    eprintln!("{ROWS} distinct text values: table_describe took {describe:?}");
    assert!(describe < std::time::Duration::from_secs(20), "describing {ROWS} rows took {describe:?}");
}

/// Changing how a graph looks says nothing about the data, so it must not copy any of it.
#[test]
fn restyling_a_graph_does_not_copy_the_rows() {
    let mut e = big_table(2_000);
    e.call("graph_create", serde_json::json!({ "kind": "column_bar", "x": "Condition", "y": ["IL2"] })).unwrap();
    let before = cell_ptrs(&e.project.tables[0]);
    e.call("graph_set_palette", serde_json::json!({ "graph": "G1", "palette": "nejm" })).unwrap();
    assert_eq!(before, cell_ptrs(&e.project.tables[0]), "a graph change must not touch the data");
}
