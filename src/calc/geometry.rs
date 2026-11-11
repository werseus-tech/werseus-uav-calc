// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kanat planformu (trapez) ve kuyruk boyutlandırma. SI birimleri.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub struct WingIn {
    pub b: f64,
    pub s: f64,
    pub taper: f64,
    pub sweep_le_deg: f64,
}

#[derive(Clone, Debug, Default)]
pub struct WingOut {
    pub ar: f64,
    pub c_r: f64,
    pub c_t: f64,
    pub mac: f64,
    pub y_mac: f64,
    pub x_le_mac: f64,
    /// Yarım-kord oku için tan(Λ_c/2) (Helmbold, spec Düzeltme 9).
    pub tan_sweep_half: f64,
}

pub fn wing(i: &WingIn) -> WingOut {
    let ar = i.b * i.b / i.s;
    let c_r = 2.0 * i.s / (i.b * (1.0 + i.taper));
    let c_t = i.taper * c_r;
    let mac = (2.0 / 3.0) * c_r * (1.0 + i.taper + i.taper * i.taper) / (1.0 + i.taper);
    let y_mac = (i.b / 6.0) * (1.0 + 2.0 * i.taper) / (1.0 + i.taper);
    let tan_le = i.sweep_le_deg.to_radians().tan();
    let x_le_mac = y_mac * tan_le;
    let tan_half = tan_le - (2.0 / ar) * (1.0 - i.taper) / (1.0 + i.taper);
    WingOut {
        ar,
        c_r,
        c_t,
        mac,
        y_mac,
        x_le_mac,
        tan_sweep_half: tan_half,
    }
}

/// Kord uzunluğu [m] (|y| uzaklığında).
pub fn chord_at(w: &WingOut, b: f64, y: f64) -> f64 {
    w.c_r * (1.0 - (1.0 - w.c_t / w.c_r) * (y.abs() / (b / 2.0)))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TailKind {
    Conventional,
    VTail,
    FlyingWing,
}

impl TailKind {
    pub fn label(&self) -> &'static str {
        match self {
            TailKind::Conventional => crate::i18n::t("tail.conventional"),
            TailKind::VTail => crate::i18n::t("tail.vtail"),
            TailKind::FlyingWing => crate::i18n::t("tail.flying_wing"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct TailIn {
    pub kind: TailKind,
    pub s_h: f64,
    pub s_v: f64,
    /// V-kuyruk toplam planform alanı (iki panel).
    pub s_vtail: f64,
    pub gamma_v_deg: f64,
    pub l_h: f64,
    pub l_v: f64,
    pub vh_target: f64,
    pub vv_target: f64,
    pub s: f64,
    pub b: f64,
    pub mac: f64,
}

#[derive(Clone, Debug, Default)]
pub struct TailOut {
    /// Etkin yatay kuyruk alanı (stabilite hesabına giren).
    pub s_h_eff: f64,
    pub s_v_eff: f64,
    /// Sürtünme için toplam kuyruk alanı S_t.
    pub s_t: f64,
    pub v_h: f64,
    pub v_v: f64,
    pub s_h_req: f64,
    pub s_v_req: f64,
    /// V-kuyruk için hedef hacimleri sağlayan en küçük toplam alan.
    pub s_vtail_req: f64,
}

pub fn tail(i: &TailIn) -> TailOut {
    let g = i.gamma_v_deg.to_radians();
    let (s_h_eff, s_v_eff, s_t) = match i.kind {
        TailKind::Conventional => (i.s_h, i.s_v, i.s_h + i.s_v),
        TailKind::VTail => (
            i.s_vtail * g.cos().powi(2),
            i.s_vtail * g.sin().powi(2),
            i.s_vtail,
        ),
        TailKind::FlyingWing => (0.0, 0.0, 0.0),
    };
    let s_h_req = i.vh_target * i.s * i.mac / i.l_h;
    let s_v_req = i.vv_target * i.s * i.b / i.l_v;
    let s_vtail_req = (s_h_req / g.cos().powi(2)).max(s_v_req / g.sin().powi(2));
    TailOut {
        s_h_eff,
        s_v_eff,
        s_t,
        v_h: s_h_eff * i.l_h / (i.s * i.mac),
        v_v: s_v_eff * i.l_v / (i.s * i.b),
        s_h_req,
        s_v_req,
        s_vtail_req,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn near(a: f64, b: f64, tol: f64) {
        assert!(((a - b) / b).abs() < tol, "{a} vs {b}");
    }
    #[test]
    fn spec_wing() {
        let w = wing(&WingIn {
            b: 1.6,
            s: 0.32,
            taper: 0.6,
            sweep_le_deg: 5.0,
        });
        near(w.ar, 8.0, 0.001);
        near(w.c_r, 0.250, 0.005);
        near(w.c_t, 0.150, 0.005);
        near(w.mac, 0.204, 0.005);
        near(w.y_mac, 0.367, 0.005);
        near(w.x_le_mac, 0.0321, 0.01);
        // Helmbold: Λ_c/2 ≈ 1.4°
        near(w.tan_sweep_half.atan().to_degrees(), 1.4, 0.05);
        near(chord_at(&w, 1.6, 0.8), 0.15, 0.001);
    }
    #[test]
    fn spec_tail() {
        let t = tail(&TailIn {
            kind: TailKind::Conventional,
            s_h: 0.055,
            s_v: 0.028,
            s_vtail: 0.083,
            gamma_v_deg: 35.0,
            l_h: 0.65,
            l_v: 0.65,
            vh_target: 0.55,
            vv_target: 0.035,
            s: 0.32,
            b: 1.6,
            mac: 0.20417,
        });
        near(t.v_h, 0.547, 0.01);
        near(t.v_v, 0.0356, 0.01);
        near(t.s_t, 0.083, 0.001);
    }
    #[test]
    fn vtail_conversion() {
        let t = tail(&TailIn {
            kind: TailKind::VTail,
            s_h: 0.0,
            s_v: 0.0,
            s_vtail: 0.1,
            gamma_v_deg: 35.0,
            l_h: 0.65,
            l_v: 0.65,
            vh_target: 0.5,
            vv_target: 0.04,
            s: 0.32,
            b: 1.6,
            mac: 0.2,
        });
        near(t.s_h_eff + t.s_v_eff, 0.1, 1e-9);
        near(t.s_h_eff, 0.1 * 35f64.to_radians().cos().powi(2), 1e-9);
    }
}
