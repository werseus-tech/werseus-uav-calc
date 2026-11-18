// SPDX-License-Identifier: AGPL-3.0-or-later
//! RF / EMI sekmesi: bağ bütçesi tablosu, anten yerleşimi şeması ve düz anlatımlı uyarılar.
//! Durum yalnızca egui belleğinde tutulur; proje dosyası değişmez.
use super::components::{self, Status, section_header};
use super::theme::{self, space};
use super::widgets::fmt;
use crate::cad::layout::Layout;
use crate::calc::Model;
use crate::calc::mr_structure::MrArmMaterialPreset;
use crate::i18n::{Lang, current_lang, t, t_lang, tf};
use crate::project::{DroneType, Project};
use crate::rfemi::emi::{
    self, AntennaLoc, EmiConfig, Scene, Severity, SourceKind, analyze, plan_bounds,
};
use crate::rfemi::link::{Band, Link, LinkKind, Pol, compute};
use egui::{Color32, Pos2, Ui, Vec2};

#[derive(Clone)]
struct RfState {
    links: Vec<Link>,
    cfg: EmiConfig,
    add_kind: LinkKind,
    inited: bool,
    last_drone: Option<DroneType>,
    last_carbon: Option<bool>,
    last_lang: Option<Lang>,
}

impl Default for RfState {
    fn default() -> Self {
        RfState {
            links: Vec::new(),
            cfg: EmiConfig::default(),
            add_kind: LinkKind::Rc,
            inited: false,
            last_drone: None,
            last_carbon: None,
            last_lang: None,
        }
    }
}

fn default_carbon(p: &Project) -> bool {
    let mat = p.material_id.to_lowercase();
    let body_cf = mat.contains("carbon") || mat.contains("cf");
    if p.drone_type == DroneType::Multirotor {
        matches!(
            p.mr_arm_material,
            MrArmMaterialPreset::PultrudedCf
                | MrArmMaterialPreset::RollWrappedCf
                | MrArmMaterialPreset::CfPetg
        ) || body_cf
    } else {
        body_cf
    }
}

fn new_link(kind: LinkKind) -> Link {
    let mut l = Link::typical(kind);
    l.name = t(kind.key()).to_string();
    l
}

fn pol_key(p: Pol) -> &'static str {
    match p {
        Pol::Linear => "rfemi.pol.linear",
        Pol::Rhcp => "rfemi.pol.rhcp",
        Pol::Lhcp => "rfemi.pol.lhcp",
    }
}

fn status_of(s: Severity) -> Status {
    match s {
        Severity::Info => Status::Info,
        Severity::Warn => Status::Warn,
        Severity::Danger => Status::Err,
    }
}

fn drag(ui: &mut Ui, v: &mut f64, lo: f64, hi: f64, speed: f64, suffix: &str) -> bool {
    ui.add(
        egui::DragValue::new(v)
            .range(lo..=hi)
            .speed(speed)
            .suffix(suffix),
    )
    .changed()
}

fn pol_combo(ui: &mut Ui, salt: (usize, &str), v: &mut Pol) {
    egui::ComboBox::from_id_salt(salt)
        .selected_text(t(pol_key(*v)))
        .show_ui(ui, |ui| {
            for p in [Pol::Linear, Pol::Rhcp, Pol::Lhcp] {
                ui.selectable_value(v, p, t(pol_key(p)));
            }
        });
}

fn loc_combo(ui: &mut Ui, salt: (usize, &str), v: &mut AntennaLoc) {
    egui::ComboBox::from_id_salt(salt)
        .selected_text(t(v.key()))
        .show_ui(ui, |ui| {
            for l in AntennaLoc::ALL {
                ui.selectable_value(v, l, t(l.key()));
            }
        });
}

