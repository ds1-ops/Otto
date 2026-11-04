//! The fixed Prism-style publication theme and color palettes. All lengths are in points.

use otto_core::{Palette, Symbol};

use crate::scene::{Color, Item, Stroke};

/// Points per millimetre.
pub const PT_PER_MM: f64 = 72.0 / 25.4;

/// Which box the format size refers to. Prism sizes graphs by axis length, so do we.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SizeRef {
    /// Format size = axis lengths; labels, titles and legend are added around it.
    Axes,
    /// Format size = whole figure; the plot area shrinks to make room for labels.
    Figure,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub size_ref: SizeRef,
    pub font_family: &'static str,
    pub font_size: f64,
    pub bold: bool,
    pub line_width: f64,
    pub tick_len: f64,
    /// Gap between tick end and tick label.
    pub label_gap: f64,
    /// Gap between tick labels and the axis title.
    pub title_gap: f64,
    /// Symbol diameter.
    pub marker_size: f64,
    /// Bar width as a fraction of its category slot (column bar graphs).
    pub bar_frac: f64,
    /// Cluster width as a fraction of the category slot (grouped interleaved).
    pub cluster_frac: f64,
    /// Error-bar cap width as a fraction of the bar width.
    pub cap_frac: f64,
    /// Error-bar cap width on XY graphs.
    pub xy_cap: f64,
    pub legend_gap: f64,
    pub legend_swatch: f64,
    pub legend_row: f64,
    /// Outer padding around the figure.
    pub pad: f64,
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            size_ref: SizeRef::Axes,
            font_family: "Arial",
            font_size: 7.0,
            bold: true,
            line_width: 1.0,
            tick_len: 3.0,
            label_gap: 1.5,
            title_gap: 2.5,
            marker_size: 3.5,
            bar_frac: 0.6,
            cluster_frac: 0.8,
            cap_frac: 0.5,
            xy_cap: 3.0,
            legend_gap: 5.0,
            legend_swatch: 5.0,
            legend_row: 8.5,
            pad: 1.0,
        }
    }
}

// Journal-inspired palettes (ggsci's house palettes). Each one starts with its red and
// its blue: slots 0 and 1 are what the focused data sets take.
const NPG: [u32; 10] =
    [0xE64B35, 0x4DBBD5, 0x00A087, 0x3C5488, 0xF39B7F, 0x8491B4, 0x91D1C2, 0xDC0000, 0x7E6148, 0xB09C85];
const AAAS: [u32; 10] =
    [0xEE0000, 0x3B4992, 0x008B45, 0x631879, 0x008280, 0xBB0021, 0x5F559B, 0xA20056, 0x808180, 0x1B1919];
const NEJM: [u32; 8] = [0xBC3C29, 0x0072B5, 0xE18727, 0x20854E, 0x7876B1, 0x6F99AD, 0xFFDC91, 0xEE4C97];
const LANCET: [u32; 9] = [0xED0000, 0x00468B, 0x42B540, 0x0099B4, 0x925E9F, 0xFDAF91, 0xAD002A, 0xADB6B6, 0x1B1919];
const JAMA: [u32; 7] = [0xB24745, 0x00A1D5, 0x374E55, 0xDF8F44, 0x79AF97, 0x6A6599, 0x80796B];
const OKABE_ITO: [u32; 8] = [0xD55E00, 0x0072B2, 0xE69F00, 0x56B4E9, 0x009E73, 0xF0E442, 0xCC79A7, 0x000000];
// Bar fills stay light enough for black points (no black / near-black fills).
const OKABE_ITO_BARS: [u32; 7] = [0xD55E00, 0x0072B2, 0xE69F00, 0x56B4E9, 0x009E73, 0xF0E442, 0xCC79A7];
const CLASSIC_BARS: [u32; 4] = [0xFFFFFF, 0xA6A6A6, 0xD9D9D9, 0x8C8C8C];
const CLASSIC_MARKS: [u32; 4] = [0x000000, 0x7F7F7F, 0xBFBFBF, 0x404040];
const GRAY_BARS: [u32; 4] = [0xF0F0F0, 0xBDBDBD, 0x969696, 0xD9D9D9];
const GRAY_MARKS: [u32; 4] = [0x000000, 0x636363, 0x969696, 0xBDBDBD];

/// The color slot of a data set pushed out of focus in highlight mode: it is drawn gray,
/// whatever the palette.
pub const MUTED_SLOT: usize = usize::MAX;
const MUTED_BAR: u32 = 0xDEDEDE;
const MUTED_MARK: u32 = 0x9E9E9E;

