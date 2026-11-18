// SPDX-License-Identifier: AGPL-3.0-or-later
//! Güvenilirlik (MTBF) sekmesi: bileşen tablosu, arıza oranı geçersiz kılma, görev süresi ve
//! sonuçlar. Hesap `crate::reliability` modülündedir; bu dosya yalnızca çizer ve girdileri okur.
use super::components::{self, Status, section_header};
use super::theme::space;
use super::widgets::fmt;
use crate::calc::Model;
use crate::i18n::t;
use crate::project::Project;
use crate::reliability::{self, ComponentKind, RateOverrides, ReliabilityError, Report};
use egui::Ui;
use std::collections::HashMap;

/// Sekmenin kalıcı olmayan girdi durumu (proje dosyasına yazılmaz).
#[derive(Default)]
pub struct ReliabilityUi {
    /// Tür başına arıza oranı metni. Boş = varsayılan.
    pub rate_text: HashMap<ComponentKind, String>,
    /// Görev süresi metni [saat]. Boş = tasarımın uçuş süresi.
    pub mission_text: String,
}

/// Görev süresi metnini ayrıştırır; boş metin için `None`.
fn parse_mission(text: &str) -> Option<f64> {
    text.trim().replace(',', ".").parse::<f64>().ok()
}

/// Arıza oranı metinlerini okur. Geçersiz bir girdi varsa `true` döner.
fn read_overrides(st: &ReliabilityUi) -> (RateOverrides, bool) {
    let mut overrides = RateOverrides::new();
    let mut bad = false;
    for kind in ComponentKind::ALL {
        let text = st.rate_text.get(&kind).map(String::as_str).unwrap_or("");
        match reliability::parse_rate(text) {
            Ok(Some(v)) => {
                overrides.insert(kind, v);
            }
            Ok(None) => {}
            Err(_) => bad = true,
        }
    }
    (overrides, bad)
}

