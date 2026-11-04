//! Layout and drawing: turns `PlotData` into a Prism-style `Scene`.
//!
//! The axis lengths are fixed by the format (30×30, 45×30 or 60×30 mm); margins are computed
//! from the measured tick labels, titles and legend so nothing is clipped.

use otto_core::{AxisScale, ErrorBars, GraphKind, GraphSpec, Replicates, Whiskers};

use crate::distill::{GroupStats, PlotData, XValue};
use crate::font::{text_width, ASCENT, CAP_HEIGHT, DESCENT};
use crate::scene::{Anchor, Color, Frame, Item, Scene, Stroke};
use crate::theme::{bar_color, mark_color, marker, outlined, point_on_bar, volcano_color, SizeRef, Theme, PT_PER_MM};
use crate::ticks::{scaled_ticks, target_intervals, AxisTicks};

const COS45: f64 = std::f64::consts::FRAC_1_SQRT_2;
/// Legend entries per column before a new column starts.
pub const LEGEND_ROWS: usize = 12;
/// Space between legend columns (pt).
const LEGEND_COL_GAP: f64 = 6.0;

/// One bar position on a categorical axis (fractions of the axis length).
#[derive(Clone, Debug)]
struct BarSlot {
    center: f64,
    width: f64,
    x: usize,
    set: usize,
    color: usize,
}

#[derive(Clone, Debug)]
struct CatLabel {
    pos: f64,
    span: f64,
    text: String,
}

enum XModel {
    Numeric(AxisTicks),
    Categorical { slots: Vec<BarSlot>, inner: Vec<CatLabel> },
}

struct Layout {
    frame: Frame,
    width: f64,
    height: f64,
    y: AxisTicks,
    y_labels: Vec<String>,
    y_label_w: f64,
    x: XModel,
    x_labels: Vec<(f64, String)>,
    rotate_x: bool,
    inner_block: f64,
    legend: bool,
    /// Width of each legend column (pt).
    legend_cols: Vec<f64>,
}

fn has_error(data: &PlotData) -> bool {
    data.summary.error != ErrorBars::None
}

/// Draws the graph. `spec` supplies format, axis limits and palette.
pub fn draw(data: &PlotData, spec: &GraphSpec, theme: &Theme) -> Scene {
    draw_with(data, spec, theme, &[])
}

/// A significance bracket between two bars (by bar label) with its text ("**", "ns").
#[derive(Clone, Debug, PartialEq)]
pub struct Bracket {
    pub a: String,
    pub b: String,
    pub text: String,
}

/// A fitted curve or regression line of one data set (by data set label), in data units.
#[derive(Clone, Debug, PartialEq)]
pub struct Curve {
    pub set: String,
    pub points: Vec<(f64, f64)>,
}

/// A bracket placed on the graph, in points. `y` is the bracket line relative to the top
/// of the plot area (negative = above it), so it does not depend on the figure margins.
#[derive(Clone, Debug)]
struct PlacedBracket {
    x1: f64,
    x2: f64,
    y: f64,
    text: String,
    /// Baseline of the text above the line.
    text_rise: f64,
    /// Top of the text, relative to the plot area like `y`.
    text_top: f64,
    /// Horizontal extent of the text.
    tx1: f64,
    tx2: f64,
}

impl PlacedBracket {
    /// Horizontal extent of everything drawn: line, end ticks and text.
    fn extent(&self) -> (f64, f64) {
        (self.x1.min(self.tx1), self.x2.max(self.tx2))
    }
}

/// Space between the data and the end ticks of the first bracket, in points.
const BRACKET_GAP: f64 = 2.0;
/// Length of the bracket end ticks, in points.
const BRACKET_TICK: f64 = 2.5;
/// Clear space between a bracket line and the text of a bracket below it, in points.
const BRACKET_CLEAR: f64 = 2.5;
/// Clear space between an end tick and the text it would otherwise touch, in points.
const BRACKET_TICK_CLEAR: f64 = 1.5;
/// Minimum distance between two overlapping bracket lines, in points.
const BRACKET_MIN_STEP: f64 = BRACKET_TICK + 2.5;
/// Horizontal padding around bracket text when checking for collisions, in points.
const BRACKET_TEXT_PAD: f64 = 1.0;
/// x-height of Arial Bold (lowercase text such as "ns"), as a fraction of the font size.
const X_HEIGHT: f64 = 0.53;

/// Draws the graph with significance brackets above the compared bars.
///
/// The graph itself (axis range, bar sizes, margins) is exactly what it is without
/// brackets: they are stacked above the data and the figure grows upward to make room.
pub fn draw_with(data: &PlotData, spec: &GraphSpec, theme: &Theme, brackets: &[Bracket]) -> Scene {
    draw_full(data, spec, theme, brackets, &[])
}

/// Draws the graph with significance brackets (bar graphs) and fitted curves (XY graphs).
/// Curves are drawn within the axes; they never change the axis ranges.
pub fn draw_full(data: &PlotData, spec: &GraphSpec, theme: &Theme, brackets: &[Bracket], curves: &[Curve]) -> Scene {
    if data.kind == GraphKind::Heatmap {
        return crate::heatmap::draw(data, spec, theme);
    }
    if data.kind.is_pie() {
        return crate::pie::draw(data, spec, theme);
    }
    if data.kind == GraphKind::Contingency {
        return crate::contingency::draw(data, spec, theme);
    }
    let (wmm, hmm) = spec.format.axis_mm();
    let (mut w, mut h) = (wmm * PT_PER_MM, hmm * PT_PER_MM);
    let mut lay = layout(data, spec, theme, w, h, 0.0);
    if theme.size_ref == SizeRef::Figure {
        // Shrink the plot area so the whole figure matches the format.
        for _ in 0..2 {
            w = (wmm * PT_PER_MM - (lay.width - lay.frame.width)).max(10.0);
            h = (hmm * PT_PER_MM - (lay.height - lay.frame.height)).max(10.0);
            lay = layout(data, spec, theme, w, h, 0.0);
        }
    }
    let mut placed = Vec::new();
    let mut stats_height = 0.0;
    if !brackets.is_empty() && matches!(lay.x, XModel::Categorical { .. }) {
        placed = place_brackets(data, &lay, brackets, theme);
        // Grow the top margin until the highest bracket text has the usual padding above it.
        let highest = placed.iter().map(|p| p.text_top).fold(0.0, f64::min);
        stats_height = (theme.pad - (lay.frame.top + highest)).max(0.0);
        if stats_height > 0.0 {
            lay = layout(data, spec, theme, w, h, stats_height);
        }
    }
    let mut scene = emit(data, spec, theme, &lay, &placed, curves);
    scene.stats_height = stats_height;
    scene
}

