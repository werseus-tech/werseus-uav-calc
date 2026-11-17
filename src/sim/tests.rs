// SPDX-License-Identifier: AGPL-3.0-or-later
//! Headless behaviour tests for the flight simulator.
use super::math::{Inertia, Quat, V3};
use super::*;
use crate::calc::{Library, Model};
use crate::project::{DroneType, Project};

#[derive(Clone, Copy, Debug)]
enum Sample {
    FixedWing,
    VTail,
    FlyingWing,
    Quad,
    Hexa,
    Vtol,
}

const ALL: [Sample; 6] = [
    Sample::FixedWing,
    Sample::VTail,
    Sample::FlyingWing,
    Sample::Quad,
    Sample::Hexa,
    Sample::Vtol,
];

fn project(s: Sample) -> (Project, Model) {
    let p = match s {
        Sample::FixedWing => Project::from_json(crate::project::EX_FIXED_WING).unwrap(),
        Sample::VTail => {
            let mut p = Project::from_json(crate::project::EX_FIXED_WING).unwrap();
            p.set_drone_type(DroneType::FixedWingVTail);
            p
        }
        Sample::FlyingWing => Project::from_json(crate::project::EX_FLYING_WING).unwrap(),
        Sample::Quad => Project::from_json(crate::project::EX_QUAD_250).unwrap(),
        Sample::Hexa => Project::from_json(crate::project::EX_HEXA_550).unwrap(),
        Sample::Vtol => Project::from_json(crate::project::EX_VTOL).unwrap(),
    };
    let m = crate::calc::evaluate(&p, &Library::embedded());
    (p, m)
}

fn sim(s: Sample, start: StartPosition) -> Sim {
    let (p, m) = project(s);
    sim_for_project(&p, &m, start)
}

fn run(sim: &mut Sim, secs: f64) {
    let n = (secs / DT).round() as usize;
    for _ in 0..n {
        sim.step();
    }
}

fn assert_sane(sim: &Sim, what: &str) {
    assert!(
        sim.state.is_finite(),
        "{what}: non-finite state {:?}",
        sim.state
    );
    assert_ne!(
        sim.status,
        FlightStatus::Crashed(CrashReason::Numerical),
        "{what}: numerical failure"
    );
    let t = &sim.telemetry;
    for v in [
        t.airspeed,
        t.agl,
        t.roll,
        t.pitch,
        t.power,
        t.soc,
        t.load_factor,
    ] {
        assert!(v.is_finite(), "{what}: telemetry {t:?}");
    }
}

#[test]
fn ground_start_sits_still() {
    for s in ALL {
        let mut sim = sim(s, StartPosition::Ground);
        let p0 = sim.state.pos;
        run(&mut sim, 5.0);
        assert_sane(&sim, &format!("{s:?}"));
        assert_eq!(sim.status, FlightStatus::OnGround, "{s:?}");
        assert!(
            (sim.state.pos - p0).norm() < 0.02,
            "{s:?} crept {:?}",
            sim.state.pos - p0
        );
        assert!(sim.state.vel.norm() < 0.01, "{s:?}");
    }
}

#[test]
fn trimmed_level_flight_stays_level() {
    for s in [
        Sample::FixedWing,
        Sample::VTail,
        Sample::FlyingWing,
        Sample::Vtol,
    ] {
        for mode in [ControlMode::Manual, ControlMode::Stabilized] {
            let mut sim = sim(s, StartPosition::Air);
            sim.mode = mode;
            let h0 = sim.telemetry.agl;
            let v0 = sim.telemetry.airspeed;
            run(&mut sim, 20.0);
            assert_sane(&sim, &format!("{s:?}"));
            let t = sim.telemetry;
            assert_eq!(sim.status, FlightStatus::Airborne, "{s:?} {mode:?}");
            assert!(
                (t.agl - h0).abs() < 2.0,
                "{s:?} {mode:?} alt {h0} -> {}",
                t.agl
            );
            assert!(t.roll.abs() < 0.03, "{s:?} {mode:?} roll {}", t.roll);
            assert!(
                (t.airspeed - v0).abs() < 0.05 * v0,
                "{s:?} {mode:?} v {v0} -> {}",
                t.airspeed
            );
        }
    }
}

