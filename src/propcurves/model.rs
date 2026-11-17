// SPDX-License-Identifier: AGPL-3.0-or-later
//! Steady-state motor / propeller / battery operating-point model.
//!
//! Everything here is a **model estimate**, not bench-test data. The assumptions are:
//!
//! * Propeller: non-dimensional coefficients. Static thrust coefficient `CT0` is the same
//!   value (0.10) the multirotor hover-RPM estimate already uses; the static power coefficient
//!   follows from the project's figure of merit through momentum theory, so that
//!   `P_shaft(T) = T^1.5 / (sqrt(2 rho A) * FM)` holds at zero airspeed exactly as in the
//!   existing hover and static-thrust calculations. With advance ratio `J = V / (n D)` and
//!   `x = J / J0` (`J0 = pitch / D`, the zero-thrust advance ratio of an ideal screw):
//!   `CT = CT0 (1 - x^2)` and `CP = CP0 (1 - CP_SLOPE * x)`, both with `x` clamped to `[0, 1]`.
//! * Motor: DC motor with torque constant `Kt = 60 / (2 pi KV)`, winding resistance `R` (rising
//!   with winding temperature, copper coefficient 0.00393 / K) and a constant no-load current
//!   `I0`. Iron loss is `I0 * E` (back-EMF times no-load current).
//! * ESC: ideal buck converter with a constant efficiency, so the battery current is
//!   `throttle * I_motor / eta_esc`.
//! * Battery: open-circuit voltage (project nominal pack voltage) minus the sag over a constant
//!   pack resistance. No Peukert effect and no state-of-charge dependence.
//! * Thermal: single lumped node (the winding). Steady-state temperature
//!   `T = T_amb + P_loss * R_th`, with `R_th = R_th0 / (1 + K_FLOW * v_air)`, where `v_air` is
//!   the airspeed plus the momentum-theory induced velocity of the propeller (the prop wash
//!   over the motor). `R_th0` (still air) scales with motor size, see [`rth_still_air`]. All
//!   copper and iron loss is assumed to heat the winding. The result is the temperature that
//!   a *sustained* run at that throttle would settle at, which is the conservative reading.

use std::f64::consts::PI;

/// Static thrust coefficient (thrust = CT n^2 D^4 rho). Same value as the hover RPM estimate.
pub const CT0: f64 = 0.10;
/// Slope of the power coefficient with the normalised advance ratio.
pub const CP_SLOPE: f64 = 0.35;
/// Exponent of the blade-count scaling of `CT0` relative to a two-blade propeller.
pub const BLADE_EXPONENT: f64 = 0.5;
/// Copper temperature coefficient of resistance [1/K], reference 20 degC.
pub const COPPER_ALPHA: f64 = 0.003_93;
/// Cooling gain per m/s of air speed over the motor [s/m].
pub const K_FLOW: f64 = 0.08;
/// Ambient temperature used for the winding estimate [degC].
pub const T_AMBIENT_C: f64 = 25.0;
/// Fraction of the full-throttle motor loss that is copper loss when sizing a default motor.
pub const COPPER_LOSS_SHARE: f64 = 0.7;
/// Default winding temperature limit [degC] (magnet and epoxy limits, not enamel).
pub const T_LIMIT_C: f64 = 110.0;

/// Propeller described by non-dimensional coefficients.
#[derive(Clone, Debug)]
pub struct Propeller {
    pub diameter_m: f64,
    pub pitch_m: f64,
    pub blades: u32,
    /// Static thrust coefficient.
    pub ct0: f64,
    /// Static power coefficient (from the figure of merit).
    pub cp0: f64,
}

impl Propeller {
    /// `fm` is the (effective) figure of merit used for the static power coefficient.
    pub fn new(diameter_m: f64, pitch_m: f64, blades: u32, fm: f64) -> Self {
        let b = blades.max(1) as f64;
        let ct0 = CT0 * (b / 2.0).powf(BLADE_EXPONENT);
        let fm = fm.clamp(0.1, 1.0);
        let cp0 = ct0.powf(1.5) * (2.0 / PI).sqrt() / fm;
        Self {
            diameter_m: diameter_m.max(1e-3),
            pitch_m: pitch_m.max(1e-3),
            blades: blades.max(1),
            ct0,
            cp0,
        }
    }

    /// Zero-thrust advance ratio.
    pub fn j0(&self) -> f64 {
        self.pitch_m / self.diameter_m
    }

    fn x(&self, v: f64, n: f64) -> f64 {
        if n <= 1e-9 {
            return 1.0;
        }
        (v / (n * self.diameter_m) / self.j0()).clamp(0.0, 1.0)
    }