/// Stacks brackets above the bars, narrow spans first and wider ones above them, so that
/// no line, end tick or text touches another. Positions are in points from the top of the
/// plot area.
fn place_brackets(data: &PlotData, lay: &Layout, brackets: &[Bracket], theme: &Theme) -> Vec<PlacedBracket> {
    let XModel::Categorical { slots, .. } = &lay.x else { return Vec::new() };
    let f = lay.frame;
    let fs = theme.font_size;
    let r = theme.marker_size / 2.0;
    // Downward distance from the top of the plot area to a value.
    let below_top = |v: f64| (1.0 - lay.y.frac(v)) * f.height;
    // Centre (pt) and the highest drawn point (pt from the plot top) of every bar, by label.
    let mut bars: Vec<(String, f64, f64)> = Vec::new();
    for slot in slots {
        let Some(g) = data.groups.iter().find(|g| g.set == slot.set && g.x == XValue::Cat(slot.x)) else { continue };
        let mut top = below_top(g.center.clamp(lay.y.min, lay.y.max));
        if has_error(data) && g.high > g.low {
            top = top.min(below_top(g.high));
        }
        top = data.points_of(g).map(|p| below_top(p.y) - r).fold(top, f64::min);
        bars.push((data.group_label(g), f.left + slot.center * f.width, top));
    }
    let find = |label: &str| bars.iter().find(|b| b.0 == label).map(|b| b.1);
    let mut todo: Vec<(f64, f64, &str)> = brackets
        .iter()
        .filter_map(|b| {
            let (x1, x2) = (find(&b.a)?, find(&b.b)?);
            Some((x1.min(x2), x1.max(x2), b.text.as_str()))
        })
        .collect();
    todo.sort_by(|a, b| (a.1 - a.0).total_cmp(&(b.1 - b.0)).then(a.0.total_cmp(&b.0)));

    let mut placed: Vec<PlacedBracket> = Vec::new();
    for (x1, x2, text) in todo {
        // Stars float above the line on their own; lowercase text ("ns") needs a small lift.
        let stars = text.chars().all(|c| c == '*');
        let (rise, height) = if stars { (0.0, CAP_HEIGHT * fs) } else { (1.2, X_HEIGHT * fs) };
        let cx = (x1 + x2) / 2.0;
        let half = text_width(text, fs) / 2.0 + BRACKET_TEXT_PAD;
        let (tx1, tx2) = (cx - half, cx + half);
        let (e1, e2) = (x1.min(tx1), x2.max(tx2));
        // Clear every bar under the line or the text.
        let data_top = bars.iter().filter(|b| b.1 >= e1 - 0.01 && b.1 <= e2 + 0.01).map(|b| b.2).fold(f64::INFINITY, f64::min);
        let mut y = data_top.min(f.height) - BRACKET_GAP - BRACKET_TICK;
        for p in &placed {
            let (p1, p2) = p.extent();
            if p1 > e2 + 0.5 || p2 < e1 - 0.5 {
                continue;
            }
            y = y.min(p.y - BRACKET_MIN_STEP);
            // The line (or this text) must clear the text below.
            if (x1 <= p.tx2 && x2 >= p.tx1) || (tx1 <= p.tx2 && tx2 >= p.tx1) {
                y = y.min(p.text_top - BRACKET_CLEAR);
            }
            // An end tick coming down onto the text below needs room too.
            if (p.tx1..=p.tx2).contains(&x1) || (p.tx1..=p.tx2).contains(&x2) {
                y = y.min(p.text_top - BRACKET_TICK_CLEAR - BRACKET_TICK);
            }
        }
        placed.push(PlacedBracket { x1, x2, y, text: text.to_string(), text_rise: rise, text_top: y - rise - height, tx1, tx2 });
    }
    placed
}

fn y_extent(data: &PlotData, scale: AxisScale) -> (f64, f64) {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    let mut add = |v: f64| {
        // A log axis cannot show zero or negative values, so they do not set its range.
        if scale.forward(v).is_finite() {
            lo = lo.min(v);
            hi = hi.max(v);
        }
    };
    let kind = data.kind;
    if kind == GraphKind::Survival {
        return (0.0, 100.0);
    }
    if kind.is_stacked() {
        // A stack reaches as high as its segments add up to, so the totals set the range.
        if kind == GraphKind::StackedPercent {
            return (0.0, 100.0);
        }
        let spans = stack_spans(data);
        let ends = spans.iter().flatten().flat_map(|&(a, b)| [a, b]);
        return ends.fold((0.0f64, 0.0f64), |(lo, hi), v| (lo.min(v), hi.max(v)));
    }
    let draws_points =
        kind.has_bars() || kind.is_distribution() || matches!(kind, GraphKind::XyScatter | GraphKind::Volcano | GraphKind::Qq) || data.summary.replicates == Replicates::Individual;
    if draws_points {
        data.points.iter().for_each(|p| add(p.y));
    }
    if !matches!(kind, GraphKind::XyScatter | GraphKind::Volcano | GraphKind::Qq) {
        for g in &data.groups {
            add(g.center);
            if has_error(data) {
                add(g.low);
                add(g.high);
            }
        }
    }
    if !lo.is_finite() || !hi.is_finite() {
        return (if scale.is_log() { 1.0 } else { 0.0 }, if scale.is_log() { 10.0 } else { 1.0 });
    }
    (lo, hi)
}

/// Prism-like automatic range: bars always start at 0; positive XY data starts at 0
/// when the minimum is close to it.
fn with_zero(lo: f64, hi: f64, force: bool) -> (f64, f64) {
    if force {
        (lo.min(0.0), hi.max(0.0))
    } else if lo >= 0.0 && lo <= 0.25 * hi {
        (0.0, hi)
    } else {
        (lo, hi)
    }
}

