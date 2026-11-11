// SPDX-License-Identifier: AGPL-3.0-or-later
//! Yeni başlayanlar için "Basit mod" ekranları: Aerodinamik, Kanat (profil kartları, kanat büyüklüğü)
//! ve Tasarım Noktası adımlarının günlük dil sürümü. Matematik `calc::aero_presets` içindedir.
use super::components::{Status, section::section_header};
use super::step_ui;
use super::theme;
use super::widgets::{fmt, fmt_sci};
use crate::calc::Model;
use crate::calc::aero_presets::*;
use crate::calc::geometry::TailKind;
use crate::i18n::{t, tf};
use crate::project::Project;
use egui::{Color32, RichText, Ui};

fn section(ui: &mut Ui, title: &str) {
    section_header(ui, title);
}

// ---------------------------------------------------------------------------------------------
// Ortak yardımcılar
// ---------------------------------------------------------------------------------------------

pub(super) fn rating_color(ui: &Ui, r: Rating) -> Color32 {
    let tk = theme::tokens(ui);
    match r {
        Rating::Good => tk.success,
        Rating::Ok => tk.warning,
        Rating::Bad => tk.danger,
    }
}

fn focus_id() -> egui::Id {
    egui::Id::NULL.with("aeros_focus_target")
}

/// "Bunu değiştir" deneylerinin odaklanacağı denetimi işaretler.
fn request_focus(ui: &Ui, target: &'static str) {
    ui.ctx().data_mut(|d| d.insert_temp(focus_id(), target));
}

/// Bu denetim odak hedefiyse ekrana kaydırıp odaklar (tek seferlik).
fn focus_hook(ui: &Ui, r: &egui::Response, id: &'static str) {
    let hit = ui
        .ctx()
        .data(|d| d.get_temp::<&'static str>(focus_id()))
        .map(|x| x == id)
        .unwrap_or(false);
    if hit {
        r.scroll_to_me(Some(egui::Align::Center));
        r.request_focus();
        ui.ctx().data_mut(|d| d.remove::<&'static str>(focus_id()));
    }
}

/// Kaydırıcı + sayı kutusu birlikte; tipik aralık yeşil bant olarak gösterilir. Etiket üstte,
/// kaydırıcı panel genişliğinde (dar sol panelde taşmaz).
#[allow(clippy::too_many_arguments)]
fn slider_row(
    ui: &mut Ui,
    id: &'static str,
    label: &str,
    v: &mut f64,
    scale: f64,
    unit: &str,
    (lo, hi): (f64, f64),
    (tlo, thi): (f64, f64),
    dec: usize,
) -> bool {
    let tk = theme::tokens(ui);
    let mut changed = false;
    step_ui::field_label(ui, label, "");
    ui.horizontal(|ui| {
        // Değer kutusu ~80 px; kaydırıcı kalan genişliği alır.
        let w = (ui.available_width() - 96.0).max(40.0);
        ui.spacing_mut().slider_width = w;
        let mut x = *v * scale;
        let r = ui.add(
            egui::Slider::new(&mut x, lo..=hi)
                .suffix(format!(" {unit}"))
                .fixed_decimals(dec),
        );
        // Tipik aralık bandı (yeşil)
        let pad = 7.0;
        let x0 = r.rect.min.x + pad;
        let span = (w - 2.0 * pad).max(1.0);
        let f = |val: f64| ((val - lo) / (hi - lo)).clamp(0.0, 1.0) as f32;
        let band = egui::Rect::from_min_max(
            egui::pos2(x0 + span * f(tlo), r.rect.bottom() - 3.0),
            egui::pos2(x0 + span * f(thi), r.rect.bottom()),
        );
        ui.painter()
            .rect_filled(band, 1.0, tk.success.gamma_multiply(0.8));
        focus_hook(ui, &r, id);
        if r.changed() {
            *v = x / scale;
            changed = true;
        }
    });
    changed
}

fn combo<T: Copy + PartialEq>(
    ui: &mut Ui,
    id: &'static str,
    cur: &mut T,
    items: &[(T, &'static str)],
) -> bool {
    let label = items
        .iter()
        .find(|(v, _)| v == cur)
        .map(|(_, l)| *l)
        .unwrap_or("");
    let mut changed = false;
    let r = egui::ComboBox::from_id_salt(id)
        .width((ui.available_width() - 8.0).max(40.0))
        .truncate()
        .selected_text(label)
        .show_ui(ui, |ui| {
            for (v, l) in items {
                if ui.selectable_value(cur, *v, *l).changed() {
                    changed = true;
                }
            }
        });
    focus_hook(ui, &r.response, id);
    changed
}

pub(super) fn analogy_name(v_ms: f64) -> &'static str {
    match speed_analogy(v_ms) {
        SpeedAnalogy::Walk => t("aeros.analogy.walk"),
        SpeedAnalogy::Run => t("aeros.analogy.run"),
        SpeedAnalogy::Bike => t("aeros.analogy.bike"),
        SpeedAnalogy::FastBike => t("aeros.analogy.fast_bike"),
        SpeedAnalogy::CityCar => t("aeros.analogy.city_car"),
    }
}

/// CL_max tahmini için Reynolds sayısı: tutunma kaybı (stall) hızındaki değeri kullanır;
/// m.aero seyir hızında hesaplandığından Re, hız oranıyla ölçeklenir (Re ∝ V). Hedef tutunma
/// kaybı hızı (girdi) kullanıldığından döngüsel bağımlılık yoktur.
pub fn re_for_cl_max(p: &Project, m: &Model) -> f64 {
    let re_cr = m.aero.re_wing;
    if p.v_stall_req.is_finite() && p.v_stall_req > 0.0 && p.v_cr > p.v_stall_req {
        re_cr * p.v_stall_req / p.v_cr
    } else {
        re_cr
    }
}

/// Türetmeyi ilk kez açar: elle girilmiş (varsayılandan farklı) değerler özelleştirilmiş sayılır.
fn activate_derived(p: &mut Project) {
    let d = Project::default();
    let cur = AeroValues {
        cl_max_2d: p.cl_max_2d,
        cd_misc: p.cd_misc,
        s_wet_fuse: p.s_wet_fuse,
    };
    let def = AeroValues {
        cl_max_2d: d.cl_max_2d,
        cd_misc: d.cd_misc,
        s_wet_fuse: d.s_wet_fuse,
    };
    p.aero_simple.activate(&cur, &def);
}

fn make_ctx(p: &Project, m: &Model) -> Option<DeriveCtx> {
    if !m.inputs_ok || m.aero.items.len() < 3 {
        return None;
    }
    let fuse = &m.aero.items[1];
    // CD0_gövde / S_wet = Cf·FF / S_ref: s_wet_fuse sıfır/boş olsa bile türetme kilitlenmez.
    let fuse_cd0_per_wet = fuse.cf * fuse.ff / m.aero_geom.s;
    let c = DeriveCtx {
        s_ref: m.aero_geom.s,
        tc_w: p.tc_w,
        camber: p.airfoil.max_camber(),
        re_wing: re_for_cl_max(p, m),
        wing_cd0: m.aero.items[0].cd0,
        tail_cd0: m.aero.items[2].cd0,
        fuse_cd0_per_wet,
        fuse_wet_geo: m.fuse_wet_geo,
    };
    let ok = [
        c.s_ref,
        c.re_wing,
        c.wing_cd0,
        c.tail_cd0,
        c.fuse_cd0_per_wet,
        c.fuse_wet_geo,
    ]
    .iter()
    .all(|x| x.is_finite());
    ok.then_some(c)
}

/// Basit moddaysa türetilmiş değerleri proje girdilerine yazar (özelleştirilenler hariç).
/// Birkaç kare içinde kararlı hale gelir; değişim olunca yeniden çizim ister.
pub fn sync_derived(ui: &Ui, p: &mut Project, m: &Model) {
    if !p.aero_simple.simple_mode || !p.aero_simple.derived_active {
        return;
    }
    let Some(x) = make_ctx(p, m) else { return };
    let d = derive(&p.aero_simple, &x);
    let mut v = AeroValues {
        cl_max_2d: p.cl_max_2d,
        cd_misc: p.cd_misc,
        s_wet_fuse: p.s_wet_fuse,
    };
    let before = v;
    p.aero_simple.apply(&d, &mut v);
    if v != before {
        p.cl_max_2d = v.cl_max_2d;
        p.cd_misc = v.cd_misc;
        p.s_wet_fuse = v.s_wet_fuse;
        ui.ctx().request_repaint();
    }
}

fn delta_line(ui: &mut Ui, label: &str, d: f64, unit: &str, higher_is_better: bool) {
    if d.abs() < 0.05 {
        return;
    }
    let tk = theme::tokens(ui);
    let good = (d > 0.0) == higher_is_better;
    let col = if good { tk.success } else { tk.danger };
    ui.label(
        RichText::new(format!("{label} {d:+.1} {unit}"))
            .color(col)
            .small(),
    );
}

fn tracker_snapshot(m: &Model) -> Snapshot {
    Snapshot {
        v_stall_kmh: m.aero.v_stall * 3.6,
        ld_max: m.aero.ld_max,
        endurance_min: m.perf_end.endurance_min(),
    }
}

/// Anlık geri bildirim: bir değer değişince önceki model değerine göre farkları kısa süre gösterir.
fn feedback(ui: &mut Ui, m: &Model) {
    if !m.inputs_ok {
        return;
    }
    let now = ui.input(|i| i.time).max(0.001);
    let id = egui::Id::NULL.with("aeros_delta_tracker");
    let mut tr = ui
        .ctx()
        .data(|d| d.get_temp::<DeltaTracker>(id))
        .unwrap_or_default();
    let snap = tracker_snapshot(m);
    if snap.v_stall_kmh.is_finite() && snap.ld_max.is_finite() && snap.endurance_min.is_finite() {
        tr.update(snap, now);
    }
    let delta = tr.delta(now);
    ui.ctx().data_mut(|d| d.insert_temp(id, tr));
    if let Some(d) = delta {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(t("aeros.delta_title")).small().weak());
            delta_line(
                ui,
                t("aeros.delta.endurance"),
                d.endurance_min,
                t("aeros.unit.min"),
                true,
            );
            delta_line(ui, t("aeros.delta.v_stall"), d.v_stall_kmh, "km/h", false);
            delta_line(ui, t("aeros.delta.ld"), d.ld_max, "", true);
        });
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(500));
    }
}

