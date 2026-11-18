// SPDX-License-Identifier: AGPL-3.0-or-later
//! Headless tests of the mass-properties estimate.
use super::*;
use crate::calc::{Library, evaluate};
use crate::project::{Component, DroneType, Link, Project};

const EXAMPLES: [&str; 6] = [
    include_str!("../../examples/fixed_wing_2kg_1600mm.json"),
    include_str!("../../examples/flying_wing_1200mm.json"),
    include_str!("../../examples/hexa_550_x.json"),
    include_str!("../../examples/quad_250_x.json"),
    include_str!("../../examples/quad_275_x.json"),
    include_str!("../../examples/vtol_quadplane.json"),
];

fn samples() -> Vec<(String, Project)> {
    let mut v = Vec::new();
    for dt in DroneType::ALL {
        let mut p = Project::default();
        p.set_drone_type(dt);
        v.push((format!("default {dt:?}"), p));
    }
    for (i, js) in EXAMPLES.iter().enumerate() {
        v.push((format!("example {i}"), Project::from_json(js).unwrap()));
    }
    v
}

fn eval(p: &Project) -> Model {
    evaluate(p, &Library::embedded())
}

fn rel(a: f64, b: f64) -> f64 {
    ((a - b) / b).abs()
}

/// Sylvester's criterion on a symmetric 3x3 tensor.
fn positive_definite(i: &M3) -> bool {
    let d2 = i[0][0] * i[1][1] - i[0][1] * i[1][0];
    let d3 = i[0][0] * (i[1][1] * i[2][2] - i[1][2] * i[2][1])
        - i[0][1] * (i[1][0] * i[2][2] - i[1][2] * i[2][0])
        + i[0][2] * (i[1][0] * i[2][1] - i[1][1] * i[2][0]);
    i[0][0] > 0.0 && d2 > 0.0 && d3 > 0.0
}

#[test]
fn point_masses_match_closed_form() {
    let mut p = Project::default();
    p.components = ["a", "b"]
        .iter()
        .zip([0.2, 0.4])
        .map(|(n, x)| Component {
            name: (*n).into(),
            link: Link::None,
            mass: 0.5,
            x,
            y: 0.0,
            z: 0.0,
        })
        .collect();
    let mp = compute(&p, &eval(&p));
    assert!(rel(mp.total_mass_kg, 1.0) < 1e-12);
    assert!(rel(mp.cg_m[0], 0.3) < 1e-12);
    // Two 0.5 kg masses 0.1 m either side of the CG along x.
    assert!(mp.inertia_kgm2[0][0].abs() < 1e-15);
    assert!(rel(mp.inertia_kgm2[1][1], 0.01) < 1e-9);
    assert!(rel(mp.inertia_kgm2[2][2], 0.01) < 1e-9);
}

#[test]
fn inertia_is_symmetric_positive_definite_for_every_vehicle() {
    for (name, p) in samples() {
        let mp = compute(&p, &eval(&p));
        assert!(mp.valid, "{name}");
        let i = mp.inertia_kgm2;
        let scale = i[0][0].max(i[1][1]).max(i[2][2]);
        for (r, row) in i.iter().enumerate() {
            for (c, v) in row.iter().enumerate() {
                assert!(v.is_finite(), "{name}");
                assert!((v - i[c][r]).abs() <= 1e-12 * scale, "{name} sym");
            }
        }
        assert!(positive_definite(&i), "{name}: {i:?}");
        // Triangle inequality of principal moments (physical tensor).
        assert!(i[0][0] + i[1][1] >= i[2][2] * (1.0 - 1e-9), "{name}");
        assert!(i[1][1] + i[2][2] >= i[0][0] * (1.0 - 1e-9), "{name}");
        assert!(i[0][0] + i[2][2] >= i[1][1] * (1.0 - 1e-9), "{name}");
    }
}

#[test]
fn symmetric_aircraft_has_zero_cg_y_and_cross_products() {
    for (name, p) in samples() {
        if p.components.iter().any(|c| c.y != 0.0) || p.mr_components.iter().any(|c| c.y != 0.0) {
            continue;
        }
        let mp = compute(&p, &eval(&p));
        let i = mp.inertia_kgm2;
        let scale = i[0][0].max(i[1][1]).max(i[2][2]);
        assert!(mp.cg_m[1].abs() < 1e-12, "{name}: cg y {}", mp.cg_m[1]);
        assert!(i[0][1].abs() < 1e-12 * scale, "{name} Ixy");
        assert!(i[1][2].abs() < 1e-12 * scale, "{name} Iyz");
    }
    // Make sure the default projects were actually covered.
    let p = Project::default();
    assert!(p.components.iter().all(|c| c.y == 0.0));
}

