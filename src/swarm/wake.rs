// SPDX-License-Identifier: AGPL-3.0-or-later
//! Wake interaction between vehicles flying in formation.
//!
//! Frame: `x` aft, `y` right, `h` up (right-handed), see [`super::formation`].
//!
//! **Fixed wing — horseshoe vortex.** Each aircraft is replaced by one horseshoe vortex (lifting
//! line, Prandtl): a bound vortex on the quarter-chord line plus two trailing legs. For an
//! elliptic load the trailing vortices roll up at `b' = (π/4)·b` and `Γ = L / (ρ V b')`
//! (Anderson, *Fundamentals of Aerodynamics*, §5.3; McCormick, *Aerodynamics, Aeronautics and
//! Flight Mechanics*, §4). Induced velocities come from the Biot–Savart law for a straight
//! segment (Anderson eq. 5.5; Katz & Plotkin, *Low-Speed Aerodynamics*, eq. 10.115) with a small
//! viscous core so the field stays finite near the vortex line. The trailing pair sinks at
//! `Γ / (2π b')` (vortex-pair self-induction). A trailing aircraft flying in upwash tilts its lift
//! forward: `ΔD = −L·w̄/V` with `w̄` the load-weighted mean upwash over its span — the mechanism
//! behind V-formation savings (Lissaman & Shollenberger, *Formation Flight of Birds*, Science
//! 168, 1970; Hummel, *Aerodynamic aspects of formation flight in birds*, J. Theor. Biol. 1983).
//! The rolling moment of the non-uniform upwash is estimated with strip theory.
//!
//! **Multirotor — momentum-theory downwash.** The rotors of one vehicle are lumped into a single
//! actuator disk of the same total area. Hover induced velocity `v_h = √(T / 2ρA)`; in forward
//! flight Glauert's relation `v = v_h² / √(V² + (V_c + v)²)` (Leishman, *Principles of Helicopter
//! Aerodynamics*, 2nd ed., §2.13–2.14, axial climb and forward flight). The ideal wake velocity along the
//! axis follows the semi-infinite vortex-cylinder result `w = v(1 + z/√(z² + R²))`, contracting to
//! `2v` and `R/√2` (Leishman §2.3). Beyond that the wake is treated as a round turbulent jet:
//! half-width grows by [`JET_SPREAD`] per metre and the centre-line velocity falls as
//! `r_ideal / r` (momentum flux conserved; Pope, *Turbulent Flows*, §5.1). The radial profile is
//! Gaussian with that half-width. The wake is carried aft at the flight speed while it descends.
//! A rotor inside a descending stream of speed `w` behaves like a rotor climbing at `w`:
//! extra power from momentum theory, thrust loss at fixed throttle from blade-element momentum
//! theory with typical small-propeller constants (Leishman §3.2).
//!
//! These are teaching / preliminary-design models: no unsteadiness, ground effect, wake
//! meandering or rotor-to-rotor interaction inside one vehicle.

use std::f64::consts::PI;

pub type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(a: V3) -> f64 {
    dot(a, a).sqrt()
}

/// Velocity induced at `p` by a straight vortex segment `a → b` of circulation `gamma`
/// (right-hand rule along `a → b`). `core`: viscous core radius [m] (0 = ideal line vortex).
///
/// `V = Γ/4π · (r1 × r2)/(|r1 × r2|² + rc²|r0|²) · r0·(r1/|r1| − r2/|r2|)`.
pub fn segment_velocity(p: V3, a: V3, b: V3, gamma: f64, core: f64) -> V3 {
    let r0 = sub(b, a);
    let r1 = sub(p, a);
    let r2 = sub(p, b);
    let n1 = norm(r1);
    let n2 = norm(r2);
    let c = cross(r1, r2);
    let l0 = dot(r0, r0);
    let den = dot(c, c) + core * core * l0;
    if n1 < 1e-12 || n2 < 1e-12 || den < 1e-24 {
        return [0.0; 3];
    }
    let k = gamma / (4.0 * PI) / den
        * dot(
            r0,
            [
                r1[0] / n1 - r2[0] / n2,
                r1[1] / n1 - r2[1] / n2,
                r1[2] / n1 - r2[2] / n2,
            ],
        );
    [c[0] * k, c[1] * k, c[2] * k]
}

