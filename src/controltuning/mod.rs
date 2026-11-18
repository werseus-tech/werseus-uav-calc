// SPDX-License-Identifier: AGPL-3.0-or-later
//! Control tuning aids for the designed vehicle:
//!
//! 1. Starting rate-loop PID and filter suggestions for Betaflight, ArduPilot and PX4 from a
//!    simple per-axis plant (inertia + first-order motor lag + loop delay), see [`loopdesign`]
//!    and [`firmware`]. Fixed-wing cruise hints in [`fixedwing`].
//! 2. A motor-loss controllability matrix based on the attainable control set / available
//!    control authority index (Du et al. 2015), see [`failure`].
//!
//! Everything here is a *starting point* computed from a simplified model; real vehicles must be
//! test-flown carefully and tuned. The inertia comes from [`vehicle::inertia_estimate`], the one
//! function to replace when a full mass-properties model is available.
pub mod failure;
pub mod firmware;
pub mod fixedwing;
pub mod loopdesign;
pub mod vehicle;

use crate::calc::Model;
use crate::project::Project;
use failure::FailureReport;
use firmware::{Firmware, FirmwareTune};
use fixedwing::FixedWingHints;
use vehicle::{VehicleData, VehicleKind};

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct TuneOptions {
    /// Motor time constant override [s]; `None` uses the estimate.
    pub motor_tau: Option<f64>,
    /// Also evaluate every pair of failed motors.
    pub include_pairs: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ControlReport {
    pub vehicle: VehicleData,
    /// One entry per firmware (in [`Firmware::ALL`] order); empty for a pure fixed wing.
    pub firmwares: Vec<FirmwareTune>,
    pub fixed_wing: Option<FixedWingHints>,
    pub failures: Option<FailureReport>,
}

pub fn analyze(p: &Project, m: &Model, opts: &TuneOptions) -> ControlReport {
    let vehicle = vehicle::vehicle_data(p, m, opts.motor_tau);
    let hover = vehicle.kind != VehicleKind::FixedWing && !vehicle.rotors.is_empty();
    let firmwares = if hover {
        Firmware::ALL
            .iter()
            .map(|&fw| firmware::tune_firmware(fw, &vehicle))
            .collect()
    } else {
        Vec::new()
    };
    let fixed_wing =
        (vehicle.kind != VehicleKind::Multirotor).then(|| fixedwing::fixed_wing_hints(p, m));
    let failures = hover.then(|| {
        failure::analyze_failures(
            &vehicle.rotors,
            vehicle.weight,
            vehicle.arm_ref,
            opts.include_pairs,
        )
    });
    ControlReport {
        vehicle,
        firmwares,
        fixed_wing,
        failures,
    }
}

#[cfg(test)]
mod tests;
