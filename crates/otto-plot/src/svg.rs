//! SVG backend. The file has its physical size in mm, a viewBox in points, live
//! `<text>` (editable in Illustrator/Inkscape) and named layers.

use std::fmt::Write;

use crate::scene::{Anchor, Item, Scene, Stroke};
use crate::theme::PT_PER_MM;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SvgOptions {
    /// Paint a white rectangle behind the graph (otherwise transparent).
    pub background: bool,
}

/// Formats a coordinate with at most 3 decimals.
fn n(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".to_string()
    } else {
        s.to_string()
    }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn stroke_attrs(s: &Stroke) -> String {
    format!(r#" stroke="{}" stroke-width="{}""#, s.color.hex(), n(s.width))
}

pub fn to_svg(scene: &Scene, opts: SvgOptions) -> String {
    let mut out = String::new();
    let _ = writeln!(out, r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    let _ = writeln!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" version="1.1" width="{}mm" height="{}mm" viewBox="0 0 {} {}">"#,
        n(scene.width / PT_PER_MM),
        n(scene.height / PT_PER_MM),
        n(scene.width),
        n(scene.height)
    );
    if opts.background {
        let _ = writeln!(out, r##"<rect id="background" x="0" y="0" width="{}" height="{}" fill="#FFFFFF"/>"##, n(scene.width), n(scene.height));
    }
    let font = format!(
        r#" font-family="{}, Helvetica, 'Liberation Sans', sans-serif" font-weight="{}""#,
        escape(&scene.font_family),
        if scene.bold { "bold" } else { "normal" }
    );
    write_items(&mut out, &scene.items, &font, 0);
    out.push_str("</svg>\n");
    out
}

fn write_items(out: &mut String, items: &[Item], font: &str, depth: usize) {
    let indent = "  ".repeat(depth);
    for item in items {
        match item {
            Item::Group { id, items } => {
                if items.is_empty() {
                    continue;
                }
                let _ = writeln!(out, r#"{indent}<g id="{}">"#, escape(id));
                write_items(out, items, font, depth + 1);
                let _ = writeln!(out, "{indent}</g>");
            }
            Item::Line { x1, y1, x2, y2, stroke } => {
                let _ = writeln!(
                    out,
                    r#"{indent}<line x1="{}" y1="{}" x2="{}" y2="{}"{} stroke-linecap="butt"/>"#,
                    n(*x1),
                    n(*y1),
                    n(*x2),
                    n(*y2),
                    stroke_attrs(stroke)
                );
            }
            Item::Polyline { points, stroke } => {
                let pts: Vec<String> = points.iter().map(|(x, y)| format!("{},{}", n(*x), n(*y))).collect();
                let _ = writeln!(
                    out,
                    r#"{indent}<polyline points="{}" fill="none"{} stroke-linecap="butt" stroke-linejoin="miter"/>"#,
                    pts.join(" "),
                    stroke_attrs(stroke)
                );
            }
            Item::Polygon { points, fill, stroke } => {
                let pts: Vec<String> = points.iter().map(|(x, y)| format!("{},{}", n(*x), n(*y))).collect();
                let fill = fill.map_or("none".to_string(), |c| c.hex());
                let stroke = stroke.as_ref().map_or(String::new(), |s| stroke_attrs(s) + r#" stroke-linejoin="miter""#);
                let _ = writeln!(out, r#"{indent}<polygon points="{}" fill="{fill}"{stroke}/>"#, pts.join(" "));
            }
            Item::Rect { x, y, w, h, fill, stroke } => {
                let fill = fill.map_or("none".to_string(), |c| c.hex());
                let stroke = stroke.as_ref().map_or(String::new(), |s| stroke_attrs(s) + r#" stroke-linejoin="miter""#);
                let _ = writeln!(
                    out,
                    r#"{indent}<rect x="{}" y="{}" width="{}" height="{}" fill="{fill}"{stroke}/>"#,
                    n(*x),
                    n(*y),
                    n(*w),
                    n(*h)
                );
            }
            Item::Circle { cx, cy, r, fill, stroke } => {
                let fill = fill.map_or("none".to_string(), |c| c.hex());
                let stroke = stroke.as_ref().map_or(String::new(), stroke_attrs);
                let _ = writeln!(out, r#"{indent}<circle cx="{}" cy="{}" r="{}" fill="{fill}"{stroke}/>"#, n(*cx), n(*cy), n(*r));
            }
            Item::Text { x, y, text, size, anchor, rotate, color } => {
                let anchor = match anchor {
                    Anchor::Start => "start",
                    Anchor::Middle => "middle",
                    Anchor::End => "end",
                };
                let place = if *rotate == 0.0 {
                    format!(r#"x="{}" y="{}""#, n(*x), n(*y))
                } else {
                    format!(r#"transform="translate({},{}) rotate({})""#, n(*x), n(*y), n(*rotate))
                };
                let _ = writeln!(
                    out,
                    r#"{indent}<text {place}{font} font-size="{}" text-anchor="{anchor}" fill="{}">{}</text>"#,
                    n(*size),
                    color.hex(),
                    escape(text)
                );
            }
        }
    }
}
