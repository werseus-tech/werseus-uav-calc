// SPDX-License-Identifier: AGPL-3.0-or-later
//! Itki tezgahi olcumlerinin uydurmasi: T = a*RPM^2 (Ct tahmini), elektrik gucu P = V*I,
//! verim (g/W), gaz -> itki polinomu ve tasarim tahminiyle karsilastirma. Saf fonksiyonlar.

use super::csv::Sample;

pub const G0: f64 = 9.80665;

/// Polinom (artan derece katsayilari); x, olcek `xs`'e bolunerek uydurulur (iyi kosullu).
#[derive(Clone, Debug, PartialEq)]
pub struct Poly {
    pub coeffs: Vec<f64>,
    pub xs: f64,
}

impl Poly {
    pub fn eval(&self, x: f64) -> f64 {
        let u = x / self.xs;
        self.coeffs.iter().rev().fold(0.0, |acc, &c| acc * u + c)
    }
}

/// En kucuk kareler polinom uydurmasi (normal denklemler, kismi pivotlu Gauss).
/// Veri azsa derece dusurulur; yeterli sonlu nokta yoksa `None`.
pub fn polyfit(xs: &[f64], ys: &[f64], deg: usize) -> Option<Poly> {
    let pts: Vec<(f64, f64)> = xs
        .iter()
        .zip(ys)
        .filter(|(x, y)| x.is_finite() && y.is_finite())
        .map(|(&x, &y)| (x, y))
        .collect();
    if pts.len() < 2 {
        return None;
    }
    let deg = deg.min(pts.len() - 1);
    let scale = pts.iter().fold(0.0_f64, |m, (x, _)| m.max(x.abs()));
    if scale < 1e-12 {
        return None;
    }
    let n = deg + 1;
    let mut a = vec![vec![0.0; n + 1]; n];
    for &(x, y) in &pts {
        let u = x / scale;
        let mut pw = vec![1.0; 2 * n];
        for k in 1..2 * n {
            pw[k] = pw[k - 1] * u;
        }
        for (i, row) in a.iter_mut().enumerate() {
            for j in 0..n {
                row[j] += pw[i + j];
            }
            row[n] += y * pw[i];
        }
    }
    // Gauss eleme
    for col in 0..n {
        let piv = (col..n).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))?;
        if a[piv][col].abs() < 1e-12 {
            return None;
        }
        a.swap(col, piv);
        let pivot_row = a[col].clone();
        for row in a.iter_mut().skip(col + 1) {
            let f = row[col] / pivot_row[col];
            for (x, pv) in row.iter_mut().zip(&pivot_row).skip(col) {
                *x -= f * pv;
            }
        }
    }
    let mut coeffs = vec![0.0; n];
    for i in (0..n).rev() {
        let s: f64 = (i + 1..n).map(|j| a[i][j] * coeffs[j]).sum();
        coeffs[i] = (a[i][n] - s) / a[i][i];
    }
    coeffs
        .iter()
        .all(|c| c.is_finite())
        .then_some(Poly { coeffs, xs: scale })
}

/// T = a*RPM^2 uydurmasi ve turetilen Ct.
#[derive(Clone, Copy, Debug)]
pub struct CtFit {
    /// T [N] = a * RPM^2
    pub a: f64,
    /// T = Ct * rho * n^2 * D^4 (n = dev/s)
    pub ct: f64,
    pub r2: f64,
    pub n: usize,
}

