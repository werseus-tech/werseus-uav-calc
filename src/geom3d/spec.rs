// SPDX-License-Identifier: AGPL-3.0-or-later
//! `Project` + `Model` → [`AircraftSpec`] dönüşümü (yalnız okuma).
//!
//! Ölçüler ve yerleşim 2D şemadaki (`ui::schematic`) kurallarla aynıdır; geçersiz değerlerde
//! şemadaki gibi makul yedek değerler kullanılır. Hesap motoruna yazılmaz.
use super::aircraft::{
    AircraftSpec, FuselageSpec, PartKind, RodSpec, RotorSpec, SurfaceSpec, WheelSpec,
    fallback_profile,
};
use super::math::Vec3;
use super::structure::{
    ControlSpec, FuselageStructure, SparSpec, SurfaceStructure, fuselage_radius_at, mid_at,
    thickness_at,
};
use crate::calc::Model;
use crate::calc::geometry::TailKind;
use crate::calc::multirotor::MultirotorLayout;
use crate::project::{DroneType, Project};

/// Görünüme özgü seçenekler (hesabı etkilemez).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct ViewOptions {
    /// Kanat dihedrali [derece] — projede alan yok; yalnız görsel önizleme.
    pub dihedral_deg: f32,
    /// İniş takımı tekerlekleri geçersiz kılma (`None`: önizleme varsayılanı,
    /// `Some(true)`: simülatör pist kalkışı, `Some(false)`: el/mancınık atışı).
    pub wheels_override: Option<bool>,
}

fn pos_or(x: f64, min: f64, fallback: f64) -> f32 {
    if x.is_finite() && x > min {
        x as f32
    } else {
        fallback as f32
    }
}

/// Çok rotorlu/kaldırma pervanesi kanat sayısı (projede yoksa 2).
fn mr_blades(p: &Project) -> u32 {
    p.mr_prop_blades.clamp(2, 6)
}

// --- Yapı varsayılanları -------------------------------------------------------------
// Projede bulunmayan üretim değerleri tek yerde. Kanat/kuyruk deri kalınlığı, kiriş çapı/konumu
// ve gövde et kalınlığı `cad` yapı verisinden okunur; aşağıdakiler henüz paylaşılan bir yapı
// arayüzünde yoktur.
// TODO: take from shared cad structure API
/// Deri kalınlığı [m]; yapı verisinde et kalınlığı 0 (dolu) ise.
const DEFAULT_SKIN_T: f32 = 0.0008;
/// Kaburga aralığı [m] ve kalınlığı [m] (yarı yüzeydeki sayı açıklıktan türetilir).
const RIB_SPACING: f32 = 0.10;
const RIB_T: f32 = 0.002;
/// Menteşe aralığı [m].
const HINGE_GAP: f32 = 0.002;
/// Kontrol yüzeyleri: (menteşe kord oranı, açıklık başı, açıklık sonu).
const AILERON: (f32, f32, f32) = (0.72, 0.55, 0.95);
const ELEVON: (f32, f32, f32) = (0.75, 0.15, 0.95);
const ELEVATOR: (f32, f32, f32) = (0.62, 0.08, 0.97);
const RUDDER: (f32, f32, f32) = (0.60, 0.08, 0.97);
const RUDDERVATOR: (f32, f32, f32) = (0.62, 0.10, 0.97);
/// Gövde çerçeveleri: burundan gövde boyu oranı; kalınlık [m].
const FORMERS: [f32; 5] = [0.06, 0.25, 0.45, 0.65, 0.85];
const FORMER_T: f32 = 0.003;
/// Bölmeler (aviyonik, batarya, yük): burundan gövde boyu oranı aralığı.
const BAYS: [(f32, f32); 3] = [(0.22, 0.34), (0.37, 0.53), (0.56, 0.72)];

fn mm_or(mm: f64, fallback: f32) -> f32 {
    if mm.is_finite() && mm > 0.0 {
        (mm / 1000.0) as f32
    } else {
        fallback
    }
}

/// Yarı yüzey iç yapısı: değerler `cad` yapı parametrelerinden (mm), kaburga sayısı açıklıktan.
fn structure(
    wall_mm: f64,
    spar_od_mm: f64,
    spar_x: f64,
    semi: f32,
    ctl: (f32, f32, f32),
) -> Option<SurfaceStructure> {
    if !(semi.is_finite() && semi > 0.0) {
        return None;
    }
    let od = mm_or(spar_od_mm, 0.0);
    let spar_x = if spar_x.is_finite() {
        spar_x as f32
    } else {
        0.25
    };
    Some(SurfaceStructure {
        skin_t: mm_or(wall_mm, DEFAULT_SKIN_T),
        ribs: ((semi / RIB_SPACING).ceil() as u32).clamp(3, 14),
        rib_t: RIB_T,
        spar: (od > 0.0).then_some(SparSpec { x_frac: spar_x, od }),
        control: Some(ControlSpec {
            hinge_frac: ctl.0,
            f0: ctl.1,
            f1: ctl.2,
            gap: HINGE_GAP,
        }),
    })
}

fn tail_structure(
    p: &Project,
    m: &Model,
    kind: TailKind,
    vertical: bool,
    semi: f32,
    ctl: (f32, f32, f32),
) -> Option<SurfaceStructure> {
    use crate::cad::freecad::TailCadParams;
    let t = if vertical {
        TailCadParams::from_project_vertical(p, m)
    } else {
        TailCadParams::from_project(p, m, kind)
    };
    structure(t.wall_mm, t.spar_od_mm, t.spar_x_frac, semi, ctl)
}

/// Gövde içi bölmeler: her biri iç yarıçapa sığan bir kutu (gövde ekseninde).
fn add_fuselage_bays(spec: &mut AircraftSpec) {
    let Some(f) = spec.fuselage.clone() else {
        return;
    };
    let wall = f.structure.as_ref().map_or(0.0, |s| s.wall);
    for (a, b) in BAYS {
        let (xa, xb) = (f.x_nose - a * f.length, f.x_nose - b * f.length);
        let r_min = (0..=8)
            .map(|i| fuselage_radius_at(&f, xb + (xa - xb) * i as f32 / 8.0))
            .fold(f32::INFINITY, f32::min);
        let r_in = r_min - wall - 0.002;
        if r_in < 0.01 {
            continue;
        }
        spec.bays.push((
            Vec3::new(0.5 * (xa + xb), 0.0, f.z_axis),
            Vec3::new(xa - xb, 1.3 * r_in, 0.9 * r_in),
        ));
    }
}

