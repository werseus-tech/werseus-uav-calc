// SPDX-License-Identifier: AGPL-3.0-or-later
//! Control tuning tab (expert mode): starting rate-loop PID / filter values for Betaflight,
//! ArduPilot and PX4, fixed-wing hints, and the motor-loss controllability matrix with a top
//! view of the failed motor. The analysis lives in `crate::controltuning`; this file only draws.
//! The panel keeps its small UI state in egui memory, so the application struct is untouched.
use super::components::{self, Status, section_header};
use super::theme::{self, space};
use super::widgets::{fmt, section};
use crate::calc::Model;
use crate::controltuning::failure::{FailureCase, HoverClass};
use crate::controltuning::firmware::{Confidence, Firmware, FirmwareTune, Param, export_text};
use crate::controltuning::vehicle::{VehicleData, VehicleKind};
use crate::controltuning::{ControlReport, TuneOptions, analyze};
use crate::i18n::{t, tf};
use crate::project::Project;
use egui::{RichText, Ui};

#[derive(Clone, Debug)]
struct CtState {
    fw: Firmware,
    tau_manual: bool,
    tau_ms: f64,
    pairs: bool,
    selected: Vec<usize>,
}

impl Default for CtState {
    fn default() -> Self {
        CtState {
            fw: Firmware::ArduPilot,
            tau_manual: false,
            tau_ms: 30.0,
            pairs: false,
            selected: vec![0],
        }
    }
}

pub fn show(ui: &mut Ui, p: &Project, m: &Model) {
    let id = ui.make_persistent_id("controltuning_state");
    let mut st: CtState = ui
        .data_mut(|d| d.get_temp::<CtState>(id))
        .unwrap_or_default();
    panel(ui, p, m, &mut st);
    ui.data_mut(|d| d.insert_temp(id, st));
}

fn panel(ui: &mut Ui, p: &Project, m: &Model, st: &mut CtState) {
    let opts = TuneOptions {
        motor_tau: st.tau_manual.then_some(st.tau_ms / 1000.0),
        include_pairs: st.pairs,
    };
    let rep = analyze(p, m, &opts);

    section_header(ui, t("controltuning.title"));
    components::banner(ui, Status::Warn, t("controltuning.disclaimer"));
    ui.add_space(space::S);
    vehicle_section(ui, &rep.vehicle, st);
    if rep.firmwares.is_empty() {
        ui.add_space(space::S);
        components::banner(ui, Status::Info, t("controltuning.fixed_wing_only"));
    } else {
        if rep.vehicle.kind == VehicleKind::Vtol {
            ui.add_space(space::S);
            components::banner(ui, Status::Info, t("controltuning.vtol_note"));
        }
        rate_section(ui, &rep, st);
    }
    if rep.fixed_wing.is_some() {
        fixed_wing_section(ui, &rep);
    }
    if rep.failures.is_some() {
        failure_section(ui, &rep, st);
    }
}

fn row(ui: &mut Ui, label: &str, value: String) {
    ui.label(label);
    ui.label(RichText::new(value).strong());
    ui.end_row();
}

fn vehicle_section(ui: &mut Ui, v: &VehicleData, st: &mut CtState) {
    section(ui, t("controltuning.vehicle_heading"));
    egui::Grid::new("ct_vehicle")
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            row(
                ui,
                t("controltuning.mass"),
                format!("{} kg", fmt(v.mass, 3)),
            );
            row(
                ui,
                t("controltuning.inertia"),
                format!(
                    "{} / {} / {} kg·m²",
                    fmt(v.inertia[0], 5),
                    fmt(v.inertia[1], 5),
                    fmt(v.inertia[2], 5)
                ),
            );
            if !v.rotors.is_empty() {
                row(ui, t("controltuning.rotors"), v.rotors.len().to_string());
                row(
                    ui,
                    t("controltuning.hover_thrust"),
                    format!("{:.0} %", 100.0 * v.hover_frac),
                );
                row(
                    ui,
                    t("controltuning.rpm"),
                    format!("{:.0} / {:.0}", v.hover_rpm, v.max_rpm),
                );
                ui.label(t("controltuning.tau"));
                ui.horizontal(|ui| {
                    if st.tau_manual {
                        ui.add(
                            egui::DragValue::new(&mut st.tau_ms)
                                .range(3.0..=500.0)
                                .speed(0.5)
                                .suffix(" ms"),
                        );
                    } else {
                        ui.label(RichText::new(format!("{:.0} ms", v.motor_tau * 1000.0)).strong());
                        ui.label(t("controltuning.tau_est"));
                    }
                    if ui
                        .checkbox(&mut st.tau_manual, t("controltuning.tau_manual"))
                        .on_hover_text(t("controltuning.tau_help"))
                        .changed()
                        && st.tau_manual
                    {
                        st.tau_ms = (v.motor_tau * 1000.0).round();
                    }
                });
                ui.end_row();
            }
        });
}

