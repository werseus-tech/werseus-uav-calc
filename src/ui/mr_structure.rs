// SPDX-License-Identifier: AGPL-3.0-or-later
//! Multirotor Yapı & Kol Boyutlandırma: kesit, malzeme, yük ve titreşim girdileri sol
//! panelde; konsol kiriş momenti, gerilme, uç sehimi ve rezonans sonuçları sağ panelde.
//! Kaydırıcılar `number_row` satırlarına dönüştü; aralıklar ve yan etkiler (iç çap sınırı,
//! elle değişince malzemenin "Özel" olması) eskisiyle aynıdır.
use super::components::{self, NumberInput, Status, number_row, section::section_header, u32_row};
use super::step_ui::{self, CardExt, TableRow, card, cards};
use super::widgets::fmt;
use crate::calc::Model;
use crate::calc::mr_structure::{MrArmMaterialPreset, MrArmSectionType};
use crate::i18n::{t, tf};
use crate::project::Project;
use egui::Ui;

/// Kesit, malzeme, yük ve devir satırları (aralıklar eski kaydırıcılarla aynı).
fn arm_rows(ui: &mut Ui, p: &mut Project, outer_label: &str, inner_label: &str) {
    if number_row(
        ui,
        NumberInput::new(outer_label, &mut p.mr_arm_d_o)
            .scale(1000.0)
            .unit("mm")
            .range(8.0, 50.0)
            .decimals(1),
    ) && p.mr_arm_d_i >= p.mr_arm_d_o
    {
        p.mr_arm_d_i = (p.mr_arm_d_o - 0.002).max(0.001);
    }
    // İç çap üst sınırı dış çapa bağlı (eski kaydırıcıyla aynı); ters aralık oluşmasın.
    let d_i_hi = (p.mr_arm_d_o * 1000.0 - 0.5).max(5.0);
    number_row(
        ui,
        NumberInput::new(inner_label, &mut p.mr_arm_d_i)
            .scale(1000.0)
            .unit("mm")
            .range(5.0, d_i_hi)
            .decimals(1),
    );
    let t_wall_mm = (p.mr_arm_d_o - p.mr_arm_d_i) * 500.0;
    step_ui::note(
        ui,
        &format!("{}: {:.2} mm", t("mr_struct.wall_thickness"), t_wall_mm),
    );

    section_header(ui, t("mr_struct.material_preset"));
    let prev_mat = p.mr_arm_material;
    step_ui::combo(
        ui,
        "mr_arm_mat_combo",
        t("mr_struct.material_preset"),
        "",
        &mut p.mr_arm_material,
        &[
            (
                MrArmMaterialPreset::PultrudedCf,
                t("mr_struct.mat_pultruded_cf"),
            ),
            (
                MrArmMaterialPreset::RollWrappedCf,
                t("mr_struct.mat_roll_wrapped_cf"),
            ),
            (MrArmMaterialPreset::Al6061, t("mr_struct.mat_al6061")),
            (MrArmMaterialPreset::Petg, t("mr_struct.mat_petg")),
            (MrArmMaterialPreset::CfPetg, t("mr_struct.mat_cf_petg")),
            (MrArmMaterialPreset::Custom, t("mr_struct.mat_custom")),
        ],
    );
    if p.mr_arm_material != prev_mat && p.mr_arm_material != MrArmMaterialPreset::Custom {
        let (sig, e, rho) = p.mr_arm_material.properties();
        p.mr_arm_sigma_allow = sig;
        p.mr_arm_e_modulus = e;
        p.mr_arm_rho = rho;
    }
    if number_row(
        ui,
        NumberInput::new(t("mr_struct.sigma_allow_mpa"), &mut p.mr_arm_sigma_allow)
            .scale(1e-6)
            .unit("MPa")
            .range(20.0, 1200.0)
            .decimals(0),
    ) {
        p.mr_arm_material = MrArmMaterialPreset::Custom;
    }
    if number_row(
        ui,
        NumberInput::new(t("mr_struct.e_gpa"), &mut p.mr_arm_e_modulus)
            .scale(1e-9)
            .unit("GPa")
            .range(1.0, 250.0)
            .decimals(1),
    ) {
        p.mr_arm_material = MrArmMaterialPreset::Custom;
    }
    if number_row(
        ui,
        NumberInput::new(t("mr_struct.rho_kg_m3"), &mut p.mr_arm_rho)
            .unit("kg/m³")
            .range(800.0, 3500.0)
            .decimals(0),
    ) {
        p.mr_arm_material = MrArmMaterialPreset::Custom;
    }

    section_header(ui, t("mr_struct.loads_heading"));
    number_row(
        ui,
        NumberInput::new(t("mr_struct.n_limit"), &mut p.mr_arm_n_limit)
            .range(1.0, 5.0)
            .decimals(2),
    );
    number_row(
        ui,
        NumberInput::new(t("mr_struct.fs"), &mut p.mr_arm_fs)
            .range(1.0, 3.0)
            .decimals(2),
    );
    number_row(
        ui,
        NumberInput::new(t("mr_struct.l_override_mm"), &mut p.mr_arm_l_override)
            .scale(1000.0)
            .unit("mm")
            .range(0.0, 800.0)
            .decimals(0),
    );
    if p.mr_arm_l_override > 0.0 && ui.button(t("mr_struct.btn_reset_l")).clicked() {
        p.mr_arm_l_override = 0.0;
    }
    number_row(
        ui,
        NumberInput::new(
            t("mr_struct.hover_rpm_override"),
            &mut p.mr_hover_rpm_override,
        )
        .unit("RPM")
        .range(0.0, 25000.0)
        .decimals(0),
    );
    if p.mr_hover_rpm_override > 0.0 && ui.button(t("mr_struct.btn_auto_rpm")).clicked() {
        p.mr_hover_rpm_override = 0.0;
    }
    u32_row(
        ui,
        t("mr_struct.prop_blades"),
        &mut p.mr_prop_blades,
        2,
        4,
        "",
    );
}