/// `extra_top` adds room above the plot (for significance brackets) without changing it.
fn layout(data: &PlotData, spec: &GraphSpec, theme: &Theme, w: f64, h: f64, extra_top: f64) -> Layout {
    let fs = theme.font_size;
    let cap = CAP_HEIGHT * fs;
    let desc = DESCENT * fs;
    let kind = data.kind;

    // Y axis.
    let y_scale = spec.y_axis.scale;
    let (lo, hi) = y_extent(data, y_scale);
    // Bars start at 0; a log axis has no zero, so it keeps the data's own range.
    let (lo, hi) = if y_scale.is_log() { (lo, hi) } else { with_zero(lo, hi, kind.has_bars()) };
    let y = scaled_ticks(lo, hi, target_intervals(h), spec.y_axis.min, spec.y_axis.max, y_scale).numbered(spec.y_axis.numbers);
    let y_labels = y.labels();
    let y_label_w = y_labels.iter().map(|l| text_width(l, fs)).fold(0.0, f64::max);

    // X axis.
    let x = if kind.is_xy() {
        let x_scale = spec.x_axis.scale;
        let (lo, hi) = data.points.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), p| {
            let v = p.x.num();
            if x_scale.forward(v).is_finite() {
                (a.min(v), b.max(v))
            } else {
                (a, b)
            }
        });
        let (lo, hi) = if !lo.is_finite() || !hi.is_finite() {
            if x_scale.is_log() { (1.0, 10.0) } else { (0.0, 1.0) }
        } else if x_scale.is_log() {
            (lo, hi)
        } else if kind == GraphKind::Volcano || kind == GraphKind::Qq {
            // Centred on zero: fold changes and normal quantiles read symmetrically.
            let m = lo.abs().max(hi.abs());
            (-m, m)
        } else {
            // Survival curves start at time 0.
            with_zero(lo, hi, kind == GraphKind::Survival)
        };
        XModel::Numeric(scaled_ticks(lo, hi, target_intervals(w), spec.x_axis.min, spec.x_axis.max, x_scale).numbered(spec.x_axis.numbers))
    } else {
        categorical_model(data, theme)
    };
    let x_labels: Vec<(f64, String)> = match &x {
        XModel::Numeric(t) => t.ticks.iter().zip(t.labels()).map(|(&v, l)| (t.frac(v), l)).collect(),
        XModel::Categorical { inner, .. } => inner.iter().map(|l| (l.pos, l.text.clone())).collect(),
    };
    let rotate_x = match &x {
        XModel::Categorical { inner, .. } => inner.iter().any(|l| text_width(&l.text, fs) > l.span * w * 0.95),
        XModel::Numeric(_) => false,
    };
    let max_xw = x_labels.iter().map(|(_, l)| text_width(l, fs)).fold(0.0, f64::max);
    let inner_block = if rotate_x { COS45 * (cap + max_xw + desc) } else { cap + desc };

    let legend = spec.show_legend
        && data.sets.len() > 1
        && matches!(
            kind,
            GraphKind::XyPoints
                | GraphKind::XyPointsLine
                | GraphKind::XyLine
                | GraphKind::XyScatter
                | GraphKind::GroupedInterleaved
                | GraphKind::StackedBar
                | GraphKind::StackedPercent
                | GraphKind::Survival
                | GraphKind::Volcano
                | GraphKind::Qq
        );
    // At most LEGEND_ROWS entries per column; overflow continues in further columns to the right.
    let legend_cols: Vec<f64> = if legend {
        data.sets
            .chunks(LEGEND_ROWS)
            .map(|chunk| legend_swatch_w(kind, theme) + 2.5 + chunk.iter().map(|s| text_width(s, fs)).fold(0.0, f64::max))
            .collect()
    } else {
        Vec::new()
    };
    let legend_w = legend_cols.iter().sum::<f64>() + legend_cols.len().saturating_sub(1) as f64 * LEGEND_COL_GAP;
    let legend_h = data.sets.len().min(LEGEND_ROWS) as f64 * theme.legend_row;

    // Margins.
    let y_title_w = if data.y_title.is_empty() { 0.0 } else { ASCENT * fs + desc + theme.title_gap };
    let mut left = theme.pad + y_title_w + y_label_w + theme.label_gap + theme.tick_len;
    for (pos, label) in &x_labels {
        let lw = text_width(label, fs);
        let overhang = if rotate_x { COS45 * lw + 0.5 * COS45 * cap } else { lw / 2.0 };
        left = left.max(theme.pad + overhang - pos * w);
    }
    let x_title_h = if data.x_title.is_empty() { 0.0 } else { theme.title_gap + cap + desc };
    let bottom = theme.tick_len + theme.label_gap + inner_block + x_title_h + theme.pad;
    let top = theme.pad + (cap / 2.0).max(theme.marker_size / 2.0) + extra_top;
    let right = if legend {
        theme.legend_gap + legend_w + theme.pad
    } else {
        let overhang = x_labels
            .iter()
            .map(|(pos, l)| if rotate_x { 0.0 } else { text_width(l, fs) / 2.0 - (1.0 - pos) * w })
            .fold(theme.marker_size / 2.0, f64::max);
        theme.pad + overhang
    };

    Layout {
        frame: Frame { left, top, width: w, height: h },
        width: left + w + right,
        // A tall legend may reach below the plot area; the figure grows to fit it.
        height: (top + h + bottom).max(if legend { top + legend_h + theme.pad } else { 0.0 }),
        y,
        y_labels,
        y_label_w,
        x,
        x_labels,
        rotate_x,
        inner_block,
        legend,
        legend_cols,
    }
}

fn legend_swatch_w(kind: GraphKind, theme: &Theme) -> f64 {
    if matches!(kind, GraphKind::XyPointsLine | GraphKind::XyLine | GraphKind::Survival) {
        theme.legend_swatch * 1.8
    } else {
        theme.legend_swatch
    }
}

fn categorical_model(data: &PlotData, theme: &Theme) -> XModel {
    let n = data.x_levels.len().max(1);
    let k = data.sets.len().max(1);
    let mut slots = Vec::new();
    let mut inner = Vec::new();
    match data.kind {
        GraphKind::GroupedInterleaved => {
            // Each cluster holds only the data sets present at that X, centred, with one
            // bar width for the whole graph (identical to the full layout when nothing is missing).
            let present: Vec<Vec<usize>> = (0..n)
                .map(|i| {
                    let mut sets: Vec<usize> = data.groups.iter().filter(|g| g.x == XValue::Cat(i)).map(|g| g.set).collect();
                    sets.sort_unstable();
                    sets.dedup();
                    sets
                })
                .collect();
            let widest = present.iter().map(Vec::len).max().unwrap_or(1).max(1);
            let bw = theme.cluster_frac / n as f64 / widest as f64;
            for (i, sets) in present.iter().enumerate() {
                let c = (i as f64 + 0.5) / n as f64;
                let start = c - sets.len() as f64 * bw / 2.0;
                for (j, &s) in sets.iter().enumerate() {
                    slots.push(BarSlot { center: start + (j as f64 + 0.5) * bw, width: bw, x: i, set: s, color: data.set_color(s) });
                }
                inner.push(CatLabel { pos: c, span: 1.0 / n as f64, text: data.x_levels[i].clone() });
            }
        }
        GraphKind::GroupedSeparated => {
            // Each block only shows the categories that have data in that data set.
            let blocks = separated_blocks(data);
            let gap = 0.7;
            let total = blocks.iter().map(|b| b.len() as f64).sum::<f64>() + (k as f64 - 1.0) * gap;
            let u = 1.0 / total;
            let mut start = 0.0;
            for (b, levels) in blocks.iter().enumerate() {
                for (j, &i) in levels.iter().enumerate() {
                    let c = (start + j as f64 + 0.5) * u;
                    slots.push(BarSlot { center: c, width: 0.8 * u, x: i, set: b, color: data.set_color(b) });
                    // One label per bar: "X | data set" (just X when there is a single data set).
                    let text = if k > 1 { format!("{}{}{}", data.x_levels[i], otto_core::COMBINE_SEP, data.sets[b]) } else { data.x_levels[i].clone() };
                    inner.push(CatLabel { pos: c, span: u, text });
                }
                start += levels.len() as f64 + gap;
            }
        }
        stacked if stacked.is_stacked() => {
            // One bar per X category; its data sets become the segments, stacked in emit().
            let bw = theme.bar_frac / n as f64;
            for i in 0..n {
                let c = (i as f64 + 0.5) / n as f64;
                for set in 0..k {
                    slots.push(BarSlot { center: c, width: bw, x: i, set, color: data.set_color(set) });
                }
                inner.push(CatLabel { pos: c, span: 1.0 / n as f64, text: data.x_levels[i].clone() });
            }
        }
        _ => {
            for i in 0..n {
                let c = (i as f64 + 0.5) / n as f64;
                slots.push(BarSlot { center: c, width: theme.bar_frac / n as f64, x: i, set: 0, color: data.x_color(i) });
                inner.push(CatLabel { pos: c, span: 1.0 / n as f64, text: data.x_levels[i].clone() });
            }
        }
    }
    XModel::Categorical { slots, inner }
}

