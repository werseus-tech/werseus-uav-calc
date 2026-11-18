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
//!   limits are ignored; the rotor inflow at the tilted attitude is included.
//! * Hotel load (avionics and payload power) is added to every mode. Energy is the usable pack
//!   energy (capacity x depth of discharge); the time is computed from the power drawn from the cells (open-circuit voltage times
//!   battery current), so the pack resistance loss is included.

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
    // The usable energy is open-circuit (charge x nominal voltage), so the time follows from
    // the power drawn from the cells: open-circuit voltage times the total battery current,
    // which includes the heat lost in the pack resistance. The hotel load is a constant power
    // on the loaded bus, so it draws `hotel_w / v_bus` of current.
    let i_total = n * op.i_bat_a + hotel_w / op.v_bus.max(0.1);
    r.time_min = minutes(e_wh, unit.pack.v_oc * i_total);
    r
}

/// Highest airspeed at or above `v_start` where `throttle` still gives thrust >= drag.
/// `None` if the thrust is short of the drag everywhere on the scanned range. The drag of a
/// fixed wing is U-shaped (induced drag near the stall speed), so the whole grid is scanned
/// rather than stopping at the first shortfall.
pub(super) fn level_speed(
    unit: &PropUnit,
    throttle: f64,
    drag: &dyn Fn(f64) -> f64,
    v_start: f64,
) -> Option<f64> {
    let excess = |v: f64| unit.operating_point(throttle, v).thrust_n - drag(v);
    let ok = |v: f64| {
        let e = excess(v);
        e.is_finite() && e >= 0.0
    };
    let v0 = v_start.max(1.0);
    let mut best: Option<f64> = None;
    let mut v = v0;
    while v <= 250.0 {
        if ok(v) {
            best = Some(v);
        }
        v += 1.0;
    }
    let lo0 = best?;
    let (mut lo, mut hi) = (lo0, lo0 + 1.0);
    if lo0 >= 250.0 || ok(hi) {
        return Some(lo0);
    }
    for _ in 0..25 {
        let mid = 0.5 * (lo + hi);
        if ok(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Some(lo)
}

/// Multirotor maximum speed [m/s] with the rotor inflow at the tilted attitude. At speed `v`
/// the required thrust is `sqrt(W^2 + D^2)` (`D` the parasite drag) with the rotor axis tilted
/// by `sin(alpha) = D / T`; the axial inflow `v sin(alpha)` unloads the rotors. Returns the
/// highest speed where full-throttle thrust still meets the requirement, limited to
/// [`PITCH_SPEED_FRACTION`] of the propeller pitch speed at that point.
pub(super) fn multirotor_v_max(lift: &PropUnit, weight_n: f64, f_area: f64) -> Option<f64> {
    let n = lift.n_motors as f64;
    let need = |v: f64| {
        let d = 0.5 * lift.rho * f_area * v * v;
        let t = (weight_n * weight_n + d * d).sqrt();
        (t, d / t)
    };
    let op_at = |v: f64| {
        let (t, sin_a) = need(v);
        (t, lift.operating_point(1.0, v * sin_a))
    };
    let feasible = |v: f64| {
        let (t, op) = op_at(v);
        n * op.thrust_n >= t && PITCH_SPEED_FRACTION * op.rpm / 60.0 * lift.prop.pitch_m >= v
    };
    if !feasible(0.0) {
        return None;
    }
    let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
    while hi < 250.0 && feasible(hi) {
        lo = hi;
        hi *= 2.0;
    }
    for _ in 0..40 {
        let mid = 0.5 * (lo + hi);
        if feasible(mid) {
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
            if let Some(dh) = d_hover {
                let d_a = (AGGRESSIVE_HOVER_FACTOR * dh).clamp(AGGRESSIVE_THROTTLE_MR_MIN, 1.0);
                aggr = solved(ModeKind::Aggressive, lift, d_a, 0.0, hotel, e_mr);
            }
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
            let f_area = if p.mr_flat_plate_area > 0.0 {
                p.mr_flat_plate_area
            } else {
                0.01
            };
            v_max = multirotor_v_max(lift, m.mr.w, f_area);
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
