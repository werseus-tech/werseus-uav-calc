// SPDX-License-Identifier: AGPL-3.0-or-later
//! Small f64 vector / quaternion helpers for the rigid-body simulation.
//!
//! Frames used across `sim`: world is NED (x north, y east, z down), body is FRD
//! (x forward, y right, z down). A quaternion `q` maps body vectors to world vectors.

use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct V3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl V3 {
    pub const ZERO: V3 = V3::new(0.0, 0.0, 0.0);

    pub const fn new(x: f64, y: f64, z: f64) -> V3 {
        V3 { x, y, z }
    }

    pub fn dot(self, o: V3) -> f64 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    pub fn cross(self, o: V3) -> V3 {
        V3::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }

    pub fn norm(self) -> f64 {
        self.dot(self).sqrt()
    }

    pub fn normalized_or(self, fallback: V3) -> V3 {
        let n = self.norm();
        if n > 1e-12 && n.is_finite() {
            self / n
        } else {
            fallback
        }
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }

    /// Scales the vector down so its length does not exceed `max`.
    pub fn clamp_norm(self, max: f64) -> V3 {
        let n = self.norm();
        if n > max && n.is_finite() {
            self * (max / n)
        } else {
            self
        }
    }

    pub fn mul_elem(self, o: V3) -> V3 {
        V3::new(self.x * o.x, self.y * o.y, self.z * o.z)
    }
}

impl Add for V3 {
    type Output = V3;
    fn add(self, o: V3) -> V3 {
        V3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}
impl AddAssign for V3 {
    fn add_assign(&mut self, o: V3) {
        *self = *self + o;
    }
}
impl Sub for V3 {
    type Output = V3;
    fn sub(self, o: V3) -> V3 {
        V3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}
impl Neg for V3 {
    type Output = V3;
    fn neg(self) -> V3 {
        V3::new(-self.x, -self.y, -self.z)
    }
}
impl Mul<f64> for V3 {
    type Output = V3;
    fn mul(self, s: f64) -> V3 {
        V3::new(self.x * s, self.y * s, self.z * s)
    }
}
impl Div<f64> for V3 {
    type Output = V3;
    fn div(self, s: f64) -> V3 {
        V3::new(self.x / s, self.y / s, self.z / s)
    }
}

/// Unit quaternion (w, x, y, z), body -> world rotation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quat {
    pub w: f64,
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Default for Quat {
    fn default() -> Self {
        Quat::IDENTITY
    }
}

impl Quat {
    pub const IDENTITY: Quat = Quat {
        w: 1.0,
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    /// From 3-2-1 Euler angles (yaw psi, pitch theta, roll phi) in radians.
    pub fn from_euler(phi: f64, theta: f64, psi: f64) -> Quat {
        let (sr, cr) = (0.5 * phi).sin_cos();
        let (sp, cp) = (0.5 * theta).sin_cos();
        let (sy, cy) = (0.5 * psi).sin_cos();
        Quat {
            w: cr * cp * cy + sr * sp * sy,
            x: sr * cp * cy - cr * sp * sy,
            y: cr * sp * cy + sr * cp * sy,
            z: cr * cp * sy - sr * sp * cy,
        }
    }

    /// Returns (roll phi, pitch theta, yaw psi) in radians.
    pub fn to_euler(self) -> (f64, f64, f64) {
        let Quat { w, x, y, z } = self;
        let phi = (2.0 * (w * x + y * z)).atan2(1.0 - 2.0 * (x * x + y * y));
        let theta = (2.0 * (w * y - z * x)).clamp(-1.0, 1.0).asin();
        let psi = (2.0 * (w * z + x * y)).atan2(1.0 - 2.0 * (y * y + z * z));
        (phi, theta, psi)
    }

    pub fn hamilton(self, o: Quat) -> Quat {
        Quat {
            w: self.w * o.w - self.x * o.x - self.y * o.y - self.z * o.z,
            x: self.w * o.x + self.x * o.w + self.y * o.z - self.z * o.y,
            y: self.w * o.y - self.x * o.z + self.y * o.w + self.z * o.x,
            z: self.w * o.z + self.x * o.y - self.y * o.x + self.z * o.w,
        }
    }

    pub fn conj(self) -> Quat {
        Quat {
            w: self.w,
            x: -self.x,
            y: -self.y,
            z: -self.z,
        }
    }

    pub fn normalized(self) -> Quat {
        let n = (self.w * self.w + self.x * self.x + self.y * self.y + self.z * self.z).sqrt();
        if n > 1e-12 && n.is_finite() {
            Quat {
                w: self.w / n,
                x: self.x / n,
                y: self.y / n,
                z: self.z / n,
            }
        } else {
            Quat::IDENTITY
        }
    }

    pub fn is_finite(self) -> bool {
        self.w.is_finite() && self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }

    /// Body -> world.
    pub fn rotate(self, v: V3) -> V3 {
        let u = V3::new(self.x, self.y, self.z);
        let t = u.cross(v) * 2.0;
        v + t * self.w + u.cross(t)
    }

    /// World -> body.
    pub fn rotate_inv(self, v: V3) -> V3 {
        self.conj().rotate(v)
    }

    /// Time derivative for body angular rate `w` [rad/s].
    pub fn derivative(self, w: V3) -> Quat {
        let o = Quat {
            w: 0.0,
            x: w.x,
            y: w.y,
            z: w.z,
        };
        let d = self.hamilton(o);
        Quat {
            w: 0.5 * d.w,
            x: 0.5 * d.x,
            y: 0.5 * d.y,
            z: 0.5 * d.z,
        }
    }

    pub fn add_scaled(self, d: Quat, h: f64) -> Quat {
        Quat {
            w: self.w + d.w * h,
            x: self.x + d.x * h,
            y: self.y + d.y * h,
            z: self.z + d.z * h,
        }
    }
}

/// Symmetric 3x3 inertia tensor about the CG in body axes (FRD) [kg m^2].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Inertia {
    pub ixx: f64,
    pub iyy: f64,
    pub izz: f64,
    /// Product of inertia (positive by the usual aircraft convention, I = [[Ixx,0,-Ixz],...]).
    pub ixz: f64,
}

impl Inertia {
    /// I * w
    pub fn apply(&self, w: V3) -> V3 {
        V3::new(
            self.ixx * w.x - self.ixz * w.z,
            self.iyy * w.y,
            self.izz * w.z - self.ixz * w.x,
        )
    }

