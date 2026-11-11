// SPDX-License-Identifier: AGPL-3.0-or-later
//! Adım 6 (Menzil / Havada kalma) basit modu: yalnızca SONUÇ + günlük dil + iyileştirme önerileri.
//! Girdi türetmez; modelin süre/menzil çıktılarını anlaşılır ifadelere çevirir.
use super::Rating;
use crate::calc::aero_presets::rate_higher;

/// Futbol sahası çevresi [m]: 105 × 68 m saha → 2·(105+68) = 346 m (FIFA standart ölçüsü).
pub const FOOTBALL_LAP_M: f64 = 346.0;

/// Modelden gelen ham sonuçlar (UI'dan bağımsız).
#[derive(Clone, Copy, Debug)]
pub struct Facts {
    /// Hedef süreyle karşılaştırılacak süre [dk] (sabit kanat: dayanıklılık, multirotor: askı).
    pub endurance_min: f64,
    /// Seyir menzili [km].
    pub range_km: f64,
    /// Hedef süre [dk] (görev adımından).
    pub target_min: f64,
    /// Seyir hızı [m/s].
    pub v_cruise: f64,
    /// En uzun süre hızı [m/s] (sabit kanat; multirotorda NaN).
    pub v_endurance: f64,
    /// Seyirdeki L/D (sabit kanat; multirotorda NaN).
    pub ld: f64,
    /// Batarya kütle payı (0..1).
    pub battery_fraction: f64,
    /// Multirotor mu (öneri metinleri farklı).
    pub multirotor: bool,
}

/// Süre benzetmesi (i18n anahtarı).
pub fn time_analogy(min: f64) -> &'static str {
    if !min.is_finite() || min < 15.0 {
        "simple.end.analogy.break"
    } else if min < 30.0 {
        "simple.end.analogy.sitcom"
    } else if min < 60.0 {
        "simple.end.analogy.episode"
    } else {
        "simple.end.analogy.movie"
    }
}

/// Menzilin futbol sahası çevresinde kaç tura denk geldiği.
pub fn football_laps(range_km: f64) -> f64 {
    if range_km.is_finite() && range_km > 0.0 {
        range_km * 1000.0 / FOOTBALL_LAP_M
    } else {
        0.0
    }
}

/// Hedefe göre derece: ≥%100 iyi, ≥%80 orta, altı kötü.
pub fn rating(endurance_min: f64, target_min: f64) -> Rating {
    if !(target_min.is_finite() && target_min > 0.0) {
        return rate_higher(endurance_min, 20.0, 10.0);
    }
    rate_higher(endurance_min, target_min, 0.8 * target_min)
}

/// İyileştirme önerisi: i18n anahtarı + biçim argümanları.
#[derive(Clone, Debug, PartialEq)]
pub struct Tip {
    pub key: &'static str,
    pub args: Vec<String>,
}

/// Hedefin altında kalındığında (veya zayıf noktalar varsa) öneriler üretir. Sayılar yaklaşık
/// ölçeklemedir (süre ≈ batarya kütlesiyle orantılı; ağırlık artışı bunu bir miktar azaltır).
pub fn tips(f: &Facts) -> Vec<Tip> {
    let mut v = Vec::new();
    let short =
        f.endurance_min.is_finite() && f.target_min.is_finite() && f.endurance_min < f.target_min;
    if short {
        let need = (f.target_min / f.endurance_min.max(1e-6) - 1.0) * 100.0;
        v.push(Tip {
            key: "simple.end.tip.battery",
            args: vec![format!("{:.0}", need.clamp(1.0, 300.0))],
        });
    }
    if !f.multirotor
        && f.v_endurance.is_finite()
        && f.v_cruise.is_finite()
        && f.v_cruise > 1.15 * f.v_endurance
    {
        v.push(Tip {
            key: "simple.end.tip.slower",
            args: vec![
                format!("{:.0}", f.v_endurance * 3.6),
                format!("{:.0}", f.v_cruise * 3.6),
            ],
        });
    }
    if !f.multirotor && f.ld.is_finite() && f.ld < 10.0 {
        v.push(Tip {
            key: "simple.end.tip.cleaner",
            args: vec![format!("{:.1}", f.ld)],
        });
    }
    if f.multirotor && short {
        v.push(Tip {
            key: "simple.end.tip.mr_props",
            args: vec![],
        });
    }
    if short && f.battery_fraction.is_finite() && f.battery_fraction < 0.25 {
        v.push(Tip {
            key: "simple.end.tip.lighter",
            args: vec![],
        });
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> Facts {
        Facts {
            endurance_min: 30.0,
            range_km: 25.0,
            target_min: 45.0,
            v_cruise: 18.0,
            v_endurance: 12.0,
            ld: 8.0,
            battery_fraction: 0.2,
            multirotor: false,
        }
    }

    #[test]
    fn laps_and_analogy() {
        assert!((football_laps(0.346) - 1.0).abs() < 1e-9);
        assert_eq!(football_laps(f64::NAN), 0.0);
        assert_eq!(time_analogy(5.0), "simple.end.analogy.break");
        assert_eq!(time_analogy(25.0), "simple.end.analogy.sitcom");
        assert_eq!(time_analogy(50.0), "simple.end.analogy.episode");
        assert_eq!(time_analogy(120.0), "simple.end.analogy.movie");
    }

    #[test]
    fn rating_against_target() {
        assert_eq!(rating(50.0, 45.0), Rating::Good);
        assert_eq!(rating(40.0, 45.0), Rating::Ok);
        assert_eq!(rating(10.0, 45.0), Rating::Bad);
    }

    #[test]
    fn short_flight_produces_actionable_tips() {
        let t = tips(&facts());
        let keys: Vec<_> = t.iter().map(|x| x.key).collect();
        assert!(keys.contains(&"simple.end.tip.battery"));
        assert!(keys.contains(&"simple.end.tip.slower"));
        assert!(keys.contains(&"simple.end.tip.cleaner"));
        assert!(keys.contains(&"simple.end.tip.lighter"));
        // 30 → 45 dk = %50 daha fazla batarya
        assert_eq!(t[0].args[0], "50");
    }

    #[test]
    fn good_result_has_no_tips() {
        let mut f = facts();
        f.endurance_min = 60.0;
        f.v_cruise = 12.0;
        f.ld = 14.0;
        assert!(tips(&f).is_empty());
    }

    #[test]
    fn multirotor_tips_skip_wing_advice() {
        let mut f = facts();
        f.multirotor = true;
        f.v_endurance = f64::NAN;
        f.ld = f64::NAN;
        let keys: Vec<_> = tips(&f).iter().map(|x| x.key).collect();
        assert!(keys.contains(&"simple.end.tip.mr_props"));
        assert!(!keys.contains(&"simple.end.tip.slower"));
    }
}
