//! The multivariable data table editor: a virtualized spreadsheet grid.
//!
//! Click selects, double-click / Enter / typing edits, Tab and arrows move,
//! Delete clears, Ctrl+C / Ctrl+V copy and paste blocks (tab-separated, e.g. from Excel).

use iced::widget::{button, column, container, mouse_area, row, scrollable, space, text, text_input, Row};
use iced::{Alignment, Fill, Length};

use otto_core::{Column, ColumnKind, ColumnType, DataTable};
use otto_engine::{AddColumnArgs, AddRowsArgs, Command};

use crate::app::{App, Message, CELL_EDITOR, GRID_SCROLL, HEADER_EDITOR, ROW_HEIGHT};
use crate::icons::{icon, Icon};
use crate::style;
use crate::view::{caption, El};

const ROW_HEADER_W: f32 = 46.0;

pub fn column_width(c: &Column) -> f32 {
    (c.name.chars().count() as f32 * 7.6 + 46.0).clamp(92.0, 240.0)
}

fn kind_color(t: ColumnType) -> iced::Color {
    match t {
        ColumnType::Numeric => iced::Color::from_rgb8(0x79, 0xC0, 0xFF),
        ColumnType::Percentage => iced::Color::from_rgb8(0x6E, 0xD6, 0xC0),
        ColumnType::Date | ColumnType::Time => iced::Color::from_rgb8(0xB9, 0xA6, 0xF0),
        ColumnType::Categorical => iced::Color::from_rgb8(0xF0, 0xB7, 0x2F),
        ColumnType::Text => iced::Color::from_rgb8(0xA5, 0xA9, 0xB1),
    }
}

pub fn view<'a>(app: &'a App, t: &'a DataTable) -> El<'a> {
    let widths: Vec<f32> = t.columns.iter().map(column_width).collect();
    let total_w = ROW_HEADER_W + widths.iter().sum::<f32>() + 44.0;

    // Header (stays put while rows scroll): click highlights, double-click renames.
    let mut header = Row::new().push(container(space()).width(ROW_HEADER_W).height(ROW_HEIGHT + 4.0).style(style::row_header));
    for (ci, c) in t.columns.iter().enumerate() {
        let selected = app.grid.highlight.contains(&ci);
        let el: El = match &app.grid.header_edit {
            Some((ec, buf)) if *ec == ci => text_input("Column name", buf)
                .id(HEADER_EDITOR)
                .on_input(Message::HeaderInput)
                .on_submit(Message::HeaderSubmit)
                .size(13)
                .padding([6, 7])
                .width(widths[ci])
                .style(style::cell_editor)
                .into(),
            _ => {
                let cell = container(
                    row![
                        text(c.name.clone()).size(13),
                        space::horizontal(),
                        text(c.column_type().badge()).size(11).color(kind_color(c.column_type())),
                    ]
                    .align_y(Alignment::Center),
                )
                .width(widths[ci])
                .height(ROW_HEIGHT + 4.0)
                .padding([4, 8])
                .align_y(Alignment::Center)
                .style(style::header_cell(selected));
                mouse_area(cell)
                    .on_press(Message::SelectColumn(ci))
                    .on_enter(Message::DragOverColumn(ci))
                    .on_double_click(Message::HeaderDoubleClicked(ci))
                    .into()
            }
        };
        header = header.push(el);
    }
    let add_col = button(icon(Icon::Plus, 14.0, style::MUTED))
        .padding([6, 12])
        .style(style::tool)
        .on_press(Message::Cmd(Command::TableAddColumn(AddColumnArgs {
            table: Some(t.id.clone()),
            name: t.unique_column_name(&format!("Column {}", t.ncols() + 1)),
            kind: ColumnKind::Numeric,
            at: None,
            values: Vec::new(),
        })));
    header = header.push(crate::view::tip(add_col, "Add a column"));

    // Visible rows only.
    let first = ((app.grid.scroll_y / ROW_HEIGHT).floor() as usize).saturating_sub(4);
    let visible = (app.grid.view_h / ROW_HEIGHT).ceil() as usize + 8;
    let last = (first + visible).min(t.nrows);
    let mut body = column![space().height(first as f32 * ROW_HEIGHT)];
    for r in first..last {
        let alt = r % 2 == 1;
        let row_selected = app.grid.rows.contains(&r);
        let head = container(text((r + 1).to_string()).size(11.5))
            .width(ROW_HEADER_W)
            .height(ROW_HEIGHT)
            .padding([4, 6])
            .align_x(Alignment::End)
            .align_y(Alignment::Center)
            .style(style::row_header_cell(row_selected));
        let mut line = Row::new().push(mouse_area(head).on_press(Message::SelectRow(r)).on_enter(Message::DragOverRow(r)));
        for (ci, c) in t.columns.iter().enumerate() {
            let selected = app.grid.sel == Some((r, ci));
            let w = widths[ci];
            let el: El = match (&app.grid.editing, selected) {
                (Some(buf), true) => text_input("", buf)
                    .id(CELL_EDITOR)
                    .on_input(Message::CellInput)
                    .on_submit(Message::CellSubmit)
                    .size(13)
                    .padding([4, 7])
                    .width(w)
                    .style(style::cell_editor)
                    .into(),
                _ => {
                    let value = text(c.display(r)).size(13).wrapping(iced::widget::text::Wrapping::None);
                    let cell = container(value)
                        .width(w)
                        .height(ROW_HEIGHT)
                        .padding([4, 8])
                        .align_y(Alignment::Center)
                        .align_x(if c.kind() == ColumnKind::Numeric { Alignment::End } else { Alignment::Start })
                        .clip(true);
                    let cell = if !selected && (row_selected || app.grid.highlight.contains(&ci)) {
                        cell.style(style::highlighted_cell(alt))
                    } else {
                        cell.style(style::cell(selected, alt))
                    };
                    mouse_area(cell).on_press(Message::CellClicked(r, ci)).on_double_click(Message::CellDoubleClicked(r, ci)).into()
                }
            };
            line = line.push(el);
        }
        body = body.push(line);
    }
    body = body.push(space().height((t.nrows - last) as f32 * ROW_HEIGHT));
    let add_rows = button(row![icon(Icon::Plus, 12.0, style::MUTED), caption("Add 10 rows")].spacing(6).align_y(Alignment::Center))
        .padding([6, 10])
        .style(style::tool)
        .on_press(Message::Cmd(Command::TableAddRows(AddRowsArgs { table: Some(t.id.clone()), count: 10, at: None })));
    body = body.push(container(add_rows).padding([6, 0]));

    let rows = scrollable(body.width(Length::Fixed(total_w)))
        .id(GRID_SCROLL)
        .on_scroll(Message::GridScrolled)
        .height(Fill)
        .width(Length::Fixed(total_w + 14.0));

    let grid = column![header.width(Length::Fixed(total_w)), rows].height(Fill);
    let title = row![
        icon(Icon::Table, 18.0, style::ACCENT),
        text(t.name.clone()).size(17),
        caption(format!("{} · {} rows × {} columns", t.id, t.nrows, t.ncols())),
        space::horizontal(),
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    column![
        container(title).padding([10, 14]),
        scrollable(container(grid).padding(iced::Padding { left: 14.0, right: 4.0, top: 0.0, bottom: 4.0 }))
            .direction(scrollable::Direction::Horizontal(scrollable::Scrollbar::default()))
            .height(Fill),
    ]
    .into()
}
