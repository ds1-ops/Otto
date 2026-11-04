//! Kaplan-Meier product-limit estimate, for drawing survival curves. The numbers Otto
//! reports (survival proportions with SE and CI, medians, tests) come from R's `survival`
//! package; a test checks that these steps are the same as R's `survfit`.

/// The curve at one time with events or censored subjects.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Step {
    pub time: f64,
    /// Subjects at risk just before `time`.
    pub at_risk: usize,
    pub events: usize,
    pub censored: usize,
    /// Fraction surviving just after `time` (0–1).
    pub survival: f64,
}

/// Product-limit estimate from (time, event) pairs; event = true for an event (death),
/// false for a censored subject. Steps are in time order.
pub fn kaplan_meier(subjects: &[(f64, bool)]) -> Vec<Step> {
    let mut times: Vec<f64> = subjects.iter().map(|s| s.0).filter(|t| t.is_finite()).collect();
    times.sort_by(f64::total_cmp);
    times.dedup();
    let mut s = 1.0;
    times
        .into_iter()
        .map(|t| {
            let at_risk = subjects.iter().filter(|x| x.0 >= t).count();
            let events = subjects.iter().filter(|x| x.0 == t && x.1).count();
            let censored = subjects.iter().filter(|x| x.0 == t && !x.1).count();
            if events > 0 && at_risk > 0 {
                s *= 1.0 - events as f64 / at_risk as f64;
            }
            Step { time: t, at_risk, events, censored, survival: s }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_limit() {
        // Times 1, 2+, 3, 3, 4+: S = 4/5 after 1; at 3, 2 of 3 at risk die: 4/5 · 1/3.
        let k = kaplan_meier(&[(1.0, true), (2.0, false), (3.0, true), (3.0, true), (4.0, false)]);
        assert_eq!(k.len(), 4);
        assert!((k[0].survival - 0.8).abs() < 1e-12);
        assert_eq!((k[1].censored, k[1].survival), (1, 0.8));
        assert_eq!(k[2].at_risk, 3);
        assert!((k[2].survival - 0.8 / 3.0).abs() < 1e-12);
    }
}
