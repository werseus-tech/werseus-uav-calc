// SPDX-License-Identifier: AGPL-3.0-or-later
//! Flight performance loss calculations for added payload power and mass.

use super::latency::sanitize;
use crate::calc::atmosphere::G;
use crate::calc::performance::{PerfIn, at_speed};

#[derive(Clone, Debug)]
pub enum Baseline {
    /// Fixed-wing / VTOL cruise: PerfIn of the unmodified aircraft and cruise speed in m/s.
    Wing { perf: PerfIn, v_cruise: f64 },
    /// Multirotor (hover or cruise): usable battery energy Wh, total electrical power W of the unmodified aircraft, and its take-off mass kg.
    Rotor {
        e_usable_wh: f64,
        p_total_w: f64,
        mass_kg: f64,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FlightTimeLoss {
    pub t_base_min: f64,
    pub t_with_min: f64,
    pub lost_min: f64,
    pub lost_pct: f64,
}

pub fn flight_time_loss(base: &Baseline, extra_power_w: f64, extra_mass_kg: f64) -> FlightTimeLoss {
    // Sanitize inputs to valid ranges
    let ep = sanitize(extra_power_w, 0.0, 1.0e5);
    let em = sanitize(extra_mass_kg, 0.0, 1.0e3);

    match base {
        Baseline::Wing { perf, v_cruise } => {
            // Check for invalid wing loading or weight
            if !perf.ws.is_finite() || !perf.w.is_finite() || perf.ws <= 0.0 || perf.w <= 0.0 {
                return FlightTimeLoss::default();
            }

            // Calculate added weight in Newtons
            let dw = em * G;

            // Calculate wing area S from original weight and wing loading
            let s = perf.w / perf.ws;

            // Create new PerfIn with added weight and power
            let mut new_perf = perf.clone();
            new_perf.w += dw;
            new_perf.ws = (perf.w + dw) / s;
            new_perf.p_hotel += ep;

            // Calculate endurance times
            let t_base = at_speed(*v_cruise, perf).endurance_min();
            let t_with = at_speed(*v_cruise, &new_perf).endurance_min();

            // Ensure results are finite
            let t_base = if t_base.is_finite() && t_base >= 0.0 {
                t_base
            } else {
                0.0
            };
            let t_with = if t_with.is_finite() && t_with >= 0.0 {
                t_with
            } else {
                0.0
            };

            let lost_min = t_base - t_with;
            let lost_pct = if t_base > 0.0 {
                100.0 * lost_min / t_base
            } else {
                0.0
            };

            FlightTimeLoss {
                t_base_min: t_base,
                t_with_min: t_with,
                lost_min,
                lost_pct,
            }
        }
        Baseline::Rotor {
            e_usable_wh,
            p_total_w,
            mass_kg,
        } => {
            // Check for invalid inputs
            if !e_usable_wh.is_finite()
                || !p_total_w.is_finite()
                || !mass_kg.is_finite()
                || *e_usable_wh <= 0.0
                || *p_total_w <= 0.0
                || *mass_kg <= 0.0
            {
                return FlightTimeLoss::default();
            }

            // Hover-style power scales with mass^1.5
            let mass_ratio = (mass_kg + em) / mass_kg;
            let p_new = p_total_w * mass_ratio.powf(1.5) + ep;

            // Calculate endurance times in minutes
            let t_base = e_usable_wh / p_total_w * 60.0;
            let t_with = e_usable_wh / p_new * 60.0;

            // Ensure results are finite
            let t_base = if t_base.is_finite() && t_base >= 0.0 {
                t_base
            } else {
                0.0
            };
            let t_with = if t_with.is_finite() && t_with >= 0.0 {
                t_with
            } else {
                0.0
            };

            let lost_min = t_base - t_with;
            let lost_pct = if t_base > 0.0 {
                100.0 * lost_min / t_base
            } else {
                0.0
            };

            FlightTimeLoss {
                t_base_min: t_base,
                t_with_min: t_with,
                lost_min,
                lost_pct,
            }
        }
    }
}

pub fn payload_power_mass(
    computer_load_w: f64,
    computer_mass_g: f64,
    sensors: &[(f64, f64)],
) -> (f64, f64) {
    let mut total_power_w = sanitize(computer_load_w, 0.0, 1.0e5);
    let mut total_mass_g = sanitize(computer_mass_g, 0.0, 1.0e6);

    for (w, g) in sensors {
        total_power_w += sanitize(*w, 0.0, 1.0e5);
        total_mass_g += sanitize(*g, 0.0, 1.0e6);
    }

    (total_power_w, total_mass_g / 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_perf_in() -> PerfIn {
        PerfIn {
            e_pack_wh: 121.0,
            dod: 0.8,
            eta_esc: 0.95,
            eta_mot: 0.85,
            eta_prop: 0.74,
            p_hotel: 14.0,
            cd0: 0.02678,
            k: 0.04878,
            rho: 1.16727,
            ws: 61.3,
            w: 19.6133,
        }
    }

    #[test]
    fn zero_payload_zero_loss() {
        let perf = make_perf_in();
        let wing = Baseline::Wing {
            perf: perf.clone(),
            v_cruise: 18.0,
        };
        let rotor = Baseline::Rotor {
            e_usable_wh: 100.0,
            p_total_w: 200.0,
            mass_kg: 2.0,
        };

        let res_wing = flight_time_loss(&wing, 0.0, 0.0);
        assert!((res_wing.lost_min - 0.0).abs() < 1e-9);
        assert!((res_wing.lost_pct - 0.0).abs() < 1e-9);

        let res_rotor = flight_time_loss(&rotor, 0.0, 0.0);
        assert!((res_rotor.lost_min - 0.0).abs() < 1e-9);
        assert!((res_rotor.lost_pct - 0.0).abs() < 1e-9);
    }

    #[test]
    fn monotonic_power() {
        let perf = make_perf_in();
        let wing = Baseline::Wing {
            perf: perf.clone(),
            v_cruise: 18.0,
        };
        let rotor = Baseline::Rotor {
            e_usable_wh: 100.0,
            p_total_w: 200.0,
            mass_kg: 2.0,
        };

        let powers = [0.0, 2.0, 5.0, 10.0, 20.0];

        let mut prev_wing = 0.0;
        for p in powers {
            let res = flight_time_loss(&wing, p, 0.0);
            assert!(res.lost_min > prev_wing || p == 0.0);
            prev_wing = res.lost_min;
        }

        let mut prev_rotor = 0.0;
        for p in powers {
            let res = flight_time_loss(&rotor, p, 0.0);
            assert!(res.lost_min > prev_rotor || p == 0.0);
            prev_rotor = res.lost_min;
        }
    }

    #[test]
    fn monotonic_mass() {
        let perf = make_perf_in();
        let wing = Baseline::Wing {
            perf: perf.clone(),
            v_cruise: 18.0,
        };
        let rotor = Baseline::Rotor {
            e_usable_wh: 100.0,
            p_total_w: 200.0,
            mass_kg: 2.0,
        };

        let masses = [0.0, 0.1, 0.3, 0.6];

        let mut prev_wing = 0.0;
        for m in masses {
            let res = flight_time_loss(&wing, 0.0, m);
            assert!(res.lost_min > prev_wing || m == 0.0);
            prev_wing = res.lost_min;
        }

        let mut prev_rotor = 0.0;
        for m in masses {
            let res = flight_time_loss(&rotor, 0.0, m);
            assert!(res.lost_min > prev_rotor || m == 0.0);
            prev_rotor = res.lost_min;
        }
    }

    #[test]
    fn rotor_hand_value() {
        let rotor = Baseline::Rotor {
            e_usable_wh: 100.0,
            p_total_w: 200.0,
            mass_kg: 2.0,
        };
        let res = flight_time_loss(&rotor, 20.0, 0.0);

        assert!((res.t_base_min - 30.0).abs() < 1e-9);
        let expected_t_with = 100.0 / 220.0 * 60.0;
        assert!((res.t_with_min - expected_t_with).abs() < 1e-9);
        let expected_lost_pct = 100.0 * (30.0 - expected_t_with) / 30.0;
        assert!((res.lost_pct - expected_lost_pct).abs() < 1e-6);
    }

    #[test]
    fn wing_value_matches_at_speed() {
        let perf = make_perf_in();
        let wing = Baseline::Wing {
            perf: perf.clone(),
            v_cruise: 18.0,
        };
        let res = flight_time_loss(&wing, 10.0, 0.0);

        let mut new_perf = perf.clone();
        new_perf.p_hotel += 10.0;
        let expected_t_with = at_speed(18.0, &new_perf).endurance_min();

        assert!((res.t_with_min - expected_t_with).abs() / expected_t_with < 1e-9);
    }

    #[test]
    fn no_nan() {
        let perf = make_perf_in();
        let wing = Baseline::Wing {
            perf: perf.clone(),
            v_cruise: 18.0,
        };
        let rotor = Baseline::Rotor {
            e_usable_wh: 100.0,
            p_total_w: 200.0,
            mass_kg: 2.0,
        };

        let bad_values = [f64::NAN, f64::INFINITY, -1.0, f64::MAX];

        for v in bad_values {
            let res_wing = flight_time_loss(&wing, v, v);
            assert!(res_wing.t_base_min.is_finite());
            assert!(res_wing.t_with_min.is_finite());
            assert!(res_wing.lost_min.is_finite());
            assert!(res_wing.lost_pct.is_finite());

            let res_rotor = flight_time_loss(&rotor, v, v);
            assert!(res_rotor.t_base_min.is_finite());
            assert!(res_rotor.t_with_min.is_finite());
            assert!(res_rotor.lost_min.is_finite());
            assert!(res_rotor.lost_pct.is_finite());
        }
    }

    #[test]
    fn payload_sums() {
        let sensors = vec![(1.5, 30.0), (2.0, 25.0)];
        let (p, m) = payload_power_mass(7.5, 46.0, &sensors);
        assert!((p - 11.0).abs() < 1e-9);
        assert!((m - 0.101).abs() < 1e-9);
    }
}
