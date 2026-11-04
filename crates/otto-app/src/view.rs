//! Window layout: ribbon toolbar, navigator (Data Tables / Results / Graphs),
//! centre area, context sidebar and status bar.

use iced::widget::{button, column, container, hover, row, rule, scrollable, space, text, toggler, tooltip, Column};
use iced::{Alignment, Color, Element, Fill, Length};

use otto_core::{GraphKind, Palette};
use otto_engine::{Command, GraphPaletteArgs, NavigateArgs, NoArgs, Section, TableCreateArgs, View};

use crate::app::{App, Message};
use crate::dropdown::drop_down;
use crate::icons::{icon, Icon};
use crate::style;

pub type El<'a> = Element<'a, Message>;

pub fn view(app: &App) -> El<'_> {
    let center: El = match &app.engine.view {
        View::Welcome => welcome(app),
        View::Table(id) => match app.engine.project.table_by_id(id) {
            Some(t) => crate::grid::view(app, t),
            None => welcome(app),
        },
        View::Graph(id) => crate::graph::view(app, id),
        View::Plotted(id) => crate::plotted::view(app, id),
        View::Result(id) => results(app, id),
    };
    let sidebar: El = match &app.engine.view {
        View::Table(id) => match app.engine.project.table_by_id(id) {
            Some(t) => crate::sidebar::table_panel(app, t),
            None => crate::sidebar::help_panel(app),
        },
        View::Graph(id) => crate::graph::panel(app, id),
        View::Plotted(id) => crate::plotted::panel(app, id),
        View::Result(id) => crate::sidebar::analysis_panel(app, id),
        View::Welcome => crate::sidebar::help_panel(app),
    };
    // A narrow window keeps the table or graph and folds the edge panels away; the toolbar
    // has a toggle for each, so nothing becomes unreachable.
    let (show_nav, show_panel) = app.panels();
    let mut body = row![].height(Fill);
    if show_nav {
        body = body.push(container(navigator(app)).width(232).height(Fill).style(style::panel)).push(vline());
    }
    body = body.push(container(center).width(Fill).height(Fill).style(style::canvas));
    if show_panel {
        body = body.push(vline()).push(container(sidebar).width(310).height(Fill).style(style::panel));
    }
    let base: El = column![
        toolbar(app),
        rule::horizontal(1).style(|_| rule::Style { color: style::BORDER, radius: 0.0.into(), fill_mode: rule::FillMode::Full, snap: true }),
        body,
        statusbar(app)
    ]
    .into();
    crate::dialogs::overlay(app, base)
}

// -- small helpers -------------------------------------------------------------

pub fn caption<'a>(s: impl text::IntoFragment<'a>) -> text::Text<'a> {
    text(s).size(11).color(style::MUTED)
}

/// Cuts a label to `max` characters with an ellipsis. Pick lists and other widgets
/// that wrap their text would otherwise grow a panel row to two or three lines.
pub fn elide(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let kept: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{}\u{2026}", kept.trim_end())
}

pub fn heading<'a>(s: &str) -> El<'a> {
    text(s.to_uppercase()).size(10.5).color(style::MUTED).into()
}

pub fn tip<'a>(content: impl Into<El<'a>>, hint: impl text::IntoFragment<'a>) -> El<'a> {
    tooltip(
        content,
        container(text(hint).size(12)).padding([4, 8]).style(style::card),
        tooltip::Position::Bottom,
    )
    .gap(4)
    .into()
}

/// A round "×" (`size` across): just the ×, with a grey circle behind it while pointed at.
pub fn round_x<'a>(size: f32, msg: Message) -> El<'a> {
    button(container(icon(Icon::Close, size * 0.5, style::TEXT)).center(Length::Fill))
        .width(size)
        .height(size)
        .padding(0)
        .style(style::delete_x)
        .on_press(msg)
        .into()
}

/// The close "×" of dialogs and search fields.
pub fn close_x<'a>(msg: Message) -> El<'a> {
    round_x(26.0, msg)
}

fn tool_button<'a>(i: Icon, label: &'a str, hint: &'a str, msg: Option<Message>) -> El<'a> {
    let enabled = msg.is_some();
    let color = if enabled { style::TEXT } else { Color::from_rgb8(0x5A, 0x5D, 0x63) };
    let b = button(
        column![icon(i, 20.0, color), text(label).size(10).color(color)]
            .spacing(2)
            .align_x(Alignment::Center)
            .width(Length::Fixed(50.0)),
    )
    .padding([4, 2])
    .style(style::tool)
    .on_press_maybe(msg);
    tip(b, hint)
}