/// Sol panel: tipik ölçüler ve "Neden?" bilgisi (katlanabilir), kesit/malzeme/yük girdileri.
pub fn inputs(ui: &mut Ui, p: &mut Project, _m: &Model) {
    let rows = [
        (
            "mr_struct.class_small",
            "210 – 250 mm",
            "mr_struct.tube_small",
        ),
        (
            "mr_struct.class_medium",
            "450 – 550 mm",
            "mr_struct.tube_medium",
        ),
        (
            "mr_struct.class_large",
            "650 – 800 mm",
            "mr_struct.tube_large",
        ),
        (
            "mr_struct.class_heavy",
            "900 – 1600 mm",
            "mr_struct.tube_heavy",
        ),
    ];
    let table: Vec<TableRow> = rows
        .iter()
        .map(|(c, f, tube)| TableRow {
            name: t(c).to_string(),
            cells: vec![f.to_string(), t(tube).to_string()],
            status: Status::Neutral,
        })
        .collect();
    step_ui::row_table(
        ui,
        "mr_struct_typical",
        t("mr_struct.typical_sizes_heading"),
        &[t("mr_struct.col_frame"), t("mr_struct.col_tube")],
        &table,
        Some(t("mr_struct.typical_sizes_desc")),
    );
    step_ui::info_box(
        ui,
        "mr_struct_why",
        t("mr_struct.why_heading"),
        &[t("mr_struct.why_text_1"), t("mr_struct.why_text_2")],
    );

    section_header(ui, t("mr_struct.inputs_heading"));
    step_ui::combo(
        ui,
        "mr_arm_section",
        t("mr_struct.section_type"),
        "",
        &mut p.mr_arm_section,
        &[
            (MrArmSectionType::Round, t("mr_struct.sec_round")),
            (MrArmSectionType::Square, t("mr_struct.sec_square")),
        ],
    );
    let round = p.mr_arm_section == MrArmSectionType::Round;
    let outer = if round {
        t("mr_struct.d_outer")
    } else {
        t("mr_struct.w_outer")
    };
    let inner = if round {
        t("mr_struct.d_inner")
    } else {
        t("mr_struct.w_inner")
    };
    arm_rows(ui, p, outer, inner);
}

