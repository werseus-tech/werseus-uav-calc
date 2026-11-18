// SPDX-License-Identifier: AGPL-3.0-or-later
//! Itki tezgahi sekmesi: CSV ice aktarma, Ct / verim / gaz egrisi uydurmasi, tasarimla karsilastirma
//! ve SITL/HIL parametre dosyasi uretimi. Proje degerlerini DEGISTIRMEZ; durum egui bellegindedir.
use super::components::{self, Status, section_header};
use super::theme::{self, space};
use super::widgets::{fmt, section};
use crate::calc::Model;
use crate::i18n::{t, tf};
use crate::project::Project;
use crate::thruststand::csv::{
    self, ForceUnit, Mapping, RawTable, ThrottleUnit, Units, build_samples,
};
use crate::thruststand::fit::{self, G0, Verdict};
use crate::thruststand::sitl::{self, VehicleKind, VehicleSpec};
use crate::thruststand::{Analysis, CT_MODEL, DesignRef, analyze, demo_csv};
use egui::{RichText, Ui};
use egui_plot::{Legend, Line, Plot, Points};

/// Sekme durumu (egui belleginde saklanir; proje dosyasina yazilmaz).
#[derive(Clone)]
struct TsState {
    table: RawTable,
    mapping: Mapping,
    units: Units,
    file: String,
    recognized: bool,
    /// Motor KV; 0 = itki ve Ct'den tahmin et.
    kv: f64,
    use_measured: bool,
    status: Option<String>,
}

impl Default for TsState {
    fn default() -> Self {
        TsState {
            table: RawTable::default(),
            mapping: Mapping::default(),
            units: Units::default(),
            file: String::new(),
            recognized: false,
            kv: 0.0,
            use_measured: false,
            status: None,
        }
    }
}

impl TsState {
    fn load_text(&mut self, text: &str, name: &str) {
        self.table = csv::parse_table(text);
        let a = csv::auto_map(&self.table.headers);
        self.mapping = a.mapping;
        self.units = a.units;
        self.recognized = a.recognized;
        self.file = name.to_string();
        self.use_measured = false;
    }
}

fn col_combo(ui: &mut Ui, id: &str, label: &str, headers: &[String], sel: &mut Option<usize>) {
    ui.label(label);
    let shown = sel
        .and_then(|i| headers.get(i))
        .map(|s| s.as_str())
        .unwrap_or("—");
    egui::ComboBox::from_id_salt(id)
        .selected_text(shown)
        .width(220.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(sel, None, "—");
            for (i, h) in headers.iter().enumerate() {
                ui.selectable_value(sel, Some(i), h);
            }
        });
    ui.end_row();
}

fn save_text(name: &str, filter: (&str, &[&str]), text: &str) -> Option<String> {
    let path = rfd::FileDialog::new()
        .set_file_name(name)
        .add_filter(filter.0, filter.1)
        .save_file()?;
    Some(match std::fs::write(&path, text) {
        Ok(()) => tf("thruststand.saved", &[&path.display().to_string()]),
        Err(e) => tf("thruststand.save_failed", &[&e.to_string()]),
    })
}

pub fn show(ui: &mut Ui, p: &Project, m: &Model) {
    let id = egui::Id::unique("thruststand_state");
    let mut st: TsState = ui
        .data_mut(|d| d.get_temp::<TsState>(id))
        .unwrap_or_default();
    body(ui, p, m, &mut st);
    ui.data_mut(|d| d.insert_temp(id, st));
}

