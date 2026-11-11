// SPDX-License-Identifier: AGPL-3.0-or-later
//! Rüzgar tüneli çözücüleri için profil geometrisi: NACA 4-haneli üretici, Selig sıralı nokta
//! listesi kabulü (normalize etme, yön düzeltme, firar kenarı kapatma) ve yeniden panelleme.
//!
//! Tüm koordinatlar kord ile normalize edilir (x: 0 = hücum kenarı, 1 = firar kenarı).
//! Nokta sırası Selig düzenidir: firar kenarı → üst yüzey → hücum kenarı → alt yüzey → firar kenarı.

use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

/// Kord ile normalize edilmiş kapalı profil çevresi (Selig sırası, ilk nokta = son nokta = TE).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Airfoil {
    pub name: String,
    pub points: Vec<[f64; 2]>,
}

/// NACA 4-haneli kalınlık dağılımı (kapalı firar kenarı, a4 = −0.1036).
/// Kaynak: Abbott & von Doenhoff, "Theory of Wing Sections" (1959), Böl. 6.4.
fn naca4_thickness(t: f64, x: f64) -> f64 {
    5.0 * t
        * (0.2969 * x.max(0.0).sqrt() - 0.1260 * x - 0.3516 * x * x + 0.2843 * x.powi(3)
            - 0.1036 * x.powi(4))
}

/// NACA 4-haneli ortalama kamber çizgisi: (y_c, dy_c/dx). `m`: maks kamber, `p`: konumu (kord oranı).
pub fn naca4_camber(m: f64, p: f64, x: f64) -> (f64, f64) {
    if !(m.is_finite() && p.is_finite()) || m.abs() < 1e-12 || p <= 1e-6 || p >= 1.0 - 1e-6 {
        return (0.0, 0.0);
    }
    if x < p {
        (
            m / (p * p) * (2.0 * p * x - x * x),
            2.0 * m / (p * p) * (p - x),
        )
    } else {
        let q = (1.0 - p) * (1.0 - p);
        (
            m / q * ((1.0 - 2.0 * p) + 2.0 * p * x - x * x),
            2.0 * m / q * (p - x),
        )
    }
}

impl Airfoil {
    /// NACA 4-haneli profil. `m`: maks kamber (ör. 0.02), `p`: kamber konumu (ör. 0.4),
    /// `t`: kalınlık oranı (ör. 0.12). `n_panels` çift sayıya yuvarlanır (40..=400).
    pub fn naca4(m: f64, p: f64, t: f64, n_panels: usize) -> Result<Airfoil, String> {
        if !(m.is_finite() && p.is_finite() && t.is_finite()) {
            return Err("NACA parametreleri sonlu olmalı".into());
        }
        if !(0.0..=0.095).contains(&m) || !(0.0..=0.9).contains(&p) || !(0.01..=0.40).contains(&t) {
            return Err(format!(
                "NACA parametreleri aralık dışı: m={m}, p={p}, t={t}"
            ));
        }
        let half = (n_panels.clamp(40, 400) / 2).max(20);
        let mut upper = Vec::with_capacity(half + 1);
        let mut lower = Vec::with_capacity(half + 1);
        for i in 0..=half {
            let beta = PI * i as f64 / half as f64;
            let x = 0.5 * (1.0 - beta.cos());
            let yt = if i == half {
                0.0
            } else {
                naca4_thickness(t, x)
            };
            let (yc, dyc) = naca4_camber(m, p, x);
            let th = dyc.atan();
            upper.push([x - yt * th.sin(), yc + yt * th.cos()]);
            lower.push([x + yt * th.sin(), yc - yt * th.cos()]);
        }
        let mut pts: Vec<[f64; 2]> = upper.into_iter().rev().collect();
        pts.extend(lower.into_iter().skip(1));
        Ok(Airfoil {
            name: format!(
                "NACA {}{}{:02}",
                (m * 100.0).round() as i64,
                (p * 10.0).round() as i64,
                (t * 100.0).round() as i64
            ),
            points: pts,
        })
    }

    /// "2412" gibi 4 haneli koddan profil.
    pub fn naca4_code(code: &str, n_panels: usize) -> Result<Airfoil, String> {
        let code = code.trim();
        if code.len() != 4 || !code.chars().all(|c| c.is_ascii_digit()) {
            return Err(format!("Geçersiz NACA 4-haneli kod: '{code}'"));
        }
        let d = |a: usize, b: usize| code[a..b].parse::<f64>().unwrap_or(0.0);
        Self::naca4(d(0, 1) / 100.0, d(1, 2) / 10.0, d(2, 4) / 100.0, n_panels)
    }

