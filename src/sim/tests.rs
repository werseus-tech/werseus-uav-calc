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
    run(&mut sim, 8.0);
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
        0.5 * s.mass * st.vel.dot(st.vel)
            + 0.5 * st.w.dot(i.apply(st.w))
            + s.mass * crate::calc::atmosphere::G * (-st.pos.z)
    };
    let momentum = |s: &Sim| s.state.q.rotate(i.apply(s.state.w));
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

#[test]
fn terrain_height_is_deterministic_and_finite() {
    for env in EnvironmentKind::ALL {
        let (sx, sy, sh) = env.spawn_position();
        assert_eq!(sh, env.ground_height(sx, sy));
        assert!(sh.is_finite());
        for ix in -15..=15 {
            for iy in -15..=15 {
                let (x, y) = (ix as f64 * 97.3, iy as f64 * 113.7);
                let h1 = env.ground_height(x, y);
                let h2 = env.ground_height(x, y);
                assert_eq!(h1, h2, "{env:?} at ({x}, {y})");
                assert!(h1.is_finite() && h1 >= 0.0, "{env:?} h={h1}");
                let n = env.ground_normal_ned(x, y);
                assert!(n.is_finite() && (n.norm() - 1.0).abs() < 1e-6);
            }
        }
        assert!(env.ground_height(f64::NAN, 0.0).is_finite());
        assert!(env.ground_height(f64::INFINITY, -10.0).is_finite());

        let m1 = WorldMesh::build(env, 0.0, 0.0, false, 1);
        let m2 = WorldMesh::build(env, 0.0, 0.0, false, 1);
        assert!(m1.is_valid(), "{env:?} invalid mesh");
        assert!(m1.triangle_count() > 0, "{env:?} empty mesh");
        assert!(
            m1.triangle_count() <= terrain::WORLD_TRIANGLE_CAP,
            "{env:?} tris {}",
            m1.triangle_count()
        );
        assert_eq!(m1.positions, m2.positions);
        assert_eq!(m1.indices, m2.indices);
    }
    // Mountains must have non-trivial elevation outside the flat valley.
    let mut peak = 0.0_f64;
    for ix in -10..=10 {
        for iy in -10..=10 {
            peak = peak
                .max(EnvironmentKind::Mountains.ground_height(ix as f64 * 60.0, iy as f64 * 60.0));
        }
    }
    assert!(peak > 50.0, "mountains peak too low: {peak}");
}

#[test]
fn city_buildings_never_overlap_spawn_point() {
    let env = EnvironmentKind::City;
    let (sx, sy, sh) = env.spawn_position();
    assert_eq!(sh, 0.0);
    assert!(env.building_hit(sx, sy, 0.0).is_none());
    assert!(env.building_hit(sx, sy, 10.0).is_none());
    for x in (-40..=200).step_by(10) {
        for y in (-20..=20).step_by(5) {
            assert!(
                env.building_hit(x as f64, y as f64, 1.0).is_none(),
                "building overlaps takeoff corridor at ({x}, {y})"
            );
        }
    }
    let nearby = env.buildings_near(0.0, 0.0, 250.0);
    assert!(!nearby.is_empty(), "city should generate nearby buildings");
    for b in &nearby {
        let overlaps_spawn = sx >= b.min_x && sx <= b.max_x && sy >= b.min_y && sy <= b.max_y;
        assert!(!overlaps_spawn, "building {b:?} overlaps spawn");
    }
}

