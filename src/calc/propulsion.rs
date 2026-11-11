// SPDX-License-Identifier: AGPL-3.0-or-later
//! İtki ve pervane. Spec Bölüm 7 (momentum teorisi + figure of merit).
use std::f64::consts::PI;

#[derive(Clone, Debug)]
pub struct PropIn {
    pub w: f64,
    /// Seyir L/D (kutup eğrisinden, seyir hızında).
    pub ld_cr: f64,
    pub v_cr: f64,
    pub eta_p_cr: f64,
    pub tw_static: f64,
    pub d_prop: f64,
    pub m_tip_max: f64,
    pub fm: f64,
    pub eta_mot: f64,
    pub eta_esc: f64,
    pub rho: f64,
    pub a_sound: f64,
    /// Kısıt analizinden tasarım elektrik gücü [W] (1.15 paylı).
    pub p_elec_design: f64,
    pub motor_margin: f64,
    pub p_installed_elec: f64,
    /// Pervane adımı [m] ve kayma (slip) oranı - seyir devri tahmini için.
    pub pitch: f64,
    pub slip: f64,
    pub pack_voltage: f64,
    pub pack_capacity_ah: f64,
    /// Paketin sürekli verebileceği akım [A] (paralel × hücre limiti).
    pub pack_max_cont_a: f64,
}

#[derive(Clone, Debug, Default)]
pub struct PropOut {
    pub t_cr: f64,
    pub p_shaft_cr: f64,
    pub p_elec_cr: f64,
    pub t_static: f64,
    pub a_disk: f64,
    pub disk_loading: f64,
    pub p_ideal: f64,
    pub p_shaft_static: f64,
    pub p_elec_static: f64,
    pub v_tip_max: f64,
    pub rpm_max: f64,
    pub p_motor_suggest: f64,
    pub i_peak: f64,
    pub c_rate: f64,
    pub rpm_cr: f64,
    pub rpm_ok: bool,
    pub current_ok: bool,
    pub installed_ok: bool,
}

pub fn propulsion(i: &PropIn) -> PropOut {
    let t_cr = i.w / i.ld_cr;
    let p_shaft_cr = t_cr * i.v_cr / i.eta_p_cr;
    let eta_el = i.eta_mot * i.eta_esc;
    let t_static = i.tw_static * i.w;
    let a_disk = PI / 4.0 * i.d_prop * i.d_prop;
    let p_ideal = t_static.powf(1.5) / (2.0 * i.rho * a_disk).sqrt();
    let p_shaft_static = p_ideal / i.fm;
    let p_elec_static = p_shaft_static / eta_el;
    let v_tip_max = i.m_tip_max * i.a_sound;
    let rpm_max = 60.0 * v_tip_max / (PI * i.d_prop);
    // Tepe elektrik gücü: statik kalkış ile seyir/tırmanış tasarım gücünden büyük olanı.
    let p_elec_peak = p_elec_static.max(i.p_elec_design);
    let p_motor_suggest = p_elec_peak * i.motor_margin;
    // Batarya akımı motor güç payı içermez (pay motor boyutlandırma içindir); gerilim nominal alınır (spec Bölüm 7).
    let i_peak = p_elec_peak / i.pack_voltage;
    let rpm_cr = i.v_cr / (1.0 - i.slip) / i.pitch * 60.0;
    PropOut {
        t_cr,
        p_shaft_cr,
        p_elec_cr: p_shaft_cr / eta_el,
        t_static,
        a_disk,
        disk_loading: t_static / a_disk,
        p_ideal,
        p_shaft_static,
        p_elec_static,
        v_tip_max,
        rpm_max,
        p_motor_suggest,
        i_peak,
        c_rate: i_peak / i.pack_capacity_ah,
        rpm_cr,
        rpm_ok: rpm_cr <= rpm_max,
        current_ok: i_peak <= i.pack_max_cont_a,
        installed_ok: i.p_installed_elec >= p_elec_peak,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::atmosphere::isa;
    fn near(a: f64, b: f64, tol: f64) {
        assert!(((a - b) / b).abs() < tol, "{a} vs {b}");
    }
    #[test]
    fn spec_propulsion() {
        let atm = isa(500.0);
        let o = propulsion(&PropIn {
            w: 19.6133,
            ld_cr: 10.15,
            v_cr: 18.0,
            eta_p_cr: 0.74,
            tw_static: 0.60,
            d_prop: 0.254,
            m_tip_max: 0.45,
            fm: 0.65,
            eta_mot: 0.85,
            eta_esc: 0.95,
            rho: 1.16727,
            a_sound: atm.a,
            p_elec_design: 213.0,
            motor_margin: 1.1,
            p_installed_elec: 240.0,
            pitch: 0.1524,
            slip: 0.2,
            pack_voltage: 14.4,
            pack_capacity_ah: 8.4,
            pack_max_cont_a: 90.0,
        });
        near(o.t_cr, 1.93, 0.01);
        near(o.p_shaft_cr, 47.0, 0.015);
        near(o.t_static, 11.8, 0.01);
        near(o.a_disk, 0.0507, 0.01);
        near(o.p_ideal, 117.0, 0.01);
        near(o.p_shaft_static, 181.0, 0.01);
        near(o.p_elec_static, 224.0, 0.01);
        near(atm.a, 338.0, 0.01);
        near(o.v_tip_max, 152.0, 0.01);
        near(o.rpm_max, 1.14e4, 0.01);
        assert!(o.rpm_ok);
    }
    fn base_in() -> PropIn {
        PropIn {
            w: 19.6133,
            ld_cr: 10.15,
            v_cr: 18.0,
            eta_p_cr: 0.74,
            tw_static: 0.60,
            d_prop: 0.254,
            m_tip_max: 0.45,
            fm: 0.65,
            eta_mot: 0.85,
            eta_esc: 0.95,
            rho: 1.16727,
            a_sound: 338.0,
            p_elec_design: 213.0,
            motor_margin: 1.1,
            p_installed_elec: 240.0,
            pitch: 0.1524,
            slip: 0.2,
            pack_voltage: 14.4,
            pack_capacity_ah: 8.4,
            pack_max_cont_a: 90.0,
        }
    }
    #[test]
    fn installed_check_includes_static_power() {
        // Seyir gücü yeterli (213 W) ama statik kalkış (≈224 W) değil: installed_ok false olmalı.
        let mut i = base_in();
        i.p_installed_elec = 220.0;
        let o = propulsion(&i);
        assert!(o.p_elec_static > 220.0);
        assert!(!o.installed_ok);
        i.p_installed_elec = 240.0;
        assert!(propulsion(&i).installed_ok);
    }
    #[test]
    fn peak_current_excludes_motor_margin() {
        let a = propulsion(&base_in());
        let mut i = base_in();
        i.motor_margin = 1.0;
        let b = propulsion(&i);
        assert!(
            (a.i_peak - b.i_peak).abs() < 1e-12,
            "motor payı akıma girmemeli"
        );
        assert!((a.i_peak - a.p_elec_static.max(213.0) / 14.4).abs() < 1e-9);
        assert!((a.i_peak - 15.6).abs() < 0.1, "{}", a.i_peak);
    }
}