    /// Rastgele nokta listesinden profil (Selig sırası beklenir; ters yön otomatik düzeltilir).
    /// Kord 1'e ölçeklenir, hücum kenarı x = 0'a taşınır, açık firar kenarı kapatılır
    /// (aralık x ile doğrusal harmanlanarak üst ve alt yüzeye eşit dağıtılır).
    pub fn from_points(name: &str, raw: &[[f64; 2]]) -> Result<Airfoil, String> {
        if raw.iter().any(|p| !p[0].is_finite() || !p[1].is_finite()) {
            return Err("Profil noktalarında sonlu olmayan değer var".into());
        }
        let mut pts: Vec<[f64; 2]> = Vec::with_capacity(raw.len());
        for p in raw {
            if let Some(q) = pts.last() {
                if (q[0] - p[0]).abs() < 1e-12 && (q[1] - p[1]).abs() < 1e-12 {
                    continue;
                }
            }
            pts.push(*p);
        }
        if pts.len() < 9 {
            return Err("Profil için en az 9 farklı nokta gerekli".into());
        }
        let xmin = pts.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
        let xmax = pts.iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max);
        let chord = xmax - xmin;
        if !(chord > 1e-9) {
            return Err("Profil kordu sıfır".into());
        }
        for p in &mut pts {
            p[0] = (p[0] - xmin) / chord;
            p[1] /= chord;
        }
        // Yön: Selig sırası saat yönünün tersidir (pozitif alan).
        let n = pts.len();
        let area: f64 = (0..n)
            .map(|i| {
                let (a, b) = (pts[i], pts[(i + 1) % n]);
                a[0] * b[1] - b[0] * a[1]
            })
            .sum::<f64>()
            * 0.5;
        if area.abs() < 1e-6 {
            return Err("Profil alanı sıfır (noktalar bir doğru üzerinde)".into());
        }
        if area < 0.0 {
            pts.reverse();
        }
        // Firar kenarını kapat.
        let le = le_index(&pts);
        if le < 2 || le + 2 >= pts.len() {
            return Err("Profil noktaları Selig sırasında değil (TE → üst → LE → alt → TE)".into());
        }
        let last = pts.len() - 1;
        let gap = [pts[0][0] - pts[last][0], pts[0][1] - pts[last][1]];
        if gap[0].abs() > 1e-12 || gap[1].abs() > 1e-12 {
            for (i, p) in pts.iter_mut().enumerate() {
                let w = 0.5 * p[0].clamp(0.0, 1.0);
                let sgn = if i <= le { -1.0 } else { 1.0 };
                p[0] += sgn * w * gap[0];
                p[1] += sgn * w * gap[1];
            }
            let te = [
                0.5 * (pts[0][0] + pts[last][0]),
                0.5 * (pts[0][1] + pts[last][1]),
            ];
            pts[0] = te;
            pts[last] = te;
        }
        Ok(Airfoil {
            name: name.to_string(),
            points: pts,
        })
    }

    /// Hücum kenarı (en küçük x) indeksi.
    pub fn le_index(&self) -> usize {
        le_index(&self.points)
    }

    /// Kübik eğri (yay uzunluğu parametreli) üzerinden kosinüs aralıklı yeniden panelleme.
    /// Dönüşte `n_panels` panel (n_panels + 1 nokta) bulunur; hata durumunda profil aynen döner.
    pub fn repaneled(&self, n_panels: usize) -> Airfoil {
        let half = (n_panels.clamp(40, 400) / 2).max(20);
        let n = self.points.len();
        if n < 9 {
            return self.clone();
        }
        let mut s = vec![0.0; n];
        for i in 1..n {
            let d = ((self.points[i][0] - self.points[i - 1][0]).powi(2)
                + (self.points[i][1] - self.points[i - 1][1]).powi(2))
            .sqrt();
            s[i] = s[i - 1] + d;
        }
        let xs: Vec<f64> = self.points.iter().map(|p| p[0]).collect();
        let ys: Vec<f64> = self.points.iter().map(|p| p[1]).collect();
        let (Some(sx), Some(sy)) = (Spline::new(&s, &xs), Spline::new(&s, &ys)) else {
            return self.clone();
        };
        let total = s[n - 1];
        // Hücum kenarı: eğri üzerinde en küçük x.
        let le = self.le_index();
        let (lo, hi) = (s[le.saturating_sub(1)], s[(le + 1).min(n - 1)]);
        let mut s_le = s[le];
        let mut best = f64::INFINITY;
        for k in 0..=400 {
            let sv = lo + (hi - lo) * k as f64 / 400.0;
            let xv = sx.eval(sv);
            if xv < best {
                best = xv;
                s_le = sv;
            }
        }
        let mut pts = Vec::with_capacity(2 * half + 1);
        for k in 0..=half {
            let sv = s_le * 0.5 * (1.0 - (PI * k as f64 / half as f64).cos());
            pts.push([sx.eval(sv), sy.eval(sv)]);
        }
        for k in 1..=half {
            let sv = s_le + (total - s_le) * 0.5 * (1.0 - (PI * k as f64 / half as f64).cos());
            pts.push([sx.eval(sv), sy.eval(sv)]);
        }
        if pts.iter().any(|p| !p[0].is_finite() || !p[1].is_finite()) {
            return self.clone();
        }
        let last = pts.len() - 1;
        pts[0] = self.points[0];
        pts[last] = self.points[n - 1];
        Airfoil {
            name: self.name.clone(),
            points: pts,
        }
    }

    /// Üst ve alt yüzeyin x'e göre sıralı kopyaları (LE → TE).
    fn split_surfaces(&self) -> (Vec<[f64; 2]>, Vec<[f64; 2]>) {
        let le = self.le_index();
        let mut upper: Vec<[f64; 2]> = self.points[..=le].to_vec();
        upper.reverse();
        let lower: Vec<[f64; 2]> = self.points[le..].to_vec();
        (upper, lower)
    }

    /// Ortalama kamber çizgisi: `n + 1` adet [x, y_c] noktası (x: 0..1, kosinüs aralıklı).
    pub fn camber_line(&self, n: usize) -> Vec<[f64; 2]> {
        let n = n.clamp(4, 400);
        let (upper, lower) = self.split_surfaces();
        (0..=n)
            .map(|i| {
                let x = 0.5 * (1.0 - (PI * i as f64 / n as f64).cos());
                let yc = 0.5 * (interp_x(&upper, x) + interp_x(&lower, x));
                [x, if yc.is_finite() { yc } else { 0.0 }]
            })
            .collect()
    }

    /// Maksimum kalınlık oranı (t/c), kord doğrultusuna dik ölçülür.
    pub fn max_thickness(&self) -> f64 {
        let (upper, lower) = self.split_surfaces();
        (1..100)
            .map(|i| {
                let x = i as f64 / 100.0;
                interp_x(&upper, x) - interp_x(&lower, x)
            })
            .filter(|v| v.is_finite())
            .fold(0.0, f64::max)
    }
}