/// Where each segment of a stacked bar begins and ends, as `[x category][data set]` in
/// data units. Segments stack from zero in data-set order, positive ones upward and
/// negative ones downward; a category with no value for a data set gets an empty span.
/// On a 100% stack each category's positive segments are scaled to fill 0–100.
pub fn stack_spans(data: &PlotData) -> Vec<Vec<(f64, f64)>> {
    let n = data.x_levels.len().max(1);
    let k = data.sets.len().max(1);
    let mut center = vec![vec![None; k]; n];
    for g in &data.groups {
        if let XValue::Cat(i) = g.x {
            if i < n && g.set < k && g.center.is_finite() {
                center[i][g.set] = Some(g.center);
            }
        }
    }
    let mut spans = vec![vec![(0.0, 0.0); k]; n];
    for (i, row) in center.iter().enumerate() {
        let total: f64 = row.iter().flatten().filter(|v| **v > 0.0).sum();
        let scale = if data.kind == GraphKind::StackedPercent && total > 0.0 { 100.0 / total } else { 1.0 };
        let (mut up, mut down) = (0.0, 0.0);
        for (set, v) in row.iter().enumerate() {
            let Some(v) = v.map(|v| v * scale) else { continue };
            spans[i][set] = if v >= 0.0 {
                let foot = up;
                up += v;
                (foot, up)
            } else {
                let head = down;
                down += v;
                (down, head)
            };
        }
    }
    spans
}

/// For separated grouped graphs: per data set, the category indices that have data.
pub fn separated_blocks(data: &PlotData) -> Vec<Vec<usize>> {
    (0..data.sets.len())
        .map(|b| {
            let mut xs: Vec<usize> = data
                .groups
                .iter()
                .filter(|g| g.set == b)
                .filter_map(|g| if let XValue::Cat(i) = g.x { Some(i) } else { None })
                .collect();
            xs.sort_unstable();
            xs.dedup();
            xs
        })
        .collect()
}

/// Deterministic beeswarm: horizontal offsets (pt) so markers of diameter `d` don't overlap.
pub fn swarm(ys: &[f64], d: f64, max_half: f64) -> Vec<f64> {
    let n = ys.len();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| ys[a].total_cmp(&ys[b]).then(a.cmp(&b)));
    let mut xs = vec![0.0; n];
    let mut placed: Vec<usize> = Vec::with_capacity(n);
    let step = d * 0.55;
    let min_d2 = (d * 0.98).powi(2);
    for &i in &order {
        let mut best = (0.0, f64::NEG_INFINITY);
        for k in 0..200 {
            let m = ((k + 1) / 2) as f64;
            let off = if k % 2 == 1 { m * step } else { -m * step };
            let nearest = placed
                .iter()
                .map(|&j| (xs[j] - off).powi(2) + (ys[j] - ys[i]).powi(2))
                .fold(f64::INFINITY, f64::min);
            if nearest >= min_d2 {
                best = (off, nearest);
                break;
            }
            if nearest > best.1 {
                best = (off, nearest);
            }
        }
        xs[i] = best.0;
        placed.push(i);
    }
    // Center each horizontal band of points around the bar's middle.
    let mut start = 0;
    while start < n {
        let mut end = start + 1;
        while end < n && ys[order[end]] - ys[order[start]] < d {
            end += 1;
        }
        let band = &order[start..end];
        let mean = band.iter().map(|&i| xs[i]).sum::<f64>() / band.len() as f64;
        band.iter().for_each(|&i| xs[i] -= mean);
        start = end;
    }
    let widest = xs.iter().map(|x| x.abs()).fold(0.0, f64::max);
    if widest > max_half && widest > 0.0 {
        let s = max_half / widest;
        xs.iter_mut().for_each(|x| *x *= s);
    }
    xs
}

fn slug(s: &str) -> String {
    let mut out: String = s.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    out.trim_matches('-').to_string()
}

pub(crate) fn text(x: f64, y: f64, s: impl Into<String>, size: f64, anchor: Anchor, rotate: f64) -> Item {
    Item::Text { x, y, text: s.into(), size, anchor, rotate, color: Color::BLACK }
}

