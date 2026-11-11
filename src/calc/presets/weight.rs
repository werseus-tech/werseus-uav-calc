// SPDX-License-Identifier: AGPL-3.0-or-later
//! Adım 2 (Ağırlık / Batarya) basit modu: "Bütçe/boyut" (küçük ~1 kg / orta ~2 kg / büyük ~4 kg),
//! yapı malzemesi ve "bana batarya öner" (hedef süreye göre hücre/paket önerisi).
//!
//! Kütle dağılımı varsayılanları, referans tasarımın (2 kg, spec Bölüm 10) kütle oranlarından
//! ölçeklenir: aviyonik %7.5, itki grubu %10, gövde+kuyruk kabuğu %17 (hobi/öğrenci ölçeği
//! tipik aralıkları; ±%30). Matematik (`calc::weight`) değişmez.
use super::{Derived, Field, Rating, choice};
use crate::calc::weight::Material;
use crate::calc::{Library, evaluate};
use crate::project::{DroneType, Project};
use serde::{Deserialize, Serialize};

choice!(
    /// "Bütçe / boyut": hedef uçuş kütlesi sınıfı.
    Budget, "simple.weight.budget", Medium, [
        Small => "small",
        Medium => "medium",
        Large => "large",
    ]
);

choice!(
    /// "Yapı malzemesi": kütle ve rijitlik için üç sade seçenek.
    Build, "simple.weight.build", Light, [
        Light => "light",
        Standard => "standard",
        Reinforced => "reinforced",
    ]
);

impl Build {
    /// Malzeme veritabanındaki kimlik (assets/materials.json) ve yoğunluk [kg/m³].
    /// Hafif: köpürtülmüş hafif plastik (600), standart: sert plastik (1270), güçlendirilmiş:
    /// karbon takviyeli (1300, daha rijit). Yoğunluklar veritabanıyla birebir eşleşir (testle doğrulanır).
    pub fn material(self) -> (&'static str, f64) {
        match self {
            Build::Light => ("lw_pla_foamed", 600.0),
            Build::Standard => ("petg", 1270.0),
            Build::Reinforced => ("cf_petg", 1300.0),
        }
    }

    /// Gövde+kuyruk kütle çarpanı: daha yoğun malzeme daha ağır gövde demektir; kabuk kalınlığı
    /// yoğunluk oranı kadar artmaz (kalınlık sabit kalır, çarpan 1.0 / 1.2 / 1.3).
    pub fn fuse_factor(self) -> f64 {
        match self {
            Build::Light => 1.0,
            Build::Standard => 1.2,
            Build::Reinforced => 1.3,
        }
    }
}

/// Ağırlık adımı cevapları.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WeightAnswers {
    pub budget: Budget,
    pub build: Build,
}

/// Hedef uçuş kütlesi [kg]: sabit kanat/uçan kanat 1/2/4 kg; VTOL aynı sınıf için ×1.5
/// (kaldırma motorları, ESC'ler ve ek yapı kütleyi ~%25-50 artırır).
pub fn target_mtow_kg(b: Budget, dt: DroneType) -> f64 {
    let base = match b {
        Budget::Small => 1.0,
        Budget::Medium => 2.0,
        Budget::Large => 4.0,
    };
    if dt == DroneType::VtolQuadplane {
        base * 1.5
    } else {
        base
    }
}

/// Cevaplardan kütle girdilerini üretir (sabit kanat / uçan kanat / VTOL).
/// Multirotor kütleleri çerçeve adımındadır (`frame`), burada yalnızca malzeme/batarya vardır.
pub fn derive(a: &WeightAnswers, dt: DroneType) -> Derived {
    if dt == DroneType::Multirotor {
        return Vec::new();
    }
    let m = target_mtow_kg(a.budget, dt);
    let avionics = 0.075 * m;
    // İtki grubu: motor+ESC+pervane ≈ %10; VTOL'da kaldırma motorları dahil ≈ %22.
    let propulsion = if dt == DroneType::VtolQuadplane {
        0.22 * m
    } else {
        0.10 * m
    };
    // Uçan kanatta kuyruk ve uzun gövde yok: kabuk kütlesi ≈ %70.
    let shape = if dt == DroneType::FlyingWing {
        0.7
    } else {
        1.0
    };
    let fuse_tail = 0.17 * m * a.build.fuse_factor() * shape;
    vec![
        (Field::Avionics, avionics),
        (Field::Propulsion, propulsion),
        (Field::FuseTail, fuse_tail),
        (Field::RhoMat, a.build.material().1),
    ]
}