fn conf_text(c: Confidence) -> (&'static str, Status) {
    match c {
        Confidence::High => (t("controltuning.conf_high"), Status::Info),
        Confidence::Medium => (t("controltuning.conf_medium"), Status::Warn),
        Confidence::Low => (t("controltuning.conf_low"), Status::Warn),
    }
}

fn note_key(fw: Firmware) -> &'static str {
    match fw {
        Firmware::Betaflight => t("controltuning.note_bf"),
        Firmware::ArduPilot => t("controltuning.note_ap"),
        Firmware::Px4 => t("controltuning.note_px4"),
    }
}

fn hint_key(fw: Firmware) -> &'static str {
    match fw {
        Firmware::Betaflight => t("controltuning.export_hint_bf"),
        Firmware::ArduPilot => t("controltuning.export_hint_ap"),
        Firmware::Px4 => t("controltuning.export_hint_px4"),
    }
}

fn axis_name(i: usize) -> &'static str {
    match i {
        0 => t("controltuning.axis_roll"),
        1 => t("controltuning.axis_pitch"),
        _ => t("controltuning.axis_yaw"),
    }
}

fn export_header(fw: &str) -> Vec<String> {
    vec![
        format!("WerSeuS UAV Calc - {fw}"),
        t("controltuning.export_comment").to_string(),
    ]
}

fn rate_section(ui: &mut Ui, rep: &ControlReport, st: &mut CtState) {
    section(ui, t("controltuning.rate_heading"));
    let opts: Vec<(Firmware, &str, &str)> =
        Firmware::ALL.iter().map(|&f| (f, f.name(), "")).collect();
    components::segmented(ui, &mut st.fw, &opts);
    let Some(ft) = rep.firmwares.iter().find(|f| f.firmware == st.fw) else {
        return;
    };
    ui.add_space(space::XS);
    if !ft.applicable {
        components::banner(ui, Status::Info, t("controltuning.bf_na"));
        return;
    }
    let (ct, cs) = conf_text(ft.confidence);
    components::banner(ui, cs, &format!("{ct} {}", note_key(ft.firmware)));
    ui.add_space(space::XS);
    axis_table(ui, ft);
    ui.label(tf(
        "controltuning.filters_line",
        &[
            format!("{:.0}", ft.filters.gyro_hz),
            format!("{:.0}", ft.filters.dterm_hz),
            format!("{:.0}", ft.filters.notch_min_hz),
            format!("{:.0}", ft.filters.notch_max_hz),
            format!("{:.0}", ft.filters.hover_motor_hz),
        ],
    ));
    if !ft.clamped.is_empty() {
        components::banner(
            ui,
            Status::Warn,
            &tf("controltuning.clamped_fmt", &[ft.clamped.join(", ")]),
        );
    }
    params_and_export(ui, ft.firmware, &ft.params, "ct_rate_params");
}

fn axis_table(ui: &mut Ui, ft: &FirmwareTune) {
    egui::Grid::new("ct_axes")
        .num_columns(7)
        .striped(true)
        .show(ui, |ui| {
            for k in [
                "controltuning.col_axis",
                "controltuning.col_kj",
                "controltuning.col_fc",
                "controltuning.col_pm",
                "controltuning.col_gm",
                "controltuning.col_bw",
                "controltuning.col_status",
            ] {
                ui.label(RichText::new(t(k)).strong());
            }
            ui.end_row();
            for ax in &ft.axes {
                ui.label(axis_name(ax.axis.index()));
                ui.label(fmt(ax.plant.k_over_j, 0));
                match &ax.result {
                    Some((_, met)) => {
                        ui.label(fmt(met.crossover_hz, 1));
                        ui.label(format!("{:.0}", met.phase_margin_deg));
                        ui.label(format!("{:.1}", met.gain_margin_db));
                        ui.label(fmt(met.bandwidth_hz, 1));
                        components::badge(ui, Status::Ok, t("controltuning.ok"));
                    }
                    None => {
                        for _ in 0..4 {
                            ui.label("—");
                        }
                        components::badge(ui, Status::Err, t("controltuning.no_design"));
                    }
                }
                ui.end_row();
            }
        });
}

