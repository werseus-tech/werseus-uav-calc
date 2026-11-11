// SPDX-License-Identifier: AGPL-3.0-or-later
//! Sağ panel: canlı analiz. Yalnız hesap motorunun mevcut çıktılarını (`Model`) okur; yeni
//! formül yoktur. Kart renk eşikleri Özet ekranındaki eşiklerin aynısıdır.
//!
//! Kart içeriği saf fonksiyonla ([`cards`]) üretilir ve test edilir; çizim ayrıdır.
use crate::calc::{Model, Sev};
use crate::i18n::{t, tf};
use crate::project::{DroneType, Project};
use crate::teach::ResultMetric;
use crate::ui::components::format::{MISSING, fmt_duration_min, fmt_mass_kg, fmt_num, fmt_percent};
use crate::ui::components::{self, MetricCard, Status, card::card_grid};
use crate::ui::theme::{self, font, space};
use crate::ui::wizard::StepId;
use egui::{RichText, Ui};

/// Bir analiz kartının verisi (çizimden bağımsız).
#[derive(Clone, Debug, PartialEq)]
pub struct CardData {
    pub title: &'static str,
    pub value: String,
    pub unit: &'static str,
    pub detail: String,
    pub status: Status,
    pub why: Option<ResultMetric>,
    /// Karta tıklanınca gidilecek adım.
    pub step: Option<StepId>,
}

/// Özet ekranıyla aynı eşikler (yeniden kullanılsın diye fonksiyon).
pub fn rate_wing_loading(ws: f64) -> Status {
    if !ws.is_finite() {
        Status::Neutral
    } else if (45.0..=100.0).contains(&ws) {
        Status::Ok
    } else {
        Status::Warn
    }
}

pub fn rate_mr_tw(tw: f64) -> Status {
    if !tw.is_finite() {
        Status::Neutral
    } else if tw >= 2.0 {
        Status::Ok
    } else if tw >= 1.6 {
        Status::Warn
    } else {
        Status::Err
    }
}

pub fn rate_hover_throttle(x: f64) -> Status {
    if !x.is_finite() {
        Status::Neutral
    } else if x <= 0.50 {
        Status::Ok
    } else if x <= 0.60 {
        Status::Warn
    } else {
        Status::Err
    }
}

pub fn rate_vtol_tw(tw: f64) -> Status {
    if !tw.is_finite() {
        Status::Neutral
    } else if tw >= 1.6 {
        Status::Ok
    } else {
        Status::Err
    }
}

fn ok_status(ok: bool, value: f64) -> Status {
    if !value.is_finite() {
        Status::Neutral
    } else {
        Status::from_ok(ok)
    }
}

fn min_label() -> (&'static str, &'static str) {
    (t("ui.unit.h"), t("ui.unit.min"))
}

