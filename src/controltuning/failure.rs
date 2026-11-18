// SPDX-License-Identifier: AGPL-3.0-or-later
//! Motor-loss controllability matrix.
//!
//! Method: attainable control set and the *available control authority index* (ACAI) of
//! G.-X. Du, Q. Quan, B. Yang and K.-Y. Cai, "Controllability Analysis for Multirotor Helicopter
//! Rotor Degradation and Failure", *Journal of Guidance, Control, and Dynamics* 38(5),
//! 978–985, 2015 (doi:10.2514/1.G000731).
//!
//! Each rotor `i` contributes the generator `b_i = T_max,i · [1, −y_i, x_i, s_i·k_Q]` (thrust,
//! roll, pitch, yaw) and can be commanded anywhere in `[0, T_max,i]`; a failed rotor is removed.
//! The attainable set `Ω = {Σ u_i·b_i}` is a zonotope. The ACAI is the signed distance from the
//! hover point `G = [W, 0, 0, 0]` to the boundary of `Ω`: positive means every direction of
//! control is still available around hover (Du et al. prove that with the rank condition this is
//! equivalent to controllability of the linearised hover dynamics). Facet normals of a zonotope
//! in ℝ^d are the normals of all (d−1)-subsets of generators, and the support function is
//! `h(n) = Σ max(0, n·b_i)`, so `ρ = min_n (h(n) − n·G)` over those normals.
//!
//! Classification per failure case (ρ normalised, "> 0" meaning above [`MIN_MARGIN`] = 2 %):
//! * full — ρ > 0 in (thrust, roll, pitch, yaw): hover with full attitude and yaw control;
//! * no yaw — ρ ≤ 0 in 4-D but ρ > 0 in the (thrust, roll, pitch) projection: hover with
//!   attitude control possible, but no (or zero-margin) yaw authority. The standard PNPNPN
//!   hexarotor falls here after one rotor loss, exactly as reported by Du et al.;
//! * no hover — ρ ≤ 0 even with yaw free: no static hover with attitude control. A quadrotor
//!   falls here; it can only survive in a spinning, reduced-attitude mode
//!   (Mueller & D'Andrea, "Stability and control of a quadrocopter despite the complete loss of
//!   one, two, or three propellers", ICRA 2014), which needs special firmware.
//!
//! The axes are normalised (thrust by W, roll/pitch by W·L_ref, yaw by W·k_Q) so the index is
//! comparable between vehicles; the sign — the decision — does not depend on the scaling.
use super::vehicle::Rotor;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoverClass {
    Full,
    NoYaw,
    NoHover,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FailureCase {
    /// Indices of the failed rotors (empty = healthy vehicle).
    pub failed: Vec<usize>,
    pub class: HoverClass,
    /// Normalised ACAI in 4-D (with yaw) and 3-D (yaw free).
    pub acai4: f64,
    pub acai3: f64,
    /// Maximum thrust with zero moments divided by the weight, minus one (fraction).
    pub thrust_margin: f64,
    /// Remaining authority about hover per axis (roll, pitch, yaw) and direction (−, +), as a
    /// fraction of the healthy vehicle's authority.
    pub authority: [[f64; 2]; 3],
    /// Axes with (practically) no authority left in at least one direction: 0 roll, 1 pitch,
    /// 2 yaw.
    pub lost_axes: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FailureReport {
    pub healthy: FailureCase,
    pub singles: Vec<FailureCase>,
    pub pairs: Vec<FailureCase>,
}

const EPS: f64 = 1e-6;
/// Minimum normalised ACAI that counts as usable control margin. Smaller positive values are
/// within the model's own uncertainty (CG position, thrust spread between motors), so a hover
/// point that close to the boundary is treated as lost. Same threshold for axis authority as a
/// fraction of the healthy vehicle's authority.
pub const MIN_MARGIN: f64 = 0.02;

/// Generalised cross product of `d − 1` vectors in ℝ^d (d = 3 or 4).
fn normal(vs: &[[f64; 4]], d: usize) -> Option<[f64; 4]> {
    let mut n = [0.0; 4];
    if d == 3 {
        let (a, b) = (vs[0], vs[1]);
        n[0] = a[1] * b[2] - a[2] * b[1];
        n[1] = a[2] * b[0] - a[0] * b[2];
        n[2] = a[0] * b[1] - a[1] * b[0];
    } else {
        // Cofactor expansion of the 4×4 determinant with the unknown row on top.
        let det3 = |c: [usize; 3]| {
            let m = |r: usize, k: usize| vs[r][c[k]];
            m(0, 0) * (m(1, 1) * m(2, 2) - m(1, 2) * m(2, 1))
                - m(0, 1) * (m(1, 0) * m(2, 2) - m(1, 2) * m(2, 0))
                + m(0, 2) * (m(1, 0) * m(2, 1) - m(1, 1) * m(2, 0))
        };
        n[0] = det3([1, 2, 3]);
        n[1] = -det3([0, 2, 3]);
        n[2] = det3([0, 1, 3]);
        n[3] = -det3([0, 1, 2]);
    }
    let len = n.iter().map(|v| v * v).sum::<f64>().sqrt();
    let scale = vs
        .iter()
        .map(|v| v.iter().map(|x| x * x).sum::<f64>().sqrt())
        .product::<f64>();
    if len <= 1e-9 * scale.max(1e-12) {
        return None;
    }
    Some(n.map(|v| v / len))
}

fn dot(a: &[f64; 4], b: &[f64; 4], d: usize) -> f64 {
    (0..d).map(|k| a[k] * b[k]).sum()
}

/// All unit facet-normal candidates (both signs) of the zonotope spanned by `gens` in ℝ^d.
fn normals(gens: &[[f64; 4]], d: usize) -> Vec<[f64; 4]> {
    let n = gens.len();
    let mut out = Vec::new();
    let mut push = |v: Option<[f64; 4]>| {
        if let Some(v) = v {
            out.push(v);
            out.push(v.map(|x| -x));
        }
    };
    if d == 3 {
        for i in 0..n {
            for j in i + 1..n {
                push(normal(&[gens[i], gens[j]], 3));
            }
        }
    } else {
        for i in 0..n {
            for j in i + 1..n {
                for k in j + 1..n {
                    push(normal(&[gens[i], gens[j], gens[k]], 4));
                }
            }
        }
    }
    out
}

fn support(gens: &[[f64; 4]], n: &[f64; 4], d: usize) -> f64 {
    gens.iter().map(|g| dot(n, g, d).max(0.0)).sum()
}

/// Rank test: the zonotope must be full-dimensional, otherwise ρ = 0 by definition. With rank d
/// no candidate normal is orthogonal to every generator; with rank d − 1 the normal of the span
/// is, and with lower rank there are no candidate normals at all.
fn full_dim(gens: &[[f64; 4]], ns: &[[f64; 4]], d: usize) -> bool {
    !ns.is_empty()
        && ns
            .iter()
            .all(|n| gens.iter().any(|g| dot(n, g, d).abs() > 1e-9))
}

struct SetEval {
    rho: f64,
    normals: Vec<[f64; 4]>,
    support: Vec<f64>,
}

fn eval_set(gens: &[[f64; 4]], d: usize, g: &[f64; 4]) -> SetEval {
    let ns = normals(gens, d);
    if !full_dim(gens, &ns, d) {
        return SetEval {
            rho: 0.0,
            normals: Vec::new(),
            support: Vec::new(),
        };
    }
    let hs: Vec<f64> = ns.iter().map(|n| support(gens, n, d)).collect();
    let rho = ns
        .iter()
        .zip(&hs)
        .map(|(n, h)| h - dot(n, g, d))
        .fold(f64::INFINITY, f64::min);
    SetEval {
        rho: if rho.is_finite() { rho } else { 0.0 },
        normals: ns,
        support: hs,
    }
}

/// Largest step from `g` along `±e_axis` that stays inside the set (0 if `g` is outside).
fn reach(s: &SetEval, g: &[f64; 4], axis: usize, sign: f64, d: usize) -> f64 {
    if s.normals.is_empty() || s.rho < -EPS {
        return 0.0;
    }
    let mut best = f64::INFINITY;
    for (n, h) in s.normals.iter().zip(&s.support) {
        let c = sign * n[axis];
        if c > 1e-12 {
            best = best.min(((h - dot(n, g, d)) / c).max(0.0));
        }
    }
    if best.is_finite() { best } else { 0.0 }
}

/// Maximum normalised thrust with zero moments (projection-free line through the origin).
fn max_thrust(s: &SetEval) -> f64 {
    let mut best = f64::INFINITY;
    for (n, h) in s.normals.iter().zip(&s.support) {
        if n[0] > 1e-12 {
            best = best.min(h / n[0]);
        }
    }
    if best.is_finite() { best } else { 0.0 }
}

fn generators(rotors: &[Rotor], failed: &[usize], weight: f64, l_ref: f64) -> Vec<[f64; 4]> {
    let w = weight.max(1e-9);
    let l = l_ref.max(1e-6);
    rotors
        .iter()
        .enumerate()
        .filter(|(i, _)| !failed.contains(i))
        .map(|(_, r)| {
            let t = r.t_max.max(0.0) / w;
            // Yaw normalised by k_Q, so the generator's yaw entry is just ±t.
            [t, -r.y * t / l, r.x * t / l, r.spin * t]
        })
        .collect()
}

fn evaluate_case(
    rotors: &[Rotor],
    failed: Vec<usize>,
    weight: f64,
    l_ref: f64,
    healthy_auth: Option<&[[f64; 2]; 3]>,
) -> FailureCase {
    let gens = generators(rotors, &failed, weight, l_ref);
    let g = [1.0, 0.0, 0.0, 0.0];
    let s4 = eval_set(&gens, 4, &g);
    let s3 = eval_set(&gens, 3, &g);
    let class = if s4.rho > MIN_MARGIN {
        HoverClass::Full
    } else if s3.rho > MIN_MARGIN {
        HoverClass::NoYaw
    } else {
        HoverClass::NoHover
    };
    let mut auth = [[0.0; 2]; 3];
    for (axis, a) in auth.iter_mut().enumerate() {
        for (k, sign) in [-1.0, 1.0].into_iter().enumerate() {
            a[k] = match (class, axis) {
                (HoverClass::Full, _) => reach(&s4, &g, axis + 1, sign, 4),
                (HoverClass::NoYaw, 2) => reach(&s4, &g, 3, sign, 4),
                (HoverClass::NoYaw, _) => reach(&s3, &g, axis + 1, sign, 3),
                (HoverClass::NoHover, 2) => 0.0,
                (HoverClass::NoHover, _) => reach(&s3, &g, axis + 1, sign, 3),
            };
        }
    }
    if let Some(h) = healthy_auth {
        for (a, hh) in auth.iter_mut().zip(h) {
            for (v, hv) in a.iter_mut().zip(hh) {
                *v = if *hv > EPS { *v / hv } else { 0.0 };
            }
        }
    }
    let mut lost: Vec<usize> = (0..3)
        .filter(|&k| auth[k][0].min(auth[k][1]) <= MIN_MARGIN)
        .collect();
    if class != HoverClass::Full && !lost.contains(&2) {
        lost.push(2);
    }
    let thrust_line = if class == HoverClass::Full {
        max_thrust(&s4)
    } else {
        max_thrust(&s3)
    };
    FailureCase {
        failed,
        class,
        acai4: s4.rho,
        acai3: s3.rho,
        thrust_margin: thrust_line - 1.0,
        authority: auth,
        lost_axes: lost,
    }
}

/// Builds the motor-loss matrix. `l_ref` is the moment normalisation length (mean arm).
pub fn analyze_failures(
    rotors: &[Rotor],
    weight: f64,
    l_ref: f64,
    include_pairs: bool,
) -> FailureReport {
    let mut healthy = evaluate_case(rotors, Vec::new(), weight, l_ref, None);
    let h_auth = healthy.authority;
    healthy.authority = h_auth.map(|a| a.map(|v| if v > EPS { 1.0 } else { 0.0 }));
    let singles = (0..rotors.len())
        .map(|i| evaluate_case(rotors, vec![i], weight, l_ref, Some(&h_auth)))
        .collect();
    let mut pairs = Vec::new();
    if include_pairs {
        for i in 0..rotors.len() {
            for j in i + 1..rotors.len() {
                pairs.push(evaluate_case(
                    rotors,
                    vec![i, j],
                    weight,
                    l_ref,
                    Some(&h_auth),
                ));
            }
        }
    }
    FailureReport {
        healthy,
        singles,
        pairs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ring(n: usize, offset: f64, t_max: f64) -> Vec<Rotor> {
        (0..n)
            .map(|i| {
                let a = offset + i as f64 * 2.0 * std::f64::consts::PI / n as f64;
                Rotor {
                    x: 0.3 * a.cos(),
                    y: 0.3 * a.sin(),
                    spin: if i % 2 == 0 { 1.0 } else { -1.0 },
                    t_max,
                    lower: false,
                }
            })
            .collect()
    }

    #[test]
    fn healthy_quad_is_fully_controllable() {
        let r = ring(4, std::f64::consts::FRAC_PI_4, 10.0);
        let rep = analyze_failures(&r, 20.0, 0.3, false);
        assert_eq!(rep.healthy.class, HoverClass::Full);
        assert!((rep.healthy.thrust_margin - 1.0).abs() < 1e-6);
    }

    #[test]
    fn quad_single_loss_cannot_hover_statically() {
        let r = ring(4, std::f64::consts::FRAC_PI_4, 10.0);
        let rep = analyze_failures(&r, 20.0, 0.3, true);
        for c in &rep.singles {
            assert_eq!(c.class, HoverClass::NoHover, "{c:?}");
        }
    }

    #[test]
    fn standard_hexa_single_loss_keeps_hover_but_loses_yaw_margin() {
        // Du et al. 2015: PNPNPN hexarotor, one rotor out → ACAI = 0 (yaw), attitude kept.
        let r = ring(6, std::f64::consts::PI / 6.0, 10.0);
        let rep = analyze_failures(&r, 30.0, 0.3, false);
        for c in &rep.singles {
            assert_eq!(c.class, HoverClass::NoYaw, "{c:?}");
            assert!(c.acai3 > 0.0);
        }
    }

    #[test]
    fn underpowered_vehicle_cannot_hover_even_healthy() {
        let r = ring(4, std::f64::consts::FRAC_PI_4, 4.0);
        let rep = analyze_failures(&r, 20.0, 0.3, false);
        assert_eq!(rep.healthy.class, HoverClass::NoHover);
        assert!(rep.healthy.thrust_margin < 0.0);
    }
}
