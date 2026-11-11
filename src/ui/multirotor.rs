// SPDX-License-Identifier: AGPL-3.0-or-later
//! Multirotor adımları: çerçeve geometrisi, kütle, motor/momentum itkisi, pil ve uçuş
//! süresi, ağırlık merkezi. Her adımın girdileri sol panelde (`*_inputs`), sonuçları sağ panelde
//! (`*_results`); araç merkez görünümde 3D/2D. Alanlar, aralıklar ve ondalıklar eski ekranlarla
//! birebir aynıdır (DragValue ile girilen mm/inç değerleri aynı SI alanlarına yazılır).
use super::components::{NumberInput, Status, number_row, section::section_header};
use super::step_ui::{self, CardExt, card, cards};
use super::widgets::fmt;
use crate::calc::multirotor::MultirotorLayout;
use crate::calc::{Library, Model};
use crate::i18n::{t, tf};
use crate::project::Project;
use egui::Ui;

fn mm_row(ui: &mut Ui, label: &str, v: &mut f64, lo: f64, hi: f64, help: &str) {
    number_row(
        ui,
        NumberInput::new(label, v)
            .scale(1000.0)
            .unit("mm")
            .range(lo, hi)
            .decimals(0)
            .help(help),
    );
}

fn g_row(ui: &mut Ui, label: &str, v: &mut f64, lo: f64, hi: f64, help: &str) {
    number_row(
        ui,
        NumberInput::new(label, v)
            .scale(1000.0)
            .unit("g")
            .range(lo, hi)
            .decimals(0)
            .help(help),
    );
}

// ---------------------------------------------------------------------------------------------
// Çerçeve & pervane geometrisi
// ---------------------------------------------------------------------------------------------

pub fn frame_inputs(ui: &mut Ui, p: &mut Project, m: &Model) {
    section_header(ui, t("mr_section_frame"));
    mm_row(
        ui,
        t("mr_frame_size"),
        &mut p.mr_frame_size,
        100.0,
        1500.0,
        t("mr.frame_size.desc"),
    );

    let prev_arms = p.mr_arm_count;
    step_ui::combo(
        ui,
        "mr_arm_cnt",
        t("mr_arm_count"),
        t("mr.arm_count.desc"),
        &mut p.mr_arm_count,
        &[
            (3, t("mr.arm_count_3")),
            (4, t("mr.arm_count_4")),
            (6, t("mr.arm_count_6")),
            (8, t("mr.arm_count_8")),
        ],
    );
    if p.mr_arm_count != prev_arms && p.mr_arm_count != 4 && p.mr_layout == MultirotorLayout::H {
        p.mr_layout = MultirotorLayout::X;
    }

    step_ui::field_label(ui, t("mr_layout"), t("mr.layout.desc"));
    egui::ComboBox::from_id_salt("mr_layout_cb")
        .selected_text(p.mr_layout.label())
        .width((ui.available_width() - 8.0).max(40.0))
        .show_ui(ui, |ui| {
            for l in MultirotorLayout::ALL {
                if l == MultirotorLayout::H && p.mr_arm_count != 4 {
                    let resp = ui.add_enabled(false, egui::Button::new(l.label()));
                    resp.on_disabled_hover_text(t("mr.layout_h_only_4_arms"));
                } else {
                    ui.selectable_value(&mut p.mr_layout, l, l.label());
                }
            }
        });

    if p.mr_layout == MultirotorLayout::H {
        let default_dim = p.mr_frame_size / std::f64::consts::SQRT_2;
        let mut lx = if p.mr_h_lx > 0.0 {
            p.mr_h_lx
        } else {
            default_dim
        };
        let mut wy = if p.mr_h_wy > 0.0 {
            p.mr_h_wy
        } else {
            default_dim
        };
        if number_row(
            ui,
            NumberInput::new(t("mr_h_lx"), &mut lx)
                .scale(1000.0)
                .unit("mm")
                .range(50.0, 1500.0)
                .decimals(0)
                .help(t("mr.h_lx.desc")),
        ) {
            p.mr_h_lx = lx;
        }
        if number_row(
            ui,
            NumberInput::new(t("mr_h_wy"), &mut wy)
                .scale(1000.0)
                .unit("mm")
                .range(50.0, 1500.0)
                .decimals(0)
                .help(t("mr.h_wy.desc")),
        ) {
            p.mr_h_wy = wy;
        }
    }

    step_ui::field_label(ui, t("mr_coaxial"), t("mr.coaxial_desc"));
    ui.checkbox(&mut p.mr_coaxial, t("mr.coaxial_checkbox"))
        .on_hover_text(t("mr.coaxial_desc"));
    if p.mr_coaxial {
        number_row(
            ui,
            NumberInput::new(t("mr_coax_kappa"), &mut p.mr_coax_kappa)
                .range(1.0, 2.5)
                .decimals(2)
                .help(t("mr.coax_kappa.desc")),
        );
    }
    mm_row(
        ui,
        t("mr_prop_clearance"),
        &mut p.mr_prop_clearance,
        2.0,
        50.0,
        t("mr.prop_clearance.desc"),
    );
    mm_row(
        ui,
        t("mr_plate_radius"),
        &mut p.mr_r_plate,
        15.0,
        150.0,
        t("mr.plate_radius.desc"),
    );

    section_header(ui, t("mr.section_prop_selection"));
    let n = tf("mr.inch_mm_fmt", &[format!("{:.1}", p.mr_d_prop * 1000.0)]);
    number_row(
        ui,
        NumberInput::new(t("mr_prop_diameter"), &mut p.mr_d_prop)
            .scale(1.0 / 0.0254)
            .unit("in")
            .range(2.0, 30.0)
            .decimals(1)
            .help(t("mr.prop_diameter.desc"))
            .note(Some(n)),
    );
    if m.mr.prop_overlap {
        step_ui::status_text(ui, Status::Err, t("mr.prop_overlap_err"));
    } else if !m.mr.prop_clearance_ok {
        step_ui::status_text(ui, Status::Warn, t("mr.prop_clearance_warn"));
    } else {
        step_ui::status_text(ui, Status::Ok, t("mr.prop_clearance_ok"));
    }
}

