// SPDX-License-Identifier: AGPL-3.0-or-later
//! ISA troposfer (0-11000 m), Sutherland viskozitesi, Reynolds sayısı. SI birimleri.

pub const T0: f64 = 288.15;
pub const P0: f64 = 101_325.0;
pub const RHO0: f64 = 1.2250;
pub const LAPSE: f64 = 0.0065;
pub const G: f64 = 9.80665;
pub const R_AIR: f64 = 287.05287;
pub const MU0: f64 = 1.7894e-5;
pub const S_MU: f64 = 110.4;
pub const GAMMA: f64 = 1.4;

#[derive(Clone, Copy, Debug, Default)]
pub struct Atmo {
    pub h: f64,
    pub t: f64,
    pub p: f64,
    pub rho: f64,
    pub mu: f64,
    pub nu: f64,
    pub a: f64,
}

/// ISA değerleri. h [0, 11000] m aralığına kısıtlanır; NaN girişte NaN döner.
pub fn isa(h: f64) -> Atmo {
    let h = h.clamp(0.0, 11_000.0);
    let t = T0 - LAPSE * h;
    let p = P0 * (1.0 - LAPSE * h / T0).powf(G / (R_AIR * LAPSE));
    let rho = p / (R_AIR * t);
    let mu = MU0 * (t / T0).powf(1.5) * (T0 + S_MU) / (t + S_MU);
    Atmo {
        h,
        t,
        p,
        rho,
        mu,
        nu: mu / rho,
        a: (GAMMA * R_AIR * t).sqrt(),
    }
}

pub fn reynolds(rho: f64, v: f64, c: f64, mu: f64) -> f64 {
    rho * v * c / mu
}

pub fn mach(v: f64, a: f64) -> f64 {
    v / a
}

#[cfg(test)]
mod tests {
    use super::*;
    fn near(a: f64, b: f64, tol: f64) {
        assert!(((a - b) / b).abs() < tol, "{a} vs {b}");
    }
    #[test]
    fn isa_500m() {
        let a = isa(500.0);
        near(a.t, 284.9, 0.01);
        near(a.p, 9.55e4, 0.01);
        near(a.rho, 1.16727, 0.005);
        near(a.mu, 1.77e-5, 0.01);
        near(a.nu, 1.52e-5, 0.01);
        near(reynolds(a.rho, 18.0, 0.2042, a.mu), 2.42e5, 0.01);
        near(mach(18.0, a.a), 0.053, 0.02);
    }
    #[test]
    fn isa_msl() {
        let a = isa(0.0);
        near(a.rho, 1.225, 0.001);
        near(a.mu, 1.79e-5, 0.01);
    }
    #[test]
    fn isa_nan_safe() {
        assert!(isa(f64::NAN).rho.is_nan());
    }
}
