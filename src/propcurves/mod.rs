// SPDX-License-Identifier: AGPL-3.0-or-later
//! Propulsion performance curves and flight time by flight mode.
//!
//! Extends the existing momentum-theory propulsion, multirotor hover and range/endurance
//! calculations with throttle-dependent motor, propeller and battery behaviour: thrust,
//! current, power, efficiency, RPM and an estimated winding temperature versus throttle, limit
//! flags, and flight time / speed estimates for hover, aggressive manoeuvring and cruise.
//!
//! These are model estimates, not bench-test data. See [`model`] for the assumptions and
//! constants, [`setup`] for how the default motor is derived from the project, and [`modes`]
//! for the flight-mode assumptions.

pub mod curves;
pub mod model;
pub mod modes;
pub mod setup;

pub use curves::{Curve, CurvePoint, LimitFlags, N_STEPS, sweep};
pub use model::{OperatingPoint, PropUnit};
pub use modes::{FlightSummary, ModeKind, ModeResult, ModeStatus, TimeBasis, flight_summary};
pub use setup::{Group, Overrides, Setup, build};

#[cfg(test)]
mod tests;
