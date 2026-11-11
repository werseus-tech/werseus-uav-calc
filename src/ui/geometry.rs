// SPDX-License-Identifier: AGPL-3.0-or-later
//! Adım 3 / Sekme 3: Kanat & Kuyruk Geometrisi.
//!
//! Yeni üç panelli düzene taşınan ilk ekran:
//! - [`inputs_panel`]: sol paneldeki girdiler (yeni `components::number_row` satırları).
//! - [`results`]: sağ paneldeki sonuçlar (profil önizleme, sonuç kartları, formüller). Uyarılar
//!   kabuk tarafından sağ panelde "bu adımın uyarıları" olarak gösterilir.
//! - Basit modda kanat büyüklüğü sorusu ve profil kartları da sol paneldedir.
//! - [`show`]: ikisini alt alta çizer (eski tek sütun kullanım ve başsız testler için).
//!
//! Alanlar, aralıklar, ondalıklar ve ölçekler eski `widgets::fld` satırlarıyla birebir aynıdır;
//! hesap sonucu değişmez. 2D şema artık merkez görünümde (3D/2D) olduğu için burada tekrar
//! çizilmez.
use super::components::{
    self, MetricCard, NumberInput, Status, card::card_grid, number_row, section::section_header,
};
use super::{theme, widgets::fmt};
use crate::calc::Model;
use crate::calc::geometry::TailKind;
use crate::i18n::{t, tf};
use crate::project::Project;
use egui::{RichText, Ui};

/// NACA kodundan profil noktalarını yeniden üretir (kod 4 hane değilse dokunmaz).
fn regen_naca(p: &mut Project, set_name: bool) {
    if p.airfoil.naca_code.len() != 4 {
        return;
    }
    if let Ok(pts) = crate::cad::airfoil::generate_naca4_points(
        &p.airfoil.naca_code,
        p.airfoil.num_points,
        p.airfoil.closed_te,
    ) {
        if set_name {
            p.airfoil.name = format!("NACA {}", p.airfoil.naca_code);
        }
        p.airfoil.points = pts;
    }
}

