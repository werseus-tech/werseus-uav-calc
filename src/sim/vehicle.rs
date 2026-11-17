// SPDX-License-Identifier: AGPL-3.0-or-later
//! Physical vehicle description for the simulator, derived from `Project` + `Model`.
//!
//! Every number here comes from the existing design calculations where the project has one
//! (mass, CG, static margin, lift slope, CLmax, CD0, induced-drag factor, tail volumes, static
//! thrust, rotor max thrust, battery energy, efficiencies). Geometry (rotor, wheel and skid
//! positions) is taken from the same `geom3d` spec that the 3D view draws, so what you see
//! is what touches the ground. Values the project does not contain (control-surface
//! effectiveness, damping derivatives, motor lag, prop zero-thrust speed) are textbook
//! estimates and are named as such below.
use super::inertia;
use super::math::{Inertia, V3};
use crate::calc::Model;
use crate::calc::atmosphere::{G, isa};
use crate::calc::geometry::TailKind;
use crate::geom3d::{self, PartKind, ViewOptions};
use crate::project::{DroneType, Link, Project};

/// Maximum control-surface deflection [rad] (20 deg, typical RC setup).
pub const DELTA_MAX: f64 = 0.349;
/// Flap effectiveness tau for the surface chord ratios drawn by the 3D view
/// (aileron ~28 % chord, elevator ~38 %, rudder ~40 %, elevon ~25 %).
const TAU_AILERON: f64 = 0.45;
const TAU_ELEVATOR: f64 = 0.55;
const TAU_RUDDER: f64 = 0.58;
const TAU_ELEVON: f64 = 0.45;
/// Thrust coefficient used for the rpm estimate (same value as the hover-rpm estimate in
/// `calc::evaluate`).
const CT_STATIC: f64 = 0.10;

/// Which surfaces share one servo pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mixing {
    /// Separate elevator, aileron, rudder.
    Conventional,
    /// Ruddervators: left = e + r, right = e - r.
    VTail,
    /// Elevons: left = e + a, right = e - a (no rudder).
    Elevon,
}

/// Fixed-wing aerodynamic model (coefficients per radian, body/stability axes).
#[derive(Clone, Debug)]
pub struct WingAero {
    pub s: f64,
    pub b: f64,
    pub mac: f64,
    pub ar: f64,
    pub cla: f64,
    pub cl_max: f64,
    /// Scale applied to the attached-flow lift curve so the blended curve peaks at CLmax.
    pub cl_att_scale: f64,
    pub alpha_stall: f64,
    pub cd0: f64,
    pub k: f64,
    /// Wing angle of attack at zero body angle of attack (incidence incl. camber) [rad].
    pub i_w: f64,
    /// Wing angle of attack in trimmed cruise (Cm = 0 with zero elevator) [rad].
    pub alpha_w_trim: f64,
    pub sm: f64,
    pub cma: f64,
    pub cmq: f64,
    pub clq: f64,
    pub cm_de: f64,
    pub cl_de: f64,
    pub clda: f64,
    pub clp: f64,
    pub clb: f64,
    pub cyb: f64,
    pub cnb: f64,
    pub cnr: f64,
    pub cn_dr: f64,
    pub cy_dr: f64,
    pub cl_dr: f64,
    pub mixing: Mixing,
}

