// SPDX-License-Identifier: AGPL-3.0-or-later
//! 2D profil analizi: panel (potansiyel akış) + integral sınır tabaka (viskoz düzeltme olmadan,
//! zayıf etkileşimsiz) ile α taraması, polar, Cp ve ayrılma uyarıları.
//!
//! Yöntem: `panel` modülü (Hess–Smith, Katz & Plotkin) kenar hızını verir; `boundary_layer`
//! modülü (Thwaites + Michel + Head + Squire–Young) her yüzey için sürüklemeyi hesaplar.
//! CL ve CM viskoz olmayan çözümden gelir (sınır tabaka geri beslemesi yok), bu yüzden CL, gerçek
//! profile göre ~%5–10 yüksek, CL_max ise ancak bir ön tasarım tahminidir.

use super::airfoil::Airfoil;
use super::boundary_layer::{BlOptions, BlResult, march};
use super::panel::{PanelModel, PanelSolution, SurfaceSpeeds};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};

/// Üst yüzey türbülanslı ayrılması bu x/c değerinin önüne geçince "tutunma kaybı" sayılır.
pub const STALL_SEP_X: f64 = 0.7;
/// Viskoz olmayan CL'yi tutunma civarında gerçek profile yaklaştıran kaba ampirik çarpan
/// (NACA 0012/2412, Re ≈ 1e6 için Abbott & von Doenhoff CL_max ≈ 1.5–1.7 ile kalibre edilmiştir).
/// CL_max yalnızca ön tasarım TAHMİNİDİR.
pub const STALL_CL_FACTOR: f64 = 0.8;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PolarOptions {
    pub reynolds: f64,
    pub alpha_min: f64,
    pub alpha_max: f64,
    pub alpha_step: f64,
    pub n_panels: usize,
    pub bl: BlOptions,
}

impl Default for PolarOptions {
    fn default() -> Self {
        PolarOptions {
            reynolds: 5e5,
            alpha_min: -6.0,
            alpha_max: 16.0,
            alpha_step: 1.0,
            n_panels: 160,
            bl: BlOptions::default(),
        }
    }
}

/// Tek α için viskoz sonuç özeti.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PolarPoint {
    pub alpha_deg: f64,
    pub cl: f64,
    /// Profil sürükleme katsayısı (Squire–Young, iki yüzey toplamı). `bl_ok = false` ise 0.
    pub cd: f64,
    pub cm_c4: f64,
    pub bl_ok: bool,
    /// Bu α'da panel çözümü başarısız oldu (sonlu olmayan sonuç); nokta özetlere katılmaz.
    pub failed: bool,
    /// Geçiş konumları (x/c); bulunamazsa 1.0.
    pub x_tr_upper: f64,
    pub x_tr_lower: f64,
    /// Türbülanslı ayrılma başlangıcı (x/c), yoksa `None`.
    pub x_sep_upper: Option<f64>,
    pub x_sep_lower: Option<f64>,
    /// Laminer ayrılma (kısa kabarcık varsayımı) görüldü mü.
    pub lam_sep_upper: bool,
    pub lam_sep_lower: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PolarResult {
    pub name: String,
    pub reynolds: f64,
    pub points: Vec<PolarPoint>,
    /// Sıfır taşıma açısı (derece), CL–α doğrusal uyduruluşundan.
    pub alpha_l0_deg: f64,
    /// CL–α eğimi [1/rad] (viskoz olmayan, sıkıştırılamaz panel).
    pub cl_alpha_per_rad: f64,
    /// Tarama boyunca en küçük CD (CL≈0 civarında olması gerekmez).
    pub cd_min: f64,
    /// Yaklaşık tutunma kaybı: (α, tahmini CL_max). Taramada ayrılma x/c < 0.7'ye gelmediyse `None`.
    pub stall: Option<(f64, f64)>,
    /// Taramanın en büyük CL değeri (tahmin değil, tarama sınırı).
    pub cl_max_sweep: f64,
}

