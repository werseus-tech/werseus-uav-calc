// SPDX-License-Identifier: AGPL-3.0-or-later
//! Adım 1 (Görev) basit modu: "Drone ne yapacak?", "Ne kadar havada kalsın?", "Ne taşıyacak?",
//! "Nereden kalkacak?", "Nerede uçacak?" → irtifa, seyir hızı, faydalı yük, hedef süre ve
//! drone türü önerisi. Doğrulanmış formüllere dokunulmaz; yalnızca girdi üretir.
use super::{Derived, Field, Rating, choice};
use crate::calc::aero_presets::Purpose;
use crate::project::DroneType;
use serde::{Deserialize, Serialize};

choice!(
    /// "Drone ne yapacak?"
    Goal, "simple.mission.goal", Photo, [
        Photo => "photo",
        Mapping => "mapping",
        Fun => "fun",
        Cargo => "cargo",
        Learning => "learning",
    ]
);

choice!(
    /// "Ne kadar havada kalsın?"
    EnduranceGoal, "simple.mission.endurance", Min20, [
        Min10 => "m10",
        Min20 => "m20",
        Min45 => "m45",
        Min90 => "m90",
    ]
);

impl EnduranceGoal {
    /// Hedef havada kalma süresi [dk].
    pub fn minutes(self) -> f64 {
        match self {
            EnduranceGoal::Min10 => 10.0,
            EnduranceGoal::Min20 => 20.0,
            EnduranceGoal::Min45 => 45.0,
            EnduranceGoal::Min90 => 90.0,
        }
    }
}

choice!(
    /// "Ne taşıyacak?"
    PayloadClass, "simple.mission.payload", Small, [
        Nothing => "none",
        Small => "small",
        Action => "action",
        Gimbal => "gimbal",
        Custom => "custom",
    ]
);

impl PayloadClass {
    /// Sınıfın tipik kütlesi [g] (özel için `None`). Değerler tipik tüketici ürün sınıflarıdır:
    /// küçük FPV/analog kamera+verici ~50 g, aksiyon kamerası ~150 g, gimbal'li kamera ~400 g.
    pub fn grams(self) -> Option<f64> {
        match self {
            PayloadClass::Nothing => Some(0.0),
            PayloadClass::Small => Some(50.0),
            PayloadClass::Action => Some(150.0),
            PayloadClass::Gimbal => Some(400.0),
            PayloadClass::Custom => None,
        }
    }
}

choice!(
    /// "Nereden kalkacak?"
    Launch, "simple.mission.launch", Runway, [
        Hand => "hand",
        Runway => "runway",
        Vertical => "vertical",
    ]
);

choice!(
    /// "Nerede uçacak?" (deniz seviyesinden yükseklik; hava yoğunluğunu belirler)
    Terrain, "simple.mission.terrain", Plain, [
        Coast => "coast",
        Plain => "plain",
        Hills => "hills",
        Mountain => "mountain",
    ]
);

impl Terrain {
    /// Bölgenin deniz seviyesinden tipik yüksekliği [m] (ISA atmosferi).
    pub fn altitude_m(self) -> f64 {
        match self {
            Terrain::Coast => 0.0,
            Terrain::Plain => 500.0,
            Terrain::Hills => 1200.0,
            Terrain::Mountain => 2500.0,
        }
    }
}

/// Görev adımı cevapları.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MissionAnswers {
    pub goal: Goal,
    pub endurance: EnduranceGoal,
    pub payload: PayloadClass,
    /// "Özel" yük kütlesi [g].
    pub custom_payload_g: f64,
    pub launch: Launch,
    pub terrain: Terrain,
}

impl Default for MissionAnswers {
    fn default() -> Self {
        Self {
            goal: Goal::default(),
            endurance: EnduranceGoal::default(),
            payload: PayloadClass::default(),
            custom_payload_g: 250.0,
            launch: Launch::default(),
            terrain: Terrain::default(),
        }
    }
}

impl MissionAnswers {
    /// Faydalı yük kütlesi [kg] (özel kütle 0..5000 g aralığına sıkıştırılır).
    pub fn payload_kg(&self) -> f64 {
        let g = self
            .payload
            .grams()
            .unwrap_or_else(|| self.custom_payload_g.clamp(0.0, 5000.0));
        g / 1000.0
    }

    pub fn target_minutes(&self) -> f64 {
        self.endurance.minutes()
    }
}