/// Malzeme kimliğini ve adını da projeye yazar (yoğunlukla birlikte tutarlı kalsın diye).
pub fn apply_material_identity(p: &mut Project, a: &WeightAnswers, materials: &[Material]) {
    let (id, rho) = a.build.material();
    p.material_id = id.to_string();
    if let Some(m) = materials.iter().find(|m| m.id == id) {
        p.material_name = m.name.clone();
        p.rho_mat = m.density_kg_m3;
    } else {
        p.rho_mat = rho;
    }
}

/// Batarya payı derecesi: kalkış kütlesinin %25-40'ı iyi denge; %20 altı kısa süre, %50 üstü ağır.
pub fn battery_fraction_rating(f: f64) -> Rating {
    if !f.is_finite() || f <= 0.0 {
        Rating::Bad
    } else if (0.25..=0.45).contains(&f) {
        Rating::Good
    } else if (0.15..=0.55).contains(&f) {
        Rating::Ok
    } else {
        Rating::Bad
    }
}

/// "Bana öner" sonucu.
#[derive(Clone, Debug, PartialEq)]
pub struct BatteryRec {
    pub cell_name: String,
    pub n_series: u32,
    pub n_parallel: u32,
    /// Modelin bu paketle verdiği havada kalma süresi [dk] (sabit kanat: seyir/dayanıklılık,
    /// multirotor: askı).
    pub endurance_min: f64,
    pub pack_mass_kg: f64,
    pub pack_wh: f64,
    /// Hedef süre sağlandı mı (değilse en uzun süreyi veren akım-uygun paket döner).
    pub meets_target: bool,
}

/// Verilen hedef süreye göre en hafif uygun kütüphane paketini bulur: kütüphanedeki her hücre için
/// 2-6S × 1-8P kombinasyonları mevcut projede `evaluate` ile denenir; akım sınırı aşılmayan ve
/// batarya payı <%55 olan adaylar arasından hedefi sağlayan en hafifi seçilir. Hedef sağlanamazsa
/// en uzun süreyi veren akım-uygun paket döner (`meets_target = false`). Formüller değişmez.
pub fn recommend_battery(p: &Project, lib: &Library, target_min: f64) -> Option<BatteryRec> {
    let mr = p.drone_type == DroneType::Multirotor;
    let mut trial = p.clone();
    trial.battery_source = crate::calc::custom_battery::BatterySourceKind::Library;
    let mut best_meet: Option<BatteryRec> = None;
    let mut best_any: Option<BatteryRec> = None;
    for cell in &lib.cells {
        for s in 2..=6u32 {
            for par in 1..=8u32 {
                trial.cell_name = cell.name.clone();
                trial.n_series = s;
                trial.n_parallel = par;
                let m = evaluate(&trial, lib);
                if !m.inputs_ok {
                    continue;
                }
                let (end, mtow, current_ok) = if mr {
                    (m.mr.hover_endurance_min, m.mr.m_mtow, m.mr.i_hover_ok)
                } else {
                    (
                        m.perf_end.endurance_min(),
                        m.weight.m_mtow,
                        m.prop.current_ok,
                    )
                };
                if !end.is_finite() || !current_ok || mtow <= 0.0 {
                    continue;
                }
                if m.weight.m_battery / mtow > 0.55 {
                    continue;
                }
                let rec = BatteryRec {
                    cell_name: cell.name.clone(),
                    n_series: s,
                    n_parallel: par,
                    endurance_min: end,
                    pack_mass_kg: m.weight.m_battery,
                    pack_wh: m.weight.pack_wh,
                    meets_target: end >= target_min,
                };
                if rec.meets_target {
                    let better = best_meet.as_ref().is_none_or(|b| {
                        rec.pack_mass_kg < b.pack_mass_kg - 1e-9
                            || ((rec.pack_mass_kg - b.pack_mass_kg).abs() <= 1e-9
                                && rec.n_series * rec.n_parallel < b.n_series * b.n_parallel)
                    });
                    if better {
                        best_meet = Some(rec.clone());
                    }
                }
                if best_any
                    .as_ref()
                    .is_none_or(|b| rec.endurance_min > b.endurance_min)
                {
                    best_any = Some(rec);
                }
            }
        }
    }
    best_meet.or(best_any)
}

