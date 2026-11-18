// SPDX-License-Identifier: AGPL-3.0-or-later
//! Görev bacakları ve bacak bacak enerji simülasyonu.
use super::env::{Env, battery_derate};
use super::vehicle::{Kind, Vehicle};
use crate::calc::atmosphere::G;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegKind {
    /// Kalkış / tırmanma (yükseklik, tırmanma hızı).
    Climb,
    /// Seyir (mesafe, hava hızı, rota yönü).
    Cruise,
    /// Askı (çok rotorlu/VTOL) veya bekleme dairesi (sabit kanat), süre.
    Loiter,
    /// Alçalma (yükseklik, alçalma hızı).
    Descent,
    /// İniş (süre).
    Landing,
    /// Yük bırakma: kütle azalır; bacak süresi boyunca askı/bekleme gücü harcanır.
    Drop,
    /// VTOL geçiş (süre); VTOL dışında bekleme gücüyle hesaplanır.
    Transition,
}

impl LegKind {
    pub const ALL: [LegKind; 7] = [
        LegKind::Climb,
        LegKind::Cruise,
        LegKind::Loiter,
        LegKind::Descent,
        LegKind::Landing,
        LegKind::Drop,
        LegKind::Transition,
    ];
}

#[derive(Clone, Debug, PartialEq)]
pub struct Leg {
    pub kind: LegKind,
    /// Climb / Descent: yükseklik farkı [m].
    pub height_m: f64,
    /// Climb / Descent: düşey hız [m/s].
    pub rate_ms: f64,
    /// Cruise: mesafe [m].
    pub distance_m: f64,
    /// Cruise / Climb (sabit kanat): hava hızı [m/s].
    pub airspeed_ms: f64,
    /// Cruise: rota yönü [derece, kuzeyden].
    pub heading_deg: f64,
    /// Loiter / Landing / Drop / Transition: süre [s].
    pub duration_s: f64,
    /// Drop: bırakılan kütle [kg].
    pub drop_kg: f64,
}

impl Leg {
    pub fn new(kind: LegKind, v: &Vehicle) -> Leg {
        let mut l = Leg {
            kind,
            height_m: 0.0,
            rate_ms: 0.0,
            distance_m: 0.0,
            airspeed_ms: v.v_cruise,
            heading_deg: 0.0,
            duration_s: 0.0,
            drop_kg: 0.0,
        };
        match kind {
            LegKind::Climb => {
                l.height_m = 100.0;
                l.rate_ms = 2.5;
            }
            LegKind::Cruise => l.distance_m = 3000.0,
            LegKind::Loiter => l.duration_s = 60.0,
            LegKind::Descent => {
                l.height_m = 100.0;
                l.rate_ms = 2.0;
            }
            LegKind::Landing => l.duration_s = 20.0,
            LegKind::Drop => {
                l.duration_s = 10.0;
                l.drop_kg = v.payload_kg;
            }
            LegKind::Transition => l.duration_s = 15.0,
        }
        l
    }
}

/// Araç türüne göre başlangıç görev planı: gidiş, yük bırakma, dönüş.
pub fn default_legs(v: &Vehicle) -> Vec<Leg> {
    let mk = |k| Leg::new(k, v);
    let back = |mut l: Leg| {
        l.heading_deg = 180.0;
        l
    };
    match v.kind {
        Kind::Multirotor => vec![
            Leg {
                height_m: 50.0,
                rate_ms: 2.0,
                ..mk(LegKind::Climb)
            },
            Leg {
                distance_m: 1500.0,
                ..mk(LegKind::Cruise)
            },
            Leg {
                duration_s: 10.0,
                ..mk(LegKind::Drop)
            },
            back(Leg {
                distance_m: 1500.0,
                ..mk(LegKind::Cruise)
            }),
            Leg {
                height_m: 50.0,
                rate_ms: 1.5,
                ..mk(LegKind::Descent)
            },
            mk(LegKind::Landing),
        ],
        Kind::FixedWing => vec![
            mk(LegKind::Climb),
            mk(LegKind::Cruise),
            Leg {
                duration_s: 10.0,
                ..mk(LegKind::Drop)
            },
            back(mk(LegKind::Cruise)),
            mk(LegKind::Descent),
            mk(LegKind::Landing),
        ],
        Kind::Vtol => vec![
            Leg {
                height_m: 30.0,
                rate_ms: 2.0,
                ..mk(LegKind::Climb)
            },
            mk(LegKind::Transition),
            mk(LegKind::Cruise),
            Leg {
                duration_s: 10.0,
                ..mk(LegKind::Drop)
            },
            back(mk(LegKind::Cruise)),
            mk(LegKind::Transition),
            Leg {
                height_m: 30.0,
                rate_ms: 2.0,
                ..mk(LegKind::Descent)
            },
            mk(LegKind::Landing),
        ],
    }
}

