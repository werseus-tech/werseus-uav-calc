// SPDX-License-Identifier: AGPL-3.0-or-later
//! Headless checks of the control-tuning and motor-loss analyses on the sample projects.
use super::failure::HoverClass;
use super::firmware::{Firmware, export_text, parse_export};
use super::loopdesign::simulate_step;
use super::*;
use crate::calc::multirotor::MultirotorLayout;
use crate::calc::{Library, evaluate};
use crate::project::{self, DroneType, Project};

fn load(json: &str) -> Project {
    Project::from_json(json).expect("sample project")
}

fn report(p: &Project) -> ControlReport {
    let m = evaluate(p, &Library::embedded());
    analyze(
        p,
        &m,
        &TuneOptions {
            include_pairs: true,
            ..Default::default()
        },
    )
}

fn hover_samples() -> Vec<(&'static str, Project)> {
    let mut x8 = load(project::EX_QUAD_275);
    x8.mr_coaxial = true;
    let mut hexa_plus = load(project::EX_HEXA_550);
    hexa_plus.mr_layout = MultirotorLayout::Plus;
    let mut def = Project::default();
    def.set_drone_type(DroneType::Multirotor);
    vec![
        ("quad_250", load(project::EX_QUAD_250)),
        ("quad_275", load(project::EX_QUAD_275)),
        ("hexa_550", load(project::EX_HEXA_550)),
        ("hexa_plus", hexa_plus),
        ("x8", x8),
        ("vtol", load(project::EX_VTOL)),
        ("default_mr", def),
    ]
}

fn all_samples() -> Vec<(String, Project)> {
    let mut v: Vec<(String, Project)> = hover_samples()
        .into_iter()
        .map(|(n, p)| (n.to_string(), p))
        .collect();
    v.push(("fixed_wing".into(), load(project::EX_FIXED_WING)));
    v.push(("flying_wing".into(), load(project::EX_FLYING_WING)));
    for dt in DroneType::ALL {
        let mut p = Project::default();
        p.set_drone_type(dt);
        v.push((format!("default_{dt:?}"), p));
    }
    v
}

#[test]
fn suggested_gains_give_stable_loops_with_margin() {
    for (name, p) in hover_samples() {
        let r = report(&p);
        assert_eq!(r.firmwares.len(), 3, "{name}");
        for fw in &r.firmwares {
            if !fw.applicable {
                assert!(
                    fw.firmware == Firmware::Betaflight && r.vehicle.kind == VehicleKind::Vtol,
                    "{name}: {:?} not applicable",
                    fw.firmware
                );
                continue;
            }
            for ax in &fw.axes {
                let (g, met) = ax
                    .result
                    .unwrap_or_else(|| panic!("{name} {:?} {:?}: no design", fw.firmware, ax.axis));
                let tag = format!("{name} {:?} {:?} {met:?} {g:?}", fw.firmware, ax.axis);
                assert!(met.phase_margin_deg >= 40.0, "{tag}");
                assert!(met.gain_margin_db >= 6.0, "{tag}");
                assert!(
                    met.bandwidth_hz > 0.3 && met.bandwidth_hz < 80.0,
                    "{tag}: bandwidth"
                );
                let wc = met.crossover_hz * 2.0 * std::f64::consts::PI;
                let s = simulate_step(&ax.plant, &ax.filters, &g, (40.0 / wc).max(1.5));
                assert!(s.settled && s.overshoot < 0.6, "{tag}: step {s:?}");
            }
        }
    }
}

