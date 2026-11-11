// SPDX-License-Identifier: AGPL-3.0-or-later
//! Elektrikli kısıt analizi (P/W - W/S diyagramı). Spec Bölüm 5.
use super::aero::{cd_total, ld_at};
use super::atmosphere::G;
use serde::{Deserialize, Serialize};

/// "Önerilen nokta" seçim kriteri.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecCriterion {
    /// En düşük gereken P/W (dayanım odaklı, daha büyük kanat). Varsayılan.
    #[default]
    MinPower,
    /// İzin verilen en yüksek W/S (daha küçük kanat, daha fazla güç).
    MaxWingLoad,
}

impl RecCriterion {
    pub const ALL: [RecCriterion; 2] = [RecCriterion::MinPower, RecCriterion::MaxWingLoad];
    pub fn label(&self) -> &'static str {
        match self {
            RecCriterion::MinPower => crate::i18n::t("rec_crit.min_power"),
            RecCriterion::MaxWingLoad => crate::i18n::t("rec_crit.max_wing_load"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ConstraintIn {
    /// Önerilen noktanın seçim kriteri (`rec_ws`/`rec_pw` alanlarını belirler).
    pub rec_criterion: RecCriterion,
    pub v_stall_req: f64,
    pub v_cr: f64,
    pub rc: f64,
    pub v_y: f64,
    pub phi_turn_deg: f64,
    pub v_turn: f64,
    pub v_launch: f64,
    pub gamma_launch_deg: f64,
    pub a_launch: f64,
    pub eta_p_cr: f64,
    pub eta_p_climb: f64,
    pub eta_mot: f64,
    pub eta_esc: f64,
    /// Tasarım güç payı (spec: 1.15).
    pub margin: f64,
    /// W/S üst sınır çarpanı: 1.0 = yalnızca V_stall <= istenen; 0.9 = %10 pay (spec Madde 13 seçenek b).
    pub ws_margin: f64,
    pub cl_max: f64,
    pub cd0: f64,
    pub k: f64,
    pub rho_cr: f64,
    pub rho_msl: f64,
    /// Kurulu motor elektrik gücü [W] (mevcut tasarım noktası için).
    pub p_installed_elec: f64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CPoint {
    pub q: f64,
    pub cl: f64,
    pub cd: f64,
    pub ld: f64,
    pub pw: f64,
}

pub fn ws_max_stall(rho: f64, v_stall_req: f64, cl_max: f64) -> f64 {
    0.5 * rho * v_stall_req * v_stall_req * cl_max
}

pub fn cruise(ws: f64, i: &ConstraintIn) -> CPoint {
    let q = 0.5 * i.rho_cr * i.v_cr * i.v_cr;
    let cl = ws / q;
    let cd = cd_total(i.cd0, i.k, cl);
    CPoint {
        q,
        cl,
        cd,
        ld: cl / cd,
        pw: (i.v_cr / i.eta_p_cr) * (i.cd0 / cl + i.k * cl),
    }
}

pub fn climb(ws: f64, i: &ConstraintIn) -> CPoint {
    let q = 0.5 * i.rho_msl * i.v_y * i.v_y;
    let cl = ws / q;
    let cd = cd_total(i.cd0, i.k, cl);
    CPoint {
        q,
        cl,
        cd,
        ld: cl / cd,
        pw: (1.0 / i.eta_p_climb) * (i.rc + i.v_y * cd / cl),
    }
}

pub fn turn(ws: f64, i: &ConstraintIn) -> CPoint {
    let n = 1.0 / i.phi_turn_deg.to_radians().cos();
    let q = 0.5 * i.rho_cr * i.v_turn * i.v_turn;
    let cl = n * ws / q;
    let cd = cd_total(i.cd0, i.k, cl);
    CPoint {
        q,
        cl,
        cd,
        ld: cl / cd,
        pw: (i.v_turn / i.eta_p_cr) * n * (i.cd0 / cl + i.k * cl),
    }
}

/// El ile fırlatma. Dönen ikinci değer T/W.
pub fn launch(ws: f64, i: &ConstraintIn) -> (CPoint, f64) {
    let q = 0.5 * i.rho_msl * i.v_launch * i.v_launch;
    let cl = ws / q;
    let cd = cd_total(i.cd0, i.k, cl);
    let ld = ld_at(i.cd0, i.k, cl);
    let tw = i.gamma_launch_deg.to_radians().sin() + 1.0 / ld + i.a_launch / G;
    (
        CPoint {
            q,
            cl,
            cd,
            ld,
            pw: tw * i.v_launch / i.eta_p_climb,
        },
        tw,
    )
}

pub const NAMES_KEYS: [&str; 4] = [
    "constraint.cruise",
    "constraint.climb",
    "constraint.turn",
    "constraint.launch",
];

pub fn name(idx: usize) -> &'static str {
    crate::i18n::t(NAMES_KEYS[idx])
}

pub const NAMES: [&str; 4] = [
    "constraint.cruise",
    "constraint.climb",
    "constraint.turn",
    "constraint.launch",
];

/// [seyir, tırmanış, dönüş, fırlatma] P/W değerleri.
pub fn pw_all(ws: f64, i: &ConstraintIn) -> [f64; 4] {
    [
        cruise(ws, i).pw,
        climb(ws, i).pw,
        turn(ws, i).pw,
        launch(ws, i).0.pw,
    ]
}

/// En büyük gereksinim ve hangi kısıt olduğu.
pub fn envelope(ws: f64, i: &ConstraintIn) -> (f64, usize) {
    let a = pw_all(ws, i);
    let mut best = (f64::NEG_INFINITY, 0);
    for (idx, v) in a.iter().enumerate() {
        if v.is_nan() {
            return (f64::NAN, 0);
        }
        if *v > best.0 {
            best = (*v, idx);
        }
    }
    best
}

#[derive(Clone, Debug, Default)]
pub struct ConstraintOut {
    pub ws: f64,
    pub ws_max: f64,
    pub ws_allowed: f64,
    pub cruise: CPoint,
    pub climb: CPoint,
    pub turn: CPoint,
    pub launch: CPoint,
    pub launch_tw: f64,
    pub n_turn: f64,
    pub pw_req: f64,
    pub critical: usize,
    pub pw_design: f64,
    pub p_shaft: f64,
    pub p_elec: f64,
    pub v_stall: f64,
    pub stall_ok: bool,
    pub installed_pw: f64,
    /// Kurulu güç yeterli mi (gereksinim, 1.0x)?
    pub power_ok: bool,
    /// Seçilen kritere göre önerilen nokta (rec_criterion).
    pub rec_ws: f64,
    pub rec_pw: f64,
    /// En düşük P/W noktası (izin verilen W/S aralığında).
    pub rec_min_ws: f64,
    pub rec_min_pw: f64,
    /// İzin verilen en yüksek W/S noktası ve o noktadaki P/W (paysız).
    pub rec_max_ws: f64,
    pub rec_max_pw: f64,
    pub s_min_margin: f64,
}

/// İzin verilen W/S aralığında (5 .. ws_allowed) zarfın en küçük P/W noktası.
fn min_power_point(ws_allowed: f64, i: &ConstraintIn) -> (f64, f64) {
    let (mut best_ws, mut best_pw) = (f64::NAN, f64::INFINITY);
    let n = 400;
    if ws_allowed.is_finite() && ws_allowed > 5.0 {
        for j in 0..=n {
            let x = 5.0 + (ws_allowed - 5.0) * j as f64 / n as f64;
            let (y, _) = envelope(x, i);
            if y.is_finite() && y < best_pw {
                best_pw = y;
                best_ws = x;
            }
        }
    }
    if best_pw.is_finite() {
        (best_ws, best_pw)
    } else {
        (f64::NAN, f64::NAN)
    }
}

pub fn constraint(ws: f64, w: f64, i: &ConstraintIn) -> ConstraintOut {
    let ws_max = ws_max_stall(i.rho_cr, i.v_stall_req, i.cl_max);
    let ws_allowed = ws_max * i.ws_margin;
    let c = cruise(ws, i);
    let cl = climb(ws, i);
    let t = turn(ws, i);
    let (l, tw) = launch(ws, i);
    let (pw_req, critical) = envelope(ws, i);
    let pw_design = i.margin * pw_req;
    let p_shaft = pw_design * w;
    let p_elec = p_shaft / (i.eta_mot * i.eta_esc);
    let v_stall = (2.0 * ws / (i.rho_cr * i.cl_max)).sqrt();
    let installed_pw = i.p_installed_elec * i.eta_mot * i.eta_esc / w;

    // Önerilen noktalar: (a) izin verilen W/S'de en küçük P/W, (b) izin verilen en yüksek W/S.
    let (rec_min_ws, rec_min_pw) = min_power_point(ws_allowed, i);
    let (rec_max_ws, rec_max_pw) = if ws_allowed.is_finite() && ws_allowed > 5.0 {
        let (y, _) = envelope(ws_allowed, i);
        (ws_allowed, y)
    } else {
        (f64::NAN, f64::NAN)
    };
    let (rec_ws, rec_pw) = match i.rec_criterion {
        RecCriterion::MinPower => (rec_min_ws, rec_min_pw),
        RecCriterion::MaxWingLoad => (rec_max_ws, rec_max_pw),
    };
    ConstraintOut {
        ws,
        ws_max,
        ws_allowed,
        cruise: c,
        climb: cl,
        turn: t,
        launch: l,
        launch_tw: tw,
        n_turn: 1.0 / i.phi_turn_deg.to_radians().cos(),
        pw_req,
        critical,
        pw_design,
        p_shaft,
        p_elec,
        v_stall,
        stall_ok: v_stall <= i.v_stall_req && ws <= ws_allowed,
        installed_pw,
        power_ok: installed_pw >= pw_req,
        rec_ws,
        rec_pw,
        rec_min_ws,
        rec_min_pw,
        rec_max_ws,
        rec_max_pw,
        // Spec Bölüm 13 alternatifi: sabit %10 pay (kullanıcı W/S payından bağımsız).
        s_min_margin: w / (0.9 * ws_max),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn near(a: f64, b: f64, tol: f64) {
        assert!(((a - b) / b).abs() < tol, "{a} vs {b}");
    }
    fn spec_in() -> ConstraintIn {
        ConstraintIn {
            rec_criterion: RecCriterion::MinPower,
            v_stall_req: 10.0,
            v_cr: 18.0,
            rc: 3.5,
            v_y: 12.5,
            phi_turn_deg: 30.0,
            v_turn: 16.0,
            v_launch: 11.0,
            gamma_launch_deg: 10.0,
            a_launch: 1.0,
            eta_p_cr: 0.74,
            eta_p_climb: 0.58,
            eta_mot: 0.85,
            eta_esc: 0.95,
            margin: 1.15,
            ws_margin: 1.0,
            cl_max: 1.125,
            cd0: 0.02678,
            k: 0.04878,
            rho_cr: 1.16727,
            rho_msl: 1.2250,
            p_installed_elec: 240.0,
        }
    }
    #[test]
    fn spec_constraints() {
        let i = spec_in();
        let w = 2.0 * 9.80665;
        let o = constraint(61.3, w, &i);
        near(o.ws_max, 65.7, 0.01);
        near(ws_max_stall(1.225, 10.0, 1.125), 68.9, 0.01);
        assert!(o.stall_ok);
        near(o.v_stall, 9.66, 0.01);
        near(o.cruise.q, 189.0, 0.01);
        near(o.cruise.cl, 0.324, 0.01);
        near(o.cruise.cd, 0.0319, 0.01);
        near(o.cruise.ld, 10.2, 0.01);
        near(o.cruise.pw, 2.39, 0.01);
        near(o.climb.q, 95.7, 0.01);
        near(o.climb.cl, 0.641, 0.01);
        near(o.climb.cd, 0.0468, 0.01);
        near(o.climb.pw, 7.61, 0.01);
        near(o.n_turn, 1.155, 0.01);
        near(o.turn.cl, 0.474, 0.01);
        near(o.turn.cd, 0.0377, 0.01);
        near(o.turn.pw, 1.99, 0.01);
        near(o.launch.ld, 13.8, 0.01);
        near(o.launch_tw, 0.348, 0.01);
        near(o.launch.pw, 6.61, 0.01);
        assert_eq!(o.critical, 1);
        near(o.pw_design, 8.75, 0.01);
        near(o.p_shaft, 172.0, 0.01);
        near(o.p_elec, 213.0, 0.01);
        assert!(o.power_ok);
        near(o.s_min_margin, 0.332, 0.01);
    }
    #[test]
    fn recommended_point_is_within_stall_limit() {
        let i = spec_in();
        let o = constraint(61.3, 19.6, &i);
        assert!(o.rec_ws <= o.ws_allowed + 1e-9);
        assert!(o.rec_pw.is_finite());
    }
    #[test]
    fn rec_criterion_selects_point_and_both_are_reported() {
        let mut i = spec_in();
        let o = constraint(61.3, 19.6, &i);
        // Her iki nokta da hesaplanır.
        assert!(
            o.rec_min_pw <= o.rec_max_pw + 1e-12,
            "en düşük güç noktası en düşük P/W olmalı"
        );
        near(o.rec_max_ws, o.ws_allowed, 1e-12);
        assert!(o.rec_min_ws <= o.rec_max_ws + 1e-9);
        // Varsayılan kriter: en düşük güç.
        assert_eq!(o.rec_ws, o.rec_min_ws);
        assert_eq!(o.rec_pw, o.rec_min_pw);
        // Kriter değişince seçilen nokta değişir.
        i.rec_criterion = RecCriterion::MaxWingLoad;
        let o2 = constraint(61.3, 19.6, &i);
        assert_eq!(o2.rec_ws, o2.rec_max_ws);
        assert_eq!(o2.rec_pw, o2.rec_max_pw);
        assert!(o2.rec_pw >= o2.rec_min_pw - 1e-12);
    }
    #[test]
    fn nan_guard() {
        let mut i = spec_in();
        i.v_cr = f64::NAN;
        let o = constraint(61.3, 19.6, &i);
        assert!(o.pw_req.is_nan());
    }
}
