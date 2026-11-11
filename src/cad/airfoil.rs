// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kanat profili (airfoil) üretimi, Selig .dat ayrıştırma ve analitik NACA 4-haneli hesaplayıcı.

use serde::{Deserialize, Serialize};

/// 2B profil veri yapısı (x: 0..1 kord boyunca, y: korda göre normalize kalınlık/kamber).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AirfoilProfile {
    pub name: String,
    pub is_naca: bool,
    pub naca_code: String,
    pub closed_te: bool,
    pub num_points: usize,
    /// Normalize koordinat çiftleri: [x, y], TE üst yüzeyden (1, 0) başlayıp LE (0, 0) üzerinden TE alt yüzeye (1, 0) döner.
    pub points: Vec<[f64; 2]>,
}

impl Default for AirfoilProfile {
    fn default() -> Self {
        let code = "2412";
        let pts = generate_naca4_points(code, 100, true).unwrap_or_default();
        Self {
            name: "NACA 2412".into(),
            is_naca: true,
            naca_code: code.into(),
            closed_te: true,
            num_points: 100,
            points: pts,
        }
    }
}

impl AirfoilProfile {
    /// Kuyruk yüzeyleri için varsayılan simetrik profil (NACA 0009).
    pub fn default_tail_symmetric() -> Self {
        let code = "0009";
        let pts = generate_naca4_points(code, 80, true).unwrap_or_default();
        Self {
            name: "NACA 0009".into(),
            is_naca: true,
            naca_code: code.into(),
            closed_te: true,
            num_points: 80,
            points: pts,
        }
    }

    /// Maksimum bağıl kalınlık oranı (t/c).
    pub fn max_thickness(&self) -> f64 {
        if self.points.is_empty() {
            return 0.12;
        }
        // Üst ve alt yüzey arasındaki maksimum farkı veya NACA kodundan türet
        if self.is_naca && self.naca_code.len() == 4 {
            if let Ok(t_val) = self.naca_code[2..4].parse::<f64>() {
                return t_val / 100.0;
            }
        }
        let mut max_y = f64::NEG_INFINITY;
        let mut min_y = f64::INFINITY;
        for p in &self.points {
            if p[1] > max_y {
                max_y = p[1];
            }
            if p[1] < min_y {
                min_y = p[1];
            }
        }
        (max_y - min_y).max(0.01)
    }

    /// Maksimum kamber oranı (m/c).
    pub fn max_camber(&self) -> f64 {
        if self.is_naca && self.naca_code.len() == 4 {
            if let Ok(m_val) = self.naca_code[0..1].parse::<f64>() {
                return m_val / 100.0;
            }
        }
        self.camber_from_points()
    }

    /// Koordinat tabanlı (.dat) profillerde ortalama kamber çizgisinin maksimumu (m/c):
    /// üst yüzey noktalarında alt yüzey doğrusal ara değerlenir, (y_üst + y_alt)/2 en büyüğü alınır.
    fn camber_from_points(&self) -> f64 {
        if self.points.len() < 5 {
            return 0.0;
        }
        let le = self
            .points
            .iter()
            .enumerate()
            .min_by(|a, b| a.1[0].total_cmp(&b.1[0]))
            .map(|(i, _)| i)
            .unwrap_or(0);
        let upper = &self.points[..=le];
        let mut lower: Vec<[f64; 2]> = self.points[le..].to_vec();
        lower.sort_by(|a, b| a[0].total_cmp(&b[0]));
        if lower.len() < 2 {
            return 0.0;
        }
        let y_lower_at = |x: f64| -> Option<f64> {
            let i = lower.iter().position(|q| q[0] >= x)?;
            if i == 0 {
                return Some(lower[0][1]);
            }
            let (p0, p1) = (lower[i - 1], lower[i]);
            let dx = p1[0] - p0[0];
            if dx.abs() < 1e-12 {
                Some(p1[1])
            } else {
                Some(p0[1] + (p1[1] - p0[1]) * (x - p0[0]) / dx)
            }
        };
        let mut best = 0.0f64;
        for u in upper {
            if u[0] <= 0.0 || u[0] >= 1.0 {
                continue;
            }
            if let Some(yl) = y_lower_at(u[0]) {
                let c = 0.5 * (u[1] + yl);
                if c.is_finite() && c.abs() > best {
                    best = c.abs();
                }
            }
        }
        best
    }
}

