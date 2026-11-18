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
//! - VTOL: the lift motors as a multirotor plus one forward motor modeled separately; the
//!   worst-case pack current is the larger of vertical full throttle and transition (hover lift +
//!   peak forward).

use super::cable::{self, RunInput, RunResult};
use super::heat::{self, Cooling, HeatResult, HeatStatus, ThermalSpec};
use crate::calc::Model;
use crate::project::{DroneType, Project};

/// Per-conductor RMS current of a 120-degree commutated three-phase lead relative to the DC bus
/// current: `sqrt(2/3)`.
pub const PHASE_RMS_FACTOR: f64 = 0.816_496_580_927_726;

/// Operating points of one identical motor / ESC group.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotorGroup {
    /// Number of identical motors / ESCs in this group.
    pub count: u32,
    /// Per-ESC DC bus current in the low state, A.
    pub i_low_esc: f64,
    /// Per-ESC DC bus current at full throttle, A.
    pub i_full_esc: f64,
    /// Estimated PWM duty cycle in the low state (`0.05..=1.0`).
    pub duty_low: f64,
    /// Estimated PWM duty cycle at full throttle (`0.05..=1.0`).
    pub duty_full: f64,
}

/// Electrical operating points read from the project model.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PowerSource {
    /// Nominal pack voltage, V.
    pub v_pack: f64,
    /// Estimated pack internal resistance, ohm.
    pub r_pack_ohm: f64,
    /// True when the low state is hover (multirotor and VTOL), false for cruise.
    pub low_is_hover: bool,
    /// Total pack current in the low state (hover or cruise), A.
    pub i_low_total: f64,
    /// Total pack current in the worst-case high-power state (full throttle or VTOL transition), A.
    pub i_full_total: f64,
    /// Lift motor group (multirotor and VTOL), if present.
    pub lift: Option<MotorGroup>,
    /// Forward / cruise motor group (fixed wing and VTOL), if present.
    pub forward: Option<MotorGroup>,
    /// Number of cells in series and parallel (derived from the pack).
    pub n_series: u32,
    pub n_parallel: u32,
}

impl PowerSource {
    /// Total number of ESCs across lift and forward groups.
    pub fn total_esc_count(&self) -> u32 {
        self.lift.map_or(0, |g| g.count) + self.forward.map_or(0, |g| g.count)
    }