/// Sol panel: kanat, kuyruk ve profil girdileri.
pub fn inputs_panel(ui: &mut Ui, p: &mut Project, m: &Model) {
    let simple = p.aero_simple.simple_mode;
    if simple {
        // Basit mod: "ne kadar büyük?" sorusu ve kanat alanı kaydırıcısı (çift giriş yok).
        super::aero_simple::wing_size_panel(ui, p, m);
        super::aero_simple::sync_derived(ui, p, m);
    }

    section_header(ui, t("geo.section_wing_inputs"));
    let b_note = format!("= {:.2} m", p.b);
    number_row(
        ui,
        NumberInput::new(t("geo.input.b"), &mut p.b)
            .scale(1000.0)
            .unit("mm")
            .range(600.0, 3000.0)
            .decimals(0)
            .help(t("geo.input.b.desc"))
            .note(Some(b_note)),
    );
    // Basit modda kanat alanı çalışma alanındaki kaydırıcıdan girilir (çift giriş olmasın).
    if !simple {
        let note = format!("= {:.0} dm²", p.s * 100.0);
        number_row(
            ui,
            NumberInput::new(t("geo.input.s"), &mut p.s)
                .unit("m²")
                .range(0.05, 1.5)
                .decimals(3)
                .help(t("geo.input.s.desc"))
                .note(Some(note)),
        );
    }
    number_row(
        ui,
        NumberInput::new(t("geo.input.taper"), &mut p.taper)
            .range(0.2, 1.0)
            .decimals(2)
            .help(t("geo.input.taper.desc")),
    );
    number_row(
        ui,
        NumberInput::new(t("geo.input.sweep_le"), &mut p.sweep_le_deg)
            .unit("°")
            .range(-5.0, 40.0)
            .decimals(1)
            .help(t("geo.input.sweep_le.desc")),
    );
    number_row(
        ui,
        NumberInput::new(t("geo.input.tc_w"), &mut p.tc_w)
            .range(0.05, 0.20)
            .decimals(3)
            .help(t("geo.input.tc_w.desc")),
    );

    section_header(ui, t("geo.section_airfoil"));
    if simple {
        super::aero_simple::airfoil_cards(ui, p, m);
    } else {
        airfoil_selector(ui, p);
    }

    section_header(ui, t("geo.section_tail_config"));
    ui.label(RichText::new(t("geo.tail_kind_label")).color(theme::tokens(ui).text_weak));
    egui::ComboBox::from_id_salt("geo_tail_kind")
        .selected_text(p.tail_kind.label())
        .width(ui.available_width() - 8.0)
        .show_ui(ui, |ui| {
            for k in [
                TailKind::Conventional,
                TailKind::VTail,
                TailKind::FlyingWing,
            ] {
                ui.selectable_value(&mut p.tail_kind, k, k.label());
            }
        });
    ui.add_space(4.0);
    match p.tail_kind {
        TailKind::Conventional => {
            let n1 = format!("= {:.0} cm²", p.s_h * 1e4);
            number_row(
                ui,
                NumberInput::new(t("geo.input.s_h"), &mut p.s_h)
                    .unit("m²")
                    .range(0.0, 0.3)
                    .decimals(3)
                    .help(t("geo.input.s_h.desc"))
                    .note(Some(n1)),
            );
            let n2 = format!("= {:.0} cm²", p.s_v * 1e4);
            number_row(
                ui,
                NumberInput::new(t("geo.input.s_v"), &mut p.s_v)
                    .unit("m²")
                    .range(0.0, 0.2)
                    .decimals(3)
                    .help(t("geo.input.s_v.desc"))
                    .note(Some(n2)),
            );
        }
        TailKind::VTail => {
            let n = format!("= {:.0} cm²", p.s_vtail * 1e4);
            number_row(
                ui,
                NumberInput::new(t("geo.input.s_vtail"), &mut p.s_vtail)
                    .unit("m²")
                    .range(0.0, 0.3)
                    .decimals(3)
                    .help(t("geo.input.s_vtail.desc"))
                    .note(Some(n)),
            );
            number_row(
                ui,
                NumberInput::new(t("geo.input.gamma_v"), &mut p.gamma_v_deg)
                    .unit("°")
                    .range(15.0, 60.0)
                    .decimals(0)
                    .help(t("geo.input.gamma_v.desc")),
            );
        }
        TailKind::FlyingWing => {
            ui.label(
                RichText::new(t("geo.flying_wing_no_tail")).color(theme::tokens(ui).text_weak),
            );
        }
    }
    if p.tail_kind != TailKind::FlyingWing {
        number_row(
            ui,
            NumberInput::new(t("geo.input.l_h"), &mut p.l_h)
                .scale(1000.0)
                .unit("mm")
                .range(200.0, 1500.0)
                .decimals(0)
                .help(t("geo.input.l_h.desc")),
        );
        number_row(
            ui,
            NumberInput::new(t("geo.input.l_v"), &mut p.l_v)
                .scale(1000.0)
                .unit("mm")
                .range(200.0, 1500.0)
                .decimals(0)
                .help(t("geo.input.l_v.desc")),
        );
        number_row(
            ui,
            NumberInput::new(t("geo.input.vh_target"), &mut p.vh_target)
                .range(0.1, 1.2)
                .decimals(2)
                .help(t("geo.input.vh_target.desc")),
        );
        number_row(
            ui,
            NumberInput::new(t("geo.input.vv_target"), &mut p.vv_target)
                .range(0.01, 0.1)
                .decimals(3)
                .help(t("geo.input.vv_target.desc")),
        );
        number_row(
            ui,
            NumberInput::new(t("geo.input.c_tail"), &mut p.c_tail)
                .scale(1000.0)
                .unit("mm")
                .range(30.0, 300.0)
                .decimals(0)
                .help(t("geo.input.c_tail.desc")),
        );
        number_row(
            ui,
            NumberInput::new(t("geo.input.tc_t"), &mut p.tc_t)
                .range(0.04, 0.20)
                .decimals(3)
                .help(t("geo.input.tc_t.desc")),
        );
        number_row(
            ui,
            NumberInput::new(t("geo.input.cla_h"), &mut p.cla_h)
                .unit("1/rad")
                .range(2.0, 6.0)
                .decimals(2)
                .help(t("geo.input.cla_h.desc")),
        );
        number_row(
            ui,
            NumberInput::new(t("geo.input.eta_h"), &mut p.eta_h)
                .range(0.5, 1.0)
                .decimals(2)
                .help(t("geo.input.eta_h.desc")),
        );
    }
    tail_airfoil_section(ui, p);
}

