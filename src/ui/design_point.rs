// SPDX-License-Identifier: AGPL-3.0-or-later
//! Adım 5 / Sekme 5: Tasarım Noktası: gereksinimler ve verimler sol panel, kısıt sonuçları
//! sağ panel, kısıt diyagramı merkez görünümün "Grafik" seçeneği. Alanlar, aralıklar ve
//! ondalıklar eski `fld` satırlarıyla birebir aynıdır.
use super::components::{self, NumberInput, Status, number_row, section::section_header};
use super::step_ui::{self, CardExt, TableRow, card, cards};
use super::{theme, widgets::*};
use crate::calc::Model;
use crate::calc::constraint::{self, RecCriterion};
use crate::i18n::{t, tf};
use crate::project::Project;
use egui::Ui;
use egui_plot::{Legend, Line, Plot, PlotBounds, PlotPoints, Points, Polygon, VLine};

#[allow(clippy::too_many_arguments)]
fn row(
    ui: &mut Ui,
    label: &str,
    v: &mut f64,
    unit: &str,
    help: &str,
    lo: f64,
    hi: f64,
    dec: usize,
    kmh: bool,
) {
    let note = kmh.then(|| ms_to_kmh(*v));
    number_row(
        ui,
        NumberInput::new(label, v)
            .unit(unit)
            .range(lo, hi)
            .decimals(dec)
            .help(help)
            .note(note),
    );
}

/// Sol panel: görev gereksinimleri, verimler ve öneri ölçütü (basit modda hız soruları).
pub fn inputs(ui: &mut Ui, p: &mut Project, m: &Model) {
    if p.aero_simple.simple_mode {
        super::aero_simple::design_point_inputs(ui, p, m);
        return;
    }
    section_header(ui, t("dp.section_mission_reqs"));
    row(
        ui,
        t("dp.input.v_stall_req"),
        &mut p.v_stall_req,
        "m/s",
        t("dp.input.v_stall_req.desc"),
        5.0,
        20.0,
        1,
        true,
    );
    row(
        ui,
        t("dp.input.rc"),
        &mut p.rc,
        "m/s",
        t("dp.input.rc.desc"),
        0.5,
        10.0,
        1,
        false,
    );
    row(
        ui,
        t("dp.input.v_y"),
        &mut p.v_y,
        "m/s",
        t("dp.input.v_y.desc"),
        5.0,
        30.0,
        1,
        true,
    );
    row(
        ui,
        t("dp.input.phi_turn"),
        &mut p.phi_turn_deg,
        "°",
        t("dp.input.phi_turn.desc"),
        10.0,
        60.0,
        0,
        false,
    );
    row(
        ui,
        t("dp.input.v_turn"),
        &mut p.v_turn,
        "m/s",
        t("dp.input.v_turn.desc"),
        5.0,
        40.0,
        1,
        true,
    );
    row(
        ui,
        t("dp.input.v_launch"),
        &mut p.v_launch,
        "m/s",
        t("dp.input.v_launch.desc"),
        5.0,
        25.0,
        1,
        true,
    );
    row(
        ui,
        t("dp.input.gamma_launch"),
        &mut p.gamma_launch_deg,
        "°",
        t("dp.input.gamma_launch.desc"),
        0.0,
        45.0,
        0,
        false,
    );
    row(
        ui,
        t("dp.input.a_launch"),
        &mut p.a_launch,
        "m/s²",
        t("dp.input.a_launch.desc"),
        0.0,
        5.0,
        1,
        false,
    );

    section_header(ui, t("dp.section_efficiencies"));
    row(
        ui,
        t("dp.input.eta_p_cr"),
        &mut p.eta_p_cr,
        "",
        t("dp.input.eta_p_cr.desc"),
        0.3,
        0.95,
        2,
        false,
    );
    row(
        ui,
        t("dp.input.eta_p_climb"),
        &mut p.eta_p_climb,
        "",
        t("dp.input.eta_p_climb.desc"),
        0.3,
        0.95,
        2,
        false,
    );
    row(
        ui,
        t("dp.input.eta_mot"),
        &mut p.eta_mot,
        "",
        t("dp.input.eta_mot.desc"),
        0.5,
        0.98,
        2,
        false,
    );
    row(
        ui,
        t("dp.input.eta_esc"),
        &mut p.eta_esc,
        "",
        t("dp.input.eta_esc.desc"),
        0.5,
        1.0,
        2,
        false,
    );
    row(
        ui,
        t("dp.input.pw_margin"),
        &mut p.pw_margin,
        "×",
        t("dp.input.pw_margin.desc"),
        1.0,
        2.0,
        2,
        false,
    );
    row(
        ui,
        t("dp.input.ws_margin"),
        &mut p.ws_margin,
        "×",
        t("dp.input.ws_margin.desc"),
        0.5,
        1.0,
        2,
        false,
    );
    row(
        ui,
        t("dp.input.p_max_elec"),
        &mut p.p_max_elec,
        "W",
        t("dp.input.p_max_elec.desc"),
        20.0,
        2000.0,
        0,
        false,
    );

    let crit: Vec<(RecCriterion, &str)> =
        RecCriterion::ALL.iter().map(|c| (*c, c.label())).collect();
    step_ui::combo(
        ui,
        "rec_criterion",
        t("dp.rec_criterion_label"),
        "",
        &mut p.rec_criterion,
        &crit,
    );
    step_ui::note(ui, t("dp.rec_criterion_explanation"));
}

