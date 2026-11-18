// SPDX-License-Identifier: AGPL-3.0-or-later
//! Güvenilirlik (MTBF) tahmini. Saf Rust, arayüz yok.
//!
//! Model: bileşenler seri bağlı kabul edilir (`λ_toplam = Σ n_i·λ_i`, `MTBF = 1/λ_toplam`,
//! `R(t) = exp(-λ_toplam·t)`). Altı veya sekiz motorlu (ortadan kaldırılmış kolda bile uçabilen)
//! multirotorlarda motor grubu k-out-of-n olarak ele alınır: n-1 motor çalışırsa araç uçmaya devam
//! eder. Bu, motor kaybı sonrası kontrol edilebilirliğin basitleştirilmiş bir modelidir.
//!
//! Arıza oranları [arıza / 10^6 uçuş saati] birimindedir ve YALNIZCA ÖRNEK değerlerdir; kullanıcı
//! kendi verisiyle değiştirmelidir.

use crate::calc::Model;
use crate::project::{DroneType, Project};
use std::collections::HashMap;

/// Tasarımda uçuş süresi hesaplanamazsa kullanılan görev süresi [saat].
pub const DEFAULT_MISSION_HOURS: f64 = 0.5;

/// Bileşen türleri. Her tür için varsayılan arıza oranı bir örnek değerdir.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ComponentKind {
    BrushlessMotor,
    Esc,
    Propeller,
    FlightController,
    Gps,
    Receiver,
    BatteryPack,
    PowerModule,
    Servo,
    AirframeJoint,
}

impl ComponentKind {
    /// Tablodaki sıra (arayüz de bu sırayı kullanır).
    pub const ALL: [ComponentKind; 10] = [
        ComponentKind::BrushlessMotor,
        ComponentKind::Esc,
        ComponentKind::Propeller,
        ComponentKind::FlightController,
        ComponentKind::Gps,
        ComponentKind::Receiver,
        ComponentKind::BatteryPack,
        ComponentKind::PowerModule,
        ComponentKind::Servo,
        ComponentKind::AirframeJoint,
    ];

    /// Varsayılan arıza oranı [arıza / 10^6 saat] (örnek değer).
    pub fn default_rate(self) -> f64 {
        match self {
            ComponentKind::BrushlessMotor => 200.0,
            ComponentKind::Esc => 300.0,
            ComponentKind::Propeller => 80.0,
            ComponentKind::FlightController => 150.0,
            ComponentKind::Gps => 250.0,
            ComponentKind::Receiver => 120.0,
            ComponentKind::BatteryPack => 500.0,
            ComponentKind::PowerModule => 100.0,
            ComponentKind::Servo => 400.0,
            ComponentKind::AirframeJoint => 50.0,
        }
    }

    /// Görünen ad için i18n anahtarı.
    pub fn label_key(self) -> &'static str {
        match self {
            ComponentKind::BrushlessMotor => "reliability.kind.brushless_motor",
            ComponentKind::Esc => "reliability.kind.esc",
            ComponentKind::Propeller => "reliability.kind.propeller",
            ComponentKind::FlightController => "reliability.kind.flight_controller",
            ComponentKind::Gps => "reliability.kind.gps",
            ComponentKind::Receiver => "reliability.kind.receiver",
            ComponentKind::BatteryPack => "reliability.kind.battery_pack",
            ComponentKind::PowerModule => "reliability.kind.power_module",
            ComponentKind::Servo => "reliability.kind.servo",
            ComponentKind::AirframeJoint => "reliability.kind.airframe_joint",
        }
    }

    /// Kısa not için i18n anahtarı.
    pub fn note_key(self) -> &'static str {
        match self {
            ComponentKind::BrushlessMotor => "reliability.note.brushless_motor",
            ComponentKind::Esc => "reliability.note.esc",
            ComponentKind::Propeller => "reliability.note.propeller",
            ComponentKind::FlightController => "reliability.note.flight_controller",
            ComponentKind::Gps => "reliability.note.gps",
            ComponentKind::Receiver => "reliability.note.receiver",
            ComponentKind::BatteryPack => "reliability.note.battery_pack",
            ComponentKind::PowerModule => "reliability.note.power_module",
            ComponentKind::Servo => "reliability.note.servo",
            ComponentKind::AirframeJoint => "reliability.note.airframe_joint",
        }
    }
}

/// Kullanıcının girdiği arıza oranı geçersiz.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReliabilityError {
    /// Arıza oranı sonlu ve > 0 olmalı.
    InvalidRate,
    /// Görev süresi sonlu ve >= 0 olmalı.
    InvalidMissionTime,
}

