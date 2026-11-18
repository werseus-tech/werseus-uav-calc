// SPDX-License-Identifier: AGPL-3.0-or-later
//! Lumped heating of electronics: loss models, convection film coefficient, steady
//! temperature, first-order warm-up and time to a temperature limit.
//!
//! Convection uses the Juerges / Watmuff flat-plate correlation (includes radiation, valid
//! roughly 0..30 m/s): `h = 5.7 + 3.8 v` for `v <= 5 m/s`, `h = 7.3 v^0.78` above (the branches
//! differ by about 4 % at 5 m/s). Still air in an enclosure uses `h = 3 W/m2K`.

/// Cooling condition.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cooling {
    EnclosedStill,
    OpenStill,
    /// Air speed over the surface, m/s.
    Airflow(f64),
}

/// Film coefficient in a closed compartment, W/m2K.
pub const H_ENCLOSED_W_M2K: f64 = 3.0;

/// Film coefficient, W/m2K.
pub fn film_coefficient(c: Cooling) -> f64 {
    match c {
        Cooling::EnclosedStill => H_ENCLOSED_W_M2K,
        Cooling::OpenStill => 5.7,
        Cooling::Airflow(v) => {
            let v = if v.is_finite() {
                v.clamp(0.0, 100.0)
            } else {
                0.0
            };
            if v <= 5.0 {
                5.7 + 3.8 * v
            } else {
                7.3 * v.powf(0.78)
            }
        }
    }
}

/// Thermal description of a part.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThermalSpec {
    /// Effective convective surface, m2.
    pub area_m2: f64,
    /// Hot spot to surface resistance in series with the air film, K/W.
    pub r_internal_k_per_w: f64,
    /// Mass times specific heat, J/K.
    pub heat_capacity_j_per_k: f64,
}

/// Minimum clamped ambient/limit temperature, C.
pub const MIN_TEMP_C: f64 = -100.0;
/// Maximum clamped ambient/limit temperature, C.
pub const MAX_TEMP_C: f64 = 500.0;

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

/// Total thermal resistance `r_internal + 1/(h A)`, K/W (finite, positive).
pub fn thermal_resistance(spec: &ThermalSpec, cooling: Cooling) -> f64 {
    let area = if spec.area_m2.is_finite() {
        spec.area_m2.max(1e-6)
    } else {
        1e-6
    };
    pos(spec.r_internal_k_per_w).min(1e6) + 1.0 / (film_coefficient(cooling) * area)
}

/// Verdict against the temperature limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeatStatus {
    Pass,
    Warn,
    Fail,
}

/// Result of [`evaluate_heat`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeatResult {
    pub loss_w: f64,
    pub r_th_k_per_w: f64,
    /// Warm-up time constant `r_th * C`, s.
    pub tau_s: f64,
    pub temp_rise_k: f64,
    pub temp_steady_c: f64,
    /// Time from ambient to the limit; `None` if the steady temperature stays below it.
    pub time_to_limit_s: Option<f64>,
    /// `limit - steady`, K (negative = over the limit).
    pub margin_k: f64,
    /// Fail if over the limit, Warn if under 10 % of the allowed rise is left.
    pub status: HeatStatus,
}

/// Steady temperature, warm-up time and verdict for a loss.
pub fn evaluate_heat(
    loss_w: f64,
    spec: &ThermalSpec,
    cooling: Cooling,
    ambient_c: f64,
    limit_c: f64,
) -> HeatResult {
    let loss = pos(loss_w).min(1e9);
    let amb = clean_temp(ambient_c, 20.0);
    let lim = clean_temp(limit_c, 100.0);
    let r_th = thermal_resistance(spec, cooling);
    let tau = r_th * pos(spec.heat_capacity_j_per_k).min(1e9);
    let rise = loss * r_th;
    let steady = amb + rise;
    let time = if amb >= lim {
        Some(0.0)
    } else if rise <= 0.0 || steady <= lim {
        None
    } else if tau <= 0.0 {
        Some(0.0)
    } else {
        let arg = ((steady - lim) / rise).clamp(f64::MIN_POSITIVE, 1.0);
        Some(-tau * arg.ln())
    };
    let margin = lim - steady;
    let status = if amb >= lim || margin <= 0.0 {
        HeatStatus::Fail
    } else if margin < 0.1 * (lim - amb) {
        HeatStatus::Warn
    } else {
        HeatStatus::Pass
    };
    HeatResult {
        loss_w: loss,
        r_th_k_per_w: r_th,
        tau_s: tau,
        temp_rise_k: rise,
        temp_steady_c: steady,
        time_to_limit_s: time,
        margin_k: margin,
        status,
    }
}

