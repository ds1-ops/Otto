//! EMF backend (Windows GDI). Enhanced Metafiles paste into Word, PowerPoint,
//! Illustrator and CorelDRAW as editable vectors with live text.
//!
//! Logical units are 1/100 pt, so coordinates survive GDI's integer rounding.

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, POINT, RECT};
use windows::Win32::Graphics::Gdi::*;

use crate::scene::{Anchor, Color, Item, Scene, Stroke};
use crate::theme::PT_PER_MM;
use crate::PlotError;

const UNITS: f64 = 100.0;

fn u(v: f64) -> i32 {
    (v * UNITS).round() as i32
}

fn colorref(c: Color) -> COLORREF {
    COLORREF(c.0 as u32 | (c.1 as u32) << 8 | (c.2 as u32) << 16)
}

/// Renders the scene into EMF bytes.
pub fn to_emf(scene: &Scene) -> Result<Vec<u8>, PlotError> {
    unsafe {
        let screen = GetDC(None);
        let frame = RECT {
            left: 0,
            top: 0,
            right: (scene.width / PT_PER_MM * 100.0).round() as i32,
            bottom: (scene.height / PT_PER_MM * 100.0).round() as i32,
        };
        let hdc = CreateEnhMetaFileW(Some(screen), PCWSTR::null(), Some(&frame), w!("Otto\0Graph\0"));
        if hdc.is_invalid() {
            ReleaseDC(None, screen);
            return Err(PlotError::Render("could not create EMF".into()));
        }
        // Exact integer ratio between 1/100 pt and reference-device pixels.
        let (hres, hsize) = (GetDeviceCaps(Some(screen), HORZRES), GetDeviceCaps(Some(screen), HORZSIZE));
        let (vres, vsize) = (GetDeviceCaps(Some(screen), VERTRES), GetDeviceCaps(Some(screen), VERTSIZE));
        ReleaseDC(None, screen);
        SetMapMode(hdc, MM_ANISOTROPIC);
        let _ = SetWindowExtEx(hdc, hsize * 72_000, vsize * 72_000, None);
        let _ = SetViewportExtEx(hdc, hres * 254, vres * 254, None);
        SetBkMode(hdc, TRANSPARENT);

        let mut painter = Painter { hdc, family: scene.font_family.encode_utf16().chain([0]).collect(), bold: scene.bold };
        painter.items(&scene.items);

        let hemf = CloseEnhMetaFile(hdc);
        if hemf.is_invalid() {
            return Err(PlotError::Render("could not finish EMF".into()));
        }
        let size = GetEnhMetaFileBits(hemf, None);
        let mut bytes = vec![0u8; size as usize];
        GetEnhMetaFileBits(hemf, Some(&mut bytes));
        let _ = DeleteEnhMetaFile(Some(hemf));
        Ok(bytes)
    }
}

struct Painter {
    hdc: HDC,
    family: Vec<u16>,
    bold: bool,
}