/// Geometrically scaled family (frame and prop ×s, masses and thrust ×s³, so T/W is kept).
/// A larger, heavier vehicle must get a slower loop: lower crossover and closed-loop bandwidth,
/// lower filter cut-offs and lower normalised loop gains `Kp·K/J` [rad/s] and `Ki·K/J`
/// [rad/s²]. (The raw P in firmware units need not drop: inertia per unit control authority,
/// J/K, grows with size, so the same bandwidth would need *more* P.)
#[test]
fn larger_heavier_vehicle_gets_lower_normalised_loop_gains() {
    let base = load(project::EX_QUAD_275);
    let scaled = |s: f64| {
        let mut p = base.clone();
        let s3 = s * s * s;
        p.mr_frame_size *= s;
        p.mr_d_prop *= s;
        p.mr_r_plate *= s;
        p.mr_m_frame *= s3;
        p.mr_m_motor *= s3;
        p.mr_m_esc_tot *= s3;
        p.mr_m_fc *= s3;
        p.mr_m_misc *= s3;
        p.m_payload *= s3;
        p.n_parallel = ((p.n_parallel as f64) * s3).round().max(1.0) as u32;
        p.mr_t_max_motor *= s3;
        for c in &mut p.mr_components {
            c.mass *= s3;
            c.x *= s;
            c.y *= s;
        }
        report(&p)
    };
    let reps: Vec<ControlReport> = [1.0, 1.6, 2.5].into_iter().map(scaled).collect();
    for w in reps.windows(2) {
        assert!(w[1].vehicle.mass > w[0].vehicle.mass);
        for (a, b) in w[0].firmwares.iter().zip(&w[1].firmwares) {
            for (axa, axb) in a.axes.iter().zip(&b.axes) {
                let (ga, ma) = axa.result.unwrap();
                let (gb, mb) = axb.result.unwrap();
                let tag = format!("{:?} {:?}", a.firmware, axa.axis);
                let (ka, kb) = (axa.plant.k_over_j, axb.plant.k_over_j);
                assert!(gb.kp * kb < ga.kp * ka, "{tag}: Kp·K/J");
                assert!(gb.ki * kb < ga.ki * ka, "{tag}: Ki·K/J");
                assert!(mb.crossover_hz < ma.crossover_hz, "{tag}: crossover");
                assert!(mb.bandwidth_hz < ma.bandwidth_hz, "{tag}: bandwidth");
            }
            assert!(b.filters.gyro_hz < a.filters.gyro_hz);
            assert!(b.filters.dterm_hz < a.filters.dterm_hz);
        }
    }
}

#[test]
fn firmware_exports_parse_without_nan() {
    for (name, p) in hover_samples() {
        let r = report(&p);
        for fw in r.firmwares.iter().filter(|f| f.applicable) {
            let text = export_text(fw.firmware, &fw.params, &["test header".to_string()]);
            let kv = parse_export(fw.firmware, &text).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(kv.len(), fw.params.len(), "{name}");
            for (k, v) in kv {
                assert!(
                    !v.to_lowercase().contains("nan") && !v.contains("inf"),
                    "{name} {k}={v}"
                );
                let is_text = matches!(v.as_str(), "OFF" | "ON" | "PT1");
                assert!(
                    is_text || v.parse::<f64>().is_ok_and(f64::is_finite),
                    "{name} {k}={v}"
                );
            }
        }
        if let Some(fwh) = &r.fixed_wing {
            for (fw, ps) in [
                (Firmware::ArduPilot, &fwh.ardupilot),
                (Firmware::Px4, &fwh.px4),
            ] {
                let text = export_text(fw, ps, &[]);
                assert_eq!(parse_export(fw, &text).unwrap().len(), ps.len());
            }
        }
    }
}

#[test]
fn firmware_names_follow_conventions() {
    let r = report(&load(project::EX_QUAD_250));
    let names = |fw: Firmware| -> Vec<String> {
        r.firmwares
            .iter()
            .find(|f| f.firmware == fw)
            .unwrap()
            .params
            .iter()
            .map(|p| p.name.clone())
            .collect()
    };
    assert!(names(Firmware::Betaflight).contains(&"p_roll".to_string()));
    assert!(names(Firmware::ArduPilot).contains(&"ATC_RAT_RLL_P".to_string()));
    assert!(names(Firmware::Px4).contains(&"MC_ROLLRATE_P".to_string()));
    let v = report(&load(project::EX_VTOL));
    let ap = v
        .firmwares
        .iter()
        .find(|f| f.firmware == Firmware::ArduPilot)
        .unwrap();
    assert!(ap.params.iter().any(|p| p.name == "Q_A_RAT_RLL_P"));
}

#[test]
fn quad_loses_hover_on_single_motor_loss() {
    for p in [
        load(project::EX_QUAD_250),
        load(project::EX_QUAD_275),
        load(project::EX_VTOL),
    ] {
        let f = report(&p).failures.unwrap();
        assert_eq!(f.healthy.class, HoverClass::Full);
        assert_eq!(f.singles.len(), 4);
        for c in &f.singles {
            assert_eq!(c.class, HoverClass::NoHover, "{c:?}");
            assert!(c.lost_axes.contains(&2));
        }
    }
}

#[test]
fn hexa_keeps_hover_on_single_motor_loss() {
    let mut plus = load(project::EX_HEXA_550);
    plus.mr_layout = MultirotorLayout::Plus;
    for p in [load(project::EX_HEXA_550), plus] {
        let f = report(&p).failures.unwrap();
        assert_eq!(f.healthy.class, HoverClass::Full);
        assert_eq!(f.singles.len(), 6);
        for c in &f.singles {
            assert_ne!(c.class, HoverClass::NoHover, "{c:?}");
            assert!(c.acai3 > 0.0);
        }
    }
}