fn group<'a>(title: &'a str, buttons: Vec<El<'a>>) -> El<'a> {
    column![row(buttons).spacing(1), caption(title).size(9.5)].spacing(1).align_x(Alignment::Center).into()
}

/// The hairline between the centre area and an edge panel.
fn vline<'a>() -> El<'a> {
    rule::vertical(1).style(|_| rule::Style { color: style::BORDER, radius: 0.0.into(), fill_mode: rule::FillMode::Full, snap: true }).into()
}

fn vsep<'a>() -> El<'a> {
    container(space().width(1).height(44)).style(style::divider).into()
}

// -- toolbar ---------------------------------------------------------------------

fn toolbar(app: &App) -> El<'_> {
    let e = &app.engine;
    let (show_nav, show_panel) = app.panels();
    let has_tables = !e.project.tables.is_empty();
    let in_graph = matches!(e.view, View::Graph(_));
    let graph_ref = || otto_engine::GraphRef { graph: None };
    let bar = row![
        group("View", vec![
            tool_button(
                Icon::PanelLeft,
                "Navigator",
                if show_nav { "Hide the navigator" } else { "Show the navigator" },
                Some(Message::TogglePanel(crate::app::Side::Left))
            ),
            tool_button(
                Icon::PanelRight,
                "Panel",
                if show_panel { "Hide the settings panel" } else { "Show the settings panel" },
                Some(Message::TogglePanel(crate::app::Side::Right))
            ),
        ]),
        vsep(),
        group(
            "File",
            vec![
                tool_button(Icon::New, "New", "New project (Ctrl+N)", Some(Message::New)),
                tool_button(Icon::Open, "Open", "Open project (Ctrl+O)", Some(Message::Open)),
                tool_button(Icon::Save, "Save", "Save project (Ctrl+S); asks where the first time", Some(Message::Save)),
                tool_button(Icon::SaveAs, "Save as", "Save the project to a new file (Ctrl+Shift+S)", Some(Message::SaveAs)),
            ]
        ),
        vsep(),
        group(
            "Edit",
            vec![
                tool_button(Icon::Undo, "Undo", "Undo (Ctrl+Z)", e.can_undo().then_some(Message::Cmd(Command::Undo(NoArgs {})))),
                tool_button(Icon::Redo, "Redo", "Redo (Ctrl+Y)", e.can_redo().then_some(Message::Cmd(Command::Redo(NoArgs {})))),
            ]
        ),
        vsep(),
        group(
            "Data",
            vec![
                tool_button(Icon::Import, "Import", "Import CSV / Excel as a data table (Ctrl+I)", Some(Message::Import)),
                tool_button(Icon::Paste, "Paste", "New data table from the clipboard (copied from Excel)", Some(Message::PasteNewTable)),
                tool_button(
                    Icon::Table,
                    "Table",
                    "New empty data table",
                    Some(Message::Cmd(Command::TableCreate(TableCreateArgs::default())))
                ),
            ]
        ),
        vsep(),
        group(
            "Analyze",
            vec![
                tool_button(
                    Icon::Stats,
                    "Analyze",
                    "Analyze data: t tests, one-way ANOVA, curve fits, regression, correlation, descriptive statistics",
                    has_tables.then_some(Message::Dialog(crate::dialogs::DialogMsg::OpenAnalyze))
                ),
                tool_button(
                    Icon::Engine,
                    "Engine",
                    "Statistics engine: R and Otto's R packages (check, set up)",
                    Some(Message::Dialog(crate::dialogs::DialogMsg::OpenEngine))
                ),
            ]
        ),
        vsep(),
        group("Colors", vec![palette_menu(app)]),
        vsep(),
        group(
            "Export",
            vec![
                tool_button(
                    Icon::Copy,
                    "Copy",
                    "Copy graph as vector (EMF + SVG) — Ctrl+C",
                    in_graph.then(|| Message::Cmd(Command::GraphCopyVector(graph_ref())))
                ),
                tool_button(Icon::Export, "Export", "Export graph as SVG / PDF / EMF / PNG", in_graph.then_some(Message::ExportGraph)),
                tool_button(Icon::Data, "Data", "Export the plotted data as CSV", in_graph.then_some(Message::ExportData)),
                tool_button(Icon::Code, "R code", "Export R / ggplot2 code for the graph", in_graph.then_some(Message::ExportCode)),
            ]
        ),
        space::horizontal(),
        mcp_switch(app),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    container(bar).padding([6, 10]).width(Fill).style(style::toolbar).into()
}