/// One propeller / rotor with its motor.
#[derive(Clone, Debug)]
pub struct Rotor {
    /// Hub position relative to the design CG, body FRD [m].
    pub pos: V3,
    /// Thrust direction, body FRD (unit).
    pub axis: V3,
    /// Static thrust at full throttle and nominal voltage [N].
    pub t_max: f64,
    pub diameter: f64,
    pub disk_area: f64,
    /// Yaw reaction torque sign (+1 nose right) and torque/thrust ratio [m].
    pub spin: f64,
    pub k_q: f64,
    /// Axial inflow speed where thrust has fallen to zero at full rpm [m/s].
    pub v_zero: f64,
    /// Motor + prop spin-up time constant [s].
    pub tau: f64,
    /// Lift rotor (multirotor / VTOL hover) versus forward propulsor.
    pub lift: bool,
    /// Tip speed at full rpm [m/s] (rotor in-plane drag scale).
    pub v_tip: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContactKind {
    /// Rolling wheel; `steer` follows the rudder command.
    Wheel { steer: bool },
    /// Landing skid / leg.
    Skid,
    /// Anything else (wing tip, nose, tail, prop): touching it hard is a crash.
    Structure,
}

#[derive(Clone, Copy, Debug)]
pub struct Contact {
    pub pos: V3,
    pub kind: ContactKind,
}

#[derive(Clone, Debug)]
pub struct Battery {
    pub v_nom: f64,
    pub energy_wh: f64,
    pub dod: f64,
    pub p_hotel: f64,
}

#[derive(Clone, Debug)]
pub struct VehicleParams {
    pub drone_type: DroneType,
    pub mass: f64,
    pub inertia: Inertia,
    pub wing: Option<WingAero>,
    pub rotors: Vec<Rotor>,
    pub contacts: Vec<Contact>,
    pub battery: Battery,
    pub eta_el: f64,
    pub eta_prop: f64,
    pub fm: f64,
    /// Body drag areas (CdA) along body x, y, z [m^2]; used for rotorcraft fuselage drag.
    pub drag_area: V3,
    pub payload_mass: f64,
    pub payload_pos: V3,
    /// Field elevation used for the ISA density [m].
    pub field_elevation: f64,
    pub v_stall: f64,
    pub v_cruise: f64,
    /// CG position in the 3D scene frame (x fwd, y left, z up), for rendering.
    pub scene_cg: [f32; 3],
    /// Crash threshold for vertical touchdown speed [m/s].
    pub crash_sink: f64,
}

/// Design take-off mass [kg] as used by the rest of the app.
pub fn design_mass(p: &Project, m: &Model) -> f64 {
    let v = if p.drone_type == DroneType::Multirotor {
        m.mr.m_mtow
    } else {
        m.weight.m_mtow
    };
    if v.is_finite() && v > 0.01 { v } else { 1.0 }
}

fn fp(x: f64, fallback: f64) -> f64 {
    if x.is_finite() && x > 0.0 {
        x
    } else {
        fallback
    }
}

fn finite_or(x: f64, fallback: f64) -> f64 {
    if x.is_finite() { x } else { fallback }
}

/// Rotor speed at full static thrust from T = CT rho n^2 D^4 [rev/s].
fn rps_for(t: f64, rho: f64, d: f64) -> f64 {
    (t / (CT_STATIC * rho * d.powi(4))).max(1.0).sqrt()
}

impl VehicleParams {
    pub fn from_project(p: &Project, m: &Model) -> VehicleParams {
        let dt = p.drone_type;
        let mass = design_mass(p, m);
        let field_elevation = finite_or(p.h, 0.0).clamp(0.0, 4000.0);
        let rho = isa(field_elevation).rho;
        let spec = geom3d::from_project(p, m, &ViewOptions::default());
        let cg = spec.cg.unwrap_or(geom3d::Vec3::ZERO);
        let to_body = |v: geom3d::Vec3| -> V3 {
            V3::new(
                (v.x - cg.x) as f64,
                -((v.y - cg.y) as f64),
                -((v.z - cg.z) as f64),
            )
        };

        let wing = if dt.has_wing() {
            Some(wing_aero(p, m, rho, mass))
        } else {
            None
        };

        // --- Rotors from the drawn geometry
        let eta_el = fp(p.eta_mot, 0.85).min(1.0) * fp(p.eta_esc, 0.95).min(1.0);
        let mut rotors = Vec::new();
        let lift_specs: Vec<_> = spec
            .rotors
            .iter()
            .filter(|r| r.axis.z.abs() > 0.5)
            .collect();
        let fwd_specs: Vec<_> = spec
            .rotors
            .iter()
            .filter(|r| r.axis.z.abs() <= 0.5)
            .collect();
        if dt.is_multirotor() {
            let d = fp(p.mr_d_prop, 0.127);
            let t_max = fp(p.mr_t_max_motor, mass * G * 2.0 / 4.0);
            let n_rps = rps_for(t_max, rho, d);
            // Hover props: geometric pitch ~0.6 D, zero thrust slightly beyond pitch speed.
            let v_zero = (1.25 * n_rps * 0.6 * d).max(8.0);
            let tau = (0.03 + 0.2 * d).clamp(0.04, 0.2);
            let centre = lift_specs
                .iter()
                .fold(geom3d::Vec3::ZERO, |a, r| a + r.center)
                * (1.0 / lift_specs.len().max(1) as f32);
            let mut order: Vec<usize> = (0..lift_specs.len()).collect();
            let az = |i: usize| {
                let c = lift_specs[i].center - centre;
                (c.y as f64).atan2(c.x as f64)
            };
            // Group coaxial pairs together: sort by azimuth of top rotors only.
            order.sort_by(|&a, &b| az(a).total_cmp(&az(b)));
            let tops: Vec<usize> = order
                .iter()
                .copied()
                .filter(|&i| lift_specs[i].axis.z > 0.0)
                .collect();
            for (rank, &i) in tops.iter().enumerate() {
                let r = lift_specs[i];
                let spin = if rank % 2 == 0 { 1.0 } else { -1.0 };
                rotors.push(Rotor {
                    pos: to_body(r.center),
                    axis: V3::new(0.0, 0.0, -1.0),
                    t_max,
                    diameter: d,
                    disk_area: std::f64::consts::PI * 0.25 * d * d,
                    spin,
                    k_q: 0.06 * d,
                    v_zero,
                    tau,
                    lift: true,
                    v_tip: std::f64::consts::PI * n_rps * d,
                });
                // Coaxial partner below (same xy), opposite spin.
                if let Some(bottom) = lift_specs.iter().find(|q| {
                    q.axis.z < 0.0
                        && (q.center.x - r.center.x).abs() < 1e-3
                        && (q.center.y - r.center.y).abs() < 1e-3
                }) {
                    rotors.push(Rotor {
                        pos: to_body(bottom.center),
                        spin: -spin,
                        ..rotors.last().cloned().expect("just pushed")
                    });
                }
            }
            if rotors.is_empty() {
                // Degenerate input: four rotors on a small square so the sim still runs.
                for (k, (x, y)) in [(0.1, 0.1), (-0.1, 0.1), (-0.1, -0.1), (0.1, -0.1)]
                    .into_iter()
                    .enumerate()
                {
                    rotors.push(Rotor {
                        pos: V3::new(x, y, -0.02),
                        axis: V3::new(0.0, 0.0, -1.0),
                        t_max,
                        diameter: d,
                        disk_area: std::f64::consts::PI * 0.25 * d * d,
                        spin: if k % 2 == 0 { 1.0 } else { -1.0 },
                        k_q: 0.06 * d,
                        v_zero,
                        tau,
                        lift: true,
                        v_tip: std::f64::consts::PI * n_rps * d,
                    });
                }
            }
        }
        if dt.has_wing() {
            let d = fp(p.d_prop, 0.25);
            let n = fwd_specs.len().max(1);
            let t_total = fp(m.prop.t_static, 0.5 * mass * G);
            let t_max = t_total / n as f64;
            let n_rps = rps_for(t_max, rho, d);
            let pitch = fp(p.prop_pitch, 0.6 * d);
            let v_zero = (1.25 * n_rps * pitch).max(1.6 * fp(p.v_cr, 15.0));
            let tau = (0.03 + 0.2 * d).clamp(0.04, 0.2);
            let positions: Vec<V3> = if fwd_specs.is_empty() {
                vec![V3::ZERO]
            } else {
                fwd_specs.iter().map(|r| to_body(r.center)).collect()
            };
            for pos in positions {
                rotors.push(Rotor {
                    // Thrust line assumed through the CG height (no pitching moment from thrust).
                    pos: V3::new(pos.x, pos.y, 0.0),
                    axis: V3::new(1.0, 0.0, 0.0),
                    t_max,
                    diameter: d,
                    disk_area: std::f64::consts::PI * 0.25 * d * d,
                    spin: 1.0,
                    k_q: 0.06 * d,
                    v_zero,
                    tau,
                    lift: false,
                    v_tip: std::f64::consts::PI * n_rps * d,
                });
            }
        }

        // --- Ground contacts from the drawn geometry
        let mut contacts = Vec::new();
        for w in &spec.wheels {
            let pt = to_body(w.center - geom3d::Vec3::Z * w.radius);
            contacts.push(Contact {
                pos: pt,
                kind: ContactKind::Wheel { steer: pt.x > 0.0 },
            });
        }
        if contacts.is_empty() {
            let gear: Vec<geom3d::Vec3> = spec
                .rods
                .iter()
                .filter(|(k, _)| *k == PartKind::Gear)
                .flat_map(|(_, r)| [r.a, r.b])
                .collect();
            if let Some(zmin) = gear.iter().map(|v| v.z).reduce(f32::min) {
                for v in gear.iter().filter(|v| v.z <= zmin + 0.004) {
                    contacts.push(Contact {
                        pos: to_body(*v),
                        kind: ContactKind::Skid,
                    });
                }
            }
        }
        // Structure points: wing tips, nose, tail, belly, top, rotor hubs.
        for (kind, s) in &spec.surfaces {
            if *kind != PartKind::Wing {
                continue;
            }
            let semi = s.semi_span;
            let dih = s.dihedral_deg.to_radians();
            let x_tip = s.root_le.x - semi * s.sweep_le_deg.to_radians().tan() - 0.5 * s.c_tip;
            for side in [1.0_f32, -1.0] {
                let tip = geom3d::Vec3::new(
                    x_tip,
                    s.root_le.y + side * semi * dih.cos(),
                    s.root_le.z + semi * dih.sin(),
                );
                contacts.push(Contact {
                    pos: to_body(tip),
                    kind: ContactKind::Structure,
                });
            }
            if spec.fuselage.is_none() {
                // Flying wing: belly skids under the root chord.
                for f in [0.15_f32, 0.8] {
                    let pt = geom3d::Vec3::new(s.root_le.x - f * s.c_root, 0.0, -0.05 * s.c_root);
                    contacts.push(Contact {
                        pos: to_body(pt),
                        kind: ContactKind::Skid,
                    });
                }
            }
        }
        if let Some(f) = &spec.fuselage {
            let r = 0.5 * f.diameter;
            for pt in [
                geom3d::Vec3::new(f.x_nose, 0.0, f.z_axis),
                geom3d::Vec3::new(f.x_nose - f.length, 0.0, f.z_axis),
                geom3d::Vec3::new(cg.x, 0.0, f.z_axis - r),
                geom3d::Vec3::new(cg.x, 0.0, f.z_axis + r),
            ] {
                contacts.push(Contact {
                    pos: to_body(pt),
                    kind: ContactKind::Structure,
                });
            }
        } else {
            contacts.push(Contact {
                pos: V3::new(0.0, 0.0, -0.06),
                kind: ContactKind::Structure,
            });
        }
        for r in &rotors {
            contacts.push(Contact {
                pos: r.pos,
                kind: ContactKind::Structure,
            });
        }
        if !contacts
            .iter()
            .any(|c| !matches!(c.kind, ContactKind::Structure))
        {
            // No gear at all: a belly point that behaves like a skid.
            contacts.push(Contact {
                pos: V3::new(0.0, 0.0, 0.05),
                kind: ContactKind::Skid,
            });
        }

        // --- Payload position (for a CG shift when it is dropped)
        let payload_pos = if dt == DroneType::Multirotor {
            p.mr_components
                .iter()
                .min_by(|a, b| {
                    (a.mass - p.m_payload)
                        .abs()
                        .total_cmp(&(b.mass - p.m_payload).abs())
                })
                .map(|c| {
                    V3::new(
                        finite_or(c.x - m.mr.cg_x, 0.0),
                        finite_or(c.y - m.mr.cg_y, 0.0),
                        0.0,
                    )
                })
                .unwrap_or(V3::ZERO)
        } else {
            p.components
                .iter()
                .find(|c| c.link == Link::Payload)
                .map(|c| {
                    V3::new(
                        finite_or(m.stab.cg_nose.x - c.x, 0.0),
                        finite_or(c.y - m.stab.cg_nose.y, 0.0),
                        finite_or(-(c.z - m.stab.cg_nose.z), 0.0),
                    )
                })
                .unwrap_or(V3::ZERO)
        };

        let f_plate = fp(p.mr_flat_plate_area, 0.01);
        let drag_area = if dt == DroneType::Multirotor {
            V3::new(f_plate, f_plate, 2.5 * f_plate)
        } else {
            V3::ZERO
        };
        let (v_stall, v_cruise) = if dt == DroneType::Multirotor {
            (0.0, fp(p.mr_v_cruise, 10.0))
        } else {
            (fp(m.aero.v_stall, 10.0), fp(p.v_cr, 15.0))
        };

        VehicleParams {
            drone_type: dt,
            mass,
            inertia: inertia::estimate(p, m),
            wing,
            rotors,
            contacts,
            battery: Battery {
                v_nom: fp(m.weight.pack_voltage, 14.8),
                energy_wh: fp(m.weight.pack_wh, 50.0),
                dod: fp(p.dod, 0.8).min(1.0),
                p_hotel: finite_or(p.p_hotel, 5.0).max(0.0),
            },
            eta_el,
            eta_prop: fp(p.eta_p_cr, 0.7).min(0.95),
            fm: fp(p.fm, 0.65).clamp(0.2, 0.95),
            drag_area,
            payload_mass: finite_or(p.m_payload, 0.0).clamp(0.0, 0.8 * mass),
            payload_pos,
            field_elevation,
            v_stall,
            v_cruise,
            scene_cg: [cg.x, cg.y, cg.z],
            crash_sink: if dt == DroneType::Multirotor {
                2.5
            } else {
                3.0
            },
        }
    }

