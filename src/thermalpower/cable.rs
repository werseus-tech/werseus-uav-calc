// SPDX-License-Identifier: AGPL-3.0-or-later
//! Copper cable runs: AWG resistance, voltage drop, heat, wire temperature rise and the
//! thinnest gauge meeting a drop and temperature limit.
//!
//! Model: diameter `d = 0.127 mm * 92^((36-AWG)/39)`, resistivity
//! `rho(T) = 1.724e-8 ohm*m * (1 + 0.00393 (T - 20))`. A run is a go and a return conductor.
//! Wire temperature rise per conductor: `dT = I^2 r(T) / (h * pi * d_outer)` where `h` combines
//! horizontal-cylinder natural convection (Churchill–Chu), crossflow forced convection
//! (Churchill–Bernstein), and silicone-jacket thermal radiation at the outer diameter `d_outer`,
//! solved by fixed-point iteration because `r` and `h` depend on wire temperature.

/// Copper resistivity at 20 C, ohm*m.
pub const RHO_CU_20C: f64 = 1.724e-8;
/// Temperature coefficient of copper resistance, 1/K.
pub const ALPHA_CU: f64 = 0.00393;
/// Copper density, kg/m3.
pub const CU_DENSITY: f64 = 8960.0;
/// Reference temperature of the resistivity, C.
pub const T_REF_C: f64 = 20.0;
/// Thickest supported gauge.
pub const AWG_MIN: u32 = 8;
/// Thinnest supported gauge.
pub const AWG_MAX: u32 = 26;
/// Radial thickness of the silicone insulation, m.
pub const INSULATION_THICKNESS_M: f64 = 0.0005;
/// Clamp of the computed temperature rise, K.
pub const MAX_RISE_K: f64 = 300.0;
/// Minimum clamped ambient/limit temperature, C.
pub const MIN_TEMP_C: f64 = -100.0;
/// Maximum clamped ambient/limit temperature, C.
pub const MAX_TEMP_C: f64 = 500.0;

const PR_AIR: f64 = 0.71;
const G_ACCEL: f64 = 9.81;
const SIGMA_SB: f64 = 5.670_374_419e-8;
const EMISSIVITY_INSULATION: f64 = 0.90;

fn pos(x: f64) -> f64 {
    if x.is_finite() && x > 0.0 { x } else { 0.0 }
}

fn clean_temp(t: f64, default: f64) -> f64 {
    if t.is_finite() {
        t.clamp(MIN_TEMP_C, MAX_TEMP_C)
    } else {
        default
    }
}

/// Surface heat-transfer coefficient for a horizontal insulated wire in air, W/m2K.
///
/// Combines Churchill–Chu natural convection, Churchill–Bernstein crossflow forced convection
/// (mixed via Churchill–Usagi), and surface radiation at the outer wire diameter `d_o_m`.
pub fn film_coefficient(d_o_m: f64, ambient_c: f64, rise_k: f64, air_speed_ms: f64) -> f64 {
    let d_o = pos(d_o_m).max(1e-4);
    let amb = clean_temp(ambient_c, 20.0);
    let rise = pos(rise_k).min(MAX_RISE_K);
    let v = pos(air_speed_ms).min(100.0);

    let t_amb_k = (amb + 273.15).max(150.0);
    let t_wire_k = (amb + rise + 273.15).max(150.0);
    let t_film_k = 0.5 * (t_amb_k + t_wire_k);
    let tr = t_film_k / 273.15;
    let k_air = 0.02414 * tr.powf(0.81);
    let nu_air = 1.33e-5 * tr.powf(1.7);

    let ra = G_ACCEL * (rise / t_film_k) * d_o.powi(3) * PR_AIR / (nu_air * nu_air);
    let nat_denom = (1.0 + (0.559 / PR_AIR).powf(9.0 / 16.0)).powf(8.0 / 27.0);
    let nu_nat = (0.60 + 0.387 * ra.powf(1.0 / 6.0) / nat_denom).powi(2);
    let h_nat = nu_nat * k_air / d_o;

    let h_forced = if v > 0.0 {
        let re = v * d_o / nu_air;
        let f_denom = (1.0 + (0.4 / PR_AIR).powf(2.0 / 3.0)).powf(0.25);
        let nu_forced = 0.3
            + (0.62 * re.sqrt() * PR_AIR.powf(1.0 / 3.0) / f_denom)
                * (1.0 + (re / 282_000.0).powf(5.0 / 8.0)).powf(4.0 / 5.0);
        nu_forced * k_air / d_o
    } else {
        0.0
    };

    let h_conv = (h_nat.powi(3) + h_forced.powi(3)).cbrt();
    let h_rad = EMISSIVITY_INSULATION
        * SIGMA_SB
        * (t_wire_k * t_wire_k + t_amb_k * t_amb_k)
        * (t_wire_k + t_amb_k);
    h_conv + h_rad
}

