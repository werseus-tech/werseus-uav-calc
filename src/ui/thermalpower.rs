// SPDX-License-Identifier: AGPL-3.0-or-later
//! Thermal and power-distribution tab: electronics heating (ESC, BEC, VTX), cable gauge, voltage
//! drop and battery sag. Everything here is a lumped steady-state estimate, not a measurement; the
//! project values are only read, never changed.
use super::components::{self, NumberInput, Status, number_row, section_header};
use super::theme::{self, Tokens, space};
use super::widgets::fmt;
use crate::calc::Model;
use crate::i18n::{t, tf};
use crate::project::{DroneType, Project};
use crate::thermalpower::cable::{AWG_MAX, AWG_MIN};
use crate::thermalpower::heat::{HeatResult, HeatStatus};
use crate::thermalpower::system::{
    Analysis, BecKind, Config, CoolingKind, EscModel, PowerSource, RunKind, RunReport, analyze,
    power_source,
};
use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};

/// Tab state: the user's inputs (kept while the tab is closed).
#[derive(Clone, Debug, Default)]
pub struct TpState {
    pub cfg: Config,
}

fn status_of(s: HeatStatus) -> Status {
    match s {
        HeatStatus::Pass => Status::Ok,
        HeatStatus::Warn => Status::Warn,
        HeatStatus::Fail => Status::Err,
    }
}

fn status_text(s: HeatStatus) -> &'static str {
    match s {
        HeatStatus::Pass => t("thermalpower.status.pass"),
        HeatStatus::Warn => t("thermalpower.status.warn"),
        HeatStatus::Fail => t("thermalpower.status.fail"),
    }
}

/// Human-readable time: seconds below two minutes, minutes above.
fn fmt_time(t_s: Option<f64>) -> String {
    match t_s {
        None => t("thermalpower.never").to_string(),
        Some(s) if !s.is_finite() => t("thermalpower.never").to_string(),
        Some(s) if s < 120.0 => format!("{} s", fmt(s, 0)),
        Some(s) => format!("{} min", fmt(s / 60.0, 1)),
    }
}

/// Heat-map colour for a temperature fraction (0 = ambient, 1 = limit): theme success, warning
/// and danger colours blended, so it follows both themes.
pub fn heat_color(tk: &Tokens, frac: f64) -> Color32 {
    let f = if frac.is_finite() {
        frac.clamp(0.0, 1.0) as f32
    } else {
        0.0
    };
    if f < 0.5 {
        tk.success.lerp_to_gamma(tk.warning, f * 2.0)
    } else {
        tk.warning.lerp_to_gamma(tk.danger, (f - 0.5) * 2.0)
    }
}

fn frac_of(r: &HeatResult, ambient: f64, limit: f64) -> f64 {
    let span = limit - ambient;
    if span > 0.0 {
        (r.temp_steady_c - ambient) / span
    } else {
        1.0
    }
}

fn cooling_combo(ui: &mut Ui, id: &str, kind: &mut CoolingKind) {
    let label = |k: CoolingKind| match k {
        CoolingKind::Enclosed => t("thermalpower.cool.enclosed"),
        CoolingKind::Open => t("thermalpower.cool.open"),
        CoolingKind::Airflow => t("thermalpower.cool.airflow"),
    };
    egui::ComboBox::from_id_salt(id)
        .selected_text(label(*kind))
        .show_ui(ui, |ui| {
            for k in [
                CoolingKind::Enclosed,
                CoolingKind::Open,
                CoolingKind::Airflow,
            ] {
                ui.selectable_value(kind, k, label(k));
            }
        });
}