/// Araç türüne göre ana sonuç kartları.
pub fn cards(p: &Project, m: &Model) -> Vec<CardData> {
    let (h_l, min_l) = min_label();
    let mut v = Vec::new();
    if p.drone_type == DroneType::Multirotor {
        let mr = &m.mr;
        v.push(CardData {
            title: t("ui.card.mass"),
            value: fmt_mass_kg(mr.m_mtow),
            unit: "",
            detail: format!("W = {} N", fmt_num(mr.w, 1)),
            status: Status::Neutral,
            why: None,
            step: Some(StepId::Weight),
        });
        v.push(CardData {
            title: t("ui.card.hover_throttle"),
            value: fmt_percent(mr.hover_throttle, 0),
            unit: "",
            detail: tf("ui.card.hover_power_fmt", &[fmt_num(mr.p_total_hover, 0)]),
            status: rate_hover_throttle(mr.hover_throttle),
            why: Some(ResultMetric::HoverThrottle),
            step: Some(StepId::MrPropulsion),
        });
        v.push(CardData {
            title: t("ui.card.tw"),
            value: fmt_num(mr.tw_ratio, 2),
            unit: "",
            detail: format!("T_max = {} N", fmt_num(mr.t_max_total, 1)),
            status: rate_mr_tw(mr.tw_ratio),
            why: None,
            step: Some(StepId::MrPropulsion),
        });
        v.push(CardData {
            title: t("ui.card.flight_time"),
            value: fmt_duration_min(mr.hover_endurance_min, h_l, min_l),
            unit: "",
            detail: tf(
                "ui.card.cruise_time_fmt",
                &[fmt_duration_min(mr.cruise_endurance_min, h_l, min_l)],
            ),
            status: Status::Neutral,
            why: Some(ResultMetric::FlightTime),
            step: Some(StepId::MrBatteryLoiter),
        });
        v.push(CardData {
            title: t("ui.card.disk_loading"),
            value: fmt_num(mr.disk_loading, 0),
            unit: "N/m²",
            detail: tf("ui.card.prop_fmt", &[fmt_num(p.mr_d_prop / 0.0254, 1)]),
            status: if mr.prop_overlap {
                Status::Err
            } else if !mr.prop_clearance_ok {
                Status::Warn
            } else {
                Status::Neutral
            },
            why: Some(ResultMetric::DiskLoading),
            step: Some(StepId::MrFrame),
        });
        v.push(CardData {
            title: t("ui.card.cg_offset"),
            value: fmt_num(mr.cg_offset_d * 1000.0, 1),
            unit: "mm",
            detail: tf(
                "ui.card.cg_offset_fmt",
                &[fmt_num(mr.cg_offset_frac * 100.0, 1)],
            ),
            status: if !mr.cg_offset_d.is_finite() {
                Status::Neutral
            } else if mr.cg_offset_ok {
                Status::Ok
            } else {
                Status::Warn
            },
            why: None,
            step: Some(StepId::MrCG),
        });
        return v;
    }

    let w = &m.weight;
    let c = &m.cons;
    v.push(CardData {
        title: t("ui.card.mass"),
        value: fmt_mass_kg(w.m_mtow),
        unit: "",
        detail: format!("W = {} N", fmt_num(w.w, 1)),
        status: Status::Neutral,
        why: None,
        step: Some(StepId::Weight),
    });
    v.push(CardData {
        title: t("ui.card.wing_loading"),
        value: fmt_num(m.ws, 1),
        unit: "N/m²",
        detail: format!("{} kg/m²", fmt_num(m.ws / 9.80665, 2)),
        status: rate_wing_loading(m.ws),
        why: Some(ResultMetric::WingLoading),
        step: Some(StepId::Geometry),
    });
    v.push(CardData {
        title: t("ui.card.stall"),
        value: fmt_num(c.v_stall, 1),
        unit: "m/s",
        detail: tf(
            "ui.card.stall_fmt",
            &[fmt_num(c.v_stall * 3.6, 0), fmt_num(p.v_stall_req, 0)],
        ),
        status: ok_status(c.stall_ok, c.v_stall),
        why: Some(ResultMetric::VStall),
        step: Some(StepId::DesignPoint),
    });
    v.push(CardData {
        title: t("ui.card.ld"),
        value: fmt_num(m.aero.ld_max, 1),
        unit: "",
        detail: tf("ui.card.ld_cruise_fmt", &[fmt_num(m.perf_cr.ld, 1)]),
        status: Status::Neutral,
        why: Some(ResultMetric::LdMax),
        step: Some(StepId::Aero),
    });
    v.push(CardData {
        title: t("ui.card.endurance"),
        value: fmt_duration_min(m.perf_end.endurance_min(), h_l, min_l),
        unit: "",
        detail: tf("ui.card.range_fmt", &[fmt_num(m.perf_cr.range_km(), 0)]),
        status: Status::Neutral,
        why: Some(ResultMetric::FlightTime),
        step: Some(StepId::Endurance),
    });
    v.push(CardData {
        title: t("ui.card.static_margin"),
        value: fmt_percent(m.stab.sm, 1),
        unit: "MAC",
        detail: tf(
            "ui.card.cg_np_fmt",
            &[
                fmt_num(m.stab.x_cg_rel / m.wing.mac * 100.0, 0),
                fmt_num(m.stab.np.x_np / m.wing.mac * 100.0, 0),
            ],
        ),
        status: ok_status(m.stab.in_band, m.stab.sm),
        why: Some(ResultMetric::StaticMargin),
        step: Some(StepId::Stability),
    });
    if p.drone_type == DroneType::VtolQuadplane {
        v.push(CardData {
            title: t("ui.card.vtol_tw"),
            value: fmt_num(m.mr.tw_ratio, 2),
            unit: "",
            detail: format!("T_max = {} N", fmt_num(m.mr.t_max_total, 1)),
            status: rate_vtol_tw(m.mr.tw_ratio),
            why: None,
            step: Some(StepId::MrFrame),
        });
        v.push(CardData {
            title: t("ui.card.vtol_hover"),
            value: fmt_duration_min(m.mr.hover_endurance_min, h_l, min_l),
            unit: "",
            detail: tf("ui.card.hover_power_fmt", &[fmt_num(m.mr.p_total_hover, 0)]),
            status: Status::Neutral,
            why: Some(ResultMetric::FlightTime),
            step: None,
        });
    }
    v
}

