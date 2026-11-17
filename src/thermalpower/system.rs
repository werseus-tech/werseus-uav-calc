// SPDX-License-Identifier: AGPL-3.0-or-later
//! Power-system adapter and combined analysis: reads motor count, pack voltage and currents from
//! the project model, then evaluates component heating ([`super::heat`]) and cable runs
//! ([`super::cable`]) together with the battery voltage sag.
//!
//! Everything here is an estimate from lumped, steady-state models, not a measurement.
//!
//! Current sources (all taken from the existing calculations):
//! - multirotor: hover current `mr.i_hover`; full throttle by momentum theory, where power scales
//!   with thrust^1.5, so `i_full = i_hover * (T/W)^1.5` (thrust-to-weight clamped to 1..6).
//! - fixed wing / flying wing / V-tail: one propulsion system; the "low" state is the cruise
//!   electrical power over the pack voltage, full throttle is the peak current `prop.i_peak`.
//! - VTOL: the lift motors as a multirotor plus one forward motor; the full-throttle total is the
//!   larger of the lift and forward cases (they do not run at full power together).

use super::cable::{self, AWG_MAX, AWG_MIN, RunInput, RunResult};
use super::heat::{self, Cooling, HeatResult, ThermalSpec};
use crate::calc::Model;
use crate::project::{DroneType, Project};

/// Per-conductor RMS current of a 120-degree commutated three-phase lead relative to the DC bus
/// current: `sqrt(2/3)`.
pub const PHASE_RMS_FACTOR: f64 = 0.816_496_580_927_726;

/// Electrical operating points read from the project model.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PowerSource {
    /// Nominal pack voltage, V.
    pub v_pack: f64,
    /// Estimated pack internal resistance, ohm.
    pub r_pack_ohm: f64,
    /// Number of ESCs (equal to the number of motors).
    pub n_esc: u32,
    /// True when the low state is hover (multirotor and VTOL), false for cruise.
    pub low_is_hover: bool,
    /// Total pack current in the low state (hover or cruise), A.
    pub i_low_total: f64,
    /// Total pack current at full throttle, A.
    pub i_full_total: f64,
    /// Current of the most loaded ESC in the low state, A.
    pub i_low_esc: f64,
    /// Current of the most loaded ESC at full throttle, A.
    pub i_full_esc: f64,
    /// Number of cells in series and parallel (derived from the pack).
    pub n_series: u32,
    pub n_parallel: u32,
}

fn clean(x: f64) -> f64 {
    if x.is_finite() && x > 0.0 { x } else { 0.0 }
}

/// Internal-resistance constant per chemistry, `r_cell = K / capacity_Ah` in mohm (rough class
/// averages; only used when the cell record has no measured value).
fn k_mohm_ah(chemistry: &str) -> f64 {
    let c = chemistry.to_lowercase();
    if c.contains("fe") {
        50.0
    } else if c.contains("po") {
        9.0
    } else if c.contains("ion") {
        90.0
    } else {
        15.0
    }
}