/// One aircraft modelled as a horseshoe vortex.
#[derive(Clone, Copy, Debug)]
pub struct Horseshoe {
    /// Quarter-chord centre `[x, y, h]`.
    pub pos: V3,
    /// Vortex half-span `b'/2` [m].
    pub half_span: f64,
    /// Circulation [m²/s].
    pub gamma: f64,
    /// Viscous core radius [m].
    pub core: f64,
    /// Flight speed (for wake sink) [m/s].
    pub v: f64,
}

/// Ratio of rolled-up vortex span to geometric span for an elliptic load.
pub const VORTEX_SPAN_RATIO: f64 = PI / 4.0;
/// Viscous core radius as a fraction of the vortex span (a few per cent is typical).
pub const CORE_FRACTION: f64 = 0.04;

impl Horseshoe {
    pub fn from_wing(pos: V3, span: f64, lift: f64, rho: f64, v: f64) -> Self {
        let bv = VORTEX_SPAN_RATIO * span.max(1e-6);
        let gamma = if rho > 0.0 && v > 0.0 {
            lift / (rho * v * bv)
        } else {
            0.0
        };
        Horseshoe {
            pos,
            half_span: 0.5 * bv,
            gamma: if gamma.is_finite() { gamma } else { 0.0 },
            core: CORE_FRACTION * bv,
            v,
        }
    }

    /// Sink speed of the trailing vortex pair, `Γ / (2π b')` [m/s].
    pub fn sink_rate(&self) -> f64 {
        if self.half_span > 0.0 {
            self.gamma / (2.0 * PI * 2.0 * self.half_span)
        } else {
            0.0
        }
    }

    /// Induced velocity `[u_aft, v_right, w_up]` at `p`. The trailing legs are lowered by the
    /// sink accumulated over the time the wake needed to reach `p`.
    pub fn velocity(&self, p: V3) -> V3 {
        let far = 1000.0 * (2.0 * self.half_span).max(1.0);
        let dx = (p[0] - self.pos[0]).max(0.0);
        let sink = if self.v > 0.0 {
            self.sink_rate() * dx / self.v
        } else {
            0.0
        };
        // Evaluating at a raised point ≡ lowering the vortex system there.
        let q = [p[0], p[1], p[2] + sink];
        let [x, y, h] = self.pos;
        let l = [x, y - self.half_span, h];
        let r = [x, y + self.half_span, h];
        let lf = [x + far, l[1], h];
        let rf = [x + far, r[1], h];
        let mut out = [0.0; 3];
        for (a, b) in [(lf, l), (l, r), (r, rf)] {
            let s = segment_velocity(q, a, b, self.gamma, self.core);
            for k in 0..3 {
                out[k] += s[k];
            }
        }
        out
    }
}

/// Data of one fixed-wing vehicle (all copies identical).
#[derive(Clone, Copy, Debug)]
pub struct WingVehicle {
    pub span: f64,
    pub s_ref: f64,
    /// 3D lift-curve slope [1/rad].
    pub cla: f64,
    /// Induced-drag factor `k` in `CD = CD0 + k·CL²`.
    pub k: f64,
    /// Weight = lift in level flight [N].
    pub weight: f64,
    pub rho: f64,
    pub v: f64,
    /// Electrical-to-thrust-power efficiency.
    pub eta: f64,
    /// Electrical power of one vehicle flying alone [W].
    pub p_alone: f64,
}

impl WingVehicle {
    pub fn q(&self) -> f64 {
        0.5 * self.rho * self.v * self.v
    }
    /// Induced drag of the isolated aircraft, `k·W²/(q·S)` [N].
    pub fn induced_drag(&self) -> f64 {
        let qs = self.q() * self.s_ref;
        if qs > 0.0 {
            self.k * self.weight * self.weight / qs
        } else {
            0.0
        }
    }
    pub fn horseshoe(&self, pos: V3) -> Horseshoe {
        Horseshoe::from_wing(pos, self.span, self.weight, self.rho, self.v)
    }
}

/// Wake effect on one fixed-wing vehicle.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WingEffect {
    /// Load-weighted mean upwash over the span [m/s] (+ up).
    pub w_mean: f64,
    /// Induced-drag change [N] (negative = saving).
    pub d_drag: f64,
    /// `d_drag / induced drag alone`.
    pub d_drag_frac: f64,
    /// Electrical power change [W].
    pub d_power: f64,
    /// Rolling-moment coefficient induced by the others' wakes (strip theory).
    pub cl_roll: f64,
}

