//! Contingency tables: rows are the X categories, columns are the data sets, and each cell
//! is the **count** in that row and column — the sum of the Y values plotted there.
//!
//! That one rule covers the three ways counts arrive. Already tabulated, as in Prism: X is
//! the row factor and the outcomes are several numeric Y columns, so each cell is one
//! number. Tabulated the long way: X is the row factor, Group by the column factor and Y
//! the count column. Or one row per subject with a column of 1s, which tallies them.
//!
//! The scene is a laid-out table rather than axes: rules above and below the header and
//! above the totals, row labels on the left, counts right-aligned under their headings.

use otto_core::GraphSpec;

use crate::distill::{PlotData, XValue};
use crate::draw::text;
use crate::font::{text_width, CAP_HEIGHT, DESCENT};
use crate::scene::{Anchor, Color, Frame, Item, Scene, Stroke};
use crate::theme::{Theme, PT_PER_MM};

/// Space left and right of a cell's text (pt).
const CELL_PAD: f64 = 3.0;
/// Row height as a multiple of the cap height.
const ROW_FACTOR: f64 = 2.1;

/// The counts, `[row = X category][column = data set]`. A cell with no rows behind it is
/// an empty count, which is not the same as a count of zero.
pub fn counts(data: &PlotData) -> Vec<Vec<Option<f64>>> {
    let (nr, nc) = (data.x_levels.len().max(1), data.sets.len().max(1));
    let mut m = vec![vec![None; nc]; nr];
    for p in &data.points {
        if let XValue::Cat(i) = p.x {
            if i < nr && p.set < nc && p.y.is_finite() {
                *m[i][p.set].get_or_insert(0.0) += p.y;
            }
        }
    }
    m
}

/// How a count is written: whole numbers without a decimal point, anything else as it is
/// (a "count" of 2.5 is a mistake worth seeing rather than hiding).
fn cell_text(v: Option<f64>) -> String {
    match v {
        Some(v) => otto_core::table::format_number(v),
        None => String::new(),
    }
}

/// The table as text: the header row, one row per X category with its counts and total,
/// and a final row of column totals. Used for drawing and for the data export.
pub fn grid(data: &PlotData) -> (Vec<String>, Vec<Vec<String>>) {
    let m = counts(data);
    let mut headers = vec![data.x_name.clone()];
    headers.extend(data.sets.iter().cloned());
    headers.push("Total".into());
    let mut rows = Vec::new();
    for (i, row) in m.iter().enumerate() {
        let mut out = vec![data.x_levels.get(i).cloned().unwrap_or_default()];
        out.extend(row.iter().map(|v| cell_text(*v)));
        out.push(cell_text(Some(row.iter().flatten().sum())));
        rows.push(out);
    }
    let mut totals = vec!["Total".to_string()];
    for c in 0..data.sets.len().max(1) {
        totals.push(cell_text(Some(m.iter().filter_map(|r| r[c]).sum())));
    }
    totals.push(cell_text(Some(m.iter().flatten().flatten().sum())));
    rows.push(totals);
    (headers, rows)
}

pub fn draw(data: &PlotData, spec: &GraphSpec, theme: &Theme) -> Scene {
    let fs = theme.font_size;
    let cap = CAP_HEIGHT * fs;
    let desc = DESCENT * fs;
    let black = Stroke { color: Color::BLACK, width: theme.line_width };
    let (headers, rows) = grid(data);
    let ncol = headers.len();
    let row_h = ROW_FACTOR * cap;

    // Columns are as wide as their widest entry; the table then grows to the format's
    // width, so choosing a wider format still widens it.
    let mut widths: Vec<f64> = (0..ncol)
        .map(|c| {
            let body = rows.iter().filter_map(|r| r.get(c)).map(|s| text_width(s, fs)).fold(0.0, f64::max);
            body.max(text_width(&headers[c], fs)) + 2.0 * CELL_PAD
        })
        .collect();
    let natural: f64 = widths.iter().sum();
    let wanted = spec.format.axis_mm().0 * PT_PER_MM;
    if wanted > natural && ncol > 1 {
        // The extra goes to the count columns, not to the row labels.
        let extra = (wanted - natural) / (ncol - 1) as f64;
        widths.iter_mut().skip(1).for_each(|w| *w += extra);
    }
    let w: f64 = widths.iter().sum();
    let h = (rows.len() + 1) as f64 * row_h;
    // Only a caption the user asked for: the automatic Y title would just repeat the
    // column headings, which already say what the counts are.
    let title = spec.y_axis.title.clone().unwrap_or_default();
    let title_h = if title.is_empty() { 0.0 } else { theme.title_gap + cap + desc };
    let f = Frame { left: theme.pad, top: theme.pad, width: w, height: h };

    // Right edge of column c, so numbers line up under their heading.
    let edge = |c: usize| f.left + widths[..=c].iter().sum::<f64>();
    let baseline = |r: usize| f.top + (r as f64 + 0.5) * row_h + cap / 2.0;
    let cell = |c: usize, r: usize, s: &str| {
        if c == 0 {
            text(f.left + CELL_PAD, baseline(r), s.to_string(), fs, Anchor::Start, 0.0)
        } else {
            text(edge(c) - CELL_PAD, baseline(r), s.to_string(), fs, Anchor::End, 0.0)
        }
    };

    let head: Vec<Item> = headers.iter().enumerate().map(|(c, label)| cell(c, 0, label)).collect();
    let mut body = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        for (c, s) in row.iter().enumerate() {
            if !s.is_empty() {
                body.push(cell(c, i + 1, s));
            }
        }
    }
    // Rules above and below the header, and above the totals row.
    let rule = |y: f64| Item::Line { x1: f.left, y1: y, x2: f.right(), y2: y, stroke: black };
    let rules = vec![rule(f.top), rule(f.top + row_h), rule(f.top + h - row_h), rule(f.bottom())];

    let mut items = vec![
        Item::Group { id: "rules".into(), items: rules },
        Item::Group { id: "header".into(), items: head },
        Item::Group { id: "counts".into(), items: body },
    ];
    if !title.is_empty() {
        items.push(Item::Group {
            id: "title".into(),
            items: vec![text(f.left, f.bottom() + theme.title_gap + cap, title, fs, Anchor::Start, 0.0)],
        });
    }

    Scene {
        width: f.width + 2.0 * theme.pad,
        height: f.height + title_h + 2.0 * theme.pad,
        font_family: theme.font_family.to_string(),
        bold: theme.bold,
        plot: f,
        items,
        stats_height: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_are_whole_numbers_without_a_point() {
        assert_eq!(cell_text(Some(12.0)), "12");
        assert_eq!(cell_text(Some(0.0)), "0");
        // Not a count, so it is shown as it is rather than rounded into looking like one.
        assert_eq!(cell_text(Some(2.5)), "2.5");
        assert_eq!(cell_text(None), "");
    }
}