/// Reads the operating points from the model. Works for every vehicle type.
pub fn power_source(p: &Project, m: &Model) -> PowerSource {
    let v_pack = clean(m.weight.pack_voltage);
    let cell_v = clean(m.cell.nominal_v);
    let cell_ah = clean(m.cell.capacity_ah);
    let n_series = if cell_v > 0.0 {
        (v_pack / cell_v).round().max(1.0) as u32
    } else {
        1
    };
    let n_parallel = if cell_ah > 0.0 {
        (clean(m.weight.pack_capacity_ah) / cell_ah)
            .round()
            .max(1.0) as u32
    } else {
        1
    };
    let r_cell_mohm = if p.battery_source
        == crate::calc::custom_battery::BatterySourceKind::CustomCell
        && p.custom_cell.r_int_mohm.is_finite()
        && p.custom_cell.r_int_mohm > 0.0
    {
        p.custom_cell.r_int_mohm
    } else if cell_ah > 0.0 {
        k_mohm_ah(&m.cell.chemistry) / cell_ah
    } else {
        0.0
    };
    let r_pack_ohm = r_cell_mohm * 1e-3 * n_series as f64 / n_parallel as f64;

    let tw = if m.mr.tw_ratio.is_finite() {
        m.mr.tw_ratio.clamp(1.0, 6.0)
    } else {
        1.0
    };
    let n_lift = m.mr.n_motors.max(1);
    let lift_low = clean(m.mr.i_hover);
    let lift_full = lift_low * tw.powf(1.5);
    let fw_peak = clean(m.prop.i_peak);
    let fw_cruise = if v_pack > 0.0 {
        clean(m.perf_cr.p_prop_elec) / v_pack
    } else {
        0.0
    };

    let (n_esc, low_is_hover, i_low_total, i_full_total, i_low_esc, i_full_esc) = match p.drone_type
    {
        DroneType::Multirotor => (
            n_lift,
            true,
            lift_low,
            lift_full,
            lift_low / n_lift as f64,
            lift_full / n_lift as f64,
        ),
        DroneType::VtolQuadplane => (
            n_lift + 1,
            true,
            lift_low,
            lift_full.max(fw_peak),
            lift_low / n_lift as f64,
            (lift_full / n_lift as f64).max(fw_peak),
        ),
        _ => (1, false, fw_cruise, fw_peak, fw_cruise, fw_peak),
    };
    PowerSource {
        v_pack,
        r_pack_ohm,
        n_esc,
        low_is_hover,
        i_low_total,
        i_full_total,
        i_low_esc,
        i_full_esc,
        n_series,
        n_parallel,
    }
}

// ---------------------------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------------------------

/// How a component is cooled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoolingKind {
    Enclosed,
    Open,
    Airflow,
}

/// ESC loss model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EscModel {
    /// Loss = `V * I * (1 - eta)`.
    Efficiency,
    /// Loss = `I^2 * R_on + sw_frac * V * I`.
    RdsOn,
}

/// BEC / regulator type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BecKind {
    Linear,
    Switching,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EscCfg {
    pub model: EscModel,
    pub eta: f64,
    pub r_on_mohm: f64,
    pub sw_frac: f64,
    pub cooling: CoolingKind,
    pub limit_c: f64,
    pub spec: ThermalSpec,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BecCfg {
    pub kind: BecKind,
    pub v_out: f64,
    pub i_load_a: f64,
    pub eta: f64,
    pub cooling: CoolingKind,
    pub limit_c: f64,
    pub spec: ThermalSpec,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VtxCfg {
    pub p_in_w: f64,
    pub p_rf_w: f64,
    pub cooling: CoolingKind,
    pub limit_c: f64,
    pub spec: ThermalSpec,
}

/// One cable run: gauge and one-way length.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RunCfg {
    pub awg: u32,
    pub length_m: f64,
}

/// All user inputs of the analysis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Config {
    pub ambient_c: f64,
    /// Air speed over components and wires when the cooling is [`CoolingKind::Airflow`], m/s
    /// (propeller wash or forward flight).
    pub air_speed_ms: f64,
    pub esc: EscCfg,
    pub bec: BecCfg,
    pub vtx: VtxCfg,
    pub battery_lead: RunCfg,
    pub esc_leads: RunCfg,
    pub phase_leads: RunCfg,
    pub max_drop_pct: f64,
    pub max_rise_k: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ambient_c: 25.0,
            air_speed_ms: 5.0,
            esc: EscCfg {
                model: EscModel::Efficiency,
                eta: 0.97,
                r_on_mohm: 4.0,
                sw_frac: 0.005,
                cooling: CoolingKind::Airflow,
                limit_c: 100.0,
                spec: ThermalSpec {
                    area_m2: 0.003,
                    r_internal_k_per_w: 1.0,
                    heat_capacity_j_per_k: 20.0,
                },
            },
            bec: BecCfg {
                kind: BecKind::Switching,
                v_out: 5.0,
                i_load_a: 2.0,
                eta: 0.88,
                cooling: CoolingKind::Enclosed,
                limit_c: 85.0,
                spec: ThermalSpec {
                    area_m2: 0.0012,
                    r_internal_k_per_w: 8.0,
                    heat_capacity_j_per_k: 8.0,
                },
            },
            vtx: VtxCfg {
                p_in_w: 4.0,
                p_rf_w: 0.8,
                cooling: CoolingKind::Enclosed,
                limit_c: 85.0,
                spec: ThermalSpec {
                    area_m2: 0.003,
                    r_internal_k_per_w: 2.0,
                    heat_capacity_j_per_k: 18.0,
                },
            },
            battery_lead: RunCfg {
                awg: 12,
                length_m: 0.10,
            },
            esc_leads: RunCfg {
                awg: 14,
                length_m: 0.15,
            },
            phase_leads: RunCfg {
                awg: 16,
                length_m: 0.10,
            },
            max_drop_pct: 2.0,
            max_rise_k: 30.0,
        }
    }
}

