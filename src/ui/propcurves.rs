// SPDX-License-Identifier: AGPL-3.0-or-later
//! Propulsion curves tab: thrust, current, power, efficiency, RPM and winding temperature
//! versus throttle, limit markers, and flight time / speed by flight mode. Everything shown
//! is a model estimate (see `crate::propcurves`), not bench-test data. The panel keeps its own
//! small state in the egui memory, so it needs no field in the application struct.
use super::components::{self, Status, section_header, segmented};
use super::theme::{self, space};
use super::widgets::fmt;
use crate::calc::Model;
use crate::i18n::{t, tf};
use crate::project::Project;
use crate::propcurves::{
    Curve, FlightSummary, Group, ModeKind, ModeResult, ModeStatus, Overrides, PropUnit, Setup,
    TimeBasis, build, flight_summary, sweep,
};
use egui::{RichText, Ui};
use egui_plot::{HLine, Legend, Line, Plot, PlotPoints, Points};

/// Panel state kept in the egui memory.
#[derive(Clone, Default)]
struct PcState {
    group: Group,
    /// Airspeed of the sweep [m/s]; 0 is the static case.
    airspeed: f64,
    /// Overrides, one set per group (`[lift, forward]`).
    overrides: [Overrides; 2],
    /// Cached flight summary with the key of the inputs it was computed from.
    summary: Option<(u64, FlightSummary)>,
}

fn group_index(g: Group) -> usize {
    match g {
        Group::Lift => 0,
        Group::Forward => 1,
    }
}

/// Throttle in percent and a value as a plot point list.
fn pts(c: &Curve, f: impl Fn(&crate::propcurves::CurvePoint) -> f64) -> Vec<[f64; 2]> {
    c.points
        .iter()
        .map(|p| [p.op.throttle * 100.0, f(p)])
        .collect()
}

