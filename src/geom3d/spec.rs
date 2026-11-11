// SPDX-License-Identifier: AGPL-3.0-or-later
//! `Project` + `Model` → [`AircraftSpec`] dönüşümü (yalnız okuma).
//!
//! Ölçüler ve yerleşim 2D şemadaki (`ui::schematic`) kurallarla aynıdır; geçersiz değerlerde
//! şemadaki gibi makul yedek değerler kullanılır. Hesap motoruna yazılmaz.
use super::aircraft::{
    AircraftSpec, FuselageSpec, PartKind, RodSpec, RotorSpec, SurfaceSpec, fallback_profile,
};
use super::math::Vec3;
use crate::calc::Model;
use crate::calc::geometry::TailKind;
use crate::calc::multirotor::MultirotorLayout;
use crate::project::{DroneType, Project};

/// Görünüme özgü seçenekler (hesabı etkilemez).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewOptions {
    /// Kanat dihedrali [derece] — projede alan yok; yalnız görsel önizleme.
    pub dihedral_deg: f32,
}

impl Default for ViewOptions {
    fn default() -> Self {
        Self { dihedral_deg: 0.0 }
    }
}

fn pos_or(x: f64, min: f64, fallback: f64) -> f32 {
    if x.is_finite() && x > min {
        x as f32
    } else {
        fallback as f32
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
                profile: profile_of(&p.tail_airfoil_v),
                mirrored: true,
            },
        ));
        // İtici pervane (firar kenarı ortası)
        let r = pos_or(p.d_prop, 0.02, 0.2) * 0.5;
        spec.rotors.push(RotorSpec {
            center: Vec3::new(-c_r - 0.03, 0.0, 0.0),
            axis: Vec3::new(-1.0, 0.0, 0.0),
            radius: r,
            motor_radius: (r * 0.18).clamp(0.008, 0.03),
            motor_height: 0.025,
        });
    } else {
        // Gövde: burun x_root_le'de, kuyruk x_root_le - l_fuse'da; kanat gövdenin üst kısmında.
        let z_axis = -d_fuse * 0.22;
        spec.fuselage = Some(FuselageSpec {
            x_nose: x_root_le,
            length: l_fuse,
            diameter: d_fuse,
            z_axis,
        });
        let tail_x = -pos_or(p.l_h, 0.05, 0.7);
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
                spec.surfaces.push((
                    PartKind::Tail,
                    SurfaceSpec {
                        root_le: Vec3::new(tail_x, 0.0, fuse_top_at_tail),
                        semi_span: b_h * 0.5,
                        c_root: c_h,
                        c_tip: c_h * 0.8,
                        sweep_le_deg: 8.0,
                        dihedral_deg: 0.0,
                        profile: tail_prof_h,
                        mirrored: true,
                    },
                ));
                let s_v = pos_or(p.s_v, 0.0, 0.02);
                let x_v = -pos_or(p.l_v, 0.05, -tail_x as f64);
                let (h_v, c_v) = if c_tail > 0.0 {
                    ((s_v / c_tail).clamp(0.03, 1.0), c_tail)
                } else {
                    let hv = (s_v * 1.5).sqrt().max(0.08);
                    (hv, s_v / hv)
                };
                spec.surfaces.push((
                    PartKind::Tail,
                    SurfaceSpec {
                        root_le: Vec3::new(x_v, 0.0, fuse_top_at_tail),
                        semi_span: h_v,
                        c_root: c_v * 1.15,
                        c_tip: c_v * 0.75,
                        sweep_le_deg: 30.0,
                        dihedral_deg: 90.0,
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
                spec.surfaces.push((
                    PartKind::Tail,
                    SurfaceSpec {
                        root_le: Vec3::new(tail_x, 0.0, fuse_top_at_tail),
                        semi_span: semi,
                        c_root: c,
                        c_tip: c * 0.75,
                        sweep_le_deg: 15.0,
                        dihedral_deg: gamma,
                        profile: tail_prof_v,
                        mirrored: true,
                    },
                ));
            }
            TailKind::FlyingWing => {}
        }
        // Çekici pervane burunda (VTOL'de ileri itki pervanesi kuyrukta).
        let r = pos_or(p.d_prop, 0.02, 0.25) * 0.5;
        let motor_r = (d_fuse * 0.3).clamp(0.01, 0.04);
        if vtol {
            spec.rotors.push(RotorSpec {
                center: Vec3::new(x_root_le - l_fuse - 0.03, 0.0, z_axis),
                axis: Vec3::new(-1.0, 0.0, 0.0),
                radius: r,
                motor_radius: motor_r,
                motor_height: 0.025,
            });
        } else {
            spec.rotors.push(RotorSpec {
                center: Vec3::new(x_root_le + 0.03, 0.0, z_axis),
                axis: Vec3::X,
                radius: r,
                motor_radius: motor_r,
                motor_height: 0.025,
            });
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
        });
        if p.mr_coaxial {
            let bottom = *c + Vec3::Z * (z_arm - r_arm - motor_h);
            spec.rotors.push(RotorSpec {
                center: bottom,
                axis: -Vec3::Z,
                radius: d_prop * 0.5,
                motor_radius: motor_r,
                motor_height: motor_h,
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
        let dih = build(&from_project(&p, &m, &ViewOptions { dihedral_deg: 6.0 }));
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
}