/// The tick on the chosen menu entry; the others keep its space, so nothing shifts.
fn tick<'a>(picked: bool) -> El<'a> {
    if picked {
        icon(Icon::Check, 12.0, style::ACCENT).into()
    } else {
        space().width(12).into()
    }
}

/// A strip of a palette's first colors, as in a journal's figure.
pub fn swatch<'a>(palette: Palette, cell: f32, height: f32) -> El<'a> {
    let cells = otto_plot::theme::palette_colors(palette).iter().take(5).map(|&hex| {
        let c = Color::from_rgb8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8);
        container(space().width(cell).height(height))
            .style(move |_: &iced::Theme| container::Style { background: Some(c.into()), ..container::Style::default() })
            .into()
    });
    container(row(cells))
        .padding(1)
        .style(|_: &iced::Theme| container::Style {
            background: Some(style::BORDER.into()),
            border: iced::border::rounded(3),
            ..container::Style::default()
        })
        .clip(true)
        .into()
}

/// The toolbar's color-theme menu: one journal palette per row, plus highlight mode.
/// It sets the theme of the graph on screen (every graph keeps its own).
fn palette_menu(app: &App) -> El<'_> {
    let current = match &app.engine.view {
        View::Graph(id) => app.engine.project.graph(id),
        _ => None,
    };
    let open = app.open_dropdown.as_deref() == Some(PALETTE_MENU);
    let palette = current.map(|g| g.palette).unwrap_or_default();
    let enabled = current.is_some();
    let label = if enabled { palette.label() } else { "Theme" };
    let anchor = button(
        column![
            swatch(palette, 9.0, 11.0),
            row![
                text(label).size(10).color(if enabled { style::TEXT } else { Color::from_rgb8(0x5A, 0x5D, 0x63) }).wrapping(text::Wrapping::None),
                icon(if open { Icon::Up } else { Icon::ChevronDown }, 9.0, style::MUTED),
            ]
            .spacing(3)
            .align_y(Alignment::Center),
        ]
        .spacing(3)
        .align_x(Alignment::Center),
    )
    .padding([4, 6])
    .style(style::tool)
    .on_press_maybe(enabled.then(|| Message::ToggleDropdown(PALETTE_MENU.to_string())));
    if !enabled {
        return tip(anchor, "Open a graph to choose its color theme");
    }
    let anchor = tip(anchor, "Color theme of this graph");
    let mut list = Column::new().spacing(2);
    for &p in Palette::ALL {
        let picked = p == palette;
        list = list.push(
            button(
                row![
                    swatch(p, 11.0, 13.0),
                    text(p.label()).size(12.5),
                    space::horizontal(),
                    tick(picked),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .width(Fill)
            .padding([4, 6])
            .style(style::tile(picked))
            .on_press(Message::Cmd(Command::GraphSetPalette(GraphPaletteArgs { graph: None, palette: p, highlight: None }))),
        );
    }
    let menu = container(
        column![
            caption("Journal-inspired. Red and blue go to the data sets in focus (picked in the graph panel)."),
            scrollable(list).height(Length::Shrink),
        ]
        .spacing(6),
    )
    .padding(8)
    .width(300)
    .max_height(420.0)
    .style(style::menu);
    drop_down(anchor, menu, open, Message::CloseDropdown).into()
}

/// Dropdown id of the toolbar's color-theme menu.
pub const PALETTE_MENU: &str = "palette";

/// On/off switch for Claude's MCP access, with its live state.
fn mcp_switch(app: &App) -> El<'_> {
    let on = app.settings.mcp_enabled;
    let (label, live) = match (on, &app.mcp.url, &app.mcp.error) {
        (false, _, _) => ("Claude MCP off".to_string(), false),
        (true, _, Some(_)) => ("Claude MCP failed".to_string(), false),
        (true, Some(_), _) if app.mcp.actions > 0 => (format!("Claude MCP · {} actions", app.mcp.actions), true),
        (true, Some(_), _) => ("Claude MCP on".to_string(), true),
        (true, None, None) => ("Claude MCP starting…".to_string(), false),
    };
    // The status bar is one clipped line, so the reason a server did not start - most
    // often another Otto window already serving Claude - is spelled out here.
    let hint: String = match (on, &app.mcp.error) {
        (false, _) => "Off: Claude cannot see or change this window. Switch on to let Claude drive Otto through MCP.".to_string(),
        (true, Some(e)) => format!("The MCP server could not start: {e}"),
        (true, None) => {
            "On: Claude can see and change this window through MCP (local only, token protected). Switch off to cut Claude's access.".to_string()
        }
    };
    // The whole pill is the switch: a click anywhere on it flips Claude's access.
    tip(
        button(
            row![
                icon(Icon::Spark, 14.0, if live { Color::from_rgb8(0x7E, 0xE7, 0x87) } else { style::MUTED }),
                text(label).size(12),
                toggler(on).size(16.0).style(style::switch),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding(iced::Padding { top: 3.0, bottom: 3.0, left: 10.0, right: 6.0 })
        .style(style::pill_button(live))
        .on_press(Message::SetMcp(!on)),
        hint,
    )
}

// -- navigator -------------------------------------------------------------------

/// How a navigator entry stands to the one on screen.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mark {
    None,
    Selected,
    /// Belongs with the selected entry (its data table, quick graph or final graphs).
    Related,
}

fn mark(selected: bool, related: bool) -> Mark {
    if selected {
        Mark::Selected
    } else if related {
        Mark::Related
    } else {
        Mark::None
    }
}

fn nav_entry<'a>(i: Icon, label: String, m: Mark, msg: Message) -> El<'a> {
    let selected = m == Mark::Selected;
    // One line per entry: a long name is cut off at the edge (the full name shows on hover).
    let long = otto_plot::font::text_width(&label, 13.0) > 165.0;
    let name = container(text(label.clone()).size(13).wrapping(iced::widget::text::Wrapping::None)).width(Fill).clip(true);
    let b = button(row![icon(i, 15.0, if selected { Color::WHITE } else { style::MUTED }), name].spacing(8).align_y(Alignment::Center))
        .width(Fill)
        .padding([5, 8])
        .on_press(msg);
    let b: El<'a> = match m {
        Mark::Related => b.style(style::nav_related),
        _ => b.style(style::nav_item(selected)),
    }
    .into();
    if long {
        tooltip(b, container(text(label).size(12)).padding([4, 8]).style(style::card), tooltip::Position::Right).gap(6).into()
    } else {
        b
    }
}

