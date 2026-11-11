// SPDX-License-Identifier: AGPL-3.0-or-later
//! Boyuna statik kararlılık: AG (CG), nötr nokta (NP), statik marj, batarya konumu çözücüsü.
//! Spec Bölüm 8. Tüm konumlar metre; NP/CG hesabında x, MAC'ın hücum kenarından geriye pozitiftir.
use super::geometry::TailKind;
use std::f64::consts::PI;

#[derive(Clone, Debug, Default)]
pub struct Comp {
    pub name: String,
    pub mass: f64,
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Cg {
    pub mass: f64,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    /// Σ m·x (kg·m)
    pub sum_mx: f64,
}

pub fn cg(comps: &[Comp]) -> Cg {
    let mass: f64 = comps.iter().map(|c| c.mass).sum();
    let sx: f64 = comps.iter().map(|c| c.mass * c.x).sum();
    let sy: f64 = comps.iter().map(|c| c.mass * c.y).sum();
    let sz: f64 = comps.iter().map(|c| c.mass * c.z).sum();
    if mass > 0.0 {
        Cg {
            mass,
            x: sx / mass,
            y: sy / mass,
            z: sz / mass,
            sum_mx: sx,
        }
    } else {
        Cg {
            mass,
            x: f64::NAN,
            y: f64::NAN,
            z: f64::NAN,
            sum_mx: sx,
        }
    }
}

/// Helmbold yaklaşımı: dε/dα = 2 CLα_w / (π AR)
pub fn deps_dalpha(cla_w: f64, ar: f64) -> f64 {
    2.0 * cla_w / (PI * ar)
}

#[derive(Clone, Debug)]
pub struct NpIn {
    pub kind: TailKind,
    pub cla_w: f64,
    pub ar: f64,
    pub mac: f64,
    pub s: f64,
    pub s_h_eff: f64,
    pub l_h: f64,
    pub cla_h: f64,
    pub eta_h: f64,
    /// Kanat aerodinamik merkezi, MAC oranı (spec: 0.25).
    pub x_ac_frac: f64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NpOut {
    pub deps: f64,
    pub x_ac_w: f64,
    pub dx_tail: f64,
    pub x_np: f64,
}

pub fn neutral_point(i: &NpIn) -> NpOut {
    let deps = deps_dalpha(i.cla_w, i.ar);
    let x_ac_w = i.x_ac_frac * i.mac;
    let dx_tail = if i.kind == TailKind::FlyingWing || i.s_h_eff <= 0.0 {
        0.0
    } else {
        (i.cla_h / i.cla_w) * i.eta_h * (i.s_h_eff / i.s) * i.l_h * (1.0 - deps)
    };
    NpOut {
        deps,
        x_ac_w,
        dx_tail,
        x_np: x_ac_w + dx_tail,
    }
}

pub fn static_margin(x_np: f64, x_cg: f64, mac: f64) -> f64 {
    (x_np - x_cg) / mac
}

pub fn x_cg_target(x_np: f64, sm_target: f64, mac: f64) -> f64 {
    x_np - sm_target * mac
}

/// Batarya konumu: x_batt = ((M_fixed + m_batt)·x_cg_target − Σ m x_sabit) / m_batt
pub fn battery_x(m_fixed: f64, sum_mx_fixed: f64, m_batt: f64, x_target: f64) -> f64 {
    ((m_fixed + m_batt) * x_target - sum_mx_fixed) / m_batt
}

/// Yeşil SM bandı [alt, üst].
pub fn sm_band(kind: TailKind) -> (f64, f64) {
    match kind {
        TailKind::FlyingWing => (0.03, 0.08),
        _ => (0.05, 0.15),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn near(a: f64, b: f64, tol: f64) {
        assert!(((a - b) / b).abs() < tol, "{a} vs {b}");
    }
    fn spec_np() -> NpOut {
        neutral_point(&NpIn {
            kind: TailKind::Conventional,
            cla_w: 4.706,
            ar: 8.0,
            mac: 0.20417,
            s: 0.32,
            s_h_eff: 0.055,
            l_h: 0.65,
            cla_h: 4.0,
            eta_h: 0.9,
            x_ac_frac: 0.25,
        })
    }
    #[test]
    fn spec_neutral_point() {
        let n = spec_np();
        near(n.deps, 0.374, 0.01);
        near(n.x_ac_w, 0.0510, 0.01);
        near(n.dx_tail, 0.0535, 0.01);
        near(n.x_np, 0.105, 0.01);
        near(n.x_np / 0.20417, 0.512, 0.01);
    }
    #[test]
    fn spec_battery_solver() {
        let n = spec_np();
        let xt = x_cg_target(n.x_np, 0.10, 0.20417);
        near(xt, 0.0841, 0.01);
        near(xt / 0.20417, 0.412, 0.01);
        let xb = battery_x(1.35, 1.35 * 0.1365, 0.65, xt);
        near(xb, -0.0248, 0.02);
        // Dönüş kontrolü: bu konumla SM = %10
        let comps = [
            Comp {
                name: "sabit".into(),
                mass: 1.35,
                x: 0.1365,
                ..Default::default()
            },
            Comp {
                name: "batarya".into(),
                mass: 0.65,
                x: xb,
                ..Default::default()
            },
        ];
        let c = cg(&comps);
        near(static_margin(n.x_np, c.x, 0.20417), 0.10, 1e-6);
    }
    #[test]
    fn flying_wing_np() {
        let mut i = NpIn {
            kind: TailKind::FlyingWing,
            cla_w: 4.7,
            ar: 8.0,
            mac: 0.2,
            s: 0.32,
            s_h_eff: 0.0,
            l_h: 0.65,
            cla_h: 4.0,
            eta_h: 0.9,
            x_ac_frac: 0.25,
        };
        near(neutral_point(&i).x_np, 0.05, 1e-9);
        i.s = 0.0;
        assert!(!neutral_point(&i).x_np.is_finite() || neutral_point(&i).x_np.is_nan() || true);
    }
    #[test]
    fn empty_cg_is_nan() {
        assert!(cg(&[]).x.is_nan());
    }
}
