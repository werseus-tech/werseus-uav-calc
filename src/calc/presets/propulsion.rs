// SPDX-License-Identifier: AGPL-3.0-or-later
//! Adım 7 (İtki / Motor & Pervane) basit modu: "Hızlı mı verimli mi?", "Elle fırlatma var mı?"
//! (görev adımındaki cevap) → T/W hedefi, motor güç payı, pervane çapı/hatvesi ve kurulu güç;
//! sonuçta "önerilen motor sınıfı" ve "pervane çapı aralığı" (marka adı yok).
//! Multirotor için: "uçuş tarzı" → motor başına azami itki. Formüller (`calc::propulsion`) değişmez.
use super::mission::Launch;
use super::{Ctx, Derived, Field, Rating, choice, round_to_inch};
use crate::project::DroneType;
use serde::{Deserialize, Serialize};

choice!(
    /// "Hızlı mı verimli mi?" (sabit kanat / VTOL seyir motoru)
    Priority, "simple.prop.priority", Balanced, [
        Efficient => "efficient",
        Balanced => "balanced",
        Fast => "fast",
    ]
);

choice!(
    /// "Nasıl uçacak?" (multirotor)
    MrStyle, "simple.prop.mr_style", Relaxed, [
        Relaxed => "relaxed",
        Sporty => "sporty",
        Racing => "racing",
    ]
);

impl MrStyle {
    /// Hedef itki/ağırlık: sakin fotoğraf/gözetleme platformu ≥2 (askı gazı ~%70'in altında,
    /// pil düşse de kontrol payı), tempolu 3, yarış/akrobasi 5 (FPV pratik aralığı 4-8).
    pub fn thrust_to_weight(self) -> f64 {
        match self {
            MrStyle::Relaxed => 2.0,
            MrStyle::Sporty => 3.0,
            MrStyle::Racing => 5.0,
        }
    }
}

/// İtki adımı cevapları (elle fırlatma görev adımındaki "nereden kalkacak" cevabıdır).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PropAnswers {
    pub priority: Priority,
    pub mr_style: MrStyle,
}

/// Statik itki/ağırlık hedefi. Verimli 0.45, dengeli 0.60 (referans tasarım), hızlı 0.90.
/// Elle fırlatmada en az 0.80: fırlatma sonrası uçak hemen kendi itkisiyle ivmelenmeli
/// (hobi kuralı: elle fırlatma için T/W ≳ 0.7-0.8).
pub fn tw_target(p: Priority, launch: Launch) -> f64 {
    let base: f64 = match p {
        Priority::Efficient => 0.45,
        Priority::Balanced => 0.60,
        Priority::Fast => 0.90,
    };
    if launch == Launch::Hand {
        base.max(0.80)
    } else {
        base
    }
}

/// Motor güç payı: verimli 1.10, dengeli 1.20, hızlı 1.35 (ısınma/ömür payı).
pub fn motor_margin(p: Priority) -> f64 {
    match p {
        Priority::Efficient => 1.10,
        Priority::Balanced => 1.20,
        Priority::Fast => 1.35,
    }
}

/// Statik itki başına pervane disk yüklemesi sınırları [N/m²]: 150-350 (hobi sabit kanat
/// pervaneleri; referans 10" pervane ≈ 232 N/m²). Momentum teorisi: D = √(4T/(π·DL)).
pub const DISK_LOADING_RANGE: (f64, f64) = (150.0, 350.0);

/// Statik itkiye [N] göre önerilen pervane çapı aralığı [m] (küçükten büyüğe).
pub fn prop_diameter_range(t_static_n: f64) -> (f64, f64) {
    let d = |dl: f64| (4.0 * t_static_n.max(0.0) / (std::f64::consts::PI * dl)).sqrt();
    (d(DISK_LOADING_RANGE.1), d(DISK_LOADING_RANGE.0))
}

/// Ham girdileri üretir.
/// Sabit kanat/VTOL: T/W, motor payı, pervane çapı (aralığın geometrik ortası, tam inç),
/// hatve ≈ 0.6·çap (referans 10×6), kurulu güç = modelin önerdiği motor gücü 10 W'a yukarı yuvarlı.
/// Multirotor (ve VTOL kaldırma motorları): motor başına azami itki = T/W · ağırlık / motor sayısı.
pub fn derive(a: &PropAnswers, launch: Launch, dt: DroneType, ctx: &Ctx) -> Derived {
    let g = 9.80665;
    let mut v = Vec::new();
    if dt == DroneType::Multirotor {
        if ctx.mtow_kg > 0.0 && ctx.n_motors > 0 {
            let t = a.mr_style.thrust_to_weight() * ctx.mtow_kg * g / ctx.n_motors as f64;
            v.push((Field::MrTMax, t));
        }
        return v;
    }
    let tw = tw_target(a.priority, launch);
    v.push((Field::TwStatic, tw));
    v.push((Field::MotorMargin, motor_margin(a.priority)));
    if ctx.mtow_kg > 0.0 {
        let (lo, hi) = prop_diameter_range(tw * ctx.mtow_kg * g);
        let d = round_to_inch((lo * hi).sqrt());
        v.push((Field::DProp, d));
        v.push((Field::PropPitch, round_to_inch(0.6 * d)));
    }
    if ctx.p_motor_suggest_w.is_finite() && ctx.p_motor_suggest_w > 0.0 {
        v.push((
            Field::PMaxElec,
            (ctx.p_motor_suggest_w / 10.0).ceil() * 10.0,
        ));
    }
    if dt == DroneType::VtolQuadplane && ctx.mtow_kg > 0.0 && ctx.n_motors > 0 {
        // Kaldırma motorları: askıda kontrol payı için T/W ≈ 2.
        let t = 2.0 * ctx.mtow_kg * g / ctx.n_motors as f64;
        v.push((Field::MrTMax, t));
    }
    v
}