fn spec_inputs(ui: &mut Ui, spec: &mut crate::thermalpower::heat::ThermalSpec) {
    number_row(
        ui,
        NumberInput::new(t("thermalpower.in.area"), &mut spec.area_m2)
            .scale(1e4)
            .unit("cm²")
            .range(0.1, 1000.0)
            .decimals(1)
            .help(t("thermalpower.in.area.help")),
    );
    number_row(
        ui,
        NumberInput::new(t("thermalpower.in.r_int"), &mut spec.r_internal_k_per_w)
            .unit("K/W")
            .range(0.0, 50.0)
            .decimals(1)
            .help(t("thermalpower.in.r_int.help")),
    );
    number_row(
        ui,
        NumberInput::new(t("thermalpower.in.cap"), &mut spec.heat_capacity_j_per_k)
            .unit("J/K")
            .range(0.1, 500.0)
            .decimals(1)
            .help(t("thermalpower.in.cap.help")),
    );
}

fn limit_cooling(ui: &mut Ui, id: &str, limit: &mut f64, cooling: &mut CoolingKind) {
    number_row(
        ui,
        NumberInput::new(t("thermalpower.in.limit"), limit)
            .unit("°C")
            .range(30.0, 200.0)
            .decimals(0),
    );
    ui.horizontal(|ui| {
        ui.label(t("thermalpower.in.cooling"));
        cooling_combo(ui, id, cooling);
    });
}

fn component_row(ui: &mut Ui, name: &str, count: u32, low: &HeatResult, full: &HeatResult) {
    ui.label(format!("{name} ×{count}"));
    ui.label(format!("{} / {}", fmt(low.loss_w, 2), fmt(full.loss_w, 2)));
    ui.label(format!(
        "{} / {}",
        fmt(low.temp_steady_c, 0),
        fmt(full.temp_steady_c, 0)
    ));
    ui.label(fmt_time(full.time_to_limit_s));
    ui.label(fmt(full.margin_k, 0));
    components::badge(ui, status_of(full.status), status_text(full.status));
    ui.end_row();
}

fn electronics_results(ui: &mut Ui, a: &Analysis, src: &PowerSource) {
    let low_name = if src.low_is_hover {
        t("thermalpower.state.hover")
    } else {
        t("thermalpower.state.cruise")
    };
    ui.label(tf(
        "thermalpower.res.columns_note",
        &[low_name, t("thermalpower.state.full")],
    ));
    egui::Grid::new("tp_el_grid")
        .num_columns(6)
        .striped(true)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            for h in [
                t("thermalpower.col.component"),
                t("thermalpower.col.loss"),
                t("thermalpower.col.temp"),
                t("thermalpower.col.time"),
                t("thermalpower.col.margin"),
                t("thermalpower.col.status"),
            ] {
                ui.strong(h);
            }
            ui.end_row();
            let vtol = a.esc_lift.is_some() && a.esc_forward.is_some();
            if let Some(lift) = &a.esc_lift {
                let label = if vtol {
                    t("thermalpower.esc.lift")
                } else {
                    t("thermalpower.esc")
                };
                component_row(ui, label, lift.count, &lift.low, &lift.full);
            }
            if let Some(fwd) = &a.esc_forward {
                let label = if vtol {
                    t("thermalpower.esc.forward")
                } else {
                    t("thermalpower.esc")
                };
                component_row(ui, label, fwd.count, &fwd.low, &fwd.full);
            }
            component_row(
                ui,
                t("thermalpower.bec"),
                a.bec.count,
                &a.bec.low,
                &a.bec.full,
            );
            component_row(
                ui,
                t("thermalpower.vtx"),
                a.vtx.count,
                &a.vtx.low,
                &a.vtx.full,
            );
        });
}