pub fn frame_results(ui: &mut Ui, _p: &mut Project, m: &Model) {
    section_header(ui, t("mr.section_geo_outputs"));
    cards(
        ui,
        &[
            card(
                t("mr_adjacent_spacing"),
                fmt(m.mr.s_adj * 1000.0, 1),
                "mm",
                t("mr.adjacent_spacing.desc"),
            ),
            card(
                t("mr_max_prop_size"),
                tf(
                    "mr.max_prop_size_fmt",
                    &[
                        fmt(m.mr.d_prop_max * 1000.0, 1),
                        format!("{:.1}", m.mr.d_prop_max_inch),
                    ],
                ),
                "",
                t("mr.max_prop_size.desc"),
            ),
            card(
                t("mr_arm_length"),
                fmt(m.mr.l_arm * 1000.0, 1),
                "mm",
                t("mr.arm_length.desc"),
            )
            .s(Status::Ok),
            card(
                t("mr_total_motors"),
                tf(
                    "mr.total_motors_fmt",
                    &[format!("{}", m.mr.n_motors), format!("{:.2}", m.mr.n_eff)],
                ),
                "",
                t("mr.total_motors.desc"),
            ),
        ],
    );
}

// ---------------------------------------------------------------------------------------------
// Kütle ve ağırlık dökümü
// ---------------------------------------------------------------------------------------------

