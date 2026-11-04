//! Right sidebar for tables (column inspector) and results.

use iced::widget::{button, column, container, pick_list, row, scrollable, space, text, text_input, Column};
use iced::{Alignment, Fill, Length};

use otto_core::{AnalysisKind, ColumnKind, ColumnType, DataTable, GraphKind};
use otto_engine::*;

use crate::app::{App, Draft, Message};
use crate::icons::{icon, Icon};
use crate::style;
use crate::view::{caption, heading, tip, El};

fn draft_input<'a>(app: &'a App, key: Draft, current: String, placeholder: &'a str, size: f32) -> El<'a> {
    let value = app.drafts.get(&key).cloned().unwrap_or(current);
    let k2 = key.clone();
    text_input(placeholder, &value)
        .on_input(move |s| Message::Draft(key.clone(), s))
        .on_submit(Message::Commit(k2))
        .size(size)
        .padding([5, 7])
        .width(Fill)
        .into()
}

fn small(label: &'static str, msg: Message) -> El<'static> {
    button(text(label).size(12)).padding([5, 10]).style(style::secondary).on_press(msg).into()
}

pub fn table_panel<'a>(app: &'a App, t: &'a DataTable) -> El<'a> {
    let tid = t.id.clone();
    let mut content = Column::new().spacing(18).padding(14);
    content = content.push(draft_input(app, Draft::TableName(tid.clone()), t.name.clone(), "Table name", 15.0));

    // Whole rows selected: the one thing to do with them is delete them.
    if !app.grid.rows.is_empty() {
        let rows = app.grid.rows.clone();
        let n = rows.len();
        content = content.push(
            column![
                heading("Rows"),
                caption(format!("{n} {} selected", if n == 1 { "row" } else { "rows" })),
                button(text(format!("Delete {n} {}", if n == 1 { "row" } else { "rows" })).size(12))
                    .padding([5, 10])
                    .style(style::danger)
                    .on_press(Message::Cmd(Command::TableDeleteRows(DeleteRowsArgs { table: Some(tid.clone()), rows }))),
            ]
            .spacing(8),
        );
        return finish_table_panel(app, t, content);
    }

    // Several columns highlighted: nothing names one column any more, so the panel says
    // so and leaves "Graph this table" (which plots exactly those columns) as the way on.
    if app.grid.highlight.len() > 1 {
        let names: Vec<String> = app.grid.highlight.iter().filter_map(|&c| t.columns.get(c)).map(|c| c.name.clone()).collect();
        let n = names.len();
        content = content.push(
            column![
                heading("Columns"),
                text(format!("{n} columns selected")).size(13),
                container(caption(names.join(", ")).size(12)).clip(true).width(Fill),
                button(text(format!("Delete {n} columns")).size(12)).padding([5, 10]).style(style::danger).on_press(Message::Cmd(
                    Command::TableDeleteColumns(DeleteColumnsArgs { table: Some(tid.clone()), columns: names })
                )),
            ]
            .spacing(8),
        );
        return finish_table_panel(app, t, content);
    }

    // Column inspector.
    let inspector: El = match app.grid.column.and_then(|c| t.columns.get(c).map(|col| (c, col))) {
        None => caption("Click a column header or cell to edit the column.").into(),
        Some((ci, col)) => {
            let t1 = tid.clone();
            let name = col.name.clone();
            let kind = pick_list(ColumnType::ALL, Some(col.column_type()), move |ty: ColumnType| {
                Message::Cmd(Command::TableSetColumnType(SetColumnTypeArgs {
                    table: Some(t1.clone()),
                    column: name.clone(),
                    kind: ty.kind(),
                    format: ty.format(),
                }))
            })
            .text_size(13)
            .width(Fill);
            let mut col_box = column![
                draft_input(app, Draft::ColumnName(tid.clone(), ci), col.name.clone(), "Column name", 13.0),
                row![caption("Type").width(Length::Fixed(50.0)), kind].spacing(6).align_y(Alignment::Center),
            ]
            .spacing(8);
            match col.column_type() {
                ColumnType::Percentage => col_box = col_box.push(caption("Written with a %; the numbers themselves are unchanged.")),
                ColumnType::Date => col_box = col_box.push(caption("Whole days since 1970-01-01, written YYYY-MM-DD.")),
                ColumnType::Time => col_box = col_box.push(caption("Seconds since midnight, written HH:MM:SS.")),
                _ => {}
            }
            if col.kind() == ColumnKind::Categorical {
                let levels = col.levels();
                let mut list = Column::new().spacing(4);
                for (li, level) in levels.iter().enumerate() {
                    let count = (0..t.nrows).filter(|&r| col.label(r) == Some(level.as_str())).count();
                    let reorder = |delta: isize| -> Option<Message> {
                        let j = li as isize + delta;
                        if j < 0 || j as usize >= levels.len() {
                            return None;
                        }
                        let mut order = levels.clone();
                        order.swap(li, j as usize);
                        Some(Message::Cmd(Command::TableSetCategoryOrder(CategoryOrderArgs {
                            table: Some(tid.clone()),
                            column: col.name.clone(),
                            levels: order,
                        })))
                    };
                    list = list.push(
                        row![
                            draft_input(app, Draft::LevelName(tid.clone(), ci, li), level.clone(), "level", 12.5),
                            caption(format!("n={count}")).width(Length::Fixed(38.0)),
                            button(icon(Icon::Up, 13.0, style::TEXT)).padding(4).style(style::tool).on_press_maybe(reorder(-1)),
                            button(icon(Icon::Down, 13.0, style::TEXT)).padding(4).style(style::tool).on_press_maybe(reorder(1)),
                        ]
                        .spacing(4)
                        .align_y(Alignment::Center),
                    );
                }
                col_box = col_box.push(caption("Levels — this order is the order of bars on graphs"));
                col_box = col_box.push(list);
            }
            col_box = col_box.push(
                button(text("Delete column").size(12)).padding([5, 10]).style(style::danger).on_press(Message::Cmd(Command::TableDeleteColumns(
                    DeleteColumnsArgs { table: Some(tid.clone()), columns: vec![col.name.clone()] },
                ))),
            );
            col_box.into()
        }
    };
    content = content.push(column![heading("Column"), inspector].spacing(8));

    // Columns and rows are added with the + buttons in the table itself.

    finish_table_panel(app, t, content)
}

/// Everything under the selection-dependent top of the table panel: how to graph the
/// table, how to describe it, and what to do with the table as a whole.
fn finish_table_panel<'a>(app: &'a App, t: &'a DataTable, mut content: Column<'a, Message>) -> El<'a> {
    let tid = t.id.clone();
    // Graph this table.
    let tiles: Vec<El> = GraphKind::ALL
        .iter()
        .map(|&k| {
            tip(
                button(icon(Icon::Kind(k), 26.0, style::TEXT)).width(Fill).padding(5).style(style::tile(false)).on_press(Message::NewGraph(k)),
                k.label(),
            )
        })
        .collect();
    // Five per row, the last row padded so the tiles keep their width.
    const PER_ROW: usize = 5;
    let mut tile_grid = Column::new().spacing(5);
    let mut tiles = tiles.into_iter().peekable();
    while tiles.peek().is_some() {
        let mut r: Vec<El> = Vec::new();
        for _ in 0..PER_ROW {
            r.push(tiles.next().unwrap_or_else(|| space::horizontal().into()));
        }
        tile_grid = tile_grid.push(row(r).spacing(5));
    }
    // Highlighted numeric columns become the Y values of the new graph.
    let _picked: Vec<String> = app
        .grid
        .highlight
        .iter()
        .filter_map(|&c| t.columns.get(c))
        .filter(|c| c.kind() == ColumnKind::Numeric)
        .map(|c| c.name.clone())
        .collect();
    content = content.push(column![heading("Graph this table"), tile_grid].spacing(8));
    content = content.push(
        column![
            heading("Analyze"),
            button(row![icon(Icon::Stats, 14.0, style::TEXT), text("Descriptive statistics").size(12)].spacing(6).align_y(Alignment::Center))
                .padding([6, 10])
                .style(style::secondary)
                .on_press(Message::NewDescriptive)
        ]
        .spacing(8),
    );
    content = content.push(
        row![
            small("Duplicate table", Message::Cmd(Command::TableDuplicate(DuplicateTableArgs { table: Some(tid.clone()), name: None }))),
            button(text("Delete table…").size(12))
                .padding([5, 10])
                .style(style::danger)
                .on_press(Message::AskDeleteTable(tid.clone())),
        ]
        .spacing(8),
    );
    scrollable(content).height(Fill).into()
}