// ---------------------------------------------------------------------------------------------
// Aerodinamik adımı (basit mod)
// ---------------------------------------------------------------------------------------------

/// Basit mod Aerodinamik: sorular, kaydırıcılar, türetilen değerler (sol panel).
pub fn aero_inputs(ui: &mut Ui, p: &mut Project, m: &Model) {
    let tk = theme::tokens(ui);
    step_ui::note(ui, t("aeros.intro"));
    section(ui, t("aeros.section_questions"));

    let mut changed = false;
    step_ui::field_label(ui, t("aeros.q.purpose"), t("aeros.q.purpose.desc"));
    changed |= combo(
        ui,
        "aeros_purpose",
        &mut p.aero_simple.purpose,
        &[
            (Purpose::Endurance, t("aeros.purpose.endurance")),
            (Purpose::Speed, t("aeros.purpose.speed")),
            (Purpose::Easy, t("aeros.purpose.easy")),
        ],
    );
    step_ui::field_label(ui, t("aeros.q.surface"), t("aeros.q.surface.desc"));
    changed |= combo(
        ui,
        "aeros_surface",
        &mut p.aero_simple.surface,
        &[
            (Surface::Rough, t("aeros.surface.rough")),
            (Surface::Sanded, t("aeros.surface.sanded")),
            (Surface::Painted, t("aeros.surface.painted")),
            (Surface::Foam, t("aeros.surface.foam")),
        ],
    );
    step_ui::field_label(ui, t("aeros.q.body"), t("aeros.q.body.desc"));
    changed |= combo(
        ui,
        "aeros_body",
        &mut p.aero_simple.body,
        &[
            (BodyShape::Slim, t("aeros.body.slim")),
            (BodyShape::Normal, t("aeros.body.normal")),
            (BodyShape::Boxy, t("aeros.body.boxy")),
        ],
    );
    step_ui::field_label(ui, t("aeros.q.externals"), t("aeros.q.externals.desc"));
    ui.horizontal_wrapped(|ui| {
        let e = &mut p.aero_simple.externals;
        changed |= ui.checkbox(&mut e.gimbal, t("aeros.ext.gimbal")).changed();
        changed |= ui.checkbox(&mut e.gear, t("aeros.ext.gear")).changed();
        changed |= ui
            .checkbox(&mut e.antenna, t("aeros.ext.antenna"))
            .changed();
        if !(e.gimbal || e.gear || e.antenna) {
            ui.label(RichText::new(t("aeros.ext.none")).weak());
        }
    });
    if changed {
        activate_derived(p);
    }

    // Öneri (uygulanmaz)
    let rec = recommend(p.aero_simple.purpose);
    ui.add_space(4.0);
    ui.add(
        egui::Label::new(
            RichText::new(tf(
                "aeros.recommend",
                &[
                    rec.naca.to_string(),
                    format!("{:.0}", rec.ar),
                    format!("{:.0}", rec.v_cruise),
                    format!("{:.0}", rec.v_cruise * 3.6),
                ],
            ))
            .color(tk.accent),
        )
        .wrap(),
    );

    // Kaydırıcılar
    section(ui, t("aeros.section_sliders"));
    let mut sl = false;
    sl |= slider_row(
        ui,
        "aeros_l_fuse",
        t("aero.input.l_fuse"),
        &mut p.l_fuse,
        1000.0,
        "mm",
        (300.0, 2000.0),
        (600.0, 1200.0),
        0,
    );
    sl |= slider_row(
        ui,
        "aeros_d_fuse",
        t("aero.input.d_fuse"),
        &mut p.d_fuse,
        1000.0,
        "mm",
        (40.0, 300.0),
        (70.0, 150.0),
        0,
    );
    step_ui::note(ui, t("aeros.slider_note"));
    let _ = sl;

    sync_derived(ui, p, m);

    // Türetilen değerler ve nedenleri
    section(ui, t("aeros.section_derived"));
    derived_table(ui, p, m);
}

