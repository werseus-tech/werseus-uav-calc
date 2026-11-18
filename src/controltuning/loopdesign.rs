// SPDX-License-Identifier: AGPL-3.0-or-later
//! Single-axis rate-loop model and PID design by loop shaping.
//!
//! Plant (one body axis, linearised about hover):
//!
//! ```text
//!   ω(s) / u(s) = (K/J) / s · 1 / (τ·s + 1) · e^(−s·T_d)
//! ```
//!
//! `u` is the firmware's normalised axis command, `K` the moment per unit command [N·m]
//! (mixer factors × thrust gradient × moment arm), `J` the axis inertia, `τ` the motor + prop
//! time constant and `T_d` the pure delay of sampling, processing and the ESC protocol.
//! The gyro low-pass filters sit in the feedback path and the D-term low-pass filters on the
//! derivative path, as in Betaflight, ArduPilot and PX4.
//!
//! Controller (parallel form, D on measurement): `C(s) = Kp + Ki/s + Kd·s·F_D(s)`.
//!
//! Design: for a decreasing grid of target crossover frequencies ω_c and a few D lead times,
//! `Kp` is chosen so that |L(jω_c)| = 1 and `Ki = Kp·ω_c / 10` (integrator zero a decade
//! below crossover); the D lead time is limited to 0.45·(τ + T_d) so that D damps rather than
//! cancels the motor lag (large props are noisy); the fastest design meeting
//! phase margin ≥ 50° and gain margin ≥ 6 dB (evaluated on the full loop, all crossings) is
//! kept. This is classic frequency-domain loop shaping (Åström & Murray, *Feedback Systems*,
//! 2nd ed., ch. 10–12). The result is a conservative *starting point*, not an optimum.

/// Minimal complex number (no external dependency).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cx {
    pub re: f64,
    pub im: f64,
}