/// The palette's colors, in slot order (red first, blue second where it has them).
pub fn palette_colors(palette: Palette) -> &'static [u32] {
    match palette {
        Palette::Npg => &NPG,
        Palette::Aaas => &AAAS,
        Palette::Nejm => &NEJM,
        Palette::Lancet => &LANCET,
        Palette::Jama => &JAMA,
        Palette::OkabeIto => &OKABE_ITO,
        Palette::Grayscale => &GRAY_MARKS,
        Palette::Classic => &CLASSIC_MARKS,
    }
}

fn pick(list: &[u32], i: usize) -> Color {
    Color::from_hex(list[i % list.len()])
}

/// Bar fill for category/data set `i`. Saturated palettes are tinted so black points stay visible.
pub fn bar_color(palette: Palette, i: usize) -> Color {
    if i == MUTED_SLOT {
        return Color::from_hex(MUTED_BAR);
    }
    match palette {
        Palette::OkabeIto => pick(&OKABE_ITO_BARS, i).tint(0.3),
        Palette::Classic => pick(&CLASSIC_BARS, i),
        Palette::Grayscale => pick(&GRAY_BARS, i),
        p => pick(palette_colors(p), i).tint(0.45),
    }
}

/// Symbol/line color for data set `i` of `n`. A single data set is drawn in black, as in Prism.
pub fn mark_color(palette: Palette, i: usize, n: usize) -> Color {
    if i == MUTED_SLOT {
        return Color::from_hex(MUTED_MARK);
    }
    if n <= 1 {
        return Color::BLACK;
    }
    pick(palette_colors(palette), i)
}

/// Individual points on bars are always black, whatever the bar fill.
pub fn point_on_bar(_fill: Color) -> Color {
    Color::BLACK
}

/// Volcano plots colour by direction, not by palette slot: down blue, unchanged grey, up red
/// (grey shades for the achromatic palettes).
pub fn volcano_color(palette: Palette, i: usize) -> Color {
    let hex = match (palette, i) {
        (_, i) if i == MUTED_SLOT => MUTED_MARK,
        (Palette::Classic | Palette::Grayscale, 0) => 0x525252,
        (Palette::Classic | Palette::Grayscale, 1) => 0xCCCCCC,
        (Palette::Classic | Palette::Grayscale, _) => 0x000000,
        (_, 0) => 0x3C5488,
        (_, 1) => 0xBDBDBD,
        (_, _) => 0xE64B35,
    };
    Color::from_hex(hex)
}

/// A symbol of diameter `d` centred on (cx, cy).
pub fn marker(symbol: Symbol, cx: f64, cy: f64, d: f64, fill: Color) -> Item {
    let r = d / 2.0;
    if symbol == Symbol::Circle {
        return Item::Circle { cx, cy, r, fill: Some(fill), stroke: None };
    }
    // Shapes are scaled so each covers about the same area as the circle of diameter `d`.
    let points: Vec<(f64, f64)> = match symbol {
        Symbol::Circle => unreachable!(),
        Symbol::Square => {
            let h = r * 0.886;
            vec![(cx - h, cy - h), (cx + h, cy - h), (cx + h, cy + h), (cx - h, cy + h)]
        }
        Symbol::TriangleUp => {
            let s = r * 1.347;
            vec![(cx, cy - s), (cx + s * COS30, cy + s / 2.0), (cx - s * COS30, cy + s / 2.0)]
        }
        Symbol::TriangleDown => {
            let s = r * 1.347;
            vec![(cx, cy + s), (cx + s * COS30, cy - s / 2.0), (cx - s * COS30, cy - s / 2.0)]
        }
        Symbol::Diamond => {
            let s = r * 1.253;
            vec![(cx, cy - s), (cx + s, cy), (cx, cy + s), (cx - s, cy)]
        }
        Symbol::Hexagon => {
            let s = r * 1.1;
            (0..6).map(|k| {
                let a = std::f64::consts::PI / 3.0 * k as f64 - std::f64::consts::FRAC_PI_2;
                (cx + s * a.cos(), cy + s * a.sin())
            }).collect()
        }
    };
    Item::Polygon { points, fill: Some(fill), stroke: None }
}

const COS30: f64 = 0.866_025_403_784_438_6;

/// An outlined shape (box plots, violins).
pub fn outlined(points: Vec<(f64, f64)>, fill: Option<Color>, stroke: Stroke) -> Item {
    Item::Polygon { points, fill, stroke: Some(stroke) }
}
