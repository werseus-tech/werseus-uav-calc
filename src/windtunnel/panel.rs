// SPDX-License-Identifier: AGPL-3.0-or-later
//! 2D potansiyel akış paneli: Hess–Smith yöntemi (panel başına sabit kaynak + tüm profilde ortak
//! sabit girdap şiddeti, firar kenarında Kutta koşulu).
//!
//! Kaynaklar: Katz & Plotkin, "Low-Speed Aerodynamics" (2. bas., 2001), Böl. 3 ve 11
//! (sabit kuvvetli kaynak/girdap panelleri, indüklenen hız formülleri); Hess & Smith (1967);
//! Moran, "An Introduction to Theoretical and Computational Aerodynamics" (1984), Böl. 3 (Kutta
//! koşulu: V_t,1 + V_t,N = 0).
//!
//! Sıkıştırılamaz, viskoz olmayan akış. Nokta sırası Selig düzenidir (TE → üst → LE → alt → TE,
//! saat yönünün tersi); dış normal n = (t_y, −t_x). Çözüm α'ya göre doğrusal olduğundan iki taban
//! çözüm (V∞ = x̂ ve ŷ) bir kez hesaplanır, her α için bileşimleri kullanılır.

use super::airfoil::Airfoil;
use super::linalg::Lu;
use std::f64::consts::PI;

/// Üst ve alt yüzey dizileri: her eleman [s, ue, x/c].
pub type SurfaceSpeeds = (Vec<[f64; 3]>, Vec<[f64; 3]>);

/// Panel modeli (geometri + iki taban çözüm).
#[derive(Clone, Debug)]
pub struct PanelModel {
    pub n: usize,
    /// Panel orta noktaları (kontrol noktaları).
    pub mid: Vec<[f64; 2]>,
    pub tan: Vec<[f64; 2]>,
    pub nrm: Vec<[f64; 2]>,
    pub len: Vec<f64>,
    pts: Vec<[f64; 2]>,
    /// Taban çözümler için teğetsel hız ve γ: V∞ = x̂ ve V∞ = ŷ.
    vt_x: Vec<f64>,
    vt_y: Vec<f64>,
    gam_x: f64,
    gam_y: f64,
}

/// Tek açı için çözüm.
#[derive(Clone, Debug)]
pub struct PanelSolution {
    pub alpha_deg: f64,
    /// Dolaşım ile kaldırma katsayısı (Kutta–Joukowski), kord = 1.
    pub cl: f64,
    /// Basınç integrasyonundan kaldırma (çapraz kontrol).
    pub cl_pressure: f64,
    /// Çeyrek-kord etrafında moment katsayısı (burun yukarı +; z-momentinin tersi).
    pub cm_c4: f64,
    /// Her panel için Cp (kontrol noktalarında).
    pub cp: Vec<f64>,
    /// Her panel için teğetsel hız (panel t yönünde, V∞'a göre).
    pub vt: Vec<f64>,
}

fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

/// Birim şiddetli sabit panelin P noktasında indüklediği hız: (kaynak, girdap), global bileşenler.
/// Yerel çerçeve: ξ = t, ζ = sol normal (−t_y, t_x). Katz & Plotkin (10.2, 10.3).
fn panel_vel(p0: [f64; 2], t: [f64; 2], len: f64, p: [f64; 2]) -> ([f64; 2], [f64; 2]) {
    let ln = [-t[1], t[0]];
    let d = [p[0] - p0[0], p[1] - p0[1]];
    let x = dot(d, t);
    let z = dot(d, ln);
    let r1s = (x * x + z * z).max(1e-30);
    let r2s = ((x - len).powi(2) + z * z).max(1e-30);
    let th1 = z.atan2(x);
    let th2 = z.atan2(x - len);
    let lg = (r1s / r2s).ln();
    let us = lg / (4.0 * PI);
    let ws = (th2 - th1) / (2.0 * PI);
    let uv = (th1 - th2) / (2.0 * PI);
    let wv = lg / (4.0 * PI);
    (
        [us * t[0] + ws * ln[0], us * t[1] + ws * ln[1]],
        [uv * t[0] + wv * ln[0], uv * t[1] + wv * ln[1]],
    )
}

