// SPDX-License-Identifier: AGPL-3.0-or-later
//! Validation of the beam model against closed-form solutions and the existing structure steps.

use super::analysis::*;
use super::beam::*;
use super::material;
use crate::calc::{Library, Model, evaluate};
use crate::project::{self, DroneType, Project};
use std::f64::consts::PI;

const BETA_L: [f64; 3] = [1.875_104_068_7, 4.694_091_133_0, 7.854_757_438_2];

fn rel(a: f64, b: f64) -> f64 {
    ((a - b) / b).abs()
}

fn uniform(n_el: usize) -> BeamModel {
    // 16 x 14 mm carbon tube, 0.22 m.
    let i = PI / 64.0 * (0.016f64.powi(4) - 0.014f64.powi(4));
    let a = PI / 4.0 * (0.016f64.powi(2) - 0.014f64.powi(2));
    BeamModel {
        length: 0.22,
        n_el,
        ei: 120e9 * i,
        mu: 1550.0 * a,
        point_masses: Vec::new(),
    }
}

fn closed_form(b: &BeamModel, k: usize) -> f64 {
    BETA_L[k].powi(2) / (2.0 * PI * b.length.powi(2)) * (b.ei / b.mu).sqrt()
}

#[test]
fn cantilever_frequencies_match_closed_form() {
    let b = uniform(20);
    let modes = b.modes(3);
    assert_eq!(modes.len(), 3);
    for (k, md) in modes.iter().enumerate() {
        let e = rel(md.freq_hz, closed_form(&b, k));
        assert!(e < 0.01, "mode {} error {e}", k + 1);
    }
    // Mode shapes: root fixed, mode 1 has no sign change, mode 2 one node.
    assert_eq!(modes[0].shape[0], 0.0);
    let sign_changes = |s: &[f64]| s.windows(2).filter(|w| w[0] * w[1] < 0.0).count();
    assert_eq!(sign_changes(&modes[0].shape[1..]), 0);
    assert_eq!(sign_changes(&modes[1].shape[1..]), 1);
    assert_eq!(sign_changes(&modes[2].shape[1..]), 2);
}

