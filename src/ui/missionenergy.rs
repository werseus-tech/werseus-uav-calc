// SPDX-License-Identifier: AGPL-3.0-or-later
//! Ortam ve görev enerji bütçesi sekmesi. Girdiler egui belleğinde durur (proje dosyası
//! değişmez); hesap `missionenergy` modülündedir, burada yalnızca giriş ve çizim vardır.
use super::components::{self, Status, section_header};
use super::theme::{self, font, space};
use super::widgets::{fmt, section};
use crate::calc::Model;
use crate::i18n::{t, tf};
use crate::missionenergy::{
    Env, Kind, Leg, LegKind, MissionResult, TempMode, Vehicle, battery_derate, default_legs,
    simulate,
};
use crate::project::Project;
use egui::{Align2, FontId, Id, Rect, Sense, Stroke, Ui, pos2, vec2};

#[derive(Clone)]
struct MeState {
    env: Env,
    legs: Vec<Leg>,
    /// Proje DoD'sine ek rezerv [%].
    reserve_pct: f64,
    kind: Kind,
    prev_v_cruise: f64,
    prev_payload_kg: f64,
}

fn kind_label(k: LegKind) -> &'static str {
    match k {
        LegKind::Climb => t("missionenergy.leg.climb"),
        LegKind::Cruise => t("missionenergy.leg.cruise"),
        LegKind::Loiter => t("missionenergy.leg.loiter"),
        LegKind::Descent => t("missionenergy.leg.descent"),
        LegKind::Landing => t("missionenergy.leg.landing"),
        LegKind::Drop => t("missionenergy.leg.drop"),
        LegKind::Transition => t("missionenergy.leg.transition"),
    }
}

fn kind_offered(veh: &Vehicle, k: LegKind) -> bool {
    k != LegKind::Transition || veh.kind == Kind::Vtol
}

fn new_state(p: &Project, veh: &Vehicle) -> MeState {
    MeState {
        env: Env {
            alt_msl_m: if p.h.is_finite() { p.h.max(0.0) } else { 0.0 },
            ..Env::default()
        },
        legs: default_legs(veh),
        reserve_pct: 0.0,
        kind: veh.kind,
        prev_v_cruise: veh.v_cruise,
        prev_payload_kg: veh.payload_kg,
    }
}

fn sync_leg_defaults(legs: &mut [Leg], old_v: f64, new_v: f64, old_pay: f64, new_pay: f64) {
    if (old_v - new_v).abs() > 1e-9 {
        for leg in legs.iter_mut() {
            if (leg.airspeed_ms - old_v).abs() < 1e-6 {
                leg.airspeed_ms = new_v;
            }
        }
    }
    if (old_pay - new_pay).abs() > 1e-9 {
        for leg in legs.iter_mut() {
            if leg.kind == LegKind::Drop && (leg.drop_kg - old_pay).abs() < 1e-6 {
                leg.drop_kg = new_pay;
            }
        }
    }
}

fn drag(ui: &mut Ui, v: &mut f64, speed: f64, lo: f64, hi: f64, suffix: &str) -> bool {
    ui.add(
        egui::DragValue::new(v)
            .speed(speed)
            .range(lo..=hi)
            .suffix(suffix),
    )
    .changed()
}