fn body(ui: &mut Ui, p: &Project, m: &Model, st: &mut TsState) {
    let tk = *theme::tokens(ui);
    section_header(ui, t("tab_thruststand"));
    components::banner(ui, Status::Info, t("thruststand.disclaimer"));
    ui.add_space(space::XS);

    // --- 1. Ice aktarma
    section(ui, t("thruststand.import_title"));
    ui.horizontal_wrapped(|ui| {
        if ui.button(t("thruststand.btn_open")).clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("CSV / TXT", &["csv", "txt", "tsv"])
                .pick_file()
            {
                match std::fs::read(&path) {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes).into_owned();
                        let name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        st.load_text(&text, &name);
                        st.status = None;
                    }
                    Err(e) => st.status = Some(tf("thruststand.read_failed", &[&e.to_string()])),
                }
            }
        }
        if ui.button(t("thruststand.btn_demo")).clicked() {
            st.load_text(demo_csv(), t("thruststand.demo_name"));
            st.status = None;
        }
        if ui
            .add_enabled(
                !st.table.headers.is_empty(),
                egui::Button::new(t("thruststand.btn_clear")),
            )
            .clicked()
        {
            *st = TsState {
                kv: st.kv,
                ..Default::default()
            };
        }
    });
    if let Some(s) = &st.status {
        ui.colored_label(tk.danger, s);
    }
    if st.table.headers.is_empty() {
        ui.label(RichText::new(t("thruststand.no_data")).color(tk.text_weak));
    } else {
        ui.label(tf(
            "thruststand.file_line",
            &[
                st.file.as_str(),
                &st.table.rows.len().to_string(),
                &st.table.headers.len().to_string(),
            ],
        ));
        if st.recognized {
            ui.colored_label(tk.success, t("thruststand.recognized"));
        } else {
            ui.colored_label(tk.warning, t("thruststand.not_recognized"));
        }
        mapping_ui(ui, st);
    }

    let report = build_samples(&st.table, &st.mapping, &st.units);
    if !st.table.headers.is_empty() {
        ui.label(tf(
            "thruststand.rows_line",
            &[
                &report.samples.len().to_string(),
                &report.skipped.to_string(),
            ],
        ));
        if report.skipped > 0 {
            ui.colored_label(tk.warning, t("thruststand.skipped_note"));
        }
    }

    let design = DesignRef::from_model(p, m);
    let an = analyze(&report.samples, &design);

    if !report.samples.is_empty() {
        results_ui(ui, &tk, &design, &an, st);
        plots_ui(ui, &tk, &report.samples, &design, &an);
    }

    sitl_ui(ui, p, m, st, &an);
    hil_ui(ui, &tk);
}

fn mapping_ui(ui: &mut Ui, st: &mut TsState) {
    egui::CollapsingHeader::new(t("thruststand.mapping_title"))
        .default_open(!st.recognized)
        .show(ui, |ui| {
            ui.label(t("thruststand.mapping_hint"));
            let headers = st.table.headers.clone();
            egui::Grid::new("ts_map").num_columns(2).show(ui, |ui| {
                col_combo(
                    ui,
                    "ts_c_thrust",
                    t("thruststand.col_thrust"),
                    &headers,
                    &mut st.mapping.thrust,
                );
                col_combo(
                    ui,
                    "ts_c_rpm",
                    t("thruststand.col_rpm"),
                    &headers,
                    &mut st.mapping.rpm,
                );
                col_combo(
                    ui,
                    "ts_c_volt",
                    t("thruststand.col_volt"),
                    &headers,
                    &mut st.mapping.volt,
                );
                col_combo(
                    ui,
                    "ts_c_curr",
                    t("thruststand.col_curr"),
                    &headers,
                    &mut st.mapping.curr,
                );
                col_combo(
                    ui,
                    "ts_c_thr",
                    t("thruststand.col_throttle"),
                    &headers,
                    &mut st.mapping.throttle,
                );
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(t("thruststand.unit_force"));
                egui::ComboBox::from_id_salt("ts_u_force")
                    .selected_text(st.units.force.label())
                    .show_ui(ui, |ui| {
                        for u in ForceUnit::ALL {
                            ui.selectable_value(&mut st.units.force, u, u.label());
                        }
                    });
                ui.label(t("thruststand.unit_throttle"));
                egui::ComboBox::from_id_salt("ts_u_thr")
                    .selected_text(st.units.throttle.label())
                    .show_ui(ui, |ui| {
                        for u in ThrottleUnit::ALL {
                            ui.selectable_value(&mut st.units.throttle, u, u.label());
                        }
                    });
                ui.label(t("thruststand.unit_fixed"));
            });
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(
                    &mut st.units.rpm_electrical,
                    t("thruststand.rpm_electrical"),
                );
                if st.units.rpm_electrical {
                    ui.label(t("thruststand.pole_pairs"));
                    ui.add(
                        egui::DragValue::new(&mut st.units.pole_pairs)
                            .range(1.0..=50.0)
                            .speed(0.1),
                    );
                }
            });
            if ui.button(t("thruststand.btn_automap")).clicked() {
                let a = csv::auto_map(&st.table.headers);
                st.mapping = a.mapping;
                st.units = a.units;
                st.recognized = a.recognized;
            }
        });
}

