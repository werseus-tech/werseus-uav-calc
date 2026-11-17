// SPDX-License-Identifier: AGPL-3.0-or-later
//! Propeller alternatives for the same required thrust, ranked by dBA.
//!
//! For a rotor of diameter `D` producing thrust `T = Ct rho n^2 D^4` with a fixed thrust
//! coefficient `Ct` (the project keeps `Ct = 0.10` for its hover RPM estimate), a diameter scale
//! factor `f = D' / D` at equal thrust gives:
//!
//! * `n' = n / f^2` (rotational speed), so tip speed scales with `1 / f`;
//! * `P' = P / f` (momentum theory: `P = T^1.5 / (FM sqrt(2 rho A))`, same figure of merit).
//!
//! A different blade count keeps `n` and `P` (same `Ct`, a simplification: real propellers with
//! more blades usually need a slightly lower pitch or RPM for the same thrust). The results are
//! a suggestion table, nothing in the project is changed.

use super::method::{Air, Observer, RotorSet, estimate};
use super::sources::Source;

/// Diameter scale factors tried (1.0 is the current propeller).
pub const DIAMETER_FACTORS: [f64; 5] = [1.0, 1.1, 1.2, 1.3, 1.5];
/// Blade counts tried (the current count is always added).
pub const BLADE_COUNTS: [u32; 3] = [2, 3, 4];

/// One candidate propeller.
#[derive(Clone, Copy, Debug)]
pub struct Alternative {
    /// Diameter (m).
    pub diameter_m: f64,
    /// Diameter relative to the current propeller.
    pub diameter_factor: f64,
    /// Blades per rotor.
    pub blades: u32,
    /// Shaft speed at equal thrust (rev/min).
    pub rpm: f64,
    /// Shaft power per rotor at equal thrust (W).
    pub shaft_power_w: f64,
    /// Helical tip Mach number.
    pub tip_mach: f64,
    /// Blade-pass frequency (Hz).
    pub bpf: f64,
    /// Blade-pass frequency minus the current one (Hz, negative = lower).
    pub bpf_shift_hz: f64,
    /// Overall level (dB) at the observer.
    pub oaspl: f64,
    /// A-weighted overall level (dBA) at the observer.
    pub oaspl_a: f64,
    /// dBA minus the current propeller's dBA (negative = quieter).
    pub delta_dba: f64,
    /// True for the propeller currently in the project.
    pub is_current: bool,
    /// False when the diameter exceeds the known airframe limit.
    pub fits_frame: bool,
}

/// Candidate rotor group at equal thrust.
pub fn scaled_rotors(base: &RotorSet, diameter_factor: f64, blades: u32) -> RotorSet {
    let f = if diameter_factor.is_finite() && diameter_factor > 0.0 {
        diameter_factor
    } else {
        1.0
    };
    RotorSet {
        diameter_m: base.diameter_m * f,
        blades,
        n_rotors: base.n_rotors,
        rpm: base.rpm / (f * f),
        shaft_power_w: base.shaft_power_w / f,
        forward_speed_ms: base.forward_speed_ms,
    }
}

/// Alternatives ranked by dBA, quietest first. Candidates whose estimate is invalid are left
/// out; an invalid current propeller gives an empty list.
pub fn alternatives(src: &Source, air: &Air, obs: &Observer) -> Vec<Alternative> {
    let cur = estimate(&src.rotors, air, obs);
    if !cur.valid {
        return Vec::new();
    }
    let mut blade_set: Vec<u32> = BLADE_COUNTS.to_vec();
    if !blade_set.contains(&src.rotors.blades) {
        blade_set.push(src.rotors.blades);
    }
    blade_set.sort_unstable();
    let mut out = Vec::new();
    for f in DIAMETER_FACTORS {
        for &b in &blade_set {
            let rs = scaled_rotors(&src.rotors, f, b);
            let e = estimate(&rs, air, obs);
            if !e.valid {
                continue;
            }
            out.push(Alternative {
                diameter_m: rs.diameter_m,
                diameter_factor: f,
                blades: b,
                rpm: rs.rpm,
                shaft_power_w: rs.shaft_power_w,
                tip_mach: e.helical_mach,
                bpf: e.bpf,
                bpf_shift_hz: e.bpf - cur.bpf,
                oaspl: e.oaspl,
                oaspl_a: e.oaspl_a,
                delta_dba: e.oaspl_a - cur.oaspl_a,
                is_current: f == 1.0 && b == src.rotors.blades,
                fits_frame: src
                    .max_diameter_m
                    .is_none_or(|mx| rs.diameter_m <= mx * (1.0 + 1e-9)),
            });
        }
    }
    out.sort_by(|a, b| a.oaspl_a.total_cmp(&b.oaspl_a));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acoustics::sources::OpPoint;

    fn src() -> Source {
        Source {
            point: OpPoint::LiftHover,
            rotors: RotorSet {
                diameter_m: 0.127,
                blades: 2,
                n_rotors: 4,
                rpm: 12_000.0,
                shaft_power_w: 60.0,
                forward_speed_ms: 0.0,
            },
            thrust_n: 5.0,
            max_diameter_m: Some(0.16),
        }
    }

    #[test]
    fn equal_thrust_scaling_keeps_d4_n2() {
        let b = src().rotors;
        for f in DIAMETER_FACTORS {
            let r = scaled_rotors(&b, f, 2);
            let k0 = b.diameter_m.powi(4) * b.rpm.powi(2);
            let k1 = r.diameter_m.powi(4) * r.rpm.powi(2);
            assert!((k0 - k1).abs() / k0 < 1e-12, "{f}");
        }
    }

    #[test]
    fn larger_slower_prop_has_lower_bpf_and_is_quieter() {
        let s = src();
        let rows = alternatives(&s, &Air::default(), &Observer::default());
        let cur = rows.iter().find(|r| r.is_current).unwrap();
        let big = rows
            .iter()
            .find(|r| r.diameter_factor == 1.3 && r.blades == cur.blades)
            .unwrap();
        assert!(big.bpf < cur.bpf);
        assert!(big.bpf_shift_hz < 0.0);
        assert!(big.oaspl_a < cur.oaspl_a);
        assert!(big.delta_dba < 0.0);
        assert!(!big.fits_frame);
        assert!(cur.fits_frame);
    }

    #[test]
    fn ranked_ascending_with_one_current_row() {
        let rows = alternatives(&src(), &Air::default(), &Observer::default());
        assert_eq!(rows.len(), DIAMETER_FACTORS.len() * BLADE_COUNTS.len());
        assert_eq!(rows.iter().filter(|r| r.is_current).count(), 1);
        for w in rows.windows(2) {
            assert!(w[0].oaspl_a <= w[1].oaspl_a);
        }
        let cur = rows.iter().find(|r| r.is_current).unwrap();
        assert!(cur.delta_dba.abs() < 1e-9);
    }

    #[test]
    fn unusual_blade_count_is_added_and_bad_source_is_empty() {
        let mut s = src();
        s.rotors.blades = 5;
        let rows = alternatives(&s, &Air::default(), &Observer::default());
        assert!(rows.iter().any(|r| r.is_current && r.blades == 5));
        s.rotors.rpm = 0.0;
        assert!(alternatives(&s, &Air::default(), &Observer::default()).is_empty());
    }
}
