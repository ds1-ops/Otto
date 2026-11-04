//! Heatmaps: a grid of colored cells, columns = X categories, rows = data sets, each cell
//! the mean (or median) of its values, with a color bar. The grid fills the format's axis
//! area (e.g. 45 × 30 mm), labels sit outside it as on the other graphs.

use otto_core::{GraphSpec, HeatNormalize, HeatScale, HeatmapOptions};

use crate::distill::{PlotData, XValue};
use crate::draw::text;
use crate::font::{text_width, ASCENT, CAP_HEIGHT, DESCENT};
use crate::scene::{Anchor, Color, Frame, Item, Scene, Stroke};
use crate::theme::{Theme, PT_PER_MM};
use crate::ticks::axis_ticks;

const COS45: f64 = std::f64::consts::FRAC_1_SQRT_2;
/// Width of the color bar (pt).
const BAR_W: f64 = 6.0;
/// Cells without values.
const EMPTY: Color = Color(0xE6, 0xE6, 0xE6);

/// Cell values, `[row = data set][column = X category]`, after the normalisation.
pub fn cells(data: &PlotData, opts: &HeatmapOptions) -> Vec<Vec<Option<f64>>> {
    let (nr, nc) = (data.sets.len(), data.x_levels.len());
    let mut m = vec![vec![None; nc]; nr];
    for g in &data.groups {
        if let (XValue::Cat(c), true) = (g.x, g.set < nr) {
            if c < nc && g.center.is_finite() {
                m[g.set][c] = Some(g.center);
            }
        }
    }
    let z = |vals: &mut [&mut Option<f64>]| {
        let v: Vec<f64> = vals.iter().filter_map(|x| **x).collect();
        if v.len() < 2 {
            return;
        }
        let mean = v.iter().sum::<f64>() / v.len() as f64;
        let sd = (v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (v.len() as f64 - 1.0)).sqrt();
        for x in vals.iter_mut() {
            if let Some(val) = x.as_mut() {
                *val = if sd > 0.0 { (*val - mean) / sd } else { 0.0 };
            }
        }
    };
    match opts.normalize {
        HeatNormalize::None => {}
        HeatNormalize::Rows => {
            for row in m.iter_mut() {
                z(&mut row.iter_mut().collect::<Vec<_>>());
            }
        }
        HeatNormalize::Columns => {
            for c in 0..nc {
                let mut col: Vec<&mut Option<f64>> = m.iter_mut().map(|r| &mut r[c]).collect();
                z(&mut col);
            }
        }
    }
    m
}

/// The value range the colors span: min to max, or symmetric around 0 for the diverging
/// scale when the values have both signs.
pub fn range(cells: &[Vec<Option<f64>>], scale: HeatScale) -> (f64, f64) {
    let v: Vec<f64> = cells.iter().flatten().filter_map(|x| *x).collect();
    let (mut lo, mut hi) = v.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &x| (a.min(x), b.max(x)));
    if !lo.is_finite() {
        return (0.0, 1.0);
    }
    if scale == HeatScale::BlueRed && lo < 0.0 && hi > 0.0 {
        let m = lo.abs().max(hi.abs());
        (lo, hi) = (-m, m);
    }
    if hi - lo < 1e-12 {
        (lo - 0.5, hi + 0.5)
    } else {
        (lo, hi)
    }
}

/// Color stops of a scale, low to high.
pub fn stops(scale: HeatScale) -> &'static [u32] {
    match scale {
        HeatScale::Viridis => &[0x440154, 0x482878, 0x3E4A89, 0x31688E, 0x26828E, 0x1F9E89, 0x35B779, 0x6DCD59, 0xFDE725],
        HeatScale::Blues => &[0xF7FBFF, 0xDEEBF7, 0xC6DBEF, 0x9ECAE1, 0x6BAED6, 0x4292C6, 0x2171B5, 0x08519C, 0x08306B],
        HeatScale::BlueRed => &[0x2166AC, 0x4393C3, 0x92C5DE, 0xD1E5F0, 0xF7F7F7, 0xFDDBC7, 0xF4A582, 0xD6604D, 0xB2182B],
        HeatScale::Grayscale => &[0xF0F0F0, 0xD9D9D9, 0xBDBDBD, 0x969696, 0x737373, 0x525252, 0x252525],
    }
}

/// Color at position `t` (0–1) of a scale.
pub fn color(scale: HeatScale, t: f64) -> Color {
    let s = stops(scale);
    let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 };
    let pos = t * (s.len() - 1) as f64;
    let i = (pos.floor() as usize).min(s.len() - 2);
    let f = pos - i as f64;
    let (a, b) = (Color::from_hex(s[i]), Color::from_hex(s[i + 1]));
    let mix = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * f).round() as u8;
    Color(mix(a.0, b.0), mix(a.1, b.1), mix(a.2, b.2))
}