/// Conductor diameter, m (gauge clamped to the supported range).
pub fn awg_diameter_m(awg: u32) -> f64 {
    let a = awg.clamp(AWG_MIN, AWG_MAX) as f64;
    0.127e-3 * 92f64.powf((36.0 - a) / 39.0)
}

/// Conductor cross-section, m2.
pub fn awg_area_m2(awg: u32) -> f64 {
    let d = awg_diameter_m(awg);
    std::f64::consts::FRAC_PI_4 * d * d
}

/// Resistance per metre of one conductor at `temp_c`, ohm/m.
pub fn resistance_per_m(awg: u32, temp_c: f64) -> f64 {
    let t = if temp_c.is_finite() { temp_c } else { T_REF_C };
    let f = (1.0 + ALPHA_CU * (t - T_REF_C)).max(0.1);
    RHO_CU_20C * f / awg_area_m2(awg)
}

/// Copper mass per metre of one conductor, kg/m.
pub fn mass_per_m_kg(awg: u32) -> f64 {
    CU_DENSITY * awg_area_m2(awg)
}

/// Inputs of one run (two conductors, each `length_m` long, each carrying `current_a`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RunInput {
    pub awg: u32,
    pub length_m: f64,
    pub current_a: f64,
    pub ambient_c: f64,
    pub air_speed_ms: f64,
}

/// Results of one run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RunResult {
    /// Loop resistance at the final wire temperature, ohm.
    pub resistance_ohm: f64,
    /// `I * R`, V.
    pub voltage_drop_v: f64,
    /// `I^2 * R`, W (both conductors).
    pub power_loss_w: f64,
    /// Steady-state rise over ambient, K.
    pub temp_rise_k: f64,
    /// Ambient plus rise, C.
    pub wire_temp_c: f64,
    /// Copper mass of both conductors, kg.
    pub mass_kg: f64,
}

/// Evaluates a run; never returns NaN or infinity.
pub fn evaluate_run(i: &RunInput) -> RunResult {
    let amb = clean_temp(i.ambient_c, 20.0);
    let len = pos(i.length_m).min(1e6);
    let cur = if i.current_a.is_finite() {
        i.current_a.abs().min(1e9)
    } else {
        0.0
    };
    let v = pos(i.air_speed_ms).min(100.0);
    let d_o = awg_diameter_m(i.awg) + 2.0 * INSULATION_THICKNESS_M;
    let mut rise = 0.0_f64;
    if cur > 0.0 {
        for _ in 0..40 {
            let h = film_coefficient(d_o, amb, rise, v);
            let r = resistance_per_m(i.awg, amb + rise);
            let next = (cur * cur * r / (h * std::f64::consts::PI * d_o)).min(MAX_RISE_K);
            if (next - rise).abs() <= 1e-9 * next.max(1.0) {
                rise = next;
                break;
            }
            rise = 0.5 * (rise + next);
        }
    }
    let t_wire = amb + rise;
    let r_loop = 2.0 * len * resistance_per_m(i.awg, t_wire);
    RunResult {
        resistance_ohm: r_loop,
        voltage_drop_v: cur * r_loop,
        power_loss_w: cur * cur * r_loop,
        temp_rise_k: rise,
        wire_temp_c: t_wire,
        mass_kg: 2.0 * len * mass_per_m_kg(i.awg),
    }
}