/// Orijinden gecen en kucuk kareler: a = sum(T*r^2) / sum(r^4). `d` pervane capi [m], `rho` [kg/m3].
pub fn fit_ct(samples: &[Sample], d: f64, rho: f64) -> Option<CtFit> {
    let pts: Vec<(f64, f64)> = samples
        .iter()
        .filter_map(|s| s.rpm.map(|r| (r, s.thrust_n)))
        .filter(|(r, t)| *r > 1.0 && t.is_finite())
        .collect();
    if pts.len() < 2 {
        return None;
    }
    let num: f64 = pts.iter().map(|(r, t)| t * r * r).sum();
    let den: f64 = pts.iter().map(|(r, _)| r.powi(4)).sum();
    if den <= 0.0 {
        return None;
    }
    let a = num / den;
    let mean = pts.iter().map(|p| p.1).sum::<f64>() / pts.len() as f64;
    let ss_tot: f64 = pts.iter().map(|p| (p.1 - mean).powi(2)).sum();
    let ss_res: f64 = pts.iter().map(|(r, t)| (t - a * r * r).powi(2)).sum();
    let r2 = if ss_tot > 1e-18 {
        1.0 - ss_res / ss_tot
    } else {
        0.0
    };
    if !(d > 0.0 && rho > 0.0) {
        return None;
    }
    // T = a r^2 = Ct rho (r/60)^2 D^4
    let ct = 3600.0 * a / (rho * d.powi(4));
    (a.is_finite() && ct.is_finite()).then_some(CtFit {
        a,
        ct,
        r2,
        n: pts.len(),
    })
}

/// Verim noktasi.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffPoint {
    pub thrust_g: f64,
    pub power_w: f64,
    pub g_per_w: f64,
}

/// g/W: itki (gram-kuvvet) / elektrik gucu. Gucu veya itkisi ~0 olan satirlar atlanir.
pub fn efficiency_points(samples: &[Sample]) -> Vec<EffPoint> {
    samples
        .iter()
        .filter_map(|s| {
            let p = s.power_w()?;
            let g = s.thrust_n / G0 * 1000.0;
            (p > 0.5 && g > 1.0).then(|| EffPoint {
                thrust_g: g,
                power_w: p,
                g_per_w: g / p,
            })
        })
        .collect()
}

/// Gaz (0..1) -> itki [N] polinomu.
pub fn throttle_curve(samples: &[Sample], deg: usize) -> Option<Poly> {
    let (x, y): (Vec<f64>, Vec<f64>) = samples
        .iter()
        .filter_map(|s| s.throttle.map(|t| (t, s.thrust_n)))
        .unzip();
    polyfit(&x, &y, deg)
}

/// Olculen elektrik gucunun belirli bir itkideki degeri.
#[derive(Clone, Copy, Debug)]
pub struct PowerAt {
    pub power_w: f64,
    /// Istenen itki olcum araliginin disinda (tahmin).
    pub extrapolated: bool,
}

/// P(T) polinomuyla (derece 2) belirli itki [N] icin guc tahmini.
pub fn power_at_thrust(samples: &[Sample], t_n: f64) -> Option<PowerAt> {
    let pts: Vec<(f64, f64)> = samples
        .iter()
        .filter_map(|s| s.power_w().map(|p| (s.thrust_n, p)))
        .filter(|(t, p)| *t > 0.0 && *p > 0.0)
        .collect();
    if pts.len() < 2 || !t_n.is_finite() || t_n <= 0.0 {
        return None;
    }
    let (x, y): (Vec<f64>, Vec<f64>) = pts.iter().cloned().unzip();
    let poly = polyfit(&x, &y, 2)?;
    let lo = x.iter().cloned().fold(f64::INFINITY, f64::min);
    let hi = x.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let p = poly.eval(t_n);
    (p.is_finite() && p > 0.0).then_some(PowerAt {
        power_w: p,
        extrapolated: t_n < lo * 0.98 || t_n > hi * 1.02,
    })
}

/// Karsilastirma yorumu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Close,
    MeasuredHigher,
    MeasuredLower,
}

/// Olculen ile tasarimin yuzde farki: (olculen - tasarim) / tasarim * 100.
pub fn diff_pct(measured: f64, design: f64) -> Option<f64> {
    (measured.is_finite() && design.is_finite() && design.abs() > 1e-12)
        .then(|| (measured - design) / design * 100.0)
}

/// Yuzde farktan yorum (+-10 % icinde "yakin").
pub fn verdict(pct: f64) -> Verdict {
    if pct.abs() <= 10.0 {
        Verdict::Close
    } else if pct > 0.0 {
        Verdict::MeasuredHigher
    } else {
        Verdict::MeasuredLower
    }
}