pub fn show(ui: &mut Ui, p: &Project, m: &Model) {
    let veh = Vehicle::from_model(p, m);
    let id = Id::unique("missionenergy_state");
    let mut st = ui
        .ctx()
        .data_mut(|d| d.get_temp::<MeState>(id))
        .unwrap_or_else(|| new_state(p, &veh));
    if st.kind != veh.kind {
        st.legs = default_legs(&veh);
        st.kind = veh.kind;
    } else {
        sync_leg_defaults(
            &mut st.legs,
            st.prev_v_cruise,
            veh.v_cruise,
            st.prev_payload_kg,
            veh.payload_kg,
        );
    }
    st.prev_v_cruise = veh.v_cruise;
    st.prev_payload_kg = veh.payload_kg;
    let tk = theme::tokens(ui);

    section_header(ui, t("tab_missionenergy"));
    components::banner(ui, Status::Info, t("missionenergy.disclaimer"));
    ui.add_space(space::XS);

    env_inputs(ui, &mut st.env);
    env_effects(ui, &veh, &st.env);

    section(ui, t("missionenergy.mission.title"));
    ui.horizontal(|ui| {
        ui.label(t("missionenergy.reserve"));
        drag(ui, &mut st.reserve_pct, 0.5, 0.0, 80.0, " %");
    });
    ui.label(
        egui::RichText::new(tf(
            "missionenergy.reserve_help",
            &[fmt(veh.dod * 100.0, 0), fmt(veh.pack_wh, 1)],
        ))
        .color(tk.text_weak)
        .size(font::CAPTION),
    );
    leg_editor(ui, &veh, &mut st.legs);

    let res = simulate(&veh, &st.env, &st.legs, st.reserve_pct / 100.0);
    results_table(ui, &res);
    summary(ui, &res);
    chart(ui, &res);

    ui.ctx().data_mut(|d| d.insert_temp(id, st));
}

fn env_inputs(ui: &mut Ui, env: &mut Env) {
    section(ui, t("missionenergy.env.title"));
    egui::Grid::new("me_env")
        .num_columns(2)
        .spacing([space::M, space::XS])
        .show(ui, |ui| {
            ui.label(t("missionenergy.env.alt"));
            drag(ui, &mut env.alt_msl_m, 10.0, 0.0, 6000.0, " m");
            ui.end_row();

            ui.label(t("missionenergy.env.temp_mode"));
            ui.horizontal(|ui| {
                ui.selectable_value(
                    &mut env.temp_mode,
                    TempMode::IsaOffset,
                    t("missionenergy.env.temp_isa"),
                );
                ui.selectable_value(
                    &mut env.temp_mode,
                    TempMode::Absolute,
                    t("missionenergy.env.temp_abs"),
                );
            });
            ui.end_row();

            match env.temp_mode {
                TempMode::IsaOffset => {
                    ui.label(t("missionenergy.env.isa_offset"));
                    drag(ui, &mut env.isa_offset_k, 0.5, -60.0, 60.0, " K");
                }
                TempMode::Absolute => {
                    ui.label(t("missionenergy.env.temp_abs_c"));
                    drag(ui, &mut env.temp_abs_c, 0.5, -50.0, 60.0, " °C");
                }
            }
            ui.end_row();

            ui.label(t("missionenergy.env.wind"));
            drag(ui, &mut env.wind_speed, 0.1, 0.0, 40.0, " m/s");
            ui.end_row();

            ui.label(t("missionenergy.env.wind_dir"));
            drag(ui, &mut env.wind_from_deg, 1.0, 0.0, 360.0, "°");
            ui.end_row();

            ui.label(t("missionenergy.env.gust"));
            drag(ui, &mut env.gust, 0.1, 0.0, 20.0, " m/s");
            ui.end_row();

            ui.label(t("missionenergy.env.batt_override"));
            ui.horizontal(|ui| {
                ui.checkbox(&mut env.batt_override, "");
                if env.batt_override {
                    drag(ui, &mut env.batt_temp_c, 0.5, -40.0, 60.0, " °C");
                }
            });
            ui.end_row();
        });
}

