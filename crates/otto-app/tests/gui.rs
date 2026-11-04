//! Renders the real Otto window headlessly (tiny-skia) and clicks through it.
//! Screenshots are written to `target/otto-gallery/gui-*.png` for visual review.

use std::path::{Path, PathBuf};

use iced_test::simulator::Simulator;

use otto_app::app::{App, Message, Pending};
use otto_app::dialogs::{Dialog, DialogMsg};
use otto_app::{style, view};
use otto_core::GraphKind;
use otto_engine::{Command, View};

fn sample() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/data.xlsx")
}

fn snapshot(app: &App, name: &str) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/otto-gallery");
    std::fs::create_dir_all(&dir).unwrap();
    for entry in std::fs::read_dir(&dir).unwrap().flatten() {
        if is_snapshot_of(&entry.file_name().to_string_lossy(), name) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    let mut sim = Simulator::with_size(iced::Settings::default(), (1440.0, 900.0), view::view(app));
    let shot = sim.snapshot(&style::theme()).expect("snapshot");
    assert!(shot.matches_image(dir.join(format!("gui-{name}"))).unwrap());
}

#[test]
fn walk_through_the_app() {
    let (mut app, _) = App::new(None);
    snapshot(&app, "welcome");

    let (mut app2, _) = App::new(Some(sample()));
    assert_eq!(app2.engine.view, View::Table("T1".into()));
    let _ = app2.update(Message::CellClicked(2, 3));
    snapshot(&app2, "table");

    let _ = app2.update(Message::NewGraph(GraphKind::GroupedInterleaved));
    assert!(matches!(app2.engine.view, View::Graph(_)));
    assert!(app2.preview.as_ref().is_some_and(|p| p.result.is_ok()));
    snapshot(&app2, "graph");

    let _ = app2.update(Message::NewDescriptive);
    snapshot(&app2, "results");

    // Clicking a graph-type tile in the graph panel sends the matching command. The
    // section is folded shut to begin with, so it is opened first.
    let _ = app2.update(Message::Cmd(Command::UiNavigate(otto_engine::NavigateArgs {
        section: otto_engine::Section::Graphs,
        item: Some("G1".into()),
    })));
    let _ = app2.update(Message::TogglePanelSection("kind", true));
    let mut sim = Simulator::with_size(iced::Settings::default(), (1440.0, 900.0), view::view(&app2));
    sim.click("Column bar").expect("Column bar tile");
    let messages: Vec<Message> = sim.into_messages().collect();
    for m in messages {
        let _ = app2.update(m);
    }
    assert_eq!(app2.engine.project.graphs[0].kind, GraphKind::ColumnBar);

    // Bars defined by two columns, in the ultra-wide format.
    let _ = app2.update(Message::Cmd(Command::GraphSetMapping(otto_engine::GraphMappingArgs {
        x: Some(otto_core::OneOrMany::Many(vec!["Condition".into(), "Additive".into()])),
        ..Default::default()
    })));
    let _ = app2.update(Message::Cmd(Command::GraphSetFormat(otto_engine::GraphFormatArgs {
        graph: None,
        format: otto_core::Format::UltraWide,
    })));
    assert!(app2.preview.as_ref().is_some_and(|p| p.result.is_ok()));
    snapshot(&app2, "graph-combined");

    // Multi-select dropdown: open the Y picker, tick IL7 in the floating menu.
    let _ = app2.update(Message::ToggleDropdown("y".into()));
    snapshot(&app2, "graph-dropdown");
    let mut sim = Simulator::with_size(iced::Settings::default(), (1440.0, 900.0), view::view(&app2));
    sim.click("IL7").expect("IL7 in the open dropdown");
    for m in sim.into_messages().collect::<Vec<_>>() {
        let _ = app2.update(m);
    }
    assert_eq!(app2.engine.project.graphs[0].y, vec!["IL2", "IL7"]);
    assert_eq!(app2.open_dropdown.as_deref(), Some("y"), "menu stays open for more picks");
    let _ = app2.update(Message::CloseDropdown);

    // Editing a cell through the grid updates the table and the linked graph.
    let _ = app.update(Message::Cmd(Command::TableImportFile(otto_engine::ImportFileArgs {
        path: sample().display().to_string(),
        sheet: None,
        name: None,
        options: None,
    })));
    let _ = app.update(Message::CellDoubleClicked(0, 3));
    let _ = app.update(Message::CellInput("7.5".into()));
    let _ = app.update(Message::CellSubmit);
    assert_eq!(app.engine.project.tables[0].columns[3].number(0), Some(7.5));
    assert_eq!(app.grid.sel, Some((1, 3)));
}

/// The toolbar's color-theme menu picks the journal palette and highlight mode; the graph
/// panel picks the data sets in focus, which take the theme's red and blue.
#[test]
fn color_theme_menu_and_focus() {
    use otto_core::Palette;
    use otto_engine::{GraphFocusArgs, GraphPaletteArgs};
    let (mut app, _) = App::new(Some(sample()));
    let _ = app.update(Message::NewGraph(GraphKind::GroupedInterleaved));

    // The menu floats over the window with a swatch per theme.
    let _ = app.update(Message::ToggleDropdown(view::PALETTE_MENU.into()));
    snapshot(&app, "theme-menu");
    let mut sim = Simulator::with_size(iced::Settings::default(), (1440.0, 900.0), view::view(&app));
    sim.click("Lancet").expect("Lancet in the theme menu");
    for m in sim.into_messages().collect::<Vec<_>>() {
        let _ = app.update(m);
    }
    assert_eq!(app.engine.project.graphs[0].palette, Palette::Lancet);
    assert_eq!(app.open_dropdown.as_deref(), Some(view::PALETTE_MENU), "the menu stays open");
    let _ = app.update(Message::CloseDropdown);

    // Highlight mode is switched in the graph panel, next to the data sets it applies to,
    // inside the Appearance section (folded away until it is wanted).
    let _ = app.update(Message::TogglePanelSection("appearance", true));
    {
        let mut sim = Simulator::with_size(iced::Settings::default(), (380.0, 1700.0), otto_app::graph::panel(&app, "G1"));
        sim.click("Highlight mode").expect("the highlight switch");
        for m in sim.into_messages().collect::<Vec<_>>() {
            let _ = app.update(m);
        }
    }
    assert!(app.engine.project.graphs[0].highlight);
    assert_eq!(app.engine.project.graphs[0].palette, Palette::Lancet, "switching it keeps the theme");

    // Two data sets in focus: the first takes the theme's red, the second its blue, and
    // highlight mode grays the others out.
    let sets = app.preview.as_ref().expect("preview").sets.clone();
    assert!(sets.len() >= 3, "{sets:?}");
    let focus = vec![sets[2].clone(), sets[0].clone()];
    let _ = app.update(Message::Cmd(Command::GraphSetFocus(GraphFocusArgs { graph: None, focus: focus.clone() })));
    assert_eq!(app.engine.project.graphs[0].focus, focus);
    assert!(app.preview.as_ref().is_some_and(|p| p.result.is_ok()));
    let _ = app.update(Message::ToggleDropdown("focus".into()));
    {
        let mut sim = Simulator::with_size(iced::Settings::default(), (380.0, 1700.0), otto_app::graph::panel(&app, "G1"));
        let shot = sim.snapshot(&style::theme()).expect("snapshot");
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/otto-gallery");
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            if is_snapshot_of(&entry.file_name().to_string_lossy(), "focus-panel") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
        assert!(shot.matches_image(dir.join("gui-focus-panel")).unwrap());
    }

    // A theme that is not there, or a data set that is not, is refused rather than guessed.
    let _ = app.update(Message::Cmd(Command::GraphSetPalette(GraphPaletteArgs {
        graph: None,
        palette: Palette::Npg,
        highlight: Some(false),
    })));
    assert!(!app.engine.project.graphs[0].highlight && app.engine.project.graphs[0].palette == Palette::Npg);
    let _ = app.update(Message::Cmd(Command::GraphSetFocus(GraphFocusArgs { graph: None, focus: vec!["Nonsense".into()] })));
    assert_eq!(app.engine.project.graphs[0].focus, focus, "the focus is left as it was");
}