pub fn analysis_panel<'a>(app: &'a App, id: &str) -> El<'a> {
    let Some(a) = app.engine.project.analyses.iter().find(|a| a.id == id) else { return space().into() };
    let table = app.engine.project.table_by_id(&a.table_id).map(|t| t.name.clone()).unwrap_or_else(|| "?".into());
    let mut content = Column::new().spacing(16).padding(14);
    content = content.push(draft_input(app, Draft::AnalysisName(a.id.clone()), a.name.clone(), "Name", 15.0));
    let graph_link = |graph_id: &str| -> El<'a> {
        let name = app.engine.project.graph(graph_id).map(|g| g.name.clone()).unwrap_or_else(|| "?".into());
        button(row![icon(Icon::Graph, 14.0, style::TEXT), text(format!("Show graph \"{name}\"")).size(12)].spacing(6).align_y(Alignment::Center))
            .padding([5, 10])
            .style(style::secondary)
            .on_press(Message::Cmd(Command::UiNavigate(NavigateArgs { section: Section::Graphs, item: Some(graph_id.to_string()) })))
            .into()
    };
    match &a.kind {
        AnalysisKind::Descriptive { y, group } => {
            content = content.push(
                column![
                    heading("Descriptive statistics"),
                    caption(format!("Table: {table}")),
                    caption(format!("Columns: {}", y.join(", "))),
                    caption(format!("Grouped by: {}", group.as_deref().unwrap_or("—")))
                ]
                .spacing(6),
            );
        }
        AnalysisKind::TTest(t) => {
            content = content.push(column![heading("t test"), caption(t.test_name()), caption(format!("Data sets: {}", t.data_sets.join("  vs.  ")))].spacing(6));
            content = content.push(setup_buttons(app, a));
            content = content.push(graph_link(&t.graph_id));
        }
        AnalysisKind::OneWayAnova(x) => {
            let mut info = column![heading("One-way ANOVA"), caption(x.test_name())].spacing(6);
            if x.comparisons != otto_core::analysis::Comparisons::None {
                info = info.push(caption(otto_engine::column_tests::followup_name(x)));
            }
            info = info.push(caption(format!("Data sets: {}", x.data_sets.join(", "))));
            content = content.push(info);
            content = content.push(setup_buttons(app, a));
            content = content.push(graph_link(&x.graph_id));
        }
        AnalysisKind::CurveFit(f) => {
            let mut info = column![heading("Nonlinear regression (curve fit)"), caption(f.model.label()), caption(f.model.equation())].spacing(6);
            if !f.fixed.is_empty() {
                let c: Vec<String> = f.fixed.iter().map(|p| format!("{} = {}", p.name, otto_core::table::format_number(p.value))).collect();
                info = info.push(caption(format!("Constraints: {}", c.join(", "))));
            }
            info = info.push(caption(format!("Data sets: {}", if f.data_sets.is_empty() { "all".to_string() } else { f.data_sets.join(", ") })));
            content = content.push(info);
            content = content.push(setup_buttons(app, a));
            content = content.push(graph_link(&f.graph_id));
        }
        AnalysisKind::LinearRegression(l) => {
            let sets = if l.data_sets.is_empty() { "all".to_string() } else { l.data_sets.join(", ") };
            content = content.push(column![heading("Simple linear regression"), caption(format!("Data sets: {sets}"))].spacing(6));
            content = content.push(setup_buttons(app, a));
            content = content.push(graph_link(&l.graph_id));
        }
        AnalysisKind::TwoWayAnova(t) => {
            let sets = if t.data_sets.is_empty() { "all bars".to_string() } else { t.data_sets.join(", ") };
            content = content.push(column![heading("Two-way ANOVA"), caption(crate::dialogs::analysis_summary(a)), caption(format!("Data: {sets}"))].spacing(6));
            content = content.push(setup_buttons(app, a));
            content = content.push(graph_link(&t.graph_id));
        }
        AnalysisKind::Auc(x) => {
            let sets = if x.data_sets.is_empty() { "all".to_string() } else { x.data_sets.join(", ") };
            content = content.push(
                column![heading("Area under curve"), caption(format!("Baseline Y = {}", otto_core::table::format_number(x.baseline))), caption(format!("Data sets: {sets}"))].spacing(6),
            );
            content = content.push(setup_buttons(app, a));
            content = content.push(graph_link(&x.graph_id));
        }
        AnalysisKind::Survival(x) => {
            let sets = if x.data_sets.is_empty() { "all".to_string() } else { x.data_sets.join(", ") };
            content = content.push(column![heading("Survival analysis (Kaplan-Meier)"), caption(format!("Curves: {sets}"))].spacing(6));
            content = content.push(setup_buttons(app, a));
            content = content.push(graph_link(&x.graph_id));
        }
        AnalysisKind::Contingency(x) => {
            let cols = if x.data_sets.is_empty() { "all".to_string() } else { x.data_sets.join(", ") };
            content = content.push(
                column![heading("Contingency table"), caption(crate::dialogs::analysis_summary(a)), caption(format!("Columns: {cols}"))].spacing(6),
            );
            content = content.push(setup_buttons(app, a));
            content = content.push(graph_link(&x.graph_id));
        }
        AnalysisKind::Normality(n) => {
            let sets = if n.data_sets.is_empty() { "all".to_string() } else { n.data_sets.join(", ") };
            content = content.push(
                column![heading("Normality tests"), caption(crate::dialogs::analysis_summary(a)), caption(format!("Data sets: {sets}"))].spacing(6),
            );
            content = content.push(setup_buttons(app, a));
            content = content.push(graph_link(&n.graph_id));
        }
        AnalysisKind::Outliers(o) => {
            let sets = if o.data_sets.is_empty() { "all".to_string() } else { o.data_sets.join(", ") };
            content = content.push(
                column![heading("Outlier identification"), caption(o.method.label()), caption(format!("Data sets: {sets}"))].spacing(6),
            );
            content = content.push(setup_buttons(app, a));
            content = content.push(graph_link(&o.graph_id));
        }
        AnalysisKind::Correlation(c) => {
            let sets = if c.data_sets.is_empty() { "all".to_string() } else { c.data_sets.join(", ") };
            content = content.push(column![heading("Correlation"), caption(crate::dialogs::analysis_summary(a)), caption(format!("Data sets: {sets}"))].spacing(6));
            content = content.push(setup_buttons(app, a));
            content = content.push(graph_link(&c.graph_id));
        }
    }
    if a.kind.graph_id().is_some() {
        content = content.push(
            button(row![icon(Icon::Code, 14.0, style::TEXT), text("Export R script…").size(12)].spacing(6).align_y(Alignment::Center))
                .padding([5, 10])
                .style(style::secondary)
                .on_press(Message::ExportRScript(a.id.clone())),
        );
    }
    let note = match &a.kind {
        AnalysisKind::Descriptive { .. } => "Results update automatically when the data change.",
        AnalysisKind::CurveFit(_) | AnalysisKind::LinearRegression(_) => {
            "Computed by R; results update automatically when the data change. Whether the curve is drawn is set in the graph's panel (Analyze). Export R script reruns the analysis in R."
        }
        AnalysisKind::Correlation(_) | AnalysisKind::Auc(_) | AnalysisKind::Survival(_) | AnalysisKind::Contingency(_) => {
            "Computed by R; results update automatically when the data change. Export R script reruns the analysis in R."
        }
        _ => "Computed by R; results update automatically when the data change. Whether the brackets are drawn is set in the graph's panel (Analyze). Export R script reruns the analysis in R.",
    };
    content = content.push(caption(note));
    content = content.push(
        button(text("Delete result").size(12))
            .padding([5, 10])
            .style(style::danger)
            .on_press(Message::Cmd(Command::AnalysisDelete(AnalysisRef { analysis: Some(a.id.clone()) }))),
    );
    scrollable(content).height(Fill).into()
}