/// Öneriyi projeye yazar (kütüphane kaynağı).
pub fn apply_battery(p: &mut Project, r: &BatteryRec) {
    p.battery_source = crate::calc::custom_battery::BatterySourceKind::Library;
    p.cell_name = r.cell_name.clone();
    p.n_series = r.n_series;
    p.n_parallel = r.n_parallel;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_combinations_valid_for_wizard_lock() {
        for &budget in Budget::ALL {
            for &build in Build::ALL {
                for dt in DroneType::ALL {
                    let d = derive(&WeightAnswers { budget, build }, dt);
                    if dt == DroneType::Multirotor {
                        assert!(d.is_empty());
                        continue;
                    }
                    for (f, v) in &d {
                        assert!(v.is_finite() && *v > 0.0, "{f:?} {v}");
                    }
                }
            }
        }
    }

    #[test]
    fn medium_light_reproduces_reference_masses() {
        let d = derive(&WeightAnswers::default(), DroneType::FixedWingConventional);
        let g = |f| d.iter().find(|(x, _)| *x == f).unwrap().1;
        assert!((g(Field::Avionics) - 0.15).abs() < 1e-9);
        assert!((g(Field::Propulsion) - 0.20).abs() < 1e-9);
        assert!((g(Field::FuseTail) - 0.34).abs() < 1e-9);
    }

    #[test]
    fn material_table_matches_database() {
        let mats = crate::calc::weight::load_materials();
        for &b in Build::ALL {
            let (id, rho) = b.material();
            let m = mats
                .iter()
                .find(|m| m.id == id)
                .unwrap_or_else(|| panic!("malzeme yok: {id}"));
            assert!((m.density_kg_m3 - rho).abs() < 1e-9, "{id}");
        }
    }

    #[test]
    fn bigger_budget_gives_heavier_parts() {
        let a = |b| {
            derive(
                &WeightAnswers {
                    budget: b,
                    build: Build::Light,
                },
                DroneType::FixedWingConventional,
            )
        };
        let f = |d: &Derived| d.iter().find(|(x, _)| *x == Field::FuseTail).unwrap().1;
        assert!(f(&a(Budget::Small)) < f(&a(Budget::Medium)));
        assert!(f(&a(Budget::Medium)) < f(&a(Budget::Large)));
    }

    #[test]
    fn battery_recommendation_meets_target_and_is_applicable() {
        let lib = Library::embedded();
        let mut p = Project::default();
        let rec = recommend_battery(&p, &lib, 20.0).expect("öneri bulunmalı");
        apply_battery(&mut p, &rec);
        let m = evaluate(&p, &lib);
        assert!(m.inputs_ok);
        if rec.meets_target {
            assert!(m.perf_end.endurance_min() >= 20.0 - 1e-6);
        }
        // Uzun hedef daha ağır (ya da eşit) paket ister
        let r2 = recommend_battery(&Project::default(), &lib, 40.0).unwrap();
        // 40 dk'yı sağlayan her paket 20 dk'yı da sağlar: en hafif 20 dk paketi daha ağır olamaz.
        if rec.meets_target && r2.meets_target {
            assert!(r2.pack_mass_kg >= rec.pack_mass_kg - 1e-9);
        }
        assert!(
            rec.meets_target,
            "varsayılan projede 20 dk hedefi sağlanabilmeli"
        );
    }

    #[test]
    fn battery_recommendation_multirotor_uses_hover_time() {
        let lib = Library::embedded();
        let p = crate::project::load_example("quad_250_x.json").unwrap();
        let rec = recommend_battery(&p, &lib, 10.0).expect("öneri");
        assert!(rec.endurance_min.is_finite() && rec.pack_mass_kg > 0.0);
    }

    #[test]
    fn fraction_rating_bands() {
        assert_eq!(battery_fraction_rating(0.32), Rating::Good);
        assert_eq!(battery_fraction_rating(0.18), Rating::Ok);
        assert_eq!(battery_fraction_rating(0.05), Rating::Bad);
        assert_eq!(battery_fraction_rating(f64::NAN), Rating::Bad);
    }
}
