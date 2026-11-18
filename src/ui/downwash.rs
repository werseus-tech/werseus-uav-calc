// SPDX-License-Identifier: AGPL-3.0-or-later
//! Pervane–gövde etkileşimi (downwash) uzman ekranı: rotorlar ve iz izdüşümleriyle üstten şema,
//! parça başına aşağı yük tablosu, toplamlar ve sade dilde "bu ne demek" satırları.
//! Hesap `crate::downwash` içindedir; ayarlanabilir katsayılar yalnız bu ekranın durumunda tutulur
//! (proje dosyasına yazılmaz) ve proje değerleri DEĞİŞTİRİLMEZ.
use super::components::{self, Status, section_header};
use super::theme::{self, Tokens, radius, space};
use super::widgets::{fmt, section};
use crate::calc::Model;
use crate::downwash::blockage::{ItemKind, Shape};
use crate::downwash::{Coeffs, DownwashOut, HoverOut, SlipOut, SlipPoint, evaluate};
use crate::i18n::{t, tf};
use crate::project::Project;
use egui::{Color32, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2, epaint::PathShape};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum View {
    Hover,
    Cruise,
}

/// Ekran durumu (projeye yazılmaz).
pub struct DwState {
    pub coeffs: Coeffs,
    view: View,
}

impl Default for DwState {
    fn default() -> Self {
        DwState {
            coeffs: Coeffs::default(),
            view: View::Hover,
        }
    }
}

pub fn show(ui: &mut Ui, p: &Project, m: &Model, st: &mut DwState) {
    section_header(ui, t("tab_downwash"));
    components::banner(ui, Status::Info, t("downwash.disclaimer"));
    ui.add_space(space::XS);
    let out = evaluate(p, m, &st.coeffs);
    let has_h = out.hover.is_some();
    let has_s = out.slip.is_some();
    if has_h && has_s {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut st.view, View::Hover, t("downwash.view_hover"));
            ui.selectable_value(&mut st.view, View::Cruise, t("downwash.view_cruise"));
        });
    } else if has_s {
        st.view = View::Cruise;
    } else {
        st.view = View::Hover;
    }
    match (st.view, &out) {
        (View::Hover, DownwashOut { hover: Some(h), .. }) => show_hover(ui, h),
        (View::Cruise, DownwashOut { slip: Some(s), .. }) => show_slip(ui, s),
        _ => {
            ui.colored_label(theme::tokens(ui).warning, t("downwash.no_data"));
        }
    }
    ui.add_space(space::S);
    coeff_editor(ui, &mut st.coeffs, p);
    ui.label(RichText::new(t("downwash.sources")).color(theme::tokens(ui).text_weak));
}

// ------------------------------------------------------------------------------------------
// Askı
// ------------------------------------------------------------------------------------------