pub fn show(ui: &mut Ui, p: &Project, m: &Model) {
    let id = egui::Id::unique("rfemi_state");
    let mut st: RfState = ui.data_mut(|d| d.get_temp(id)).unwrap_or_default();
    let lang = current_lang();
    let carbon = default_carbon(p);
    if !st.inited {
        st.inited = true;
        st.links = LinkKind::ALL.iter().map(|k| new_link(*k)).collect();
        st.cfg.carbon_body = carbon;
        st.last_drone = Some(p.drone_type);
        st.last_carbon = Some(carbon);
        st.last_lang = Some(lang);
    } else {
        if st.last_drone != Some(p.drone_type) || st.last_carbon != Some(carbon) {
            st.cfg.carbon_body = carbon;
            st.last_drone = Some(p.drone_type);
            st.last_carbon = Some(carbon);
        }
        if let Some(prev) = st.last_lang
            && prev != lang
        {
            for l in &mut st.links {
                if l.name == t_lang(l.kind.key(), prev) {
                    l.name = t_lang(l.kind.key(), lang).to_string();
                }
            }
            st.last_lang = Some(lang);
        }
    }
    body(ui, p, m, &mut st);
    ui.data_mut(|d| d.insert_temp(id, st));
}

fn body(ui: &mut Ui, p: &Project, m: &Model, st: &mut RfState) {
    section_header(ui, t("tab_rfemi"));
    components::banner(ui, Status::Info, t("rfemi.disclaimer"));
    ui.add_space(space::S);

    let lay = Layout::new(p, m);
    let scene = Scene::new(p, &lay);

    // --- Bağ bütçesi sonuç tablosu
    section_header(ui, t("rfemi.table"));
    let tk = theme::tokens(ui);
    egui::ScrollArea::horizontal()
        .id_salt("rfemi_table")
        .show(ui, |ui| {
            egui::Grid::new("rfemi_grid")
                .striped(true)
                .num_columns(10)
                .show(ui, |ui| {
                    for h in [
                        "rfemi.col.link",
                        "rfemi.col.band",
                        "rfemi.col.eirp",
                        "rfemi.col.fspl",
                        "rfemi.col.rx",
                        "rfemi.col.margin",
                        "rfemi.col.range",
                        "rfemi.col.fresnel",
                        "rfemi.col.minh",
                        "rfemi.col.loc",
                    ] {
                        ui.label(egui::RichText::new(t_dyn(h)).strong());
                    }
                    ui.end_row();
                    for l in &st.links {
                        let r = compute(l);
                        ui.label(&l.name);
                        ui.label(format!("{} MHz", fmt(l.f_mhz, 1)));
                        ui.label(fmt(r.eirp_dbm, 1));
                        ui.label(fmt(r.fspl_db, 1));
                        ui.label(fmt(r.rx_dbm, 1));
                        let c = if r.margin_db >= 0.0 {
                            tk.success
                        } else {
                            tk.danger
                        };
                        ui.label(egui::RichText::new(fmt(r.margin_db, 1)).color(c));
                        match r.max_range_km {
                            Some(x) => ui.label(fmt(x, 1)),
                            None => ui.label("—"),
                        };
                        ui.label(fmt(r.fresnel_mid_m, 1));
                        ui.label(fmt(r.min_height_m, 1));
                        ui.label(t(l.loc.key()));
                        ui.end_row();
                    }
                });
        });
    ui.label(egui::RichText::new(t("rfemi.table_note")).color(tk.text_weak));

    // --- Bağ ayrıntıları
    section_header(ui, t("rfemi.links"));
    let mut remove: Option<usize> = None;
    for i in 0..st.links.len() {
        let name = st.links[i].name.clone();
        egui::CollapsingHeader::new(name)
            .id_salt(("rfemi_link", i))
            .default_open(false)
            .show(ui, |ui| {
                link_editor(ui, i, &mut st.links[i], &mut remove);
            });
    }
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("rfemi_add_kind")
            .selected_text(t(st.add_kind.key()))
            .show_ui(ui, |ui| {
                for k in LinkKind::ALL {
                    ui.selectable_value(&mut st.add_kind, k, t(k.key()));
                }
            });
        if ui.button(t("rfemi.add_link")).clicked() {
            st.links.push(new_link(st.add_kind));
        }
    });
    if let Some(i) = remove {
        st.links.remove(i);
    }

    // --- EMI ayarları
    section_header(ui, t("rfemi.emi"));
    ui.checkbox(&mut st.cfg.carbon_body, t("rfemi.carbon"));
    ui.horizontal(|ui| {
        ui.label(t("rfemi.vtx_loc"));
        loc_combo(ui, (0, "rfemi_vtx_loc"), &mut st.cfg.vtx_loc);
    });
    egui::CollapsingHeader::new(t("rfemi.thresholds"))
        .id_salt("rfemi_thr")
        .show(ui, |ui| {
            thresholds_grid(ui, &mut st.cfg);
        });

    let rep = analyze(&st.links, &scene, &st.cfg);

    // --- Üstten görünüş şeması
    section_header(ui, t("rfemi.plan"));
    plan(ui, &scene, st, &rep);

    // --- Mesafeler
    egui::CollapsingHeader::new(t("rfemi.distances"))
        .id_salt("rfemi_dist")
        .show(ui, |ui| {
            distances_grid(ui, &st.links, &rep, tk);
        });

    // --- Uyarılar
    section_header(ui, t("rfemi.warnings"));
    if rep.warnings.is_empty() {
        components::banner(ui, Status::Ok, t("rfemi.no_warnings"));
    }
    for w in &rep.warnings {
        components::banner(ui, status_of(w.severity), &tf(w.key, &w.args));
        ui.add_space(space::XS);
    }
}