#[test]
fn analysis_dialogs() {
    if !otto_r::available() {
        eprintln!("skipped: the statistics engine (R with Otto's packages) is not set up");
        return;
    }
    use otto_app::dialogs::{Change, CompMode, Dialog, DialogMsg, Method, Tab};
    let text = "Condition,Donor,Value\n\
        Ctrl,d1,10\nCtrl,d2,12\nCtrl,d3,11\nCtrl,d4,13\n\
        DrugA,d1,15\nDrugA,d2,18\nDrugA,d3,16\nDrugA,d4,19\n\
        DrugB,d1,30\nDrugB,d2,33\nDrugB,d3,31\nDrugB,d4,35\n";
    let (mut app, _) = App::new(None);
    otto_r::set_background(false);
    let _ = app.update(Message::Cmd(Command::TableImportText(otto_engine::ImportTextArgs { text: text.into(), name: None, has_header: true, options: None })));
    let _ = app.update(Message::NewGraph(GraphKind::ColumnBar));

    // Analyze Data: all three bars ticked; a t test needs exactly two.
    let _ = app.update(Message::Dialog(DialogMsg::OpenAnalyze));
    snapshot(&app, "dialog-analyze");
    let _ = app.update(Message::Dialog(DialogMsg::AnalyzeOk));
    let Some(Dialog::Analyze(a)) = &app.dialog else { panic!("still analyze") };
    assert!(a.error.as_deref().unwrap_or("").contains("exactly two"));
    let _ = app.update(Message::Dialog(DialogMsg::Toggle("DrugB".into(), false)));
    let _ = app.update(Message::Dialog(DialogMsg::AnalyzeOk));
    assert!(matches!(app.dialog, Some(Dialog::Params(_))));
    snapshot(&app, "dialog-ttest-design");
    let _ = app.update(Message::Dialog(DialogMsg::Tab(Tab::Options)));
    snapshot(&app, "dialog-ttest-options");
    let _ = app.update(Message::Dialog(DialogMsg::ParamsOk));
    assert!(app.dialog.is_none());
    assert_eq!(app.engine.project.analyses.len(), 1);
    // Statistics on the quick graph made final graph G2, which is now shown.
    assert_eq!(app.engine.view, View::Graph("G2".into()));
    assert!(app.engine.project.graph("G2").unwrap().is_final());
    snapshot(&app, "results-ttest");

    // One-way ANOVA with Dunnett vs. control, from the graph.
    let _ = app.update(Message::Cmd(Command::UiNavigate(otto_engine::NavigateArgs { section: otto_engine::Section::FinalGraphs, item: None })));
    let _ = app.update(Message::Dialog(DialogMsg::OpenAnalyze));
    let _ = app.update(Message::Dialog(DialogMsg::PickMethod(Method::OneWayAnova)));
    let _ = app.update(Message::Dialog(DialogMsg::AnalyzeOk));
    let _ = app.update(Message::Dialog(DialogMsg::Tab(Tab::Comparisons)));
    let _ = app.update(Message::Dialog(DialogMsg::Change(Change::Mode(CompMode::VsControl))));
    snapshot(&app, "dialog-anova-comparisons");
    let _ = app.update(Message::Dialog(DialogMsg::Tab(Tab::Options)));
    snapshot(&app, "dialog-anova-options");
    let _ = app.update(Message::Dialog(DialogMsg::ParamsOk));
    assert!(app.dialog.is_none(), "{:?}", app.status);
    snapshot(&app, "results-anova");

    // The graph now carries the ANOVA's brackets; a graph draws one analysis at a time,
    // so making the ANOVA took the t test off it.
    let _ = app.update(Message::Cmd(Command::UiNavigate(otto_engine::NavigateArgs { section: otto_engine::Section::FinalGraphs, item: None })));
    let r = app.engine.render_graph("G2").unwrap();
    assert!(!r.brackets.is_empty());
    snapshot(&app, "graph-significance");

    // Reopen the ANOVA parameters from the result.
    let _ = app.update(Message::Dialog(DialogMsg::EditAnalysis("R2".into())));
    assert!(matches!(app.dialog, Some(Dialog::Params(ref p)) if p.editing.as_deref() == Some("R2")));
    let _ = app.update(Message::Dialog(DialogMsg::Cancel));
    assert!(app.dialog.is_none());
    stale_statistics(app);
}

