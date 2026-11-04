//! Pie and donut charts: one slice per X category, sized by the **sum** of its Y values.
//!
//! A pie shows parts of a whole, so its slices add up rather than average: two rows of 3
//! make a slice of 6, not of 3. That is the one place Otto departs from the bar graphs,
//! where a category is its mean, and it is why a pie carries no error bars or points.
//! The scene has no axes; the circle fills the format's plot area and the categories are
//! named in the legend, with each slice's share written on it where it fits.

use otto_core::{GraphKind, GraphSpec};

use crate::distill::{PlotData, XValue};
use crate::draw::text;
use crate::font::{text_width, CAP_HEIGHT, DESCENT};
use crate::scene::{Anchor, Color, Frame, Item, Scene, Stroke};
use crate::theme::{bar_color, Theme, PT_PER_MM};

/// Inner radius of a donut, as a fraction of the outer one.
const DONUT_INNER: f64 = 0.55;
/// Arc resolution: one polygon point per this many degrees (the scene has no arcs, so a
/// slice is a polygon; at 2 degrees the straight edges are invisible at publication size).
const ARC_STEP_DEG: f64 = 2.0;
/// Clear space around a percentage written on a slice (pt).
const LABEL_PAD: f64 = 1.0;

/// Slice value per X category: the sum of the values plotted there. Categories with a
/// negative or zero total contribute nothing — a pie cannot show a negative share.
pub fn slices(data: &PlotData) -> Vec<f64> {
    let mut out = vec![0.0; data.x_levels.len().max(1)];
    for p in &data.points {
        if let XValue::Cat(i) = p.x {
            if i < out.len() && p.y.is_finite() {
                out[i] += p.y;
            }
        }
    }
    out.iter_mut().for_each(|v| *v = v.max(0.0));
    out
}

/// A point on the circle. Angles run clockwise from twelve o'clock, as pies are read.
fn on_circle(cx: f64, cy: f64, r: f64, turns: f64) -> (f64, f64) {
    let a = turns * std::f64::consts::TAU - std::f64::consts::FRAC_PI_2;
    (cx + r * a.cos(), cy + r * a.sin())
}

/// The outline of one slice, from `start` to `end` in turns (0–1) of the circle.
fn slice_polygon(cx: f64, cy: f64, r_out: f64, r_in: f64, start: f64, end: f64) -> Vec<(f64, f64)> {
    let steps = (((end - start) * 360.0 / ARC_STEP_DEG).ceil() as usize).max(2);
    let at = |i: usize| start + (end - start) * i as f64 / steps as f64;
    let mut pts: Vec<(f64, f64)> = (0..=steps).map(|i| on_circle(cx, cy, r_out, at(i))).collect();
    if r_in > 0.0 {
        pts.extend((0..=steps).rev().map(|i| on_circle(cx, cy, r_in, at(i))));
    } else {
        pts.push((cx, cy));
    }
    pts
}