fn le_index(pts: &[[f64; 2]]) -> usize {
    pts.iter()
        .enumerate()
        .min_by(|a, b| a.1[0].total_cmp(&b.1[0]))
        .map(|(i, _)| i)
        .unwrap_or(0)
}

/// x'e göre (yaklaşık) artan nokta dizisinde doğrusal ara değer; aralık dışında uç değer.
fn interp_x(pts: &[[f64; 2]], x: f64) -> f64 {
    if pts.is_empty() {
        return 0.0;
    }
    if x <= pts[0][0] {
        return pts[0][1];
    }
    for w in pts.windows(2) {
        if x <= w[1][0] {
            let dx = w[1][0] - w[0][0];
            return if dx.abs() < 1e-14 {
                w[1][1]
            } else {
                w[0][1] + (w[1][1] - w[0][1]) * (x - w[0][0]) / dx
            };
        }
    }
    pts[pts.len() - 1][1]
}

/// Doğal kübik eğri (ikinci türev uçlarda sıfır).
struct Spline {
    t: Vec<f64>,
    y: Vec<f64>,
    m: Vec<f64>,
}

impl Spline {
    fn new(t: &[f64], y: &[f64]) -> Option<Spline> {
        let n = t.len();
        if n < 3 || y.len() != n || t.windows(2).any(|w| !(w[1] - w[0] > 0.0)) {
            return None;
        }
        // Thomas algoritması ile ikinci türevler.
        let mut m = vec![0.0; n];
        let mut c = vec![0.0; n];
        let mut d = vec![0.0; n];
        for i in 1..n - 1 {
            let h0 = t[i] - t[i - 1];
            let h1 = t[i + 1] - t[i];
            let diag = 2.0 * (h0 + h1) - h0 * c[i - 1];
            let rhs = 6.0 * ((y[i + 1] - y[i]) / h1 - (y[i] - y[i - 1]) / h0);
            c[i] = h1 / diag;
            d[i] = (rhs - h0 * d[i - 1]) / diag;
        }
        for i in (1..n - 1).rev() {
            m[i] = d[i] - c[i] * m[i + 1];
        }
        Some(Spline {
            t: t.to_vec(),
            y: y.to_vec(),
            m,
        })
    }