/// Kullanıcı tarafından değiştirilmiş arıza oranları (türe göre). Eksik tür varsayılanı kullanır.
pub type RateOverrides = HashMap<ComponentKind, f64>;

/// Bir bileşen satırı: tür, adet ve uygulanacak arıza oranı [arıza / 10^6 saat].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Item {
    pub kind: ComponentKind,
    pub qty: u32,
    pub rate: f64,
}

/// Bir bileşen türünün toplam arıza içindeki payı.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contribution {
    pub kind: ComponentKind,
    /// Yüzde [%].
    pub pct: f64,
}

/// Hesap sonucu.
#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    /// Toplam arıza oranı [1/saat] (seri, yedeklilik kredisi verilmeden).
    pub lambda_per_h: f64,
    /// Seri MTBF [saat] = 1/λ_toplam (yedeklilik kredisi verilmeden).
    pub mtbf_h: f64,
    /// Seri sistemin t saatlik güvenilirliği (yedeklilik kredisi yok).
    pub r_series: f64,
    /// Motor grubunun t saatlik güvenilirliği (yedeklilik uygulanmışsa k-out-of-n).
    pub r_motor_group: f64,
    /// Yedeklilik kredisi dahil görev güvenilirliği.
    pub r_mission: f64,
    /// 100 uçuşta en az bir arıza olasılığı = 1 - R_görev^100.
    pub p_fail_100_flights: f64,
    /// Bileşen türü başına katkı yüzdesi (seri temele göre).
    pub contributions: Vec<Contribution>,
}

/// Arıza oranı metnini ayrıştırır. Boş metin `None` (varsayılan kullanılır). Virgül ondalık
/// ayracı kabul edilir. Geçersiz (sayı değil, <= 0, sonsuz) değer hata verir.
pub fn parse_rate(text: &str) -> Result<Option<f64>, ReliabilityError> {
    let s = text.trim();
    if s.is_empty() {
        return Ok(None);
    }
    let v: f64 = s
        .replace(',', ".")
        .parse()
        .map_err(|_| ReliabilityError::InvalidRate)?;
    validate_rate(v).map(Some)
}

/// Arıza oranı doğrulaması: sonlu ve > 0.
pub fn validate_rate(rate: f64) -> Result<f64, ReliabilityError> {
    if rate.is_finite() && rate > 0.0 {
        Ok(rate)
    } else {
        Err(ReliabilityError::InvalidRate)
    }
}

/// Bir tür için uygulanacak arıza oranı (geçersiz/eksik ise varsayılan).
pub fn rate_for(kind: ComponentKind, overrides: &RateOverrides) -> f64 {
    overrides
        .get(&kind)
        .copied()
        .unwrap_or_else(|| kind.default_rate())
}

/// Tasarımdan türetilen bileşen adetleri.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DesignCounts {
    /// Motor sayısı (her motora bir ESC ve bir pervane).
    pub motors: u32,
    /// Servo sayısı (sabit kanat: kanat/kuyruk yüzeyleri, varsayımsal 4).
    pub servos: u32,
    /// Gövde/kol birleşim sayısı.
    pub airframe_joints: u32,
    /// Motor kaybında araç uçmaya devam edebilir mi (k-out-of-n uygulanır mı)?
    pub motor_redundant: bool,
}

/// Projeden bileşen adetlerini çıkarır.
/// - Multirotor: motor = `m.mr.n_motors`; 6 veya 8 kollu ve koaksiyel olmayanlar yedekli.
/// - VTOL: kaldırma motorları + 1 itici motor; yedeklilik yok.
/// - Sabit kanat: tek motor; yedeklilik yok.
pub fn design_counts(p: &Project, m: &Model) -> DesignCounts {
    match p.drone_type {
        DroneType::Multirotor => DesignCounts {
            motors: m.mr.n_motors,
            servos: 0,
            airframe_joints: p.mr_arm_count,
            motor_redundant: !p.mr_coaxial && p.mr_arm_count >= 6,
        },
        DroneType::VtolQuadplane => DesignCounts {
            motors: m.mr.n_motors + 1,
            servos: 4,
            airframe_joints: 2 + p.mr_arm_count,
            motor_redundant: false,
        },
        _ => DesignCounts {
            motors: 1,
            servos: 4,
            airframe_joints: 2,
            motor_redundant: false,
        },
    }
}

/// Varsayılan görev süresi: tasarımın uçuş (dayanıklılık) süresi; yoksa [`DEFAULT_MISSION_HOURS`].
pub fn default_mission_hours(m: &Model) -> f64 {
    let h = m.perf_end.endurance_h();
    if h.is_finite() && h > 0.0 {
        h
    } else {
        DEFAULT_MISSION_HOURS
    }
}