/// Sağ panel: kol boyu ve yükler, gerilme/dayanım, uç sehimi, titreşim, kütle, formüller.
pub fn results(ui: &mut Ui, p: &mut Project, m: &Model) {
    let r = &m.mr_st;
    section_header(ui, t("mr_struct.results_heading"));
    let sigma_mpa = r.sigma_max / 1e6;
    let sig_allow_mpa = p.mr_arm_sigma_allow / 1e6;
    cards(
        ui,
        &[
            card(
                t("mr_struct.res_l_arm"),
                format!("{:.1}", r.l_arm * 1000.0),
                "mm",
                "",
            ),
            card(
                t("mr_struct.res_f_thrust"),
                format!("{:.1}", r.f_thrust_max),
                "N",
                "",
            ),
            card(
                t("mr_struct.res_m_limit"),
                format!("{:.2}", r.m_limit),
                "N·m",
                "",
            ),
            card(
                t("mr_struct.res_m_ult"),
                format!("{:.2}", r.m_ult),
                "N·m",
                "",
            ),
            card(
                t("mr_struct.res_sigma_max"),
                format!("{sigma_mpa:.1}"),
                "MPa",
                "",
            )
            .d(tf(
                "mr_struct.allowed_fmt",
                &[format!("{sig_allow_mpa:.0}")],
            )),
            card(t("mr_struct.res_ms"), format!("{:.2}", r.ms), "", "")
                .s(step_ui::tri(r.ms, 0.2, 0.0)),
            card(
                t("mr_struct.res_deflection"),
                format!("{:.2}", r.deflection_max * 1000.0),
                "mm",
                "",
            )
            .d(tf(
                "mr_struct.target_fmt",
                &[format!("{:.2}", r.deflection_limit * 1000.0)],
            ))
            .s(if r.deflection_ok {
                Status::Ok
            } else {
                Status::Warn
            }),
            card(
                t("mr_struct.res_f_natural"),
                format!("{:.1}", r.f_natural),
                "Hz",
                "",
            ),
            card(
                t("mr_struct.res_f_rot"),
                format!("{:.1}", r.f_rot_hover),
                "Hz",
                "",
            )
            .d(format!("{:.0} RPM", r.f_rot_hover * 60.0)),
            card(
                t("mr_struct.res_f_bpf"),
                format!("{:.1}", r.f_bpf_hover),
                "Hz",
                "",
            ),
            card(
                t("mr_struct.res_m_single"),
                format!("{:.1}", r.m_arm_single * 1000.0),
                "g",
                "",
            ),
            card(
                t("mr_struct.res_m_total"),
                format!("{:.1}", r.m_arm_total * 1000.0),
                "g",
                "",
            )
            .d(tf("mr_struct.arms_fmt", &[p.mr_arm_count.to_string()])),
        ],
    );
    if r.resonance_rot_warn {
        components::banner(ui, Status::Err, t("mr_struct.res_resonance_rot"));
    } else if r.resonance_bpf_warn {
        components::banner(ui, Status::Warn, t("mr_struct.res_resonance_bpf"));
    } else {
        components::banner(ui, Status::Ok, t("mr_struct.res_vibration_ok"));
    }

    // Anlık değerlerle formüller
    let body = tf(
        "mr_struct.formula_lines_fmt",
        &[
            format!("{:.1}", p.mr_t_max_motor),
            format!("{:.0}", r.n_coax),
            format!("{:.3}", r.l_arm),
            format!("{:.1}", p.mr_arm_n_limit),
            format!("{:.1}", p.mr_arm_fs),
            format!("{:.2}", r.m_ult),
            format!("{:.2}", r.m_ult),
            format!("{:.4}", p.mr_arm_d_o * 0.5),
            format!("{:.2e}", r.inertia_i),
            format!("{:.1}", r.sigma_max / 1e6),
            format!("{:.0}", p.mr_arm_sigma_allow / 1e6),
            format!("{:.2}", r.ms),
            format!("{:.1}", r.f_thrust_max),
            format!("{:.4}", r.l_arm.powi(3)),
            format!("{:.1e}", p.mr_arm_e_modulus),
            format!("{:.2e}", r.inertia_i),
            format!("{:.2}", r.deflection_max * 1000.0),
            format!("{:.1}", r.f_natural),
            format!("{:.1}", r.f_rot_hover),
        ],
    );
    step_ui::formula_box(
        ui,
        "mr_struct_formulas",
        t("mr_struct.formulas_heading"),
        &body,
    );
    let _ = fmt;
}

/// Girdiler + sonuçlar alt alta (başsız testler için).
pub fn show(ui: &mut Ui, p: &mut Project, m: &Model, _lib: &crate::calc::Library) {
    inputs(ui, p, m);
    results(ui, p, m);
}