/// Data tables and graphs that belong with what is on screen: a table's quick graphs and
/// the final graphs made from it; a graph's table and the graphs made from or for it; a
/// result's graph (and so on). Ids of tables and graphs.
fn related_ids(app: &App) -> std::collections::HashSet<String> {
    use otto_core::GraphSpec;
    let p = &app.engine.project;
    let mut out = std::collections::HashSet::new();
    // The data table a graph shows (a final graph: the table its locked copy came from).
    let source_table = |g: &GraphSpec| g.locked.as_ref().map(|l| l.table_id.clone()).unwrap_or_else(|| g.table_id.clone());
    let add_graph_family = |g: &GraphSpec, out: &mut std::collections::HashSet<String>| {
        out.insert(source_table(g));
        match &g.locked {
            Some(lock) => {
                out.insert(lock.from_graph.clone());
            }
            None => {
                for f in p.graphs.iter().filter(|f| f.locked.as_ref().is_some_and(|l| l.from_graph == g.id)) {
                    out.insert(f.id.clone());
                }
            }
        }
    };
    match &app.engine.view {
        View::Table(t) => {
            for g in p.graphs.iter().filter(|g| source_table(g) == *t) {
                out.insert(g.id.clone());
            }
        }
        View::Graph(id) | View::Plotted(id) => {
            if let Some(g) = p.graph(id) {
                add_graph_family(g, &mut out);
            }
        }
        View::Result(id) => {
            if let Some(a) = p.analyses.iter().find(|a| a.id == *id) {
                match app.engine.analysis_graph(&a.id).and_then(|g| p.graph(&g)) {
                    Some(g) => {
                        out.insert(g.id.clone());
                        add_graph_family(g, &mut out);
                    }
                    None => {
                        out.insert(a.table_id.clone());
                    }
                }
            }
        }
        View::Welcome => {}
    }
    out
}