pub fn mass_inputs(ui: &mut Ui, p: &mut Project, m: &Model, lib: &Library) {
    section_header(ui, t("mr_section_mass"));
    g_row(
        ui,
        t("mr.mass.frame"),
        &mut p.mr_m_frame,
        10.0,
        3000.0,
        t("mr.mass.frame.desc"),
    );
    g_row(
        ui,
        t("mr.mass.motor"),
        &mut p.mr_m_motor,
        5.0,
        500.0,
        t("mr.mass.motor.desc"),
    );
    g_row(
        ui,
        t("mr.mass.esc"),
        &mut p.mr_m_esc_tot,
        5.0,
        300.0,
        t("mr.mass.esc.desc"),
    );
    g_row(
        ui,
        t("mr.mass.fc"),
        &mut p.mr_m_fc,
        5.0,
        300.0,
        t("mr.mass.fc.desc"),
    );
    g_row(
        ui,
        t("mr.mass.payload"),
        &mut p.m_payload,
        0.0,
        5000.0,
        t("mr.mass.payload.desc"),
    );
    g_row(
        ui,
        t("mr.mass.misc"),
        &mut p.mr_m_misc,
        0.0,
        500.0,
        t("mr.mass.misc.desc"),
    );
    super::weight::render_battery_source_ui(ui, p, m, lib);
}

pub fn mass_results(ui: &mut Ui, p: &mut Project, m: &Model) {
    section_header(ui, t("mr_section_mass"));
    cards(
        ui,
        &[
            card(
                t("weight_m_batt"),
                fmt(m.weight.m_battery * 1000.0, 0),
                "g",
                t("mr.batt_pack.desc"),
            )
            .d(tf(
                "mr.batt_mtow_pct_fmt",
                &[format!("{:.0}", m.weight.f_batt * 100.0)],
            )),
            card(
                t("weight_mtow"),
                fmt(m.mr.m_mtow * 1000.0, 0),
                "g",
                t("mr.mtow.desc"),
            )
            .d(tf("mr.mtow_w_fmt", &[format!("{:.2}", m.mr.w)])),
        ],
    );
    super::weight::battery_formulas(ui, p, m);
}

// ---------------------------------------------------------------------------------------------
// Motor & itki (momentum teorisi)
// ---------------------------------------------------------------------------------------------

pub fn motors_inputs(ui: &mut Ui, p: &mut Project, _m: &Model) {
    section_header(ui, t("mr_section_motors"));
    number_row(
        ui,
        NumberInput::new(t("mr_motor_thrust_max"), &mut p.mr_t_max_motor)
            .unit("N")
            .range(1.0, 150.0)
            .decimals(1)
            .help(t("mr.motor_thrust_max.desc")),
    );
    number_row(
        ui,
        NumberInput::new(t("mr_fm"), &mut p.fm)
            .range(0.40, 0.85)
            .decimals(2)
            .help(t("mr.fm.desc")),
    );
    number_row(
        ui,
        NumberInput::new(t("mr_eta_motor"), &mut p.eta_mot)
            .scale(100.0)
            .unit("%")
            .range(50.0, 98.0)
            .decimals(1)
            .help(t("mr.eta_motor.desc")),
    );
    number_row(
        ui,
        NumberInput::new(t("mr_eta_esc"), &mut p.eta_esc)
            .scale(100.0)
            .unit("%")
            .range(70.0, 99.0)
            .decimals(1)
            .help(t("mr.eta_esc.desc")),
    );
}

pub fn motors_results(ui: &mut Ui, _p: &mut Project, m: &Model) {
    let mr = &m.mr;
    section_header(ui, t("mr_section_performance"));
    cards(
        ui,
        &[
            card(
                t("mr_hover_thrust_motor"),
                fmt(mr.t_hover_motor, 2),
                "N",
                t("mr.hover_thrust_motor.desc"),
            )
            .d(tf(
                "mr.force_g_fmt",
                &[format!(
                    "{:.0}",
                    mr.t_hover_motor / crate::calc::multirotor::G * 1000.0
                )],
            )),
            card(
                t("mr_ideal_power"),
                fmt(mr.p_ideal_total, 1),
                "W",
                t("mr.ideal_power.desc"),
            )
            .d(tf(
                "mr.power_per_motor_fmt",
                &[format!("{:.1}", mr.p_ideal_motor)],
            )),
            card(
                t("mr_shaft_power"),
                fmt(mr.p_shaft_total, 1),
                "W",
                t("mr.shaft_power.desc"),
            )
            .d(tf(
                "mr.power_per_motor_fmt",
                &[format!("{:.1}", mr.p_shaft_motor)],
            )),
            card(
                t("mr_elec_hover_power"),
                fmt(mr.p_elec_hover_total, 1),
                "W",
                t("mr.elec_hover_power.desc"),
            )
            .d(tf(
                "mr.power_per_motor_fmt",
                &[format!("{:.1}", mr.p_elec_motor)],
            )),
            card(
                t("mr_disk_loading"),
                fmt(mr.disk_loading, 1),
                "N/m²",
                t("mr.disk_loading.desc"),
            ),
            card(
                t("mr_tw_ratio"),
                fmt(mr.tw_ratio, 2),
                "",
                t("mr.tw_ratio.desc"),
            )
            .d(tf("mr.t_max_fmt", &[format!("{:.1}", mr.t_max_total)]))
            .s(step_ui::tri(mr.tw_ratio, 2.0, 1.6)),
            card(
                t("mr_hover_throttle"),
                format!("%{:.1}", mr.hover_throttle * 100.0),
                "",
                t("mr.hover_throttle.desc"),
            )
            .d("≈ √(1/TW)")
            .s(step_ui::tri_low(mr.hover_throttle, 0.50, 0.60)),
        ],
    );
}

