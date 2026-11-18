// SPDX-License-Identifier: AGPL-3.0-or-later
//! Onboard computer panel: compute-to-flight ratio and perception latency check.
//! Read-only with respect to the project; the entered values live in `CbState`.
use super::components::{self, Status};
use super::theme::{self, space};
use super::widgets::{fmt, out, out_c, section};
use crate::calc::Model;
use crate::computebudget::flight::{Baseline, flight_time_loss, payload_power_mass};
use crate::computebudget::latency::{self, LatencyIn, Verdict};
use crate::computebudget::{baseline_from_model, presets};
use crate::i18n::{t, tf};
use crate::project::Project;
use egui::Ui;

#[derive(Clone, Debug)]
struct SensorEntry {
    name: String,
    power_w: f64,
    mass_g: f64,
}

/// User-editable inputs of the panel (not saved in the project file).
#[derive(Clone, Debug)]
pub struct CbState {
    computer_name: String,
    idle_w: f64,
    load_w: f64,
    mass_g: f64,
    use_load: bool,
    sensors: Vec<SensorEntry>,
    lat: LatencyIn,
}

impl Default for CbState {
    fn default() -> Self {
        let c = presets::COMPUTERS[0];
        Self {
            computer_name: c.name.to_string(),
            idle_w: c.idle_w,
            load_w: c.load_w,
            mass_g: c.mass_g,
            use_load: true,
            sensors: Vec::new(),
            lat: LatencyIn::default(),
        }
    }
}

fn drag_range(ui: &mut Ui, label: &str, v: &mut f64, min: f64, max: f64, dec: usize) {
    ui.label(label);
    ui.add(
        egui::DragValue::new(v)
            .range(min..=max)
            .max_decimals(dec)
            .speed(0.1),
    );
    ui.end_row();
}

fn drag(ui: &mut Ui, label: &str, v: &mut f64, max: f64, dec: usize) {
    drag_range(ui, label, v, 0.0, max, dec);
}

