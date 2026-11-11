// SPDX-License-Identifier: AGPL-3.0-or-later
//! Adım 7 / Sekme 7: İtki Sistemi: girdiler sol panel, sonuçlar sağ panel. Alanlar,
//! aralıklar ve ondalıklar eski `fld` satırlarıyla birebir aynıdır.
use super::components::{NumberInput, Status, number_row, section::section_header};
use super::step_ui::{self, CardExt, card, cards};
use super::widgets::fmt;
use crate::calc::Model;
use crate::i18n::{t, tf};
use crate::project::Project;
use egui::Ui;

fn inch(d: f64) -> String {
    tf("prop.inch_fmt", &[format!("{:.1}", d / 0.0254)])
}

/// Sol panel: itki sistemi girdileri.
pub fn inputs(ui: &mut Ui, p: &mut Project, _m: &Model) {
    section_header(ui, t("section_inputs"));
    number_row(
        ui,
        NumberInput::new(t("prop.input.tw_static"), &mut p.tw_static)
            .range(0.2, 1.5)
            .decimals(2)
            .help(t("prop.input.tw_static.desc")),
    );
    let n = inch(p.d_prop);
    number_row(
        ui,
        NumberInput::new(t("prop.input.d_prop"), &mut p.d_prop)
            .scale(1000.0)
            .unit("mm")
            .range(100.0, 450.0)
            .decimals(0)
            .help(t("prop.input.d_prop.desc"))
            .note(Some(n)),
    );
    let n = inch(p.prop_pitch);
    number_row(
        ui,
        NumberInput::new(t("prop.input.prop_pitch"), &mut p.prop_pitch)
            .scale(1000.0)
            .unit("mm")
            .range(50.0, 400.0)
            .decimals(0)
            .help(t("prop.input.prop_pitch.desc"))
            .note(Some(n)),
    );
    number_row(
        ui,
        NumberInput::new(t("prop.input.prop_slip"), &mut p.prop_slip)
            .range(0.0, 0.6)
            .decimals(2)
            .help(t("prop.input.prop_slip.desc")),
    );
    number_row(
        ui,
        NumberInput::new(t("prop.input.m_tip_max"), &mut p.m_tip_max)
            .range(0.2, 0.8)
            .decimals(2)
            .help(t("prop.input.m_tip_max.desc")),
    );
    number_row(
        ui,
        NumberInput::new("Figure of merit (FM)", &mut p.fm)
            .range(0.3, 0.9)
            .decimals(2)
            .help(t("prop.input.fm.desc")),
    );
    number_row(
        ui,
        NumberInput::new(t("prop.input.p_max_elec"), &mut p.p_max_elec)
            .unit("W")
            .range(20.0, 2000.0)
            .decimals(0)
            .help(t("prop.input.p_max_elec.desc")),
    );
    number_row(
        ui,
        NumberInput::new(t("prop.input.motor_margin"), &mut p.motor_margin)
            .unit("×")
            .range(1.0, 2.0)
            .decimals(2)
            .help(t("prop.input.motor_margin.desc")),
    );
    if p.d_prop < 0.18 || p.d_prop > 0.30 {
        step_ui::status_text(ui, Status::Warn, t("prop.d_prop_unusual_warning"));
    }
}

