// SPDX-License-Identifier: AGPL-3.0-or-later
//! Regulatory class check tab (EASA / FAA / Turkey SHGM). Informational screening only; the
//! rules live in one table in `crate::regulation::rules`. Nothing here changes the project.
use super::components::{self, Status, section_header};
use super::theme::{self, space};
use super::widgets::{fmt, section};
use crate::calc::Model;
use crate::i18n::{t, tf};
use crate::project::Project;
use crate::regulation::rules::{Authority, LAST_CHECKED, ReqLevel, Tier};
use crate::regulation::{Criterion, DesignFacts, Failure, GroupResult, evaluate_all};
use egui::{RichText, Ui};

/// Mass as "249 g" below one kilogram, otherwise "4.00 kg".
fn fmt_mass(kg: f64) -> String {
    if kg.abs() < 1.0 {
        format!("{} g", fmt(kg * 1000.0, 0))
    } else {
        format!("{} kg", fmt(kg, 2))
    }
}

fn fmt_value(c: Criterion, v: f64) -> String {
    match c {
        Criterion::Mass => fmt_mass(v),
        Criterion::Speed => format!("{} m/s", fmt(v, 1)),
        Criterion::Energy => format!("{} J", fmt(v, 1)),
        Criterion::Dimension => format!("{} m", fmt(v, 2)),
    }
}

fn failure_text(f: &Failure) -> String {
    let crit = t(f.criterion.key());
    if f.above_max {
        tf(
            "regulation.fail.over_fmt",
            &[
                crit.to_string(),
                fmt_value(f.criterion, f.value),
                fmt_value(f.criterion, f.value - f.limit),
                fmt_value(f.criterion, f.limit),
            ],
        )
    } else {
        tf(
            "regulation.fail.under_fmt",
            &[
                crit.to_string(),
                fmt_value(f.criterion, f.value),
                fmt_value(f.criterion, f.limit),
            ],
        )
    }
}

fn verify_badge(ui: &mut Ui) {
    let tk = theme::tokens(ui);
    ui.label(
        RichText::new(t("regulation.verify"))
            .color(tk.warning)
            .strong(),
    );
}

/// Speed the user typed (m/s); 0 means "use the estimate". Kept in egui memory, not the project.
fn speed_override(ui: &mut Ui) -> Option<f64> {
    let id = egui::Id::unique("regulation_speed_override");
    let mut v: f64 = ui.data_mut(|d| d.get_temp(id)).unwrap_or(0.0);
    ui.horizontal(|ui| {
        ui.label(t("regulation.speed_override"));
        ui.add(
            egui::DragValue::new(&mut v)
                .range(0.0..=400.0)
                .speed(0.5)
                .suffix(" m/s"),
        );
        ui.label(
            RichText::new(t("regulation.speed_override_hint")).color(theme::tokens(ui).text_weak),
        );
    });
    ui.data_mut(|d| d.insert_temp(id, v));
    (v > 0.0).then_some(v)
}

fn facts_block(ui: &mut Ui, f: &DesignFacts) {
    let tk = theme::tokens(ui);
    egui::Grid::new("regulation_facts")
        .num_columns(2)
        .spacing([space::L, space::XS])
        .show(ui, |ui| {
            ui.label(t("regulation.fact.mtow"));
            ui.label(RichText::new(fmt_mass(f.mtow_kg)).strong());
            ui.end_row();
            ui.label(t("regulation.fact.speed"));
            ui.label(RichText::new(format!("{} m/s", fmt(f.v_max_ms, 1))).strong());
            ui.end_row();
            ui.label(t("regulation.fact.speed_source"));
            ui.label(RichText::new(t(f.speed_source.key())).color(tk.text_weak));
            ui.end_row();
            ui.label(t("regulation.fact.dimension"));
            ui.label(RichText::new(format!("{} m", fmt(f.dimension_m, 2))).strong());
            ui.end_row();
            ui.label(t("regulation.fact.energy"));
            ui.label(RichText::new(format!("{} J", fmt(f.energy_j, 1))).strong());
            ui.end_row();
            ui.label(t("regulation.fact.energy_cap"));
            ui.label(RichText::new(format!("{} J", fmt(f.energy_at_class_cap_j, 1))).strong());
            ui.end_row();
        });
    ui.label(
        RichText::new(tf(
            "regulation.fact.energy_refs_fmt",
            &[
                fmt(crate::regulation::rules::EASA_C1_ENERGY_J, 0),
                fmt(crate::regulation::rules::FAA_11_FTLBF_J, 1),
                fmt(crate::regulation::rules::FAA_25_FTLBF_J, 1),
            ],
        ))
        .small()
        .color(tk.text_weak),
    );
}

fn tier_name(tier: &Tier) -> String {
    t(tier.name_key).to_string()
}