/// Seyir hızı [m/s]. Sabit kanat/VTOL için: fotoğraf/gözetleme 16 (kamera sarsıntısı ve stabil
/// akış), haritalama 15 (hareket bulanıklığı ve örtüşme sınırı; tipik 12-18), eğlence/FPV 22
/// (hızlı ama hobi aralığında), yük 15 (verimli seyir), öğrenme 13 (kolay, yavaş). Multirotor
/// için: fotoğraf 8, haritalama 7, eğlence 15, yük 8, öğrenme 6 (tipik 5-15; Kısıt 5-100 içinde).
pub fn cruise_speed(goal: Goal, dt: DroneType) -> f64 {
    if dt == DroneType::Multirotor {
        match goal {
            Goal::Photo => 8.0,
            Goal::Mapping => 7.0,
            Goal::Fun => 15.0,
            Goal::Cargo => 8.0,
            Goal::Learning => 6.0,
        }
    } else {
        match goal {
            Goal::Photo => 16.0,
            Goal::Mapping => 15.0,
            Goal::Fun => 22.0,
            Goal::Cargo => 15.0,
            Goal::Learning => 13.0,
        }
    }
}

/// Cevaplardan ham girdileri üretir: irtifa, seyir hızı, faydalı yük (+ multirotorda askı/seyir hızı).
pub fn derive(a: &MissionAnswers, dt: DroneType) -> Derived {
    let v = cruise_speed(a.goal, dt);
    let mut d = vec![
        (Field::Altitude, a.terrain.altitude_m()),
        (Field::CruiseSpeed, v),
        (Field::Payload, a.payload_kg()),
    ];
    if dt == DroneType::Multirotor {
        d.push((Field::MrCruise, v));
    }
    d
}

/// Aerodinamik basit modun "amaç" sorusu için görev amacından varsayılan öneri.
pub fn aero_purpose(goal: Goal) -> Purpose {
    match goal {
        Goal::Mapping => Purpose::Endurance,
        Goal::Fun => Purpose::Speed,
        Goal::Photo | Goal::Cargo | Goal::Learning => Purpose::Easy,
    }
}

/// Önerilen drone türü ve nedeni (i18n anahtarı).
///
/// Mantık (sezgisel): dikey kalkış + uzun süre/haritalama → VTOL (hem pistsiz hem verimli);
/// dikey kalkış + kısa süre → multirotor; aksi halde sabit kanat (öğrenme: klasik, anlaşılır;
/// eğlence/FPV: dayanıklı uçan kanat). Multirotorların tipik uçuş süresi 15-30 dk civarındadır.
pub fn suggest_type(a: &MissionAnswers) -> (DroneType, &'static str) {
    let long = a.target_minutes() >= 45.0;
    match a.launch {
        Launch::Vertical => {
            if long || a.goal == Goal::Mapping {
                (DroneType::VtolQuadplane, "simple.mission.sug.vtol")
            } else {
                (DroneType::Multirotor, "simple.mission.sug.multirotor")
            }
        }
        _ => match a.goal {
            Goal::Fun => (DroneType::FlyingWing, "simple.mission.sug.flying_wing"),
            Goal::Learning => (
                DroneType::FixedWingConventional,
                "simple.mission.sug.classic_learn",
            ),
            _ => (
                DroneType::FixedWingConventional,
                "simple.mission.sug.classic",
            ),
        },
    }
}

/// Seçilen tür ile cevaplar arasındaki tutarsızlık uyarıları (i18n anahtarları).
pub fn consistency_notes(a: &MissionAnswers, dt: DroneType) -> Vec<&'static str> {
    let mut v = Vec::new();
    let long = a.target_minutes() >= 45.0;
    if dt == DroneType::Multirotor && long {
        v.push("simple.mission.note.mr_long");
    }
    if matches!(
        dt,
        DroneType::FixedWingConventional | DroneType::FixedWingVTail | DroneType::FlyingWing
    ) && a.launch == Launch::Vertical
    {
        v.push("simple.mission.note.fw_vertical");
    }
    if dt == DroneType::Multirotor && a.launch == Launch::Hand {
        v.push("simple.mission.note.mr_hand");
    }
    if dt == DroneType::VtolQuadplane && a.goal == Goal::Learning {
        v.push("simple.mission.note.vtol_learning");
    }
    if a.goal == Goal::Mapping && a.payload == PayloadClass::Nothing {
        v.push("simple.mission.note.mapping_no_payload");
    }
    if a.goal == Goal::Cargo && a.payload == PayloadClass::Nothing {
        v.push("simple.mission.note.cargo_no_payload");
    }
    if a.launch == Launch::Hand && a.payload_kg() >= 0.35 {
        v.push("simple.mission.note.hand_heavy");
    }
    v
}