/// The title of the rows: the Group by column(s), or none when the rows are Y columns.
pub fn row_title(data: &PlotData, spec: &GraphSpec) -> String {
    spec.y_axis.title.clone().unwrap_or_else(|| data.set_name.clone().filter(|n| n != "Variable").unwrap_or_default())
}

/// What the color bar shows.
pub fn value_title(data: &PlotData, opts: &HeatmapOptions) -> String {
    match opts.normalize {
        HeatNormalize::None => data.y_title.clone(),
        HeatNormalize::Rows => "Z-score (rows)".into(),
        HeatNormalize::Columns => "Z-score (columns)".into(),
    }
}

/// Colour-bar ticks: both ends always labelled, plus the round values between them that
/// are not too close to an end. Labels use the heatmap's own number of decimals.
pub fn bar_ticks(lo: f64, hi: f64, digits: usize) -> (Vec<f64>, Vec<String>) {
    let mut ticks = vec![lo];
    if hi > lo {
        let span = hi - lo;
        for &v in &axis_ticks(lo, hi, 3, None, None).ticks {
            if v > lo + 0.15 * span && v < hi - 0.15 * span {
                ticks.push(v);
            }
        }
    }
    ticks.push(hi);
    let labels = ticks.iter().map(|v| format!("{:.*}", digits, v)).collect();
    (ticks, labels)
}

