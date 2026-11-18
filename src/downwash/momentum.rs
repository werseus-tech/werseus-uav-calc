// SPDX-License-Identifier: AGPL-3.0-or-later
//! Eyleyici disk (momentum teorisi) bağıntıları: indüklenmiş hız, iz hızı ve iz daralması.
//!
//! Kaynaklar:
//! - Leishman, J. G., *Principles of Helicopter Aerodynamics*, 2. baskı, Cambridge UP, 2006,
//!   Bölüm 2 (askıda momentum teorisi: v_i = √(T / 2ρA), uzak izde w = 2v_i, iz alanı A/2) ve
//!   Bölüm 2.14 (ileri/eksenel hareketli disk: T = 2ρA(V + v_i)v_i).
//! - McCormick, B. W., *Aerodynamics, Aeronautics, and Flight Mechanics*, 2. baskı, Wiley, 1995,
//!   Bölüm 6 (pervane eyleyici diski, çekici pervanenin hava akımı / slipstream hızı V + 2v_i).
//! - Disk ekseni boyunca hız dağılımı, düzgün yüklü eyleyici diskin kesin eksen çözümüdür:
//!   v(z) = v_i (1 + z / √(R² + z²)) (z: diskin akış yönünde aşağısı +). Diskte v_i, uzakta 2v_i.
//!
//! Tüm fonksiyonlar saf, SI birimli ve geçersiz girişte 0 (ya da sonlu bir değer) döndürür.

use std::f64::consts::PI;

/// Disk alanı A = πR².
pub fn disk_area(r: f64) -> f64 {
    if r.is_finite() && r > 0.0 {
        PI * r * r
    } else {
        0.0
    }
}

/// Askıda indüklenmiş hız v_i = √(T / (2ρA)) [m/s] (Leishman 2006, denklem 2.15).
pub fn induced_velocity_hover(thrust: f64, rho: f64, area: f64) -> f64 {
    if !(thrust.is_finite() && rho.is_finite() && area.is_finite()) {
        return 0.0;
    }
    if thrust <= 0.0 || rho <= 0.0 || area <= 0.0 {
        return 0.0;
    }
    (thrust / (2.0 * rho * area)).sqrt()
}

/// Eksenel V hızında ilerleyen diskin indüklenmiş hızı:
/// T = 2ρA(V + v_i)v_i ⇒ v_i = −V/2 + √(V²/4 + T/(2ρA)) (Leishman 2006, §2.14; McCormick 1995, §6).
/// V = 0'da askı değerine eşittir.
pub fn induced_velocity_axial(thrust: f64, v: f64, rho: f64, area: f64) -> f64 {
    let v = if v.is_finite() { v.max(0.0) } else { 0.0 };
    let vh = induced_velocity_hover(thrust, rho, area);
    if vh <= 0.0 {
        return 0.0;
    }
    -0.5 * v + (0.25 * v * v + vh * vh).sqrt()
}

/// Uzak iz (slipstream) hızı V_s = V + 2v_i (McCormick 1995, §6).
pub fn slipstream_velocity(v: f64, v_i: f64) -> f64 {
    let v = if v.is_finite() { v.max(0.0) } else { 0.0 };
    let vi = if v_i.is_finite() { v_i.max(0.0) } else { 0.0 };
    v + 2.0 * vi
}

/// Disk ekseninde, diskten z kadar aşağı akışta (z < 0: disk önünde) ek eksenel hız:
/// Δv(z) = v_i (1 + z / √(R² + z²)). z = 0'da v_i, z → +∞'da 2v_i, z → −∞'da 0.
pub fn added_velocity_at(v_i: f64, r: f64, z: f64) -> f64 {
    if !(v_i.is_finite() && r.is_finite() && z.is_finite()) || v_i <= 0.0 || r <= 0.0 {
        return 0.0;
    }
    v_i * (1.0 + z / (r * r + z * z).sqrt())
}

/// İz tüpünün z'deki yarıçapı (süreklilik: (V + v_i)·πR² = (V + Δv(z))·πr²):
/// r(z) = R √((V + v_i) / (V + Δv(z))). Askıda (V = 0) uzak izde R/√2.
pub fn slipstream_radius_at(v: f64, v_i: f64, r: f64, z: f64) -> f64 {
    if !(r.is_finite() && r > 0.0) {
        return 0.0;
    }
    let v = if v.is_finite() { v.max(0.0) } else { 0.0 };
    let vi = if v_i.is_finite() { v_i.max(0.0) } else { 0.0 };
    let dv = added_velocity_at(vi, r, z);
    let den = v + dv;
    if den <= 1e-12 {
        return r;
    }
    r * ((v + vi) / den).sqrt()
}

/// İdeal (indüklenmiş) güç P_i = T (V + v_i) [W].
pub fn ideal_power(thrust: f64, v: f64, rho: f64, area: f64) -> f64 {
    if !(thrust.is_finite() && thrust > 0.0) {
        return 0.0;
    }
    let v = if v.is_finite() { v.max(0.0) } else { 0.0 };
    thrust * (v + induced_velocity_axial(thrust, v, rho, area))
}

