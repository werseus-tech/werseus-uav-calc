// SPDX-License-Identifier: AGPL-3.0-or-later
//! 3D at nalı girdap kafesi (Vortex Lattice Method, VLM), düz (düzlemsel, kamber ve burulma yok).
//!
//! Kaynaklar: Katz & Plotkin, "Low-Speed Aerodynamics" (2. bas., 2001), Böl. 12.4 (at nalı girdap,
//! bağ girdabı panel çeyrek-kordunda, kontrol noktası 3/4-kordda, Biot–Savart); Trefftz düzlemi
//! indüklenmiş sürükleme: Katz & Plotkin Böl. 8 ve Drela, "Flight Vehicle Aerodynamics" (2014),
//! Böl. 6.
//!
//! Varsayımlar: sıkıştırılamaz, viskoz olmayan akış; iz girdapları serbest akım (x) yönünde;
//! tüm yüzeyler z = 0 düzleminde (dihedral/V-kuyruk yok); yatay kuyruk, kanat izi düzleminde
//! ve eş düzlemli kabul edilir (kaba bir yaklaşım). Birim: V∞ = 1; uzunluklar metre.

use std::f64::consts::PI;
use std::sync::atomic::{AtomicBool, Ordering};

/// Bir yüzeyin açıklık istasyonu (kenar): y konumu, hücum kenarı x'i, kord.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Station {
    pub y: f64,
    pub x_le: f64,
    pub chord: f64,
}

/// Bir kaldırma yüzeyi: y'ye göre artan istasyonlar (tam açıklık, −b/2 → +b/2).
#[derive(Clone, Debug)]
pub struct Surface {
    pub name: String,
    pub stations: Vec<Station>,
}

impl Surface {
    /// Trapez kanat: kök kordu `c_r`, sivrilme `taper`, hücum kenarı süpürme açısı (derece).
    /// `x0`: kök hücum kenarı x'i. Açıklık boyunca kosinüs aralıklı `n_span` (çift) şerit.
    pub fn trapezoid(
        name: &str,
        b: f64,
        c_r: f64,
        taper: f64,
        sweep_le_deg: f64,
        x0: f64,
        n_span: usize,
    ) -> Surface {
        let n = (n_span.clamp(4, 80) / 2) * 2;
        let tan_le = sweep_le_deg.to_radians().tan();
        let stations = (0..=n)
            .map(|k| {
                let y = -0.5 * b * (PI * k as f64 / n as f64).cos();
                let f = y.abs() / (0.5 * b);
                Station {
                    y,
                    x_le: x0 + y.abs() * tan_le,
                    chord: c_r * (1.0 - (1.0 - taper) * f),
                }
            })
            .collect();
        Surface {
            name: name.to_string(),
            stations,
        }
    }

    /// Alan (şerit trapezlerinin toplamı).
    pub fn area(&self) -> f64 {
        self.stations
            .windows(2)
            .map(|w| 0.5 * (w[0].chord + w[1].chord) * (w[1].y - w[0].y))
            .sum()
    }

    pub fn span(&self) -> f64 {
        match (self.stations.first(), self.stations.last()) {
            (Some(a), Some(b)) => b.y - a.y,
            _ => 0.0,
        }
    }
}

/// Bir açıklık şeridi için yük.
#[derive(Clone, Copy, Debug, Default)]
pub struct StripLoad {
    pub surface: usize,
    pub y: f64,
    pub chord: f64,
    /// Şerit dolaşımı (toplam, kord boyunca) — V∞ = 1.
    pub gamma: f64,
    /// Yerel kaldırma katsayısı cl = 2Γ/(V c).
    pub cl: f64,
}

#[derive(Clone, Debug, Default)]
pub struct VlmResult {
    pub alpha_deg: f64,
    pub cl: f64,
    pub cd_i: f64,
    /// Oswald verimliliği e = CL²/(π AR CD_i); CL ≈ 0 ise 0.
    pub e: f64,
    pub cl_alpha_per_rad: f64,
    /// Nötr nokta x konumu (V∞ eksenine göre, m); CL_α ≈ 0 ise `None`.
    pub x_np: Option<f64>,
    pub strips: Vec<StripLoad>,
    /// Yüzey başına CL katkısı (S_ref'e göre).
    pub cl_per_surface: Vec<f64>,
    pub ar: f64,
}

