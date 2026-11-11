// SPDX-License-Identifier: AGPL-3.0-or-later
//! Adım 6 / Sekme 6: Menzil & Havada Kalma: girdiler sol panel, batarya ve performans
//! tablosu sağ panel, güç/süre eğrileri merkez görünümün "Grafik" seçeneği.
use super::components::{NumberInput, Status, number_row, section::section_header};
use super::step_ui::{self, TableRow, card, cards};
use super::{theme, widgets::*};
use crate::calc::Model;
use crate::calc::performance::{self, PerfPoint};
use crate::i18n::t;
use crate::project::Project;
use egui::Ui;
use egui_plot::{Legend, Line, Plot, PlotPoints, Points, VLine};

/// Sol panel: batarya kullanım ve verim girdileri, seyir hızı.
pub fn inputs(ui: &mut Ui, p: &mut Project, _m: &Model) {
    section_header(ui, t("section_inputs"));
    let n = format!("= %{:.0}", p.dod * 100.0);
    number_row(
        ui,
        NumberInput::new(t("end.input.dod"), &mut p.dod)
            .range(0.3, 1.0)
            .decimals(2)
            .help(t("end.input.dod.desc"))
            .note(Some(n)),
    );
    number_row(
        ui,
        NumberInput::new(t("end.input.p_hotel"), &mut p.p_hotel)
            .unit("W")
            .range(0.0, 100.0)
            .decimals(1)
            .help(t("end.input.p_hotel.desc")),
    );
    number_row(
        ui,
        NumberInput::new(t("end.input.eta_p_cr"), &mut p.eta_p_cr)
            .range(0.3, 0.95)
            .decimals(2)
            .help(t("end.input.eta_p_cr.desc")),
    );
    number_row(
        ui,
        NumberInput::new(t("end.input.eta_mot"), &mut p.eta_mot)
            .range(0.5, 0.98)
            .decimals(2)
            .help(t("end.input.eta_mot.desc")),
    );
    number_row(
        ui,
        NumberInput::new(t("end.input.eta_esc"), &mut p.eta_esc)
            .range(0.5, 1.0)
            .decimals(2)
            .help(t("end.input.eta_esc.desc")),
    );
    let n = ms_to_kmh(p.v_cr);
    number_row(
        ui,
        NumberInput::new(t("end.input.v_cr"), &mut p.v_cr)
            .unit("m/s")
            .range(5.0, 60.0)
            .decimals(1)
            .help(t("end.input.v_cr.desc"))
            .note(Some(n)),
    );
}

/// Sağ panel: batarya kapasitesi ve hız noktaları performans tablosu.
pub fn results(ui: &mut Ui, _p: &mut Project, m: &Model) {
    section_header(ui, t("end.section_batt_capacity"));
    cards(
        ui,
        &[
            card(
                t("end.out.pack_energy"),
                fmt(m.weight.pack_wh, 0),
                "Wh",
                t("end.out.pack_energy.desc"),
            ),
            card(
                t("end.out.usable_energy"),
                fmt(performance::e_usable_j(&m.perf_in) / 3600.0, 1),
                "Wh",
                t("end.out.usable_energy.desc"),
            ),
            card(
                t("end.out.total_eff"),
                fmt(performance::eta_pt(&m.perf_in), 3),
                "",
                t("end.out.total_eff.desc"),
            ),
        ],
    );

    section_header(ui, t("end.section_perf_table"));
    let pts: [(&str, &PerfPoint); 3] = [
        (t("end.row_v_end"), &m.perf_end),
        (t("end.row_v_range"), &m.perf_range),
        (t("end.row_v_cruise"), &m.perf_cr),
    ];
    // Her hız noktasının başlıca sonuçları kart olarak; tam tablo katlanabilir bölümde.
    let summary: Vec<_> = pts
        .iter()
        .map(|(nm, pt)| {
            use super::step_ui::CardExt;
            card(
                nm,
                fmt(pt.endurance_min(), 0),
                t("ui.unit.min"),
                t("end.table_note"),
            )
            .d(format!(
                "{} km · {} m/s",
                fmt(pt.range_km(), 1),
                fmt(pt.v, 1)
            ))
        })
        .collect();
    cards(ui, &summary);
    let rows: Vec<TableRow> = pts
        .iter()
        .map(|(nm, pt)| TableRow {
            name: nm.to_string(),
            cells: vec![
                fmt(pt.v, 1),
                fmt(pt.v * 3.6, 0),
                fmt(pt.cl, 2),
                fmt(pt.ld, 1),
                fmt(pt.p_aero, 1),
                fmt(pt.p_prop_elec, 1),
                fmt(pt.p_total, 1),
                fmt(pt.endurance_min(), 0),
                fmt(pt.endurance_h(), 2),
                fmt(pt.range_km(), 1),
            ],
            // CL, CL_max'ın %85'ini aşıyorsa satır uyarı renginde (eski: amber CL).
            status: if pt.cl > 0.85 * m.aero.cl_max {
                Status::Warn
            } else {
                Status::Neutral
            },
        })
        .collect();
    step_ui::row_table(
        ui,
        "e_tab",
        t("end.col_speed_point"),
        &[
            "V [m/s]",
            "V [km/h]",
            "CL",
            "L/D",
            "P_aero [W]",
            t("end.col_p_prop_elec"),
            t("end.col_p_total"),
            t("end.col_time_min"),
            t("end.col_time_h"),
            t("end.col_range_km"),
        ],
        &rows,
        Some(t("end.table_note")),
    );
}

