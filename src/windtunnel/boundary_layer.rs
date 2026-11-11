// SPDX-License-Identifier: AGPL-3.0-or-later
//! İntegral sınır tabaka yürüyüşü (tek yüzey, durma noktasından firar kenarına).
//!
//! Yöntemler ve kaynaklar:
//! - Laminer: Thwaites yöntemi, θ² = 0.45 ν / Ue⁶ · ∫ Ue⁵ ds (Thwaites 1949); H(λ) ve l(λ)
//!   bağıntıları Cebeci & Bradshaw, "Momentum Transfer in Boundary Layers" (1977) eğri uydurmaları.
//! - Geçiş: Michel ölçütü, Cebeci–Smith biçimi Re_θ,tr = 1.174 (1 + 22400/Re_x) Re_x^0.46
//!   (Cebeci & Smith 1974). Laminer ayrılma (λ ≤ −0.09) görülürse kısa kabarcık varsayımıyla
//!   o noktada türbülansa geçildiği kabul edilir.
//! - Türbülanslı: Head'in sürüklenme (entrainment) yöntemi (Head 1958) + Ludwieg–Tillmann
//!   sürtünme yasası; H₁(H) ve F(H₁) bağıntıları Cebeci & Bradshaw (1977).
//! - Profil sürüklemesi: Squire–Young formülü (Squire & Young 1938).
//!
//! Boyutsuzlaştırma: uzunluklar kord c ile, hızlar V∞ ile; ν = 1/Re.
//! Viskoz–viskoz olmayan etkileşim (yer değiştirme kalınlığı geri beslemesi) YOKTUR; sonuçlar
//! ön tasarım düzeyinde yaklaşık değerlerdir.

use serde::{Deserialize, Serialize};

/// Sınır tabaka yürüyüş ayarları.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BlOptions {
    /// Türbülanslı ayrılma eşiği (şekil faktörü H). Literatürde 1.8–2.4 aralığı kullanılır.
    pub h_separation: f64,
    /// Geçişten hemen sonraki başlangıç şekil faktörü.
    pub h_transition: f64,
    /// Zorlanmış geçiş konumu (durma noktasından yüzey mesafesi, kord oranı). `None`: serbest geçiş.
    pub forced_transition_s: Option<f64>,
}

impl Default for BlOptions {
    fn default() -> Self {
        BlOptions {
            h_separation: 2.4,
            h_transition: 1.4,
            forced_transition_s: None,
        }
    }
}

/// Tek bir sınır tabaka istasyonu.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BlStation {
    /// Durma noktasından yüzey boyunca mesafe (s/c).
    pub s: f64,
    /// Sınır tabaka kenar hızı (Ue/V∞).
    pub ue: f64,
    /// Momentum kalınlığı (θ/c).
    pub theta: f64,
    /// Şekil faktörü H = δ*/θ.
    pub h: f64,
    /// Yerel sürtünme katsayısı (Ue dinamik basıncına göre).
    pub cf: f64,
    pub turbulent: bool,
}

/// Bir yüzeyin sınır tabaka sonucu.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BlResult {
    pub stations: Vec<BlStation>,
    /// Türbülansa geçilen ilk istasyonun indeksi.
    pub transition_index: Option<usize>,
    /// Geçiş, laminer ayrılma nedeniyle tetiklendiyse `true`.
    pub laminar_separation: bool,
    /// Türbülanslı ayrılmanın görüldüğü ilk istasyonun indeksi.
    pub separation_index: Option<usize>,
    /// Squire–Young ile bu yüzeyin sürükleme katkısı (kord ve V∞ ile boyutsuz).
    pub cd: f64,
}

fn thwaites_h(lambda: f64) -> f64 {
    let l = lambda.clamp(-0.1, 0.1);
    if l >= 0.0 {
        2.61 - 3.75 * l + 5.24 * l * l
    } else {
        2.088 + 0.0731 / (l + 0.14)
    }
}