fn emit(data: &PlotData, spec: &GraphSpec, theme: &Theme, lay: &Layout, brackets: &[PlacedBracket], curves: &[Curve]) -> Scene {
    let f = lay.frame;
    let fs = theme.font_size;
    let cap = CAP_HEIGHT * fs;
    let desc = DESCENT * fs;
    let lw = theme.line_width;
    let black = Stroke { color: Color::BLACK, width: lw };
    let y_px = |v: f64| f.bottom() - lay.y.frac(v) * f.height;
    // A log axis cannot place zero or a negative value anywhere, so those are left out
    // rather than drawn off the figure.
    let on_y = |v: f64| lay.y.frac(v).is_finite();
    let r = theme.marker_size / 2.0;
    let nsets = data.sets.len();
    let mut data_items = Vec::new();

    match &lay.x {
        XModel::Categorical { slots, .. } => {
            // A log axis has no zero, so a bar stands on the foot of the axis instead.
            let (y_lo, y_hi) = lay.y.data_range();
            let clamp_y = |v: f64| if v.is_finite() { v.clamp(y_lo, y_hi) } else { y_lo };
            let baseline = if lay.y.scale.is_log() { f.bottom() } else { y_px(clamp_y(0.0)) };
            let mut bars = Vec::new();
            let mut points = Vec::new();
            let mut errors = Vec::new();
            let distribution = data.kind.is_distribution();
            // A column, box or violin graph has no data sets to tell apart, so its symbols
            // follow the bars; a grouped graph's follow its data sets.
            let symbol_of = |set: usize, x: usize| spec.symbol(if data.kind.is_column_like() { x } else { set });
            let spans = data.kind.is_stacked().then(|| stack_spans(data));
            for slot in slots {
                let Some(g) = data.groups.iter().find(|g| g.set == slot.set && g.x == XValue::Cat(slot.x)) else { continue };
                let cx = f.left + slot.center * f.width;
                let bw = slot.width * f.width;
                let fill = bar_color(spec.palette, slot.color);
                let id = format!("points-{}-{}", slot.x + 1, slug(&data.sets[slot.set]));
                if let Some(spans) = &spans {
                    // A stacked segment spans from the top of the one below it to its own
                    // total; error bars and individual points have nowhere to go.
                    let (foot, head) = spans[slot.x][slot.set];
                    if foot == head {
                        continue;
                    }
                    let (y0, y1) = (y_px(clamp_y(foot)), y_px(clamp_y(head)));
                    bars.push(Item::Rect { x: cx - bw / 2.0, y: y0.min(y1), w: bw, h: (y0 - y1).abs(), fill: Some(fill), stroke: Some(black) });
                    continue;
                }
                if distribution {
                    let values: Vec<f64> = data.points_of(g).map(|p| p.y).filter(|&v| on_y(v)).collect();
                    let (body, outliers) = if data.kind == GraphKind::Box {
                        box_items(&values, spec.whiskers, cx, bw, fill, black, &y_px)
                    } else {
                        violin_items(&values, cx, bw, fill, black, &y_px)
                    };
                    bars.extend(body);
                    let shape = symbol_of(slot.set, slot.x);
                    let mut marks: Vec<Item> = outliers.iter().map(|&y| marker(shape, cx, y, theme.marker_size, Color::BLACK)).collect();
                    if data.summary.replicates == Replicates::Individual {
                        let ys: Vec<f64> = values.iter().map(|&v| y_px(v)).collect();
                        let offsets = swarm(&ys, theme.marker_size, bw * 0.35);
                        marks = ys.iter().zip(&offsets).map(|(&y, &dx)| marker(shape, cx + dx, y, theme.marker_size, Color::BLACK)).collect();
                    }
                    points.push(Item::Group { id, items: marks });
                    continue;
                }
                if !on_y(g.center) {
                    continue;
                }
                let top = y_px(clamp_y(g.center));
                bars.push(Item::Rect {
                    x: cx - bw / 2.0,
                    y: top.min(baseline),
                    w: bw,
                    h: (baseline - top).abs(),
                    fill: Some(fill),
                    stroke: Some(black),
                });
                let ys: Vec<f64> = data.points_of(g).map(|p| p.y).filter(|&v| on_y(v)).map(&y_px).collect();
                let offsets = swarm(&ys, theme.marker_size, bw * 0.4);
                let dot = point_on_bar(fill);
                let shape = symbol_of(slot.set, slot.x);
                let group_points = ys.iter().zip(&offsets).map(|(&y, &dx)| marker(shape, cx + dx, y, theme.marker_size, dot)).collect();
                points.push(Item::Group { id, items: group_points });
                if has_error(data) && g.high > g.low && on_y(g.low) && on_y(g.high) {
                    errors.extend(error_bar(cx, y_px(g.low), y_px(g.high), bw * theme.cap_frac, black));
                }
            }
            data_items.push(Item::Group { id: if data.kind.is_distribution() { "boxes".to_string() } else { "bars".to_string() }, items: bars });
            data_items.push(Item::Group { id: "points".into(), items: points });
            data_items.push(Item::Group { id: "error-bars".into(), items: errors });
        }
        XModel::Numeric(xt) => {
            let x_px = |v: f64| f.left + xt.frac(v) * f.width;
            let on_xy = |x: f64, y: f64| xt.frac(x).is_finite() && on_y(y);
            // A symbol or error bar outside a range set by hand is left out, not drawn past the axes.
            let in_frame = |px: f64, py: f64| {
                px >= f.left - 1e-6 && px <= f.right() + 1e-6 && py >= f.top - 1e-6 && py <= f.bottom() + 1e-6
            };
            let shown = |x: f64, y: f64| on_xy(x, y) && in_frame(x_px(x), y_px(y));
            let clamp_py = |py: f64| py.clamp(f.top, f.bottom());
            let mut lines = Vec::new();
            let mut errors = Vec::new();
            let mut marks = Vec::new();
            if data.kind == GraphKind::Survival {
                // Kaplan-Meier steps (percent surviving) with a tick at each censored time.
                for (si, set) in data.sets.iter().enumerate() {
                    let stroke = Stroke { color: mark_color(spec.palette, data.set_color(si), nsets), width: lw };
                    let subjects: Vec<(f64, bool)> = data.points.iter().filter(|p| p.set == si).map(|p| (p.x.num(), p.y == 1.0)).collect();
                    let steps = crate::survival::kaplan_meier(&subjects);
                    let mut pts = vec![(x_px(0f64.max(xt.min)), y_px(100.0))];
                    let mut s = 100.0;
                    let mut ticks = Vec::new();
                    for st in &steps {
                        let x = x_px(st.time);
                        let next = st.survival * 100.0;
                        if next != s {
                            pts.push((x, y_px(s)));
                            pts.push((x, y_px(next)));
                            s = next;
                        }
                        if st.censored > 0 && in_frame(x, y_px(s)) {
                            let y = y_px(s);
                            ticks.push(Item::Line { x1: x, y1: y - theme.marker_size * 0.6, x2: x, y2: y + theme.marker_size * 0.6, stroke });
                        }
                    }
                    if let Some(last) = steps.last() {
                        pts.push((x_px(last.time), y_px(s)));
                    }
                    let mut items: Vec<Item> = clip_polyline(&pts, f).into_iter().map(|points| Item::Polyline { points, stroke }).collect();
                    items.extend(ticks);
                    marks.push(Item::Group { id: format!("survival-{}-{}", si + 1, slug(set)), items });
                }
            }
            for (si, set) in data.sets.iter().enumerate().filter(|_| data.kind != GraphKind::Survival) {
                let color = if data.kind == GraphKind::Volcano {
                    volcano_color(spec.palette, data.set_color(si))
                } else {
                    mark_color(spec.palette, data.set_color(si), nsets)
                };
                let shape = spec.symbol(si);
                let stroke = Stroke { color, width: lw };
                let groups: Vec<&GroupStats> = data.groups.iter().filter(|g| g.set == si).collect();
                // Scatter, volcano and QQ plots draw one symbol per row; there is nothing to average.
                let individual = matches!(data.kind, GraphKind::XyScatter | GraphKind::Volcano | GraphKind::Qq)
                    || data.summary.replicates == Replicates::Individual;
                if data.kind.has_line() {
                    if individual {
                        // One line per replicate (e.g. per mouse), sorted by X.
                        let mut reps: Vec<usize> = data.points.iter().filter(|p| p.set == si).map(|p| p.replicate).collect();
                        reps.sort_unstable();
                        reps.dedup();
                        for rep in reps.into_iter().filter(|&k| k != usize::MAX) {
                            let mut pts: Vec<(f64, f64)> = data
                                .points
                                .iter()
                                .filter(|p| p.set == si && p.replicate == rep && on_xy(p.x.num(), p.y))
                                .map(|p| (p.x.num(), p.y))
                                .collect();
                            pts.sort_by(|a, b| a.0.total_cmp(&b.0));
                            let px: Vec<(f64, f64)> = pts.into_iter().map(|(x, y)| (x_px(x), y_px(y))).collect();
                            lines.extend(clip_polyline(&px, f).into_iter().map(|points| Item::Polyline { points, stroke }));
                        }
                    } else if groups.len() > 1 {
                        let pts: Vec<(f64, f64)> =
                            groups.iter().filter(|g| on_xy(g.x.num(), g.center)).map(|g| (x_px(g.x.num()), y_px(g.center))).collect();
                        lines.extend(clip_polyline(&pts, f).into_iter().map(|points| Item::Polyline { points, stroke }));
                    }
                }
                let mut set_marks = Vec::new();
                let symbols = data.kind != GraphKind::XyLine;
                if individual {
                    if symbols {
                        for p in data.points.iter().filter(|p| p.set == si && shown(p.x.num(), p.y)) {
                            set_marks.push(marker(shape, x_px(p.x.num()), y_px(p.y), theme.marker_size, color));
                        }
                    }
                } else {
                    for g in groups.iter().filter(|g| shown(g.x.num(), g.center)) {
                        let cx = x_px(g.x.num());
                        if has_error(data) && g.high > g.low && on_y(g.low) && on_y(g.high) {
                            // An error bar reaching past a Y range set by hand stops at the axis.
                            errors.extend(error_bar(cx, clamp_py(y_px(g.low)), clamp_py(y_px(g.high)), theme.xy_cap, stroke));
                        }
                        if symbols {
                            set_marks.push(marker(shape, cx, y_px(g.center), theme.marker_size, color));
                        }
                    }
                }
                marks.push(Item::Group { id: format!("data-{}-{}", si + 1, slug(set)), items: set_marks });
            }
            // Volcano cutoffs and QQ reference line, drawn under the data.
            let mut guides = Vec::new();
            if data.kind == GraphKind::Volcano {
                let o = &spec.volcano;
                let dashed = Stroke { color: Color(0x80, 0x80, 0x80), width: lw * 0.75 };
                let y_cut = -o.p_cutoff.log10();
                if y_cut.is_finite() && lay.y.frac(y_cut) > 0.0 && lay.y.frac(y_cut) < 1.0 {
                    let y = y_px(y_cut);
                    guides.push(Item::Line { x1: f.left, y1: y, x2: f.right(), y2: y, stroke: dashed });
                }
                for cut in [-o.fc_cutoff, o.fc_cutoff] {
                    let t = xt.frac(cut);
                    if t > 0.0 && t < 1.0 {
                        let x = x_px(cut);
                        guides.push(Item::Line { x1: x, y1: f.top, x2: x, y2: f.bottom(), stroke: dashed });
                    }
                }
            }
            if data.kind == GraphKind::Qq {
                for si in 0..data.sets.len() {
                    let mut ys: Vec<f64> = data.points.iter().filter(|p| p.set == si).map(|p| p.y).collect();
                    ys.sort_by(f64::total_cmp);
                    // Robust line through the first and third quartiles, as Prism draws it.
                    let (q1, q3) = (crate::describe::percentile_sorted(&ys, 0.25), crate::describe::percentile_sorted(&ys, 0.75));
                    let (z1, z3) = (crate::distill::normal_quantile(0.25), crate::distill::normal_quantile(0.75));
                    if (z3 - z1).abs() < 1e-9 || !q1.is_finite() || !q3.is_finite() {
                        continue;
                    }
                    let slope = (q3 - q1) / (z3 - z1);
                    let at = |z: f64| q1 + slope * (z - z1);
                    let (lo, hi) = xt.data_range();
                    let stroke = Stroke { color: mark_color(spec.palette, data.set_color(si), nsets), width: lw * 0.75 };
                    let pts = vec![(x_px(lo), y_px(at(lo))), (x_px(hi), y_px(at(hi)))];
                    guides.extend(clip_polyline(&pts, f).into_iter().map(|points| Item::Polyline { points, stroke }));
                }
            }
            if !guides.is_empty() {
                data_items.push(Item::Group { id: "guides".into(), items: guides });
            }
            let mut fits = Vec::new();
            for c in curves {
                let Some(si) = data.sets.iter().position(|s| *s == c.set) else { continue };
                let stroke = Stroke { color: mark_color(spec.palette, data.set_color(si), nsets), width: lw };
                let pts: Vec<(f64, f64)> = c.points.iter().map(|&(x, y)| (x_px(x), y_px(y))).collect();
                fits.extend(clip_polyline(&pts, f).into_iter().map(|points| Item::Polyline { points, stroke }));
            }
            data_items.push(Item::Group { id: "lines".into(), items: lines });
            data_items.push(Item::Group { id: "fits".into(), items: fits });
            data_items.push(Item::Group { id: "error-bars".into(), items: errors });
            data_items.push(Item::Group { id: "data".into(), items: marks });
            if data.point_labels.len() == data.points.len() {
                let labels: Vec<Item> = data
                    .points
                    .iter()
                    .zip(&data.point_labels)
                    .filter(|(p, l)| !l.is_empty() && shown(p.x.num(), p.y))
                    .map(|(p, l)| text(x_px(p.x.num()) + r + 1.0, y_px(p.y) + cap / 2.0, l.clone(), fs * 0.9, Anchor::Start, 0.0))
                    .collect();
                if !labels.is_empty() {
                    data_items.push(Item::Group { id: "labels".into(), items: labels });
                }
            }
        }
    }

    // Axes first so data drawn on them (e.g. points at 0) stays visible, as in Prism.
    let mut items = Vec::new();
    // Axis frame: one L-shaped line so the corner is clean.
    items.push(Item::Group {
        id: "axes".into(),
        items: vec![Item::Polyline { points: vec![(f.left, f.top), (f.left, f.bottom()), (f.right(), f.bottom())], stroke: black }],
    });

    // Y axis ticks, labels and title.
    let mut yaxis = Vec::new();
    let label_x = f.left - theme.tick_len - theme.label_gap;
    for &v in &lay.y.minors {
        let y = y_px(v);
        yaxis.push(Item::Line { x1: f.left - theme.tick_len * 0.55, y1: y, x2: f.left, y2: y, stroke: black });
    }
    for (&v, label) in lay.y.ticks.iter().zip(&lay.y_labels) {
        let y = y_px(v);
        yaxis.push(Item::Line { x1: f.left - theme.tick_len, y1: y, x2: f.left, y2: y, stroke: black });
        yaxis.push(text(label_x, y + cap / 2.0, label.clone(), fs, Anchor::End, 0.0));
    }
    if !data.y_title.is_empty() {
        let x = label_x - lay.y_label_w - theme.title_gap - desc;
        yaxis.push(text(x, f.top + f.height / 2.0, data.y_title.clone(), fs, Anchor::Middle, -90.0));
    }
    items.push(Item::Group { id: "y-axis".into(), items: yaxis });

    // X axis ticks, labels and title.
    let mut xaxis = Vec::new();
    let y0 = f.bottom() + theme.tick_len + theme.label_gap;
    if let XModel::Numeric(t) = &lay.x {
        for &v in &t.minors {
            let x = f.left + t.frac(v) * f.width;
            xaxis.push(Item::Line { x1: x, y1: f.bottom(), x2: x, y2: f.bottom() + theme.tick_len * 0.55, stroke: black });
        }
    }
    for (pos, label) in &lay.x_labels {
        let x = f.left + pos * f.width;
        xaxis.push(Item::Line { x1: x, y1: f.bottom(), x2: x, y2: f.bottom() + theme.tick_len, stroke: black });
        if lay.rotate_x {
            xaxis.push(text(x + 0.5 * COS45 * cap, y0 + COS45 * cap, label.clone(), fs, Anchor::End, -45.0));
        } else {
            xaxis.push(text(x, y0 + cap, label.clone(), fs, Anchor::Middle, 0.0));
        }
    }
    if !data.x_title.is_empty() {
        let y = y0 + lay.inner_block + theme.title_gap + cap;
        xaxis.push(text(f.left + f.width / 2.0, y, data.x_title.clone(), fs, Anchor::Middle, 0.0));
    }
    items.push(Item::Group { id: "x-axis".into(), items: xaxis });
    items.extend(data_items);

    // Significance brackets: a 1 pt line between bar centres with short end ticks and the stars above.
    if !brackets.is_empty() {
        let mut sig = Vec::new();
        for b in brackets {
            let y = f.top + b.y;
            sig.push(Item::Polyline {
                points: vec![(b.x1, y + BRACKET_TICK), (b.x1, y), (b.x2, y), (b.x2, y + BRACKET_TICK)],
                stroke: black,
            });
            sig.push(text((b.x1 + b.x2) / 2.0, y - b.text_rise, b.text.clone(), fs, Anchor::Middle, 0.0));
        }
        items.push(Item::Group { id: "significance".into(), items: sig });
    }

    if lay.legend {
        items.push(Item::Group { id: "legend".into(), items: legend(data, spec, theme, lay) });
    }

    Scene {
        width: lay.width,
        height: lay.height,
        font_family: theme.font_family.to_string(),
        bold: theme.bold,
        plot: f,
        items,
        stats_height: 0.0,
    }
}