pub fn show(ui: &mut Ui, p: &Project, m: &Model, st: &mut ReliabilityUi) {
    section_header(ui, t("tab_reliability"));
    components::banner(ui, Status::Info, t("reliability.disclaimer"));
    ui.add_space(space::XS);

    let counts = reliability::design_counts(p, m);
    let (overrides, bad_rate) = read_overrides(st);
    let items = reliability::build_items(&counts, &overrides);
    let default_hours = reliability::default_mission_hours(m);

    ui.horizontal(|ui| {
        ui.label(t("reliability.mission_time"));
        ui.add(
            egui::TextEdit::singleline(&mut st.mission_text)
                .desired_width(80.0)
                .hint_text(fmt(default_hours, 2)),
        );
        ui.label(t("reliability.unit_hours"));
    });
    ui.weak(t("reliability.mission_hint"));
    ui.add_space(space::S);

    let mission = if st.mission_text.trim().is_empty() {
        Some(default_hours)
    } else {
        parse_mission(&st.mission_text)
    };
    let result: Result<Report, ReliabilityError> = match mission {
        _ if bad_rate => Err(ReliabilityError::InvalidRate),
        None => Err(ReliabilityError::InvalidMissionTime),
        Some(hours) => reliability::evaluate(&items, counts.motor_redundant, hours),
    };

    section_header(ui, t("reliability.section_components"));
    egui::Grid::new("reliability_components")
        .striped(true)
        .num_columns(5)
        .show(ui, |ui| {
            ui.strong(t("reliability.col_component"));
            ui.strong(t("reliability.col_qty"));
            ui.strong(t("reliability.col_rate"));
            ui.strong(t("reliability.col_override"));
            ui.strong(t("reliability.col_share"));
            ui.end_row();

            for it in items.iter().filter(|i| i.qty > 0) {
                let kind = it.kind;
                ui.vertical(|ui| {
                    ui.label(t(kind.label_key()));
                    ui.weak(t(kind.note_key()));
                });
                ui.label(it.qty.to_string());
                ui.label(fmt(kind.default_rate(), 0));
                let entry = st.rate_text.entry(kind).or_default();
                ui.add(
                    egui::TextEdit::singleline(entry)
                        .desired_width(90.0)
                        .hint_text(fmt(kind.default_rate(), 0)),
                );
                let share = match &result {
                    Ok(rep) => rep
                        .contributions
                        .iter()
                        .find(|c| c.kind == kind)
                        .map(|c| format!("{}%", fmt(c.pct, 1)))
                        .unwrap_or_default(),
                    Err(_) => "-".to_string(),
                };
                ui.label(share);
                ui.end_row();
            }
        });
    ui.weak(t("reliability.rate_unit_note"));
    ui.add_space(space::S);

    section_header(ui, t("reliability.section_results"));
    match result {
        Ok(rep) => {
            egui::Grid::new("reliability_results")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label(t("reliability.result_mtbf"));
                    ui.strong(format!(
                        "{} {}",
                        fmt(rep.mtbf_h, 0),
                        t("reliability.unit_hours")
                    ));
                    ui.end_row();

                    ui.label(t("reliability.result_lambda"));
                    ui.label(fmt(rep.lambda_per_h * 1e6, 1));
                    ui.end_row();

                    ui.label(t("reliability.result_r_mission"));
                    ui.strong(format!("{}%", fmt(rep.r_mission * 100.0, 4)));
                    ui.end_row();

                    ui.label(t("reliability.result_r_series"));
                    ui.label(format!("{}%", fmt(rep.r_series * 100.0, 4)));
                    ui.end_row();

                    ui.label(t("reliability.result_r_motor"));
                    ui.label(format!("{}%", fmt(rep.r_motor_group * 100.0, 4)));
                    ui.end_row();

                    ui.label(t("reliability.result_p100"));
                    ui.label(format!("{}%", fmt(rep.p_fail_100_flights * 100.0, 3)));
                    ui.end_row();
                });
            let note = if counts.motor_redundant {
                t("reliability.note_redundant")
            } else {
                t("reliability.note_no_redundancy")
            };
            components::banner(ui, Status::Info, note);
        }
        Err(e) => {
            let msg = match e {
                ReliabilityError::InvalidRate => t("reliability.err_rate"),
                ReliabilityError::InvalidMissionTime => t("reliability.err_mission"),
            };
            components::banner(ui, Status::Err, msg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Library, evaluate};
    use crate::project::DroneType;
    use crate::ui::theme::{self, ThemeChoice};
    use egui::{Pos2, Rect, vec2};

    fn run(ctx: &egui::Context, f: &mut dyn FnMut(&mut Ui)) {
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, vec2(1100.0, 800.0)));
        let mut out = ctx.run_ui(input, |ui| f(ui));
        out.textures_delta.clear();
    }

    #[test]
    fn renders_for_every_type_in_both_themes() {
        for choice in [ThemeChoice::Dark, ThemeChoice::Light] {
            for dt in DroneType::ALL {
                let ctx = egui::Context::default();
                theme::install(&ctx, choice);
                let mut p = Project::default();
                p.set_drone_type(dt);
                let m = evaluate(&p, &Library::embedded());
                let mut st = ReliabilityUi::default();
                run(&ctx, &mut |ui| show(ui, &p, &m, &mut st));
            }
        }
    }

    #[test]
    fn renders_invalid_inputs_without_panic() {
        let ctx = egui::Context::default();
        theme::install(&ctx, ThemeChoice::Dark);
        let mut p = Project::default();
        p.set_drone_type(DroneType::Multirotor);
        let m = evaluate(&p, &Library::embedded());
        let mut st = ReliabilityUi::default();
        st.rate_text.insert(ComponentKind::Gps, "-3".into());
        st.mission_text = "abc".into();
        run(&ctx, &mut |ui| show(ui, &p, &m, &mut st));
        st.rate_text.clear();
        st.mission_text = "2,5".into();
        run(&ctx, &mut |ui| show(ui, &p, &m, &mut st));
    }

    #[test]
    fn overrides_text_is_parsed_and_bad_entries_flagged() {
        let mut st = ReliabilityUi::default();
        st.rate_text.insert(ComponentKind::Esc, "450,5".into());
        let (o, bad) = read_overrides(&st);
        assert!(!bad);
        assert_eq!(o.get(&ComponentKind::Esc), Some(&450.5));

        st.rate_text.insert(ComponentKind::Servo, "0".into());
        let (_, bad) = read_overrides(&st);
        assert!(bad);
    }
}