/// "Change parameters…", or for an out-of-date analysis: recalculate / set up again.
fn setup_buttons<'a>(app: &'a App, a: &'a otto_core::AnalysisSpec) -> El<'a> {
    use crate::dialogs::DialogMsg;
    match app.stale.get(&a.id) {
        Some(reasons) => {
            let recalc = crate::dialogs::can_recalculate(app, a);
            let mut c = column![
                row![icon(Icon::Warning, 15.0, style::WARNING), text("Out of date").size(13).color(style::WARNING)].spacing(6).align_y(Alignment::Center),
                caption(format!("The graph changed: {}.", reasons.join("; "))),
            ]
            .spacing(6);
            if let Err(e) = &recalc {
                c = c.push(caption(e.clone()));
            }
            let buttons = row![
                button(text("Recalculate with the data shown").size(12))
                    .padding([5, 10])
                    .style(style::primary)
                    .on_press_maybe(recalc.is_ok().then(|| Message::Dialog(DialogMsg::Recalculate(Some(a.id.clone()))))),
                button(text("Set up again…").size(12)).padding([5, 10]).style(style::secondary).on_press(Message::Dialog(DialogMsg::SetUpAgain(a.id.clone()))),
            ]
            .spacing(6);
            container(c.push(buttons)).padding(10).width(Fill).style(style::warning_box).into()
        }
        None => button(row![icon(Icon::Stats, 14.0, iced::Color::WHITE), text("Change parameters…").size(12.5)].spacing(6).align_y(Alignment::Center))
            .padding([6, 12])
            .style(style::primary)
            .on_press(Message::Dialog(DialogMsg::EditAnalysis(a.id.clone())))
            .into(),
    }
}

pub fn help_panel(_app: &App) -> El<'_> {
    let step = |n: &'static str, title: &'static str, body: &'static str| -> El<'static> {
        row![
            container(text(n).size(12)).padding([2, 7]).style(style::pill(true)),
            column![text(title).size(13), caption(body)].spacing(2)
        ]
        .spacing(10)
        .into()
    };
    column![
        heading("How Otto works"),
        step("1", "Data table", "Import or paste a multivariable table: one row per sample, columns for conditions and measurements."),
        step("2", "Quick graphs", "Highlight columns and pick a graph type to look through the data. Each graph has a plotted table (Prism columns)."),
        step("3", "Statistics", "When you know what to test, run t tests or ANOVA: this makes a final graph on a locked copy of the data."),
        step("4", "Final graphs", "Choose which statistics are drawn, refine for publication and export as vectors, data or R code."),
    ]
    .spacing(14)
    .padding(14)
    .into()
}