/// Gelişmiş modda profil seçimi (NACA 4 haneli ya da .dat dosyası).
fn airfoil_selector(ui: &mut Ui, p: &mut Project) {
    ui.horizontal_wrapped(|ui| {
        ui.label(t("geo.airfoil_select_label"));
        let is_naca = p.airfoil.is_naca;
        if ui
            .selectable_label(is_naca, t("geo.airfoil_type_naca"))
            .clicked()
        {
            p.airfoil.is_naca = true;
            if p.airfoil.naca_code.is_empty() {
                p.airfoil.naca_code = "2412".into();
            }
            regen_naca(p, true);
        }
        if ui
            .selectable_label(!is_naca, t("geo.airfoil_type_dat"))
            .clicked()
        {
            p.airfoil.is_naca = false;
        }
    });
    if p.airfoil.is_naca {
        ui.horizontal_wrapped(|ui| {
            ui.label(t("geo.naca_code_label"));
            let mut code = p.airfoil.naca_code.clone();
            if ui
                .add(egui::TextEdit::singleline(&mut code).desired_width(55.0))
                .changed()
                && code.len() <= 4
                && code.chars().all(|c| c.is_ascii_digit())
            {
                p.airfoil.naca_code = code;
                regen_naca(p, true);
            }
            if ui
                .checkbox(&mut p.airfoil.closed_te, t("geo.airfoil_closed_te"))
                .changed()
            {
                regen_naca(p, false);
            }
        });
        ui.horizontal(|ui| {
            ui.label(t("geo.airfoil_pts_count"));
            let mut n = p.airfoil.num_points;
            if ui
                .add(egui::DragValue::new(&mut n).range(40..=300).speed(2.0))
                .changed()
            {
                p.airfoil.num_points = n;
                regen_naca(p, false);
            }
        });
    } else {
        ui.label(tf("geo.current_profile_name", &[&p.airfoil.name]));
        if ui.button(t("geo.btn_load_dat")).clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Airfoil DAT (*.dat)", &["dat"])
                .pick_file()
            {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(prof) = crate::cad::airfoil::parse_selig_dat(&content) {
                        p.airfoil = prof;
                    }
                }
            }
        }
    }
}

fn status_ws(ws: f64) -> Status {
    if (45.0..=100.0).contains(&ws) {
        Status::Ok
    } else {
        Status::Warn
    }
}

