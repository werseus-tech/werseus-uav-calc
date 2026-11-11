// SPDX-License-Identifier: AGPL-3.0-or-later
//! Adım 5 (Tasarım Noktası) basit modu: "Ne kadar yavaş uçabilsin?", "Rüzgârlı havada da uçsun mu?",
//! "Tırmanma ne kadar hızlı olsun?" → kısıt diyagramı girdileri (tutunma kaybı hızı, tırmanma
//! hızı, güç payı, fırlatma hızı). Doğrulanmış kısıt formülleri (`calc::constraint`) değişmez.
use super::mission::Launch;
use super::{Derived, Field, Rating, choice};
use crate::calc::aero_presets::suggest_launch_speed;
use serde::{Deserialize, Serialize};

choice!(
    /// "Ne kadar yavaş uçabilsin?" (tutunma kaybı hızı hedefi)
    SpeedClass, "simple.dp.speed", Normal, [
        Slow => "slow",
        Normal => "normal",
        Fast => "fast",
    ]
);

impl SpeedClass {
    /// Hedef tutunma kaybı hızı [m/s]: 8 (≈29 km/h, kolay iniş), 10 (≈36 km/h, referans), 13 (≈47 km/h).
    pub fn v_stall(self) -> f64 {
        match self {
            SpeedClass::Slow => 8.0,
            SpeedClass::Normal => 10.0,
            SpeedClass::Fast => 13.0,
        }
    }
}

choice!(
    /// "Rüzgârlı havada da uçsun mu?"
    Wind, "simple.dp.wind", Calm, [
        Calm => "calm",
        Breezy => "breezy",
        Windy => "windy",
    ]
);

impl Wind {
    /// Tipik rüzgâr hızı [m/s]: sakin ~2, hafif rüzgâr ~6 (Beaufort 4), güçlü ~9 (Beaufort 5).
    pub fn speed_ms(self) -> f64 {
        match self {
            Wind::Calm => 2.0,
            Wind::Breezy => 6.0,
            Wind::Windy => 9.0,
        }
    }

    /// Ek güç payı (tırmanma/rüzgâra karşı): 1.15 (referans) / 1.25 / 1.40.
    pub fn power_margin(self) -> f64 {
        match self {
            Wind::Calm => 1.15,
            Wind::Breezy => 1.25,
            Wind::Windy => 1.40,
        }
    }
}

choice!(
    /// "Tırmanma ne kadar hızlı olsun?"
    Climb, "simple.dp.climb", Normal, [
        Gentle => "gentle",
        Normal => "normal",
        Strong => "strong",
    ]
);

impl Climb {
    /// Hedef tırmanma hızı [m/s]: 2 (yumuşak), 3.5 (referans), 5 (güçlü).
    pub fn rate(self) -> f64 {
        match self {
            Climb::Gentle => 2.0,
            Climb::Normal => 3.5,
            Climb::Strong => 5.0,
        }
    }
}

/// Tasarım noktası cevapları.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DpAnswers {
    pub speed: SpeedClass,
    pub wind: Wind,
    pub climb: Climb,
}

/// İnsan elinin pratik fırlatma hızı sınırı [m/s] (yaklaşık 8-12 m/s; üstü zor).
pub const HAND_THROW_MAX: f64 = 12.0;

/// Cevaplardan kısıt girdilerini üretir. Dönüş: (değerler, notlar).
/// - V_y ≈ 1.25·V_stall, V_dönüş ≈ 1.6·V_stall (referans tasarımla aynı oranlar: 12.5/10, 16/10).
/// - Elle fırlatmada V_fırlatma = 1.1·V_stall (`suggest_launch_speed`) ≤ 12 m/s kalsın diye
///   V_stall en çok 10.9 m/s'ye sınırlanır ve not düşülür; pistten kalkışta liftoff 1.2·V_stall.
pub fn derive(a: &DpAnswers, launch: Launch) -> (Derived, Vec<&'static str>) {
    let mut notes = Vec::new();
    let mut vs = a.speed.v_stall();
    if launch == Launch::Hand && suggest_launch_speed(vs) > HAND_THROW_MAX {
        vs = HAND_THROW_MAX / 1.1;
        notes.push("simple.dp.note.hand_capped");
    }
    let v_launch = if launch == Launch::Runway {
        1.2 * vs
    } else {
        suggest_launch_speed(vs)
    };
    let d = vec![
        (Field::VStall, vs),
        (Field::ClimbRate, a.climb.rate()),
        (Field::VBestClimb, 1.25 * vs),
        (Field::VTurn, 1.6 * vs),
        (Field::PwMargin, a.wind.power_margin()),
        (Field::VLaunch, v_launch),
    ];
    (d, notes)
}

