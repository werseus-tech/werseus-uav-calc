// SPDX-License-Identifier: AGPL-3.0-or-later
//! Headless tests of the propulsion curve model.

use super::*;
use crate::calc::{Library, Model, evaluate};
use crate::project::{DroneType, Project, load_example};

const EXAMPLES: [&str; 6] = [
    "fixed_wing_2kg_1600mm.json",
    "flying_wing_1200mm.json",
    "hexa_550_x.json",
    "quad_250_x.json",
    "quad_275_x.json",
    "vtol_quadplane.json",
];

/// Every example project plus the default project of every vehicle type.
fn samples() -> Vec<(String, Project, Model)> {
    let lib = Library::embedded();
    let mut v = Vec::new();
    for f in EXAMPLES {
        let p = load_example(f).unwrap_or_else(|e| panic!("{f}: {e}"));
        let m = evaluate(&p, &lib);
        v.push((f.to_string(), p, m));
    }
    for dt in DroneType::ALL {
        let mut p = Project::default();
        p.set_drone_type(dt);
        let m = evaluate(&p, &lib);
        v.push((format!("default {dt:?}"), p, m));
    }
    v
}

fn units(s: &Setup) -> Vec<(&'static str, &PropUnit)> {
    let mut v = Vec::new();
    if let Some(u) = s.lift.as_ref() {
        v.push(("lift", u));
    }
    if let Some(u) = s.forward.as_ref() {
        v.push(("forward", u));
    }
    v
}

fn airspeeds(p: &Project, g: &str) -> Vec<f64> {
    let cruise = if g == "lift" { p.mr_v_cruise } else { p.v_cr };
    vec![0.0, 0.5 * cruise, cruise]
}

fn setup_for(p: &Project, m: &Model) -> Setup {
    build(p, m, &[Overrides::default(); 2])
}

#[test]
fn every_vehicle_type_has_a_model() {
    for (name, p, m) in samples() {
        let s = setup_for(&p, &m);
        assert_eq!(s.lift.is_some(), p.drone_type.is_multirotor(), "{name}");
        assert_eq!(s.forward.is_some(), p.drone_type.is_fixed_wing(), "{name}");
    }
}

#[test]
fn thrust_and_current_rise_with_throttle() {
    for (name, p, m) in samples() {
        let s = setup_for(&p, &m);
        for (g, u) in units(&s) {
            for v in airspeeds(&p, g) {
                let c = sweep(u, v);
                assert_eq!(c.points.len(), N_STEPS);
                for w in c.points.windows(2) {
                    assert!(
                        w[1].op.thrust_n >= w[0].op.thrust_n - 1e-9,
                        "{name} {g} v={v}: thrust fell {} -> {}",
                        w[0].op.thrust_n,
                        w[1].op.thrust_n
                    );
                    assert!(
                        w[1].op.i_bat_a >= w[0].op.i_bat_a - 1e-9,
                        "{name} {g} v={v}: current fell"
                    );
                    assert!(w[1].op.rpm >= w[0].op.rpm - 1e-6, "{name} {g}: rpm fell");
                }
            }
        }
    }
}

#[test]
fn zero_throttle_gives_zero_thrust_and_current() {
    for (name, p, m) in samples() {
        let s = setup_for(&p, &m);
        for (g, u) in units(&s) {
            for v in airspeeds(&p, g) {
                let c = sweep(u, v);
                let z = &c.points[0];
                assert_eq!(z.op.thrust_n, 0.0, "{name} {g}");
                assert_eq!(z.op.i_bat_a, 0.0, "{name} {g}");
                assert_eq!(z.op.p_elec_w, 0.0, "{name} {g}");
                assert_eq!(z.efficiency_gf_per_w, 0.0, "{name} {g}");
            }
        }
    }
}