/// Exact first frequency of a cantilever with tip mass M:
/// 1 + cos x cosh x + r x (cos x sinh x - sin x cosh x) = 0, r = M / (mu L), x = beta L.
fn tip_mass_exact(b: &BeamModel, m_tip: f64) -> f64 {
    let r = m_tip / (b.mu * b.length);
    let g = |x: f64| 1.0 + x.cos() * x.cosh() + r * x * (x.cos() * x.sinh() - x.sin() * x.cosh());
    let (mut lo, mut hi) = (1e-3, BETA_L[0]);
    assert!(g(lo) * g(hi) <= 0.0);
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if g(lo) * g(mid) <= 0.0 {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    let x = 0.5 * (lo + hi);
    x * x / (2.0 * PI * b.length.powi(2)) * (b.ei / b.mu).sqrt()
}

#[test]
fn cantilever_with_tip_mass_matches_exact_solution() {
    for m_tip in [0.01, 0.06, 0.3] {
        let mut b = uniform(20);
        b.point_masses.push(PointMass {
            x: b.length,
            mass: m_tip,
            rot_inertia: 0.0,
        });
        let f = b.modes(1)[0].freq_hz;
        let exact = tip_mass_exact(&b, m_tip);
        assert!(rel(f, exact) < 0.01, "M={m_tip}: {f} vs {exact}");
        // Rayleigh estimate used by the arm structure step (0.236 m_arm) is within 2 %.
        let rayleigh = (3.0 * b.ei / ((m_tip + 0.236 * b.mu * b.length) * b.length.powi(3))).sqrt()
            / (2.0 * PI);
        assert!(rel(f, rayleigh) < 0.02, "Rayleigh {rayleigh} vs {f}");
    }
}

#[test]
fn static_tip_deflection_and_moment() {
    let b = uniform(10);
    let p = 15.0;
    let r = b.solve_static(
        &[PointForce {
            x: b.length,
            force: p,
        }],
        &|_| 0.0,
    );
    let exact = p * b.length.powi(3) / (3.0 * b.ei);
    assert!(rel(r.max_deflection, exact) < 1e-9);
    assert!(rel(r.max_moment, p * b.length) < 1e-9);
    assert!(rel(r.moment[0], p * b.length) < 1e-9);
    // Uniform load: q L^4 / 8 EI, root moment q L^2 / 2 (exact with consistent loads).
    let q = 40.0;
    let r = b.solve_static(&[], &|_| q);
    assert!(rel(r.max_deflection, q * b.length.powi(4) / (8.0 * b.ei)) < 1e-9);
    assert!(rel(r.max_moment, q * b.length.powi(2) / 2.0) < 1e-9);
}

#[test]
fn mesh_convergence_is_monotonic() {
    let mut prev = f64::INFINITY;
    for n in [1, 2, 4, 8, 16, 32] {
        let b = uniform(n);
        let modes = b.modes(2);
        let e = rel(modes[0].freq_hz, closed_form(&b, 0));
        assert!(e <= prev + 1e-12, "n={n}: {e} > {prev}");
        prev = e;
        if n >= 4 {
            assert!(rel(modes[1].freq_hz, closed_form(&b, 1)) < 0.01, "n={n}");
        }
    }
    assert!(prev < 1e-5, "32 elements: {prev}");
    // The consistent-mass FE frequency approaches from above.
    let b = uniform(4);
    assert!(b.modes(1)[0].freq_hz >= closed_form(&b, 0));
}

#[test]
fn torsion_matches_closed_form() {
    let s = ShaftModel {
        length: 0.3,
        n_el: 30,
        gj: 50.0,
        rho_ip: 2e-5,
        tip_inertia: 0.0,
    };
    let f = s.frequencies(2);
    assert_eq!(f.len(), 2);
    let with_tip = ShaftModel {
        tip_inertia: 1e-4,
        ..s.clone()
    };
    assert!(with_tip.frequencies(1)[0] < f[0]);
    for (k, fk) in f.iter().enumerate() {
        let exact =
            (2 * k + 1) as f64 * PI / (2.0 * s.length) * (s.gj / s.rho_ip).sqrt() / (2.0 * PI);
        assert!(rel(*fk, exact) < 0.01, "torsion {k}: {fk} vs {exact}");
    }
}

#[test]
fn resonance_flag_when_band_crosses_mode() {
    let modes = vec![ModeResult {
        kind: ModeKind::Bending,
        index: 1,
        freq_hz: 120.0,
        shape: Vec::new(),
    }];
    let band = |lo: f64, hi: f64, dwell: f64| Excitation {
        source: ExcitationSource::LiftRotor,
        rpm_lo: lo,
        rpm_hi: hi,
        rpm_dwell: dwell,
        blades: 2,
        direct: true,
    };
    // 1x crossing at 7200 RPM = hover: high risk; blade-pass crossing at 3600 RPM lies in
    // the operating range widened by the margin (4000 x 0.85 = 3400): caution.
    let r = find_resonances(&modes, &[band(4000.0, 12000.0, 7200.0)], 0.15);
    assert_eq!(r.len(), 2);
    let rev = r.iter().find(|x| x.harmonic == Harmonic::Rev1).unwrap();
    assert_eq!(rev.risk, Risk::High);
    assert!((rev.rpm_cross - 7200.0).abs() < 1e-9);
    let bpf = r
        .iter()
        .find(|x| x.harmonic == Harmonic::BladePass)
        .unwrap();
    assert_eq!(bpf.risk, Risk::Caution);
    assert!((bpf.rpm_cross - 3600.0).abs() < 1e-9);
    // Band crosses the mode but away from hover: caution only.
    let r = find_resonances(&modes, &[band(6000.0, 12000.0, 10000.0)], 0.15);
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].risk, Risk::Caution);
    // Far away: no flag.
    assert!(find_resonances(&modes, &[band(600.0, 900.0, 800.0)], 0.15).is_empty());
}

