// SPDX-License-Identifier: AGPL-3.0-or-later
//! Pilot inputs, flight modes and the simple autopilot used by the stabilised mode.
//!
//! - Multirotor (and VTOL in hover): angle mode with altitude hold, or rate (acro) mode with
//!   direct collective. Rotor thrusts come from a pseudo-inverse allocation of the rotor
//!   geometry; when a motor fails the fail-safe re-allocates over the remaining motors
//!   (dropping yaw control when the remaining set cannot provide it, as a real flight
//!   controller would have to).
//! - Fixed wing: bank / pitch attitude hold with turn coordination and flight-path based
//!   altitude assist when the pitch stick is centred; manual mode passes sticks straight to
//!   the surfaces (plus trim).
use super::aero::Surfaces;
use super::math::{Inertia, V3, wrap_pi};
use super::vehicle::{DELTA_MAX, Rotor, VehicleParams};
use crate::calc::atmosphere::G;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ControlMode {
    /// Attitude / altitude assist (multirotor: angle mode).
    #[default]
    Stabilized,
    /// Fixed wing: direct surfaces. Multirotor: rate (acro) mode with direct collective.
    Manual,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum VtolPhase {
    #[default]
    Hover,
    Transition,
    Cruise,
    BackTransition,
}

/// Stick positions: pitch + = pull (nose up), roll + = right, yaw + = nose right, all in
/// [-1, 1]; throttle in [0, 1].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PilotInput {
    pub pitch: f64,
    pub roll: f64,
    pub yaw: f64,
    pub throttle: f64,
}

impl PilotInput {
    pub fn sanitized(self) -> PilotInput {
        let c = |v: f64, lo: f64| if v.is_finite() { v.clamp(lo, 1.0) } else { 0.0 };
        PilotInput {
            pitch: c(self.pitch, -1.0),
            roll: c(self.roll, -1.0),
            yaw: c(self.yaw, -1.0),
            throttle: c(self.throttle, 0.0),
        }
    }
}

/// Everything the autopilot may read in one control step.
pub struct CtrlContext<'a> {
    pub params: &'a VehicleParams,
    pub mass: f64,
    pub inertia: Inertia,
    pub euler: (f64, f64, f64),
    pub rates: V3,
    pub airspeed: f64,
    pub alpha: f64,
    pub beta: f64,
    pub vz_up: f64,
    pub agl: f64,
    pub on_ground: bool,
    pub rho: f64,
    /// Battery voltage factor on available thrust (1.0 nominal).
    pub k_batt: f64,
    pub input: PilotInput,
    pub mode: ControlMode,
    pub phase: VtolPhase,
    pub failsafe: bool,
    /// Motors the flight controller knows to be dead (after the detection delay).
    pub known_failed: &'a [bool],
    pub trim_elevator: f64,
}

#[derive(Clone, Debug, Default)]
pub struct Actuators {
    /// Thrust fraction command per rotor in [0, 1] (same order as `params.rotors`).
    pub rotor: Vec<f64>,
    pub surfaces: Surfaces,
}

#[derive(Clone, Debug, Default)]
pub struct Autopilot {
    rate_i: V3,
    vz_i: f64,
    alt_hold: Option<f64>,
    fw_alt_hold: Option<f64>,
    pitch_i: f64,
    heading_hold: Option<f64>,
}

const STICK_DEADBAND: f64 = 0.05;
const MR_MAX_TILT: f64 = 0.61; // 35 deg
const FW_MAX_BANK: f64 = 0.785; // 45 deg
const FW_MAX_PITCH: f64 = 0.436; // 25 deg

fn deadband(x: f64) -> f64 {
    if x.abs() < STICK_DEADBAND {
        0.0
    } else {
        (x - STICK_DEADBAND * x.signum()) / (1.0 - STICK_DEADBAND)
    }
}

impl Autopilot {
    pub fn reset(&mut self) {
        *self = Autopilot::default();
    }