/// Sağ panel: kısıt durumu, kısıt tablosu ve tasarım noktası çıktıları.
pub fn results(ui: &mut Ui, p: &mut Project, m: &Model) {
    if p.aero_simple.simple_mode {
        super::aero_simple::design_point_results(ui, p, m);
        return;
    }
    // Kısıt hataları kabuğun "bu adımın uyarıları" listesinde (sağ panelde hemen altta) çıkar;
    // hiç hata yoksa eski ekrandaki "tüm kısıtlar sağlanıyor" şeridi gösterilir.
    let any = m
        .warnings
        .iter()
        .any(|w| w.area_key == "area.design_point" && w.sev == crate::calc::Sev::Err);
    if !any && m.inputs_ok {
        components::banner(ui, Status::Ok, t("dp.all_constraints_satisfied"));
    }

    let c = &m.cons;
    let rows: Vec<TableRow> = [&c.cruise, &c.climb, &c.turn, &c.launch]
        .iter()
        .enumerate()
        .map(|(i, pt)| TableRow {
            name: constraint::name(i).to_string(),
            cells: vec![
                fmt(pt.q, 1),
                fmt(pt.cl, 3),
                fmt(pt.cd, 4),
                fmt(pt.ld, 1),
                fmt(pt.pw, 2),
            ],
            status: if i == c.critical {
                Status::Warn
            } else {
                Status::Neutral
            },
        })
        .collect();
    section_header(ui, t("dp.section_current_constraints"));
    step_ui::row_table(
        ui,
        "d_out",
        t("dp.col_constraint"),
        &["q [Pa]", "CL", "CD", "L/D", "P/W [W/N]"],
        &rows,
        Some(t("dp.crit_constraint_note")),
    );
    cards(
        ui,
        &[
            card(t("dp.out.ws"), fmt(c.ws, 1), "N/m²", t("dp.out.ws.desc")),
            card(
                t("dp.out.ws_max"),
                fmt(c.ws_max, 1),
                "N/m²",
                t("dp.out.ws_max.desc"),
            )
            .d(tf("dp.out.ws_allowed_fmt", &[fmt(c.ws_allowed, 1)])),
            card(
                "V_stall",
                fmt(c.v_stall, 2),
                "m/s",
                t("dp.out.v_stall.desc"),
            )
            .d(tf(
                "dp.out.v_stall_req_fmt",
                &[format!("{:.1}", p.v_stall_req)],
            ))
            .s(step_ui::ok(c.stall_ok)),
            card(
                t("dp.out.crit_constraint"),
                constraint::name(c.critical).to_string(),
                "",
                t("dp.out.crit_constraint.desc"),
            ),
            card(
                t("dp.out.pw_req"),
                fmt(c.pw_req, 2),
                "W/N",
                t("dp.out.pw_req.desc"),
            ),
            card(
                t("dp.out.pw_design"),
                fmt(c.pw_design, 2),
                "W/N",
                t("dp.out.pw_design.desc"),
            ),
            card(
                t("dp.out.p_shaft"),
                fmt(c.p_shaft, 0),
                "W",
                t("dp.out.p_shaft.desc"),
            ),
            card(
                t("dp.out.p_elec"),
                fmt(c.p_elec, 0),
                "W",
                t("dp.out.p_elec.desc"),
            ),
            card(
                t("dp.out.installed_pw"),
                fmt(c.installed_pw, 2),
                "W/N",
                t("dp.out.installed_pw.desc"),
            )
            .d(tf("dp.out.installed_pw_fmt", &[fmt(p.p_max_elec, 0)]))
            .s(step_ui::ok(c.power_ok)),
            card(
                t("dp.out.rec_ws"),
                fmt(c.rec_ws, 1),
                "N/m²",
                t("dp.out.rec_ws.desc"),
            ),
            card(
                t("dp.out.rec_pw"),
                fmt(c.rec_pw, 2),
                "W/N",
                t("dp.out.rec_pw.desc"),
            ),
            card(
                t("dp.out.rec_s"),
                fmt(m.weight.w / c.rec_ws, 3),
                "m²",
                t("dp.out.rec_s.desc"),
            ),
            card(
                t("dp.out.rec_min_power_pt"),
                format!("{} / {}", fmt(c.rec_min_ws, 1), fmt(c.rec_min_pw, 2)),
                "",
                t("dp.out.rec_min_power_pt.desc"),
            )
            .d("N/m² / W/N"),
            card(
                t("dp.out.rec_max_ws_pt"),
                format!("{} / {}", fmt(c.rec_max_ws, 1), fmt(c.rec_max_pw, 2)),
                "",
                t("dp.out.rec_max_ws_pt.desc"),
            )
            .d("N/m² / W/N"),
            card(
                t("dp.out.min_s_margin"),
                fmt(c.s_min_margin, 3),
                "m²",
                t("dp.out.min_s_margin.desc"),
            ),
        ],
    );
}