#[test]
fn coaxial_x8_keeps_full_control_on_single_motor_loss() {
    let mut x8 = load(project::EX_QUAD_275);
    x8.mr_coaxial = true;
    let f = report(&x8).failures.unwrap();
    assert_eq!(f.singles.len(), 8);
    for c in &f.singles {
        assert_eq!(c.class, HoverClass::Full, "{c:?}");
        assert!(c.thrust_margin > 0.0);
    }
    assert_eq!(f.pairs.len(), 28);
}

fn assert_finite(tag: &str, x: f64) {
    assert!(x.is_finite(), "{tag}: {x}");
}

#[test]
fn no_nan_for_every_sample_and_vehicle_type() {
    for (name, p) in all_samples() {
        let r = report(&p);
        let v = &r.vehicle;
        for (k, x) in [
            ("mass", v.mass),
            ("weight", v.weight),
            ("kq", v.kq),
            ("tau", v.motor_tau),
            ("hover_rpm", v.hover_rpm),
            ("max_rpm", v.max_rpm),
            ("hover_frac", v.hover_frac),
        ] {
            assert_finite(&format!("{name} {k}"), x);
        }
        for i in v.inertia {
            assert!(i.is_finite() && i > 0.0, "{name} inertia {i}");
        }
        for fw in &r.firmwares {
            for pr in &fw.params {
                assert!(!pr.value_string().contains("NaN"), "{name} {}", pr.name);
            }
            for ax in &fw.axes {
                assert_finite(&format!("{name} K"), ax.k_axis);
                if let Some((g, m)) = ax.result {
                    for x in [g.kp, g.ki, g.kd, g.kff, m.phase_margin_deg, m.bandwidth_hz] {
                        assert_finite(&format!("{name} gains"), x);
                    }
                }
            }
        }
        if let Some(f) = &r.failures {
            for c in std::iter::once(&f.healthy)
                .chain(&f.singles)
                .chain(&f.pairs)
            {
                assert_finite(&format!("{name} acai4"), c.acai4);
                assert_finite(&format!("{name} acai3"), c.acai3);
                assert_finite(&format!("{name} margin"), c.thrust_margin);
                for a in c.authority.iter().flatten() {
                    assert_finite(&format!("{name} auth"), *a);
                }
            }
        }
        if let Some(fwh) = &r.fixed_wing {
            for pr in fwh.ardupilot.iter().chain(&fwh.px4) {
                assert!(!pr.value_string().contains("NaN"), "{name} {}", pr.name);
            }
        }
        match v.kind {
            VehicleKind::FixedWing => {
                assert!(r.firmwares.is_empty() && r.failures.is_none() && r.fixed_wing.is_some())
            }
            VehicleKind::Vtol => assert!(r.fixed_wing.is_some() && r.failures.is_some()),
            VehicleKind::Multirotor => assert!(r.fixed_wing.is_none() && r.failures.is_some()),
        }
    }
}

#[test]
fn motor_time_constant_override_is_used() {
    let p = load(project::EX_QUAD_250);
    let m = evaluate(&p, &Library::embedded());
    let fast = analyze(
        &p,
        &m,
        &TuneOptions {
            motor_tau: Some(0.01),
            include_pairs: false,
        },
    );
    let slow = analyze(
        &p,
        &m,
        &TuneOptions {
            motor_tau: Some(0.08),
            include_pairs: false,
        },
    );
    assert!(!fast.vehicle.motor_tau_estimated);
    let wc = |r: &ControlReport| r.firmwares[1].axes[0].result.unwrap().1.crossover_hz;
    assert!(wc(&slow) < wc(&fast));
}

#[test]
fn designed_gains_use_d_when_enabled() {
    use super::loopdesign::{DesignOpts, Filter, LoopFilters, Plant, design};
    let p = Plant {
        k_over_j: 800.0,
        tau: 0.04,
        delay: 0.004,
    };
    let f = LoopFilters {
        gyro: vec![Filter::biquad(45.0)],
        dterm: vec![Filter::pt1(22.0)],
    };
    // A limit on Kp lowers the crossover so that plain PI already meets the margins.
    for max_kp in [f64::INFINITY, 0.01] {
        let opts = DesignOpts {
            max_kp,
            ..Default::default()
        };
        let (g, m) = design(&p, &f, &opts).expect("design");
        assert!(g.kd > 0.0, "max_kp {max_kp}: kd {}", g.kd);
        assert!(m.phase_margin_deg >= 50.0);
        let pi = design(
            &p,
            &f,
            &DesignOpts {
                use_d: false,
                ..opts
            },
        )
        .unwrap();
        assert_eq!(pi.0.kd, 0.0);
    }
}