fn env_effects(ui: &mut Ui, veh: &Vehicle, env: &Env) {
    let tk = theme::tokens(ui);
    section(ui, t("missionenergy.eff.title"));
    let rho = env.rho_at(env.alt_msl_m);
    let rho0 = crate::calc::atmosphere::RHO0;
    ui.label(tf(
        "missionenergy.eff.density",
        &[
            fmt(env.density_alt_m(), 0),
            fmt(rho, 3),
            fmt((rho / rho0 - 1.0) * 100.0, 1),
        ],
    ));
    let pct = |a: f64, b: f64| fmt((a / b - 1.0) * 100.0, 1);
    if let Some(h_env) = veh.hover_w(rho, 0.0) {
        let h0 = veh.hover_w(rho0, 0.0).unwrap_or(f64::NAN);
        ui.label(tf(
            "missionenergy.eff.hover",
            &[fmt(h_env, 0), fmt(h0, 0), pct(h_env, h0)],
        ));
    } else {
        ui.label(egui::RichText::new(t("missionenergy.eff.no_hover")).color(tk.text_weak));
    }
    let c_env = veh.level_w(rho, 0.0, veh.v_cruise);
    let c0 = veh.level_w(rho0, 0.0, veh.v_cruise);
    ui.label(tf(
        "missionenergy.eff.cruise",
        &[
            fmt(veh.v_cruise, 1),
            fmt(c_env, 0),
            fmt(c0, 0),
            pct(c_env, c0),
        ],
    ));
    let d = battery_derate(env.battery_temp_c());
    ui.label(tf(
        "missionenergy.eff.battery",
        &[
            fmt(env.battery_temp_c(), 0),
            fmt(d.capacity * 100.0, 0),
            fmt(d.resistance, 2),
            fmt(d.energy_factor * 100.0, 0),
        ],
    ));
    ui.label(
        egui::RichText::new(t("missionenergy.eff.curve_note"))
            .color(tk.text_weak)
            .size(font::CAPTION),
    );
}

fn leg_editor(ui: &mut Ui, veh: &Vehicle, legs: &mut Vec<Leg>) {
    let mut remove = None;
    let mut swap_up = None;
    let n = legs.len();
    let mut in_hover = veh.has_hover();
    egui::Grid::new("me_legs")
        .num_columns(4)
        .spacing([space::S, space::XS])
        .show(ui, |ui| {
            for (i, leg) in legs.iter_mut().enumerate() {
                ui.label(format!("{}", i + 1));
                let mut kind = leg.kind;
                egui::ComboBox::from_id_salt(("me_kind", i))
                    .selected_text(kind_label(kind))
                    .show_ui(ui, |ui| {
                        for k in LegKind::ALL {
                            if kind_offered(veh, k) {
                                ui.selectable_value(&mut kind, k, kind_label(k));
                            }
                        }
                    });
                if kind != leg.kind {
                    *leg = Leg::new(kind, veh);
                }
                ui.horizontal(|ui| leg_params(ui, leg, !in_hover));
                if veh.kind == Kind::Vtol && leg.kind == LegKind::Transition {
                    in_hover = !in_hover;
                }
                ui.horizontal(|ui| {
                    if ui.add_enabled(i > 0, egui::Button::new("↑")).clicked() {
                        swap_up = Some(i);
                    }
                    if ui.add_enabled(i + 1 < n, egui::Button::new("↓")).clicked() {
                        swap_up = Some(i + 1);
                    }
                    if ui
                        .button("×")
                        .on_hover_text(t("missionenergy.remove"))
                        .clicked()
                    {
                        remove = Some(i);
                    }
                });
                ui.end_row();
            }
        });
    if let Some(i) = swap_up {
        legs.swap(i - 1, i);
    }
    if let Some(i) = remove {
        legs.remove(i);
    }
    ui.horizontal(|ui| {
        ui.label(t("missionenergy.add"));
        for k in LegKind::ALL {
            if kind_offered(veh, k) && ui.small_button(kind_label(k)).clicked() {
                legs.push(Leg::new(k, veh));
            }
        }
    });
}

fn leg_params(ui: &mut Ui, leg: &mut Leg, wing_borne: bool) {
    match leg.kind {
        LegKind::Climb | LegKind::Descent => {
            ui.label(t("missionenergy.p.height"));
            drag(ui, &mut leg.height_m, 1.0, 0.0, 5000.0, " m");
            ui.label(t("missionenergy.p.rate"));
            drag(ui, &mut leg.rate_ms, 0.1, 0.1, 20.0, " m/s");
            if wing_borne {
                ui.label(t("missionenergy.p.airspeed"));
                drag(ui, &mut leg.airspeed_ms, 0.2, 1.0, 80.0, " m/s");
            }
        }
        LegKind::Cruise => {
            ui.label(t("missionenergy.p.distance"));
            drag(ui, &mut leg.distance_m, 50.0, 0.0, 200_000.0, " m");
            ui.label(t("missionenergy.p.airspeed"));
            drag(ui, &mut leg.airspeed_ms, 0.2, 1.0, 80.0, " m/s");
            ui.label(t("missionenergy.p.heading"));
            drag(ui, &mut leg.heading_deg, 1.0, 0.0, 360.0, "°");
        }
        LegKind::Loiter | LegKind::Landing | LegKind::Transition => {
            ui.label(t("missionenergy.p.duration"));
            drag(ui, &mut leg.duration_s, 1.0, 0.0, 7200.0, " s");
        }
        LegKind::Drop => {
            ui.label(t("missionenergy.p.drop_kg"));
            drag(ui, &mut leg.drop_kg, 0.01, 0.0, 50.0, " kg");
            ui.label(t("missionenergy.p.duration"));
            drag(ui, &mut leg.duration_s, 1.0, 0.0, 600.0, " s");
        }
    }
}

