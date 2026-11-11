// SPDX-License-Identifier: AGPL-3.0-or-later
//! Özet adımı/sekmesi: sol panelde proje adı ve platform, sağ panelde özet kartları ve
//! "Neden bu sonuç çıktı?" düğmeleri. Uyarı listesi (YZ ile düzelt düğmeleriyle) sağ panelin canlı
//! analiz bölümündedir; 2D şema merkez görünümde (3D/2D seçici).
use super::components::{Status, section::section_header};
use super::step_ui::{self, CardExt, card, cards};
use super::widgets::fmt;
use crate::calc::Model;
use crate::i18n::{t, tf};
use crate::project::{DroneType, Project};
use crate::teach::ResultMetric;
use egui::Ui;

/// Sol panel: proje adı ve platform bilgisi.
pub fn inputs(ui: &mut Ui, p: &mut Project, _m: &Model) {
    section_header(ui, t("summary.heading"));
    step_ui::field_label(ui, t("project_name_label"), "");
    ui.add(
        egui::TextEdit::singleline(&mut p.name)
            .desired_width((ui.available_width() - 8.0).max(40.0)),
    );
    step_ui::text(
        ui,
        &tf(
            "summary.platform_header_fmt",
            &[p.drone_type.label(), &p.name],
        ),
    );
}

fn tri3(x: f64, good: f64, warn: f64) -> Status {
    step_ui::tri(x, good, warn)
}

