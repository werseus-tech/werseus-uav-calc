// SPDX-License-Identifier: AGPL-3.0-or-later
//! Structural strength and vibration analysis with a small in-house beam finite-element model.
//!
//! Members (multirotor arms, VTOL booms, half wing spar) are modelled as clamped-free
//! Euler-Bernoulli beams with consistent mass and lumped tip masses; natural frequencies come
//! from the generalised eigenproblem `K x = lambda M x`, static load cases from `K u = f`.
//! This is a simplified beam model, not a full 3D finite-element analysis.

pub mod analysis;
pub mod beam;
pub mod eigen;
pub mod material;

#[cfg(test)]
mod tests;