/// Bileşen listesini oluşturur. Her satırın arıza oranı, geçersiz/eksikse varsayılandır.
pub fn build_items(c: &DesignCounts, overrides: &RateOverrides) -> Vec<Item> {
    let rows = [
        (ComponentKind::BrushlessMotor, c.motors),
        (ComponentKind::Esc, c.motors),
        (ComponentKind::Propeller, c.motors),
        (ComponentKind::FlightController, 1),
        (ComponentKind::Gps, 1),
        (ComponentKind::Receiver, 1),
        (ComponentKind::BatteryPack, 1),
        (ComponentKind::PowerModule, 1),
        (ComponentKind::Servo, c.servos),
        (ComponentKind::AirframeJoint, c.airframe_joints),
    ];
    rows.iter()
        .map(|&(kind, qty)| Item {
            kind,
            qty,
            rate: rate_for(kind, overrides),
        })
        .collect()
}

/// Binom katsayısı C(n, k) (k <= n).
fn binom(n: u32, k: u32) -> f64 {
    let k = k.min(n - k);
    let mut c = 1.0;
    for i in 0..k {
        c = c * f64::from(n - i) / f64::from(i + 1);
    }
    c
}

/// En az `k` tanesi çalışan, n birimden oluşan grubun güvenilirliği; birim güvenilirliği `r`.
/// `R_grup = Σ_{j=k..n} C(n,j) r^j (1-r)^(n-j)`.
pub fn k_out_of_n_survival(n: u32, r: f64, k: u32) -> f64 {
    (k..=n)
        .map(|j| binom(n, j) * r.powi(j as i32) * (1.0 - r).powi((n - j) as i32))
        .sum()
}