/// Uyarı sayıları (hata, uyarı, not).
pub fn warning_counts(m: &Model) -> (usize, usize, usize) {
    let mut c = (0, 0, 0);
    for w in &m.warnings {
        match w.sev {
            Sev::Err => c.0 += 1,
            Sev::Warn => c.1 += 1,
            Sev::Info => c.2 += 1,
        }
    }
    c
}

/// Uyarılar önem sırasıyla (hata → uyarı → not), aynı önemde özgün sıra korunur.
pub fn sorted_warnings(m: &Model) -> Vec<&crate::calc::Warn> {
    let mut ws: Vec<_> = m.warnings.iter().collect();
    ws.sort_by_key(|w| match w.sev {
        Sev::Err => 0,
        Sev::Warn => 1,
        Sev::Info => 2,
    });
    ws
}

/// Sağ panelden çıkan istekler (kabuk uygular).
#[derive(Default)]
pub struct AnalysisActions {
    pub go_to: Option<StepId>,
    pub why: Option<ResultMetric>,
    pub fix_warning: Option<String>,
}

/// Uyarı satırları: tıklayınca ilgili adıma gider; sağ tık menüsünde "adıma git" ve "YZ ile
/// düzelt". `ai_buttons` açıksa (Özet adımı) her satırın altında görünür bir "YZ ile düzelt"
/// düğmesi de vardır (eski Özet ekranındaki düğmenin karşılığı).
pub fn warning_list(
    ui: &mut Ui,
    p: &Project,
    list: &[&crate::calc::Warn],
    ai_buttons: bool,
) -> AnalysisActions {
    let mut act = AnalysisActions::default();
    for wn in list {
        let r = components::warning_row(ui, wn);
        let target = super::nav::step_for_warning(wn.area_key, wn.msg_key, p.drone_type);
        let r = if target.is_some() {
            r.on_hover_text(t("ui.analysis.warning_tip"))
        } else {
            r
        };
        if r.clicked() {
            act.go_to = target;
        }
        let fix = format!("[{}] {}", wn.area_name(), wn.text());
        r.context_menu(|ui| {
            if target.is_some() && ui.button(t("ui.analysis.go_to_step")).clicked() {
                act.go_to = target;
                ui.close();
            }
            if ui.button(t("ai.btn_fix_warning")).clicked() {
                act.fix_warning = Some(fix.clone());
                ui.close();
            }
        });
        if ai_buttons && ui.small_button(t("ai.btn_fix_warning")).clicked() {
            act.fix_warning = Some(fix);
        }
    }
    act
}