/// Basit mod Aerodinamik: anlık fark, günlük dilde sonuçlar, karşılaştırma, öğren köşesi (sağ panel).
pub fn aero_results(ui: &mut Ui, p: &Project, m: &Model) {
    feedback(ui, m);
    section(ui, t("aeros.section_results"));
    results_panel(ui, p, m);
    ui.add_space(6.0);
    comparison_table(ui, p, m);
    ui.add_space(6.0);
    learn_corner(ui);
}

fn derived_table(ui: &mut Ui, p: &mut Project, m: &Model) {
    if !p.aero_simple.derived_active {
        step_ui::note(ui, t("aeros.not_active"));
        if ui.button(t("aeros.btn_derive_now")).clicked() {
            activate_derived(p);
        }
        return;
    }
    let Some(x) = make_ctx(p, m) else {
        ui.label("—");
        return;
    };
    let d = derive(&p.aero_simple, &x);
    let row = |ui: &mut Ui, label: &str, value: String, custom: bool, why: String| {
        ui.horizontal_wrapped(|ui| {
            ui.label(label);
            ui.strong(value);
            if custom {
                ui.label(RichText::new(t("aeros.badge_custom")).color(theme::tokens(ui).warning));
            } else {
                ui.label(RichText::new(t("aeros.badge_estimate")).weak());
            }
        });
        step_ui::note(ui, &why);
        ui.add_space(2.0);
    };
    row(
        ui,
        t("aero.input.cl_max_2d"),
        fmt(p.cl_max_2d, 2),
        p.aero_simple.ov_cl_max,
        tf(
            "aeros.why.clmax",
            &[
                format!("{:.0}", p.tc_w * 100.0),
                format!("{:.1}", p.airfoil.max_camber() * 100.0),
                fmt_sci(x.re_wing, 2),
            ],
        ),
    );
    row(
        ui,
        t("aero.input.cd_misc"),
        fmt(p.cd_misc, 4),
        p.aero_simple.ov_cd_misc,
        tf(
            "aeros.why.cdmisc",
            &[
                fmt(d.base_misc, 4),
                fmt(d.skin_extra, 4),
                fmt(d.form_extra, 4),
                fmt(d.ext_extra, 4),
            ],
        ),
    );
    row(
        ui,
        t("aero.input.s_wet_fuse"),
        format!("{} m²", fmt(p.s_wet_fuse, 3)),
        p.aero_simple.ov_s_wet,
        tf(
            "aeros.why.swet",
            &[
                fmt(m.fuse_wet_geo, 3),
                fmt(p.aero_simple.body.wet_multiplier(), 2),
            ],
        ),
    );
    if p.aero_simple.any_override() {
        step_ui::status_text(ui, Status::Warn, t("aeros.custom_note"));
        if ui.button(t("aeros.btn_reset_custom")).clicked() {
            p.aero_simple.reset_overrides();
        }
    }
    step_ui::note(ui, t("aeros.estimate_note"));
}

// ---------------------------------------------------------------------------------------------
// Günlük dilde sonuçlar
// ---------------------------------------------------------------------------------------------

pub(super) fn result_row(
    ui: &mut Ui,
    rating: Rating,
    text: String,
    tip: Option<&'static str>,
    extra: Option<String>,
) {
    let col = rating_color(ui, rating);
    egui::Frame::new()
        .stroke(egui::Stroke::new(1.5, col))
        .corner_radius(theme::corner(theme::radius::M))
        .inner_margin(theme::margin(theme::space::S - 2.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(egui::Label::new(RichText::new(text).color(col).strong()).wrap());
            if let Some(e) = extra {
                step_ui::text(ui, &e);
            }
            if rating != Rating::Good {
                if let Some(k) = tip {
                    step_ui::note(ui, &format!("{} {}", t("aeros.how_to_improve"), k));
                }
            }
        });
    ui.add_space(2.0);
}

/// El fırlatma hızı derecesi: tutunma kaybı hızının altı = Kötü, önerilenin altı = Orta.
pub fn launch_rating(v_launch: f64, v_stall: f64) -> Rating {
    if !(v_launch.is_finite() && v_stall.is_finite()) || v_launch < v_stall {
        Rating::Bad
    } else if v_launch >= suggest_launch_speed(v_stall) * 0.98 {
        Rating::Good
    } else {
        Rating::Ok
    }
}

