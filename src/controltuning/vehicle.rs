// SPDX-License-Identifier: AGPL-3.0-or-later
//! Vehicle data used by the control-tuning and motor-loss analyses: rotor layout relative to the
//! CG, per-rotor thrust/torque model, motor time constant and the inertia estimate.
//!
//! Body axes: x forward, y right, z down. A rotor spinning counter-clockwise seen from above
//! (`spin = +1`) produces a positive (nose-right) reaction yaw moment. Thrust `T` at `(x, y)`
//! produces roll `-y·T` and pitch `x·T`.
use crate::calc::Model;
use crate::calc::multirotor::motor_positions;
use crate::project::{DroneType, Link, Project};

/// Thrust coefficient used by the project's hover-RPM estimate (`calc::evaluate`), reused here so
/// that RPM values agree with the rest of the program: `T = C_T·ρ·n²·D⁴` (n in rev/s).
pub const CT_STATIC: f64 = 0.10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleKind {
    Multirotor,
    /// Quadplane: lift rotors for hover plus a fixed wing for cruise.
    Vtol,
    FixedWing,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rotor {
    /// Position relative to the CG [m].
    pub x: f64,
    pub y: f64,
    /// +1 = counter-clockwise seen from above, -1 = clockwise.
    pub spin: f64,
    /// Maximum static thrust [N].
    pub t_max: f64,
    /// Coaxial lower rotor (drawn smaller in the top view).
    pub lower: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VehicleData {
    pub kind: VehicleKind,
    /// Mass [kg] and weight [N] that the hover rotors must carry.
    pub mass: f64,
    pub weight: f64,
    /// Principal moments of inertia about the CG [kg·m²]: Ixx, Iyy, Izz.
    pub inertia: [f64; 3],
    /// Hover rotors relative to the CG (empty for a pure fixed wing).
    pub rotors: Vec<Rotor>,
    pub arm_count: u32,
    pub coaxial: bool,
    /// Prop diameter [m] of the hover rotors (cruise prop for a pure fixed wing).
    pub d_prop: f64,
    /// Torque-to-thrust ratio Q/T [m] of one rotor.
    pub kq: f64,
    /// Hover thrust per rotor divided by its maximum thrust (0..1).
    pub hover_frac: f64,
    pub hover_rpm: f64,
    pub max_rpm: f64,
    /// Estimated (or user-set) first-order motor + prop time constant [s].
    pub motor_tau: f64,
    pub motor_tau_estimated: bool,
    /// Mean rotor distance from the CG [m] (normalisation length).
    pub arm_ref: f64,
}

fn finite_pos(x: f64, fallback: f64) -> f64 {
    if x.is_finite() && x > 0.0 {
        x
    } else {
        fallback
    }
}

pub fn kind_of(dt: DroneType) -> VehicleKind {
    match dt {
        DroneType::Multirotor => VehicleKind::Multirotor,
        DroneType::VtolQuadplane => VehicleKind::Vtol,
        _ => VehicleKind::FixedWing,
    }
}

/// Arm count normalisation identical to `calc::multirotor::evaluate_multirotor`.
fn arm_count(p: &Project) -> u32 {
    match p.mr_arm_count {
        3 | 4 | 6 | 8 => p.mr_arm_count,
        n if n < 4 => 3,
        n if n < 6 => 4,
        n if n < 8 => 6,
        _ => 8,
    }
}

/// Empirical first-order time constant of a motor + prop pair (thrust response to a command
/// step), as a function of prop diameter. Typical bench measurements put 5-inch racing props
/// around 15–25 ms, 10-inch props around 40–60 ms and 15-inch props near 0.1 s; the fit
/// `τ ≈ 20 ms·(D / 5 in)^1.5` follows that trend. It is an estimate only (it depends on motor
/// Kv, voltage, ESC and prop inertia); the UI lets the user override it.
pub fn estimate_motor_tau(d_prop_m: f64) -> f64 {
    let d = finite_pos(d_prop_m, 0.127);
    (0.020 * (d / 0.127).powf(1.5)).clamp(0.008, 0.25)
}

/// Q/T [m] from momentum theory: with `C_P = C_T^1.5 / (√2·FM)` and `C_Q = C_P / 2π`,
/// `Q/T = C_Q·D / C_T`. For FM 0.65 and a 10-inch prop this gives ≈ 14 mm, in line with
/// typical published k_Q/k_T values of 10–20 mm.
pub fn torque_ratio(d_prop_m: f64, fm: f64) -> f64 {
    let fm = if fm.is_finite() {
        fm.clamp(0.4, 0.85)
    } else {
        0.65
    };
    let cp = CT_STATIC.powf(1.5) / (std::f64::consts::SQRT_2 * fm);
    let cq = cp / (2.0 * std::f64::consts::PI);
    cq * finite_pos(d_prop_m, 0.127) / CT_STATIC
}

fn rpm_for_thrust(t: f64, rho: f64, d: f64) -> f64 {
    let n_sq = t / (CT_STATIC * rho * d.powi(4));
    if n_sq.is_finite() && n_sq > 0.0 {
        n_sq.sqrt() * 60.0
    } else {
        0.0
    }
}

/// Hover rotors with spin directions. Ring rotors alternate spin (PNPN…); a coaxial arm carries
/// an upper rotor with the ring spin and a lower rotor with the opposite spin. Positions are
/// relative to `(cx, cy)`.
fn hover_rotors(p: &Project, cx: f64, cy: f64) -> Vec<Rotor> {
    let n = arm_count(p);
    let pos = motor_positions(p.mr_frame_size, n, p.mr_layout, p.mr_h_lx, p.mr_h_wy);
    let t_max = p.mr_t_max_motor.max(0.0);
    let mut out = Vec::with_capacity(pos.len() * 2);
    for (i, &(x, y)) in pos.iter().enumerate() {
        let s = if i % 2 == 0 { 1.0 } else { -1.0 };
        out.push(Rotor {
            x: x - cx,
            y: y - cy,
            spin: s,
            t_max,
            lower: false,
        });
        if p.mr_coaxial {
            out.push(Rotor {
                x: x - cx,
                y: y - cy,
                spin: -s,
                t_max,
                lower: true,
            });
        }
    }
    out
}

/// Inertia estimate about the CG [Ixx, Iyy, Izz] in kg·m².
///
/// This is the single swap point for a proper mass-properties module: everything else in
/// `controltuning` only calls this function. The estimate reuses the existing mass and CG data:
/// * Multirotor: motors as point masses at the rotor positions, the frame split into a centre
///   plate (disc, radius `mr_r_plate`) and uniform arms, ESCs at 60 % arm radius, and the rest of
///   the take-off mass distributed over the multirotor component list (`mr_components`) with a
///   small box spread and a vertical offset term for hanging battery/payload.
/// * Fixed wing: the CG component table (`Model::stab.comps`, the same masses used for the CG)
///   as point masses about the CG, with spanwise spread for the wing rows (`m·b²/12`) and
///   lengthwise spread for fuselage/tail rows (`m·L²/12`). With no usable table it falls back to
///   typical non-dimensional radii of gyration (Roskam, *Airplane Design* Part V:
///   R̄x ≈ 0.25, R̄y ≈ 0.38, R̄z ≈ 0.39).
/// * VTOL: the fixed-wing estimate plus the lift motors as point masses.
pub fn inertia_estimate(p: &Project, m: &Model) -> [f64; 3] {
    match kind_of(p.drone_type) {
        VehicleKind::Multirotor => multirotor_inertia(p, m),
        VehicleKind::FixedWing => fixed_wing_inertia(p, m),
        VehicleKind::Vtol => {
            let mut i = fixed_wing_inertia(p, m);
            let m_mot = p.mr_m_motor.max(0.0);
            for r in hover_rotors(p, 0.0, 0.0) {
                i[0] += m_mot * r.y * r.y;
                i[1] += m_mot * r.x * r.x;
                i[2] += m_mot * (r.x * r.x + r.y * r.y);
            }
            i
        }
    }
}

fn multirotor_inertia(p: &Project, m: &Model) -> [f64; 3] {
    let n = arm_count(p);
    let cx = m.mr.cg_x;
    let cy = m.mr.cg_y;
    let rotors = hover_rotors(p, cx, cy);
    let mut i = [0.0_f64; 3];
    fn add_point(i: &mut [f64; 3], mass: f64, x: f64, y: f64) {
        if mass.is_finite() && mass > 0.0 && x.is_finite() && y.is_finite() {
            i[0] += mass * y * y;
            i[1] += mass * x * x;
            i[2] += mass * (x * x + y * y);
        }
    }
    let m_motor = p.mr_m_motor.max(0.0);
    for r in &rotors {
        add_point(&mut i, m_motor, r.x, r.y);
    }
    let r_out = 0.5 * finite_pos(p.mr_frame_size, 0.25);
    let r_in = p.mr_r_plate.clamp(0.0, 0.9 * r_out);
    let m_frame = p.mr_m_frame.max(0.0);
    let m_plate = 0.5 * m_frame;
    let m_arm = 0.5 * m_frame / n as f64;
    let arm_dirs = motor_positions(p.mr_frame_size, n, p.mr_layout, p.mr_h_lx, p.mr_h_wy);
    let esc_each = p.mr_m_esc_tot.max(0.0) / n as f64;
    for &(x, y) in &arm_dirs {
        let r = (x * x + y * y).sqrt().max(1e-6);
        let (c, s) = (x / r, y / r);
        // Uniform rod from r_in to r along (c, s): ∫ρ² dm = m·(r³ − r_in³) / (3·(r − r_in)).
        let r0 = r_in.min(0.9 * r);
        let k2 = (r.powi(3) - r0.powi(3)) / (3.0 * (r - r0).max(1e-9));
        // Shift of the rod about the CG is small; use the frame-centre value plus the parallel
        // axis offset of its mass centre.
        let rm = 0.5 * (r + r0);
        let (dx, dy) = (-cx, -cy);
        i[0] += m_arm * (s * s * k2 + 2.0 * s * rm * dy + dy * dy);
        i[1] += m_arm * (c * c * k2 + 2.0 * c * rm * dx + dx * dx);
        i[2] += m_arm * (k2 + 2.0 * rm * (c * dx + s * dy) + dx * dx + dy * dy);
        add_point(&mut i, esc_each, 0.6 * x - cx, 0.6 * y - cy);
    }
    // Centre plate as a disc.
    let rp = r_in.max(0.02);
    i[0] += m_plate * (rp * rp / 4.0 + cy * cy);
    i[1] += m_plate * (rp * rp / 4.0 + cx * cx);
    i[2] += m_plate * (rp * rp / 2.0 + cx * cx + cy * cy);

    // Remaining mass (battery, payload, FC, misc) over the component list positions.
    let m_known = m_motor * rotors.len() as f64 + m_frame + p.mr_m_esc_tot.max(0.0);
    let m_total = finite_pos(m.mr.m_mtow, m_known.max(0.1));
    let m_rest = (m_total - m_known).max(0.0);
    let comps: Vec<(f64, f64, f64)> = p
        .mr_components
        .iter()
        .filter(|c| c.mass.is_finite() && c.mass > 0.0)
        .map(|c| (c.mass, c.x, c.y))
        .collect();
    let comp_mass: f64 = comps.iter().map(|c| c.0).sum();
    if comp_mass > 0.0 {
        for (cm, x, y) in &comps {
            add_point(&mut i, m_rest * cm / comp_mass, x - cx, y - cy);
        }
    } else {
        add_point(&mut i, m_rest, -cx, -cy);
    }
    // Finite size of the central stack: box of half-width r_plate and height h.
    let a = rp;
    let h = 0.03 + 0.12 * r_out;
    i[0] += m_rest * (a * a / 3.0 + h * h / 3.0);
    i[1] += m_rest * (a * a / 3.0 + h * h / 3.0);
    i[2] += m_rest * (2.0 * a * a / 3.0);
    sanitize(i, m_total, r_out)
}

fn fixed_wing_inertia(p: &Project, m: &Model) -> [f64; 3] {
    let mass = finite_pos(m.weight.m_mtow, 1.0);
    let b = finite_pos(p.b, 1.0);
    let l = finite_pos(p.l_fuse, 0.6 * b);
    let comps = &m.stab.comps;
    let cg = &m.stab.cg_nose;
    let comp_mass: f64 = comps
        .iter()
        .filter(|c| c.mass.is_finite() && c.mass > 0.0)
        .map(|c| c.mass)
        .sum();
    let fallback = [
        mass * (0.25 * b / 2.0).powi(2),
        mass * (0.38 * l / 2.0).powi(2),
        mass * (0.39 * (b + l) / 4.0).powi(2),
    ];
    if comp_mass <= 0.0 || !cg.x.is_finite() {
        return sanitize(fallback, mass, b / 2.0);
    }
    let mut i = [0.0_f64; 3];
    for (k, c) in comps.iter().enumerate() {
        if !(c.mass.is_finite() && c.mass > 0.0) {
            continue;
        }
        let (dx, dy, dz) = (c.x - cg.x, c.y - cg.y, c.z - cg.z);
        let (dx, dy, dz) = (fin(dx), fin(dy), fin(dz));
        i[0] += c.mass * (dy * dy + dz * dz);
        i[1] += c.mass * (dx * dx + dz * dz);
        i[2] += c.mass * (dx * dx + dy * dy);
        match p.components.get(k).map(|c| c.link) {
            Some(Link::Wing) => {
                i[0] += c.mass * b * b / 12.0;
                i[2] += c.mass * b * b / 12.0;
            }
            Some(Link::FuseTail) => {
                i[1] += c.mass * l * l / 12.0;
                i[2] += c.mass * l * l / 12.0;
            }
            _ => {}
        }
    }
    // Mass not represented in the table sits at the CG with the fallback distribution.
    let missing = (mass - comp_mass).max(0.0) / mass;
    for (ik, fk) in i.iter_mut().zip(fallback) {
        *ik += missing * fk;
    }
    sanitize(i, mass, b / 2.0)
}

fn fin(x: f64) -> f64 {
    if x.is_finite() { x } else { 0.0 }
}

/// Guards against zero/NaN values: never below a tiny solid sphere of the vehicle's size.
fn sanitize(i: [f64; 3], mass: f64, r: f64) -> [f64; 3] {
    let floor = 0.02 * mass.max(0.01) * r.max(0.02).powi(2);
    i.map(|v| if v.is_finite() && v > floor { v } else { floor })
}

/// Collects the vehicle data for the analyses. `tau_override` replaces the motor time constant
/// estimate when set (> 0).
pub fn vehicle_data(p: &Project, m: &Model, tau_override: Option<f64>) -> VehicleData {
    let kind = kind_of(p.drone_type);
    let rho = finite_pos(m.atmo.rho, 1.225);
    let (mass, cx, cy) = match kind {
        VehicleKind::Multirotor => (finite_pos(m.mr.m_mtow, 0.5), m.mr.cg_x, m.mr.cg_y),
        // Quadplane lift rotors are laid out around the CG (simplification).
        _ => (finite_pos(m.weight.m_mtow, 1.0), 0.0, 0.0),
    };
    let (cx, cy) = (fin(cx), fin(cy));
    let weight = mass * crate::calc::atmosphere::G;
    let rotors = if kind == VehicleKind::FixedWing {
        Vec::new()
    } else {
        hover_rotors(p, cx, cy)
    };
    let d_prop = if kind == VehicleKind::FixedWing {
        finite_pos(p.d_prop, 0.25)
    } else {
        finite_pos(p.mr_d_prop, 0.127)
    };
    let t_max = finite_pos(p.mr_t_max_motor, 1.0);
    let n = rotors.len().max(1) as f64;
    let t_hover = weight / n;
    let hover_frac = (t_hover / t_max).clamp(0.0, 1.5);
    let mut hover_rpm = rpm_for_thrust(t_hover, rho, d_prop);
    if kind == VehicleKind::Multirotor
        && p.mr_hover_rpm_override.is_finite()
        && p.mr_hover_rpm_override > 0.0
    {
        hover_rpm = p.mr_hover_rpm_override;
    }
    let max_rpm = if hover_frac > 0.0 {
        hover_rpm / hover_frac.sqrt()
    } else {
        rpm_for_thrust(t_max, rho, d_prop)
    };
    let (motor_tau, est) = match tau_override {
        Some(t) if t.is_finite() && t > 0.0 => (t.clamp(0.003, 0.5), false),
        _ => (estimate_motor_tau(d_prop), true),
    };
    let arm_ref = if rotors.is_empty() {
        0.0
    } else {
        rotors
            .iter()
            .map(|r| (r.x * r.x + r.y * r.y).sqrt())
            .sum::<f64>()
            / n
    };
    VehicleData {
        kind,
        mass,
        weight,
        inertia: inertia_estimate(p, m),
        rotors,
        arm_count: arm_count(p),
        coaxial: p.mr_coaxial,
        d_prop,
        kq: torque_ratio(d_prop, p.fm),
        hover_frac,
        hover_rpm: fin(hover_rpm),
        max_rpm: fin(max_rpm),
        motor_tau,
        motor_tau_estimated: est,
        arm_ref: finite_pos(arm_ref, 0.1),
    }
}