fn params_and_export(ui: &mut Ui, fw: Firmware, params: &[Param], salt: &str) {
    egui::CollapsingHeader::new(t("controltuning.params_heading"))
        .id_salt((salt, "params"))
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new((salt, "grid"))
                .num_columns(2)
                .striped(true)
                .show(ui, |ui| {
                    for p in params {
                        ui.label(RichText::new(&p.name).monospace());
                        ui.label(RichText::new(p.value_string()).monospace().strong());
                        ui.end_row();
                    }
                });
        });
    egui::CollapsingHeader::new(t("controltuning.export_heading"))
        .id_salt((salt, "export"))
        .default_open(false)
        .show(ui, |ui| {
            ui.label(hint_key(fw));
            let text = export_text(fw, params, &export_header(fw.name()));
            if ui.button(t("controltuning.copy")).clicked() {
                ui.ctx().copy_text(text.clone());
            }
            let mut shown = text.as_str();
            ui.add(
                egui::TextEdit::multiline(&mut shown)
                    .code_editor()
                    .desired_width(f32::INFINITY)
                    .desired_rows(8),
            );
        });
}

fn fixed_wing_section(ui: &mut Ui, rep: &ControlReport) {
    let Some(h) = &rep.fixed_wing else {
        return;
    };
    section(ui, t("controltuning.fw_heading"));
    egui::Grid::new("ct_fw")
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            let ms = |v: f64| format!("{} m/s", fmt(v, 1));
            row(ui, t("controltuning.fw_vstall"), ms(h.v_stall));
            row(ui, t("controltuning.fw_vmin"), ms(h.v_min));
            row(ui, t("controltuning.fw_vcr"), ms(h.v_cruise));
            row(ui, t("controltuning.fw_vmax"), ms(h.v_max));
            row(ui, t("controltuning.fw_climb"), ms(h.climb_max));
            row(ui, t("controltuning.fw_sink_min"), ms(h.sink_min));
            row(ui, t("controltuning.fw_sink_max"), ms(h.sink_max));
            row(
                ui,
                t("controltuning.fw_roll"),
                format!("{:.0}°", h.roll_limit_deg),
            );
            row(
                ui,
                t("controltuning.fw_pitch"),
                format!("{:.0}° / {:.0}°", h.pitch_min_deg, h.pitch_max_deg),
            );
            row(
                ui,
                t("controltuning.fw_tc"),
                format!("{} s", fmt(h.tconst, 2)),
            );
        });
    components::banner(ui, Status::Warn, t("controltuning.fw_not_covered"));
    ui.label(RichText::new(t("controltuning.fw_ap")).strong());
    params_and_export(ui, Firmware::ArduPilot, &h.ardupilot, "ct_fw_ap");
    ui.label(RichText::new(t("controltuning.fw_px4")).strong());
    params_and_export(ui, Firmware::Px4, &h.px4, "ct_fw_px4");
}

fn class_badge(c: HoverClass) -> (Status, &'static str) {
    match c {
        HoverClass::Full => (Status::Ok, t("controltuning.class_full")),
        HoverClass::NoYaw => (Status::Warn, t("controltuning.class_noyaw")),
        HoverClass::NoHover => (Status::Err, t("controltuning.class_nohover")),
    }
}

fn case_label(c: &FailureCase) -> String {
    if c.failed.is_empty() {
        t("controltuning.healthy").to_string()
    } else {
        c.failed
            .iter()
            .map(|i| format!("M{}", i + 1))
            .collect::<Vec<_>>()
            .join(" + ")
    }
}