#[test]
fn hover_holds_altitude_and_matches_design_power() {
    for s in [Sample::Quad, Sample::Hexa] {
        let (p, m) = project(s);
        let mut sim = sim_for_project(&p, &m, StartPosition::Air);
        run(&mut sim, 3.0);
        let h0 = sim.telemetry.agl;
        run(&mut sim, 30.0);
        assert_sane(&sim, &format!("{s:?}"));
        let t = sim.telemetry;
        assert!((t.agl - h0).abs() < 0.3, "{s:?} {h0} -> {}", t.agl);
        assert!(t.roll.abs() < 0.035 && t.pitch.abs() < 0.035, "{s:?} {t:?}");
        // Electrical hover power within 15 % of the design calculation.
        let design = m.mr.p_total_hover;
        assert!(
            (t.power / design - 1.0).abs() < 0.15,
            "{s:?} sim {} W vs design {} W",
            t.power,
            design
        );
    }
}

#[test]
fn vtol_takes_off_hovers_and_lands() {
    let mut sim = sim(Sample::Vtol, StartPosition::Ground);
    sim.input.throttle = 0.8;
    run(&mut sim, 6.0);
    assert_eq!(sim.status, FlightStatus::Airborne);
    sim.input.throttle = 0.5;
    run(&mut sim, 2.0);
    let h0 = sim.telemetry.agl;
    run(&mut sim, 10.0);
    assert!(
        (sim.telemetry.agl - h0).abs() < 0.3,
        "{h0} -> {}",
        sim.telemetry.agl
    );
    sim.input.throttle = 0.2;
    run(&mut sim, 30.0);
    assert_eq!(sim.status, FlightStatus::OnGround);
}

#[test]
fn torque_free_rigid_body_conserves_energy_and_momentum() {
    let (p, m) = project(Sample::FixedWing);
    let mut params = VehicleParams::from_project(&p, &m);
    params.inertia = Inertia {
        ixx: 0.12,
        iyy: 0.20,
        izz: 0.29,
        ixz: 0.015,
    };
    let cfg = SimConfig {
        aero: false,
        gravity: true,
        ground: false,
        seed: 1,
    };
    let mut sim = Sim::with_config(params, StartPosition::Air, cfg);
    for n in sim.motor_n.iter_mut() {
        *n = 0.0;
    }
    sim.state.pos = V3::new(0.0, 0.0, -100.0);
    sim.state.vel = V3::new(5.0, 1.0, -3.0);
    sim.state.q = Quat::from_euler(0.2, 0.1, 0.4);
    sim.state.w = V3::new(1.0, 2.0, 0.5);
    let i = sim.inertia;
    let energy = |s: &Sim| {
        let st = &s.state;
        0.5 * s.mass * st.vel.dot(st.vel) + 0.5 * st.w.dot(i.mul(st.w))
            - s.mass * crate::calc::atmosphere::G * (-st.pos.z)
    };
    let momentum = |s: &Sim| s.state.q.rotate(i.mul(s.state.w));
    let (e0, h0) = (energy(&sim), momentum(&sim));
    for _ in 0..(10.0 / DT) as usize {
        sim.step_passive();
    }
    let (e1, h1) = (energy(&sim), momentum(&sim));
    assert!(((e1 - e0) / e0.abs()).abs() < 1e-6, "energy {e0} -> {e1}");
    assert!(
        (h1 - h0).norm() / h0.norm() < 1e-6,
        "momentum {h0:?} -> {h1:?}"
    );
    // Horizontal momentum unchanged by gravity.
    assert!((sim.state.vel.x - 5.0).abs() < 1e-9 && (sim.state.vel.y - 1.0).abs() < 1e-9);
}

