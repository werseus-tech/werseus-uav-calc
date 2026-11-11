// SPDX-License-Identifier: AGPL-3.0-or-later
//! Aerodinamik: CLα, CLmax, Oswald e, CD0 yığılımı (build-up), kutup eğrisi, karakteristik hızlar.
use super::atmosphere::{Atmo, mach, reynolds};
use std::f64::consts::PI;

#[derive(Clone, Debug)]
pub struct AeroIn {
    pub cl_alpha_2d: f64,
    pub cl_max_2d: f64,
    pub tc_w: f64,
    pub tc_t: f64,
    pub l_fuse: f64,
    pub d_fuse: f64,
    pub s_wet_fuse: f64,
    /// Toplam kuyruk planform alanı S_t (S_h + S_v).
    pub s_t: f64,
    pub c_tail: f64,
    pub cd_misc: f64,
    pub q_tail: f64,
}

#[derive(Clone, Debug)]
pub struct AeroGeom {
    pub ar: f64,
    pub mac: f64,
    pub s: f64,
    pub a_wet_w: f64,
    pub tan_sweep_half: f64,
}

#[derive(Clone, Debug, Default)]
pub struct DragItem {
    pub name: &'static str,
    pub re: f64,
    pub cf: f64,
    pub ff: f64,
    pub q: f64,
    pub s_wet: f64,
    pub cd0: f64,
}

#[derive(Clone, Debug, Default)]
pub struct AeroOut {
    pub eta_af: f64,
    pub cla_w: f64,
    pub cl_max: f64,
    pub e: f64,
    pub k: f64,
    pub items: Vec<DragItem>,
    pub cd0: f64,
    pub ld_max: f64,
    pub cl_range: f64,
    pub v_range: f64,
    /// Efektif CL_end (sınır uygulanmışsa 1.2·V_stall hızında).
    pub cl_end: f64,
    /// Sınırsız formül değeri √(3CD0/k).
    pub cl_end_unclamped: f64,
    pub ld_end: f64,
    pub v_end_unclamped: f64,
    pub v_stall: f64,
    pub v_min_loiter: f64,
    /// CL_end > 0.9 CLmax ise true: V_end = 1.2 V_stall uygulanır.
    pub end_clamped: bool,
    pub v_end: f64,
    pub mach: f64,
    pub re_wing: f64,
    pub rho: f64,
    pub ws: f64,
}

/// Tam türbülanslı sürtünme katsayısı (Schlichting/Raymer).
pub fn cf_turb(re: f64) -> f64 {
    if re.is_finite() && re > 10.0 {
        0.455 / re.log10().powf(2.58)
    } else {
        f64::NAN
    }
}

/// Laminer referans (kullanılmaz).
pub fn cf_lam(re: f64) -> f64 {
    1.328 / re.sqrt()
}

pub fn ff_wing(tc: f64) -> f64 {
    1.0 + 2.0 * tc + 60.0 * tc.powi(4)
}
pub fn ff_fuse(l: f64, d: f64) -> f64 {
    let f = l / d;
    1.0 + 60.0 / f.powi(3) + 0.0025 * f
}

/// Hoerner'e göre yaklaşık gövde ıslak alanı (kapalı gövde, l/d = f).
pub fn fuse_wetted_geometric(l: f64, d: f64) -> f64 {
    let f = l / d;
    PI * d * l * (1.0 - 2.0 / f).max(0.0).powf(2.0 / 3.0) * (1.0 + 1.0 / (f * f))
}

pub fn oswald_e(ar: f64) -> f64 {
    1.0 / (1.05 + 0.007 * PI * ar)
}

pub fn cl_alpha_wing(ar: f64, cl_alpha_2d: f64, mach_no: f64, tan_sweep_half: f64) -> f64 {
    let eta = cl_alpha_2d / (2.0 * PI);
    let beta2 = 1.0 - mach_no * mach_no;
    let beta = beta2.sqrt();
    let inner = (ar * beta / eta).powi(2) * (1.0 + tan_sweep_half.powi(2) / beta2);
    2.0 * PI * ar / (2.0 + (4.0 + inner).sqrt())
}