pub fn draw(data: &PlotData, spec: &GraphSpec, theme: &Theme) -> Scene {
    let opts = spec.heatmap;
    let fs = theme.font_size;
    let cap = CAP_HEIGHT * fs;
    let desc = DESCENT * fs;
    let lw = theme.line_width;
    let (wmm, hmm) = spec.format.axis_mm();
    let (w, h) = (wmm * PT_PER_MM, hmm * PT_PER_MM);
    let m = cells(data, &opts);
    let (nr, nc) = (data.sets.len().max(1), data.x_levels.len().max(1));
    let (cw, ch) = (w / nc as f64, h / nr as f64);
    let (lo, hi) = range(&m, opts.scale);

    // Labels and titles around the grid.
    let rotate = data.x_levels.iter().any(|l| text_width(l, fs) > cw * 0.95);
    let max_xw = data.x_levels.iter().map(|l| text_width(l, fs)).fold(0.0, f64::max);
    let inner_block = if rotate { COS45 * (cap + max_xw + desc) } else { cap + desc };
    let row_w = data.sets.iter().map(|s| text_width(s, fs)).fold(0.0, f64::max);
    let y_title = row_title(data, spec);
    let y_title_w = if y_title.is_empty() { 0.0 } else { ASCENT * fs + desc + theme.title_gap };
    let mut left = theme.pad + y_title_w + row_w + theme.label_gap;
    if rotate {
        // The first rotated label reaches left of its column.
        if let Some(first) = data.x_levels.first() {
            left = left.max(theme.pad + COS45 * text_width(first, fs) + 0.5 * COS45 * cap - cw / 2.0);
        }
    }
    let x_title_h = if data.x_title.is_empty() { 0.0 } else { theme.title_gap + cap + desc };
    let bottom = theme.label_gap + inner_block + x_title_h + theme.pad;
    let bar_title = value_title(data, &opts);
    let (ticks, labels) = bar_ticks(lo, hi, opts.digits());
    let label_w = labels.iter().map(|l| text_width(l, fs)).fold(0.0, f64::max);
    let legend = spec.show_legend;
    let top = theme.pad + if legend && !bar_title.is_empty() { cap + desc + theme.title_gap } else { 1.0 };
    let right = if legend {
        let bar_block = BAR_W + theme.tick_len * 0.7 + theme.label_gap + label_w;
        theme.legend_gap + bar_block.max(text_width(&bar_title, fs)) + theme.pad
    } else {
        let overhang = if rotate { 0.0 } else { data.x_levels.last().map(|l| (text_width(l, fs) - cw) / 2.0).unwrap_or(0.0) };
        theme.pad + overhang.max(0.0)
    };
    let f = Frame { left, top, width: w, height: h };
    let mut items = Vec::new();

    // Cells.
    let mut grid = Vec::new();
    let mut values = Vec::new();
    // Numbers are drawn only while they fit inside a cell; one that would spill over its
    // neighbours is worse than none, and a part-numbered heatmap reads as an error.
    let widest = m
        .iter()
        .flatten()
        .filter_map(|v| v.map(|v| text_width(&format!("{:.*}", opts.digits(), v), fs)))
        .fold(0.0, f64::max);
    let numbers_fit = widest <= cw - 1.0 && cap + 1.0 <= ch;
    for (r, row) in m.iter().enumerate() {
        for (c, v) in row.iter().enumerate() {
            let (x, y) = (f.left + c as f64 * cw, f.top + r as f64 * ch);
            let fill = match v {
                Some(v) => color(opts.scale, (v - lo) / (hi - lo)),
                None => EMPTY,
            };
            // A hair of overlap so no seams show between cells in viewers.
            grid.push(Item::Rect { x, y, w: cw + 0.05, h: ch + 0.05, fill: Some(fill), stroke: None });
            if let (true, Some(v)) = (opts.show_values && numbers_fit, v) {
                // Black or white, whichever contrasts more (they tie at relative luminance ≈ 0.18).
                let ink = if fill.luminance() > 0.179 { Color::BLACK } else { Color::WHITE };
                values.push(Item::Text {
                    x: x + cw / 2.0,
                    y: y + ch / 2.0 + cap / 2.0,
                    text: format!("{:.*}", opts.digits(), v),
                    size: fs,
                    anchor: Anchor::Middle,
                    rotate: 0.0,
                    color: ink,
                });
            }
        }
    }
    items.push(Item::Group { id: "cells".into(), items: grid });
    if !values.is_empty() {
        items.push(Item::Group { id: "values".into(), items: values });
    }

    // Row labels and title (left), column labels and title (below).
    let mut rows = Vec::new();
    for (r, s) in data.sets.iter().enumerate() {
        rows.push(text(f.left - theme.label_gap, f.top + (r as f64 + 0.5) * ch + cap / 2.0, s.clone(), fs, Anchor::End, 0.0));
    }
    if !y_title.is_empty() {
        let x = f.left - theme.label_gap - row_w - theme.title_gap - desc;
        rows.push(text(x, f.top + f.height / 2.0, y_title, fs, Anchor::Middle, -90.0));
    }
    items.push(Item::Group { id: "y-axis".into(), items: rows });
    let mut cols = Vec::new();
    let y0 = f.bottom() + theme.label_gap;
    for (c, l) in data.x_levels.iter().enumerate() {
        let x = f.left + (c as f64 + 0.5) * cw;
        if rotate {
            cols.push(text(x + 0.5 * COS45 * cap, y0 + COS45 * cap, l.clone(), fs, Anchor::End, -45.0));
        } else {
            cols.push(text(x, y0 + cap, l.clone(), fs, Anchor::Middle, 0.0));
        }
    }
    if !data.x_title.is_empty() {
        cols.push(text(f.left + f.width / 2.0, y0 + inner_block + theme.title_gap + cap, data.x_title.clone(), fs, Anchor::Middle, 0.0));
    }
    items.push(Item::Group { id: "x-axis".into(), items: cols });

    // Color bar: low at the bottom, high at the top.
    if legend {
        let x0 = f.right() + theme.legend_gap;
        let mut bar = Vec::new();
        let n = 64;
        for i in 0..n {
            let t0 = i as f64 / n as f64;
            let y = f.bottom() - (i + 1) as f64 / n as f64 * f.height;
            bar.push(Item::Rect { x: x0, y, w: BAR_W, h: f.height / n as f64 + 0.05, fill: Some(color(opts.scale, t0 + 0.5 / n as f64)), stroke: None });
        }
        let black = Stroke { color: Color::BLACK, width: lw };
        bar.push(Item::Rect { x: x0, y: f.top, w: BAR_W, h: f.height, fill: None, stroke: Some(black) });
        for (&v, l) in ticks.iter().zip(&labels) {
            let y = f.bottom() - (v - lo) / (hi - lo) * f.height;
            bar.push(Item::Line { x1: x0 + BAR_W, y1: y, x2: x0 + BAR_W + theme.tick_len * 0.7, y2: y, stroke: black });
            bar.push(text(x0 + BAR_W + theme.tick_len * 0.7 + theme.label_gap, y + cap / 2.0, l.clone(), fs, Anchor::Start, 0.0));
        }
        if !bar_title.is_empty() {
            bar.push(text(x0, f.top - theme.title_gap - desc, bar_title, fs, Anchor::Start, 0.0));
        }
        items.push(Item::Group { id: "legend".into(), items: bar });
    }

    Scene {
        width: left + w + right,
        height: top + h + bottom,
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
    fn scales_and_ranges() {
        assert_eq!(color(HeatScale::Viridis, 0.0), Color::from_hex(0x440154));
        assert_eq!(color(HeatScale::Viridis, 1.0), Color::from_hex(0xFDE725));
        assert_eq!(color(HeatScale::BlueRed, 0.5), Color::from_hex(0xF7F7F7));
        // Diverging: symmetric around 0 when both signs occur.
        assert_eq!(range(&[vec![Some(-1.0), Some(3.0)]], HeatScale::BlueRed), (-3.0, 3.0));
        assert_eq!(range(&[vec![Some(-1.0), Some(3.0)]], HeatScale::Viridis), (-1.0, 3.0));
        assert_eq!(range(&[vec![Some(2.0), None]], HeatScale::Viridis), (1.5, 2.5));
    }
}
