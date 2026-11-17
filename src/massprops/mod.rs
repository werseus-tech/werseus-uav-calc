// SPDX-License-Identifier: AGPL-3.0-or-later
//! Mass properties estimate: total mass, centre of gravity and the inertia tensor about the CG,
//! plus the CG offset from the thrust centre.
//!
//! Masses and positions are **not** recomputed here. They come from the existing mass model:
//! fixed-wing and VTOL use the component table already evaluated into [`crate::calc::Model`]
//! (`stab.comps`, positions from the burn datum: x aft from the nose, y right, z up), the
//! multirotor uses the same list the existing CG calculation uses (user components + frame +
//! motors at [`crate::calc::multirotor::MultirotorOut::motor_coords`]; x forward from the frame
//! centre, y right, z up). This module only adds a simplified solid shape per component so each
//! item has an own inertia tensor, then sums them with the parallel-axis theorem
//! (`I = sum_i [I_i + m_i (|d_i|^2 E - d_i d_i^T)]`, `d_i` measured from the total CG).
//!
//! Shape model (see [`shapes`] for the closed forms):
//! - wing: uniform trapezoidal lamina (strips), area centroid placed at the declared position
//! - fuselage + tail: solid cylinder (ellipsoid for a flying-wing pod) plus thin rectangular
//!   plates for the horizontal, vertical or V tail, mass shared by wetted area, the whole group
//!   moved rigidly so its CG sits at the declared position
//! - battery: solid box (assumed pack density and 3:2:1 edge ratio)
//! - motors / payload / avionics / manual rows: point masses
//! - multirotor frame: thin disc (centre plate) plus one thin rod per arm, motors as points
//!
//! The values are estimates from simplified shapes, not measured data.
use crate::calc::Model;
use crate::calc::geometry::TailKind;
use crate::calc::multirotor::CG_OFFSET_MAX_FRAC;
use crate::project::{DroneType, Link, Project};
use shapes::{Body, M3, V3};

pub mod shapes;

/// Assumed average density of a battery pack [kg/m^3] (Li-ion / LiPo pack incl. wrap).
pub const PACK_DENSITY: f64 = 2400.0;

/// Axis convention of the reported numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AxisFrame {
    /// x aft from the nose datum, y right, z up (fixed wing, flying wing, VTOL).
    NoseAft,
    /// x forward from the frame centre, y right, z up (multirotor).
    CentreForward,
}

/// What the CG offset is measured against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OffsetKind {
    /// Offset from the motor centroid in the rotor plane, relative to the arm length.
    MotorCentroid,
    /// Offset from the thrust line (y, z), relative to the MAC; CG position as % MAC.
    ThrustAxis,
}

/// CG relative to the centre of thrust.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThrustOffset {
    pub kind: OffsetKind,
    /// Centre of thrust: motor centroid (multirotor) or mass-weighted propulsion position.
    pub centre_m: V3,
    /// Offset vector of the CG from the thrust centre in the plane normal to the thrust
    /// ((x, y) for the multirotor, (y, z) for fixed wing) [m].
    pub offset_vec_m: [f64; 2],
    pub offset_m: f64,
    /// Reference length: arm length (multirotor) or MAC (fixed wing) [m].
    pub reference_len_m: f64,
    /// `offset_m / reference_len_m` in percent.
    pub offset_pct: f64,
    /// Multirotor limit in the same unit (percent of arm length); `None` for fixed wing.
    pub limit_pct: Option<f64>,
    /// CG position relative to the MAC leading edge, percent of MAC (fixed wing only).
    pub cg_pct_mac: Option<f64>,
    /// Static margin of the existing stability model (fixed wing only).
    pub static_margin: Option<f64>,
    /// Multirotor: offset above the existing limit. Fixed wing: static margin outside its band.
    pub exceeds: bool,
}

/// One component with its own inertia about its own centre of mass.
#[derive(Clone, Debug, PartialEq)]
pub struct MassItem {
    /// i18n key of the item class (always set).
    pub name_key: &'static str,
    /// User-given row name; empty for items synthesised by this module.
    pub name: String,
    /// i18n key describing the shape used for this item.
    pub shape_key: &'static str,
    pub mass_kg: f64,
    pub pos_m: V3,
    pub own_inertia: M3,
}