fn cooling_of(kind: CoolingKind, air_speed_ms: f64) -> Cooling {
    match kind {
        CoolingKind::Enclosed => Cooling::EnclosedStill,
        CoolingKind::Open => Cooling::OpenStill,
        CoolingKind::Airflow => Cooling::Airflow(air_speed_ms),
    }
}

// ---------------------------------------------------------------------------------------------
// Results
// ---------------------------------------------------------------------------------------------

/// Heating of one component type in the low state and at full throttle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ComponentReport {
    /// Number of identical units.
    pub count: u32,
    pub low: HeatResult,
    pub full: HeatResult,
}

/// Which lead a run represents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunKind {
    /// Battery lead: carries the total pack current.
    Battery,
    /// ESC power leads: one run per ESC, carrying that ESC's current.
    EscPower,
    /// Motor phase leads: three conductors per motor, per-conductor RMS current
    /// `sqrt(2/3) * I_dc`.
    Phase,
}

/// Evaluation of one cable run (one unit) and its totals.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RunReport {
    pub kind: RunKind,
    pub count: u32,
    pub cfg: RunCfg,
    pub current_low_a: f64,
    pub current_full_a: f64,
    pub low: RunResult,
    pub full: RunResult,
    /// Voltage drop at full throttle as a percentage of the pack voltage.
    pub drop_full_pct: f64,
    /// Heat of all units of this run kind, W.
    pub total_loss_low_w: f64,
    pub total_loss_full_w: f64,
    /// Copper mass of all units, kg.
    pub total_mass_kg: f64,
    /// Thinnest AWG meeting both limits at full throttle, `None` if even the thickest fails.
    pub recommended_awg: Option<u32>,
    /// Chosen gauge meets the drop and rise limits at full throttle.
    pub ok: bool,
}

/// Battery voltage sag.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SagReport {
    pub v_nominal: f64,
    pub r_pack_ohm: f64,
    pub sag_low_v: f64,
    pub sag_full_v: f64,
    /// Sum of the series cable drops on the path pack -> motor (battery lead + ESC lead).
    pub cable_drop_low_v: f64,
    pub cable_drop_full_v: f64,
    pub v_motor_low: f64,
    pub v_motor_full: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Analysis {
    pub esc: ComponentReport,
    pub bec: ComponentReport,
    pub vtx: ComponentReport,
    pub runs: [RunReport; 3],
    pub sag: SagReport,
}