/// Merkez görünüm grafiği: gereken güç ve havada kalma/menzil – hız eğrileri.
pub fn chart(ui: &mut Ui, p: &Project, m: &Model, h: f32) {
    if !m.inputs_ok {
        step_ui::missing(ui);
        return;
    }
    let tk = theme::tokens(ui);
    let a = &m.aero;
    let v0 = a.v_stall.max(1.0);
    let v1 = (p.v_cr * 1.8).max(v0 * 1.5);
    let n = 140;
    let vs: Vec<f64> = (0..=n)
        .map(|i| v0 + (v1 - v0) * i as f64 / n as f64)
        .collect();
    let pts = |f: &dyn Fn(&PerfPoint) -> f64| -> Vec<[f64; 2]> {
        vs.iter()
            .map(|&v| [v, f(&performance::at_speed(v, &m.perf_in))])
            .collect()
    };
    let marks = [
        ("V_end", m.perf_end.v, tk.info),
        ("V_range", m.perf_range.v, tk.success),
        (t("end.plot_v_cruise"), m.perf_cr.v, tk.danger),
    ];
    let side = ui.available_width() > 2.0 * h.max(200.0);
    let ph = if side {
        (h - 24.0).max(80.0)
    } else {
        (h * 0.5 - 24.0).max(80.0)
    };
    let power = |ui: &mut Ui| {
        ui.label(t("end.plot_power_title"));
        Plot::new("p_req")
            .height(ph)
            .x_axis_label("V [m/s]")
            .y_axis_label("P [W]")
            .legend(Legend::default())
            .allow_scroll(false)
            .show(ui, |pu| {
                pu.line(
                    Line::new(
                        t("end.plot_total_electric"),
                        PlotPoints::from(pts(&|q| q.p_total)),
                    )
                    .color(tk.accent)
                    .width(2.0),
                );
                pu.line(
                    Line::new(
                        t("end.plot_thrust_only"),
                        PlotPoints::from(pts(&|q| q.p_prop_elec)),
                    )
                    .color(tk.text_weak)
                    .width(1.5),
                );
                pu.vline(VLine::new("V_stall", a.v_stall).color(tk.warning));
                for (nm, v, c) in marks {
                    pu.vline(VLine::new(nm, v).color(c));
                }
            });
    };
    let time = |ui: &mut Ui| {
        ui.label(t("end.plot_time_range_title"));
        Plot::new("end_rng")
            .height(ph)
            .x_axis_label("V [m/s]")
            .y_axis_label(t("end.plot_time_range_ylabel"))
            .legend(Legend::default())
            .allow_scroll(false)
            .show(ui, |pu| {
                pu.line(
                    Line::new(
                        t("end.col_time_min"),
                        PlotPoints::from(pts(&|q| q.endurance_min())),
                    )
                    .color(tk.accent)
                    .width(2.0),
                );
                pu.line(
                    Line::new(
                        t("end.col_range_km"),
                        PlotPoints::from(pts(&|q| q.range_km())),
                    )
                    .color(tk.success)
                    .width(2.0),
                );
                pu.vline(VLine::new("V_stall", a.v_stall).color(tk.warning));
                for (nm, v, c) in marks {
                    pu.points(
                        Points::new(
                            nm,
                            vec![[v, performance::at_speed(v, &m.perf_in).endurance_min()]],
                        )
                        .radius(5.0)
                        .color(c),
                    );
                }
            });
    };
    if side {
        ui.columns(2, |cols| {
            power(&mut cols[0]);
            time(&mut cols[1]);
        });
    } else {
        power(ui);
        time(ui);
    }
}

/// Girdiler + sonuçlar + grafik alt alta (başsız testler için).
pub fn show(ui: &mut Ui, p: &mut Project, m: &Model) {
    inputs(ui, p, m);
    results(ui, p, m);
    chart(ui, p, m, 400.0);
}
