// SPDX-License-Identifier: AGPL-3.0-or-later
//! Adım 8 (Ağırlık merkezi & Stabilite) basit modu: "Bataryayı nereye koyayım?" tek düğme
//! (mevcut çözücünün sonucu `batt_x_shift`), renkli statik marj ve "parmak ucunda dengele"
//! açıklaması. Multirotor için ağırlık merkezi sapması. Formüller (`calc::stability`) değişmez.
use super::Rating;
use crate::calc::StabOut;

/// Statik marjın günlük dil değerlendirmesi (i18n anahtarı ile birlikte).
/// SM yeşil bandın içindeyse iyi; bandın biraz dışında ama pozitifse orta;
/// negatif (kararsız) ya da çok uzak ise kötü.
pub fn rate_static_margin(sm: f64, band: (f64, f64)) -> (Rating, &'static str) {
    if !sm.is_finite() {
        return (Rating::Bad, "simple.stab.sm.unknown");
    }
    if sm < 0.0 {
        return (Rating::Bad, "simple.stab.sm.unstable");
    }
    if sm >= band.0 && sm <= band.1 {
        return (Rating::Good, "simple.stab.sm.good");
    }
    if sm < band.0 {
        let r = if sm >= 0.5 * band.0 {
            Rating::Ok
        } else {
            Rating::Bad
        };
        (r, "simple.stab.sm.low")
    } else {
        let r = if sm <= 1.5 * band.1 {
            Rating::Ok
        } else {
            Rating::Bad
        };
        (r, "simple.stab.sm.high")
    }
}

/// Bataryayı kaydırma yönü ve miktarı.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BatteryMove {
    /// true = geriye (kuyruğa doğru), false = ileriye (buruna doğru).
    pub backward: bool,
    pub mm: f64,
}

/// Çözücü sonucunu günlük dile çevirir (`shift` m, + = geriye). Çok küçük kaymada `None`
/// ("zaten dengede").
pub fn battery_move(shift_m: f64) -> Option<BatteryMove> {
    if !shift_m.is_finite() || shift_m.abs() < 0.001 {
        return None;
    }
    Some(BatteryMove {
        backward: shift_m > 0.0,
        mm: shift_m.abs() * 1000.0,
    })
}

/// "Uçağı parmak ucunda dengele": hedef ağırlık merkezinin kanat kökü hücum kenarından geriye
/// uzaklığı [mm] (burundan ölçülen hedef − kök hücum kenarı konumu).
pub fn balance_point_behind_wing_le_mm(x_cg_target_nose: f64, x_root_le: f64) -> f64 {
    (x_cg_target_nose - x_root_le) * 1000.0
}

/// Çözücünün batarya satırlarına uyguladığı kayma (ortak: arayüzdeki tek düğme bunu çağırır).
/// Tüm batarya satırları aynı miktarda kayar; satır dizini geçersizse atlanır.
pub fn apply_battery_shift(components: &mut [crate::project::Component], s: &StabOut) {
    if !s.batt_x_shift.is_finite() {
        return;
    }
    for &r in &s.batt_rows {
        if let Some(c) = components.get_mut(r) {
            c.x += s.batt_x_shift;
        }
    }
}

/// Multirotor ağırlık merkezi sapması (köşegene oranı): ≤%2 iyi, ≤%5 orta, üstü kötü
/// (merkezden kaçık ağırlık merkezi motorları dengesiz yükler).
pub fn mr_cg_rating(offset_frac: f64) -> Rating {
    let f = offset_frac.abs();
    if !f.is_finite() {
        Rating::Bad
    } else if f <= 0.02 {
        Rating::Good
    } else if f <= 0.05 {
        Rating::Ok
    } else {
        Rating::Bad
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Library, evaluate};
    use crate::project::Project;

    #[test]
    fn static_margin_bands() {
        let band = (0.05, 0.15);
        assert_eq!(rate_static_margin(0.10, band).0, Rating::Good);
        assert_eq!(rate_static_margin(0.03, band).0, Rating::Ok);
        assert_eq!(rate_static_margin(0.01, band).0, Rating::Bad);
        assert_eq!(rate_static_margin(0.20, band).0, Rating::Ok);
        assert_eq!(rate_static_margin(0.40, band).0, Rating::Bad);
        assert_eq!(
            rate_static_margin(-0.05, band),
            (Rating::Bad, "simple.stab.sm.unstable")
        );
        assert_eq!(rate_static_margin(f64::NAN, band).0, Rating::Bad);
    }

    #[test]
    fn battery_move_direction() {
        assert_eq!(battery_move(0.0004), None);
        let m = battery_move(-0.025).unwrap();
        assert!(!m.backward && (m.mm - 25.0).abs() < 1e-9);
        assert!(battery_move(0.05).unwrap().backward);
    }

    #[test]
    fn one_button_solver_reaches_target_margin() {
        let lib = Library::embedded();
        let mut p = Project::default();
        // Bataryayı bilerek kaydır → marj bozulur
        for c in &mut p.components {
            if c.link == crate::project::Link::Battery {
                c.x += 0.08;
            }
        }
        let m = evaluate(&p, &lib);
        assert!(m.stab.batt_x_shift.is_finite());
        apply_battery_shift(&mut p.components, &m.stab);
        let m2 = evaluate(&p, &lib);
        assert!(
            (m2.stab.sm - p.sm_target).abs() < 0.01,
            "sm {} hedef {}",
            m2.stab.sm,
            p.sm_target
        );
        assert!(rate_static_margin(m2.stab.sm, m2.stab.band).0 != Rating::Bad);
    }

    #[test]
    fn balance_point_and_mr_cg() {
        assert!((balance_point_behind_wing_le_mm(0.25, 0.20) - 50.0).abs() < 1e-9);
        assert_eq!(mr_cg_rating(0.01), Rating::Good);
        assert_eq!(mr_cg_rating(-0.04), Rating::Ok);
        assert_eq!(mr_cg_rating(0.2), Rating::Bad);
    }
}
