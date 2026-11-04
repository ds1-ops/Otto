//! A backend-neutral vector scene in points (1/72 inch), y pointing down.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color(pub u8, pub u8, pub u8);

impl Color {
    pub const BLACK: Color = Color(0, 0, 0);
    pub const WHITE: Color = Color(255, 255, 255);

    pub fn from_hex(v: u32) -> Self {
        Color((v >> 16) as u8, (v >> 8) as u8, v as u8)
    }

    pub fn hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.0, self.1, self.2)
    }

    /// Mixes the color with white; `amount` 0 = unchanged, 1 = white.
    pub fn tint(self, amount: f64) -> Self {
        let mix = |c: u8| (c as f64 + (255.0 - c as f64) * amount).round() as u8;
        Color(mix(self.0), mix(self.1), mix(self.2))
    }

    /// Relative luminance (0 = black, 1 = white).
    pub fn luminance(self) -> f64 {
        let lin = |c: u8| {
            let c = c as f64 / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(self.0) + 0.7152 * lin(self.1) + 0.0722 * lin(self.2)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stroke {
    pub color: Color,
    pub width: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    Start,
    Middle,
    End,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// A named layer (becomes `<g id>` in SVG so it is easy to find in Illustrator/Inkscape).
    Group { id: String, items: Vec<Item> },
    Line { x1: f64, y1: f64, x2: f64, y2: f64, stroke: Stroke },
    Polyline { points: Vec<(f64, f64)>, stroke: Stroke },
    /// A closed, optionally filled shape (symbols other than circles, violin outlines).
    Polygon { points: Vec<(f64, f64)>, fill: Option<Color>, stroke: Option<Stroke> },
    Rect { x: f64, y: f64, w: f64, h: f64, fill: Option<Color>, stroke: Option<Stroke> },
    Circle { cx: f64, cy: f64, r: f64, fill: Option<Color>, stroke: Option<Stroke> },
    /// Text whose baseline passes through (x, y), rotated by `rotate` degrees (negative = counter-clockwise).
    Text { x: f64, y: f64, text: String, size: f64, anchor: Anchor, rotate: f64, color: Color },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

impl Frame {
    pub fn right(&self) -> f64 {
        self.left + self.width
    }
    pub fn bottom(&self) -> f64 {
        self.top + self.height
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub width: f64,
    pub height: f64,
    pub font_family: String,
    pub bold: bool,
    /// The plot area (axis box) inside the figure.
    pub plot: Frame,
    pub items: Vec<Item>,
    /// Height added at the top for significance brackets (pt); without brackets the
    /// figure would be this much shorter and otherwise identical.
    pub stats_height: f64,
}

impl Scene {
    /// Calls `f` for every non-group item, depth first.
    pub fn walk(&self, mut f: impl FnMut(&Item)) {
        fn rec(items: &[Item], f: &mut dyn FnMut(&Item)) {
            for it in items {
                match it {
                    Item::Group { items, .. } => rec(items, f),
                    other => f(other),
                }
            }
        }
        rec(&self.items, &mut f);
    }
}
