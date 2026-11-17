// SPDX-License-Identifier: AGPL-3.0-or-later
//! Builds the motor / propeller / battery model from the project and its evaluated model.
//!
//! The project has no motor datasheet fields (KV, resistance, no-load current), so a default
//! motor is *derived* so that it reproduces the project's own static design point: the full
//! throttle static thrust and the propeller diameter, figure of merit, motor and ESC
//! efficiencies and pack voltage come from the existing inputs, and the motor constants follow
//! from them (see [`derive_motor`]). Every derived constant can be overridden with
//! [`Overrides`] when the user knows the real hardware.

use super::model::{
    COPPER_ALPHA, COPPER_LOSS_SHARE, Esc, Motor, Pack, PropUnit, Propeller, T_LIMIT_C,
    rth_still_air,
};
use crate::calc::Model;
use crate::calc::custom_battery::BatterySourceKind;
use crate::project::{DroneType, Project};
use std::f64::consts::PI;

/// Pitch to diameter ratio assumed for multirotor propellers (the project stores no pitch).
pub const MR_PITCH_RATIO: f64 = 0.5;
/// Blade count assumed for the forward-flight propeller (the project stores none).
pub const FW_BLADES: u32 = 2;
/// Pack sag at the cell's continuous current limit, as a fraction of nominal cell voltage,
/// used when no cell internal resistance is known.
pub const CELL_SAG_AT_MAX: f64 = 0.10;
/// Tip Mach number above which a multirotor rotor is flagged. The project's own limit
/// (`m_tip_max`) is a fixed-wing propeller input; small multirotor rotors routinely run faster.
pub const MR_TIP_MACH_LIMIT: f64 = 0.70;
/// Winding temperature assumed for the first pass of the default-motor derivation [degC].
const DESIGN_WINDING_C: f64 = 60.0;
/// Multirotor full-throttle static thrust per motor when the project gives none, as a multiple
/// of the hover thrust.
pub const FALLBACK_TW: f64 = 2.0;

/// Which propulsion group an override or a curve refers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Group {
    /// Lift rotors of a multirotor or the lift motors of a VTOL.
    #[default]
    Lift,
    /// Forward-flight propeller of a fixed wing or the pusher of a VTOL.
    Forward,
}

/// User overrides of the derived motor constants (`None` keeps the derived value).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Overrides {
    pub kv: Option<f64>,
    pub r_mohm: Option<f64>,
    pub i0_a: Option<f64>,
    pub motor_i_max_a: Option<f64>,
    pub esc_i_max_a: Option<f64>,
    pub t_limit_c: Option<f64>,
}

/// Everything the curve and flight-mode code needs.
#[derive(Clone, Debug)]
pub struct Setup {
    pub drone_type: DroneType,
    /// Lift rotors (multirotor, VTOL).
    pub lift: Option<PropUnit>,
    /// Forward-flight propeller (all fixed-wing types, VTOL).
    pub forward: Option<PropUnit>,
}

impl Setup {
    pub fn unit(&self, g: Group) -> Option<&PropUnit> {
        match g {
            Group::Lift => self.lift.as_ref(),
            Group::Forward => self.forward.as_ref(),
        }
    }

    /// Groups that exist for this vehicle, lift first.
    pub fn groups(&self) -> Vec<Group> {
        let mut v = Vec::new();
        if self.lift.is_some() {
            v.push(Group::Lift);
        }
        if self.forward.is_some() {
            v.push(Group::Forward);
        }
        v
    }
}

/// Inputs of the motor derivation for one group.
struct UnitSpec {
    d_prop: f64,
    pitch: f64,
    blades: u32,
    fm: f64,
    eta_mot: f64,
    eta_esc: f64,
    /// Full-throttle static thrust per motor [N].
    t_max_n: f64,
    n_motors: u32,
    rho: f64,
    pack: Pack,
    rpm_limit: f64,
    /// Default current rating when the project gives one (installed electric power / voltage).
    i_rating: Option<f64>,
}

