// SPDX-License-Identifier: AGPL-3.0-or-later
//! Fixed-wing starting values for ArduPilot Plane and PX4 fixed-wing (also the cruise part of a
//! quadplane): airspeeds, TECS climb/sink limits, attitude limits and attitude time constants.
//!
//! Covered (from the program's own performance model):
//! * airspeeds — minimum = max(1.25·V_stall, best-endurance speed) (ArduPilot advises keeping
//!   `AIRSPEED_MIN` comfortably above stall), cruise = design cruise speed, maximum ≈ 1.35·cruise
//!   (heuristic: the model has no V_max);
//! * TECS — max climb = design rate of climb, min sink = V_end / (L/D)_end, max sink ≈ 3× min
//!   sink (clamped 2…10 m/s);
//! * limits — bank = design turn bank angle, pitch up = climb angle + 5°, pitch down from max sink;
//! * roll/pitch time constants (`RLL2SRV_TCONST`, `PTCH2SRV_TCONST`, `FW_R_TC`, `FW_P_TC`) grow
//!   with span (heuristic 0.35 + 0.1·b, clamped 0.4…1.0 s; defaults are 0.5 s / 0.4 s).
//!
//! Not covered: roll/pitch/yaw *rate* P/I/D/FF gains. They depend on control-surface size,
//! hinge-moment and servo data and on aerodynamic damping derivatives that the model does not
//! contain; keep the firmware defaults and use ArduPilot AUTOTUNE or PX4 fixed-wing autotune.
use super::firmware::Param;
use crate::calc::Model;
use crate::project::Project;

#[derive(Clone, Debug, PartialEq)]
pub struct FixedWingHints {
    pub v_stall: f64,
    pub v_min: f64,
    pub v_cruise: f64,
    pub v_max: f64,
    pub climb_max: f64,
    pub sink_min: f64,
    pub sink_max: f64,
    pub roll_limit_deg: f64,
    pub pitch_max_deg: f64,
    pub pitch_min_deg: f64,
    pub tconst: f64,
    pub ardupilot: Vec<Param>,
    pub px4: Vec<Param>,
}

fn pos(x: f64, fb: f64) -> f64 {
    if x.is_finite() && x > 0.0 { x } else { fb }
}

pub fn fixed_wing_hints(p: &Project, m: &Model) -> FixedWingHints {
    let v_stall = pos(m.aero.v_stall, 10.0);
    let v_min = (1.25 * v_stall).max(pos(m.aero.v_end, 0.0));
    // The cruise speed must stay above the minimum speed (and a margin above stall).
    let v_cruise = pos(p.v_cr, 1.6 * v_stall).max((v_min / 0.95).max(1.3 * v_stall));
    let v_max = 1.35 * v_cruise;
    let climb_max = pos(p.rc, 3.0).clamp(0.5, 15.0);
    let sink_min = if m.aero.ld_end.is_finite() && m.aero.ld_end > 1.0 {
        pos(m.aero.v_end, v_min) / m.aero.ld_end
    } else {
        1.0
    }
    .clamp(0.3, 5.0);
    let sink_max = (3.0 * sink_min).clamp(2.0, 10.0);
    let roll_limit_deg = pos(p.phi_turn_deg, 45.0).clamp(30.0, 65.0);
    let v_y = pos(p.v_y, v_cruise);
    let pitch_max_deg =
        ((climb_max / v_y).clamp(0.0, 0.9).asin().to_degrees() + 5.0).clamp(10.0, 30.0);
    let pitch_min_deg =
        -((sink_max / v_cruise).clamp(0.0, 0.9).asin().to_degrees() + 5.0).clamp(10.0, 30.0);
    let tconst = (0.35 + 0.1 * pos(p.b, 1.5)).clamp(0.4, 1.0);

    let ardupilot = vec![
        Param::num("AIRSPEED_MIN", v_min.round(), 0),
        Param::num("AIRSPEED_CRUISE", v_cruise, 1),
        Param::num("AIRSPEED_MAX", v_max.round(), 0),
        Param::num("TECS_CLMB_MAX", climb_max, 1),
        Param::num("TECS_SINK_MIN", sink_min, 1),
        Param::num("TECS_SINK_MAX", sink_max, 1),
        Param::num("ROLL_LIMIT_DEG", roll_limit_deg.round(), 0),
        Param::num("PTCH_LIM_MAX_DEG", pitch_max_deg.round(), 0),
        Param::num("PTCH_LIM_MIN_DEG", pitch_min_deg.round(), 0),
        Param::num("RLL2SRV_TCONST", tconst, 2),
        Param::num("PTCH2SRV_TCONST", tconst, 2),
    ];
    let px4 = vec![
        Param::num("FW_AIRSPD_STALL", v_stall, 1),
        Param::num("FW_AIRSPD_MIN", v_min, 1),
        Param::num("FW_AIRSPD_TRIM", v_cruise, 1),
        Param::num("FW_AIRSPD_MAX", v_max, 1),
        Param::num("FW_T_CLMB_MAX", climb_max, 1),
        Param::num("FW_T_SINK_MIN", sink_min, 1),
        Param::num("FW_T_SINK_MAX", sink_max, 1),
        Param::num("FW_R_LIM", roll_limit_deg.round(), 0),
        Param::num("FW_P_LIM_MAX", pitch_max_deg.round(), 0),
        Param::num("FW_P_LIM_MIN", pitch_min_deg.round(), 0),
        Param::num("FW_R_TC", tconst, 2),
        Param::num("FW_P_TC", tconst, 2),
    ];
    FixedWingHints {
        v_stall,
        v_min,
        v_cruise,
        v_max,
        climb_max,
        sink_min,
        sink_max,
        roll_limit_deg,
        pitch_max_deg,
        pitch_min_deg,
        tconst,
        ardupilot,
        px4,
    }
}
