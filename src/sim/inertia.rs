// SPDX-License-Identifier: AGPL-3.0-or-later
//! Inertia tensor estimate for the flight simulator.
//!
//! This is deliberately kept behind the single function [`estimate`] so that a dedicated
//! mass-properties module can replace it without touching the rest of the simulator.
//! The estimate uses the component table (point masses), the wing as a thin plate, the
//! fuselage/tail group as a slender rod and, for rotorcraft, motors at the rotor positions.
use super::math::Inertia;
use crate::calc::Model;
use crate::project::{DroneType, Link, Project};

fn finite_pos(x: f64, fallback: f64) -> f64 {
    if x.is_finite() && x > 0.0 {
        x
    } else {
        fallback
    }
}

/// Inertia about the design CG for the design take-off mass, body axes FRD.
pub fn estimate(p: &Project, m: &Model) -> Inertia {
    let mass = super::vehicle::design_mass(p, m);
    let raw = match p.drone_type {
        DroneType::Multirotor => multirotor(p, m),
        DroneType::VtolQuadplane => {
            let mut i = fixed_wing(p, m);
            let lift = lift_rotors(p, m);
            i.ixx += lift.ixx;
            i.iyy += lift.iyy;
            i.izz += lift.izz;
            i
        }
        _ => fixed_wing(p, m),
    };
    sanitize(raw, mass, characteristic_length(p, m))
}

fn characteristic_length(p: &Project, m: &Model) -> f64 {
    if p.drone_type == DroneType::Multirotor {
        finite_pos(p.mr_frame_size, 0.25)
    } else {
        finite_pos(p.b, 1.0)
            .max(finite_pos(p.l_fuse, 0.5))
            .max(m.wing.mac)
    }
}

/// Guarantees a physically valid tensor: every radius of gyration between 5 % and 60 % of
/// the characteristic length, and the triangle inequality between the principal moments.
fn sanitize(i: Inertia, mass: f64, len: f64) -> Inertia {
    let lo = mass * (0.05 * len).powi(2);
    let hi = mass * (0.6 * len).powi(2);
    let c = |v: f64| finite_pos(v, lo).clamp(lo, hi);
    let (mut ixx, mut iyy, mut izz) = (c(i.ixx), c(i.iyy), c(i.izz));
    // Triangle inequality (Izz <= Ixx + Iyy etc.).
    izz = izz.min(ixx + iyy);
    ixx = ixx.min(iyy + izz);
    iyy = iyy.min(ixx + izz);
    let ixz = if i.ixz.is_finite() {
        i.ixz.clamp(-0.3 * ixx.min(izz), 0.3 * ixx.min(izz))
    } else {
        0.0
    };
    Inertia { ixx, iyy, izz, ixz }
}

fn fixed_wing(p: &Project, m: &Model) -> Inertia {
    let cg = m.stab.cg_nose;
    let b = finite_pos(p.b, 1.0);
    let mac = finite_pos(m.wing.mac, 0.2);
    let l_fuse = finite_pos(p.l_fuse, 0.6);
    let mut i = Inertia {
        ixx: 0.0,
        iyy: 0.0,
        izz: 0.0,
        ixz: 0.0,
    };
    let total: f64 = m.stab.comps.iter().map(|c| c.mass.max(0.0)).sum();
    let scale = if total > 1e-6 {
        super::vehicle::design_mass(p, m) / total
    } else {
        1.0
    };
    for (k, c) in m.stab.comps.iter().enumerate() {
        let mass = (c.mass * scale).max(0.0);
        if !mass.is_finite() || mass <= 0.0 {
            continue;
        }
        let dx = cg.x - c.x; // forward positive
        let dy = c.y - cg.y;
        let dz = -(c.z - cg.z); // down positive
        let (dx, dy, dz) = (
            if dx.is_finite() { dx } else { 0.0 },
            if dy.is_finite() { dy } else { 0.0 },
            if dz.is_finite() { dz } else { 0.0 },
        );
        // Point-mass transfer terms.
        i.ixx += mass * (dy * dy + dz * dz);
        i.iyy += mass * (dx * dx + dz * dz);
        i.izz += mass * (dx * dx + dy * dy);
        i.ixz += mass * dx * dz;
        // Distributed own inertia for large items.
        match p.components.get(k).map(|c| c.link) {
            Some(Link::Wing) => {
                // Tapered thin plate: span^2/12 slightly reduced for taper.
                let k_taper = 0.85;
                i.ixx += mass * b * b / 12.0 * k_taper;
                i.izz += mass * (b * b / 12.0 * k_taper + mac * mac / 12.0);
                i.iyy += mass * mac * mac / 12.0;
            }
            Some(Link::FuseTail) => {
                if p.drone_type == DroneType::FlyingWing {
                    i.ixx += mass * b * b / 24.0;
                    i.izz += mass * b * b / 24.0;
                } else {
                    // Slender rod along x, tail group concentrated towards the end.
                    let l = l_fuse.max(p.l_h.abs() * 1.2);
                    i.iyy += mass * l * l / 9.0;
                    i.izz += mass * l * l / 9.0;
                }
            }
            _ => {}
        }
    }
    i
}