    /// Computes actuator commands. `dt` is the control period.
    pub fn compute(&mut self, c: &CtrlContext, dt: f64) -> Actuators {
        let p = c.params;
        let n = p.rotors.len();
        let mut out = Actuators {
            rotor: vec![0.0; n],
            surfaces: Surfaces::default(),
        };
        let inp = c.input.sanitized();
        let has_lift = p.lift_rotor_count() > 0;
        let has_wing = p.wing.is_some();
        let vtol = has_lift && has_wing;

        // Which subsystems are active.
        let (lift_active, fw_weight, pusher) = if vtol {
            let vs = p.v_stall.max(3.0);
            let w_aero = ((c.airspeed - 0.5 * vs) / (0.5 * vs)).clamp(0.0, 1.0);
            match c.phase {
                VtolPhase::Hover => (true, w_aero, 0.0),
                VtolPhase::Transition => (true, w_aero, 1.0),
                VtolPhase::BackTransition => (true, w_aero, 0.0),
                VtolPhase::Cruise => (false, 1.0, inp.throttle),
            }
        } else if has_lift {
            (true, 0.0, 0.0)
        } else {
            (false, 1.0, inp.throttle)
        };

        if lift_active {
            let thrusts = self.multirotor(c, &inp, dt, vtol);
            for (k, r) in p.rotors.iter().enumerate() {
                if r.lift {
                    out.rotor[k] = thrusts[k];
                }
            }
        } else {
            self.rate_i = V3::ZERO;
            self.vz_i = 0.0;
            self.alt_hold = None;
        }
        if has_wing {
            let s = self.fixed_wing(c, &inp, dt, vtol);
            out.surfaces = Surfaces {
                elevator: s.elevator * fw_weight + c.trim_elevator * (1.0 - fw_weight),
                aileron: s.aileron * fw_weight,
                rudder: s.rudder * fw_weight,
            };
            let pusher = if c.failsafe && !vtol { 0.0 } else { pusher };
            for (k, r) in p.rotors.iter().enumerate() {
                if !r.lift {
                    out.rotor[k] = pusher.clamp(0.0, 1.0);
                }
            }
        }
        out
    }

    fn multirotor(&mut self, c: &CtrlContext, inp: &PilotInput, dt: f64, vtol: bool) -> Vec<f64> {
        let p = c.params;
        let (phi, theta, psi) = c.euler;
        let w = c.rates;
        let i = c.inertia;
        let tau_m = p
            .rotors
            .iter()
            .filter(|r| r.lift)
            .map(|r| r.tau)
            .fold(0.05, f64::max);
        let kp_rate = (0.8 / tau_m).min(14.0);
        let kp = V3::new(kp_rate, kp_rate, 0.5 * kp_rate);
        let ki = V3::new(0.25 * kp_rate, 0.25 * kp_rate, 0.15 * kp_rate);
        let k_ang = 0.4 * kp_rate;
        let stabilized = c.mode == ControlMode::Stabilized || c.failsafe;

        // --- Desired body rates
        let rate_des = if stabilized {
            let (mut phi_cmd, mut theta_cmd) = (
                deadband(inp.roll) * MR_MAX_TILT,
                deadband(inp.pitch) * MR_MAX_TILT,
            );
            let mut yaw_rate = deadband(inp.yaw) * 2.0;
            if c.failsafe {
                phi_cmd = 0.0;
                theta_cmd = 0.0;
                yaw_rate = 0.0;
            }
            if vtol {
                match c.phase {
                    VtolPhase::Transition => theta_cmd = 0.03,
                    VtolPhase::BackTransition => theta_cmd = 0.09,
                    _ => {}
                }
            }
            // Heading hold when the yaw stick is centred.
            if yaw_rate == 0.0 && !c.on_ground && !c.failsafe {
                let h = *self.heading_hold.get_or_insert(psi);
                yaw_rate = (1.5 * wrap_pi(h - psi)).clamp(-1.0, 1.0);
            } else {
                self.heading_hold = None;
            }
            let phi_dot = k_ang * (phi_cmd - phi);
            let theta_dot = k_ang * (theta_cmd - theta);
            let (sp, cp) = phi.sin_cos();
            let (st, ct) = theta.sin_cos();
            V3::new(
                phi_dot - st * yaw_rate,
                cp * theta_dot + sp * ct * yaw_rate,
                -sp * theta_dot + cp * ct * yaw_rate,
            )
        } else {
            self.heading_hold = None;
            V3::new(inp.roll * 3.5, inp.pitch * 3.5, inp.yaw * 2.5)
        };
        let rate_des = rate_des.clamp_norm(6.0);

        // --- Rate loop -> torque
        let err = rate_des - w;
        let grounded = c.on_ground && (inp.throttle < 0.55 || !stabilized);
        if grounded {
            self.rate_i = V3::ZERO;
        } else {
            self.rate_i = (self.rate_i + err * dt).clamp_norm(1.0);
        }
        let ang_acc = err.mul_elem(kp) + self.rate_i.mul_elem(ki);
        let torque = i.diag().mul_elem(ang_acc) + w.cross(i.apply(w));

        // --- Collective
        let lift: Vec<&Rotor> = p.rotors.iter().filter(|r| r.lift).collect();
        let t_avail: f64 = p
            .rotors
            .iter()
            .enumerate()
            .filter(|(_, r)| r.lift)
            .filter(|(k, _)| !c.known_failed.get(*k).copied().unwrap_or(false))
            .map(|(_, r)| r.t_max * c.k_batt)
            .sum();
        let tilt = (phi.cos() * theta.cos()).clamp(0.5, 1.0);
        let collective = if stabilized {
            let mut vz_cmd = deadband((inp.throttle - 0.5) * 2.0) * 3.0;
            if c.failsafe {
                vz_cmd = -1.0;
            }
            if vtol && matches!(c.phase, VtolPhase::Transition | VtolPhase::BackTransition) {
                vz_cmd = 0.0;
            }
            if vz_cmd == 0.0 && !c.on_ground {
                let h = *self.alt_hold.get_or_insert(c.agl);
                vz_cmd = (0.8 * (h - c.agl)).clamp(-1.5, 1.5);
            } else {
                self.alt_hold = None;
            }
            if c.on_ground && vz_cmd <= 0.0 {
                self.vz_i = 0.0;
                0.0
            } else {
                let e = vz_cmd - c.vz_up;
                self.vz_i = (self.vz_i + 1.0 * e * dt).clamp(-4.0, 4.0);
                let a_cmd = (2.5 * e + self.vz_i).clamp(-6.0, 6.0);
                c.mass * (G + a_cmd) / tilt
            }
        } else {
            self.vz_i = 0.0;
            self.alt_hold = None;
            inp.throttle * lift.iter().map(|r| r.t_max * c.k_batt).sum::<f64>()
        };
        let collective = collective.clamp(0.0, t_avail.max(0.0));
        if c.on_ground && collective <= 0.0 {
            self.rate_i = V3::ZERO;
        }

        let thrusts = allocate(p, c.known_failed, c.k_batt, collective, torque);
        p.rotors
            .iter()
            .enumerate()
            .map(|(k, r)| {
                if r.lift {
                    let tmax = (r.t_max * c.k_batt).max(1e-6);
                    (thrusts[k] / tmax).clamp(0.0, 1.0)
                } else {
                    0.0
                }
            })
            .collect()
    }