/// Draw one throttle plot with an optional list of horizontal limits and the flagged points.
fn plot(
    ui: &mut Ui,
    (id, title, y_label): (&str, &str, &str),
    c: &Curve,
    f: impl Fn(&crate::propcurves::CurvePoint) -> f64 + Copy,
    limits: &[(&'static str, f64)],
    flagged: impl Fn(&crate::propcurves::CurvePoint) -> bool,
) {
    let tk = theme::tokens(ui);
    ui.label(RichText::new(title).strong());
    Plot::new(id)
        .height(170.0)
        .x_axis_label(t("propcurves.plot.throttle"))
        .y_axis_label(y_label)
        .legend(Legend::default())
        .allow_scroll(false)
        .show(ui, |pu| {
            pu.line(
                Line::new(title, PlotPoints::from(pts(c, f)))
                    .color(tk.accent)
                    .width(2.0),
            );
            for (name, y) in limits {
                if y.is_finite() {
                    pu.hline(HLine::new(*name, *y).color(tk.warning));
                }
            }
            let bad: Vec<[f64; 2]> = c
                .points
                .iter()
                .filter(|p| flagged(p))
                .map(|p| [p.op.throttle * 100.0, f(p)])
                .collect();
            if !bad.is_empty() {
                pu.points(
                    Points::new(t("propcurves.limit.exceeded"), bad)
                        .radius(4.5)
                        .color(tk.danger),
                );
            }
        });
}

fn plots(ui: &mut Ui, u: &PropUnit, c: &Curve) {
    let n = u.n_motors as f64;
    let i_pack_per_motor = u.pack.max_cont_a / n;
    let left = |ui: &mut Ui| {
        plot(
            ui,
            ("pc_thrust", t("propcurves.plot.thrust"), "N"),
            c,
            |p| p.op.thrust_n,
            &[],
            |_| false,
        );
        plot(
            ui,
            ("pc_power", t("propcurves.plot.power"), "W"),
            c,
            |p| p.op.p_elec_w,
            &[],
            |_| false,
        );
        plot(
            ui,
            ("pc_rpm", t("propcurves.plot.rpm"), "RPM"),
            c,
            |p| p.op.rpm,
            &[(t("propcurves.limit.tip"), u.rpm_limit)],
            |p| p.flags.tip_speed,
        );
    };
    let right = |ui: &mut Ui| {
        plot(
            ui,
            ("pc_current", t("propcurves.plot.current"), "A"),
            c,
            |p| p.op.i_motor_a,
            &[
                (t("propcurves.limit.motor"), u.motor.i_max_a),
                (t("propcurves.limit.esc"), u.esc.i_max_a),
            ],
            |p| p.flags.motor_current || p.flags.esc_current,
        );
        plot(
            ui,
            ("pc_current_bat", t("propcurves.plot.current_bat"), "A"),
            c,
            |p| p.op.i_bat_a,
            &[(t("propcurves.limit.battery"), i_pack_per_motor)],
            |p| p.flags.battery,
        );
        plot(
            ui,
            ("pc_eff", t("propcurves.plot.eff"), "gf/W"),
            c,
            |p| p.efficiency_gf_per_w,
            &[],
            |_| false,
        );
        plot(
            ui,
            ("pc_temp", t("propcurves.plot.temp"), "°C"),
            c,
            |p| p.op.temp_c,
            &[(t("propcurves.limit.temp"), u.motor.t_limit_c)],
            |p| p.flags.temperature,
        );
    };
    if ui.available_width() > 760.0 {
        ui.columns(2, |cols| {
            left(&mut cols[0]);
            right(&mut cols[1]);
        });
    } else {
        left(ui);
        right(ui);
    }
}

/// One override row: the effective value is shown and edited; editing stores an override.
fn override_row(
    ui: &mut Ui,
    label: &str,
    effective: f64,
    ov: &mut Option<f64>,
    unit: &str,
    (decimals, speed, min): (usize, f64, f64),
) {
    ui.label(label);
    let mut v = ov.unwrap_or(effective);
    let r = ui.add(
        egui::DragValue::new(&mut v)
            .speed(speed)
            .range(min..=f64::MAX)
            .max_decimals(decimals)
            .suffix(format!(" {unit}")),
    );
    if r.changed() {
        *ov = Some(v);
    }
    if ov.is_some() && ui.small_button(t("propcurves.motor.reset")).clicked() {
        *ov = None;
    }
    ui.end_row();
}

fn motor_panel(ui: &mut Ui, u: &PropUnit, ov: &mut Overrides) {
    ui.label(
        RichText::new(t("propcurves.motor.note"))
            .small()
            .color(theme::tokens(ui).text_weak),
    );
    egui::Grid::new("pc_motor").num_columns(3).show(ui, |ui| {
        override_row(ui, "KV", u.motor.kv, &mut ov.kv, "rpm/V", (0, 1.0, 1.5));
        override_row(
            ui,
            t("propcurves.motor.r"),
            u.motor.r_ohm * 1000.0,
            &mut ov.r_mohm,
            "mΩ",
            (1, 0.5, 0.001),
        );
        override_row(
            ui,
            t("propcurves.motor.i0"),
            u.motor.i0_a,
            &mut ov.i0_a,
            "A",
            (2, 0.05, 0.0),
        );
        override_row(
            ui,
            t("propcurves.motor.i_motor"),
            u.motor.i_max_a,
            &mut ov.motor_i_max_a,
            "A",
            (1, 0.2, 0.01),
        );
        override_row(
            ui,
            t("propcurves.motor.i_esc"),
            u.esc.i_max_a,
            &mut ov.esc_i_max_a,
            "A",
            (1, 0.2, 0.01),
        );
        override_row(
            ui,
            t("propcurves.motor.t_limit"),
            u.motor.t_limit_c,
            &mut ov.t_limit_c,
            "°C",
            (0, 1.0, 30.5),
        );
    });
}

fn mode_name(k: ModeKind) -> &'static str {
    match k {
        ModeKind::Hover => t("propcurves.mode.hover"),
        ModeKind::Aggressive => t("propcurves.mode.aggressive"),
        ModeKind::Cruise => t("propcurves.mode.cruise"),
    }
}

fn opt(x: Option<f64>, dec: usize) -> String {
    x.map(|v| fmt(v, dec)).unwrap_or_else(|| "—".to_string())
}

fn mode_row(ui: &mut Ui, r: &ModeResult, fixed_wing_only: bool) {
    let tk = theme::tokens(ui);
    ui.label(RichText::new(mode_name(r.kind)).strong());
    match r.status {
        ModeStatus::NotApplicable => {
            let msg = if fixed_wing_only {
                t("propcurves.na_fixed_wing")
            } else {
                t("propcurves.na")
            };
            ui.label(RichText::new(msg).color(tk.text_weak));
            for _ in 0..5 {
                ui.label("—");
            }
        }
        ModeStatus::Unreachable => {
            ui.label(RichText::new(t("propcurves.unreachable")).color(tk.danger));
            for _ in 0..5 {
                ui.label("—");
            }
        }
        ModeStatus::Ok => {
            ui.label(opt(r.throttle.map(|d| d * 100.0), 0));
            ui.label(opt(r.airspeed_ms, 1));
            ui.label(opt(r.current_a, 1));
            ui.label(opt(r.power_w, 0));
            let mark = if r.basis == TimeBasis::ExistingModel {
                " *"
            } else {
                ""
            };
            ui.label(RichText::new(format!("{}{}", opt(r.time_min, 1), mark)).strong());
            ui.label(opt(r.reference_time_min, 1));
        }
    }
    ui.end_row();
}

/// Key of everything the flight summary depends on (it is far costlier than a throttle sweep,
/// so it is only recomputed when one of these changes).
fn summary_key(p: &Project, m: &Model, setup: &Setup) -> u64 {
    use std::hash::{Hash, Hasher};
    let text = format!(
        "{:?}|{:?}|{:?}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        setup,
        p.drone_type,
        m.perf_in,
        p.dod,
        p.p_hotel,
        p.v_cr,
        p.mr_v_cruise,
        p.mr_flat_plate_area,
        m.weight.w,
        m.weight.pack_wh,
        m.mr.w,
        m.mr.t_hover_motor,
        m.mr.e_usable_wh,
        m.mr.p_total_cruise,
        m.mr.hover_endurance_min,
        m.cons.v_stall,
    );
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    m.perf_cr.endurance_s.to_bits().hash(&mut h);
    m.mr.cruise_endurance_min.to_bits().hash(&mut h);
    h.finish()
}

fn modes_table(ui: &mut Ui, p: &Project, f: &FlightSummary) {
    egui::Grid::new("pc_modes")
        .num_columns(7)
        .striped(true)
        .show(ui, |ui| {
            for h in [
                "propcurves.col.mode",
                "propcurves.col.throttle",
                "propcurves.col.speed",
                "propcurves.col.current",
                "propcurves.col.power",
                "propcurves.col.time",
                "propcurves.col.reference",
            ] {
                ui.label(RichText::new(t(h)).strong());
            }
            ui.end_row();
            let fixed_wing_only = !p.drone_type.is_multirotor();
            for r in &f.modes {
                mode_row(ui, r, fixed_wing_only);
            }
        });
    ui.add_space(space::XS);
    match f.v_max_ms {
        Some(v) => ui.label(tf("propcurves.vmax_fmt", &[fmt(v, 1), fmt(v * 3.6, 0)])),
        None => ui.label(t("propcurves.vmax_none")),
    };
    if f.modes
        .iter()
        .any(|r| r.basis == TimeBasis::ExistingModel && r.status == ModeStatus::Ok)
    {
        ui.label(
            RichText::new(t("propcurves.basis_existing"))
                .small()
                .color(theme::tokens(ui).text_weak),
        );
    }
    ui.label(
        RichText::new(t("propcurves.modes_assumptions"))
            .small()
            .color(theme::tokens(ui).text_weak),
    );
}

fn flag_text(f: &crate::propcurves::LimitFlags) -> String {
    let mut v: Vec<&str> = Vec::new();
    if f.motor_current {
        v.push(t("propcurves.flag.motor"));
    }
    if f.esc_current {
        v.push(t("propcurves.flag.esc"));
    }
    if f.battery {
        v.push(t("propcurves.flag.battery"));
    }
    if f.temperature {
        v.push(t("propcurves.flag.temp"));
    }
    if f.tip_speed {
        v.push(t("propcurves.flag.tip"));
    }
    if v.is_empty() {
        "—".to_string()
    } else {
        v.join(", ")
    }
}

fn data_table(ui: &mut Ui, c: &Curve) {
    let tk = theme::tokens(ui);
    egui::Grid::new("pc_table")
        .num_columns(9)
        .striped(true)
        .show(ui, |ui| {
            for h in [
                "propcurves.col.throttle",
                "propcurves.col.thrust_n",
                "propcurves.col.thrust_gf",
                "propcurves.col.current",
                "propcurves.col.power",
                "propcurves.col.eff",
                "propcurves.col.rpm",
                "propcurves.col.temp",
                "propcurves.col.flags",
            ] {
                ui.label(RichText::new(t(h)).strong());
            }
            ui.end_row();
            for p in &c.points {
                let o = &p.op;
                ui.label(fmt(o.throttle * 100.0, 0));
                ui.label(fmt(o.thrust_n, 2));
                ui.label(fmt(o.thrust_gf(), 0));
                ui.label(fmt(o.i_bat_a, 1));
                ui.label(fmt(o.p_elec_w, 0));
                ui.label(fmt(p.efficiency_gf_per_w, 2));
                ui.label(fmt(o.rpm, 0));
                ui.label(fmt(o.temp_c, 0));
                let txt = flag_text(&p.flags);
                if p.flags.any() {
                    ui.label(RichText::new(txt).color(tk.danger));
                } else {
                    ui.label(txt);
                }
                ui.end_row();
            }
        });
}

/// Draw the whole panel.
pub fn show(ui: &mut Ui, p: &Project, m: &Model) {
    let id = egui::Id::unique("propcurves_state");
    let mut st: PcState = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    section_header(ui, t("propcurves.title"));
    ui.label(t("propcurves.intro"));
    components::banner(ui, Status::Info, t("propcurves.estimate_notice"));

    let setup = build(p, m, &st.overrides);
    let groups = setup.groups();
    if groups.is_empty() {
        return;
    }
    if !groups.contains(&st.group) {
        st.group = groups[0];
    }
    if groups.len() > 1 {
        ui.add_space(space::S);
        ui.horizontal(|ui| {
            ui.label(t("propcurves.group_label"));
            segmented(
                ui,
                &mut st.group,
                &[
                    (Group::Lift, t("propcurves.group.lift"), ""),
                    (Group::Forward, t("propcurves.group.forward"), ""),
                ],
            );
        });
    }
    ui.add_space(space::S);
    ui.horizontal(|ui| {
        ui.label(t("propcurves.airspeed"));
        ui.add(
            egui::DragValue::new(&mut st.airspeed)
                .range(0.0..=80.0)
                .speed(0.2)
                .suffix(" m/s"),
        );
        if ui.button(t("propcurves.airspeed_static")).clicked() {
            st.airspeed = 0.0;
        }
        // The sweep uses the airspeed along the propeller axis. That is the cruise speed for a
        // forward propeller, but not for lift rotors, whose axis is roughly vertical.
        if st.group == Group::Forward && ui.button(t("propcurves.airspeed_cruise")).clicked() {
            st.airspeed = p.v_cr;
        }
    });

    if let Some(unit) = setup.unit(st.group) {
        ui.label(
            RichText::new(tf(
                "propcurves.per_motor_fmt",
                &[unit.n_motors.to_string(), fmt(unit.pack.max_cont_a, 0)],
            ))
            .small()
            .color(theme::tokens(ui).text_weak),
        );
        let curve = sweep(unit, st.airspeed);
        match curve.first_limit_throttle() {
            Some(d) => {
                components::banner(
                    ui,
                    Status::Warn,
                    &tf("propcurves.limits_first_fmt", &[fmt(d * 100.0, 0)]),
                );
            }
            None => {
                components::banner(ui, Status::Ok, t("propcurves.limits_ok"));
            }
        }
        plots(ui, unit, &curve);
        egui::CollapsingHeader::new(t("propcurves.motor_title"))
            .id_salt("pc_motor_hdr")
            .default_open(false)
            .show(ui, |ui| {
                let idx = group_index(st.group);
                motor_panel(ui, unit, &mut st.overrides[idx]);
            });
        egui::CollapsingHeader::new(t("propcurves.table_title"))
            .id_salt("pc_table_hdr")
            .default_open(false)
            .show(ui, |ui| data_table(ui, &curve));
    }

    section_header(ui, t("propcurves.modes_title"));
    let key = summary_key(p, m, &setup);
    if st.summary.as_ref().map(|s| s.0) != Some(key) {
        st.summary = Some((key, flight_summary(p, m, &setup)));
    }
    if let Some((_, f)) = st.summary.as_ref() {
        modes_table(ui, p, f);
    }
    ui.data_mut(|d| d.insert_temp(id, st));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Library, evaluate};
    use crate::i18n::Lang;
    use crate::project::DroneType;
    use egui::{Pos2, Rect, vec2};

    #[test]
    fn panel_draws_for_every_vehicle_type_and_language() {
        let lib = Library::embedded();
        for dt in DroneType::ALL {
            let mut p = Project::default();
            p.set_drone_type(dt);
            let m = evaluate(&p, &lib);
            for lang in [Lang::En, Lang::Tr] {
                crate::i18n::with_lang(lang, || {
                    let ctx = egui::Context::default();
                    let mut input = egui::RawInput::default();
                    input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, vec2(1100.0, 800.0)));
                    let mut out = ctx.run_ui(input, |ui| {
                        egui::ScrollArea::vertical().show(ui, |ui| show(ui, &p, &m));
                    });
                    out.textures_delta.clear();
                    assert!(!out.shapes.is_empty(), "{dt:?} {lang:?}");
                });
            }
        }
    }
}
