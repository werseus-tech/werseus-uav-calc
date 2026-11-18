// SPDX-License-Identifier: AGPL-3.0-or-later
//! Itki tezgahi verisi ice aktarma ve SITL/HIL parametre koprusu (saf kod).
//! Proje formatini DEGISTIRMEZ: olculen egri yalnizca oturum icin bir karsilastirma/not olarak
//! gosterilir.

pub mod csv;
pub mod fit;
pub mod sitl;

use crate::calc::Model;
use crate::project::{DroneType, Project};
use csv::Sample;
use fit::{CtFit, EffPoint, HoverWhat, Poly, PowerAt};

/// Uygulamanin Ct varsayimi (ucus modeli askida RPM tahmininde kullanir).
pub const CT_MODEL: f64 = 0.10;

/// Tasarimin motor basina tahmin ettigi degerler (karsilastirma referansi).
#[derive(Clone, Copy, Debug, Default)]
pub struct DesignRef {
    /// Askida (coklu govde / VTOL) ya da statik azami itkide (sabit kanat) motor basina itki [N].
    pub t_motor_n: f64,
    /// Ayni durumda motor basina elektrik gucu [W].
    pub p_elec_motor_w: f64,
    pub d_prop_m: f64,
    pub rho: f64,
    pub n_motors: u32,
    /// Askida toplam elektrik gucu [W]; sabit kanatta 0 (askida kalma yok).
    pub p_elec_total_w: f64,
    pub p_other_w: f64,
    pub e_usable_wh: f64,
    pub hover_endurance_min: f64,
    /// true: askida (dikey kaldirma) referans, false: statik tezgah referansi (sabit kanat).
    pub is_hover: bool,
}

impl DesignRef {
    pub fn from_model(p: &Project, m: &Model) -> DesignRef {
        let rho = if m.atmo.rho > 0.1 { m.atmo.rho } else { 1.225 };
        match p.drone_type {
            DroneType::Multirotor | DroneType::VtolQuadplane => DesignRef {
                t_motor_n: m.mr.t_hover_motor,
                p_elec_motor_w: m.mr.p_elec_motor,
                d_prop_m: p.mr_d_prop,
                rho,
                n_motors: m.mr.n_motors,
                p_elec_total_w: m.mr.p_elec_hover_total,
                p_other_w: (m.mr.p_total_hover - m.mr.p_elec_hover_total).max(0.0),
                e_usable_wh: m.mr.e_usable_wh,
                hover_endurance_min: m.mr.hover_endurance_min,
                is_hover: true,
            },
            // Sabit kanat: tezgah ölçümü durağandır (V = 0); seyir gücüyle değil, statik
            // azami itki noktasıyla karşılaştırılır.
            _ => DesignRef {
                t_motor_n: m.prop.t_static,
                p_elec_motor_w: m.prop.p_elec_static,
                d_prop_m: p.d_prop,
                rho,
                n_motors: 1,
                ..Default::default()
            },
        }
    }
}

/// Olcum analizi sonucu.
#[derive(Clone, Debug, Default)]
pub struct Analysis {
    pub ct: Option<CtFit>,
    pub eff: Vec<EffPoint>,
    pub throttle_curve: Option<Poly>,
    pub power_at_design: Option<PowerAt>,
    /// (olculen - tasarim) / tasarim gucu, yuzde.
    pub power_diff_pct: Option<f64>,
    /// Olculen Ct ile modelin Ct varsayimi arasindaki yuzde fark.
    pub ct_diff_pct: Option<f64>,
    pub what_if: Option<HoverWhat>,
    /// Olculen guc / tasarim gucu orani.
    pub power_ratio: Option<f64>,
}