    fn fixed_wing(&mut self, c: &CtrlContext, inp: &PilotInput, dt: f64, vtol: bool) -> Surfaces {
        let p = c.params;
        let Some(wa) = p.wing.as_ref() else {
            return Surfaces::default();
        };
        let manual = c.mode == ControlMode::Manual && !c.failsafe;
        if manual {
            self.pitch_i = 0.0;
            self.fw_alt_hold = None;
            return Surfaces {
                elevator: inp.pitch + c.trim_elevator,
                aileron: inp.roll,
                rudder: inp.yaw,
            }
            .clamped();
        }
        let (phi, theta, _) = c.euler;
        let w = c.rates;
        let i = c.inertia;
        let v = c.airspeed.max(1.0);
        let vs = p.v_stall.max(3.0);
        let qbar = (0.5 * c.rho * v * v).max(0.5 * c.rho * (0.6 * vs).powi(2));
        let qs = qbar * wa.s;
        let v_d = v.max(3.0);

        // --- Commands
        let mut phi_cmd = deadband(inp.roll) * FW_MAX_BANK;
        let airborne = !c.on_ground && c.agl > 0.5;
        let pitch_stick = deadband(inp.pitch);
        let gamma_from = |vz: f64| (vz / v).clamp(-0.5, 0.5).asin();
        let mut theta_cmd = if c.failsafe {
            phi_cmd = 0.0;
            self.fw_alt_hold = None;
            if c.agl < 3.0 {
                0.07
            } else {
                let gamma = (-0.08 + 0.02 * (c.airspeed - 1.4 * vs)).clamp(-0.4, 0.05);
                gamma + c.alpha
            }
        } else if vtol && c.phase == VtolPhase::BackTransition {
            self.fw_alt_hold.get_or_insert(c.agl);
            let h = self.fw_alt_hold.unwrap_or(c.agl);
            gamma_from((0.5 * (h - c.agl)).clamp(-2.0, 2.0)) + c.alpha
        } else if pitch_stick == 0.0 && airborne && c.airspeed > 1.1 * vs {
            let h = *self.fw_alt_hold.get_or_insert(c.agl);
            let vz_des = (0.5 * (h - c.agl)).clamp(-3.0, 3.0);
            gamma_from(vz_des) + c.alpha
        } else {
            self.fw_alt_hold = None;
            pitch_stick * FW_MAX_PITCH
        };
        theta_cmd = theta_cmd.clamp(-FW_MAX_PITCH, FW_MAX_PITCH);
        if vtol && c.phase == VtolPhase::BackTransition {
            phi_cmd = 0.0;
        }

        // --- Roll (dynamic inversion of the roll axis)
        let p_des = (3.5 * (phi_cmd - phi)).clamp(-2.5, 2.5);
        let ph = w.x * wa.b / (2.0 * v_d);
        let cl_cmd = i.ixx * 8.0 * (p_des - w.x) / (qs * wa.b) - wa.clp * ph;
        let aileron = cl_cmd / (wa.clda.max(1e-4) * DELTA_MAX);

        // --- Pitch
        let turn = if phi.cos() > 0.2 {
            G / v * phi.tan() * phi.sin()
        } else {
            0.0
        };
        let q_des = (3.0 * (theta_cmd - theta) * phi.cos() + turn).clamp(-1.5, 1.5);
        if airborne {
            self.pitch_i = (self.pitch_i + 0.3 * (theta_cmd - theta) * dt).clamp(-0.3, 0.3);
        } else {
            self.pitch_i = 0.0;
        }
        let qh = w.y * wa.mac / (2.0 * v_d);
        let alpha_w = c.alpha + wa.i_w;
        let cm_cmd = i.iyy * 8.0 * (q_des - w.y) / (qs * wa.mac)
            - wa.cmq * qh
            - wa.cma * (alpha_w - wa.alpha_w_trim).sin()
            + self.pitch_i;
        let elevator = cm_cmd / (wa.cm_de.max(1e-4) * DELTA_MAX);

        // --- Yaw: turn coordination + yaw stick
        let rudder = if wa.cn_dr > 1e-4 {
            let r_des = if airborne {
                G * phi.tan().clamp(-3.0, 3.0) / v.max(5.0) + deadband(inp.yaw) * 0.6
            } else {
                deadband(inp.yaw) * 0.6
            };
            let cn_cmd = i.izz * 4.0 * (r_des - w.z) / (qs * wa.b) + 0.5 * wa.cnb * c.beta.sin();
            if airborne {
                cn_cmd / (wa.cn_dr * DELTA_MAX)
            } else {
                // On the ground the rudder (and steered nose wheel) follow the stick.
                inp.yaw
            }
        } else {
            0.0
        };
        Surfaces {
            elevator,
            aileron,
            rudder,
        }
        .clamped()
    }
}

