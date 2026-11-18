// SPDX-License-Identifier: AGPL-3.0-or-later
//! Approximate 6-DoF flight simulator for the designed aircraft (learning tool, not a
//! certified flight model).
//!
//! - [`vehicle`]: turns the project and its evaluated `Model` into physical parameters,
//!   reusing the design calculations (mass, CG, static margin, polar, thrust, battery).
//! - [`inertia`]: inertia tensor estimate behind a single function.
//! - [`aero`]: fixed-wing coefficients incl. stall, sideslip, surfaces and damping.
//! - [`env`]: wind, turbulence and gusts.
//! - [`control`]: pilot input, stabilised / manual modes, rotor allocation.
//! - [`world`]: rigid-body integration (RK4, fixed 400 Hz step), ground contact, battery,
//!   VTOL transition logic and scenarios.
//!
//! No egui dependency: the UI lives in `ui::flightsim`.
pub mod aero;
pub mod control;
pub mod env;
pub mod inertia;
pub mod math;
pub mod terrain;
pub mod vehicle;
pub mod world;

pub use control::{ControlMode, PilotInput, VtolPhase};
pub use env::WindConfig;
pub use terrain::{BuildingBox, EnvironmentKind, SurfaceType, WorldMesh};
pub use vehicle::{TakeoffMode, VehicleParams, default_takeoff_for};
pub use world::{CrashReason, DT, FlightStatus, Sim, SimConfig, StartPosition, Telemetry};

/// Builds a simulator for a project (evaluates the design model first).
pub fn sim_for_project(
    p: &crate::project::Project,
    m: &crate::calc::Model,
    start: StartPosition,
) -> Sim {
    Sim::new(VehicleParams::from_project(p, m), start)
}

/// Builds a simulator for a project with an explicit takeoff mode and environment.
pub fn sim_for_project_with(
    p: &crate::project::Project,
    m: &crate::calc::Model,
    start: StartPosition,
    takeoff: TakeoffMode,
    env: EnvironmentKind,
) -> Sim {
    let mut sim = Sim::new(
        VehicleParams::from_project_with_takeoff(p, m, takeoff),
        start,
    );
    sim.set_environment(env);
    sim
}

#[cfg(test)]
mod tests;
