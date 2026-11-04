//! PNG, PDF and outlined-SVG output via resvg/usvg/svg2pdf (the same SVG renderer iced uses).

use std::sync::{Arc, OnceLock};

use resvg::tiny_skia;
use usvg::fontdb;

use crate::PlotError;

/// System fonts, loaded once (Arial on Windows; Liberation Sans/Helvetica elsewhere).
fn fonts() -> Arc<fontdb::Database> {
    static DB: OnceLock<Arc<fontdb::Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        db.set_sans_serif_family("Arial");
        Arc::new(db)
    })
    .clone()
}

fn parse(svg: &str) -> Result<usvg::Tree, PlotError> {
    let opt = usvg::Options { fontdb: fonts(), font_family: "Arial".into(), ..Default::default() };
    usvg::Tree::from_str(svg, &opt).map_err(|e| PlotError::Render(format!("SVG parse: {e}")))
}

/// Rasterizes an SVG at `dpi` on a white background.
pub fn svg_to_png(svg: &str, dpi: f32) -> Result<Vec<u8>, PlotError> {
    let tree = parse(svg)?;
    let scale = dpi / 96.0;
    let size = tree.size();
    let w = (size.width() * scale).ceil().max(1.0) as u32;
    let h = (size.height() * scale).ceil().max(1.0) as u32;
    let mut pixmap = tiny_skia::Pixmap::new(w, h).ok_or_else(|| PlotError::Render("image too large".into()))?;
    pixmap.fill(tiny_skia::Color::WHITE);
    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    pixmap.encode_png().map_err(|e| PlotError::Render(format!("PNG encode: {e}")))
}

/// Converts an SVG to a single-page PDF at the SVG's physical size, with fonts embedded.
pub fn svg_to_pdf(svg: &str) -> Result<Vec<u8>, PlotError> {
    let tree = parse(svg)?;
    // usvg resolves mm to px at 96 dpi; tell svg2pdf so the page keeps its physical size.
    svg2pdf::to_pdf(&tree, svg2pdf::ConversionOptions::default(), svg2pdf::PageOptions { dpi: 96.0 })
        .map_err(|e| PlotError::Render(format!("PDF conversion: {e}")))
}

/// Re-writes an SVG with all text converted to outlines (for journals that require it).
pub fn outline_text(svg: &str, width_mm: f64, height_mm: f64) -> Result<String, PlotError> {
    let tree = parse(svg)?;
    let out = tree.to_string(&usvg::WriteOptions { preserve_text: false, ..Default::default() });
    // usvg writes px sizes; restore the physical size on the root element.
    let Some(start) = out.find("<svg") else { return Ok(out) };
    let end = start + out[start..].find('>').unwrap_or(0);
    let root = &out[start..end];
    let fixed = replace_attr(&replace_attr(root, "width", &format!("{width_mm:.3}mm")), "height", &format!("{height_mm:.3}mm"));
    Ok(format!("{}{}{}", &out[..start], fixed, &out[end..]))
}

fn replace_attr(tag: &str, name: &str, value: &str) -> String {
    let key = format!(" {name}=\"");
    match tag.find(&key) {
        Some(i) => {
            let vstart = i + key.len();
            let vend = vstart + tag[vstart..].find('"').unwrap_or(0);
            format!("{}{}{}", &tag[..vstart], value, &tag[vend..])
        }
        None => tag.to_string(),
    }
}