fn results_ui(ui: &mut Ui, tk: &theme::Tokens, d: &DesignRef, an: &Analysis, st: &mut TsState) {
    section(ui, t("thruststand.results_title"));
    egui::Grid::new("ts_res").num_columns(2).show(ui, |ui| {
        match &an.ct {
            Some(c) => {
                ui.label(t("thruststand.row_ct"));
                ui.label(format!(
                    "{}  (R² = {}, n = {})",
                    fmt(c.ct, 4),
                    fmt(c.r2, 4),
                    c.n
                ));
                ui.end_row();
                ui.label(t("thruststand.row_a"));
                ui.label(format!("{} N/RPM²", super::widgets::fmt_sci(c.a, 3)));
                ui.end_row();
            }
            None => {
                ui.label(t("thruststand.row_ct"));
                ui.label(t("thruststand.ct_missing"));
                ui.end_row();
            }
        }
        if let Some(best) = an.eff.iter().max_by(|a, b| a.g_per_w.total_cmp(&b.g_per_w)) {
            ui.label(t("thruststand.row_best_eff"));
            ui.label(tf(
                "thruststand.best_eff_fmt",
                &[&fmt(best.g_per_w, 2), &fmt(best.thrust_g, 0)],
            ));
            ui.end_row();
        }
        if let Some(thr) = an.throttle_curve.as_ref() {
            ui.label(t("thruststand.row_thr_curve"));
            ui.label(tf(
                "thruststand.thr_curve_fmt",
                &[&fmt(thr.eval(0.5).max(0.0) / G0 * 1000.0, 0)],
            ));
            ui.end_row();
        }
    });

    // Tasarimla karsilastirma
    ui.add_space(space::XS);
    ui.label(
        RichText::new(if d.is_hover {
            t("thruststand.cmp_title_hover")
        } else {
            t("thruststand.cmp_title_cruise")
        })
        .strong(),
    );
    ui.label(tf(
        "thruststand.design_line",
        &[
            &fmt(d.t_motor_n / G0 * 1000.0, 0),
            &fmt(d.p_elec_motor_w, 1),
        ],
    ));
    match (an.power_at_design, an.power_diff_pct) {
        (Some(pa), Some(pct)) => {
            ui.label(tf(
                "thruststand.cmp_power",
                &[&fmt(pa.power_w, 1), &fmt(pct, 1)],
            ));
            let (color, key) = match fit::verdict(pct) {
                Verdict::Close => (tk.success, "thruststand.verdict_close"),
                Verdict::MeasuredHigher => (tk.warning, "thruststand.verdict_higher"),
                Verdict::MeasuredLower => (tk.info, "thruststand.verdict_lower"),
            };
            ui.colored_label(color, t(key));
            if pa.extrapolated {
                ui.colored_label(tk.warning, t("thruststand.extrapolated"));
            }
        }
        _ => {
            ui.label(RichText::new(t("thruststand.cmp_power_missing")).color(tk.text_weak));
        }
    }
    if let Some(pct) = an.ct_diff_pct {
        ui.label(tf("thruststand.cmp_ct", &[&fmt(CT_MODEL, 2), &fmt(pct, 1)]));
        let key = match fit::verdict(pct) {
            Verdict::Close => "thruststand.ct_close",
            Verdict::MeasuredHigher => "thruststand.ct_higher",
            Verdict::MeasuredLower => "thruststand.ct_lower",
        };
        ui.label(RichText::new(t(key)).color(tk.text_weak));
    }

    // Oturumluk "olculen egriyi kullan" notu
    ui.add_space(space::XS);
    ui.checkbox(&mut st.use_measured, t("thruststand.use_measured"));
    if st.use_measured {
        components::banner(ui, Status::Info, t("thruststand.use_measured_note"));
        match (an.what_if, d.is_hover) {
            (Some(w), true) => {
                ui.label(tf(
                    "thruststand.whatif_line",
                    &[
                        &fmt(d.p_elec_total_w, 0),
                        &fmt(w.p_elec_total_w, 0),
                        &fmt(d.hover_endurance_min, 1),
                        &fmt(w.endurance_min, 1),
                    ],
                ));
            }
            _ => {
                ui.label(RichText::new(t("thruststand.whatif_na")).color(tk.text_weak));
            }
        }
    }
}

