// SPDX-License-Identifier: AGPL-3.0-or-later
//! Titreşim sönümleme: uyarım frekansları (pervane devri), doğal frekans, geçirgenlik,
//! izolasyon yüzdesi, statik çökme ve sertlik önerisi. Tek serbestlik dereceli kütle-yay-sönüm.
use crate::calc::Model;
use crate::project::{DroneType, Project};

pub const G: f64 = 9.80665;

/// Tipik sönümleyici örnekleri: (ad, tek takoz sertliği N/mm, yaklaşık Shore A). Yalnızca tipik
/// örnek değerlerdir; gerçek takozun veri sayfasındaki değer kullanılmalıdır.
pub const DAMPER_PRESETS: &[(&str, f64, u32)] = &[
    ("Soft silicone ball", 0.25, 30),
    ("Medium silicone ball", 0.6, 40),
    ("Firm silicone ball", 1.2, 50),
    ("Rubber grommet", 3.0, 60),
];

/// f_n = (1/2π)·√(k_toplam / m). `k_total_n_mm` N/mm, `m_kg` kg, sonuç Hz.
pub fn natural_freq_hz(k_total_n_mm: f64, m_kg: f64) -> f64 {
    if !(k_total_n_mm > 0.0 && m_kg > 0.0) {
        return 0.0;
    }
    (k_total_n_mm * 1000.0 / m_kg).sqrt() / std::f64::consts::TAU
}

/// Taban uyarımı geçirgenliği: T = √((1+(2ζr)²) / ((1−r²)² + (2ζr)²)), r = f / f_n.
pub fn transmissibility(r: f64, zeta: f64) -> f64 {
    if !(r.is_finite() && zeta.is_finite()) {
        return 0.0;
    }
    let a = (2.0 * zeta * r).powi(2);
    let den = (1.0 - r * r).powi(2) + a;
    if den <= 1e-12 {
        return f64::INFINITY.min(1e6);
    }
    ((1.0 + a) / den).sqrt()
}

/// İzolasyon yüzdesi = (1 − T)·100 (T > 1 ise negatif: titreşim büyür).
pub fn isolation_pct(t: f64) -> f64 {
    (1.0 - t) * 100.0
}

/// Statik çökme [mm] = m·g / k_toplam.
pub fn static_deflection_mm(m_kg: f64, k_total_n_mm: f64) -> f64 {
    if k_total_n_mm > 0.0 {
        m_kg * G / k_total_n_mm
    } else {
        0.0
    }
}

/// T(r) = `t_target` olan (r > √2) frekans oranı; ikili aramayla. T bu bölgede azalandır.
pub fn r_for_transmissibility(t_target: f64, zeta: f64) -> f64 {
    let (mut lo, mut hi) = (std::f64::consts::SQRT_2, 1.0e3);
    if transmissibility(lo, zeta) <= t_target {
        return lo;
    }
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if transmissibility(mid, zeta) > t_target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    hi
}