/// Ana sonuçların günlük dil çevirisi, hedefe göre renk ve iyileştirme önerisi.
pub fn results_panel(ui: &mut Ui, p: &Project, m: &Model) {
    if !m.inputs_ok {
        ui.label("—");
        return;
    }
    let a = &m.aero;
    // Tutunma kaybı hızı
    let vs_kmh = a.v_stall * 3.6;
    let r = rate_lower(a.v_stall, p.v_stall_req);
    result_row(
        ui,
        r,
        tf(
            "aeros.res.stall",
            &[
                fmt(a.v_stall, 1),
                format!("{:.0}", vs_kmh),
                analogy_name(a.v_stall).to_string(),
            ],
        ),
        Some(t("aeros.tip.stall")),
        Some(tf(
            "aeros.res.stall_launch",
            &[
                format!("{:.0}", vs_kmh),
                format!("{:.0}", suggest_launch_speed(a.v_stall) * 3.6),
            ],
        )),
    );
    // L/D ve süzülme
    let glide_km = glide_distance_m(a.ld_max, 100.0) / 1000.0;
    result_row(
        ui,
        rate_higher(a.ld_max, 12.0, 8.0),
        tf("aeros.res.ld", &[fmt(a.ld_max, 1), fmt(glide_km, 1)]),
        Some(t("aeros.tip.ld")),
        None,
    );
    // Havada kalma
    let end_min = m.perf_end.endurance_min();
    result_row(
        ui,
        rate_higher(end_min, 45.0, 20.0),
        tf("aeros.res.endurance", &[format!("{:.0}", end_min)]),
        Some(t("aeros.tip.endurance")),
        None,
    );
    // Seyir hızı
    let vc = p.v_cr;
    ui.label(
        RichText::new(tf(
            "aeros.res.cruise",
            &[
                fmt(vc, 1),
                format!("{:.0}", vc * 3.6),
                analogy_name(vc).to_string(),
            ],
        ))
        .weak(),
    );
    ui.label(RichText::new(t("aeros.res.legend")).weak().small());
}

fn range_txt(r: (f64, f64), dec: usize) -> String {
    format!("{}–{}", fmt(r.0, dec), fmt(r.1, dec))
}

/// Senin tasarımın ↔ tipik hobi sınıfları (yaklaşık aralıklar; marka/ürün adı yok). Dar panelde
/// her satır "sütun: değer" kutusu olarak çizilir.
pub fn comparison_table(ui: &mut Ui, p: &Project, m: &Model) {
    let mine = [
        fmt(m.aero.ws, 0),
        fmt(m.wing.ar, 1),
        fmt(m.aero.ld_max, 1),
        fmt(m.aero.v_stall * 3.6, 0),
        fmt(m.perf_end.endurance_min(), 0),
    ];
    let _ = p;
    let rows: [(&str, usize); 5] = [
        (t("aeros.cmp.row.ws"), 0),
        (t("aeros.cmp.row.ar"), 1),
        (t("aeros.cmp.row.ld"), 2),
        (t("aeros.cmp.row.v_stall"), 3),
        (t("aeros.cmp.row.endurance"), 4),
    ];
    let table: Vec<step_ui::TableRow> = rows
        .iter()
        .map(|(label, i)| {
            let mut cells = vec![mine[*i].clone()];
            for row in TYPICAL {
                let r = match i {
                    0 => row.ws,
                    1 => row.ar,
                    2 => row.ld,
                    3 => row.v_stall_kmh,
                    _ => row.endurance_min,
                };
                cells.push(range_txt(r, 0));
            }
            step_ui::TableRow {
                name: label.to_string(),
                cells,
                status: Status::Neutral,
            }
        })
        .collect();
    step_ui::row_table(
        ui,
        "aeros_cmp",
        t("aeros.cmp.title"),
        &[
            t("aeros.cmp.mine"),
            t("aeros.cmp.motor_glider"),
            t("aeros.cmp.fpv_wing"),
            t("aeros.cmp.trainer"),
        ],
        &table,
        Some(t("aeros.cmp.note")),
    );
}

// ---------------------------------------------------------------------------------------------
// "Öğren" köşesi
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Concept {
    Lift,
    Drag,
    LiftDrag,
    Reynolds,
    Stall,
    Camber,
}

fn foil_points(code: &str) -> Vec<[f64; 2]> {
    crate::cad::airfoil::generate_naca4_points(code, 50, true).unwrap_or_default()
}

/// Profili `origin` etrafında `angle_deg` döndürüp ekran noktalarına çevirir (y yukarı).
fn foil_screen(
    pts: &[[f64; 2]],
    origin: egui::Pos2,
    chord_px: f32,
    angle_deg: f32,
) -> Vec<egui::Pos2> {
    let a = angle_deg.to_radians();
    let (s, c) = a.sin_cos();
    pts.iter()
        .map(|q| {
            let x = (q[0] as f32 - 0.25) * chord_px;
            let y = q[1] as f32 * chord_px;
            // pozitif açı: burun yukarı
            let rx = x * c + y * s;
            let ry = -x * s + y * c;
            egui::pos2(origin.x + rx, origin.y - ry)
        })
        .collect()
}