fn show_hover(ui: &mut Ui, h: &HoverOut) {
    let tk = theme::tokens(ui);
    section(ui, t("downwash.hover_title"));
    ui.label(tf(
        "downwash.hover_inputs",
        &[
            format!("{}", h.rotors.len()),
            fmt(h.weight, 1),
            fmt(h.v_i, 1),
            fmt(h.v_far, 1),
            fmt(h.r_slip_far * 1000.0, 0),
        ],
    ));
    let w = ui.available_width().max(260.0);
    let height = (w * 0.55).clamp(220.0, 420.0);
    hover_schematic(ui, h, w, height);
    ui.label(RichText::new(t("downwash.schematic_legend")).color(tk.text_weak));

    section(ui, t("downwash.table_title"));
    if h.rows.is_empty() {
        ui.label(t("downwash.nothing_under"));
    } else {
        egui::Grid::new("dw_items")
            .num_columns(5)
            .striped(true)
            .show(ui, |ui| {
                for k in [
                    "downwash.col_item",
                    "downwash.col_cd",
                    "downwash.col_area",
                    "downwash.col_force",
                    "downwash.col_pct",
                ] {
                    ui.label(RichText::new(t(k)).strong());
                }
                ui.end_row();
                for r in &h.rows {
                    let name = if r.count > 1 {
                        format!("{} ×{}", r.name, r.count)
                    } else {
                        r.name.clone()
                    };
                    ui.label(name);
                    ui.label(fmt(r.cd, 2));
                    ui.label(fmt(r.area * 1e4, 1));
                    ui.label(fmt(r.force, 2));
                    ui.label(format!("{} %", fmt(r.pct, 2)));
                    ui.end_row();
                }
            });
    }

    section(ui, t("downwash.totals_title"));
    let dl = 100.0 * h.download_frac;
    let it = 100.0 * h.interaction_frac;
    egui::Grid::new("dw_totals")
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            kv(ui, t("downwash.row_download"), format!("{} %", fmt(dl, 2)));
            kv(
                ui,
                t("downwash.row_interaction"),
                format!("{} %", fmt(it, 2)),
            );
            kv(
                ui,
                t("downwash.row_net"),
                format!("{} %", fmt(100.0 * h.net_frac, 1)),
            );
            kv(
                ui,
                t("downwash.row_thrust_req"),
                format!("{} N ({} N)", fmt(h.thrust_required, 1), fmt(h.weight, 1)),
            );
            kv(
                ui,
                t("downwash.row_extra_power"),
                format!(
                    "{} W (+{} %)",
                    fmt(h.extra_power, 1),
                    fmt(h.extra_power_pct, 1)
                ),
            );
            if let Some(c) = h.coax {
                kv(
                    ui,
                    t("downwash.row_coax"),
                    format!("κ = {} (+{} %)", fmt(c.kappa, 2), fmt(c.penalty_pct, 0)),
                );
            }
        });

    // Bu ne demek?
    section(ui, t("downwash.meaning_title"));
    let (st, key) = if dl < 3.0 {
        (Status::Ok, "downwash.mean_dl_low")
    } else if dl < 8.0 {
        (Status::Warn, "downwash.mean_dl_mid")
    } else {
        (Status::Err, "downwash.mean_dl_high")
    };
    components::banner(ui, st, &tf(key, &[fmt(dl, 1)]));
    components::banner(
        ui,
        Status::Info,
        &tf(
            "downwash.mean_power",
            &[fmt(h.extra_power, 0), fmt(h.endurance_drop_pct, 1)],
        ),
    );
    let worst = h
        .pairs
        .iter()
        .map(|p| p.tip_ratio)
        .fold(f64::INFINITY, f64::min);
    if h.pairs.iter().any(|p| p.overlap()) {
        components::banner(
            ui,
            Status::Err,
            &tf(
                "downwash.mean_overlap",
                &[fmt(100.0 * worst, 0), fmt(it, 1)],
            ),
        );
    } else if worst.is_finite() && h.pairs.iter().any(|p| p.loss > 0.0) {
        components::banner(
            ui,
            Status::Warn,
            &tf(
                "downwash.mean_tip_close",
                &[fmt(100.0 * worst, 0), fmt(it, 1)],
            ),
        );
    } else {
        components::banner(ui, Status::Ok, t("downwash.mean_tip_ok"));
    }
    if h.coax.is_some() {
        components::banner(ui, Status::Info, t("downwash.mean_coax"));
    }
}

fn kv(ui: &mut Ui, k: &str, v: String) {
    ui.label(k);
    ui.label(v);
    ui.end_row();
}

/// Model koordinatından (x ileri +, y sağ +) ekrana: yukarı = ileri.
struct Vp {
    c: Pos2,
    mid: [f64; 2],
    s: f32,
}

impl Vp {
    fn fit(rect: Rect, b: [f64; 4]) -> Self {
        let (wx, wy) = ((b[1] - b[0]).max(1e-3), (b[3] - b[2]).max(1e-3));
        let s = ((rect.height() as f64 / wx).min(rect.width() as f64 / wy) * 0.9) as f32;
        Vp {
            c: rect.center(),
            mid: [0.5 * (b[0] + b[1]), 0.5 * (b[2] + b[3])],
            s,
        }
    }
    fn pt(&self, q: [f64; 2]) -> Pos2 {
        Pos2::new(
            self.c.x + ((q[1] - self.mid[1]) as f32) * self.s,
            self.c.y - ((q[0] - self.mid[0]) as f32) * self.s,
        )
    }
}