/// Solves a 4x4 (or smaller) linear system in place; returns false if singular.
fn invert(a: &mut [[f64; 4]; 4], dim: usize) -> Option<[[f64; 4]; 4]> {
    let mut inv = [[0.0; 4]; 4];
    for (k, row) in inv.iter_mut().enumerate().take(dim) {
        row[k] = 1.0;
    }
    for col in 0..dim {
        let piv = (col..dim).max_by(|&x, &y| a[x][col].abs().total_cmp(&a[y][col].abs()))?;
        if a[piv][col].abs() < 1e-9 {
            return None;
        }
        a.swap(col, piv);
        inv.swap(col, piv);
        let d = a[col][col];
        for j in 0..dim {
            a[col][j] /= d;
            inv[col][j] /= d;
        }
        for r in 0..dim {
            if r != col {
                let f = a[r][col];
                if f != 0.0 {
                    for j in 0..dim {
                        a[r][j] -= f * a[col][j];
                        inv[r][j] -= f * inv[col][j];
                    }
                }
            }
        }
    }
    Some(inv)
}

/// Thrust per rotor [N] for a collective thrust and body torque over the healthy lift
/// rotors (active-set least squares: saturated rotors are fixed at their limit and the
/// rest re-solved). Yaw is dropped when fewer than four rotors remain free, then roll /
/// pitch keep priority over collective.
pub fn allocate(
    p: &VehicleParams,
    failed: &[bool],
    k_batt: f64,
    collective: f64,
    torque: V3,
) -> Vec<f64> {
    // Yaw authority of rotor drag torque is small: never let it eat roll/pitch authority.
    let yaw_cap: f64 = 0.1
        * p.rotors
            .iter()
            .filter(|r| r.lift)
            .map(|r| r.t_max * k_batt * r.k_q)
            .sum::<f64>();
    let torque = V3::new(torque.x, torque.y, torque.z.clamp(-yaw_cap, yaw_cap));
    // Attitude has priority: lower the collective until the torque is achieved.
    let mut last = Vec::new();
    for scale in [1.0, 0.85, 0.7, 0.55, 0.4, 0.25] {
        let t = allocate_once(p, failed, k_batt, collective * scale, torque);
        let mut achieved = V3::ZERO;
        for (k, r) in p.rotors.iter().enumerate() {
            if r.lift {
                achieved += V3::new(-r.pos.y, r.pos.x, r.spin * r.k_q) * t[k];
            }
        }
        let err = V3::new(achieved.x - torque.x, achieved.y - torque.y, 0.0).norm();
        let ok = err <= 0.1 * V3::new(torque.x, torque.y, 0.0).norm() + 1e-3;
        last = t;
        if ok {
            break;
        }
    }
    last
}