/// Typical full-aileron rolling-moment coefficient of a small UAV (assumption for the hazard
/// flag; real values ~0.03–0.08).
pub const TYPICAL_ROLL_AUTHORITY: f64 = 0.05;
/// Share of the aileron authority above which a wake encounter is flagged (leaves margin for
/// gusts and manoeuvres).
pub const ROLL_HAZARD_SHARE: f64 = 0.4;

impl WingEffect {
    /// Wake-induced roll needs more than [`ROLL_HAZARD_SHARE`] of the typical aileron authority.
    pub fn roll_hazard(&self) -> bool {
        self.cl_roll.abs() > ROLL_HAZARD_SHARE * TYPICAL_ROLL_AUTHORITY
    }
}

const SPAN_STATIONS: usize = 24;

/// Effect of all other vehicles' wakes on each vehicle.
pub fn wing_interactions(veh: &WingVehicle, pos: &[V3]) -> Vec<WingEffect> {
    let shoes: Vec<Horseshoe> = pos.iter().map(|p| veh.horseshoe(*p)).collect();
    let di = veh.induced_drag();
    let q = veh.q();
    let chord = if veh.span > 0.0 {
        veh.s_ref / veh.span
    } else {
        0.0
    };
    (0..pos.len())
        .map(|j| {
            let mut wsum = 0.0;
            let mut wt = 0.0;
            let mut moment = 0.0;
            let deta = veh.span / SPAN_STATIONS as f64;
            for s in 0..SPAN_STATIONS {
                let eta = -0.5 * veh.span + (s as f64 + 0.5) * deta;
                let p = [pos[j][0], pos[j][1] + eta, pos[j][2]];
                let w: f64 = shoes
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != j)
                    .map(|(_, h)| h.velocity(p)[2])
                    .sum();
                let g = (1.0 - (2.0 * eta / veh.span).powi(2)).max(0.0).sqrt();
                wsum += w * g;
                wt += g;
                // Strip lift change q·c·a·(w/V)·Δη; right-wing-up rolling moment positive.
                if veh.v > 0.0 {
                    moment += q * chord * veh.cla * (w / veh.v) * eta * deta;
                }
            }
            let w_mean = if wt > 0.0 { wsum / wt } else { 0.0 };
            let d_drag = if veh.v > 0.0 {
                -veh.weight * w_mean / veh.v
            } else {
                0.0
            };
            let qsb = q * veh.s_ref * veh.span;
            WingEffect {
                w_mean,
                d_drag,
                d_drag_frac: if di > 0.0 { d_drag / di } else { 0.0 },
                d_power: if veh.eta > 0.0 {
                    d_drag * veh.v / veh.eta
                } else {
                    0.0
                },
                cl_roll: if qsb > 0.0 { moment / qsb } else { 0.0 },
            }
        })
        .collect()
}

/// Lateral spacing (centre to centre, in spans) with the largest saving for a two-ship echelon
/// at streamwise step `dx` [m]. Returns `(spacing / b, d_drag_frac)`.
pub fn best_lateral_spacing(veh: &WingVehicle, dx: f64) -> (f64, f64) {
    let mut best = (1.0, 0.0);
    for i in 0..=100 {
        let f = 0.5 + i as f64 * 0.01;
        let e = wing_interactions(veh, &[[0.0, 0.0, 0.0], [dx, f * veh.span, 0.0]])[1];
        if e.d_drag_frac < best.1 {
            best = (f, e.d_drag_frac);
        }
    }
    best
}

// ------------------------------------------------------------------------------------------
// Multirotor
// ------------------------------------------------------------------------------------------

/// Half-width growth rate of the far wake (round turbulent jet: ≈ 0.09–0.10).
pub const JET_SPREAD: f64 = 0.1;
/// Thrust-loss threshold used for the separation recommendation (fixed throttle).
pub const SAFE_THRUST_LOSS: f64 = 0.05;

/// Typical small fixed-pitch propeller for the blade-element estimate:
/// `σ·a/2` (solidity ≈ 0.1, lift slope ≈ 5.7) and hover `C_T`.
const BEMT_SIGMA_A_HALF: f64 = 0.285;
const BEMT_CT_HOVER: f64 = 0.01;