/// "Seyir hızım rüzgâra karşı yeterli mi?": seyir hızı / rüzgâr ≥2 iyi, ≥1.4 orta, altı kötü
/// (rüzgâra karşı ilerleyebilmek için yer hızı pozitif ve manevra payı kalmalı).
pub fn wind_headway(v_cruise: f64, wind: Wind) -> Rating {
    let r = v_cruise / wind.speed_ms();
    if !r.is_finite() {
        Rating::Bad
    } else if r >= 2.0 {
        Rating::Good
    } else if r >= 1.4 {
        Rating::Ok
    } else {
        Rating::Bad
    }
}

/// Elle fırlatma uygunluğu: fırlatma hızı insan sınırının altında mı, tutunma kaybı hızının üstünde mi.
pub fn hand_launch_speed_rating(v_launch: f64, v_stall: f64) -> Rating {
    if !(v_launch.is_finite() && v_stall.is_finite())
        || v_launch < v_stall
        || v_launch > HAND_THROW_MAX
    {
        Rating::Bad
    } else if v_launch > 0.9 * HAND_THROW_MAX {
        Rating::Ok
    } else {
        Rating::Good
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_combinations_valid_and_in_ui_ranges() {
        for &speed in SpeedClass::ALL {
            for &wind in Wind::ALL {
                for &climb in Climb::ALL {
                    for &launch in Launch::ALL {
                        let a = DpAnswers { speed, wind, climb };
                        let (d, _) = derive(&a, launch);
                        let g = |f| d.iter().find(|(x, _)| *x == f).unwrap().1;
                        let vs = g(Field::VStall);
                        // Sihirbaz: v_stall>0, rc>=0, pw_margin>=1 ; arayüz aralığı 5-20 m/s
                        assert!((5.0..=20.0).contains(&vs));
                        assert!(g(Field::ClimbRate) >= 0.0);
                        assert!(g(Field::PwMargin) >= 1.0);
                        assert!(g(Field::VLaunch) >= vs);
                        assert!(g(Field::VBestClimb) > vs && g(Field::VTurn) > vs);
                    }
                }
            }
        }
    }

    #[test]
    fn normal_reproduces_reference_point() {
        let (d, notes) = derive(&DpAnswers::default(), Launch::Hand);
        let g = |f| d.iter().find(|(x, _)| *x == f).unwrap().1;
        assert!(notes.is_empty());
        assert!((g(Field::VStall) - 10.0).abs() < 1e-12);
        assert!((g(Field::VBestClimb) - 12.5).abs() < 1e-12);
        assert!((g(Field::VTurn) - 16.0).abs() < 1e-12);
        assert!((g(Field::VLaunch) - 11.0).abs() < 1e-12);
        assert!((g(Field::ClimbRate) - 3.5).abs() < 1e-12);
    }

    #[test]
    fn hand_launch_caps_fast_stall_speed() {
        let a = DpAnswers {
            speed: SpeedClass::Fast,
            ..Default::default()
        };
        let (d, notes) = derive(&a, Launch::Hand);
        let vs = d.iter().find(|(f, _)| *f == Field::VStall).unwrap().1;
        assert!(vs < 13.0 && suggest_launch_speed(vs) <= HAND_THROW_MAX + 1e-9);
        assert_eq!(notes, vec!["simple.dp.note.hand_capped"]);
        // Pistte sınırlama yok
        let (d2, n2) = derive(&a, Launch::Runway);
        assert!(n2.is_empty());
        assert_eq!(
            d2.iter().find(|(f, _)| *f == Field::VStall).unwrap().1,
            13.0
        );
    }

    #[test]
    fn wind_and_launch_ratings() {
        assert_eq!(wind_headway(16.0, Wind::Calm), Rating::Good);
        assert_eq!(wind_headway(16.0, Wind::Windy), Rating::Ok);
        assert_eq!(wind_headway(10.0, Wind::Windy), Rating::Bad);
        assert_eq!(hand_launch_speed_rating(11.0, 10.0), Rating::Ok);
        assert_eq!(hand_launch_speed_rating(9.0, 8.0), Rating::Good);
        assert_eq!(hand_launch_speed_rating(15.0, 10.0), Rating::Bad);
    }
}