/// Thinnest gauge whose drop is within `max_drop_pct` of `v_ref` and whose rise is within
/// `max_rise_k`; `None` if even the thickest supported gauge fails.
pub fn recommend_awg(
    length_m: f64,
    current_a: f64,
    ambient_c: f64,
    air_speed_ms: f64,
    v_ref: f64,
    max_drop_pct: f64,
    max_rise_k: f64,
) -> Option<u32> {
    let drop_limit = pos(v_ref).min(1e6) * pos(max_drop_pct).min(1e6) / 100.0;
    (AWG_MIN..=AWG_MAX).rev().find(|&awg| {
        let r = evaluate_run(&RunInput {
            awg,
            length_m,
            current_a,
            ambient_c,
            air_speed_ms,
        });
        r.voltage_drop_v <= drop_limit && r.temp_rise_k <= max_rise_k
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(awg: u32, len: f64, cur: f64, v: f64) -> RunResult {
        evaluate_run(&RunInput {
            awg,
            length_m: len,
            current_a: cur,
            ambient_c: 25.0,
            air_speed_ms: v,
        })
    }

    #[test]
    fn awg_table_values() {
        for (awg, want) in [
            (10, 3.277e-3),
            (12, 5.211e-3),
            (16, 13.17e-3),
            (20, 33.31e-3),
        ] {
            let r = resistance_per_m(awg, 20.0);
            assert!(((r - want) / want).abs() < 0.01, "AWG {awg}: {r}");
        }
        let m = mass_per_m_kg(12);
        assert!(((m - 0.0297) / 0.0297).abs() < 0.02, "{m}");
    }

    #[test]
    fn ohms_law_and_loss() {
        for (awg, l, c) in [(10, 0.3, 40.0), (18, 1.0, 7.5), (14, 0.05, 90.0)] {
            let r = run(awg, l, c, 0.0);
            assert!((r.voltage_drop_v - c * r.resistance_ohm).abs() <= 1e-12 * r.voltage_drop_v);
            assert!((r.power_loss_w - c * c * r.resistance_ohm).abs() <= 1e-12 * r.power_loss_w);
        }
    }

    #[test]
    fn thicker_wire_lowers_drop() {
        for awg in AWG_MIN..AWG_MAX {
            let thick = run(awg, 0.3, 30.0, 0.0);
            let thin = run(awg + 1, 0.3, 30.0, 0.0);
            assert!(thin.resistance_ohm > thick.resistance_ohm);
            assert!(thin.voltage_drop_v > thick.voltage_drop_v);
        }
    }

    #[test]
    fn airflow_cools_and_zero_current_is_ambient() {
        let a = run(14, 0.5, 40.0, 0.0).temp_rise_k;
        let b = run(14, 0.5, 40.0, 5.0).temp_rise_k;
        let c = run(14, 0.5, 40.0, 15.0).temp_rise_k;
        assert!(a > b && b > c);
        let z = run(14, 0.5, 0.0, 0.0);
        assert_eq!(z.temp_rise_k, 0.0);
        assert_eq!(z.wire_temp_c, 25.0);
        assert_eq!(z.voltage_drop_v, 0.0);
        assert_eq!(z.power_loss_w, 0.0);
    }

    #[test]
    fn recommendation_meets_limits() {
        for l in [0.1, 0.5, 2.0] {
            for c in [5.0, 30.0, 100.0] {
                let ok = |awg| {
                    let r = run(awg, l, c, 0.0);
                    r.voltage_drop_v <= 14.8 * 0.02 && r.temp_rise_k <= 30.0
                };
                match recommend_awg(l, c, 25.0, 0.0, 14.8, 2.0, 30.0) {
                    Some(a) => {
                        assert!(ok(a));
                        if a < AWG_MAX {
                            assert!(!ok(a + 1));
                        }
                    }
                    None => assert!(!ok(AWG_MIN)),
                }
            }
        }
        assert_eq!(
            recommend_awg(0.1, 0.0, 25.0, 0.0, 14.8, 2.0, 30.0),
            Some(AWG_MAX)
        );
    }

    #[test]
    fn cylinder_convection_scales_with_diameter_and_airflow() {
        let d_12 = awg_diameter_m(12) + 2.0 * INSULATION_THICKNESS_M;
        let d_20 = awg_diameter_m(20) + 2.0 * INSULATION_THICKNESS_M;
        let h_still_12 = film_coefficient(d_12, 25.0, 30.0, 0.0);
        let h_still_20 = film_coefficient(d_20, 25.0, 30.0, 0.0);
        assert!(
            (15.0..=35.0).contains(&h_still_12),
            "still h_12={h_still_12}"
        );
        assert!(
            (18.0..=40.0).contains(&h_still_20),
            "still h_20={h_still_20}"
        );
        assert!(h_still_20 > h_still_12);
        let h_flow_12 = film_coefficient(d_12, 25.0, 30.0, 5.0);
        assert!(
            (100.0..=220.0).contains(&h_flow_12),
            "5 m/s h_12={h_flow_12}"
        );
    }

    #[test]
    fn no_nan_for_hostile_inputs() {
        for cur in [-5.0, 0.0, 1e12, f64::NAN, f64::INFINITY] {
            for len in [-1.0, 0.0, 1e9, f64::NAN] {
                for amb in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1e12, 1e308] {
                    for awg in [0, 100] {
                        let r = evaluate_run(&RunInput {
                            awg,
                            length_m: len,
                            current_a: cur,
                            ambient_c: amb,
                            air_speed_ms: f64::NAN,
                        });
                        let r2 = evaluate_run(&RunInput {
                            awg,
                            length_m: len,
                            current_a: cur,
                            ambient_c: amb,
                            air_speed_ms: -3.0,
                        });
                        for res in [r, r2] {
                            for x in [
                                res.resistance_ohm,
                                res.voltage_drop_v,
                                res.power_loss_w,
                                res.temp_rise_k,
                                res.wire_temp_c,
                                res.mass_kg,
                            ] {
                                assert!(x.is_finite(), "cur={cur} len={len} amb={amb}: {x}");
                            }
                        }
                    }
                }
            }
        }
    }
}