fn eval(p: &Project) -> Model {
    evaluate(p, &Library::embedded())
}

fn samples() -> Vec<Project> {
    let mut v: Vec<Project> = [
        project::EX_FIXED_WING,
        project::EX_FLYING_WING,
        project::EX_QUAD_250,
        project::EX_QUAD_275,
        project::EX_HEXA_550,
        project::EX_VTOL,
    ]
    .iter()
    .map(|s| Project::from_json(s).expect("example parses"))
    .collect();
    for dt in DroneType::ALL {
        let mut p = Project::default();
        p.drone_type = dt;
        v.push(p);
    }
    v
}

#[test]
fn consistent_with_existing_arm_calculation() {
    for p in samples() {
        if !matches!(
            p.drone_type,
            DroneType::Multirotor | DroneType::VtolQuadplane
        ) {
            continue;
        }
        let m = eval(&p);
        let s = FeaSettings {
            n_el: 40,
            ..FeaSettings::default()
        };
        let kind = MemberKind::Arm;
        let a = analyse_arm(&p, &m, &s, kind);
        assert!(a.valid, "{}", p.name);
        let st = &m.mr_st;
        let thrust = a
            .load_cases
            .iter()
            .find(|c| c.case == LoadCase::MaxThrust)
            .unwrap();
        // Ultimate stress = limit stress x factor of safety.
        assert!(rel(thrust.max_stress * thrust.required_sf, st.sigma_max) < 1e-6);
        assert!(rel(thrust.safety_factor / thrust.required_sf - 1.0, st.ms) < 1e-6);
        // Deflection: existing step reports it under the unfactored max thrust.
        assert!(rel(thrust.max_deflection, st.deflection_max) < 1e-6);
        // First bending frequency vs the Rayleigh estimate of the arm step.
        let f1 = a.modes[0].freq_hz;
        assert!(
            rel(f1, st.f_natural) < 0.02,
            "{}: {f1} vs {}",
            p.name,
            st.f_natural
        );
        // High risk on bending mode 1 with the default 20 % margin is the same criterion as
        // the arm step's hover resonance warning (up to the small frequency difference).
        let high = |h: Harmonic| {
            a.resonances.iter().any(|r| {
                r.mode_kind == ModeKind::Bending
                    && r.mode_index == 1
                    && r.harmonic == h
                    && r.risk == Risk::High
            })
        };
        let near_edge = |f_exc: f64| (((f1 - f_exc) / f_exc).abs() - 0.2).abs() < 0.03;
        if !near_edge(st.f_rot_hover) {
            assert_eq!(high(Harmonic::Rev1), st.resonance_rot_warn, "{}", p.name);
        }
        if !near_edge(st.f_bpf_hover) && p.mr_prop_blades > 1 {
            assert_eq!(
                high(Harmonic::BladePass),
                st.resonance_bpf_warn,
                "{}",
                p.name
            );
        }
    }
}

#[test]
fn resonance_matches_existing_warning_when_rpm_is_tuned() {
    let mut p = Project::from_json(project::EX_QUAD_250).unwrap();
    let m0 = eval(&p);
    p.mr_hover_rpm_override = m0.mr_st.f_natural * 60.0;
    let m = eval(&p);
    assert!(m.mr_st.resonance_rot_warn);
    let a = analyse_arm(&p, &m, &FeaSettings::default(), MemberKind::Arm);
    assert!(a.resonances.iter().any(|r| r.mode_kind == ModeKind::Bending
        && r.mode_index == 1
        && r.harmonic == Harmonic::Rev1
        && r.risk == Risk::High));
}

#[test]
fn wing_bending_consistent_with_wing_structure_step() {
    for p in samples() {
        if p.drone_type == DroneType::Multirotor {
            continue;
        }
        let m = eval(&p);
        let w = analyse_wing(&p, &m, &FeaSettings::default());
        assert!(w.valid, "{}", p.name);
        let b = w
            .load_cases
            .iter()
            .find(|c| c.case == LoadCase::WingBending)
            .unwrap();
        assert!(rel(b.max_moment, m.st.m_b_limit) < 0.005, "{}", p.name);
        assert!(
            rel(b.max_stress * b.required_sf, m.st.sigma_max) < 0.005,
            "{}",
            p.name
        );
    }
}