fn nav_section<'a>(app: &'a App, index: usize, title: &'a str, count: usize, items: Vec<El<'a>>, add: Option<(&'a str, Message)>) -> El<'a> {
    let collapsed = app.collapsed[index];
    let header = button(
        row![
            icon(if collapsed { Icon::ChevronRight } else { Icon::ChevronDown }, 12.0, style::MUTED),
            text(title).size(12.5).color(style::TEXT),
            space::horizontal(),
            caption(count.to_string()),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .width(Fill)
    .padding([6, 6])
    .style(style::tool)
    .on_press(Message::ToggleSection(index));
    let mut col = Column::new().push(header).spacing(1);
    if !collapsed {
        for item in items {
            col = col.push(container(item).padding(iced::Padding { left: 10.0, ..Default::default() }));
        }
        if let Some((label, msg)) = add {
            let add_btn = button(row![icon(Icon::Plus, 13.0, style::MUTED), caption(label).size(12)].spacing(8).align_y(Alignment::Center))
                .width(Fill)
                .padding([4, 8])
                .style(style::tool)
                .on_press(msg);
            col = col.push(container(add_btn).padding(iced::Padding { left: 10.0, ..Default::default() }));
        }
    }
    col.into()
}

fn navigator(app: &App) -> El<'_> {
    let p = &app.engine.project;
    let v = &app.engine.view;
    let nav = |section: Section, id: &str| Message::Cmd(Command::UiNavigate(NavigateArgs { section, item: Some(id.to_string()) }));
    let related = related_ids(app);
    // A result under its data table or final graph; its × deletes it.
    let result_entry = |a: &otto_core::AnalysisSpec| {
        let i = if app.stale.contains_key(&a.id) { Icon::Warning } else { Icon::Result };
        let entry = nav_child(i, a.name.clone(), mark(*v == View::Result(a.id.clone()), false), nav(Section::Results, &a.id));
        with_delete(entry, Message::Cmd(Command::AnalysisDelete(otto_engine::AnalysisRef { analysis: Some(a.id.clone()) })))
    };
    // A graph; its × deletes it (a final graph takes its final table and statistics along).
    let graph_entry = |g: &otto_core::GraphSpec, section: Section| {
        let entry = nav_entry(Icon::Kind(g.kind), g.name.clone(), mark(*v == View::Graph(g.id.clone()), related.contains(&g.id)), nav(section, &g.id));
        with_delete(entry, Message::Cmd(Command::GraphDelete(otto_engine::GraphRef { graph: Some(g.id.clone()) })))
    };
    let graph_of = |a: &otto_core::AnalysisSpec| app.engine.analysis_graph(&a.id);

    // Data tables only (a final graph's locked copy lives with that graph), each with its
    // descriptive statistics. Deleting a data table asks first.
    let data_tables: Vec<&otto_core::DataTable> = p.tables.iter().filter(|t| t.locked_by.is_none()).collect();
    let mut tables: Vec<El> = Vec::new();
    for t in &data_tables {
        let entry = nav_entry(Icon::Table, t.name.clone(), mark(*v == View::Table(t.id.clone()), related.contains(&t.id)), nav(Section::Tables, &t.id));
        tables.push(with_delete(entry, Message::AskDeleteTable(t.id.clone())));
        for a in p.analyses.iter().filter(|a| a.table_id == t.id && graph_of(a).is_none()) {
            tables.push(result_entry(a));
        }
    }
    let quick: Vec<&otto_core::GraphSpec> = p.graphs.iter().filter(|g| !g.is_final()).collect();
    let finals: Vec<&otto_core::GraphSpec> = p.graphs.iter().filter(|g| g.is_final()).collect();
    // Each quick graph with its plotted table underneath (built only when opened).
    let mut graphs: Vec<El> = Vec::new();
    // The plotted table sits behind its graph: it appears once that graph is the one shown,
    // so a long list of graphs stays readable.
    let showing = |id: &str| matches!(v, View::Graph(x) | View::Plotted(x) if x == id);
    for g in &quick {
        graphs.push(graph_entry(g, Section::Graphs));
        if showing(&g.id) {
            graphs.push(nav_child(Icon::Data, "Plotted table".into(), mark(*v == View::Plotted(g.id.clone()), false), nav(Section::PlottedTables, &g.id)));
        }
    }
    // Each final graph with its locked table and its statistics underneath.
    let mut final_graphs: Vec<El> = Vec::new();
    for g in &finals {
        final_graphs.push(graph_entry(g, Section::FinalGraphs));
        // The locked table and the statistics sit behind their graph, appearing once it is
        // the one shown (or holds the result being read).
        if showing(&g.id) || related.contains(&g.id) {
            final_graphs.push(nav_child(Icon::Lock, "Final table".into(), mark(*v == View::Plotted(g.id.clone()), false), nav(Section::PlottedTables, &g.id)));
            for a in p.analyses.iter().filter(|a| graph_of(a).as_deref() == Some(g.id.as_str()) || (graph_of(a).is_none() && a.table_id == g.table_id)) {
                final_graphs.push(result_entry(a));
            }
        }
    }
    let analyze = Message::Dialog(crate::dialogs::DialogMsg::OpenAnalyze);
    // The workflow top to bottom: data → quick graphs → final graphs with statistics.
    let content = column![
        container(text(app.engine.project.name.clone()).size(14)).padding([4, 6]),
        nav_section(
            app,
            0,
            "Data Tables",
            data_tables.len(),
            tables,
            Some(("New data table…", Message::Cmd(Command::TableCreate(TableCreateArgs::default()))))
        ),
        nav_section(app, 1, "Quick Graphs", quick.len(), graphs, Some(("New quick graph…", Message::NewGraph(GraphKind::ColumnBar)))),
        nav_section(app, 2, "Final Graphs", finals.len(), final_graphs, Some(("Statistics → final graph…", analyze))),
    ]
    .spacing(10)
    .padding(8);
    scrollable(content).height(Fill).into()
}

/// A navigator entry with a × at its right end that shows only while the mouse is over it.
fn with_delete<'a>(entry: El<'a>, on_delete: Message) -> El<'a> {
    let x = round_x(22.0, on_delete);
    hover(entry, container(x).align_right(Fill).center_y(Fill).padding(iced::Padding { right: 4.0, ..Default::default() }))
}