/// Çeviri anahtarı yalnızca bu dosyadaki sabit listelerden gelir.
fn t_dyn(key: &'static str) -> &'static str {
    t(key)
}

fn thresholds_grid(ui: &mut Ui, cfg: &mut EmiConfig) {
    egui::Grid::new("rfemi_thr_grid").show(ui, |ui| {
        for (k, v) in [
            ("rfemi.thr.gnss_vtx", &mut cfg.gnss_vtx_cm),
            ("rfemi.thr.gnss_esc", &mut cfg.gnss_esc_cm),
            ("rfemi.thr.gnss_motor", &mut cfg.gnss_motor_cm),
            ("rfemi.thr.gnss_power", &mut cfg.gnss_power_cm),
            ("rfemi.thr.other_vtx", &mut cfg.other_vtx_cm),
            ("rfemi.thr.other_esc", &mut cfg.other_esc_cm),
        ] {
            ui.label(t_dyn(k));
            drag(ui, v, 0.0, 100.0, 0.5, " cm");
            ui.end_row();
        }
    });
    if ui.button(t("rfemi.thr.reset")).clicked() {
        let keep = (cfg.vtx_loc, cfg.carbon_body);
        *cfg = EmiConfig {
            vtx_loc: keep.0,
            carbon_body: keep.1,
            ..EmiConfig::default()
        };
    }
}

fn distances_grid(ui: &mut Ui, links: &[Link], rep: &emi::Report, tk: &theme::Tokens) {
    egui::Grid::new("rfemi_dist_grid")
        .striped(true)
        .show(ui, |ui| {
            for h in [
                "rfemi.col.antenna",
                "rfemi.col.source",
                "rfemi.col.dist",
                "rfemi.col.limit",
            ] {
                ui.label(egui::RichText::new(t_dyn(h)).strong());
            }
            ui.end_row();
            for r in &rep.rows {
                ui.label(&links[r.link].name);
                ui.label(r.source.label());
                let c = if r.violated() { tk.danger } else { tk.text };
                ui.label(egui::RichText::new(format!("{} cm", fmt(r.cm, 1))).color(c));
                ui.label(format!("{} cm", fmt(r.threshold_cm.unwrap_or(0.0), 1)));
                ui.end_row();
            }
        });
}

fn link_editor(ui: &mut Ui, i: usize, l: &mut Link, remove: &mut Option<usize>) {
    egui::Grid::new(("rfemi_edit", i))
        .num_columns(2)
        .show(ui, |ui| {
            ui.label(t("rfemi.f.name"));
            ui.text_edit_singleline(&mut l.name);
            ui.end_row();
            ui.label(t("rfemi.f.band"));
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt((i, "band"))
                    .selected_text(l.band.info().label)
                    .show_ui(ui, |ui| {
                        for b in Band::ALL {
                            if ui.selectable_label(l.band == b, b.info().label).clicked() {
                                if b == Band::Custom {
                                    l.band = b;
                                } else {
                                    l.apply_band(b);
                                }
                            }
                        }
                    });
                ui.label(egui::RichText::new(t("rfemi.typical")).small().weak());
            });
            ui.end_row();
            ui.label(t("rfemi.f.freq"));
            if drag(ui, &mut l.f_mhz, 1.0, 12000.0, 1.0, " MHz") {
                l.band = Band::Custom;
            }
            ui.end_row();
            ui.label(t("rfemi.f.bw"));
            if drag(ui, &mut l.bw_mhz, 0.1, 500.0, 0.5, " MHz") {
                l.band = Band::Custom;
            }
            ui.end_row();
            if l.fixed_rx_dbm.is_none() {
                ui.label(t("rfemi.f.tx_power"));
                drag(ui, &mut l.tx_dbm, -10.0, 40.0, 0.5, " dBm");
                ui.end_row();
            } else if let Some(x) = l.fixed_rx_dbm.as_mut() {
                ui.label(t("rfemi.f.rx_level"));
                drag(ui, x, -170.0, -60.0, 0.5, " dBm");
                ui.end_row();
            }
            ui.label(t("rfemi.f.tx_gain"));
            drag(ui, &mut l.tx_gain_dbi, -10.0, 30.0, 0.1, " dBi");
            ui.end_row();
            ui.label(t("rfemi.f.rx_gain"));
            drag(ui, &mut l.rx_gain_dbi, -10.0, 30.0, 0.1, " dBi");
            ui.end_row();
            ui.label(t("rfemi.f.loss"));
            drag(ui, &mut l.loss_db, 0.0, 10.0, 0.1, " dB");
            ui.end_row();
            ui.label(t("rfemi.f.sens"));
            drag(ui, &mut l.sens_dbm, -160.0, -40.0, 0.5, " dBm");
            ui.end_row();
            ui.label(t("rfemi.f.fade"));
            drag(ui, &mut l.fade_margin_db, 0.0, 40.0, 0.5, " dB");
            ui.end_row();
            if l.fixed_rx_dbm.is_none() {
                ui.label(t("rfemi.f.dist"));
                drag(ui, &mut l.dist_km, 0.01, 500.0, 0.1, " km");
                ui.end_row();
                ui.label(t("rfemi.f.h_ground"));
                drag(ui, &mut l.h_ground_m, 0.0, 100.0, 0.1, " m");
                ui.end_row();
                ui.label(t("rfemi.f.h_air"));
                drag(ui, &mut l.h_air_m, 0.0, 5000.0, 1.0, " m");
                ui.end_row();
            }
            ui.label(t("rfemi.f.loc"));
            loc_combo(ui, (i, "loc"), &mut l.loc);
            ui.end_row();
            ui.label(t("rfemi.f.pol_air"));
            pol_combo(ui, (i, "pol_air"), &mut l.pol_air);
            ui.end_row();
            ui.label(t("rfemi.f.pol_ground"));
            pol_combo(ui, (i, "pol_ground"), &mut l.pol_ground);
            ui.end_row();
            if l.pol_air == Pol::Linear && l.pol_ground == Pol::Linear {
                ui.label(t("rfemi.f.pol_angle"));
                drag(ui, &mut l.pol_angle_deg, 0.0, 90.0, 0.5, "°");
                ui.end_row();
            }
        });
    let r = compute(l);
    if l.fixed_rx_dbm.is_none() {
        ui.label(tf(
            "rfemi.two_ray",
            &[fmt(r.two_ray_breakpoint_m, 0), fmt(r.two_ray_extra_db, 1)],
        ));
    }
    if ui.button(t("rfemi.remove")).clicked() {
        *remove = Some(i);
    }
}

