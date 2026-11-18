// SPDX-License-Identifier: AGPL-3.0-or-later
//! Onboard computer (edge AI) load analysis: how much flight time a companion computer
//! and its sensors cost, and whether the perception-to-control latency leaves enough
//! stopping distance. Pure functions; all results are rough estimates.

pub mod flight;
pub mod latency;
pub mod presets;

use crate::calc::Model;
use crate::project::DroneType;
use flight::Baseline;

/// Picks the flight-time baseline for the vehicle type from the evaluated model:
/// multirotors use the hover power, every other type the fixed-wing cruise point.
/// Returns `None` while the model values are not usable yet.
pub fn baseline_from_model(dt: DroneType, m: &Model) -> Option<Baseline> {
    let ok = |x: f64| x.is_finite() && x > 0.0;
    match dt {
        DroneType::Multirotor => {
            let r = &m.mr;
            let p_hotel_w = (r.p_total_hover - r.p_elec_hover_total).max(0.0);
            (ok(r.e_usable_wh) && ok(r.p_elec_hover_total) && ok(r.m_mtow)).then_some(
                Baseline::Rotor {
                    e_usable_wh: r.e_usable_wh,
                    p_prop_w: r.p_elec_hover_total,
                    p_hotel_w,
                    mass_kg: r.m_mtow,
                    t_max_n: if r.t_max_total.is_finite() && r.t_max_total > 0.0 {
                        r.t_max_total
                    } else {
                        0.0
                    },
                },
            )
        }
        _ => {
            let p = &m.perf_in;
            let v = m.perf_cr.v;
            let vtol_t_max_n = (dt == DroneType::VtolQuadplane).then_some(
                if m.mr.t_max_total.is_finite() && m.mr.t_max_total > 0.0 {
                    m.mr.t_max_total
                } else {
                    0.0
                },
            );
            (ok(p.w) && ok(p.ws) && ok(p.e_pack_wh) && ok(v)).then(|| Baseline::Wing {
                perf: p.clone(),
                v_cruise: v,
                cl_max: m.aero.cl_max,
                p_max_elec: m.prop_in.p_installed_elec,
                vtol_t_max_n,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::flight::flight_time_loss;
    use super::*;
    use crate::calc::{Library, evaluate};
    use crate::project::Project;

    #[test]
    fn every_vehicle_type_has_a_working_baseline() {
        let lib = Library::embedded();
        for dt in DroneType::ALL {
            let mut p = Project::default();
            p.set_drone_type(dt);
            let m = evaluate(&p, &lib);
            let base = baseline_from_model(dt, &m).unwrap_or_else(|| panic!("no baseline {dt:?}"));
            let none = flight_time_loss(&base, 0.0, 0.0);
            let some = flight_time_loss(&base, 10.0, 0.2);
            assert!(none.t_base_min > 0.0, "{dt:?}");
            assert!(none.lost_min.abs() < 1e-9, "{dt:?}");
            assert!(some.lost_min > 0.0 && some.lost_pct < 100.0, "{dt:?}");
            assert_eq!(some.vtol_hover_omitted, dt == DroneType::VtolQuadplane);
        }
    }
}
