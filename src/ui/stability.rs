// SPDX-License-Identifier: AGPL-3.0-or-later
//! Adım 8 / Sekme 8: Ağırlık Merkezi & Stabilite: datum/geometri girdileri ve bileşen
//! listesi sol panel, sonuçlar + statik marj çubuğu + batarya konumu önerisi sağ panel; AG/NN
//! merkez görünümde 3D/2D işaretli. Alanlar, aralıklar ve ondalıklar eskisiyle birebir aynıdır.
use super::components::{NumberInput, Status, number_row, section::section_header};
use super::step_ui::{self, CardExt, card, cards};
use super::{theme, widgets::fmt};
use crate::calc::Model;
use crate::calc::geometry::TailKind;
use crate::i18n::{t, tf};
use crate::project::{Component, Link, Project};
use egui::{Align2, FontId, Sense, Stroke, Ui};

/// Sol panel: datum ve geometri girdileri + bileşen tablosu (her bileşen katlanabilir satır).
pub fn inputs(ui: &mut Ui, p: &mut Project, m: &Model) {
    section_header(ui, t("stab.section_datum_geo"));
    step_ui::note(ui, t("stab.datum_explanation"));
    number_row(
        ui,
        NumberInput::new(t("stab.input.x_root_le"), &mut p.x_root_le)
            .scale(1000.0)
            .unit("mm")
            .range(0.0, 1500.0)
            .decimals(0)
            .help(t("stab.input.x_root_le.desc")),
    );
    number_row(
        ui,
        NumberInput::new(t("stab.input.x_ac_frac"), &mut p.x_ac_frac)
            .scale(100.0)
            .unit("% MAC")
            .range(20.0, 30.0)
            .decimals(1)
            .help(t("stab.input.x_ac_frac.desc")),
    );
    number_row(
        ui,
        NumberInput::new(t("stab.input.sm_target"), &mut p.sm_target)
            .scale(100.0)
            .unit("%")
            .range(1.0, 25.0)
            .decimals(1)
            .help(t("stab.input.sm_target.desc")),
    );
    number_row(
        ui,
        NumberInput::new(t("stab.input.cla_h"), &mut p.cla_h)
            .unit("1/rad")
            .range(2.0, 6.0)
            .decimals(2)
            .help(t("stab.input.cla_h.desc")),
    );
    number_row(
        ui,
        NumberInput::new(t("stab.input.eta_h"), &mut p.eta_h)
            .range(0.5, 1.0)
            .decimals(2)
            .help(t("stab.input.eta_h.desc")),
    );

    section_header(ui, t("stab.section_comp_table"));
    let mut remove: Option<usize> = None;
    let links: Vec<(Link, &str)> = Link::ALL.iter().map(|l| (*l, l.label())).collect();
    for (i, c) in p.components.iter_mut().enumerate() {
        let auto_g = m
            .stab
            .comps
            .get(i)
            .map(|x| x.mass * 1000.0)
            .unwrap_or(f64::NAN);
        let shown_g = if c.link == Link::None {
            c.mass * 1000.0
        } else {
            auto_g
        };
        let head = format!("{}. {} · {} g", i + 1, c.name, fmt(shown_g, 0));
        let tk = theme::tokens(ui);
        egui::CollapsingHeader::new(step_ui::short_header(ui, &head, tk.text, false))
            .id_salt(("stab_comp", i))
            .default_open(false)
            .show(ui, |ui| {
                step_ui::field_label(ui, t("stab.col_name"), "");
                ui.add(
                    egui::TextEdit::singleline(&mut c.name)
                        .desired_width((ui.available_width() - 8.0).max(40.0)),
                );
                step_ui::combo(ui, ("link", i), t("col_link"), "", &mut c.link, &links);
                if c.link == Link::None {
                    number_row(
                        ui,
                        NumberInput::new(t("col_mass_g"), &mut c.mass)
                            .scale(1000.0)
                            .unit("g")
                            .range(0.0, 5000.0)
                            .decimals(0)
                            .help(t("stab.tip_manual_mass")),
                    );
                } else {
                    ui.label(tf("stab.auto_mass_label", &[fmt(auto_g, 0)]))
                        .on_hover_text(t("stab.tip_auto_mass"));
                }
                for (label, v, tip) in [
                    ("x", &mut c.x, t("stab.tip_x")),
                    ("y", &mut c.y, t("stab.tip_y")),
                    ("z", &mut c.z, t("stab.tip_z")),
                ] {
                    number_row(
                        ui,
                        NumberInput::new(label, v)
                            .scale(1000.0)
                            .unit("mm")
                            .range(-2000.0, 3000.0)
                            .decimals(0)
                            .help(tip),
                    );
                }
                if ui.button(t("btn_delete")).clicked() {
                    remove = Some(i);
                }
            });
    }
    if let Some(i) = remove {
        p.components.remove(i);
    }
    if ui.button(t("stab.btn_add_row")).clicked() {
        p.components.push(Component {
            name: t("stab.default_new_part").into(),
            link: Link::None,
            mass: 0.05,
            x: 0.3,
            y: 0.0,
            z: 0.0,
        });
    }
}