/// NACA 4-haneli analitik formülü ile kosinüs aralıklı profil noktaları üretir.
///
/// Formül:
/// m = M / 100 (maks kamber)
/// p = P / 10 (maks kamber konumu)
/// t = XX / 100 (maks kalınlık)
///
/// y_t(x) = 5*t * (0.2969*sqrt(x) - 0.1260*x - 0.3516*x^2 + 0.2843*x^3 + a4*x^4)
/// Kapalı firar kenarı (closed_te = true) için a4 = -0.1036 (y_t(1) = 0).
/// Açık firar kenarı (closed_te = false) için a4 = -0.1015 (y_t(1) = 0.0105*t).
pub fn generate_naca4_points(
    code: &str,
    num_points: usize,
    closed_te: bool,
) -> Result<Vec<[f64; 2]>, String> {
    let code = code.trim();
    if code.len() != 4 || !code.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!("Geçersiz NACA 4-haneli kod: '{}'", code));
    }

    let m_digit = code[0..1].parse::<f64>().map_err(|e| e.to_string())?;
    let p_digit = code[1..2].parse::<f64>().map_err(|e| e.to_string())?;
    let t_digit = code[2..4].parse::<f64>().map_err(|e| e.to_string())?;

    let m = m_digit / 100.0;
    let p = p_digit / 10.0;
    let t = (t_digit / 100.0).max(0.01);

    let n = num_points.clamp(20, 500);
    let half_n = n / 2 + 1;

    let a4 = if closed_te { -0.1036 } else { -0.1015 };

    let mut upper = Vec::with_capacity(half_n);
    let mut lower = Vec::with_capacity(half_n);

    // Kosinüs aralıklı dağılım: beta = 0..PI
    for i in 0..half_n {
        let beta = (i as f64) * std::f64::consts::PI / ((half_n - 1) as f64);
        let x = (1.0 - beta.cos()) / 2.0;

        let y_t = 5.0
            * t
            * (0.2969 * x.sqrt() - 0.1260 * x - 0.3516 * x * x
                + 0.2843 * x.powi(3)
                + a4 * x.powi(4));

        let (y_c, dy_c) = if m.abs() < 1e-9 || p.abs() < 1e-9 {
            (0.0, 0.0)
        } else if x < p {
            let y_c = (m / (p * p)) * (2.0 * p * x - x * x);
            let dy_c = (2.0 * m / (p * p)) * (p - x);
            (y_c, dy_c)
        } else {
            let y_c = (m / ((1.0 - p) * (1.0 - p))) * ((1.0 - 2.0 * p) + 2.0 * p * x - x * x);
            let dy_c = (2.0 * m / ((1.0 - p) * (1.0 - p))) * (p - x);
            (y_c, dy_c)
        };

        let theta = dy_c.atan();

        let xu = x - y_t * theta.sin();
        let yu = y_c + y_t * theta.cos();

        let xl = x + y_t * theta.sin();
        let yl = y_c - y_t * theta.cos();

        upper.push([xu, yu]);
        lower.push([xl, yl]);
    }

    // Sıralama: TE üst (x=1) -> LE (x=0) -> TE alt (x=1)
    let mut full_pts = Vec::with_capacity(half_n * 2);
    // Üst yüzey ters sırada (1.0 -> 0.0)
    for pt in upper.into_iter().rev() {
        full_pts.push(pt);
    }
    // Alt yüzey (0.0 -> 1.0), LE noktası (0,0) iki kez eklenmemesi için 1'den başla
    for pt in lower.into_iter().skip(1) {
        full_pts.push(pt);
    }

    Ok(full_pts)
}