/// Pack model from the evaluated project: nominal voltage, capacity and the cell/pack current
/// limit are the existing values; the internal resistance comes from the custom cell when one
/// is entered, otherwise from a nominal sag of [`CELL_SAG_AT_MAX`] at the cell current limit.
fn pack_from(p: &Project, m: &Model) -> Pack {
    let v_oc = m.weight.pack_voltage.max(1.0);
    let max_cont = m.prop_in.pack_max_cont_a.max(1.0);
    let cell = &m.cell;
    let n_s = (v_oc / cell.nominal_v.max(0.1)).round().max(1.0);
    let n_p = (max_cont / cell.max_cont_a.max(0.1)).round().max(1.0);
    let r_cell = match p.battery_source {
        BatterySourceKind::CustomCell if p.custom_cell.r_int_mohm > 0.0 => {
            p.custom_cell.r_int_mohm / 1000.0
        }
        _ => CELL_SAG_AT_MAX * cell.nominal_v / cell.max_cont_a.max(0.1),
    };
    Pack {
        v_oc,
        r_ohm: n_s * r_cell / n_p,
        capacity_ah: m.weight.pack_capacity_ah,
        max_cont_a: max_cont,
    }
}

/// Derive a default motor that reproduces the static design point.
///
/// At full throttle the propeller needs `P_shaft` at `n_max` (from `T_max = CT n^2 D^4 rho`).
/// The motor input power is `P_shaft / eta_mot`; the loss is split into copper
/// ([`COPPER_LOSS_SHARE`]) and iron/no-load shares, which fixes `R` and `I0`; the back-EMF at
/// that point then fixes `KV`. Two passes account for the pack sag. The resistance is a hot
/// value at the winding temperature of the design point; `hot_factor` converts it to 20 degC.
fn derive_motor(spec: &UnitSpec, prop: &Propeller, hot_factor: f64) -> Motor {
    let n_m = spec.n_motors.max(1) as f64;
    let eta_mot = spec.eta_mot.clamp(0.3, 0.99);
    let eta_esc = spec.eta_esc.clamp(0.3, 1.0);
    let n_max = (spec.t_max_n.max(1e-6) / (prop.ct0 * spec.rho * spec.d_prop.powi(4))).sqrt();
    let p_shaft = prop.cp0 * spec.rho * n_max.powi(3) * spec.d_prop.powi(5);
    let p_in = p_shaft / eta_mot;
    let loss = p_in - p_shaft;
    let mut v_m = spec.pack.v_oc;
    let mut i_m = p_in / v_m;
    for _ in 0..4 {
        let i_bat = i_m / eta_esc;
        v_m = (spec.pack.v_oc - n_m * i_bat * spec.pack.r_ohm).max(0.3 * spec.pack.v_oc);
        i_m = p_in / v_m;
    }
    // `r` is the hot design resistance; the stored 20 degC value is lower by `hot_factor`.
    let r = COPPER_LOSS_SHARE * loss / (i_m * i_m);
    let emf = (v_m - i_m * r).max(0.1);
    let i0 = (1.0 - COPPER_LOSS_SHARE) * loss / emf;
    let kv = 60.0 * n_max / emf;
    let i_rating = spec.i_rating.unwrap_or(i_m).max(i_m * 0.5);
    Motor {
        kv,
        r_ohm: r / hot_factor.max(1.0),
        i0_a: i0,
        i_max_a: i_rating,
        rth_still: rth_still_air(p_in),
        t_limit_c: T_LIMIT_C,
    }
}