/// The data-first workflow: highlight columns → quick graph → plotted table → final graph,
/// and a new table filled by pasting a table including its header row.
#[test]
fn quick_graphs_plotted_tables_and_final_graphs() {
    use otto_engine::{NavigateArgs, Section};
    let (mut app, _) = App::new(Some(sample()));
    app.settings_file = None;
    let t = app.engine.project.tables[0].clone();
    let col = |name: &str| t.columns.iter().position(|c| c.name == name).unwrap();

    // Click the IL7 header, then Ctrl+click IL15: both are highlighted, in that order.
    let _ = app.update(Message::SelectColumn(col("IL7")));
    app.modifiers = iced::keyboard::Modifiers::CTRL;
    let _ = app.update(Message::SelectColumn(col("IL15")));
    app.modifiers = iced::keyboard::Modifiers::default();
    assert_eq!(app.grid.highlight, vec![col("IL7"), col("IL15")]);
    snapshot(&app, "table-highlight");
    let _ = app.update(Message::NewGraph(GraphKind::ColumnBar));
    let g = &app.engine.project.graphs[0];
    assert_eq!(g.y, vec!["IL7", "IL15"]);
    // Two Y columns on a column bar graph: one bar per column, so no X.
    assert!(g.x.is_empty() && !g.is_final());
    assert!(app.preview.as_ref().is_some_and(|p| p.result.is_ok()));

    // The plotted table is built only when it is opened.
    assert!(app.plotted.is_none());
    let _ = app.update(Message::Cmd(Command::UiNavigate(NavigateArgs { section: Section::PlottedTables, item: Some("G1".into()) })));
    let table = app.plotted.as_ref().unwrap().result.as_ref().unwrap();
    // Prism layout: a block of 24 replicate subcolumns per bar, the values in one row.
    assert_eq!((table.headers[0].as_str(), table.headers[24].as_str(), table.headers.len()), ("IL7", "IL15", 48));
    assert_eq!(table.rows[0][0], "replicate 1");
    assert_eq!(table.rows.len(), 2);
    snapshot(&app, "plotted-table");

    // "without statistics" at the top of the quick graph: a final graph on a locked copy of the data.
    let _ = app.update(Message::Cmd(Command::UiNavigate(NavigateArgs { section: Section::Graphs, item: Some("G1".into()) })));
    snapshot(&app, "quick-graph");
    let mut sim = Simulator::with_size(iced::Settings::default(), (1440.0, 900.0), view::view(&app));
    sim.click("without statistics").expect("the finalize button at the top");
    for m in sim.into_messages().collect::<Vec<_>>() {
        let _ = app.update(m);
    }
    let fin = app.engine.project.graph("G2").unwrap();
    assert!(fin.is_final() && app.engine.project.table_by_id(&fin.table_id).unwrap().locked_by.is_some());
    assert_eq!(app.engine.view, View::Graph("G2".into()));
    // Axis titles stay editable on the final graph.
    let _ = app.update(Message::Draft(otto_app::app::Draft::AxisTitle("G2".into(), otto_engine::Axis::Y), "Cytokine (pg/ml)".into()));
    let _ = app.update(Message::Commit(otto_app::app::Draft::AxisTitle("G2".into(), otto_engine::Axis::Y)));
    assert_eq!(app.engine.project.graph("G2").unwrap().y_axis.title.as_deref(), Some("Cytokine (pg/ml)"), "{:?}", app.status);
    snapshot(&app, "final-graph");

    // A new table: click the first header and paste a table that includes its header row.
    let _ = app.update(Message::Cmd(Command::TableCreate(Default::default())));
    let View::Table(tid) = app.engine.view.clone() else { panic!("expected the new table") };
    let _ = app.update(Message::SelectColumn(0));
    let _ = app.update(Message::HeaderPaste(Some("Condition\tIL2\nCtrl\t1.5\nCtrl\t2\nDrug\t3\n".into())));
    let t = app.engine.project.table_by_id(&tid).unwrap();
    assert_eq!((t.columns[0].name.as_str(), t.columns[1].name.as_str()), ("Condition", "IL2"));
    assert_eq!(t.columns[1].kind(), otto_core::ColumnKind::Numeric);
    assert_eq!(t.columns[1].number(2), Some(3.0));
    // Double-click a header to rename it in place.
    let _ = app.update(Message::HeaderDoubleClicked(1));
    let _ = app.update(Message::HeaderInput("IL-2".into()));
    let _ = app.update(Message::HeaderSubmit);
    assert_eq!(app.engine.project.table_by_id(&tid).unwrap().columns[1].name, "IL-2");
    snapshot(&app, "table-pasted");

    // Several Y columns: a row per Y column, a block of replicates per X category.
    let _ = app.update(Message::Cmd(Command::GraphCreate(otto_engine::GraphCreateArgs {
        table: Some("T1".into()),
        kind: GraphKind::GroupedInterleaved,
        x: Some(otto_core::OneOrMany::Many(vec!["Condition".into(), "Additive".into()])),
        y: vec!["IL2".into(), "IL7".into(), "IL15".into()],
        group: None,
        filters: Vec::new(),
        format: None,
        name: None,
    })));
    let View::Graph(gid) = app.engine.view.clone() else { panic!("expected the new graph") };
    let _ = app.update(Message::Cmd(Command::UiNavigate(NavigateArgs { section: Section::PlottedTables, item: Some(gid) })));
    let table = app.plotted.as_ref().unwrap().result.as_ref().unwrap();
    assert_eq!(table.headers[1], "Construct A | (none)");
    let names: Vec<&str> = table.rows[1..].iter().map(|r| r[0].as_str()).collect();
    assert_eq!(names, vec!["IL2", "IL7", "IL15"]);
    snapshot(&app, "plotted-table-multi");
}

/// Where the hover × of a navigator entry sits (right end of the 232 px navigator).
const NAV_DELETE_X: f32 = 208.0;

/// Navigator entries show a × while hovered: delete graphs and results directly, delete a
/// data table only after a warning.
#[test]
fn navigator_delete_buttons() {
    if !otto_r::available() {
        eprintln!("skipped: the statistics engine (R with Otto's packages) is not set up");
        return;
    }
    use otto_app::dialogs::Dialog;
    use otto_engine::{ImportTextArgs, NavigateArgs, Section, TTestArgs};
    let text = "Condition,Donor,Value\nCtrl,d1,10\nCtrl,d2,12\nCtrl,d3,11\nDrugA,d1,15\nDrugA,d2,18\nDrugA,d3,16\n";
    let (mut app, _) = App::new(None);
    otto_r::set_background(false);
    app.settings_file = None;
    let _ = app.update(Message::Cmd(Command::TableImportText(ImportTextArgs { text: text.into(), name: Some("Assay".into()), has_header: true, options: None })));
    let _ = app.update(Message::NewGraph(GraphKind::ColumnBar));
    let _ = app.update(Message::Cmd(Command::GraphDuplicate(otto_engine::DuplicateGraphArgs { graph: Some("G1".into()), name: None })));
    let spec = otto_core::analysis::TTestSpec::new("G1", vec!["Ctrl".into(), "DrugA".into()]);
    let _ = app.update(Message::Cmd(Command::AnalysisCreateTTest(TTestArgs { name: None, spec })));
    let fin = app.engine.project.graphs.iter().find(|g| g.is_final()).unwrap().id.clone();
    let _ = app.update(Message::Cmd(Command::UiNavigate(NavigateArgs { section: Section::Tables, item: Some("T1".into()) })));

    // Point at the right end of the entry showing `label`: its × appears there.
    let at = |sim: &mut Simulator<'_, Message>, label: &str| {
        let target = sim.find(label).unwrap_or_else(|e| panic!("{label}: {e:?}"));
        sim.point_at(iced::Point::new(NAV_DELETE_X, target.bounds().center_y()));
    };
    let click_x = |app: &mut App, label: &str| {
        let mut sim = Simulator::with_size(iced::Settings::default(), (1440.0, 900.0), view::view(app));
        at(&mut sim, label);
        sim.simulate(iced_test::simulator::click());
        for m in sim.into_messages().collect::<Vec<_>>() {
            let _ = app.update(m);
        }
    };

    // The × only shows while hovering.
    {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/otto-gallery");
        let mut sim = Simulator::with_size(iced::Settings::default(), (1440.0, 900.0), view::view(&app));
        at(&mut sim, "Graph 1 copy");
        let shot = sim.snapshot(&style::theme()).expect("snapshot");
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            if is_snapshot_of(&entry.file_name().to_string_lossy(), "nav-hover-delete") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
        assert!(shot.matches_image(dir.join("gui-nav-hover-delete")).unwrap());
    }

    // Quick graph: deleted at once; its final graph and the data table stay.
    click_x(&mut app, "Graph 1 copy");
    assert!(app.engine.project.graphs.iter().all(|g| g.name != "Graph 1 copy"));
    assert!(app.engine.project.graph(&fin).is_some() && app.engine.project.table_by_id("T1").is_some());

    // A statistics result: deleted, its final graph stays.
    click_x(&mut app, "t test of Graph 1");
    assert!(app.engine.project.analyses.is_empty() && app.engine.project.graph(&fin).is_some());

    // Final graph: its final table goes too; the quick graph and the data table stay.
    click_x(&mut app, "Graph 1 (final)");
    assert!(app.engine.project.graph(&fin).is_none() && app.engine.project.tables.iter().all(|t| t.locked_by.is_none()));
    assert!(app.engine.project.graph("G1").is_some() && app.engine.project.table_by_id("T1").is_some());

    // Data table: asks first, then takes every graph made from it.
    click_x(&mut app, "Assay");
    assert!(matches!(app.dialog, Some(Dialog::ConfirmDeleteTable(_))));
    assert!(app.engine.project.table_by_id("T1").is_some());
    snapshot(&app, "dialog-delete-table");
    let mut sim = Simulator::with_size(iced::Settings::default(), (1440.0, 900.0), view::view(&app));
    sim.click("Delete").expect("the Delete button of the warning");
    for m in sim.into_messages().collect::<Vec<_>>() {
        let _ = app.update(m);
    }
    assert!(app.dialog.is_none());
    assert!(app.engine.project.tables.is_empty() && app.engine.project.graphs.is_empty());
}

