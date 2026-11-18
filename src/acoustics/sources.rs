// SPDX-License-Identifier: AGPL-3.0-or-later
//! Builds the noise sources (rotor groups at operating points) from the project model.
//!
//! * Multirotor: lift rotors at hover and at full throttle.
//! * VTOL (quadplane): lift rotors at hover and full throttle, plus the pusher in cruise and
//!   at full throttle (static).
//! * Fixed wing, V-tail, flying wing: the single propeller in cruise and at full throttle
//!   (static).
//!
//! Hover and full-throttle RPM follow the thrust law `T = Ct rho n^2 D^4` with the same
//! `Ct = 0.10` the project uses for its hover RPM estimate (the project's hover RPM override is
//! honoured). Shaft power at full throttle scales with `T^1.5` (momentum theory at constant
//! figure of merit and diameter), i.e. with `(T_max / T_hover)^1.5`.

use super::method::{Air, RotorSet};
use crate::calc::Model;
use crate::project::{DroneType, Project};

/// Thrust coefficient used by the project for the hover RPM estimate (`T = Ct rho n^2 D^4`).
pub const CT_HOVER: f64 = 0.10;
/// Default blade count of the pusher / single propeller (the project has no input for it).
pub const DEFAULT_PUSHER_BLADES: u32 = 2;

/// Operating point of a source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpPoint {
    /// Lift rotors hovering.
    LiftHover,
    /// Lift rotors at full throttle.
    LiftMax,
    /// Forward propeller in cruise.
    Cruise,
    /// Forward propeller at full throttle (static).
    PusherMax,
}

impl OpPoint {
    /// Translation key of the operating-point label.
    pub fn label_key(self) -> &'static str {
        match self {
            OpPoint::LiftHover => "acoustics.op.lift_hover",
            OpPoint::LiftMax => "acoustics.op.lift_max",
            OpPoint::Cruise => "acoustics.op.cruise",
            OpPoint::PusherMax => "acoustics.op.pusher_max",
        }
    }
}

/// One rotor group at one operating point, plus what the alternatives table needs.
#[derive(Clone, Copy, Debug)]
pub struct Source {
    /// Operating point.
    pub point: OpPoint,
    /// Rotor group.
    pub rotors: RotorSet,
    /// Thrust per rotor (N).
    pub thrust_n: f64,
    /// Largest diameter that fits the airframe, when the project knows one (m).
    pub max_diameter_m: Option<f64>,
}

/// Air properties at the project altitude.
pub fn air_of(m: &Model) -> Air {
    let rho = m.atmo.rho;
    let a = m.atmo.a;
    let d = Air::default();
    Air {
        rho: if rho.is_finite() && rho > 0.0 {
            rho
        } else {
            d.rho
        },
        a_sound: if a.is_finite() && a > 0.0 {
            a
        } else {
            d.a_sound
        },
    }
}

/// Operating points offered for a vehicle type, in display order.
pub fn points_for(dt: DroneType) -> &'static [OpPoint] {
    match dt {
        DroneType::Multirotor => &[OpPoint::LiftHover, OpPoint::LiftMax],
        DroneType::VtolQuadplane => &[
            OpPoint::LiftHover,
            OpPoint::LiftMax,
            OpPoint::Cruise,
            OpPoint::PusherMax,
        ],
        _ => &[OpPoint::Cruise, OpPoint::PusherMax],
    }
}

/// Hover RPM per lift rotor: the project override, else `sqrt(T / (Ct rho D^4))`, or `0.0`
/// if inputs cannot yield a positive finite RPM.
pub fn hover_rpm(p: &Project, m: &Model) -> f64 {
    let ovr = p.mr_hover_rpm_override;
    if ovr.is_finite() && ovr > 0.0 {
        return ovr;
    }
    let d4 = p.mr_d_prop.powi(4);
    let t = m.mr.t_hover_motor;
    let rho = m.atmo.rho;
    if p.mr_d_prop.is_finite()
        && p.mr_d_prop > 0.0
        && t.is_finite()
        && t > 0.0
        && rho.is_finite()
        && rho > 0.0
    {
        let n_sq = t / (CT_HOVER * rho * d4);
        if n_sq.is_finite() && n_sq > 0.0 {
            let rpm = n_sq.sqrt() * 60.0;
            if rpm.is_finite() && rpm > 0.0 {
                return rpm;
            }
        }
    }
    0.0
}

fn lift_source(p: &Project, m: &Model, point: OpPoint) -> Source {
    let t_hover = m.mr.t_hover_motor;
    let t_max = p.mr_t_max_motor;
    let (ratio, thrust) = if point == OpPoint::LiftMax {
        let r = if t_hover > 0.0 && t_max.is_finite() && t_max > 0.0 {
            t_max / t_hover
        } else {
            0.0
        };
        (r, t_max)
    } else {
        (1.0, t_hover)
    };
    let rpm = hover_rpm(p, m) * ratio.sqrt();
    let power = m.mr.p_shaft_motor * ratio.powf(1.5);
    Source {
        point,
        rotors: RotorSet {
            diameter_m: p.mr_d_prop,
            blades: p.mr_prop_blades.max(1),
            n_rotors: m.mr.n_motors.max(1),
            rpm,
            shaft_power_w: power,
            forward_speed_ms: 0.0,
        },
        thrust_n: thrust,
        max_diameter_m: Some(m.mr.d_prop_max).filter(|d| d.is_finite() && *d > 0.0),
    }
}

