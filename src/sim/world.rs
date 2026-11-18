// SPDX-License-Identifier: AGPL-3.0-or-later
//! The simulation world: 6-DoF rigid body integrated with RK4 at a fixed step, rotor and
//! servo dynamics, ground contact, battery and scenarios (wind, gust, payload drop, extra
//! mass, motor failure).
use super::aero::{self, AeroOut, Surfaces};
use super::control::{Actuators, Autopilot, ControlMode, CtrlContext, PilotInput, VtolPhase};
use super::env::{Wind, WindConfig};
use super::math::{Inertia, Quat, V3};
use super::vehicle::{ContactKind, VehicleParams};
use crate::calc::atmosphere::{G, isa};

/// Fixed physics step [s] (400 Hz).
pub const DT: f64 = 1.0 / 400.0;
const MAX_SPEED: f64 = 120.0;
const MAX_RATE: f64 = 40.0;
const SERVO_RATE: f64 = 6.0; // full travel (-1..1) in ~0.33 s
const FAILSAFE_DELAY: f64 = 0.2;
const GEAR_DEFLECTION: f64 = 0.01;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrashReason {
    HardLanding,
    Structure,
    PropStrike,
    Overturned,
    Overload,
    Numerical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlightStatus {
    OnGround,
    Airborne,
    Crashed(CrashReason),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RigidState {
    /// Position of the design CG reference, NED [m] (z = -altitude above the field).
    pub pos: V3,
    /// Velocity, NED [m/s].
    pub vel: V3,
    pub q: Quat,
    /// Body rates p, q, r [rad/s].
    pub w: V3,
}

impl Default for RigidState {
    fn default() -> Self {
        Self {
            pos: V3::ZERO,
            vel: V3::ZERO,
            q: Quat::IDENTITY,
            w: V3::ZERO,
        }
    }
}

#[derive(Clone, Copy)]
struct Deriv {
    dpos: V3,
    dvel: V3,
    dq: Quat,
    dw: V3,
}

impl RigidState {
    fn advanced(&self, d: &Deriv, h: f64) -> RigidState {
        RigidState {
            pos: self.pos + d.dpos * h,
            vel: self.vel + d.dvel * h,
            q: self.q.add_scaled(d.dq, h),
            w: self.w + d.dw * h,
        }
    }

    pub fn is_finite(&self) -> bool {
        self.pos.is_finite() && self.vel.is_finite() && self.q.is_finite() && self.w.is_finite()
    }
}

/// Switches used by tests to isolate parts of the model.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SimConfig {
    pub aero: bool,
    pub gravity: bool,
    pub ground: bool,
    pub seed: u64,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            aero: true,
            gravity: true,
            ground: true,
            seed: 0x5EED,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum StartPosition {
    /// On the runway / pad at rest.
    #[default]
    Ground,
    /// Airborne: trimmed cruise (fixed wing, VTOL) or hover (multirotor).
    Air,
}

/// Values shown on the HUD and used by tests.
#[derive(Clone, Copy, Debug, Default)]
pub struct Telemetry {
    pub t: f64,
    pub airspeed: f64,
    pub groundspeed: f64,
    pub agl: f64,
    pub vz_up: f64,
    pub roll: f64,
    pub pitch: f64,
    pub heading: f64,
    pub alpha: f64,
    pub beta: f64,
    pub cl: f64,
    pub stall: f64,
    pub load_factor: f64,
    pub soc: f64,
    pub voltage: f64,
    pub current: f64,
    pub power: f64,
    pub energy_used_wh: f64,
    pub motor_avg: f64,
    pub wind: V3,
}

struct Forces {
    f_world: V3,
    m_body: V3,
    power: f64,
    aero: AeroOut,
}

#[derive(Clone, Debug)]
pub struct Sim {
    pub params: VehicleParams,
    pub cfg: SimConfig,
    pub mass: f64,
    pub inertia: Inertia,
    /// Current CG relative to the design CG, body FRD [m].
    pub cg_offset: V3,
    pub state: RigidState,
    pub t: f64,
    pub motor_n: Vec<f64>,
    motor_failed: Vec<bool>,
    fail_time: Option<f64>,
    pub surfaces: Surfaces,
    pub mode: ControlMode,
    pub phase: VtolPhase,
    phase_timer: f64,
    pub transition_aborted: bool,
    pub input: PilotInput,
    pub trim_elevator: f64,
    pub trim_throttle: f64,
    autopilot: Autopilot,
    pub wind: Wind,
    pub energy_used_wh: f64,
    pub status: FlightStatus,
    pub failsafe: bool,
    pub payload_dropped: bool,
    pub extra_mass: f64,
    pub telemetry: Telemetry,
    last_cmd: Actuators,
    start: StartPosition,
}

impl Sim {
    pub fn new(params: VehicleParams, start: StartPosition) -> Sim {
        Sim::with_config(params, start, SimConfig::default())
    }

    pub fn with_config(params: VehicleParams, start: StartPosition, cfg: SimConfig) -> Sim {
        let n = params.rotors.len();
        let mut s = Sim {
            mass: params.mass,
            inertia: params.inertia,
            params,
            cfg,
            cg_offset: V3::ZERO,
            state: RigidState::default(),
            t: 0.0,
            motor_n: vec![0.0; n],
            motor_failed: vec![false; n],
            fail_time: None,
            surfaces: Surfaces::default(),
            mode: ControlMode::Stabilized,
            phase: VtolPhase::Hover,
            phase_timer: 0.0,
            transition_aborted: false,
            input: PilotInput::default(),
            trim_elevator: 0.0,
            trim_throttle: 0.0,
            autopilot: Autopilot::default(),
            wind: Wind::new(WindConfig::default(), cfg.seed),
            energy_used_wh: 0.0,
            status: FlightStatus::OnGround,
            failsafe: false,
            payload_dropped: false,
            extra_mass: 0.0,
            telemetry: Telemetry::default(),
            last_cmd: Actuators::default(),
            start,
        };
        s.reset(start);
        s
    }

    /// Restores the initial condition (mass changes and failures are cleared too; the wind
    /// settings are kept).
    pub fn reset(&mut self, start: StartPosition) {
        let n = self.params.rotors.len();
        let wind_cfg = self.wind.cfg;
        self.start = start;
        self.mass = self.params.mass;
        self.inertia = self.params.inertia;
        self.cg_offset = V3::ZERO;
        self.t = 0.0;
        self.motor_n = vec![0.0; n];
        self.motor_failed = vec![false; n];
        self.fail_time = None;
        self.surfaces = Surfaces::default();
        self.phase_timer = 0.0;
        self.transition_aborted = false;
        self.autopilot.reset();
        self.wind = Wind::new(wind_cfg, self.cfg.seed);
        self.energy_used_wh = 0.0;
        self.failsafe = false;
        self.payload_dropped = false;
        self.extra_mass = 0.0;
        self.trim_elevator = 0.0;
        self.trim_throttle = 0.0;
        self.input = PilotInput::default();
        self.telemetry = Telemetry::default();
        let has_wing = self.params.wing.is_some();
        self.phase = VtolPhase::Hover;
        match start {
            StartPosition::Ground => {
                let lowest = self
                    .params
                    .contacts
                    .iter()
                    .map(|c| c.pos.z)
                    .fold(0.0_f64, f64::max);
                self.state = RigidState {
                    pos: V3::new(0.0, 0.0, -(lowest - GEAR_DEFLECTION * 0.5)),
                    ..Default::default()
                };
                self.status = FlightStatus::OnGround;
                // Let the gear settle so the run starts from rest.
                let saved_mode = self.mode;
                for _ in 0..400 {
                    self.physics_step(&Actuators {
                        rotor: vec![0.0; n],
                        surfaces: Surfaces::default(),
                    });
                }
                self.state.vel = V3::ZERO;
                self.state.w = V3::ZERO;
                self.mode = saved_mode;
                self.t = 0.0;
                self.energy_used_wh = 0.0;
                if has_wing {
                    // Trim values for manual flying (state is restored afterwards).
                    let (st, surf, motors) = (self.state, self.surfaces, self.motor_n.clone());
                    self.trim_level(self.params.v_cruise);
                    self.state = st;
                    self.surfaces = surf;
                    self.motor_n = motors;
                }
            }
            StartPosition::Air => {
                let alt = if has_wing { 60.0 } else { 15.0 };
                self.state = RigidState {
                    pos: V3::new(0.0, 0.0, -alt),
                    ..Default::default()
                };
                self.status = FlightStatus::Airborne;
                if has_wing {
                    self.phase = VtolPhase::Cruise;
                    self.trim_level(self.params.v_cruise);
                    self.input.throttle = self.trim_throttle;
                } else {
                    // Available thrust includes the battery voltage factor (full pack).
                    let t_tot: f64 =
                        self.params.rotors.iter().map(|r| r.t_max).sum::<f64>() * self.k_batt();
                    let n_h = (self.mass * G / t_tot.max(1e-6)).clamp(0.0, 1.0).sqrt();
                    for m in self.motor_n.iter_mut() {
                        *m = n_h;
                    }
                    self.input.throttle = 0.5;
                }
            }
        }
        // Force-derived values (load factor, CL, power) for the initial state too, so the
        // HUD never shows values from before the reset.
        let s = self.state;
        let f = self.forces(&s);
        self.record_forces(&s, &f);
        self.update_telemetry();
    }

    pub fn start_position(&self) -> StartPosition {
        self.start
    }

    pub fn set_wind(&mut self, cfg: WindConfig) {
        self.wind.cfg = cfg;
    }

    pub fn trigger_gust(&mut self) {
        self.wind.trigger_gust();
    }

    pub fn is_crashed(&self) -> bool {
        matches!(self.status, FlightStatus::Crashed(_))
    }

    pub fn motor_failed(&self, k: usize) -> bool {
        self.motor_failed.get(k).copied().unwrap_or(false)
    }

    /// Simulates a motor/ESC failure on rotor `k`.
    pub fn fail_motor(&mut self, k: usize) {
        if k < self.motor_failed.len() && !self.motor_failed[k] {
            self.motor_failed[k] = true;
            self.fail_time.get_or_insert(self.t);
        }
    }

    /// Drops the payload: mass and CG change instantly.
    pub fn drop_payload(&mut self) {
        if self.payload_dropped || self.params.payload_mass <= 0.0 {
            return;
        }
        self.payload_dropped = true;
        let mp = self.params.payload_mass;
        let m_old = self.mass;
        let m_new = (m_old - mp).max(0.2 * self.params.mass);
        self.cg_offset = (self.cg_offset * m_old - self.params.payload_pos * mp) / m_new;
        self.inertia = self.inertia.scaled(m_new / m_old);
        self.mass = m_new;
    }

    /// Adds `frac` of the design mass at the current CG (e.g. a heavier payload). A point
    /// mass at the CG has no moment arm, so the inertia about the CG is unchanged.
    pub fn add_mass(&mut self, frac: f64) {
        let dm = self.params.mass * frac.clamp(0.0, 1.0);
        if dm <= 0.0 {
            return;
        }
        self.mass += dm;
        self.extra_mass += dm;
    }

    /// Starts a forward transition (VTOL in hover) or a back transition (VTOL in cruise).
    pub fn toggle_transition(&mut self) {
        if self.params.wing.is_none() || self.params.lift_rotor_count() == 0 {
            return;
        }
        self.phase = match self.phase {
            VtolPhase::Hover | VtolPhase::BackTransition => VtolPhase::Transition,
            VtolPhase::Transition | VtolPhase::Cruise => VtolPhase::BackTransition,
        };
        self.phase_timer = 0.0;
        self.transition_aborted = false;
    }

    pub fn rho(&self) -> f64 {
        isa(self.params.field_elevation + (-self.state.pos.z).max(0.0)).rho
    }

    fn k_batt(&self) -> f64 {
        let soc = self.soc();
        if soc <= 0.0 {
            return 0.0;
        }
        let v = 0.88 + 0.26 * soc;
        (v * v).min(1.1)
    }

    pub fn soc(&self) -> f64 {
        (1.0 - self.energy_used_wh / self.params.battery.energy_wh.max(1e-6)).clamp(0.0, 1.0)
    }

    pub fn battery_voltage(&self) -> f64 {
        self.params.battery.v_nom * (0.88 + 0.26 * self.soc())
    }

    fn known_failed(&self) -> Vec<bool> {
        let detected = self
            .fail_time
            .is_some_and(|t0| self.t - t0 >= FAILSAFE_DELAY);
        if detected {
            self.motor_failed.clone()
        } else {
            vec![false; self.motor_failed.len()]
        }
    }

    /// One fixed physics step with autopilot. Does nothing after a crash.
    pub fn step(&mut self) {
        if self.is_crashed() {
            return;
        }
        let tel = self.telemetry;
        // Fail-safe engages once the failure is detected (stabilised mode only).
        let detected = self
            .fail_time
            .is_some_and(|t0| self.t - t0 >= FAILSAFE_DELAY)
            && self.mode == ControlMode::Stabilized;
        let vtol = self.params.wing.is_some() && self.params.lift_rotor_count() > 0;
        if detected && !self.failsafe {
            self.failsafe = true;
            // VTOL: an interrupted transition goes back to hover. In hover the lift rotors
            // already fly (the fail-safe descends straight away); in cruise a dead lift motor
            // is tolerated (the fail-safe is masked below) until the back transition, where
            // the re-allocation over the remaining motors takes over.
            if vtol && self.phase == VtolPhase::Transition {
                self.phase = VtolPhase::BackTransition;
                self.phase_timer = 0.0;
            }
        }
        if detected && vtol && matches!(self.phase, VtolPhase::Transition | VtolPhase::Cruise) {
            // A dead pusher means hover and land.
            let pusher_dead = self
                .params
                .rotors
                .iter()
                .zip(&self.motor_failed)
                .any(|(r, &f)| f && !r.lift);
            if pusher_dead {
                self.phase = VtolPhase::BackTransition;
                self.phase_timer = 0.0;
            }
        }
        self.update_vtol_phase(&tel);
        let known = self.known_failed();
        let (phi, theta, psi) = self.state.q.to_euler();
        let ctx = CtrlContext {
            params: &self.params,
            mass: self.mass,
            inertia: self.inertia,
            euler: (phi, theta, psi),
            rates: self.state.w,
            airspeed: tel.airspeed,
            alpha: tel.alpha,
            beta: tel.beta,
            vz_up: tel.vz_up,
            agl: tel.agl,
            on_ground: self.status == FlightStatus::OnGround,
            rho: self.rho(),
            k_batt: self.k_batt(),
            input: self.input,
            mode: self.mode,
            phase: self.phase,
            failsafe: self.failsafe && !(vtol && self.phase == VtolPhase::Cruise),
            known_failed: &known,
            trim_elevator: self.trim_elevator,
        };
        let cmd = self.autopilot.compute(&ctx, DT);
        self.physics_step(&cmd);
        self.last_cmd = cmd;
    }

    /// One physics step with all motors commanded off and surfaces centred (tests, settling).
    pub fn step_passive(&mut self) {
        let cmd = Actuators {
            rotor: vec![0.0; self.params.rotors.len()],
            surfaces: Surfaces::default(),
        };
        self.physics_step(&cmd);
    }

    fn update_vtol_phase(&mut self, tel: &Telemetry) {
        if self.params.wing.is_none() || self.params.lift_rotor_count() == 0 {
            return;
        }
        self.phase_timer += DT;
        let vs = self.params.v_stall.max(3.0);
        let v_trans = (1.3 * vs).max(0.75 * self.params.v_cruise);
        match self.phase {
            VtolPhase::Transition => {
                if tel.airspeed >= v_trans {
                    self.phase = VtolPhase::Cruise;
                    self.phase_timer = 0.0;
                    if self.trim_throttle <= 0.0 {
                        self.trim_throttle = 0.6;
                    }
                    self.input.throttle = self.trim_throttle.max(0.45);
                } else if self.phase_timer > 30.0 {
                    self.phase = VtolPhase::BackTransition;
                    self.phase_timer = 0.0;
                    self.transition_aborted = true;
                }
            }
            VtolPhase::BackTransition => {
                if tel.airspeed < 3.0 || self.phase_timer > 30.0 {
                    self.phase = VtolPhase::Hover;
                    self.phase_timer = 0.0;
                    self.input.throttle = 0.5;
                }
            }
            VtolPhase::Cruise => {
                // Stall protection: hand back to the lift rotors.
                if self.mode == ControlMode::Stabilized
                    && self.status == FlightStatus::Airborne
                    && tel.airspeed < 1.0 * vs
                    && self.phase_timer > 2.0
                {
                    self.phase = VtolPhase::BackTransition;
                    self.phase_timer = 0.0;
                }
            }
            VtolPhase::Hover => {}
        }
    }

    /// Integrates one step with the given actuator commands (no autopilot).
    fn physics_step(&mut self, cmd: &Actuators) {
        let dt = DT;
        // Actuators: servo rate limit and motor lag (exact first-order update).
        let target = cmd.surfaces.clamped();
        let rate = |cur: f64, tgt: f64| cur + (tgt - cur).clamp(-SERVO_RATE * dt, SERVO_RATE * dt);
        self.surfaces = Surfaces {
            elevator: rate(self.surfaces.elevator, target.elevator),
            aileron: rate(self.surfaces.aileron, target.aileron),
            rudder: rate(self.surfaces.rudder, target.rudder),
        };
        for (k, r) in self.params.rotors.iter().enumerate() {
            let u = cmd.rotor.get(k).copied().unwrap_or(0.0);
            let u = if u.is_finite() {
                u.clamp(0.0, 1.0)
            } else {
                0.0
            };
            let n_cmd = if self.motor_failed[k] { 0.0 } else { u.sqrt() };
            let a = 1.0 - (-dt / r.tau).exp();
            self.motor_n[k] += (n_cmd - self.motor_n[k]) * a;
        }
        let airspeed = self.telemetry.airspeed;
        self.wind.update(dt, airspeed);

        let s0 = self.state;
        let f1 = self.forces(&s0);
        let k1 = self.deriv(&s0, &f1);
        let s2 = s0.advanced(&k1, 0.5 * dt);
        let k2 = self.deriv(&s2, &self.forces(&s2));
        let s3 = s0.advanced(&k2, 0.5 * dt);
        let k3 = self.deriv(&s3, &self.forces(&s3));
        let s4 = s0.advanced(&k3, dt);
        let k4 = self.deriv(&s4, &self.forces(&s4));
        let comb = |a: V3, b: V3, c: V3, d: V3| (a + (b + c) * 2.0 + d) * (dt / 6.0);
        let mut next = RigidState {
            pos: s0.pos + comb(k1.dpos, k2.dpos, k3.dpos, k4.dpos),
            vel: s0.vel + comb(k1.dvel, k2.dvel, k3.dvel, k4.dvel),
            q: s0
                .q
                .add_scaled(k1.dq, dt / 6.0)
                .add_scaled(k2.dq, dt / 3.0)
                .add_scaled(k3.dq, dt / 3.0)
                .add_scaled(k4.dq, dt / 6.0)
                .normalized(),
            w: s0.w + comb(k1.dw, k2.dw, k3.dw, k4.dw),
        };
        next.vel = next.vel.clamp_norm(MAX_SPEED);
        next.w = next.w.clamp_norm(MAX_RATE);
        if !next.is_finite() {
            self.status = FlightStatus::Crashed(CrashReason::Numerical);
            self.update_telemetry();
            return;
        }
        // Battery: electrical energy over the step.
        self.energy_used_wh += f1.power.max(0.0) * dt / 3600.0;
        self.record_forces(&s0, &f1);
        self.state = next;
        self.t += dt;
        self.update_ground_status(&s0);
        self.update_telemetry();
    }

    /// Stores the force-derived telemetry (load factor, CL, stall, electrical power).
    fn record_forces(&mut self, s: &RigidState, f: &Forces) {
        let load =
            (f.f_world - V3::new(0.0, 0.0, self.mass * G * self.grav())) * (1.0 / (self.mass * G));
        self.telemetry.load_factor = -s.q.rotate_inv(load).z;
        self.telemetry.cl = f.aero.cl;
        self.telemetry.stall = f.aero.stall;
        self.telemetry.power = f.power;
    }

    fn grav(&self) -> f64 {
        if self.cfg.gravity { 1.0 } else { 0.0 }
    }

    fn deriv(&self, s: &RigidState, f: &Forces) -> Deriv {
        let i = &self.inertia;
        let gyro = s.w.cross(i.apply(s.w));
        Deriv {
            dpos: s.vel,
            dvel: f.f_world / self.mass,
            dq: s.q.derivative(s.w),
            dw: i.solve(f.m_body - gyro),
        }
    }

    fn contact_stiffness(&self) -> (f64, f64) {
        let n_support = self
            .params
            .contacts
            .iter()
            .filter(|c| !matches!(c.kind, ContactKind::Structure))
            .count()
            .max(3) as f64;
        let k = self.mass * G / (GEAR_DEFLECTION * n_support);
        let c = 2.0 * 0.6 * (k * self.mass / n_support).sqrt();
        (k, c)
    }

    fn forces(&self, s: &RigidState) -> Forces {
        let p = &self.params;
        let q = s.q;
        let agl = -s.pos.z;
        let rho = isa(p.field_elevation + agl.max(0.0)).rho;
        let wind = self.wind.velocity(agl);
        let v_air_b = q.rotate_inv(s.vel - wind);
        let mut f_b = V3::ZERO;
        let mut m_b = V3::ZERO;
        let lever = -self.cg_offset;
        let mut aero_out = AeroOut::default();

        if self.cfg.aero {
            if let Some(wa) = p.wing.as_ref() {
                let mixed = self.surfaces.mixed(wa.mixing);
                aero_out = aero::wing_forces(wa, v_air_b, s.w, mixed, rho);
                f_b += aero_out.force;
                m_b += aero_out.moment + lever.cross(aero_out.force);
            }
            if p.drag_area != V3::ZERO {
                let vmag = v_air_b.norm();
                let d = v_air_b.mul_elem(p.drag_area) * (-0.5 * rho * vmag);
                f_b += d;
            }
        }

        // Rotors.
        let k_batt = self.k_batt();
        let mut p_shaft = 0.0;
        for (k, r) in p.rotors.iter().enumerate() {
            let n = self.motor_n[k];
            if n <= 1e-4 {
                continue;
            }
            let v_ax = if self.cfg.aero {
                v_air_b.dot(r.axis)
            } else {
                0.0
            };
            let f_ax = (1.0 - v_ax / (r.v_zero * n.max(0.05))).clamp(0.0, 1.3);
            let t = r.t_max * n * n * k_batt * f_ax;
            let mut f = r.axis * t;
            if self.cfg.aero {
                let v_in = v_air_b - r.axis * v_ax;
                f += v_in * (-t / r.v_tip.max(1.0));
            }
            let arm = r.pos - self.cg_offset;
            f_b += f;
            m_b += arm.cross(f);
            // Yaw reaction of lift rotors (the forward prop's roll torque is neglected).
            if r.lift {
                m_b += V3::new(0.0, 0.0, r.spin * r.k_q * t);
            }
            let vax_p = v_ax.max(0.0);
            let v_i = -0.5 * vax_p + (0.25 * vax_p * vax_p + t / (2.0 * rho * r.disk_area)).sqrt();
            // Lift rotors: momentum theory with the figure of merit. Propellers: the cruise
            // efficiency already contains the induced losses, so T V / eta_p, but never below
            // the static (figure of merit) power near zero airspeed.
            let p_momentum = t * (vax_p + v_i) / p.fm;
            p_shaft += if r.lift {
                p_momentum
            } else {
                (t * vax_p / p.eta_prop).max(t * v_i / p.fm)
            };
        }
        let power = if self.soc() > 0.0 {
            p_shaft / p.eta_el.max(0.1) + p.battery.p_hotel
        } else {
            0.0
        };

        // Gravity (world) and ground contact.
        let mut f_w = q.rotate(f_b) + V3::new(0.0, 0.0, self.mass * G * self.grav());
        if self.cfg.ground {
            let (kc, cc) = self.contact_stiffness();
            let fwd_b = |steer: f64| V3::new(steer.cos(), steer.sin(), 0.0);
            for c in &p.contacts {
                let r_b = c.pos - self.cg_offset;
                let r_w = q.rotate(r_b);
                let pz = s.pos.z + r_w.z;
                if pz <= 0.0 {
                    continue;
                }
                let v_pt = s.vel + q.rotate(s.w.cross(r_b));
                let normal = (kc * pz + cc * v_pt.z).max(0.0);
                let vt = V3::new(v_pt.x, v_pt.y, 0.0);
                let fric = match c.kind {
                    ContactKind::Wheel { steer } => {
                        let st = if steer {
                            self.surfaces.rudder * 0.35
                        } else {
                            0.0
                        };
                        let fw = q.rotate(fwd_b(st));
                        let fw = V3::new(fw.x, fw.y, 0.0).normalized_or(V3::new(1.0, 0.0, 0.0));
                        let lat = V3::new(-fw.y, fw.x, 0.0);
                        let (vl, vs) = (vt.dot(fw), vt.dot(lat));
                        fw * (-0.04 * normal * (vl / 0.2).tanh())
                            + lat * (-0.7 * normal * (vs / 0.15).tanh())
                    }
                    ContactKind::Skid | ContactKind::Structure => {
                        let mu = if p.lift_rotor_count() == 0 && c.kind == ContactKind::Skid {
                            0.3
                        } else {
                            0.5
                        };
                        let sp = vt.norm();
                        if sp > 1e-9 {
                            vt * (-mu * normal * (sp / 0.1).tanh() / sp)
                        } else {
                            V3::ZERO
                        }
                    }
                };
                let fc = V3::new(0.0, 0.0, -normal) + fric;
                f_w += fc;
                m_b += r_b.cross(q.rotate_inv(fc));
            }
        }
        Forces {
            f_world: f_w,
            m_body: m_b,
            power,
            aero: aero_out,
        }
    }

    fn update_ground_status(&mut self, before: &RigidState) {
        if !self.cfg.ground {
            self.status = FlightStatus::Airborne;
            return;
        }
        let s = self.state;
        let mut touching = false;
        let mut crash: Option<CrashReason> = None;
        for c in &self.params.contacts {
            let r_b = c.pos - self.cg_offset;
            let pz = s.pos.z + s.q.rotate(r_b).z;
            if pz <= 0.0 {
                continue;
            }
            let was_in = before.pos.z + before.q.rotate(r_b).z > 0.0;
            let v_pt = before.vel + before.q.rotate(before.w.cross(r_b));
            match c.kind {
                ContactKind::Wheel { .. } | ContactKind::Skid => {
                    touching = true;
                    if !was_in && v_pt.z > self.params.crash_sink {
                        crash = Some(CrashReason::HardLanding);
                    }
                }
                ContactKind::Structure => {
                    let hard = v_pt.z > 0.8 * self.params.crash_sink
                        || V3::new(v_pt.x, v_pt.y, 0.0).norm() > 6.0;
                    let spinning_rotor = self
                        .params
                        .rotors
                        .iter()
                        .enumerate()
                        .any(|(k, r)| (r.pos - c.pos).norm() < 1e-6 && self.motor_n[k] > 0.3);
                    if spinning_rotor {
                        crash = Some(CrashReason::PropStrike);
                    } else if hard && !was_in {
                        crash = Some(CrashReason::Structure);
                    }
                    touching = true;
                }
            }
        }
        if touching {
            // Upside down on the ground.
            let up_b = s.q.rotate(V3::new(0.0, 0.0, -1.0));
            if up_b.z > 0.3 {
                crash.get_or_insert(CrashReason::Overturned);
            }
        }
        if self.telemetry.load_factor.abs() > 20.0 {
            crash = Some(CrashReason::Overload);
        }
        self.status = match crash {
            Some(r) => FlightStatus::Crashed(r),
            None if touching => FlightStatus::OnGround,
            None => FlightStatus::Airborne,
        };
    }

    pub fn update_telemetry(&mut self) {
        let s = self.state;
        let agl = -s.pos.z;
        let wind = self.wind.velocity(agl);
        let v_air_b = s.q.rotate_inv(s.vel - wind);
        let airspeed = v_air_b.norm();
        let (roll, pitch, heading) = s.q.to_euler();
        let (alpha, beta) = if airspeed > 0.5 {
            (
                v_air_b.z.atan2(v_air_b.x),
                (v_air_b.y / airspeed).clamp(-1.0, 1.0).asin(),
            )
        } else {
            (0.0, 0.0)
        };
        let n = self.motor_n.len().max(1) as f64;
        let soc = self.soc();
        let v = self.battery_voltage();
        let t = &mut self.telemetry;
        t.t = self.t;
        t.airspeed = airspeed;
        t.groundspeed = V3::new(s.vel.x, s.vel.y, 0.0).norm();
        t.agl = agl;
        t.vz_up = -s.vel.z;
        t.roll = roll;
        t.pitch = pitch;
        t.heading = heading;
        t.alpha = alpha;
        t.beta = beta;
        t.wind = wind;
        t.energy_used_wh = self.energy_used_wh;
        t.motor_avg = self.motor_n.iter().sum::<f64>() / n;
        t.soc = soc;
        t.voltage = v;
        t.current = t.power / v.max(1.0);
    }

    /// Finds elevator, throttle and angle of attack for level flight at airspeed `v` and sets
    /// the state accordingly (fixed wing / VTOL cruise, heading north, flying with the local
    /// wind). Returns false if no trim was found.
    pub fn trim_level(&mut self, v: f64) -> bool {
        let Some(wa) = self.params.wing.clone() else {
            return false;
        };
        let pos = self.state.pos;
        let fwd: Vec<usize> = (0..self.params.rotors.len())
            .filter(|&k| !self.params.rotors[k].lift)
            .collect();
        let saved_cfg = self.cfg;
        self.cfg.ground = false;
        // Aerodynamic trim is solved in still air; the wind is added to the inertial
        // velocity afterwards so the airspeed is `v` with no sideslip.
        let saved_wind = std::mem::replace(&mut self.wind, Wind::new(WindConfig::default(), 0));
        let residual = |sim: &mut Sim, x: [f64; 3]| -> [f64; 3] {
            let (alpha, de, n) = (x[0], x[1], x[2]);
            sim.state = RigidState {
                pos,
                vel: V3::new(v, 0.0, 0.0),
                q: Quat::from_euler(0.0, alpha, 0.0),
                w: V3::ZERO,
            };
            sim.surfaces = Surfaces {
                elevator: de,
                ..Default::default()
            };
            for (k, m) in sim.motor_n.iter_mut().enumerate() {
                *m = if fwd.contains(&k) {
                    n.clamp(0.0, 1.0)
                } else {
                    0.0
                };
            }
            let f = sim.forces(&sim.state);
            let fb = sim.state.q.rotate_inv(f.f_world);
            [
                fb.x / (sim.mass * G),
                fb.z / (sim.mass * G),
                f.m_body.y / (sim.mass * G * wa.mac),
            ]
        };
        let mut x = [wa.alpha_w_trim - wa.i_w, 0.0, 0.85];
        let mut ok = false;
        for _ in 0..40 {
            let r = residual(self, x);
            if r.iter().map(|v| v.abs()).fold(0.0, f64::max) < 1e-7 {
                ok = true;
                break;
            }
            let mut j = [[0.0; 3]; 3];
            for c in 0..3 {
                let mut xp = x;
                let h = 1e-5;
                xp[c] += h;
                let rp = residual(self, xp);
                for row in 0..3 {
                    j[row][c] = (rp[row] - r[row]) / h;
                }
            }
            let Some(dx) = solve3(j, r) else {
                // Flat Jacobian (e.g. prop beyond its zero-thrust speed): more throttle.
                x[2] = (x[2] + 0.1).min(1.0);
                continue;
            };
            for c in 0..3 {
                x[c] -= dx[c].clamp(-0.2, 0.2);
            }
            x[1] = x[1].clamp(-1.0, 1.0);
            x[2] = x[2].clamp(0.0, 1.0);
        }
        self.cfg = saved_cfg;
        self.wind = saved_wind;
        let (alpha, de, n) = (x[0], x[1], x[2]);
        self.state = RigidState {
            pos,
            vel: V3::new(v, 0.0, 0.0) + self.wind.velocity(-pos.z),
            q: Quat::from_euler(0.0, alpha, 0.0),
            w: V3::ZERO,
        };
        self.surfaces = Surfaces {
            elevator: de,
            ..Default::default()
        };
        for (k, m) in self.motor_n.iter_mut().enumerate() {
            *m = if fwd.contains(&k) { n } else { 0.0 };
        }
        self.trim_elevator = de;
        // Throttle command is a thrust fraction (n^2).
        self.trim_throttle = n * n;
        ok
    }
}

fn solve3(a: [[f64; 3]; 3], b: [f64; 3]) -> Option<[f64; 3]> {
    let det = |m: [[f64; 3]; 3]| {
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    };
    let d = det(a);
    if !(d.abs() > 1e-14 && d.is_finite()) {
        return None;
    }
    let mut out = [0.0; 3];
    for (c, o) in out.iter_mut().enumerate() {
        let mut m = a;
        for r in 0..3 {
            m[r][c] = b[r];
        }
        *o = det(m) / d;
    }
    Some(out)
}