#[test]
fn applicability_per_vehicle_type() {
    for p in samples() {
        let m = eval(&p);
        let r = analyse(&p, &m, &FeaSettings::default());
        let kinds: Vec<MemberKind> = r.members.iter().map(|x| x.kind).collect();
        match p.drone_type {
            DroneType::Multirotor => {
                assert_eq!(kinds, vec![MemberKind::Arm]);
                assert_eq!(r.not_applicable, vec![NotApplicable::NoWing]);
            }
            DroneType::VtolQuadplane => {
                assert_eq!(kinds, vec![MemberKind::Boom, MemberKind::WingHalf]);
                assert!(r.not_applicable.is_empty());
            }
            _ => {
                assert_eq!(kinds, vec![MemberKind::WingHalf]);
                assert_eq!(r.not_applicable, vec![NotApplicable::NoArms]);
                let wing = &r.members[0];
                let thrust = wing
                    .load_cases
                    .iter()
                    .find(|c| c.case == LoadCase::MaxThrust)
                    .unwrap();
                assert!(!thrust.applies);
            }
        }
    }
}

fn assert_finite_report(r: &FeaReport, name: &str) {
    for mbr in &r.members {
        assert!(mbr.length.is_finite(), "{name}");
        for md in &mbr.modes {
            assert!(md.freq_hz.is_finite() && md.freq_hz > 0.0, "{name}");
            assert!(md.shape.iter().all(|v| v.is_finite()), "{name}");
        }
        for c in &mbr.load_cases {
            for v in [c.max_moment, c.max_stress, c.max_deflection, c.required_sf] {
                assert!(v.is_finite(), "{name}: {c:?}");
            }
            assert!(!c.safety_factor.is_nan(), "{name}");
        }
        for e in &mbr.excitations {
            assert!(e.rpm_lo.is_finite() && e.rpm_hi >= e.rpm_lo, "{name}");
        }
    }
}

#[test]
fn no_nan_for_sample_projects_and_settings() {
    for p in samples() {
        let m = eval(&p);
        for s in [
            FeaSettings::default(),
            FeaSettings {
                n_el: 2,
                landing_g: 8.0,
                arm_material: Some(super::material::StructMaterial::Pla),
                arm_section: ArmSection::Plate {
                    width: 0.02,
                    thickness: 0.004,
                },
                wing_material: Some(super::material::StructMaterial::BalsaMedium),
                ..FeaSettings::default()
            },
        ] {
            let r = analyse(&p, &m, &s);
            assert!(!r.members.is_empty());
            assert!(r.members.iter().all(|x| x.valid), "{}", p.name);
            assert_finite_report(&r, &p.name);
        }
    }
}

#[test]
fn invalid_inputs_give_no_numbers() {
    let mut p = Project::from_json(project::EX_QUAD_250).unwrap();
    p.mr_arm_d_i = p.mr_arm_d_o; // zero wall
    let m = eval(&p);
    let r = analyse(&p, &m, &FeaSettings::default());
    assert!(!r.members[0].valid);
    assert!(r.members[0].modes.is_empty() && r.members[0].load_cases.is_empty());
    let mut p = Project::from_json(project::EX_FIXED_WING).unwrap();
    p.b = f64::NAN;
    let m = eval(&p);
    let r = analyse(&p, &m, &FeaSettings::default());
    assert!(!r.members[0].valid);
}