fn plan(ui: &mut Ui, scene: &Scene, st: &RfState, rep: &emi::Report) {
    let tk = theme::tokens(ui);
    let width = ui.available_width().max(300.0);
    let height = 260.0;
    let (resp, painter) = ui.allocate_painter(Vec2::new(width, height), egui::Sense::hover());
    let rect = resp.rect;
    painter.rect_filled(rect, theme::corner(theme::radius::M), tk.viewport_bg);
    painter.rect_stroke(
        rect,
        theme::corner(theme::radius::M),
        egui::Stroke::new(1.0, tk.border),
        egui::StrokeKind::Inside,
    );
    let has_video = st.links.iter().any(|l| l.kind == LinkKind::Video);
    let extra = if has_video {
        vec![scene.point(st.cfg.vtx_loc)]
    } else {
        Vec::new()
    };
    let (x0, x1, y0, y1) = plan_bounds(scene, &extra);
    let (dx, dy) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
    let margin = 28.0f32;
    let scale = ((rect.width() - 2.0 * margin) as f64 / dx)
        .min((rect.height() - 2.0 * margin) as f64 / dy)
        .max(1e-6);
    let origin = rect.center();
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    // Burun solda, sol kanat yukarıda.
    let to = |q: emi::P3| -> Pos2 {
        Pos2::new(
            origin.x + ((q[0] - cx) * scale) as f32,
            origin.y - ((q[1] - cy) * scale) as f32,
        )
    };
    let font = egui::FontId::proportional(11.0);
    let stroke = egui::Stroke::new(1.5, tk.border_strong);

    // Gövde / plaka.
    if scene.multirotor {
        for s in scene.sources.iter().filter(|s| s.kind == SourceKind::Motor) {
            painter.line_segment([to([0.0, 0.0, 0.0]), to(s.a)], stroke);
        }
        let r = (scene.radius_mm * scale) as f32;
        painter.circle_filled(to([0.0, 0.0, 0.0]), r.max(6.0), tk.surface_alt);
    } else {
        let a = to([0.0, scene.radius_mm, 0.0]);
        let b = to([scene.length_mm, -scene.radius_mm, 0.0]);
        let body = egui::Rect::from_two_pos(a, b);
        painter.rect_filled(body, theme::corner(theme::radius::S), tk.surface_alt);
        painter.rect_stroke(
            body,
            theme::corner(theme::radius::S),
            stroke,
            egui::StrokeKind::Inside,
        );
        let wl = scene.point(AntennaLoc::WingtipL);
        let wr = scene.point(AntennaLoc::WingtipR);
        painter.line_segment([to(wl), to(wr)], egui::Stroke::new(3.0, tk.border_strong));
    }

    // Gürültü kaynakları.
    let src_color = |k: SourceKind| -> Color32 {
        match k {
            SourceKind::Motor => tk.warning,
            SourceKind::Esc => tk.danger,
            SourceKind::Battery => tk.info,
            SourceKind::PowerLine => tk.danger,
            SourceKind::Vtx => tk.accent,
        }
    };
    let all = scene.sources_for(&st.links, st.cfg.vtx_loc);
    for s in &all {
        let c = src_color(s.kind);
        match (s.kind, s.b) {
            (SourceKind::PowerLine, Some(b)) => {
                painter.line_segment([to(s.a), to(b)], egui::Stroke::new(1.0, c));
            }
            (SourceKind::Motor, _) => {
                painter.circle_stroke(to(s.a), 5.0, egui::Stroke::new(2.0, c));
            }
            (SourceKind::Vtx, _) => {
                let q = to(s.a);
                painter.rect_filled(
                    egui::Rect::from_center_size(q, Vec2::splat(9.0)),
                    theme::corner(2),
                    c,
                );
                painter.text(
                    q + Vec2::new(8.0, 10.0),
                    egui::Align2::LEFT_CENTER,
                    t(s.kind.key()),
                    font.clone(),
                    c,
                );
            }
            _ => {
                painter.rect_filled(
                    egui::Rect::from_center_size(to(s.a), Vec2::splat(7.0)),
                    theme::corner(1),
                    c,
                );
            }
        }
    }

    // İhlal eden mesafeler: anten ile kaynak arası çizgi.
    for r in rep.rows.iter().filter(|r| r.violated()) {
        let ant = scene.point(st.links[r.link].loc);
        let target = match r.source.b {
            Some(b) => {
                // Doğru parçası üzerindeki en yakın nokta.
                let (a, ab) = (
                    r.source.a,
                    [
                        b[0] - r.source.a[0],
                        b[1] - r.source.a[1],
                        b[2] - r.source.a[2],
                    ],
                );
                let l2 = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];
                let s = if l2 < 1e-9 {
                    0.0
                } else {
                    (((ant[0] - a[0]) * ab[0] + (ant[1] - a[1]) * ab[1] + (ant[2] - a[2]) * ab[2])
                        / l2)
                        .clamp(0.0, 1.0)
                };
                [a[0] + ab[0] * s, a[1] + ab[1] * s, a[2] + ab[2] * s]
            }
            None => r.source.a,
        };
        painter.line_segment([to(ant), to(target)], egui::Stroke::new(1.5, tk.danger));
    }

    // Antenler.
    for (i, l) in st.links.iter().enumerate() {
        let q = to(scene.point(l.loc)) + Vec2::new(0.0, (i as f32 % 3.0 - 1.0) * 4.0);
        painter.circle_filled(q, 5.0, tk.success);
        painter.circle_stroke(q, 5.0, egui::Stroke::new(1.0, tk.text));
        painter.text(
            q + Vec2::new(8.0, -9.0 - 2.0 * (i as f32 % 2.0)),
            egui::Align2::LEFT_CENTER,
            &l.name,
            font.clone(),
            tk.text,
        );
    }
    painter.text(
        rect.left_bottom() + Vec2::new(8.0, -10.0),
        egui::Align2::LEFT_CENTER,
        t("rfemi.plan_hint"),
        font,
        tk.text_weak,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::Library;
    use crate::project::DroneType;
    use crate::ui::theme::ThemeChoice;

    #[test]
    fn renders_in_both_themes_and_languages_for_every_type() {
        for dt in DroneType::ALL {
            let mut p = Project::default();
            p.set_drone_type(dt);
            let m = crate::calc::evaluate(&p, &Library::embedded());
            let lay = Layout::new(&p, &m);
            let scene = Scene::new(&p, &lay);
            for choice in [ThemeChoice::Dark, ThemeChoice::Light] {
                for lang in [crate::i18n::Lang::Tr, crate::i18n::Lang::En] {
                    let ctx = egui::Context::default();
                    theme::install(&ctx, choice);
                    crate::i18n::with_lang(lang, || {
                        for _ in 0..2 {
                            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                                egui::CentralPanel::default().show(ui, |ui| {
                                    show(ui, &p, &m);
                                    let mut cfg = EmiConfig::default();
                                    let mut links: Vec<Link> =
                                        LinkKind::ALL.iter().map(|k| new_link(*k)).collect();
                                    let rep = analyze(&links, &scene, &cfg);
                                    let tk = theme::tokens(ui);
                                    thresholds_grid(ui, &mut cfg);
                                    distances_grid(ui, &links, &rep, tk);
                                    let mut remove = None;
                                    for (i, l) in links.iter_mut().enumerate() {
                                        link_editor(ui, i, l, &mut remove);
                                    }
                                });
                            });
                            out.textures_delta.clear();
                        }
                    });
                }
            }
        }
    }

    #[test]
    fn default_carbon_checks_multirotor_arm_material_and_refreshes_lang() {
        let mut p = Project::default();
        p.set_drone_type(DroneType::Multirotor);
        p.mr_arm_material = MrArmMaterialPreset::PultrudedCf;
        assert!(default_carbon(&p));
        p.mr_arm_material = MrArmMaterialPreset::Al6061;
        assert!(!default_carbon(&p));

        let m = crate::calc::evaluate(&p, &Library::embedded());
        let ctx = egui::Context::default();
        theme::install(&ctx, ThemeChoice::Dark);
        let id = egui::Id::unique("rfemi_state");

        crate::i18n::with_lang(Lang::En, || {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| show(ui, &p, &m));
            });
            out.textures_delta.clear();
        });
        let st_en: RfState = ctx.data(|d| d.get_temp(id)).unwrap();
        assert_eq!(st_en.links[0].name, t_lang(LinkKind::Rc.key(), Lang::En));

        crate::i18n::with_lang(Lang::Tr, || {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| show(ui, &p, &m));
            });
            out.textures_delta.clear();
        });
        let st_tr: RfState = ctx.data(|d| d.get_temp(id)).unwrap();
        assert_eq!(st_tr.links[0].name, t_lang(LinkKind::Rc.key(), Lang::Tr));
    }
}