    /// Highest per-ESC current at full throttle across all groups, A.
    pub fn max_full_esc_current(&self) -> f64 {
        let a = self.lift.map_or(0.0, |g| clean(g.i_full_esc));
        let b = self.forward.map_or(0.0, |g| clean(g.i_full_esc));
        a.max(b)
    }
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
    } else if c.contains("po") || c.contains("hv") {
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
    let hover_duty = if m.mr.hover_throttle.is_finite() && m.mr.hover_throttle > 0.0 {
        m.mr.hover_throttle.clamp(0.05, 1.0)
    } else {
        (1.0 / tw.sqrt()).clamp(0.05, 1.0)
    };
    let fw_peak = clean(m.prop.i_peak);
    let fw_cruise = if v_pack > 0.0 {
        clean(m.perf_cr.p_prop_elec) / v_pack
    } else {
        0.0
    };
    let fw_duty = if fw_peak > 0.0 && fw_cruise > 0.0 {
        (fw_cruise / fw_peak)
            .clamp(0.05, 1.0)
            .cbrt()
            .clamp(0.05, 1.0)
    } else {
        1.0
    };

    let (low_is_hover, i_low_total, i_full_total, lift, forward) = match p.drone_type {
        DroneType::Multirotor => (
            true,
            lift_low,
            lift_full,
            Some(MotorGroup {
                count: n_lift,
                i_low_esc: lift_low / n_lift as f64,
                i_full_esc: lift_full / n_lift as f64,
                duty_low: hover_duty,
                duty_full: 1.0,
            }),
            None,
        ),
        DroneType::VtolQuadplane => (
            true,
            lift_low,
            // Worst-case pack draw is either full vertical climb or transition (hover lift + peak
            // forward thrust).
            lift_full.max(lift_low + fw_peak),
            Some(MotorGroup {
                count: n_lift,
                i_low_esc: lift_low / n_lift as f64,
                i_full_esc: lift_full / n_lift as f64,
                duty_low: hover_duty,
                duty_full: 1.0,
            }),
            Some(MotorGroup {
                count: 1,
                i_low_esc: 0.0,
                i_full_esc: fw_peak,
                duty_low: 1.0,
                duty_full: 1.0,
            }),
        ),
        _ => (
            false,
            fw_cruise,
            fw_peak,
            None,
            Some(MotorGroup {
                count: 1,
                i_low_esc: fw_cruise,
                i_full_esc: fw_peak,
                duty_low: fw_duty,
                duty_full: 1.0,
            }),
        ),
    };
    PowerSource {
        v_pack,
        r_pack_ohm,
        low_is_hover,
        i_low_total,
        i_full_total,
        lift,
        forward,
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
    pub fwd_esc_lead: RunCfg,
    pub fwd_phase_lead: RunCfg,
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
            fwd_esc_lead: RunCfg {
                awg: 14,
                length_m: 0.15,
            },
            fwd_phase_lead: RunCfg {
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
    /// Forward/pusher ESC power lead (VTOL only).
    FwdEscPower,
    /// Forward/pusher motor phase leads (VTOL only).
    FwdPhase,
}

fn is_phase_run(kind: RunKind) -> bool {
    matches!(kind, RunKind::Phase | RunKind::FwdPhase)
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
    /// Sum of the series cable drops on the path pack -> motor (battery + ESC + phase leads).
    pub cable_drop_low_v: f64,
    pub cable_drop_full_v: f64,
    pub v_motor_low: f64,
    pub v_motor_full: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Analysis {
    pub esc_lift: Option<ComponentReport>,
    pub esc_forward: Option<ComponentReport>,
    pub bec: ComponentReport,
    pub vtx: ComponentReport,
    pub bec_invalid: bool,
    pub vtx_invalid: bool,
    pub runs: Vec<RunReport>,
    pub sag: SagReport,
}

fn clean_duty(d: f64) -> f64 {
    if d.is_finite() {
        d.clamp(0.05, 1.0)
    } else {
        1.0
    }
}

fn evaluate_one(
    kind: RunKind,
    cfg: RunCfg,
    count: u32,
    i_low_dc: f64,
    i_full_dc: f64,
    c: &Config,
    v_pack: f64,
) -> RunReport {
    let i_low_dc = clean(i_low_dc).min(1e9);
    let i_full_dc = clean(i_full_dc).min(1e9);
    let v_pack = clean(v_pack).min(1e6);
    let max_drop_pct = clean(c.max_drop_pct).min(100.0);
    let max_rise_k = clean(c.max_rise_k).min(1e6);
    let phase = is_phase_run(kind);
    let factor = if phase { PHASE_RMS_FACTOR } else { 1.0 };

    let run = |i_dc: f64| {
        let res = cable::evaluate_run(&RunInput {
            awg: cfg.awg,
            length_m: cfg.length_m,
            current_a: i_dc * factor,
            ambient_c: c.ambient_c,
            air_speed_ms: c.air_speed_ms,
        });
        if phase {
            // Two phases conduct in series at any instant (DC-equivalent drop = I_dc * R_loop(T_rms));
            // three conductors dissipate 3 * I_rms^2 * R_single(T_rms) = 1.5 * res.power_loss_w.
            RunResult {
                voltage_drop_v: i_dc * res.resistance_ohm,
                power_loss_w: 1.5 * res.power_loss_w,
                mass_kg: 1.5 * res.mass_kg,
                ..res
            }
        } else {
            res
        }
    };
    let low = run(i_low_dc);
    let full = run(i_full_dc);
    let drop_limit = v_pack * max_drop_pct / 100.0;
    let ok = full.voltage_drop_v <= drop_limit && full.temp_rise_k <= max_rise_k;
    let recommended_awg = cable::recommend_awg(
        cfg.length_m,
        i_full_dc * factor,
        c.ambient_c,
        c.air_speed_ms,
        v_pack * factor,
        max_drop_pct,
        max_rise_k,
    );
    let n = count as f64;
    RunReport {
        kind,
        count,
        cfg,
        current_low_a: i_low_dc * factor,
        current_full_a: i_full_dc * factor,
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
    let v = clean(src.v_pack).min(1e6);
    let r_pack = clean(src.r_pack_ohm).min(1e6);
    let i_low_tot = clean(src.i_low_total).min(1e9);
    let i_full_tot = clean(src.i_full_total).min(1e9);
    let r_on_ohm = clean(c.esc.r_on_mohm).min(1e6) * 1e-3;
    let sw_frac = clean(c.esc.sw_frac).min(1.0);

    let esc_loss = |i: f64, duty: f64| match c.esc.model {
        EscModel::Efficiency => heat::esc_loss_efficiency(v, i, c.esc.eta),
        EscModel::RdsOn => heat::esc_loss_rds_on(v, i, duty, r_on_ohm, sw_frac),
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
    let eval_group = |g: MotorGroup| -> (u32, f64, f64, ComponentReport) {
        let i_low = clean(g.i_low_esc).min(1e9);
        let i_full = clean(g.i_full_esc).min(1e9);
        let d_low = clean_duty(g.duty_low);
        let d_full = clean_duty(g.duty_full);
        let rep = ComponentReport {
            count: g.count,
            low: ev(
                esc_loss(i_low, d_low),
                &c.esc.spec,
                c.esc.cooling,
                c.esc.limit_c,
            ),
            full: ev(
                esc_loss(i_full, d_full),
                &c.esc.spec,
                c.esc.cooling,
                c.esc.limit_c,
            ),
        };
        (g.count, i_low, i_full, rep)
    };

    let lift_eval = src.lift.map(eval_group);
    let fwd_eval = src.forward.map(eval_group);
    let esc_lift = lift_eval.as_ref().map(|x| x.3);
    let esc_forward = fwd_eval.as_ref().map(|x| x.3);

    let bec_invalid = c.bec.kind == BecKind::Linear && clean(c.bec.v_out) > v;
    let vtx_invalid = clean(c.vtx.p_rf_w) > clean(c.vtx.p_in_w);
    let mut bec_r = ev(bec_loss, &c.bec.spec, c.bec.cooling, c.bec.limit_c);
    if bec_invalid {
        bec_r.status = HeatStatus::Fail;
    }
    let mut vtx_r = ev(vtx_loss, &c.vtx.spec, c.vtx.cooling, c.vtx.limit_c);
    if vtx_invalid {
        vtx_r.status = HeatStatus::Fail;
    }

    let mut runs = vec![evaluate_one(
        RunKind::Battery,
        c.battery_lead,
        1,
        i_low_tot,
        i_full_tot,
        c,
        v,
    )];
    if let Some((cnt, i_low, i_full, _)) = lift_eval {
        runs.push(evaluate_one(
            RunKind::EscPower,
            c.esc_leads,
            cnt,
            i_low,
            i_full,
            c,
            v,
        ));
        runs.push(evaluate_one(
            RunKind::Phase,
            c.phase_leads,
            cnt,
            i_low,
            i_full,
            c,
            v,
        ));
    }
    if let Some((cnt, i_low, i_full, _)) = fwd_eval {
        let (esc_kind, phase_kind, esc_cfg, phase_cfg) = if lift_eval.is_some() {
            (
                RunKind::FwdEscPower,
                RunKind::FwdPhase,
                c.fwd_esc_lead,
                c.fwd_phase_lead,
            )
        } else {
            (
                RunKind::EscPower,
                RunKind::Phase,
                c.esc_leads,
                c.phase_leads,
            )
        };
        runs.push(evaluate_one(esc_kind, esc_cfg, cnt, i_low, i_full, c, v));
        runs.push(evaluate_one(
            phase_kind, phase_cfg, cnt, i_low, i_full, c, v,
        ));
    }

    let sag_low = i_low_tot * r_pack;
    let sag_full = i_full_tot * r_pack;
    let bat_low = runs[0].low.voltage_drop_v;
    let bat_full = runs[0].full.voltage_drop_v;
    let branch_drop = |low: bool, fwd: bool| -> f64 {
        runs.iter()
            .filter(|r| {
                if fwd {
                    matches!(r.kind, RunKind::FwdEscPower | RunKind::FwdPhase)
                } else {
                    matches!(r.kind, RunKind::EscPower | RunKind::Phase)
                }
            })
            .map(|r| {
                if low {
                    r.low.voltage_drop_v
                } else {
                    r.full.voltage_drop_v
                }
            })
            .sum()
    };
    let cd_low = bat_low + branch_drop(true, false).max(branch_drop(true, true));
    let cd_full = bat_full + branch_drop(false, false).max(branch_drop(false, true));
    let sag = SagReport {
        v_nominal: v,
        r_pack_ohm: r_pack,
        sag_low_v: sag_low,
        sag_full_v: sag_full,
        cable_drop_low_v: cd_low,
        cable_drop_full_v: cd_full,
        v_motor_low: (v - sag_low - cd_low).max(0.0),
        v_motor_full: (v - sag_full - cd_full).max(0.0),
    };
    Analysis {
        esc_lift,
        esc_forward,
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
        bec_invalid,
        vtx_invalid,
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
    use super::cable::{AWG_MAX, AWG_MIN};
    use super::*;
    use crate::calc::{Library, evaluate};

    fn src_for(dt: DroneType) -> (Project, PowerSource) {
        let mut p = Project::default();
        p.set_drone_type(dt);
        let m = evaluate(&p, &Library::embedded());
        let s = power_source(&p, &m);
        (p, s)
    }

    fn assert_analysis_finite(a: &Analysis, ctx: &str) {
        for r in [&a.bec, &a.vtx]
            .into_iter()
            .chain(a.esc_lift.iter())
            .chain(a.esc_forward.iter())
        {
            for h in [r.low, r.full] {
                for v in [
                    h.loss_w,
                    h.r_th_k_per_w,
                    h.tau_s,
                    h.temp_rise_k,
                    h.temp_steady_c,
                    h.margin_k,
                ] {
                    assert!(v.is_finite(), "{ctx}: {v}");
                }
                if let Some(t) = h.time_to_limit_s {
                    assert!(t.is_finite(), "{ctx}: time={t}");
                }
            }
        }
        for r in &a.runs {
            for v in [
                r.current_low_a,
                r.current_full_a,
                r.low.resistance_ohm,
                r.low.voltage_drop_v,
                r.low.power_loss_w,
                r.low.temp_rise_k,
                r.low.wire_temp_c,
                r.low.mass_kg,
                r.full.resistance_ohm,
                r.full.voltage_drop_v,
                r.full.power_loss_w,
                r.full.temp_rise_k,
                r.full.wire_temp_c,
                r.full.mass_kg,
                r.drop_full_pct,
                r.total_loss_low_w,
                r.total_loss_full_w,
                r.total_mass_kg,
            ] {
                assert!(v.is_finite(), "{ctx} {:?}: {v}", r.kind);
            }
        }
        for v in [
            a.sag.v_nominal,
            a.sag.r_pack_ohm,
            a.sag.sag_low_v,
            a.sag.sag_full_v,
            a.sag.cable_drop_low_v,
            a.sag.cable_drop_full_v,
            a.sag.v_motor_low,
            a.sag.v_motor_full,
        ] {
            assert!(v.is_finite(), "{ctx} sag: {v}");
        }
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
                s.max_full_esc_current(),
            ] {
                assert!(x.is_finite() && x >= 0.0, "{dt:?}: {x}");
            }
            for g in s.lift.into_iter().chain(s.forward) {
                assert!(g.count >= 1);
                for v in [g.i_low_esc, g.i_full_esc, g.duty_low, g.duty_full] {
                    assert!(v.is_finite() && v >= 0.0, "{dt:?}: {v}");
                }
            }
            assert!(s.total_esc_count() >= 1);
            assert!(s.v_pack > 0.0, "{dt:?}");
            assert!(
                s.i_full_total >= s.i_low_total * 0.99,
                "{dt:?}: full={} low={}",
                s.i_full_total,
                s.i_low_total
            );
        }
    }

    #[test]
    fn motor_counts_follow_the_vehicle() {
        let (_, fw) = src_for(DroneType::FixedWingConventional);
        assert_eq!(fw.total_esc_count(), 1);
        assert!(fw.lift.is_none() && fw.forward.is_some());
        let (p, mr) = src_for(DroneType::Multirotor);
        assert_eq!(
            mr.total_esc_count(),
            p.mr_arm_count * if p.mr_coaxial { 2 } else { 1 }
        );
        assert!(mr.lift.is_some() && mr.forward.is_none());
        let (_, vt) = src_for(DroneType::VtolQuadplane);
        assert!(vt.total_esc_count() >= 2);
        assert!(vt.lift.is_some() && vt.forward.is_some());
    }

    #[test]
    fn vtol_separates_lift_and_forward_groups_and_models_transition() {
        let mut p = Project::default();
        p.set_drone_type(DroneType::VtolQuadplane);
        let m = evaluate(&p, &Library::embedded());
        let s = power_source(&p, &m);
        let lift = s.lift.expect("VTOL lift group");
        let fwd = s.forward.expect("VTOL forward group");
        assert_eq!(fwd.count, 1);
        assert_eq!(fwd.i_low_esc, 0.0);
        // Transition draws hover lift + forward peak simultaneously.
        assert!(s.i_full_total >= s.i_low_total + fwd.i_full_esc - 1e-9);

        let a = analyze(&s, &Config::default());
        let esc_l = a.esc_lift.expect("lift ESC report");
        let esc_f = a.esc_forward.expect("forward ESC report");
        assert_eq!(esc_l.count, lift.count);
        assert_eq!(esc_f.count, 1);
        // Idle forward ESC in hover has zero loss.
        assert_eq!(esc_f.low.loss_w, 0.0);
        assert_eq!(a.runs.len(), 5);
        let fwd_esc_run = a
            .runs
            .iter()
            .find(|r| r.kind == RunKind::FwdEscPower)
            .unwrap();
        assert_eq!(fwd_esc_run.count, 1);
        assert_eq!(fwd_esc_run.low.power_loss_w, 0.0);
    }

    #[test]
    fn analysis_has_no_nan_for_every_type() {
        for dt in DroneType::ALL {
            let (_, s) = src_for(dt);
            let a = analyze(&s, &Config::default());
            assert_analysis_finite(&a, &format!("{dt:?}"));
        }
    }

    #[test]
    fn zero_current_gives_ambient_and_no_drop() {
        let mut s = src_for(DroneType::Multirotor).1;
        s.i_low_total = 0.0;
        s.i_full_total = 0.0;
        if let Some(ref mut g) = s.lift {
            g.i_low_esc = 0.0;
            g.i_full_esc = 0.0;
        }
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
        assert_eq!(a.esc_lift.unwrap().full.temp_steady_c, c.ambient_c);
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
            for r in &a.runs {
                if let Some(awg) = r.recommended_awg {
                    let mut c = Config::default();
                    let run_cfg = RunCfg { awg, ..r.cfg };
                    match r.kind {
                        RunKind::Battery => c.battery_lead = run_cfg,
                        RunKind::EscPower => c.esc_leads = run_cfg,
                        RunKind::Phase => c.phase_leads = run_cfg,
                        RunKind::FwdEscPower => c.fwd_esc_lead = run_cfg,
                        RunKind::FwdPhase => c.fwd_phase_lead = run_cfg,
                    }
                    let b = analyze(&s, &c);
                    let matched = b.runs.iter().find(|x| x.kind == r.kind).unwrap();
                    assert!(matched.ok, "{dt:?} {:?} awg {awg}", r.kind);
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
    fn phase_run_uses_rms_current_temperature_and_three_phase_loss() {
        let (_, s) = src_for(DroneType::Multirotor);
        let a = analyze(&s, &Config::default());
        let i_dc = s.lift.unwrap().i_full_esc;
        let phase = a.runs.iter().find(|r| r.kind == RunKind::Phase).unwrap();
        assert!((phase.current_full_a - i_dc * PHASE_RMS_FACTOR).abs() < 1e-12);
        let expected_r_loop = 2.0
            * phase.cfg.length_m
            * cable::resistance_per_m(phase.cfg.awg, phase.full.wire_temp_c);
        assert!((phase.full.resistance_ohm - expected_r_loop).abs() < 1e-12);
        let expected_3ph_loss = 3.0 * phase.current_full_a.powi(2) * (expected_r_loop * 0.5);
        assert!((phase.full.power_loss_w - expected_3ph_loss).abs() < 1e-9);
        assert!((phase.full.voltage_drop_v - i_dc * expected_r_loop).abs() < 1e-12);
        // Sag includes battery + ESC + phase lead drops.
        let bat = &a.runs[0];
        let esc = a.runs.iter().find(|r| r.kind == RunKind::EscPower).unwrap();
        let expected_cd =
            bat.full.voltage_drop_v + esc.full.voltage_drop_v + phase.full.voltage_drop_v;
        assert!((a.sag.cable_drop_full_v - expected_cd).abs() < 1e-12);
    }

    #[test]
    fn invalid_linear_bec_and_vtx_surface_fail_status() {
        let (_, mut s) = src_for(DroneType::Multirotor);
        s.v_pack = 7.4;
        let mut c = Config::default();
        c.bec.kind = BecKind::Linear;
        c.bec.v_out = 12.0;
        c.vtx.p_in_w = 1.0;
        c.vtx.p_rf_w = 2.0;
        let a = analyze(&s, &c);
        assert!(a.bec_invalid);
        assert!(a.vtx_invalid);
        assert_eq!(a.bec.full.status, HeatStatus::Fail);
        assert_eq!(a.vtx.full.status, HeatStatus::Fail);
    }

    #[test]
    fn lihv_uses_lipo_resistance_constant() {
        assert_eq!(k_mohm_ah("LiHV"), k_mohm_ah("LiPo"));
    }

    #[test]
    fn hostile_inputs_do_not_produce_nan() {
        let (_, s) = src_for(DroneType::VtolQuadplane);
        for bad in [f64::NAN, -5.0, 0.0, 1e12, f64::INFINITY] {
            let mut c = Config::default();
            c.ambient_c = bad;
            c.air_speed_ms = bad;
            c.esc.eta = bad;
            c.esc.r_on_mohm = bad;
            c.esc.sw_frac = bad;
            c.bec.i_load_a = bad;
            c.vtx.p_in_w = bad;
            c.battery_lead.length_m = bad;
            c.esc_leads.awg = 0;
            c.phase_leads.awg = 999;
            c.max_drop_pct = bad;
            c.max_rise_k = bad;
            let a = analyze(&s, &c);
            assert_analysis_finite(&a, &format!("config bad={bad}"));
        }
        let mut s2 = s;
        s2.i_low_total = f64::NEG_INFINITY;
        s2.i_full_total = f64::NAN;
        s2.v_pack = f64::INFINITY;
        s2.r_pack_ohm = f64::INFINITY;
        if let Some(ref mut g) = s2.lift {
            g.i_low_esc = f64::NAN;
            g.duty_low = 0.0;
        }
        if let Some(ref mut g) = s2.forward {
            g.i_full_esc = f64::INFINITY;
            g.duty_full = f64::NAN;
        }
        let a2 = analyze(&s2, &Config::default());
        assert_analysis_finite(&a2, "hostile PowerSource");
    }
}