fn results_table(ui: &mut Ui, res: &MissionResult) {
    let tk = theme::tokens(ui);
    section(ui, t("missionenergy.results.title"));
    egui::Grid::new("me_results")
        .num_columns(8)
        .striped(true)
        .spacing([space::M, space::XS])
        .show(ui, |ui| {
            for h in [
                "missionenergy.col.leg",
                "missionenergy.col.type",
                "missionenergy.col.time",
                "missionenergy.col.gs",
                "missionenergy.col.power",
                "missionenergy.col.energy",
                "missionenergy.col.remaining",
                "missionenergy.col.note",
            ] {
                ui.label(egui::RichText::new(t(h)).strong());
            }
            ui.end_row();
            for (i, l) in res.legs.iter().enumerate() {
                ui.label(format!("{}", i + 1));
                ui.label(kind_label(l.kind));
                ui.label(fmt(l.time_s, 0));
                if l.ground_speed > 0.0 {
                    ui.label(fmt(l.ground_speed, 1));
                } else {
                    ui.label("—");
                }
                ui.label(fmt(l.power_w, 0));
                ui.label(fmt(l.energy_wh, 2));
                let rem_color = if l.remaining_pct < 0.0 {
                    tk.danger
                } else {
                    tk.text
                };
                ui.label(egui::RichText::new(fmt(l.remaining_pct, 0)).color(rem_color));
                let note = if l.stall_warn {
                    egui::RichText::new(t("missionenergy.note.stall")).color(tk.warning)
                } else if !l.feasible {
                    let key = if matches!(l.kind, LegKind::Climb | LegKind::Descent) {
                        "missionenergy.note.rate_exceeds_airspeed"
                    } else {
                        "missionenergy.note.infeasible"
                    };
                    egui::RichText::new(t(key)).color(tk.danger)
                } else if l.headwind.abs() > 0.05 || l.crosswind.abs() > 0.05 {
                    egui::RichText::new(tf(
                        "missionenergy.note.wind",
                        &[fmt(l.headwind, 1), fmt(l.crosswind.abs(), 1)],
                    ))
                    .color(tk.text_weak)
                } else {
                    egui::RichText::new("")
                };
                ui.label(note);
                ui.end_row();
            }
        });
}

fn summary(ui: &mut Ui, res: &MissionResult) {
    section(ui, t("missionenergy.sum.title"));
    ui.label(tf(
        "missionenergy.sum.totals",
        &[
            fmt(res.total_wh, 1),
            fmt(res.total_time_s / 60.0, 1),
            fmt(res.budget_wh, 1),
            fmt(res.cold_factor * 100.0, 0),
        ],
    ));
    if !res.all_feasible {
        let msg = if res.legs.iter().any(|l| l.stall_warn) {
            t("missionenergy.sum.stall")
        } else if res
            .legs
            .iter()
            .any(|l| !l.feasible && matches!(l.kind, LegKind::Climb | LegKind::Descent))
        {
            t("missionenergy.sum.rate")
        } else {
            t("missionenergy.sum.infeasible")
        };
        components::banner(ui, Status::Err, msg);
    } else if res.pass {
        components::banner(
            ui,
            Status::Ok,
            &tf(
                "missionenergy.sum.pass",
                &[fmt(res.margin_wh, 1), fmt(res.margin_pct, 0)],
            ),
        );
    } else {
        components::banner(
            ui,
            Status::Err,
            &tf(
                "missionenergy.sum.fail",
                &[fmt(-res.margin_wh, 1), fmt(-res.margin_pct, 0)],
            ),
        );
    }
}

