// SPDX-License-Identifier: AGPL-3.0-or-later
//! Adım 3 (Kanat & Kuyruk / Çerçeve) basit modu.
//!
//! Sabit kanat: "ne kadar büyük olsun" (mevcut `SizePref` kanat yüklemesi), "taşınabilir mi /
//! sökülebilir kanat" → kanat alanı, açıklık ve kuyruk alanı planı (`plan_wing`, kullanıcı onayıyla
//! uygulanır). Multirotor/VTOL: "çerçeve boyu sınıfı" (250/450/550/650+), "kaç motor" → çerçeve,
//! otomatik pervane boyu ve bu sınıfa uygun bileşen kütleleri (saf, cevaplara bağlı).
use super::mission::{Goal, aero_purpose};
use super::{Derived, Field, choice};
use crate::calc::aero_presets::{SizePref, recommend, suggest_wing_area};
use crate::calc::multirotor::{MultirotorLayout, adjacent_spacing, max_prop_diameter};
use crate::calc::{Library, evaluate};
use crate::project::{DroneType, Project};
use serde::{Deserialize, Serialize};

choice!(
    /// "Çerçeve boyu sınıfı" (motor-motor köşegen).
    FrameClass, "simple.frame.class", C450, [
        C250 => "c250",
        C450 => "c450",
        C550 => "c550",
        C650 => "c650",
    ]
);

impl FrameClass {
    /// Köşegen çerçeve boyu [m].
    pub fn diagonal_m(self) -> f64 {
        match self {
            FrameClass::C250 => 0.25,
            FrameClass::C450 => 0.45,
            FrameClass::C550 => 0.55,
            FrameClass::C650 => 0.65,
        }
    }
}

choice!(
    /// "Kaç motor?"
    MotorCount, "simple.frame.motors", Quad, [
        Quad => "quad",
        Hexa => "hexa",
        Octo => "octo",
    ]
);

impl MotorCount {
    pub fn arms(self) -> u32 {
        match self {
            MotorCount::Quad => 4,
            MotorCount::Hexa => 6,
            MotorCount::Octo => 8,
        }
    }
}

choice!(
    /// "Uçak nasıl taşınacak?" (sabit kanat)
    Transport, "simple.frame.transport", Home, [
        Home => "home",
        Backpack => "backpack",
    ]
);

/// Çerçeve/kanat adımı cevapları. Kanat büyüklüğü tercihi `aero_simple.size_pref` ile ortaktır.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FrameAnswers {
    pub class: FrameClass,
    pub motors: MotorCount,
    pub transport: Transport,
    /// Kanat sökülebilir mi (iki yarım kanat ayrı taşınır).
    pub removable_wing: bool,
}

/// Pervane uç boşluğu [m]: küçük çerçevede 25 mm (referans 250 sınıfı 5" pervane ile tutarlı),
/// büyükte 15 mm (hexa 550 örneğiyle tutarlı).
pub fn prop_clearance(d_frame: f64) -> f64 {
    if d_frame <= 0.30 { 0.025 } else { 0.015 }
}

/// Çerçeveye sığan en büyük TAM İNÇ pervane çapı [m] (komşu motor aralığı − boşluk; alta yuvarlanır).
pub fn auto_prop_diameter(d_frame: f64, arms: u32, layout: MultirotorLayout) -> f64 {
    let s = adjacent_spacing(d_frame, arms, layout, 0.0, 0.0);
    let d_max = max_prop_diameter(s, prop_clearance(d_frame));
    ((d_max / 0.0254).floor().max(2.0)) * 0.0254
}

/// Bileşen kütleleri [kg]: proje referans örneklerinden (250: çerçeve 110 g, motor 32 g, ESC 10 g,
/// FC 25 g, diğer 50 g; 550: 280/65/15/45/60 g) köşegen boyuna göre doğrusal uydurma (±%30).
/// Dönüş: (çerçeve, tek motor, toplam ESC, FC, diğer).
pub fn mr_component_masses(d_frame: f64, arms: u32) -> (f64, f64, f64, f64, f64) {
    let dd = d_frame - 0.25;
    let frame = (0.11 + 0.567 * dd).max(0.03);
    let motor = (0.032 + 0.11 * dd).max(0.005);
    let esc_each = (0.010 + 0.0167 * dd).max(0.004);
    let fc = (0.025 + 0.0667 * dd).max(0.010);
    let misc = (0.050 + 0.0333 * dd).max(0.020);
    (frame, motor, esc_each * arms as f64, fc, misc)
}