/// Merkez görünüm grafiği: kısıt diyagramı (W/S – P/W).
pub fn chart(ui: &mut Ui, p: &Project, m: &Model, h: f32) {
    if !m.inputs_ok {
        step_ui::missing(ui);
        return;
    }
    let tk = theme::tokens(ui);
    let c = &m.cons;
    let i = &m.cons_in;
    let x_lo = 20.0;
    let x_hi = (c.ws_max * 1.45).max(c.ws * 1.2).max(90.0);
    let n = 160;
    let xs: Vec<f64> = (0..=n)
        .map(|k| x_lo + (x_hi - x_lo) * k as f64 / n as f64)
        .collect();
    let curves: Vec<Vec<[f64; 2]>> = (0..4)
        .map(|j| {
            xs.iter()
                .map(|&x| [x, constraint::pw_all(x, i)[j]])
                .filter(|q| q[1].is_finite())
                .collect()
        })
        .collect();
    let env: Vec<[f64; 2]> = xs
        .iter()
        .map(|&x| [x, constraint::envelope(x, i).0])
        .filter(|q| q[1].is_finite())
        .collect();
    let mut ymax = env
        .iter()
        .map(|q| q[1])
        .fold(0.0_f64, f64::max)
        .max(c.installed_pw)
        .max(c.pw_design)
        * 1.15;
    if !ymax.is_finite() || ymax <= 0.0 {
        ymax = 10.0;
    }
    let cols = [tk.accent, tk.warning, tk.info, tk.success];
    ui.label(t("dp.section_diagram"));
    Plot::new("constraint")
        .height((h - 24.0).max(120.0))
        .x_axis_label("W/S [N/m²]")
        .y_axis_label("P/W [W/N]")
        .legend(Legend::default())
        .allow_zoom(false)
        .allow_drag(false)
        .allow_scroll(false)
        .show(ui, |pu| {
            pu.set_plot_bounds(PlotBounds::from_min_max([x_lo, 0.0], [x_hi, ymax]));
            // Uygun bölge: stall sınırının solu, zarfın üstü
            let x_end = c.ws_allowed.min(x_hi);
            if x_end > x_lo {
                let mut poly: Vec<[f64; 2]> =
                    env.iter().filter(|q| q[0] <= x_end).cloned().collect();
                if !poly.is_empty() {
                    poly.push([x_end, constraint::envelope(x_end, i).0]);
                    poly.push([x_end, ymax]);
                    poly.push([poly[0][0], ymax]);
                    pu.polygon(
                        Polygon::new(t("dp.plot_feasible_region"), PlotPoints::from(poly))
                            .fill_color(tk.success.gamma_multiply(0.22))
                            .stroke(egui::Stroke::NONE),
                    );
                }
            }
            for (j, pts) in curves.into_iter().enumerate() {
                pu.line(
                    Line::new(constraint::name(j), PlotPoints::from(pts))
                        .color(cols[j])
                        .width(2.0),
                );
            }
            pu.vline(
                VLine::new(t("dp.plot_stall_boundary"), c.ws_allowed)
                    .color(tk.danger)
                    .width(2.0),
            );
            pu.points(
                Points::new(t("dp.plot_current_design"), vec![[c.ws, c.installed_pw]])
                    .radius(8.0)
                    .color(if c.power_ok && c.stall_ok {
                        tk.success
                    } else {
                        tk.danger
                    }),
            );
            pu.points(
                Points::new(t("dp.plot_design_pw"), vec![[c.ws, c.pw_design]])
                    .radius(6.0)
                    .shape(egui_plot::MarkerShape::Diamond)
                    .color(tk.text_weak),
            );
            if c.rec_min_ws.is_finite() {
                pu.points(
                    Points::new(
                        t("dp.plot_rec_min_power"),
                        vec![[c.rec_min_ws, c.rec_min_pw]],
                    )
                    .radius(8.0)
                    .shape(egui_plot::MarkerShape::Asterisk)
                    .color(tk.text),
                );
            }
            if c.rec_max_ws.is_finite() {
                pu.points(
                    Points::new(t("dp.plot_rec_max_ws"), vec![[c.rec_max_ws, c.rec_max_pw]])
                        .radius(8.0)
                        .shape(egui_plot::MarkerShape::Plus)
                        .color(tk.warning),
                );
            }
        });
    let _ = p;
}

/// Girdiler + sonuçlar + grafik alt alta (başsız testler için).
pub fn show(ui: &mut Ui, p: &mut Project, m: &Model) {
    inputs(ui, p, m);
    results(ui, p, m);
    chart(ui, p, m, 400.0);
}