#[test]
fn crash_detected_for_building_and_terrain() {
    // 1) City building collision
    let (p, m) = project(Sample::FixedWing);
    let mut sim_city = sim_for_project_with(
        &p,
        &m,
        StartPosition::Air,
        TakeoffMode::Runway,
        EnvironmentKind::City,
    );
    let b = EnvironmentKind::City
        .buildings_near(0.0, 0.0, 300.0)
        .into_iter()
        .find(|b| b.height > 15.0)
        .expect("city building > 15 m");
    let cx = 0.5 * (b.min_x + b.max_x);
    let cy = 0.5 * (b.min_y + b.max_y);
    let alt = (0.5 * b.height).max(4.0);
    // Start 40 m south of building centre flying north directly into the building.
    sim_city.state.pos = V3::new(b.min_x - 35.0, cy, -alt);
    sim_city.state.vel = V3::new(18.0, 0.0, 0.0);
    sim_city.state.q = Quat::IDENTITY;
    sim_city.input.throttle = 0.7;
    for _ in 0..(6.0 / DT) as usize {
        sim_city.step();
        if sim_city.is_crashed() {
            break;
        }
    }
    assert_eq!(
        sim_city.status,
        FlightStatus::Crashed(CrashReason::Building),
        "expected building crash near ({cx}, {cy}), got {:?}",
        sim_city.status
    );

    // 2) Mountains terrain collision
    let mut sim_mtn = sim_for_project_with(
        &p,
        &m,
        StartPosition::Air,
        TakeoffMode::Runway,
        EnvironmentKind::Mountains,
    );
    let mut target = None;
    for ix in 2..=15 {
        for iy in -10..=10 {
            let (tx, ty) = (ix as f64 * 40.0, iy as f64 * 40.0);
            let h = EnvironmentKind::Mountains.ground_height(tx, ty);
            if h > 45.0 {
                target = Some((tx, ty, h));
                break;
            }
        }
        if target.is_some() {
            break;
        }
    }
    let (tx, ty, th) = target.expect("mountain slope > 45 m");
    let dx = tx;
    let dy = ty;
    let dist = dx.hypot(dy).max(1.0);
    let heading = dy.atan2(dx);
    sim_mtn.state.pos = V3::new(0.0, 0.0, -(th * 0.55));
    sim_mtn.state.vel = V3::new(18.0 * dx / dist, 18.0 * dy / dist, 0.0);
    sim_mtn.state.q = Quat::from_euler(0.0, 0.0, heading);
    sim_mtn.mode = ControlMode::Manual;
    sim_mtn.input.throttle = 0.8;
    for _ in 0..((dist / 10.0) / DT) as usize {
        sim_mtn.step();
        if sim_mtn.is_crashed() {
            break;
        }
    }
    assert_eq!(
        sim_mtn.status,
        FlightStatus::Crashed(CrashReason::Terrain),
        "expected terrain crash, got {:?}",
        sim_mtn.status
    );
}