#[derive(Clone, Debug)]
pub struct LegResult {
    pub kind: LegKind,
    pub time_s: f64,
    pub airspeed: f64,
    pub ground_speed: f64,
    pub headwind: f64,
    pub crosswind: f64,
    pub power_w: f64,
    pub energy_wh: f64,
    pub rho: f64,
    /// Bacağın başındaki kütle [kg].
    pub mass_kg: f64,
    /// Bu bacağın sonunda toplam harcanan enerji [Wh] ve bütçenin kalan yüzdesi.
    pub cum_wh: f64,
    pub remaining_pct: f64,
    /// Rota uçulabilir mi (rüzgâr hava hızını aşmıyor, değerler geçerli).
    pub feasible: bool,
    /// Hava hızı bu yoğunlukta stall sınırının altında.
    pub stall_warn: bool,
}

#[derive(Clone, Debug, Default)]
pub struct MissionResult {
    pub legs: Vec<LegResult>,
    pub total_wh: f64,
    pub total_time_s: f64,
    /// Kullanılabilir bütçe [Wh] = paket × DoD × soğuk faktörü × (1 − ek rezerv).
    pub budget_wh: f64,
    pub cold_factor: f64,
    pub battery_temp_c: f64,
    pub margin_wh: f64,
    pub margin_pct: f64,
    pub pass: bool,
    pub all_feasible: bool,
}

fn fin(x: f64) -> f64 {
    if x.is_finite() { x.max(0.0) } else { 0.0 }
}