/// Mass properties of the whole aircraft.
#[derive(Clone, Debug)]
pub struct MassProperties {
    /// `false` when the project inputs are invalid and nothing could be computed.
    pub valid: bool,
    pub frame: AxisFrame,
    pub total_mass_kg: f64,
    pub cg_m: V3,
    /// Inertia tensor about the CG in the body axes of [`MassProperties::frame`] [kg m^2].
    pub inertia_kgm2: M3,
    pub items: Vec<MassItem>,
    /// Total of the existing mass breakdown this estimate is compared against [kg]
    /// (MTOW of the weight model, or of the multirotor model).
    pub reference_mass_kg: f64,
    pub thrust: ThrustOffset,
}

impl MassProperties {
    /// Diagonal moments (Ixx, Iyy, Izz).
    pub fn principal_diag(&self) -> [f64; 3] {
        [
            self.inertia_kgm2[0][0],
            self.inertia_kgm2[1][1],
            self.inertia_kgm2[2][2],
        ]
    }
}

fn finite_pos(x: f64) -> bool {
    x.is_finite() && x > 0.0
}

fn item(
    name_key: &'static str,
    name: &str,
    shape_key: &'static str,
    body: &Body,
    pos: V3,
) -> MassItem {
    MassItem {
        name_key,
        name: name.to_string(),
        shape_key,
        mass_kg: body.mass,
        pos_m: pos,
        own_inertia: body.inertia,
    }
}

/// Fuselage + tail group (all positions in the nose-datum frame), mass shared by wetted area.
fn fuse_tail_body(p: &Project, m: &Model, mass: f64) -> (Body, &'static str) {
    let flying = p.tail_kind == TailKind::FlyingWing;
    let l = if finite_pos(p.l_fuse) { p.l_fuse } else { 0.9 };
    let d = if finite_pos(p.d_fuse) { p.d_fuse } else { 0.1 };
    if flying {
        let pod = shapes::ellipsoid(mass, [l / 2.0, d / 2.0, d / 2.0], [l / 2.0, 0.0, 0.0]);
        return (pod, "massprops.shape.ellipsoid");
    }
    let x_ac = m.stab.datum + p.x_ac_frac * m.wing.mac;
    let c_tail = if finite_pos(p.c_tail) { p.c_tail } else { 0.0 };
    // (area, chord, span) of each tail panel and its position; chord positions use the quarter
    // chord at the tail arm measured from the wing aerodynamic centre.
    let mut panels: Vec<(f64, f64, f64, V3, V3)> = Vec::new();
    let mut add = |area: f64, chord: f64, u: V3, centre: V3| {
        if finite_pos(area) && finite_pos(chord) {
            panels.push((area, chord, area / chord, u, centre));
        }
    };
    match p.tail_kind {
        TailKind::Conventional => {
            let s_h = p.s_h;
            let ch = if c_tail > 0.0 {
                c_tail
            } else {
                (s_h / 4.0).sqrt()
            };
            add(
                s_h,
                ch,
                [0.0, 1.0, 0.0],
                [x_ac + p.l_h + 0.25 * ch, 0.0, 0.0],
            );
            let s_v = p.s_v;
            let cv = if c_tail > 0.0 {
                c_tail
            } else {
                (s_v / 1.5).sqrt()
            };
            let h_v = if finite_pos(cv) { s_v / cv } else { 0.0 };
            add(
                s_v,
                cv,
                [0.0, 0.0, 1.0],
                [x_ac + p.l_v + 0.25 * cv, 0.0, h_v / 2.0],
            );
        }
        TailKind::VTail => {
            let area = p.s_vtail / 2.0;
            let ch = if c_tail > 0.0 {
                c_tail
            } else {
                (area / 2.0).sqrt()
            };
            let g = p.gamma_v_deg.to_radians();
            let w = if finite_pos(ch) { area / ch } else { 0.0 };
            for side in [-1.0, 1.0] {
                add(
                    area,
                    ch,
                    [0.0, side * g.cos(), g.sin()],
                    [
                        x_ac + p.l_h + 0.25 * ch,
                        side * 0.5 * w * g.cos(),
                        0.5 * w * g.sin(),
                    ],
                );
            }
        }
        TailKind::FlyingWing => {}
    }
    // Mass shares by wetted area (two sides of every panel).
    let w_fuse = if finite_pos(p.s_wet_fuse) {
        p.s_wet_fuse
    } else {
        std::f64::consts::PI * d * l
    };
    let w_total = w_fuse + panels.iter().map(|q| 2.0 * q.0).sum::<f64>();
    let mut parts = vec![shapes::cylinder_x(
        mass * w_fuse / w_total,
        l,
        d,
        [l / 2.0, 0.0, 0.0],
    )];
    for (area, chord, span, u, centre) in &panels {
        parts.push(shapes::plate_rect(
            mass * 2.0 * area / w_total,
            *chord,
            *span,
            *u,
            *centre,
        ));
    }
    (shapes::combine(&parts), "massprops.shape.fuse_tail")
}

