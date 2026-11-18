// SPDX-License-Identifier: AGPL-3.0-or-later
//! Thermal management and power-distribution estimates: electronics heating, cable gauge,
//! voltage drop and battery sag. All results are lumped steady-state estimates, not measurements.

pub mod cable;
pub mod heat;
pub mod system;