fn thwaites_l(lambda: f64) -> f64 {
    let l = lambda.clamp(-0.1, 0.1);
    if l >= 0.0 {
        0.22 + 1.57 * l - 1.8 * l * l
    } else {
        0.22 + 1.402 * l + 0.018 * l / (l + 0.107)
    }
}

/// Head: H₁ = G(H).
fn head_h1(h: f64) -> f64 {
    let h = h.max(1.12);
    if h <= 1.6 {
        3.3 + 0.8234 * (h - 1.1).powf(-1.287)
    } else {
        3.3 + 1.5501 * (h - 0.6778).powf(-3.064)
    }
}

/// Head: H = G⁻¹(H₁).
fn head_h(h1: f64) -> f64 {
    let d = (h1 - 3.3).max(1e-6);
    // H = 1.6 için H₁ ≈ 5.3
    if h1 >= head_h1(1.6) {
        1.1 + (d / 0.8234).powf(-1.0 / 1.287)
    } else {
        0.6778 + (d / 1.5501).powf(-1.0 / 3.064)
    }
}

/// Ludwieg–Tillmann sürtünme yasası.
fn cf_ludwieg_tillmann(h: f64, re_theta: f64) -> f64 {
    0.246 * 10f64.powf(-0.678 * h) * re_theta.max(1.0).powf(-0.268)
}

/// ∫ Ue⁵ ds, Ue aralıkta doğrusal kabul edilerek (kesin integral).
fn int_ue5(u1: f64, u2: f64, ds: f64) -> f64 {
    if (u2 - u1).abs() < 1e-9 * (u1.abs() + u2.abs()).max(1e-12) {
        ds * (0.5 * (u1 + u2)).powi(5)
    } else {
        ds * (u2.powi(6) - u1.powi(6)) / (6.0 * (u2 - u1))
    }
}

/// Düzgün olmayan aralıklı üç noktalı türev.
fn derivative(s: &[f64], u: &[f64], k: usize) -> f64 {
    let n = s.len();
    let safe = |a: usize, b: usize| {
        let d = s[b] - s[a];
        if d.abs() > 1e-14 {
            (u[b] - u[a]) / d
        } else {
            0.0
        }
    };
    if k == 0 {
        safe(0, 1)
    } else if k + 1 >= n {
        safe(n - 2, n - 1)
    } else {
        let (h0, h1) = (s[k] - s[k - 1], s[k + 1] - s[k]);
        if h0 <= 1e-14 || h1 <= 1e-14 {
            safe(k - 1, k + 1)
        } else {
            (safe(k - 1, k) * h1 + safe(k, k + 1) * h0) / (h0 + h1)
        }
    }
}