/// Uçan kanatta bölmeler kök kesitin içinde (kalınlığa sığan iki kutu).
fn add_wing_bays(spec: &mut AircraftSpec, prof: &[[f32; 2]], c_r: f32) {
    for (x0, x1) in [(0.28_f32, 0.46_f32), (0.50, 0.64)] {
        let xm = 0.5 * (x0 + x1);
        let h = 0.7
            * thickness_at(prof, xm)
                .min(thickness_at(prof, x0))
                .min(thickness_at(prof, x1))
            * c_r;
        if h < 0.004 {
            continue;
        }
        spec.bays.push((
            Vec3::new(-xm * c_r, 0.0, mid_at(prof, xm) * c_r),
            Vec3::new((x1 - x0) * c_r, 0.35 * c_r, h),
        ));
    }
}

fn profile_of(p: &crate::cad::AirfoilProfile) -> Vec<[f32; 2]> {
    let v: Vec<[f32; 2]> = p
        .points
        .iter()
        .filter(|q| q[0].is_finite() && q[1].is_finite())
        .map(|q| [q[0] as f32, q[1] as f32])
        .collect();
    if v.len() < 3 { fallback_profile() } else { v }
}

/// Sahne girdilerini projeden üretir.
pub fn from_project(p: &Project, m: &Model, opt: &ViewOptions) -> AircraftSpec {
    match p.drone_type {
        DroneType::Multirotor => multirotor(p, m),
        DroneType::FlyingWing => fixed_wing(p, m, opt, TailKind::FlyingWing, false),
        DroneType::FixedWingConventional => fixed_wing(p, m, opt, TailKind::Conventional, false),
        DroneType::FixedWingVTail => fixed_wing(p, m, opt, TailKind::VTail, false),
        DroneType::VtolQuadplane => fixed_wing(p, m, opt, p.tail_kind, true),
    }
}

