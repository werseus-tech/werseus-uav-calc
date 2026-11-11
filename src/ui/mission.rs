// SPDX-License-Identifier: AGPL-3.0-or-later
//! Adım 1 / Sekme 1: Görev & Atmosfer: girdiler sol panel, sonuçlar sağ panel.
use super::components::{NumberInput, Status, number_row, section::section_header};
use super::step_ui::{CardExt, card, cards};
use super::widgets::*;
use crate::calc::Model;
use crate::i18n::t;
use crate::project::Project;
use egui::Ui;

/// Sol panel: irtifa ve seyir hızı.
pub fn inputs(ui: &mut Ui, p: &mut Project, _m: &Model) {
    section_header(ui, t("section_inputs"));
    let n = format!("= {:.0} ft", p.h * 3.28084);
    number_row(
        ui,
        NumberInput::new(t("mission_alt"), &mut p.h)
            .unit("m")
            .range(0.0, 11000.0)
            .decimals(0)
            .help(t("mission_alt_desc"))
            .note(Some(n)),
    );
    let n = ms_to_kmh(p.v_cr);
    number_row(
        ui,
        NumberInput::new(t("mission_v_cr"), &mut p.v_cr)
            .unit("m/s")
            .range(5.0, 60.0)
            .decimals(1)
            .help(t("mission_v_cr_desc"))
            .note(Some(n)),
    );
}

/// Sağ panel: atmosfer çıktıları ve Reynolds sayıları.
pub fn results(ui: &mut Ui, _p: &mut Project, m: &Model) {
    section_header(ui, t("section_atmo_outputs"));
    let a = &m.atmo;
    cards(
        ui,
        &[
            card(t("mission_t"), fmt(a.t, 1), "K", t("mission_t_desc"))
                .d(format!("{} °C", fmt(a.t - 273.15, 1))),
            card(t("mission_p"), fmt(a.p, 0), "Pa", t("mission_p_desc"))
                .d(format!("{} hPa", fmt(a.p / 100.0, 1))),
            card(
                t("mission_rho"),
                fmt(a.rho, 4),
                "kg/m³",
                t("mission_rho_desc"),
            ),
            card(
                t("mission_mu"),
                fmt_sci(a.mu, 3),
                "Pa·s",
                t("mission.out_mu_desc"),
            ),
            card(
                t("mission.out_nu"),
                fmt_sci(a.nu, 3),
                "m²/s",
                t("mission.out_nu_desc"),
            ),
            card(t("mission_a"), fmt(a.a, 1), "m/s", t("mission.out_a_desc")),
            card(
                t("mission_mach"),
                fmt(m.aero.mach, 3),
                "",
                t("mission.out_mach_desc"),
            )
            .d(t("mission.out_mach_limit"))
            .s(super::step_ui::ok(m.aero.mach <= 0.15)),
            card(
                t("mission.out_rho_msl"),
                fmt(m.atmo_msl.rho, 4),
                "kg/m³",
                t("mission.out_rho_msl_desc"),
            ),
        ],
    );

    section_header(ui, t("mission.section_reynolds"));
    let re = m.aero.re_wing;
    let mut list = vec![
        card(
            t("mission_re"),
            fmt_sci(re, 2),
            "",
            t("mission.out_re_desc"),
        )
        .d(if re < 5e5 {
            t("mission.out_re_low")
        } else {
            ""
        })
        .s(if re < 1e5 {
            Status::Err
        } else if re < 5e5 {
            Status::Warn
        } else {
            Status::Ok
        }),
    ];
    for it in m.aero.items.iter().take(3).skip(1) {
        list.push(card(
            it.name,
            fmt_sci(it.re, 2),
            "",
            t("mission.out_re_item_desc"),
        ));
    }
    list.push(
        card(
            t("mission.out_re_tip"),
            fmt_sci(m.re_tip, 2),
            "",
            t("mission.out_re_tip_desc"),
        )
        .d(t("mission.out_re_tip_req"))
        .s(super::step_ui::ok(m.re_tip >= 1e5)),
    );
    cards(ui, &list);
}

/// Girdiler + sonuçlar alt alta (başsız testler için).
pub fn show(ui: &mut Ui, p: &mut Project, m: &Model) {
    inputs(ui, p, m);
    results(ui, p, m);
}