#[test]
fn interior_point_force_keeps_its_position_on_any_mesh() {
    // A load at x = 0.2 L gives the root moment F x on any mesh (it used to snap to the root
    // node for coarse meshes and to the nearest node otherwise).
    let f = 15.0;
    for n in [2, 3, 5, 7, 20] {
        let b = uniform(n);
        let x = 0.2 * b.length;
        let r = b.solve_static(&[PointForce { x, force: f }], &|_| 0.0);
        assert!(rel(r.max_moment, f * x) < 1e-9, "n={n}: {}", r.max_moment);
        // Tip deflection of a cantilever with an interior load: F x^2 (3L - x) / 6EI.
        let exact_tip = f * x * x * (3.0 * b.length - x) / (6.0 * b.ei);
        assert!(rel(*r.w.last().unwrap(), exact_tip) < 1e-6, "n={n}");
    }
}

#[test]
fn interior_point_mass_frequency_is_continuous_in_the_mesh() {
    let mk = |n: usize| {
        let mut b = uniform(n);
        b.point_masses = vec![PointMass {
            x: 0.4 * b.length,
            mass: 0.2,
            rot_inertia: 0.0,
        }];
        b.modes(1)[0].freq_hz
    };
    let reference = mk(80);
    for n in [10, 11, 12, 13, 21] {
        assert!(
            rel(mk(n), reference) < 0.01,
            "n={n}: {} vs {reference}",
            mk(n)
        );
    }
}

#[test]
fn non_finite_load_is_not_reported_as_zero_stress() {
    let b = uniform(10);
    let r = b.solve_static(
        &[PointForce {
            x: b.length,
            force: f64::NAN,
        }],
        &|_| 0.0,
    );
    assert!(r.w.is_empty());
    let r = b.solve_static(&[], &|_| f64::NAN);
    assert!(r.w.is_empty());
}

#[test]
fn missing_loads_make_the_members_invalid() {
    let p = Project::from_json(project::EX_QUAD_250).unwrap();
    let mut m = eval(&p);
    m.mr_st.f_thrust_max = f64::NAN;
    assert!(!analyse_arm(&p, &m, &FeaSettings::default(), MemberKind::Arm).valid);
    m.mr_st.f_thrust_max = 0.0;
    assert!(!analyse_arm(&p, &m, &FeaSettings::default(), MemberKind::Arm).valid);
    let p = Project::from_json(project::EX_FIXED_WING).unwrap();
    let mut m = eval(&p);
    m.weight.w = f64::NAN;
    assert!(!analyse_wing(&p, &m, &FeaSettings::default()).valid);
    m.weight.w = 0.0;
    assert!(!analyse_wing(&p, &m, &FeaSettings::default()).valid);
    // A VTOL wing needs the lift motor thrust for its boom case.
    let p = Project::from_json(project::EX_VTOL).unwrap();
    let mut m = eval(&p);
    m.mr_st.f_thrust_max = f64::NAN;
    assert!(!analyse_wing(&p, &m, &FeaSettings::default()).valid);
}

#[test]
fn vtol_boom_is_shorter_than_the_arm() {
    let p = Project::from_json(project::EX_VTOL).unwrap();
    let m = eval(&p);
    let s = FeaSettings::default();
    let arm = analyse_arm(&p, &m, &s, MemberKind::Arm);
    let boom = analyse_arm(&p, &m, &s, MemberKind::Boom);
    assert!(rel(boom.length * 2f64.sqrt(), arm.length) < 1e-12);
    assert!(boom.modes[0].freq_hz > arm.modes[0].freq_hz);
    let t = |r: &MemberResult| {
        r.load_cases
            .iter()
            .find(|c| c.case == LoadCase::MaxThrust)
            .unwrap()
            .max_stress
    };
    assert!(rel(t(&boom) * 2f64.sqrt(), t(&arm)) < 1e-6);
}

#[test]
fn max_thrust_deflection_matches_the_arm_step() {
    for p in samples() {
        if p.drone_type != DroneType::Multirotor {
            continue;
        }
        let m = eval(&p);
        let a = analyse_arm(&p, &m, &FeaSettings::default(), MemberKind::Arm);
        let c = a
            .load_cases
            .iter()
            .find(|c| c.case == LoadCase::MaxThrust)
            .unwrap();
        assert!(
            rel(c.max_deflection, m.mr_st.deflection_max) < 0.01,
            "{}",
            p.name
        );
    }
}