/// Çerçeve/MR adımı için ham girdiler (yalnızca multirotor/VTOL; sabit kanatta boş).
pub fn derive(a: &FrameAnswers, dt: DroneType, layout: MultirotorLayout) -> Derived {
    if !dt.is_multirotor() {
        return Vec::new();
    }
    let d = a.class.diagonal_m();
    let arms = a.motors.arms();
    let mut v = vec![
        (Field::MrFrame, d),
        (Field::MrArms, arms as f64),
        (Field::MrClearance, prop_clearance(d)),
        (Field::MrDProp, auto_prop_diameter(d, arms, layout)),
        // Merkez plaka yarıçapı ≈ 15 mm + 0.08·D (250: 35 mm, 550: 59 mm; örnekler 35/55 mm).
        (Field::MrRPlate, 0.015 + 0.08 * d),
    ];
    // Saf multirotorda kütleler çerçeve sınıfından gelir; VTOL'da kaldırma sistemi
    // ağırlık adımındaki bütçeden türetilir (çift kayıt olmasın).
    if dt == DroneType::Multirotor {
        let (fr, mo, esc, fc, misc) = mr_component_masses(d, arms);
        v.push((Field::MrMFrame, fr));
        v.push((Field::MrMMotor, mo));
        v.push((Field::MrMEsc, esc));
        v.push((Field::MrMFc, fc));
        v.push((Field::MrMMisc, misc));
    }
    v
}

/// Sırt çantası/taşıma eşiği [m]: 40-50 L yürüyüş çantası ~60-70 cm yüksekliğindedir.
pub const BACKPACK_MAX_M: f64 = 0.65;

/// Taşıma kontrolü: en uzun parça ve sığıp sığmadığı.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PackFit {
    pub longest_m: f64,
    pub fits: bool,
}

pub fn pack_fit(b: f64, l_fuse: f64, removable_wing: bool) -> PackFit {
    let wing_piece = if removable_wing { b / 2.0 } else { b };
    let longest = wing_piece.max(l_fuse);
    PackFit {
        longest_m: longest,
        fits: longest <= BACKPACK_MAX_M,
    }
}

/// Kanat planı (kullanıcı onayıyla uygulanır).
#[derive(Clone, Debug, PartialEq)]
pub struct WingPlan {
    pub s: f64,
    pub b: f64,
    pub ar: f64,
    pub l_spar: f64,
    pub s_h: f64,
    pub s_v: f64,
    pub s_vtail: f64,
    pub fit: PackFit,
    /// Sırt çantası sınırı açıklığı kısıtladı mı.
    pub capped_by_pack: bool,
}

/// Kanat alanı = W / (W/S) (tercih edilen kanat yüklemesi); açıklık = √(AR·S) (AR amaca göre
/// `aero_presets::recommend`); sırt çantası seçilmişse en uzun parça ≤ 65 cm olacak şekilde
/// açıklık kısılır; kuyruk alanları modelin hacim-oranı hedeflerinden (`tail.s_*_req`) okunur.
/// Kanat kabuk kütlesi alana bağlı olduğu için kütle 4 turda yakınsatılır.
pub fn plan_wing(
    p: &Project,
    lib: &Library,
    goal: Goal,
    size: SizePref,
    transport: Transport,
    removable: bool,
) -> Option<WingPlan> {
    let ar = recommend(aero_purpose(goal)).ar;
    let mut t = p.clone();
    let mut s = t.s;
    let mut b = t.b;
    let mut capped = false;
    for _ in 0..4 {
        let m = evaluate(&t, lib);
        let ns = suggest_wing_area(m.weight.m_mtow, size);
        if !ns.is_finite() || ns <= 0.0 {
            return None;
        }
        s = ns;
        b = (ar * s).sqrt();
        capped = false;
        if transport == Transport::Backpack {
            let cap = if removable {
                2.0 * BACKPACK_MAX_M
            } else {
                BACKPACK_MAX_M
            };
            if b > cap {
                b = cap;
                capped = true;
            }
        }
        t.s = s;
        t.b = b;
    }
    let m = evaluate(&t, lib);
    if !m.inputs_ok {
        return None;
    }
    Some(WingPlan {
        s,
        b,
        ar: b * b / s,
        // Kiriş boyu ≈ 0.8125·açıklık (referans: 1.3 m / 1.6 m).
        l_spar: 0.8125 * b,
        s_h: m.tail.s_h_req,
        s_v: m.tail.s_v_req,
        s_vtail: m.tail.s_vtail_req,
        fit: pack_fit(b, t.l_fuse, removable),
        capped_by_pack: capped,
    })
}