/// Bileşen listesi için güvenilirlik ve MTBF hesabı.
///
/// `items`: bileşen satırları (qty = 0 olanlar yok sayılır). `motor_redundant`: motor grubu
/// bir motor kaybına dayanıklı mı (k = n-1 of n). `t_hours`: görev süresi [saat].
pub fn evaluate(
    items: &[Item],
    motor_redundant: bool,
    t_hours: f64,
) -> Result<Report, ReliabilityError> {
    if !t_hours.is_finite() || t_hours < 0.0 {
        return Err(ReliabilityError::InvalidMissionTime);
    }

    let mut total = 0.0;
    let mut motor_n: u32 = 0;
    let mut motor_lambda = 0.0;
    let mut lambdas: Vec<(ComponentKind, f64)> = Vec::with_capacity(items.len());
    for it in items {
        validate_rate(it.rate)?;
        let lam = f64::from(it.qty) * it.rate * 1e-6;
        total += lam;
        if it.kind == ComponentKind::BrushlessMotor {
            motor_n += it.qty;
            motor_lambda = it.rate * 1e-6;
        }
        lambdas.push((it.kind, lam));
    }

    let contributions = lambdas
        .into_iter()
        .map(|(kind, lam)| Contribution {
            kind,
            pct: if total > 0.0 {
                lam / total * 100.0
            } else {
                0.0
            },
        })
        .collect();

    let r_series = (-total * t_hours).exp();
    let r_unit = (-motor_lambda * t_hours).exp();
    let r_motor_group = if motor_n == 0 {
        1.0
    } else if motor_redundant && motor_n >= 2 {
        k_out_of_n_survival(motor_n, r_unit, motor_n - 1)
    } else {
        r_unit.powi(motor_n as i32)
    };
    let rest = (total - f64::from(motor_n) * motor_lambda).max(0.0);
    let r_mission = r_motor_group * (-rest * t_hours).exp();

    Ok(Report {
        lambda_per_h: total,
        mtbf_h: if total > 0.0 {
            1.0 / total
        } else {
            f64::INFINITY
        },
        r_series,
        r_motor_group,
        r_mission,
        p_fail_100_flights: 1.0 - r_mission.powi(100),
        contributions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Library, evaluate as eval_model};

    fn item(kind: ComponentKind, qty: u32, rate: f64) -> Item {
        Item { kind, qty, rate }
    }

    #[test]
    fn series_mtbf_matches_hand_calculation() {
        // FC 100 + GPS 200 = 300e-6 /h  ->  MTBF = 1/3e-4 = 3333.33 h
        let items = [
            item(ComponentKind::FlightController, 1, 100.0),
            item(ComponentKind::Gps, 1, 200.0),
        ];
        let r = evaluate(&items, false, 2.0).unwrap();
        assert!((r.lambda_per_h - 3.0e-4).abs() < 1e-15);
        assert!((r.mtbf_h - 10000.0 / 3.0).abs() < 1e-9);
        // Katkılar: %33.33 FC, %66.67 GPS
        assert!((r.contributions[0].pct - 100.0 / 3.0).abs() < 1e-9);
        assert!((r.contributions[1].pct - 200.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn series_reliability_is_exponential() {
        let items = [
            item(ComponentKind::FlightController, 1, 100.0),
            item(ComponentKind::Gps, 1, 200.0),
        ];
        let r = evaluate(&items, false, 2.0).unwrap();
        assert!((r.r_series - (-6.0e-4f64).exp()).abs() < 1e-12);
        // Motor yok: görev güvenilirliği seri ile aynı olmalı.
        assert!((r.r_mission - r.r_series).abs() < 1e-12);
    }

    #[test]
    fn k_out_of_n_matches_hand_value() {
        // Hexa, birim güvenilirlik 0.9, en az 5 çalışan:
        // 6*0.9^5*0.1 + 0.9^6 = 0.354294 + 0.531441 = 0.885735
        let r = k_out_of_n_survival(6, 0.9, 5);
        assert!((r - 0.885_735).abs() < 1e-9, "{r}");
    }

    #[test]
    fn hexa_motor_group_is_more_reliable_than_quad_and_plain_six() {
        let t: f64 = 500.0;
        let rate: f64 = 200.0;
        let r_m = (-rate * 1e-6 * t).exp();

        // Quad: yedeklilik yok, 4 motor seri -> r^4.
        let quad = evaluate(&[item(ComponentKind::BrushlessMotor, 4, rate)], false, t).unwrap();
        assert!((quad.r_motor_group - r_m.powi(4)).abs() < 1e-12);

        // Hexa (yedekli) ile aynı 6 motor, yedeklilik yok (seri) karşılaştırması.
        let six_series =
            evaluate(&[item(ComponentKind::BrushlessMotor, 6, rate)], false, t).unwrap();
        let hexa = evaluate(&[item(ComponentKind::BrushlessMotor, 6, rate)], true, t).unwrap();
        assert!(hexa.r_motor_group > six_series.r_motor_group);
        assert!(hexa.r_motor_group > quad.r_motor_group);
    }

    #[test]
    fn overrides_apply_to_items() {
        let mut o = RateOverrides::new();
        o.insert(ComponentKind::BrushlessMotor, 1000.0);
        let c = DesignCounts {
            motors: 4,
            servos: 0,
            airframe_joints: 4,
            motor_redundant: false,
        };
        let items = build_items(&c, &o);
        let motor = items
            .iter()
            .find(|i| i.kind == ComponentKind::BrushlessMotor)
            .unwrap();
        assert_eq!(motor.rate, 1000.0);
        // Diğer türler varsayılanda kalır.
        let gps = items.iter().find(|i| i.kind == ComponentKind::Gps).unwrap();
        assert_eq!(gps.rate, ComponentKind::Gps.default_rate());
    }

    #[test]
    fn invalid_rates_are_rejected() {
        for bad in ["0", "-5", "abc", "nan", "inf", "-0.1"] {
            assert_eq!(parse_rate(bad), Err(ReliabilityError::InvalidRate), "{bad}");
        }
        assert_eq!(parse_rate(""), Ok(None));
        assert_eq!(parse_rate("250,5"), Ok(Some(250.5)));

        let bad_items = [item(ComponentKind::Gps, 1, 0.0)];
        assert_eq!(
            evaluate(&bad_items, false, 1.0),
            Err(ReliabilityError::InvalidRate)
        );
        let ok_items = [item(ComponentKind::Gps, 1, 10.0)];
        assert_eq!(
            evaluate(&ok_items, false, -1.0),
            Err(ReliabilityError::InvalidMissionTime)
        );
    }

    #[test]
    fn default_rates_are_positive() {
        for k in ComponentKind::ALL {
            assert!(validate_rate(k.default_rate()).is_ok(), "{k:?}");
        }
    }

    #[test]
    fn design_counts_follow_vehicle_type() {
        let lib = Library::embedded();

        let mut quad = Project::default();
        quad.set_drone_type(DroneType::Multirotor);
        quad.mr_arm_count = 4;
        let m = eval_model(&quad, &lib);
        let c = design_counts(&quad, &m);
        assert_eq!(c.motors, m.mr.n_motors);
        assert!(!c.motor_redundant);

        let mut hexa = quad.clone();
        hexa.mr_arm_count = 6;
        let m = eval_model(&hexa, &lib);
        let c = design_counts(&hexa, &m);
        assert!(c.motor_redundant);
        assert_eq!(c.airframe_joints, 6);

        let mut fw = Project::default();
        fw.set_drone_type(DroneType::FixedWingConventional);
        let m = eval_model(&fw, &lib);
        let c = design_counts(&fw, &m);
        assert_eq!(c.motors, 1);
        assert!(!c.motor_redundant);
    }
}