/// An entry nested under the one above it (indented).
fn nav_child<'a>(i: Icon, label: String, m: Mark, msg: Message) -> El<'a> {
    container(nav_entry(i, label, m, msg)).padding(iced::Padding { left: 22.0, ..Default::default() }).into()
}

// -- status bar ------------------------------------------------------------------

/// The height of the status bar's single line of text. Importing a whole table can
/// report a great deal; it is cut to one line so the bar never eats the window.
const STATUS_H: f32 = 17.0;

fn statusbar(app: &App) -> El<'_> {
    let (msg, err) = &app.status;
    let left = text(status_line(msg)).size(12).wrapping(text::Wrapping::None).color(if *err { style::DANGER } else { style::MUTED });
    let left: El = container(left).height(STATUS_H).clip(true).into();
    let dims = app.active_table().map(|t| format!("{}: {} rows × {} columns", t.name, t.nrows, t.ncols())).unwrap_or_default();
    let mcp = match (app.settings.mcp_enabled, &app.mcp.url, &app.mcp.last) {
        (false, _, _) => "Claude MCP off".to_string(),
        (true, Some(url), Some(last)) => format!("MCP {url} · last: {last}"),
        (true, Some(url), None) => format!("MCP {url}"),
        _ => String::new(),
    };
    let right = |s: String| -> El<'static> { container(caption(s).wrapping(text::Wrapping::None)).height(STATUS_H).clip(true).into() };
    container(row![left, space::horizontal(), right(dims), right(mcp)].spacing(18).align_y(Alignment::Center))
        .padding([4, 10])
        .height(STATUS_H + 8.0)
        .width(Fill)
        .style(style::statusbar)
        .into()
}

/// One line for the status bar. A report of several things (an import names every column
/// it read) is boiled down rather than left to wrap into a wall of text.
fn status_line(msg: &str) -> String {
    let mut lines = msg.lines().filter(|l| !l.trim().is_empty());
    let first = lines.next().unwrap_or("").trim();
    if lines.next().is_some() {
        let head = first.split(['.', ';']).next().unwrap_or(first).trim();
        return if head.is_empty() { "Multiple actions".to_string() } else { elide(&format!("{head} \u{2026}"), 150) };
    }
    elide(first, 150)
}

// -- welcome ---------------------------------------------------------------------