fn draw_concept(ui: &mut Ui, kind: Concept) {
    let tk = theme::tokens(ui);
    let w = ui.available_width().clamp(120.0, 300.0);
    // Çizim 300 px genişliğe göre tasarlandı; dar panelde orantılı küçülür.
    let k = w / 300.0;
    let (resp, painter) = ui.allocate_painter(egui::vec2(w, 110.0 * k), egui::Sense::hover());
    let rect = resp.rect;
    let fg = tk.text;
    let weak = tk.text_weak;
    let line = egui::Stroke::new(1.5, fg);
    let blue = tk.accent;
    let (green, red, amber) = (tk.success, tk.danger, tk.warning);
    let c = rect.center();
    let v = |x: f32, y: f32| egui::vec2(x * k, y * k);
    let label = |pos: egui::Pos2, s: &str, col: Color32| {
        painter.text(
            pos,
            egui::Align2::CENTER_CENTER,
            s,
            egui::FontId::proportional(12.0 * k.max(0.8)),
            col,
        );
    };
    match kind {
        Concept::Lift | Concept::Drag | Concept::Camber | Concept::Stall => {
            let angle = if kind == Concept::Stall { 18.0 } else { 4.0 };
            let code = if kind == Concept::Camber {
                "6412"
            } else {
                "2412"
            };
            let pts = foil_screen(&foil_points(code), c, 150.0 * k, angle);
            painter.add(egui::Shape::closed_line(pts, line));
            // akış çizgileri
            for n in 0..3 {
                let y = rect.top() + (14.0 + n as f32 * 8.0) * k;
                let col = blue;
                if kind == Concept::Stall && n == 0 {
                    // ayrılmış (girdaplı) akış
                    let mut zz = Vec::new();
                    for i in 0..12 {
                        let x = c.x + (-10.0 + i as f32 * 9.0) * k;
                        zz.push(egui::pos2(
                            x,
                            y + (12.0 + if i % 2 == 0 { 4.0 } else { -4.0 }) * k,
                        ));
                    }
                    painter.add(egui::Shape::line(zz, egui::Stroke::new(1.5, red)));
                } else {
                    painter.arrow(
                        egui::pos2(rect.left() + 6.0 * k, y),
                        v(60.0, 0.0),
                        egui::Stroke::new(1.2, col),
                    );
                }
            }
            match kind {
                Concept::Lift => {
                    painter.arrow(c, v(0.0, -45.0), egui::Stroke::new(2.5, green));
                    label(c + v(18.0, -40.0), "L", green);
                    painter.arrow(c, v(0.0, 40.0), egui::Stroke::new(2.0, weak));
                    label(c + v(18.0, 38.0), "W", weak);
                }
                Concept::Drag => {
                    painter.arrow(c, v(60.0, 0.0), egui::Stroke::new(2.5, red));
                    label(c + v(64.0, -10.0), "D", red);
                    painter.arrow(c, v(-60.0, 0.0), egui::Stroke::new(2.0, weak));
                    label(c + v(-64.0, -10.0), "T", weak);
                }
                Concept::Camber => {
                    // ortalama kamber çizgisi (kesikli)
                    let mean: Vec<[f64; 2]> = (0..=20)
                        .map(|i| {
                            let x = i as f64 / 20.0;
                            let m = 0.06;
                            let pp = 0.4;
                            let y = if x < pp {
                                m / (pp * pp) * (2.0 * pp * x - x * x)
                            } else {
                                m / ((1.0 - pp) * (1.0 - pp))
                                    * ((1.0 - 2.0 * pp) + 2.0 * pp * x - x * x)
                            };
                            [x, y]
                        })
                        .collect();
                    let mp = foil_screen(&mean, c, 150.0 * k, angle);
                    painter.extend(egui::Shape::dashed_line(
                        &mp,
                        egui::Stroke::new(1.5, amber),
                        4.0,
                        3.0,
                    ));
                    label(c + v(0.0, 36.0), t("aeros.learn.camber.short"), amber);
                }
                Concept::Stall => {
                    label(c + v(60.0, -34.0), "α↑", red);
                }
                _ => {}
            }
        }
        Concept::LiftDrag => {
            let a = egui::pos2(rect.left() + 20.0 * k, rect.top() + 14.0 * k);
            let b = egui::pos2(rect.left() + 200.0 * k, rect.bottom() - 16.0 * k);
            let corner = egui::pos2(b.x, a.y);
            painter.line_segment([a, b], egui::Stroke::new(2.0, blue));
            painter.line_segment([a, corner], line);
            painter.line_segment([corner, b], line);
            label(
                egui::pos2((a.x + corner.x) / 2.0, a.y - 6.0),
                "L/D × h",
                weak,
            );
            label(corner + egui::vec2(14.0 * k, (b.y - a.y) / 2.0), "h", fg);
        }
        Concept::Reynolds => {
            let small = foil_screen(&foil_points("2412"), c + v(-80.0, 0.0), 50.0 * k, 0.0);
            let large = foil_screen(&foil_points("2412"), c + v(40.0, 0.0), 130.0 * k, 0.0);
            painter.add(egui::Shape::closed_line(small, line));
            painter.add(egui::Shape::closed_line(large, line));
            label(c + v(-80.0, 32.0), "Re ↓", amber);
            label(c + v(40.0, 32.0), "Re ↑", green);
        }
    }
}

fn learn_corner(ui: &mut Ui) {
    ui.collapsing(t("aeros.learn.title"), |ui| {
        ui.label(RichText::new(t("aeros.learn.intro")).weak());
        let items: [(Concept, &str, &str, &str, Option<&'static str>); 6] = [
            (
                Concept::Lift,
                t("aeros.learn.lift.title"),
                t("aeros.learn.lift.body"),
                t("aeros.learn.lift.exp"),
                None,
            ),
            (
                Concept::Drag,
                t("aeros.learn.drag.title"),
                t("aeros.learn.drag.body"),
                t("aeros.learn.drag.exp"),
                Some("aeros_d_fuse"),
            ),
            (
                Concept::LiftDrag,
                t("aeros.learn.ld.title"),
                t("aeros.learn.ld.body"),
                t("aeros.learn.ld.exp"),
                Some("aeros_surface"),
            ),
            (
                Concept::Reynolds,
                t("aeros.learn.re.title"),
                t("aeros.learn.re.body"),
                t("aeros.learn.re.exp"),
                None,
            ),
            (
                Concept::Stall,
                t("aeros.learn.stall.title"),
                t("aeros.learn.stall.body"),
                t("aeros.learn.stall.exp"),
                None,
            ),
            (
                Concept::Camber,
                t("aeros.learn.camber.title"),
                t("aeros.learn.camber.body"),
                t("aeros.learn.camber.exp"),
                None,
            ),
        ];
        for (kind, title, body, exp, focus) in items {
            ui.collapsing(title, |ui| {
                draw_concept(ui, kind);
                ui.label(body);
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(t("aeros.learn.try")).strong());
                    ui.label(exp);
                    if let Some(f) = focus {
                        if ui.small_button(t("aeros.learn.go")).clicked() {
                            request_focus(ui, f);
                        }
                    }
                });
            });
        }
        ui.add_space(4.0);
        ui.label(RichText::new(t("aeros.learn.ai_hint")).weak().small());
    });
}

// ---------------------------------------------------------------------------------------------
// Kanat adımı: profil kartları ve kanat büyüklüğü
// ---------------------------------------------------------------------------------------------

struct Card {
    code: &'static str,
    tags: &'static [&'static str],
}

const CARDS: [Card; 6] = [
    Card {
        code: "2412",
        tags: &["aeros.tag.beginner"],
    },
    Card {
        code: "2415",
        tags: &["aeros.tag.thick_strong"],
    },
    Card {
        code: "4412",
        tags: &["aeros.tag.endurance"],
    },
    Card {
        code: "4415",
        tags: &["aeros.tag.endurance", "aeros.tag.thick_strong"],
    },
    Card {
        code: "0009",
        tags: &["aeros.tag.speed"],
    },
    Card {
        code: "0012",
        tags: &["aeros.tag.symmetric"],
    },
];

fn tag_text(k: &str) -> &'static str {
    match k {
        "aeros.tag.beginner" => t("aeros.tag.beginner"),
        "aeros.tag.endurance" => t("aeros.tag.endurance"),
        "aeros.tag.speed" => t("aeros.tag.speed"),
        "aeros.tag.symmetric" => t("aeros.tag.symmetric"),
        "aeros.tag.thick_strong" => t("aeros.tag.thick_strong"),
        _ => "",
    }
}

