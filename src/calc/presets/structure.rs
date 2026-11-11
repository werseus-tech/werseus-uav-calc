// SPDX-License-Identifier: AGPL-3.0-or-later
//! Adım 9 (Yapı) ve multirotor kol borusu basit modu: "otomatik seç" — dayanım (ve kol için
//! sehim) hedefinden en hafif uygun boru. Adaylar, standart ticari boru ölçüleri listesinden
//! mevcut projeyle `evaluate` edilerek denenir; yapı formülleri (`calc::structure`,
//! `calc::mr_structure`) değişmez, yalnızca girdi (boru çapları) seçilir.
use super::choice;
use crate::calc::{Library, evaluate};
use crate::project::Project;
use serde::{Deserialize, Serialize};

choice!(
    /// "Ne kadar sağlam olsun?"
    Strength, "simple.struct.strength", Normal, [
        Normal => "normal",
        Extra => "extra",
    ]
);

impl Strength {
    /// Hedef emniyet payı (MS = σ_izin/σ_max − 1): normal ≥0.25, ekstra ≥0.60.
    /// (Uyarı eşiği 0.2; hobi İHA'da %25-60 ek pay pratik.)
    pub fn target_ms(self) -> f64 {
        match self {
            Strength::Normal => 0.25,
            Strength::Extra => 0.60,
        }
    }
}

/// Yapı adımı cevapları.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StructAnswers {
    pub strength: Strength,
}

/// Standart boru ölçüleri (dış, iç) [mm]: yaygın çekme/sarım karbon ve alüminyum boru katalog
/// ölçüleri (1 ve 1.5 mm cidar ağırlıklı).
pub const TUBES_MM: &[(f64, f64)] = &[
    (6.0, 4.0),
    (8.0, 6.0),
    (8.0, 5.0),
    (10.0, 8.0),
    (10.0, 7.0),
    (12.0, 10.0),
    (12.0, 9.0),
    (14.0, 12.0),
    (14.0, 11.0),
    (16.0, 14.0),
    (16.0, 13.0),
    (18.0, 16.0),
    (20.0, 18.0),
    (20.0, 17.0),
    (22.0, 20.0),
    (25.0, 22.0),
    (30.0, 27.0),
];

/// Seçilen boru ve günlük dil sonuçları.
#[derive(Clone, Debug, PartialEq)]
pub struct TubePick {
    pub d_o_mm: f64,
    pub d_i_mm: f64,
    /// Boru (kiriş) kütlesi [g]: tek kiriş veya tüm kollar.
    pub mass_g: f64,
    /// Emniyet payı (MS).
    pub ms: f64,
    /// Tasarım sınır yükünün kaç katına kadar dayanır = FS·(1+MS).
    pub fail_multiple: f64,
    /// Sınır yükte uç sehiminin yarım açıklığa oranı (yalnızca kanat kirişi; kolda NaN).
    pub tip_deflection_frac: f64,
    /// Hedef sağlandı mı (değilse en güçlü aday döner).
    pub meets_target: bool,
}

/// Karbon boru elastisite modülü [Pa]: çekme (pultruded) CFRP boru, 120 GPa
/// (`MrArmMaterialPreset::PultrudedCf` ile aynı kaynak değer).
pub const SPAR_E_PA: f64 = 120.0e9;

/// Sınır yükte izin verilen uç sehimi: yarım açıklığın %10'u. Dayanak: hobi/İHA kanatlarında
/// sınır yükte uç sehiminin yarım açıklığın ~%10-15'ini aşmaması pratik kuraldır (aşırı esneme
/// aeroelastik kayıplara ve kontrol zayıflığına yol açar); alt sınır (%10) seçildi. Yaklaşık değerdir.
pub const SPAR_DEFLECTION_MAX: f64 = 0.10;

/// Kiriş uç sehiminin yarım açıklığa oranı (saf, yaklaşık): yarım kanat, kökten ankastre konsol;
/// toplam yarım yük n·W/2 yarım açıklık L=b/2 boyunca düzgün yayılı kabul edilir:
/// δ = (nW/2)·L³ / (8·E·I). Eliptik dağılım biraz daha az sehim verir (bu tahmin muhafazakâr).
pub fn spar_tip_deflection_frac(n_limit: f64, w: f64, b: f64, d_o: f64, d_i: f64, e: f64) -> f64 {
    let ei = e * crate::calc::structure::spar_i(d_o, d_i);
    let l = b / 2.0;
    if !(ei > 0.0 && l > 0.0) {
        return f64::INFINITY;
    }
    let delta = (n_limit * w / 2.0) * l.powi(3) / (8.0 * ei);
    delta / l
}