#[test]
fn claude_mcp_switch() {
    let (mut app, _) = App::new(None);
    // Never write the user's real settings file from a test.
    app.settings_file = None;
    app.settings.mcp_enabled = true;
    let _ = app.update(Message::McpStarted(Ok("http://127.0.0.1:47800/mcp".into())));
    snapshot(&app, "mcp-on");

    let click = |app: &mut App, label: &str| {
        let mut sim = Simulator::with_size(iced::Settings::default(), (1440.0, 900.0), view::view(app));
        sim.click(label).unwrap_or_else(|e| panic!("{label}: {e:?}"));
        for m in sim.into_messages().collect::<Vec<_>>() {
            let _ = app.update(m);
        }
    };
    click(&mut app, "Claude MCP on");
    assert!(!app.settings.mcp_enabled && app.mcp.url.is_none());
    // The server stops with the subscription; a late start notice doesn't count it as on.
    let _ = app.update(Message::McpStarted(Ok("http://127.0.0.1:47800/mcp".into())));
    assert!(app.mcp.url.is_none());
    snapshot(&app, "mcp-off");

    click(&mut app, "Claude MCP off");
    assert!(app.settings.mcp_enabled);
}

/// The graph panel alone, tall enough to show every section.
/// The quick-graph panel: the graph type and the columns open, everything else folded, and
/// a numeric column filtered by a range rather than by listing its values.
#[test]
fn quick_graph_panel_folds_and_filters_by_range() {
    // The last column's name is far too long for the panel's 78 px label column: the
    // filter row has to cut it rather than wrap onto three lines.
    let text = "Condition,Dose,Response,Table3.Experimental group\n\
        Ctrl,1,5,A\nCtrl,10,8,A\nCtrl,100,20,A\n\
        DrugA,1,6,B\nDrugA,10,9,B\nDrugA,100,25,B\n";
    let (mut app, _) = App::new(None);
    let _ = app.update(Message::Cmd(Command::TableImportText(otto_engine::ImportTextArgs {
        text: text.into(),
        name: Some("Doses".into()),
        has_header: true,
        options: None,
    })));
    let _ = app.update(Message::NewGraph(GraphKind::ColumnBar));

    // Folded by default: the appearance settings are not on screen.
    {
        let mut sim = Simulator::with_size(iced::Settings::default(), (380.0, 1700.0), otto_app::graph::panel(&app, "G1"));
        assert!(sim.click("Highlight mode").is_err(), "Appearance starts folded");
    }

    // Typing a range into the Filters section keeps only the rows inside it.
    let _ = app.update(Message::TogglePanelSection("filters", true));
    let min = otto_app::app::Draft::FilterMin("G1".into(), "Dose".into());
    let _ = app.update(Message::Draft(min.clone(), "5".into()));
    let _ = app.update(Message::Commit(min));
    let f = &app.engine.project.graphs[0].filters;
    assert_eq!(f.len(), 1);
    assert_eq!((f[0].column.as_str(), f[0].min, f[0].max), ("Dose", Some(5.0), None));
    assert_eq!(app.engine.plot_data("G1").unwrap().points.len(), 4, "the two rows at dose 1 are out");

    let mut sim = Simulator::with_size(iced::Settings::default(), (380.0, 1700.0), otto_app::graph::panel(&app, "G1"));
    sim.click("APPEARANCE").expect("the Appearance section unfolds");
    for m in sim.into_messages().collect::<Vec<_>>() {
        let _ = app.update(m);
    }
    assert!(app.section_open("appearance", false), "clicking the title opens it");
    panel_snapshot_of(&app, "G1", "panel-quick-graph");
}

fn panel_snapshot(app: &App, name: &str) {
    panel_snapshot_of(app, "G2", name)
}

fn panel_snapshot_of(app: &App, graph: &str, name: &str) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/otto-gallery");
    let mut sim = Simulator::with_size(iced::Settings::default(), (380.0, 1700.0), otto_app::graph::panel(app, graph));
    let shot = sim.snapshot(&style::theme()).expect("snapshot");
    for entry in std::fs::read_dir(&dir).unwrap().flatten() {
        if is_snapshot_of(&entry.file_name().to_string_lossy(), name) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    assert!(shot.matches_image(dir.join(format!("gui-{name}"))).unwrap());
}