fn forward_source(p: &Project, m: &Model, point: OpPoint, blades: u32) -> Source {
    let cruise = point == OpPoint::Cruise;
    Source {
        point,
        rotors: RotorSet {
            diameter_m: m.prop_in.d_prop,
            blades: blades.max(1),
            n_rotors: 1,
            rpm: if cruise {
                m.prop.rpm_cr
            } else {
                m.prop.rpm_max
            },
            shaft_power_w: if cruise {
                m.prop.p_shaft_cr
            } else {
                m.prop.p_shaft_static
            },
            forward_speed_ms: if cruise { p.v_cr } else { 0.0 },
        },
        thrust_n: if cruise { m.prop.t_cr } else { m.prop.t_static },
        max_diameter_m: None,
    }
}

/// The source for one operating point (None if the vehicle type does not have it).
pub fn source_for(p: &Project, m: &Model, point: OpPoint, forward_blades: u32) -> Option<Source> {
    if !points_for(p.drone_type).contains(&point) {
        return None;
    }
    Some(match point {
        OpPoint::LiftHover | OpPoint::LiftMax => lift_source(p, m, point),
        OpPoint::Cruise | OpPoint::PusherMax => forward_source(p, m, point, forward_blades),
    })
}

/// All sources of the project, in display order.
pub fn all_sources(p: &Project, m: &Model, forward_blades: u32) -> Vec<Source> {
    points_for(p.drone_type)
        .iter()
        .filter_map(|pt| source_for(p, m, *pt, forward_blades))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Library, evaluate};

    #[test]
    fn every_vehicle_type_has_sources_with_finite_numbers() {
        for dt in DroneType::ALL {
            let mut p = Project::default();
            p.set_drone_type(dt);
            let m = evaluate(&p, &Library::embedded());
            let s = all_sources(&p, &m, DEFAULT_PUSHER_BLADES);
            assert_eq!(s.len(), points_for(dt).len(), "{dt:?}");
            assert!(!s.is_empty());
            for x in &s {
                let r = &x.rotors;
                assert!(
                    r.diameter_m.is_finite() && r.diameter_m > 0.0,
                    "{dt:?} {:?}",
                    x.point
                );
                assert!(r.rpm.is_finite() && r.rpm > 0.0, "{dt:?} {:?}", x.point);
                assert!(
                    r.shaft_power_w.is_finite() && r.shaft_power_w > 0.0,
                    "{dt:?} {:?}",
                    x.point
                );
                assert!(
                    x.thrust_n.is_finite() && x.thrust_n > 0.0,
                    "{dt:?} {:?}",
                    x.point
                );
                assert!(
                    r.forward_speed_ms.is_finite() && r.forward_speed_ms >= 0.0,
                    "{dt:?} {:?}",
                    x.point
                );
                assert!(r.blades >= 1 && r.n_rotors >= 1);
            }
        }
    }

    #[test]
    fn vtol_has_lift_and_forward_groups() {
        let mut p = Project::default();
        p.set_drone_type(DroneType::VtolQuadplane);
        let m = evaluate(&p, &Library::embedded());
        let s = all_sources(&p, &m, 3);
        assert_eq!(s.len(), 4);
        assert!(s[0].rotors.n_rotors >= 2);
        assert_eq!(s[2].rotors.n_rotors, 1);
        assert_eq!(s[2].rotors.blades, 3);
        // Full throttle spins faster than hover.
        assert!(s[1].rotors.rpm >= s[0].rotors.rpm);
    }

    #[test]
    fn multirotor_has_no_forward_points() {
        let mut p = Project::default();
        p.set_drone_type(DroneType::Multirotor);
        let m = evaluate(&p, &Library::embedded());
        assert!(source_for(&p, &m, OpPoint::Cruise, 2).is_none());
        assert!(source_for(&p, &m, OpPoint::LiftHover, 2).is_some());
    }

    #[test]
    fn hover_rpm_returns_zero_when_inputs_invalid() {
        let mut p = Project::default();
        p.set_drone_type(DroneType::Multirotor);
        p.mr_hover_rpm_override = 0.0;
        let mut m = evaluate(&p, &Library::embedded());
        m.mr.t_hover_motor = 0.0;
        assert_eq!(hover_rpm(&p, &m), 0.0);
        let src = source_for(&p, &m, OpPoint::LiftHover, 2).unwrap();
        assert!(
            !crate::acoustics::method::estimate(&src.rotors, &air_of(&m), &Default::default())
                .valid
        );

        let mut m2 = evaluate(&p, &Library::embedded());
        m2.atmo.rho = 0.0;
        assert_eq!(hover_rpm(&p, &m2), 0.0);

        p.mr_d_prop = 1e80;
        assert_eq!(hover_rpm(&p, &evaluate(&p, &Library::embedded())), 0.0);
    }

    #[test]
    fn underpowered_lift_max_uses_actual_t_max() {
        let mut p = Project::default();
        p.set_drone_type(DroneType::Multirotor);
        let m = evaluate(&p, &Library::embedded());
        p.mr_t_max_motor = m.mr.t_hover_motor * 0.5;
        let hover = source_for(&p, &m, OpPoint::LiftHover, 2).unwrap();
        let max = source_for(&p, &m, OpPoint::LiftMax, 2).unwrap();
        assert!((max.thrust_n - p.mr_t_max_motor).abs() < 1e-12);
        assert!(max.rotors.rpm < hover.rotors.rpm);
        assert!(max.rotors.shaft_power_w < hover.rotors.shaft_power_w);
    }
}