fn bl_for(
    model: &PanelModel,
    sol: &PanelSolution,
    re: f64,
    opts: &BlOptions,
) -> Option<(BlResult, BlResult, SurfaceSpeeds)> {
    let (up, lo) = model.surface_speeds(sol)?;
    let run = |d: &[[f64; 3]]| {
        let s: Vec<f64> = d.iter().map(|v| v[0]).collect();
        let u: Vec<f64> = d.iter().map(|v| v[1]).collect();
        march(&s, &u, re, opts)
    };
    let (bu, bl) = (run(&up)?, run(&lo)?);
    Some((bu, bl, (up, lo)))
}

/// Tek α'da viskoz analiz.
pub fn analyze(
    model: &PanelModel,
    alpha_deg: f64,
    re: f64,
    bl: &BlOptions,
) -> Result<PolarPoint, String> {
    let sol = model.solve(alpha_deg)?;
    let mut pt = PolarPoint {
        alpha_deg,
        cl: sol.cl,
        cm_c4: sol.cm_c4,
        x_tr_upper: 1.0,
        x_tr_lower: 1.0,
        ..Default::default()
    };
    if let Some((bu, bl_l, (up, lo))) = bl_for(model, &sol, re, bl) {
        let cd = bu.cd + bl_l.cd;
        if cd.is_finite() {
            pt.cd = cd;
            pt.bl_ok = true;
        }
        let xc = |d: &[[f64; 3]], i: usize| d.get(i).map(|v| v[2]).unwrap_or(1.0);
        if let Some(i) = bu.transition_index {
            pt.x_tr_upper = xc(&up, i);
        }
        if let Some(i) = bl_l.transition_index {
            pt.x_tr_lower = xc(&lo, i);
        }
        pt.x_sep_upper = bu.separation_index.map(|i| xc(&up, i));
        pt.x_sep_lower = bl_l.separation_index.map(|i| xc(&lo, i));
        pt.lam_sep_upper = bu.laminar_separation;
        pt.lam_sep_lower = bl_l.laminar_separation;
    }
    Ok(pt)
}

