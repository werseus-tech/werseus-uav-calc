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
fn larger_heavier_vehicle_gets_lower_gains() {
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
            assert_ne!(c.class, HoverClass::Full, "{c:?}");
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
