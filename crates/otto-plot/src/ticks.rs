//! "Nice" axis ranges and tick labels (steps of 1, 2 or 5 × 10ⁿ; axes end on a tick),
//! for linear and logarithmic axes.

use otto_core::{AxisNumbers, AxisScale};

#[derive(Clone, Debug, PartialEq)]
pub struct AxisTicks {
    /// Axis ends, in *scale* units (the same as the data for a linear axis, log of the
    /// data for a logarithmic one).
    pub min: f64,
    pub max: f64,
    pub step: f64,
    /// Labelled ticks, in data units.
    pub ticks: Vec<f64>,
    /// Unlabelled minor ticks (log axes), in data units.
    pub minors: Vec<f64>,
    pub decimals: usize,
    pub scale: AxisScale,
    /// How the labels are written (plain numbers by default).
    pub numbers: AxisNumbers,
}

impl AxisTicks {
    /// The same ticks written in another number format.
    pub fn numbered(mut self, numbers: AxisNumbers) -> Self {
        self.numbers = numbers;
        self
    }

    pub fn labels(&self) -> Vec<String> {
        let plain = |v: f64| if self.scale.is_log() { log_label(v) } else { format_tick(v, self.decimals) };
        self.ticks
            .iter()
            .map(|&v| match self.numbers {
                AxisNumbers::Plain => plain(v),
                AxisNumbers::Grouped => group_thousands(&plain(v)),
                AxisNumbers::Scientific => scientific_label(v, false),
                AxisNumbers::Power10 => scientific_label(v, true),
                AxisNumbers::LogValue => log_value_label(v, self.scale),
            })
            .collect()
    }

    /// Position of `v` as a fraction of the axis (0 at min, 1 at max). NaN when the value
    /// cannot be shown (zero or negative on a log axis).
    pub fn frac(&self, v: f64) -> f64 {
        (self.scale.forward(v) - self.min) / (self.max - self.min)
    }

    /// The axis ends in data units.
    pub fn data_range(&self) -> (f64, f64) {
        (self.scale.inverse(self.min), self.scale.inverse(self.max))
    }
}

/// A tick label on a log axis: plain decimals where they stay short, else 1e-5 notation.
pub fn log_label(v: f64) -> String {
    let a = v.abs();
    if a == 0.0 {
        return "0".into();
    }
    if (1e-4..1e6).contains(&a) {
        // As few decimals as still show the value exactly (2 for 0.25, none for 64).
        let decimals = (0..=6).find(|&d| format!("{v:.d$}").parse::<f64>().is_ok_and(|x| (x - v).abs() <= a * 1e-9)).unwrap_or(6);
        format_tick(v, decimals)
    } else {
        let e = a.log10().round() as i32;
        if (10f64.powi(e) - a).abs() < a * 1e-9 {
            format!("1e{e}")
        } else {
            format!("{v:e}")
        }
    }
}

/// Digits as superscripts, for 10³ and 1.5×10⁻⁵.
fn superscript(n: i32) -> String {
    const DIGITS: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];
    let mut s: String = if n < 0 { "⁻".to_string() } else { String::new() };
    s.extend(n.unsigned_abs().to_string().chars().map(|c| DIGITS[c.to_digit(10).unwrap_or(0) as usize]));
    s
}

/// 1.5×10³; with `bare`, a whole power of ten drops its mantissa and reads 10³.
pub fn scientific_label(v: f64, bare: bool) -> String {
    if v == 0.0 || !v.is_finite() {
        return format_tick(v, 0);
    }
    let e = v.abs().log10().floor() as i32;
    let m = v / 10f64.powi(e);
    // Rounding can push the mantissa to 10 (0.00009999…); carry it into the exponent.
    let (m, e) = if m.abs() >= 9.9995 { (m / 10.0, e + 1) } else { (m, e) };
    if bare && (m.abs() - 1.0).abs() < 1e-9 {
        let sign = if m < 0.0 { "-" } else { "" };
        return format!("{sign}10{}", superscript(e));
    }
    format!("{}×10{}", trim_zeros(m, 3), superscript(e))
}

/// The exponent of `v` on a log axis (3 for 1000), or log10 on a linear one.
fn log_value_label(v: f64, scale: AxisScale) -> String {
    let base = scale.base().unwrap_or(10.0);
    if v <= 0.0 || !v.is_finite() {
        return format_tick(v, 0);
    }
    trim_zeros(v.ln() / base.ln(), 3)
}

/// `v` with at most `decimals` decimals and no trailing zeros.
fn trim_zeros(v: f64, decimals: usize) -> String {
    let s = format!("{v:.decimals$}");
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.') } else { &s };
    if s == "-0" {
        "0".to_string()
    } else {
        s.to_string()
    }
}