fn build_unit(spec: UnitSpec, ov: &Overrides) -> PropUnit {
    let prop = Propeller::new(spec.d_prop, spec.pitch, spec.blades, spec.fm);
    let esc = Esc {
        eta: spec.eta_esc.clamp(0.3, 1.0),
        i_max_a: 0.0,
    };
    let make = |motor: Motor| PropUnit {
        prop: prop.clone(),
        motor,
        esc: esc.clone(),
        pack: spec.pack.clone(),
        n_motors: spec.n_motors.max(1),
        rho: spec.rho,
        rpm_limit: spec.rpm_limit,
    };
    // The design point is a hot motor: iterate the winding temperature so that the full
    // throttle static thrust of the finished model matches the design thrust.
    let mut hot = 1.0 + COPPER_ALPHA * (DESIGN_WINDING_C - 20.0);
    let mut motor = derive_motor(&spec, &prop, hot);
    for _ in 0..4 {
        let temp = make(motor.clone()).operating_point(1.0, 0.0).temp_c;
        hot = 1.0 + COPPER_ALPHA * (temp - 20.0);
        motor = derive_motor(&spec, &prop, hot);
    }
    if let Some(v) = ov.kv.filter(|v| *v > 1.0) {
        motor.kv = v;
    }
    if let Some(v) = ov.r_mohm.filter(|v| *v > 0.0) {
        motor.r_ohm = v / 1000.0;
    }
    if let Some(v) = ov.i0_a.filter(|v| *v >= 0.0) {
        motor.i0_a = v;
    }
    if let Some(v) = ov.motor_i_max_a.filter(|v| *v > 0.0) {
        motor.i_max_a = v;
    }
    if let Some(v) = ov.t_limit_c.filter(|v| *v > 30.0) {
        motor.t_limit_c = v;
    }
    let mut unit = make(motor);
    unit.esc.i_max_a = ov
        .esc_i_max_a
        .filter(|v| *v > 0.0)
        .unwrap_or(unit.motor.i_max_a);
    unit
}

/// Rotation speed at a tip Mach limit: the existing static-thrust expression
/// `rpm = 60 M_tip a / (pi D)`.
fn tip_rpm(m: &Model, mach: f64, d: f64) -> f64 {
    60.0 * mach * m.atmo.a / (PI * d.max(1e-3))
}

/// Build the model for the project's vehicle type. `ov` holds one override set per group
/// (`[lift, forward]`).
pub fn build(p: &Project, m: &Model, ov: &[Overrides; 2]) -> Setup {
    let pack = pack_from(p, m);
    let rho = m.atmo.rho.max(0.1);
    let mut lift = None;
    let mut forward = None;
    if p.drone_type.is_multirotor() {
        let n = m.mr.n_motors.max(1);
        let t_hover = m.mr.t_hover_motor.max(1e-3);
        let t_max = if p.mr_t_max_motor > 0.0 {
            p.mr_t_max_motor
        } else {
            FALLBACK_TW * t_hover
        };
        // A coaxial pair is modelled through a lower effective figure of merit so that the
        // static power matches the existing coaxial interference factor:
        // existing per-motor power / single-rotor power = kappa * sqrt(2).
        let fm_eff = if p.mr_coaxial {
            p.fm / (m.mr.coax_kappa.max(1.0) * 2.0_f64.sqrt())
        } else {
            p.fm
        };
        let d = p.mr_d_prop.max(1e-3);
        lift = Some(build_unit(
            UnitSpec {
                d_prop: d,
                pitch: MR_PITCH_RATIO * d,
                blades: p.mr_prop_blades,
                fm: fm_eff,
                eta_mot: p.eta_mot,
                eta_esc: p.eta_esc,
                t_max_n: t_max,
                n_motors: n,
                rho,
                pack: pack.clone(),
                rpm_limit: tip_rpm(m, MR_TIP_MACH_LIMIT, d),
                i_rating: None,
            },
            &ov[0],
        ));
    }
    if p.drone_type.is_fixed_wing() {
        let d = p.d_prop.max(1e-3);
        forward = Some(build_unit(
            UnitSpec {
                d_prop: d,
                pitch: p.prop_pitch,
                blades: FW_BLADES,
                fm: p.fm,
                eta_mot: p.eta_mot,
                eta_esc: p.eta_esc,
                t_max_n: m.prop.t_static.max(1e-3),
                n_motors: 1,
                rho,
                pack: pack.clone(),
                rpm_limit: tip_rpm(m, p.m_tip_max, d),
                i_rating: Some(p.p_max_elec / pack.v_oc),
            },
            &ov[1],
        ));
    }
    Setup {
        drone_type: p.drone_type,
        lift,
        forward,
    }
}