/// First-order warm-up from ambient at time `t_s`.
pub fn temperature_at(result: &HeatResult, ambient_c: f64, t_s: f64) -> f64 {
    let amb = clean_temp(ambient_c, 20.0);
    let rise = if result.temp_rise_k.is_finite() {
        result.temp_rise_k.clamp(0.0, 1e6)
    } else {
        0.0
    };
    if !t_s.is_finite() || t_s <= 0.0 {
        return amb;
    }
    if !result.tau_s.is_finite() || result.tau_s <= 0.0 {
        return amb + rise;
    }
    amb + rise * (1.0 - (-t_s / result.tau_s).exp())
}

/// ESC loss from efficiency: `V I (1 - eta)`, W.
pub fn esc_loss_efficiency(v_in: f64, i_a: f64, eta: f64) -> f64 {
    let eta = if eta.is_finite() {
        eta.clamp(0.0, 1.0)
    } else {
        1.0
    };
    pos(v_in).min(1e6) * pos(i_a).min(1e9) * (1.0 - eta)
}

/// ESC loss from conduction and switching: two MOSFETs in series (`2 R_ds_on`) carrying the
/// PWM-scaled phase current `I_phase = I_dc / duty`, plus switching loss on `I_phase`, W.
pub fn esc_loss_rds_on(v_in: f64, i_dc_a: f64, duty: f64, r_on_ohm: f64, sw_frac: f64) -> f64 {
    let i_dc = pos(i_dc_a).min(1e9);
    if i_dc == 0.0 {
        return 0.0;
    }
    let d = if duty.is_finite() {
        duty.clamp(0.05, 1.0)
    } else {
        1.0
    };
    let i_phase = i_dc / d;
    i_phase * i_phase * (2.0 * pos(r_on_ohm).min(1e6))
        + pos(sw_frac).min(1.0) * pos(v_in).min(1e6) * i_phase
}

/// Linear regulator loss `(Vin - Vout) I`, W.
pub fn bec_loss_linear(v_in: f64, v_out: f64, i_load_a: f64) -> f64 {
    pos(pos(v_in).min(1e6) - pos(v_out).min(1e6)) * pos(i_load_a).min(1e9)
}

/// Switching regulator loss `(1 - eta) P_in = P_out (1/eta - 1)`, W (eta clamped 0.3..1).
pub fn bec_loss_switching(v_out: f64, i_load_a: f64, eta: f64) -> f64 {
    let eta = if eta.is_finite() {
        eta.clamp(0.3, 1.0)
    } else {
        1.0
    };
    pos(v_out).min(1e6) * pos(i_load_a).min(1e9) * (1.0 / eta - 1.0)
}