fn fixed_wing(
    p: &Project,
    m: &Model,
    opt: &ViewOptions,
    tail: TailKind,
    vtol: bool,
) -> AircraftSpec {
    let flying = tail == TailKind::FlyingWing && p.drone_type == DroneType::FlyingWing;
    let b = pos_or(p.b, 0.2, if flying { 1.2 } else { 1.6 });
    let c_r = pos_or(m.wing.c_r, 0.05, if flying { 0.28 } else { 0.25 });
    let c_t = pos_or(m.wing.c_t, 0.02, if flying { 0.14 } else { 0.15 });
    // Mesh üretici de ±60° ile sınırlar; winglet konumu aynı değeri kullanmalı.
    let sweep = if p.sweep_le_deg.is_finite() {
        (p.sweep_le_deg as f32).clamp(-60.0, 60.0)
    } else {
        0.0
    };
    let dihedral = if opt.dihedral_deg.is_finite() {
        opt.dihedral_deg.clamp(-15.0, 20.0)
    } else {
        0.0
    };
    let wing_profile = profile_of(&p.airfoil);
    let wing_structure = {
        let w = crate::cad::freecad::WingCadParams::from_project(p, m);
        structure(
            w.wall_mm,
            w.spar_od_mm,
            w.spar_x_frac,
            b * 0.5,
            if flying { ELEVON } else { AILERON },
        )
    };
    let mut spec = AircraftSpec::default();
    spec.surfaces.push((
        PartKind::Wing,
        SurfaceSpec {
            root_le: Vec3::ZERO,
            semi_span: b * 0.5,
            c_root: c_r,
            c_tip: c_t,
            sweep_le_deg: sweep,
            dihedral_deg: dihedral,
            twist_tip_deg: 0.0,
            structure: wing_structure.clone(),
            profile: wing_profile.clone(),
            mirrored: true,
        },
    ));

    let x_root_le = pos_or(p.x_root_le, 0.05, 0.20);
    let l_fuse = pos_or(p.l_fuse, 0.3, 0.90);
    let d_fuse = pos_or(p.d_fuse, 0.03, 0.11);

    if flying {
        // Gövde yok; uç winglet'leri (2D şemadaki gibi), dikey ve aynalı.
        let x_le_tip = (b * 0.5) * sweep.to_radians().tan();
        let h = (c_t * 1.1).clamp(0.03, 0.3);
        spec.surfaces.push((
            PartKind::Tail,
            SurfaceSpec {
                root_le: Vec3::new(-x_le_tip, b * 0.5, 0.0),
                semi_span: h,
                c_root: c_t,
                c_tip: c_t * 0.55,
                sweep_le_deg: 35.0,
                dihedral_deg: 90.0,
                twist_tip_deg: 0.0,
                structure: None,
                profile: profile_of(&p.tail_airfoil_v),
                mirrored: true,
            },
        ));
        add_wing_bays(&mut spec, &wing_profile, c_r);
        // İtici pervane (firar kenarı ortası)
        let r = pos_or(p.d_prop, 0.02, 0.2) * 0.5;
        spec.rotors.push(RotorSpec {
            center: Vec3::new(-c_r - 0.03, 0.0, 0.0),
            axis: Vec3::new(-1.0, 0.0, 0.0),
            radius: r,
            motor_radius: (r * 0.18).clamp(0.008, 0.03),
            motor_height: 0.025,
            blades: 2,
        });
    } else {
        // Gövde: burun x_root_le'de, kuyruk x_root_le - l_fuse'da; kanat gövdenin üst kısmında.
        let z_axis = -d_fuse * 0.22;
        spec.fuselage = Some(FuselageSpec {
            x_nose: x_root_le,
            length: l_fuse,
            diameter: d_fuse,
            z_axis,
            structure: Some(FuselageStructure {
                wall: mm_or(crate::cad::freecad::wall_mm_from(p), DEFAULT_SKIN_T),
                formers: FORMERS.to_vec(),
                former_t: FORMER_T,
            }),
        });
        add_fuselage_bays(&mut spec);
        let x_fuse_end = x_root_le - l_fuse;
        let r = pos_or(p.d_prop, 0.02, 0.25) * 0.5;
        let min_clear = 0.10 * r;
        let standoff = 0.030_f32.max(min_clear + 0.005);
        let x_prop = x_fuse_end - standoff;
        let x_tail_aft_limit = x_fuse_end.max(x_prop + min_clear + 0.005);

        let nom_h = -pos_or(p.l_h, 0.05, 0.7);
        let fuse_top_at_tail = z_axis + d_fuse * 0.5 * 0.35;
        let c_tail = pos_or(p.c_tail, 0.02, 0.0);
        let tail_prof_h = profile_of(&p.tail_airfoil_h);
        let tail_prof_v = profile_of(&p.tail_airfoil_v);
        match tail {
            TailKind::Conventional => {
                let s_h = pos_or(p.s_h, 0.0, 0.05);
                let (b_h, c_h) = if c_tail > 0.0 {
                    ((s_h / c_tail).clamp(0.05, 3.0), c_tail)
                } else {
                    let bh = (s_h * 4.0).sqrt().max(0.2);
                    (bh, s_h / bh)
                };
                let s_v = pos_or(p.s_v, 0.0, 0.02);
                let nom_v = -pos_or(p.l_v, 0.05, (-nom_h) as f64);
                let (h_v, c_v) = if c_tail > 0.0 {
                    ((s_v / c_tail).clamp(0.03, 1.0), c_tail)
                } else {
                    let hv = (s_v * 1.5).sqrt().max(0.08);
                    (hv, s_v / hv)
                };
                let (tail_x, x_v) = if vtol {
                    let dx_h = c_h.max((b_h * 0.5) * 8f32.to_radians().tan() + c_h * 0.8);
                    let dx_v = (c_v * 1.15).max(h_v * 30f32.to_radians().tan() + c_v * 0.75);
                    let dx_tail = dx_h.max(dx_v);
                    let x_nom = nom_h.min(nom_v);
                    let x_le = x_nom
                        .max(x_tail_aft_limit + dx_tail)
                        .min((-c_r - 0.01).max(x_nom));
                    (x_le, x_le)
                } else {
                    (nom_h, nom_v)
                };
                spec.surfaces.push((
                    PartKind::Tail,
                    SurfaceSpec {
                        root_le: Vec3::new(tail_x, 0.0, fuse_top_at_tail),
                        semi_span: b_h * 0.5,
                        c_root: c_h,
                        c_tip: c_h * 0.8,
                        sweep_le_deg: 8.0,
                        dihedral_deg: 0.0,
                        twist_tip_deg: 0.0,
                        structure: tail_structure(
                            p,
                            m,
                            TailKind::Conventional,
                            false,
                            b_h * 0.5,
                            ELEVATOR,
                        ),
                        profile: tail_prof_h,
                        mirrored: true,
                    },
                ));
                spec.surfaces.push((
                    PartKind::Tail,
                    SurfaceSpec {
                        root_le: Vec3::new(x_v, 0.0, fuse_top_at_tail),
                        semi_span: h_v,
                        c_root: c_v * 1.15,
                        c_tip: c_v * 0.75,
                        sweep_le_deg: 30.0,
                        dihedral_deg: 90.0,
                        twist_tip_deg: 0.0,
                        structure: tail_structure(p, m, TailKind::Conventional, true, h_v, RUDDER),
                        profile: tail_prof_v,
                        mirrored: false,
                    },
                ));
            }
            TailKind::VTail => {
                let s_vt = pos_or(p.s_vtail, 0.0, 0.06);
                let gamma = if p.gamma_v_deg.is_finite() && p.gamma_v_deg > 0.0 {
                    p.gamma_v_deg as f32
                } else {
                    35.0
                };
                let (semi, c) = if c_tail > 0.0 {
                    ((s_vt / c_tail * 0.5).clamp(0.05, 1.5), c_tail)
                } else {
                    let span = (s_vt * 3.5).sqrt().max(0.22);
                    (span * 0.5, s_vt / span)
                };
                let tail_x = if vtol {
                    let dx_vt = c.max(semi * 15f32.to_radians().tan() + c * 0.75);
                    nom_h
                        .max(x_tail_aft_limit + dx_vt)
                        .min((-c_r - 0.01).max(nom_h))
                } else {
                    nom_h
                };
                spec.surfaces.push((
                    PartKind::Tail,
                    SurfaceSpec {
                        root_le: Vec3::new(tail_x, 0.0, fuse_top_at_tail),
                        semi_span: semi,
                        c_root: c,
                        c_tip: c * 0.75,
                        sweep_le_deg: 15.0,
                        dihedral_deg: gamma,
                        twist_tip_deg: 0.0,
                        structure: tail_structure(p, m, TailKind::VTail, false, semi, RUDDERVATOR),
                        profile: tail_prof_v,
                        mirrored: true,
                    },
                ));
            }
            TailKind::FlyingWing => {
                let x_le_tip = (b * 0.5) * sweep.to_radians().tan();
                let h = (c_t * 1.1).clamp(0.03, 0.3);
                spec.surfaces.push((
                    PartKind::Tail,
                    SurfaceSpec {
                        root_le: Vec3::new(-x_le_tip, b * 0.5, 0.0),
                        semi_span: h,
                        c_root: c_t,
                        c_tip: c_t * 0.55,
                        sweep_le_deg: 35.0,
                        dihedral_deg: 90.0,
                        twist_tip_deg: 0.0,
                        structure: None,
                        profile: tail_prof_v,
                        mirrored: true,
                    },
                ));
            }
        }
        // Çekici pervane burunda (VTOL'de ileri itki pervanesi kuyrukta).
        let motor_r = (d_fuse * 0.3).clamp(0.01, 0.04);
        if vtol {
            spec.rotors.push(RotorSpec {
                center: Vec3::new(x_prop, 0.0, z_axis),
                axis: Vec3::new(-1.0, 0.0, 0.0),
                radius: r,
                motor_radius: motor_r,
                motor_height: (standoff - 0.005).max(0.025),
                blades: 2,
            });
        } else {
            spec.rotors.push(RotorSpec {
                center: Vec3::new(x_root_le + 0.03, 0.0, z_axis),
                axis: Vec3::X,
                radius: r,
                motor_radius: motor_r,
                motor_height: 0.025,
                blades: 2,
            });
        }
    }

    let want_wheels = match opt.wheels_override {
        Some(w) => w,
        None => !flying && !vtol,
    };
    if want_wheels {
        let r_prop = pos_or(p.d_prop, 0.02, 0.25) * 0.5;
        if flying {
            let x_cg = (x_root_le as f64 - m.stab.cg_nose.x) as f32;
            add_flying_wing_gear(&mut spec, c_r, b, r_prop, x_cg);
        } else {
            add_fixed_wing_gear(
                &mut spec,
                (x_root_le, l_fuse, d_fuse),
                -d_fuse * 0.22,
                c_r,
                r_prop,
                opt.wheels_override.is_some(),
            );
        }
    }

    if vtol {
        // İki bom + dört kaldırma pervanesi (2D şemadaki yerleşim).
        let boom_y = b * 0.5 * 0.38;
        let front = 0.15_f32.max(c_r * 0.6);
        let rear = -c_r - 0.25;
        let r_boom = (d_fuse * 0.12).clamp(0.006, 0.02);
        let z = -0.012 - r_boom;
        let r_prop = pos_or(p.mr_d_prop, 0.02, 0.25) * 0.5;
        for side in [1.0_f32, -1.0] {
            spec.rods.push((
                PartKind::Boom,
                RodSpec {
                    a: Vec3::new(front + r_prop * 0.2, side * boom_y, z),
                    b: Vec3::new(rear - r_prop * 0.2, side * boom_y, z),
                    radius: r_boom,
                },
            ));
            for x in [front, rear] {
                spec.rotors.push(RotorSpec {
                    center: Vec3::new(x, side * boom_y, z + r_boom + 0.03),
                    axis: Vec3::Z,
                    radius: r_prop,
                    motor_radius: (r_prop * 0.16).clamp(0.01, 0.035),
                    motor_height: 0.028,
                    blades: mr_blades(p),
                });
            }
        }
    }

    // Ağırlık merkezi ve nötr nokta (burun datumundan kanat kökü datumuna).
    let x_cg = x_root_le as f64 - m.stab.cg_nose.x;
    let y_cg = -m.stab.cg_nose.y;
    let x_np = x_root_le as f64 - m.stab.x_np_nose;
    let z_mark = 0.0;
    // NaN içeren bir değer `AircraftSpec == AircraftSpec` karşılaştırmasını hep yanlış yapar ve
    // sahneyi her karede yeniden üretirdi; bu yüzden sonlu olmayanlar baştan elenir.
    if x_cg.is_finite() && y_cg.is_finite() {
        spec.cg = Some(Vec3::new(x_cg as f32, y_cg as f32, z_mark));
    }
    if x_np.is_finite() {
        spec.np = Some(Vec3::new(x_np as f32, 0.0, z_mark));
    }
    spec
}