/// Computes `(scale_px_per_m, nose_x_m)` from the combined bounding box of airframe and motors.
fn heat_map_layout(rect: Rect, p: &Project, m: &Model) -> (f32, f64) {
    let multirotor = p.drone_type == DroneType::Multirotor;
    if multirotor {
        let ext =
            m.mr.motor_coords
                .iter()
                .fold(0.1_f64, |e, &(x, y)| e.max(x.abs()).max(y.abs()))
                + 0.08;
        let scale = (rect.height().min(rect.width()) * 0.5 - 14.0) / ext as f32;
        return (scale, 0.0);
    }
    let l = if p.l_fuse.is_finite() && p.l_fuse > 0.05 {
        p.l_fuse
    } else {
        0.8
    };
    let b = if p.b.is_finite() && p.b > 0.05 {
        p.b
    } else {
        1.5
    };
    let chord = if p.s.is_finite() && p.s > 0.0 {
        (p.s / b).clamp(0.03, l)
    } else {
        0.2
    };
    let wing_x = if p.drone_type == DroneType::FlyingWing {
        0.0
    } else {
        l * 0.05
    };
    let mut half_x = (l * 0.5)
        .max(wing_x + chord * 0.5)
        .max((wing_x - chord * 0.5).abs());
    let mut half_y = b * 0.5;
    if p.drone_type == DroneType::VtolQuadplane {
        for &(x, y) in &m.mr.motor_coords {
            if x.is_finite() && y.is_finite() {
                half_x = half_x.max(x.abs() + 0.08);
                half_y = half_y.max(y.abs() + 0.08);
            }
        }
    }
    let scale = ((rect.width() - 24.0) as f64 / (2.0 * half_y))
        .min((rect.height() - 24.0) as f64 / (2.0 * half_x)) as f32;
    (scale, l / 2.0)
}

/// Schematic top view with the components coloured by their steady temperature at full throttle.
fn heat_map(ui: &mut Ui, p: &Project, m: &Model, a: &Analysis, cfg: &Config) {
    let tk = theme::tokens(ui);
    let width = ui.available_width().clamp(280.0, 720.0);
    let (resp, painter) = ui.allocate_painter(Vec2::new(width, 250.0), Sense::hover());
    let rect = resp.rect;
    painter.rect_filled(rect, theme::corner(theme::radius::M), tk.viewport_bg);
    painter.rect_stroke(
        rect,
        theme::corner(theme::radius::M),
        Stroke::new(1.0, tk.border),
        egui::StrokeKind::Inside,
    );
    let c = rect.center();
    let multirotor = p.drone_type == DroneType::Multirotor;
    let body = Stroke::new(1.5, tk.border_strong);
    // World metres -> screen: x forward (up), y right.
    let (scale, nose_x) = heat_map_layout(rect, p, m);
    let to = |x: f64, y: f64| Pos2::new(c.x + y as f32 * scale, c.y - x as f32 * scale);
    let mut motors: Vec<(f64, f64)> = Vec::new();
    if multirotor || p.drone_type == DroneType::VtolQuadplane {
        motors = m.mr.motor_coords.clone();
    }
    if multirotor {
        for &(x, y) in &motors {
            painter.line_segment([c, to(x, y)], body);
        }
        painter.circle_stroke(c, 14.0, body);
    } else {
        let l = nose_x * 2.0;
        let b = if p.b.is_finite() && p.b > 0.05 {
            p.b
        } else {
            1.5
        };
        let chord = if p.s.is_finite() && p.s > 0.0 {
            (p.s / b).clamp(0.03, l)
        } else {
            0.2
        };
        let d = if p.d_fuse.is_finite() && p.d_fuse > 0.01 {
            p.d_fuse
        } else {
            0.1
        };
        painter.rect_stroke(
            Rect::from_two_pos(to(l / 2.0, -d / 2.0), to(-l / 2.0, d / 2.0)),
            theme::corner(theme::radius::M),
            body,
            egui::StrokeKind::Inside,
        );
        let wing_x = if p.drone_type == DroneType::FlyingWing {
            0.0
        } else {
            l * 0.05
        };
        painter.rect_stroke(
            Rect::from_two_pos(
                to(wing_x + chord / 2.0, -b / 2.0),
                to(wing_x - chord / 2.0, b / 2.0),
            ),
            theme::corner(theme::radius::S),
            body,
            egui::StrokeKind::Inside,
        );
    }
    let blob = |pos: Pos2, name: &str, r: &HeatResult, limit: f64| {
        let col = heat_color(tk, frac_of(r, cfg.ambient_c, limit));
        let rc = Rect::from_center_size(pos, Vec2::new(30.0, 18.0));
        painter.rect_filled(rc, theme::corner(theme::radius::S), col);
        painter.rect_stroke(
            rc,
            theme::corner(theme::radius::S),
            Stroke::new(1.0, tk.border_strong),
            egui::StrokeKind::Inside,
        );
        painter.text(
            pos + Vec2::new(0.0, 19.0),
            egui::Align2::CENTER_TOP,
            format!("{name} {}°", fmt(r.temp_steady_c, 0)),
            egui::FontId::proportional(theme::font::CAPTION),
            tk.text,
        );
    };
    let esc_label = t("thermalpower.esc");
    if (multirotor || p.drone_type == DroneType::VtolQuadplane)
        && let Some(lift_rep) = &a.esc_lift
    {
        for &(x, y) in &motors {
            let q = to(x, y);
            if !multirotor {
                painter.circle_stroke(q, 5.0, body);
            }
            blob(q, esc_label, &lift_rep.full, cfg.esc.limit_c);
        }
    }
    if multirotor {
        blob(
            c + Vec2::new(-40.0, -4.0),
            t("thermalpower.bec"),
            &a.bec.full,
            cfg.bec.limit_c,
        );
        blob(
            c + Vec2::new(40.0, -4.0),
            t("thermalpower.vtx"),
            &a.vtx.full,
            cfg.vtx.limit_c,
        );
    } else {
        let l = nose_x * 2.0;
        if let Some(fwd_rep) = &a.esc_forward {
            // Forward motor ESC near the nose (also for VTOL's pusher).
            blob(
                to(nose_x - 0.12 * l, 0.0),
                esc_label,
                &fwd_rep.full,
                cfg.esc.limit_c,
            );
        }
        blob(
            to(0.12 * l, 0.0),
            t("thermalpower.bec"),
            &a.bec.full,
            cfg.bec.limit_c,
        );
        blob(
            to(-0.2 * l, 0.0),
            t("thermalpower.vtx"),
            &a.vtx.full,
            cfg.vtx.limit_c,
        );
    }
    // Legend: ambient -> limit gradient.
    let lg = Rect::from_min_size(
        rect.left_bottom() + Vec2::new(10.0, -16.0),
        Vec2::new(110.0, 8.0),
    );
    let n = 22;
    for i in 0..n {
        let f = i as f32 / (n - 1) as f32;
        let seg = Rect::from_min_size(
            lg.min + Vec2::new(lg.width() * i as f32 / n as f32, 0.0),
            Vec2::new(lg.width() / n as f32 + 0.5, lg.height()),
        );
        painter.rect_filled(seg, 0.0, heat_color(tk, f as f64));
    }
    painter.text(
        lg.left_top() + Vec2::new(0.0, -2.0),
        egui::Align2::LEFT_BOTTOM,
        t("thermalpower.legend"),
        egui::FontId::proportional(theme::font::CAPTION),
        tk.text_weak,
    );
}