fn evaluate_one(
    kind: RunKind,
    cfg: RunCfg,
    count: u32,
    i_low: f64,
    i_full: f64,
    c: &Config,
    v_pack: f64,
) -> RunReport {
    let run = |awg: u32, i: f64| {
        let dc = cable::evaluate_run(&RunInput {
            awg,
            length_m: cfg.length_m,
            current_a: i,
            ambient_c: c.ambient_c,
            air_speed_ms: c.air_speed_ms,
        });
        match kind {
            // Drop and loss follow the DC-equivalent path (two phases in series); the conductor
            // temperature follows the lower per-conductor RMS current.
            RunKind::Phase => {
                let rms = cable::evaluate_run(&RunInput {
                    awg,
                    length_m: cfg.length_m,
                    current_a: i * PHASE_RMS_FACTOR,
                    ambient_c: c.ambient_c,
                    air_speed_ms: c.air_speed_ms,
                });
                RunResult {
                    temp_rise_k: rms.temp_rise_k,
                    wire_temp_c: rms.wire_temp_c,
                    mass_kg: dc.mass_kg * 1.5,
                    ..dc
                }
            }
            _ => dc,
        }
    };
    let low = run(cfg.awg, i_low);
    let full = run(cfg.awg, i_full);
    let drop_limit = v_pack * c.max_drop_pct / 100.0;
    let ok = full.voltage_drop_v <= drop_limit && full.temp_rise_k <= c.max_rise_k;
    let recommended_awg = (AWG_MIN..=AWG_MAX).rev().find(|&a| {
        let r = run(a, i_full);
        r.voltage_drop_v <= drop_limit && r.temp_rise_k <= c.max_rise_k
    });
    let n = count as f64;
    RunReport {
        kind,
        count,
        cfg,
        current_low_a: i_low,
        current_full_a: i_full,
        low,
        full,
        drop_full_pct: if v_pack > 0.0 {
            100.0 * full.voltage_drop_v / v_pack
        } else {
            0.0
        },
        total_loss_low_w: low.power_loss_w * n,
        total_loss_full_w: full.power_loss_w * n,
        total_mass_kg: full.mass_kg * n,
        recommended_awg,
        ok,
    }
}

/// Runs the whole analysis for the given operating points and inputs.
pub fn analyze(src: &PowerSource, c: &Config) -> Analysis {
    let v = src.v_pack;
    let esc_loss = |i: f64| match c.esc.model {
        EscModel::Efficiency => heat::esc_loss_efficiency(v, i, c.esc.eta),
        EscModel::RdsOn => heat::esc_loss_rds_on(v, i, c.esc.r_on_mohm * 1e-3, c.esc.sw_frac),
    };
    let bec_loss = match c.bec.kind {
        BecKind::Linear => heat::bec_loss_linear(v, c.bec.v_out, c.bec.i_load_a),
        BecKind::Switching => heat::bec_loss_switching(c.bec.v_out, c.bec.i_load_a, c.bec.eta),
    };
    let vtx_loss = heat::vtx_loss(c.vtx.p_in_w, c.vtx.p_rf_w);
    let ev = |loss: f64, spec: &ThermalSpec, k: CoolingKind, limit: f64| {
        heat::evaluate_heat(
            loss,
            spec,
            cooling_of(k, c.air_speed_ms),
            c.ambient_c,
            limit,
        )
    };
    let esc = ComponentReport {
        count: src.n_esc,
        low: ev(
            esc_loss(src.i_low_esc),
            &c.esc.spec,
            c.esc.cooling,
            c.esc.limit_c,
        ),
        full: ev(
            esc_loss(src.i_full_esc),
            &c.esc.spec,
            c.esc.cooling,
            c.esc.limit_c,
        ),
    };
    let bec_r = ev(bec_loss, &c.bec.spec, c.bec.cooling, c.bec.limit_c);
    let vtx_r = ev(vtx_loss, &c.vtx.spec, c.vtx.cooling, c.vtx.limit_c);

    let n = src.n_esc;
    let runs = [
        evaluate_one(
            RunKind::Battery,
            c.battery_lead,
            1,
            src.i_low_total,
            src.i_full_total,
            c,
            v,
        ),
        evaluate_one(
            RunKind::EscPower,
            c.esc_leads,
            n,
            src.i_low_esc,
            src.i_full_esc,
            c,
            v,
        ),
        evaluate_one(
            RunKind::Phase,
            c.phase_leads,
            n,
            src.i_low_esc,
            src.i_full_esc,
            c,
            v,
        ),
    ];
    let sag_low = src.i_low_total * src.r_pack_ohm;
    let sag_full = src.i_full_total * src.r_pack_ohm;
    let cd_low = runs[0].low.voltage_drop_v + runs[1].low.voltage_drop_v;
    let cd_full = runs[0].full.voltage_drop_v + runs[1].full.voltage_drop_v;
    let sag = SagReport {
        v_nominal: v,
        r_pack_ohm: src.r_pack_ohm,
        sag_low_v: sag_low,
        sag_full_v: sag_full,
        cable_drop_low_v: cd_low,
        cable_drop_full_v: cd_full,
        v_motor_low: (v - sag_low - cd_low).max(0.0),
        v_motor_full: (v - sag_full - cd_full).max(0.0),
    };
    Analysis {
        esc,
        bec: ComponentReport {
            count: 1,
            low: bec_r,
            full: bec_r,
        },
        vtx: ComponentReport {
            count: 1,
            low: vtx_r,
            full: vtx_r,
        },
        runs,
        sag,
    }
}