/// Selig formatındaki .dat dosya içeriğini okur.
///
/// Tipik format:
/// Satır 1: Profil Başlığı / Adı
/// Satır 2..N: x y koordinatları (TE üst -> LE -> TE alt veya iki blok halinde)
pub fn parse_selig_dat(content: &str) -> Result<AirfoilProfile, String> {
    let mut lines = content.lines();
    let mut name = String::from("Custom Airfoil");

    // İlk boş olmayan satırı kontrol et
    let mut raw_points = Vec::new();

    for line in lines.by_ref() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
            continue;
        }

        let parts: Vec<&str> = trimmed
            .split_whitespace()
            .map(|s| s.trim_matches(','))
            .filter(|s| !s.is_empty())
            .collect();

        if parts.len() == 2 {
            if let (Ok(x), Ok(y)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>()) {
                raw_points.push([x, y]);
                continue;
            }
        }

        // Sayısal iki değer değilse ve henüz isim atanmamışsa başlık satırıdır
        if raw_points.is_empty() {
            name = trimmed.to_string();
        }
    }

    if raw_points.len() < 10 {
        return Err("Geçerli bir profil için en az 10 koordinat noktası gereklidir.".into());
    }

    // Koordinatları 0..1 aralığına normalize et (eğer 0..100 gibi yüzde değerleri verilmişse)
    let max_x = raw_points
        .iter()
        .map(|p| p[0])
        .fold(f64::NEG_INFINITY, f64::max);
    if max_x > 2.0 {
        for p in &mut raw_points {
            p[0] /= max_x;
            p[1] /= max_x;
        }
    }

    let num_points = raw_points.len();
    Ok(AirfoilProfile {
        name,
        is_naca: false,
        naca_code: String::new(),
        closed_te: false,
        num_points,
        points: raw_points,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dat_airfoil_camber_from_coordinates() {
        let mut a = AirfoilProfile::default();
        a.points = generate_naca4_points("4412", 100, true).unwrap();
        a.naca_code = String::new();
        a.is_naca = false;
        let c = a.max_camber();
        assert!((c - 0.04).abs() < 0.004, "kamber ~0.04 olmali, bulunan {c}");
        // Simetrik profil -> ~0
        a.points = generate_naca4_points("0012", 100, true).unwrap();
        assert!(a.max_camber() < 0.002);
        // NACA yolunda kod kullanilir
        let n = AirfoilProfile::default();
        assert!((n.max_camber() - 0.02).abs() < 1e-12);
    }

    #[test]
    fn test_naca_0012_symmetric() {
        let pts = generate_naca4_points("0012", 100, true).unwrap();
        assert!(!pts.is_empty());
        // Simetrik profil kontrolü: LE noktasında y=0 civarı
        let le = pts
            .iter()
            .min_by(|a, b| a[0].partial_cmp(&b[0]).unwrap())
            .unwrap();
        assert!(le[0].abs() < 1e-3);
        assert!(le[1].abs() < 1e-3);

        // Firar kenarı kapalı kontrolü
        let te_start = pts.first().unwrap();
        let te_end = pts.last().unwrap();
        assert!((te_start[0] - 1.0).abs() < 1e-3);
        assert!((te_end[0] - 1.0).abs() < 1e-3);
        assert!(te_start[1].abs() < 1e-3);
        assert!(te_end[1].abs() < 1e-3);
    }

    #[test]
    fn test_naca_2412_cambered() {
        let pts = generate_naca4_points("2412", 100, true).unwrap();
        assert!(pts.len() >= 50);
        let prof = AirfoilProfile {
            name: "NACA 2412".into(),
            is_naca: true,
            naca_code: "2412".into(),
            closed_te: true,
            num_points: pts.len(),
            points: pts,
        };
        assert!((prof.max_thickness() - 0.12).abs() < 1e-4);
        assert!((prof.max_camber() - 0.02).abs() < 1e-4);
    }

    #[test]
    fn test_parse_selig_dat() {
        let dat = r#"
CLARK Y TEST
1.0000 0.0000
0.7500 0.0450
0.5000 0.0800
0.2500 0.0850
0.0000 0.0000
0.2500 -0.0250
0.5000 -0.0150
0.7500 -0.0050
0.9000 -0.0020
1.0000 0.0000
"#;
        let prof = parse_selig_dat(dat).unwrap();
        assert_eq!(prof.name, "CLARK Y TEST");
        assert_eq!(prof.points.len(), 10);
        assert_eq!(prof.points[0], [1.0, 0.0]);
    }
}