/// Bacak başına enerji çubukları (Wh). Bütçeyi aşan bacaklar `danger`, uçulamayanlar `warning`.
fn chart(ui: &mut Ui, res: &MissionResult) {
    let tk = theme::tokens(ui);
    section(ui, t("missionenergy.chart.title"));
    if res.legs.is_empty() {
        return;
    }
    let (resp, painter) =
        ui.allocate_painter(vec2(ui.available_width().max(200.0), 180.0), Sense::hover());
    let r = resp.rect;
    painter.rect_filled(r, theme::corner(theme::radius::M), tk.viewport_bg);
    let plot = Rect::from_min_max(
        pos2(r.left() + 44.0, r.top() + 22.0),
        pos2(r.right() - 10.0, r.bottom() - 22.0),
    );
    let max_e = res
        .legs
        .iter()
        .map(|l| l.energy_wh)
        .fold(0.0_f64, f64::max)
        .max(1e-6);
    let small = FontId::proportional(font::CAPTION);
    painter.line_segment(
        [plot.left_bottom(), plot.right_bottom()],
        Stroke::new(1.0, tk.border_strong),
    );
    painter.text(
        pos2(plot.left() - 4.0, plot.top()),
        Align2::RIGHT_TOP,
        fmt(max_e, 1),
        small.clone(),
        tk.text_weak,
    );
    painter.text(
        pos2(plot.left() - 4.0, plot.bottom()),
        Align2::RIGHT_BOTTOM,
        "0",
        small.clone(),
        tk.text_weak,
    );
    painter.text(
        pos2(r.left() + 6.0, r.top() + 4.0),
        Align2::LEFT_TOP,
        t("missionenergy.chart.axis"),
        small.clone(),
        tk.text_weak,
    );
    let n = res.legs.len() as f32;
    let slot = plot.width() / n;
    let bw = (slot * 0.6).clamp(4.0, 48.0);
    for (i, l) in res.legs.iter().enumerate() {
        let cx = plot.left() + slot * (i as f32 + 0.5);
        let h = (l.energy_wh / max_e) as f32 * plot.height();
        let color = if !l.feasible {
            tk.warning
        } else if l.cum_wh > res.budget_wh {
            tk.danger
        } else {
            tk.accent
        };
        let bar = Rect::from_min_max(
            pos2(cx - bw / 2.0, plot.bottom() - h),
            pos2(cx + bw / 2.0, plot.bottom()),
        );
        painter.rect_filled(bar, 2.0, color);
        painter.text(
            pos2(cx, bar.top() - 2.0),
            Align2::CENTER_BOTTOM,
            format!("{}%", fmt(l.remaining_pct, 0)),
            small.clone(),
            tk.text_weak,
        );
        painter.text(
            pos2(cx, plot.bottom() + 3.0),
            Align2::CENTER_TOP,
            format!("{}", i + 1),
            small.clone(),
            tk.text,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Library, evaluate};
    use crate::project::DroneType;

    #[test]
    fn sync_leg_defaults_updates_unedited_fields_and_preserves_custom() {
        let mut p = Project::default();
        p.set_drone_type(DroneType::FixedWingConventional);
        let m = evaluate(&p, &Library::embedded());
        let veh = Vehicle::from_model(&p, &m);
        let mut legs = default_legs(&veh);
        // İlk seyir bacağı varsayılan hızı kullanır; ikinciye özel hız verelim.
        legs[3].airspeed_ms = veh.v_cruise + 5.0;
        let old_v = veh.v_cruise;
        let old_pay = veh.payload_kg;
        sync_leg_defaults(&mut legs, old_v, old_v + 3.0, old_pay, old_pay + 0.5);
        assert!((legs[0].airspeed_ms - (old_v + 3.0)).abs() < 1e-9);
        assert!((legs[1].airspeed_ms - (old_v + 3.0)).abs() < 1e-9);
        assert!((legs[3].airspeed_ms - (old_v + 5.0)).abs() < 1e-9);
        assert!((legs[2].drop_kg - (old_pay + 0.5)).abs() < 1e-9);
    }
}