fn failure_section(ui: &mut Ui, rep: &ControlReport, st: &mut CtState) {
    let Some(f) = &rep.failures else {
        return;
    };
    section(ui, t("controltuning.fail_heading"));
    ui.label(t("controltuning.fail_method"));
    ui.checkbox(&mut st.pairs, t("controltuning.fail_pairs"));
    let cases: Vec<&FailureCase> = std::iter::once(&f.healthy)
        .chain(&f.singles)
        .chain(&f.pairs)
        .collect();
    if !cases.iter().any(|c| c.failed == st.selected) {
        st.selected = f
            .singles
            .first()
            .map(|c| c.failed.clone())
            .unwrap_or_default();
    }
    let any_nohover = f.singles.iter().any(|c| c.class == HoverClass::NoHover);
    let draw_table = |ui: &mut Ui, st: &mut CtState| {
        egui::ScrollArea::vertical()
            .id_salt("ct_fail_scroll")
            .max_height(320.0)
            .show(ui, |ui| {
                egui::Grid::new("ct_fail")
                    .num_columns(5)
                    .striped(true)
                    .show(ui, |ui| {
                        for k in [
                            "controltuning.col_failed",
                            "controltuning.col_result",
                            "controltuning.col_margin",
                            "controltuning.col_authority",
                            "controltuning.col_lost",
                        ] {
                            ui.label(RichText::new(t(k)).strong());
                        }
                        ui.end_row();
                        for c in &cases {
                            let sel = c.failed == st.selected;
                            if ui.selectable_label(sel, case_label(c)).clicked() {
                                st.selected = c.failed.clone();
                            }
                            let (s, txt) = class_badge(c.class);
                            components::badge(ui, s, txt);
                            if c.class == HoverClass::NoHover && c.thrust_margin < 0.0 {
                                ui.label("—");
                            } else {
                                ui.label(format!("{:+.0} %", 100.0 * c.thrust_margin));
                            }
                            let a: Vec<String> = c
                                .authority
                                .iter()
                                .map(|d| format!("{:.0}%", 100.0 * d[0].min(d[1]).min(9.99)))
                                .collect();
                            ui.label(a.join(" / "));
                            if c.lost_axes.is_empty() && c.class != HoverClass::NoHover {
                                ui.label("—");
                            } else if c.class == HoverClass::NoHover {
                                ui.label(t("controltuning.lost_hover"));
                            } else {
                                let names: Vec<&str> =
                                    c.lost_axes.iter().map(|&k| axis_name(k)).collect();
                                ui.label(names.join(", "));
                            }
                            ui.end_row();
                        }
                    });
            });
    };
    ui.horizontal_top(|ui| {
        let side = (ui.available_width() * 0.35).clamp(160.0, 300.0);
        ui.vertical(|ui| {
            ui.set_width(side);
            ui.label(RichText::new(t("controltuning.topview")).strong());
            top_view(ui, &rep.vehicle, &st.selected, side);
        });
        ui.vertical(|ui| draw_table(ui, st));
    });
    if any_nohover {
        components::banner(ui, Status::Warn, t("controltuning.fail_nohover_note"));
    }
}