/// Sağ panel: profil önizleme, kanat ve kuyruk sonuç kartları, formüller.
pub fn results(ui: &mut Ui, p: &mut Project, m: &Model) {
    section_header(ui, t("geo.section_airfoil"));
    airfoil_preview(ui, p);

    section_header(ui, t("geo.section_wing_outputs"));
    let w = &m.wing;
    let wing_cards = [
        MetricCard {
            title: t("geo.out.ar"),
            value: fmt(w.ar, 2),
            unit: "",
            detail: t("geo.out.ar_rec").into(),
            status: if w.ar >= 4.0 {
                Status::Ok
            } else {
                Status::Warn
            },
            why: false,
            tooltip: t("geo.out.ar.desc"),
            clickable: false,
        },
        MetricCard {
            title: t("geo.out.c_r"),
            value: fmt(w.c_r * 1000.0, 0),
            unit: "mm",
            detail: String::new(),
            status: Status::Neutral,
            why: false,
            tooltip: t("geo.out.c_r.desc"),
            clickable: false,
        },
        MetricCard {
            title: t("geo.out.c_t"),
            value: fmt(w.c_t * 1000.0, 0),
            unit: "mm",
            detail: "≥ ~85 mm".into(),
            status: Status::from_ok(m.re_tip >= 1e5),
            why: false,
            tooltip: t("geo.out.c_t.desc"),
            clickable: false,
        },
        MetricCard {
            title: t("geo.out.mac"),
            value: fmt(w.mac * 1000.0, 1),
            unit: "mm",
            detail: String::new(),
            status: Status::Neutral,
            why: false,
            tooltip: t("geo.out.mac.desc"),
            clickable: false,
        },
        MetricCard {
            title: t("geo.out.y_mac"),
            value: fmt(w.y_mac * 1000.0, 0),
            unit: "mm",
            detail: String::new(),
            status: Status::Neutral,
            why: false,
            tooltip: t("geo.out.y_mac.desc"),
            clickable: false,
        },
        MetricCard {
            title: t("geo.out.x_le_mac"),
            value: fmt(w.x_le_mac * 1000.0, 1),
            unit: "mm",
            detail: String::new(),
            status: Status::Neutral,
            why: false,
            tooltip: t("geo.out.x_le_mac.desc"),
            clickable: false,
        },
        MetricCard {
            title: t("geo.out.ws"),
            value: fmt(m.ws, 1),
            unit: "N/m²",
            detail: format!("{} kg/m²", fmt(m.ws / 9.80665, 2)),
            status: status_ws(m.ws),
            why: false,
            tooltip: t("geo.out.ws.desc"),
            clickable: false,
        },
    ];
    card_grid(ui, wing_cards.len(), 140.0, |ui, i, w| {
        components::metric_card(ui, &wing_cards[i], w);
    });

    section_header(ui, t("geo.section_tail_outputs"));
    let tail = &m.tail;
    let mut tail_cards = vec![
        MetricCard {
            title: t("geo.out.s_h_eff"),
            value: fmt(tail.s_h_eff * 1e4, 0),
            unit: "cm²",
            detail: String::new(),
            status: Status::Neutral,
            why: false,
            tooltip: t("geo.out.s_h_eff.desc"),
            clickable: false,
        },
        MetricCard {
            title: t("geo.out.s_v_eff"),
            value: fmt(tail.s_v_eff * 1e4, 0),
            unit: "cm²",
            detail: String::new(),
            status: Status::Neutral,
            why: false,
            tooltip: t("geo.out.s_v_eff.desc"),
            clickable: false,
        },
        MetricCard {
            title: t("geo.out.s_t"),
            value: fmt(tail.s_t * 1e4, 0),
            unit: "cm²",
            detail: String::new(),
            status: Status::Neutral,
            why: false,
            tooltip: t("geo.out.s_t.desc"),
            clickable: false,
        },
    ];
    if p.tail_kind != TailKind::FlyingWing {
        tail_cards.push(MetricCard {
            title: "V_H",
            value: fmt(tail.v_h, 3),
            unit: "",
            detail: format!("{} {:.2}", t("geo.target_prefix"), p.vh_target),
            status: Status::Neutral,
            why: false,
            tooltip: t("geo.out.v_h.desc"),
            clickable: false,
        });
        tail_cards.push(MetricCard {
            title: "V_V",
            value: fmt(tail.v_v, 4),
            unit: "",
            detail: format!("{} {:.3}", t("geo.target_prefix"), p.vv_target),
            status: Status::Neutral,
            why: false,
            tooltip: t("geo.out.v_v.desc"),
            clickable: false,
        });
        tail_cards.push(MetricCard {
            title: t("geo.out.s_h_req"),
            value: fmt(tail.s_h_req * 1e4, 0),
            unit: "cm²",
            detail: String::new(),
            status: Status::Neutral,
            why: false,
            tooltip: t("geo.out.s_h_req.desc"),
            clickable: false,
        });
        tail_cards.push(MetricCard {
            title: t("geo.out.s_v_req"),
            value: fmt(tail.s_v_req * 1e4, 0),
            unit: "cm²",
            detail: String::new(),
            status: Status::Neutral,
            why: false,
            tooltip: t("geo.out.s_v_req.desc"),
            clickable: false,
        });
        if p.tail_kind == TailKind::VTail {
            tail_cards.push(MetricCard {
                title: t("geo.out.s_vtail_req"),
                value: fmt(tail.s_vtail_req * 1e4, 0),
                unit: "cm²",
                detail: String::new(),
                status: Status::Neutral,
                why: false,
                tooltip: t("geo.out.s_vtail_req.desc"),
                clickable: false,
            });
        }
    }
    card_grid(ui, tail_cards.len(), 140.0, |ui, i, w| {
        components::metric_card(ui, &tail_cards[i], w);
    });
}

/// Profil önizleme: hesaplanan t/c, "uygula" düğmesi ve kontur çizimi.
fn airfoil_preview(ui: &mut Ui, p: &mut Project) {
    let tk = theme::tokens(ui);
    let plot_pts: Vec<[f64; 2]> = p.airfoil.points.clone();
    let tc_calc = p.airfoil.max_thickness();
    egui::Frame::new()
        .fill(tk.surface)
        .stroke(egui::Stroke::new(1.0, tk.border))
        .corner_radius(theme::corner(theme::radius::M))
        .inner_margin(theme::margin(theme::space::S))
        .show(ui, |ui| {
            ui.set_max_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(tf(
                    "geo.airfoil_tc_label",
                    &[format!("{:.1}%", tc_calc * 100.0)],
                ));
                if ui.button(t("geo.btn_apply_tc")).clicked() {
                    p.tc_w = tc_calc;
                }
            });
            egui_plot::Plot::new("airfoil_preview")
                .width((ui.available_width() - 2.0).max(40.0))
                .data_aspect(1.0)
                .height(110.0)
                .allow_zoom(false)
                .allow_drag(false)
                .allow_scroll(false)
                .show(ui, |plot_ui| {
                    plot_ui.line(
                        egui_plot::Line::new(
                            t("geo.plot_airfoil_contour"),
                            egui_plot::PlotPoints::from(plot_pts),
                        )
                        .color(tk.accent)
                        .width(2.0),
                    );
                    plot_ui.line(
                        egui_plot::Line::new(
                            t("geo.plot_chord_line"),
                            egui_plot::PlotPoints::from(vec![[0.0, 0.0], [1.0, 0.0]]),
                        )
                        .color(tk.text_weak)
                        .style(egui_plot::LineStyle::Dashed { length: 5.0 }),
                    );
                });
        });
}