    fn eval(&self, x: f64) -> f64 {
        let n = self.t.len();
        let i = match self.t.partition_point(|v| *v <= x) {
            0 => 0,
            k if k >= n => n - 2,
            k => k - 1,
        };
        let h = self.t[i + 1] - self.t[i];
        let a = (self.t[i + 1] - x) / h;
        let b = (x - self.t[i]) / h;
        a * self.y[i]
            + b * self.y[i + 1]
            + ((a * a * a - a) * self.m[i] + (b * b * b - b) * self.m[i + 1]) * h * h / 6.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn naca0012_geometry() {
        let a = Airfoil::naca4(0.0, 0.0, 0.12, 160).unwrap();
        assert_eq!(a.points.len(), 161);
        assert_eq!(a.points[0], a.points[160]);
        assert!((a.max_thickness() - 0.12).abs() < 1e-3);
        // Simetri: üst ve alt yüzey ayna görüntüsü.
        for i in 0..=80 {
            let (u, l) = (a.points[80 - i], a.points[80 + i]);
            assert!((u[0] - l[0]).abs() < 1e-12 && (u[1] + l[1]).abs() < 1e-12);
        }
        assert_eq!(a.name, "NACA 0012");
    }

    #[test]
    fn naca2412_camber_line_recovered() {
        let a = Airfoil::naca4_code("2412", 200).unwrap();
        let cl = a.camber_line(50);
        let max = cl.iter().map(|p| p[1]).fold(0.0, f64::max);
        assert!((max - 0.02).abs() < 5e-4, "maks kamber {max}");
    }

    #[test]
    fn from_points_normalises_orientation_scale_and_open_te() {
        let base = Airfoil::naca4_code("4412", 120).unwrap();
        // Ters yön, 250 mm kord, kaydırılmış ve açık firar kenarlı kopya.
        let n = base.points.len();
        let mut raw: Vec<[f64; 2]> = base
            .points
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let gap = if i <= n / 2 { 0.002 } else { -0.002 } * p[0];
                [30.0 + 250.0 * p[0], 250.0 * (p[1] + gap)]
            })
            .collect();
        raw.reverse();
        let a = Airfoil::from_points("test", &raw).unwrap();
        let last = a.points.len() - 1;
        assert_eq!(a.points[0], a.points[last]);
        assert!((a.points[0][0] - 1.0).abs() < 1e-9);
        // Selig yönü: ikinci nokta üst yüzeyde.
        assert!(a.points[3][1] > a.points[last - 3][1]);
        // Burun yakınında x < 0 noktaları (kamber eğimi) yeniden normallemeyi ~1e-4 kaydırır;
        // nokta nokta değil, kalınlık ve kamber özetiyle karşılaştır.
        assert!((a.max_thickness() - base.max_thickness()).abs() < 1e-3);
        let (ca, cb) = (a.camber_line(20), base.camber_line(20));
        let mid = ca.len() / 2;
        assert!(
            (ca[mid][1] - cb[mid][1]).abs() < 1e-3,
            "{:?} {:?}",
            ca[mid],
            cb[mid]
        );
    }

    #[test]
    fn from_points_rejects_bad_input() {
        assert!(Airfoil::from_points("x", &[[0.0, 0.0]; 20]).is_err());
        assert!(Airfoil::from_points("x", &[[f64::NAN, 0.0]; 20]).is_err());
        let line: Vec<[f64; 2]> = (0..20).map(|i| [i as f64, 0.0]).collect();
        assert!(Airfoil::from_points("x", &line).is_err());
        assert!(Airfoil::naca4(0.02, 0.4, f64::NAN, 100).is_err());
        assert!(Airfoil::naca4_code("24a2", 100).is_err());
    }

    #[test]
    fn repanel_preserves_shape() {
        let coarse = Airfoil::naca4_code("2412", 60).unwrap();
        let fine = coarse.repaneled(200);
        assert_eq!(fine.points.len(), 201);
        let exact = Airfoil::naca4_code("2412", 200).unwrap();
        assert!((fine.max_thickness() - exact.max_thickness()).abs() < 1e-3);
        let (a, b) = (fine.camber_line(20), exact.camber_line(20));
        for (p, q) in a.iter().zip(&b) {
            assert!((p[1] - q[1]).abs() < 1e-3);
        }
    }
}
