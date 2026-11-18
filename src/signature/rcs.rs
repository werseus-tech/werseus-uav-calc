// SPDX-License-Identifier: AGPL-3.0-or-later
//! Radar kesit alanı (RCS) için kaba, eğitim amaçlı tahmin.
//!
//! Yöntem: gövde montaj yerleşiminden ([`Layout`]) basit şekillere ayrılır (düz plaka, silindir,
//! küre, dönen pervane palleri) ve her birinin RCS'i fiziksel optik (PO) kapalı formlarıyla
//! hesaplanır; parçalar arasındaki faz ilişkisi yok sayılarak **eş fazsız (incoherent) toplam**
//! alınır. Malzeme etkisi tek bir güç yansıtma çarpanıyla (0…1) temsil edilir.
//!
//! Kaynaklar:
//! - E. F. Knott, J. F. Shaeffer, M. T. Tuley, *Radar Cross Section*, 2nd ed., SciTech 2004,
//!   Tablo 6.1 / Böl. 5–6 (plaka σ = 4πA²/λ², silindir σ = 2πaL²/λ, küre σ = πa², açı düşüşü
//!   [sin x / x]²).
//! - M. I. Skolnik, *Introduction to Radar Systems*, 3rd ed., McGraw-Hill 2001, Böl. 2.7–2.8
//!   (radar denklemi: R ∝ σ^¼; Rayleigh bölgesi küre σ ≈ 9πa²(ka)⁴).
//! - I. H. Abbott, A. E. von Doenhoff, *Theory of Wing Sections*, Dover 1959 (NACA 4 haneli hücum
//!   kenarı yarıçapı r = 1.1019 (t/c)² c).
//! - Dielektrik yarı uzay dik geliş güç yansıması |Γ|² = ((√εr − 1)/(√εr + 1))² (ince kabuklar
//!   için kaba üst sınır).
//!
//! Basitleştirmeler (bilerek): kenar kırınımı, gezici dalgalar, kavite (motor içi, gövde içi)
//! rezonansları, çoklu yansıma ve parça gölgelemesi yok; [sin x / x]² lob yapısı, lobların ortalaması
//! olan zarfla (≈ 1/(2x²)) değiştirilir; pervaneler dönme açısı üzerinden zaman ortalamasıdır (gerçek
//! sinyalde pal parlamaları/mikro-Doppler vardır). Sonuç ±10 dB mertebesinde belirsizdir.

use super::to_db;
use crate::cad::geom::{Frame, V3, cross, dot, unit};
use crate::cad::layout::Layout;
use crate::cad::outputs::PartId;
use crate::calc::Model;
use crate::calc::mr_structure::{MrArmMaterialPreset, MrArmSectionType};
use crate::project::{DroneType, Project};
use std::f64::consts::{FRAC_1_SQRT_2, PI};

/// Işık hızı [m/s].
pub const C0: f64 = 299_792_458.0;

/// Pervane zaman ortalaması için dönme açısı örnek sayısı.
const N_ROT: usize = 24;

/// Değerlendirilen radar bantları (tipik merkez frekanslar).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Band {
    /// S bandı, 3 GHz.
    S,
    /// X bandı, 10 GHz.
    X,
    /// Ku bandı, 15 GHz.
    Ku,
    /// 24 GHz (otomotiv/çevre algılama tipi).
    K24,
    /// 77 GHz (otomotiv tipi).
    W77,
}

impl Band {
    pub const ALL: [Band; 5] = [Band::S, Band::X, Band::Ku, Band::K24, Band::W77];

    pub fn freq_hz(self) -> f64 {
        match self {
            Band::S => 3.0e9,
            Band::X => 10.0e9,
            Band::Ku => 15.0e9,
            Band::K24 => 24.0e9,
            Band::W77 => 77.0e9,
        }
    }

    pub fn wavelength_m(self) -> f64 {
        C0 / self.freq_hz()
    }

    pub fn short(self) -> &'static str {
        match self {
            Band::S => "S 3 GHz",
            Band::X => "X 10 GHz",
            Band::Ku => "Ku 15 GHz",
            Band::K24 => "K 24 GHz",
            Band::W77 => "W 77 GHz",
        }
    }
}

/// Radar açısından malzeme sınıfı.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialClass {
    Metal,
    Carbon,
    CfFilled,
    Fibreglass,
    Plastic,
    Wood,
    Foam,
}