/// Sağ panelin canlı analiz bölümü: ana kartlar ve uyarılar. `shown_areas`: adım bölümünde zaten
/// listelenen uyarı alanları (burada tekrar edilmez). `ai_buttons`: satır altı "YZ ile düzelt".
pub fn show(
    ui: &mut Ui,
    p: &Project,
    m: &Model,
    show_info: &mut bool,
    shown_areas: &[&str],
    ai_buttons: bool,
) -> AnalysisActions {
    let tk = theme::tokens(ui);
    let mut act = AnalysisActions::default();

    ui.add_space(space::S);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(t("ui.analysis.title"))
                .size(font::HEADING)
                .strong()
                .color(tk.text),
        );
    });
    ui.label(
        RichText::new(p.drone_type.label())
            .size(font::CAPTION)
            .color(tk.text_weak),
    );
    ui.add_space(space::S);

    let data = cards(p, m);
    card_grid(ui, data.len(), 128.0, |ui, i, w| {
        let d = &data[i];
        let card = MetricCard {
            title: d.title,
            value: if d.value.is_empty() {
                MISSING.into()
            } else {
                d.value.clone()
            },
            unit: d.unit,
            detail: d.detail.clone(),
            status: d.status,
            why: d.why.is_some(),
            tooltip: if d.step.is_some() {
                t("ui.card.go_to_step")
            } else {
                ""
            },
            clickable: d.step.is_some(),
        };
        let out = components::metric_card(ui, &card, w);
        if out.why_clicked {
            act.why = d.why;
        } else if out.response.clicked() {
            act.go_to = d.step;
        }
    });

    // Uyarılar
    let (e, w, n) = warning_counts(m);
    components::section::section_header_with(ui, t("ui.analysis.warnings"), |ui| {
        if n > 0 {
            ui.toggle_value(show_info, tf("ui.analysis.notes_fmt", &[n.to_string()]));
        }
    });
    ui.horizontal_wrapped(|ui| {
        if e > 0 {
            components::badge(
                ui,
                Status::Err,
                &tf("ui.analysis.err_fmt", &[e.to_string()]),
            );
        }
        if w > 0 {
            components::badge(
                ui,
                Status::Warn,
                &tf("ui.analysis.warn_fmt", &[w.to_string()]),
            );
        }
    });
    let visible: Vec<_> = sorted_warnings(m)
        .into_iter()
        .filter(|x| *show_info || x.sev != Sev::Info)
        .collect();
    let list: Vec<_> = visible
        .iter()
        .copied()
        .filter(|x| !shown_areas.contains(&x.area_key))
        .collect();
    if visible.is_empty() {
        components::banner(ui, Status::Ok, t("ui.analysis.all_clear"));
    } else if list.is_empty() {
        components::banner(ui, Status::Info, t("ui.analysis.rest_shown_above"));
    }
    let a = warning_list(ui, p, &list, ai_buttons);
    if a.go_to.is_some() {
        act.go_to = a.go_to;
    }
    if a.fix_warning.is_some() {
        act.fix_warning = a.fix_warning;
    }
    ui.add_space(space::S);
    ui.label(
        RichText::new(t("ui.analysis.disclaimer"))
            .size(font::CAPTION)
            .color(tk.text_weak),
    );
    act
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(dt: DroneType) -> (Project, Model) {
        let mut p = Project::default();
        p.set_drone_type(dt);
        let lib = crate::calc::Library::embedded();
        let m = crate::calc::evaluate(&p, &lib);
        (p, m)
    }

    #[test]
    fn cards_per_type() {
        for dt in DroneType::ALL {
            let (p, m) = eval(dt);
            let c = cards(&p, &m);
            match dt {
                DroneType::Multirotor => assert_eq!(c.len(), 6),
                DroneType::VtolQuadplane => assert_eq!(c.len(), 8),
                _ => assert_eq!(c.len(), 6),
            }
            // Varsayılan projede değerler dolu (— değil) ve kütle kartı ilk sırada.
            assert_eq!(c[0].title, t("ui.card.mass"));
            for card in &c {
                assert!(!card.value.is_empty());
                assert_ne!(card.value, MISSING, "{dt:?} {}", card.title);
            }
        }
    }

    #[test]
    fn mass_card_matches_model() {
        let (p, m) = eval(DroneType::FixedWingConventional);
        let c = cards(&p, &m);
        assert_eq!(c[0].value, fmt_mass_kg(m.weight.m_mtow));
        let (p, m) = eval(DroneType::Multirotor);
        let c = cards(&p, &m);
        assert_eq!(c[0].value, fmt_mass_kg(m.mr.m_mtow));
    }

    #[test]
    fn invalid_inputs_never_panic_and_show_missing() {
        let mut p = Project::default();
        p.s = 0.0;
        p.b = 0.0;
        p.v_cr = f64::NAN;
        let lib = crate::calc::Library::embedded();
        let m = crate::calc::evaluate(&p, &lib);
        let c = cards(&p, &m);
        assert_eq!(c.len(), 6);
    }

    #[test]
    fn thresholds_match_summary_screen() {
        assert_eq!(rate_wing_loading(60.0), Status::Ok);
        assert_eq!(rate_wing_loading(120.0), Status::Warn);
        assert_eq!(rate_mr_tw(2.1), Status::Ok);
        assert_eq!(rate_mr_tw(1.7), Status::Warn);
        assert_eq!(rate_mr_tw(1.2), Status::Err);
        assert_eq!(rate_hover_throttle(0.45), Status::Ok);
        assert_eq!(rate_hover_throttle(0.55), Status::Warn);
        assert_eq!(rate_hover_throttle(0.7), Status::Err);
        assert_eq!(rate_vtol_tw(1.6), Status::Ok);
        assert_eq!(rate_vtol_tw(f64::NAN), Status::Neutral);
    }

    #[test]
    fn warnings_sorted_by_severity() {
        let mut p = Project::default();
        p.m_avionics = 0.0;
        p.s = 0.02;
        let lib = crate::calc::Library::embedded();
        let m = crate::calc::evaluate(&p, &lib);
        let ws = sorted_warnings(&m);
        let ranks: Vec<u8> = ws.iter().map(|w| Status::from_sev(w.sev).rank()).collect();
        assert!(ranks.windows(2).all(|x| x[0] >= x[1]));
        let (e, w, n) = warning_counts(&m);
        assert_eq!(e + w + n, m.warnings.len());
    }
}
