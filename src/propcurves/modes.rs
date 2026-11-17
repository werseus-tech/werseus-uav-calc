// SPDX-License-Identifier: AGPL-3.0-or-later
//! Flight time and speed estimates for hover, aggressive manoeuvring and cruise, plus the
//! maximum speed. Model estimates, not flight-test data.
//!
//! Assumptions (also shown in the UI):
//!
//! * **Hover** (multirotor, VTOL lift motors): throttle is solved so that the static thrust per
//!   motor carries the weight. A fixed wing has no hover; the row is marked not applicable.
//! * **Aggressive manoeuvring**: an average-throttle assumption. Multirotor / VTOL lift:
//!   `max(0.65, 1.25 x hover throttle)`, capped at 1, held statically (no forward-flight
//!   benefit). Fixed wing: `max(0.70, 1.15 x cruise throttle)`, capped at 1, evaluated at the
//!   highest steady level-flight speed the propeller can hold at that throttle against the
//!   drag polar.
//! * **Cruise**: fixed wing: throttle is solved so that thrust equals drag at the project's
//!   cruise speed. Multirotor: the existing forward-flight power model supplies the power and
//!   time (it is not duplicated here); the throttle and current are found by inverting the
//!   propeller model for that power. A VTOL cruises on its forward propeller (lift motors off).
//! * **Maximum speed**: fixed wing: highest speed where full-throttle thrust still equals drag.
//!   Multirotor: the speed where the tilted thrust vector balances weight and parasite drag at
//!   full throttle, limited to 90 % of the propeller's zero-thrust (pitch) speed; attitude
//!   limits and rotor inflow effects are ignored.
//! * Hotel load (avionics and payload power) is added to every mode. Energy is the usable pack
//!   energy (capacity x depth of discharge); pack sag lowers the bus voltage and is included.

use super::model::PropUnit;
use super::setup::Setup;
use crate::calc::Model;
use crate::calc::performance;
use crate::project::{DroneType, Project};

/// Lower bound of the average throttle assumed for aggressive fixed-wing flight.
pub const AGGRESSIVE_THROTTLE_FW_MIN: f64 = 0.70;
/// Multiple of the cruise throttle assumed for aggressive fixed-wing flight.
pub const AGGRESSIVE_CRUISE_FACTOR: f64 = 1.15;
/// Lower bound of the average throttle assumed for aggressive multirotor flight.
pub const AGGRESSIVE_THROTTLE_MR_MIN: f64 = 0.65;
/// Multiple of the hover throttle assumed for aggressive multirotor flight.
pub const AGGRESSIVE_HOVER_FACTOR: f64 = 1.25;
/// Fraction of the propeller pitch speed that bounds the multirotor maximum speed.
pub const PITCH_SPEED_FRACTION: f64 = 0.9;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModeKind {
    Hover,
    Aggressive,
    Cruise,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModeStatus {
    Ok,
    /// The mode does not exist for this vehicle (hover of a fixed wing).
    NotApplicable,
    /// The propulsion cannot produce the required thrust or power.
    Unreachable,
}

/// Where the flight time of a row comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeBasis {
    /// Power from the propeller / motor / battery model of this module.
    PropCurves,
    /// Power and time from the existing forward-flight power model of the multirotor code.
    ExistingModel,
}

#[derive(Clone, Copy, Debug)]
pub struct ModeResult {
    pub kind: ModeKind,
    pub status: ModeStatus,
    pub throttle: Option<f64>,
    pub airspeed_ms: Option<f64>,
    /// Battery current of all propulsion motors in this mode [A].
    pub current_a: Option<f64>,
    /// Total electrical power including the hotel load [W].
    pub power_w: Option<f64>,
    pub time_min: Option<f64>,
    /// The existing calculation's time for the same condition, for comparison.
    pub reference_time_min: Option<f64>,
    pub basis: TimeBasis,
}

impl ModeResult {
    fn blank(kind: ModeKind, status: ModeStatus) -> Self {
        Self {
            kind,
            status,
            throttle: None,
            airspeed_ms: None,
            current_a: None,
            power_w: None,
            time_min: None,
            reference_time_min: None,
            basis: TimeBasis::PropCurves,
        }
    }
}

#[derive(Clone, Debug)]
pub struct FlightSummary {
    /// Hover, aggressive, cruise (always in this order).
    pub modes: Vec<ModeResult>,
    /// Estimated maximum airspeed [m/s].
    pub v_max_ms: Option<f64>,
}

fn minutes(e_wh: f64, p_w: f64) -> Option<f64> {
    (e_wh.is_finite() && p_w.is_finite() && p_w > 1e-9).then(|| e_wh / p_w * 60.0)
}

/// Fill in a mode from a solved throttle on `unit` at airspeed `v`.
fn solved(
    kind: ModeKind,
    unit: &PropUnit,
    throttle: f64,
    v: f64,
    hotel_w: f64,
    e_wh: f64,
) -> ModeResult {
    let op = unit.operating_point(throttle, v);
    let n = unit.n_motors as f64;
    let power = n * op.p_elec_w + hotel_w;
    let mut r = ModeResult::blank(kind, ModeStatus::Ok);
    r.throttle = Some(throttle);
    r.airspeed_ms = Some(v);
    r.current_a = Some(n * op.i_bat_a);
    r.power_w = Some(power);
    r.time_min = minutes(e_wh, power);
    r
}