    /// Thrust coefficient at airspeed `v` [m/s] and rotation speed `n` [rev/s].
    pub fn ct(&self, v: f64, n: f64) -> f64 {
        let x = self.x(v, n);
        self.ct0 * (1.0 - x * x)
    }

    /// Power coefficient at airspeed `v` [m/s] and rotation speed `n` [rev/s].
    pub fn cp(&self, v: f64, n: f64) -> f64 {
        self.cp0 * (1.0 - CP_SLOPE * self.x(v, n))
    }

    /// Disk area [m^2].
    pub fn disk_area(&self) -> f64 {
        PI / 4.0 * self.diameter_m * self.diameter_m
    }
}

/// Brushless motor.
#[derive(Clone, Debug)]
pub struct Motor {
    /// Velocity constant [rpm / V].
    pub kv: f64,
    /// Winding resistance at 20 degC [ohm].
    pub r_ohm: f64,
    /// No-load current [A].
    pub i0_a: f64,
    /// Continuous current rating [A].
    pub i_max_a: f64,
    /// Still-air thermal resistance winding to ambient [K/W].
    pub rth_still: f64,
    /// Winding temperature limit [degC].
    pub t_limit_c: f64,
}

/// Electronic speed controller.
#[derive(Clone, Debug)]
pub struct Esc {
    pub eta: f64,
    pub i_max_a: f64,
}

/// Battery pack shared by all motors.
#[derive(Clone, Debug)]
pub struct Pack {
    /// Open-circuit (nominal) voltage [V].
    pub v_oc: f64,
    /// Internal resistance of the whole pack [ohm].
    pub r_ohm: f64,
    pub capacity_ah: f64,
    /// Continuous current limit of the whole pack (cell limit x parallel count) [A].
    pub max_cont_a: f64,
}

/// A group of identical motor + propeller units on one battery.
#[derive(Clone, Debug)]
pub struct PropUnit {
    pub prop: Propeller,
    pub motor: Motor,
    pub esc: Esc,
    pub pack: Pack,
    pub n_motors: u32,
    /// Air density [kg/m^3].
    pub rho: f64,
    /// Rotation speed limit from the tip Mach limit [rpm].
    pub rpm_limit: f64,
}

/// One solved operating point of a single motor (the pack is shared by `n_motors`).
#[derive(Clone, Copy, Debug, Default)]
pub struct OperatingPoint {
    pub throttle: f64,
    pub airspeed: f64,
    pub rpm: f64,
    pub thrust_n: f64,
    pub torque_nm: f64,
    pub p_shaft_w: f64,
    /// Motor (phase-equivalent DC) current [A].
    pub i_motor_a: f64,
    /// Battery current drawn by this motor [A].
    pub i_bat_a: f64,
    /// Loaded bus voltage [V].
    pub v_bus: f64,
    /// Electrical power drawn from the battery by this motor [W]; always `v_bus * i_bat_a`.
    pub p_elec_w: f64,
    /// Copper plus iron loss in the motor [W].
    pub p_loss_w: f64,
    /// Estimated steady-state winding temperature [degC].
    pub temp_c: f64,
}

impl OperatingPoint {
    /// Thrust in gram-force.
    pub fn thrust_gf(&self) -> f64 {
        self.thrust_n / 9.806_65 * 1000.0
    }
}

/// Still-air winding-to-ambient thermal resistance for a motor that handles `p_rated_w` of
/// electrical input: `R_th0 = 60 / sqrt(P)` K/W (about 3.5 K/W at 300 W, 1.9 K/W at 1 kW).
/// A rough size scaling, not a datasheet value.
pub fn rth_still_air(p_rated_w: f64) -> f64 {
    60.0 / p_rated_w.max(20.0).sqrt()
}

impl PropUnit {
    /// Zero-throttle state: nothing spins and nothing is drawn.
    fn idle(&self, v: f64) -> OperatingPoint {
        OperatingPoint {
            airspeed: v,
            v_bus: self.pack.v_oc,
            temp_c: T_AMBIENT_C,
            ..Default::default()
        }
    }

