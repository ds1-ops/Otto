//! Text measurement with Arial Bold metrics.
//!
//! Advance widths are embedded (taken from Arial Bold, 2048 units/em) so layout is
//! identical on every machine and in tests, whether or not Arial is installed.

const UNITS_PER_EM: f64 = 2048.0;

/// Advance widths for ASCII 32..=126.
const ASCII: [u16; 95] = [
    569, 682, 971, 1139, 1139, 1821, 1479, 487, 682, 682, 797, 1196, 569, 682, 569, 569, 1139, 1139, 1139, 1139,
    1139, 1139, 1139, 1139, 1139, 1139, 682, 682, 1196, 1196, 1196, 1251, 1997, 1479, 1479, 1479, 1479, 1366, 1251,
    1593, 1479, 569, 1139, 1479, 1251, 1706, 1479, 1593, 1366, 1593, 1479, 1366, 1251, 1479, 1366, 1933, 1366, 1366,
    1251, 682, 569, 682, 1196, 1139, 682, 1139, 1251, 1139, 1251, 1139, 682, 1251, 1251, 569, 569, 1139, 569, 1821,
    1251, 1251, 1251, 1251, 797, 1139, 682, 1251, 1139, 1593, 1139, 1139, 1024, 797, 573, 797, 1196,
];

/// Common scientific symbols outside ASCII.
const EXTRA: [(char, u16); 33] = [
    ('µ', 1180), ('°', 819), ('±', 1124), ('α', 1259), ('β', 1250), ('γ', 1139), ('δ', 1242), ('Δ', 1472),
    ('−', 1196), ('²', 682), ('³', 682), ('×', 1196), ('·', 682), ('κ', 1142), ('λ', 1139), ('ω', 1730),
    ('Ω', 1642), ('‰', 2048), ('–', 1139), ('—', 2048), ('é', 1139), ('ü', 1251), ('ö', 1251), ('ä', 1139),
    // Superscripts for axis labels in scientific notation (10³, 1.5×10⁻⁵) and the thin
    // space that separates thousands.
    ('⁰', 682), ('⁴', 682), ('⁵', 682), ('⁶', 682), ('⁷', 682), ('⁸', 682), ('⁹', 682), ('⁻', 717),
    ('\u{202f}', 569),
];

/// Cap height as a fraction of the font size.
pub const CAP_HEIGHT: f64 = 1466.0 / UNITS_PER_EM;
/// Ascender as a fraction of the font size.
pub const ASCENT: f64 = 1854.0 / UNITS_PER_EM;
/// Descender (positive) as a fraction of the font size.
pub const DESCENT: f64 = 434.0 / UNITS_PER_EM;

fn advance(c: char) -> u16 {
    let code = c as u32;
    if (32..=126).contains(&code) {
        return ASCII[(code - 32) as usize];
    }
    EXTRA.iter().find(|(ch, _)| *ch == c).map(|(_, w)| *w).unwrap_or(1139)
}

/// Width of `text` in points at `size` points.
pub fn text_width(text: &str, size: f64) -> f64 {
    text.chars().map(|c| advance(c) as f64).sum::<f64>() / UNITS_PER_EM * size
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digits_are_tabular() {
        assert_eq!(text_width("10", 7.0), text_width("88", 7.0));
        assert!((text_width("0", 2048.0) - 1139.0).abs() < 1e-9);
    }
}