/// Sağ panel: AG/NN sonuçları, statik marj çubuğu, batarya konumu çözücüsü.
pub fn results(ui: &mut Ui, p: &mut Project, m: &Model) {
    let s = &m.stab;
    section_header(ui, t("stab.section_outputs"));
    cards(
        ui,
        &[
            card(
                t("stab.out.total_mass"),
                fmt(s.cg_nose.mass * 1000.0, 0),
                "g",
                t("stab.out.total_mass.desc"),
            )
            .d(tf("stab.out.mtow_ref", &[fmt(m.weight.m_mtow * 1000.0, 0)])),
            card(
                t("stab.out.cg_nose"),
                fmt(s.cg_nose.x * 1000.0, 1),
                "mm",
                t("stab.out.cg_nose.desc"),
            ),
            card(
                t("stab.out.cg_rel_mac"),
                fmt(s.x_cg_rel * 1000.0, 1),
                "mm",
                t("stab.out.cg_rel_mac.desc"),
            )
            .d(tf(
                "stab.out.mac_pct",
                &[fmt(s.x_cg_rel / m.wing.mac * 100.0, 1)],
            )),
            card(
                t("stab.out.cg_yz"),
                format!(
                    "{} / {}",
                    fmt(s.cg_nose.y * 1000.0, 1),
                    fmt(s.cg_nose.z * 1000.0, 1)
                ),
                "mm",
                t("stab.out.cg_yz.desc"),
            ),
            card(
                t("stab.out.x_ac_w"),
                fmt(s.np.x_ac_w * 1000.0, 1),
                "mm",
                t("stab.out.x_ac_w.desc"),
            )
            .d(t("stab.unit_mm_mac_le")),
            card(
                t("stab.out.dx_tail"),
                fmt(s.np.dx_tail * 1000.0, 1),
                "mm",
                t("stab.out.dx_tail.desc"),
            ),
            card(
                t("stab.out.deps"),
                fmt(s.np.deps, 3),
                "",
                t("stab.out.deps.desc"),
            ),
            card(
                t("stab.out.np_nose"),
                fmt(s.x_np_nose * 1000.0, 1),
                "mm",
                t("stab.out.np_nose.desc"),
            ),
            card(
                t("stab.out.np_rel_mac"),
                fmt(s.np.x_np * 1000.0, 1),
                "mm",
                t("stab.out.np_rel_mac.desc"),
            )
            .d(tf(
                "stab.out.mac_pct",
                &[fmt(s.np.x_np / m.wing.mac * 100.0, 1)],
            )),
            card(
                t("stab.out.sm"),
                fmt(s.sm * 100.0, 1),
                "%",
                t("stab.out.sm.desc"),
            )
            .d(tf(
                "stab.out.sm_band_fmt",
                &[
                    format!("{:.0}", s.band.0 * 100.0),
                    format!("{:.0}", s.band.1 * 100.0),
                ],
            ))
            .s(step_ui::ok(s.in_band)),
        ],
    );
    sm_bar(ui, s.sm, s.band, p.tail_kind);
    if p.tail_kind == TailKind::FlyingWing {
        step_ui::note(ui, t("stab.flying_wing_note"));
    }

    section_header(ui, t("stab.section_batt_solver"));
    let mut list = vec![
        card(
            t("stab.out.target_sm"),
            fmt(p.sm_target * 100.0, 1),
            "%",
            t("stab.out.target_sm.desc"),
        ),
        card(
            t("stab.out.target_cg"),
            fmt(s.x_cg_target_rel * 1000.0, 1),
            "mm",
            t("stab.out.target_cg.desc"),
        )
        .d(tf(
            "stab.out.mac_pct",
            &[fmt(s.x_cg_target_rel / m.wing.mac * 100.0, 1)],
        )),
    ];
    if s.batt_row.is_some() {
        list.push(card(
            t("stab.out.sug_batt_pos"),
            fmt(s.batt_x_nose * 1000.0, 0),
            "mm",
            t("stab.out.sug_batt_pos.desc"),
        ));
        let sh = s.batt_x_shift * 1000.0;
        list.push(
            card(
                t("stab.out.rel_to_current"),
                format!("{}{}", if sh >= 0.0 { "+" } else { "" }, fmt(sh, 0)),
                "mm",
                t("stab.out.rel_to_current.desc"),
            )
            .d(if sh >= 0.0 {
                t("stab.shift_backward")
            } else {
                t("stab.shift_forward")
            }),
        );
    }
    cards(ui, &list);
    if s.batt_row.is_none() {
        step_ui::status_text(ui, Status::Warn, t("stab.no_batt_row_err"));
    }
    if s.batt_row.is_some()
        && s.batt_x_nose.is_finite()
        && ui.button(t("stab.btn_apply_batt_pos")).clicked()
    {
        // Tüm batarya satırları aynı miktarda kayar: ağırlık merkezi hedefe tam oturur.
        for &r in &s.batt_rows {
            p.components[r].x += s.batt_x_shift;
        }
    }
}

