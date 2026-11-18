// SPDX-License-Identifier: AGPL-3.0-or-later
//! Tasarım tahmini ile gerçek log ölçümünün karşılaştırması.

use super::segments::{Analysis, SegKind, Segment};
use crate::calc::{Library, Model, evaluate, performance};
use crate::project::Project;

/// Tasarımın tahminleri (yoksa None).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DesignPrediction {
    pub hover_power_w: Option<f64>,
    pub hover_current_a: Option<f64>,
    /// Ölçülen seyir hızındaki tahmini elektrik gücü.
    pub cruise_power_w: Option<f64>,
    pub hover_endurance_min: Option<f64>,
    /// Ölçülen seyir hızındaki tahmini uçuş süresi.
    pub cruise_endurance_min: Option<f64>,
    /// Kullanılabilir pil enerjisi (Wh).
    pub usable_wh: f64,
    pub pack_voltage: f64,
}

/// Tasarım tahminleri; `cruise_v` ölçülen seyir hızıdır (None ise tasarım seyir hızı).
pub fn design_prediction(
    p: &Project,
    m: &Model,
    lib: &Library,
    cruise_v: Option<f64>,
) -> DesignPrediction {
    let mut d = DesignPrediction::default();
    if p.drone_type.is_multirotor() {
        d.hover_power_w = Some(m.mr.p_total_hover).filter(|x| *x > 0.0);
        d.hover_endurance_min = Some(m.mr.hover_endurance_min).filter(|x| *x > 0.0);
        d.usable_wh = m.mr.e_usable_wh;
        d.hover_current_a =
            m.mr.i_hover
                .is_finite()
                .then_some(m.mr.i_hover)
                .filter(|x| *x > 0.0);
        d.pack_voltage = m.weight.pack_voltage;
        if let Some(v) = cruise_v.filter(|v| *v > 0.5) {
            let mut p2 = p.clone();
            p2.mr_v_cruise = v;
            let m2 = evaluate(&p2, lib);
            d.cruise_power_w = Some(m2.mr.p_total_cruise).filter(|x| *x > 0.0);
            d.cruise_endurance_min = Some(m2.mr.cruise_endurance_min).filter(|x| *x > 0.0);
        } else {
            d.cruise_power_w = Some(m.mr.p_total_cruise).filter(|x| *x > 0.0);
            d.cruise_endurance_min = Some(m.mr.cruise_endurance_min).filter(|x| *x > 0.0);
        }
    } else {
        let v = cruise_v.filter(|v| *v > 0.5).unwrap_or(m.perf_cr.v);
        let pt = performance::at_speed(v, &m.perf_in);
        d.cruise_power_w = Some(pt.p_total).filter(|x| x.is_finite() && *x > 0.0);
        d.cruise_endurance_min = Some(pt.endurance_min()).filter(|x| x.is_finite() && *x > 0.0);
        d.usable_wh = performance::e_usable_j(&m.perf_in) / 3600.0;
        d.pack_voltage = m.weight.pack_voltage;
    }
    d
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Metric {
    HoverPower,
    HoverCurrent,
    CruisePower,
    FlightTime,
}

/// Karşılaştırma satırı.
#[derive(Clone, Copy, Debug)]
pub struct Row {
    pub metric: Metric,
    pub predicted: f64,
    pub measured: f64,
    /// (ölçülen − tahmin) / tahmin × 100.
    pub diff_pct: f64,
}

/// Yüzde fark; tahmin 0/NaN ise None.
pub fn diff_pct(measured: f64, predicted: f64) -> Option<f64> {
    if measured.is_finite() && predicted.is_finite() && predicted.abs() > 1e-12 {
        Some((measured - predicted) / predicted * 100.0)
    } else {
        None
    }
}

/// Süre ağırlıklı kesim ortalaması.
fn weighted(segs: &[&Segment], f: impl Fn(&Segment) -> f64) -> f64 {
    let (mut s, mut w) = (0.0, 0.0);
    for g in segs {
        let x = f(g);
        if x.is_finite() {
            s += x * g.duration();
            w += g.duration();
        }
    }
    if w > 0.0 { s / w } else { f64::NAN }
}

pub fn by_kind(a: &Analysis, kind: SegKind) -> Vec<&Segment> {
    a.segments.iter().filter(|s| s.kind == kind).collect()
}

/// Ölçülen seyir hızı (süre ağırlıklı).
pub fn measured_cruise_speed(a: &Analysis) -> Option<f64> {
    let v = weighted(&by_kind(a, SegKind::Cruise), |s| s.mean_speed);
    v.is_finite().then_some(v)
}

/// Karşılaştırma satırları.
pub fn compare(a: &Analysis, d: &DesignPrediction) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut push = |metric, predicted: Option<f64>, measured: f64| {
        if let Some(pr) = predicted {
            if let Some(df) = diff_pct(measured, pr) {
                rows.push(Row {
                    metric,
                    predicted: pr,
                    measured,
                    diff_pct: df,
                });
            }
        }
    };
    let hov = by_kind(a, SegKind::Hover);
    let cru = by_kind(a, SegKind::Cruise);
    let hp = weighted(&hov, |s| s.mean_p);
    push(Metric::HoverPower, d.hover_power_w, hp);
    push(
        Metric::HoverCurrent,
        d.hover_current_a,
        weighted(&hov, |s| s.mean_i),
    );
    let cp = weighted(&cru, |s| s.mean_p);
    push(Metric::CruisePower, d.cruise_power_w, cp);
    // Uçuş süresi: tasarımın kullanılabilir enerjisi / ölçülen ortalama güç.
    let hover_t: f64 = hov.iter().map(|s| s.duration()).sum();
    let cruise_t: f64 = cru.iter().map(|s| s.duration()).sum();
    let p_mean = if hover_t >= cruise_t && hp.is_finite() {
        hp
    } else if cp.is_finite() {
        cp
    } else {
        a.overall.active_mean_p
    };
    let predicted_min = if hover_t >= cruise_t && hp.is_finite() {
        d.hover_endurance_min
    } else {
        d.cruise_endurance_min
    };
    if p_mean.is_finite() && p_mean > 0.0 && d.usable_wh > 0.0 {
        push(
            Metric::FlightTime,
            predicted_min,
            d.usable_wh / p_mean * 60.0,
        );
    }
    rows
}