/// Sağ panel: "Neden?" düğmeleri ve platforma özel özet kartları.
pub fn results(ui: &mut Ui, p: &mut Project, m: &Model, teach_why: &mut Option<ResultMetric>) {
    // Öğretmen modu: Neden bu sonuç çıktı?
    section_header(ui, t("teach.btn_why_result"));
    ui.horizontal_wrapped(|ui| {
        let mut b = |ui: &mut Ui, label: &str, metric: ResultMetric| {
            if ui.small_button(label).clicked() {
                *teach_why = Some(metric);
            }
        };
        if p.drone_type.is_multirotor() {
            b(
                ui,
                t("teach.metric.hover_throttle"),
                ResultMetric::HoverThrottle,
            );
            b(
                ui,
                t("teach.metric.disk_loading"),
                ResultMetric::DiskLoading,
            );
            b(ui, t("teach.metric.hover_time"), ResultMetric::FlightTime);
        } else {
            b(ui, "V_stall", ResultMetric::VStall);
            b(ui, "L/D max", ResultMetric::LdMax);
            b(
                ui,
                t("teach.metric.static_margin"),
                ResultMetric::StaticMargin,
            );
            b(
                ui,
                t("teach.metric.wing_loading"),
                ResultMetric::WingLoading,
            );
            b(ui, t("teach.metric.flight_time"), ResultMetric::FlightTime);
            b(ui, t("teach.metric.spar_stress"), ResultMetric::SparStress);
        }
    });

    section_header(ui, t("summary.title"));
    let w = &m.weight;
    let a = &m.aero;
    let c = &m.cons;
    let min = t("ui.unit.min");
    let list = if p.drone_type == DroneType::Multirotor {
        let mr = &m.mr;
        vec![
            card("MTOW", fmt(mr.m_mtow * 1000.0, 0), "g", "").d(format!("W = {} N", fmt(mr.w, 1))),
            card(
                t("summary.card_mr_frame_size"),
                fmt(p.mr_frame_size * 1000.0, 0),
                "mm",
                "",
            )
            .d(tf(
                "summary.card_mr_frame_detail_fmt",
                &[
                    format!("{}", p.mr_arm_count),
                    p.mr_layout.label().to_string(),
                ],
            )),
            card(
                t("summary.card_mr_prop_dia"),
                fmt(p.mr_d_prop * 1000.0, 1),
                "mm",
                "",
            )
            .d(tf(
                "summary.card_mr_prop_detail_fmt",
                &[
                    format!("{:.1}", p.mr_d_prop / 0.0254),
                    format!("{:.1}", mr.d_prop_max_inch),
                ],
            ))
            .s(if mr.prop_overlap {
                Status::Err
            } else if !mr.prop_clearance_ok {
                Status::Warn
            } else {
                Status::Ok
            }),
            card(t("summary.card_mr_tw"), fmt(mr.tw_ratio, 2), "", "")
                .d(format!("T_max = {:.1} N", mr.t_max_total))
                .s(tri3(mr.tw_ratio, 2.0, 1.6)),
            card(
                t("summary.card_mr_hover_throttle"),
                format!("%{:.0}", mr.hover_throttle * 100.0),
                "",
                "",
            )
            .d(t("summary.card_mr_hover_throttle_desc"))
            .s(step_ui::tri_low(mr.hover_throttle, 0.50, 0.60)),
            card(
                t("summary.card_mr_hover_power"),
                fmt(mr.p_total_hover, 0),
                "W",
                "",
            )
            .d(tf(
                "summary.card_mr_hover_power_detail_fmt",
                &[
                    format!("{:.1}", mr.p_elec_motor),
                    format!("{:.1}", mr.p_shaft_motor),
                ],
            )),
            card(
                t("summary.card_mr_hover_endurance"),
                fmt(mr.hover_endurance_min, 1),
                min,
                "",
            )
            .d(tf(
                "summary.card_mr_hover_endurance_detail_fmt",
                &[
                    format!("{:.2}", mr.hover_endurance_min / 60.0),
                    format!("{:.0}", p.dod * 100.0),
                ],
            )),
            card(
                t("summary.card_mr_cruise_endurance_range"),
                fmt(mr.cruise_endurance_min, 1),
                min,
                "",
            )
            .d(tf(
                "summary.card_mr_range_detail_fmt",
                &[fmt(mr.range_km, 1), format!("{:.0}", p.mr_v_cruise * 3.6)],
            )),
            card(t("summary.card_battery_pack"), fmt(w.pack_wh, 0), "Wh", "").d(tf(
                "summary.card_battery_pack_detail_fmt",
                &[
                    format!("{}", p.n_series),
                    format!("{}", p.n_parallel),
                    fmt(w.m_battery * 1000.0, 0),
                    format!("{:.0}", w.f_batt * 100.0),
                ],
            )),
            card(
                t("summary.card_mr_hover_current"),
                fmt(mr.i_hover, 1),
                "A",
                "",
            )
            .d(tf(
                "summary.card_mr_hover_current_limit_fmt",
                &[format!("{:.0}", p.n_parallel as f64 * m.cell.max_cont_a)],
            ))
            .s(step_ui::ok(mr.i_hover_ok)),
            card(
                t("summary.card_mr_cg_offset"),
                fmt(mr.cg_offset_d * 1000.0, 1),
                "mm",
                "",
            )
            .d(tf(
                "summary.card_mr_cg_offset_detail_fmt",
                &[format!("{:.1}", mr.cg_offset_frac * 100.0)],
            ))
            .s(if mr.cg_offset_ok {
                Status::Ok
            } else {
                Status::Warn
            }),
        ]
    } else {
        let mut v = vec![
            card("MTOW", fmt(w.m_mtow * 1000.0, 0), "g", "").d(format!("W = {} N", fmt(w.w, 1))),
            card(
                t("summary.card_wing_span_area"),
                fmt(p.b * 1000.0, 0),
                "mm",
                "",
            )
            .d(format!(
                "S = {} m², AR = {}",
                fmt(p.s, 3),
                fmt(m.wing.ar, 1)
            )),
            card(t("summary.card_ws"), fmt(m.ws, 1), "N/m²", "").s(
                if (45.0..=100.0).contains(&m.ws) {
                    Status::Ok
                } else {
                    Status::Warn
                },
            ),
            card("V_stall", fmt(c.v_stall, 1), "m/s", "")
                .d(tf(
                    "summary.card_v_stall_detail_fmt",
                    &[fmt(c.v_stall * 3.6, 0), format!("{:.0}", p.v_stall_req)],
                ))
                .s(step_ui::ok(c.stall_ok)),
            card(t("summary.card_cruise_speed"), fmt(p.v_cr, 1), "m/s", "").d(tf(
                "summary.card_cruise_speed_detail_fmt",
                &[fmt(p.v_cr * 3.6, 0), format!("{:.0}", p.h)],
            )),
            card(
                t("summary.card_ld"),
                format!("{} / {}", fmt(m.perf_cr.ld, 1), fmt(a.ld_max, 1)),
                "",
                "",
            )
            .d(format!("CD0 = {}", fmt(a.cd0, 4))),
            card(
                t("summary.card_endurance_best"),
                fmt(m.perf_end.endurance_min(), 0),
                min,
                "",
            )
            .d(tf("summary.card_at_speed_fmt", &[fmt(m.perf_end.v, 1)])),
            card(
                t("summary.card_endurance_cruise"),
                fmt(m.perf_cr.endurance_min(), 0),
                min,
                "",
            )
            .d(tf(
                "summary.card_range_fmt",
                &[fmt(m.perf_cr.range_km(), 0)],
            )),
            card(
                t("summary.card_best_range"),
                fmt(m.perf_range.range_km(), 0),
                "km",
                "",
            )
            .d(tf("summary.card_at_speed_fmt", &[fmt(m.perf_range.v, 1)])),
            card(t("summary.card_battery"), fmt(w.pack_wh, 0), "Wh", "").d(format!(
                "{}S{}P, {} g, %{}",
                p.n_series,
                p.n_parallel,
                fmt(w.m_battery * 1000.0, 0),
                fmt(w.f_batt * 100.0, 0)
            )),
            card(
                t("summary.card_motor_power"),
                fmt(m.prop.p_motor_suggest, 0),
                "W",
                "",
            )
            .d(tf(
                "summary.card_motor_power_detail_fmt",
                &[fmt(p.p_max_elec, 0), fmt(m.prop.t_static, 1)],
            ))
            .s(step_ui::ok(m.prop.installed_ok)),
            card(
                t("summary.card_cruise_elec_power"),
                fmt(m.perf_cr.p_total, 0),
                "W",
                "",
            )
            .d(t("summary.card_including_hotel_load")),
            card(
                t("summary.card_static_margin"),
                format!("%{}", fmt(m.stab.sm * 100.0, 1)),
                "",
                "",
            )
            .d(tf(
                "summary.card_cg_np_mac_fmt",
                &[
                    fmt(m.stab.x_cg_rel / m.wing.mac * 100.0, 0),
                    fmt(m.stab.np.x_np / m.wing.mac * 100.0, 0),
                ],
            ))
            .s(step_ui::ok(m.stab.in_band)),
            card(t("summary.card_spar_ms"), fmt(m.st.ms, 2), "", "")
                .d(format!("σ = {} MPa", fmt(m.st.sigma_max / 1e6, 0)))
                .s(tri3(m.st.ms, 0.2, 0.0)),
        ];
        if p.drone_type == DroneType::VtolQuadplane {
            v.push(
                card(
                    t("summary.card_vtol_thrust"),
                    fmt(m.mr.t_max_total, 1),
                    "N",
                    "",
                )
                .d(tf(
                    "summary.card_vtol_thrust_detail_fmt",
                    &[format!("{:.2}", m.mr.tw_ratio)],
                ))
                .s(step_ui::ok(m.mr.tw_ratio >= 1.6)),
            );
            v.push(
                card(
                    t("summary.card_vtol_hover_endurance"),
                    fmt(m.mr.hover_endurance_min, 1),
                    min,
                    "",
                )
                .d(tf(
                    "summary.card_vtol_hover_power_fmt",
                    &[fmt(m.mr.p_total_hover, 0)],
                )),
            );
        }
        v
    };
    cards(ui, &list);
}

/// Sol + sağ içerik alt alta (başsız testler için).
pub fn show(ui: &mut Ui, p: &mut Project, m: &Model) {
    let mut why = None;
    inputs(ui, p, m);
    results(ui, p, m, &mut why);
}