    /// Rotation speed [rev/s] at which the motor voltage balance closes, given motor voltage
    /// `v_m` and hot winding resistance `r`. Bisection on a monotone balance function.
    fn solve_rps(&self, v_m: f64, r: f64, v: f64) -> f64 {
        let kt = 60.0 / (2.0 * PI * self.motor.kv.max(1.0));
        let d = self.prop.diameter_m;
        let balance = |n: f64| {
            let q = self.prop.cp(v, n) * self.rho * n * n * d.powi(5) / (2.0 * PI);
            let i = self.motor.i0_a + q / kt;
            v_m - (r * i + 60.0 * n / self.motor.kv.max(1.0))
        };
        if balance(0.0) <= 0.0 {
            return 0.0;
        }
        let mut lo = 0.0;
        let mut hi = (v_m * self.motor.kv / 60.0).max(1e-6) * 1.05;
        for _ in 0..60 {
            let mid = 0.5 * (lo + hi);
            if balance(mid) > 0.0 {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        0.5 * (lo + hi)
    }

    /// Solve the operating point for a throttle in `[0, 1]` and airspeed `v` [m/s].
    pub fn operating_point(&self, throttle: f64, v: f64) -> OperatingPoint {
        let d = throttle.clamp(0.0, 1.0);
        let v = v.max(0.0);
        if d <= 0.0 {
            return self.idle(v);
        }
        let n_m = self.n_motors.max(1) as f64;
        let kt = 60.0 / (2.0 * PI * self.motor.kv.max(1.0));
        let dia = self.prop.diameter_m;
        let eta = self.esc.eta.clamp(0.1, 1.0);
        let mut v_bus = self.pack.v_oc;
        let mut temp = T_AMBIENT_C;
        let mut op = self.idle(v);
        // Fixed-point iteration over the coupled pack sag and winding resistance (damped).
        for _ in 0..40 {
            let r_hot = self.motor.r_ohm * (1.0 + COPPER_ALPHA * (temp - 20.0)).max(0.5);
            let n = self.solve_rps(d * v_bus, r_hot, v);
            let q = self.prop.cp(v, n) * self.rho * n * n * dia.powi(5) / (2.0 * PI);
            let i_m = self.motor.i0_a + q / kt;
            let thrust = self.prop.ct(v, n) * self.rho * n * n * dia.powi(4);
            let i_bat = d * i_m / eta;
            let emf = 60.0 * n / self.motor.kv.max(1.0);
            let p_loss = i_m * i_m * r_hot + self.motor.i0_a * emf;
            let v_ind = (thrust.max(0.0) / (2.0 * self.rho * self.prop.disk_area())).sqrt();
            let rth = self.motor.rth_still / (1.0 + K_FLOW * (v + v_ind));
            let temp_new = (T_AMBIENT_C + p_loss * rth).min(400.0);
            let v_bus_new = (self.pack.v_oc - n_m * i_bat * self.pack.r_ohm).max(0.1);
            op = OperatingPoint {
                throttle: d,
                airspeed: v,
                rpm: n * 60.0,
                thrust_n: thrust,
                torque_nm: q,
                p_shaft_w: 2.0 * PI * n * q,
                i_motor_a: i_m,
                i_bat_a: i_bat,
                v_bus,
                p_elec_w: v_bus * i_bat,
                p_loss_w: p_loss,
                temp_c: temp,
            };
            let done = (v_bus_new - v_bus).abs() < 1e-9 && (temp_new - temp).abs() < 1e-9;
            v_bus = 0.5 * (v_bus + v_bus_new);
            temp = 0.5 * (temp + temp_new);
            if done {
                break;
            }
        }
        // Report the final, consistent state (bus voltage and temperature as iterated).
        op.v_bus = v_bus;
        op.p_elec_w = v_bus * op.i_bat_a;
        op.temp_c = temp;
        op
    }

    /// Smallest throttle in `[0, 1]` whose thrust at airspeed `v` reaches `target_n`.
    /// `None` if even full throttle cannot reach it.
    pub fn throttle_for_thrust(&self, target_n: f64, v: f64) -> Option<f64> {
        if target_n <= 0.0 {
            return Some(0.0);
        }
        if self.operating_point(1.0, v).thrust_n < target_n {
            return None;
        }
        let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
        for _ in 0..40 {
            let mid = 0.5 * (lo + hi);
            if self.operating_point(mid, v).thrust_n >= target_n {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        Some(hi)
    }

    /// Smallest throttle whose per-motor battery power at airspeed `v` reaches `target_w`.
    pub fn throttle_for_power(&self, target_w: f64, v: f64) -> Option<f64> {
        if target_w <= 0.0 {
            return Some(0.0);
        }
        if self.operating_point(1.0, v).p_elec_w < target_w {
            return None;
        }
        let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
        for _ in 0..40 {
            let mid = 0.5 * (lo + hi);
            if self.operating_point(mid, v).p_elec_w >= target_w {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        Some(hi)
    }
}