/// Planı projeye yazar (kuyruk alanlarını yalnızca ilgili kuyruk türü için).
pub fn apply_wing_plan(p: &mut Project, w: &WingPlan) {
    use crate::calc::geometry::TailKind;
    p.s = w.s;
    p.b = w.b;
    p.l_spar = w.l_spar;
    match p.tail_kind {
        TailKind::Conventional => {
            p.s_h = w.s_h;
            p.s_v = w.s_v;
        }
        TailKind::VTail => p.s_vtail = w.s_vtail,
        TailKind::FlyingWing => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mr_derivation_valid_for_all_combinations() {
        for &class in FrameClass::ALL {
            for &motors in MotorCount::ALL {
                for dt in [DroneType::Multirotor, DroneType::VtolQuadplane] {
                    for layout in [MultirotorLayout::X, MultirotorLayout::Plus] {
                        let a = FrameAnswers {
                            class,
                            motors,
                            ..Default::default()
                        };
                        let d = derive(&a, dt, layout);
                        let mut p = Project::default();
                        for (f, v) in &d {
                            assert!(v.is_finite() && *v > 0.0, "{f:?}");
                            f.set(&mut p, *v);
                        }
                        // Pervane birbirine çarpmaz: komşu aralık ≥ pervane + boşluk
                        let s = adjacent_spacing(p.mr_frame_size, p.mr_arm_count, layout, 0.0, 0.0);
                        assert!(
                            p.mr_d_prop + p.mr_prop_clearance <= s + 1e-9,
                            "{class:?} {motors:?}"
                        );
                        assert!(p.mr_d_prop >= 0.0508 - 1e-9);
                        assert!(p.mr_r_plate < p.mr_frame_size / 2.0);
                        assert_eq!(
                            dt == DroneType::Multirotor,
                            d.iter().any(|(f, _)| *f == Field::MrMFrame)
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn reference_examples_are_reproduced_roughly() {
        // 250 quad → 5", 550 hexa → 10" (proje örnekleri)
        let q = auto_prop_diameter(0.25, 4, MultirotorLayout::X);
        assert!((q / 0.0254 - 5.0).abs() < 1e-6);
        let h = auto_prop_diameter(0.55, 6, MultirotorLayout::X);
        assert!((h / 0.0254 - 10.0).abs() < 1e-6);
        let (fr, mo, _esc, _fc, _misc) = mr_component_masses(0.25, 4);
        assert!((fr - 0.11).abs() < 1e-9 && (mo - 0.032).abs() < 1e-9);
    }

    #[test]
    fn backpack_report() {
        assert!(pack_fit(1.2, 0.5, true).fits);
        assert!(!pack_fit(1.6, 0.5, false).fits);
        assert!(!pack_fit(1.2, 0.9, true).fits);
    }

    #[test]
    fn wing_plan_is_consistent_and_respects_pack() {
        let lib = Library::embedded();
        let p = Project::default();
        let free = plan_wing(
            &p,
            &lib,
            Goal::Photo,
            SizePref::Normal,
            Transport::Home,
            false,
        )
        .unwrap();
        assert!(free.s > 0.0 && free.b > 0.0 && (free.ar - 7.0).abs() < 1e-6);
        assert!(!free.capped_by_pack);
        let pack = plan_wing(
            &p,
            &lib,
            Goal::Mapping,
            SizePref::Floaty,
            Transport::Backpack,
            false,
        )
        .unwrap();
        assert!(pack.b <= BACKPACK_MAX_M + 1e-9 && pack.capped_by_pack);
        let rem = plan_wing(
            &p,
            &lib,
            Goal::Photo,
            SizePref::Normal,
            Transport::Backpack,
            true,
        )
        .unwrap();
        assert!(rem.b <= 2.0 * BACKPACK_MAX_M + 1e-9);
        // Plan uygulanınca model geçerli kalır
        let mut q = p.clone();
        apply_wing_plan(&mut q, &free);
        let m = evaluate(&q, &lib);
        assert!(m.inputs_ok);
        assert!(
            (m.tail.v_h - q.vh_target).abs() < 0.02,
            "yatay kuyruk hacmi hedefte olmalı"
        );
    }

    #[test]
    fn fixed_wing_has_no_mr_derivation() {
        let d = derive(
            &FrameAnswers::default(),
            DroneType::FlyingWing,
            MultirotorLayout::X,
        );
        assert!(d.is_empty());
    }
}