#[test]
fn total_mass_and_cg_match_existing_breakdown() {
    for (name, p) in samples() {
        let m = eval(&p);
        let mp = compute(&p, &m);
        if p.drone_type == DroneType::Multirotor {
            let listed: f64 = p.mr_components.iter().map(|c| c.mass).sum::<f64>()
                + p.mr_m_frame
                + m.mr.motor_coords.len() as f64
                    * p.mr_m_motor
                    * if p.mr_coaxial { 2.0 } else { 1.0 };
            assert!(rel(mp.total_mass_kg, listed) < 1e-12, "{name}");
            assert!((mp.cg_m[0] - m.mr.cg_x).abs() < 1e-12, "{name} cg x");
            assert!((mp.cg_m[1] - m.mr.cg_y).abs() < 1e-12, "{name} cg y");
            assert!(
                (mp.thrust.offset_m - m.mr.cg_offset_d).abs() < 1e-12,
                "{name}"
            );
            assert_eq!(mp.thrust.exceeds, !m.mr.cg_offset_ok, "{name}");
        } else {
            assert!(rel(mp.total_mass_kg, m.stab.cg_nose.mass) < 1e-12, "{name}");
            assert!((mp.cg_m[0] - m.stab.cg_nose.x).abs() < 1e-12, "{name} cg x");
            assert!((mp.cg_m[2] - m.stab.cg_nose.z).abs() < 1e-12, "{name} cg z");
            let pct = mp.thrust.cg_pct_mac.unwrap();
            assert!(
                rel(pct, 100.0 * m.stab.x_cg_rel / m.wing.mac) < 1e-12,
                "{name}"
            );
        }
    }
    // The default fixed-wing project closes its own breakdown (component table = MTOW).
    let p = Project::default();
    let m = eval(&p);
    let mp = compute(&p, &m);
    assert!(rel(mp.total_mass_kg, m.weight.m_mtow) < 1e-9);
    assert_eq!(mp.reference_mass_kg, m.weight.m_mtow);
}

#[test]
fn moving_a_component_moves_the_cg_the_right_way() {
    for dt in [DroneType::FixedWingConventional, DroneType::VtolQuadplane] {
        let mut p = Project::default();
        p.set_drone_type(dt);
        let before = compute(&p, &eval(&p));
        let row = p
            .components
            .iter()
            .position(|c| c.link == Link::Battery)
            .unwrap();
        let m_batt = eval(&p).stab.comps[row].mass;
        p.components[row].x += 0.05;
        p.components[row].z += 0.02;
        let after = compute(&p, &eval(&p));
        assert!(
            (after.cg_m[0] - before.cg_m[0] - m_batt * 0.05 / after.total_mass_kg).abs() < 1e-12
        );
        assert!(
            (after.cg_m[2] - before.cg_m[2] - m_batt * 0.02 / after.total_mass_kg).abs() < 1e-12
        );
        assert!(after.cg_m[1].abs() < 1e-12);
        // CG % MAC grows when the battery moves aft.
        assert!(after.thrust.cg_pct_mac.unwrap() > before.thrust.cg_pct_mac.unwrap());
    }
    let mut p = Project::default();
    p.set_drone_type(DroneType::Multirotor);
    let before = compute(&p, &eval(&p));
    p.mr_components[0].y += 0.04;
    let after = compute(&p, &eval(&p));
    let dy = p.mr_components[0].mass * 0.04 / after.total_mass_kg;
    assert!((after.cg_m[1] - before.cg_m[1] - dy).abs() < 1e-12);
    assert!(after.thrust.offset_m > before.thrust.offset_m);
}

#[test]
fn heavier_wing_span_raises_roll_inertia() {
    let mut p = Project::default();
    let a = compute(&p, &eval(&p)).inertia_kgm2[0][0];
    p.b *= 1.2;
    p.s *= 1.2;
    let b = compute(&p, &eval(&p)).inertia_kgm2[0][0];
    assert!(b > a);
}