impl MaterialClass {
    pub const COUNT: usize = 7;
    pub const ALL: [MaterialClass; 7] = [
        MaterialClass::Metal,
        MaterialClass::Carbon,
        MaterialClass::CfFilled,
        MaterialClass::Fibreglass,
        MaterialClass::Plastic,
        MaterialClass::Wood,
        MaterialClass::Foam,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    /// Varsayılan güç yansıtma çarpanı (iletken = 1). Dielektrikler için tipik εr ile dik geliş
    /// yarı uzay |Γ|²; karbon kompozit ≈ −1 dB; karbon katkılı plastik kaba ara değer.
    pub fn default_reflectivity(self) -> f64 {
        match self {
            MaterialClass::Metal => 1.0,
            MaterialClass::Carbon => 0.8,
            MaterialClass::CfFilled => 0.3,
            MaterialClass::Fibreglass => gamma2_normal(4.5),
            MaterialClass::Plastic => gamma2_normal(2.8),
            MaterialClass::Wood => gamma2_normal(2.0),
            MaterialClass::Foam => gamma2_normal(1.1),
        }
    }
}

/// Dielektrik yarı uzayda dik geliş güç yansıması |Γ|² (εr: bağıl dielektrik sabiti).
pub fn gamma2_normal(eps_r: f64) -> f64 {
    let n = eps_r.max(1.0).sqrt();
    ((n - 1.0) / (n + 1.0)).powi(2)
}

/// Proje malzemesinden (kimlik ya da ad) radar malzeme sınıfı.
pub fn material_class_for(id_or_name: &str) -> MaterialClass {
    match crate::calc::weight::resolve_material_id(id_or_name) {
        "cf_petg" => MaterialClass::CfFilled,
        "carbon_tube_pultruded" | "carbon_tube_roll_wrapped" => MaterialClass::Carbon,
        "aluminum_6061_tube" => MaterialClass::Metal,
        "epp_foam_30" | "epo_foam" | "depron_xps" => MaterialClass::Foam,
        "balsa_medium" | "plywood_poplar_3mm" => MaterialClass::Wood,
        _ => MaterialClass::Plastic,
    }
}

/// Multirotor kol / VTOL bom malzemesinin sınıfı.
pub fn arm_class(preset: MrArmMaterialPreset) -> MaterialClass {
    match preset {
        MrArmMaterialPreset::Al6061 => MaterialClass::Metal,
        MrArmMaterialPreset::Petg => MaterialClass::Plastic,
        MrArmMaterialPreset::CfPetg => MaterialClass::CfFilled,
        _ => MaterialClass::Carbon,
    }
}

/// Katkı tablosunda gösterilen parça grubu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Fuselage,
    Wing,
    LeadingEdge,
    Spar,
    Tail,
    Boom,
    Arm,
    CenterPlate,
    Motor,
    Propeller,
    Battery,
    Gear,
}

impl Part {
    pub const ALL: [Part; 12] = [
        Part::Fuselage,
        Part::Wing,
        Part::LeadingEdge,
        Part::Spar,
        Part::Tail,
        Part::Boom,
        Part::Arm,
        Part::CenterPlate,
        Part::Motor,
        Part::Propeller,
        Part::Battery,
        Part::Gear,
    ];
}