/// Thin spaces between groups of three digits: 25 000. Four digits stay together, as
/// they usually do in print, and the decimals are left alone.
fn group_thousands(label: &str) -> String {
    let (int, rest) = match label.find('.') {
        Some(i) => label.split_at(i),
        None => (label, ""),
    };
    let (sign, digits) = match int.strip_prefix('-') {
        Some(d) => ("-", d),
        None => ("", int),
    };
    if !digits.chars().all(|c| c.is_ascii_digit()) || digits.len() <= 4 {
        return label.to_string();
    }
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(' ');
        }
        out.push(c);
    }
    format!("{sign}{out}{rest}")
}

pub fn nice_step(range: f64, target: usize) -> f64 {
    let raw = range / target.max(1) as f64;
    let mag = 10f64.powf(raw.log10().floor());
    let norm = raw / mag;
    let nice = if norm <= 1.0 {
        1.0
    } else if norm <= 2.0 {
        2.0
    } else if norm <= 5.0 {
        5.0
    } else {
        10.0
    };
    nice * mag
}

/// Number of tick intervals that reads well on an axis of `len_pt` points with 7 pt labels.
pub fn target_intervals(len_pt: f64) -> usize {
    ((len_pt / 24.0).round() as usize).clamp(2, 6)
}

/// Builds axis ticks covering `lo..=hi` on `scale`. Fixed limits are in data units.
pub fn scaled_ticks(lo: f64, hi: f64, target: usize, fixed_min: Option<f64>, fixed_max: Option<f64>, scale: AxisScale) -> AxisTicks {
    if scale.is_log() {
        log_ticks(lo, hi, target, fixed_min, fixed_max, scale)
    } else {
        axis_ticks(lo, hi, target, fixed_min, fixed_max)
    }
}

/// Ticks of a logarithmic axis: whole powers of the base, with minor ticks between them.
/// `lo`/`hi` and the fixed limits are data values; non-positive ones are ignored.
fn log_ticks(lo: f64, hi: f64, target: usize, fixed_min: Option<f64>, fixed_max: Option<f64>, scale: AxisScale) -> AxisTicks {
    let base = scale.base().unwrap_or(10.0);
    let positive = |v: f64| if v.is_finite() && v > 0.0 { Some(v) } else { None };
    let data_lo = fixed_min.and_then(positive).or_else(|| positive(lo.min(hi))).unwrap_or(1.0);
    let data_hi = fixed_max.and_then(positive).or_else(|| positive(hi.max(lo))).unwrap_or(data_lo * base);
    let (data_lo, data_hi) = if data_hi > data_lo { (data_lo, data_hi) } else { (data_lo / base, data_lo * base) };
    let (t_lo, t_hi) = (scale.forward(data_lo), scale.forward(data_hi));
    // Whole powers of the base, taking every `step`-th one when there would be too many.
    let span = t_hi - t_lo;
    let step = (span / target.max(1) as f64).ceil().max(1.0);
    let min = fixed_min.and_then(positive).map_or_else(|| (t_lo / step + 1e-9).floor() * step, |_| t_lo);
    let max = fixed_max.and_then(positive).map_or_else(|| (t_hi / step - 1e-9).ceil() * step, |_| t_hi);
    let first = (min / step - 1e-9).ceil() as i64;
    let last = (max / step + 1e-9).floor() as i64;
    let ticks: Vec<f64> = (first..=last).map(|k| scale.inverse(k as f64 * step)).collect();
    // Minor ticks: 2…9 inside each decade (only meaningful one power at a time).
    let mut minors = Vec::new();
    if step == 1.0 && base >= 2.0 {
        let n = base.round() as i64;
        for k in (min.floor() as i64 - 1)..=(max.ceil() as i64) {
            let decade = scale.inverse(k as f64);
            for m in 2..n.max(2) {
                let v = decade * m as f64;
                if scale.forward(v) > min + 1e-9 && scale.forward(v) < max - 1e-9 {
                    minors.push(v);
                }
            }
        }
    }
    AxisTicks { min, max, step, ticks, minors, decimals: 0, scale, numbers: AxisNumbers::Plain }
}

/// Builds axis ticks covering `lo..=hi`. Fixed limits override the automatic ones.
pub fn axis_ticks(lo: f64, hi: f64, target: usize, fixed_min: Option<f64>, fixed_max: Option<f64>) -> AxisTicks {
    let (mut lo, mut hi) = (lo.min(hi), lo.max(hi));
    if let Some(m) = fixed_min {
        lo = m;
    }
    if let Some(m) = fixed_max {
        hi = m;
    }
    if !lo.is_finite() || !hi.is_finite() {
        lo = 0.0;
        hi = 1.0;
    }
    if (hi - lo).abs() < 1e-12 {
        if lo == 0.0 {
            hi = 1.0;
        } else {
            let pad = lo.abs() * 0.5;
            if fixed_min.is_none() && lo > 0.0 {
                lo = 0.0;
            } else {
                lo -= pad;
            }
            hi += pad;
        }
    }
    let step = nice_step(hi - lo, target);
    let eps = step * 1e-9;
    let min = fixed_min.unwrap_or_else(|| (lo / step + 1e-9).floor() * step);
    let max = fixed_max.unwrap_or_else(|| (hi / step - 1e-9).ceil() * step);
    let first = (min / step - 1e-9).ceil() as i64;
    let last = (max / step + 1e-9).floor() as i64;
    let ticks: Vec<f64> = (first..=last)
        .map(|k| {
            let v = k as f64 * step;
            if v.abs() < eps {
                0.0
            } else {
                v
            }
        })
        .collect();
    let decimals = if step >= 1.0 { 0 } else { (-(step.log10() + 1e-9).floor()) as usize };
    AxisTicks { min, max, step, ticks, minors: Vec::new(), decimals, scale: AxisScale::Linear, numbers: AxisNumbers::Plain }
}