fn lift_rotors(p: &Project, m: &Model) -> Inertia {
    let r = finite_pos(p.mr_frame_size, 0.5) * 0.5;
    let n = finite_pos(m.mr.n_motors as f64, 4.0);
    let mm = finite_pos(p.mr_m_motor, 0.05) * n;
    // Lift motors on booms, roughly evenly spread at radius r.
    Inertia {
        ixx: mm * r * r * 0.5,
        iyy: mm * r * r * 0.5,
        izz: mm * r * r,
        ixz: 0.0,
    }
}

fn multirotor(p: &Project, m: &Model) -> Inertia {
    let mut i = Inertia {
        ixx: 0.0,
        iyy: 0.0,
        izz: 0.0,
        ixz: 0.0,
    };
    let (cx, cy) = (
        if m.mr.cg_x.is_finite() {
            m.mr.cg_x
        } else {
            0.0
        },
        if m.mr.cg_y.is_finite() {
            m.mr.cg_y
        } else {
            0.0
        },
    );
    let point = |i: &mut Inertia, mass: f64, x: f64, y: f64, h: f64| {
        if !(mass.is_finite() && mass > 0.0 && x.is_finite() && y.is_finite()) {
            return;
        }
        let (dx, dy) = (x - cx, y - cy);
        i.ixx += mass * (dy * dy + h * h / 12.0);
        i.iyy += mass * (dx * dx + h * h / 12.0);
        i.izz += mass * (dx * dx + dy * dy);
    };
    // Motors at the rotor positions (coaxial: two per arm at the same xy).
    let per_arm = if p.mr_coaxial { 2.0 } else { 1.0 };
    for &(x, y) in &m.mr.motor_coords {
        point(&mut i, p.mr_m_motor * per_arm, x, y, 0.03);
    }
    // Frame: disc of the plate plus arms as rods.
    let r_arm = finite_pos(p.mr_frame_size, 0.25) * 0.5;
    let m_frame = finite_pos(p.mr_m_frame, 0.1);
    i.ixx += m_frame * r_arm * r_arm / 6.0;
    i.iyy += m_frame * r_arm * r_arm / 6.0;
    i.izz += m_frame * r_arm * r_arm / 3.0;
    // Centre electronics.
    point(
        &mut i,
        p.mr_m_esc_tot + p.mr_m_fc + p.mr_m_misc,
        0.0,
        0.0,
        0.03,
    );
    // Component table (battery, payload ...) as small boxes.
    for c in &p.mr_components {
        point(&mut i, c.mass, c.x, c.y, 0.05);
    }
    // Battery from the weight model if not represented in the table.
    if p.mr_components.is_empty() {
        point(&mut i, m.weight.m_battery, 0.0, 0.0, 0.04);
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_is_valid_for_every_type() {
        let lib = crate::calc::Library::embedded();
        for dt in DroneType::ALL {
            let mut p = Project::default();
            p.set_drone_type(dt);
            let m = crate::calc::evaluate(&p, &lib);
            let i = estimate(&p, &m);
            assert!(i.is_valid(), "{dt:?}: {i:?}");
            if dt.is_fixed_wing() {
                // A wing dominated aircraft rolls easier than it yaws.
                assert!(i.izz >= i.ixx * 0.9, "{dt:?}: {i:?}");
            }
        }
    }
}
