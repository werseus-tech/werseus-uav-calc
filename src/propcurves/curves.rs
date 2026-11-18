// SPDX-License-Identifier: AGPL-3.0-or-later
//! Throttle sweeps (0..100 % in 5 % steps) with limit flags. Model estimates only.

use super::model::{OperatingPoint, PropUnit};

/// Number of throttle samples: 0, 5, ..., 100 %.
pub const N_STEPS: usize = 21;
/// Relative tolerance before a current counts as exceeding its limit (the derived default
/// ratings sit exactly at the full-throttle design current).
const LIMIT_TOL: f64 = 1.001;

/// Which limits a sample violates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LimitFlags {
    /// Motor current above the motor rating.
    pub motor_current: bool,
    /// Motor current above the ESC rating.
    pub esc_current: bool,
    /// Total battery current above the pack continuous limit (cell C rating x parallel).
    pub battery: bool,
    /// Winding temperature above the limit.
    pub temperature: bool,
    /// Rotation speed above the tip Mach limit.
    pub tip_speed: bool,
}

impl LimitFlags {
    pub fn any(&self) -> bool {
        self.motor_current || self.esc_current || self.battery || self.temperature || self.tip_speed
    }
}

/// One sample of a sweep (per motor unless the name says total).
#[derive(Clone, Copy, Debug)]
pub struct CurvePoint {
    pub op: OperatingPoint,
    /// Battery current of all motors together [A].
    pub total_current_a: f64,
    /// Thrust per electrical watt [gf/W]; zero at zero throttle.
    pub efficiency_gf_per_w: f64,
    pub flags: LimitFlags,
}

/// A full sweep at one airspeed.
#[derive(Clone, Debug)]
pub struct Curve {
    pub airspeed: f64,
    pub points: Vec<CurvePoint>,
}

impl Curve {
    /// Index of the most efficient sample (largest gf/W).
    pub fn peak_efficiency_index(&self) -> usize {
        let mut best = 0;
        for (i, p) in self.points.iter().enumerate() {
            if p.efficiency_gf_per_w > self.points[best].efficiency_gf_per_w {
                best = i;
            }
        }
        best
    }

    /// First throttle (fraction) at which any limit is violated.
    pub fn first_limit_throttle(&self) -> Option<f64> {
        self.points
            .iter()
            .find(|p| p.flags.any())
            .map(|p| p.op.throttle)
    }
}

/// Evaluate the limits for one operating point of `unit`.
pub fn flags_for(unit: &PropUnit, op: &OperatingPoint) -> LimitFlags {
    let total = op.i_bat_a * unit.n_motors as f64;
    LimitFlags {
        motor_current: op.i_motor_a > unit.motor.i_max_a * LIMIT_TOL,
        esc_current: op.i_motor_a > unit.esc.i_max_a * LIMIT_TOL,
        battery: total > unit.pack.max_cont_a * LIMIT_TOL,
        temperature: op.temp_c > unit.motor.t_limit_c,
        tip_speed: op.rpm > unit.rpm_limit * LIMIT_TOL,
    }
}

/// Make a curve point from an operating point.
pub fn point(unit: &PropUnit, op: OperatingPoint) -> CurvePoint {
    let eff = if op.p_elec_w > 1e-9 {
        op.thrust_gf() / op.p_elec_w
    } else {
        0.0
    };
    CurvePoint {
        total_current_a: op.i_bat_a * unit.n_motors as f64,
        efficiency_gf_per_w: eff,
        flags: flags_for(unit, &op),
        op,
    }
}

/// Sweep throttle at the given airspeed [m/s] (0 = static).
pub fn sweep(unit: &PropUnit, airspeed: f64) -> Curve {
    let points = (0..N_STEPS)
        .map(|i| {
            let throttle = i as f64 / (N_STEPS - 1) as f64;
            point(unit, unit.operating_point(throttle, airspeed))
        })
        .collect();
    Curve { airspeed, points }
}
