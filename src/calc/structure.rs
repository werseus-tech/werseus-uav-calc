// SPDX-License-Identifier: AGPL-3.0-or-later
//! Yapı: kök eğilme momenti, karbon boru kontrolü. Spec Bölüm 9.
use std::f64::consts::PI;

#[derive(Clone, Debug)]
pub struct StructIn {
    pub n_limit: f64,
    pub fs: f64,
    pub sigma_allow: f64,
    pub d_o: f64,
    pub d_i: f64,
    pub b: f64,
    pub w: f64,
}

#[derive(Clone, Debug, Default)]
pub struct StructOut {
    pub n_ult: f64,
    pub m_b_limit: f64,
    pub m_b_ult: f64,
    pub i_spar: f64,
    pub z_spar: f64,
    pub sigma_max: f64,
    pub ms: f64,
}

/// Eliptik yük dağılımı, yarım kanat kök momenti: M = n W b / (3π)
pub fn root_moment(n: f64, w: f64, b: f64) -> f64 {
    n * w * b / (3.0 * PI)
}

pub fn spar_i(d_o: f64, d_i: f64) -> f64 {
    PI / 64.0 * (d_o.powi(4) - d_i.powi(4))
}
pub fn spar_z(d_o: f64, d_i: f64) -> f64 {
    spar_i(d_o, d_i) / (d_o / 2.0)
}

pub fn structure(i: &StructIn) -> StructOut {
    let n_ult = i.n_limit * i.fs;
    let m_b_ult = root_moment(n_ult, i.w, i.b);
    let z = spar_z(i.d_o, i.d_i);
    let sigma = m_b_ult / z;
    StructOut {
        n_ult,
        m_b_limit: root_moment(i.n_limit, i.w, i.b),
        m_b_ult,
        i_spar: spar_i(i.d_o, i.d_i),
        z_spar: z,
        sigma_max: sigma,
        ms: i.sigma_allow / sigma - 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn near(a: f64, b: f64, tol: f64) {
        assert!(((a - b) / b).abs() < tol, "{a} vs {b}");
    }
    #[test]
    fn spec_structure() {
        let o = structure(&StructIn {
            n_limit: 3.8,
            fs: 1.5,
            sigma_allow: 5.0e8,
            d_o: 0.010,
            d_i: 0.008,
            b: 1.6,
            w: 19.61,
        });
        near(o.n_ult, 5.70, 0.001);
        near(o.m_b_ult, 19.0, 0.01);
        near(o.z_spar, 5.796e-8, 0.005);
        near(o.sigma_max, 327e6, 0.01);
        near(o.ms, 0.527, 0.01);
    }
    #[test]
    fn guards() {
        let o = structure(&StructIn {
            n_limit: 3.8,
            fs: 1.5,
            sigma_allow: 5e8,
            d_o: 0.01,
            d_i: 0.01,
            b: f64::NAN,
            w: 19.0,
        });
        assert!(!o.ms.is_finite() || o.ms.is_nan() || o.ms.is_infinite());
    }
}