fn run_name(k: RunKind, is_vtol: bool) -> &'static str {
    match k {
        RunKind::Battery => t("thermalpower.run.battery"),
        RunKind::EscPower if is_vtol => t("thermalpower.run.lift_esc"),
        RunKind::EscPower => t("thermalpower.run.esc"),
        RunKind::Phase if is_vtol => t("thermalpower.run.lift_phase"),
        RunKind::Phase => t("thermalpower.run.phase"),
        RunKind::FwdEscPower => t("thermalpower.run.fwd_esc"),
        RunKind::FwdPhase => t("thermalpower.run.fwd_phase"),
    }
}

fn run_results(ui: &mut Ui, r: &RunReport, v_pack: f64, id: usize) {
    egui::Grid::new(("tp_run_grid", id))
        .num_columns(3)
        .striped(true)
        .spacing([14.0, 3.0])
        .show(ui, |ui| {
            ui.label("");
            ui.strong(t("thermalpower.state.low"));
            ui.strong(t("thermalpower.state.full"));
            ui.end_row();
            let pct = |v: f64| {
                if v_pack > 0.0 {
                    100.0 * v / v_pack
                } else {
                    0.0
                }
            };
            let rows: [(&str, String, String); 7] = [
                (
                    t("thermalpower.cab.current"),
                    format!("{} A", fmt(r.current_low_a, 1)),
                    format!("{} A", fmt(r.current_full_a, 1)),
                ),
                (
                    t("thermalpower.cab.resistance"),
                    format!("{} mΩ", fmt(r.low.resistance_ohm * 1e3, 2)),
                    format!("{} mΩ", fmt(r.full.resistance_ohm * 1e3, 2)),
                ),
                (
                    t("thermalpower.cab.drop"),
                    format!(
                        "{} V ({} %)",
                        fmt(r.low.voltage_drop_v, 3),
                        fmt(pct(r.low.voltage_drop_v), 2)
                    ),
                    format!(
                        "{} V ({} %)",
                        fmt(r.full.voltage_drop_v, 3),
                        fmt(pct(r.full.voltage_drop_v), 2)
                    ),
                ),
                (
                    t("thermalpower.cab.loss"),
                    format!("{} W", fmt(r.total_loss_low_w, 2)),
                    format!("{} W", fmt(r.total_loss_full_w, 2)),
                ),
                (
                    t("thermalpower.cab.rise"),
                    format!("{} K", fmt(r.low.temp_rise_k, 1)),
                    format!("{} K", fmt(r.full.temp_rise_k, 1)),
                ),
                (
                    t("thermalpower.cab.wire_temp"),
                    format!("{} °C", fmt(r.low.wire_temp_c, 0)),
                    format!("{} °C", fmt(r.full.wire_temp_c, 0)),
                ),
                (
                    t("thermalpower.cab.mass"),
                    String::new(),
                    format!("{} g", fmt(r.total_mass_kg * 1e3, 1)),
                ),
            ];
            for (label, lo, hi) in rows {
                ui.label(label);
                ui.label(lo);
                ui.label(hi);
                ui.end_row();
            }
        });
}