/// Sabit ideal güçte (tam gaz varsayımı) V hızındaki itki: P_i = T (V + v_i(T)) denklemi ikiye
/// bölmeyle çözülür (T'de monoton artan). `t_max`: arama üst sınırı (ör. statik itki).
pub fn thrust_at_ideal_power(p_ideal: f64, v: f64, rho: f64, area: f64, t_max: f64) -> f64 {
    if !(p_ideal.is_finite() && p_ideal > 0.0 && t_max.is_finite() && t_max > 0.0) {
        return 0.0;
    }
    if !(rho.is_finite() && rho > 0.0 && area.is_finite() && area > 0.0) {
        return 0.0;
    }
    let (mut lo, mut hi) = (0.0_f64, t_max * 1.05);
    if ideal_power(hi, v, rho, area) <= p_ideal {
        return hi;
    }
    for _ in 0..80 {
        let mid = 0.5 * (lo + hi);
        if ideal_power(mid, v, rho, area) < p_ideal {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// İki eşit yarıçaplı dairenin kesişim (mercek) alanı; d: merkezler arası uzaklık.
pub fn lens_area(r: f64, d: f64) -> f64 {
    if !(r.is_finite() && d.is_finite()) || r <= 0.0 {
        return 0.0;
    }
    if d >= 2.0 * r {
        return 0.0;
    }
    if d <= 0.0 {
        return PI * r * r;
    }
    2.0 * r * r * (d / (2.0 * r)).acos() - 0.5 * d * (4.0 * r * r - d * d).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Elle hesap: T = 10 N, ρ = 1.225, R = 0.127 m ⇒ A = 0.050671 m²,
    /// v_i = √(10 / (2·1.225·0.050671)) = √80.553 = 8.975 m/s.
    #[test]
    fn hover_induced_velocity_matches_hand_calculation() {
        let a = disk_area(0.127);
        assert!((a - 0.050_671).abs() < 1e-5, "{a}");
        let vi = induced_velocity_hover(10.0, 1.225, a);
        assert!((vi - 8.975).abs() < 0.005, "{vi}");
        // Askı ideal gücü T·v_i = 89.75 W = T^1.5/√(2ρA).
        let p = ideal_power(10.0, 0.0, 1.225, a);
        let p2 = 10.0_f64.powf(1.5) / (2.0 * 1.225 * a).sqrt();
        assert!((p - p2).abs() < 1e-9);
        assert_eq!(induced_velocity_hover(-1.0, 1.225, a), 0.0);
        assert_eq!(induced_velocity_hover(f64::NAN, 1.225, a), 0.0);
    }

    /// Eksenel disk: v_i(V) T = 2ρA(V + v_i)v_i'yi sağlar; V = 0'da askı değeri; slipstream V + 2v_i.
    #[test]
    fn slipstream_velocity_formula() {
        let (t, rho, a, v) = (6.0, 1.2, disk_area(0.127), 18.0);
        let vi = induced_velocity_axial(t, v, rho, a);
        let back = 2.0 * rho * a * (v + vi) * vi;
        assert!((back - t).abs() < 1e-9, "{back}");
        assert!(
            (induced_velocity_axial(t, 0.0, rho, a) - induced_velocity_hover(t, rho, a)).abs()
                < 1e-12
        );
        // Elle: v_h² = 6/(2·1.2·0.050671) = 49.338; v_i = −9 + √(81 + 49.338) = 2.4167.
        assert!((vi - 2.4167).abs() < 1e-3, "{vi}");
        assert!((slipstream_velocity(v, vi) - (18.0 + 2.0 * 2.4167)).abs() < 1e-3);
    }

    #[test]
    fn wake_contracts_to_r_over_sqrt2_and_velocity_doubles() {
        let (vi, r) = (8.0, 0.1);
        assert!((added_velocity_at(vi, r, 0.0) - vi).abs() < 1e-12);
        assert!((added_velocity_at(vi, r, 1e4) - 2.0 * vi).abs() < 1e-6);
        assert!(added_velocity_at(vi, r, -1e4) < 1e-6);
        let r_far = slipstream_radius_at(0.0, vi, r, 1e4);
        assert!((r_far - r / 2f64.sqrt()).abs() < 1e-6, "{r_far}");
        assert!((slipstream_radius_at(0.0, vi, r, 0.0) - r).abs() < 1e-12);
        // Disk önünde akım tüpü daha geniştir.
        assert!(slipstream_radius_at(15.0, 3.0, r, -0.2) > r);
    }

    #[test]
    fn thrust_at_power_recovers_static_and_drops_with_speed() {
        let (rho, a, t0) = (1.225, disk_area(0.127), 20.0);
        let p = ideal_power(t0, 0.0, rho, a);
        let t_static = thrust_at_ideal_power(p, 0.0, rho, a, t0);
        assert!((t_static - t0).abs() < 1e-6, "{t_static}");
        let t_10 = thrust_at_ideal_power(p, 10.0, rho, a, t0);
        assert!(t_10 < t0 && t_10 > 0.0);
        assert_eq!(thrust_at_ideal_power(f64::NAN, 10.0, rho, a, t0), 0.0);
    }

    #[test]
    fn lens_area_limits() {
        let r = 0.1;
        assert_eq!(lens_area(r, 0.25), 0.0);
        assert!((lens_area(r, 0.0) - PI * r * r).abs() < 1e-12);
        // d = R: alan = (2π/3 − √3/2) R².
        let exp = (2.0 * PI / 3.0 - 3f64.sqrt() / 2.0) * r * r;
        assert!((lens_area(r, r) - exp).abs() < 1e-12);
    }
}