/// Highest airspeed at or above `v_start` where `throttle` still gives thrust >= drag.
/// `None` if the thrust is already short of the drag at `v_start`.
fn level_speed(
    unit: &PropUnit,
    throttle: f64,
    drag: &dyn Fn(f64) -> f64,
    v_start: f64,
) -> Option<f64> {
    let excess = |v: f64| unit.operating_point(throttle, v).thrust_n - drag(v);
    let v0 = v_start.max(1.0);
    let e0 = excess(v0);
    if !e0.is_finite() || e0 < 0.0 {
        return None;
    }
    let (mut lo, mut hi) = (v0, v0);
    let mut v = v0;
    while v < 250.0 {
        v += 1.0;
        let e = excess(v);
        if e.is_finite() && e >= 0.0 {
            lo = v;
        } else {
            hi = v;
            break;
        }
    }
    if hi <= lo {
        return Some(lo);
    }
    for _ in 0..25 {
        let mid = 0.5 * (lo + hi);
        if excess(mid) >= 0.0 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Some(lo)
}

/// Compute the three flight modes and the maximum speed for the project's vehicle.
pub fn flight_summary(p: &Project, m: &Model, setup: &Setup) -> FlightSummary {
    let hotel = p.p_hotel.max(0.0);
    let e_fw = m.weight.pack_wh * p.dod;
    let mut hover = ModeResult::blank(ModeKind::Hover, ModeStatus::NotApplicable);
    let mut aggr = ModeResult::blank(ModeKind::Aggressive, ModeStatus::Unreachable);
    let mut cruise = ModeResult::blank(ModeKind::Cruise, ModeStatus::Unreachable);
    let mut v_max = None;

    // Lift group: hover, and for a pure multirotor also aggressive, cruise and maximum speed.
    if let (true, Some(lift)) = (p.drone_type.is_multirotor(), setup.lift.as_ref()) {
        let n = lift.n_motors as f64;
        let e_mr = m.mr.e_usable_wh;
        let d_hover = lift.throttle_for_thrust(m.mr.t_hover_motor, 0.0);
        hover = match d_hover {
            Some(d) => {
                let mut r = solved(ModeKind::Hover, lift, d, 0.0, hotel, e_mr);
                r.reference_time_min = Some(m.mr.hover_endurance_min);
                r
            }
            None => ModeResult::blank(ModeKind::Hover, ModeStatus::Unreachable),
        };
        if p.drone_type == DroneType::Multirotor {
            let d_a = (AGGRESSIVE_HOVER_FACTOR * d_hover.unwrap_or(1.0))
                .clamp(AGGRESSIVE_THROTTLE_MR_MIN, 1.0);
            aggr = solved(ModeKind::Aggressive, lift, d_a, 0.0, hotel, e_mr);
            // Cruise: existing forward-flight power, throttle by inverting the model.
            let p_motors = m.mr.p_total_cruise - hotel;
            cruise = match lift.throttle_for_power(p_motors / n, 0.0) {
                Some(d) if p_motors > 0.0 => {
                    let op = lift.operating_point(d, 0.0);
                    let mut r = ModeResult::blank(ModeKind::Cruise, ModeStatus::Ok);
                    r.throttle = Some(d);
                    r.airspeed_ms = Some(p.mr_v_cruise);
                    r.current_a = Some(n * op.i_bat_a);
                    r.power_w = Some(m.mr.p_total_cruise);
                    r.time_min = Some(m.mr.cruise_endurance_min);
                    r.reference_time_min = Some(m.mr.cruise_endurance_min);
                    r.basis = TimeBasis::ExistingModel;
                    r
                }
                _ => ModeResult::blank(ModeKind::Cruise, ModeStatus::Unreachable),
            };
            // Maximum speed: tilted full-throttle thrust vs weight and parasite drag.
            let full = lift.operating_point(1.0, 0.0);
            let t_total = n * full.thrust_n;
            if t_total > m.mr.w {
                let f_area = if p.mr_flat_plate_area > 0.0 {
                    p.mr_flat_plate_area
                } else {
                    0.01
                };
                let d_max = (t_total * t_total - m.mr.w * m.mr.w).sqrt();
                let v_drag = (2.0 * d_max / (lift.rho * f_area)).sqrt();
                let v_pitch = PITCH_SPEED_FRACTION * full.rpm / 60.0 * lift.prop.pitch_m;
                v_max = Some(v_drag.min(v_pitch));
            }
        }
    }

    // Forward group: fixed wing and VTOL cruise propeller.
    if let (true, Some(fwd)) = (p.drone_type.is_fixed_wing(), setup.forward.as_ref()) {
        let drag = |v: f64| m.weight.w / performance::at_speed(v, &m.perf_in).ld;
        let v_start = m.cons.v_stall.max(1.0);
        let t_req = drag(p.v_cr);
        if let Some(d) = fwd.throttle_for_thrust(t_req, p.v_cr) {
            cruise = solved(ModeKind::Cruise, fwd, d, p.v_cr, hotel, e_fw);
            cruise.reference_time_min = Some(m.perf_cr.endurance_min());
        }
        let d_cruise = cruise.throttle.unwrap_or(1.0);
        let d_a = (AGGRESSIVE_CRUISE_FACTOR * d_cruise).clamp(AGGRESSIVE_THROTTLE_FW_MIN, 1.0);
        if let Some(v) = level_speed(fwd, d_a, &drag, v_start) {
            aggr = solved(ModeKind::Aggressive, fwd, d_a, v, hotel, e_fw);
        }
        v_max = level_speed(fwd, 1.0, &drag, v_start);
    }

    FlightSummary {
        modes: vec![hover, aggr, cruise],
        v_max_ms: v_max,
    }
}
