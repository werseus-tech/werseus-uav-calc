// SPDX-License-Identifier: AGPL-3.0-or-later
//! Adım 4 / Sekme 4: Aerodinamik: girdiler sol panel, sonuçlar ve CD0 dökümü sağ panel,
//! polar ve L/D grafikleri merkez görünümün "Grafik" seçeneği. Alanlar, aralıklar ve ondalıklar
//! eski `fld` satırlarıyla birebir aynıdır.
use super::components::{NumberInput, Status, number_row, section::section_header};
use super::step_ui::{self, CardExt, TableRow, card, cards};
use super::{theme, widgets::*};
use crate::calc::Model;
use crate::calc::aero::{cd_total, cl_for, ld_at};
use crate::i18n::{t, tf};
use crate::project::Project;
use egui::Ui;
use egui_plot::{Legend, Line, Plot, PlotPoints, Points, VLine};

fn ov(p: &Project) -> crate::calc::aero_presets::AeroValues {
    crate::calc::aero_presets::AeroValues {
        cl_max_2d: p.cl_max_2d,
        cd_misc: p.cd_misc,
        s_wet_fuse: p.s_wet_fuse,
    }
}

/// Sol panel: basit modda sorular, gelişmiş modda ham aerodinamik girdiler.
pub fn inputs(ui: &mut Ui, p: &mut Project, m: &Model) {
    if p.aero_simple.simple_mode {
        super::aero_simple::aero_inputs(ui, p, m);
        return;
    }
    let ov_before = ov(p);
    section_header(ui, t("section_inputs"));
    number_row(
        ui,
        NumberInput::new(t("aero.input.cl_alpha_2d"), &mut p.cl_alpha_2d)
            .unit("1/rad")
            .range(4.0, 7.0)
            .decimals(2)
            .help(t("aero.input.cl_alpha_2d.desc")),
    );
    number_row(
        ui,
        NumberInput::new(t("aero.input.cl_max_2d"), &mut p.cl_max_2d)
            .range(0.6, 2.0)
            .decimals(2)
            .help(t("aero.input.cl_max_2d.desc")),
    );
    number_row(
        ui,
        NumberInput::new(t("aero.input.l_fuse"), &mut p.l_fuse)
            .scale(1000.0)
            .unit("mm")
            .range(300.0, 2000.0)
            .decimals(0)
            .help(t("aero.input.l_fuse.desc")),
    );
    number_row(
        ui,
        NumberInput::new(t("aero.input.d_fuse"), &mut p.d_fuse)
            .scale(1000.0)
            .unit("mm")
            .range(40.0, 300.0)
            .decimals(0)
            .help(t("aero.input.d_fuse.desc")),
    );
    number_row(
        ui,
        NumberInput::new(t("aero.input.s_wet_fuse"), &mut p.s_wet_fuse)
            .unit("m²")
            .range(0.05, 0.8)
            .decimals(3)
            .help(t("aero.input.s_wet_fuse.desc")),
    );
    number_row(
        ui,
        NumberInput::new(t("aero.input.cd_misc"), &mut p.cd_misc)
            .range(0.0, 0.03)
            .decimals(4)
            .help(t("aero.input.cd_misc.desc")),
    );
    number_row(
        ui,
        NumberInput::new(t("aero.input.q_tail"), &mut p.q_tail)
            .range(1.0, 1.5)
            .decimals(2)
            .help(t("aero.input.q_tail.desc")),
    );
    ui.horizontal_wrapped(|ui| {
        ui.label(tf("aero.geo_fuse_wet_est", &[fmt(m.fuse_wet_geo, 3)]));
        if ui.button(t("aero.btn_use_this")).clicked() && m.fuse_wet_geo.is_finite() {
            p.s_wet_fuse = m.fuse_wet_geo;
        }
    });
    step_ui::note(ui, t("aero.wing_tail_tc_note"));
    // Gelişmiş modda elle değiştirilen değerler basit modda "özelleştirildi" olarak korunur.
    let ov_after = ov(p);
    p.aero_simple.mark_overrides(&ov_before, &ov_after);
}