/// Top view: forward is up, right is right. Failed rotors are drawn in the danger colour with a
/// cross; spin direction by fill (accent = CCW, info = CW).
fn top_view(ui: &mut Ui, v: &VehicleData, failed: &[usize], side: f32) {
    let tk = theme::tokens(ui);
    let (resp, painter) = ui.allocate_painter(egui::vec2(side, side), egui::Sense::hover());
    let rect = resp.rect;
    painter.rect_filled(rect, theme::corner(6), tk.viewport_bg);
    let c = rect.center();
    let r_prop = 0.5 * v.d_prop;
    let reach = v
        .rotors
        .iter()
        .map(|q| (q.x * q.x + q.y * q.y).sqrt())
        .fold(0.0_f64, f64::max)
        + r_prop;
    let scale = if reach > 0.0 {
        (0.5 * side as f64 - 14.0) / reach
    } else {
        1.0
    };
    let to_screen = |x: f64, y: f64| egui::pos2(c.x + (y * scale) as f32, c.y - (x * scale) as f32);
    // Forward marker.
    painter.arrow(
        egui::pos2(c.x, c.y + 8.0),
        egui::vec2(0.0, -18.0),
        egui::Stroke::new(2.0, tk.text_weak),
    );
    for q in &v.rotors {
        painter.line_segment(
            [c, to_screen(q.x, q.y)],
            egui::Stroke::new(3.0, tk.border_strong),
        );
    }
    let rad = ((r_prop * scale) as f32).clamp(6.0, side / 4.0);
    for (i, q) in v.rotors.iter().enumerate() {
        let mut p = to_screen(q.x, q.y);
        let mut r = rad;
        if q.lower {
            p += egui::vec2(rad * 0.35, rad * 0.35);
            r *= 0.75;
        }
        let dead = failed.contains(&i);
        let (fill, stroke) = if dead {
            (tk.danger_soft, tk.danger)
        } else if q.spin > 0.0 {
            (tk.accent_soft, tk.accent)
        } else {
            (tk.info_soft, tk.info)
        };
        painter.circle(
            p,
            r,
            fill,
            egui::Stroke::new(if dead { 2.5 } else { 1.5 }, stroke),
        );
        if dead {
            let d = r * 0.6;
            let s = egui::Stroke::new(2.5, tk.danger);
            painter.line_segment([p + egui::vec2(-d, -d), p + egui::vec2(d, d)], s);
            painter.line_segment([p + egui::vec2(-d, d), p + egui::vec2(d, -d)], s);
        }
        painter.text(
            p,
            egui::Align2::CENTER_CENTER,
            format!("{}", i + 1),
            egui::FontId::proportional(11.0),
            tk.text,
        );
    }
    ui.horizontal_wrapped(|ui| {
        ui.colored_label(tk.accent, "●");
        ui.label(t("controltuning.ccw"));
        ui.colored_label(tk.info, "●");
        ui.label(t("controltuning.cw"));
        ui.colored_label(tk.danger, "●");
        ui.label(t("controltuning.failed"));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Library, evaluate};
    use crate::i18n::Lang;
    use crate::project::{self, DroneType};
    use crate::ui::theme::ThemeChoice;

    fn render(ctx: &egui::Context, p: &Project, m: &Model) {
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1100.0, 800.0),
        ));
        let mut out = ctx.run_ui(input, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| show(ui, p, m));
        });
        out.textures_delta.clear();
        assert!(!out.shapes.is_empty());
    }

    #[test]
    fn panel_renders_for_every_vehicle_type_in_both_themes_and_languages() {
        let mut projects: Vec<Project> = DroneType::ALL
            .iter()
            .map(|&dt| {
                let mut p = Project::default();
                p.set_drone_type(dt);
                p
            })
            .collect();
        for json in [project::EX_QUAD_250, project::EX_HEXA_550, project::EX_VTOL] {
            projects.push(Project::from_json(json).unwrap());
        }
        let lib = Library::embedded();
        for p in &projects {
            let m = evaluate(p, &lib);
            for choice in [ThemeChoice::Dark, ThemeChoice::Light] {
                for lang in [Lang::Tr, Lang::En] {
                    let ctx = egui::Context::default();
                    theme::install(&ctx, choice);
                    crate::i18n::with_lang(lang, || {
                        // Two frames: the second one runs with the stored state.
                        render(&ctx, p, &m);
                        render(&ctx, p, &m);
                    });
                }
            }
        }
    }

    #[test]
    fn every_firmware_view_and_option_renders() {
        let lib = Library::embedded();
        for json in [project::EX_HEXA_550, project::EX_VTOL] {
            let p = Project::from_json(json).unwrap();
            let m = evaluate(&p, &lib);
            let ctx = egui::Context::default();
            theme::install(&ctx, ThemeChoice::Light);
            for fw in Firmware::ALL {
                for (pairs, manual) in [(false, false), (true, true)] {
                    let mut st = CtState {
                        fw,
                        tau_manual: manual,
                        tau_ms: 45.0,
                        pairs,
                        selected: vec![0, 1],
                    };
                    let mut input = egui::RawInput::default();
                    input.screen_rect = Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(700.0, 900.0),
                    ));
                    let mut out = ctx.run_ui(input, |ui| {
                        egui::ScrollArea::vertical().show(ui, |ui| panel(ui, &p, &m, &mut st));
                    });
                    out.textures_delta.clear();
                    assert_eq!(st.fw, fw);
                }
            }
        }
    }

    #[test]
    fn all_controltuning_keys_exist_in_both_languages() {
        let src = include_str!("controltuning.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        let tr = crate::i18n::tr_table();
        let en = crate::i18n::en_table();
        let mut rest = src;
        let mut n = 0;
        while let Some(i) = rest.find("\"controltuning.") {
            let tail = &rest[i + 1..];
            let end = tail.find('"').unwrap();
            let key = &tail[..end];
            assert!(tr.contains_key(key), "TR missing {key}");
            assert!(en.contains_key(key), "EN missing {key}");
            n += 1;
            rest = &tail[end..];
        }
        assert!(n > 40);
    }
}