/// Sağ panel: seyir ve statik itki koşulları, motor/pervane eşleştirme önerisi.
pub fn results(ui: &mut Ui, p: &mut Project, m: &Model) {
    let o = &m.prop;
    section_header(ui, t("prop.section_cruise_conditions"));
    cards(
        ui,
        &[
            card(
                t("prop.out.t_cr"),
                fmt(o.t_cr, 2),
                "N",
                t("prop.out.t_cr.desc"),
            )
            .d(tf(
                "prop.out.force_gf_fmt",
                &[fmt(o.t_cr / 9.80665 * 1000.0, 0)],
            )),
            card(
                t("prop.out.cruise_ld"),
                fmt(m.perf_cr.ld, 1),
                "",
                t("prop.out.cruise_ld.desc"),
            ),
            card(
                t("prop.out.p_shaft_cr"),
                fmt(o.p_shaft_cr, 1),
                "W",
                t("prop.out.p_shaft_cr.desc"),
            ),
            card(
                t("prop.out.p_elec_cr"),
                fmt(o.p_elec_cr, 1),
                "W",
                t("prop.out.p_elec_cr.desc"),
            ),
        ],
    );
    section_header(ui, t("prop.section_static_thrust"));
    cards(
        ui,
        &[
            card(
                t("prop.out.t_static"),
                fmt(o.t_static, 1),
                "N",
                t("prop.out.t_static.desc"),
            )
            .d(tf(
                "prop.out.force_gf_fmt",
                &[fmt(o.t_static / 9.80665 * 1000.0, 0)],
            )),
            card(
                t("prop.out.a_disk"),
                fmt(o.a_disk, 4),
                "m²",
                t("prop.out.a_disk.desc"),
            ),
            card(
                t("prop.out.disk_loading"),
                fmt(o.disk_loading, 0),
                "N/m²",
                t("prop.out.disk_loading.desc"),
            ),
            card(
                t("prop.out.p_ideal"),
                fmt(o.p_ideal, 0),
                "W",
                t("prop.out.p_ideal.desc"),
            ),
            card(
                t("prop.out.p_shaft_static"),
                fmt(o.p_shaft_static, 0),
                "W",
                t("prop.out.p_shaft_static.desc"),
            ),
            card(
                t("prop.out.p_elec_static"),
                fmt(o.p_elec_static, 0),
                "W",
                t("prop.out.p_elec_static.desc"),
            ),
        ],
    );
    section_header(ui, t("prop.section_match_rec"));
    cards(
        ui,
        &[
            card(
                t("prop.out.p_design_cons"),
                fmt(m.cons.p_elec, 0),
                "W",
                t("prop.out.p_design_cons.desc"),
            ),
            card(
                t("prop.out.p_motor_suggest"),
                fmt(o.p_motor_suggest, 0),
                "W",
                t("prop.out.p_motor_suggest.desc"),
            )
            .d(t("prop.unit_w_elec_peak"))
            .s(step_ui::ok(o.installed_ok)),
            card(
                t("prop.out.installed_vs_sug"),
                fmt(p.p_max_elec, 0),
                "W",
                t("prop.out.installed_vs_sug.desc"),
            )
            .d(tf("prop.w_ratio_fmt", &[fmt(o.p_motor_suggest, 0)]))
            .s(step_ui::ok(o.installed_ok)),
            card(
                t("prop.out.p_shaft_suggest"),
                fmt(o.p_motor_suggest * p.eta_mot * p.eta_esc, 0),
                "W",
                t("prop.out.p_shaft_suggest.desc"),
            ),
            card(
                t("prop.out.i_peak"),
                fmt(o.i_peak, 1),
                "A",
                t("prop.out.i_peak.desc"),
            )
            .d(tf("prop.i_peak_fmt", &[fmt(m.weight.pack_voltage, 1)])),
            card(
                t("prop.out.c_rate"),
                fmt(o.c_rate, 2),
                "C",
                t("prop.out.c_rate.desc"),
            )
            .d(tf(
                "prop.c_rate_limit_fmt",
                &[fmt(m.prop_in.pack_max_cont_a, 0)],
            ))
            .s(step_ui::ok(o.current_ok)),
            card(
                t("prop.out.v_tip_max"),
                fmt(o.v_tip_max, 0),
                "m/s",
                t("prop.out.v_tip_max.desc"),
            ),
            card(
                t("prop.out.rpm_max"),
                fmt(o.rpm_max, 0),
                "RPM",
                t("prop.out.rpm_max.desc"),
            ),
            card(
                t("prop.out.rpm_cr"),
                fmt(o.rpm_cr, 0),
                "RPM",
                t("prop.out.rpm_cr.desc"),
            )
            .s(step_ui::ok(o.rpm_ok)),
            card(
                t("prop.out.kv_suggest"),
                fmt(o.rpm_max * 0.8 / m.weight.pack_voltage, 0),
                "RPM/V",
                t("prop.out.kv_suggest.desc"),
            ),
        ],
    );
}

/// Girdiler + sonuçlar alt alta (başsız testler için).
pub fn show(ui: &mut Ui, p: &mut Project, m: &Model) {
    inputs(ui, p, m);
    results(ui, p, m);
}