#[test]
fn stall_breaks_lift_and_loses_altitude() {
    let mut sim = sim(Sample::FixedWing, StartPosition::Air);
    let cl_max = sim.params.wing.as_ref().unwrap().cl_max;
    let vs = sim.params.v_stall;
    sim.mode = ControlMode::Manual;
    sim.input.throttle = 0.0;
    let h0 = sim.telemetry.agl;
    let (mut max_stall, mut max_cl, mut min_v): (f64, f64, f64) = (0.0, 0.0, f64::MAX);
    for _ in 0..(8.0 / DT) as usize {
        sim.input.pitch = 1.0;
        sim.step();
        let t = sim.telemetry;
        max_stall = max_stall.max(t.stall);
        max_cl = max_cl.max(t.cl);
        min_v = min_v.min(t.airspeed);
    }
    assert_sane(&sim, "stall");
    assert!(max_stall > 0.5, "never stalled: sigma {max_stall}");
    // CL includes pitch-rate and elevator increments; allow a small margin over CLmax.
    assert!(max_cl < 1.15 * cl_max, "CL {max_cl} vs CLmax {cl_max}");
    assert!(min_v < 1.1 * vs, "min speed {min_v} vs stall {vs}");
    assert!(sim.telemetry.agl < h0, "should lose altitude");
}

#[test]
fn vtol_transition_completes_and_returns() {
    let mut sim = sim(Sample::Vtol, StartPosition::Ground);
    sim.input.throttle = 0.8;
    run(&mut sim, 8.0);
    sim.input.throttle = 0.5;
    run(&mut sim, 3.0);
    assert_eq!(sim.phase, VtolPhase::Hover);
    let h0 = sim.telemetry.agl;
    sim.toggle_transition();
    let mut t_done = None;
    let mut min_h = h0;
    for k in 0..(30.0 / DT) as usize {
        sim.step();
        min_h = min_h.min(sim.telemetry.agl);
        if sim.phase == VtolPhase::Cruise {
            t_done = Some(k as f64 * DT);
            break;
        }
    }
    let t_done = t_done.expect("transition did not complete");
    assert!(t_done < 20.0, "transition took {t_done} s");
    assert!(h0 - min_h < 5.0, "lost {} m in transition", h0 - min_h);
    assert!(sim.telemetry.airspeed > sim.params.v_stall * 1.2);
    // Lift motors spin down in cruise.
    run(&mut sim, 5.0);
    let lift_max = sim
        .params
        .rotors
        .iter()
        .zip(&sim.motor_n)
        .filter(|(r, _)| r.lift)
        .map(|(_, n)| *n)
        .fold(0.0, f64::max);
    assert!(lift_max < 0.05, "lift rotors still at {lift_max}");
    assert_eq!(sim.status, FlightStatus::Airborne);
    // Back to hover.
    sim.toggle_transition();
    run(&mut sim, 30.0);
    assert_eq!(sim.phase, VtolPhase::Hover);
    assert_sane(&sim, "vtol back transition");
    assert!(!sim.is_crashed());
}

#[test]
fn motor_failure_never_produces_nan() {
    for s in ALL {
        for k in 0..sim(s, StartPosition::Air).params.rotors.len() {
            let mut sim = sim(s, StartPosition::Air);
            run(&mut sim, 2.0);
            sim.fail_motor(k);
            run(&mut sim, 40.0);
            assert_sane(&sim, &format!("{s:?} motor {k}"));
        }
    }
}

#[test]
fn hexa_fail_safe_lands_after_motor_loss() {
    let mut sim = sim(Sample::Hexa, StartPosition::Air);
    run(&mut sim, 2.0);
    sim.fail_motor(0);
    run(&mut sim, 40.0);
    assert!(sim.failsafe);
    assert_eq!(sim.status, FlightStatus::OnGround, "{:?}", sim.status);
}

#[test]
fn simulation_is_deterministic() {
    let script = |sim: &mut Sim| {
        sim.set_wind(WindConfig {
            speed: 6.0,
            from_deg: 200.0,
            turbulence: 0.8,
        });
        for k in 0..(20.0 / DT) as usize {
            let t = k as f64 * DT;
            sim.input.roll = (0.7 * t).sin() * 0.5;
            sim.input.pitch = (0.3 * t).cos() * 0.3;
            sim.input.throttle = 0.6;
            if k == 2000 {
                sim.trigger_gust();
            }
            sim.step();
        }
        sim.state
    };
    for s in [Sample::FixedWing, Sample::Quad, Sample::Vtol] {
        let a = script(&mut sim(s, StartPosition::Air));
        let b = script(&mut sim(s, StartPosition::Air));
        assert_eq!(a, b, "{s:?}");
    }
}

