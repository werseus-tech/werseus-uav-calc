// SPDX-License-Identifier: AGPL-3.0-or-later
//! Ortam: irtifa, sıcaklık, rüzgâr, hamle payı ve soğukta batarya kapasite düşüşü.
use crate::calc::atmosphere::{G, LAPSE, R_AIR, RHO0, T0, isa};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TempMode {
    /// ISA sıcaklığına göre fark (K).
    IsaOffset,
    /// Görev bölgesinde mutlak sıcaklık (°C); irtifadan bağımsız sabit sapma uygulanır.
    Absolute,
}

#[derive(Clone, Debug)]
pub struct Env {
    /// Kalkış noktası deniz seviyesinden irtifa [m].
    pub alt_msl_m: f64,
    pub temp_mode: TempMode,
    pub isa_offset_k: f64,
    pub temp_abs_c: f64,
    /// Rüzgâr hızı [m/s] ve rüzgârın GELDİĞİ yön [derece, kuzeyden].
    pub wind_speed: f64,
    pub wind_from_deg: f64,
    /// Hamle payı [m/s]: kafa rüzgârına eklenir, arka rüzgârdan düşülür (kötü durum).
    pub gust: f64,
    /// Batarya sıcaklığı ortamdan farklı mı (ısıtılmış pil vb.).
    pub batt_override: bool,
    pub batt_temp_c: f64,
}

impl Default for Env {
    fn default() -> Self {
        Env {
            alt_msl_m: 0.0,
            temp_mode: TempMode::IsaOffset,
            isa_offset_k: 0.0,
            temp_abs_c: 15.0,
            wind_speed: 0.0,
            wind_from_deg: 0.0,
            gust: 0.0,
            batt_override: false,
            batt_temp_c: 20.0,
        }
    }
}

impl Env {
    /// ISA'ya göre sabit sıcaklık sapması [K].
    fn dev_k(&self) -> f64 {
        match self.temp_mode {
            TempMode::IsaOffset => self.isa_offset_k,
            TempMode::Absolute => self.temp_abs_c + 273.15 - isa(self.alt_msl_m).t,
        }
    }

    /// `h_msl` irtifasındaki gerçek sıcaklık [K].
    pub fn temp_k_at(&self, h_msl: f64) -> f64 {
        isa(h_msl).t + self.dev_k()
    }

    /// `h_msl` irtifasındaki hava yoğunluğu [kg/m³] (ISA basıncı, sapmalı sıcaklık).
    pub fn rho_at(&self, h_msl: f64) -> f64 {
        let t = self.temp_k_at(h_msl);
        let t = if t.is_finite() { t.max(100.0) } else { T0 };
        isa(h_msl).p / (R_AIR * t)
    }

    /// Kalkış noktası yoğunluk irtifası [m].
    pub fn density_alt_m(&self) -> f64 {
        density_altitude_m(self.rho_at(self.alt_msl_m))
    }

    /// Batarya sıcaklığı [°C].
    pub fn battery_temp_c(&self) -> f64 {
        if self.batt_override {
            self.batt_temp_c
        } else {
            self.temp_k_at(self.alt_msl_m) - 273.15
        }
    }

    /// Verilen rota yönü için (kafa, yan) rüzgâr bileşenleri [m/s]; kafa rüzgârı pozitif.
    /// Hamle payı kafa rüzgârını artırır, arka rüzgârı azaltır; yan rüzgârı küçültmez.
    pub fn wind_components(&self, track_deg: f64) -> (f64, f64) {
        let track = if track_deg.is_finite() {
            track_deg
        } else {
            0.0
        };
        let from = if self.wind_from_deg.is_finite() {
            self.wind_from_deg
        } else {
            0.0
        };
        let ws = if self.wind_speed.is_finite() {
            self.wind_speed.max(0.0)
        } else {
            0.0
        };
        let gust = if self.gust.is_finite() {
            self.gust.max(0.0)
        } else {
            0.0
        };
        let a = (track - from).to_radians();
        let cos_a = a.cos();
        let head = if cos_a >= 0.0 {
            (ws + gust) * cos_a
        } else {
            (ws - gust).max(0.0) * cos_a
        };
        let cross = (ws + gust) * a.sin();
        (head, cross)
    }
}

/// Yoğunluk irtifası: ISA yoğunluğu bu değere eşit olan irtifa [m].
pub fn density_altitude_m(rho: f64) -> f64 {
    let n = G / (R_AIR * LAPSE) - 1.0;
    let ratio = (rho / RHO0).clamp(1e-3, 10.0);
    T0 / LAPSE * (1.0 - ratio.powf(1.0 / n))
}

#[derive(Clone, Copy, Debug)]
pub struct BatteryDerate {
    /// Kullanılabilir kapasite oranı (1 = 25 °C).
    pub capacity: f64,
    /// İç direnç çarpanı (1 = 25 °C).
    pub resistance: f64,
    /// Kapasite ve iç direnç kaybını birleştiren, kullanılabilir enerji oranı.
    pub energy_factor: f64,
}