/// Kuyruk (ya da winglet) profil bilgisi ve t/c eşleştirme.
fn tail_airfoil_section(ui: &mut Ui, p: &mut Project) {
    let tk = theme::tokens(ui);
    match p.tail_kind {
        TailKind::Conventional | TailKind::VTail => {
            section_header(ui, t("geo.section_tail_airfoil"));
            let conventional = p.tail_kind == TailKind::Conventional;
            // Klasik: yatay + dikey yüzeyin alan ağırlıklı profil t/c'si; V: V yüzey profili.
            let tc_tail = if conventional {
                p.tail_tc_from_profiles()
            } else {
                p.tail_airfoil_v.max_thickness()
            };
            if conventional {
                ui.label(format!(
                    "{} | {}",
                    tf("geo.tail_airfoil_h_label", &[&p.tail_airfoil_h.name]),
                    tf("geo.tail_airfoil_v_label", &[&p.tail_airfoil_v.name])
                ));
            } else {
                ui.label(tf(
                    "geo.tail_airfoil_vtail_label",
                    &[&p.tail_airfoil_v.name],
                ));
            }
            ui.horizontal_wrapped(|ui| {
                ui.label(tf(
                    "geo.tail_airfoil_tc_label",
                    &[format!("{:.1}%", tc_tail * 100.0)],
                ));
                if ui.button(t("geo.btn_apply_tc_tail")).clicked() {
                    p.tc_t = tc_tail;
                }
            });
            if (p.tc_t - tc_tail).abs() > 0.005 {
                components::banner(
                    ui,
                    Status::Warn,
                    &tf(
                        "geo.tail_tc_mismatch_warning",
                        &[
                            format!("{:.1}%", p.tc_t * 100.0),
                            format!("{:.1}%", tc_tail * 100.0),
                        ],
                    ),
                );
            }
        }
        TailKind::FlyingWing => {
            section_header(ui, t("geo.section_winglet_airfoil"));
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(tf("geo.winglet_airfoil_label", &[&p.tail_airfoil_v.name]))
                        .color(tk.text),
                );
                if ui.button(t("geo.btn_derive_winglet_from_wing")).clicked() {
                    p.tail_airfoil_v = p.airfoil.clone();
                    p.tail_airfoil_v.name = format!("{} (Winglet)", p.airfoil.name);
                }
                if ui.button(t("geo.btn_reset_winglet_symmetric")).clicked() {
                    p.tail_airfoil_v = crate::cad::AirfoilProfile::default_tail_symmetric();
                }
            });
        }
    }
}

/// Girdiler + sonuçlar alt alta (tek sütun kullanım ve testler).
pub fn show(ui: &mut Ui, p: &mut Project, m: &Model) {
    inputs_panel(ui, p, m);
    ui.separator();
    results(ui, p, m);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Taşınan ekran her kuyruk türünde ve iki modda başsız çizilir; girdi yokken proje değişmez.
    #[test]
    fn geometry_screen_renders_without_changing_project() {
        let lib = crate::calc::Library::embedded();
        for kind in [
            TailKind::Conventional,
            TailKind::VTail,
            TailKind::FlyingWing,
        ] {
            for simple in [false, true] {
                let mut p = Project::default();
                p.tail_kind = kind;
                p.aero_simple.simple_mode = simple;
                let ctx = egui::Context::default();
                for _ in 0..2 {
                    let m = crate::calc::evaluate(&p, &lib);
                    let before = p.to_json().unwrap();
                    let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                        egui::CentralPanel::default().show(ui, |ui| {
                            egui::ScrollArea::vertical().show(ui, |ui| {
                                inputs_panel(ui, &mut p, &m);
                                results(ui, &mut p, &m);
                            });
                        });
                    });
                    out.textures_delta.clear();
                    if !simple {
                        // Gelişmiş modda çizim projeyi değiştirmez (basit mod türetme yazabilir).
                        assert_eq!(before, p.to_json().unwrap(), "{kind:?}");
                    }
                }
            }
        }
    }
}
