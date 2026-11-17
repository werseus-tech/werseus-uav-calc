// SPDX-License-Identifier: AGPL-3.0-or-later
//! Propeller / rotor noise estimate: tip speed, blade-pass frequency, overall SPL and dBA at an
//! observer, a 1/3-octave spectrum for plotting, and a ranked table of alternative propellers.
//!
//! This is a rough, semi-empirical estimate (expect several dB of error). It is not a
//! measurement and not a certification-grade prediction. See [`method`] for the model and its
//! validity range.
pub mod alternatives;
pub mod method;
pub mod sources;
pub mod weighting;