/// 3B önizleme sahnesinde itici pervane diskinin kuyruk, bom ve gövdeye olan en küçük mesafeleri [m].
#[derive(Clone, Debug, PartialEq)]
pub struct PusherClearance3d {
    pub min_clearance_m: f32,
    pub tail_clearance_m: Option<f32>,
    pub boom_clearance_m: Option<f32>,
    pub fuselage_clearance_m: Option<f32>,
    pub wing_tail_overlap: bool,
}

impl PusherClearance3d {
    pub fn tail_ok(&self) -> bool {
        !self.wing_tail_overlap
            && self
                .tail_clearance_m
                .is_none_or(|d| d >= self.min_clearance_m - 1e-5)
    }

    pub fn boom_ok(&self) -> bool {
        self.boom_clearance_m
            .is_none_or(|d| d >= self.min_clearance_m - 1e-5)
    }

    pub fn fuselage_ok(&self) -> bool {
        self.fuselage_clearance_m
            .is_none_or(|d| d >= self.min_clearance_m - 1e-5)
    }

    pub fn is_ok(&self) -> bool {
        self.tail_ok() && self.boom_ok() && self.fuselage_ok()
    }
}

fn dist_pt_x_disc_m(pt: Vec3, center: Vec3, radius: f32) -> f32 {
    let dx = (pt.x - center.x).abs();
    let rho = (pt.y - center.y).hypot(pt.z - center.z);
    let dr = (rho - radius).max(0.0);
    dx.hypot(dr)
}

fn mesh_x_disc_clearance_m(m: &super::mesh::Mesh, center: Vec3, radius: f32) -> f32 {
    let mut best = f32::INFINITY;
    for p in &m.positions {
        let v = Vec3::new(p[0], p[1], p[2]);
        best = best.min(dist_pt_x_disc_m(v, center, radius));
    }
    for tri in m.indices.as_chunks::<3>().0 {
        let pts = [
            m.positions[tri[0] as usize],
            m.positions[tri[1] as usize],
            m.positions[tri[2] as usize],
        ];
        let mut hits: Vec<Vec3> = Vec::with_capacity(3);
        for (i, j) in [(0, 1), (1, 2), (2, 0)] {
            let a = Vec3::new(pts[i][0], pts[i][1], pts[i][2]);
            let b = Vec3::new(pts[j][0], pts[j][1], pts[j][2]);
            let dx = b.x - a.x;
            if (a.x - center.x) * (b.x - center.x) <= 0.0 && dx.abs() > 1e-9 {
                let t = ((center.x - a.x) / dx).clamp(0.0, 1.0);
                let hit = a + (b - a) * t;
                best = best.min(dist_pt_x_disc_m(hit, center, radius));
                hits.push(hit);
            }
        }
        if hits.len() >= 2 {
            for k in 0..hits.len() {
                for l in (k + 1)..hits.len() {
                    let ay = hits[k].y - center.y;
                    let az = hits[k].z - center.z;
                    let dy = hits[l].y - hits[k].y;
                    let dz = hits[l].z - hits[k].z;
                    let len2 = dy * dy + dz * dz;
                    if len2 > 1e-12 {
                        let t = (-(ay * dy + az * dz) / len2).clamp(0.0, 1.0);
                        let rho = (ay + t * dy).hypot(az + t * dz);
                        best = best.min((rho - radius).max(0.0));
                    }
                }
            }
        }
    }
    best
}

/// 3B sahnedeki itici pervane diskinin kuyruk, bom ve gövdeyle açıklığını ölçer.
pub fn check_pusher_clearance(spec: &AircraftSpec) -> Option<PusherClearance3d> {
    let pusher = spec.rotors.iter().find(|r| r.axis.x < -0.5)?;
    let c = pusher.center;
    let r = pusher.radius;
    let min_clearance_m = 0.10 * r;

    let wing_te = spec
        .surfaces
        .iter()
        .find(|(k, _)| *k == PartKind::Wing)
        .map(|(_, w)| w.root_le.x - w.c_root);

    let mut min_tail: Option<f32> = None;
    let mut wing_tail_overlap = false;
    for (k, s) in &spec.surfaces {
        if *k != PartKind::Tail {
            continue;
        }
        let mesh = super::aircraft::surface_half(s);
        let mut d = mesh_x_disc_clearance_m(&mesh, c, r);
        if s.mirrored {
            let m2 = super::aircraft::mirror_y(&mesh);
            d = d.min(mesh_x_disc_clearance_m(&m2, c, r));
        }
        if s.root_le.y.abs() < 1e-3 {
            if let Some((bmin, _)) = mesh.bounds() {
                if bmin.x < c.x + min_clearance_m - 1e-5 {
                    d = d.min((bmin.x - c.x).max(0.0));
                }
            }
            if let Some(w_te) = wing_te {
                if s.root_le.x > w_te + 1e-4 {
                    wing_tail_overlap = true;
                }
            }
        }
        min_tail = Some(min_tail.map_or(d, |m| m.min(d)));
    }

    let mut min_boom: Option<f32> = None;
    for (k, rod) in &spec.rods {
        if *k != PartKind::Boom {
            continue;
        }
        for i in 0..=24 {
            let t = i as f32 / 24.0;
            let pt = rod.a + (rod.b - rod.a) * t;
            let d = (dist_pt_x_disc_m(pt, c, r) - rod.radius).max(0.0);
            min_boom = Some(min_boom.map_or(d, |m| m.min(d)));
        }
    }
    for lr in spec.rotors.iter().filter(|rt| rt.axis.z > 0.5) {
        for ir in 1..=6 {
            let rr = lr.radius * (ir as f32) / 6.0;
            for ia in 0..24 {
                let ang = std::f32::consts::TAU * (ia as f32) / 24.0;
                let pt = Vec3::new(
                    lr.center.x + rr * ang.cos(),
                    lr.center.y + rr * ang.sin(),
                    lr.center.z,
                );
                let d = dist_pt_x_disc_m(pt, c, r);
                min_boom = Some(min_boom.map_or(d, |m| m.min(d)));
            }
        }
    }

    let fuselage_clearance_m = spec.fuselage.as_ref().map(|f| {
        let x_end = f.x_nose - f.length;
        if c.x <= x_end {
            x_end - c.x
        } else if c.x >= f.x_nose {
            c.x - f.x_nose
        } else {
            0.0
        }
    });

    Some(PusherClearance3d {
        min_clearance_m,
        tail_clearance_m: min_tail,
        boom_clearance_m: min_boom,
        fuselage_clearance_m,
        wing_tail_overlap,
    })
}