pub fn v_stall(ws: f64, rho: f64, cl_max: f64) -> f64 {
    (2.0 * ws / (rho * cl_max)).sqrt()
}
pub fn cl_for(ws: f64, rho: f64, v: f64) -> f64 {
    ws / (0.5 * rho * v * v)
}
pub fn cd_total(cd0: f64, k: f64, cl: f64) -> f64 {
    cd0 + k * cl * cl
}
pub fn ld_at(cd0: f64, k: f64, cl: f64) -> f64 {
    cl / cd_total(cd0, k, cl)
}

pub fn aero(g: &AeroGeom, i: &AeroIn, atmo: &Atmo, v: f64, w: f64) -> AeroOut {
    let ws = w / g.s;
    let rho = atmo.rho;
    let mu = atmo.mu;
    let m = mach(v, atmo.a);
    let eta_af = i.cl_alpha_2d / (2.0 * PI);
    let cla_w = cl_alpha_wing(g.ar, i.cl_alpha_2d, m, g.tan_sweep_half);
    let cl_max = 0.9 * i.cl_max_2d;
    let e = oswald_e(g.ar);
    let k = 1.0 / (PI * e * g.ar);

    let re_w = reynolds(rho, v, g.mac, mu);
    let re_f = reynolds(rho, v, i.l_fuse, mu);
    let re_t = reynolds(rho, v, i.c_tail, mu);
    let (cf_w, cf_f, cf_t) = (cf_turb(re_w), cf_turb(re_f), cf_turb(re_t));
    let ff_w = ff_wing(i.tc_w);
    let ff_f = ff_fuse(i.l_fuse, i.d_fuse);
    let ff_t = ff_wing(i.tc_t);
    let s_wet_t = 2.05 * i.s_t;
    let tail_cd0 = if i.s_t > 0.0 {
        cf_t * ff_t * i.q_tail * s_wet_t / g.s
    } else {
        0.0
    };
    let items = vec![
        DragItem {
            name: crate::i18n::t("drag_item.wing"),
            re: re_w,
            cf: cf_w,
            ff: ff_w,
            q: 1.0,
            s_wet: g.a_wet_w,
            cd0: cf_w * ff_w * g.a_wet_w / g.s,
        },
        DragItem {
            name: crate::i18n::t("drag_item.fuse"),
            re: re_f,
            cf: cf_f,
            ff: ff_f,
            q: 1.0,
            s_wet: i.s_wet_fuse,
            cd0: cf_f * ff_f * i.s_wet_fuse / g.s,
        },
        DragItem {
            name: crate::i18n::t("drag_item.tail"),
            re: re_t,
            cf: cf_t,
            ff: ff_t,
            q: i.q_tail,
            s_wet: s_wet_t,
            cd0: tail_cd0,
        },
        DragItem {
            name: crate::i18n::t("drag_item.misc"),
            re: f64::NAN,
            cf: f64::NAN,
            ff: f64::NAN,
            q: f64::NAN,
            s_wet: f64::NAN,
            cd0: i.cd_misc,
        },
    ];
    let cd0: f64 = items.iter().map(|x| x.cd0).sum();

    let ld_max = 1.0 / (2.0 * (cd0 * k).sqrt());
    let cl_range = (cd0 / k).sqrt();
    let cl_end = (3.0 * cd0 / k).sqrt();
    let vs = v_stall(ws, rho, cl_max);
    let v_end_un = (2.0 * ws / (rho * cl_end)).sqrt();
    let clamp = cl_end > 0.9 * cl_max;
    let v_min_loiter = 1.2 * vs;
    // Sınır uygulanırsa CL ve L/D sınırlı hızda yeniden hesaplanır (CL = 2W/(ρV²S) ≤ CLmax).
    let v_end = if clamp { v_min_loiter } else { v_end_un };
    let cl_end_eff = if clamp {
        cl_for(ws, rho, v_end)
    } else {
        cl_end
    };
    AeroOut {
        eta_af,
        cla_w,
        cl_max,
        e,
        k,
        items,
        cd0,
        ld_max,
        cl_range,
        v_range: (2.0 * ws / (rho * cl_range)).sqrt(),
        cl_end: cl_end_eff,
        cl_end_unclamped: cl_end,
        ld_end: ld_at(cd0, k, cl_end_eff),
        v_end_unclamped: v_end_un,
        v_stall: vs,
        v_min_loiter,
        end_clamped: clamp,
        v_end,
        mach: m,
        re_wing: re_w,
        rho,
        ws,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::atmosphere::isa;
    fn near(a: f64, b: f64, tol: f64) {
        assert!(((a - b) / b).abs() < tol, "{a} vs {b}");
    }
    fn spec() -> AeroOut {
        let g = AeroGeom {
            ar: 8.0,
            mac: 0.20417,
            s: 0.32,
            a_wet_w: 0.6528,
            tan_sweep_half: (1.4f64).to_radians().tan(),
        };
        let i = AeroIn {
            cl_alpha_2d: 5.95,
            cl_max_2d: 1.25,
            tc_w: 0.10,
            tc_t: 0.09,
            l_fuse: 0.9,
            d_fuse: 0.11,
            s_wet_fuse: 0.235,
            s_t: 0.083,
            c_tail: 0.10,
            cd_misc: 0.004,
            q_tail: 1.05,
        };
        // W/S = 61.3 -> W = 19.616 N
        aero(&g, &i, &isa(500.0), 18.0, 2.0 * 9.80665)
    }
    #[test]
    fn spec_polar() {
        let a = spec();
        near(a.eta_af, 0.947, 0.002);
        near(a.cla_w, 4.706, 0.005);
        near(a.cl_max, 1.125, 0.001);
        near(a.e, 0.816, 0.005);
        near(a.k, 0.04878, 0.005);
        near(a.items[0].re, 2.42e5, 0.01);
        near(a.items[1].re, 1.07e6, 0.01);
        near(a.items[2].re, 1.18e5, 0.01);
        near(a.items[0].cf, 0.00591, 0.01);
        near(a.items[1].cf, 0.00442, 0.01);
        near(a.items[2].cf, 0.00689, 0.01);
        near(a.items[0].ff, 1.206, 0.005);
        near(a.items[1].ff, 1.130, 0.005);
        near(a.items[2].ff, 1.184, 0.005);
        near(a.items[0].cd0, 0.0146, 0.01);
        near(a.items[1].cd0, 0.00367, 0.01);
        near(a.items[2].cd0, 0.00456, 0.01);
        near(a.cd0, 0.02678, 0.01);
        near(a.ld_max, 13.8, 0.01);
        near(a.cl_range, 0.741, 0.01);
        near(a.v_range, 11.9, 0.01);
        near(a.v_stall, 9.66, 0.01);
        near(a.cl_end_unclamped, 1.28, 0.01);
        assert!(a.end_clamped);
        near(a.v_end_unclamped, 9.05, 0.01);
        near(a.v_min_loiter, 11.6, 0.01);
        near(a.v_end, 11.6, 0.01);
        assert_eq!(a.v_end, a.v_min_loiter);
        // Sınırlı hızda CL = CLmax/1.44 = 0.781 ve L/D ≈ 13.8 (kutup eğrisinden)
        near(a.cl_end, 1.125 / 1.44, 0.005);
        assert!(a.cl_end <= a.cl_max);
        near(a.ld_end, 13.81, 0.01);
    }
    #[test]
    fn stall_msl_and_sensitivity() {
        near(v_stall(61.3, 1.225, 1.125), 9.43, 0.01);
        // Spec Madde 30 "Cl_max_2d = 1.00 -> V_stall = 10.4 m/s" der; kendi formülü (CLmax = 0.9 Cl_max_2d)
        // 10.80 m/s verir (10.4 yeniden üretilemedi). Formül esas alındı; her iki durumda 10 m/s şartı sağlanmaz.
        near(v_stall(61.3, 1.16727, 0.9), 10.80, 0.005);
    }
    #[test]
    fn fuse_geo() {
        near(fuse_wetted_geometric(0.9, 0.11), 0.261, 0.01);
    }
    #[test]
    fn cf_guard() {
        assert!(cf_turb(0.0).is_nan());
        assert!(cf_turb(f64::NAN).is_nan());
    }
}