pub fn show(ui: &mut Ui, p: &Project, m: &Model, st: &mut TpState) {
    let tk = theme::tokens(ui);
    section_header(ui, t("thermalpower.tab"));
    components::banner(ui, Status::Warn, t("thermalpower.disclaimer"));
    ui.add_space(space::XS);

    let src = power_source(p, m);
    ui.label(tf(
        "thermalpower.source_line",
        &[
            p.drone_type.label(),
            &src.total_esc_count().to_string(),
            &fmt(src.v_pack, 1),
            &fmt(src.i_low_total, 1),
            &fmt(src.i_full_total, 1),
            &fmt(src.max_full_esc_current(), 1),
        ],
    ));
    ui.label(
        egui::RichText::new(if src.low_is_hover {
            t("thermalpower.state.hover_note")
        } else {
            t("thermalpower.state.cruise_note")
        })
        .color(tk.text_weak),
    );
    ui.add_space(space::XS);

    section_header(ui, t("thermalpower.sec.general"));
    {
        let c = &mut st.cfg;
        number_row(
            ui,
            NumberInput::new(t("thermalpower.in.ambient"), &mut c.ambient_c)
                .unit("°C")
                .range(-40.0, 70.0)
                .decimals(0),
        );
        number_row(
            ui,
            NumberInput::new(t("thermalpower.in.air_speed"), &mut c.air_speed_ms)
                .unit("m/s")
                .range(0.0, 60.0)
                .decimals(1)
                .help(t("thermalpower.in.air_speed.help")),
        );
        number_row(
            ui,
            NumberInput::new(t("thermalpower.in.max_drop"), &mut c.max_drop_pct)
                .unit("%")
                .range(0.1, 20.0)
                .decimals(1)
                .help(t("thermalpower.in.max_drop.help")),
        );
        number_row(
            ui,
            NumberInput::new(t("thermalpower.in.max_rise"), &mut c.max_rise_k)
                .unit("K")
                .range(5.0, 120.0)
                .decimals(0)
                .help(t("thermalpower.in.max_rise.help")),
        );
    }

    section_header(ui, t("thermalpower.sec.electronics"));
    egui::CollapsingHeader::new(t("thermalpower.esc"))
        .default_open(false)
        .show(ui, |ui| {
            let e = &mut st.cfg.esc;
            ui.horizontal(|ui| {
                ui.selectable_value(
                    &mut e.model,
                    EscModel::Efficiency,
                    t("thermalpower.esc.eff"),
                );
                ui.selectable_value(&mut e.model, EscModel::RdsOn, t("thermalpower.esc.rds"));
            });
            if e.model == EscModel::Efficiency {
                number_row(
                    ui,
                    NumberInput::new(t("thermalpower.in.eta"), &mut e.eta)
                        .range(0.5, 1.0)
                        .decimals(3),
                );
            } else {
                number_row(
                    ui,
                    NumberInput::new(t("thermalpower.in.r_on"), &mut e.r_on_mohm)
                        .unit("mΩ")
                        .range(0.1, 100.0)
                        .decimals(2)
                        .help(t("thermalpower.in.r_on.help")),
                );
                number_row(
                    ui,
                    NumberInput::new(t("thermalpower.in.sw_frac"), &mut e.sw_frac)
                        .scale(100.0)
                        .unit("%")
                        .range(0.0, 10.0)
                        .decimals(2)
                        .help(t("thermalpower.in.sw_frac.help")),
                );
            }
            limit_cooling(ui, "tp_esc_cool", &mut e.limit_c, &mut e.cooling);
            spec_inputs(ui, &mut e.spec);
        });
    egui::CollapsingHeader::new(t("thermalpower.bec"))
        .default_open(false)
        .show(ui, |ui| {
            let b = &mut st.cfg.bec;
            ui.horizontal(|ui| {
                ui.selectable_value(&mut b.kind, BecKind::Linear, t("thermalpower.bec.linear"));
                ui.selectable_value(
                    &mut b.kind,
                    BecKind::Switching,
                    t("thermalpower.bec.switching"),
                );
            });
            let v_out_max = if b.kind == BecKind::Linear && src.v_pack > 1.0 {
                src.v_pack.clamp(1.0, 24.0)
            } else {
                24.0
            };
            number_row(
                ui,
                NumberInput::new(t("thermalpower.in.v_out"), &mut b.v_out)
                    .unit("V")
                    .range(1.0, v_out_max)
                    .decimals(1),
            );
            if b.kind == BecKind::Linear && b.v_out > src.v_pack {
                ui.colored_label(tk.danger, t("thermalpower.bec.linear_invalid"));
            }
            number_row(
                ui,
                NumberInput::new(t("thermalpower.in.i_load"), &mut b.i_load_a)
                    .unit("A")
                    .range(0.0, 30.0)
                    .decimals(2),
            );
            if b.kind == BecKind::Switching {
                number_row(
                    ui,
                    NumberInput::new(t("thermalpower.in.eta"), &mut b.eta)
                        .range(0.3, 1.0)
                        .decimals(2),
                );
            }
            limit_cooling(ui, "tp_bec_cool", &mut b.limit_c, &mut b.cooling);
            spec_inputs(ui, &mut b.spec);
        });
    egui::CollapsingHeader::new(t("thermalpower.vtx"))
        .default_open(false)
        .show(ui, |ui| {
            let v = &mut st.cfg.vtx;
            number_row(
                ui,
                NumberInput::new(t("thermalpower.in.p_in"), &mut v.p_in_w)
                    .unit("W")
                    .range(0.0, 60.0)
                    .decimals(2),
            );
            number_row(
                ui,
                NumberInput::new(t("thermalpower.in.p_rf"), &mut v.p_rf_w)
                    .unit("W")
                    .range(0.0, v.p_in_w.clamp(0.0, 30.0))
                    .decimals(2),
            );
            if v.p_rf_w > v.p_in_w {
                ui.colored_label(tk.danger, t("thermalpower.vtx.invalid"));
            }
            limit_cooling(ui, "tp_vtx_cool", &mut v.limit_c, &mut v.cooling);
            spec_inputs(ui, &mut v.spec);
        });

    let a = analyze(&src, &st.cfg);
    ui.add_space(space::XS);
    electronics_results(ui, &a, &src);
    if a.bec_invalid {
        ui.colored_label(tk.danger, t("thermalpower.bec.linear_invalid"));
    }
    if a.vtx_invalid {
        ui.colored_label(tk.danger, t("thermalpower.vtx.invalid"));
    }
    ui.add_space(space::XS);
    ui.label(egui::RichText::new(t("thermalpower.heatmap_note")).color(tk.text_weak));
    heat_map(ui, p, m, &a, &st.cfg);

    ui.add_space(space::S);
    section_header(ui, t("thermalpower.sec.cables"));
    let is_vtol = a.esc_lift.is_some() && a.esc_forward.is_some();
    for (i, &r) in a.runs.iter().enumerate() {
        let status = if r.ok { Status::Ok } else { Status::Err };
        ui.horizontal(|ui| {
            ui.strong(format!("{} ×{}", run_name(r.kind, is_vtol), r.count));
            components::badge(
                ui,
                status,
                if r.ok {
                    t("thermalpower.status.pass")
                } else {
                    t("thermalpower.status.fail")
                },
            );
        });
        let cfg_run = match r.kind {
            RunKind::Battery => &mut st.cfg.battery_lead,
            RunKind::EscPower => &mut st.cfg.esc_leads,
            RunKind::Phase => &mut st.cfg.phase_leads,
            RunKind::FwdEscPower => &mut st.cfg.fwd_esc_lead,
            RunKind::FwdPhase => &mut st.cfg.fwd_phase_lead,
        };
        ui.horizontal(|ui| {
            ui.label(t("thermalpower.in.awg"));
            ui.add(
                egui::DragValue::new(&mut cfg_run.awg)
                    .range(AWG_MIN..=AWG_MAX)
                    .prefix("AWG "),
            );
            ui.label(t("thermalpower.in.length"));
            ui.add(
                egui::DragValue::new(&mut cfg_run.length_m)
                    .range(0.0..=5.0)
                    .speed(0.005)
                    .fixed_decimals(3)
                    .suffix(" m"),
            );
        });
        run_results(ui, &r, src.v_pack, i);
        ui.horizontal(|ui| match r.recommended_awg {
            Some(awg) => {
                ui.label(tf("thermalpower.recommended_fmt", &[awg.to_string()]));
                if awg != cfg_run.awg && ui.small_button(t("thermalpower.apply")).clicked() {
                    cfg_run.awg = awg;
                }
            }
            None => {
                ui.colored_label(tk.danger, t("thermalpower.recommended_none"));
            }
        });
        ui.add_space(space::XS);
    }
    if matches!(p.drone_type, DroneType::VtolQuadplane) {
        ui.label(egui::RichText::new(t("thermalpower.vtol_note")).color(tk.text_weak));
    }

    section_header(ui, t("thermalpower.sec.sag"));
    let g = a.sag;
    egui::Grid::new("tp_sag_grid")
        .num_columns(3)
        .striped(true)
        .spacing([14.0, 3.0])
        .show(ui, |ui| {
            ui.label("");
            ui.strong(t("thermalpower.state.low"));
            ui.strong(t("thermalpower.state.full"));
            ui.end_row();
            let rows: [(&str, String, String); 4] = [
                (
                    t("thermalpower.sag.pack"),
                    format!("{} V", fmt(g.sag_low_v, 2)),
                    format!("{} V", fmt(g.sag_full_v, 2)),
                ),
                (
                    t("thermalpower.sag.cables"),
                    format!("{} V", fmt(g.cable_drop_low_v, 2)),
                    format!("{} V", fmt(g.cable_drop_full_v, 2)),
                ),
                (
                    t("thermalpower.sag.v_motor"),
                    format!("{} V", fmt(g.v_motor_low, 2)),
                    format!("{} V", fmt(g.v_motor_full, 2)),
                ),
                (
                    t("thermalpower.sag.pct"),
                    format!("{} %", fmt(pct_of(g.v_nominal, g.v_motor_low), 1)),
                    format!("{} %", fmt(pct_of(g.v_nominal, g.v_motor_full), 1)),
                ),
            ];
            for (l, a, b) in rows {
                ui.label(l);
                ui.label(a);
                ui.label(b);
                ui.end_row();
            }
        });
    ui.label(
        egui::RichText::new(tf(
            "thermalpower.sag_note_fmt",
            &[
                fmt(g.v_nominal, 1),
                fmt(g.r_pack_ohm * 1e3, 1),
                src.n_series.to_string(),
                src.n_parallel.to_string(),
            ],
        ))
        .color(tk.text_weak),
    );
}