// ---------------------------------------------------------------------------------------------
// Pil, uçuş süresi ve menzil
// ---------------------------------------------------------------------------------------------

pub fn battery_inputs(ui: &mut Ui, p: &mut Project, _m: &Model) {
    section_header(ui, t("mr.section_cruise_conditions"));
    number_row(
        ui,
        NumberInput::new(t("mr_v_cruise"), &mut p.mr_v_cruise)
            .unit("m/s")
            .range(5.0, 35.0)
            .decimals(1)
            .help(t("mr.v_cruise.desc")),
    );
    number_row(
        ui,
        NumberInput::new(t("mr_flat_plate_area"), &mut p.mr_flat_plate_area)
            .unit("m²")
            .range(0.001, 0.100)
            .decimals(3)
            .help(t("mr.flat_plate_area.desc")),
    );
    number_row(
        ui,
        NumberInput::new(t("mr_profile_power_ratio"), &mut p.mr_profile_power_ratio)
            .scale(100.0)
            .unit("%")
            .range(0.0, 50.0)
            .decimals(1)
            .help(t("mr.profile_power_ratio.desc")),
    );
}

pub fn battery_results(ui: &mut Ui, p: &mut Project, m: &Model) {
    let mr = &m.mr;
    section_header(ui, t("mr.range.heading"));
    cards(
        ui,
        &[
            card(
                t("mr.hover_current"),
                fmt(mr.i_hover, 1),
                "A",
                t("mr.hover_current.desc"),
            )
            .d(tf(
                "mr.hover_current_limit_fmt",
                &[format!("{:.0}", p.n_parallel as f64 * m.cell.max_cont_a)],
            ))
            .s(step_ui::ok(mr.i_hover_ok)),
            card(
                t("mr_hover_time"),
                tf("mr.time_min_fmt", &[fmt(mr.hover_endurance_min, 1)]),
                "",
                t("mr.hover_time.desc"),
            )
            .d(tf(
                "mr.hover_time_detail_fmt",
                &[
                    format!("{:.2}", mr.hover_endurance_min / 60.0),
                    format!("{:.1}", mr.e_usable_wh),
                ],
            )),
            card(
                t("mr_cruise_power"),
                fmt(mr.p_total_cruise, 1),
                "W",
                t("mr.cruise_power.desc"),
            )
            .d(tf(
                "mr.cruise_power_detail_fmt",
                &[format!("{:.1}", mr.p_total_cruise_85)],
            )),
            card(
                t("mr_cruise_time"),
                tf("mr.time_min_fmt", &[fmt(mr.cruise_endurance_min, 1)]),
                "",
                t("mr.cruise_time.desc"),
            )
            .d(tf(
                "mr.at_speed_kmh_fmt",
                &[format!("{:.1}", p.mr_v_cruise * 3.6)],
            )),
            card(
                t("mr_range"),
                tf("mr.range_km_fmt", &[fmt(mr.range_km, 1)]),
                "",
                t("mr.range.desc"),
            )
            .d(tf(
                "mr.range_detail_fmt",
                &[
                    format!("{:.1}", mr.range_best_km),
                    format!("{:.0}", mr.v_best_range),
                ],
            )),
        ],
    );
}

