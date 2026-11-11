// SPDX-License-Identifier: AGPL-3.0-or-later
//! Elektrikli uçak havada kalma süresi ve menzil. Spec Bölüm 6.
use super::aero::{cd_total, cl_for};

#[derive(Clone, Debug)]
pub struct PerfIn {
    pub e_pack_wh: f64,
    pub dod: f64,
    pub eta_esc: f64,
    pub eta_mot: f64,
    pub eta_prop: f64,
    /// Sabit (otel) yük: aviyonik + faydalı yük [W].
    pub p_hotel: f64,
    pub cd0: f64,
    pub k: f64,
    pub rho: f64,
    pub ws: f64,
    pub w: f64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PerfPoint {
    pub v: f64,
    pub cl: f64,
    pub ld: f64,
    pub p_aero: f64,
    pub p_prop_elec: f64,
    pub p_total: f64,
    pub endurance_s: f64,
    pub range_m: f64,
}

impl PerfPoint {
    pub fn endurance_min(&self) -> f64 {
        self.endurance_s / 60.0
    }
    pub fn endurance_h(&self) -> f64 {
        self.endurance_s / 3600.0
    }
    pub fn range_km(&self) -> f64 {
        self.range_m / 1000.0
    }
}

pub fn eta_pt(i: &PerfIn) -> f64 {
    i.eta_esc * i.eta_mot * i.eta_prop
}

pub fn e_usable_j(i: &PerfIn) -> f64 {
    i.e_pack_wh * i.dod * 3600.0
}

pub fn at_speed(v: f64, i: &PerfIn) -> PerfPoint {
    let cl = cl_for(i.ws, i.rho, v);
    let cd = cd_total(i.cd0, i.k, cl);
    let ld = cl / cd;
    let p_aero = i.w * v / ld;
    let p_prop_elec = p_aero / eta_pt(i);
    let p_total = p_prop_elec + i.p_hotel;
    let endurance_s = e_usable_j(i) / p_total;
    PerfPoint {
        v,
        cl,
        ld,
        p_aero,
        p_prop_elec,
        p_total,
        endurance_s,
        range_m: v * endurance_s,
    }
}

/// Kapalı form menzil (spec 6.2); `at_speed().range_m` ile aynı olmalıdır.
pub fn range_closed(v: f64, i: &PerfIn) -> f64 {
    let p = at_speed(v, i);
    (e_usable_j(i) / i.w) * p.ld * eta_pt(i) / (1.0 + i.p_hotel / p.p_prop_elec)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn near(a: f64, b: f64, tol: f64) {
        assert!(((a - b) / b).abs() < tol, "{a} vs {b}");
    }
    #[test]
    fn spec_endurance() {
        let i = PerfIn {
            e_pack_wh: 121.0,
            dod: 0.8,
            eta_esc: 0.95,
            eta_mot: 0.85,
            eta_prop: 0.74,
            p_hotel: 14.0,
            cd0: 0.02678,
            k: 0.04878,
            rho: 1.16727,
            ws: 61.3,
            w: 19.6133,
        };
        near(eta_pt(&i), 0.598, 0.01);
        near(e_usable_j(&i), 3.48e5, 0.01);
        let p = at_speed(18.0, &i);
        near(p.p_aero, 34.7, 0.01);
        near(p.p_prop_elec, 58.1, 0.01);
        near(p.p_total, 72.1, 0.01);
        near(p.endurance_s, 4830.0, 0.01);
        near(p.endurance_h(), 1.34, 0.01);
        near(p.range_km(), 86.9, 0.01);
        near(range_closed(18.0, &i), p.range_m, 1e-9);
    }
}