fn plots_ui(
    ui: &mut Ui,
    tk: &theme::Tokens,
    samples: &[csv::Sample],
    d: &DesignRef,
    an: &Analysis,
) {
    section(ui, t("thruststand.plots_title"));
    let g_of = |n: f64| n / G0 * 1000.0;
    let rpm_pts: Vec<[f64; 2]> = samples
        .iter()
        .filter_map(|s| s.rpm.map(|r| [r, g_of(s.thrust_n)]))
        .collect();
    let rpm_max = rpm_pts.iter().fold(0.0_f64, |m, p| m.max(p[0]));
    let curve = |a: f64| -> Vec<[f64; 2]> {
        (0..=40)
            .map(|i| {
                let r = rpm_max * i as f64 / 40.0;
                [r, g_of(a * r * r)]
            })
            .collect()
    };
    let a_model = if d.d_prop_m > 0.0 && d.rho > 0.0 {
        CT_MODEL * d.rho * d.d_prop_m.powi(4) / 3600.0
    } else {
        0.0
    };
    ui.columns(3, |c| {
        Plot::new("ts_plot_rpm")
            .height(230.0)
            .legend(Legend::default())
            .x_axis_label(t("thruststand.ax_rpm"))
            .y_axis_label(t("thruststand.ax_thrust_g"))
            .allow_scroll(false)
            .show(&mut c[0], |pu| {
                pu.points(
                    Points::new(t("thruststand.leg_measured"), rpm_pts.clone())
                        .color(tk.info)
                        .radius(3.0),
                );
                if let Some(ct) = &an.ct {
                    pu.line(
                        Line::new(t("thruststand.leg_fit"), curve(ct.a))
                            .color(tk.accent)
                            .width(2.0),
                    );
                }
                if a_model > 0.0 && rpm_max > 0.0 {
                    pu.line(
                        Line::new(t("thruststand.leg_model"), curve(a_model))
                            .color(tk.warning)
                            .width(2.0),
                    );
                }
            });
        let eff_pts: Vec<[f64; 2]> = an.eff.iter().map(|e| [e.thrust_g, e.g_per_w]).collect();
        Plot::new("ts_plot_eff")
            .height(230.0)
            .legend(Legend::default())
            .x_axis_label(t("thruststand.ax_thrust_g"))
            .y_axis_label(t("thruststand.ax_gpw"))
            .allow_scroll(false)
            .show(&mut c[1], |pu| {
                pu.points(
                    Points::new(t("thruststand.leg_measured"), eff_pts)
                        .color(tk.info)
                        .radius(3.0),
                );
                if d.p_elec_motor_w > 0.0 && d.t_motor_n > 0.0 {
                    pu.points(
                        Points::new(
                            t("thruststand.leg_model"),
                            vec![[g_of(d.t_motor_n), g_of(d.t_motor_n) / d.p_elec_motor_w]],
                        )
                        .color(tk.warning)
                        .radius(5.0),
                    );
                }
            });
        let thr_pts: Vec<[f64; 2]> = samples
            .iter()
            .filter_map(|s| s.throttle.map(|x| [x * 100.0, g_of(s.thrust_n)]))
            .collect();
        Plot::new("ts_plot_thr")
            .height(230.0)
            .legend(Legend::default())
            .x_axis_label(t("thruststand.ax_throttle"))
            .y_axis_label(t("thruststand.ax_thrust_g"))
            .allow_scroll(false)
            .show(&mut c[2], |pu| {
                pu.points(
                    Points::new(t("thruststand.leg_measured"), thr_pts)
                        .color(tk.info)
                        .radius(3.0),
                );
                if let Some(poly) = &an.throttle_curve {
                    let line: Vec<[f64; 2]> = (0..=40)
                        .map(|i| {
                            let x = i as f64 / 40.0;
                            [x * 100.0, g_of(poly.eval(x)).max(0.0)]
                        })
                        .collect();
                    pu.line(
                        Line::new(t("thruststand.leg_fit"), line)
                            .color(tk.accent)
                            .width(2.0),
                    );
                }
            });
    });
}