/// Splits a polyline (in points) into the pieces inside the plot area, ending each piece
/// exactly on the edge it crosses.
fn clip_polyline(pts: &[(f64, f64)], f: Frame) -> Vec<Vec<(f64, f64)>> {
    // Liang-Barsky: the part of the segment a-b inside the frame, if any.
    let clip = |a: (f64, f64), b: (f64, f64)| -> Option<((f64, f64), (f64, f64))> {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let (mut t0, mut t1) = (0.0f64, 1.0f64);
        for (p, q) in [
            (-dx, a.0 - f.left),
            (dx, f.right() - a.0),
            (-dy, a.1 - f.top),
            (dy, f.bottom() - a.1),
        ] {
            if p == 0.0 {
                if q < 0.0 {
                    return None;
                }
            } else {
                let r = q / p;
                if p < 0.0 {
                    t0 = t0.max(r);
                } else {
                    t1 = t1.min(r);
                }
            }
        }
        (t0 <= t1).then(|| ((a.0 + t0 * dx, a.1 + t0 * dy), (a.0 + t1 * dx, a.1 + t1 * dy)))
    };
    let mut out = Vec::new();
    let mut cur: Vec<(f64, f64)> = Vec::new();
    for w in pts.windows(2) {
        match clip(w[0], w[1]) {
            Some((p, q)) => {
                if cur.last() != Some(&p) {
                    if cur.len() >= 2 {
                        out.push(std::mem::take(&mut cur));
                    }
                    cur.clear();
                    cur.push(p);
                }
                cur.push(q);
                // The segment left the frame: this piece ends here.
                if q != w[1] {
                    out.push(std::mem::take(&mut cur));
                }
            }
            None => {
                if cur.len() >= 2 {
                    out.push(std::mem::take(&mut cur));
                }
                cur.clear();
            }
        }
    }
    out.push(cur);
    out.retain(|c| c.len() >= 2);
    out
}