type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn dot3(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Birim şiddetli sonlu girdap parçasının (A→B) P'deki hızı (Biot–Savart).
fn seg(p: V3, a: V3, b: V3) -> V3 {
    let (r1, r2, r0) = (sub(p, a), sub(p, b), sub(b, a));
    let cr = cross(r1, r2);
    let cr2 = dot3(cr, cr);
    let (n1, n2, n0) = (
        dot3(r1, r1).sqrt(),
        dot3(r2, r2).sqrt(),
        dot3(r0, r0).sqrt(),
    );
    // Çekirdek koruması: P parça üzerine (ya da uzantısına) çok yakınsa katkı sıfır.
    if n0 < 1e-12 || n1 < 1e-12 || n2 < 1e-12 || cr2 < 1e-18 * n0 * n0 {
        return [0.0; 3];
    }
    let k = (dot3(r0, r1) / n1 - dot3(r0, r2) / n2) / (4.0 * PI * cr2);
    [cr[0] * k, cr[1] * k, cr[2] * k]
}

/// At nalı girdabın (bağ: A→B, iz bacakları +x yönünde sonsuza) P'deki hızı.
fn horseshoe(p: V3, a: V3, b: V3, far: f64) -> V3 {
    let a_inf = [a[0] + far, a[1], a[2]];
    let b_inf = [b[0] + far, b[1], b[2]];
    let (v1, v2, v3) = (seg(p, a_inf, a), seg(p, a, b), seg(p, b, b_inf));
    [
        v1[0] + v2[0] + v3[0],
        v1[1] + v2[1] + v3[1],
        v1[2] + v2[2] + v3[2],
    ]
}

struct Panel {
    a: V3,
    b: V3,
    cp: V3,
    surf: usize,
    strip: usize,
}

fn build_panels(surfaces: &[Surface], nc: usize) -> Vec<Panel> {
    let mut out = Vec::new();
    for (si, s) in surfaces.iter().enumerate() {
        for (j, w) in s.stations.windows(2).enumerate() {
            let (l, r) = (w[0], w[1]);
            for k in 0..nc {
                let f = |st: Station, q: f64| st.x_le + st.chord * (k as f64 + q) / nc as f64;
                out.push(Panel {
                    a: [f(l, 0.25), l.y, 0.0],
                    b: [f(r, 0.25), r.y, 0.0],
                    cp: [0.5 * (f(l, 0.75) + f(r, 0.75)), 0.5 * (l.y + r.y), 0.0],
                    surf: si,
                    strip: j,
                });
            }
        }
    }
    out
}

/// Çözüm. `s_ref`: referans alan (genelde ana kanat), `x_ref`: moment/nötr nokta referansı
/// (yalnız x_np için kullanılmaz; x_np türevlerden hesaplanır), `c_ref`: referans kord (MAC).
/// `n_chord`: kord yönü panel sayısı (1–12). Geçersiz girdide `Err`; `cancel` ile iptal edilebilir.
pub fn solve(
    surfaces: &[Surface],
    s_ref: f64,
    c_ref: f64,
    x_ref: f64,
    alpha_deg: f64,
    n_chord: usize,
    cancel: &AtomicBool,
) -> Result<VlmResult, String> {
    if surfaces.is_empty() || !(s_ref > 0.0) || !(c_ref > 0.0) || !alpha_deg.is_finite() {
        return Err("Geçersiz VLM girdisi".into());
    }
    for s in surfaces {
        if s.stations.len() < 3
            || s.stations.iter().any(|t| {
                !(t.y.is_finite() && t.x_le.is_finite() && t.chord.is_finite()) || t.chord < 0.0
            })
            || s.stations.windows(2).any(|w| w[1].y <= w[0].y)
        {
            return Err(format!("Yüzey geçersiz: {}", s.name));
        }
    }
    let nc = n_chord.clamp(1, 12);
    let panels = build_panels(surfaces, nc);
    let n = panels.len();
    if n > 2500 {
        return Err("Panel sayısı çok fazla".into());
    }
    let max_dim = surfaces
        .iter()
        .flat_map(|s| s.stations.iter())
        .fold(1e-6f64, |m, t| m.max(t.y.abs()).max(t.x_le.abs() + t.chord));
    let far = 1e5 * max_dim;
    let mut a = vec![0.0; n * n];
    for i in 0..n {
        if cancel.load(Ordering::Relaxed) {
            return Err("İptal edildi".into());
        }
        for j in 0..n {
            a[i * n + j] = horseshoe(panels[i].cp, panels[j].a, panels[j].b, far)[2];
        }
    }
    let lu = super::linalg::Lu::factor(a, n).ok_or("VLM sistemi tekil")?;
    let solve_at = |alpha: f64| -> Result<Vec<f64>, String> {
        let rhs = vec![-alpha.to_radians().sin(); n];
        lu.solve(&rhs)
            .ok_or_else(|| "VLM çözümü sonlu değil".to_string())
    };
    let gam = solve_at(alpha_deg)?;

    // Şerit dolaşımları ve kuvvetler.
    let mut strips: Vec<StripLoad> = Vec::new();
    let mut offsets = Vec::new(); // yüzey başına şerit başlangıcı
    for (si, s) in surfaces.iter().enumerate() {
        offsets.push(strips.len());
        for w in s.stations.windows(2) {
            let chord = 0.5 * (w[0].chord + w[1].chord);
            strips.push(StripLoad {
                surface: si,
                y: 0.5 * (w[0].y + w[1].y),
                chord,
                gamma: 0.0,
                cl: 0.0,
            });
        }
    }
    let force = |g: &[f64]| -> (f64, f64, Vec<f64>) {
        // (CL, CM_ref, yüzey CL'leri) — V∞ = 1
        let mut cl_s = vec![0.0; surfaces.len()];
        let mut mom = 0.0;
        for (p, gi) in panels.iter().zip(g) {
            let dy = p.b[1] - p.a[1];
            let dl = 2.0 * gi * dy / s_ref;
            cl_s[p.surf] += dl;
            mom -= dl * (0.5 * (p.a[0] + p.b[0]) - x_ref);
        }
        (cl_s.iter().sum(), mom / c_ref, cl_s)
    };
    let (cl, _cm, cl_per_surface) = force(&gam);
    for (p, gi) in panels.iter().zip(&gam) {
        strips[offsets[p.surf] + p.strip].gamma += gi;
    }
    for s in &mut strips {
        s.cl = if s.chord > 1e-12 {
            2.0 * s.gamma / s.chord
        } else {
            0.0
        };
    }

    // Trefftz düzlemi: iz girdapları kenarlarda Γ_x,e = G_{e−1} − G_e.
    let mut cd_i = 0.0;
    let mut edges: Vec<(f64, f64)> = Vec::new(); // (y, Γ_x)
    for (si, s) in surfaces.iter().enumerate() {
        let ns = s.stations.len() - 1;
        for e in 0..=ns {
            let g_prev = if e > 0 {
                strips[offsets[si] + e - 1].gamma
            } else {
                0.0
            };
            let g_next = if e < ns {
                strips[offsets[si] + e].gamma
            } else {
                0.0
            };
            edges.push((s.stations[e].y, g_prev - g_next));
        }
    }
    for (si, s) in surfaces.iter().enumerate() {
        for (j, w) in s.stations.windows(2).enumerate() {
            let (ys, dy) = (0.5 * (w[0].y + w[1].y), w[1].y - w[0].y);
            let wz: f64 = edges
                .iter()
                .map(|(ye, gx)| {
                    let d = ys - ye;
                    if d.abs() < 1e-12 {
                        0.0
                    } else {
                        gx / (2.0 * PI * d)
                    }
                })
                .sum();
            cd_i += -strips[offsets[si] + j].gamma * wz * dy;
        }
    }
    // D/(q S) = −(ρ/2)ΣGwΔy / (½ρ S) (V∞ = 1). Burada w, Trefftz düzlemindeki (2D) iniş hızıdır
    // ve kanat düzlemindekinin iki katıdır (w_TP = 2 w_kanat); D = −ρ ΣG w_kanat Δy = −(ρ/2) ΣG w_TP Δy.
    cd_i /= s_ref;
    let span = surfaces[0].span();
    let ar = span * span / s_ref;
    let e = if cl.abs() > 1e-6 && cd_i > 1e-12 {
        cl * cl / (PI * ar * cd_i)
    } else {
        0.0
    };

    // Türevler (CL_α, CM_α) iki küçük açıdan; model doğrusal olduğundan kesindir.
    let (cl1, cm1, _) = force(&solve_at(1.0)?);
    let (cl0, cm0, _) = force(&solve_at(0.0)?);
    let da = 1.0f64.to_radians();
    let cla = (cl1 - cl0) / da;
    let cma = (cm1 - cm0) / da;
    let x_np = if cla.abs() > 1e-9 {
        Some(x_ref - c_ref * cma / cla)
    } else {
        None
    };
    let res = VlmResult {
        alpha_deg,
        cl,
        cd_i,
        e,
        cl_alpha_per_rad: cla,
        x_np,
        strips,
        cl_per_surface,
        ar,
    };
    if !(res.cl.is_finite() && res.cd_i.is_finite() && res.cl_alpha_per_rad.is_finite()) {
        return Err("VLM sonucu sonlu değil".into());
    }
    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn never() -> AtomicBool {
        AtomicBool::new(false)
    }

    /// Eliptik planform, çeyrek-kord düz: c(y) = c0 √(1 − (2y/b)²).
    fn elliptic(b: f64, c0: f64, n: usize) -> Surface {
        let stations = (0..=n)
            .map(|k| {
                let y = -0.5 * b * (PI * k as f64 / n as f64).cos();
                let c = c0 * (1.0 - (2.0 * y / b).powi(2)).max(0.0).sqrt();
                Station {
                    y,
                    x_le: -0.25 * c,
                    chord: c,
                }
            })
            .collect();
        Surface {
            name: "elips".into(),
            stations,
        }
    }

    #[test]
    fn elliptic_wing_has_e_near_one_and_elliptic_load() {
        // Kaynak: Prandtl teorisi — eliptik dağılım e = 1. VLM ayrıklaştırma hatası ve Trefftz
        // integrali için ±%4 tolerans.
        let b = 8.0;
        let c0 = 1.0;
        let s = elliptic(b, c0, 40);
        let area = std::f64::consts::PI * b * c0 / 4.0;
        let r = solve(&[s], area, area / b, 0.0, 4.0, 6, &never()).unwrap();
        assert!((r.e - 1.0).abs() < 0.04, "e={}", r.e);
        // Yük dağılımı eliptik: orta şerit Γ, kenar şeritten büyük; Γ(y)/Γ0 ≈ √(1−(2y/b)²).
        let g0 = r.strips.iter().map(|s| s.gamma).fold(0.0, f64::max);
        for s in &r.strips {
            let ell = (1.0 - (2.0 * s.y / b).powi(2)).max(0.0).sqrt();
            assert!(
                (s.gamma / g0 - ell).abs() < 0.06,
                "y={} {} vs {}",
                s.y,
                s.gamma / g0,
                ell
            );
        }
    }

    #[test]
    fn rectangular_wing_cl_alpha_close_to_helmbold() {
        // Helmbold: CLα = 2πAR/(2 + √(4 + AR²)), (a0 = 2π, Mach 0, süpürme yok) — programın
        // mevcut kanat formülü. Katz & Plotkin Şekil 12.x: VLM bu AR'lerde ±%6 içinde.
        for ar in [6.0, 10.0] {
            let b = 2.0;
            let c = b / ar;
            let s = Surface::trapezoid("kanat", b, c, 1.0, 0.0, 0.0, 30);
            let r = solve(&[s], b * c, c, 0.25 * c, 3.0, 6, &never()).unwrap();
            let helm = crate::calc::aero::cl_alpha_wing(ar, 2.0 * PI, 0.0, 0.0);
            assert!(
                (r.cl_alpha_per_rad / helm - 1.0).abs() < 0.06,
                "AR={ar} vlm={} helm={helm}",
                r.cl_alpha_per_rad
            );
            // Düz plakada nötr nokta ≈ %25 MAC (±%3 kord).
            let xnp = r.x_np.unwrap();
            assert!((xnp / c - 0.25).abs() < 0.04, "xnp/c={}", xnp / c);
            assert!(r.e > 0.85 && r.e < 1.02, "e={}", r.e);
        }
    }

    #[test]
    fn tail_adds_lift_and_moves_np_aft_of_wing_only() {
        let wing = Surface::trapezoid("kanat", 2.0, 0.25, 0.8, 5.0, 0.0, 24);
        let tail = Surface::trapezoid("kuyruk", 0.6, 0.15, 1.0, 0.0, 0.9, 12);
        let s = wing.area();
        let w = solve(std::slice::from_ref(&wing), s, 0.23, 0.06, 2.0, 4, &never()).unwrap();
        let wt = solve(&[wing, tail], s, 0.23, 0.06, 2.0, 4, &never()).unwrap();
        assert!(wt.cl_per_surface[1] > 0.0);
        assert!(
            wt.x_np.unwrap() > w.x_np.unwrap(),
            "{:?} {:?}",
            wt.x_np,
            w.x_np
        );
    }

    #[test]
    fn invalid_input_and_cancel() {
        let w = Surface::trapezoid("k", 2.0, 0.25, 1.0, 0.0, 0.0, 8);
        assert!(solve(&[], 1.0, 1.0, 0.0, 0.0, 2, &never()).is_err());
        assert!(solve(std::slice::from_ref(&w), 0.0, 1.0, 0.0, 0.0, 2, &never()).is_err());
        assert!(
            solve(
                std::slice::from_ref(&w),
                0.5,
                1.0,
                0.0,
                f64::NAN,
                2,
                &never()
            )
            .is_err()
        );
        assert!(solve(&[w], 0.5, 0.25, 0.0, 2.0, 2, &AtomicBool::new(true)).is_err());
    }
}