fn grow(b: &mut [f64; 4], q: [f64; 4]) {
    if q.iter().all(|v| v.is_finite()) {
        b[0] = b[0].min(q[0]);
        b[1] = b[1].max(q[1]);
        b[2] = b[2].min(q[2]);
        b[3] = b[3].max(q[3]);
    }
}

fn canvas(ui: &mut Ui, w: f32, h: f32) -> (Rect, egui::Painter) {
    let tk = theme::tokens(ui);
    let (resp, painter) = ui.allocate_painter(Vec2::new(w, h), Sense::hover());
    let rect = resp.rect;
    painter.rect_filled(rect, theme::corner(radius::M), tk.viewport_bg);
    painter.rect_stroke(
        rect,
        theme::corner(radius::M),
        Stroke::new(1.0, tk.border),
        egui::StrokeKind::Inside,
    );
    (rect, painter.with_clip_rect(rect))
}

fn body_fill(tk: &Tokens) -> Color32 {
    if tk.dark { tk.surface_alt } else { tk.surface }
}

fn draw_shape(painter: &egui::Painter, vp: &Vp, shape: &Shape, fill: Color32, stroke: Stroke) {
    for poly in shape.outline() {
        let pts: Vec<Pos2> = poly.iter().map(|q| vp.pt(*q)).collect();
        if pts.len() >= 3 && pts.iter().all(|p| p.x.is_finite() && p.y.is_finite()) {
            painter.add(PathShape::convex_polygon(pts, fill, stroke));
        }
    }
}

fn hover_schematic(ui: &mut Ui, h: &HoverOut, w: f32, height: f32) {
    let tk = theme::tokens(ui);
    let (rect, painter) = canvas(ui, w, height);
    let mut b = [
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ];
    for r in &h.rotors {
        grow(&mut b, [r.x - r.r, r.x + r.r, r.y - r.r, r.y + r.r]);
    }
    for it in &h.items {
        // Kanat çok genişse şema rotorlara odaklansın: yalnız rotor çevresi.
        if it.kind != ItemKind::Wing {
            grow(&mut b, it.shape.bbox());
        }
    }
    if !b.iter().all(|v| v.is_finite()) {
        return;
    }
    let vp = Vp::fit(rect, b);
    for it in &h.items {
        draw_shape(
            &painter,
            &vp,
            &it.shape,
            body_fill(tk),
            Stroke::new(1.0, tk.border_strong),
        );
    }
    for r in &h.rotors {
        let c = vp.pt([r.x, r.y]);
        painter.circle_filled(
            c,
            (h.r_slip_struct as f32) * vp.s,
            tk.accent.gamma_multiply(0.18),
        );
        painter.circle_stroke(c, (r.r as f32) * vp.s, Stroke::new(1.2, tk.accent));
        painter.circle_stroke(
            c,
            (h.r_slip_far as f32) * vp.s,
            Stroke::new(0.8, tk.accent.gamma_multiply(0.6)),
        );
    }
    let dot = (1.2 * (vp.s / 400.0)).clamp(0.8, 2.0);
    for q in &h.block.blocked_pts {
        painter.circle_filled(vp.pt(*q), dot, tk.warning);
    }
    for pr in &h.pairs {
        if pr.warn(0.10) {
            let (a, bb) = (h.rotors[pr.i], h.rotors[pr.j]);
            let col = if pr.overlap() { tk.danger } else { tk.warning };
            painter.line_segment(
                [vp.pt([a.x, a.y]), vp.pt([bb.x, bb.y])],
                Stroke::new(1.5, col),
            );
        }
    }
    painter.text(
        rect.left_top() + Vec2::new(8.0, 6.0),
        egui::Align2::LEFT_TOP,
        t("downwash.forward"),
        egui::FontId::proportional(12.0),
        tk.text_weak,
    );
}

