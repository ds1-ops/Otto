//! Otto's icon (`assets/otto.svg`): the window icon and the
//! generated `otto.ico` / extension PNG (see `examples/make_icons.rs`).

/// The icon as SVG.
pub const SVG: &[u8] = include_bytes!("../assets/otto.svg");

/// The icon rendered at `size` × `size` pixels, as straight (not premultiplied) RGBA.
pub fn rgba(size: u32) -> Option<Vec<u8>> {
    // tiny-skia stores premultiplied alpha.
    Some(
        pixmap(size)?
            .pixels()
            .iter()
            .flat_map(|p| {
                let c = p.demultiply();
                [c.red(), c.green(), c.blue(), c.alpha()]
            })
            .collect(),
    )
}

/// The icon as PNG at `size` × `size` pixels.
pub fn png(size: u32) -> Option<Vec<u8>> {
    pixmap(size)?.encode_png().ok()
}

fn pixmap(size: u32) -> Option<resvg::tiny_skia::Pixmap> {
    let tree = resvg::usvg::Tree::from_data(SVG, &resvg::usvg::Options::default()).ok()?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size)?;
    let s = size as f32 / tree.size().width().max(tree.size().height());
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(s, s), &mut pixmap.as_mut());
    Some(pixmap)
}

/// The window icon.
pub fn window_icon() -> Option<iced::window::Icon> {
    iced::window::icon::from_rgba(rgba(256)?, 256, 256).ok()
}

/// A Windows .ico holding the icon at the usual sizes (PNG-compressed entries).
pub fn ico() -> Option<Vec<u8>> {
    let sizes = [16u32, 20, 24, 32, 40, 48, 64, 128, 256];
    let images: Vec<Vec<u8>> = sizes.iter().map(|&s| png(s)).collect::<Option<_>>()?;
    let mut out = Vec::new();
    out.extend([0u8, 0, 1, 0]);
    out.extend((sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (&s, img) in sizes.iter().zip(&images) {
        let dim = if s >= 256 { 0 } else { s as u8 };
        out.extend([dim, dim, 0, 0]);
        out.extend(1u16.to_le_bytes()); // colour planes
        out.extend(32u16.to_le_bytes()); // bits per pixel
        out.extend((img.len() as u32).to_le_bytes());
        out.extend(offset.to_le_bytes());
        offset += img.len() as u32;
    }
    for img in images {
        out.extend(img);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn renders_at_every_size() {
        let px = super::rgba(32).unwrap();
        assert_eq!(px.len(), 32 * 32 * 4);
        // Opaque in the middle (the dark tile), transparent in the corner.
        let at = |x: usize, y: usize| px[(y * 32 + x) * 4 + 3];
        assert_eq!(at(16, 4), 255);
        assert_eq!(at(0, 0), 0);
        assert!(super::window_icon().is_some());
        let ico = super::ico().unwrap();
        assert_eq!(&ico[..4], &[0, 0, 1, 0]);
    }
}