/// Yorum anahtarı: "ok" | "high" | "low" (±%10 eşik).
pub fn verdict(diff_pct: f64) -> &'static str {
    if diff_pct > 10.0 {
        "high"
    } else if diff_pct < -10.0 {
        "low"
    } else {
        "ok"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::Library;
    use crate::flightlog::segments::{Analysis, Overall};
    use crate::project::DroneType;

    fn seg(kind: SegKind, p: f64, v: f64, dur: f64) -> Segment {
        Segment {
            kind,
            t0: 0.0,
            t1: dur,
            mean_v: 12.0,
            mean_i: p / 12.0,
            mean_p: p,
            mean_thr: f64::NAN,
            mean_speed: v,
            mean_alt: f64::NAN,
            mean_rpm: f64::NAN,
        }
    }

    #[test]
    fn percent_math() {
        assert!((diff_pct(118.0, 100.0).unwrap() - 18.0).abs() < 1e-9);
        assert!((diff_pct(80.0, 100.0).unwrap() + 20.0).abs() < 1e-9);
        assert!(diff_pct(1.0, 0.0).is_none());
        assert!(diff_pct(f64::NAN, 1.0).is_none());
        assert_eq!(verdict(18.0), "high");
        assert_eq!(verdict(-25.0), "low");
        assert_eq!(verdict(3.0), "ok");
    }

    #[test]
    fn compare_rows_and_flight_time() {
        let a = Analysis {
            overall: Overall::default(),
            segments: vec![
                seg(SegKind::Hover, 118.0, 0.0, 30.0),
                seg(SegKind::Cruise, 90.0, 12.0, 10.0),
            ],
            resistance: None,
        };
        let d = DesignPrediction {
            hover_power_w: Some(100.0),
            hover_current_a: Some(118.0 / 12.0 / 1.18),
            cruise_power_w: Some(100.0),
            hover_endurance_min: Some(10.0),
            cruise_endurance_min: Some(12.0),
            usable_wh: 20.0,
            pack_voltage: 12.0,
        };
        let r = compare(&a, &d);
        let hp = r.iter().find(|x| x.metric == Metric::HoverPower).unwrap();
        assert!((hp.diff_pct - 18.0).abs() < 1e-9);
        let cp = r.iter().find(|x| x.metric == Metric::CruisePower).unwrap();
        assert!((cp.diff_pct + 10.0).abs() < 1e-9);
        // Askı baskın: ölçülen süre = 20 Wh / 118 W · 60 = 10.17 dk; tahmin 10 dk.
        let ft = r.iter().find(|x| x.metric == Metric::FlightTime).unwrap();
        assert!((ft.measured - 20.0 / 118.0 * 60.0).abs() < 1e-9);
        assert!((ft.diff_pct - (ft.measured - 10.0) / 10.0 * 100.0).abs() < 1e-9);
        assert!((measured_cruise_speed(&a).unwrap() - 12.0).abs() < 1e-9);
    }

    #[test]
    fn design_prediction_is_filled_for_both_families() {
        let lib = Library::embedded();
        for dt in [DroneType::Multirotor, DroneType::FixedWingConventional] {
            let mut p = Project::default();
            p.set_drone_type(dt);
            let m = evaluate(&p, &lib);
            let d = design_prediction(&p, &m, &lib, Some(14.0));
            assert!(d.cruise_power_w.is_some(), "{dt:?}");
            assert!(d.usable_wh > 0.0, "{dt:?}");
            assert_eq!(d.hover_power_w.is_some(), dt == DroneType::Multirotor);
        }
    }
}