/// A final graph's statistics panel: show / hide toggles, fixed data, and the safety net
/// for statistics that no longer match (recalculate, set up again).
fn stale_statistics(mut app: App) {
    use otto_app::dialogs::{Dialog, DialogMsg};
    use otto_core::{AnalysisKind, Filter};
    use otto_engine::{GraphFiltersArgs, GraphFormatArgs};
    assert_eq!(app.engine.view, View::Graph("G2".into()));

    // The graph panel lists both analyses, with the ANOVA drawn and the t test off.
    let AnalysisKind::TTest(t) = &app.engine.project.analyses[0].kind else { panic!() };
    assert!(!t.graph.show);
    // Ticking the t test draws it instead and takes the ANOVA off: one at a time.
    let mut sim = Simulator::with_size(iced::Settings::default(), (380.0, 1700.0), otto_app::graph::panel(&app, "G2"));
    sim.click("t test of Graph 1").expect("show toggle of the t test");
    for m in sim.into_messages().collect::<Vec<_>>() {
        let _ = app.update(m);
    }
    assert_eq!(app.engine.project.analyses[0].kind.drawn(), Some(true));
    assert_eq!(app.engine.project.analyses[1].kind.drawn(), Some(false));
    assert_eq!(app.engine.view, View::Graph("G2".into()));
    panel_snapshot(&app, "panel-statistics");

    // A final graph's data are fixed: changing its rows is refused, nothing goes out of date.
    let _ = app.update(Message::Cmd(Command::GraphSetFilters(GraphFiltersArgs {
        graph: Some("G2".into()),
        filters: vec![Filter { column: "Condition".into(), include: vec!["Ctrl".into(), "DrugA".into()], ..Default::default() }],
    })));
    assert!(app.status.1 && app.status.0.contains("is a final graph"), "{:?}", app.status);
    assert!(app.dialog.is_none() && app.stale.is_empty());
    // Format and appearance still change.
    let _ = app.update(Message::Cmd(Command::GraphSetFormat(GraphFormatArgs { graph: Some("G2".into()), format: otto_core::Format::Wide })));
    assert!(!app.status.1, "{:?}", app.status);

    // Safety net: a final graph changed outside Otto (bars by donor) shows its statistics as out of date.
    let gi = app.engine.project.graph_index("G2").unwrap();
    app.engine.project.graphs[gi].x = vec!["Donor".into()];
    let _ = app.update(Message::Cmd(Command::GraphSetFormat(GraphFormatArgs { graph: Some("G2".into()), format: otto_core::Format::Square })));
    assert_eq!(app.stale.len(), 2);
    assert!(app.engine.render_graph("G2").unwrap().brackets.is_empty());
    snapshot(&app, "graph-stale");
    panel_snapshot(&app, "panel-stale");

    // Recalculate the ANOVA on the bars shown (the four donors).
    let _ = app.update(Message::Dialog(DialogMsg::Recalculate(Some("R2".into()))));
    let AnalysisKind::OneWayAnova(x) = &app.engine.project.analyses[1].kind else { panic!() };
    assert_eq!(x.data_sets, vec!["d1", "d2", "d3", "d4"]);
    assert!(!app.stale.contains_key("R2"));

    // Set the t test up again: Analyze Data opens on the same method with the new bars.
    let _ = app.update(Message::Dialog(DialogMsg::SetUpAgain("R1".into())));
    let Some(Dialog::Analyze(a)) = &app.dialog else { panic!("expected Analyze Data") };
    assert_eq!(a.replacing.as_deref(), Some("R1"));
    assert_eq!(a.checked.len(), 4);
    snapshot(&app, "dialog-setup-again");
    let _ = app.update(Message::Dialog(DialogMsg::Toggle("d3".into(), false)));
    let _ = app.update(Message::Dialog(DialogMsg::Toggle("d4".into(), false)));
    let _ = app.update(Message::Dialog(DialogMsg::AnalyzeOk));
    assert!(matches!(&app.dialog, Some(Dialog::Params(p)) if p.editing.as_deref() == Some("R1")));
    let _ = app.update(Message::Dialog(DialogMsg::ParamsOk));
    assert!(app.dialog.is_none(), "{:?}", app.status);
    assert!(app.stale.is_empty());
    let AnalysisKind::TTest(t) = &app.engine.project.analyses[0].kind else { panic!() };
    assert_eq!(t.data_sets, vec!["d1", "d2"]);
    assert_eq!(app.engine.view, View::Graph("G2".into()));
}


/// Whether `file` is the saved snapshot `name` ("gui-<name>-<renderer>.png"), and not
/// another snapshot whose name merely starts the same way.
/// The import wizard on a file Otto cannot read by itself: a title line above the table
/// and a header over two rows. Picking what each row is fixes the names and the types,
/// and OK imports exactly what the preview showed.
#[test]
fn import_wizard_sorts_out_a_messy_file() {
    use otto_app::dialogs::{Dialog, DialogMsg};
    use otto_core::import::RowRole;

    let dir = std::env::temp_dir().join("otto-gui-import");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("plate.csv");
    std::fs::write(
        &path,
        "Plate 3 - run of 12 May,,\nWell,Treatment,Treatment\n,Low,High\nA1,1,5\nA2,2,7\nA3,3,9\n",
    )
    .unwrap();

    let (mut app, _) = App::new(None);
    // Picking a file opens the wizard instead of importing straight away.
    let _ = app.update(Message::ImportPicked(Some(path.clone())));
    let Some(Dialog::Import(s)) = &app.dialog else { panic!("the wizard should be open") };
    assert_eq!(s.preview.names[0], "Plate 3 - run of 12 May", "as it stands, the title line is the header");

    let _ = app.update(Message::Dialog(DialogMsg::ImportRole(0, RowRole::Skip)));
    let _ = app.update(Message::Dialog(DialogMsg::ImportRole(1, RowRole::Name)));
    let _ = app.update(Message::Dialog(DialogMsg::ImportRole(2, RowRole::Name)));
    let Some(Dialog::Import(s)) = &app.dialog else { panic!("the wizard should still be open") };
    assert_eq!(s.preview.names, vec!["Well", "Treatment | Low", "Treatment | High"]);
    assert_eq!(s.preview.data_rows, 3);
    snapshot(&app, "import-wizard");

    let _ = app.update(Message::Dialog(DialogMsg::ImportOk));
    assert!(app.dialog.is_none(), "OK closes the wizard");
    let t = &app.engine.project.tables[0];
    let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["Well", "Treatment | Low", "Treatment | High"]);
    assert_eq!(t.nrows, 3);
    let _ = std::fs::remove_file(&path);
}

/// Selecting whole rows: a click picks one, a drag over the row numbers extends to a
/// block, Ctrl+A takes the lot, and the panel offers the one thing to do with them.
#[test]
fn rows_select_by_click_and_drag() {
    use iced::keyboard::{self, key::Named, Key, Modifiers};

    let (mut app, _) = App::new(Some(sample()));
    let rows_before = app.engine.project.tables[0].nrows;

    let _ = app.update(Message::SelectRow(2));
    assert_eq!(app.grid.rows, vec![2], "a click picks the row it is on");
    assert!(app.grid.sel.is_none() && app.grid.highlight.is_empty(), "a row is not a cell or a column");

    // Dragging down the row numbers extends the block; the button coming up ends it.
    let _ = app.update(Message::DragOverRow(5));
    assert_eq!(app.grid.rows, vec![2, 3, 4, 5]);
    let _ = app.update(Message::DragEnd);
    let _ = app.update(Message::DragOverRow(9));
    assert_eq!(app.grid.rows, vec![2, 3, 4, 5], "with the button up, moving over a row does nothing");

    snapshot(&app, "table-rows-selected");

    // The panel's delete button is the only thing offered for them.
    let mut sim = Simulator::with_size(iced::Settings::default(), (1440.0, 900.0), view::view(&app));
    sim.click("Delete 4 rows").expect("the delete button for the selected rows");
    for m in sim.into_messages().collect::<Vec<Message>>() {
        let _ = app.update(m);
    }
    assert_eq!(app.engine.project.tables[0].nrows, rows_before - 4);
    assert!(app.grid.rows.is_empty(), "the rows are gone, so nothing is selected any more");

    // Ctrl+A takes every row and every column.
    let key = |k: Key, m: Modifiers| Message::Key(keyboard::Event::KeyPressed {
        key: k.clone(),
        modified_key: k.clone(),
        physical_key: keyboard::key::Physical::Unidentified(keyboard::key::NativeCode::Unidentified),
        location: keyboard::Location::Standard,
        modifiers: m,
        text: None,
        repeat: false,
    });
    let _ = app.update(Message::Key(keyboard::Event::ModifiersChanged(Modifiers::CTRL)));
    let _ = app.update(key(Key::Character("a".into()), Modifiers::CTRL));
    let t = &app.engine.project.tables[0];
    assert_eq!(app.grid.rows.len(), t.nrows);
    assert_eq!(app.grid.highlight.len(), t.ncols());
    let _ = Named::Escape;
}