/// Convenience: operating points from the model, then the analysis.
pub fn analyze_model(p: &Project, m: &Model, c: &Config) -> Analysis {
    analyze(&power_source(p, m), c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Library, evaluate};

    fn src_for(dt: DroneType) -> (Project, PowerSource) {
        let mut p = Project::default();
        p.set_drone_type(dt);
        let m = evaluate(&p, &Library::embedded());
        let s = power_source(&p, &m);
        (p, s)
    }

    #[test]
    fn power_source_is_finite_for_every_type() {
        for dt in DroneType::ALL {
            let (_, s) = src_for(dt);
            for x in [
                s.v_pack,
                s.r_pack_ohm,
                s.i_low_total,
                s.i_full_total,
                s.i_low_esc,
                s.i_full_esc,
            ] {
                assert!(x.is_finite() && x >= 0.0, "{dt:?}: {x}");
            }
            assert!(s.n_esc >= 1);
            assert!(s.v_pack > 0.0, "{dt:?}");
            assert!(s.i_full_total >= s.i_low_total * 0.99 || !s.low_is_hover);
        }
    }

    #[test]
    fn motor_counts_follow_the_vehicle() {
        let (_, fw) = src_for(DroneType::FixedWingConventional);
        assert_eq!(fw.n_esc, 1);
        let (p, mr) = src_for(DroneType::Multirotor);
        assert_eq!(mr.n_esc, p.mr_arm_count * if p.mr_coaxial { 2 } else { 1 });
        let (_, vt) = src_for(DroneType::VtolQuadplane);
        assert!(vt.n_esc >= 2);
    }

    #[test]
    fn analysis_has_no_nan_for_every_type() {
        for dt in DroneType::ALL {
            let (_, s) = src_for(dt);
            let a = analyze(&s, &Config::default());
            for r in a.runs {
                for x in [
                    r.low.voltage_drop_v,
                    r.full.power_loss_w,
                    r.total_mass_kg,
                    r.drop_full_pct,
                ] {
                    assert!(x.is_finite());
                }
            }
            assert!(a.esc.full.temp_steady_c.is_finite());
            assert!(a.sag.v_motor_full.is_finite());
        }
    }

    #[test]
    fn zero_current_gives_ambient_and_no_drop() {
        let mut s = src_for(DroneType::Multirotor).1;
        s.i_low_total = 0.0;
        s.i_full_total = 0.0;
        s.i_low_esc = 0.0;
        s.i_full_esc = 0.0;
        let c = Config {
            bec: BecCfg {
                i_load_a: 0.0,
                ..Config::default().bec
            },
            vtx: VtxCfg {
                p_in_w: 0.0,
                p_rf_w: 0.0,
                ..Config::default().vtx
            },
            ..Config::default()
        };
        let a = analyze(&s, &c);
        assert_eq!(a.esc.full.temp_steady_c, c.ambient_c);
        assert_eq!(a.bec.full.temp_steady_c, c.ambient_c);
        assert_eq!(a.vtx.full.temp_steady_c, c.ambient_c);
        assert_eq!(a.runs[0].full.voltage_drop_v, 0.0);
        assert_eq!(a.sag.sag_full_v, 0.0);
    }

    #[test]
    fn recommended_awg_satisfies_the_limits() {
        for dt in DroneType::ALL {
            let (_, s) = src_for(dt);
            let a = analyze(&s, &Config::default());
            for r in a.runs {
                if let Some(awg) = r.recommended_awg {
                    let c = Config {
                        battery_lead: RunCfg { awg, ..r.cfg },
                        esc_leads: RunCfg { awg, ..r.cfg },
                        phase_leads: RunCfg { awg, ..r.cfg },
                        ..Config::default()
                    };
                    let b = analyze(&s, &c);
                    let idx = match r.kind {
                        RunKind::Battery => 0,
                        RunKind::EscPower => 1,
                        RunKind::Phase => 2,
                    };
                    assert!(b.runs[idx].ok, "{dt:?} {:?} awg {awg}", r.kind);
                }
            }
        }
    }

    #[test]
    fn thicker_wire_lowers_the_drop() {
        let (_, s) = src_for(DroneType::Multirotor);
        let mut last = f64::INFINITY;
        for awg in (AWG_MIN..=AWG_MAX).rev() {
            let c = Config {
                battery_lead: RunCfg { awg, length_m: 0.2 },
                ..Config::default()
            };
            let d = analyze(&s, &c).runs[0].full.voltage_drop_v;
            assert!(d <= last + 1e-12 || last.is_infinite());
            last = d;
        }
    }

    #[test]
    fn drop_and_loss_follow_ohms_law() {
        let (_, s) = src_for(DroneType::FlyingWing);
        let a = analyze(&s, &Config::default());
        let r = a.runs[0];
        assert!((r.full.voltage_drop_v - r.current_full_a * r.full.resistance_ohm).abs() < 1e-9);
        assert!(
            (r.full.power_loss_w - r.current_full_a.powi(2) * r.full.resistance_ohm).abs() < 1e-9
        );
    }

    #[test]
    fn hostile_inputs_do_not_produce_nan() {
        let (_, s) = src_for(DroneType::VtolQuadplane);
        for bad in [f64::NAN, -5.0, 0.0, 1e12, f64::INFINITY] {
            let mut c = Config::default();
            c.ambient_c = bad;
            c.air_speed_ms = bad;
            c.esc.eta = bad;
            c.bec.i_load_a = bad;
            c.vtx.p_in_w = bad;
            c.battery_lead.length_m = bad;
            c.esc_leads.awg = 0;
            c.phase_leads.awg = 999;
            c.max_drop_pct = bad;
            c.max_rise_k = bad;
            let a = analyze(&s, &c);
            assert!(a.esc.full.temp_steady_c.is_finite(), "{bad}");
            assert!(a.bec.full.temp_rise_k.is_finite(), "{bad}");
            assert!(a.vtx.full.loss_w.is_finite(), "{bad}");
            for r in a.runs {
                assert!(r.full.power_loss_w.is_finite(), "{bad}");
                assert!(r.total_mass_kg.is_finite(), "{bad}");
            }
        }
        let mut s2 = s;
        s2.i_full_total = f64::NAN;
        s2.v_pack = f64::INFINITY;
        let _ = analyze(&s2, &Config::default());
    }
}