/// Data of one multirotor vehicle (all copies identical).
#[derive(Clone, Copy, Debug)]
pub struct RotorVehicle {
    /// Thrust = weight [N].
    pub thrust: f64,
    /// Total disk area [m²].
    pub disk_area: f64,
    pub rho: f64,
    /// Flight speed [m/s] (0 = hover).
    pub v: f64,
    /// Electrical hover power [W] (used to turn ideal power into electrical power).
    pub p_elec_hover: f64,
    /// Electrical power of one vehicle alone at speed `v` [W].
    pub p_alone: f64,
}

/// Hover induced velocity `√(T / 2ρA)`.
pub fn hover_induced_velocity(thrust: f64, rho: f64, area: f64) -> f64 {
    let d = 2.0 * rho * area;
    if d > 0.0 && thrust > 0.0 {
        (thrust / d).sqrt()
    } else {
        0.0
    }
}

/// Induced velocity with forward speed `v_fwd` and axial (climb / descending-air) speed `v_c`:
/// solves `v = v_h² / √(v_fwd² + (v_c + v)²)` (Glauert / momentum theory, `v_c ≥ 0`).
pub fn induced_velocity(v_h: f64, v_fwd: f64, v_c: f64) -> f64 {
    if v_h <= 0.0 {
        return 0.0;
    }
    let vc = v_c.max(0.0);
    let f = |v: f64| v * (v_fwd * v_fwd + (vc + v).powi(2)).sqrt() - v_h * v_h;
    // f is increasing in v; bisection on [0, v_h] (the solution never exceeds v_h).
    let (mut lo, mut hi) = (0.0, v_h);
    for _ in 0..80 {
        let mid = 0.5 * (lo + hi);
        if f(mid) > 0.0 {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    0.5 * (lo + hi)
}

impl RotorVehicle {
    pub fn v_h(&self) -> f64 {
        hover_induced_velocity(self.thrust, self.rho, self.disk_area)
    }
    /// Induced velocity at the flight speed.
    pub fn v_i(&self) -> f64 {
        induced_velocity(self.v_h(), self.v.max(0.0), 0.0)
    }
    /// Equivalent single-disk radius `√(A/π)`.
    pub fn r_eq(&self) -> f64 {
        (self.disk_area.max(0.0) / PI).sqrt()
    }
    /// Electrical watts per ideal induced watt (`1 / (FM·η)` taken from the hover numbers).
    fn elec_per_ideal(&self) -> f64 {
        let p_ideal = self.thrust * self.v_h();
        if p_ideal > 0.0 && self.p_elec_hover > 0.0 {
            self.p_elec_hover / p_ideal
        } else {
            1.0
        }
    }
}

/// Wake shape at path length `s` behind the disk: `(centre-line velocity, half-width)`.
pub fn wake_profile(s: f64, v_i: f64, r: f64) -> (f64, f64) {
    if r <= 0.0 || v_i <= 0.0 {
        return (0.0, r.max(1e-6));
    }
    let s = s.max(0.0);
    let w_ideal = v_i * (1.0 + s / (s * s + r * r).sqrt());
    let r_ideal = r * (v_i / w_ideal).sqrt();
    let r_mix = r_ideal + JET_SPREAD * s;
    (w_ideal * r_ideal / r_mix, r_mix)
}

/// Wake centre line `(x_aft, depth, s)` sampled from the disk: carried aft at `v` while
/// descending at the local centre-line velocity.
pub fn wake_centerline(veh: &RotorVehicle, s_max: f64) -> Vec<[f64; 3]> {
    let v_i = veh.v_i();
    let r = veh.r_eq();
    let n = 400;
    let ds = s_max.max(1e-3) / n as f64;
    let mut out = Vec::with_capacity(n + 1);
    let (mut x, mut z, mut s) = (0.0, 0.0, 0.0);
    out.push([x, z, s]);
    for _ in 0..n {
        let (w, _) = wake_profile(s, v_i, r);
        let m = (veh.v * veh.v + w * w).sqrt();
        if m <= 1e-9 {
            z += ds;
        } else {
            x += ds * veh.v / m;
            z += ds * w / m;
        }
        s += ds;
        out.push([x, z, s]);
    }
    out
}

/// Downward wake velocity [m/s] of a vehicle at the origin at a point `dx` aft, `dy` to the side
/// and `depth` below it. Zero above the disk plane.
pub fn wake_velocity(veh: &RotorVehicle, line: &[[f64; 3]], dx: f64, dy: f64, depth: f64) -> f64 {
    if depth <= 0.0 || line.is_empty() {
        return 0.0;
    }
    let mut best = (f64::INFINITY, 0.0);
    for c in line {
        let d2 = (c[0] - dx).powi(2) + (c[1] - depth).powi(2);
        if d2 < best.0 {
            best = (d2, c[2]);
        }
    }
    let d = (best.0 + dy * dy).sqrt();
    let (w, r) = wake_profile(best.1, veh.v_i(), veh.r_eq());
    w * 2f64.powf(-(d / r).powi(2))
}

/// Ideal power ratio of a rotor holding the same thrust in descending air `w` vs. clean air.
pub fn power_ratio(veh: &RotorVehicle, w: f64) -> f64 {
    let v_h = veh.v_h();
    let v0 = veh.v_i();
    if v0 <= 0.0 {
        return 1.0;
    }
    let v1 = induced_velocity(v_h, veh.v, w);
    (w.max(0.0) + v1) / v0
}

/// Thrust ratio at fixed throttle (fixed pitch, fixed rpm) in descending air `w`, from
/// blade-element momentum theory: `C_T = (σa/2)(θ/3 − λ/2) = 2λ_i(λ_c + λ_i)`.
pub fn thrust_ratio_fixed_throttle(v_ref: f64, w: f64) -> f64 {
    if v_ref <= 0.0 || w <= 0.0 {
        return 1.0;
    }
    let s = BEMT_SIGMA_A_HALF;
    let lh = (BEMT_CT_HOVER / 2.0).sqrt();
    let theta3 = 2.0 * lh * lh / s + 0.5 * lh;
    let lc = w / v_ref * lh;
    let b = 2.0 * lc + 0.5 * s;
    let c = 0.5 * s * lc - s * theta3;
    let li = (-b + (b * b - 8.0 * c).sqrt()) / 4.0;
    (li.max(0.0) * (lc + li.max(0.0)) / (lh * lh)).clamp(0.0, 1.0)
}

/// Wake effect on one multirotor.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RotorEffect {
    /// Sum of the others' downward wake velocities at this vehicle [m/s].
    pub w: f64,
    /// Thrust lost at fixed throttle (fraction).
    pub thrust_loss: f64,
    /// Extra electrical power to hold altitude [W].
    pub d_power: f64,
}

/// Effect of all other vehicles' downwash on each vehicle (velocities superposed).
pub fn rotor_interactions(veh: &RotorVehicle, pos: &[V3]) -> Vec<RotorEffect> {
    let r = veh.r_eq();
    let s_max = (400.0 * r).max(10.0);
    let line = wake_centerline(veh, s_max);
    let v_ref = veh.v_i();
    let k = veh.elec_per_ideal();
    (0..pos.len())
        .map(|j| {
            let w: f64 = (0..pos.len())
                .filter(|i| *i != j)
                .map(|i| {
                    let dx = pos[j][0] - pos[i][0];
                    let dy = pos[j][1] - pos[i][1];
                    let depth = pos[i][2] - pos[j][2];
                    wake_velocity(veh, &line, dx, dy, depth)
                })
                .sum();
            let ratio = power_ratio(veh, w);
            RotorEffect {
                w,
                thrust_loss: 1.0 - thrust_ratio_fixed_throttle(v_ref, w),
                d_power: k * veh.thrust * v_ref * (ratio - 1.0),
            }
        })
        .collect()
}

/// Recommended minimum separations in hover so that the lower vehicle loses less than
/// [`SAFE_THRUST_LOSS`] of its thrust at fixed throttle: `(vertical directly below, horizontal at
/// any depth)` [m].
pub fn safe_separation(veh: &RotorVehicle) -> (f64, f64) {
    let hover = RotorVehicle { v: 0.0, ..*veh };
    let r = hover.r_eq().max(1e-3);
    let v_ref = hover.v_i();
    let line = wake_centerline(&hover, 1000.0 * r);
    let loss = |dy: f64, z: f64| {
        1.0 - thrust_ratio_fixed_throttle(v_ref, wake_velocity(&hover, &line, 0.0, dy, z))
    };
    // Vertical: loss decreases monotonically with depth on the axis.
    let mut z = r * 0.1;
    while z < 1000.0 * r && loss(0.0, z) > SAFE_THRUST_LOSS {
        z *= 1.05;
    }
    // Horizontal: worst depth for each offset.
    let worst = |d: f64| {
        (1..=200)
            .map(|i| loss(d, z * i as f64 / 200.0 * 1.2))
            .fold(0.0, f64::max)
    };
    let mut d = r * 0.1;
    while d < 100.0 * r && worst(d) > SAFE_THRUST_LOSS {
        d *= 1.05;
    }
    (z, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn biot_savart_segment_matches_analytic() {
        // Point at distance h from the middle of a segment of length L along y.
        let (g, h, l) = (2.0, 0.5, 3.0);
        let v = segment_velocity(
            [h, 0.0, 0.0],
            [0.0, -l / 2.0, 0.0],
            [0.0, l / 2.0, 0.0],
            g,
            0.0,
        );
        let cos1 = (l / 2.0) / ((l / 2.0).powi(2) + h * h).sqrt();
        let expect = g / (4.0 * PI * h) * (2.0 * cos1);
        assert!((v[2].abs() - expect).abs() < 1e-12, "{v:?} vs {expect}");
        assert!(v[0].abs() < 1e-14 && v[1].abs() < 1e-14);
        // Right-hand rule: Γ along +y, point at +x (aft) → velocity along y × x = −z.
        assert!(v[2] < 0.0);
        // Very long segment → infinite line vortex Γ / (2π h).
        let v = segment_velocity([h, 0.0, 0.0], [0.0, -1e5, 0.0], [0.0, 1e5, 0.0], g, 0.0);
        assert!((v[2].abs() - g / (2.0 * PI * h)).abs() < 1e-6);
        // Off-centre point: (cos θ1 − cos θ2) form.
        let p = [h, 2.0, 0.0];
        let v = segment_velocity(p, [0.0, -l / 2.0, 0.0], [0.0, l / 2.0, 0.0], g, 0.0);
        let c1 = (2.0 + l / 2.0) / ((2.0 + l / 2.0).powi(2) + h * h).sqrt();
        let c2 = (2.0 - l / 2.0) / ((2.0 - l / 2.0).powi(2) + h * h).sqrt();
        assert!((v[2].abs() - g / (4.0 * PI * h) * (c1 - c2)).abs() < 1e-12);
    }

    fn wing() -> WingVehicle {
        WingVehicle {
            span: 1.6,
            s_ref: 0.32,
            cla: 4.5,
            k: 0.04,
            weight: 20.0,
            rho: 1.2,
            v: 16.0,
            eta: 0.6,
            p_alone: 120.0,
        }
    }

    #[test]
    fn horseshoe_downwash_behind_and_upwash_outboard() {
        let v = wing();
        let hs = v.horseshoe([0.0, 0.0, 0.0]);
        let behind = hs.velocity([2.0 * v.span, 0.0, 0.0]);
        assert!(behind[2] < 0.0, "downwash behind: {behind:?}");
        // Far-wake downwash of an elliptic wing ≈ 2·CL·V/(π·AR) at the centre line (order).
        let tip_out = hs.velocity([2.0 * v.span, 0.75 * v.span, 0.0]);
        assert!(tip_out[2] > 0.0, "upwash outboard of the tip: {tip_out:?}");
        // Bound vortex ahead induces upwash in front of the wing.
        assert!(hs.velocity([-0.5 * v.span, 0.0, 0.0])[2] > 0.0);
        // Symmetric.
        let l = hs.velocity([2.0, -1.0, 0.0])[2];
        let r = hs.velocity([2.0, 1.0, 0.0])[2];
        assert!((l - r).abs() < 1e-12);
        assert!(hs.sink_rate() > 0.0);
    }

    #[test]
    fn v_formation_saves_drag_at_overlap_and_not_far_apart() {
        let v = wing();
        let close = wing_interactions(&v, &[[0.0, 0.0, 0.0], [2.0 * v.span, 0.85 * v.span, 0.0]]);
        assert!(close[1].d_drag_frac < -0.1, "{:?}", close[1]);
        assert!(close[1].d_power < 0.0);
        let far = wing_interactions(&v, &[[0.0, 0.0, 0.0], [2.0 * v.span, 30.0 * v.span, 0.0]]);
        assert!(far[1].d_drag_frac.abs() < 0.005, "{:?}", far[1]);
        // In trail, directly behind: penalty.
        let trail = wing_interactions(&v, &[[0.0, 0.0, 0.0], [2.0 * v.span, 0.0, 0.0]]);
        assert!(trail[1].d_drag_frac > 0.0);
        // Half-overlapping the tip vortex: strong roll.
        let half = wing_interactions(&v, &[[0.0, 0.0, 0.0], [2.0 * v.span, 0.4 * v.span, 0.0]]);
        assert!(half[1].cl_roll.abs() > close[1].cl_roll.abs() * 0.5);
        assert!(half[1].roll_hazard(), "{:?}", half[1]);
        assert!(!far[1].roll_hazard());
        let (best, frac) = best_lateral_spacing(&v, 2.0 * v.span);
        assert!(best > 0.6 && best < 1.0, "{best}");
        assert!(frac < -0.2);
    }

    fn quad() -> RotorVehicle {
        // ≈ 0.6 kg quad on four 5-inch props.
        RotorVehicle {
            thrust: 0.6 * 9.81,
            disk_area: 4.0 * PI * 0.0635 * 0.0635,
            rho: 1.225,
            v: 0.0,
            p_elec_hover: 120.0,
            p_alone: 125.0,
        }
    }

    #[test]
    fn induced_velocity_limits() {
        let q = quad();
        let vh = q.v_h();
        assert!((vh - (q.thrust / (2.0 * q.rho * q.disk_area)).sqrt()).abs() < 1e-12);
        assert!((induced_velocity(vh, 0.0, 0.0) - vh).abs() < 1e-9);
        // Axial climb: v = −Vc/2 + √((Vc/2)² + vh²).
        let vc: f64 = 3.0;
        let ex = -vc / 2.0 + ((vc / 2.0).powi(2) + vh * vh).sqrt();
        assert!((induced_velocity(vh, 0.0, vc) - ex).abs() < 1e-9);
        // High speed: v ≈ vh² / V.
        assert!((induced_velocity(vh, 60.0, 0.0) - vh * vh / 60.0).abs() < 0.02);
        assert!((thrust_ratio_fixed_throttle(vh, 0.0) - 1.0).abs() < 1e-12);
        assert!(thrust_ratio_fixed_throttle(vh, 1.0) < 0.97);
    }

    #[test]
    fn rotor_wake_loss_drops_with_vertical_and_horizontal_separation() {
        let q = quad();
        let loss = |dy: f64, dz: f64| {
            rotor_interactions(&q, &[[0.0, 0.0, 0.0], [0.0, dy, -dz]])[1].thrust_loss
        };
        let near = loss(0.0, 0.5);
        let mid = loss(0.0, 2.0);
        let far = loss(0.0, 10.0);
        assert!(near > mid && mid > far, "{near} {mid} {far}");
        assert!(near > 0.2);
        assert!(loss(0.3, 2.0) < mid && loss(1.0, 2.0) < loss(0.3, 2.0));
        assert!(loss(5.0, 2.0) < 1e-3);
        // The upper vehicle is not affected by the lower one.
        let e = rotor_interactions(&q, &[[0.0, 0.0, 0.0], [0.0, 0.0, -1.0]]);
        assert_eq!(e[0].w, 0.0);
        assert!(e[1].d_power > 0.0);
        let (zv, dh) = safe_separation(&q);
        assert!(zv > dh && dh > q.r_eq(), "{zv} {dh}");
        assert!(loss(0.0, zv * 1.01) <= SAFE_THRUST_LOSS + 1e-3);
    }

    #[test]
    fn forward_flight_carries_wake_aft() {
        let q = RotorVehicle { v: 10.0, ..quad() };
        let line = wake_centerline(&q, 20.0);
        let end = line.last().unwrap();
        assert!(end[0] > end[1], "wake should be swept aft: {end:?}");
        // A vehicle 3 m behind on the same level, slightly lower, sees more wake than one ahead.
        let behind = rotor_interactions(&q, &[[0.0, 0.0, 0.0], [3.0, 0.0, -0.8]])[1].w;
        let ahead = rotor_interactions(&q, &[[0.0, 0.0, 0.0], [-3.0, 0.0, -0.8]])[1].w;
        assert!(behind > ahead);
    }
}