/// The X axis range of an XY graph (where fitted curves are drawn), or `None` for bar graphs.
/// On a log axis the ends are returned in data units.
pub fn x_axis_range(data: &PlotData, spec: &GraphSpec, theme: &Theme) -> Option<(f64, f64)> {
    x_axis_ticks(data, spec, theme).map(|t| t.data_range())
}

/// The X axis of an XY graph as drawn, or `None` for bar graphs.
pub fn x_axis_ticks(data: &PlotData, spec: &GraphSpec, theme: &Theme) -> Option<AxisTicks> {
    let (wmm, hmm) = spec.format.axis_mm();
    let lay = layout(data, spec, theme, wmm * PT_PER_MM, hmm * PT_PER_MM, 0.0);
    match lay.x {
        XModel::Numeric(t) => Some(t),
        XModel::Categorical { .. } => None,
    }
}

/// The Y axis as drawn.
pub fn y_axis_ticks(data: &PlotData, spec: &GraphSpec, theme: &Theme) -> AxisTicks {
    let (wmm, hmm) = spec.format.axis_mm();
    layout(data, spec, theme, wmm * PT_PER_MM, hmm * PT_PER_MM, 0.0).y
}

fn error_bar(x: f64, y_low: f64, y_high: f64, cap_w: f64, stroke: Stroke) -> Vec<Item> {
    let h = cap_w / 2.0;
    vec![
        Item::Line { x1: x, y1: y_low, x2: x, y2: y_high, stroke },
        Item::Line { x1: x - h, y1: y_low, x2: x + h, y2: y_low, stroke },
        Item::Line { x1: x - h, y1: y_high, x2: x + h, y2: y_high, stroke },
    ]
}