#[test]
fn every_vehicle_runs_sixty_seconds_without_nan() {
    for s in ALL {
        for start in [StartPosition::Ground, StartPosition::Air] {
            let mut sim = sim(s, start);
            sim.set_wind(WindConfig {
                speed: 5.0,
                from_deg: 90.0,
                turbulence: 0.6,
            });
            for k in 0..(60.0 / DT) as usize {
                let t = k as f64 * DT;
                sim.input.roll = (0.5 * t).sin() * 0.6;
                sim.input.pitch = (0.23 * t).sin() * 0.4;
                sim.input.yaw = (0.17 * t).sin() * 0.3;
                sim.input.throttle = 0.55 + 0.4 * (0.05 * t).sin();
                if k == 6000 {
                    sim.trigger_gust();
                    sim.toggle_transition();
                }
                if k == 12000 {
                    sim.drop_payload();
                }
                sim.step();
            }
            assert_sane(&sim, &format!("{s:?} {start:?}"));
        }
    }
}

#[test]
fn fixed_wing_takes_off_from_the_runway() {
    for s in [Sample::FixedWing, Sample::VTail] {
        let mut sim = sim(s, StartPosition::Ground);
        sim.input.throttle = 1.0;
        let mut airborne_at = None;
        for k in 0..(25.0 / DT) as usize {
            let v = sim.telemetry.airspeed;
            sim.input.pitch = if v > 0.85 * sim.params.v_cruise {
                0.4
            } else {
                0.0
            };
            sim.step();
            if sim.status == FlightStatus::Airborne && sim.telemetry.agl > 3.0 {
                airborne_at = Some(k as f64 * DT);
                break;
            }
        }
        assert!(
            airborne_at.is_some(),
            "{s:?} never left the ground: {:?}",
            sim.telemetry
        );
        assert!(!sim.is_crashed());
    }
}

#[test]
fn payload_drop_and_extra_mass_change_dynamics() {
    let mut sim = sim(Sample::Quad, StartPosition::Air);
    run(&mut sim, 2.0);
    let m0 = sim.mass;
    sim.drop_payload();
    assert!(sim.mass < m0);
    run(&mut sim, 5.0);
    assert_sane(&sim, "drop");
    sim.add_mass(0.25);
    assert!(sim.mass > m0 * 0.99);
    run(&mut sim, 5.0);
    assert_sane(&sim, "add mass");
    sim.reset(StartPosition::Air);
    assert_eq!(sim.mass, m0);
    assert!(!sim.payload_dropped);
}

#[test]
fn hard_impact_is_detected_as_crash() {
    let mut sim = sim(Sample::Quad, StartPosition::Air);
    sim.mode = ControlMode::Manual;
    sim.input.throttle = 0.0;
    run(&mut sim, 10.0);
    assert!(sim.is_crashed(), "{:?}", sim.status);
    let t = sim.t;
    run(&mut sim, 1.0);
    assert_eq!(sim.t, t, "simulation must freeze after a crash");
}

#[test]
fn simulation_is_faster_than_real_time() {
    let mut sim = sim(Sample::Vtol, StartPosition::Air);
    let t0 = std::time::Instant::now();
    run(&mut sim, 60.0);
    let wall = t0.elapsed().as_secs_f64();
    eprintln!(
        "60 s of VTOL flight ({} steps) simulated in {:.3} s ({:.1} us/step)",
        (60.0 / DT) as usize,
        wall,
        wall / (60.0 / DT) * 1e6
    );
    assert!(wall < 60.0);
}

#[test]
fn every_drone_type_builds_parameters() {
    let lib = Library::embedded();
    for dt in DroneType::ALL {
        let mut p = Project::default();
        p.set_drone_type(dt);
        let m = crate::calc::evaluate(&p, &lib);
        let v = VehicleParams::from_project(&p, &m);
        assert!(v.mass > 0.0 && v.inertia.is_valid(), "{dt:?}");
        assert_eq!(v.wing.is_some(), dt.has_wing(), "{dt:?}");
        assert_eq!(v.lift_rotor_count() > 0, dt.is_multirotor(), "{dt:?}");
        assert!(!v.contacts.is_empty());
        if dt == DroneType::FixedWingVTail {
            assert_eq!(v.wing.as_ref().unwrap().mixing, vehicle::Mixing::VTail);
        }
        if dt == DroneType::FlyingWing {
            assert_eq!(v.wing.as_ref().unwrap().mixing, vehicle::Mixing::Elevon);
        }
    }
}