#[test]
fn runway_ground_roll_accelerates_steers_and_lifts_off() {
    let (p, m) = project(Sample::FixedWing);
    let mut sim = sim_for_project_with(
        &p,
        &m,
        StartPosition::Ground,
        TakeoffMode::Runway,
        EnvironmentKind::Runway,
    );
    assert!(sim.params.has_wheels);
    let vs = sim.params.v_stall;
    sim.input.throttle = 1.0;

    let mut liftoff: Option<(f64, f64, f64)> = None;
    for _ in 0..(20.0 / DT) as usize {
        let v = sim.telemetry.airspeed;
        sim.input.pitch = if v >= 1.08 * vs { 0.65 } else { 0.0 };
        sim.step();
        if sim.status == FlightStatus::Airborne && sim.telemetry.agl > 0.6 {
            liftoff = Some((sim.t, sim.state.pos.x, v));
            break;
        }
    }
    let (t_lo, dist, v_lo) = liftoff.expect("fixed-wing did not lift off");
    eprintln!(
        "Sample FixedWing runway takeoff: dist={dist:.1} m, speed={v_lo:.2} m/s ({:.2} x Vstall={vs:.2}), t={t_lo:.2} s",
        v_lo / vs
    );
    assert!(!sim.is_crashed());
    assert!(
        (10.0..=350.0).contains(&dist),
        "implausible takeoff distance {dist} m"
    );
    assert!(
        v_lo >= 1.0 * vs && v_lo <= 1.45 * vs,
        "liftoff speed {v_lo} vs Vstall {vs}"
    );

    // Verify grass rolling resistance is higher than asphalt and brakes slow ground roll.
    assert!(
        EnvironmentKind::OpenField.rolling_mu(0.0, 0.0)
            > EnvironmentKind::Runway.rolling_mu(0.0, 0.0)
    );
    let mut sim_steer = sim_for_project_with(
        &p,
        &m,
        StartPosition::Ground,
        TakeoffMode::Runway,
        EnvironmentKind::Runway,
    );
    sim_steer.input.throttle = 0.5;
    sim_steer.input.yaw = 0.6;
    run(&mut sim_steer, 2.5);
    assert!(
        sim_steer.telemetry.heading > 0.05 && sim_steer.state.pos.y > 0.1,
        "nose-wheel steering should turn right: heading={}, y={}",
        sim_steer.telemetry.heading,
        sim_steer.state.pos.y
    );

    // Brakes slow down rolling speed compared to free rolling.
    let mut sim_roll = sim_for_project_with(
        &p,
        &m,
        StartPosition::Ground,
        TakeoffMode::Runway,
        EnvironmentKind::Runway,
    );
    sim_roll.input.throttle = 0.8;
    run(&mut sim_roll, 2.0);
    sim_roll.input.throttle = 0.0;
    let mut sim_brake = sim_roll.clone();
    sim_brake.brakes = 1.0;
    run(&mut sim_roll, 2.0);
    run(&mut sim_brake, 2.0);
    assert!(
        sim_brake.telemetry.groundspeed < sim_roll.telemetry.groundspeed * 0.5,
        "brakes {} vs coast {}",
        sim_brake.telemetry.groundspeed,
        sim_roll.telemetry.groundspeed
    );

    // VTOL in fixed-wing runway takeoff mode also gets wheels and lifts off horizontally.
    let (pv, mv) = project(Sample::Vtol);
    let mut sim_vtol_rw = sim_for_project_with(
        &pv,
        &mv,
        StartPosition::Ground,
        TakeoffMode::Runway,
        EnvironmentKind::Runway,
    );
    assert!(sim_vtol_rw.params.has_wheels);
    assert_eq!(sim_vtol_rw.phase, VtolPhase::Cruise);
    sim_vtol_rw.input.throttle = 1.0;
    let mut vtol_air = false;
    for _ in 0..(25.0 / DT) as usize {
        if sim_vtol_rw.telemetry.airspeed >= 1.08 * sim_vtol_rw.params.v_stall {
            sim_vtol_rw.input.pitch = 0.65;
        }
        sim_vtol_rw.step();
        if sim_vtol_rw.status == FlightStatus::Airborne && sim_vtol_rw.telemetry.agl > 0.6 {
            vtol_air = true;
            break;
        }
    }
    assert!(vtol_air, "VTOL fixed-wing runway takeoff failed");
}

#[test]
fn hand_launch_has_no_wheels_in_physics_or_geometry() {
    use crate::calc::presets::mission::Launch;
    use crate::geom3d::{PartKind, ViewOptions, build, from_project};

    for dt in [
        DroneType::FixedWingConventional,
        DroneType::FixedWingVTail,
        DroneType::FlyingWing,
    ] {
        let mut p = Project::default();
        p.set_drone_type(dt);
        p.simple.mission.launch = Launch::Hand;
        let m = crate::calc::evaluate(&p, &Library::embedded());
        let sim = sim_for_project(&p, &m, StartPosition::Ground);
        assert!(!sim.params.has_wheels, "{dt:?} should not have wheels");
        assert!(
            sim.params
                .contacts
                .iter()
                .all(|c| !matches!(c.kind, vehicle::ContactKind::Wheel { .. })),
            "{dt:?} still has wheel contact"
        );
        let opts = ViewOptions {
            wheels_override: Some(false),
            ..Default::default()
        };
        let sc = build(&from_project(&p, &m, &opts));
        assert!(
            sc.part(PartKind::Wheel).is_none(),
            "{dt:?} drew wheels in hand launch"
        );

        // Runway launch adds tricycle wheels for all fixed-wing types (incl. FlyingWing).
        p.simple.mission.launch = Launch::Runway;
        let sim_rw = sim_for_project(&p, &m, StartPosition::Ground);
        assert!(
            sim_rw.params.has_wheels,
            "{dt:?} should have wheels on runway"
        );
        let opts_rw = ViewOptions {
            wheels_override: Some(true),
            ..Default::default()
        };
        let sc_rw = build(&from_project(&p, &m, &opts_rw));
        assert!(
            sc_rw.part(PartKind::Wheel).is_some(),
            "{dt:?} missing wheels on runway"
        );
    }
}