/// "Elle fırlatılabilir mi?" — kalkış kütlesine göre. Pratik eşik: ≤2 kg rahat, ≤3 kg zor ama
/// mümkün, üstü güvensiz/yorucu (hobi uygulaması kuralı; yalnızca yaklaşık).
pub fn hand_launch_rating(mtow_kg: f64) -> Rating {
    if !mtow_kg.is_finite() {
        Rating::Bad
    } else if mtow_kg <= 2.0 {
        Rating::Good
    } else if mtow_kg <= 3.0 {
        Rating::Ok
    } else {
        Rating::Bad
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_answers() -> Vec<MissionAnswers> {
        let mut v = Vec::new();
        for &goal in Goal::ALL {
            for &endurance in EnduranceGoal::ALL {
                for &payload in PayloadClass::ALL {
                    for &launch in Launch::ALL {
                        for &terrain in Terrain::ALL {
                            v.push(MissionAnswers {
                                goal,
                                endurance,
                                payload,
                                custom_payload_g: 123.0,
                                launch,
                                terrain,
                            });
                        }
                    }
                }
            }
        }
        v
    }

    #[test]
    fn every_combination_is_valid_for_every_type() {
        let n = all_answers().len();
        assert_eq!(n, 5 * 4 * 5 * 3 * 4);
        for a in all_answers() {
            for dt in DroneType::ALL {
                let d = derive(&a, dt);
                for (f, v) in &d {
                    assert!(v.is_finite(), "{f:?}");
                }
                let get = |f: Field| d.iter().find(|(g, _)| *g == f).map(|(_, v)| *v).unwrap();
                let h = get(Field::Altitude);
                let vc = get(Field::CruiseSpeed);
                let pl = get(Field::Payload);
                // Sihirbaz doğrulama aralıkları (wizard::validate_step Mission/Weight)
                assert!((0.0..=11000.0).contains(&h));
                assert!((5.0..=100.0).contains(&vc));
                assert!((0.0..=5.0).contains(&pl));
                assert_eq!(
                    d.iter().any(|(f, _)| *f == Field::MrCruise),
                    dt == DroneType::Multirotor
                );
            }
        }
    }

    #[test]
    fn payload_classes_match_brief() {
        let mut a = MissionAnswers::default();
        let kg = |c: PayloadClass, a: &mut MissionAnswers| {
            a.payload = c;
            a.payload_kg()
        };
        assert_eq!(kg(PayloadClass::Nothing, &mut a), 0.0);
        assert!((kg(PayloadClass::Small, &mut a) - 0.05).abs() < 1e-12);
        assert!((kg(PayloadClass::Action, &mut a) - 0.15).abs() < 1e-12);
        assert!((kg(PayloadClass::Gimbal, &mut a) - 0.40).abs() < 1e-12);
        a.custom_payload_g = -5.0;
        assert_eq!(kg(PayloadClass::Custom, &mut a), 0.0);
        a.custom_payload_g = 1e9;
        assert_eq!(kg(PayloadClass::Custom, &mut a), 5.0);
    }

    #[test]
    fn type_suggestion_and_consistency() {
        let mut a = MissionAnswers::default();
        a.launch = Launch::Vertical;
        a.endurance = EnduranceGoal::Min20;
        a.goal = Goal::Photo;
        assert_eq!(suggest_type(&a).0, DroneType::Multirotor);
        a.endurance = EnduranceGoal::Min90;
        assert_eq!(suggest_type(&a).0, DroneType::VtolQuadplane);
        a.launch = Launch::Hand;
        a.goal = Goal::Fun;
        assert_eq!(suggest_type(&a).0, DroneType::FlyingWing);
        // Tutarsızlıklar uyarı üretir
        a.launch = Launch::Vertical;
        assert!(
            consistency_notes(&a, DroneType::FixedWingConventional)
                .contains(&"simple.mission.note.fw_vertical")
        );
        a.endurance = EnduranceGoal::Min45;
        assert!(
            consistency_notes(&a, DroneType::Multirotor).contains(&"simple.mission.note.mr_long")
        );
        // Önerilen türle tutarlı seçimde uyarı yok (varsayılan klasik, pist, 20 dk)
        let d = MissionAnswers::default();
        assert!(consistency_notes(&d, suggest_type(&d).0).is_empty());
    }

    #[test]
    fn hand_launch_thresholds() {
        assert_eq!(hand_launch_rating(1.5), Rating::Good);
        assert_eq!(hand_launch_rating(2.5), Rating::Ok);
        assert_eq!(hand_launch_rating(5.0), Rating::Bad);
    }

    #[test]
    fn aero_purpose_mapping_is_total() {
        for &g in Goal::ALL {
            let _ = aero_purpose(g);
        }
    }
}