// ------------------------------------------------------------------------------------------
// Pervane hava akımı (sabit kanat / VTOL seyir)
// ------------------------------------------------------------------------------------------

/// Hava akımı tablosu satırı: (anahtar, biçimleyici).
type SlipRow = (&'static str, fn(&SlipPoint) -> String);

fn show_slip(ui: &mut Ui, s: &SlipOut) {
    let tk = theme::tokens(ui);
    section(ui, t("downwash.slip_title"));
    ui.label(tf(
        if s.pusher {
            "downwash.slip_inputs_pusher"
        } else {
            "downwash.slip_inputs_tractor"
        },
        &[fmt(s.d_prop * 1000.0, 0)],
    ));
    let w = ui.available_width().max(260.0);
    let height = (w * 0.5).clamp(200.0, 380.0);
    slip_schematic(ui, s, w, height);
    ui.label(RichText::new(t("downwash.slip_legend")).color(tk.text_weak));

    egui::Grid::new("dw_slip")
        .num_columns(3)
        .striped(true)
        .show(ui, |ui| {
            ui.label("");
            ui.label(RichText::new(t("downwash.col_cruise")).strong());
            ui.label(RichText::new(t("downwash.col_low")).strong());
            ui.end_row();
            let rows: [SlipRow; 9] = [
                ("downwash.row_v", |q| format!("{} m/s", fmt(q.v, 1))),
                ("downwash.row_thrust", |q| format!("{} N", fmt(q.thrust, 2))),
                ("downwash.row_vslip", |q| {
                    format!("{} m/s", fmt(q.v_slip, 1))
                }),
                ("downwash.row_vwing", |q| {
                    format!("{} m/s", fmt(q.v_at_wing, 1))
                }),
                ("downwash.row_simm", |q| {
                    format!("{} m² ({} %)", fmt(q.s_imm, 4), fmt(100.0 * q.frac_wing, 1))
                }),
                ("downwash.row_dd_wing", |q| {
                    format!("{} N", fmt(q.dd_wing, 3))
                }),
                ("downwash.row_dd_fuse", |q| {
                    format!("{} N", fmt(q.dd_fuse, 3))
                }),
                ("downwash.row_dd_total", |q| {
                    format!("{} N ({} %)", fmt(q.dd_total, 3), fmt(q.dd_pct, 1))
                }),
                ("downwash.row_dl", |q| {
                    format!("{} N ({} %)", fmt(q.dl, 2), fmt(q.dl_pct, 1))
                }),
            ];
            for (k, f) in rows {
                ui.label(t(k));
                ui.label(f(&s.cruise));
                ui.label(f(&s.low));
                ui.end_row();
            }
        });

    section(ui, t("downwash.meaning_title"));
    let st = if s.cruise.dd_pct < 5.0 {
        Status::Ok
    } else {
        Status::Warn
    };
    components::banner(
        ui,
        st,
        &tf("downwash.mean_slip_drag", &[fmt(s.cruise.dd_pct, 1)]),
    );
    components::banner(
        ui,
        Status::Info,
        &tf("downwash.mean_slip_lift", &[fmt(s.low.dl_pct, 1)]),
    );
    if s.pusher {
        components::banner(ui, Status::Info, t("downwash.mean_pusher"));
    }
}

fn slip_schematic(ui: &mut Ui, s: &SlipOut, w: f32, height: f32) {
    let tk = theme::tokens(ui);
    let (rect, painter) = canvas(ui, w, height);
    let mut b = [
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ];
    for it in &s.items {
        grow(&mut b, it.shape.bbox());
    }
    let r = 0.5 * s.d_prop;
    grow(&mut b, [s.x_prop - 0.05, s.x_prop + 0.05, -r, r]);
    if !b.iter().all(|v| v.is_finite()) {
        return;
    }
    let vp = Vp::fit(rect, b);
    for it in &s.items {
        draw_shape(
            &painter,
            &vp,
            &it.shape,
            body_fill(tk),
            Stroke::new(1.0, tk.border_strong),
        );
    }
    // Hava akımı şeridi: pervaneden akış yönünde (çekicide geriye), kanattaki yarıçapla.
    let rw = s.cruise.r_at_wing.max(1e-4);
    let x_end = if s.pusher { s.x_prop - 0.6 } else { b[0] };
    let band = Shape::Rect {
        x0: s.x_prop,
        x1: x_end,
        y0: -rw,
        y1: rw,
    };
    draw_shape(
        &painter,
        &vp,
        &band,
        tk.accent.gamma_multiply(0.18),
        Stroke::new(1.0, tk.accent.gamma_multiply(0.6)),
    );
    // Hava akımındaki kanat parçası.
    for it in &s.items {
        if let Shape::Wing {
            x_le_root,
            c_root,
            taper,
            half_span,
            tan_sweep_le,
            y_min,
        } = it.shape
        {
            let y1 = rw.min(half_span);
            if y1 > y_min {
                let imm = Shape::Wing {
                    x_le_root,
                    c_root,
                    taper: (1.0 - (1.0 - taper) * (y1 / half_span)).max(0.0),
                    half_span: y1,
                    tan_sweep_le,
                    y_min,
                };
                draw_shape(
                    &painter,
                    &vp,
                    &imm,
                    tk.warning.gamma_multiply(0.5),
                    Stroke::new(1.0, tk.warning),
                );
            }
        }
    }
    // Pervane diski (üstten bir çizgi).
    painter.line_segment(
        [vp.pt([s.x_prop, -r]), vp.pt([s.x_prop, r])],
        Stroke::new(3.0, tk.accent),
    );
    painter.text(
        rect.left_top() + Vec2::new(8.0, 6.0),
        egui::Align2::LEFT_TOP,
        t("downwash.forward"),
        egui::FontId::proportional(12.0),
        tk.text_weak,
    );
}

// ------------------------------------------------------------------------------------------
// Ayarlanabilir katsayılar
// ------------------------------------------------------------------------------------------

fn coeff_editor(ui: &mut Ui, c: &mut Coeffs, p: &Project) {
    egui::CollapsingHeader::new(RichText::new(t("downwash.coeffs_title")).strong())
        .id_salt("dw_coeffs")
        .default_open(false)
        .show(ui, |ui| {
            ui.label(RichText::new(t("downwash.coeffs_intro")).color(theme::tokens(ui).text_weak));
            let auto_cd = c.arm_cd(p);
            egui::Grid::new("dw_coeffs_grid")
                .num_columns(2)
                .show(ui, |ui| {
                    let row =
                        |ui: &mut Ui, key: &'static str, v: &mut f64, lo: f64, hi: f64, sp: f64| {
                            ui.label(t(key));
                            ui.add(
                                egui::DragValue::new(v)
                                    .range(lo..=hi)
                                    .speed(sp)
                                    .max_decimals(3),
                            );
                            ui.end_row();
                        };
                    row(ui, "downwash.c_cd_arm", &mut c.cd_arm, 0.0, 3.0, 0.01);
                    row(
                        ui,
                        "downwash.c_arm_width",
                        &mut c.arm_width,
                        0.0,
                        0.2,
                        0.001,
                    );
                    row(ui, "downwash.c_cd_body", &mut c.cd_body, 0.0, 3.0, 0.01);
                    row(
                        ui,
                        "downwash.c_cd_component",
                        &mut c.cd_component,
                        0.0,
                        3.0,
                        0.01,
                    );
                    row(
                        ui,
                        "downwash.c_density",
                        &mut c.component_density,
                        100.0,
                        5000.0,
                        10.0,
                    );
                    row(ui, "downwash.c_cd_wing", &mut c.cd_wing, 0.0, 3.0, 0.01);
                    row(
                        ui,
                        "downwash.c_cd_fuselage",
                        &mut c.cd_fuselage,
                        0.0,
                        3.0,
                        0.01,
                    );
                    row(ui, "downwash.c_cd_boom", &mut c.cd_boom, 0.0, 3.0, 0.01);
                    row(
                        ui,
                        "downwash.c_rotor_height",
                        &mut c.rotor_height,
                        0.0,
                        0.5,
                        0.001,
                    );
                    row(ui, "downwash.c_hub", &mut c.hub_cutout, 0.0, 0.5, 0.01);
                    row(ui, "downwash.c_tip_warn", &mut c.tip_warn, 0.01, 0.5, 0.005);
                    row(
                        ui,
                        "downwash.c_tip_loss",
                        &mut c.tip_loss_touch,
                        0.0,
                        0.2,
                        0.001,
                    );
                    row(
                        ui,
                        "downwash.c_lift_factor",
                        &mut c.slip_lift_factor,
                        0.0,
                        1.5,
                        0.01,
                    );
                });
            ui.label(tf("downwash.c_auto_note", &[fmt(auto_cd, 2)]));
            if ui.button(t("downwash.c_reset")).clicked() {
                *c = Coeffs::default();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Her tür ve örnek proje, iki temada ve iki dilde, her görünümde başsız çizilir (panik yok).
    #[test]
    fn renders_every_type_in_both_themes_and_languages() {
        use crate::ui::theme::ThemeChoice;
        let lib = crate::calc::Library::embedded();
        let mut projects: Vec<Project> = crate::project::DroneType::ALL
            .iter()
            .map(|dt| {
                let mut p = Project::default();
                p.set_drone_type(*dt);
                p
            })
            .collect();
        projects.push(crate::project::load_example("quad_250_x.json").unwrap());
        projects.push(crate::project::load_example("vtol_quadplane.json").unwrap());
        for p in &projects {
            let m = crate::calc::evaluate(p, &lib);
            for choice in [ThemeChoice::Dark, ThemeChoice::Light] {
                for lang in [crate::i18n::Lang::Tr, crate::i18n::Lang::En] {
                    let ctx = egui::Context::default();
                    theme::install(&ctx, choice);
                    for view in [View::Hover, View::Cruise] {
                        let mut st = DwState {
                            view,
                            ..Default::default()
                        };
                        crate::i18n::with_lang(lang, || {
                            for _ in 0..2 {
                                let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                                    egui::CentralPanel::default().show(ui, |ui| {
                                        egui::ScrollArea::vertical().show(ui, |ui| {
                                            show(ui, p, &m, &mut st);
                                            // Katsayı bölümü açıkken de çizilsin.
                                            coeff_editor(ui, &mut st.coeffs, p);
                                        });
                                    });
                                });
                                out.textures_delta.clear();
                            }
                        });
                    }
                }
            }
        }
    }

    /// Tüm `downwash.` anahtarları iki dilde de var (eksik anahtar anahtarın kendisini gösterir).
    #[test]
    fn downwash_i18n_keys_exist_in_both_languages() {
        let src = include_str!("downwash.rs");
        let prod = src.split("#[cfg(test)]").next().unwrap();
        let tr = crate::i18n::tr_table();
        let en = crate::i18n::en_table();
        let mut n = 0;
        for part in prod.split('"').skip(1).step_by(2) {
            if part.starts_with("downwash.") || part == "tab_downwash" {
                n += 1;
                assert!(tr.contains_key(part), "TR eksik: {part}");
                assert!(en.contains_key(part), "EN eksik: {part}");
            }
        }
        for k in [
            "downwash.item_body",
            "downwash.item_arms",
            "downwash.item_component",
            "downwash.item_wing",
            "downwash.item_fuselage",
            "downwash.item_booms",
            "downwash.item_tail",
        ] {
            assert!(tr.contains_key(k) && en.contains_key(k), "{k}");
        }
        assert!(n > 40, "{n}");
    }
}