impl PanelModel {
    /// Profilden panel modeli kurar. Geçersiz/tekil geometride `Err`.
    pub fn new(af: &Airfoil) -> Result<PanelModel, String> {
        let pts = af.points.clone();
        let n = pts.len().saturating_sub(1);
        if n < 20 {
            return Err("Panel yöntemi için en az 20 panel gerekli".into());
        }
        let mut mid = Vec::with_capacity(n);
        let mut tan = Vec::with_capacity(n);
        let mut nrm = Vec::with_capacity(n);
        let mut len = Vec::with_capacity(n);
        for i in 0..n {
            let (a, b) = (pts[i], pts[i + 1]);
            let l = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
            if !(l > 1e-9) {
                return Err("Sıfır uzunluklu panel (yinelenen nokta)".into());
            }
            let t = [(b[0] - a[0]) / l, (b[1] - a[1]) / l];
            mid.push([0.5 * (a[0] + b[0]), 0.5 * (a[1] + b[1])]);
            tan.push(t);
            nrm.push([t[1], -t[0]]);
            len.push(l);
        }
        // (N+1)×(N+1): N normal-hız denklemi + Kutta.
        let m = n + 1;
        let mut a = vec![0.0; m * m];
        let mut ct = vec![0.0; n * n]; // kaynakların teğetsel etkisi
        let mut dt = vec![0.0; n]; // girdabın teğetsel etkisi
        for i in 0..n {
            for j in 0..n {
                if i == j {
                    // Kendi etkisi: kaynak dış normalde +σ/2, girdap teğette +γ/2 (dış yüzey).
                    a[i * m + j] = 0.5;
                    dt[i] += 0.5;
                } else {
                    let (vs, vv) = panel_vel(pts[j], tan[j], len[j], mid[i]);
                    a[i * m + j] = dot(vs, nrm[i]);
                    ct[i * n + j] = dot(vs, tan[i]);
                    a[i * m + n] += dot(vv, nrm[i]);
                    dt[i] += dot(vv, tan[i]);
                }
            }
        }
        // Kutta: Vt[0] + Vt[N-1] = 0 (V∞ terimi sağ tarafta).
        for j in 0..n {
            a[n * m + j] = ct[j] + ct[(n - 1) * n + j];
        }
        a[n * m + n] = dt[0] + dt[n - 1];
        let lu = Lu::factor(a, m).ok_or("Panel sistemi tekil (geometriyi kontrol edin)")?;
        let solve_dir = |v: [f64; 2]| -> Result<(Vec<f64>, f64), String> {
            let mut rhs = vec![0.0; m];
            for i in 0..n {
                rhs[i] = -dot(v, nrm[i]);
            }
            rhs[n] = -(dot(v, tan[0]) + dot(v, tan[n - 1]));
            let x = lu
                .solve(&rhs)
                .ok_or("Panel sistemi çözülemedi (sayısal bozulma)")?;
            let gam = x[n];
            let vt: Vec<f64> = (0..n)
                .map(|i| {
                    dot(v, tan[i]) + (0..n).map(|j| ct[i * n + j] * x[j]).sum::<f64>() + dt[i] * gam
                })
                .collect();
            Ok((vt, gam))
        };
        let (vt_x, gam_x) = solve_dir([1.0, 0.0])?;
        let (vt_y, gam_y) = solve_dir([0.0, 1.0])?;
        Ok(PanelModel {
            n,
            mid,
            tan,
            nrm,
            len,
            pts,
            vt_x,
            vt_y,
            gam_x,
            gam_y,
        })
    }

    /// Verilen hücum açısında (derece) çözüm. `Err`: sonlu olmayan sonuç.
    pub fn solve(&self, alpha_deg: f64) -> Result<PanelSolution, String> {
        if !alpha_deg.is_finite() {
            return Err("α sonlu olmalı".into());
        }
        let al = alpha_deg.to_radians();
        let (c, s) = (al.cos(), al.sin());
        let vt: Vec<f64> = (0..self.n)
            .map(|i| c * self.vt_x[i] + s * self.vt_y[i])
            .collect();
        let gam = c * self.gam_x + s * self.gam_y;
        let cp: Vec<f64> = vt.iter().map(|v| 1.0 - v * v).collect();
        let perim: f64 = self.len.iter().sum();
        // γ > 0: saat yönünün tersi dolaşım; kaldırma için saat yönü gerekir → Γ = −γ·L.
        let cl = -2.0 * gam * perim;
        let (mut fx, mut fy, mut mz) = (0.0, 0.0, 0.0);
        for i in 0..self.n {
            let f = [
                -cp[i] * self.len[i] * self.nrm[i][0],
                -cp[i] * self.len[i] * self.nrm[i][1],
            ];
            fx += f[0];
            fy += f[1];
            mz += (self.mid[i][0] - 0.25) * f[1] - self.mid[i][1] * f[0];
        }
        let cl_pressure = fy * c - fx * s;
        if !(cl.is_finite() && cl_pressure.is_finite() && mz.is_finite())
            || cp.iter().any(|v| !v.is_finite())
        {
            return Err("Panel çözümü sonlu değil".into());
        }
        Ok(PanelSolution {
            alpha_deg,
            cl,
            cl_pressure,
            cm_c4: -mz,
            cp,
            vt,
        })
    }