/// Önerilen motor sınıfı (i18n anahtarı), elektrik gücüne göre. Eşikler tipik fırçasız motor
/// güç sınıflarıdır: <80 W mikro, <200 W küçük, <500 W orta, <1200 W büyük, üstü çok büyük.
pub fn motor_class_key(p_elec_w: f64) -> &'static str {
    if !p_elec_w.is_finite() || p_elec_w < 80.0 {
        "simple.prop.class.micro"
    } else if p_elec_w < 200.0 {
        "simple.prop.class.small"
    } else if p_elec_w < 500.0 {
        "simple.prop.class.medium"
    } else if p_elec_w < 1200.0 {
        "simple.prop.class.large"
    } else {
        "simple.prop.class.xl"
    }
}

/// Multirotor askı gazı derecesi: ≤%50 iyi, ≤%65 orta, üstü kötü (askıda gaz payı kalmalı).
pub fn hover_throttle_rating(throttle: f64) -> Rating {
    if !throttle.is_finite() || throttle <= 0.0 {
        Rating::Bad
    } else if throttle <= 0.50 {
        Rating::Good
    } else if throttle <= 0.65 {
        Rating::Ok
    } else {
        Rating::Bad
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(mtow: f64) -> Ctx<'static> {
        Ctx {
            mtow_kg: mtow,
            n_motors: 4,
            p_motor_suggest_w: 233.0,
            materials: &[],
        }
    }

    #[test]
    fn all_combinations_valid_for_every_type() {
        for &priority in Priority::ALL {
            for &mr_style in MrStyle::ALL {
                for &launch in Launch::ALL {
                    for dt in DroneType::ALL {
                        let a = PropAnswers { priority, mr_style };
                        for mtow in [0.5, 2.0, 8.0] {
                            let d = derive(&a, launch, dt, &ctx(mtow));
                            assert!(!d.is_empty());
                            for (f, v) in &d {
                                assert!(v.is_finite() && *v > 0.0, "{f:?} {v}");
                            }
                            if dt != DroneType::Multirotor {
                                let g = |f| d.iter().find(|(x, _)| *x == f).unwrap().1;
                                assert!(g(Field::MotorMargin) >= 1.0);
                                assert!(g(Field::PropPitch) <= g(Field::DProp));
                                let (lo, hi) =
                                    prop_diameter_range(g(Field::TwStatic) * mtow * 9.80665);
                                assert!(lo < hi && g(Field::DProp) >= 0.0254);
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn balanced_two_kg_reproduces_reference_prop() {
        let d = derive(
            &PropAnswers::default(),
            Launch::Runway,
            DroneType::FixedWingConventional,
            &ctx(2.0),
        );
        let g = |f| d.iter().find(|(x, _)| *x == f).unwrap().1;
        assert!((g(Field::TwStatic) - 0.60).abs() < 1e-12);
        assert!((g(Field::DProp) / 0.0254 - 10.0).abs() < 1e-6, "10 inç");
        assert!(
            (g(Field::PropPitch) / 0.0254 - 6.0).abs() < 1e-6,
            "6 inç hatve"
        );
        assert_eq!(g(Field::PMaxElec), 240.0);
    }

    #[test]
    fn hand_launch_raises_thrust_target() {
        let a = tw_target(Priority::Efficient, Launch::Runway);
        let b = tw_target(Priority::Efficient, Launch::Hand);
        assert!(b > a && b >= 0.8);
        assert!(
            tw_target(Priority::Fast, Launch::Hand) >= tw_target(Priority::Fast, Launch::Runway)
        );
    }

    #[test]
    fn multirotor_thrust_scales_with_style_and_mass() {
        let g = |s, m| {
            let a = PropAnswers {
                mr_style: s,
                ..Default::default()
            };
            derive(&a, Launch::Vertical, DroneType::Multirotor, &ctx(m))[0].1
        };
        assert!(g(MrStyle::Racing, 1.0) > g(MrStyle::Sporty, 1.0));
        assert!(g(MrStyle::Sporty, 1.0) > g(MrStyle::Relaxed, 1.0));
        assert!(g(MrStyle::Relaxed, 2.0) > g(MrStyle::Relaxed, 1.0));
        // T/W = 2: 4 motor × t = 2·W
        assert!((4.0 * g(MrStyle::Relaxed, 1.0) - 2.0 * 9.80665).abs() < 1e-9);
    }

    #[test]
    fn classes_and_ratings() {
        assert_eq!(motor_class_key(50.0), "simple.prop.class.micro");
        assert_eq!(motor_class_key(250.0), "simple.prop.class.medium");
        assert_eq!(motor_class_key(5000.0), "simple.prop.class.xl");
        assert_eq!(hover_throttle_rating(0.45), Rating::Good);
        assert_eq!(hover_throttle_rating(0.6), Rating::Ok);
        assert_eq!(hover_throttle_rating(0.8), Rating::Bad);
    }
}