pub fn format_tick(v: f64, decimals: usize) -> String {
    let s = format!("{v:.decimals$}");
    if s.starts_with('-') && s[1..].chars().all(|c| c == '0' || c == '.') {
        s[1..].to_string()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nice_ranges() {
        let t = axis_ticks(0.0, 10.0, 3, None, None);
        assert_eq!(t.ticks, vec![0.0, 5.0, 10.0]);
        assert_eq!(t.labels(), vec!["0", "5", "10"]);
        let t = axis_ticks(0.0, 12.0, 4, None, None);
        assert_eq!((t.min, t.max), (0.0, 15.0));
        let t = axis_ticks(0.03, 0.42, 4, None, None);
        assert_eq!(t.labels(), vec!["0.0", "0.1", "0.2", "0.3", "0.4", "0.5"]);
        let t = axis_ticks(-3.2, 7.9, 4, None, None);
        assert_eq!(t.labels(), vec!["-5", "0", "5", "10"]);
        let t = axis_ticks(5.0, 5.0, 4, None, None);
        assert!(t.min <= 5.0 && t.max >= 5.0 && t.max > t.min);
    }

    #[test]
    fn log_axes() {
        let t = scaled_ticks(3.0, 4000.0, 4, None, None, AxisScale::Log10);
        assert_eq!(t.labels(), vec!["1", "10", "100", "1000", "10000"]);
        assert!((t.frac(100.0) - 0.5).abs() < 1e-9);
        assert!(t.frac(0.0).is_nan() && t.frac(-2.0).is_nan());
        assert!(t.minors.contains(&20.0));
        // Fixed limits are data values and are used exactly.
        let t = scaled_ticks(3.0, 4000.0, 4, Some(1.0), Some(1000.0), AxisScale::Log10);
        assert_eq!(t.data_range().0.round(), 1.0);
        assert!((t.data_range().1 - 1000.0).abs() < 1e-6);
        // Six powers of two over four intervals: every second power gets a label.
        let t = scaled_ticks(1.0, 64.0, 4, None, None, AxisScale::Log2);
        assert_eq!(t.labels(), vec!["1", "4", "16", "64"]);
        let t = scaled_ticks(0.25, 8.0, 6, None, None, AxisScale::Log2);
        assert_eq!(t.labels(), vec!["0.25", "0.5", "1", "2", "4", "8"]);
    }

    #[test]
    fn number_formats() {
        let t = axis_ticks(0.0, 40000.0, 4, None, None).numbered(AxisNumbers::Grouped);
        assert_eq!(t.labels(), vec!["0", "10 000", "20 000", "30 000", "40 000"]);
        // Four digits stay together, as they usually do in print.
        let t = axis_ticks(0.0, 2000.0, 2, None, None).numbered(AxisNumbers::Grouped);
        assert_eq!(t.labels(), vec!["0", "1000", "2000"]);
        let t = axis_ticks(0.0, 0.0003, 3, None, None).numbered(AxisNumbers::Scientific);
        assert_eq!(t.labels(), vec!["0", "1×10⁻⁴", "2×10⁻⁴", "3×10⁻⁴"]);
        let t = scaled_ticks(1.0, 1000.0, 4, None, None, AxisScale::Log10).numbered(AxisNumbers::Power10);
        assert_eq!(t.labels(), vec!["10⁰", "10¹", "10²", "10³"]);
        let t = scaled_ticks(1.0, 1000.0, 4, None, None, AxisScale::Log10).numbered(AxisNumbers::LogValue);
        assert_eq!(t.labels(), vec!["0", "1", "2", "3"]);
        // A half-decade value keeps its digits under "power of 10".
        assert_eq!(scientific_label(1500.0, true), "1.5×10³");
        assert_eq!(scientific_label(-0.25, false), "-2.5×10⁻¹");
    }

    #[test]
    fn fixed_limits() {
        let t = axis_ticks(0.0, 10.0, 4, Some(0.0), Some(20.0));
        assert_eq!((t.min, t.max), (0.0, 20.0));
        assert_eq!(t.ticks, vec![0.0, 5.0, 10.0, 15.0, 20.0]);
    }
}