    pub fn lift_rotor_count(&self) -> usize {
        self.rotors.iter().filter(|r| r.lift).count()
    }

    pub fn has_forward_motor(&self) -> bool {
        self.rotors.iter().any(|r| !r.lift)
    }
}

fn wing_aero(p: &Project, m: &Model, rho: f64, mass: f64) -> WingAero {
    let s = fp(p.s, 0.3);
    let b = fp(p.b, 1.5);
    let mac = fp(m.wing.mac, s / b);
    let ar = fp(m.wing.ar, b * b / s);
    let cla_w = fp(m.aero.cla_w, 4.5);
    let cl_max = fp(m.aero.cl_max, 1.1);
    let cd0 = fp(m.aero.cd0, 0.03);
    let k = fp(m.aero.k, 1.0 / (std::f64::consts::PI * 0.8 * ar));
    let taper = fp(p.taper, 0.6).min(1.0);
    let flying = p.drone_type == DroneType::FlyingWing || p.tail_kind == TailKind::FlyingWing;
    let vtail = p.tail_kind == TailKind::VTail && !flying;
    let mixing = if flying {
        Mixing::Elevon
    } else if vtail {
        Mixing::VTail
    } else {
        Mixing::Conventional
    };

    // Tail contributions (volumes from the geometry calculation).
    let eta_h = fp(p.eta_h, 0.9).min(1.0);
    let cla_h = fp(p.cla_h, 3.5);
    let cla_v = if vtail { cla_h } else { 0.75 * cla_h };
    let (v_h, v_v) = if flying {
        (0.0, 0.0)
    } else {
        (
            finite_or(m.tail.v_h, 0.0).max(0.0),
            finite_or(m.tail.v_v, 0.0).max(0.0),
        )
    };
    let (s_h, s_v) = if flying {
        (0.0, 0.0)
    } else {
        (
            finite_or(m.tail.s_h_eff, 0.0).max(0.0),
            finite_or(m.tail.s_v_eff, 0.0).max(0.0),
        )
    };
    let l_h = fp(p.l_h, 3.0 * mac);
    let l_v = fp(p.l_v, l_h);
    let deps = finite_or(m.stab.np.deps, 0.4).clamp(0.0, 0.9);
    let cla_tail = cla_h * eta_h * (s_h / s) * (1.0 - deps);
    let cla = cla_w + cla_tail;

    let sm = finite_or(m.stab.sm, 0.1).clamp(-0.2, 0.6);
    let cma = -cla * sm;
    let sweep = finite_or(p.sweep_le_deg, 0.0).to_radians().clamp(-1.0, 1.0);

    let (cmq, clq) = if flying {
        (
            (-cla * 0.25 * (1.0 + ar * sweep.tan().abs() / 2.0)).clamp(-3.0, -0.3),
            0.0,
        )
    } else {
        (
            -2.0 * eta_h * v_h * cla_h * (l_h / mac) - 0.1,
            2.0 * eta_h * v_h * cla_h,
        )
    };

    // Elevator / elevon pitch effectiveness.
    let (cm_de, cl_de) = if flying {
        // Moment arm from CG to the elevon centroid (60 % semi span, 87.5 % local chord).
        let c_r = fp(m.wing.c_r, mac);
        let c_t = fp(m.wing.c_t, mac * taper);
        let eta = 0.6;
        let c_loc = c_r + (c_t - c_r) * eta;
        let x_elev = eta * 0.5 * b * sweep.tan() + 0.875 * c_loc;
        let x_cg = finite_or(m.wing.x_le_mac, 0.0) + finite_or(m.stab.x_cg_rel, 0.25 * mac);
        let arm = (x_elev - x_cg).max(0.1 * mac);
        let area_frac = 0.75;
        (
            cla_w * TAU_ELEVON * area_frac * arm / mac,
            cla_w * TAU_ELEVON * area_frac,
        )
    } else {
        (
            eta_h * v_h * cla_h * TAU_ELEVATOR,
            eta_h * (s_h / s) * cla_h * TAU_ELEVATOR,
        )
    };

    // Ailerons (strip theory, rectangular approximation).
    let (eta1, eta2, tau_a) = if flying {
        (0.15, 0.95, TAU_ELEVON)
    } else {
        (0.55, 0.95, TAU_AILERON)
    };
    let clda = cla_w * tau_a * (eta2 * eta2 - eta1 * eta1) / 4.0;
    let clp = -(cla_w / 12.0) * (1.0 + 3.0 * taper) / (1.0 + taper);

    // Directional / lateral static derivatives.
    let fus_vol = 0.6 * std::f64::consts::FRAC_PI_4 * fp(p.d_fuse, 0.1).powi(2) * fp(p.l_fuse, 0.8);
    let cnb_fus = if flying {
        0.0
    } else {
        -1.3 * fus_vol / (s * b)
    };
    let cnb_base = if flying { 0.012 } else { 0.0 };
    let cnb = (eta_h * v_v * cla_v + cnb_fus + cnb_base).max(-0.02);
    let cnr = -2.0 * eta_h * v_v * cla_v * (l_v / b) - 0.015 - if flying { 0.01 } else { 0.0 };
    let cyb = -0.25 - eta_h * (s_v / s) * cla_v;
    let w_n = mass * G;
    let cl_cr = w_n / (0.5 * rho * fp(p.v_cr, 15.0).powi(2) * s);
    let z_v = if vtail {
        0.5 * fp(p.s_vtail, 0.08).sqrt() * fp(p.gamma_v_deg, 35.0).to_radians().sin()
    } else {
        0.4 * (2.0 * s_v).sqrt()
    };
    let clb = -0.06
        - 0.5 * cl_cr * sweep.tan().max(0.0) / (1.0 + ar / 4.0)
        - eta_h * (s_v / s) * cla_v * (z_v / b);

    let (cn_dr, cy_dr) = if flying {
        (0.0, 0.0)
    } else {
        (
            eta_h * v_v * cla_v * TAU_RUDDER,
            eta_h * (s_v / s) * cla_v * TAU_RUDDER,
        )
    };
    let cl_dr = -cy_dr * (z_v / b);

    // Trim: design cruise CL at the design speed, cruise body AoA 0.5 deg.
    let mut w = WingAero {
        s,
        b,
        mac,
        ar,
        cla,
        cl_max,
        cl_att_scale: 1.0,
        alpha_stall: 1.25 * cl_max / cla,
        cd0,
        k,
        i_w: 0.0,
        alpha_w_trim: 0.0,
        sm,
        cma,
        cmq,
        clq,
        cm_de,
        cl_de,
        clda,
        clp,
        clb,
        cyb,
        cnb,
        cnr,
        cn_dr,
        cy_dr,
        cl_dr,
        mixing,
    };
    w.cl_att_scale = 1.0 / super::aero::peak_cl_ratio(&w);
    let alpha_w_trim = super::aero::alpha_for_cl(&w, cl_cr.min(0.9 * cl_max));
    w.alpha_w_trim = alpha_w_trim;
    w.i_w = alpha_w_trim - 0.5_f64.to_radians();
    w
}