fn fixed_wing_items(p: &Project, m: &Model) -> Vec<MassItem> {
    let mut items = Vec::new();
    for (row, c) in p.components.iter().zip(&m.stab.comps) {
        if !finite_pos(c.mass) || ![c.x, c.y, c.z].iter().all(|v| v.is_finite()) {
            continue;
        }
        let pos = [c.x, c.y, c.z];
        let (key, shape_key, body) = match row.link {
            Link::Wing if finite_pos(p.b) => (
                "massprops.item.wing",
                "massprops.shape.lamina",
                shapes::wing_lamina(c.mass, &m.wing, p.b, p.sweep_le_deg),
            ),
            Link::FuseTail => {
                let (b, sk) = fuse_tail_body(p, m, c.mass);
                ("massprops.item.fuse_tail", sk, b)
            }
            Link::Battery => {
                // Edge ratio 3:2:1 (x:y:z), volume from the assumed pack density.
                let s = (c.mass / PACK_DENSITY / 6.0).cbrt();
                (
                    "massprops.item.battery",
                    "massprops.shape.box",
                    shapes::solid_box(c.mass, [3.0 * s, 2.0 * s, s], [0.0; 3]),
                )
            }
            Link::Propulsion => (
                "massprops.item.propulsion",
                "massprops.shape.point",
                shapes::point(c.mass, [0.0; 3]),
            ),
            Link::Payload => (
                "massprops.item.payload",
                "massprops.shape.point",
                shapes::point(c.mass, [0.0; 3]),
            ),
            Link::Avionics => (
                "massprops.item.avionics",
                "massprops.shape.point",
                shapes::point(c.mass, [0.0; 3]),
            ),
            Link::Wing | Link::None => (
                "massprops.item.other",
                "massprops.shape.point",
                shapes::point(c.mass, [0.0; 3]),
            ),
        };
        // The declared row position is authoritative: shapes only supply the inertia.
        let placed = body.placed_at(pos);
        items.push(item(key, &c.name, shape_key, &placed, pos));
    }
    items
}

fn multirotor_items(p: &Project, m: &Model) -> Vec<MassItem> {
    let mut items = Vec::new();
    for c in &p.mr_components {
        if finite_pos(c.mass) && c.x.is_finite() && c.y.is_finite() {
            let pos = [c.x, c.y, 0.0];
            let b = shapes::point(c.mass, pos);
            items.push(item(
                "massprops.item.other",
                &c.name,
                "massprops.shape.point",
                &b,
                pos,
            ));
        }
    }
    let coords = &m.mr.motor_coords;
    // Frame: central plate (disc) plus one thin rod per arm, from the plate edge to the motor.
    if finite_pos(p.mr_m_frame) {
        let r_plate = if p.mr_r_plate.is_finite() {
            p.mr_r_plate.max(0.0)
        } else {
            0.0
        };
        let arm_len = |x: f64, y: f64| (x.hypot(y) - r_plate).max(0.0);
        let rods: Vec<(f64, f64, f64)> = coords
            .iter()
            .filter(|&&(x, y)| arm_len(x, y) > 1e-6)
            .map(|&(x, y)| (x, y, arm_len(x, y)))
            .collect();
        let m_arm_each = if m.mr_st.m_arm_single.is_finite() {
            m.mr_st.m_arm_single.max(0.0)
        } else {
            0.0
        };
        let m_arms = (m_arm_each * rods.len() as f64).min(0.9 * p.mr_m_frame);
        let m_plate = p.mr_m_frame - m_arms;
        if m_plate > 0.0 {
            let r = if r_plate > 0.0 {
                r_plate
            } else {
                0.5 * p.mr_frame_size.max(0.0)
            };
            let b = shapes::disc_z(m_plate, r, [0.0; 3]);
            items.push(item(
                "massprops.item.frame_plate",
                "",
                "massprops.shape.disc",
                &b,
                [0.0; 3],
            ));
        }
        if m_arms > 0.0 && !rods.is_empty() {
            let each = m_arms / rods.len() as f64;
            let parts: Vec<Body> = rods
                .iter()
                .map(|&(x, y, len)| {
                    let d = x.hypot(y);
                    let u = [x / d, y / d, 0.0];
                    let mid = (r_plate + len / 2.0) / d;
                    shapes::rod(each, len, u, [x * mid, y * mid, 0.0])
                })
                .collect();
            let g = shapes::combine(&parts);
            items.push(item(
                "massprops.item.arms",
                "",
                "massprops.shape.rod",
                &g,
                g.cg,
            ));
        }
    }
    if finite_pos(p.mr_m_motor) && !coords.is_empty() {
        let per = if p.mr_coaxial {
            2.0 * p.mr_m_motor
        } else {
            p.mr_m_motor
        };
        let parts: Vec<Body> = coords
            .iter()
            .map(|&(x, y)| shapes::point(per, [x, y, 0.0]))
            .collect();
        let g = shapes::combine(&parts);
        items.push(item(
            "massprops.item.motors",
            "",
            "massprops.shape.point",
            &g,
            g.cg,
        ));
    }
    items
}