    /// Durma noktasına göre üst ve alt yüzey dizileri: her eleman [s, ue, x/c]; s durma noktasından
    /// yüzey mesafesi, ilk eleman (s = 0, ue = 0). Durma noktası bulunamazsa `None`.
    pub fn surface_speeds(&self, sol: &PanelSolution) -> Option<SurfaceSpeeds> {
        let n = self.n;
        let le = (0..n).min_by(|a, b| self.mid[*a][0].total_cmp(&self.mid[*b][0]))?;
        // Üstte vt < 0 (t öne bakar), altta vt > 0: işaret değişimi, LE'ye en yakın olan.
        let mut k: Option<(usize, usize)> = None;
        for i in 1..n {
            if sol.vt[i - 1] < 0.0 && sol.vt[i] >= 0.0 {
                let dist = i.abs_diff(le);
                if k.is_none_or(|(_, d)| dist < d) {
                    k = Some((i, dist));
                }
            }
        }
        let (i, _) = k?;
        let (v0, v1) = (sol.vt[i - 1], sol.vt[i]);
        let f = (-v0 / (v1 - v0)).clamp(0.0, 1.0);
        let dist = |a: usize, b: usize| {
            ((self.mid[a][0] - self.mid[b][0]).powi(2) + (self.mid[a][1] - self.mid[b][1]).powi(2))
                .sqrt()
        };
        let d = dist(i, i - 1);
        let xs = self.mid[i - 1][0] + f * (self.mid[i][0] - self.mid[i - 1][0]);
        let mut upper = vec![[0.0, 0.0, xs]];
        let mut s = f * d;
        upper.push([s, sol.vt[i - 1].abs(), self.mid[i - 1][0]]);
        for j in (0..i - 1).rev() {
            s += dist(j, j + 1);
            upper.push([s, sol.vt[j].abs(), self.mid[j][0]]);
        }
        let mut lower = vec![[0.0, 0.0, xs]];
        let mut s = (1.0 - f) * d;
        lower.push([s, sol.vt[i].abs(), self.mid[i][0]]);
        for j in (i + 1)..n {
            s += dist(j, j - 1);
            lower.push([s, sol.vt[j].abs(), self.mid[j][0]]);
        }
        Some((upper, lower))
    }

    /// Geometri noktaları (çizim için).
    pub fn points(&self) -> &[[f64; 2]] {
        &self.pts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(code: &str) -> PanelModel {
        PanelModel::new(&Airfoil::naca4_code(code, 160).unwrap()).unwrap()
    }

    #[test]
    fn symmetric_zero_lift_at_zero_alpha() {
        let sol = model("0012").solve(0.0).unwrap();
        // Simetri → CL = 0; sayısal gürültü < 5e-3 (panel yöntemi hata mertebesi).
        assert!(sol.cl.abs() < 5e-3, "cl={}", sol.cl);
        assert!(sol.cl_pressure.abs() < 5e-3);
    }

    #[test]
    fn lift_slope_near_two_pi() {
        // İnce profil teorisi 2π = 6.28/rad; gerçek kalınlıkta panel yöntemi 2π(1 + ~0.77 t/c)
        // ≈ 6.9/rad verir (Abbott & von Doenhoff). Aralık geniş tutuldu: 6.0–7.4.
        for code in ["0012", "2412"] {
            let m = model(code);
            let (a, b) = (m.solve(0.0).unwrap(), m.solve(4.0).unwrap());
            let slope = (b.cl - a.cl) / 4.0f64.to_radians();
            assert!((6.0..7.4).contains(&slope), "{code}: slope={slope}");
        }
    }

    #[test]
    fn zero_lift_angle_naca2412() {
        // Yayımlanmış α_L0 (NACA 2412) ≈ −2.1° (Abbott & von Doenhoff); ±0.4° tolerans.
        let m = model("2412");
        let (a, b) = (m.solve(0.0).unwrap(), m.solve(4.0).unwrap());
        let a0 = -a.cl / ((b.cl - a.cl) / 4.0);
        assert!((a0 + 2.1).abs() < 0.4, "a0={a0}");
    }

    #[test]
    fn circulation_and_pressure_lift_agree() {
        let s = model("2412").solve(5.0).unwrap();
        assert!(
            (s.cl - s.cl_pressure).abs() < 0.03,
            "{} vs {}",
            s.cl,
            s.cl_pressure
        );
    }

    #[test]
    fn camber_gives_negative_cm_and_stagnation_found() {
        let m = model("2412");
        let s = m.solve(3.0).unwrap();
        assert!(s.cm_c4 < 0.0 && s.cm_c4 > -0.12, "cm={}", s.cm_c4);
        let (u, l) = m.surface_speeds(&s).unwrap();
        assert!(u.len() > 20 && l.len() > 20);
        assert!(u.windows(2).all(|w| w[1][0] > w[0][0]));
    }

    #[test]
    fn rejects_bad_geometry() {
        let af = Airfoil {
            name: "x".into(),
            points: vec![[1.0, 0.0]; 5],
        };
        assert!(PanelModel::new(&af).is_err());
    }
}
