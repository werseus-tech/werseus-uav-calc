// SPDX-License-Identifier: AGPL-3.0-or-later
//! A-weighting (IEC 61672-1) and the 1/3-octave band grid used for the spectrum.

/// A-weighting correction in dB at frequency `f` (Hz), exact IEC 61672-1 analytic form:
///
/// `R_A(f) = 12194^2 f^4 / ((f^2 + 20.6^2) sqrt((f^2 + 107.7^2)(f^2 + 737.9^2)) (f^2 + 12194^2))`
/// and `A(f) = 20 log10 R_A(f) + 2.00` dB (the +2.00 dB normalises to 0 dB at 1 kHz).
///
/// Non-finite or non-positive frequencies return a large negative value (-200 dB) instead of NaN.
pub fn a_weight_db(f: f64) -> f64 {
    if !f.is_finite() || f <= 0.0 {
        return -200.0;
    }
    let f2 = f * f;
    let num = 12194.0_f64.powi(2) * f2 * f2;
    let den = (f2 + 20.6_f64.powi(2))
        * ((f2 + 107.7_f64.powi(2)) * (f2 + 737.9_f64.powi(2))).sqrt()
        * (f2 + 12194.0_f64.powi(2));
    let r = num / den;
    if r <= 0.0 {
        return -200.0;
    }
    (20.0 * r.log10() + 2.00).max(-200.0)
}

/// Nominal 1/3-octave centre frequencies from 20 Hz to 20 kHz (base-ten series, `10^(k/10)`).
pub fn third_octave_centres() -> Vec<f64> {
    // k = 13 -> 19.95 Hz ... k = 43 -> 19953 Hz
    (13..=43).map(|k| 10f64.powf(k as f64 / 10.0)).collect()
}

/// Index of the 1/3-octave band that contains `f` (None outside 20 Hz - 20 kHz bands).
pub fn band_index(f: f64) -> Option<usize> {
    if !f.is_finite() || f <= 0.0 {
        return None;
    }
    let k = (10.0 * f.log10()).round();
    if (13.0..=43.0).contains(&k) {
        Some((k as i64 - 13) as usize)
    } else {
        None
    }
}

/// Energy sum of levels in dB. Empty input gives a very low floor (-200 dB).
pub fn db_sum<I: IntoIterator<Item = f64>>(levels: I) -> f64 {
    let e: f64 = levels
        .into_iter()
        .filter(|l| l.is_finite())
        .map(|l| 10f64.powf(l / 10.0))
        .sum();
    if e > 0.0 { 10.0 * e.log10() } else { -200.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_weight_reference_points() {
        assert!(a_weight_db(1000.0).abs() < 0.01);
        assert!((a_weight_db(100.0) + 19.1).abs() < 0.1);
        assert!((a_weight_db(10_000.0) + 2.5).abs() < 0.1);
        assert!((a_weight_db(63.0) + 26.2).abs() < 0.1);
        assert!((a_weight_db(2000.0) - 1.2).abs() < 0.1);
    }

    #[test]
    fn a_weight_bad_input_is_finite() {
        for f in [0.0, -5.0, f64::NAN, f64::INFINITY, 1e30] {
            assert!(a_weight_db(f).is_finite(), "{f}");
        }
    }

    #[test]
    fn bands_cover_audible_range() {
        let c = third_octave_centres();
        assert_eq!(c.len(), 31);
        assert!((c[0] - 20.0).abs() < 0.5);
        assert!((c[c.len() - 1] - 20_000.0).abs() < 100.0);
        assert_eq!(band_index(1000.0), Some(17));
        assert_eq!(band_index(5.0), None);
        assert_eq!(band_index(40_000.0), None);
    }

    #[test]
    fn db_sum_of_two_equal_levels_adds_3db() {
        assert!((db_sum([60.0, 60.0]) - 63.0103).abs() < 1e-3);
        assert!(db_sum(std::iter::empty()) < -100.0);
    }
}