/// Kanat kirişi için en hafif uygun boru: dayanım (MS ≥ hedef) VE sehim (uç sehimi ≤ %10 yarım
/// açıklık, bkz. `SPAR_DEFLECTION_MAX`) birlikte sağlanmalı. Hiçbiri yetmezse en dengeli aday döner.
pub fn auto_spar(p: &Project, lib: &Library, target_ms: f64) -> Option<TubePick> {
    let mut best_meet: Option<TubePick> = None;
    let mut best_any: Option<(f64, TubePick)> = None;
    let mut t = p.clone();
    for &(o, i) in TUBES_MM {
        t.d_spar_o = o / 1000.0;
        t.d_spar_i = i / 1000.0;
        let m = evaluate(&t, lib);
        if !m.inputs_ok || !m.st.ms.is_finite() {
            continue;
        }
        let area = crate::calc::weight::spar_area(t.d_spar_o, t.d_spar_i);
        let mass_g = area * t.l_spar * t.rho_cf * 1000.0;
        let defl = spar_tip_deflection_frac(
            t.n_limit, m.weight.w, t.b, t.d_spar_o, t.d_spar_i, SPAR_E_PA,
        );
        let pick = TubePick {
            d_o_mm: o,
            d_i_mm: i,
            mass_g,
            ms: m.st.ms,
            fail_multiple: t.fs * (1.0 + m.st.ms),
            tip_deflection_frac: defl,
            meets_target: m.st.ms >= target_ms && defl <= SPAR_DEFLECTION_MAX,
        };
        let score = (m.st.ms / target_ms).min(SPAR_DEFLECTION_MAX / defl);
        if pick.meets_target
            && best_meet
                .as_ref()
                .is_none_or(|b| pick.mass_g < b.mass_g - 1e-9)
        {
            best_meet = Some(pick.clone());
        }
        if best_any.as_ref().is_none_or(|(sc, _)| score > *sc) {
            best_any = Some((score, pick));
        }
    }
    best_meet.or(best_any.map(|(_, b)| b))
}

/// Multirotor kolu için en hafif uygun boru (MS ≥ hedef ve sehim sınırı içinde). Yuvarlak kesit
/// varsayılır (kare kesitte `None`). Hiçbiri yetmezse en yüksek MS'li aday.
pub fn auto_arm(p: &Project, lib: &Library, target_ms: f64) -> Option<TubePick> {
    if p.mr_arm_section != crate::calc::mr_structure::MrArmSectionType::Round {
        return None;
    }
    let mut best_meet: Option<TubePick> = None;
    let mut best_any: Option<TubePick> = None;
    let mut t = p.clone();
    for &(o, i) in TUBES_MM {
        t.mr_arm_d_o = o / 1000.0;
        t.mr_arm_d_i = i / 1000.0;
        let m = evaluate(&t, lib);
        if !m.inputs_ok || !m.mr_st.ms.is_finite() {
            continue;
        }
        let pick = TubePick {
            d_o_mm: o,
            d_i_mm: i,
            mass_g: m.mr_st.m_arm_total * 1000.0,
            ms: m.mr_st.ms,
            fail_multiple: t.mr_arm_fs * (1.0 + m.mr_st.ms),
            tip_deflection_frac: f64::NAN,
            meets_target: m.mr_st.ms >= target_ms && m.mr_st.deflection_ok,
        };
        if pick.meets_target
            && best_meet
                .as_ref()
                .is_none_or(|b| pick.mass_g < b.mass_g - 1e-9)
        {
            best_meet = Some(pick.clone());
        }
        if best_any.as_ref().is_none_or(|b| pick.ms > b.ms) {
            best_any = Some(pick);
        }
    }
    best_meet.or(best_any)
}

/// Seçimi kanat kirişine yazar.
pub fn apply_spar(p: &mut Project, t: &TubePick) {
    p.d_spar_o = t.d_o_mm / 1000.0;
    p.d_spar_i = t.d_i_mm / 1000.0;
}