fn group_block(ui: &mut Ui, r: &GroupResult, f: &DesignFacts) {
    let tk = theme::tokens(ui);
    ui.add_space(space::S);
    ui.label(RichText::new(t(r.group.title_key)).strong());
    match r.matched_tier() {
        Some(tier) => {
            ui.horizontal_wrapped(|ui| {
                ui.label(t("regulation.result"));
                ui.label(
                    RichText::new(tier_name(tier))
                        .strong()
                        .size(theme::font::SUBHEAD)
                        .color(tk.accent),
                );
                if tier.verify {
                    verify_badge(ui);
                }
            });
            ui.label(RichText::new(t(tier.desc_key)).color(tk.text_weak));
            // Margin to the next threshold and what would change the class.
            if let Some(head) = r.mass_headroom_kg(f) {
                let limit = tier.mass_lt_kg.unwrap_or(0.0);
                let mut line = tf(
                    "regulation.margin.under_fmt",
                    &[fmt_mass(head), fmt_mass(limit)],
                );
                if let Some(next) = r.next_tier() {
                    line.push(' ');
                    line.push_str(&tf("regulation.margin.next_fmt", &[tier_name(next)]));
                }
                ui.label(line);
            }
            ui.add_space(space::XS);
            for rq in tier.reqs {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(t(rq.kind.label_key())).strong());
                    let color = match rq.level {
                        ReqLevel::Required => tk.danger,
                        ReqLevel::Conditional => tk.warning,
                        ReqLevel::NotRequired => tk.success,
                        ReqLevel::Info => tk.info,
                    };
                    ui.label(RichText::new(t(rq.level.label_key())).color(color));
                    ui.label(t(rq.text_key));
                    if rq.verify {
                        verify_badge(ui);
                    }
                });
            }
            ui.label(
                RichText::new(tf(
                    "regulation.source_fmt",
                    &[tier.source.to_string(), tier.checked.to_string()],
                ))
                .small()
                .color(tk.text_weak),
            );
        }
        None => {
            ui.label(RichText::new(t(r.group.none_key)).color(tk.warning));
        }
    }
    // What blocks the tiers listed before the matched one.
    let blocked: Vec<_> = r.blocked_before_match().collect();
    if !blocked.is_empty() {
        egui::CollapsingHeader::new(t("regulation.change_hdr"))
            .id_salt(("regulation_change", r.group.id))
            .show(ui, |ui| {
                for b in blocked {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(tier_name(b.tier)).strong());
                        if b.tier.verify {
                            verify_badge(ui);
                        }
                    });
                    for fl in &b.failures {
                        ui.label(
                            RichText::new(format!("  - {}", failure_text(fl))).color(tk.text_weak),
                        );
                    }
                }
            });
    }
}

pub fn show(ui: &mut Ui, p: &Project, m: &Model) {
    section_header(ui, t("regulation.tab"));
    components::banner(ui, Status::Warn, t("regulation.disclaimer"));
    ui.label(tf("regulation.last_checked_fmt", &[LAST_CHECKED]));
    ui.add_space(space::XS);
    let ov = speed_override(ui);
    let Some(f) = DesignFacts::from_model(p, m, ov) else {
        components::banner(ui, Status::Info, t("regulation.no_data"));
        return;
    };
    section(ui, t("regulation.facts_hdr"));
    facts_block(ui, &f);
    let results = evaluate_all(&f);
    for a in Authority::ALL {
        section(ui, t(a.name_key()));
        for r in results.iter().filter(|r| r.group.authority == a) {
            group_block(ui, r, &f);
        }
    }
    ui.add_space(space::M);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Library, evaluate};
    use crate::i18n::Lang;
    use crate::project::DroneType;
    use crate::ui::theme::ThemeChoice;
    use egui::{Pos2, Rect, vec2};

    #[test]
    fn panel_renders_headless_in_both_themes_and_languages() {
        for dt in DroneType::ALL {
            let mut p = Project::default();
            p.set_drone_type(dt);
            let m = evaluate(&p, &Library::embedded());
            for choice in [ThemeChoice::Dark, ThemeChoice::Light] {
                for lang in [Lang::Tr, Lang::En] {
                    let ctx = egui::Context::default();
                    theme::install(&ctx, choice);
                    crate::i18n::with_lang(lang, || {
                        for _ in 0..2 {
                            let mut input = egui::RawInput::default();
                            input.screen_rect =
                                Some(Rect::from_min_size(Pos2::ZERO, vec2(1100.0, 800.0)));
                            let mut out = ctx.run_ui(input, |ui| show(ui, &p, &m));
                            out.textures_delta.clear();
                        }
                    });
                }
            }
        }
    }

    #[test]
    fn ui_source_keys_exist_in_both_languages() {
        let src = include_str!("regulation.rs");
        let prod = src.split("#[cfg(test)]").next().unwrap();
        let mut n = 0;
        for part in prod.split("\"regulation.").skip(1) {
            let end = part.find('"').unwrap();
            let key = format!("regulation.{}", &part[..end]);
            for table in [crate::i18n::tr_table(), crate::i18n::en_table()] {
                let v = table.get(key.as_str());
                assert!(
                    v.is_some_and(|s| !s.trim().is_empty()),
                    "missing i18n key {key}"
                );
            }
            n += 1;
        }
        assert!(n > 20);
    }

    #[test]
    fn fmt_mass_handles_sub_kg_and_negative_kg() {
        assert_eq!(fmt_mass(0.249), "249 g");
        assert_eq!(fmt_mass(4.0), "4.00 kg");
        assert_eq!(fmt_mass(-0.5), "-500 g");
        assert_eq!(fmt_mass(-1.5), "-1.50 kg");
    }
}