    /// I^-1 * m (closed form for the x-z coupled block).
    pub fn solve(&self, m: V3) -> V3 {
        let det = self.ixx * self.izz - self.ixz * self.ixz;
        let det = if det.abs() < 1e-12 { 1e-12 } else { det };
        V3::new(
            (self.izz * m.x + self.ixz * m.z) / det,
            m.y / self.iyy.max(1e-9),
            (self.ixz * m.x + self.ixx * m.z) / det,
        )
    }

    pub fn scaled(&self, k: f64) -> Inertia {
        Inertia {
            ixx: self.ixx * k,
            iyy: self.iyy * k,
            izz: self.izz * k,
            ixz: self.ixz * k,
        }
    }

    pub fn diag(&self) -> V3 {
        V3::new(self.ixx, self.iyy, self.izz)
    }

    pub fn is_valid(&self) -> bool {
        let ok = |v: f64| v.is_finite() && v > 0.0;
        ok(self.ixx)
            && ok(self.iyy)
            && ok(self.izz)
            && self.ixz.is_finite()
            && self.ixx * self.izz > self.ixz * self.ixz
    }
}

/// Small deterministic xorshift generator (no external dependency, reproducible runs).
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// Uniform in [-1, 1).
    pub fn uniform(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 52) as f64 - 1.0
    }

    /// Approximately standard normal (sum of four uniforms, variance-scaled).
    pub fn normal(&mut self) -> f64 {
        let s: f64 = (0..4).map(|_| self.uniform()).sum();
        s * (3.0_f64 / 4.0).sqrt()
    }
}

pub fn wrap_pi(a: f64) -> f64 {
    let mut a = a % std::f64::consts::TAU;
    if a > std::f64::consts::PI {
        a -= std::f64::consts::TAU;
    } else if a < -std::f64::consts::PI {
        a += std::f64::consts::TAU;
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn euler_roundtrip_and_rotation() {
        let q = Quat::from_euler(0.3, -0.2, 1.1);
        let (p, t, y) = q.to_euler();
        assert!((p - 0.3).abs() < 1e-12 && (t + 0.2).abs() < 1e-12 && (y - 1.1).abs() < 1e-12);
        // Pure yaw of 90 deg maps body x (forward) to world y (east).
        let q = Quat::from_euler(0.0, 0.0, std::f64::consts::FRAC_PI_2);
        let v = q.rotate(V3::new(1.0, 0.0, 0.0));
        assert!((v.y - 1.0).abs() < 1e-12 && v.x.abs() < 1e-12);
        let back = q.rotate_inv(v);
        assert!((back.x - 1.0).abs() < 1e-12);
    }

    #[test]
    fn inertia_solve_inverts_mul() {
        let i = Inertia {
            ixx: 0.2,
            iyy: 0.3,
            izz: 0.45,
            ixz: 0.02,
        };
        let w = V3::new(0.7, -1.2, 2.0);
        let back = i.solve(i.apply(w));
        assert!((back - w).norm() < 1e-12);
    }

    #[test]
    fn rng_is_deterministic_and_bounded() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        for _ in 0..1000 {
            let x = a.uniform();
            assert_eq!(x, b.uniform());
            assert!((-1.0..1.0).contains(&x));
        }
    }
}