/// Basit saçıcı şekil (gövde ekseninde: x kıç yönü, z yukarı).
#[derive(Clone, Copy, Debug)]
pub enum Shape {
    /// Dikdörtgen plaka: kenar uzunlukları `a` (e1 yönü) ve `b` (e2 yönü), normal = e1 × e2.
    Plate { a: f64, b: f64, e1: V3, e2: V3 },
    /// Dairesel silindir: yarıçap, boy, eksen (birim vektör).
    Cylinder { radius: f64, length: f64, axis: V3 },
    /// Küre (ya da küresel burun/kuyruk kapağı).
    Sphere { radius: f64 },
    /// Dönen pervane: paller ince plaka, dönme açısı üzerinden ortalama.
    Rotor {
        radius: f64,
        chord: f64,
        blades: u32,
        /// 0.75R'deki pal açısı [rad].
        pitch_rad: f64,
        axis: V3,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct Primitive {
    pub part: Part,
    pub shape: Shape,
    pub material: MaterialClass,
    /// Kaç adet (eş fazsız toplanır).
    pub count: f64,
    /// İç parçalar için kabuktan iki yönlü geçiş çarpanı (dış parçalarda 1).
    pub shield: f64,
}

/// RCS ayarları (kullanıcı değiştirebilir; proje dosyasına yazılmaz).
#[derive(Clone, Debug, PartialEq)]
pub struct RcsSettings {
    /// Radarın araca göre yükseliş açısı [°] (pozitif: radar aracın altında, aşağıdan bakıyor).
    pub elevation_deg: f64,
    /// Malzeme sınıfı başına güç yansıtma çarpanı.
    pub reflectivity: [f64; MaterialClass::COUNT],
    /// Gövde kabuğu malzemesi (None: projeden).
    pub skin: Option<MaterialClass>,
    pub prop_material: MaterialClass,
    /// İç donanımı (pil kutusu) dahil et.
    pub include_internal: bool,
    /// Motor çapı [mm]; 0 = motor kütlesinden tahmin.
    pub motor_d_mm: f64,
    /// Referans hedef RCS'i [m²] ve o hedefin algılandığı varsayılan menzil [km].
    pub ref_sigma_m2: f64,
    pub ref_range_km: f64,
}

impl Default for RcsSettings {
    fn default() -> Self {
        let mut reflectivity = [0.0; MaterialClass::COUNT];
        for c in MaterialClass::ALL {
            reflectivity[c.index()] = c.default_reflectivity();
        }
        RcsSettings {
            elevation_deg: 5.0,
            reflectivity,
            skin: None,
            prop_material: MaterialClass::Plastic,
            include_internal: true,
            motor_d_mm: 0.0,
            ref_sigma_m2: 1.0,
            ref_range_km: 10.0,
        }
    }
}

impl RcsSettings {
    pub fn refl(&self, c: MaterialClass) -> f64 {
        let r = self.reflectivity[c.index()];
        if r.is_finite() {
            r.clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

// ------------------------------------------------------------------------------------------
// Kapalı form PO formülleri
// ------------------------------------------------------------------------------------------

/// [sin x / x]² çarpanının lob-ortalamalı zarfı: |x| < 1/√2 için 1, sonra 1/(2x²)
/// (sin² ortalaması 1/2). Açı örneklemesinde lob titreşimini (aliasing) önler.
pub fn sinc2_envelope(x: f64) -> f64 {
    let x = x.abs();
    if x < FRAC_1_SQRT_2 {
        1.0
    } else {
        0.5 / (x * x)
    }
}

/// Düz plakanın dik gelişteki tepe RCS'i: σ = 4πA²/λ² (Knott Tablo 6.1).
pub fn plate_rcs_normal(area: f64, lambda: f64) -> f64 {
    4.0 * PI * area * area / (lambda * lambda)
}

/// a×b dikdörtgen plakanın monostatik PO RCS'i. `u1`, `u2`: radar yönünün kenar eksenleri
/// üzerindeki bileşenleri (sinθ cosφ, sinθ sinφ), `un`: normal bileşeni (cosθ).
/// σ = 4π(ab)²/λ² · cos²θ · S(k a u1) · S(k b u2), S = sinc² zarfı.
pub fn plate_rcs(a: f64, b: f64, lambda: f64, u1: f64, u2: f64, un: f64) -> f64 {
    let k = 2.0 * PI / lambda;
    plate_rcs_normal(a * b, lambda)
        * un
        * un
        * sinc2_envelope(k * a * u1)
        * sinc2_envelope(k * b * u2)
}

/// Silindirin geniş yüz (broadside) RCS'i: σ = 2πaL²/λ (Knott Tablo 6.1).
pub fn cylinder_rcs_broadside(radius: f64, length: f64, lambda: f64) -> f64 {
    2.0 * PI * radius * length * length / lambda
}

/// Silindir, broadside'dan ψ kadar eğik: σ = 2πaL²/λ · cosψ · S(kL sinψ).
/// `sin_psi`: radar yönünün silindir eksenindeki bileşeni.
pub fn cylinder_rcs(radius: f64, length: f64, lambda: f64, sin_psi: f64) -> f64 {
    let s = sin_psi.clamp(-1.0, 1.0);
    let cos_psi = (1.0 - s * s).sqrt();
    let k = 2.0 * PI / lambda;
    cylinder_rcs_broadside(radius, length, lambda) * cos_psi * sinc2_envelope(k * length * s)
}

/// İletken küre: optik bölgede πa², küçük kürede Rayleigh 9πa²(ka)⁴; ikisinin küçüğü
/// (rezonans tepesi yok sayılır).
pub fn sphere_rcs(radius: f64, lambda: f64) -> f64 {
    let ka = 2.0 * PI * radius / lambda;
    PI * radius * radius * (9.0 * ka.powi(4)).min(1.0)
}

/// Radar menzil oranı: aynı radar ve koşullarda R/R_ref = (σ/σ_ref)^¼ (Skolnik, radar denklemi).
pub fn range_ratio(sigma: f64, sigma_ref: f64) -> f64 {
    if sigma > 0.0 && sigma_ref > 0.0 {
        (sigma / sigma_ref).powf(0.25)
    } else {
        0.0
    }
}

/// Radar yön birim vektörü (hedeften radara), gövde ekseninde. az = 0: burun karşıdan,
/// 90°: sağ yandan; el > 0: radar aracın altında.
pub fn look_dir(az_deg: f64, el_deg: f64) -> V3 {
    let (sa, ca) = az_deg.to_radians().sin_cos();
    let (se, ce) = el_deg.to_radians().sin_cos();
    [-ce * ca, ce * sa, -se]
}

/// Eksene dik iki birim vektör.
fn perp_basis(axis: V3) -> (V3, V3) {
    let a = unit(axis);
    let hint = if a[2].abs() < 0.9 {
        [0.0, 0.0, 1.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let u = unit(cross(a, hint));
    let v = cross(a, u);
    (u, v)
}

/// Tek şeklin (malzeme ve adet çarpanı olmadan) RCS'i [m²].
pub fn shape_rcs(sh: &Shape, lambda: f64, d: V3) -> f64 {
    match *sh {
        Shape::Plate { a, b, e1, e2 } => {
            let n = unit(cross(e1, e2));
            plate_rcs(a, b, lambda, dot(d, e1), dot(d, e2), dot(d, n))
        }
        Shape::Cylinder {
            radius,
            length,
            axis,
        } => cylinder_rcs(radius, length, lambda, dot(d, axis)),
        Shape::Sphere { radius } => sphere_rcs(radius, lambda),
        Shape::Rotor {
            radius,
            chord,
            blades,
            pitch_rad,
            axis,
        } => {
            let a = unit(axis);
            let (u, v) = perp_basis(a);
            let len = 0.85 * radius;
            let (sb, cb) = pitch_rad.sin_cos();
            let mut sum = 0.0;
            for j in 0..N_ROT {
                let psi = 2.0 * PI * j as f64 / N_ROT as f64;
                let (s, c) = psi.sin_cos();
                let er = [
                    c * u[0] + s * v[0],
                    c * u[1] + s * v[1],
                    c * u[2] + s * v[2],
                ];
                let et = cross(a, er);
                let e2 = [
                    cb * et[0] + sb * a[0],
                    cb * et[1] + sb * a[1],
                    cb * et[2] + sb * a[2],
                ];
                let n = cross(er, e2);
                sum += plate_rcs(len, chord, lambda, dot(d, er), dot(d, e2), dot(d, n));
            }
            blades as f64 * sum / N_ROT as f64
        }
    }
}

/// Bir parçanın ayarlar dahil katkısı [m²].
pub fn primitive_rcs(pr: &Primitive, lambda: f64, d: V3, s: &RcsSettings) -> f64 {
    let v = shape_rcs(&pr.shape, lambda, d) * pr.count * s.refl(pr.material) * pr.shield;
    if v.is_finite() { v.max(0.0) } else { 0.0 }
}

// ------------------------------------------------------------------------------------------
// Gövdenin şekillere ayrılması
// ------------------------------------------------------------------------------------------

/// Gövdenin RCS modeli.
#[derive(Clone, Debug)]
pub struct Airframe {
    pub prims: Vec<Primitive>,
    pub skin: MaterialClass,
    /// Kullanılan motor çapı [m].
    pub motor_d_m: f64,
    pub n_motors: u32,
}

/// Motor çapı tahmini [m]: dış rotorlu motorun eşdeğer yoğunluğu ≈ 3500 kg/m³ ve h ≈ 0.8d
/// (hacim ≈ 0.628 d³) varsayımıyla. 2207 (~30 g) → ~24 mm, 4114 (~150 g) → ~41 mm.
pub fn motor_diameter_from_mass(m_kg: f64) -> f64 {
    let m = if m_kg.is_finite() { m_kg } else { 0.05 };
    (m.max(0.005) / (3500.0 * 0.628)).cbrt()
}

fn pos(x: f64, fallback: f64) -> f64 {
    if x.is_finite() && x > 0.0 {
        x
    } else {
        fallback
    }
}

fn dir_of(f: &Frame, local: V3) -> V3 {
    let d = f.apply_dir(local);
    if d.iter().all(|c| c.is_finite()) && (d[0].abs() + d[1].abs() + d[2].abs()) > 1e-9 {
        unit(d)
    } else {
        local
    }
}

/// NACA 4 haneli hücum kenarı yarıçapı r = 1.1019 (t/c)² c.
pub fn le_radius(tc: f64, chord: f64) -> f64 {
    1.1019 * tc * tc * chord
}

/// Projeden RCS şekil listesi (montaj yerleşimi [`Layout`] üzerinden).
pub fn airframe(p: &Project, m: &Model, s: &RcsSettings) -> Airframe {
    let lay = Layout::new(p, m);
    let mm = 1e-3;
    let skin = s.skin.unwrap_or_else(|| {
        material_class_for(if p.material_id.is_empty() {
            &p.material_name
        } else {
            &p.material_id
        })
    });
    let arm_mat = arm_class(p.mr_arm_material);
    let skin_pass = (1.0 - s.refl(skin)).powi(2);
    let multirotor = p.drone_type == DroneType::Multirotor;
    let mut prims: Vec<Primitive> = Vec::new();
    let mut push = |part: Part, shape: Shape, material: MaterialClass, count: f64, shield: f64| {
        prims.push(Primitive {
            part,
            shape,
            material,
            count,
            shield,
        })
    };

    // Motor boyutu.
    let m_motor_lift = pos(p.mr_m_motor, 0.06);
    let m_motor_cruise = 0.45 * pos(p.m_propulsion, 0.2);
    let d_override = if s.motor_d_mm.is_finite() && s.motor_d_mm > 1.0 {
        Some(s.motor_d_mm * mm)
    } else {
        None
    };
    let d_lift = d_override.unwrap_or_else(|| motor_diameter_from_mass(m_motor_lift));
    let d_cruise = d_override.unwrap_or_else(|| motor_diameter_from_mass(m_motor_cruise));
    let motor = |push: &mut dyn FnMut(Part, Shape, MaterialClass, f64, f64),
                 d: f64,
                 axis: V3,
                 count: f64| {
        let h = 0.8 * d;
        push(
            Part::Motor,
            Shape::Cylinder {
                radius: d / 2.0,
                length: h,
                axis,
            },
            MaterialClass::Metal,
            count,
            1.0,
        );
        // Çan kapağı: eşit alanlı kare plaka.
        let (u, v) = perp_basis(axis);
        let side = d * PI.sqrt() / 2.0;
        push(
            Part::Motor,
            Shape::Plate {
                a: side,
                b: side,
                e1: u,
                e2: v,
            },
            MaterialClass::Metal,
            count,
            1.0,
        );
    };
    let prop_mat = s.prop_material;
    let rotor = |push: &mut dyn FnMut(Part, Shape, MaterialClass, f64, f64),
                 d_prop: f64,
                 pitch_over_d: f64,
                 blades: u32,
                 axis: V3,
                 count: f64| {
        let r = d_prop / 2.0;
        let pitch_rad = (pitch_over_d * d_prop / (2.0 * PI * 0.75 * r)).atan();
        push(
            Part::Propeller,
            Shape::Rotor {
                radius: r,
                chord: 0.08 * d_prop,
                blades: blades.clamp(2, 6),
                pitch_rad,
                axis,
            },
            prop_mat,
            count,
            1.0,
        );
    };

    let mut n_motors = 0u32;
    let has = |label: &str| lay.instances.iter().any(|i| i.label == label);
    let count_prefix = |pre: &str| {
        lay.instances
            .iter()
            .filter(|i| i.label.starts_with(pre))
            .count()
    };

    if !multirotor {
        // --- Gövde: orta silindir (etkin boy ≈ 0.7 L) + burun/kuyruk küresel kapakları.
        let fl = pos(lay.fuselage.length_mm * mm, 0.8);
        let fr = pos(lay.fuselage.max_diameter_mm * mm / 2.0, 0.04);
        push(
            Part::Fuselage,
            Shape::Cylinder {
                radius: fr,
                length: 0.7 * fl,
                axis: [1.0, 0.0, 0.0],
            },
            skin,
            1.0,
            1.0,
        );
        for r in [lay.fuselage.nose_r_mm, lay.fuselage.tail_r_mm] {
            push(
                Part::Fuselage,
                Shape::Sphere {
                    radius: pos(r * mm, 0.01),
                },
                skin,
                1.0,
                1.0,
            );
        }

        // --- Taşıyıcı ve kuyruk yüzeyleri (yerel X kord, Z açıklık, Y kalınlık normali).
        let tc_w = pos(p.tc_w, 0.12);
        let tc_t = pos(p.tc_t, 0.09);
        let mut surface = |part: Part,
                           f: &Frame,
                           c_root: f64,
                           c_tip: f64,
                           span: f64,
                           sweep_deg: f64,
                           tc: f64,
                           with_le: bool| {
            let c_mean = pos(0.5 * (c_root + c_tip) * mm, 0.1);
            let span = pos(span * mm, 0.2);
            let ex = dir_of(f, [1.0, 0.0, 0.0]);
            let ez = dir_of(f, [0.0, 0.0, 1.0]);
            push(
                part,
                Shape::Plate {
                    a: c_mean,
                    b: span,
                    e1: ex,
                    e2: ez,
                },
                skin,
                1.0,
                1.0,
            );
            if with_le {
                let tl = sweep_deg.to_radians().tan();
                let axis = unit([ex[0] * tl + ez[0], ex[1] * tl + ez[1], ex[2] * tl + ez[2]]);
                push(
                    Part::LeadingEdge,
                    Shape::Cylinder {
                        radius: le_radius(tc, c_mean).max(1e-4),
                        length: span * (1.0 + tl * tl).sqrt(),
                        axis,
                    },
                    skin,
                    1.0,
                    1.0,
                );
            }
        };
        for inst in &lay.instances {
            let f = &inst.frame;
            match inst.id {
                PartId::Wing => surface(
                    Part::Wing,
                    f,
                    lay.wing.root_chord_mm,
                    lay.wing.tip_chord_mm,
                    lay.wing.half_span_mm,
                    lay.wing.sweep_le_deg,
                    tc_w,
                    true,
                ),
                PartId::TailHorizontal => surface(
                    Part::Tail,
                    f,
                    lay.tail_h.root_chord_mm,
                    lay.tail_h.tip_chord_mm,
                    lay.tail_h.span_mm,
                    lay.tail_h.sweep_le_deg,
                    tc_t,
                    true,
                ),
                PartId::TailVertical => surface(
                    Part::Tail,
                    f,
                    lay.tail_v.root_chord_mm,
                    lay.tail_v.tip_chord_mm,
                    lay.tail_v.span_mm,
                    lay.tail_v.sweep_le_deg,
                    tc_t,
                    true,
                ),
                PartId::TailVTail => surface(
                    Part::Tail,
                    f,
                    lay.vtail.root_chord_mm,
                    lay.vtail.tip_chord_mm,
                    lay.vtail.span_mm,
                    lay.vtail.sweep_le_deg,
                    tc_t,
                    true,
                ),
                PartId::Winglet => surface(
                    Part::Tail,
                    f,
                    lay.winglet.root_chord_mm,
                    lay.winglet.tip_chord_mm,
                    lay.winglet.span_mm,
                    lay.winglet.sweep_le_deg,
                    tc_t,
                    false,
                ),
                _ => {}
            }
        }

        // --- Kanat kirişi (karbon, kabuğun içinde).
        if let Some(sp) = &lay.spar {
            for inst in lay.instances.iter().filter(|i| i.id == PartId::WingSpar) {
                push(
                    Part::Spar,
                    Shape::Cylinder {
                        radius: pos(sp.od_mm * mm / 2.0, 0.005),
                        length: pos(sp.length_mm * mm, 0.5),
                        axis: dir_of(&inst.frame, [0.0, 0.0, 1.0]),
                    },
                    MaterialClass::Carbon,
                    1.0,
                    skin_pass,
                );
            }
        }

        // --- İtki motoru (eksen x) ve pervanesi.
        if has("MotorMount") {
            n_motors += 1;
            motor(&mut push, d_cruise, [1.0, 0.0, 0.0], 1.0);
            let d_prop = pos(p.d_prop, 0.25);
            let pod = pos(p.prop_pitch / d_prop, 0.6).clamp(0.2, 1.5);
            rotor(&mut push, d_prop, pod, 2, [1.0, 0.0, 0.0], 1.0);
        }

        // --- VTOL: bomlar ve kaldırma motorları.
        for inst in lay.instances.iter().filter(|i| i.id == PartId::VtolBoom) {
            push(
                Part::Boom,
                Shape::Cylinder {
                    radius: pos(lay.boom.od_mm * mm / 2.0, 0.008),
                    length: pos(lay.boom.length_mm * mm, 0.5),
                    axis: dir_of(&inst.frame, [0.0, 0.0, 1.0]),
                },
                arm_mat,
                1.0,
                1.0,
            );
        }
        let n_lift = count_prefix("LiftMount_") as u32;
        if n_lift > 0 {
            n_motors += n_lift;
            let n = n_lift as f64;
            motor(&mut push, d_lift, [0.0, 0.0, 1.0], n);
            rotor(
                &mut push,
                pos(p.mr_d_prop, 0.3),
                0.45,
                p.mr_prop_blades,
                [0.0, 0.0, 1.0],
                n,
            );
        }
    } else {
        // --- Multirotor: merkez plakalar (üst+alt), kollar, motorlar, pervaneler.
        let pr = pos(lay.mr_plate.plate_radius_mm * mm, 0.05);
        let side = pr * PI.sqrt();
        push(
            Part::CenterPlate,
            Shape::Plate {
                a: side,
                b: side,
                e1: [1.0, 0.0, 0.0],
                e2: [0.0, 1.0, 0.0],
            },
            arm_mat,
            2.0,
            1.0,
        );
        let arm_len = pos(lay.mr_arm.arm_length_mm * mm, 0.15);
        let d_arm = pos(p.mr_arm_d_o, 0.016);
        for inst in lay.instances.iter().filter(|i| i.id == PartId::MrArm) {
            let ax = dir_of(&inst.frame, [1.0, 0.0, 0.0]);
            match p.mr_arm_section {
                MrArmSectionType::Round => push(
                    Part::Arm,
                    Shape::Cylinder {
                        radius: d_arm / 2.0,
                        length: arm_len,
                        axis: ax,
                    },
                    arm_mat,
                    1.0,
                    1.0,
                ),
                MrArmSectionType::Square => {
                    // Kare kesit: yan yüz (normal yatay) ve üst yüz (normal düşey).
                    let side_n = unit(cross([0.0, 0.0, 1.0], ax));
                    push(
                        Part::Arm,
                        Shape::Plate {
                            a: arm_len,
                            b: d_arm,
                            e1: ax,
                            e2: [0.0, 0.0, 1.0],
                        },
                        arm_mat,
                        1.0,
                        1.0,
                    );
                    push(
                        Part::Arm,
                        Shape::Plate {
                            a: arm_len,
                            b: d_arm,
                            e1: ax,
                            e2: side_n,
                        },
                        arm_mat,
                        1.0,
                        1.0,
                    );
                }
            }
        }
        let n_mounts = count_prefix("MrMotorMount_").max(1) as u32;
        let per = if p.mr_coaxial { 2 } else { 1 };
        n_motors = n_mounts * per;
        let n = n_motors as f64;
        motor(&mut push, d_lift, [0.0, 0.0, 1.0], n);
        rotor(
            &mut push,
            pos(p.mr_d_prop, 0.25),
            0.45,
            p.mr_prop_blades,
            [0.0, 0.0, 1.0],
            n,
        );
    }

    // --- İniş takımı bacakları (düşey silindir; açılma yok sayılır).
    let n_gear = count_prefix("LandingGear_");
    if n_gear > 0 {
        push(
            Part::Gear,
            Shape::Cylinder {
                radius: pos(lay.leg.leg_dia_mm * mm / 2.0, 0.004),
                length: pos(lay.leg.leg_len_mm * mm, 0.08),
                axis: [0.0, 0.0, 1.0],
            },
            skin,
            n_gear as f64,
            1.0,
        );
    }

    // --- Pil kutusu (pil folyosu/hücre kutuları iletken kabul edilir). Sabit kanatta kabuğun içinde.
    if s.include_internal {
        let m_b = pos(m.weight.m_battery, 0.3);
        let vol = m_b / 2200.0;
        let h = (vol / 4.5).cbrt();
        let (l, w) = (3.0 * h, 1.5 * h);
        let shield = if multirotor { 1.0 } else { skin_pass };
        for (a, b, e1, e2) in [
            (w, h, [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
            (l, h, [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            (l, w, [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ] {
            push(
                Part::Battery,
                Shape::Plate { a, b, e1, e2 },
                MaterialClass::Metal,
                1.0,
                shield,
            );
        }
    }

    Airframe {
        prims,
        skin,
        motor_d_m: if multirotor || p.drone_type == DroneType::VtolQuadplane {
            d_lift
        } else {
            d_cruise
        },
        n_motors,
    }
}

// ------------------------------------------------------------------------------------------
// Açı taraması
// ------------------------------------------------------------------------------------------

/// Bir bant için azimut taraması sonucu.
#[derive(Clone, Debug)]
pub struct Sweep {
    pub band: Band,
    pub az_deg: Vec<f64>,
    /// Toplam σ [m²], her azimut için.
    pub sigma: Vec<f64>,
    /// Parça grubu başına azimut ortalaması σ [m²] (yalnız sıfır olmayanlar).
    pub by_part: Vec<(Part, f64)>,
    /// Tüm azimutların doğrusal ortalaması [m²].
    pub mean: f64,
    pub max: f64,
    /// Burun ±30° ortalaması ve yan (90° ± 30°, iki taraf) ortalaması [m²].
    pub front: f64,
    pub side: f64,
}

impl Sweep {
    pub fn mean_dbsm(&self) -> f64 {
        to_db(self.mean)
    }
}

/// `n_az` eşit aralıklı azimutta (0…360°) toplam RCS.
pub fn sweep(af: &Airframe, band: Band, s: &RcsSettings, n_az: usize) -> Sweep {
    let n_az = n_az.max(4);
    let lambda = band.wavelength_m();
    let el = if s.elevation_deg.is_finite() {
        s.elevation_deg.clamp(-89.0, 89.0)
    } else {
        0.0
    };
    let mut az_deg = Vec::with_capacity(n_az + 1);
    let mut sigma = Vec::with_capacity(n_az + 1);
    let mut part_sum = [0.0f64; Part::ALL.len()];
    let (mut front, mut nf, mut side, mut ns) = (0.0, 0usize, 0.0, 0usize);
    for i in 0..n_az {
        let az = 360.0 * i as f64 / n_az as f64;
        let d = look_dir(az, el);
        let mut tot = 0.0;
        for pr in &af.prims {
            let v = primitive_rcs(pr, lambda, d, s);
            tot += v;
            if let Some(k) = Part::ALL.iter().position(|q| *q == pr.part) {
                part_sum[k] += v;
            }
        }
        let a = if az > 180.0 { 360.0 - az } else { az };
        if a <= 30.0 {
            front += tot;
            nf += 1;
        }
        if (60.0..=120.0).contains(&a) {
            side += tot;
            ns += 1;
        }
        az_deg.push(az);
        sigma.push(tot);
    }
    // Kutup çiziminde kapalı eğri için ilk nokta sona eklenir.
    az_deg.push(360.0);
    sigma.push(sigma[0]);
    let mean = sigma[..n_az].iter().sum::<f64>() / n_az as f64;
    let max = sigma.iter().cloned().fold(0.0, f64::max);
    let by_part = Part::ALL
        .iter()
        .zip(part_sum)
        .filter(|(_, v)| *v > 0.0)
        .map(|(p, v)| (*p, v / n_az as f64))
        .collect();
    Sweep {
        band,
        az_deg,
        sigma,
        by_part,
        mean,
        max,
        front: front / nf.max(1) as f64,
        side: side / ns.max(1) as f64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signature::{from_db, to_db};

    fn near(a: f64, b: f64, rel: f64) {
        assert!(((a - b) / b).abs() < rel, "{a} vs {b}");
    }

    #[test]
    fn sphere_matches_textbook() {
        // Optik bölge: a = 0.5 m, X bandı (ka ≈ 105) → πa² = 0.785 m² (Knott Tablo 6.1).
        let l = Band::X.wavelength_m();
        near(sphere_rcs(0.5, l), PI * 0.25, 1e-12);
        // Rayleigh bölgesi: ka = 0.1 → σ/πa² = 9·1e-4.
        let a = 0.1 * l / (2.0 * PI);
        near(sphere_rcs(a, l), 9.0e-4 * PI * a * a, 1e-9);
        // Açıdan bağımsız.
        let sh = Shape::Sphere { radius: 0.5 };
        near(shape_rcs(&sh, l, look_dir(37.0, 12.0)), PI * 0.25, 1e-12);
    }

    #[test]
    fn flat_plate_matches_textbook() {
        // 1 m² kare plaka, 10 GHz (λ = 0.02998 m): σ = 4πA²/λ² ≈ 13 982 m² ≈ 41.5 dBsm.
        let l = Band::X.wavelength_m();
        let s = plate_rcs_normal(1.0, l);
        near(s, 4.0 * PI / (l * l), 1e-12);
        near(s, 13_982.0, 0.002);
        assert!((to_db(s) - 41.455).abs() < 0.01, "{}", to_db(s));
        // Dik gelişte genel formül tepe değerini verir; eğik gelişte düşer.
        let sh = Shape::Plate {
            a: 1.0,
            b: 1.0,
            e1: [1.0, 0.0, 0.0],
            e2: [0.0, 1.0, 0.0],
        };
        near(shape_rcs(&sh, l, [0.0, 0.0, 1.0]), s, 1e-12);
        let off = shape_rcs(&sh, l, unit([0.2, 0.0, 1.0]));
        assert!(off < 1e-3 * s, "eğik gelişte düşmeli: {off}");
        // Zarf: x = 10 → 1/(2·100).
        near(sinc2_envelope(10.0), 0.005, 1e-12);
    }

    #[test]
    fn cylinder_broadside_matches_textbook() {
        // a = 0.1 m, L = 1 m, λ = 0.03 m: σ = 2π·0.1·1/0.03 ≈ 20.94 m².
        near(cylinder_rcs_broadside(0.1, 1.0, 0.03), 20.944, 1e-3);
        near(cylinder_rcs(0.1, 1.0, 0.03, 0.0), 20.944, 1e-3);
        assert!(cylinder_rcs(0.1, 1.0, 0.03, 0.3) < 0.01 * 20.944);
    }

    #[test]
    fn dbsm_conversion() {
        assert!((to_db(1.0)).abs() < 1e-12);
        assert!((to_db(0.01) + 20.0).abs() < 1e-9);
        assert!((to_db(10.0) - 10.0).abs() < 1e-9);
        near(from_db(-13.0), 0.050_118_7, 1e-6);
        near(from_db(to_db(0.0042)), 0.0042, 1e-12);
    }

    #[test]
    fn range_scales_with_fourth_root() {
        near(range_ratio(16.0, 1.0), 2.0, 1e-12);
        near(range_ratio(0.01, 1.0), 0.316_227_8, 1e-6);
        near(range_ratio(1e-4, 1.0), 0.1, 1e-12);
        assert_eq!(range_ratio(0.0, 1.0), 0.0);
    }

    #[test]
    fn dielectric_reflectivity_order() {
        let s = RcsSettings::default();
        let r = |c| s.refl(c);
        assert!(r(MaterialClass::Metal) > r(MaterialClass::Carbon));
        assert!(r(MaterialClass::Carbon) > r(MaterialClass::Fibreglass));
        assert!(r(MaterialClass::Fibreglass) > r(MaterialClass::Plastic));
        assert!(r(MaterialClass::Plastic) > r(MaterialClass::Foam));
        near(gamma2_normal(4.0), 1.0 / 9.0, 1e-12);
    }

    #[test]
    fn rotor_average_is_finite_and_positive() {
        let sh = Shape::Rotor {
            radius: 0.12,
            chord: 0.02,
            blades: 2,
            pitch_rad: 0.2,
            axis: [0.0, 0.0, 1.0],
        };
        for b in Band::ALL {
            let v = shape_rcs(&sh, b.wavelength_m(), look_dir(30.0, 5.0));
            assert!(v.is_finite() && v > 0.0, "{b:?}: {v}");
        }
    }

    #[test]
    fn carbon_skin_raises_rcs_over_foam() {
        let p = Project::default();
        let m = crate::calc::evaluate(&p, &crate::calc::Library::embedded());
        let mut s = RcsSettings::default();
        s.skin = Some(MaterialClass::Foam);
        // Kabuk parçalarının (gövde, kanat, hücum kenarı, kuyruk) katkısı karbonda çok daha büyük.
        let skin_parts = |s: &RcsSettings| -> f64 {
            sweep(&airframe(&p, &m, s), Band::X, s, 90)
                .by_part
                .iter()
                .filter(|(q, _)| {
                    matches!(
                        q,
                        Part::Fuselage | Part::Wing | Part::LeadingEdge | Part::Tail
                    )
                })
                .map(|(_, v)| v)
                .sum()
        };
        let foam = skin_parts(&s);
        s.skin = Some(MaterialClass::Carbon);
        let carbon = skin_parts(&s);
        assert!(carbon > 100.0 * foam, "carbon {carbon} foam {foam}");
    }
}