/// Görevi bacak bacak simüle eder. `extra_reserve` (0..1) proje DoD'sine EK rezerv oranıdır.
pub fn simulate(v: &Vehicle, env: &Env, legs: &[Leg], extra_reserve: f64) -> MissionResult {
    let derate = battery_derate(env.battery_temp_c());
    let reserve = if extra_reserve.is_finite() {
        extra_reserve.clamp(0.0, 0.9)
    } else {
        0.0
    };
    let budget = fin(v.pack_wh) * v.dod.clamp(0.0, 1.0) * derate.energy_factor * (1.0 - reserve);
    let mut out = MissionResult {
        budget_wh: budget,
        cold_factor: derate.energy_factor,
        battery_temp_c: env.battery_temp_c(),
        all_feasible: true,
        ..Default::default()
    };
    let eta = v.eta_vertical();
    let mut agl = 0.0_f64;
    let mut dropped = 0.0_f64;
    let mut cum = 0.0_f64;
    for leg in legs {
        let mass = v.mass_kg(dropped);
        let weight = mass * G;
        let mut agl_mid = agl;
        let mut agl_next = agl;
        let h = fin(leg.height_m);
        match leg.kind {
            LegKind::Climb => {
                agl_mid = agl + 0.5 * h;
                agl_next = agl + h;
            }
            LegKind::Descent => {
                agl_mid = (agl - 0.5 * h).max(0.0);
                agl_next = (agl - h).max(0.0);
            }
            LegKind::Landing => agl_next = 0.0,
            _ => {}
        }
        let rho = env.rho_at(env.alt_msl_m + agl_mid);
        let loiter_v = v.loiter_speed(rho);
        let airspeed_in = if fin(leg.airspeed_ms) > 0.0 {
            leg.airspeed_ms
        } else {
            v.v_cruise
        };
        // Bekleme/askı gücü: çok rotorlu ve VTOL askıda, sabit kanat bekleme hızında uçar.
        let hold_w = match v.hover_w(rho, dropped) {
            Some(p) => p,
            None => v.level_w(rho, dropped, loiter_v),
        };
        let mut feasible = true;
        let (mut time, mut power, mut va, mut gs, mut hw, mut cw) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let mut stall = false;
        match leg.kind {
            LegKind::Climb => {
                let vz = fin(leg.rate_ms).max(0.1);
                time = h / vz;
                let base = if v.has_hover() {
                    hold_w
                } else {
                    va = airspeed_in;
                    stall = v.below_stall(rho, dropped, va);
                    v.level_w(rho, dropped, va)
                };
                power = base + weight * vz / eta;
            }
            LegKind::Descent => {
                let vz = fin(leg.rate_ms).max(0.1);
                time = h / vz;
                let (base, floor) = if v.has_hover() {
                    (hold_w, 0.5 * hold_w)
                } else {
                    va = airspeed_in;
                    stall = v.below_stall(rho, dropped, va);
                    (v.level_w(rho, dropped, va), v.hotel_w())
                };
                power = (base - weight * vz / eta).max(floor);
            }
            LegKind::Cruise => {
                va = airspeed_in;
                stall = v.below_stall(rho, dropped, va);
                let (head, cross) = env.wind_components(leg.heading_deg);
                hw = head;
                cw = cross;
                let along2 = va * va - cross * cross;
                if along2 <= 0.0 {
                    feasible = false;
                } else {
                    gs = along2.sqrt() - head;
                    if gs < 0.5 {
                        feasible = false;
                    }
                }
                if feasible {
                    time = fin(leg.distance_m) / gs;
                    power = v.level_w(rho, dropped, va);
                } else {
                    gs = 0.0;
                }
            }
            LegKind::Loiter | LegKind::Drop | LegKind::Transition => {
                time = fin(leg.duration_s);
                power = hold_w;
                if !v.has_hover() {
                    va = loiter_v;
                }
            }
            LegKind::Landing => {
                time = fin(leg.duration_s);
                power = if v.has_hover() {
                    hold_w
                } else {
                    // Sabit kanat: yaklaşma/sönümleme, bekleme gücünün ~%60'ı (basitleştirme).
                    va = loiter_v;
                    0.6 * hold_w
                };
            }
        }
        let mut energy = power * time / 3600.0;
        if !(energy.is_finite() && power.is_finite() && time.is_finite()) {
            feasible = false;
            energy = 0.0;
            power = 0.0;
            time = 0.0;
        }
        cum += energy;
        out.all_feasible &= feasible;
        out.total_time_s += time;
        out.legs.push(LegResult {
            kind: leg.kind,
            time_s: time,
            airspeed: va,
            ground_speed: gs,
            headwind: hw,
            crosswind: cw,
            power_w: power,
            energy_wh: energy,
            rho,
            mass_kg: mass,
            cum_wh: cum,
            remaining_pct: if budget > 0.0 {
                (budget - cum) / budget * 100.0
            } else {
                -999.0
            },
            feasible,
            stall_warn: stall,
        });
        if leg.kind == LegKind::Drop {
            dropped += v.clamp_drop(dropped, fin(leg.drop_kg));
        }
        agl = agl_next;
    }
    out.total_wh = cum;
    out.margin_wh = budget - cum;
    out.margin_pct = if budget > 0.0 {
        out.margin_wh / budget * 100.0
    } else {
        -100.0
    };
    out.pass = out.all_feasible && budget > 0.0 && cum <= budget;
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Library, evaluate};
    use crate::missionenergy::battery_derate;
    use crate::missionenergy::env::{Env, TempMode};
    use crate::project::{DroneType, Project};

    fn veh(dt: DroneType) -> (Project, Vehicle) {
        let mut p = Project::default();
        p.set_drone_type(dt);
        let m = evaluate(&p, &Library::embedded());
        let v = Vehicle::from_model(&p, &m);
        (p, v)
    }

    fn cruise(dist: f64, v: &Vehicle, heading: f64) -> Leg {
        Leg {
            distance_m: dist,
            heading_deg: heading,
            ..Leg::new(LegKind::Cruise, v)
        }
    }

    #[test]
    fn hover_leg_matches_model_hand_check() {
        let (p, v) = veh(DroneType::Multirotor);
        let m = evaluate(&p, &Library::embedded());
        // ISA deniz seviyesi: model yoğunluğu proje irtifasında olduğundan yeniden hesaplanır.
        let env = Env::default();
        let leg = Leg {
            duration_s: 120.0,
            ..Leg::new(LegKind::Loiter, &v)
        };
        let r = simulate(&v, &env, &[leg], 0.0);
        let rho = env.rho_at(0.0);
        let want_w = v.hover_w(rho, 0.0).unwrap();
        assert!((r.legs[0].power_w - want_w).abs() < 1e-9);
        assert!((r.legs[0].energy_wh - want_w * 120.0 / 3600.0).abs() < 1e-9);
        // Proje kendi irtifasındaki yoğunlukla aynı modeli verir (aynı kod yolu).
        let mut e2 = Env::default();
        e2.alt_msl_m = p.h;
        let w2 = v.hover_w(e2.rho_at(p.h), 0.0).unwrap();
        assert!((w2 - m.mr.p_total_hover).abs() / w2 < 1e-6);
    }

    #[test]
    fn cruise_leg_matches_performance_model() {
        let (p, v) = veh(DroneType::FixedWingConventional);
        let m = evaluate(&p, &Library::embedded());
        let mut env = Env::default();
        env.alt_msl_m = p.h;
        let r = simulate(&v, &env, &[cruise(9000.0, &v, 0.0)], 0.0);
        let l = &r.legs[0];
        assert!((l.ground_speed - p.v_cr).abs() < 1e-9);
        assert!((l.time_s - 9000.0 / p.v_cr).abs() < 1e-9);
        assert!((l.power_w - m.perf_cr.p_total).abs() / l.power_w < 1e-6);
        assert!((l.energy_wh - m.perf_cr.p_total * l.time_s / 3600.0).abs() < 1e-3);
    }

    #[test]
    fn headwind_costs_tailwind_saves() {
        for dt in [DroneType::FixedWingConventional, DroneType::Multirotor] {
            let (_, v) = veh(dt);
            let leg = cruise(3000.0, &v, 0.0);
            let calm = simulate(&v, &Env::default(), std::slice::from_ref(&leg), 0.0).total_wh;
            let head = Env {
                wind_speed: 5.0,
                wind_from_deg: 0.0,
                ..Env::default()
            };
            let tail = Env {
                wind_from_deg: 180.0,
                ..head.clone()
            };
            let ew = simulate(&v, &head, std::slice::from_ref(&leg), 0.0).total_wh;
            let et = simulate(&v, &tail, &[leg], 0.0).total_wh;
            assert!(ew > calm && calm > et, "{dt:?}: {ew} {calm} {et}");
        }
    }

    #[test]
    fn crosswind_too_strong_is_infeasible() {
        let (_, v) = veh(DroneType::FixedWingConventional);
        let env = Env {
            wind_speed: v.v_cruise + 5.0,
            wind_from_deg: 90.0,
            ..Env::default()
        };
        let r = simulate(&v, &env, &[cruise(1000.0, &v, 0.0)], 0.0);
        assert!(!r.pass && !r.all_feasible);
        assert!(r.total_wh.is_finite());
    }

    #[test]
    fn density_altitude_raises_hover_power() {
        let (_, v) = veh(DroneType::Multirotor);
        let leg = Leg::new(LegKind::Loiter, &v);
        let lo = simulate(&v, &Env::default(), std::slice::from_ref(&leg), 0.0);
        let hi = Env {
            alt_msl_m: 2500.0,
            isa_offset_k: 20.0,
            ..Env::default()
        };
        let hi = simulate(&v, &hi, &[leg], 0.0);
        assert!(hi.legs[0].power_w > lo.legs[0].power_w * 1.1);
    }

    #[test]
    fn cold_reduces_budget() {
        let (_, v) = veh(DroneType::Multirotor);
        let warm = simulate(&v, &Env::default(), &[], 0.0);
        let cold = Env {
            temp_mode: TempMode::Absolute,
            temp_abs_c: -10.0,
            ..Env::default()
        };
        let cold = simulate(&v, &cold, &[], 0.0);
        assert!(cold.budget_wh < warm.budget_wh * 0.8);
        let f = battery_derate(Env::default().battery_temp_c()).energy_factor;
        assert!((warm.budget_wh - v.pack_wh * v.dod * f).abs() < 1e-9);
    }

    #[test]
    fn reserve_pass_fail_boundary() {
        let (_, v) = veh(DroneType::Multirotor);
        let env = Env::default();
        let budget = simulate(&v, &env, &[], 0.0).budget_wh;
        let hold = v.hover_w(env.rho_at(0.0), 0.0).unwrap();
        // Bütçenin yarısını harcayan askı süresi.
        let legs = vec![Leg {
            duration_s: 0.5 * budget / hold * 3600.0,
            ..Leg::new(LegKind::Loiter, &v)
        }];
        let e = simulate(&v, &env, &legs, 0.0).total_wh;
        assert!(e < budget);
        let edge = 1.0 - e / budget;
        assert!(simulate(&v, &env, &legs, edge - 0.01).pass);
        assert!(!simulate(&v, &env, &legs, edge + 0.01).pass);
        let r = simulate(&v, &env, &legs, 0.0);
        assert!(r.margin_wh > 0.0 && r.legs[0].remaining_pct < 100.0);
    }

    #[test]
    fn payload_drop_reduces_later_power() {
        for dt in [DroneType::Multirotor, DroneType::FixedWingConventional] {
            let (_, v) = veh(dt);
            assert!(v.payload_kg > 0.0);
            let legs = vec![
                Leg::new(LegKind::Loiter, &v),
                Leg::new(LegKind::Drop, &v),
                Leg::new(LegKind::Loiter, &v),
            ];
            let r = simulate(&v, &Env::default(), &legs, 0.0);
            assert!(r.legs[2].power_w < r.legs[0].power_w, "{dt:?}");
            assert!(r.legs[2].mass_kg < r.legs[0].mass_kg);
            // Bırakma bacağının kendisi bırakmadan önceki kütleyle hesaplanır.
            assert!((r.legs[1].power_w - r.legs[0].power_w).abs() < 1e-9);
        }
    }

    #[test]
    fn climb_costs_more_than_hover_descent_less() {
        let (_, v) = veh(DroneType::Multirotor);
        let r = simulate(
            &v,
            &Env::default(),
            &[
                Leg::new(LegKind::Loiter, &v),
                Leg::new(LegKind::Climb, &v),
                Leg::new(LegKind::Descent, &v),
            ],
            0.0,
        );
        assert!(r.legs[1].power_w > r.legs[0].power_w);
        assert!(r.legs[2].power_w < r.legs[0].power_w);
    }

    #[test]
    fn all_types_and_examples_are_finite() {
        use crate::project::{
            EX_FIXED_WING, EX_FLYING_WING, EX_HEXA_550, EX_QUAD_250, EX_QUAD_275, EX_VTOL,
        };
        let mut projects: Vec<Project> = DroneType::ALL
            .iter()
            .map(|&dt| {
                let mut p = Project::default();
                p.set_drone_type(dt);
                p
            })
            .collect();
        for s in [
            EX_FIXED_WING,
            EX_FLYING_WING,
            EX_HEXA_550,
            EX_QUAD_250,
            EX_QUAD_275,
            EX_VTOL,
        ] {
            projects.push(serde_json::from_str(s).expect("örnek proje"));
        }
        let lib = Library::embedded();
        for p in &projects {
            let m = evaluate(p, &lib);
            let v = Vehicle::from_model(p, &m);
            let legs = default_legs(&v);
            let env = Env {
                alt_msl_m: 1200.0,
                isa_offset_k: 15.0,
                wind_speed: 4.0,
                wind_from_deg: 45.0,
                gust: 2.0,
                ..Env::default()
            };
            let r = simulate(&v, &env, &legs, 0.1);
            assert!(r.total_wh.is_finite() && r.budget_wh.is_finite());
            for l in &r.legs {
                assert!(l.energy_wh.is_finite() && l.power_w.is_finite() && l.time_s.is_finite());
                assert!(!l.remaining_pct.is_nan());
            }
            assert!(r.total_wh > 0.0, "{:?}", p.drone_type);
        }
    }

    #[test]
    fn degenerate_inputs_do_not_nan() {
        let (_, v) = veh(DroneType::FixedWingConventional);
        let bad = Leg {
            kind: LegKind::Cruise,
            height_m: f64::NAN,
            rate_ms: f64::NAN,
            distance_m: f64::INFINITY,
            airspeed_ms: f64::NAN,
            heading_deg: f64::NAN,
            duration_s: -5.0,
            drop_kg: f64::NAN,
        };
        let r = simulate(&v, &Env::default(), &[bad], f64::NAN);
        assert!(r.total_wh.is_finite());
    }
}