/// Üç tekerlekli sabit iniş takımı: iki ana ayak (kanat altı) + burun ayağı. Ayak boyu,
/// ön pervane ucunun yerden yüksekliğini koruyacak kadardır; projede veri yoktur, görseldir.
/// `fuse` = (burun x, uzunluk, çap).
fn add_fixed_wing_gear(
    spec: &mut AircraftSpec,
    fuse: (f32, f32, f32),
    z_axis: f32,
    c_r: f32,
    r_prop: f32,
    force: bool,
) {
    let (x_root_le, l_fuse, d_fuse) = fuse;
    let wheel_r = (d_fuse * 0.4).clamp(0.015, 0.05);
    let top_z = z_axis - d_fuse * 0.4;
    let mut ground = z_axis - r_prop - 0.03;
    if force {
        ground = ground.min(top_z - wheel_r - 0.02);
    }
    let hub_z = ground + wheel_r;
    if top_z - hub_z < 0.01 {
        return;
    }
    let strut_r = (d_fuse * 0.04).clamp(0.003, 0.007);
    let track = (d_fuse * 1.1).clamp(0.05, 0.25);
    let x_main = -c_r * 0.55;
    for side in [1.0_f32, -1.0] {
        spec.rods.push((
            PartKind::Gear,
            RodSpec {
                a: Vec3::new(x_main, side * d_fuse * 0.2, top_z),
                b: Vec3::new(x_main, side * track, hub_z),
                radius: strut_r,
            },
        ));
        spec.wheels.push(WheelSpec {
            center: Vec3::new(x_main, side * track, hub_z),
            radius: wheel_r,
            width: wheel_r * 0.55,
        });
    }
    let x_nose = x_root_le - l_fuse * 0.15;
    spec.rods.push((
        PartKind::Gear,
        RodSpec {
            a: Vec3::new(x_nose, 0.0, top_z),
            b: Vec3::new(x_nose, 0.0, hub_z),
            radius: strut_r,
        },
    ));
    spec.wheels.push(WheelSpec {
        center: Vec3::new(x_nose, 0.0, hub_z),
        radius: wheel_r * 0.85,
        width: wheel_r * 0.5,
    });
}

/// Uçan kanat için pist kalkışı iniş takımı (üç tekerlekli, arka pervane açıklığı korunur).
fn add_flying_wing_gear(spec: &mut AircraftSpec, c_r: f32, b: f32, r_prop: f32, x_cg: f32) {
    let wheel_r = (c_r * 0.09).clamp(0.015, 0.038);
    let ground = -(r_prop + 0.05).max(0.11);
    let hub_z = ground + wheel_r;
    let top_z = -0.012;
    if top_z - hub_z < 0.01 {
        return;
    }
    let cg = if x_cg.is_finite() {
        x_cg.clamp(-0.8 * c_r, -0.1 * c_r)
    } else {
        -0.35 * c_r
    };
    let strut_r = (c_r * 0.018).clamp(0.003, 0.006);
    let track = (b * 0.11).clamp(0.08, 0.24);
    let x_main = cg - (c_r * 0.22).max(0.04);
    let x_nose = (cg + (c_r * 0.42).max(0.10)).min(-0.01);
    for side in [1.0_f32, -1.0] {
        spec.rods.push((
            PartKind::Gear,
            RodSpec {
                a: Vec3::new(x_main, side * track * 0.25, top_z),
                b: Vec3::new(x_main, side * track, hub_z),
                radius: strut_r,
            },
        ));
        spec.wheels.push(WheelSpec {
            center: Vec3::new(x_main, side * track, hub_z),
            radius: wheel_r,
            width: wheel_r * 0.55,
        });
    }
    spec.rods.push((
        PartKind::Gear,
        RodSpec {
            a: Vec3::new(x_nose, 0.0, top_z),
            b: Vec3::new(x_nose, 0.0, hub_z),
            radius: strut_r,
        },
    ));
    spec.wheels.push(WheelSpec {
        center: Vec3::new(x_nose, 0.0, hub_z),
        radius: wheel_r * 0.85,
        width: wheel_r * 0.5,
    });
}

/// Multirotor kızakları: plakanın altında iki boyuna boru + dört kısa ayak.
fn add_skids(spec: &mut AircraftSpec, r_plate: f32, plate_t: f32) {
    let leg_h = (r_plate * 1.3).clamp(0.03, 0.12);
    let z_plate = -plate_t * 0.5;
    let z_skid = z_plate - leg_h;
    let r = (r_plate * 0.07).clamp(0.002, 0.006);
    for side in [1.0_f32, -1.0] {
        let y = side * r_plate * 0.7;
        spec.rods.push((
            PartKind::Gear,
            RodSpec {
                a: Vec3::new(-r_plate * 1.25, y, z_skid),
                b: Vec3::new(r_plate * 1.25, y, z_skid),
                radius: r,
            },
        ));
        for fx in [-1.0_f32, 1.0] {
            spec.rods.push((
                PartKind::Gear,
                RodSpec {
                    a: Vec3::new(fx * r_plate * 0.5, y, z_plate),
                    b: Vec3::new(fx * r_plate * 0.5, y, z_skid),
                    radius: r,
                },
            ));
        }
    }
}