/// Mass properties of the project. `m` is the model already evaluated from `p`
/// ([`crate::calc::evaluate`]); it supplies the component masses, positions and the existing
/// CG / stability results that are reused rather than recomputed.
pub fn compute(p: &Project, m: &Model) -> MassProperties {
    let multirotor = p.drone_type == DroneType::Multirotor;
    let (frame, items, reference_mass) = if multirotor {
        (
            AxisFrame::CentreForward,
            multirotor_items(p, m),
            m.mr.m_mtow,
        )
    } else {
        (AxisFrame::NoseAft, fixed_wing_items(p, m), m.weight.m_mtow)
    };
    let bodies: Vec<Body> = items
        .iter()
        .map(|i| Body {
            mass: i.mass_kg,
            cg: i.pos_m,
            inertia: i.own_inertia,
        })
        .collect();
    let total = shapes::combine(&bodies);
    let valid = m.inputs_ok && total.mass > 0.0;
    let thrust = thrust_offset(p, m, &items, total.cg, multirotor);
    MassProperties {
        valid,
        frame,
        total_mass_kg: total.mass,
        cg_m: total.cg,
        inertia_kgm2: total.inertia,
        items,
        reference_mass_kg: reference_mass,
        thrust,
    }
}

fn thrust_offset(
    p: &Project,
    m: &Model,
    items: &[MassItem],
    cg: V3,
    multirotor: bool,
) -> ThrustOffset {
    if multirotor {
        let centre = [m.mr.geom_center_x, m.mr.geom_center_y, 0.0];
        let v = [cg[0] - centre[0], cg[1] - centre[1]];
        let off = v[0].hypot(v[1]);
        let l_arm = if finite_pos(m.mr.l_arm) {
            m.mr.l_arm
        } else {
            0.5 * p.mr_frame_size
        };
        let pct = if finite_pos(l_arm) {
            100.0 * off / l_arm
        } else {
            f64::NAN
        };
        let limit = if finite_pos(l_arm) {
            Some(100.0 * CG_OFFSET_MAX_FRAC * p.mr_frame_size / l_arm)
        } else {
            None
        };
        ThrustOffset {
            kind: OffsetKind::MotorCentroid,
            centre_m: centre,
            offset_vec_m: v,
            offset_m: off,
            reference_len_m: l_arm,
            offset_pct: pct,
            limit_pct: limit,
            cg_pct_mac: None,
            static_margin: None,
            exceeds: off > CG_OFFSET_MAX_FRAC * p.mr_frame_size,
        }
    } else {
        let prop: Vec<&MassItem> = items
            .iter()
            .filter(|i| i.name_key == "massprops.item.propulsion")
            .collect();
        let pm: f64 = prop.iter().map(|i| i.mass_kg).sum();
        let centre = if pm > 0.0 {
            let mut c = [0.0; 3];
            for i in &prop {
                for (k, v) in c.iter_mut().enumerate() {
                    *v += i.mass_kg * i.pos_m[k] / pm;
                }
            }
            c
        } else {
            [cg[0], 0.0, 0.0]
        };
        let v = [cg[1] - centre[1], cg[2] - centre[2]];
        let off = v[0].hypot(v[1]);
        let mac = m.wing.mac;
        ThrustOffset {
            kind: OffsetKind::ThrustAxis,
            centre_m: centre,
            offset_vec_m: v,
            offset_m: off,
            reference_len_m: mac,
            offset_pct: if finite_pos(mac) {
                100.0 * off / mac
            } else {
                f64::NAN
            },
            limit_pct: None,
            cg_pct_mac: Some(100.0 * m.stab.x_cg_rel / mac),
            static_margin: Some(m.stab.sm),
            exceeds: !m.stab.in_band,
        }
    }
}

#[cfg(test)]
mod tests;