fn legend(data: &PlotData, spec: &GraphSpec, theme: &Theme, lay: &Layout) -> Vec<Item> {
    let fs = theme.font_size;
    let cap = CAP_HEIGHT * fs;
    let s = theme.legend_swatch;
    let sw = legend_swatch_w(data.kind, theme);
    let nsets = data.sets.len();
    let mut items = Vec::new();
    for (i, name) in data.sets.iter().enumerate() {
        let (col, row) = (i / LEGEND_ROWS, i % LEGEND_ROWS);
        let x0 = lay.frame.right() + theme.legend_gap + lay.legend_cols[..col].iter().map(|w| w + LEGEND_COL_GAP).sum::<f64>();
        let yc = lay.frame.top + (row as f64 + 0.5) * theme.legend_row;
        if data.kind.has_bars() {
            items.push(Item::Rect {
                x: x0,
                y: yc - s / 2.0,
                w: s,
                h: s,
                fill: Some(bar_color(spec.palette, data.set_color(i))),
                stroke: Some(Stroke { color: Color::BLACK, width: theme.line_width }),
            });
        } else {
            let color = if data.kind == GraphKind::Volcano {
                volcano_color(spec.palette, data.set_color(i))
            } else {
                mark_color(spec.palette, data.set_color(i), nsets)
            };
            if matches!(data.kind, GraphKind::XyPointsLine | GraphKind::XyLine | GraphKind::Survival) {
                items.push(Item::Line { x1: x0, y1: yc, x2: x0 + sw, y2: yc, stroke: Stroke { color, width: theme.line_width } });
            }
            if !matches!(data.kind, GraphKind::Survival | GraphKind::XyLine) {
                items.push(marker(spec.symbol(i), x0 + sw / 2.0, yc, theme.marker_size, color));
            }
        }
        items.push(text(x0 + sw + 2.5, yc + cap / 2.0, name.clone(), fs, Anchor::Start, 0.0));
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_stop_at_the_edge_of_the_plot_area() {
        let f = Frame { left: 0.0, top: 0.0, width: 100.0, height: 100.0 };
        // Leaves through the right edge and never comes back.
        let pieces = clip_polyline(&[(10.0, 50.0), (90.0, 50.0), (150.0, 50.0), (200.0, 60.0)], f);
        assert_eq!(pieces, vec![vec![(10.0, 50.0), (90.0, 50.0), (100.0, 50.0)]]);
        // Both ends outside, but the segment crosses the area.
        let pieces = clip_polyline(&[(-50.0, 50.0), (150.0, 50.0)], f);
        assert_eq!(pieces, vec![vec![(0.0, 50.0), (100.0, 50.0)]]);
        // Out and back in: two pieces.
        let pieces = clip_polyline(&[(50.0, 50.0), (50.0, 150.0), (60.0, 50.0)], f);
        assert_eq!(pieces.len(), 2);
        assert!(pieces.iter().flatten().all(|p| p.1 <= 100.0 + 1e-6));
    }

    #[test]
    fn swarm_is_symmetric_and_non_overlapping() {
        let xs = swarm(&[10.0, 10.0], 3.5, 20.0);
        assert!((xs[0] + xs[1]).abs() < 1e-9 && (xs[0] - xs[1]).abs() >= 3.4);
        let ys = [5.0, 5.2, 5.1, 9.0, 12.0];
        let xs = swarm(&ys, 3.5, 50.0);
        for i in 0..ys.len() {
            for j in i + 1..ys.len() {
                let d = ((xs[i] - xs[j]).powi(2) + (ys[i] - ys[j]).powi(2)).sqrt();
                assert!(d > 3.0, "points {i} and {j} overlap");
            }
        }
        let xs = swarm(&[1.0; 10], 3.5, 5.0);
        assert!(xs.iter().all(|x| x.abs() <= 5.0 + 1e-9));
    }
}

/// A box plot: the quartile box with the median, the whiskers, and the outliers beyond
/// them (`y` maps a value to a pixel row).
fn box_items(
    values: &[f64],
    whiskers: Whiskers,
    cx: f64,
    bw: f64,
    fill: Color,
    stroke: Stroke,
    y: &impl Fn(f64) -> f64,
) -> (Vec<Item>, Vec<f64>) {
    use crate::describe::percentile_sorted;
    let mut v: Vec<f64> = values.iter().copied().filter(|x| x.is_finite()).collect();
    if v.is_empty() {
        return (Vec::new(), Vec::new());
    }
    v.sort_by(f64::total_cmp);
    let (q1, med, q3) = (percentile_sorted(&v, 0.25), percentile_sorted(&v, 0.5), percentile_sorted(&v, 0.75));
    let (lo_limit, hi_limit) = match whiskers {
        Whiskers::MinMax => (f64::NEG_INFINITY, f64::INFINITY),
        Whiskers::Tukey => {
            let iqr = q3 - q1;
            (q1 - 1.5 * iqr, q3 + 1.5 * iqr)
        }
        Whiskers::Pct10To90 => (percentile_sorted(&v, 0.10), percentile_sorted(&v, 0.90)),
    };
    let lo = v.iter().copied().find(|&x| x >= lo_limit).unwrap_or(v[0]);
    let hi = v.iter().rev().copied().find(|&x| x <= hi_limit).unwrap_or(v[v.len() - 1]);
    let outliers: Vec<f64> = v.iter().filter(|&&x| x < lo || x > hi).map(|&x| y(x)).collect();
    let (left, right) = (cx - bw / 2.0, cx + bw / 2.0);
    let cap = bw * 0.5;
    let items = vec![
        Item::Line { x1: cx, y1: y(hi), x2: cx, y2: y(q3), stroke },
        Item::Line { x1: cx, y1: y(q1), x2: cx, y2: y(lo), stroke },
        Item::Line { x1: cx - cap / 2.0, y1: y(hi), x2: cx + cap / 2.0, y2: y(hi), stroke },
        Item::Line { x1: cx - cap / 2.0, y1: y(lo), x2: cx + cap / 2.0, y2: y(lo), stroke },
        outlined(vec![(left, y(q3)), (right, y(q3)), (right, y(q1)), (left, y(q1))], Some(fill), stroke),
        Item::Line { x1: left, y1: y(med), x2: right, y2: y(med), stroke },
    ];
    (items, outliers)
}

/// A violin: a mirrored kernel density with the quartile box and the median inside it.
fn violin_items(values: &[f64], cx: f64, bw: f64, fill: Color, stroke: Stroke, y: &impl Fn(f64) -> f64) -> (Vec<Item>, Vec<f64>) {
    use crate::describe::percentile_sorted;
    let mut v: Vec<f64> = values.iter().copied().filter(|x| x.is_finite()).collect();
    if v.len() < 2 {
        return (Vec::new(), Vec::new());
    }
    v.sort_by(f64::total_cmp);
    let n = v.len() as f64;
    let mean = v.iter().sum::<f64>() / n;
    let sd = (v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt();
    let (q1, med, q3) = (percentile_sorted(&v, 0.25), percentile_sorted(&v, 0.5), percentile_sorted(&v, 0.75));
    // Silverman's rule of thumb, with a floor so a degenerate spread still draws.
    let spread = sd.min((q3 - q1) / 1.349).max(1e-12);
    let h = (0.9 * spread * n.powf(-0.2)).max((v[v.len() - 1] - v[0]).abs() * 1e-3).max(1e-12);
    const STEPS: usize = 48;
    // Trimmed to the observed range: the violin never claims density where there are no
    // values, and never reaches outside the axes.
    let (lo, hi) = (v[0], v[v.len() - 1]);
    let density =
        |at: f64| v.iter().map(|&x| (-0.5 * ((at - x) / h).powi(2)).exp()).sum::<f64>() / (n * h * (2.0 * std::f64::consts::PI).sqrt());
    let grid: Vec<(f64, f64)> = (0..=STEPS)
        .map(|i| {
            let at = lo + (hi - lo) * i as f64 / STEPS as f64;
            (at, density(at))
        })
        .collect();
    let peak = grid.iter().map(|&(_, d)| d).fold(0.0, f64::max).max(1e-12);
    let half = bw / 2.0;
    let mut outline: Vec<(f64, f64)> = grid.iter().map(|&(at, d)| (cx + d / peak * half, y(at))).collect();
    outline.extend(grid.iter().rev().map(|&(at, d)| (cx - d / peak * half, y(at))));
    // A slim black quartile bar with a white median dot: at a third of the old width an
    // outlined box would be all outline.
    let qw = (bw * 0.06).max(0.5);
    let items = vec![
        outlined(outline, Some(fill), stroke),
        Item::Line { x1: cx, y1: y(v[0]), x2: cx, y2: y(v[v.len() - 1]), stroke },
        Item::Rect { x: cx - qw, y: y(q3), w: 2.0 * qw, h: (y(q1) - y(q3)).abs(), fill: Some(Color::BLACK), stroke: None },
        Item::Circle { cx, cy: y(med), r: qw.max(0.9), fill: Some(Color::WHITE), stroke: None },
    ];
    (items, Vec::new())
}