fn allocate_once(
    p: &VehicleParams,
    failed: &[bool],
    k_batt: f64,
    collective: f64,
    torque: V3,
) -> Vec<f64> {
    let n = p.rotors.len();
    let mut out = vec![0.0; n];
    let healthy: Vec<usize> = (0..n)
        .filter(|&k| p.rotors[k].lift && !failed.get(k).copied().unwrap_or(false))
        .collect();
    if healthy.is_empty() {
        return out;
    }
    // Effectiveness rows: thrust, roll (-y T), pitch (x T), yaw (spin k_q T).
    let row = |k: usize| -> [f64; 4] {
        let r = &p.rotors[k];
        [1.0, -r.pos.y, r.pos.x, r.spin * r.k_q]
    };
    let tmax = |k: usize| p.rotors[k].t_max * k_batt;
    let desired = [collective, torque.x, torque.y, torque.z];
    // Minimum-norm solution over `free` for `target` using the first `dim` rows.
    let solve = |free: &[usize], target: [f64; 4], dim: usize| -> Option<Vec<f64>> {
        let mut bbt = [[0.0; 4]; 4];
        for &k in free {
            let b = row(k);
            for r in 0..dim {
                for c2 in 0..dim {
                    bbt[r][c2] += b[r] * b[c2];
                }
            }
        }
        let inv = invert(&mut bbt, dim)?;
        let mut y = [0.0; 4];
        for (r, yr) in y.iter_mut().enumerate().take(dim) {
            *yr = (0..dim).map(|c2| inv[r][c2] * target[c2]).sum();
        }
        Some(
            free.iter()
                .map(|&k| {
                    let b = row(k);
                    (0..dim).map(|r| b[r] * y[r]).sum()
                })
                .collect(),
        )
    };
    let mut fixed: Vec<(usize, f64)> = Vec::new();
    for _ in 0..8 {
        let free: Vec<usize> = healthy
            .iter()
            .copied()
            .filter(|k| !fixed.iter().any(|(f, _)| f == k))
            .collect();
        if free.is_empty() {
            break;
        }
        let mut target = desired;
        for &(k, t) in &fixed {
            let b = row(k);
            for r in 0..4 {
                target[r] -= b[r] * t;
            }
        }
        let dims: &[usize] = if free.len() >= 4 { &[4, 3, 1] } else { &[3, 1] };
        let Some(sol) = dims.iter().find_map(|&d| solve(&free, target, d)) else {
            break;
        };
        let mut violated = false;
        for (j, &k) in free.iter().enumerate() {
            out[k] = sol[j];
            if sol[j] < 0.0 {
                fixed.push((k, 0.0));
                violated = true;
            } else if sol[j] > tmax(k) {
                fixed.push((k, tmax(k)));
                violated = true;
            }
        }
        for &(k, t) in &fixed {
            out[k] = t;
        }
        if !violated {
            break;
        }
    }
    for &k in &healthy {
        out[k] = out[k].clamp(0.0, tmax(k));
    }
    out
}