impl Painter {
    unsafe fn items(&mut self, items: &[Item]) {
        for item in items {
            match item {
                Item::Group { items, .. } => self.items(items),
                Item::Line { x1, y1, x2, y2, stroke } => {
                    let old = self.select_pen(Some(stroke));
                    let _ = MoveToEx(self.hdc, u(*x1), u(*y1), None);
                    let _ = LineTo(self.hdc, u(*x2), u(*y2));
                    self.restore(old);
                }
                Item::Polyline { points, stroke } => {
                    let old = self.select_pen(Some(stroke));
                    let pts: Vec<POINT> = points.iter().map(|(x, y)| POINT { x: u(*x), y: u(*y) }).collect();
                    let _ = Polyline(self.hdc, &pts);
                    self.restore(old);
                }
                Item::Polygon { points, fill, stroke } => {
                    let old_pen = self.select_pen(stroke.as_ref());
                    let old_brush = self.select_brush(*fill);
                    let pts: Vec<POINT> = points.iter().map(|(x, y)| POINT { x: u(*x), y: u(*y) }).collect();
                    let _ = Polygon(self.hdc, &pts);
                    self.restore(old_brush);
                    self.restore(old_pen);
                }
                Item::Rect { x, y, w, h, fill, stroke } => {
                    let old_pen = self.select_pen(stroke.as_ref());
                    let old_brush = self.select_brush(*fill);
                    let _ = Rectangle(self.hdc, u(*x), u(*y), u(x + w), u(y + h));
                    self.restore(old_brush);
                    self.restore(old_pen);
                }
                Item::Circle { cx, cy, r, fill, stroke } => {
                    let old_pen = self.select_pen(stroke.as_ref());
                    let old_brush = self.select_brush(*fill);
                    let _ = Ellipse(self.hdc, u(cx - r), u(cy - r), u(cx + r), u(cy + r));
                    self.restore(old_brush);
                    self.restore(old_pen);
                }
                Item::Text { x, y, text, size, anchor, rotate, color } => {
                    let esc = (-rotate * 10.0).round() as i32;
                    let font = CreateFontW(
                        -u(*size),
                        0,
                        esc,
                        esc,
                        if self.bold { FW_BOLD.0 as i32 } else { FW_NORMAL.0 as i32 },
                        0,
                        0,
                        0,
                        DEFAULT_CHARSET,
                        OUT_TT_PRECIS,
                        CLIP_DEFAULT_PRECIS,
                        ANTIALIASED_QUALITY,
                        0,
                        PCWSTR(self.family.as_ptr()),
                    );
                    let old = SelectObject(self.hdc, font.into());
                    let align = match anchor {
                        Anchor::Start => TA_LEFT,
                        Anchor::Middle => TA_CENTER,
                        Anchor::End => TA_RIGHT,
                    };
                    SetTextAlign(self.hdc, TEXT_ALIGN_OPTIONS(TA_BASELINE.0 | align.0));
                    SetTextColor(self.hdc, colorref(*color));
                    let wide: Vec<u16> = text.encode_utf16().collect();
                    let _ = TextOutW(self.hdc, u(*x), u(*y), &wide);
                    SelectObject(self.hdc, old);
                    let _ = DeleteObject(font.into());
                }
            }
        }
    }

    /// Selects a pen (or the null pen); returns the previous object and the new pen to delete.
    unsafe fn select_pen(&mut self, stroke: Option<&Stroke>) -> (HGDIOBJ, Option<HGDIOBJ>) {
        match stroke {
            Some(s) => {
                let brush = LOGBRUSH { lbStyle: BS_SOLID, lbColor: colorref(s.color), lbHatch: 0 };
                let style = PEN_STYLE(PS_GEOMETRIC.0 | PS_SOLID.0 | PS_ENDCAP_FLAT.0 | PS_JOIN_MITER.0);
                let pen: HGDIOBJ = ExtCreatePen(style, u(s.width).max(1) as u32, &brush, None).into();
                (SelectObject(self.hdc, pen), Some(pen))
            }
            None => (SelectObject(self.hdc, GetStockObject(NULL_PEN)), None),
        }
    }

    unsafe fn select_brush(&mut self, fill: Option<Color>) -> (HGDIOBJ, Option<HGDIOBJ>) {
        match fill {
            Some(c) => {
                let brush: HGDIOBJ = CreateSolidBrush(colorref(c)).into();
                (SelectObject(self.hdc, brush), Some(brush))
            }
            None => (SelectObject(self.hdc, GetStockObject(NULL_BRUSH)), None),
        }
    }

    unsafe fn restore(&mut self, (old, created): (HGDIOBJ, Option<HGDIOBJ>)) {
        SelectObject(self.hdc, old);
        if let Some(obj) = created {
            let _ = DeleteObject(obj);
        }
    }
}