pub fn select_naca_for_wing(p: &mut Project, code: &str) {
    if let Ok(pts) =
        crate::cad::airfoil::generate_naca4_points(code, p.airfoil.num_points, p.airfoil.closed_te)
    {
        p.airfoil.is_naca = true;
        p.airfoil.naca_code = code.to_string();
        p.airfoil.name = format!("NACA {code}");
        p.airfoil.points = pts;
        p.tc_w = p.airfoil.max_thickness();
    }
}

pub fn select_naca_for_tail_h(p: &mut Project, code: &str) {
    if let Ok(pts) = crate::cad::airfoil::generate_naca4_points(
        code,
        p.tail_airfoil_h.num_points,
        p.tail_airfoil_h.closed_te,
    ) {
        p.tail_airfoil_h.is_naca = true;
        p.tail_airfoil_h.naca_code = code.to_string();
        p.tail_airfoil_h.name = format!("NACA {code}");
        p.tail_airfoil_h.points = pts;
        p.tc_t = p.tail_tc_from_profiles();
    }
}

pub fn select_naca_for_tail_v(p: &mut Project, code: &str) {
    if let Ok(pts) = crate::cad::airfoil::generate_naca4_points(
        code,
        p.tail_airfoil_v.num_points,
        p.tail_airfoil_v.closed_te,
    ) {
        p.tail_airfoil_v.is_naca = true;
        p.tail_airfoil_v.naca_code = code.to_string();
        p.tail_airfoil_v.name = format!("NACA {code}");
        p.tail_airfoil_v.points = pts;
        // Klasik kuyrukta dikey yüzeyin t/c'si de (alan ağırlıklı) otomatik doldurulur.
        if p.tail_kind != TailKind::FlyingWing {
            p.tc_t = p.tail_tc_from_profiles();
        }
    }
}

pub fn select_naca_for_tail(p: &mut Project, code: &str) {
    match p.tail_kind {
        TailKind::Conventional => {
            select_naca_for_tail_h(p, code);
            select_naca_for_tail_v(p, code);
        }
        TailKind::VTail | TailKind::FlyingWing => {
            select_naca_for_tail_v(p, code);
        }
    }
}

/// Resimli profil kartları (yalnızca analitik üretilebilen NACA 4 haneli örnekler + .dat kartı).
pub fn airfoil_cards(ui: &mut Ui, p: &mut Project, m: &Model) {
    let tk = theme::tokens(ui);
    step_ui::note(ui, t("aeros.cards.intro"));
    // Kart genişliği panele göre: geniş panelde iki, dar panelde tek sütun (taşma yok).
    let avail = ui.available_width();
    let cw = if avail >= 340.0 {
        ((avail - 8.0) / 2.0 - 18.0).max(120.0)
    } else {
        (avail - 18.0).max(120.0)
    };
    let re = if m.aero.re_wing.is_finite() && m.aero.re_wing > 0.0 {
        m.aero.re_wing
    } else {
        2.0e5
    };
    ui.horizontal_wrapped(|ui| {
        for card in &CARDS {
            let tc = card.code[2..4].parse::<f64>().unwrap_or(12.0) / 100.0;
            let camber = card.code[0..1].parse::<f64>().unwrap_or(0.0) / 100.0;
            let wing_selected = p.airfoil.is_naca && p.airfoil.naca_code == card.code;
            let tail_h_selected =
                p.tail_airfoil_h.is_naca && p.tail_airfoil_h.naca_code == card.code;
            let tail_v_selected =
                p.tail_airfoil_v.is_naca && p.tail_airfoil_v.naca_code == card.code;
            let tail_relevant_selected = match p.tail_kind {
                TailKind::Conventional => tail_h_selected || tail_v_selected,
                TailKind::VTail | TailKind::FlyingWing => tail_v_selected,
            };
            let stroke_col = if wing_selected {
                tk.success
            } else if tail_relevant_selected {
                tk.accent
            } else {
                ui.visuals().widgets.noninteractive.bg_stroke.color
            };
            egui::Frame::group(ui.style())
                .stroke(egui::Stroke::new(
                    if wing_selected || tail_relevant_selected {
                        2.0
                    } else {
                        1.0
                    },
                    stroke_col,
                ))
                .show(ui, |ui| {
                    ui.set_width(cw);
                    ui.vertical(|ui| {
                        ui.strong(format!("NACA {}", card.code));
                        let (resp, painter) = ui.allocate_painter(
                            egui::vec2(cw.min(145.0), 44.0),
                            egui::Sense::hover(),
                        );
                        let pts = foil_screen(
                            &foil_points(card.code),
                            resp.rect.center() - egui::vec2(32.5, 0.0),
                            130.0,
                            0.0,
                        );
                        painter.add(egui::Shape::closed_line(
                            pts,
                            egui::Stroke::new(1.5, tk.accent),
                        ));
                        for k in card.tags {
                            ui.label(RichText::new(tag_text(k)).small().color(tk.warning));
                        }
                        ui.label(
                            RichText::new(tf(
                                "aeros.cards.clmax_est",
                                &[fmt(cl_max_2d_estimate(tc, camber, re), 2)],
                            ))
                            .small()
                            .weak(),
                        );
                        ui.add_space(2.0);
                        let wing_lbl = if wing_selected {
                            t("geo.wing_selected")
                        } else {
                            t("geo.btn_apply_to_wing")
                        };
                        if ui
                            .add_enabled(!wing_selected, egui::Button::new(wing_lbl))
                            .clicked()
                        {
                            select_naca_for_wing(p, card.code);
                        }

                        match p.tail_kind {
                            TailKind::Conventional => {
                                let h_lbl = if tail_h_selected {
                                    t("geo.tail_h_selected")
                                } else {
                                    t("geo.btn_apply_to_tail_h")
                                };
                                if ui
                                    .add_enabled(!tail_h_selected, egui::Button::new(h_lbl))
                                    .clicked()
                                {
                                    select_naca_for_tail_h(p, card.code);
                                }
                                let v_lbl = if tail_v_selected {
                                    t("geo.tail_v_selected")
                                } else {
                                    t("geo.btn_apply_to_tail_v")
                                };
                                if ui
                                    .add_enabled(!tail_v_selected, egui::Button::new(v_lbl))
                                    .clicked()
                                {
                                    select_naca_for_tail_v(p, card.code);
                                }
                            }
                            TailKind::VTail => {
                                let vtail_lbl = if tail_v_selected {
                                    t("geo.tail_vtail_selected")
                                } else {
                                    t("geo.btn_apply_to_tail_vtail")
                                };
                                if ui
                                    .add_enabled(!tail_v_selected, egui::Button::new(vtail_lbl))
                                    .clicked()
                                {
                                    select_naca_for_tail_v(p, card.code);
                                }
                            }
                            TailKind::FlyingWing => {
                                let w_lbl = if tail_v_selected {
                                    t("geo.winglet_selected")
                                } else {
                                    t("geo.btn_apply_to_winglet")
                                };
                                if ui
                                    .add_enabled(!tail_v_selected, egui::Button::new(w_lbl))
                                    .clicked()
                                {
                                    select_naca_for_tail_v(p, card.code);
                                }
                            }
                        }
                    });
                });
        }
        // Reflex / uçan kanat: analitik üretim yok → kendi .dat dosyası
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(cw);
            ui.vertical(|ui| {
                ui.strong(t("aeros.cards.dat_title"));
                ui.label(
                    RichText::new(t("aeros.tag.reflex"))
                        .small()
                        .color(tk.warning),
                );
                step_ui::note(ui, t("aeros.cards.dat_desc"));
                ui.add_space(2.0);
                if ui.button(t("geo.btn_load_dat_wing")).clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Airfoil DAT (*.dat)", &["dat"])
                        .pick_file()
                    {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            if let Ok(prof) = crate::cad::airfoil::parse_selig_dat(&content) {
                                p.tc_w = prof.max_thickness();
                                p.airfoil = prof;
                            }
                        }
                    }
                }
                match p.tail_kind {
                    TailKind::Conventional => {
                        if ui.button(t("geo.btn_load_dat_tail_h")).clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("Airfoil DAT (*.dat)", &["dat"])
                                .pick_file()
                            {
                                if let Ok(content) = std::fs::read_to_string(&path) {
                                    if let Ok(prof) = crate::cad::airfoil::parse_selig_dat(&content)
                                    {
                                        p.tail_airfoil_h = prof;
                                        p.tc_t = p.tail_tc_from_profiles();
                                    }
                                }
                            }
                        }
                        if ui.button(t("geo.btn_load_dat_tail_v")).clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("Airfoil DAT (*.dat)", &["dat"])
                                .pick_file()
                            {
                                if let Ok(content) = std::fs::read_to_string(&path) {
                                    if let Ok(prof) = crate::cad::airfoil::parse_selig_dat(&content)
                                    {
                                        p.tail_airfoil_v = prof;
                                        p.tc_t = p.tail_tc_from_profiles();
                                    }
                                }
                            }
                        }
                    }
                    TailKind::VTail => {
                        if ui.button(t("geo.btn_load_dat_tail_vtail")).clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("Airfoil DAT (*.dat)", &["dat"])
                                .pick_file()
                            {
                                if let Ok(content) = std::fs::read_to_string(&path) {
                                    if let Ok(prof) = crate::cad::airfoil::parse_selig_dat(&content)
                                    {
                                        p.tc_t = prof.max_thickness();
                                        p.tail_airfoil_v = prof;
                                    }
                                }
                            }
                        }
                    }
                    TailKind::FlyingWing => {
                        if ui.button(t("geo.btn_load_dat_winglet")).clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("Airfoil DAT (*.dat)", &["dat"])
                                .pick_file()
                            {
                                if let Ok(content) = std::fs::read_to_string(&path) {
                                    if let Ok(prof) = crate::cad::airfoil::parse_selig_dat(&content)
                                    {
                                        p.tail_airfoil_v = prof;
                                    }
                                }
                            }
                        }
                    }
                }
            });
        });
    });
    step_ui::note(ui, t("aeros.cards.note"));
}