/// Sağ panel: aerodinamik çıktılar ve CD0 dökümü (basit modda günlük dilde sonuçlar).
pub fn results(ui: &mut Ui, p: &mut Project, m: &Model) {
    if p.aero_simple.simple_mode {
        super::aero_simple::aero_results(ui, p, m);
        return;
    }
    section_header(ui, t("section_outputs"));
    let a = &m.aero;
    let clamped_suffix = if a.end_clamped {
        t("aero.out.v_end_clamped")
    } else {
        ""
    };
    cards(
        ui,
        &[
            card(
                t("aero.out.cla_w"),
                fmt(a.cla_w, 3),
                "1/rad",
                t("aero.out.cla_w.desc"),
            )
            .d(format!("{} 1/°", fmt(a.cla_w.to_radians(), 4))),
            card(
                t("aero.out.eta_af"),
                fmt(a.eta_af, 3),
                "",
                t("aero.out.eta_af.desc"),
            ),
            card(
                t("aero.out.cl_max_3d"),
                fmt(a.cl_max, 3),
                "",
                t("aero.out.cl_max_3d.desc"),
            ),
            card(
                t("aero.out.oswald_e"),
                fmt(a.e, 3),
                "",
                t("aero.out.oswald_e.desc"),
            ),
            card(t("aero.out.k"), fmt(a.k, 4), "", t("aero.out.k.desc")),
            card(
                t("aero.out.cd0_total"),
                fmt(a.cd0, 4),
                "",
                t("aero.out.cd0_total.desc"),
            ),
            card(
                t("aero.out.ld_max"),
                fmt(a.ld_max, 1),
                "",
                t("aero.out.ld_max.desc"),
            ),
            card(
                t("aero.out.v_stall"),
                fmt(a.v_stall, 2),
                "m/s",
                t("aero.out.v_stall.desc"),
            )
            .d(format!("{} km/h", fmt(a.v_stall * 3.6, 1))),
            card(
                t("aero.out.v_range"),
                fmt(a.v_range, 2),
                "m/s",
                t("aero.out.v_range.desc"),
            )
            .d(format!("{} km/h", fmt(a.v_range * 3.6, 1))),
            card(
                t("aero.out.v_endurance"),
                fmt(a.v_end, 2),
                "m/s",
                t("aero.out.v_endurance.desc"),
            )
            .d(format!("{} km/h{}", fmt(a.v_end * 3.6, 1), clamped_suffix))
            .s(if a.end_clamped {
                Status::Warn
            } else {
                Status::Neutral
            }),
            card(
                t("aero.out.v_end_free"),
                fmt(a.v_end_unclamped, 2),
                "m/s",
                t("aero.out.v_end_free.desc"),
            ),
            card(
                t("aero.out.cl_range_cl_end"),
                format!("{} / {}", fmt(a.cl_range, 3), fmt(a.cl_end, 3)),
                "",
                t("aero.out.cl_range_cl_end.desc"),
            ),
        ],
    );

    // CD0 dökümü (geniş tablo → satır kutuları)
    let mut rows: Vec<TableRow> = a
        .items
        .iter()
        .map(|it| TableRow {
            name: it.name.to_string(),
            cells: vec![
                fmt_sci(it.re, 2),
                fmt(it.cf, 5),
                fmt(it.ff, 3),
                fmt(it.q, 2),
                fmt(it.s_wet, 3),
                fmt(it.cd0, 5),
                format!("%{}", fmt(it.cd0 / a.cd0 * 100.0, 0)),
            ],
            status: Status::Neutral,
        })
        .collect();
    rows.push(TableRow {
        name: t("aero.table_total").to_string(),
        cells: vec![
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            fmt(a.cd0, 5),
            "%100".into(),
        ],
        status: Status::Info,
    });
    step_ui::row_table(
        ui,
        "a_cd0",
        t("aero.section_cd0_breakdown"),
        &[
            "Re",
            "Cf",
            "FF",
            "Q",
            t("aero.table_s_wet"),
            "CD0",
            t("aero.table_share"),
        ],
        &rows,
        None,
    );
}