/// Olcumleri analiz eder ve tasarimla karsilastirir.
pub fn analyze(samples: &[Sample], d: &DesignRef) -> Analysis {
    let ct = fit::fit_ct(samples, d.d_prop_m, d.rho);
    let eff = fit::efficiency_points(samples);
    let throttle_curve = fit::throttle_curve(samples, 2);
    let power_at_design = fit::power_at_thrust(samples, d.t_motor_n);
    let power_diff_pct = power_at_design.and_then(|pa| fit::diff_pct(pa.power_w, d.p_elec_motor_w));
    // CT_MODEL askı (çok gövde) varsayımıdır; sabit kanat için karşılaştırılmaz.
    let ct_diff_pct = ct
        .filter(|_| d.is_hover)
        .and_then(|c| fit::diff_pct(c.ct, CT_MODEL));
    let power_ratio = power_at_design
        .filter(|_| d.p_elec_motor_w > 0.0)
        .map(|pa| pa.power_w / d.p_elec_motor_w);
    let what_if = if d.is_hover {
        power_ratio
            .and_then(|r| fit::hover_with_ratio(d.p_elec_total_w, d.p_other_w, d.e_usable_wh, r))
    } else {
        None
    };
    Analysis {
        ct,
        eff,
        throttle_curve,
        power_at_design,
        power_diff_pct,
        ct_diff_pct,
        what_if,
        power_ratio,
    }
}

/// Yaygin tezgah dokumlerini taklit eden, URETILMIS (gercek olmayan) ornek CSV.
pub fn demo_csv() -> &'static str {
    include_str!("../../tests/fixtures/thrust_stand_rcbenchmark_sample.csv")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Library, evaluate};

    #[test]
    fn fixtures_parse_and_analyze() {
        for txt in [
            include_str!("../../tests/fixtures/thrust_stand_rcbenchmark_sample.csv"),
            include_str!("../../tests/fixtures/thrust_stand_generic_semicolon.csv"),
        ] {
            let t = csv::parse_table(txt);
            let a = csv::auto_map(&t.headers);
            let r = csv::build_samples(&t, &a.mapping, &a.units);
            assert!(r.samples.len() >= 8, "{}", r.samples.len());
            assert!(r.skipped >= 1);
            let mut p = Project::default();
            p.set_drone_type(DroneType::Multirotor);
            let m = evaluate(&p, &Library::embedded());
            let an = analyze(&r.samples, &DesignRef::from_model(&p, &m));
            assert!(an.ct.is_some());
            assert!(!an.eff.is_empty());
            assert!(an.throttle_curve.is_some());
            let ct = an.ct.unwrap();
            assert!(ct.ct > 0.02 && ct.ct < 0.3, "{}", ct.ct);
        }
    }

    #[test]
    fn design_ref_is_finite_for_all_types() {
        for dt in DroneType::ALL {
            let mut p = Project::default();
            p.set_drone_type(dt);
            let m = evaluate(&p, &Library::embedded());
            let d = DesignRef::from_model(&p, &m);
            for v in [d.t_motor_n, d.p_elec_motor_w, d.d_prop_m, d.rho] {
                assert!(v.is_finite(), "{dt:?}");
            }
            let an = analyze(&[], &d);
            assert!(an.ct.is_none() && an.what_if.is_none());
        }
    }

    #[test]
    fn comparison_flows_to_whatif() {
        let mut p = Project::default();
        p.set_drone_type(DroneType::Multirotor);
        let m = evaluate(&p, &Library::embedded());
        let d = DesignRef::from_model(&p, &m);
        // Olcum: tasarimdan %20 fazla guc ceken motor.
        let s: Vec<Sample> = (1..=8)
            .map(|i| {
                let t = d.t_motor_n * i as f64 / 4.0;
                let pw = 1.2 * d.p_elec_motor_w * (t / d.t_motor_n).powf(1.5);
                Sample {
                    thrust_n: t,
                    rpm: Some(3000.0 * (t / d.t_motor_n).sqrt()),
                    volt: Some(12.0),
                    curr: Some(pw / 12.0),
                    throttle: Some(i as f64 / 10.0),
                }
            })
            .collect();
        let an = analyze(&s, &d);
        let pct = an.power_diff_pct.unwrap();
        assert!((pct - 20.0).abs() < 4.0, "{pct}");
        let w = an.what_if.unwrap();
        assert!(w.endurance_min < d.hover_endurance_min);
    }
}
