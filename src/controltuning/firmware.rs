// SPDX-License-Identifier: AGPL-3.0-or-later
//! Firmware-specific rate-loop models, gain scaling and parameter export for Betaflight,
//! ArduPilot (Copter / QuadPlane) and PX4 (multicopter / VTOL).
//!
//! # From physical gains to firmware parameters
//!
//! The loop design (`loopdesign`) works with physical gains: rate error in rad/s, output the
//! firmware's normalised axis command `u`. What one unit of `u` does physically depends on each
//! firmware's mixer and thrust model; that is folded into the plant gain `K` (N·m per unit `u`),
//! so the designed `Kp, Ki, Kd` are already in each firmware's command units and only the error
//! units / internal scale constants remain:
//!
//! **ArduPilot** (`AC_AttitudeControl_Multi`, `AC_PID`, `AP_MotorsMatrix`):
//! the rate PID takes the error in rad/s and outputs a roll/pitch/yaw command in −1…1
//! (`ATC_RAT_RLL_P` etc. are therefore used directly). `AP_MotorsMatrix::normalise_rpy_factors`
//! scales each axis' motor factors so the largest is ±0.5, and the output is a fraction of
//! maximum thrust after `MOT_THST_EXPO` linearisation, so `dT/du = T_max`. Battery/air-density
//! compensation (≈ 1 near nominal voltage) is ignored. This mapping is the most direct of the
//! three. Filter starting points follow the ArduPilot "Initial Tuning Parameters" helper of
//! Mission Planner / wiki ("Setting up for Tuning"): `INS_GYRO_FILTER ≈ 289.22·D_in^−0.838` Hz,
//! `ATC_RAT_*_FLTD = FLTT = INS_GYRO_FILTER / 2`, yaw `FLTE = 2` Hz; harmonic notch per the wiki
//! "Harmonic Notch" page (throttle-based: `INS_HNTCH_FREQ` = hover motor frequency,
//! `INS_HNTCH_BW` = half of it, `INS_HNTCH_REF` = hover thrust).
//!
//! **Betaflight** (`src/main/flight/pid.h`, `pid.c`, `mixer.c`): the error is in deg/s and the
//! terms are scaled by `PTERM_SCALE = 0.032029`, `ITERM_SCALE = 0.244381`,
//! `DTERM_SCALE = 0.000529` (D on the gyro derivative in deg/s²); the PID sum is divided by
//! `PID_MIXER_SCALING = 1000` before the mixer, whose factors are ±1 for the largest arm
//! coordinate (e.g. quad X ±1.0, hex X 0.5/0.866/1.0). Hence
//! `p_roll = Kp·1000 / (0.032029·180/π)` and likewise for I and D. Motor output ≈ throttle and
//! static thrust ∝ throttle², so `dT/du ≈ 2·√(T_hover·T_max)` at hover. Feed-forward (`f_*`)
//! acts on the stick's rate of change and only shapes stick response; its scaling is not
//! reproduced here, so it is left at the firmware default.
//!
//! **PX4** (`mc_rate_control` / `RateControl`, `control_allocator`): the rate controller takes
//! rad/s and outputs a normalised torque setpoint; with `MC_*RATE_K = 1` the gains are used
//! directly. The allocator is the pseudo-inverse of the rotor effectiveness matrix, normalised in
//! `ControlAllocationPseudoInverse::updateControlAllocationMatrixScale` (roll/pitch columns by
//! `√(‖col‖² / (n_nonzero/2))`, yaw column by its maximum). That normalisation is reproduced
//! approximately; with the default `THR_MDL_FAC = 0` thrust ∝ output², so
//! `dT/du ≈ 2·√(T_hover·T_max)`. Expect roughly ±30 % uncertainty on the PX4 scaling.
//!
//! Loop delays assumed: Betaflight 4 kHz PID loop + DShot (≈ 0.7 ms), ArduPilot and PX4 400 Hz
//! rate loop + ESC (≈ 4.75 ms).
use super::loopdesign::{DesignOpts, Filter, Gains, LoopFilters, LoopMetrics, Plant, design};
use super::vehicle::{VehicleData, VehicleKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Firmware {
    Betaflight,
    ArduPilot,
    Px4,
}

impl Firmware {
    pub const ALL: [Firmware; 3] = [Firmware::Betaflight, Firmware::ArduPilot, Firmware::Px4];
    pub fn name(self) -> &'static str {
        match self {
            Firmware::Betaflight => "Betaflight",
            Firmware::ArduPilot => "ArduPilot",
            Firmware::Px4 => "PX4",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Roll,
    Pitch,
    Yaw,
}

impl Axis {
    pub const ALL: [Axis; 3] = [Axis::Roll, Axis::Pitch, Axis::Yaw];
    pub fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Confidence {
    High,
    Medium,
    Low,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ParamValue {
    Num(f64, usize),
    Text(&'static str),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub name: String,
    pub value: ParamValue,
}

impl Param {
    pub fn num(name: impl Into<String>, v: f64, dec: usize) -> Self {
        Param {
            name: name.into(),
            value: ParamValue::Num(if v.is_finite() { v } else { 0.0 }, dec),
        }
    }
    pub fn text(name: impl Into<String>, v: &'static str) -> Self {
        Param {
            name: name.into(),
            value: ParamValue::Text(v),
        }
    }
    pub fn value_string(&self) -> String {
        match &self.value {
            ParamValue::Num(v, d) => format!("{:.*}", *d, v),
            ParamValue::Text(s) => s.to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AxisTune {
    pub axis: Axis,
    /// Moment per unit firmware command [N·m].
    pub k_axis: f64,
    pub inertia: f64,
    pub plant: Plant,
    pub filters: LoopFilters,
    pub result: Option<(Gains, LoopMetrics)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FilterPlan {
    pub gyro_hz: f64,
    pub dterm_hz: f64,
    pub notch_min_hz: f64,
    pub notch_max_hz: f64,
    pub hover_motor_hz: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FirmwareTune {
    pub firmware: Firmware,
    pub applicable: bool,
    pub axes: Vec<AxisTune>,
    pub filters: FilterPlan,
    pub params: Vec<Param>,
    pub confidence: Confidence,
    /// Parameters clamped to the firmware's allowed range.
    pub clamped: Vec<String>,
}

const DEG: f64 = 180.0 / std::f64::consts::PI;
/// Betaflight `pid.h` scale constants.
pub const BF_PTERM_SCALE: f64 = 0.032029;
pub const BF_ITERM_SCALE: f64 = 0.244381;
pub const BF_DTERM_SCALE: f64 = 0.000529;
/// Betaflight `mixer.c`: PID sum → mixer input.
pub const BF_PID_MIXER_SCALING: f64 = 1000.0;

fn loop_delay(fw: Firmware) -> f64 {
    match fw {
        Firmware::Betaflight => 1.5 / 4000.0 + 0.0003,
        Firmware::ArduPilot | Firmware::Px4 => 1.5 / 400.0 + 0.001,
    }
}

/// ArduPilot "Initial Tuning Parameters" gyro filter vs prop diameter in inches.
pub fn reference_gyro_hz(d_prop_m: f64) -> f64 {
    let d_in = (d_prop_m / 0.0254).max(1.0);
    (289.22 * d_in.powf(-0.838)).clamp(20.0, 150.0)
}

fn filter_plan(fw: Firmware, v: &VehicleData) -> FilterPlan {
    let f_ref = reference_gyro_hz(v.d_prop);
    let (gyro_hz, dterm_hz) = match fw {
        Firmware::Betaflight => {
            let g = (3.3 * f_ref).clamp(80.0, 500.0);
            (g, (0.4 * g).clamp(40.0, 200.0))
        }
        Firmware::ArduPilot => (f_ref, 0.5 * f_ref),
        Firmware::Px4 => (f_ref, 0.75 * f_ref),
    };
    let f_hover = v.hover_rpm / 60.0;
    let f_max = v.max_rpm / 60.0;
    FilterPlan {
        gyro_hz,
        dterm_hz,
        notch_min_hz: (0.6 * f_hover).max(20.0),
        notch_max_hz: (1.1 * f_max).max(0.6 * f_hover + 50.0),
        hover_motor_hz: f_hover,
    }
}

fn loop_filters(fw: Firmware, fp: &FilterPlan, axis: Axis) -> LoopFilters {
    match fw {
        Firmware::Betaflight => LoopFilters {
            gyro: vec![Filter::pt1(fp.gyro_hz), Filter::pt1(2.0 * fp.gyro_hz)],
            dterm: vec![Filter::pt1(fp.dterm_hz), Filter::pt1(1.5 * fp.dterm_hz)],
        },
        Firmware::ArduPilot => {
            let mut gyro = vec![Filter::biquad(fp.gyro_hz)];
            if axis == Axis::Yaw {
                // ATC_RAT_YAW_FLTE filters the yaw error inside the loop.
                gyro.push(Filter::pt1(AP_YAW_FLTE));
            }
            LoopFilters {
                gyro,
                dterm: vec![Filter::pt1(fp.dterm_hz)],
            }
        }
        Firmware::Px4 => LoopFilters {
            gyro: vec![Filter::biquad(fp.gyro_hz)],
            dterm: vec![Filter::biquad(fp.dterm_hz)],
        },
    }
}

const AP_YAW_FLTE: f64 = 2.0;

/// Solves `A·x = b` for a small dense system (Gauss–Jordan, partial pivoting).
fn solve_small(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for c in 0..n {
        let piv = (c..n).max_by(|&i, &j| a[i][c].abs().total_cmp(&a[j][c].abs()))?;
        if a[piv][c].abs() < 1e-12 {
            return None;
        }
        a.swap(c, piv);
        b.swap(c, piv);
        for r in 0..n {
            if r != c {
                let f = a[r][c] / a[c][c];
                let pivot_row = a[c].clone();
                for (x, y) in a[r][c..].iter_mut().zip(&pivot_row[c..]) {
                    *x -= f * y;
                }
                b[r] -= f * b[c];
            }
        }
    }
    Some((0..n).map(|i| b[i] / a[i][i]).collect())
}

/// Per-motor mixer factors (roll, pitch, yaw) so that a positive unit command produces a
/// positive moment, as a fraction of the motor's output range.
pub fn mixer_factors(fw: Firmware, v: &VehicleData) -> Option<Vec<[f64; 3]>> {
    let r = &v.rotors;
    if r.is_empty() {
        return None;
    }
    let n = r.len() as f64;
    let mx = r.iter().map(|q| q.x).sum::<f64>() / n;
    let my = r.iter().map(|q| q.y).sum::<f64>() / n;
    match fw {
        Firmware::Betaflight => {
            let c = r
                .iter()
                .map(|q| (q.x - mx).abs().max((q.y - my).abs()))
                .fold(0.0, f64::max)
                .max(1e-9);
            Some(
                r.iter()
                    .map(|q| [-(q.y - my) / c, (q.x - mx) / c, q.spin])
                    .collect(),
            )
        }
        Firmware::ArduPilot => {
            let cy = r
                .iter()
                .map(|q| (q.y - my).abs())
                .fold(0.0, f64::max)
                .max(1e-9);
            let cx = r
                .iter()
                .map(|q| (q.x - mx).abs())
                .fold(0.0, f64::max)
                .max(1e-9);
            Some(
                r.iter()
                    .map(|q| [-0.5 * (q.y - my) / cy, 0.5 * (q.x - mx) / cx, 0.5 * q.spin])
                    .collect(),
            )
        }
        Firmware::Px4 => {
            // Effectiveness rows: roll, pitch, yaw, thrust (per unit thrust).
            let rows: Vec<Vec<f64>> = vec![
                r.iter().map(|q| -q.y).collect(),
                r.iter().map(|q| q.x).collect(),
                r.iter().map(|q| q.spin * v.kq).collect(),
                r.iter().map(|_| 1.0).collect(),
            ];
            let bbt: Vec<Vec<f64>> = (0..4)
                .map(|i| {
                    (0..4)
                        .map(|j| rows[i].iter().zip(&rows[j]).map(|(a, b)| a * b).sum())
                        .collect()
                })
                .collect();
            // mix = Bᵀ (B Bᵀ)⁻¹; column k = Bᵀ · solve(BBᵀ, e_k).
            let mut cols = Vec::new();
            for k in 0..3 {
                let mut e = vec![0.0; 4];
                e[k] = 1.0;
                let z = solve_small(bbt.clone(), e)?;
                let col: Vec<f64> = (0..r.len())
                    .map(|i| (0..4).map(|j| rows[j][i] * z[j]).sum())
                    .collect();
                cols.push(col);
            }
            let rp_scale = |col: &Vec<f64>| {
                let nz = col.iter().filter(|v| v.abs() > 1e-3 * col_max(col)).count();
                let n2: f64 = col.iter().map(|v| v * v).sum();
                if nz > 0 {
                    (n2 / (nz as f64 / 2.0)).sqrt()
                } else {
                    1.0
                }
            };
            let s_rp = rp_scale(&cols[0]).max(rp_scale(&cols[1])).max(1e-12);
            let s_y = col_max(&cols[2]).max(1e-12);
            Some(
                (0..r.len())
                    .map(|i| [cols[0][i] / s_rp, cols[1][i] / s_rp, cols[2][i] / s_y])
                    .collect(),
            )
        }
    }
}

fn col_max(c: &[f64]) -> f64 {
    c.iter().map(|v| v.abs()).fold(0.0, f64::max)
}

/// Thrust gradient per unit output [N] at hover.
fn thrust_gradient(fw: Firmware, v: &VehicleData, t_max: f64) -> f64 {
    match fw {
        Firmware::ArduPilot => t_max,
        Firmware::Betaflight | Firmware::Px4 => {
            2.0 * (v.hover_frac.clamp(0.05, 1.0) * t_max * t_max).sqrt()
        }
    }
}

/// Moment per unit command [N·m] for each axis.
pub fn axis_gains(fw: Firmware, v: &VehicleData) -> Option<[f64; 3]> {
    let f = mixer_factors(fw, v)?;
    let mut k = [0.0; 3];
    for (q, fi) in v.rotors.iter().zip(&f) {
        let dtdu = thrust_gradient(fw, v, q.t_max);
        k[0] += fi[0] * (-q.y) * dtdu;
        k[1] += fi[1] * q.x * dtdu;
        k[2] += fi[2] * q.spin * v.kq * dtdu;
    }
    Some(k.map(|x| if x.is_finite() { x.max(0.0) } else { 0.0 }))
}

fn tune_axes(fw: Firmware, v: &VehicleData, fp: &FilterPlan) -> Vec<AxisTune> {
    let k = axis_gains(fw, v).unwrap_or([0.0; 3]);
    let max_wc = 2.0 * std::f64::consts::PI * (fp.gyro_hz / 2.5).min(40.0);
    let mut out: Vec<AxisTune> = Vec::new();
    for axis in Axis::ALL {
        let i = axis.index();
        let j = v.inertia[i];
        let plant = Plant {
            k_over_j: if j > 0.0 { k[i] / j } else { 0.0 },
            tau: v.motor_tau,
            delay: loop_delay(fw),
        };
        let filters = loop_filters(fw, fp, axis);
        let (max_kp, max_ki, max_kd) = gain_limits(fw, axis);
        let mut opts = DesignOpts {
            max_crossover: max_wc,
            max_kp,
            max_ki,
            max_kd,
            ..Default::default()
        };
        if axis == Axis::Yaw {
            opts.use_d = false;
            // Yaw is usually tuned softer than roll/pitch (drag-torque authority saturates).
            let rp = out
                .iter()
                .filter_map(|a| a.result.as_ref())
                .map(|(_, m)| m.crossover_hz)
                .fold(f64::INFINITY, f64::min);
            if rp.is_finite() {
                opts.max_crossover = 2.0 * std::f64::consts::PI * 0.6 * rp;
            }
        }
        let result = design(&plant, &filters, &opts);
        out.push(AxisTune {
            axis,
            k_axis: k[i],
            inertia: j,
            plant,
            filters,
            result,
        });
    }
    out
}

/// Parameter ranges as physical gains (P, I, D). Betaflight: 0…250 for every term. ArduPilot:
/// documented ranges of `ATC_RAT_*` (roll/pitch P ≤ 0.5, I ≤ 2, D ≤ 0.05; yaw P ≤ 2.5, I ≤ 1).
/// PX4: `MC_*RATE_P` ≤ 0.5 (yaw 0.6), `MC_*RATE_D` ≤ 0.01; I capped at 1.5 as a sanity bound.
fn gain_limits(fw: Firmware, axis: Axis) -> (f64, f64, f64) {
    let yaw = axis == Axis::Yaw;
    match fw {
        Firmware::Betaflight => {
            let s = DEG / BF_PID_MIXER_SCALING * 250.0;
            (s * BF_PTERM_SCALE, s * BF_ITERM_SCALE, s * BF_DTERM_SCALE)
        }
        Firmware::ArduPilot if yaw => (2.5, 1.0, 0.05),
        Firmware::ArduPilot => (0.5, 2.0, 0.05),
        Firmware::Px4 if yaw => (0.6, 1.5, 0.01),
        Firmware::Px4 => (0.5, 1.5, 0.01),
    }
}

fn gains(axes: &[AxisTune], a: Axis) -> Gains {
    axes.get(a.index())
        .and_then(|x| x.result.as_ref())
        .map(|r| r.0)
        .unwrap_or_default()
}

fn clamp_param(
    name: &str,
    v: f64,
    lo: f64,
    hi: f64,
    dec: usize,
    clamped: &mut Vec<String>,
) -> Param {
    let c = v.clamp(lo, hi);
    if (c - v).abs() > 1e-12 {
        clamped.push(name.to_string());
    }
    Param::num(name, c, dec)
}

fn betaflight_params(
    v: &VehicleData,
    axes: &[AxisTune],
    fp: &FilterPlan,
) -> (Vec<Param>, Vec<String>) {
    let mut cl = Vec::new();
    let mut p = vec![
        Param::text("simplified_pids_mode", "OFF"),
        Param::text("simplified_dterm_filter", "OFF"),
        Param::text("simplified_gyro_filter", "OFF"),
    ];
    for (a, suffix) in [
        (Axis::Roll, "roll"),
        (Axis::Pitch, "pitch"),
        (Axis::Yaw, "yaw"),
    ] {
        let g = gains(axes, a);
        let s = BF_PID_MIXER_SCALING / DEG;
        p.push(clamp_param(
            &format!("p_{suffix}"),
            (g.kp * s / BF_PTERM_SCALE).round(),
            0.0,
            250.0,
            0,
            &mut cl,
        ));
        p.push(clamp_param(
            &format!("i_{suffix}"),
            (g.ki * s / BF_ITERM_SCALE).round(),
            0.0,
            250.0,
            0,
            &mut cl,
        ));
        p.push(clamp_param(
            &format!("d_{suffix}"),
            (g.kd * s / BF_DTERM_SCALE).round(),
            0.0,
            250.0,
            0,
            &mut cl,
        ));
    }
    p.push(Param::text("gyro_lpf1_type", "PT1"));
    p.push(Param::num("gyro_lpf1_dyn_min_hz", 0.0, 0));
    p.push(Param::num("gyro_lpf1_static_hz", fp.gyro_hz.round(), 0));
    p.push(Param::text("gyro_lpf2_type", "PT1"));
    p.push(Param::num(
        "gyro_lpf2_static_hz",
        (2.0 * fp.gyro_hz).min(1000.0).round(),
        0,
    ));
    p.push(Param::text("dterm_lpf1_type", "PT1"));
    p.push(Param::num("dterm_lpf1_dyn_min_hz", 0.0, 0));
    p.push(Param::num("dterm_lpf1_static_hz", fp.dterm_hz.round(), 0));
    p.push(Param::text("dterm_lpf2_type", "PT1"));
    p.push(Param::num(
        "dterm_lpf2_static_hz",
        (1.5 * fp.dterm_hz).round(),
        0,
    ));
    p.push(clamp_param(
        "dyn_notch_min_hz",
        fp.notch_min_hz.round(),
        60.0,
        250.0,
        0,
        &mut cl,
    ));
    p.push(clamp_param(
        "dyn_notch_max_hz",
        fp.notch_max_hz.round(),
        200.0,
        1000.0,
        0,
        &mut cl,
    ));
    p.push(clamp_param(
        "rpm_filter_min_hz",
        (0.5 * fp.hover_motor_hz).round(),
        30.0,
        200.0,
        0,
        &mut cl,
    ));
    let _ = v;
    (p, cl)
}

/// MOT_THST_EXPO starting value vs prop size (larger props → more curvature). Heuristic fit of
/// the ArduPilot initial-parameter helper; verify with a thrust stand if possible.
pub fn ap_thrust_expo(d_prop_m: f64) -> f64 {
    let d_in = (d_prop_m / 0.0254).max(1.0);
    (0.15686 * d_in.ln() + 0.23693).clamp(0.2, 0.8)
}

fn ardupilot_params(
    v: &VehicleData,
    axes: &[AxisTune],
    fp: &FilterPlan,
) -> (Vec<Param>, Vec<String>) {
    let mut cl = Vec::new();
    let vtol = v.kind == VehicleKind::Vtol;
    let atc = if vtol { "Q_A_" } else { "ATC_" };
    let mot = if vtol { "Q_M_" } else { "MOT_" };
    let mut p = Vec::new();
    for (a, ax) in [
        (Axis::Roll, "RLL"),
        (Axis::Pitch, "PIT"),
        (Axis::Yaw, "YAW"),
    ] {
        let g = gains(axes, a);
        let (hp, hi, hd) = gain_limits(Firmware::ArduPilot, a);
        p.push(clamp_param(
            &format!("{atc}RAT_{ax}_P"),
            g.kp,
            0.0,
            hp,
            4,
            &mut cl,
        ));
        p.push(clamp_param(
            &format!("{atc}RAT_{ax}_I"),
            g.ki,
            0.0,
            hi,
            4,
            &mut cl,
        ));
        p.push(clamp_param(
            &format!("{atc}RAT_{ax}_D"),
            g.kd,
            0.0,
            hd,
            5,
            &mut cl,
        ));
        p.push(Param::num(format!("{atc}RAT_{ax}_FF"), 0.0, 1));
        p.push(Param::num(
            format!("{atc}RAT_{ax}_FLTT"),
            (0.5 * fp.gyro_hz).round(),
            0,
        ));
        if a == Axis::Yaw {
            p.push(Param::num(format!("{atc}RAT_{ax}_FLTE"), AP_YAW_FLTE, 0));
            p.push(Param::num(format!("{atc}RAT_{ax}_FLTD"), 0.0, 0));
        } else {
            p.push(Param::num(
                format!("{atc}RAT_{ax}_FLTD"),
                fp.dterm_hz.round(),
                0,
            ));
        }
    }
    // Angular acceleration limits: half of the attainable acceleration about hover.
    let u_avail = (v.hover_frac.min(1.0 - v.hover_frac).max(0.05)) / 0.5;
    for (a, name, lo, hi) in [
        (Axis::Roll, "ACCEL_R_MAX", 10000.0, 180000.0),
        (Axis::Pitch, "ACCEL_P_MAX", 10000.0, 180000.0),
        (Axis::Yaw, "ACCEL_Y_MAX", 4000.0, 72000.0),
    ] {
        let at = &axes[a.index()];
        let acc = 0.5 * at.plant.k_over_j * u_avail * DEG * 100.0;
        p.push(clamp_param(
            &format!("{atc}{name}"),
            (acc / 1000.0).round() * 1000.0,
            lo,
            hi,
            0,
            &mut cl,
        ));
    }
    p.push(Param::num(
        format!("{mot}THST_HOVER"),
        v.hover_frac.clamp(0.125, 0.6875),
        3,
    ));
    p.push(Param::num(
        format!("{mot}THST_EXPO"),
        ap_thrust_expo(v.d_prop),
        2,
    ));
    p.push(Param::num("INS_GYRO_FILTER", fp.gyro_hz.round(), 0));
    p.push(Param::num("INS_HNTCH_ENABLE", 1.0, 0));
    p.push(Param::num("INS_HNTCH_MODE", 1.0, 0));
    p.push(Param::num("INS_HNTCH_REF", v.hover_frac.clamp(0.1, 0.9), 3));
    p.push(Param::num(
        "INS_HNTCH_FREQ",
        fp.hover_motor_hz.max(10.0).round(),
        0,
    ));
    p.push(Param::num(
        "INS_HNTCH_BW",
        (0.5 * fp.hover_motor_hz).max(5.0).round(),
        0,
    ));
    p.push(Param::num("INS_HNTCH_ATT", 40.0, 0));
    p.push(Param::num("INS_HNTCH_HMNCS", 3.0, 0));
    (p, cl)
}

fn px4_params(v: &VehicleData, axes: &[AxisTune], fp: &FilterPlan) -> (Vec<Param>, Vec<String>) {
    let mut cl = Vec::new();
    let mut p = Vec::new();
    for (a, ax) in [
        (Axis::Roll, "ROLLRATE"),
        (Axis::Pitch, "PITCHRATE"),
        (Axis::Yaw, "YAWRATE"),
    ] {
        let g = gains(axes, a);
        p.push(Param::num(format!("MC_{ax}_K"), 1.0, 1));
        let (hp, hi, hd) = gain_limits(Firmware::Px4, a);
        p.push(clamp_param(
            &format!("MC_{ax}_P"),
            g.kp,
            0.0,
            hp,
            4,
            &mut cl,
        ));
        p.push(clamp_param(
            &format!("MC_{ax}_I"),
            g.ki,
            0.0,
            hi,
            4,
            &mut cl,
        ));
        p.push(clamp_param(
            &format!("MC_{ax}_D"),
            g.kd,
            0.0,
            hd,
            5,
            &mut cl,
        ));
        p.push(Param::num(format!("MC_{ax}_FF"), 0.0, 1));
    }
    p.push(Param::num("IMU_GYRO_CUTOFF", fp.gyro_hz.round(), 0));
    p.push(Param::num("IMU_DGYRO_CUTOFF", fp.dterm_hz.round(), 0));
    p.push(Param::num("IMU_GYRO_FFT_EN", 1.0, 0));
    p.push(clamp_param(
        "IMU_GYRO_FFT_MIN",
        fp.notch_min_hz.round(),
        30.0,
        1000.0,
        0,
        &mut cl,
    ));
    p.push(clamp_param(
        "IMU_GYRO_FFT_MAX",
        fp.notch_max_hz.round(),
        40.0,
        1000.0,
        0,
        &mut cl,
    ));
    p.push(Param::num("IMU_GYRO_DNF_EN", 2.0, 0));
    p.push(Param::num(
        "IMU_GYRO_DNF_MIN",
        (0.5 * fp.hover_motor_hz).max(25.0).round(),
        0,
    ));
    p.push(Param::num(
        "MPC_THR_HOVER",
        v.hover_frac.sqrt().clamp(0.1, 0.8),
        2,
    ));
    (p, cl)
}

/// Rate-loop suggestions for one firmware. Betaflight does not support VTOL or fixed wing.
pub fn tune_firmware(fw: Firmware, v: &VehicleData) -> FirmwareTune {
    let fp = filter_plan(fw, v);
    let applicable =
        !v.rotors.is_empty() && !(fw == Firmware::Betaflight && v.kind != VehicleKind::Multirotor);
    if !applicable {
        return FirmwareTune {
            firmware: fw,
            applicable,
            axes: Vec::new(),
            filters: fp,
            params: Vec::new(),
            confidence: Confidence::Low,
            clamped: Vec::new(),
        };
    }
    let axes = tune_axes(fw, v, &fp);
    let (params, clamped, confidence) = match fw {
        Firmware::Betaflight => {
            let (p, c) = betaflight_params(v, &axes, &fp);
            (p, c, Confidence::Medium)
        }
        Firmware::ArduPilot => {
            let (p, c) = ardupilot_params(v, &axes, &fp);
            (p, c, Confidence::High)
        }
        Firmware::Px4 => {
            let (p, c) = px4_params(v, &axes, &fp);
            (p, c, Confidence::Low)
        }
    };
    FirmwareTune {
        firmware: fw,
        applicable,
        axes,
        filters: fp,
        params,
        confidence,
        clamped,
    }
}

/// Paste-able text: Betaflight CLI `set` lines, ArduPilot Mission Planner `.param`
/// (`NAME,value`), PX4 MAVLink-console `param set NAME value`. `#` lines are comments.
pub fn export_text(fw: Firmware, params: &[Param], header: &[String]) -> String {
    let mut s = String::new();
    for h in header {
        s.push_str("# ");
        s.push_str(h);
        s.push('\n');
    }
    for p in params {
        let v = p.value_string();
        match fw {
            Firmware::Betaflight => s.push_str(&format!("set {} = {}\n", p.name, v)),
            Firmware::ArduPilot => s.push_str(&format!("{},{}\n", p.name, v)),
            Firmware::Px4 => s.push_str(&format!("param set {} {}\n", p.name, v)),
        }
    }
    if fw == Firmware::Betaflight {
        s.push_str("save\n");
    }
    s
}

/// Parses an export back into (name, value) pairs; used by tests and as a self-check.
pub fn parse_export(fw: Firmware, text: &str) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') || (fw == Firmware::Betaflight && l == "save") {
            continue;
        }
        let (k, v) = match fw {
            Firmware::Betaflight => l
                .strip_prefix("set ")
                .and_then(|r| r.split_once('='))
                .ok_or_else(|| format!("bad line: {l}"))?,
            Firmware::ArduPilot => l.split_once(',').ok_or_else(|| format!("bad line: {l}"))?,
            Firmware::Px4 => l
                .strip_prefix("param set ")
                .and_then(|r| r.split_once(' '))
                .ok_or_else(|| format!("bad line: {l}"))?,
        };
        let (k, v) = (k.trim(), v.trim());
        if k.is_empty() || v.is_empty() || k.contains(char::is_whitespace) {
            return Err(format!("bad line: {l}"));
        }
        out.push((k.to_string(), v.to_string()));
    }
    Ok(out)
}
