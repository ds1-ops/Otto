//! Plotted tables: the data a graph plots, laid out like a Prism data table (one column
//! per bar / data set). Read-only; built only while on screen. A final graph's plotted
//! table shows its locked data copy.

use iced::widget::{button, column, container, row, scrollable, space, text, Column, Row};
use iced::{Alignment, Fill, Length};

use otto_core::{DataLayout, GraphSpec};
use otto_engine::{Command, CopyDataArgs, NavigateArgs, Section};

use crate::app::{App, Message};
use crate::icons::{icon, Icon};
use crate::style;
use crate::view::{caption, heading, El};

/// Rows drawn on screen; copy or export for the rest.
const MAX_ROWS: usize = 2000;


pub fn view<'a>(app: &'a App, graph_id: &str) -> El<'a> {
    let Some(g) = app.engine.project.graph(graph_id) else { return space().into() };
    let table = app.plotted.as_ref().filter(|p| p.graph_id == g.id).map(|p| &p.result);
    let size = match table {
        Some(Ok(t)) => {
            let n = t.rows.len().saturating_sub(1);
            format!("{} columns × {n} row{} of values", t.headers.len(), if n == 1 { "" } else { "s" })
        }
        _ => String::new(),
    };
    let mut title = row![
        icon(if g.is_final() { Icon::Lock } else { Icon::Data }, 18.0, style::ACCENT),
        text(g.name.clone()).size(17),
        caption(format!("plotted table · Prism layout, replicates side by side · {size}")),
        space::horizontal(),
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    title = title.push(
        button(row![icon(Icon::Kind(g.kind), 14.0, style::TEXT), text("Show graph").size(12)].spacing(6).align_y(Alignment::Center))
            .padding([4, 10])
            .style(style::secondary)
            .on_press(show_graph(g)),
    );
    let page = Column::new().push(container(title).padding([10, 14]));
    let body: El = match table {
        Some(Ok(t)) => grid(t),
        Some(Err(e)) => container(column![text("Nothing plotted yet").size(15), text(e.clone()).size(13)].spacing(6))
            .padding(16)
            .style(style::error_box)
            .into(),
        None => caption("Preparing the table…").into(),
    };
    page.push(container(body).padding(iced::Padding { left: 14.0, right: 4.0, top: 8.0, bottom: 4.0 }).height(Fill)).into()
}

fn show_graph(g: &GraphSpec) -> Message {
    let section = if g.is_final() { Section::FinalGraphs } else { Section::Graphs };
    Message::Cmd(Command::UiNavigate(NavigateArgs { section, item: Some(g.id.clone()) }))
}

/// Columns covered by each header cell: a title spans the blank cells after it (its
/// replicate subcolumns); a blank cell before any title stands alone.
fn header_spans(headers: &[String]) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut i = 0;
    while i < headers.len() {
        let mut j = i + 1;
        if !headers[i].is_empty() {
            while j < headers.len() && headers[j].is_empty() {
                j += 1;
            }
        }
        spans.push((i, j));
        i = j;
    }
    spans
}

/// A read-only spreadsheet of the table: each title merged over its replicate
/// subcolumns, data set names as row headers, numbers right-aligned.
fn grid<'a>(t: &otto_plot::TextTable) -> El<'a> {
    let chars = |s: &str| s.chars().count() as f32 * 7.4 + 22.0;
    let mut widths: Vec<f32> = (0..t.headers.len())
        .map(|i| {
            let longest = t.rows.iter().take(200).map(|r| r.get(i).map_or(0.0, |s| chars(s))).fold(0.0, f32::max);
            longest.clamp(70.0, 260.0)
        })
        .collect();
    // Widen a merged title's columns evenly when the title needs more room.
    let spans = header_spans(&t.headers);
    for &(a, b) in &spans {
        let need = chars(&t.headers[a]).min(520.0);
        let have: f32 = widths[a..b].iter().sum();
        if need > have {
            let extra = (need - have) / (b - a) as f32;
            widths[a..b].iter_mut().for_each(|w| *w += extra);
        }
    }
    let total: f32 = widths.iter().sum();
    let header = Row::with_children(spans.iter().map(|&(a, b)| {
        let merged = b - a > 1;
        container(text(t.headers[a].clone()).size(12.5))
            .width(widths[a..b].iter().sum::<f32>())
            .padding([6, 8])
            .align_x(if merged { Alignment::Center } else { Alignment::Start })
            .style(style::header_cell(false))
            .into()
    }));
    // A blank top-left cell means the first column names the rows (data sets).
    let row_titles = t.headers.first().is_some_and(|h| h.is_empty());
    let mut rows = Column::new();
    for (ri, r) in t.rows.iter().take(MAX_ROWS).enumerate() {
        rows = rows.push(Row::with_children(widths.iter().enumerate().map(|(ci, w)| {
            let v = r.get(ci).cloned().unwrap_or_default();
            let numeric = !v.is_empty() && otto_core::table::parse_number(&v, false).is_some();
            let cell = container(text(v).size(12.5)).width(*w).padding([4, 8]).align_x(if numeric { Alignment::End } else { Alignment::Start });
            // The first row names the replicate subcolumns: a second header row.
            if ri == 0 || (row_titles && ci == 0) { cell.style(style::header_cell(false)) } else { cell.style(style::cell(false, ri % 2 == 0)) }.into()
        })));
    }
    if t.rows.len() > MAX_ROWS {
        rows = rows.push(container(caption(format!("Showing the first {MAX_ROWS} of {} rows; copy or export for all.", t.rows.len()))).padding(8));
    }
    scrollable(column![header, rows].width(Length::Fixed(total)))
        .direction(scrollable::Direction::Both { vertical: scrollable::Scrollbar::default(), horizontal: scrollable::Scrollbar::default() })
        .width(Fill)
        .height(Fill)
        .into()
}

pub fn panel<'a>(app: &'a App, graph_id: &str) -> El<'a> {
    let Some(g) = app.engine.project.graph(graph_id) else { return space().into() };
    let gid = Some(g.id.clone());
    let btn = |label: &'static str, i: Icon, msg: Message, primary: bool| -> El<'a> {
        button(row![icon(i, 14.0, if primary { iced::Color::WHITE } else { style::TEXT }), text(label).size(12)].spacing(6).align_y(Alignment::Center))
            .width(Fill)
            .padding([6, 8])
            .style(if primary { style::primary } else { style::secondary })
            .on_press(msg)
            .into()
    };
    let mut content = Column::new().spacing(18).padding(14);
    content = content.push(column![heading("Plotted table"), text(g.name.clone()).size(15)].spacing(6));
    content = content.push(
        column![
            heading("Use the data"),
            row![
                btn("Copy", Icon::Copy, Message::Cmd(Command::GraphCopyData(CopyDataArgs { graph: gid.clone(), layout: DataLayout::Wide })), true),
                btn("Export CSV…", Icon::Data, Message::ExportData, false),
            ]
            .spacing(6),
        ]
        .spacing(8),
    );
    content = content.push(btn("Show the graph", Icon::Kind(g.kind), show_graph(g), false));
    scrollable(content).height(Fill).into()
}