/// Merkez görünüm grafiği: polar ve L/D–hız eğrisi (geniş alanda yan yana, dar alanda alt alta).
pub fn chart(ui: &mut Ui, p: &Project, m: &Model, h: f32) {
    if !m.inputs_ok {
        step_ui::missing(ui);
        return;
    }
    let side = ui.available_width() > 2.0 * h.max(200.0);
    if side {
        ui.columns(2, |cols| {
            polar_plot(&mut cols[0], p, m, h - 24.0);
            ld_plot(&mut cols[1], p, m, h - 24.0);
        });
    } else {
        let hh = (h * 0.5 - 24.0).max(80.0);
        polar_plot(ui, p, m, hh);
        ld_plot(ui, p, m, hh);
    }
}

fn polar_plot(ui: &mut Ui, p: &Project, m: &Model, h: f32) {
    let tk = theme::tokens(ui);
    ui.label(t("aero.plot_polar_title"));
    let a = &m.aero;
    let n = 120;
    let pts: Vec<[f64; 2]> = (0..=n)
        .map(|i| {
            let cl = a.cl_max * i as f64 / n as f64;
            [cd_total(a.cd0, a.k, cl), cl]
        })
        .collect();
    let cl_cr = cl_for(m.ws, m.atmo.rho, p.v_cr);
    Plot::new("polar")
        .height(h.max(80.0))
        .x_axis_label("CD")
        .y_axis_label("CL")
        .legend(Legend::default())
        .allow_scroll(false)
        .show(ui, |pu| {
            pu.line(
                Line::new(t("aero.plot_polar_curve"), PlotPoints::from(pts))
                    .color(tk.accent)
                    .width(2.0),
            );
            pu.points(
                Points::new(
                    t("aero.plot_cruise"),
                    vec![[cd_total(a.cd0, a.k, cl_cr), cl_cr]],
                )
                .radius(6.0)
                .color(tk.danger),
            );
            pu.points(
                Points::new(
                    t("aero.plot_ld_max"),
                    vec![[cd_total(a.cd0, a.k, a.cl_range), a.cl_range]],
                )
                .radius(6.0)
                .color(tk.success),
            );
            pu.points(
                Points::new(
                    t("aero.plot_cl_max_stall"),
                    vec![[cd_total(a.cd0, a.k, a.cl_max), a.cl_max]],
                )
                .radius(6.0)
                .color(tk.warning),
            );
        });
}

fn ld_plot(ui: &mut Ui, p: &Project, m: &Model, h: f32) {
    let tk = theme::tokens(ui);
    ui.label(t("aero.plot_ld_title"));
    let a = &m.aero;
    let v0 = a.v_stall.max(1.0);
    let v1 = (p.v_cr * 1.8).max(v0 * 1.5);
    let n = 140;
    let pts: Vec<[f64; 2]> = (0..=n)
        .map(|i| {
            let v = v0 + (v1 - v0) * i as f64 / n as f64;
            [v, ld_at(a.cd0, a.k, cl_for(m.ws, m.atmo.rho, v))]
        })
        .collect();
    Plot::new("ld_v")
        .height(h.max(80.0))
        .x_axis_label("V [m/s]")
        .y_axis_label("L/D")
        .legend(Legend::default())
        .allow_scroll(false)
        .show(ui, |pu| {
            pu.line(
                Line::new("L/D", PlotPoints::from(pts))
                    .color(tk.accent)
                    .width(2.0),
            );
            pu.vline(VLine::new("V_stall", a.v_stall).color(tk.warning));
            pu.vline(VLine::new("V_range", a.v_range).color(tk.success));
            pu.vline(VLine::new("V_endurance", a.v_end).color(tk.info));
            pu.vline(VLine::new(t("aero.plot_v_cruise"), p.v_cr).color(tk.danger));
        });
}

/// Girdiler + sonuçlar + grafik alt alta (başsız testler için).
pub fn show(ui: &mut Ui, p: &mut Project, m: &Model) {
    inputs(ui, p, m);
    results(ui, p, m);
    chart(ui, p, m, 400.0);
}
