//! Descriptive statistics for drawing bars, symbols and error bars (mean, SD, SEM, 95% CI,
//! median, quartiles). Hypothesis tests are not done here: R computes every test Otto reports.

use serde::Serialize;

use statrs::distribution::{ContinuousCDF, StudentsT};

/// Descriptive statistics of one sample. Values that need n ≥ 2 are NaN for n = 1.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Descriptive {
    pub n: usize,
    pub mean: f64,
    pub sd: f64,
    pub sem: f64,
    pub median: f64,
    /// 25th percentile (same method as Prism: (n + 1)·p).
    pub q1: f64,
    /// 75th percentile.
    pub q3: f64,
    pub min: f64,
    pub max: f64,
    pub ci95_low: f64,
    pub ci95_high: f64,
}

/// Computes descriptive statistics; `None` for an empty sample. Non-finite values are ignored.
pub fn describe(values: &[f64]) -> Option<Descriptive> {
    let mut v: Vec<f64> = values.iter().copied().filter(|x| x.is_finite()).collect();
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    let n = v.len();
    let nf = n as f64;
    let mean = v.iter().sum::<f64>() / nf;
    let (sd, sem, ci) = if n >= 2 {
        let var = v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (nf - 1.0);
        let sd = var.sqrt();
        let sem = sd / nf.sqrt();
        (sd, sem, t_critical_95(n - 1) * sem)
    } else {
        (f64::NAN, f64::NAN, f64::NAN)
    };
    Some(Descriptive {
        n,
        mean,
        sd,
        sem,
        median: percentile_sorted(&v, 0.5),
        q1: percentile_sorted(&v, 0.25),
        q3: percentile_sorted(&v, 0.75),
        min: v[0],
        max: v[n - 1],
        ci95_low: mean - ci,
        ci95_high: mean + ci,
    })
}

/// Two-sided 95% critical value of Student's t with `df` degrees of freedom.
pub fn t_critical_95(df: usize) -> f64 {
    StudentsT::new(0.0, 1.0, df as f64).map(|t| t.inverse_cdf(0.975)).unwrap_or(f64::NAN)
}

/// Percentile of sorted data using the (n + 1)·p rule (Hyndman–Fan type 6), as Prism does.
pub fn percentile_sorted(sorted: &[f64], p: f64) -> f64 {
    let n = sorted.len();
    if n == 0 {
        return f64::NAN;
    }
    let h = (n as f64 + 1.0) * p;
    if h <= 1.0 {
        return sorted[0];
    }
    if h >= n as f64 {
        return sorted[n - 1];
    }
    let lo = h.floor() as usize;
    let frac = h - h.floor();
    sorted[lo - 1] + frac * (sorted[lo] - sorted[lo - 1])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn basic_descriptives() {
        let d = describe(&[2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]).unwrap();
        assert_eq!(d.n, 8);
        assert!(close(d.mean, 5.0));
        assert!(close(d.sd, (32.0f64 / 7.0).sqrt()));
        assert!(close(d.sem, d.sd / 8f64.sqrt()));
        assert!(close(d.median, 4.5));
        assert!(close(d.q1, 4.0));
        assert!(close(d.q3, 6.5));
        assert!(close(d.min, 2.0) && close(d.max, 9.0));
        // t(0.975, 7) = 2.364624...
        assert!((d.ci95_high - (5.0 + 2.364624251 * d.sem)).abs() < 1e-6);
    }

    #[test]
    fn single_value_and_empty() {
        let d = describe(&[3.0]).unwrap();
        assert!(d.sd.is_nan() && close(d.mean, 3.0) && close(d.median, 3.0));
        assert!(describe(&[]).is_none());
        assert!(describe(&[f64::NAN]).is_none());
    }
}