pub fn show(ui: &mut Ui, p: &Project, m: &Model, st: &mut CbState) {
    let tk = theme::tokens(ui);
    components::section_header(ui, t("computebudget.title"));
    components::banner(ui, Status::Info, t("computebudget.estimate_note"));
    ui.add_space(space::XS);

    section(ui, t("computebudget.sec_ratio"));
    ui.label(t("computebudget.computer"));
    ui.horizontal(|ui| {
        ui.label(t("computebudget.preset"));
        egui::ComboBox::from_id_salt("cb_computer_preset")
            .selected_text(st.computer_name.clone())
            .show_ui(ui, |ui| {
                for c in presets::COMPUTERS {
                    if ui
                        .selectable_label(st.computer_name == c.name, c.name)
                        .clicked()
                    {
                        st.computer_name = c.name.to_string();
                        st.idle_w = c.idle_w;
                        st.load_w = c.load_w;
                        st.mass_g = c.mass_g;
                    }
                }
            });
        ui.weak(t("computebudget.preset_note"));
    });
    egui::Grid::new("cb_computer_grid")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label(t("computebudget.name"));
            ui.text_edit_singleline(&mut st.computer_name);
            ui.end_row();
            drag(ui, t("computebudget.idle_w"), &mut st.idle_w, 1.0e3, 2);
            if st.load_w < st.idle_w {
                st.load_w = st.idle_w;
            }
            drag_range(
                ui,
                t("computebudget.load_w"),
                &mut st.load_w,
                st.idle_w,
                1.0e3,
                2,
            );
            drag(ui, t("computebudget.mass_g"), &mut st.mass_g, 1.0e5, 1);
        });
    ui.checkbox(&mut st.use_load, t("computebudget.use_load"));

    ui.add_space(space::XS);
    ui.label(t("computebudget.sensors"));
    let mut remove = None;
    egui::Grid::new("cb_sensor_grid")
        .num_columns(4)
        .show(ui, |ui| {
            for (i, s) in st.sensors.iter_mut().enumerate() {
                ui.text_edit_singleline(&mut s.name);
                ui.add(
                    egui::DragValue::new(&mut s.power_w)
                        .range(0.0..=1.0e3)
                        .max_decimals(2)
                        .suffix(" W"),
                );
                ui.add(
                    egui::DragValue::new(&mut s.mass_g)
                        .range(0.0..=1.0e5)
                        .max_decimals(1)
                        .suffix(" g"),
                );
                if ui.button(t("computebudget.remove")).clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
    if let Some(i) = remove {
        st.sensors.remove(i);
    }
    ui.horizontal_wrapped(|ui| {
        ui.label(t("computebudget.add_sensor"));
        for s in presets::SENSORS {
            if ui.small_button(s.name).clicked() {
                st.sensors.push(SensorEntry {
                    name: s.name.to_string(),
                    power_w: s.power_w,
                    mass_g: s.mass_g,
                });
            }
        }
        if ui.small_button(t("computebudget.preset_custom")).clicked() {
            st.sensors.push(SensorEntry {
                name: String::new(),
                power_w: 1.0,
                mass_g: 20.0,
            });
        }
    });

    let comp_w = if st.use_load { st.load_w } else { st.idle_w };
    let sensors: Vec<(f64, f64)> = st.sensors.iter().map(|s| (s.power_w, s.mass_g)).collect();
    let (power_w, mass_kg) = payload_power_mass(comp_w, st.mass_g, &sensors);

    ui.add_space(space::XS);
    match baseline_from_model(p.drone_type, m) {
        None => {
            components::banner(ui, Status::Warn, t("computebudget.no_baseline"));
        }
        Some(base) => {
            let note = match &base {
                Baseline::Wing { v_cruise, .. } => {
                    tf("computebudget.baseline_wing", &[fmt(*v_cruise, 1)])
                }
                Baseline::Rotor { .. } => t("computebudget.baseline_rotor").to_string(),
            };
            ui.weak(note);
            let r = flight_time_loss(&base, power_w, mass_kg);
            egui::Grid::new("cb_ratio_out")
                .num_columns(3)
                .striped(true)
                .show(ui, |ui| {
                    out(
                        ui,
                        t("computebudget.payload_power"),
                        fmt(power_w, 2),
                        "W",
                        "",
                    );
                    out(
                        ui,
                        t("computebudget.payload_mass"),
                        fmt(mass_kg * 1000.0, 0),
                        "g",
                        "",
                    );
                    out(
                        ui,
                        t("computebudget.t_base"),
                        fmt(r.t_base_min, 1),
                        "min",
                        "",
                    );
                    out(
                        ui,
                        t("computebudget.t_with"),
                        fmt(r.t_with_min, 1),
                        "min",
                        "",
                    );
                    let c = if r.lost_pct > 25.0 {
                        tk.danger
                    } else if r.lost_pct > 10.0 {
                        tk.warning
                    } else {
                        tk.success
                    };
                    out_c(
                        ui,
                        t("computebudget.t_lost"),
                        format!("{} ({} %)", fmt(r.lost_min, 1), fmt(r.lost_pct, 1)),
                        "min",
                        "",
                        c,
                    );
                });
            if r.thrust_exceeded {
                components::banner(ui, Status::Err, t("computebudget.warn_thrust_exceeded"));
            }
            if r.stall_exceeded {
                components::banner(ui, Status::Err, t("computebudget.warn_stall_exceeded"));
            }
            if r.power_exceeded {
                components::banner(ui, Status::Err, t("computebudget.warn_power_exceeded"));
            }
            if r.vtol_hover_omitted {
                components::banner(ui, Status::Warn, t("computebudget.warn_vtol_hover"));
            }
        }
    }

    ui.add_space(space::M);
    section(ui, t("computebudget.sec_latency"));
    egui::Grid::new("cb_latency_in")
        .num_columns(2)
        .show(ui, |ui| {
            let l = &mut st.lat;
            drag_range(ui, t("computebudget.fps"), &mut l.fps, 0.1, 1.0e4, 1);
            drag(
                ui,
                t("computebudget.exposure"),
                &mut l.exposure_readout_ms,
                1.0e4,
                1,
            );
            drag(
                ui,
                t("computebudget.inference"),
                &mut l.inference_ms,
                1.0e4,
                1,
            );
            drag(ui, t("computebudget.comm"), &mut l.comm_ms, 1.0e4, 1);
            drag(ui, t("computebudget.fc_loop"), &mut l.fc_loop_ms, 1.0e4, 1);
            drag(ui, t("computebudget.speed"), &mut l.speed_ms, 1.0e3, 1);
            drag(
                ui,
                t("computebudget.range"),
                &mut l.detect_range_m,
                1.0e5,
                1,
            );
            drag_range(
                ui,
                t("computebudget.decel"),
                &mut l.max_decel_ms2,
                0.01,
                1.0e3,
                2,
            );
        });
    let r = latency::evaluate(&st.lat);
    ui.add_space(space::XS);
    egui::Grid::new("cb_latency_out")
        .num_columns(3)
        .striped(true)
        .show(ui, |ui| {
            out(
                ui,
                t("computebudget.r_frame_wait"),
                fmt(r.frame_wait_ms, 1),
                "ms",
                "",
            );
            out(
                ui,
                t("computebudget.r_total_latency"),
                fmt(r.total_latency_ms, 1),
                "ms",
                "",
            );
            out(
                ui,
                t("computebudget.r_blind"),
                fmt(r.blind_distance_m, 2),
                "m",
                "",
            );
            out(
                ui,
                t("computebudget.r_brake"),
                fmt(r.braking_distance_m, 2),
                "m",
                "",
            );
            out(
                ui,
                t("computebudget.r_stop"),
                fmt(r.stopping_distance_m, 2),
                "m",
                "",
            );
            out(ui, t("computebudget.r_margin"), fmt(r.margin_m, 2), "m", "");
            out(
                ui,
                t("computebudget.r_max_speed"),
                fmt(r.max_safe_speed_ms, 2),
                "m/s",
                "",
            );
        });
    let (status, key) = match r.verdict {
        Verdict::Pass => (Status::Ok, "computebudget.verdict_pass"),
        Verdict::Warn => (Status::Warn, "computebudget.verdict_warn"),
        Verdict::Fail => (Status::Err, "computebudget.verdict_fail"),
    };
    ui.add_space(space::XS);
    ui.label(t("computebudget.verdict"));
    components::banner(ui, status, t(key));
    if r.inference_bottleneck {
        components::banner(
            ui,
            Status::Warn,
            t("computebudget.warn_inference_bottleneck"),
        );
    }
}