fn multirotor(p: &Project, m: &Model) -> AircraftSpec {
    let d_prop = pos_or(p.mr_d_prop, 0.02, 0.127);
    let r_plate = pos_or(p.mr_r_plate, 0.01, 0.035);
    let r_arm = if p.mr_arm_d_o.is_finite() && p.mr_arm_d_o > 0.002 {
        (p.mr_arm_d_o as f32 * 0.5).min(0.03)
    } else {
        0.006
    };
    let mut spec = AircraftSpec::default();
    // Motor konumları hesap motorundan (x ileri, y sağ) → görünüm (y sol).
    let coords: Vec<Vec3> =
        m.mr.motor_coords
            .iter()
            .filter(|c| c.0.is_finite() && c.1.is_finite())
            .map(|c| Vec3::new(c.0 as f32, -c.1 as f32, 0.0))
            .collect();
    let plate_t = (r_plate * 0.08).clamp(0.002, 0.006);
    spec.plate = Some((Vec3::ZERO, r_plate, plate_t));
    add_skids(&mut spec, r_plate, plate_t);
    // Elektronik/batarya yığını (görsel ölçek, kütle hesabına katılmaz).
    spec.boxes.push((
        Vec3::new(0.0, 0.0, plate_t * 0.5 + r_plate * 0.28),
        Vec3::new(r_plate * 1.3, r_plate * 0.8, r_plate * 0.5),
    ));
    let z_arm = plate_t * 0.5 + r_arm;
    if p.mr_layout == MultirotorLayout::H && coords.len() == 4 {
        let (fl, fr, rr, rl) = (coords[0], coords[1], coords[2], coords[3]);
        let lift = Vec3::Z * z_arm;
        for (a, b) in [(fl, rl), (fr, rr)] {
            spec.rods.push((
                PartKind::Arm,
                RodSpec {
                    a: a + lift,
                    b: b + lift,
                    radius: r_arm,
                },
            ));
        }
        spec.rods.push((
            PartKind::Arm,
            RodSpec {
                a: (fl + rl) * 0.5 + lift,
                b: (fr + rr) * 0.5 + lift,
                radius: r_arm,
            },
        ));
    } else {
        for c in &coords {
            spec.rods.push((
                PartKind::Arm,
                RodSpec {
                    a: Vec3::new(0.0, 0.0, z_arm),
                    b: *c + Vec3::Z * z_arm,
                    radius: r_arm,
                },
            ));
        }
    }
    let motor_r = (d_prop * 0.11).clamp(0.008, 0.04);
    let motor_h = motor_r * 1.1;
    for c in &coords {
        let top = *c + Vec3::Z * (z_arm + r_arm + motor_h);
        spec.rotors.push(RotorSpec {
            center: top,
            axis: Vec3::Z,
            radius: d_prop * 0.5,
            motor_radius: motor_r,
            motor_height: motor_h,
            blades: mr_blades(p),
        });
        if p.mr_coaxial {
            let bottom = *c + Vec3::Z * (z_arm - r_arm - motor_h);
            spec.rotors.push(RotorSpec {
                center: bottom,
                axis: -Vec3::Z,
                radius: d_prop * 0.5,
                motor_radius: motor_r,
                motor_height: motor_h,
                blades: mr_blades(p),
            });
        }
    }
    if m.mr.cg_x.is_finite() && m.mr.cg_y.is_finite() {
        spec.cg = Some(Vec3::new(m.mr.cg_x as f32, -m.mr.cg_y as f32, z_arm));
    }
    spec
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom3d::aircraft::build;

    fn eval(p: &Project) -> Model {
        crate::calc::evaluate(p, &crate::calc::Library::embedded())
    }

    /// Araç türü başına üçgen bütçesi: Katı biçimde görünenler ve tüm sahne (Yapı biçimi).
    /// Her parçada yüz normali köşe normalleriyle uyumlu mu, dejenere üçgen ne kadar?
    #[test]
    fn scene_winding_agrees_with_normals() {
        for dt in DroneType::ALL {
            let mut p = Project::default();
            p.set_drone_type(dt);
            let m = eval(&p);
            let sc = build(&from_project(&p, &m, &ViewOptions::default()));
            for part in &sc.parts {
                let mm = &part.mesh;
                let (mut bad, mut degen) = (0usize, 0usize);
                for t in mm.indices.as_chunks::<3>().0 {
                    let v = |i: u32| {
                        let q = mm.positions[i as usize];
                        Vec3::new(q[0], q[1], q[2])
                    };
                    let n = |i: u32| {
                        let q = mm.normals[i as usize];
                        Vec3::new(q[0], q[1], q[2])
                    };
                    let f = (v(t[1]) - v(t[0])).cross(v(t[2]) - v(t[0]));
                    if f.length() < 1e-12 {
                        degen += 1;
                    } else if f.dot(n(t[0]) + n(t[1]) + n(t[2])) < 0.0 {
                        bad += 1;
                    }
                }
                println!(
                    "WIND {dt:?} {:?} tris={} bad={bad} degen={degen}",
                    part.kind,
                    mm.triangle_count()
                );
                // Çizim iki yüzlü aydınlatır; yine de sarım büyük ölçüde tutarlı olmalı.
                assert!(
                    bad * 50 <= mm.triangle_count(),
                    "{dt:?} {:?} bad={bad}",
                    part.kind
                );
            }
        }
    }

    #[test]
    fn triangle_budget_per_type() {
        for dt in DroneType::ALL {
            let mut p = Project::default();
            p.set_drone_type(dt);
            let m = eval(&p);
            let sc = build(&from_project(&p, &m, &ViewOptions::default()));
            let inner = |k: PartKind| {
                matches!(
                    k,
                    PartKind::Rib | PartKind::Spar | PartKind::Former | PartKind::Bay
                )
            };
            let solid: usize = sc
                .parts
                .iter()
                .filter(|q| !inner(q.kind))
                .map(|q| q.mesh.triangle_count())
                .sum();
            let all = sc.triangle_count();
            println!("TRIS {dt:?} solid={solid} structure={all}");
            assert!(solid > 500, "{dt:?} {solid}");
            assert!(all < 150_000, "{dt:?} {all}");
        }
    }

    #[test]
    fn every_type_builds_a_valid_scene() {
        for dt in DroneType::ALL {
            let mut p = Project::default();
            p.set_drone_type(dt);
            let m = eval(&p);
            let sc = build(&from_project(&p, &m, &ViewOptions::default()));
            assert!(!sc.is_empty(), "{dt:?}");
            for part in &sc.parts {
                assert!(part.mesh.is_valid(), "{dt:?} {:?}", part.kind);
            }
            assert!(sc.radius().is_finite() && sc.radius() > 0.05);
            assert!(sc.cg.is_some(), "{dt:?} AG yok");
        }
    }

    #[test]
    fn fixed_wing_span_matches_project() {
        let p = Project::default();
        let m = eval(&p);
        let sc = build(&from_project(&p, &m, &ViewOptions::default()));
        let (a, b) = sc.part(PartKind::Wing).unwrap().mesh.bounds().unwrap();
        assert!(((b.y - a.y) as f64 - p.b).abs() < 1e-3);
        assert!(sc.part(PartKind::Body).is_some());
        assert!(sc.part(PartKind::Tail).is_some());
        // NP, AG'nin önünde değil (statik kararlı varsayılan proje): x_np < x_cg (X ileri).
        let (cg, np) = (sc.cg.unwrap(), sc.np.unwrap());
        assert!(np.x <= cg.x + 1e-6, "cg {cg:?} np {np:?}");
    }

    #[test]
    fn multirotor_arms_follow_motor_angles() {
        for (layout, n) in [
            (MultirotorLayout::X, 4u32),
            (MultirotorLayout::Plus, 4),
            (MultirotorLayout::X, 6),
            (MultirotorLayout::X, 8),
        ] {
            let mut p = Project::default();
            p.set_drone_type(DroneType::Multirotor);
            p.mr_layout = layout;
            p.mr_arm_count = n;
            let m = eval(&p);
            let spec = from_project(&p, &m, &ViewOptions::default());
            let arms: Vec<_> = spec
                .rods
                .iter()
                .filter(|(k, _)| *k == PartKind::Arm)
                .collect();
            assert_eq!(arms.len(), m.mr.motor_coords.len());
            for ((_, r), c) in arms.iter().zip(&m.mr.motor_coords) {
                // Kol açısı motor koordinatının açısıyla aynı (y ekseni görünümde ters).
                let ang_arm = r.b.y.atan2(r.b.x);
                let ang_motor = (-c.1 as f32).atan2(c.0 as f32);
                assert!((ang_arm - ang_motor).abs() < 1e-4);
                let len = (r.b.x * r.b.x + r.b.y * r.b.y).sqrt() as f64;
                assert!((len - c.0.hypot(c.1)).abs() < 1e-4);
            }
            if layout == MultirotorLayout::X && n == 4 {
                // X düzeninde ilk kol ±45° civarında.
                let a = arms[0].1.b.y.atan2(arms[0].1.b.x).to_degrees().abs();
                assert!((a - 45.0).abs() < 1.0 || (a - 135.0).abs() < 1.0, "{a}");
            }
            assert_eq!(spec.rotors.len(), n as usize);
        }
    }

    #[test]
    fn vtol_has_booms_and_four_lift_rotors() {
        let mut p = Project::default();
        p.set_drone_type(DroneType::VtolQuadplane);
        let m = eval(&p);
        let spec = from_project(&p, &m, &ViewOptions::default());
        assert_eq!(
            spec.rods
                .iter()
                .filter(|(k, _)| *k == PartKind::Boom)
                .count(),
            2
        );
        assert_eq!(spec.rotors.iter().filter(|r| r.axis == Vec3::Z).count(), 4);
        assert_eq!(spec.rotors.len(), 5);
    }

    #[test]
    fn dihedral_option_raises_tips_only_visually() {
        let p = Project::default();
        let m = eval(&p);
        let flat = build(&from_project(&p, &m, &ViewOptions::default()));
        let dih = build(&from_project(
            &p,
            &m,
            &ViewOptions {
                dihedral_deg: 6.0,
                ..Default::default()
            },
        ));
        let zf = flat
            .part(PartKind::Wing)
            .unwrap()
            .mesh
            .bounds()
            .unwrap()
            .1
            .z;
        let zd = dih.part(PartKind::Wing).unwrap().mesh.bounds().unwrap().1.z;
        assert!(zd > zf + 0.05);
    }

    #[test]
    fn garbage_project_values_fall_back() {
        let mut p = Project::default();
        p.b = f64::NAN;
        p.s = 0.0;
        p.taper = f64::NAN;
        p.l_fuse = -1.0;
        p.d_prop = f64::INFINITY;
        p.sweep_le_deg = f64::NAN;
        p.airfoil.points.clear();
        let m = eval(&p);
        let sc = build(&from_project(&p, &m, &ViewOptions::default()));
        assert!(!sc.is_empty());
        for part in &sc.parts {
            assert!(part.mesh.is_valid());
        }
        let mut q = Project::default();
        q.set_drone_type(DroneType::Multirotor);
        q.mr_d_prop = f64::NAN;
        q.mr_arm_count = 0;
        let m = eval(&q);
        let sc = build(&from_project(&q, &m, &ViewOptions::default()));
        for part in &sc.parts {
            assert!(part.mesh.is_valid());
        }
    }

    /// NaN içeren proje değerleri sahne girdisini kendisiyle eşitsiz yapmamalı; aksi halde
    /// görünüm her karede mesh üretip GPU'ya yüklerdi.
    #[test]
    fn spec_is_stable_and_finite_with_garbage_inputs() {
        for dt in DroneType::ALL {
            let mut p = Project::default();
            p.set_drone_type(dt);
            p.b = f64::NAN;
            p.s = 0.0;
            p.taper = f64::NAN;
            p.sweep_le_deg = 90.0;
            let m = eval(&p);
            let a = from_project(&p, &m, &ViewOptions::default());
            let b = from_project(&p, &m, &ViewOptions::default());
            assert_eq!(a, b, "{dt:?}: spec kendisine eşit olmalı");
            let sc = build(&a);
            assert!(sc.bounds_min.is_finite() && sc.bounds_max.is_finite());
            assert!(sc.radius() < 100.0, "{dt:?}: sahne makul boyutta olmalı");
        }
    }

    fn scene_of(dt: DroneType) -> (Project, crate::geom3d::Scene) {
        let mut p = Project::default();
        p.set_drone_type(dt);
        let m = eval(&p);
        let sc = build(&from_project(&p, &m, &ViewOptions::default()));
        (p, sc)
    }

    #[test]
    fn all_types_have_clean_meshes_within_budget() {
        for dt in DroneType::ALL {
            let (_, sc) = scene_of(dt);
            assert!(!sc.is_empty(), "{dt:?}");
            assert!(
                sc.triangle_count() < 150_000,
                "{dt:?}: {}",
                sc.triangle_count()
            );
            for part in &sc.parts {
                let m = &part.mesh;
                assert!(m.is_valid(), "{dt:?} {:?}", part.kind);
                assert!(m.triangle_count() > 0);
                assert_eq!(m.normals.len(), m.positions.len());
                for n in &m.normals {
                    let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                    assert!(
                        (l - 1.0).abs() < 1e-3,
                        "{dt:?} {:?}: normal length {l}",
                        part.kind
                    );
                }
                for p in &m.positions {
                    assert!(p.iter().all(|v| v.is_finite() && v.abs() < 100.0));
                }
            }
        }
    }

    #[test]
    fn mesh_generation_is_deterministic() {
        for dt in DroneType::ALL {
            let (p, a) = scene_of(dt);
            let m = eval(&p);
            let b = build(&from_project(&p, &m, &ViewOptions::default()));
            assert_eq!(a.parts.len(), b.parts.len());
            for (x, y) in a.parts.iter().zip(&b.parts) {
                assert_eq!(x.kind, y.kind);
                assert_eq!(x.mesh, y.mesh, "{dt:?} {:?}", x.kind);
            }
        }
    }

    #[test]
    fn symmetric_designs_have_symmetric_bounds() {
        for dt in DroneType::ALL {
            let (_, sc) = scene_of(dt);
            let tol = 2e-3;
            assert!(
                (sc.bounds_min.y + sc.bounds_max.y).abs() < tol,
                "{dt:?}: y {} .. {}",
                sc.bounds_min.y,
                sc.bounds_max.y
            );
            if let Some(w) = sc.part(PartKind::Wing) {
                let (a, b) = w.mesh.bounds().unwrap();
                assert!((a.y + b.y).abs() < 1e-4);
            }
        }
    }

    #[test]
    fn wing_chord_and_span_match_design() {
        let p = Project::default();
        let m = eval(&p);
        let spec = from_project(&p, &m, &ViewOptions::default());
        let sc = build(&spec);
        let (a, b) = sc.part(PartKind::Wing).unwrap().mesh.bounds().unwrap();
        assert!(((b.y - a.y) as f64 - p.b).abs() < 1e-3);
        let s = &spec.surfaces[0].1;
        // Ok açısı kadar kayma + uç veteri: x aralığı kök veterinden az değil.
        assert!(b.x - a.x >= s.c_root - 1e-4);
        // Gerçek profil kalınlığı: kök veterinin NACA 2412'ye uygun yüzdesi.
        assert!(b.z - a.z > 0.05 * s.c_root);
    }

    #[test]
    fn gear_follows_vehicle_type() {
        let (_, conv) = scene_of(DroneType::FixedWingConventional);
        assert!(conv.part(PartKind::Gear).is_some());
        assert!(conv.part(PartKind::Wheel).is_some());
        let (_, fw) = scene_of(DroneType::FlyingWing);
        assert!(fw.part(PartKind::Gear).is_none());
        let (_, mr) = scene_of(DroneType::Multirotor);
        assert!(mr.part(PartKind::Gear).is_some());
        assert!(mr.part(PartKind::Wheel).is_none());
        // Tekerlek alt ucu, ön pervane ucunun altında kalır (pervane yere değmez).
        let wheel_min = conv
            .part(PartKind::Wheel)
            .unwrap()
            .mesh
            .bounds()
            .unwrap()
            .0
            .z;
        let prop_min = conv
            .part(PartKind::Rotor)
            .unwrap()
            .mesh
            .bounds()
            .unwrap()
            .0
            .z;
        assert!(wheel_min < prop_min);
    }

    #[test]
    fn blade_count_comes_from_design() {
        let mut p = Project::default();
        p.set_drone_type(DroneType::Multirotor);
        let m = eval(&p);
        p.mr_prop_blades = 2;
        let two = build(&from_project(&p, &m, &ViewOptions::default()));
        p.mr_prop_blades = 3;
        let three = build(&from_project(&p, &m, &ViewOptions::default()));
        let t2 = two.part(PartKind::Prop).unwrap().mesh.triangle_count();
        let t3 = three.part(PartKind::Prop).unwrap().mesh.triangle_count();
        assert!(t3 > t2);
    }

    fn skin_bounds(sc: &crate::geom3d::Scene) -> (Vec3, Vec3) {
        let mut mn = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
        let mut mx = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
        for part in &sc.parts {
            if matches!(
                part.kind,
                PartKind::Wing | PartKind::Tail | PartKind::Body | PartKind::Control
            ) {
                let (a, b) = part.mesh.bounds().unwrap();
                mn = mn.min(a);
                mx = mx.max(b);
            }
        }
        (mn, mx)
    }

    #[test]
    fn structure_meshes_exist_for_fixed_wing_flying_wing_and_vtol() {
        for dt in [
            DroneType::FixedWingConventional,
            DroneType::FixedWingVTail,
            DroneType::FlyingWing,
            DroneType::VtolQuadplane,
        ] {
            let (_, sc) = scene_of(dt);
            for kind in [
                PartKind::Rib,
                PartKind::Spar,
                PartKind::Control,
                PartKind::Bay,
            ] {
                assert!(sc.part(kind).is_some(), "{dt:?}: {kind:?} yok");
            }
            if dt != DroneType::FlyingWing {
                assert!(sc.part(PartKind::Former).is_some(), "{dt:?}: çerçeve yok");
            }
            // Üretim yapısı kapalı dış yüzeyin içinde kalır.
            let (smin, smax) = skin_bounds(&sc);
            for kind in [
                PartKind::Rib,
                PartKind::Spar,
                PartKind::Former,
                PartKind::Bay,
            ] {
                if let Some(p) = sc.part(kind) {
                    let (a, b) = p.mesh.bounds().unwrap();
                    let t = 1e-4;
                    assert!(
                        a.x >= smin.x - t
                            && b.x <= smax.x + t
                            && a.y >= smin.y - t
                            && b.y <= smax.y + t
                            && a.z >= smin.z - t
                            && b.z <= smax.z + t,
                        "{dt:?} {kind:?}: {a:?} {b:?} / {smin:?} {smax:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn wing_rib_count_follows_the_spec() {
        let p = Project::default();
        let m = eval(&p);
        let spec = from_project(&p, &m, &ViewOptions::default());
        let n = spec.surfaces[0].1.structure.as_ref().unwrap().ribs as usize;
        let sc = build(&spec);
        // Kanat kaburgaları (aynalı): y > 0 yarısında kümeler.
        let ribs = &sc.part(PartKind::Rib).unwrap().mesh;
        let mut ys: Vec<f32> = ribs
            .positions
            .iter()
            .filter(|q| q[0] > -0.6 && q[1] > 0.0)
            .map(|q| q[1])
            .collect();
        ys.sort_by(f32::total_cmp);
        let mut clusters = 0;
        let mut last = f32::NEG_INFINITY;
        for y in ys {
            if y - last > 0.004 {
                clusters += 1;
            }
            last = y;
        }
        assert_eq!(clusters, n);
    }

    #[test]
    fn vtol_pusher_disc_clears_tail_and_booms_for_all_tail_kinds() {
        let mut p = crate::project::load_example("vtol_quadplane.json").unwrap();
        for kind in TailKind::ALL {
            p.tail_kind = kind;
            let m = eval(&p);
            let spec = from_project(&p, &m, &ViewOptions::default());
            let clr = check_pusher_clearance(&spec).expect("pusher rotor");
            assert!(clr.is_ok(), "{kind:?}: {clr:?}");
            let sc = build(&spec);
            assert!(!sc.is_empty(), "{kind:?}");
            for part in &sc.parts {
                assert!(part.mesh.is_valid(), "{kind:?} {:?}", part.kind);
            }
        }

        // Short fuselage makes tail clearance impossible -> detected cleanly.
        p.tail_kind = TailKind::Conventional;
        p.l_fuse = 0.40;
        let m = eval(&p);
        let spec = from_project(&p, &m, &ViewOptions::default());
        let clr = check_pusher_clearance(&spec).unwrap();
        assert!(!clr.tail_ok(), "{clr:?}");
    }
}