impl Cx {
    pub const fn new(re: f64, im: f64) -> Self {
        Cx { re, im }
    }
    pub fn abs(self) -> f64 {
        self.re.hypot(self.im)
    }
    pub fn cmul(self, o: Cx) -> Cx {
        Cx::new(
            self.re * o.re - self.im * o.im,
            self.re * o.im + self.im * o.re,
        )
    }
    pub fn cadd(self, o: Cx) -> Cx {
        Cx::new(self.re + o.re, self.im + o.im)
    }
    pub fn cdiv(self, o: Cx) -> Cx {
        let d = o.re * o.re + o.im * o.im;
        Cx::new(
            (self.re * o.re + self.im * o.im) / d,
            (self.im * o.re - self.re * o.im) / d,
        )
    }
    pub fn scale(self, k: f64) -> Cx {
        Cx::new(self.re * k, self.im * k)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterKind {
    /// First-order low-pass (PT1).
    Pt1,
    /// Second-order Butterworth low-pass (biquad, Q = 0.707).
    Biquad,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Filter {
    pub kind: FilterKind,
    pub hz: f64,
}

impl Filter {
    pub fn pt1(hz: f64) -> Self {
        Filter {
            kind: FilterKind::Pt1,
            hz,
        }
    }
    pub fn biquad(hz: f64) -> Self {
        Filter {
            kind: FilterKind::Biquad,
            hz,
        }
    }
    fn wf(&self) -> f64 {
        2.0 * std::f64::consts::PI * self.hz.max(0.1)
    }
    pub fn response(&self, w: f64) -> Cx {
        let wf = self.wf();
        match self.kind {
            FilterKind::Pt1 => Cx::new(1.0, 0.0).cdiv(Cx::new(1.0, w / wf)),
            FilterKind::Biquad => Cx::new(wf * wf, 0.0)
                .cdiv(Cx::new(wf * wf - w * w, std::f64::consts::SQRT_2 * wf * w)),
        }
    }
    /// Continuous phase [rad] (≤ 0).
    pub fn phase(&self, w: f64) -> f64 {
        let wf = self.wf();
        match self.kind {
            FilterKind::Pt1 => -(w / wf).atan(),
            FilterKind::Biquad => -(std::f64::consts::SQRT_2 * wf * w).atan2(wf * wf - w * w),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plant {
    /// K/J [rad/s² per unit command].
    pub k_over_j: f64,
    pub tau: f64,
    pub delay: f64,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct LoopFilters {
    pub gyro: Vec<Filter>,
    pub dterm: Vec<Filter>,
}

/// Physical gains: command per rad/s (P), per rad (I), per rad/s² (D, FF on setpoint
/// acceleration).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Gains {
    pub kp: f64,
    pub ki: f64,
    pub kd: f64,
    pub kff: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct LoopMetrics {
    pub crossover_hz: f64,
    pub phase_margin_deg: f64,
    /// Capped at 60 dB when the phase never reaches −180°.
    pub gain_margin_db: f64,
    /// Closed-loop −3 dB bandwidth of setpoint → rate.
    pub bandwidth_hz: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DesignOpts {
    pub use_d: bool,
    pub pm_target_deg: f64,
    pub gm_target_db: f64,
    /// Upper limit for the crossover [rad/s].
    pub max_crossover: f64,
    /// `Ki = Kp·ω_c / ki_div`.
    pub ki_div: f64,
    /// Firmware parameter limits expressed as physical gains; designs exceeding `max_kp` or
    /// `max_kd` are rejected (a slower design is tried), `Ki` is capped at `max_ki`.
    pub max_kp: f64,
    pub max_ki: f64,
    pub max_kd: f64,
}

impl Default for DesignOpts {
    fn default() -> Self {
        DesignOpts {
            use_d: true,
            pm_target_deg: 50.0,
            gm_target_db: 6.0,
            max_crossover: 2.0 * std::f64::consts::PI * 40.0,
            ki_div: 10.0,
            max_kp: f64::INFINITY,
            max_ki: f64::INFINITY,
            max_kd: f64::INFINITY,
        }
    }
}

fn controller(g: &Gains, f: &LoopFilters, w: f64) -> Cx {
    let mut d = Cx::new(0.0, g.kd * w);
    for flt in &f.dterm {
        d = d.cmul(flt.response(w));
    }
    Cx::new(g.kp, -g.ki / w).cadd(d)
}

/// Open-loop magnitude and continuous phase [rad] at ω.
pub fn open_loop(p: &Plant, f: &LoopFilters, g: &Gains, w: f64) -> (f64, f64) {
    let c = controller(g, f, w);
    let mut mag = c.abs() * p.k_over_j / w / (1.0 + (w * p.tau).powi(2)).sqrt();
    // Re(C) = Kp + Re(D path) > 0, so atan2 stays in (−π/2, π/2) without wrapping.
    let mut ph = c.im.atan2(c.re) - std::f64::consts::FRAC_PI_2 - (w * p.tau).atan() - w * p.delay;
    for flt in &f.gyro {
        mag *= flt.response(w).abs();
        ph += flt.phase(w);
    }
    (mag, ph)
}

/// Setpoint → rate closed loop |C·P / (1 + C·P·F_g)| (D on error for this estimate).
fn closed_loop_mag(p: &Plant, f: &LoopFilters, g: &Gains, w: f64) -> f64 {
    let c = controller(g, f, w);
    let lag = Cx::new(1.0, 0.0).cdiv(Cx::new(1.0, w * p.tau));
    let delay = Cx::new((w * p.delay).cos(), -(w * p.delay).sin());
    let plant = Cx::new(0.0, -p.k_over_j / w).cmul(lag).cmul(delay);
    let cp = c.cmul(plant);
    let mut fg = Cx::new(1.0, 0.0);
    for flt in &f.gyro {
        fg = fg.cmul(flt.response(w));
    }
    cp.cdiv(Cx::new(1.0, 0.0).cadd(cp.cmul(fg))).abs()
}

fn grid() -> impl Iterator<Item = f64> {
    // 0.3 … 30 000 rad/s, 60 points per decade.
    (0..=300).map(|i| 0.3 * 10f64.powf(i as f64 / 60.0))
}

pub fn analyze_loop(p: &Plant, f: &LoopFilters, g: &Gains) -> LoopMetrics {
    let mut pm = f64::INFINITY;
    let mut wc = 0.0;
    let mut gm = 60.0_f64;
    let mut prev: Option<(f64, f64, f64)> = None;
    for w in grid() {
        let (mag, ph) = open_loop(p, f, g, w);
        if let Some((w0, m0, ph0)) = prev {
            if m0 >= 1.0 && mag < 1.0 {
                // Gain crossover: refine in log ω.
                let (mut a, mut b) = (w0.ln(), w.ln());
                for _ in 0..40 {
                    let mid = 0.5 * (a + b);
                    if open_loop(p, f, g, mid.exp()).0 >= 1.0 {
                        a = mid;
                    } else {
                        b = mid;
                    }
                }
                let wx = (0.5 * (a + b)).exp();
                let margin = 180.0 + open_loop(p, f, g, wx).1.to_degrees();
                if margin < pm {
                    pm = margin;
                    wc = wx;
                }
            }
            // Phase crossings of −180° (and −540°, …).
            let k0 = ((ph0.to_degrees() + 180.0) / 360.0).floor();
            let k1 = ((ph.to_degrees() + 180.0) / 360.0).floor();
            if k1 < k0 {
                let gx = -20.0 * (0.5 * (m0 + mag)).log10();
                gm = gm.min(gx);
            }
        }
        prev = Some((w, mag, ph));
    }
    if !pm.is_finite() {
        pm = 0.0;
    }
    let mut bw = 0.0;
    for w in grid() {
        if closed_loop_mag(p, f, g, w) < std::f64::consts::FRAC_1_SQRT_2 {
            bw = w;
            break;
        }
    }
    LoopMetrics {
        crossover_hz: wc / (2.0 * std::f64::consts::PI),
        phase_margin_deg: pm,
        gain_margin_db: gm,
        bandwidth_hz: bw / (2.0 * std::f64::consts::PI),
    }
}

/// Designs rate-loop gains. Returns `None` when the plant is invalid or no grid point meets the
/// margins.
pub fn design(p: &Plant, f: &LoopFilters, opts: &DesignOpts) -> Option<(Gains, LoopMetrics)> {
    if !(p.k_over_j.is_finite() && p.k_over_j > 0.0 && p.tau >= 0.0 && p.delay >= 0.0) {
        return None;
    }
    let lag = p.tau + p.delay;
    let td_ratios: &[f64] = if opts.use_d {
        &[0.0, 0.15, 0.3, 0.45]
    } else {
        &[0.0]
    };
    let mut wc = opts.max_crossover.max(1.0);
    while wc > 0.5 {
        let mut best: Option<(Gains, LoopMetrics)> = None;
        for &r in td_ratios {
            let unit = Gains {
                kp: 1.0,
                ki: wc / opts.ki_div,
                kd: r * lag,
                kff: 0.0,
            };
            let (m1, _) = open_loop(p, f, &unit, wc);
            if !(m1.is_finite() && m1 > 0.0) {
                continue;
            }
            let kp = 1.0 / m1;
            if kp > opts.max_kp || kp * unit.kd > opts.max_kd {
                continue;
            }
            let g = Gains {
                kp,
                ki: (kp * unit.ki).min(opts.max_ki),
                kd: kp * unit.kd,
                kff: 1.0 / p.k_over_j,
            };
            let met = analyze_loop(p, f, &g);
            let wx = met.crossover_hz * 2.0 * std::f64::consts::PI;
            if met.phase_margin_deg >= opts.pm_target_deg
                && met.gain_margin_db >= opts.gm_target_db
                && (wx / wc - 1.0).abs() < 0.35
            {
                best = Some((g, met));
                break;
            }
        }
        if best.is_some() {
            return best;
        }
        wc /= 1.04;
    }
    None
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepResult {
    /// Rate error |1 − ω| stays below 5 % over the final 20 % of the run.
    pub settled: bool,
    pub overshoot: f64,
    pub max_abs: f64,
}

/// Fine-step time simulation of the closed loop for a unit rate step (D on measurement,
/// feed-forward off). Used by the tests as an independent stability check.
pub fn simulate_step(p: &Plant, f: &LoopFilters, g: &Gains, t_end: f64) -> StepResult {
    let dt = 1e-5;
    let steps = (t_end / dt) as usize;
    let nd = ((p.delay / dt).round() as usize).max(1);
    let mut buf = vec![0.0_f64; nd];
    let mut bi = 0;
    let (mut mot, mut w) = (0.0_f64, 0.0_f64);
    let mut gyro_states = vec![(0.0_f64, 0.0_f64); f.gyro.len()];
    let mut d_states = vec![(0.0_f64, 0.0_f64); f.dterm.len()];
    let mut integ = 0.0;
    let mut prev_meas = 0.0;
    let mut peak = 0.0_f64;
    let mut late_ok = true;
    let mut max_abs = 0.0_f64;
    let step = |x: f64, flt: &Filter, s: &mut (f64, f64)| -> f64 {
        let wf = flt.wf();
        match flt.kind {
            FilterKind::Pt1 => {
                let a = 1.0 - (-wf * dt).exp();
                s.0 += a * (x - s.0);
                s.0
            }
            FilterKind::Biquad => {
                // y'' = wf²(x − y) − √2·wf·y', semi-implicit Euler (dt ≪ 1/wf).
                let acc = wf * wf * (x - s.0) - std::f64::consts::SQRT_2 * wf * s.1;
                s.1 += acc * dt;
                s.0 += s.1 * dt;
                s.0
            }
        }
    };
    for k in 0..steps {
        let mut meas = w;
        for (flt, s) in f.gyro.iter().zip(gyro_states.iter_mut()) {
            meas = step(meas, flt, s);
        }
        let err = 1.0 - meas;
        integ += g.ki * err * dt;
        let mut dterm = -(meas - prev_meas) / dt;
        prev_meas = meas;
        for (flt, s) in f.dterm.iter().zip(d_states.iter_mut()) {
            dterm = step(dterm, flt, s);
        }
        let u = g.kp * err + integ + g.kd * dterm;
        let delayed = buf[bi];
        buf[bi] = u;
        bi = (bi + 1) % nd;
        if p.tau > 0.0 {
            mot += (delayed - mot) * (1.0 - (-dt / p.tau).exp());
        } else {
            mot = delayed;
        }
        w += p.k_over_j * mot * dt;
        if !w.is_finite() {
            return StepResult {
                settled: false,
                overshoot: f64::INFINITY,
                max_abs: f64::INFINITY,
            };
        }
        peak = peak.max(w);
        max_abs = max_abs.max(w.abs());
        if k as f64 > 0.8 * steps as f64 && (1.0 - w).abs() > 0.05 {
            late_ok = false;
        }
    }
    StepResult {
        settled: late_ok,
        overshoot: (peak - 1.0).max(0.0),
        max_abs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pure_integrator_has_90_deg_margin() {
        let p = Plant {
            k_over_j: 100.0,
            tau: 0.0,
            delay: 0.0,
        };
        let g = Gains {
            kp: 0.1,
            ..Default::default()
        };
        let m = analyze_loop(&p, &LoopFilters::default(), &g);
        assert!((m.crossover_hz * 2.0 * std::f64::consts::PI - 10.0).abs() < 0.1);
        assert!((m.phase_margin_deg - 90.0).abs() < 0.5);
    }

    #[test]
    fn designed_loop_meets_margins_and_settles() {
        let p = Plant {
            k_over_j: 800.0,
            tau: 0.04,
            delay: 0.004,
        };
        let f = LoopFilters {
            gyro: vec![Filter::biquad(45.0)],
            dterm: vec![Filter::pt1(22.0)],
        };
        let (g, m) = design(&p, &f, &DesignOpts::default()).expect("design");
        assert!(m.phase_margin_deg >= 50.0, "{m:?}");
        let s = simulate_step(&p, &f, &g, 1.5);
        assert!(s.settled, "{s:?} {g:?}");
    }
}