fn welcome(app: &App) -> El<'_> {
    let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "otto.exe".into());
    let big = |i: Icon, label: &'static str, hint: &'static str, msg: Message| -> El<'static> {
        button(
            row![icon(i, 26.0, style::ACCENT), column![text(label).size(15), caption(hint)].spacing(2)]
                .spacing(14)
                .align_y(Alignment::Center),
        )
        .width(330)
        .padding([12, 16])
        .style(style::secondary)
        .on_press(msg)
        .into()
    };
    let recent = if app.engine.project.tables.is_empty() {
        column![]
    } else {
        column![caption("This project already has data — pick a table or graph on the left.")]
    };
    let content = column![
        text("Otto").size(44),
        caption("Prism-style scientific graphs from multivariable tables").size(14),
        space().height(18),
        big(Icon::Import, "Import data…", "CSV, TSV or Excel — one row per sample", Message::Import),
        big(Icon::Paste, "Paste table", "From Excel or Prism (first row = column names)", Message::PasteNewTable),
        big(Icon::Table, "New data table", "Type values directly", Message::Cmd(Command::TableCreate(TableCreateArgs::default()))),
        big(Icon::Open, "Open project…", "An existing .otto file", Message::Open),
        space().height(14),
        recent,
        container(
            column![
                row![icon(Icon::Spark, 16.0, style::ACCENT), text("Plot with Claude").size(14)].spacing(8).align_y(Alignment::Center),
                caption("Every button is also an MCP tool. Add Otto to Claude Code with:"),
                container(text(format!("claude mcp add otto -- \"{exe}\" mcp")).size(12).font(iced::Font::MONOSPACE))
                    .padding(8)
                    .style(style::canvas),
                caption(if app.settings.mcp_enabled {
                    "Claude then sees and changes this window live (tables, graphs, exports)."
                } else {
                    "Claude MCP is switched off: turn it on at the top right to let Claude see and change this window."
                })
            ]
            .spacing(8)
        )
        .padding(14)
        .width(520)
        .style(style::card),
    ]
    .spacing(10)
    .align_x(Alignment::Center);
    container(scrollable(content)).center(Fill).padding(30).into()
}

// -- results ---------------------------------------------------------------------

fn results<'a>(app: &'a App, id: &str) -> El<'a> {
    let Some(spec) = app.engine.project.analyses.iter().find(|a| a.id == id) else { return welcome(app) };
    if let Some(reasons) = app.stale.get(id) {
        // Numbers computed on a graph that changed would be misleading: show none.
        let recalc = crate::dialogs::can_recalculate(app, spec);
        let mut notice = column![
            row![icon(Icon::Warning, 18.0, style::WARNING), text("These results are out of date").size(15)].spacing(8).align_y(Alignment::Center),
            text(format!(
                "The graph they were calculated from changed ({}). The results and the significance brackets are hidden until you recalculate with what the graph shows now, or set the analysis up again.",
                reasons.join("; ")
            ))
            .size(13),
        ]
        .spacing(8);
        if let Err(e) = &recalc {
            notice = notice.push(text(e.clone()).size(12.5));
        }
        notice = notice.push(
            row![
                button(text("Recalculate with the data shown").size(13))
                    .padding([6, 14])
                    .style(style::primary)
                    .on_press_maybe(recalc.is_ok().then(|| Message::Dialog(crate::dialogs::DialogMsg::Recalculate(Some(spec.id.clone()))))),
                button(text("Set up again…").size(13))
                    .padding([6, 14])
                    .style(style::secondary)
                    .on_press(Message::Dialog(crate::dialogs::DialogMsg::SetUpAgain(spec.id.clone()))),
                button(text("Undo").size(13)).padding([6, 14]).style(style::secondary).on_press(Message::Cmd(Command::Undo(NoArgs {}))),
            ]
            .spacing(8),
        );
        return column![
            container(column![text(spec.name.clone()).size(18), caption(crate::dialogs::analysis_summary(spec))].spacing(2)).padding([14, 18]),
            container(container(notice).padding(14).max_width(720).style(style::warning_box)).padding([0, 18]),
        ]
        .into();
    }
    // Computed in `after_change`, never here: `results::compute` distills the whole table.
    // `None` would mean this result was not the one the view asked for, which cannot
    // normally happen; show it as not ready rather than as an error.
    let report = match app.report(&spec.id) {
        Some(Ok(r)) => r,
        Some(Err(otto_engine::EngineError::Calculating)) | None => {
            return column![
                container(column![text(spec.name.clone()).size(18), caption(crate::dialogs::analysis_summary(spec))].spacing(2)).padding([14, 18]),
                container(
                    container(
                        column![
                            row![icon(Icon::Engine, 16.0, style::ACCENT), text("Calculating in R…").size(15)].spacing(8).align_y(Alignment::Center),
                            caption("You can keep working — look at other graphs and tables meanwhile. The results appear here as soon as R is done.").size(12.5),
                        ]
                        .spacing(8)
                    )
                    .padding(14)
                    .max_width(720)
                    .style(style::card)
                )
                .padding([0, 18]),
            ]
            .into();
        }
        Some(Err(e)) => {
            // Without the statistics engine, say how to set it up instead of just failing.
            let engine = matches!(e, otto_engine::EngineError::Engine(_));
            let mut notice = column![text(e.to_string()).size(13)].spacing(10);
            if engine {
                notice = notice.push(
                    button(row![icon(Icon::Engine, 14.0, iced::Color::WHITE), text("Set up the statistics engine…").size(13)].spacing(6).align_y(Alignment::Center))
                        .padding([6, 14])
                        .style(style::primary)
                        .on_press(Message::Dialog(crate::dialogs::DialogMsg::OpenEngine)),
                );
            }
            return column![
                container(text(spec.name.clone()).size(18)).padding([14, 18]),
                container(container(notice).padding(12).max_width(720).style(if engine { style::warning_box } else { style::error_box })).padding([0, 18]),
            ]
            .into();
        }
    };
    let mut body = Column::new().spacing(18);
    let report_title = report.title.clone();
    let engine = report.engine.clone();
    for section in report.sections.iter().cloned() {
        body = body.push(section_view(section));
    }
    let table_name = app.active_table().map(|t| t.name.as_str()).unwrap_or("?");
    let mut title = column![
        text(spec.name.clone()).size(18),
        caption(format!("{} · {} · table \"{table_name}\"", spec.id, report_title))
    ]
    .spacing(2);
    if !engine.is_empty() {
        title = title.push(
            row![
                caption(format!("Computed with {engine}")).size(11.5),
                button(text("Export R script…").size(11.5)).padding([1, 6]).style(style::link).on_press(Message::ExportRScript(spec.id.clone())),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );
    }
    column![
        container(title).padding([14, 18]),
        scrollable(container(body).padding(iced::Padding { left: 18.0, right: 18.0, top: 0.0, bottom: 24.0 }))
            .direction(scrollable::Direction::Both {
                vertical: scrollable::Scrollbar::default(),
                horizontal: scrollable::Scrollbar::default()
            })
            .width(Fill)
            .height(Fill)
    ]
    .into()
}