pub fn draw(data: &PlotData, spec: &GraphSpec, theme: &Theme) -> Scene {
    let fs = theme.font_size;
    let cap = CAP_HEIGHT * fs;
    let desc = DESCENT * fs;
    let lw = theme.line_width;
    let black = Stroke { color: Color::BLACK, width: lw };
    let (wmm, hmm) = spec.format.axis_mm();
    let (w, h) = (wmm * PT_PER_MM, hmm * PT_PER_MM);

    let values = slices(data);
    let total: f64 = values.iter().sum();
    // Only the categories that actually have a slice; a legend entry for a slice that
    // was never drawn reads as a missing wedge.
    let shown: Vec<usize> = (0..data.x_levels.len()).filter(|&i| values[i] > 0.0).collect();
    let labels: Vec<&String> = shown.iter().map(|&i| &data.x_levels[i]).collect();

    // The circle is as large as the plot area allows and sits in the middle of it.
    let d = w.min(h);
    let legend = spec.show_legend && labels.len() > 1;
    let legend_w = if legend {
        theme.legend_swatch + 2.5 + labels.iter().map(|l| text_width(l, fs)).fold(0.0, f64::max)
    } else {
        0.0
    };
    let (left, top) = (theme.pad, theme.pad);
    let f = Frame { left, top, width: w, height: h };
    let (cx, cy) = (f.left + w / 2.0, f.top + h / 2.0);
    let r_out = d / 2.0;
    let r_in = if data.kind == GraphKind::Donut { r_out * DONUT_INNER } else { 0.0 };

    let mut wedges = Vec::new();
    let mut shares = Vec::new();
    let mut at = 0.0;
    for (i, &v) in values.iter().enumerate() {
        if v <= 0.0 || total <= 0.0 {
            continue;
        }
        let span = v / total;
        let (start, end) = (at, at + span);
        at = end;
        let fill = bar_color(spec.palette, data.x_color(i));
        wedges.push(Item::Polygon { points: slice_polygon(cx, cy, r_out, r_in, start, end), fill: Some(fill), stroke: Some(black) });

        // The share, written across the middle of the slice where there is room for it.
        // One decimal, with the trailing zero dropped: "25%", "33.3%".
        let label = format!("{}%", otto_core::table::format_number((1000.0 * span).round() / 10.0));
        let r_mid = (r_out + r_in) / 2.0;
        let (tx, ty) = on_circle(cx, cy, r_mid, (start + end) / 2.0);
        // The slice is as wide as its chord at the text's radius, and as deep as its band.
        let chord = 2.0 * r_mid * (span * std::f64::consts::PI).sin().abs();
        let band = r_out - r_in;
        if chord >= text_width(&label, fs) + 2.0 * LABEL_PAD && band >= cap + 2.0 * LABEL_PAD {
            let ink = if fill.luminance() > 0.179 { Color::BLACK } else { Color::WHITE };
            shares.push(Item::Text { x: tx, y: ty + cap / 2.0, text: label, size: fs, anchor: Anchor::Middle, rotate: 0.0, color: ink });
        }
    }

    let mut items = vec![Item::Group { id: "slices".into(), items: wedges }];
    if !shares.is_empty() {
        items.push(Item::Group { id: "shares".into(), items: shares });
    }
    if legend {
        let sw = theme.legend_swatch;
        let x0 = f.right() + theme.legend_gap;
        let mut rows = Vec::new();
        for (row, (&cat, l)) in shown.iter().zip(&labels).enumerate() {
            let y = f.top + row as f64 * theme.legend_row;
            rows.push(Item::Rect {
                x: x0,
                y: y + (theme.legend_row - sw) / 2.0,
                w: sw,
                h: sw,
                fill: Some(bar_color(spec.palette, data.x_color(cat))),
                stroke: Some(black),
            });
            rows.push(text(x0 + sw + 2.5, y + theme.legend_row / 2.0 + cap / 2.0, (*l).clone(), fs, Anchor::Start, 0.0));
        }
        items.push(Item::Group { id: "legend".into(), items: rows });
    }
    // The title names what the slices add up to, since a pie has no axis to carry it.
    let title_h = if data.y_title.is_empty() { 0.0 } else { cap + desc + theme.title_gap };
    if !data.y_title.is_empty() {
        items.push(Item::Group {
            id: "title".into(),
            items: vec![text(cx, f.bottom() + theme.title_gap + cap, data.y_title.clone(), fs, Anchor::Middle, 0.0)],
        });
    }
    let legend_h = labels.len() as f64 * theme.legend_row;
    Scene {
        width: left + w + if legend { theme.legend_gap + legend_w } else { 0.0 } + theme.pad,
        height: (top + h + title_h + theme.pad).max(if legend { top + legend_h + theme.pad } else { 0.0 }),
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
    fn slices_close_the_circle() {
        // A full turn of slices starts at twelve o'clock and comes back to it.
        let p = slice_polygon(0.0, 0.0, 10.0, 0.0, 0.0, 1.0);
        let first = p[0];
        assert!((first.0 - 0.0).abs() < 1e-9 && (first.1 + 10.0).abs() < 1e-9, "starts at the top: {first:?}");
        // Every point of a pie slice is inside the circle, and the last one is its centre.
        assert_eq!(*p.last().unwrap(), (0.0, 0.0));
        assert!(p.iter().all(|&(x, y)| x.hypot(y) <= 10.0 + 1e-9));
        // A donut sector never reaches the centre.
        let d = slice_polygon(0.0, 0.0, 10.0, 5.5, 0.25, 0.5);
        assert!(d.iter().all(|&(x, y)| x.hypot(y) >= 5.5 - 1e-9 && x.hypot(y) <= 10.0 + 1e-9));
    }
}