/// Seçimi multirotor kollarına yazar.
pub fn apply_arm(p: &mut Project, t: &TubePick) {
    p.mr_arm_d_o = t.d_o_mm / 1000.0;
    p.mr_arm_d_i = t.d_i_mm / 1000.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tube_list_is_well_formed_and_sorted_sensibly() {
        for &(o, i) in TUBES_MM {
            assert!(i > 0.0 && i < o);
        }
    }

    #[test]
    fn spar_pick_meets_target_and_is_lightest() {
        let lib = Library::embedded();
        let p = Project::default();
        let t = auto_spar(&p, &lib, 0.25).expect("seçim");
        assert!(t.meets_target && t.ms >= 0.25);
        let mut q = p.clone();
        apply_spar(&mut q, &t);
        let m = evaluate(&q, &lib);
        assert!(m.st.ms >= 0.25 - 1e-9);
        assert!(
            crate::ui::wizard::validate_step(crate::ui::wizard::StepId::Structure, &q, &m).is_ok()
        );
        // Daha sıkı hedef daha ağır (ya da eşit) boru ister
        let t2 = auto_spar(&p, &lib, 0.60).unwrap();
        assert!(t2.mass_g >= t.mass_g - 1e-9);
        // Hiçbir aday daha hafif olup hedefi sağlamıyor
        let mut r = p.clone();
        for &(o, i) in TUBES_MM {
            r.d_spar_o = o / 1000.0;
            r.d_spar_i = i / 1000.0;
            let m = evaluate(&r, &lib);
            let mass = crate::calc::weight::spar_area(r.d_spar_o, r.d_spar_i)
                * r.l_spar
                * r.rho_cf
                * 1000.0;
            if m.st.ms >= 0.25 {
                assert!(mass >= t.mass_g - 1e-9, "{o}/{i} daha hafif ve yeterli");
            }
        }
    }

    #[test]
    fn spar_pick_also_meets_deflection_and_is_lightest_among_both() {
        let lib = Library::embedded();
        let mut p = Project::default();
        p.m_payload = 1.2; // ağır uçak: sehim bağlayıcı hale gelir
        let t = auto_spar(&p, &lib, 0.25).unwrap();
        assert!(t.meets_target);
        assert!(t.tip_deflection_frac <= SPAR_DEFLECTION_MAX + 1e-12);
        let mut r = p.clone();
        for &(o, i) in TUBES_MM {
            r.d_spar_o = o / 1000.0;
            r.d_spar_i = i / 1000.0;
            let m = evaluate(&r, &lib);
            let d = spar_tip_deflection_frac(
                r.n_limit, m.weight.w, r.b, r.d_spar_o, r.d_spar_i, SPAR_E_PA,
            );
            let mass = crate::calc::weight::spar_area(r.d_spar_o, r.d_spar_i)
                * r.l_spar
                * r.rho_cf
                * 1000.0;
            if m.st.ms >= 0.25 && d <= SPAR_DEFLECTION_MAX {
                assert!(mass >= t.mass_g - 1e-9, "{o}/{i}");
            }
        }
    }

    #[test]
    fn deflection_never_makes_the_pick_lighter_than_strength_only() {
        let lib = Library::embedded();
        let mut p = Project::default();
        p.b = 3.0;
        p.s = 0.6;
        p.l_spar = 2.4;
        let with = auto_spar(&p, &lib, 0.25).unwrap();
        let mut best: Option<f64> = None;
        let mut q = p.clone();
        for &(o, i) in TUBES_MM {
            q.d_spar_o = o / 1000.0;
            q.d_spar_i = i / 1000.0;
            let m = evaluate(&q, &lib);
            let mass = crate::calc::weight::spar_area(q.d_spar_o, q.d_spar_i)
                * q.l_spar
                * q.rho_cf
                * 1000.0;
            if m.st.ms >= 0.25 && best.is_none_or(|b| mass < b) {
                best = Some(mass);
            }
        }
        assert!(with.mass_g >= best.unwrap() - 1e-9);
    }

    #[test]
    fn deflection_formula_scales_physically() {
        let d = |o, i| spar_tip_deflection_frac(3.8, 20.0, 1.6, o, i, SPAR_E_PA);
        assert!(d(0.012, 0.010) < d(0.010, 0.008));
        // δ/L ∝ L²: açıklık 2x → 4x
        let a = spar_tip_deflection_frac(3.8, 20.0, 1.0, 0.01, 0.008, SPAR_E_PA);
        let b = spar_tip_deflection_frac(3.8, 20.0, 2.0, 0.01, 0.008, SPAR_E_PA);
        assert!((b / a - 4.0).abs() < 1e-9);
        assert!(spar_tip_deflection_frac(3.8, 20.0, 1.6, 0.0, 0.0, SPAR_E_PA).is_infinite());
    }

    #[test]
    fn heavier_airframe_needs_stronger_spar() {
        let lib = Library::embedded();
        let mut heavy = Project::default();
        heavy.m_payload = 1.5;
        let a = auto_spar(&Project::default(), &lib, 0.25).unwrap();
        let b = auto_spar(&heavy, &lib, 0.25).unwrap();
        assert!(b.mass_g >= a.mass_g);
    }

    #[test]
    fn arm_pick_on_examples() {
        let lib = Library::embedded();
        for f in ["quad_250_x.json", "hexa_550_x.json", "vtol_quadplane.json"] {
            let p = crate::project::load_example(f).unwrap();
            let t = auto_arm(&p, &lib, 0.25).unwrap_or_else(|| panic!("{f}"));
            let mut q = p.clone();
            apply_arm(&mut q, &t);
            let m = evaluate(&q, &lib);
            assert!(m.mr_st.ms.is_finite(), "{f}");
            assert!(
                crate::ui::wizard::validate_step(crate::ui::wizard::StepId::MrStructure, &q, &m)
                    .is_ok(),
                "{f}"
            );
            assert!(t.fail_multiple > 0.0);
        }
    }

    #[test]
    fn square_section_is_not_auto_picked() {
        let lib = Library::embedded();
        let mut p = Project::default();
        p.mr_arm_section = crate::calc::mr_structure::MrArmSectionType::Square;
        assert!(auto_arm(&p, &lib, 0.25).is_none());
    }
}
