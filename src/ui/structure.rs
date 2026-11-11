// SPDX-License-Identifier: AGPL-3.0-or-later
//! Adım 9 / Sekme 9: Yapı: yük, malzeme ve kiriş girdileri sol panel, eğilme dayanımı
//! sonuçları sağ panel. Alanlar, aralıklar ve ondalıklar eski `fld` satırlarıyla birebir aynıdır.
use super::components::{NumberInput, Status, number_row, section::section_header};
use super::step_ui::{self, CardExt, card, cards};
use super::widgets::*;
use crate::calc::Model;
use crate::i18n::{t, tf};
use crate::project::Project;
use egui::Ui;

/// Yaygın karbon boru ölçüleri: (dış, iç) mm
const TUBES: [(f64, f64); 7] = [
    (8.0, 6.0),
    (10.0, 8.0),
    (10.0, 9.0),
    (12.0, 10.0),
    (12.0, 11.0),
    (14.0, 12.0),
    (16.0, 14.0),
];

/// Sol panel: yük/malzeme ve kanat kirişi girdileri.
pub fn inputs(ui: &mut Ui, p: &mut Project, _m: &Model) {
    section_header(ui, t("struct.section_load_material"));
    number_row(
        ui,
        NumberInput::new(t("struct.input.n_limit"), &mut p.n_limit)
            .unit("g")
            .range(1.5, 8.0)
            .decimals(2)
            .help(t("struct.input.n_limit.desc")),
    );
    number_row(
        ui,
        NumberInput::new(t("struct.input.fs"), &mut p.fs)
            .unit("×")
            .range(1.0, 3.0)
            .decimals(2)
            .help(t("struct.input.fs.desc")),
    );
    number_row(
        ui,
        NumberInput::new(t("struct.input.sigma_allow"), &mut p.sigma_allow)
            .scale(1e-6)
            .unit("MPa")
            .range(100.0, 1500.0)
            .decimals(0)
            .help(t("struct.input.sigma_allow.desc")),
    );

    section_header(ui, t("struct.section_spar"));
    step_ui::info_box(
        ui,
        "spar_guide",
        t("struct.spar_guide_heading"),
        &[t("struct.spar_guide_text")],
    );
    let cur = format!("{:.0} × {:.0} mm", p.d_spar_o * 1000.0, p.d_spar_i * 1000.0);
    step_ui::field_label(ui, t("struct.preset_size_label"), "");
    egui::ComboBox::from_id_salt("struct_tube_preset")
        .selected_text(cur)
        .width((ui.available_width() - 8.0).max(40.0))
        .show_ui(ui, |ui| {
            for (o, i) in TUBES {
                let lbl = tf(
                    "struct.tube_format",
                    &[format!("{o:.0}"), format!("{i:.0}")],
                );
                if ui.selectable_label(false, lbl).clicked() {
                    p.d_spar_o = o / 1000.0;
                    p.d_spar_i = i / 1000.0;
                }
            }
        });
    number_row(
        ui,
        NumberInput::new(t("struct.input.d_spar_o"), &mut p.d_spar_o)
            .scale(1000.0)
            .unit("mm")
            .range(4.0, 30.0)
            .decimals(1)
            .help(t("struct.input.d_spar_o.desc")),
    );
    number_row(
        ui,
        NumberInput::new(t("struct.input.d_spar_i"), &mut p.d_spar_i)
            .scale(1000.0)
            .unit("mm")
            .range(0.0, 28.0)
            .decimals(1)
            .help(t("struct.input.d_spar_i.desc")),
    );
    if p.d_spar_i >= p.d_spar_o {
        step_ui::status_text(ui, Status::Err, t("struct.id_lt_od_err"));
    }
}

/// Sağ panel: eğilme ve dayanım sonuçları.
pub fn results(ui: &mut Ui, _p: &mut Project, m: &Model) {
    section_header(ui, t("struct.section_bending_strength"));
    let s = &m.st;
    cards(
        ui,
        &[
            card(
                t("struct.out.n_ult"),
                fmt(s.n_ult, 2),
                "g",
                t("struct.out.n_ult.desc"),
            ),
            card(
                t("struct.out.m_b_limit"),
                fmt(s.m_b_limit, 1),
                "N·m",
                t("struct.out.m_b_limit.desc"),
            ),
            card(
                t("struct.out.m_b_ult"),
                fmt(s.m_b_ult, 1),
                "N·m",
                t("struct.out.m_b_ult.desc"),
            ),
            card(
                t("struct.out.i_spar"),
                fmt_sci(s.i_spar, 3),
                "m⁴",
                t("struct.out.i_spar.desc"),
            ),
            card(
                t("struct.out.z_spar"),
                fmt_sci(s.z_spar, 3),
                "m³",
                t("struct.out.z_spar.desc"),
            ),
            card(
                t("struct.out.sigma_max"),
                fmt(s.sigma_max / 1e6, 0),
                "MPa",
                t("struct.out.sigma_max.desc"),
            ),
            card(
                t("struct.out.ms"),
                fmt(s.ms, 2),
                "",
                t("struct.out.ms.desc"),
            )
            .d(t("struct.out.ms_req"))
            .s(step_ui::tri(s.ms, 0.2, 0.0)),
        ],
    );
}

/// Girdiler + sonuçlar alt alta (başsız testler için).
pub fn show(ui: &mut Ui, p: &mut Project, m: &Model, _lib: &crate::calc::Library) {
    inputs(ui, p, m);
    results(ui, p, m);
}
