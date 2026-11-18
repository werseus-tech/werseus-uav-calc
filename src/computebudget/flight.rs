// SPDX-License-Identifier: AGPL-3.0-or-later
//! Flight performance loss calculations for added payload power and mass.

use super::latency::sanitize;
use crate::calc::aero::v_stall;
use crate::calc::atmosphere::G;
use crate::calc::performance::{PerfIn, at_speed};

#[derive(Clone, Debug)]
pub enum Baseline {
    /// Fixed-wing / VTOL cruise: PerfIn of the unmodified aircraft, cruise speed (m/s),
    /// CL_max for stall check (<= 0 disables), installed motor electrical power W (<= 0 disables),
    /// and optional VTOL max hover thrust N (`Some` for VTOL Quadplane).
    Wing {
        perf: PerfIn,
        v_cruise: f64,
        cl_max: f64,
        p_max_elec: f64,
        vtol_t_max_n: Option<f64>,
    },
    /// Multirotor hover: usable battery energy Wh, propulsive electrical hover power W
    /// (scales with mass^1.5), constant avionics/hotel power W, take-off mass kg,
    /// and max total thrust N (<= 0 disables thrust limit check).
    Rotor {
        e_usable_wh: f64,
        p_prop_w: f64,
        p_hotel_w: f64,
        mass_kg: f64,
        t_max_n: f64,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FlightTimeLoss {
    pub t_base_min: f64,
    pub t_with_min: f64,
    pub lost_min: f64,
    pub lost_pct: f64,
    /// True if added weight pushes stall speed above `v_cruise`.
    pub stall_exceeded: bool,
    /// True if added weight/drag pushes cruise propulsion power above installed motor power.
    pub power_exceeded: bool,
    /// True if total takeoff weight exceeds maximum available rotor/VTOL thrust.
    pub thrust_exceeded: bool,
    /// True if baseline is a VTOL Quadplane (cruise model omits VTOL hover energy).
    pub vtol_hover_omitted: bool,
}

pub fn flight_time_loss(base: &Baseline, extra_power_w: f64, extra_mass_kg: f64) -> FlightTimeLoss {
    // Sanitize inputs to valid ranges
    let ep = sanitize(extra_power_w, 0.0, 1.0e5);
    let em = sanitize(extra_mass_kg, 0.0, 1.0e3);

    match base {
        Baseline::Wing {
            perf,
            v_cruise,
            cl_max,
            p_max_elec,
            vtol_t_max_n,
        } => {
            // Check for invalid baseline parameters on entry
            if !perf.ws.is_finite()
                || !perf.w.is_finite()
                || !perf.p_hotel.is_finite()
                || !perf.e_pack_wh.is_finite()
                || !perf.rho.is_finite()
                || !v_cruise.is_finite()
                || perf.ws <= 0.0
                || perf.w <= 0.0
                || perf.p_hotel < 0.0
                || perf.e_pack_wh <= 0.0
                || perf.rho <= 0.0
                || *v_cruise <= 0.0
            {
                return FlightTimeLoss::default();
            }

            // Compute baseline endurance; abort immediately if non-finite or non-positive
            let t_base = at_speed(*v_cruise, perf).endurance_min();
            if !t_base.is_finite() || t_base <= 0.0 {
                return FlightTimeLoss::default();
            }

            // Calculate added weight in Newtons and wing area S from original weight and wing loading
            let dw = em * G;
            let s = perf.w / perf.ws;

            // Create new PerfIn with added weight and power
            let mut new_perf = perf.clone();
            new_perf.w += dw;
            new_perf.ws = new_perf.w / s;
            new_perf.p_hotel += ep;

            let new_pt = at_speed(*v_cruise, &new_perf);

            let stall_exceeded = cl_max.is_finite() && *cl_max > 0.0 && {
                let v_st = v_stall(new_perf.ws, new_perf.rho, *cl_max);
                !v_st.is_finite() || v_st > *v_cruise
            };
            let power_exceeded = p_max_elec.is_finite()
                && *p_max_elec > 0.0
                && (!new_pt.p_prop_elec.is_finite() || new_pt.p_prop_elec > *p_max_elec);
            let vtol_hover_omitted = vtol_t_max_n.is_some();
            let thrust_exceeded = match vtol_t_max_n {
                Some(t_max) if t_max.is_finite() && *t_max > 0.0 => new_perf.w > *t_max,
                _ => false,
            };

            let t_with = if stall_exceeded || power_exceeded || thrust_exceeded {
                0.0
            } else {
                let raw = new_pt.endurance_min();
                if raw.is_finite() && raw > 0.0 {
                    raw.clamp(0.0, t_base)
                } else {
                    0.0
                }
            };

            let lost_min = t_base - t_with;
            let lost_pct = (lost_min / t_base) * 100.0;

            FlightTimeLoss {
                t_base_min: t_base,
                t_with_min: t_with,
                lost_min,
                lost_pct,
                stall_exceeded,
                power_exceeded,
                thrust_exceeded,
                vtol_hover_omitted,
            }
        }
        Baseline::Rotor {
            e_usable_wh,
            p_prop_w,
            p_hotel_w,
            mass_kg,
            t_max_n,
        } => {
            // Check for invalid inputs
            if !e_usable_wh.is_finite()
                || !p_prop_w.is_finite()
                || !p_hotel_w.is_finite()
                || !mass_kg.is_finite()
                || *e_usable_wh <= 0.0
                || *p_prop_w <= 0.0
                || *p_hotel_w < 0.0
                || *mass_kg <= 0.0
            {
                return FlightTimeLoss::default();
            }

            // Compute baseline endurance; abort immediately if non-finite or non-positive
            let p_base_total = p_prop_w + p_hotel_w;
            let t_base = e_usable_wh / p_base_total * 60.0;
            if !t_base.is_finite() || t_base <= 0.0 {
                return FlightTimeLoss::default();
            }

            let new_mass = mass_kg + em;
            let thrust_exceeded = t_max_n.is_finite() && *t_max_n > 0.0 && new_mass * G > *t_max_n;

            // Only propulsive hover power scales with mass^1.5; base hotel/avionics load stays constant
            let mass_ratio = new_mass / mass_kg;
            let p_new = p_prop_w * mass_ratio.powf(1.5) + p_hotel_w + ep;

            let t_with = if thrust_exceeded {
                0.0
            } else {
                let raw = e_usable_wh / p_new * 60.0;
                if raw.is_finite() && raw > 0.0 {
                    raw.clamp(0.0, t_base)
                } else {
                    0.0
                }
            };

            let lost_min = t_base - t_with;
            let lost_pct = (lost_min / t_base) * 100.0;

            FlightTimeLoss {
                t_base_min: t_base,
                t_with_min: t_with,
                lost_min,
                lost_pct,
                stall_exceeded: false,
                power_exceeded: false,
                thrust_exceeded,
                vtol_hover_omitted: false,
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

    fn make_wing(perf: PerfIn) -> Baseline {
        Baseline::Wing {
            perf,
            v_cruise: 18.0,
            cl_max: 1.2,
            p_max_elec: 240.0,
            vtol_t_max_n: None,
        }
    }

    fn make_rotor() -> Baseline {
        Baseline::Rotor {
            e_usable_wh: 100.0,
            p_prop_w: 180.0,
            p_hotel_w: 20.0,
            mass_kg: 2.0,
            t_max_n: 40.0,
        }
    }

    #[test]
    fn zero_payload_zero_loss() {
        let wing = make_wing(make_perf_in());
        let rotor = make_rotor();

        let res_wing = flight_time_loss(&wing, 0.0, 0.0);
        assert!((res_wing.lost_min - 0.0).abs() < 1e-9);
        assert!((res_wing.lost_pct - 0.0).abs() < 1e-9);

        let res_rotor = flight_time_loss(&rotor, 0.0, 0.0);
        assert!((res_rotor.lost_min - 0.0).abs() < 1e-9);
        assert!((res_rotor.lost_pct - 0.0).abs() < 1e-9);
    }

    #[test]
    fn monotonic_power() {
        let wing = make_wing(make_perf_in());
        let rotor = make_rotor();

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
        let wing = make_wing(make_perf_in());
        let rotor = make_rotor();

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
        let rotor = make_rotor();

        let res_zero_mass = flight_time_loss(&rotor, 20.0, 0.0);
        assert!((res_zero_mass.t_base_min - 30.0).abs() < 1e-9);
        let expected_t_with_0 = 100.0 / 220.0 * 60.0;
        assert!((res_zero_mass.t_with_min - expected_t_with_0).abs() < 1e-9);
        let expected_lost_pct_0 = 100.0 * (30.0 - expected_t_with_0) / 30.0;
        assert!((res_zero_mass.lost_pct - expected_lost_pct_0).abs() < 1e-6);

        // Non-zero added mass: only propulsive power (180 W) scales with mass^1.5; hotel (20 W) stays constant
        let res_with_mass = flight_time_loss(&rotor, 20.0, 0.5);
        let p_expected = 180.0 * (2.5_f64 / 2.0).powf(1.5) + 20.0 + 20.0;
        let expected_t_with_m = 100.0 / p_expected * 60.0;
        assert!(!res_with_mass.thrust_exceeded);
        assert!((res_with_mass.t_base_min - 30.0).abs() < 1e-9);
        assert!((res_with_mass.t_with_min - expected_t_with_m).abs() < 1e-9);
        assert!((res_with_mass.lost_min - (30.0 - expected_t_with_m)).abs() < 1e-9);
    }

    #[test]
    fn wing_value_matches_at_speed() {
        let perf = make_perf_in();
        let wing = make_wing(perf.clone());

        let res_zero_mass = flight_time_loss(&wing, 10.0, 0.0);
        let mut new_perf_0 = perf.clone();
        new_perf_0.p_hotel += 10.0;
        let expected_0 = at_speed(18.0, &new_perf_0).endurance_min();
        assert!((res_zero_mass.t_with_min - expected_0).abs() / expected_0 < 1e-9);

        // Non-zero added mass: weight and wing loading both increase
        let res_with_mass = flight_time_loss(&wing, 10.0, 0.3);
        let s = perf.w / perf.ws;
        let mut new_perf_m = perf.clone();
        new_perf_m.w += 0.3 * G;
        new_perf_m.ws = new_perf_m.w / s;
        new_perf_m.p_hotel += 10.0;
        let expected_m = at_speed(18.0, &new_perf_m).endurance_min();
        assert!(!res_with_mass.stall_exceeded);
        assert!(!res_with_mass.power_exceeded);
        assert!((res_with_mass.t_with_min - expected_m).abs() / expected_m < 1e-9);
    }

    #[test]
    fn rotor_thrust_limit_invalidates_hover() {
        let rotor = Baseline::Rotor {
            e_usable_wh: 100.0,
            p_prop_w: 180.0,
            p_hotel_w: 20.0,
            mass_kg: 2.0,
            t_max_n: 25.0,
        };
        // 2.0 + 1.0 = 3.0 kg -> ~29.42 N > 25.0 N
        let res = flight_time_loss(&rotor, 10.0, 1.0);
        assert!(res.thrust_exceeded);
        assert_eq!(res.t_with_min, 0.0);
        assert!((res.lost_min - res.t_base_min).abs() < 1e-9);
        assert!((res.lost_pct - 100.0).abs() < 1e-9);
    }

    #[test]
    fn wing_stall_power_and_vtol_limits() {
        let perf = make_perf_in();
        // Low CL_max or heavy payload causes stall at v_cruise = 18 m/s
        let wing_stall = Baseline::Wing {
            perf: perf.clone(),
            v_cruise: 18.0,
            cl_max: 0.40,
            p_max_elec: 500.0,
            vtol_t_max_n: None,
        };
        let res_stall = flight_time_loss(&wing_stall, 0.0, 1.0);
        assert!(res_stall.stall_exceeded);
        assert_eq!(res_stall.t_with_min, 0.0);

        // Tight motor power limit exceeded when adding mass
        let wing_power = Baseline::Wing {
            perf: perf.clone(),
            v_cruise: 18.0,
            cl_max: 1.5,
            p_max_elec: 65.0,
            vtol_t_max_n: None,
        };
        let res_power = flight_time_loss(&wing_power, 0.0, 1.0);
        assert!(res_power.power_exceeded);
        assert_eq!(res_power.t_with_min, 0.0);

        // VTOL flags omitted hover energy and checks hover thrust
        let vtol = Baseline::Wing {
            perf,
            v_cruise: 18.0,
            cl_max: 1.5,
            p_max_elec: 500.0,
            vtol_t_max_n: Some(25.0),
        };
        let res_vtol_ok = flight_time_loss(&vtol, 5.0, 0.2);
        assert!(res_vtol_ok.vtol_hover_omitted);
        assert!(!res_vtol_ok.thrust_exceeded);
        let res_vtol_over = flight_time_loss(&vtol, 5.0, 1.0);
        assert!(res_vtol_over.vtol_hover_omitted);
        assert!(res_vtol_over.thrust_exceeded);
        assert_eq!(res_vtol_over.t_with_min, 0.0);
    }

    #[test]
    fn overflow_t_base_returns_default() {
        let rotor = Baseline::Rotor {
            e_usable_wh: 1.0e308,
            p_prop_w: 1.0e-10,
            p_hotel_w: 0.0,
            mass_kg: 2.0,
            t_max_n: 0.0,
        };
        let res = flight_time_loss(&rotor, 10.0, 0.0);
        assert_eq!(res, FlightTimeLoss::default());
        assert!(res.lost_min >= 0.0);
    }

    #[test]
    fn no_nan() {
        let perf = make_perf_in();
        let wing = make_wing(perf.clone());
        let rotor = make_rotor();

        let bad_values = [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            0.0,
            -1.0,
            f64::MAX,
        ];

        let check_finite = |res: FlightTimeLoss| {
            assert!(res.t_base_min.is_finite() && res.t_base_min >= 0.0);
            assert!(res.t_with_min.is_finite() && res.t_with_min >= 0.0);
            assert!(res.lost_min.is_finite() && res.lost_min >= 0.0);
            assert!(res.lost_pct.is_finite() && res.lost_pct >= 0.0);
        };

        for v in bad_values {
            check_finite(flight_time_loss(&wing, v, v));
            check_finite(flight_time_loss(&rotor, v, v));

            // Inject bad values into Baseline::Wing fields
            let mut bad_perf = perf.clone();
            bad_perf.p_hotel = v;
            check_finite(flight_time_loss(&make_wing(bad_perf.clone()), 5.0, 0.1));
            bad_perf.p_hotel = perf.p_hotel;
            bad_perf.w = v;
            check_finite(flight_time_loss(&make_wing(bad_perf.clone()), 5.0, 0.1));
            bad_perf.w = perf.w;
            bad_perf.ws = v;
            check_finite(flight_time_loss(&make_wing(bad_perf), 5.0, 0.1));
            check_finite(flight_time_loss(
                &Baseline::Wing {
                    perf: perf.clone(),
                    v_cruise: v,
                    cl_max: v,
                    p_max_elec: v,
                    vtol_t_max_n: Some(v),
                },
                5.0,
                0.1,
            ));

            // Inject bad values into Baseline::Rotor fields
            check_finite(flight_time_loss(
                &Baseline::Rotor {
                    e_usable_wh: v,
                    p_prop_w: 180.0,
                    p_hotel_w: 20.0,
                    mass_kg: 2.0,
                    t_max_n: 40.0,
                },
                5.0,
                0.1,
            ));
            check_finite(flight_time_loss(
                &Baseline::Rotor {
                    e_usable_wh: 100.0,
                    p_prop_w: v,
                    p_hotel_w: v,
                    mass_kg: v,
                    t_max_n: v,
                },
                5.0,
                0.1,
            ));
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