/// "Ne kadar büyük olsun?" sorusu: kanat alanı önerisi (uygulama kullanıcı onayıyla).
pub fn wing_size_panel(ui: &mut Ui, p: &mut Project, m: &Model) {
    section(ui, t("aeros.wing.section"));
    step_ui::field_label(ui, t("aeros.wing.q"), "");
    combo(
        ui,
        "aeros_size_pref",
        &mut p.aero_simple.size_pref,
        &[
            (SizePref::Floaty, t("aeros.size.floaty")),
            (SizePref::Normal, t("aeros.size.normal")),
            (SizePref::Fast, t("aeros.size.fast")),
        ],
    );
    let ws_t = p.aero_simple.size_pref.wing_loading();
    let s_sug = suggest_wing_area(m.weight.m_mtow, p.aero_simple.size_pref);
    if s_sug.is_finite() {
        let vs = if m.aero.cl_max > 0.0 && m.atmo.rho > 0.0 {
            (2.0 * ws_t / (m.atmo.rho * m.aero.cl_max)).sqrt()
        } else {
            f64::NAN
        };
        let vs_text = if vs.is_finite() {
            format!("{:.0}", vs * 3.6)
        } else {
            "–".to_string()
        };
        ui.horizontal_wrapped(|ui| {
            ui.label(tf(
                "aeros.wing.suggest",
                &[
                    fmt(s_sug * 100.0, 0),
                    fmt(p.s * 100.0, 0),
                    fmt(ws_t, 0),
                    vs_text,
                ],
            ));
            if ui.button(t("aeros.wing.apply")).clicked() {
                p.s = s_sug;
            }
        });
    }
    let _ = slider_row(
        ui,
        "aeros_wing_area",
        t("geo.input.s"),
        &mut p.s,
        100.0,
        "dm²",
        (5.0, 150.0),
        (20.0, 60.0),
        0,
    );
    step_ui::note(ui, t("aeros.wing.tail_note"));
}

// ---------------------------------------------------------------------------------------------
// Tasarım noktası (basit mod)
// ---------------------------------------------------------------------------------------------

/// Basit mod Tasarım noktası: hız sorusu ve el fırlatma hızı (sol panel).
pub fn design_point_inputs(ui: &mut Ui, p: &mut Project, m: &Model) {
    step_ui::note(ui, t("aeros.dp.intro"));
    section(ui, t("aeros.dp.section_q"));
    slider_row(
        ui,
        "aeros_v_stall_req",
        t("aeros.dp.q_speed"),
        &mut p.v_stall_req,
        3.6,
        "km/h",
        (18.0, 72.0),
        (30.0, 40.0),
        0,
    );
    step_ui::note(
        ui,
        &tf(
            "aeros.dp.speed_is",
            &[
                fmt(p.v_stall_req, 1),
                analogy_name(p.v_stall_req).to_string(),
            ],
        ),
    );
    step_ui::field_label(ui, t("aeros.dp.q_launch"), "");
    let want = suggest_launch_speed(p.v_stall_req);
    step_ui::text(
        ui,
        &tf(
            "aeros.dp.launch_need",
            &[format!("{:.0}", want * 3.6), fmt(want, 1)],
        ),
    );
    if ui.button(t("aeros.dp.apply_launch")).clicked() {
        p.v_launch = want;
    }
    sync_derived(ui, p, m);
}