#[test]
fn wing_material_override_changes_the_wing_mass_per_span() {
    let p = Project::from_json(project::EX_FIXED_WING).unwrap();
    let m = eval(&p);
    let base = analyse_wing(&p, &m, &FeaSettings::default());
    let heavy = analyse_wing(
        &p,
        &m,
        &FeaSettings {
            wing_material: Some(material::StructMaterial::Al7075),
            ..FeaSettings::default()
        },
    );
    assert!(base.valid && heavy.valid);
    // A denser spar must lower the frequency below what the stiffness gain alone gives.
    assert!(heavy.material.rho > base.material.rho);
    let e_ratio = (heavy.material.e / base.material.e).sqrt();
    assert!(heavy.modes[0].freq_hz < base.modes[0].freq_hz * e_ratio * 0.999);
}

#[test]
fn square_section_torsion_constant_covers_solid_and_thin_walls() {
    use crate::calc::mr_structure::MrArmSectionType;
    let d = 0.02;
    let j_solid = section(ArmSection::Project, MrArmSectionType::Square, d, 0.0)[3];
    assert!(rel(j_solid, 0.1406 * d.powi(4)) < 0.05, "{j_solid}");
    let (d_o, d_i) = (0.02, 0.0184);
    let j_thin = section(ArmSection::Project, MrArmSectionType::Square, d_o, d_i)[3];
    let t = 0.5 * (d_o - d_i);
    let a_m = 0.5 * (d_o + d_i);
    assert!(rel(j_thin, a_m.powi(3) * t) < 0.01, "{j_thin}");
}

#[test]
fn plate_bends_about_its_weak_axis() {
    use crate::calc::mr_structure::MrArmSectionType;
    for (w, t) in [(0.022, 0.004), (0.004, 0.022)] {
        let s = section(
            ArmSection::Plate {
                width: w,
                thickness: t,
            },
            MrArmSectionType::Round,
            0.0,
            0.0,
        );
        let (long, short) = (0.022f64, 0.004f64);
        assert!(rel(s[1], long * short.powi(3) / 12.0) < 1e-12);
        assert!(rel(s[2], long * short * short / 6.0) < 1e-12);
    }
}

#[test]
fn jacobi_handles_huge_rotation_ratios() {
    use super::eigen::jacobi_symmetric;
    let a = [1.0, 1e-100, 1e-100, 1e60];
    let (vals, vecs) = jacobi_symmetric(&a, 2);
    assert!(rel(vals[0], 1.0) < 1e-12 && rel(vals[1], 1e60) < 1e-12);
    assert!(vecs.iter().all(|v| v.is_finite()));
}

#[test]
fn generalized_solves_with_a_coupled_mass_matrix() {
    use super::eigen::generalized;
    let k = [4.0, -1.0, 0.5, -1.0, 3.0, -0.5, 0.5, -0.5, 2.0];
    let m = [2.0, 0.3, 0.1, 0.3, 1.5, -0.2, 0.1, -0.2, 1.0];
    let (vals, x) = generalized(&k, &m, 3).expect("positive definite mass");
    for c in 0..3 {
        for r in 0..3 {
            let kx: f64 = (0..3).map(|j| k[r * 3 + j] * x[j * 3 + c]).sum();
            let mx: f64 = (0..3).map(|j| m[r * 3 + j] * x[j * 3 + c]).sum();
            assert!((kx - vals[c] * mx).abs() < 1e-9, "K x = lambda M x");
        }
        for d in 0..3 {
            let xmx: f64 = (0..3)
                .map(|r| {
                    (0..3)
                        .map(|j| x[r * 3 + c] * m[r * 3 + j] * x[j * 3 + d])
                        .sum::<f64>()
                })
                .sum();
            let want = if c == d { 1.0 } else { 0.0 };
            assert!((xmx - want).abs() < 1e-9, "X^T M X = I");
        }
    }
}