/// Percentages, dates and times are offered as column types and change only how the
/// cells read: the numbers behind them, and every graph on them, are untouched.
#[test]
fn date_and_percentage_column_types() {
    use otto_core::{ColumnType, NumberFormat};
    use otto_engine::SetColumnTypeArgs;

    let (mut app, _) = App::new(Some(sample()));
    let name = app.engine.project.tables[0].columns.iter().find(|c| c.kind() == otto_core::ColumnKind::Numeric).unwrap().name.clone();
    let before: Vec<Option<f64>> = {
        let t = &app.engine.project.tables[0];
        let c = t.columns.iter().find(|c| c.name == name).unwrap();
        (0..t.nrows).map(|r| c.number(r)).collect()
    };

    let _ = app.update(Message::Cmd(Command::TableSetColumnType(SetColumnTypeArgs {
        table: None,
        column: name.clone(),
        kind: otto_core::ColumnKind::Numeric,
        format: NumberFormat::Date,
    })));
    let t = &app.engine.project.tables[0];
    let c = t.columns.iter().find(|c| c.name == name).unwrap();
    assert_eq!(c.column_type(), ColumnType::Date);
    assert_eq!((0..t.nrows).map(|r| c.number(r)).collect::<Vec<_>>(), before, "the numbers are untouched");
    assert!(c.display(0).starts_with("19") || c.display(0).starts_with("20"), "cells read as dates: {}", c.display(0));

    // A graph over the column still draws.
    let _ = app.update(Message::NewGraph(GraphKind::XyPoints));
    assert!(app.preview.as_ref().is_some_and(|p| p.result.is_ok()), "a date column still graphs");
}

fn is_snapshot_of(file: &str, name: &str) -> bool {
    file.strip_prefix(&format!("gui-{name}-")).is_some_and(|rest| matches!(rest, "wgpu.png" | "tiny-skia.png"))
}

/// Fit curve from a quick XY graph: the Prism-like model popup, constraints, then the final
/// graph with the fitted curves and the results computed by R.
#[test]
fn curve_fit_popup() {
    use otto_app::dialogs::{Change, Dialog, DialogMsg, Tab};
    use otto_core::analysis::{FitFamily, FitModel};
    use otto_engine::{GraphCreateArgs, ImportTextArgs};
    if !otto_r::available() {
        eprintln!("skipped: the statistics engine (R with Otto's packages) is not set up");
        return;
    }
    let mut text = String::from("LogDose,Drug,Response\n");
    let a = [11.1, 12.5, 11.3, 7.3, 17.5, 27.4, 48.4, 75.7, 86.3, 89.6, 89.8, 91.6, 87.8];
    let b = [9.2, 10.1, 12.0, 11.4, 10.9, 14.8, 22.5, 41.2, 60.3, 77.9, 84.1, 88.5, 90.2];
    for i in 0..13 {
        let x = -9.0 + 0.5 * i as f64;
        text.push_str(&format!("{x},A,{}\n{x},B,{}\n", a[i], b[i]));
    }
    let (mut app, _) = App::new(None);
    otto_r::set_background(false);
    app.settings_file = None;
    let _ = app.update(Message::Cmd(Command::TableImportText(ImportTextArgs { text, name: Some("Dose response".into()), has_header: true, options: None })));
    let _ = app.update(Message::Cmd(Command::GraphCreate(GraphCreateArgs {
        table: None,
        kind: GraphKind::XyPoints,
        x: Some(otto_core::OneOrMany::One("LogDose".into())),
        y: vec!["Response".into()],
        group: Some(otto_core::OneOrMany::One("Drug".into())),
        filters: Vec::new(),
        format: None,
        name: None,
    })));
    snapshot(&app, "fit-quick-graph");
    // "Make final graph" at the top of the quick XY graph opens Analyze Data with the
    // curve fit chosen; OK opens the model popup.
    let mut sim = Simulator::with_size(iced::Settings::default(), (1440.0, 900.0), view::view(&app));
    sim.click("Make final graph").expect("the button at the top");
    for m in sim.into_messages().collect::<Vec<_>>() {
        let _ = app.update(m);
    }
    assert!(matches!(&app.dialog, Some(Dialog::Analyze(a)) if a.method == otto_app::dialogs::Method::CurveFit));
    let _ = app.update(Message::Dialog(DialogMsg::AnalyzeOk));
    assert!(matches!(&app.dialog, Some(Dialog::Params(p)) if p.tab == Tab::Model));
    snapshot(&app, "dialog-fit-model");
    let _ = app.update(Message::Dialog(DialogMsg::Change(Change::Family(FitFamily::Inhibition))));
    snapshot(&app, "dialog-fit-inhibition");
    let _ = app.update(Message::Dialog(DialogMsg::Change(Change::Family(FitFamily::Stimulation))));
    let _ = app.update(Message::Dialog(DialogMsg::Change(Change::Model(FitModel::Agonist4pl))));
    let _ = app.update(Message::Dialog(DialogMsg::Tab(Tab::Constraints)));
    let _ = app.update(Message::Dialog(DialogMsg::Change(Change::Fix("Bottom".into(), true))));
    snapshot(&app, "dialog-fit-constraints");
    let _ = app.update(Message::Dialog(DialogMsg::ParamsOk));
    assert!(app.dialog.is_none(), "{:?}", app.status);
    // A final graph with both curves.
    let fin = app.engine.project.graphs.iter().find(|g| g.is_final()).unwrap().id.clone();
    assert_eq!(app.engine.view, View::Graph(fin.clone()));
    assert_eq!(app.engine.render_graph(&fin).unwrap().curves.len(), 2);
    snapshot(&app, "fit-final-graph");
    let id = app.engine.project.analyses[0].id.clone();
    let _ = app.update(Message::Cmd(Command::UiNavigate(otto_engine::NavigateArgs { section: otto_engine::Section::Results, item: Some(id) })));
    snapshot(&app, "fit-results");
    // The statistics engine dialog.
    let _ = app.update(Message::Dialog(DialogMsg::OpenEngine));
    snapshot(&app, "dialog-engine");
}