#[test]
fn electrical_power_is_voltage_times_current_and_balances() {
    for (name, p, m) in samples() {
        let s = setup_for(&p, &m);
        for (g, u) in units(&s) {
            for v in airspeeds(&p, g) {
                for pt in sweep(u, v).points {
                    let o = pt.op;
                    assert!(
                        (o.p_elec_w - o.v_bus * o.i_bat_a).abs() <= 1e-9 * (1.0 + o.p_elec_w),
                        "{name} {g}: P != V*I"
                    );
                    // Energy balance: ESC output = shaft + motor losses.
                    let p_motor_in = o.throttle * o.v_bus * o.i_motor_a;
                    let rhs = o.p_shaft_w + o.p_loss_w;
                    assert!(
                        (p_motor_in - rhs).abs() <= 1e-3 * (1.0 + p_motor_in),
                        "{name} {g} d={}: {p_motor_in} vs {rhs}",
                        o.throttle
                    );
                    // The pack sags but never exceeds its open-circuit voltage.
                    assert!(o.v_bus <= u.pack.v_oc + 1e-9 && o.v_bus > 0.0);
                }
            }
        }
    }
}

#[test]
fn static_efficiency_peaks_below_full_throttle() {
    for (name, p, m) in samples() {
        let s = setup_for(&p, &m);
        for (g, u) in units(&s) {
            let c = sweep(u, 0.0);
            let i = c.peak_efficiency_index();
            assert!(
                i > 0 && i < N_STEPS - 1,
                "{name} {g}: efficiency peak at index {i}"
            );
        }
    }
}

#[test]
fn derived_motor_reaches_design_thrust_at_full_throttle() {
    for (name, p, m) in samples() {
        let s = setup_for(&p, &m);
        if let Some(u) = s.lift.as_ref() {
            let target = if p.mr_t_max_motor > 0.0 {
                p.mr_t_max_motor
            } else {
                2.0 * m.mr.t_hover_motor
            };
            let t = u.operating_point(1.0, 0.0).thrust_n;
            assert!(
                (t / target - 1.0).abs() < 0.03,
                "{name} lift {t} vs {target}"
            );
        }
        if let Some(u) = s.forward.as_ref() {
            let t = u.operating_point(1.0, 0.0).thrust_n;
            assert!(
                (t / m.prop.t_static - 1.0).abs() < 0.03,
                "{name} forward {t} vs {}",
                m.prop.t_static
            );
        }
    }
}

/// Documented tolerance between the curve-model hover time and the existing hover endurance:
/// the existing calculation uses constant motor and ESC efficiencies, the curve model lets the
/// motor efficiency vary with load and adds pack sag.
const HOVER_TOLERANCE: f64 = 0.25;

#[test]
fn hover_time_matches_existing_endurance() {
    let mut checked = 0;
    for (name, p, m) in samples() {
        if !p.drone_type.is_multirotor() {
            continue;
        }
        let s = setup_for(&p, &m);
        let f = flight_summary(&p, &m, &s);
        let h = &f.modes[0];
        assert_eq!(h.kind, ModeKind::Hover);
        if h.status != ModeStatus::Ok {
            // A project whose motors cannot lift it has no hover either way.
            assert!(
                m.mr.tw_ratio < 1.0,
                "{name}: hover unreachable at TW {}",
                m.mr.tw_ratio
            );
            continue;
        }
        let t = h.time_min.unwrap();
        let r = m.mr.hover_endurance_min;
        assert!(
            (t / r - 1.0).abs() < HOVER_TOLERANCE,
            "{name}: curve hover {t:.1} min vs existing {r:.1} min"
        );
        checked += 1;
    }
    assert!(checked >= 4, "only {checked} multirotor samples checked");
}

#[test]
fn temperature_rises_with_current_and_falls_with_airflow() {
    for (name, p, m) in samples() {
        let s = setup_for(&p, &m);
        for (g, u) in units(&s) {
            let c = sweep(u, 0.0);
            for w in c.points.windows(2) {
                assert!(
                    w[1].op.temp_c >= w[0].op.temp_c - 1e-9,
                    "{name} {g}: temperature fell with throttle"
                );
            }
            assert!(c.points[N_STEPS - 1].op.temp_c > c.points[1].op.temp_c);
            // Same motor, same throttle, more airflow: cooler winding.
            let still = u.operating_point(0.8, 0.0);
            let moving = u.operating_point(0.8, 15.0);
            assert!(
                moving.temp_c < still.temp_c,
                "{name} {g}: {} vs {}",
                moving.temp_c,
                still.temp_c
            );
        }
    }
}