/// One results table: key/value sheets (Prism's "Table of results") or a grid.
fn section_view<'a>(section: otto_engine::results::Section) -> El<'a> {
    let t = &section.table;
    let title = text(section.title).size(14).color(style::ACCENT);
    if t.headers.iter().all(|h| h.is_empty()) {
        let key_w = t.rows.iter().map(|r| r.first().map_or(0, |s| s.chars().count())).max().unwrap_or(10) as f32 * 7.0 + 24.0;
        let mut list = Column::new();
        for (i, r) in t.rows.iter().enumerate() {
            let key = r.first().cloned().unwrap_or_default();
            let value = r.get(1).cloned().unwrap_or_default();
            let line: El = if key.is_empty() && value.is_empty() {
                space().height(8).into()
            } else if value.is_empty() {
                // Sub-heading such as the test name or "How big is the difference?".
                container(text(key).size(13).font(iced::Font { weight: iced::font::Weight::Bold, ..Default::default() }))
                    .padding([5, 8])
                    .into()
            } else {
                row![
                    container(text(key).size(12.5)).width(key_w.clamp(160.0, 460.0)).padding([3, 8]),
                    container(text(value).size(12.5)).padding([3, 8]),
                ]
                .into()
            };
            list = list.push(container(line).width(Fill).style(style::cell(false, i % 2 == 1)));
        }
        return column![title, container(list).max_width(900.0)].spacing(6).into();
    }
    let widths: Vec<f32> = t
        .headers
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let longest = t.rows.iter().map(|r| r.get(i).map_or(0, |s| s.chars().count())).max().unwrap_or(0);
            (h.chars().count().max(longest) as f32 * 7.2 + 22.0).clamp(64.0, 340.0)
        })
        .collect();
    let header = row(t.headers.iter().zip(&widths).enumerate().map(|(ci, (h, w))| {
        let c = container(text(h.clone()).size(12));
        let c = if ci > 0 { c.align_x(Alignment::End) } else { c };
        c.width(*w).padding([5, 8]).style(style::header_cell(false)).into()
    }));
    let mut grid = Column::new().push(header);
    for (ri, r) in t.rows.iter().enumerate() {
        grid = grid.push(row(r.iter().zip(&widths).enumerate().map(|(ci, (v, w))| {
            let c = container(text(v.clone()).size(12.5));
            let c = if ci > 0 { c.align_x(Alignment::End) } else { c };
            c.width(*w).padding([4, 8]).style(style::cell(false, ri % 2 == 1)).into()
        })));
    }
    column![title, grid].spacing(6).into()
}