/// Video transmitter heat: input power minus RF output, W.
pub fn vtx_loss(p_in_w: f64, p_rf_w: f64) -> f64 {
    pos(pos(p_in_w).min(1e9) - pos(p_rf_w).min(1e9))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: ThermalSpec = ThermalSpec {
        area_m2: 0.003,
        r_internal_k_per_w: 1.0,
        heat_capacity_j_per_k: 20.0,
    };

    #[test]
    fn film_coefficient_order() {
        assert_eq!(
            film_coefficient(Cooling::Airflow(0.0)),
            film_coefficient(Cooling::OpenStill)
        );
        let mut last = 0.0;
        for v in [0.0, 1.0, 2.0, 5.0, 10.0, 20.0] {
            let h = film_coefficient(Cooling::Airflow(v));
            assert!(h > last);
            last = h;
        }
        assert!(film_coefficient(Cooling::EnclosedStill) < film_coefficient(Cooling::OpenStill));
    }

    #[test]
    fn more_airflow_is_cooler() {
        let t = |c| evaluate_heat(5.0, &SPEC, c, 25.0, 100.0).temp_steady_c;
        assert!(t(Cooling::Airflow(0.0)) > t(Cooling::Airflow(3.0)));
        assert!(t(Cooling::Airflow(3.0)) > t(Cooling::Airflow(10.0)));
        assert!(t(Cooling::EnclosedStill) > t(Cooling::OpenStill));
    }

    #[test]
    fn zero_loss_is_ambient() {
        let r = evaluate_heat(0.0, &SPEC, Cooling::OpenStill, 25.0, 100.0);
        assert_eq!(r.temp_steady_c, 25.0);
        assert_eq!(r.temp_rise_k, 0.0);
        assert_eq!(r.time_to_limit_s, None);
        assert_eq!(r.status, HeatStatus::Pass);
    }

    #[test]
    fn transient_and_time_to_limit() {
        let r = evaluate_heat(5.0, &SPEC, Cooling::OpenStill, 25.0, 100.0);
        assert!((r.temp_rise_k - 5.0 * r.r_th_k_per_w).abs() < 1e-12 * r.temp_rise_k);
        assert_eq!(temperature_at(&r, 25.0, 0.0), 25.0);
        let at_tau = temperature_at(&r, 25.0, r.tau_s);
        assert!((at_tau - (25.0 + 0.632 * r.temp_rise_k)).abs() < 2e-3 * r.temp_rise_k);
        assert!((temperature_at(&r, 25.0, 1e9) - r.temp_steady_c).abs() < 1e-6 * r.temp_rise_k);
        let hot = evaluate_heat(12.0, &SPEC, Cooling::OpenStill, 25.0, 100.0);
        let t = hot.time_to_limit_s.expect("reaches the limit");
        assert!((temperature_at(&hot, 25.0, t) - 100.0).abs() < 1e-6);
        let cool = evaluate_heat(1.0, &SPEC, Cooling::OpenStill, 25.0, 100.0);
        assert_eq!(cool.time_to_limit_s, None);
    }

    #[test]
    fn status_levels() {
        let r_th = thermal_resistance(&SPEC, Cooling::OpenStill);
        let at =
            |rise: f64| evaluate_heat(rise / r_th, &SPEC, Cooling::OpenStill, 25.0, 100.0).status;
        assert_eq!(at(80.0), HeatStatus::Fail);
        assert_eq!(at(70.0), HeatStatus::Warn);
        assert_eq!(at(40.0), HeatStatus::Pass);
    }

    #[test]
    fn edge_cases_ambient_at_limit_zero_tau_and_negative_ambient() {
        let at_limit = evaluate_heat(0.0, &SPEC, Cooling::OpenStill, 100.0, 100.0);
        assert_eq!(at_limit.status, HeatStatus::Fail);
        assert_eq!(at_limit.time_to_limit_s, Some(0.0));

        let zero_tau_spec = ThermalSpec {
            heat_capacity_j_per_k: 0.0,
            ..SPEC
        };
        let instant = evaluate_heat(10.0, &zero_tau_spec, Cooling::OpenStill, 25.0, 100.0);
        assert_eq!(instant.status, HeatStatus::Fail);
        assert_eq!(instant.time_to_limit_s, Some(0.0));

        let r_th = thermal_resistance(&SPEC, Cooling::OpenStill);
        let amb = -40.0_f64;
        let lim = 10.0_f64;
        // Steady just above limit so (lim - amb) / rise is close to 1.0 under negative ambient.
        let loss = ((lim - amb) + 1e-13) / r_th;
        let near = evaluate_heat(loss, &SPEC, Cooling::OpenStill, amb, lim);
        if let Some(t) = near.time_to_limit_s {
            assert!(t.is_finite() && t >= 0.0, "time_to_limit_s={t}");
        }
        assert_eq!(near.status, HeatStatus::Fail);
    }

    #[test]
    fn loss_models() {
        assert!((esc_loss_efficiency(14.8, 20.0, 0.95) - 14.8).abs() < 1e-9);
        assert!((esc_loss_rds_on(14.8, 30.0, 1.0, 0.004, 0.005) - (7.2 + 2.22)).abs() < 1e-9);
        // At 50% duty cycle with 10 A DC bus current, phase current is 20 A through 2*R_on.
        assert!((esc_loss_rds_on(14.8, 10.0, 0.5, 0.004, 0.005) - (3.2 + 1.48)).abs() < 1e-9);
        assert_eq!(esc_loss_rds_on(14.8, 0.0, 0.5, 0.004, 0.005), 0.0);
        assert_eq!(bec_loss_linear(12.0, 5.0, 1.0), 7.0);
        assert_eq!(bec_loss_linear(4.0, 5.0, 1.0), 0.0);
        assert!((bec_loss_switching(5.0, 2.0, 0.9) - 10.0 * (1.0 / 0.9 - 1.0)).abs() < 1e-9);
        assert!((vtx_loss(3.0, 0.8) - 2.2).abs() < 1e-12);
        assert_eq!(vtx_loss(1.0, 2.0), 0.0);
    }

    #[test]
    fn no_nan() {
        let bad = [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            -5.0,
            1e12,
            1e308,
        ];
        for &x in &bad {
            let spec = ThermalSpec {
                area_m2: x,
                r_internal_k_per_w: x,
                heat_capacity_j_per_k: x,
            };
            for c in [Cooling::Airflow(x), Cooling::EnclosedStill] {
                let r = evaluate_heat(x, &spec, c, x, x);
                for v in [
                    r.r_th_k_per_w,
                    r.tau_s,
                    r.temp_rise_k,
                    r.temp_steady_c,
                    r.margin_k,
                ] {
                    assert!(v.is_finite());
                }
                if let Some(t) = r.time_to_limit_s {
                    assert!(t.is_finite());
                }
                assert!(temperature_at(&r, x, x).is_finite());
            }
            for v in [
                esc_loss_efficiency(x, x, x),
                esc_loss_rds_on(x, x, x, x, x),
                bec_loss_linear(x, x, x),
                bec_loss_switching(x, x, x),
                vtx_loss(x, x),
            ] {
                assert!(v.is_finite() && v >= 0.0);
            }
        }
    }
}