#[test]
fn airflow_alone_lowers_temperature_at_equal_load() {
    // Same electrical loss, only the cooling differs: compare thermal resistance directly.
    let (_, p, m) = samples().into_iter().next().unwrap();
    let s = setup_for(&p, &m);
    let u = s.forward.as_ref().unwrap();
    let mut slow = u.clone();
    slow.motor.rth_still = 2.0 * u.motor.rth_still;
    let a = u.operating_point(0.6, 0.0);
    let b = slow.operating_point(0.6, 0.0);
    assert!(b.temp_c > a.temp_c);
}

#[test]
fn limits_are_flagged() {
    let (_, p, m) = samples().into_iter().next().unwrap();
    let mut ov = [Overrides::default(); 2];
    ov[1].motor_i_max_a = Some(5.0);
    ov[1].t_limit_c = Some(40.0);
    let s = build(&p, &m, &ov);
    let u = s.forward.as_ref().unwrap();
    let c = sweep(u, 0.0);
    let last = c.points.last().unwrap();
    assert!(last.flags.motor_current && last.flags.temperature);
    assert!(!c.points[0].flags.any());
    assert!(c.first_limit_throttle().is_some());
    // A tiny pack limit trips the battery flag.
    let mut u2 = u.clone();
    u2.pack.max_cont_a = 1.0;
    assert!(sweep(&u2, 0.0).points.last().unwrap().flags.battery);
}

#[test]
fn fixed_wing_has_no_hover_and_vtol_does() {
    for (name, p, m) in samples() {
        let s = setup_for(&p, &m);
        let f = flight_summary(&p, &m, &s);
        assert_eq!(f.modes.len(), 3, "{name}");
        let hover = &f.modes[0];
        match p.drone_type {
            DroneType::FixedWingConventional
            | DroneType::FixedWingVTail
            | DroneType::FlyingWing => {
                assert_eq!(hover.status, ModeStatus::NotApplicable, "{name}");
                assert!(hover.time_min.is_none(), "{name}: fake hover number");
            }
            _ => assert_ne!(hover.status, ModeStatus::NotApplicable, "{name}"),
        }
    }
}

#[test]
fn flight_modes_are_finite_and_plausible() {
    for (name, p, m) in samples() {
        let s = setup_for(&p, &m);
        let f = flight_summary(&p, &m, &s);
        for r in &f.modes {
            for x in [
                r.throttle,
                r.airspeed_ms,
                r.current_a,
                r.power_w,
                r.time_min,
                r.reference_time_min,
            ]
            .into_iter()
            .flatten()
            {
                assert!(x.is_finite() && x >= 0.0, "{name} {:?}: {x}", r.kind);
            }
            if r.status == ModeStatus::Ok {
                assert!(r.throttle.unwrap() <= 1.0 + 1e-9, "{name}");
            }
        }
        if let Some(v) = f.v_max_ms {
            assert!(v.is_finite() && v > 0.0, "{name}: v_max {v}");
        }
        // Aggressive flight burns more power than cruise or hover where both exist.
        let (a, c) = (&f.modes[1], &f.modes[2]);
        if a.status == ModeStatus::Ok && c.status == ModeStatus::Ok && p.drone_type.is_fixed_wing()
        {
            assert!(a.power_w.unwrap() > c.power_w.unwrap(), "{name}");
            assert!(
                a.airspeed_ms.unwrap() >= c.airspeed_ms.unwrap() - 1.0,
                "{name}"
            );
        }
    }
}

#[test]
fn fixed_wing_cruise_time_is_in_the_range_of_the_existing_estimate() {
    let (_, p, m) = samples().into_iter().next().unwrap();
    let s = setup_for(&p, &m);
    let c = &flight_summary(&p, &m, &s).modes[2];
    assert_eq!(c.status, ModeStatus::Ok);
    let t = c.time_min.unwrap();
    let r = c.reference_time_min.unwrap();
    // Existing estimate assumes a constant propeller efficiency; wide documented band.
    assert!(t > 0.4 * r && t < 2.5 * r, "{t} vs {r}");
}