/// Olculen verimle askida kalma tahmini (yalnizca oturum, proje degismez).
#[derive(Clone, Copy, Debug)]
pub struct HoverWhat {
    pub p_elec_total_w: f64,
    pub endurance_min: f64,
}

/// `ratio` = olculen guc / tasarim gucu (motor basina). Toplam elektrik gucu ve sure yeniden hesaplanir.
pub fn hover_with_ratio(
    p_elec_total_design_w: f64,
    p_other_w: f64,
    e_usable_wh: f64,
    ratio: f64,
) -> Option<HoverWhat> {
    if !(ratio.is_finite() && ratio > 0.0 && p_elec_total_design_w > 0.0 && e_usable_wh >= 0.0) {
        return None;
    }
    let p = p_elec_total_design_w * ratio;
    let tot = p + p_other_w.max(0.0);
    (tot > 0.0).then(|| HoverWhat {
        p_elec_total_w: p,
        endurance_min: e_usable_wh / tot * 60.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synth(a: f64, n: usize) -> Vec<Sample> {
        (1..=n)
            .map(|i| {
                let rpm = 2000.0 + 1000.0 * i as f64;
                let t = a * rpm * rpm;
                let thr = 0.1 + 0.8 * i as f64 / n as f64;
                Sample {
                    thrust_n: t,
                    rpm: Some(rpm),
                    volt: Some(16.0),
                    curr: Some(2.0 + 20.0 * thr * thr),
                    throttle: Some(thr),
                }
            })
            .collect()
    }

    #[test]
    fn ct_recovers_known_coefficient() {
        let (d, rho, ct_true) = (0.127_f64, 1.225_f64, 0.11_f64);
        let a = ct_true * rho * d.powi(4) / 3600.0;
        let s = synth(a, 8);
        let f = fit_ct(&s, d, rho).unwrap();
        assert!((f.ct - ct_true).abs() < 1e-9, "{}", f.ct);
        assert!(f.r2 > 0.999999);
        assert_eq!(f.n, 8);
    }

    #[test]
    fn ct_fit_with_noise_is_close() {
        let (d, rho) = (0.2_f64, 1.2_f64);
        let a = 0.09 * rho * d.powi(4) / 3600.0;
        let mut s = synth(a, 12);
        for (i, x) in s.iter_mut().enumerate() {
            x.thrust_n *= 1.0 + if i % 2 == 0 { 0.02 } else { -0.02 };
        }
        let f = fit_ct(&s, d, rho).unwrap();
        assert!((f.ct - 0.09).abs() < 0.005);
    }

    #[test]
    fn ct_needs_data() {
        assert!(fit_ct(&[], 0.1, 1.2).is_none());
        assert!(fit_ct(&synth(1e-8, 5), 0.0, 1.2).is_none());
    }

    #[test]
    fn g_per_w_math() {
        let s = [Sample {
            thrust_n: 0.5 * G0, // 500 g
            rpm: None,
            volt: Some(12.0),
            curr: Some(5.0), // 60 W
            throttle: None,
        }];
        let e = efficiency_points(&s);
        assert_eq!(e.len(), 1);
        assert!((e[0].power_w - 60.0).abs() < 1e-9);
        assert!((e[0].thrust_g - 500.0).abs() < 1e-9);
        assert!((e[0].g_per_w - 500.0 / 60.0).abs() < 1e-9);
    }

    #[test]
    fn efficiency_skips_idle_rows() {
        let s = [Sample {
            thrust_n: 0.0,
            rpm: None,
            volt: Some(12.0),
            curr: Some(0.1),
            throttle: None,
        }];
        assert!(efficiency_points(&s).is_empty());
    }

    #[test]
    fn polyfit_recovers_quadratic() {
        let x: Vec<f64> = (0..10).map(|i| i as f64 * 0.1).collect();
        let y: Vec<f64> = x.iter().map(|&x| 1.0 + 2.0 * x + 3.0 * x * x).collect();
        let p = polyfit(&x, &y, 2).unwrap();
        for &xi in &[0.0, 0.33, 0.9, 1.2] {
            assert!((p.eval(xi) - (1.0 + 2.0 * xi + 3.0 * xi * xi)).abs() < 1e-8);
        }
    }

    #[test]
    fn polyfit_edge_cases() {
        assert!(polyfit(&[], &[], 2).is_none());
        assert!(polyfit(&[1.0], &[1.0], 2).is_none());
        assert!(polyfit(&[0.0, 0.0], &[1.0, 2.0], 1).is_none());
        // Az veri: derece dusurulur (2 nokta -> dogru).
        let p = polyfit(&[1.0, 2.0], &[2.0, 4.0], 3).unwrap();
        assert!((p.eval(3.0) - 6.0).abs() < 1e-9);
        // Ayni x tekrar eden veride panik yok.
        let _ = polyfit(&[1.0, 1.0, 1.0], &[1.0, 2.0, 3.0], 2);
        assert!(polyfit(&[f64::NAN, 1.0], &[1.0, 2.0], 1).is_none());
    }

    #[test]
    fn throttle_curve_fits_samples() {
        let a = 1e-8;
        let s = synth(a, 10);
        let p = throttle_curve(&s, 2).unwrap();
        // Kendi noktalarinda yaklasik dogru
        for x in &s {
            let v = p.eval(x.throttle.unwrap());
            assert!(
                (v - x.thrust_n).abs() < 0.05 * x.thrust_n.max(0.01),
                "{v} {}",
                x.thrust_n
            );
        }
    }

    #[test]
    fn power_at_thrust_interpolates() {
        // P = 10 + 4 T^2 (T in N)
        let s: Vec<Sample> = (1..=8)
            .map(|i| {
                let t = i as f64;
                Sample {
                    thrust_n: t,
                    rpm: None,
                    volt: Some(10.0),
                    curr: Some((10.0 + 4.0 * t * t) / 10.0),
                    throttle: None,
                }
            })
            .collect();
        let r = power_at_thrust(&s, 4.5).unwrap();
        assert!((r.power_w - (10.0 + 4.0 * 4.5 * 4.5)).abs() < 1e-6);
        assert!(!r.extrapolated);
        assert!(power_at_thrust(&s, 20.0).unwrap().extrapolated);
        assert!(power_at_thrust(&s, 0.0).is_none());
        assert!(power_at_thrust(&[], 1.0).is_none());
    }

    #[test]
    fn comparison_percent_math() {
        assert!((diff_pct(110.0, 100.0).unwrap() - 10.0).abs() < 1e-12);
        assert!((diff_pct(80.0, 100.0).unwrap() + 20.0).abs() < 1e-12);
        assert!(diff_pct(1.0, 0.0).is_none());
        assert!(diff_pct(f64::NAN, 1.0).is_none());
        assert_eq!(verdict(3.0), Verdict::Close);
        assert_eq!(verdict(-10.0), Verdict::Close);
        assert_eq!(verdict(25.0), Verdict::MeasuredHigher);
        assert_eq!(verdict(-25.0), Verdict::MeasuredLower);
    }

    #[test]
    fn hover_with_measured_efficiency() {
        // 100 W tasarim, 10 W avionik, 55 Wh: 55/110 h = 30 dk
        let base = hover_with_ratio(100.0, 10.0, 55.0, 1.0).unwrap();
        assert!((base.endurance_min - 30.0).abs() < 1e-9);
        let worse = hover_with_ratio(100.0, 10.0, 55.0, 1.2).unwrap();
        assert!((worse.p_elec_total_w - 120.0).abs() < 1e-9);
        assert!(worse.endurance_min < base.endurance_min);
        assert!(hover_with_ratio(0.0, 10.0, 55.0, 1.0).is_none());
        assert!(hover_with_ratio(100.0, 10.0, 55.0, f64::NAN).is_none());
    }
}