fn sitl_ui(ui: &mut Ui, p: &Project, m: &Model, st: &mut TsState, an: &Analysis) {
    let tk = *theme::tokens(ui);
    section(ui, t("thruststand.sitl_title"));
    components::banner(ui, Status::Info, t("thruststand.sitl_disclaimer"));
    ui.horizontal_wrapped(|ui| {
        ui.label(t("thruststand.kv_label"));
        ui.add(
            egui::DragValue::new(&mut st.kv)
                .range(0.0..=20000.0)
                .speed(5.0),
        );
        ui.label(RichText::new(t("thruststand.kv_hint")).color(tk.text_weak));
    });
    let kv = (st.kv > 0.0).then_some(st.kv);
    let ct = if st.use_measured {
        an.ct.map(|c| c.ct)
    } else {
        None
    };
    let spec = VehicleSpec::from_model(p, m, kv, ct);
    if spec.kind == VehicleKind::FixedWing {
        ui.label(RichText::new(t("thruststand.fixedwing_note")).color(tk.text_weak));
    }
    ui.label(tf(
        "thruststand.spec_line",
        &[
            &fmt(spec.mass_kg, 3),
            &spec.n_motors.to_string(),
            &format!("{}S", spec.cells_s),
            &fmt(spec.capacity_mah, 0),
            &fmt(spec.hover_throttle * 100.0, 0),
        ],
    ));
    let parm = sitl::ardupilot_parm(&spec);
    let px4 = sitl::px4_params(&spec);
    let gz = sitl::gazebo_snippet(&spec);
    let files: [(&str, &str, &str, &str, &str); 3] = [
        (
            "thruststand.btn_parm",
            "werseus_ardupilot_sitl.parm",
            "ArduPilot parm",
            "parm",
            &parm,
        ),
        (
            "thruststand.btn_px4",
            "werseus_px4_sitl_params.txt",
            "PX4 params",
            "txt",
            &px4,
        ),
        (
            "thruststand.btn_gazebo",
            "werseus_gazebo_snippet.txt",
            "Gazebo snippet",
            "txt",
            &gz,
        ),
    ];
    ui.horizontal_wrapped(|ui| {
        for (key, fname, flabel, ext, text) in files {
            if ui.button(t(key)).clicked() {
                if let Some(msg) = save_text(fname, (flabel, &[ext]), text) {
                    st.status = Some(msg);
                }
            }
        }
    });
    if let Some(s) = &st.status {
        ui.label(RichText::new(s).color(tk.text_weak));
    }
    for (key, text) in [
        ("thruststand.preview_parm", &parm),
        ("thruststand.preview_px4", &px4),
        ("thruststand.preview_gazebo", &gz),
    ] {
        egui::CollapsingHeader::new(t(key)).show(ui, |ui| {
            egui::ScrollArea::vertical()
                .max_height(180.0)
                .id_salt(key)
                .show(ui, |ui| {
                    let mut s: &str = text.as_str();
                    ui.add(
                        egui::TextEdit::multiline(&mut s)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY),
                    );
                });
        });
    }
}