#[test]
fn closed_loop_setpoint_path_excludes_the_d_term() {
    use super::loopdesign::{Filter, Gains, LoopFilters, Plant, closed_loop_mag, open_loop};
    let p = Plant {
        k_over_j: 800.0,
        tau: 0.04,
        delay: 0.004,
    };
    let f = LoopFilters {
        gyro: vec![Filter::biquad(45.0)],
        dterm: vec![Filter::pt1(22.0)],
    };
    let base = Gains {
        kp: 0.05,
        ki: 0.5,
        kd: 0.0,
        kff: 0.0,
    };
    let with_d = Gains { kd: 0.004, ..base };
    for w in [5.0, 30.0, 100.0, 300.0] {
        // Numerator = |T|·|1 + L| must not depend on Kd.
        let num = |g: &Gains| {
            let (mag, ph) = open_loop(&p, &f, g, w);
            let den = ((1.0 + mag * ph.cos()).powi(2) + (mag * ph.sin()).powi(2)).sqrt();
            closed_loop_mag(&p, &f, g, w) * den
        };
        let (a, b) = (num(&base), num(&with_d));
        assert!((a / b - 1.0).abs() < 1e-9, "w {w}: {a} vs {b}");
    }
}

#[test]
fn px4_mixer_exists_for_three_rotor_layouts() {
    let mut p = load(project::EX_QUAD_275);
    p.mr_arm_count = 3;
    let m = evaluate(&p, &Library::embedded());
    let v = vehicle::vehicle_data(&p, &m, None);
    assert_eq!(v.rotors.len(), 3);
    let f = firmware::mixer_factors(Firmware::Px4, &v).expect("PX4 mixer");
    assert!(f.iter().flatten().all(|x| x.is_finite()));
    assert!(f.iter().any(|r| r[0].abs() > 1e-3 || r[1].abs() > 1e-3));
    let r = report(&p);
    let px4 = r
        .firmwares
        .iter()
        .find(|t| t.firmware == Firmware::Px4)
        .unwrap();
    assert!(px4.applicable);
    let (g, _) = px4.axes[0].result.expect("roll gains");
    assert!(g.kp > 0.0);
}

#[test]
fn underpowered_vehicle_has_consistent_rpm_limits() {
    let mut p = load(project::EX_QUAD_275);
    let m = evaluate(&p, &Library::embedded());
    let healthy = vehicle::vehicle_data(&p, &m, None);
    assert!(healthy.hover_rpm < healthy.max_rpm);
    p.mr_t_max_motor = 0.2 * healthy.weight / healthy.rotors.len() as f64;
    let v = vehicle::vehicle_data(&p, &m, None);
    assert!(
        v.hover_rpm <= v.max_rpm + 1e-9,
        "{} > {}",
        v.hover_rpm,
        v.max_rpm
    );
    assert!(v.hover_frac <= 1.0);
}

#[test]
fn fixed_wing_airspeed_hints_keep_their_order() {
    let mut p = load(project::EX_FIXED_WING);
    let m = evaluate(&p, &Library::embedded());
    for factor in [0.9, 1.1, 1.25, 1.3, 1.6] {
        p.v_cr = factor * m.aero.v_stall;
        let h = fixedwing::fixed_wing_hints(&p, &m);
        assert!(h.v_min >= 1.25 * h.v_stall - 1e-9, "{factor}: {h:?}");
        assert!(h.v_min <= h.v_cruise + 1e-9, "{factor}: {h:?}");
        assert!(h.v_cruise <= h.v_max);
    }
}

#[test]
fn nan_cg_does_not_collapse_multirotor_inertia() {
    let p = load(project::EX_QUAD_275);
    let mut m = evaluate(&p, &Library::embedded());
    m.mr.cg_x = 0.0;
    m.mr.cg_y = 0.0;
    let clean = vehicle::inertia_estimate(&p, &m);
    m.mr.cg_x = f64::NAN;
    m.mr.cg_y = f64::NAN;
    assert_eq!(vehicle::inertia_estimate(&p, &m), clean);
}

#[test]
fn fixed_wing_roll_inertia_without_wing_components_is_not_tiny() {
    let mut p = load(project::EX_FIXED_WING);
    for c in &mut p.components {
        c.link = project::Link::None;
    }
    let m = evaluate(&p, &Library::embedded());
    let i = vehicle::inertia_estimate(&p, &m);
    let mass = m.weight.m_mtow;
    assert!(i[0] >= mass * (0.25 * p.b / 2.0).powi(2) * 0.999, "{i:?}");
}