/// Basit mod Tasarım noktası: anlık fark, günlük dilde sonuçlar, öğren köşesi (sağ panel).
pub fn design_point_results(ui: &mut Ui, p: &Project, m: &Model) {
    feedback(ui, m);
    section(ui, t("aeros.dp.section_results"));
    if m.inputs_ok {
        let c = &m.cons;
        result_row(
            ui,
            if c.stall_ok {
                Rating::Good
            } else {
                Rating::Bad
            },
            tf(
                "aeros.dp.res_stall",
                &[
                    fmt(c.v_stall, 1),
                    format!("{:.0}", c.v_stall * 3.6),
                    format!("{:.0}", p.v_stall_req * 3.6),
                ],
            ),
            Some(t("aeros.tip.stall")),
            None,
        );
        result_row(
            ui,
            if c.ws <= c.ws_max {
                Rating::Good
            } else {
                Rating::Bad
            },
            tf("aeros.dp.res_ws", &[fmt(c.ws, 0), fmt(c.ws_max, 0)]),
            Some(t("aeros.tip.ws")),
            None,
        );
        result_row(
            ui,
            if c.power_ok {
                Rating::Good
            } else {
                Rating::Bad
            },
            tf(
                "aeros.dp.res_power",
                &[fmt(c.p_elec, 0), fmt(c.installed_pw, 2)],
            ),
            Some(t("aeros.tip.power")),
            None,
        );
        result_row(
            ui,
            launch_rating(p.v_launch, c.v_stall),
            tf(
                "aeros.dp.res_launch",
                &[
                    format!("{:.0}", p.v_launch * 3.6),
                    format!("{:.0}", suggest_launch_speed(c.v_stall) * 3.6),
                ],
            ),
            Some(t("aeros.tip.launch")),
            None,
        );
    } else {
        ui.label("—");
    }
    ui.add_space(6.0);
    learn_corner(ui);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Library, evaluate};

    #[test]
    fn make_ctx_survives_zero_wetted_area() {
        let lib = Library::embedded();
        let mut p = Project::default();
        let m0 = evaluate(&p, &lib);
        let x0 = make_ctx(&p, &m0).expect("varsayilan baglam");
        p.s_wet_fuse = 0.0;
        let m = evaluate(&p, &lib);
        let x = make_ctx(&p, &m).expect("s_wet_fuse=0 iken de turetme kilitlenmemeli");
        assert!(
            (x.fuse_cd0_per_wet - x0.fuse_cd0_per_wet).abs() < 1e-12,
            "birim islak alan basina CD0 s_wet'ten bagimsiz"
        );
        let d = derive(
            &AeroSimple {
                derived_active: true,
                ..Default::default()
            },
            &x,
        );
        assert!(d.s_wet_fuse.is_finite() && d.s_wet_fuse > 0.0);
    }

    #[test]
    fn first_answer_keeps_custom_values() {
        let mut p = Project::default();
        p.cl_max_2d = 1.55; // elle girilmis
        activate_derived(&mut p);
        assert!(p.aero_simple.derived_active);
        assert!(p.aero_simple.ov_cl_max);
        assert!(!p.aero_simple.ov_cd_misc && !p.aero_simple.ov_s_wet);
        // Varsayilan projede hicbir sey ozellestirilmis sayilmaz
        let mut q = Project::default();
        activate_derived(&mut q);
        assert!(!q.aero_simple.any_override());
    }

    #[test]
    fn launch_rating_can_be_bad() {
        assert_eq!(launch_rating(3.0, 12.0), Rating::Bad);
        assert_eq!(launch_rating(12.0, 12.0), Rating::Ok);
        assert_eq!(launch_rating(14.0, 12.0), Rating::Good);
        assert_eq!(launch_rating(f64::NAN, 12.0), Rating::Bad);
    }

    #[test]
    fn cl_max_reynolds_uses_stall_speed() {
        let lib = Library::embedded();
        let p = Project::default();
        let m = evaluate(&p, &lib);
        let re = re_for_cl_max(&p, &m);
        assert!(re < m.aero.re_wing && re > 0.0);
        assert!((re / m.aero.re_wing - p.v_stall_req / p.v_cr).abs() < 1e-9);
    }

    #[test]
    fn test_select_naca_for_tail_separation() {
        let mut p = Project::default();
        p.tail_kind = TailKind::Conventional;

        // Yatay kuyruğa NACA 0012 seç
        select_naca_for_tail_h(&mut p, "0012");
        assert_eq!(p.tail_airfoil_h.naca_code, "0012");
        assert_eq!(p.tail_airfoil_h.name, "NACA 0012");
        // Dikey kuyruk varsayılan 0009'da kalmalı, etkilenmemeli
        assert_eq!(p.tail_airfoil_v.naca_code, "0009");
        // Klasik kuyruk: t/c alan ağırlıklı (yatay 0.12, dikey 0.09)
        let w = |th: f64, tv: f64| (p.s_h * th + p.s_v * tv) / (p.s_h + p.s_v);
        assert!((p.tc_t - w(0.12, 0.09)).abs() < 1e-4);

        // Dikey kuyruğa NACA 0008 seç
        select_naca_for_tail_v(&mut p, "0008");
        assert_eq!(p.tail_airfoil_v.naca_code, "0008");
        assert_eq!(p.tail_airfoil_v.name, "NACA 0008");
        // Yatay kuyruk 0012'de kalmalı
        assert_eq!(p.tail_airfoil_h.naca_code, "0012");
        // Konvansiyonel kuyrukta dikey yüzeyin t/c'si de otomatik dolar (alan ağırlıklı)
        let w = |th: f64, tv: f64| (p.s_h * th + p.s_v * tv) / (p.s_h + p.s_v);
        assert!((p.tc_t - w(0.12, 0.08)).abs() < 1e-4);

        // V-kuyrukta select_naca_for_tail_v çağrıldığında tc_t güncellenmeli
        p.tail_kind = TailKind::VTail;
        select_naca_for_tail_v(&mut p, "0010");
        assert_eq!(p.tail_airfoil_v.naca_code, "0010");
        assert!((p.tc_t - 0.10).abs() < 1e-4);
    }
}