fn hil_ui(ui: &mut Ui, tk: &theme::Tokens) {
    section(ui, t("thruststand.hil_title"));
    components::banner(ui, Status::Warn, t("thruststand.hil_safety"));
    for line in [
        t("thruststand.hil_1"),
        t("thruststand.hil_2"),
        t("thruststand.hil_3"),
        t("thruststand.hil_4"),
        t("thruststand.hil_5"),
        t("thruststand.hil_6"),
    ] {
        ui.label(format!("•  {line}"));
    }
    ui.label(RichText::new(t("thruststand.hil_note")).color(tk.text_weak));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Library, evaluate};
    use crate::i18n::Lang;
    use crate::project::DroneType;
    use crate::ui::theme::ThemeChoice;
    use egui::{Pos2, Rect, vec2};

    fn run(ctx: &egui::Context, f: &mut dyn FnMut(&mut Ui)) {
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, vec2(1100.0, 900.0)));
        let mut out = ctx.run_ui(input, |ui| f(ui));
        out.textures_delta.clear();
    }

    fn loaded(text: &str) -> TsState {
        let mut st = TsState::default();
        st.load_text(text, "test.csv");
        st
    }

    #[test]
    fn renders_in_both_themes_and_languages_with_and_without_data() {
        let rcb = include_str!("../../tests/fixtures/thrust_stand_rcbenchmark_sample.csv");
        let generic = include_str!("../../tests/fixtures/thrust_stand_generic_semicolon.csv");
        for choice in [ThemeChoice::Dark, ThemeChoice::Light] {
            for lang in [Lang::Tr, Lang::En] {
                for dt in DroneType::ALL {
                    let mut p = Project::default();
                    p.set_drone_type(dt);
                    let m = evaluate(&p, &Library::embedded());
                    for text in ["", rcb, generic, "garbage\u{0}\n1;;;\n,,,"] {
                        let ctx = egui::Context::default();
                        theme::install(&ctx, choice);
                        let mut st = loaded(text);
                        st.use_measured = true;
                        st.status = Some("x".into());
                        crate::i18n::with_lang(lang, || {
                            for _ in 0..2 {
                                run(&ctx, &mut |ui| body(ui, &p, &m, &mut st));
                            }
                        });
                    }
                }
            }
        }
    }

    #[test]
    fn show_keeps_state_in_memory_without_panic() {
        let p = Project::default();
        let m = evaluate(&p, &Library::embedded());
        let ctx = egui::Context::default();
        theme::install(&ctx, ThemeChoice::Dark);
        for _ in 0..3 {
            run(&ctx, &mut |ui| show(ui, &p, &m));
        }
    }

    #[test]
    fn dynamic_keys_exist_in_both_languages() {
        for key in [
            "thruststand.btn_parm",
            "thruststand.btn_px4",
            "thruststand.btn_gazebo",
            "thruststand.preview_parm",
            "thruststand.preview_px4",
            "thruststand.preview_gazebo",
            "thruststand.verdict_close",
            "thruststand.verdict_higher",
            "thruststand.verdict_lower",
            "thruststand.ct_close",
            "thruststand.ct_higher",
            "thruststand.ct_lower",
        ] {
            assert!(crate::i18n::tr_table().contains_key(key), "{key}");
            assert!(crate::i18n::en_table().contains_key(key), "{key}");
        }
    }

    #[test]
    fn every_thruststand_key_in_source_is_translated() {
        let src = include_str!("thruststand.rs");
        let prod = src.split("#[cfg(test)]").next().unwrap();
        let mut n = 0;
        for part in prod.split('"').skip(1).step_by(2) {
            if part.starts_with("thruststand.") || part == "tab_thruststand" {
                n += 1;
                assert!(crate::i18n::tr_table().contains_key(part), "{part}");
                assert!(crate::i18n::en_table().contains_key(part), "{part}");
            }
        }
        assert!(n > 60);
    }
}