/// Sınır tabakayı `s` (artan, s[0] = durma noktası ya da levha ucu) boyunca yürütür.
///
/// `ue`: kenar hızı (≥ 0), `reynolds`: V∞·c/ν. Girdi geçersizse `None` döner; çıktı alanlarının
/// tümü sonludur.
pub fn march(s: &[f64], ue: &[f64], reynolds: f64, opts: &BlOptions) -> Option<BlResult> {
    let n = s.len();
    if n < 3
        || ue.len() != n
        || !reynolds.is_finite()
        || reynolds < 1.0
        || s.iter().chain(ue).any(|v| !v.is_finite())
        || s.windows(2).any(|w| w[1] <= w[0])
        || ue.iter().any(|v| *v < 0.0)
    {
        return None;
    }
    let nu = 1.0 / reynolds;
    let h_sep = if opts.h_separation.is_finite() {
        opts.h_separation.clamp(1.6, 3.0)
    } else {
        2.4
    };
    let h_tr = if opts.h_transition.is_finite() {
        opts.h_transition.clamp(1.2, 1.8)
    } else {
        1.4
    };
    let mut out = BlResult {
        stations: Vec::with_capacity(n),
        ..Default::default()
    };

    // --- Laminer bölüm (Thwaites)
    let mut integral = 0.0;
    let mut k_tr = None;
    for k in 0..n {
        let due = derivative(s, ue, k);
        let theta = if k == 0 {
            if ue[0] < 1e-9 && due > 0.0 {
                (0.075 * nu / due).sqrt()
            } else {
                0.0
            }
        } else {
            integral += int_ue5(ue[k - 1], ue[k], s[k] - s[k - 1]);
            if ue[k] > 1e-9 {
                (0.45 * nu * integral / ue[k].powi(6)).sqrt()
            } else {
                out.stations[k - 1].theta
            }
        };
        let lambda = theta * theta * due / nu;
        let h = thwaites_h(lambda);
        let cf = if theta > 0.0 && ue[k] > 1e-9 {
            2.0 * nu * thwaites_l(lambda) / (ue[k] * theta)
        } else {
            0.0
        };
        out.stations.push(BlStation {
            s: s[k],
            ue: ue[k],
            theta,
            h,
            cf: cf.max(0.0),
            turbulent: false,
        });
        if k == 0 {
            continue;
        }
        let re_x = ue[k] * (s[k] - s[0]) / nu;
        let re_theta = ue[k] * theta / nu;
        let forced = opts.forced_transition_s.is_some_and(|sf| s[k] - s[0] >= sf);
        let michel = re_x > 1.0 && re_theta >= 1.174 * (1.0 + 22400.0 / re_x) * re_x.powf(0.46);
        let lam_sep = lambda <= -0.09;
        if forced || michel || lam_sep {
            out.laminar_separation = lam_sep && !forced && !michel;
            k_tr = Some(k);
            break;
        }
    }

    // --- Türbülanslı bölüm (Head)
    if let Some(k0) = k_tr {
        out.transition_index = Some(k0);
        let mut theta = out.stations[k0].theta.max(1e-9);
        let mut h1 = head_h1(h_tr);
        let mut separated = false;
        let st = &mut out.stations[k0];
        st.turbulent = true;
        st.h = h_tr;
        st.cf = cf_ludwieg_tillmann(h_tr, ue[k0] * theta / nu);
        for k in (k0 + 1)..n {
            let ds = s[k] - s[k - 1];
            let due = (ue[k] - ue[k - 1]) / ds;
            const SUB: usize = 8;
            let hstep = ds / SUB as f64;
            for j in 0..SUB {
                let u_at = |frac: f64| (ue[k - 1] + due * ds * frac).max(1e-6);
                let rhs = |th: f64, hh1: f64, u: f64, sep: bool| -> (f64, f64) {
                    if sep {
                        return (-(h_sep + 2.0) * th / u * due, 0.0);
                    }
                    let h = head_h(hh1);
                    let cf = cf_ludwieg_tillmann(h, u * th / nu);
                    let dth = 0.5 * cf - (h + 2.0) * th / u * due;
                    let f = 0.0306 * (hh1 - 3.0).max(0.05).powf(-0.6169);
                    let dh1 = (u * f - hh1 * (due * th + u * dth)) / (u * th);
                    (dth, dh1)
                };
                let f0 = j as f64 / SUB as f64;
                let (a1, b1) = rhs(theta, h1, u_at(f0), separated);
                let th_mid = (theta + 0.5 * hstep * a1).max(1e-10);
                let h1_mid = (h1 + 0.5 * hstep * b1).max(3.31);
                let (a2, b2) = rhs(th_mid, h1_mid, u_at(f0 + 0.5 / SUB as f64), separated);
                theta = (theta + hstep * a2).max(1e-10);
                h1 = (h1 + hstep * b2).max(3.31);
                if !separated && head_h(h1) >= h_sep {
                    separated = true;
                }
            }
            if !theta.is_finite() || !h1.is_finite() {
                // Sayısal bozulma: son geçerli değerle dondur.
                theta = out.stations[k - 1].theta;
                h1 = head_h1(h_sep);
                separated = true;
            }
            if separated && out.separation_index.is_none() {
                out.separation_index = Some(k);
            }
            let h = if separated { h_sep } else { head_h(h1) };
            let cf = if separated {
                0.0
            } else {
                cf_ludwieg_tillmann(h, ue[k] * theta / nu)
            };
            out.stations.push(BlStation {
                s: s[k],
                ue: ue[k],
                theta,
                h,
                cf,
                turbulent: true,
            });
        }
    }

    // --- Squire–Young: CD = 2 θ_TE · Ue_TE^((H_TE + 5)/2)
    let te = out.stations[out.stations.len() - 1];
    let h_te = te.h.clamp(1.0, h_sep.max(2.6));
    out.cd = 2.0 * te.theta * te.ue.max(0.0).powf(0.5 * (h_te + 5.0));
    if !out.cd.is_finite() {
        out.cd = 0.0;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plate(n: usize) -> (Vec<f64>, Vec<f64>) {
        let s: Vec<f64> = (0..=n).map(|i| i as f64 / n as f64).collect();
        (s, vec![1.0; n + 1])
    }

    /// Düz levha, laminer: Blasius çözümü θ = 0.664 x / √Re_x, Cf = 0.664 / √Re_x
    /// (Schlichting, "Boundary-Layer Theory", Böl. 7). Thwaites sabitleri 0.671 ve 0.656 verir (%1).
    #[test]
    fn flat_plate_laminar_matches_blasius() {
        let re = 1.0e5; // Michel ölçütü bu Re'de geçiş vermez
        let (s, ue) = plate(200);
        let r = march(&s, &ue, re, &BlOptions::default()).unwrap();
        assert!(r.transition_index.is_none());
        for &k in &[50usize, 100, 200] {
            let st = r.stations[k];
            let re_x = re * st.s;
            let theta_blasius = 0.664 * st.s / re_x.sqrt();
            let cf_blasius = 0.664 / re_x.sqrt();
            assert!(
                (st.theta / theta_blasius - 1.0).abs() < 0.02,
                "theta {} vs {}",
                st.theta,
                theta_blasius
            );
            assert!(
                (st.cf / cf_blasius - 1.0).abs() < 0.02,
                "cf {} vs {}",
                st.cf,
                cf_blasius
            );
            // Bağımsız referans: Blasius H = 2.59 (Thwaites eğri uydurması λ=0'da 2.61 verir).
            assert!((st.h - 2.59).abs() < 0.04, "H = {}", st.h);
        }
    }

    /// Düz levha, tam türbülanslı: ortalama sürtünme Cf = 0.455 / (log10 Re)^2.58
    /// (Prandtl–Schlichting; projede `calc::aero::cf_turb`). Head + Ludwieg–Tillmann aynı
    /// mertebede olmalı (±%15).
    #[test]
    fn flat_plate_turbulent_order_of_magnitude() {
        for re in [1.0e6, 1.0e7] {
            let (s, ue) = plate(400);
            let opts = BlOptions {
                forced_transition_s: Some(0.0),
                ..Default::default()
            };
            let r = march(&s, &ue, re, &opts).unwrap();
            assert_eq!(r.transition_index, Some(1));
            assert!(r.separation_index.is_none());
            let cf_avg = 2.0 * r.stations[400].theta; // tek yüz, L = 1
            let cf_ref = 0.455 / re.log10().powf(2.58);
            assert!((cf_avg / cf_ref - 1.0).abs() < 0.15, "{cf_avg} vs {cf_ref}");
            let h = r.stations[400].h;
            assert!((1.2..1.6).contains(&h), "H = {h}");
        }
    }

    /// Düz levhada Ue = V∞ olduğundan Squire–Young CD = 2 θ_TE; momentum integralinden
    /// θ_TE / L = Cf_ort / 2, yani iki yüz için CD = 2 · Cf_ort.
    #[test]
    fn flat_plate_squire_young_equals_twice_cf_avg() {
        let re = 1.0e5;
        let (s, ue) = plate(400);
        let r = march(&s, &ue, re, &BlOptions::default()).unwrap();
        // Cf_ort: yerel Cf'nin sayısal integrali (s = 0 tekilliği için analitik ilk aralık).
        let mut cf_int = 2.0 * r.stations[1].cf * r.stations[1].s;
        for w in r.stations[1..].windows(2) {
            cf_int += 0.5 * (w[0].cf + w[1].cf) * (w[1].s - w[0].s);
        }
        let cd_two_sides = 2.0 * r.cd;
        assert!(
            (cd_two_sides / (2.0 * cf_int) - 1.0).abs() < 0.03,
            "{cd_two_sides} vs {cf_int}"
        );
        // Blasius: CD (iki yüz) = 2 · 1.328 / √Re
        let blasius = 2.0 * 1.328 / re.sqrt();
        assert!((cd_two_sides / blasius - 1.0).abs() < 0.02);
    }

    /// Michel ölçütü: düz levhada geçiş Re_x birkaç milyon mertebesinde olmalı
    /// (ölçütün kendi tanımından: Re_θ = 0.671 √Re_x ile kesişim Re_x ≈ 2–3·10⁶).
    #[test]
    fn michel_transition_on_flat_plate() {
        let re = 1.0e7;
        let (s, ue) = plate(1000);
        let r = march(&s, &ue, re, &BlOptions::default()).unwrap();
        let k = r.transition_index.expect("geçiş bekleniyor");
        let re_x_tr = re * r.stations[k].s;
        assert!((1.5e6..4.0e6).contains(&re_x_tr), "Re_x,tr = {re_x_tr:e}");
        assert!(!r.laminar_separation);
    }

    /// Doğrusal yavaşlayan akış (Howarth): Ue = 1 − s. Kesin çözümde laminer ayrılma
    /// s ≈ 0.120'dedir (Howarth 1938); Thwaites yöntemi s ≈ 0.123 verir (White, "Viscous Fluid Flow").
    #[test]
    fn howarth_laminar_separation_location() {
        let n = 2000;
        let s: Vec<f64> = (0..=n).map(|i| 0.5 * i as f64 / n as f64).collect();
        let ue: Vec<f64> = s.iter().map(|x| 1.0 - x).collect();
        let r = march(&s, &ue, 1.0e4, &BlOptions::default()).unwrap();
        assert!(r.laminar_separation);
        let s_sep = r.stations[r.transition_index.unwrap()].s;
        assert!((s_sep - 0.123).abs() < 0.006, "s_sep = {s_sep}");
    }

    #[test]
    fn head_closure_is_invertible() {
        for h in [1.2, 1.4, 1.6, 1.9, 2.4] {
            assert!((head_h(head_h1(h)) - h).abs() < 1e-9);
        }
    }

    #[test]
    fn invalid_input_is_rejected_without_panic() {
        let (s, ue) = plate(10);
        assert!(march(&s, &ue, f64::NAN, &BlOptions::default()).is_none());
        assert!(march(&s[..2], &ue[..2], 1e5, &BlOptions::default()).is_none());
        let mut bad = ue.clone();
        bad[3] = f64::INFINITY;
        assert!(march(&s, &bad, 1e5, &BlOptions::default()).is_none());
        let nan_opts = BlOptions {
            h_separation: f64::NAN,
            h_transition: f64::NAN,
            forced_transition_s: Some(f64::NAN),
        };
        let r = march(&s, &ue, 1e5, &nan_opts).unwrap();
        assert!(r.cd.is_finite() && r.stations.iter().all(|x| x.theta.is_finite()));
    }
}