fn sm_bar(ui: &mut Ui, sm: f64, band: (f64, f64), kind: TailKind) {
    let tk = theme::tokens(ui);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().min(700.0), 46.0),
        Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    let (lo, hi) = (-0.05, 0.25);
    let x = |v: f64| rect.left() + ((v - lo) / (hi - lo)).clamp(0.0, 1.0) as f32 * rect.width();
    let bar = egui::Rect::from_min_max(
        egui::pos2(rect.left(), rect.top() + 14.0),
        egui::pos2(rect.right(), rect.top() + 32.0),
    );
    painter.rect_filled(bar, 3.0, tk.danger.gamma_multiply(0.45));
    let green = egui::Rect::from_min_max(
        egui::pos2(x(band.0), bar.top()),
        egui::pos2(x(band.1), bar.bottom()),
    );
    painter.rect_filled(green, 0.0, tk.success);
    for t_val in [-0.05, 0.0, 0.05, 0.10, 0.15, 0.20, 0.25] {
        // Kenar etiketleri dikdörtgenin içinde kalsın diye hizalama uçlarda değişir.
        let align = if t_val <= -0.05 {
            Align2::LEFT_BOTTOM
        } else if t_val >= 0.25 {
            Align2::RIGHT_BOTTOM
        } else {
            Align2::CENTER_BOTTOM
        };
        painter.text(
            egui::pos2(x(t_val), rect.bottom() - 2.0),
            align,
            format!("%{:.0}", t_val * 100.0),
            FontId::proportional(10.0),
            tk.text_weak,
        );
    }
    if sm.is_finite() {
        let top = egui::pos2(x(sm), bar.top() - 8.0);
        let bot = egui::pos2(x(sm), bar.bottom() + 2.0);
        painter.line_segment([top, bot], Stroke::new(3.0, tk.bg));
        painter.line_segment([top, bot], Stroke::new(1.5, tk.text));
    }
    let kind_str = if kind == TailKind::FlyingWing {
        t("stab.kind_flying_wing")
    } else {
        t("stab.kind_conventional")
    };
    painter.text(
        egui::pos2(rect.left(), rect.top()),
        Align2::LEFT_TOP,
        tf("stab.sm_band_label", &[kind_str]),
        FontId::proportional(11.0),
        tk.text_weak,
    );
}

/// Girdiler + sonuçlar alt alta (başsız testler için).
pub fn show(ui: &mut Ui, p: &mut Project, m: &Model) {
    inputs(ui, p, m);
    results(ui, p, m);
}