#[test]
fn multirotor_offset_is_reported_in_percent_of_frame_size() {
    let mut p = Project::default();
    p.set_drone_type(DroneType::Multirotor);
    p.mr_components[0].x += 0.02;
    let m = eval(&p);
    let mp = compute(&p, &m);
    assert_eq!(mp.thrust.kind, OffsetKind::MotorCentroid);
    assert!(
        rel(
            mp.thrust.offset_pct,
            100.0 * mp.thrust.offset_m / p.mr_frame_size
        ) < 1e-12
    );
    // Same limit as the existing multirotor CG check.
    assert!(rel(mp.thrust.limit_pct.unwrap(), 100.0 * CG_OFFSET_MAX_FRAC) < 1e-12);
    assert_eq!(mp.thrust.exceeds, !m.mr.cg_offset_ok);
}

fn multirotor_project() -> Project {
    let mut p = Project::default();
    p.set_drone_type(DroneType::Multirotor);
    p
}

#[test]
fn hub_without_plate_radius_is_small() {
    let mut p = multirotor_project();
    p.mr_r_plate = 0.0;
    let m = eval(&p);
    let mp = compute(&p, &m);
    let hub = mp
        .items
        .iter()
        .find(|i| i.name_key == "massprops.item.frame_plate")
        .expect("hub item");
    let r = HUB_FRACTION * p.mr_frame_size;
    assert!(rel(hub.own_inertia[2][2], 0.5 * hub.mass_kg * r * r) < 1e-9);
}

#[test]
fn every_motor_and_arm_is_listed_at_its_own_position() {
    let p = multirotor_project();
    let m = eval(&p);
    let mp = compute(&p, &m);
    let motors: Vec<&MassItem> = mp
        .items
        .iter()
        .filter(|i| i.name_key == "massprops.item.motors")
        .collect();
    assert_eq!(motors.len(), m.mr.motor_coords.len());
    for (it, &(x, y)) in motors.iter().zip(&m.mr.motor_coords) {
        assert!((it.pos_m[0] - x).abs() < 1e-12 && (it.pos_m[1] - y).abs() < 1e-12);
    }
    let arms = mp
        .items
        .iter()
        .filter(|i| i.name_key == "massprops.item.arms")
        .count();
    assert_eq!(arms, m.mr.motor_coords.len());
}

#[test]
fn frame_group_stays_at_the_frame_centre_for_asymmetric_layouts() {
    let p = multirotor_project();
    let mut m = eval(&p);
    // Pull one motor outwards: the arms are no longer symmetric.
    m.mr.motor_coords[0].0 += 0.1;
    let mp = compute(&p, &m);
    let (mut mass, mut cg) = (0.0, [0.0; 2]);
    for i in mp.items.iter().filter(|i| {
        i.name_key == "massprops.item.frame_plate" || i.name_key == "massprops.item.arms"
    }) {
        mass += i.mass_kg;
        cg[0] += i.mass_kg * i.pos_m[0];
        cg[1] += i.mass_kg * i.pos_m[1];
    }
    assert!(rel(mass, p.mr_m_frame) < 1e-9);
    assert!(cg[0].abs() / mass < 1e-12 && cg[1].abs() / mass < 1e-12);
    // Longer arm carries more mass.
    let arms: Vec<&MassItem> = mp
        .items
        .iter()
        .filter(|i| i.name_key == "massprops.item.arms")
        .collect();
    assert!(arms[0].mass_kg > arms[1].mass_kg);
}

#[test]
fn non_finite_geometry_does_not_poison_the_inertia() {
    for (name, mut p) in samples() {
        if p.drone_type == DroneType::Multirotor {
            continue;
        }
        p.l_h = f64::NAN;
        p.x_ac_frac = f64::INFINITY;
        p.sweep_le_deg = f64::NAN;
        let m = eval(&p);
        let mp = compute(&p, &m);
        if !mp.valid {
            continue;
        }
        assert!(
            mp.inertia_kgm2.iter().flatten().all(|v| v.is_finite()),
            "{name}"
        );
    }
}

#[test]
fn cg_percent_of_mac_needs_a_valid_mac() {
    let p = Project::default();
    let mut m = eval(&p);
    m.wing.mac = 0.0;
    assert!(compute(&p, &m).thrust.cg_pct_mac.is_none());
}

#[test]
fn placing_a_body_sets_its_centre_exactly() {
    let b = Body {
        mass: 1.0,
        cg: [f64::NAN; 3],
        inertia: [[0.0; 3]; 3],
    };
    assert_eq!(b.placed_at([1.0, 2.0, 3.0]).cg, [1.0, 2.0, 3.0]);
}