/// Survival curves from a multivariable table, the Kaplan-Meier dialog and results; two-way
/// ANOVA and AUC dialogs; and the "calculating in R" state while R works in the background.
#[test]
fn survival_two_way_and_background() {
    use otto_app::dialogs::{Dialog, DialogMsg, Method, Tab};
    use otto_engine::{GraphCreateArgs, ImportTextArgs};
    if !otto_r::available() {
        eprintln!("skipped: the statistics engine (R with Otto's packages) is not set up");
        return;
    }
    let times = [9, 13, 13, 18, 23, 28, 31, 34, 45, 48, 161, 5, 5, 8, 8, 12, 16, 23, 27, 30, 33, 43, 45];
    let status = [1, 1, 0, 1, 1, 0, 1, 1, 0, 1, 0, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1];
    let mut text = String::from("Patient,Weeks,Relapse,Treatment\n");
    for i in 0..times.len() {
        text.push_str(&format!("P{},{},{},{}\n", i + 1, times[i], status[i], if i < 11 { "Maintained" } else { "Nonmaintained" }));
    }
    let (mut app, _) = App::new(None);
    app.settings_file = None;
    let _ = app.update(Message::Cmd(Command::TableImportText(ImportTextArgs { text, name: Some("AML".into()), has_header: true, options: None })));
    let _ = app.update(Message::NewGraph(GraphKind::Survival));
    assert!(app.preview.as_ref().is_some_and(|p| p.result.is_ok()), "{:?}", app.status);
    snapshot(&app, "survival-quick-graph");
    // A long name is cut off; the buttons stay where they are.
    let gid = app.engine.project.graphs[0].id.clone();
    let _ = app.update(Message::Cmd(Command::GraphRename(otto_engine::RenameGraphArgs {
        graph: Some(gid.clone()),
        name: "Kaplan-Meier of relapse-free survival, AE culls censored, maintenance vs. no maintenance".into(),
    })));
    snapshot(&app, "header-long-name");
    // A long table name must not wrap the "Table" pick list of the panel onto a second
    // line and push every field under it down.
    let _ = app.update(Message::Cmd(Command::TableRename(otto_engine::RenameTableArgs {
        table: Some("T1".into()),
        name: "EMA027_study data \u{2013} weights (T1) of every animal".into(),
    })));
    panel_snapshot_of(&app, &gid, "panel-long-table-name");
    let _ = app.update(Message::Cmd(Command::GraphRename(otto_engine::RenameGraphArgs { graph: Some(gid), name: "Graph 1".into() })));
    // Make final graph → Analyze Data, with the survival analysis chosen → its parameters.
    let _ = app.update(Message::Dialog(DialogMsg::OpenAnalyze));
    assert!(matches!(&app.dialog, Some(Dialog::Analyze(a)) if a.method == Method::Survival));
    let _ = app.update(Message::Dialog(DialogMsg::AnalyzeOk));
    snapshot(&app, "dialog-survival");
    // OK while R works in the background: the final graph appears at once, the results follow.
    let _ = app.update(Message::Dialog(DialogMsg::ParamsOk));
    assert!(app.dialog.is_none(), "{:?}", app.status);
    let id = app.engine.project.analyses[0].id.clone();
    let _ = app.update(Message::Cmd(Command::UiNavigate(otto_engine::NavigateArgs { section: otto_engine::Section::Results, item: Some(id) })));
    if otto_r::pending() > 0 {
        snapshot(&app, "results-calculating");
    }
    let start = std::time::Instant::now();
    while otto_r::pending() > 0 && start.elapsed().as_secs() < 60 {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let _ = app.update(Message::StatsTick);
    snapshot(&app, "results-survival");

    // Two-way ANOVA on a grouped graph.
    let mut text = String::from("Dose,Strain,Value\n");
    let y = [10.2, 11.1, 9.8, 10.6, 13.0, 12.4, 13.9, 12.8, 12.1, 11.7, 12.9, 12.4, 16.3, 15.2, 16.8, 15.9, 14.2, 13.8, 14.9, 14.1, 19.8, 20.4, 18.9, 19.6];
    for (i, v) in y.iter().enumerate() {
        text.push_str(&format!("x{},{},{v}\n", i / 8 + 1, if i % 8 < 4 { "WT" } else { "KO" }));
    }
    let _ = app.update(Message::Cmd(Command::TableImportText(ImportTextArgs { text, name: Some("Grouped".into()), has_header: true, options: None })));
    let _ = app.update(Message::Cmd(Command::GraphCreate(GraphCreateArgs {
        table: None,
        kind: GraphKind::GroupedInterleaved,
        x: Some(otto_core::OneOrMany::One("Dose".into())),
        y: vec!["Value".into()],
        group: Some(otto_core::OneOrMany::One("Strain".into())),
        filters: Vec::new(),
        format: None,
        name: None,
    })));
    let _ = app.update(Message::Dialog(DialogMsg::OpenAnalyze));
    let _ = app.update(Message::Dialog(DialogMsg::PickMethod(Method::TwoWayAnova)));
    snapshot(&app, "dialog-analyze-two-way");
    let _ = app.update(Message::Dialog(DialogMsg::AnalyzeOk));
    assert!(matches!(app.dialog, Some(Dialog::Params(_))));
    let _ = app.update(Message::Dialog(DialogMsg::Tab(Tab::Comparisons)));
    snapshot(&app, "dialog-two-way-comparisons");
    otto_r::set_background(false);
    let _ = app.update(Message::Dialog(DialogMsg::ParamsOk));
    assert!(app.dialog.is_none(), "{:?}", app.status);
    snapshot(&app, "graph-two-way");
}

/// Selecting a navigator entry outlines what belongs with it in grey: a quick graph's data
/// table and final graphs, a table's graphs, a final graph's quick graph and table.
#[test]
fn navigator_outlines_related_entries() {
    use otto_engine::{FinalizeArgs, ImportTextArgs, NavigateArgs, Section};
    let (mut app, _) = App::new(None);
    app.settings_file = None;
    for (name, text) in [("Assay A", "Group,Value\nA,1\nA,2\nB,3\nB,4\n"), ("Assay B", "Group,Value\nC,5\nC,6\nD,7\nD,8\n")] {
        let _ = app.update(Message::Cmd(Command::TableImportText(ImportTextArgs { text: text.into(), name: Some(name.into()), has_header: true, options: None })));
        let _ = app.update(Message::NewGraph(GraphKind::ColumnBar));
        let _ = app.update(Message::Cmd(Command::GraphFinalize(FinalizeArgs { graph: None, name: None })));
    }
    let _ = app.update(Message::Cmd(Command::GraphDuplicate(otto_engine::DuplicateGraphArgs { graph: Some("G1".into()), name: None })));
    // The first quick graph: its table (Assay A) and its final graph are outlined.
    let _ = app.update(Message::Cmd(Command::UiNavigate(NavigateArgs { section: Section::Graphs, item: Some("G1".into()) })));
    snapshot(&app, "navigator-related-quick-graph");
    let _ = app.update(Message::Cmd(Command::UiNavigate(NavigateArgs { section: Section::Tables, item: Some("T3".into()) })));
    snapshot(&app, "navigator-related-table");
}

/// A heatmap of cytokines (rows, from Group by) × conditions (columns), with its panel.
#[test]
fn heatmap_graph() {
    use otto_engine::{GraphCreateArgs, ImportTextArgs};
    let mut text = String::from("Condition,Cytokine,Donor,Level\n");
    let base = [("IL-2", 40.0), ("IL-6", 900.0), ("IL-10", 120.0), ("TNF", 300.0), ("IFN-g", 60.0)];
    for (ci, cond) in ["Vehicle", "LPS", "LPS + Drug A", "LPS + Drug B"].iter().enumerate() {
        for (k, (cyt, b)) in base.iter().enumerate() {
            for d in 0..3 {
                let effect = [1.0, 6.0, 2.5, 4.0][ci] * (1.0 + 0.3 * ((k + ci) % 3) as f64);
                text.push_str(&format!("{cond},{cyt},D{},{:.1}\n", d + 1, b * effect * (1.0 + 0.05 * d as f64)));
            }
        }
    }
    let (mut app, _) = App::new(None);
    app.settings_file = None;
    let _ = app.update(Message::Cmd(Command::TableImportText(ImportTextArgs { text, name: Some("Cytokines".into()), has_header: true, options: None })));
    let _ = app.update(Message::Cmd(Command::GraphCreate(GraphCreateArgs {
        table: None,
        kind: GraphKind::Heatmap,
        x: Some(otto_core::OneOrMany::One("Condition".into())),
        y: vec!["Level".into()],
        group: Some(otto_core::OneOrMany::One("Cytokine".into())),
        filters: Vec::new(),
        format: None,
        name: None,
    })));
    let _ = app.update(Message::Cmd(Command::GraphSetHeatmap(otto_engine::GraphHeatmapArgs {
        graph: None,
        scale: None,
        normalize: Some(otto_core::HeatNormalize::Rows),
        show_values: Some(true),
        decimals: None,
    })));
    assert!(app.preview.as_ref().is_some_and(|p| p.result.is_ok()), "{:?}", app.status);
    snapshot(&app, "heatmap");
}

/// A narrow window folds the edge panels away and the toolbar toggles bring them back;
/// widening it hands the decision back to the width.
#[test]
fn narrow_window_folds_the_edge_panels() {
    use otto_app::app::Side;

    let (mut app, _) = App::new(Some(sample()));
    assert_eq!(app.panels(), (true, true), "a wide window shows both");

    let _ = app.update(Message::Resized(1100.0));
    assert_eq!(app.panels(), (false, true), "the navigator goes first");
    let _ = app.update(Message::Resized(880.0));
    assert_eq!(app.panels(), (false, false), "then the settings panel");
    snapshot_at(&app, "narrow", (880.0, 700.0));

    // Either one can be called back while the window stays narrow.
    let _ = app.update(Message::TogglePanel(Side::Right));
    assert_eq!(app.panels(), (false, true));
    snapshot_at(&app, "narrow-panel", (880.0, 700.0));
    let _ = app.update(Message::TogglePanel(Side::Left));
    assert_eq!(app.panels(), (true, true));

    // Widening past a threshold forgets what was toggled while it was narrow.
    let _ = app.update(Message::Resized(1400.0));
    assert_eq!(app.panels(), (true, true));
    let _ = app.update(Message::TogglePanel(Side::Left));
    assert_eq!(app.panels(), (false, true), "a wide window can still be tidied");
}

/// A snapshot of the whole window at a given size.
fn snapshot_at(app: &App, name: &str, size: (f32, f32)) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/otto-gallery");
    std::fs::create_dir_all(&dir).unwrap();
    for entry in std::fs::read_dir(&dir).unwrap().flatten() {
        if is_snapshot_of(&entry.file_name().to_string_lossy(), name) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    let mut sim = Simulator::with_size(iced::Settings::default(), size, view::view(app));
    let shot = sim.snapshot(&style::theme()).expect("snapshot");
    assert!(shot.matches_image(dir.join(format!("gui-{name}"))).unwrap());
}

/// New, Open and closing the window must not throw unsaved work away without asking.
#[test]
fn new_open_and_close_ask_before_discarding() {
    let (mut app, _) = App::new(None);
    let _ = app.update(Message::Cmd(Command::TableCreate(Default::default())));
    assert!(app.engine.dirty, "a new table makes the project dirty");
    let before = app.engine.project.tables.len();

    // Clicking New in the toolbar sends Message::New ...
    let mut sim = Simulator::with_size(iced::Settings::default(), (1440.0, 900.0), view::view(&app));
    sim.click("New").expect("the New button");
    let msgs: Vec<Message> = sim.into_messages().collect();
    assert!(msgs.iter().any(|m| matches!(m, Message::New)), "got {msgs:?}");

    // ... which leaves the project alone and puts the question on screen instead.
    for m in msgs {
        let _ = app.update(m);
    }
    assert_eq!(app.engine.project.tables.len(), before, "New must not wipe the project on its own");
    assert!(app.engine.dirty);
    assert!(matches!(app.dialog, Some(Dialog::ConfirmDiscard(Pending::New))));
    snapshot(&app, "unsaved-changes");

    // Cancel puts everything back as it was.
    let _ = app.update(Message::Dialog(DialogMsg::Cancel));
    assert!(app.dialog.is_none());
    assert_eq!(app.engine.project.tables.len(), before);

    // Open asks the same question; "Don't save" goes ahead with it.
    let _ = app.update(Message::Open);
    assert!(matches!(app.dialog, Some(Dialog::ConfirmDiscard(Pending::Open))));
    let mut sim = Simulator::with_size(iced::Settings::default(), (1440.0, 900.0), view::view(&app));
    sim.click("Don't save").expect("the Don't save button");
    let msgs: Vec<Message> = sim.into_messages().collect();
    assert!(msgs.iter().any(|m| matches!(m, Message::Dialog(DialogMsg::Discard(Pending::Open)))), "got {msgs:?}");

    // And New with nothing unsaved goes straight through.
    let _ = app.update(Message::Dialog(DialogMsg::Cancel));
    let _ = app.update(Message::Dialog(DialogMsg::Discard(Pending::New)));
    assert!(app.dialog.is_none());
    assert!(app.engine.project.tables.is_empty(), "the new project is empty");
    assert!(!app.engine.dirty);
    let _ = app.update(Message::New);
    assert!(app.dialog.is_none(), "a clean project is not asked about");

    // Closing the window asks the same question, and does not close while it is up.
    let id = iced::window::Id::unique();
    let _ = app.update(Message::Cmd(Command::TableCreate(Default::default())));
    let _ = app.update(Message::CloseRequested(id));
    assert!(matches!(app.dialog, Some(Dialog::ConfirmDiscard(Pending::Close(_)))));
    let _ = app.update(Message::Dialog(DialogMsg::Cancel));
    assert!(app.dialog.is_none(), "Cancel keeps the window open");
    assert!(app.engine.dirty);
}