/// (sıcaklık °C, kapasite oranı, iç direnç çarpanı): yaklaşık, tipik Li-ion/LiPo.
const COLD_TABLE: [(f64, f64, f64); 6] = [
    (25.0, 1.00, 1.0),
    (10.0, 0.95, 1.2),
    (0.0, 0.85, 1.6),
    (-10.0, 0.70, 2.3),
    (-20.0, 0.50, 3.5),
    (-30.0, 0.35, 5.0),
];

/// Pil iç direncindeki taban enerji kaybı payı (25 °C'de model verimine zaten dahildir).
const IR_LOSS_BASE: f64 = 0.04;

/// Soğukta kapasite düşüşü ve iç direnç artışı. Yaklaşık, tipik Li-ion/LiPo eğrisi; gerçek
/// hücre için üretici verisi kullanılmalıdır. NaN girişte 25 °C varsayılır.
pub fn battery_derate(temp_c: f64) -> BatteryDerate {
    let t = if temp_c.is_finite() { temp_c } else { 25.0 };
    let t = t.clamp(-30.0, 25.0);
    let mut cap = 1.0;
    let mut res = 1.0;
    for w in COLD_TABLE.windows(2) {
        let (t0, c0, r0) = w[0];
        let (t1, c1, r1) = w[1];
        if t <= t0 && t >= t1 {
            let f = (t0 - t) / (t0 - t1);
            cap = c0 + f * (c1 - c0);
            res = r0 + f * (r1 - r0);
            break;
        }
    }
    let ir = ((1.0 - IR_LOSS_BASE * res) / (1.0 - IR_LOSS_BASE)).clamp(0.2, 1.0);
    BatteryDerate {
        capacity: cap,
        resistance: res,
        energy_factor: (cap * ir).clamp(0.05, 1.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isa_sea_level_is_neutral() {
        let e = Env::default();
        assert!((e.rho_at(0.0) - RHO0).abs() < 1e-3);
        assert!(e.density_alt_m().abs() < 5.0);
    }

    #[test]
    fn hot_high_density_altitude() {
        let e = Env {
            alt_msl_m: 1500.0,
            isa_offset_k: 20.0,
            ..Env::default()
        };
        assert!(e.density_alt_m() > 1500.0 + 600.0);
        let abs = Env {
            alt_msl_m: 0.0,
            temp_mode: TempMode::Absolute,
            temp_abs_c: 35.0,
            ..Env::default()
        };
        assert!((abs.temp_k_at(0.0) - 308.15).abs() < 1e-9);
        assert!(abs.density_alt_m() > 600.0);
    }

    #[test]
    fn da_round_trip() {
        let rho = isa(2000.0).rho;
        assert!((density_altitude_m(rho) - 2000.0).abs() < 1.0);
    }

    #[test]
    fn wind_components_geometry() {
        let e = Env {
            wind_speed: 10.0,
            wind_from_deg: 90.0,
            ..Env::default()
        };
        let (h, c) = e.wind_components(90.0);
        assert!((h - 10.0).abs() < 1e-9 && c.abs() < 1e-9);
        let (h, _) = e.wind_components(270.0);
        assert!((h + 10.0).abs() < 1e-9);
        let (h, c) = e.wind_components(0.0);
        assert!(h.abs() < 1e-9 && (c + 10.0).abs() < 1e-9);
    }

    #[test]
    fn gust_hurts_both_directions() {
        let steady = Env {
            wind_speed: 6.0,
            wind_from_deg: 0.0,
            gust: 0.0,
            ..Env::default()
        };
        let e = Env {
            gust: 2.0,
            ..steady.clone()
        };
        assert!((e.wind_components(0.0).0 - 8.0).abs() < 1e-9);
        assert!((e.wind_components(180.0).0 + 4.0).abs() < 1e-9);
        // Çapraz arka rüzgâr (135°): hamle yan rüzgârı küçültmemeli ve kafa bileşeni artmalı (daha az arka rüzgâr).
        let (h0, c0) = steady.wind_components(135.0);
        let (hg, cg) = e.wind_components(135.0);
        assert!(hg > h0 && cg.abs() > c0.abs());
        // ±90° civarında süreklilik:
        let (h_lo, c_lo) = e.wind_components(89.99);
        let (h_hi, c_hi) = e.wind_components(90.01);
        assert!((h_lo - h_hi).abs() < 1e-2 && (c_lo - c_hi).abs() < 1e-3);
    }

    #[test]
    fn rho_at_clamps_extreme_cold_temperature() {
        let e = Env {
            temp_mode: TempMode::Absolute,
            temp_abs_c: -273.15,
            ..Env::default()
        };
        let rho = e.rho_at(0.0);
        assert!(rho.is_finite() && rho > 0.0);
    }

    #[test]
    fn cold_derating_is_monotonic() {
        assert!((battery_derate(25.0).energy_factor - 1.0).abs() < 1e-12);
        assert!((battery_derate(40.0).energy_factor - 1.0).abs() < 1e-12);
        let mut prev = 1.0;
        for t in [10.0, 0.0, -10.0, -20.0, -30.0] {
            let d = battery_derate(t);
            assert!(d.energy_factor < prev && d.energy_factor > 0.0);
            assert!(d.energy_factor <= d.capacity);
            prev = d.energy_factor;
        }
        assert!(battery_derate(f64::NAN).energy_factor.is_finite());
    }
}