/// Voltage lost relative to nominal, in percent (0 when nominal is not positive).
fn pct_of(v_nominal: f64, v: f64) -> f64 {
    if v_nominal.is_finite() && v.is_finite() && v_nominal > 0.0 {
        100.0 * (v_nominal - v) / v_nominal
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Library, evaluate};
    use crate::i18n::Lang;
    use crate::ui::theme::ThemeChoice;
    use egui::epaint::ClippedShape;

    fn run(ctx: &egui::Context, f: &mut dyn FnMut(&mut egui::Ui)) -> Vec<ClippedShape> {
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1100.0, 1400.0)));
        let mut out = ctx.run_ui(input, |ui| f(ui));
        out.textures_delta.clear();
        out.shapes
    }

    #[test]
    fn panel_renders_for_every_vehicle_theme_and_language() {
        for dt in DroneType::ALL {
            let mut p = Project::default();
            p.set_drone_type(dt);
            let m = evaluate(&p, &Library::embedded());
            for choice in [ThemeChoice::Dark, ThemeChoice::Light] {
                for lang in [Lang::Tr, Lang::En] {
                    let ctx = egui::Context::default();
                    theme::install(&ctx, choice);
                    let mut st = TpState::default();
                    crate::i18n::with_lang(lang, || {
                        for _ in 0..2 {
                            let shapes = run(&ctx, &mut |ui| show(ui, &p, &m, &mut st));
                            assert!(!shapes.is_empty(), "{dt:?}");
                        }
                    });
                }
            }
        }
    }

    #[test]
    fn heat_color_is_defined_for_any_fraction_and_theme() {
        for tk in [&theme::DARK, &theme::LIGHT] {
            for f in [
                f64::NAN,
                -1.0,
                0.0,
                0.25,
                0.5,
                0.75,
                1.0,
                9.0,
                f64::INFINITY,
            ] {
                let _ = heat_color(tk, f);
            }
            assert_eq!(heat_color(tk, 0.0), tk.success);
            assert_eq!(heat_color(tk, 1.0), tk.danger);
        }
    }

    #[test]
    fn time_formatting_covers_all_cases() {
        assert_eq!(fmt_time(None), t("thermalpower.never"));
        assert!(fmt_time(Some(30.0)).ends_with(" s"));
        assert!(fmt_time(Some(600.0)).ends_with(" min"));
        assert_eq!(fmt_time(Some(f64::NAN)), t("thermalpower.never"));
    }

    #[test]
    fn vtol_heat_map_layout_fits_airframe_and_motors_at_same_scale() {
        let mut p = Project::default();
        p.set_drone_type(DroneType::VtolQuadplane);
        p.mr_frame_size = 2.5;
        let m = evaluate(&p, &Library::embedded());
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(500.0, 250.0));
        let (scale, nose_x) = heat_map_layout(rect, &p, &m);
        let c = rect.center();
        assert!(scale > 0.0 && scale.is_finite());
        assert!(nose_x > 0.0);
        for &(x, y) in &m.mr.motor_coords {
            let pos = Pos2::new(c.x + y as f32 * scale, c.y - x as f32 * scale);
            assert!(
                rect.contains(pos),
                "motor ({x},{y}) -> {pos:?} outside {rect:?}"
            );
        }
        assert_eq!(pct_of(f64::INFINITY, f64::INFINITY), 0.0);
    }
}