/// Yaklaşık takoz sertliği (Shore A) ipucu: tek takoz sertliğine göre kaba bant.
pub fn shore_hint(k_per_damper_n_mm: f64) -> u32 {
    match k_per_damper_n_mm {
        k if k <= 0.4 => 30,
        k if k <= 0.9 => 40,
        k if k <= 2.0 => 50,
        _ => 60,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Rotor1x,
    BladePass,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Excitation {
    /// 0: askı, 1: seyir.
    pub phase: u8,
    pub source: Source,
    pub freq_hz: f64,
}

/// Askı ve seyir devri [rpm] (yoksa None) ile kanat sayısı.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RpmInfo {
    pub hover_rpm: Option<f64>,
    pub cruise_rpm: Option<f64>,
    pub hover_blades: u32,
    pub cruise_blades: u32,
}

fn pos(x: f64) -> Option<f64> {
    (x.is_finite() && x > 0.0).then_some(x)
}

/// Modelden devir tahmini. Multirotor: askı devri çok-rotor modelinden, seyir ≈ askı·1,05
/// (kaba); sabit kanat: seyir devri `rpm_cr` (2 kanatlı varsayım); VTOL: ikisi de.
pub fn rpm_sources(p: &Project, m: &Model) -> RpmInfo {
    let mr_hover = pos(m.mr_st.f_rot_hover * 60.0);
    let blades_mr = p.mr_prop_blades.max(1);
    match p.drone_type {
        DroneType::Multirotor => RpmInfo {
            hover_rpm: mr_hover,
            cruise_rpm: mr_hover.map(|r| r * 1.05),
            hover_blades: blades_mr,
            cruise_blades: blades_mr,
        },
        DroneType::VtolQuadplane => RpmInfo {
            hover_rpm: mr_hover,
            cruise_rpm: pos(m.prop.rpm_cr),
            hover_blades: blades_mr,
            cruise_blades: 2,
        },
        _ => RpmInfo {
            hover_rpm: None,
            cruise_rpm: pos(m.prop.rpm_cr),
            hover_blades: 2,
            cruise_blades: 2,
        },
    }
}

/// Bir devirden uyarımlar: 1× rotor ve kanat geçiş (N_b×) frekansı.
pub fn excitations(phase: u8, rpm: f64, blades: u32) -> Vec<Excitation> {
    if !(rpm.is_finite() && rpm > 0.0) {
        return Vec::new();
    }
    let f1 = rpm / 60.0;
    vec![
        Excitation {
            phase,
            source: Source::Rotor1x,
            freq_hz: f1,
        },
        Excitation {
            phase,
            source: Source::BladePass,
            freq_hz: f1 * blades.max(1) as f64,
        },
    ]
}

#[derive(Clone, Copy, Debug)]
pub struct DamperInput {
    pub mass_kg: f64,
    pub n_dampers: u32,
    pub k_per_n_mm: f64,
    pub zeta: f64,
    /// İzin verilen en büyük statik çökme [mm].
    pub max_sag_mm: f64,
}

#[derive(Clone, Debug)]
pub struct ExcitationResult {
    pub exc: Excitation,
    pub r: f64,
    pub t: f64,
    pub isolation_pct: f64,
    /// 0,7 ≤ r ≤ 1,4: rezonansa yakın.
    pub near_resonance: bool,
    /// r < √2: yalıtım yok, büyütme var.
    pub amplified: bool,
}

#[derive(Clone, Debug)]
pub struct Recommendation {
    /// Toplam sertlik üst sınırı [N/mm]: en düşük uyarımda ≥ %80 izolasyon.
    pub k_max_total_80: f64,
    /// En düşük uyarımda r = 3 için toplam sertlik [N/mm].
    pub k_target_r3: f64,
    /// Statik çökme sınırından alt sınır [N/mm].
    pub k_min_sag: f64,
    pub per_damper_max_80: f64,
    pub per_damper_min_sag: f64,
    pub per_damper_target: f64,
    /// Alt sınır ≤ üst sınır mı.
    pub feasible: bool,
}

#[derive(Clone, Debug)]
pub struct DampingReport {
    pub k_total: f64,
    pub f_n: f64,
    pub static_defl_mm: f64,
    pub too_soft: bool,
    pub results: Vec<ExcitationResult>,
    pub recommendation: Option<Recommendation>,
}

pub fn analyze(inp: &DamperInput, exc: &[Excitation]) -> DampingReport {
    let k_total = inp.k_per_n_mm * inp.n_dampers as f64;
    let f_n = natural_freq_hz(k_total, inp.mass_kg);
    let defl = static_deflection_mm(inp.mass_kg, k_total);
    let results: Vec<ExcitationResult> = exc
        .iter()
        .map(|e| {
            let r = if f_n > 0.0 { e.freq_hz / f_n } else { 0.0 };
            let t = transmissibility(r, inp.zeta);
            ExcitationResult {
                exc: *e,
                r,
                t,
                isolation_pct: isolation_pct(t),
                near_resonance: (0.7..=1.4).contains(&r),
                amplified: r < std::f64::consts::SQRT_2,
            }
        })
        .collect();
    let f_min = exc.iter().map(|e| e.freq_hz).fold(f64::INFINITY, f64::min);
    let recommendation = (f_min.is_finite() && f_min > 0.0 && inp.mass_kg > 0.0).then(|| {
        let k_from_r = |r: f64| inp.mass_kg * (std::f64::consts::TAU * f_min / r).powi(2) / 1000.0;
        let r80 = r_for_transmissibility(0.2, inp.zeta);
        let k_max = k_from_r(r80);
        let k_min = inp.mass_kg * G / inp.max_sag_mm.max(0.1);
        let feasible = k_min <= k_max;
        let k_raw = k_from_r(3.0_f64.max(r80));
        let k_target = if feasible {
            k_raw.clamp(k_min, k_max)
        } else {
            k_raw
        };
        let n = inp.n_dampers.max(1) as f64;
        Recommendation {
            k_max_total_80: k_max,
            k_target_r3: k_target,
            k_min_sag: k_min,
            per_damper_max_80: k_max / n,
            per_damper_min_sag: k_min / n,
            per_damper_target: k_target / n,
            feasible,
        }
    });
    DampingReport {
        k_total,
        f_n,
        static_defl_mm: defl,
        too_soft: defl > inp.max_sag_mm,
        results,
        recommendation,
    }
}

/// T–f eğrisi (log aralıklı), [f_hz, T].
pub fn curve(f_n: f64, zeta: f64, f_max: f64, n: usize) -> Vec<[f64; 2]> {
    if !(f_n > 0.0 && f_max > 0.0 && n >= 2) {
        return Vec::new();
    }
    let f_lo = (f_n * 0.1).max(0.5).min(f_max / 2.0);
    (0..n)
        .map(|i| {
            let f = f_lo * (f_max / f_lo).powf(i as f64 / (n - 1) as f64);
            [f, transmissibility(f / f_n, zeta)]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Library, evaluate};

    #[test]
    fn natural_frequency_formula() {
        // k = 2 N/mm = 2000 N/m, m = 0.5 kg => sqrt(4000)/2π = 10.07 Hz.
        let f = natural_freq_hz(2.0, 0.5);
        assert!((f - 10.066).abs() < 0.01, "{f}");
        assert_eq!(natural_freq_hz(0.0, 0.5), 0.0);
    }

    #[test]
    fn transmissibility_key_points() {
        let z = 0.05;
        let t1 = transmissibility(1.0, z);
        assert!(
            (t1 - 1.0 / (2.0 * z)).abs() / (1.0 / (2.0 * z)) < 0.01,
            "{t1}"
        );
        // r = √2: T = 1 (sönümden bağımsız).
        for z in [0.0, 0.1, 0.3] {
            let t = transmissibility(std::f64::consts::SQRT_2, z);
            assert!((t - 1.0).abs() < 1e-9, "{z}: {t}");
        }
        // r = 0: T = 1.
        assert!((transmissibility(0.0, 0.1) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn isolation_at_r3() {
        // ζ = 0: T = 1/(r²−1) = 0.125 => %87,5.
        let t = transmissibility(3.0, 0.0);
        assert!((t - 0.125).abs() < 1e-12);
        assert!((isolation_pct(t) - 87.5).abs() < 1e-9);
        // ζ = 0,1 için biraz daha düşük izolasyon.
        let iso = isolation_pct(transmissibility(3.0, 0.1));
        assert!(iso > 80.0 && iso < 87.5, "{iso}");
    }

    #[test]
    fn static_deflection() {
        // 0,3 kg, k_toplam 2 N/mm => 0,3·9,80665/2 = 1,47 mm.
        let d = static_deflection_mm(0.3, 2.0);
        assert!((d - 1.471).abs() < 0.001, "{d}");
    }

    #[test]
    fn required_r_for_80_percent() {
        let r = r_for_transmissibility(0.2, 0.0);
        assert!((r - 6.0_f64.sqrt()).abs() < 1e-6, "{r}");
        assert!(r_for_transmissibility(0.2, 0.1) > r);
    }

    #[test]
    fn analysis_flags_resonance_and_recommends() {
        let exc = excitations(0, 6000.0, 2); // 100 Hz, 200 Hz
        assert_eq!(exc.len(), 2);
        assert!((exc[1].freq_hz - 200.0).abs() < 1e-9);
        let soft = DamperInput {
            mass_kg: 0.3,
            n_dampers: 4,
            k_per_n_mm: 0.5,
            zeta: 0.1,
            max_sag_mm: 5.0,
        };
        let rep = analyze(&soft, &exc);
        assert!(
            rep.results
                .iter()
                .all(|r| r.isolation_pct > 90.0 && !r.near_resonance)
        );
        assert!(!rep.too_soft);
        let rec = rep.recommendation.unwrap();
        assert!(
            rec.feasible
                && rec.k_min_sag <= rec.k_target_r3
                && rec.k_target_r3 <= rec.k_max_total_80
        );
        // Yüksek sönüm (ζ = 0.5): r80 > 3 iken hedef sertlik üst sınırı aşmamalı.
        let high_zeta = DamperInput { zeta: 0.5, ..soft };
        let rec_hz = analyze(&high_zeta, &exc).recommendation.unwrap();
        assert!(rec_hz.feasible);
        assert!(
            rec_hz.k_min_sag <= rec_hz.k_target_r3 && rec_hz.k_target_r3 <= rec_hz.k_max_total_80,
            "{rec_hz:?}"
        );
        // Düşük uyarım frekansı: r = 3 sertliği k_min_sag altında kalırsa alt sınıra kenetlenmeli.
        let low_exc = excitations(0, 2400.0, 2); // 40 Hz
        let low_f = DamperInput {
            mass_kg: 1.0,
            max_sag_mm: 2.0,
            ..soft
        };
        let rec_lf = analyze(&low_f, &low_exc).recommendation.unwrap();
        assert!(rec_lf.feasible);
        assert!(
            rec_lf.k_min_sag <= rec_lf.k_target_r3 && rec_lf.k_target_r3 <= rec_lf.k_max_total_80,
            "{rec_lf:?}"
        );
        // f_n uyarıma çok yakın: rezonans.
        let stiff = DamperInput {
            k_per_n_mm: 29.6,
            ..soft
        };
        let rep = analyze(&stiff, &exc);
        assert!(rep.results.iter().any(|r| r.near_resonance));
        // Çok yumuşak: çökme uyarısı.
        let sag = DamperInput {
            k_per_n_mm: 0.05,
            ..soft
        };
        assert!(analyze(&sag, &exc).too_soft);
    }

    #[test]
    fn curve_is_finite_and_sorted() {
        let c = curve(10.0, 0.1, 500.0, 100);
        assert_eq!(c.len(), 100);
        assert!(c.iter().all(|p| p[0].is_finite() && p[1].is_finite()));
        assert!(c.windows(2).all(|w| w[1][0] > w[0][0]));
    }

    #[test]
    fn all_vehicle_types_give_finite_rpm_and_reports() {
        for dt in DroneType::ALL {
            let mut p = Project::default();
            p.set_drone_type(dt);
            let m = evaluate(&p, &Library::embedded());
            let info = rpm_sources(&p, &m);
            let mut exc = Vec::new();
            if let Some(r) = info.hover_rpm {
                exc.extend(excitations(0, r, info.hover_blades));
            }
            if let Some(r) = info.cruise_rpm {
                exc.extend(excitations(1, r, info.cruise_blades));
            }
            assert!(!exc.is_empty(), "{dt:?}");
            let inp = DamperInput {
                mass_kg: 0.35,
                n_dampers: 4,
                k_per_n_mm: 0.6,
                zeta: 0.1,
                max_sag_mm: 5.0,
            };
            let rep = analyze(&inp, &exc);
            assert!(rep.f_n.is_finite() && rep.static_defl_mm.is_finite());
            for r in &rep.results {
                assert!(
                    r.t.is_finite() && r.isolation_pct.is_finite() && r.r.is_finite(),
                    "{dt:?}"
                );
            }
            let rec = rep.recommendation.unwrap();
            assert!(rec.k_max_total_80.is_finite() && rec.k_min_sag.is_finite());
        }
    }
}