/// α taraması. `cancel` true olursa o ana kadarki sonuçla erken döner (Err değil).
pub fn polar(af: &Airfoil, o: &PolarOptions, cancel: &AtomicBool) -> Result<PolarResult, String> {
    if !(o.reynolds.is_finite() && o.reynolds >= 1e4) {
        return Err("Reynolds sayısı en az 1e4 olmalı".into());
    }
    if !(o.alpha_step.is_finite() && o.alpha_step > 0.0 && o.alpha_max >= o.alpha_min) {
        return Err("Geçersiz α aralığı".into());
    }
    let n_a = ((o.alpha_max - o.alpha_min) / o.alpha_step).floor() as usize + 1;
    if n_a > 200 {
        return Err("En fazla 200 açı noktası".into());
    }
    let model = PanelModel::new(&af.repaneled(o.n_panels))?;
    let mut res = PolarResult {
        name: af.name.clone(),
        reynolds: o.reynolds,
        ..Default::default()
    };
    for k in 0..n_a {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let a = o.alpha_min + k as f64 * o.alpha_step;
        // Tek bir α'daki hata taramayı düşürmez: nokta `failed` işaretlenir, devam edilir.
        let pt = analyze(&model, a, o.reynolds, &o.bl).unwrap_or(PolarPoint {
            alpha_deg: a,
            x_tr_upper: 1.0,
            x_tr_lower: 1.0,
            failed: true,
            ..Default::default()
        });
        res.points.push(pt);
    }
    if res.points.is_empty() {
        return Err("İptal edildi".into());
    }
    if res.points.iter().all(|p| p.failed) {
        return Err("Hiçbir açıda panel çözümü sonlu değil".into());
    }
    // Doğrusal uyduruluş (en küçük kareler) CL = a (α − α_L0).
    let good: Vec<PolarPoint> = res.points.iter().filter(|p| !p.failed).cloned().collect();
    let pts = &good;
    if pts.len() >= 2 {
        let n = pts.len() as f64;
        let (sx, sy): (f64, f64) = pts
            .iter()
            .fold((0.0, 0.0), |s, p| (s.0 + p.alpha_deg, s.1 + p.cl));
        let (mx, my) = (sx / n, sy / n);
        let sxx: f64 = pts.iter().map(|p| (p.alpha_deg - mx).powi(2)).sum();
        let sxy: f64 = pts.iter().map(|p| (p.alpha_deg - mx) * (p.cl - my)).sum();
        if sxx > 0.0 {
            let slope = sxy / sxx; // 1/derece
            res.cl_alpha_per_rad = slope * 180.0 / std::f64::consts::PI;
            res.alpha_l0_deg = if slope.abs() > 1e-9 {
                mx - my / slope
            } else {
                0.0
            };
        }
    }
    res.cl_max_sweep = res
        .points
        .iter()
        .filter(|p| !p.failed)
        .map(|p| p.cl)
        .fold(f64::NEG_INFINITY, f64::max);
    res.cd_min = res
        .points
        .iter()
        .filter(|p| p.bl_ok && !p.failed)
        .map(|p| p.cd)
        .fold(f64::INFINITY, f64::min);
    if !res.cd_min.is_finite() {
        res.cd_min = 0.0;
    }
    res.stall = res
        .points
        .iter()
        .find(|p| !p.failed && p.alpha_deg > 0.0 && p.x_sep_upper.is_some_and(|x| x < STALL_SEP_X))
        .map(|p| (p.alpha_deg, STALL_CL_FACTOR * p.cl));
    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(code: &str, re: f64) -> PolarResult {
        let af = Airfoil::naca4_code(code, 160).unwrap();
        let o = PolarOptions {
            reynolds: re,
            alpha_min: -4.0,
            alpha_max: 18.0,
            alpha_step: 2.0,
            ..Default::default()
        };
        polar(&af, &o, &AtomicBool::new(false)).unwrap()
    }

    #[test]
    fn naca0012_cd0_reasonable_at_re_1e6() {
        // XFOIL (Re=1e6, serbest geçiş) CD0 ≈ 0.0060; deneysel ≈ 0.006–0.007 (Abbott & von
        // Doenhoff). İntegral yöntem için 0.004–0.009 kabul aralığı.
        let r = run("0012", 1e6);
        let p0 = r.points.iter().find(|p| p.alpha_deg == 0.0).unwrap();
        assert!(p0.bl_ok);
        assert!(p0.cl.abs() < 5e-3);
        assert!((0.004..0.009).contains(&p0.cd), "cd0={}", p0.cd);
        let (a_s, cl_s) = r.stall.expect("0012 taramada tutunma kaybı bulunmalı");
        // Deneysel (Abbott & von Doenhoff, Re≈1e6): α_stall ≈ 14–16°, CL_max ≈ 1.4–1.6.
        assert!(
            (12.0..=18.0).contains(&a_s) && (1.2..1.9).contains(&cl_s),
            "{a_s} {cl_s}"
        );
    }

    #[test]
    fn naca2412_polar_properties() {
        let r = run("2412", 1e6);
        assert!((r.alpha_l0_deg + 2.1).abs() < 0.5, "al0={}", r.alpha_l0_deg);
        assert!(
            (6.0..7.4).contains(&r.cl_alpha_per_rad),
            "a={}",
            r.cl_alpha_per_rad
        );
        assert!((0.004..0.010).contains(&r.cd_min), "cdmin={}", r.cd_min);
        // CD, taşıma arttıkça (pozitif α) artar.
        let d = |a: f64| r.points.iter().find(|p| p.alpha_deg == a).unwrap().cd;
        assert!(d(10.0) > d(2.0));
        let cm = r.points.iter().find(|p| p.alpha_deg == 2.0).unwrap().cm_c4;
        assert!((-0.08..-0.03).contains(&cm), "cm={cm}"); // deneysel ≈ −0.045
    }

    #[test]
    fn sweep_keeps_good_points_and_marks_none_failed_normally() {
        let r = run("0012", 1e6);
        assert!(r.points.iter().all(|p| !p.failed));
        assert!(r.cd_min > 0.0);
    }

    #[test]
    fn invalid_options_and_cancel() {
        let af = Airfoil::naca4_code("0012", 100).unwrap();
        let bad = PolarOptions {
            reynolds: f64::NAN,
            ..Default::default()
        };
        assert!(polar(&af, &bad, &AtomicBool::new(false)).is_err());
        let o = PolarOptions::default();
        assert!(polar(&af, &o, &AtomicBool::new(true)).is_err()); // hemen iptal: boş
    }
}