// ---------------------------------------------------------------------------------------------
// Ağırlık merkezi & denge
// ---------------------------------------------------------------------------------------------

pub fn cg_inputs(ui: &mut Ui, p: &mut Project, _m: &Model) {
    section_header(ui, t("mr_section_cg_table"));
    step_ui::note(ui, t("mr.cg.datum"));
    let mut to_remove = None;
    for (i, c) in p.mr_components.iter_mut().enumerate() {
        let head = format!("{}. {} · {} g", i + 1, c.name, fmt(c.mass * 1000.0, 0));
        let col = super::theme::tokens(ui).text;
        egui::CollapsingHeader::new(step_ui::short_header(ui, &head, col, false))
            .id_salt(("mr_cg_comp", i))
            .default_open(false)
            .show(ui, |ui| {
                step_ui::field_label(ui, t("col_name"), "");
                ui.add(
                    egui::TextEdit::singleline(&mut c.name)
                        .desired_width((ui.available_width() - 8.0).max(40.0)),
                );
                g_row(ui, t("col_mass_g"), &mut c.mass, 0.0, 5000.0, "");
                mm_row(ui, t("col_x_mm"), &mut c.x, -1000.0, 1000.0, t("mr.tip_x"));
                mm_row(ui, t("col_y_mm"), &mut c.y, -1000.0, 1000.0, t("mr.tip_y"));
                if ui.button(t("btn_delete")).clicked() {
                    to_remove = Some(i);
                }
            });
    }
    if let Some(idx) = to_remove {
        p.mr_components.remove(idx);
    }
    ui.horizontal_wrapped(|ui| {
        if ui.button(t("btn_add_component")).clicked() {
            p.mr_components.push(crate::project::MrComponent::new(
                t("comp.default.new_component"),
                0.050,
                0.0,
                0.0,
            ));
        }
        if ui.button(t("btn_reset_default_components")).clicked() {
            p.mr_components = crate::project::default_mr_components();
        }
    });
    step_ui::note(ui, t("mr.cg.note"));
}

pub fn cg_results(ui: &mut Ui, _p: &mut Project, m: &Model) {
    let mr = &m.mr;
    section_header(ui, t("mr_section_cg"));
    cards(
        ui,
        &[
            card(
                t("mr.cg.geom_center"),
                format!(
                    "x: {:.1}, y: {:.1}",
                    mr.geom_center_x * 1000.0,
                    mr.geom_center_y * 1000.0
                ),
                "mm",
                t("mr.cg.geom_center.desc"),
            ),
            card(
                t("mr.cg.calc_cg"),
                format!("x: {:.1}, y: {:.1}", mr.cg_x * 1000.0, mr.cg_y * 1000.0),
                "mm",
                t("mr.cg.calc_cg.desc"),
            ),
            card(
                t("mr_cg_offset"),
                fmt(mr.cg_offset_d * 1000.0, 1),
                "mm",
                t("mr.cg_offset.desc"),
            )
            .d(tf(
                "mr.cg_offset_detail_fmt",
                &[format!("{:.1}", mr.cg_offset_frac * 100.0)],
            ))
            .s(if mr.cg_offset_ok {
                Status::Ok
            } else {
                Status::Warn
            }),
        ],
    );
}

/// Tüm multirotor adımları alt alta (başsız testler için).
pub fn show(ui: &mut Ui, p: &mut Project, m: &Model, lib: &Library) {
    frame_inputs(ui, p, m);
    frame_results(ui, p, m);
    mass_inputs(ui, p, m, lib);
    mass_results(ui, p, m);
    motors_inputs(ui, p, m);
    motors_results(ui, p, m);
    battery_inputs(ui, p, m);
    battery_results(ui, p, m);
    cg_inputs(ui, p, m);
    cg_results(ui, p, m);
}
