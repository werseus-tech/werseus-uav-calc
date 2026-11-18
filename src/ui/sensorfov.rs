// SPDX-License-Identifier: AGPL-3.0-or-later
//! Sensör görüş açısı ve titreşim sönümleme sekmesi (modül 9). Girdiler egui belleğinde tutulur;
//! proje dosyası değişmez. Hesaplar `crate::sensorfov` içindedir.
use super::components::{self, Status, section_header};
use super::theme::{self, Tokens, radius, space};
use super::widgets::{fmt, section};
use crate::calc::Model;
use crate::i18n::{t, tf};
use crate::project::Project;
use crate::sensorfov::damping::{
    self, DAMPER_PRESETS, DamperInput, DampingReport, Excitation, Source,
};
use crate::sensorfov::fov::{
    self, Airframe, Camera, ItemKind, Mount, ObstructionReport, SENSOR_PRESETS, Solid, V3,
};
use egui::{FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use egui_plot::{Legend, Line, Plot, PlotPoints, VLine};

#[derive(Clone, Debug)]
struct SfState {
    /// SENSOR_PRESETS indeksi; len() = özel.
    preset: usize,
    sensor_w: f64,
    sensor_h: f64,
    use_fov: bool,
    focal_mm: f64,
    hfov_deg: f64,
    px_w: f64,
    alt_m: f64,
    mount: usize,
    tilt_deg: f64,
    gimbal_drop_mm: f64,
    // Sönümleme
    mass_kg: f64,
    n_dampers: u32,
    k_per: f64,
    damper_preset: usize,
    zeta: f64,
    max_sag_mm: f64,
    hover_rpm: f64,
    cruise_rpm: f64,
}

impl SfState {
    fn new(p: &Project) -> Self {
        Self {
            preset: 0,
            sensor_w: 6.17,
            sensor_h: 4.55,
            use_fov: false,
            focal_mm: 4.5,
            hfov_deg: 69.0,
            px_w: 4000.0,
            alt_m: 100.0,
            mount: 1,
            tilt_deg: 90.0,
            gimbal_drop_mm: 60.0,
            mass_kg: if p.m_payload.is_finite() && p.m_payload > 0.0 {
                p.m_payload.clamp(0.05, 5.0)
            } else {
                0.3
            },
            n_dampers: 4,
            k_per: DAMPER_PRESETS[1].1,
            damper_preset: 1,
            zeta: 0.1,
            max_sag_mm: 5.0,
            hover_rpm: 0.0,
            cruise_rpm: 0.0,
        }
    }
}

fn mount_of(i: usize) -> Mount {
    match i {
        0 => Mount::Nose,
        2 => Mount::Gimbal,
        _ => Mount::Belly,
    }
}

fn kind_label(k: ItemKind) -> &'static str {
    match k {
        ItemKind::Prop => t("sensorfov.kind_prop"),
        ItemKind::Arm => t("sensorfov.kind_arm"),
        ItemKind::Gear => t("sensorfov.kind_gear"),
        ItemKind::Fuselage => t("sensorfov.kind_fuselage"),
    }
}

fn damper_name(i: usize) -> &'static str {
    match i {
        0 => t("sensorfov.damper.soft"),
        1 => t("sensorfov.damper.medium"),
        2 => t("sensorfov.damper.firm"),
        _ => t("sensorfov.damper.grommet"),
    }
}

fn row(ui: &mut Ui, label: &str, value: String) {
    ui.label(label);
    ui.label(value);
    ui.end_row();
}

pub fn show(ui: &mut Ui, p: &Project, m: &Model) {
    let id = egui::Id::NULL.with("sensorfov_state");
    let mut st: SfState = ui
        .data_mut(|d| d.get_temp::<SfState>(id))
        .unwrap_or_else(|| SfState::new(p));
    show_inner(ui, p, m, &mut st);
    ui.data_mut(|d| d.insert_temp(id, st));
}

fn show_inner(ui: &mut Ui, p: &Project, m: &Model, st: &mut SfState) {
    section_header(ui, t("tab_sensorfov"));
    components::banner(ui, Status::Info, t("sensorfov.disclaimer"));
    ui.add_space(space::S);

    // ---------------- Kamera girdileri ----------------
    section(ui, t("sensorfov.cam_title"));
    ui.horizontal_wrapped(|ui| {
        ui.label(t("sensorfov.sensor"));
        let sel = SENSOR_PRESETS
            .get(st.preset)
            .map(|s| s.name)
            .unwrap_or(t("sensorfov.custom"));
        egui::ComboBox::from_id_salt("sfov_sensor")
            .selected_text(sel)
            .show_ui(ui, |ui| {
                for (i, s) in SENSOR_PRESETS.iter().enumerate() {
                    if ui.selectable_value(&mut st.preset, i, s.name).clicked() {
                        st.sensor_w = s.w_mm;
                        st.sensor_h = s.h_mm;
                    }
                }
                ui.selectable_value(&mut st.preset, SENSOR_PRESETS.len(), t("sensorfov.custom"));
            });
        ui.label(t("sensorfov.sensor_w"));
        let a = ui
            .add(
                egui::DragValue::new(&mut st.sensor_w)
                    .range(1.0..=60.0)
                    .speed(0.05)
                    .suffix(" mm"),
            )
            .changed();
        ui.label(t("sensorfov.sensor_h"));
        let b = ui
            .add(
                egui::DragValue::new(&mut st.sensor_h)
                    .range(1.0..=60.0)
                    .speed(0.05)
                    .suffix(" mm"),
            )
            .changed();
        if a || b {
            st.preset = SENSOR_PRESETS.len();
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(&mut st.use_fov, false, t("sensorfov.mode_focal"));
        ui.selectable_value(&mut st.use_fov, true, t("sensorfov.mode_fov"));
        if st.use_fov {
            ui.label(t("sensorfov.hfov_in"));
            ui.add(
                egui::DragValue::new(&mut st.hfov_deg)
                    .range(5.0..=170.0)
                    .speed(0.2)
                    .suffix("°"),
            );
        } else {
            ui.label(t("sensorfov.focal"));
            ui.add(
                egui::DragValue::new(&mut st.focal_mm)
                    .range(1.0..=600.0)
                    .speed(0.05)
                    .suffix(" mm"),
            );
        }
        ui.label(t("sensorfov.px_w"));
        ui.add(
            egui::DragValue::new(&mut st.px_w)
                .range(320.0..=20000.0)
                .speed(10.0)
                .suffix(" px"),
        );
        ui.label(t("sensorfov.altitude"));
        ui.add(
            egui::DragValue::new(&mut st.alt_m)
                .range(1.0..=5000.0)
                .speed(1.0)
                .suffix(" m"),
        );
    });
    ui.horizontal_wrapped(|ui| {
        ui.label(t("sensorfov.mount"));
        ui.selectable_value(&mut st.mount, 0, t("sensorfov.mount_nose"));
        ui.selectable_value(&mut st.mount, 1, t("sensorfov.mount_belly"));
        ui.selectable_value(&mut st.mount, 2, t("sensorfov.mount_gimbal"));
        ui.label(t("sensorfov.tilt"));
        ui.add(
            egui::DragValue::new(&mut st.tilt_deg)
                .range(0.0..=90.0)
                .speed(0.5)
                .suffix("°"),
        );
        if st.mount == 2 {
            ui.label(t("sensorfov.gimbal_drop"));
            ui.add(
                egui::DragValue::new(&mut st.gimbal_drop_mm)
                    .range(10.0..=300.0)
                    .speed(1.0)
                    .suffix(" mm"),
            );
        }
    });

    // ---------------- Hesap ----------------
    let (w, h) = (st.sensor_w.max(0.1), st.sensor_h.max(0.1));
    let (focal, hfov) = if st.use_fov {
        (fov::focal_from_fov(w, st.hfov_deg), st.hfov_deg)
    } else {
        (st.focal_mm.max(0.1), fov::fov_deg(w, st.focal_mm.max(0.1)))
    };
    let vfov = fov::fov_deg(h, focal);
    let airframe = Airframe::from_project(p, m, st.gimbal_drop_mm);
    let cam = Camera {
        pos: airframe.camera_pos(mount_of(st.mount)),
        tilt_deg: st.tilt_deg,
        hfov_deg: hfov,
        vfov_deg: vfov,
    };
    let fp = fov::footprint(st.alt_m, hfov, vfov, st.tilt_deg, w, focal, st.px_w);
    let gsd_nadir = fov::gsd_cm_px(st.alt_m, w, focal, st.px_w);
    let obs = fov::obstruction(&airframe.items, &cam);

    ui.add_space(space::S);
    egui::Grid::new("sfov_res").num_columns(2).show(ui, |ui| {
        row(ui, t("sensorfov.res_hfov"), format!("{}°", fmt(hfov, 1)));
        row(ui, t("sensorfov.res_vfov"), format!("{}°", fmt(vfov, 1)));
        row(
            ui,
            t("sensorfov.res_focal"),
            format!("{} mm", fmt(focal, 2)),
        );
        row(
            ui,
            t("sensorfov.res_gsd_nadir"),
            format!("{} cm/px", fmt(gsd_nadir, 2)),
        );
        row(
            ui,
            t("sensorfov.res_gsd_center"),
            format!(
                "{} / {} cm/px",
                fmt(fp.gsd_cross_cm, 2),
                fmt(fp.gsd_along_cm, 2)
            ),
        );
        row(
            ui,
            t("sensorfov.res_ground"),
            match fp.far_m {
                Some(far) => format!("{} – {} m", fmt(fp.near_m, 0), fmt(far, 0)),
                None => format!(
                    "{} m – {}",
                    fmt(fp.near_m, 0),
                    t("sensorfov.beyond_horizon")
                ),
            },
        );
        row(
            ui,
            t("sensorfov.res_width"),
            match fp.width_far_m {
                Some(wf) => format!("{} / {} m", fmt(fp.width_near_m, 0), fmt(wf, 0)),
                None => format!("{} m", fmt(fp.width_near_m, 0)),
            },
        );
        if let Some(a) = fp.area_m2 {
            row(ui, t("sensorfov.res_area"), format!("{} m²", fmt(a, 0)));
        }
    });

    // ---------------- Kapanma ----------------
    section(ui, t("sensorfov.obs_title"));
    show_obstruction(ui, &obs);
    ui.add_space(space::S);
    draw_views(ui, &airframe, &cam, &obs);
    ui.label(egui::RichText::new(t("sensorfov.view_note")).weak());

    ui.add_space(space::L);
    show_damping(ui, p, m, st);
}

fn show_obstruction(ui: &mut Ui, obs: &ObstructionReport) {
    if obs.items.is_empty() {
        components::banner(ui, Status::Ok, t("sensorfov.obs_none"));
    } else {
        let status = if obs.total_pct >= 10.0 {
            Status::Err
        } else {
            Status::Warn
        };
        components::banner(
            ui,
            status,
            &tf("sensorfov.obs_total_fmt", &[fmt(obs.total_pct, 1)]),
        );
        for it in obs.items.iter().take(12) {
            ui.label(tf(
                "sensorfov.obs_row_fmt",
                &[
                    it.label.clone(),
                    kind_label(it.kind).to_string(),
                    fmt(it.pct, 1),
                ],
            ));
        }
    }
    ui.label(egui::RichText::new(t("sensorfov.obs_note")).weak());
}

// ---------------------------------------------------------------------------------------------
// Yan ve ön görünüm şeması
// ---------------------------------------------------------------------------------------------

type P2 = [f64; 2];

fn proj(v: V3, front: bool) -> P2 {
    if front { [v[1], v[2]] } else { [v[0], v[2]] }
}

fn draw_views(ui: &mut Ui, af: &Airframe, cam: &Camera, obs: &ObstructionReport) {
    let tk = theme::tokens(ui);
    let width = ui.available_width().max(300.0);
    let height = 280.0;
    let (resp, painter) = ui.allocate_painter(Vec2::new(width, height), Sense::hover());
    let rect = resp.rect;
    painter.rect_filled(rect, theme::corner(radius::M), tk.viewport_bg);
    painter.rect_stroke(
        rect,
        theme::corner(radius::M),
        Stroke::new(1.0, tk.border),
        egui::StrokeKind::Inside,
    );
    let half = rect.width() / 2.0;
    for (vi, front) in [false, true].into_iter().enumerate() {
        let r = Rect::from_min_size(
            Pos2::new(rect.min.x + half * vi as f32, rect.min.y),
            Vec2::new(half, rect.height()),
        )
        .shrink(8.0);
        draw_one_view(&painter, r, tk, af, cam, obs, front);
    }
}

fn draw_one_view(
    painter: &egui::Painter,
    rect: Rect,
    tk: &Tokens,
    af: &Airframe,
    cam: &Camera,
    obs: &ObstructionReport,
    front: bool,
) {
    let ext = af
        .items
        .iter()
        .flat_map(|s| s.vertices())
        .chain(std::iter::once(cam.pos))
        .fold(100.0_f64, |a, v| {
            a.max(v[0].abs()).max(v[1].abs()).max(v[2].abs())
        });
    let reach = 1.4 * ext;
    // Kama: kamera + dört köşe ışınının ucu (şematik uzunluk).
    let mut wedge_pts: Vec<P2> = vec![proj(cam.pos, front)];
    for d in cam.corner_dirs() {
        let n = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-9);
        let far = [
            cam.pos[0] + d[0] / n * reach,
            cam.pos[1] + d[1] / n * reach,
            cam.pos[2] + d[2] / n * reach,
        ];
        wedge_pts.push(proj(far, front));
    }
    let wedge = fov::hull(wedge_pts);
    let item_polys: Vec<(&Solid, Vec<P2>)> = af
        .items
        .iter()
        .map(|s| (s, fov::hull(s.vertices().map(|v| proj(v, front)).collect())))
        .collect();
    // Sınırlar
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for q in wedge
        .iter()
        .chain(item_polys.iter().flat_map(|(_, p)| p.iter()))
    {
        for k in 0..2 {
            lo[k] = lo[k].min(q[k]);
            hi[k] = hi[k].max(q[k]);
        }
    }
    if !(lo[0].is_finite() && hi[0] > lo[0] && hi[1] > lo[1]) {
        return;
    }
    let label_h = 16.0;
    let avail = Rect::from_min_max(rect.min + Vec2::new(0.0, label_h), rect.max);
    let scale = ((avail.width() as f64) / (hi[0] - lo[0]))
        .min((avail.height() as f64) / (hi[1] - lo[1]))
        * 0.95;
    let cx = (lo[0] + hi[0]) / 2.0;
    let cz = (lo[1] + hi[1]) / 2.0;
    let to_screen = |q: P2| -> Pos2 {
        Pos2::new(
            avail.center().x + ((q[0] - cx) * scale) as f32,
            avail.center().y - ((q[1] - cz) * scale) as f32,
        )
    };
    painter.text(
        rect.min,
        egui::Align2::LEFT_TOP,
        if front {
            t("sensorfov.view_front")
        } else {
            t("sensorfov.view_side")
        },
        FontId::proportional(12.0),
        tk.text_weak,
    );
    if wedge.len() >= 3 {
        painter.add(egui::Shape::convex_polygon(
            wedge.iter().map(|q| to_screen(*q)).collect(),
            tk.accent.gamma_multiply(0.22),
            Stroke::new(1.0, tk.accent),
        ));
    }
    for (i, (s, poly)) in item_polys.iter().enumerate() {
        if poly.len() < 3 {
            continue;
        }
        let hit = obs.items.iter().any(|b| b.index == i);
        let (fill, stroke) = if hit {
            (tk.danger_soft, Stroke::new(1.5, tk.danger))
        } else if s.kind == ItemKind::Fuselage {
            (tk.surface_alt, Stroke::new(1.0, tk.border_strong))
        } else {
            (tk.surface, Stroke::new(1.0, tk.border_strong))
        };
        painter.add(egui::Shape::convex_polygon(
            poly.iter().map(|q| to_screen(*q)).collect(),
            fill,
            stroke,
        ));
    }
    painter.circle_filled(to_screen(proj(cam.pos, front)), 3.5, tk.accent_fill);
}

// ---------------------------------------------------------------------------------------------
// Titreşim sönümleme
// ---------------------------------------------------------------------------------------------

fn source_label(e: &Excitation) -> String {
    let phase = if e.phase == 0 {
        t("sensorfov.phase_hover")
    } else {
        t("sensorfov.phase_cruise")
    };
    let src = match e.source {
        Source::Rotor1x => t("sensorfov.src_rotor"),
        Source::BladePass => t("sensorfov.src_bpf"),
    };
    format!("{phase} – {src}")
}

fn show_damping(ui: &mut Ui, p: &Project, m: &Model, st: &mut SfState) {
    let tk = theme::tokens(ui);
    section(ui, t("sensorfov.damp_title"));
    ui.label(t("sensorfov.damp_intro"));
    let info = damping::rpm_sources(p, m);
    let fmt_opt = |o: Option<f64>| o.map(|r| fmt(r, 0)).unwrap_or_else(|| "—".into());
    ui.label(tf(
        "sensorfov.rpm_model_fmt",
        &[fmt_opt(info.hover_rpm), fmt_opt(info.cruise_rpm)],
    ));
    ui.horizontal_wrapped(|ui| {
        ui.label(t("sensorfov.hover_rpm"));
        ui.add(
            egui::DragValue::new(&mut st.hover_rpm)
                .range(0.0..=60000.0)
                .speed(20.0)
                .suffix(" rpm"),
        );
        ui.label(t("sensorfov.cruise_rpm"));
        ui.add(
            egui::DragValue::new(&mut st.cruise_rpm)
                .range(0.0..=60000.0)
                .speed(20.0)
                .suffix(" rpm"),
        );
        ui.label(egui::RichText::new(t("sensorfov.rpm_auto")).weak());
    });
    ui.horizontal_wrapped(|ui| {
        ui.label(t("sensorfov.mass"));
        ui.add(
            egui::DragValue::new(&mut st.mass_kg)
                .range(0.01..=20.0)
                .speed(0.01)
                .suffix(" kg"),
        );
        ui.label(t("sensorfov.n_dampers"));
        ui.add(egui::DragValue::new(&mut st.n_dampers).range(1..=12));
        ui.label(t("sensorfov.k_per"));
        if ui
            .add(
                egui::DragValue::new(&mut st.k_per)
                    .range(0.01..=50.0)
                    .speed(0.01)
                    .suffix(" N/mm"),
            )
            .changed()
        {
            st.damper_preset = DAMPER_PRESETS.len();
        }
        ui.label(t("sensorfov.zeta"));
        ui.add(
            egui::DragValue::new(&mut st.zeta)
                .range(0.0..=1.0)
                .speed(0.005),
        );
        ui.label(t("sensorfov.max_sag"));
        ui.add(
            egui::DragValue::new(&mut st.max_sag_mm)
                .range(0.5..=30.0)
                .speed(0.1)
                .suffix(" mm"),
        );
    });
    ui.horizontal_wrapped(|ui| {
        ui.label(t("sensorfov.preset_label"));
        for (i, (_, k, shore)) in DAMPER_PRESETS.iter().enumerate() {
            let txt = format!("{} ({} N/mm, ~{}A)", damper_name(i), fmt(*k, 2), shore);
            if ui.selectable_label(st.damper_preset == i, txt).clicked() {
                st.damper_preset = i;
                st.k_per = *k;
            }
        }
    });
    ui.label(egui::RichText::new(t("sensorfov.typical_example")).weak());

    // Uyarımlar
    let hover = if st.hover_rpm > 0.0 {
        Some(st.hover_rpm)
    } else {
        info.hover_rpm
    };
    let cruise = if st.cruise_rpm > 0.0 {
        Some(st.cruise_rpm)
    } else {
        info.cruise_rpm
    };
    let mut exc: Vec<Excitation> = Vec::new();
    if let Some(r) = hover {
        exc.extend(damping::excitations(0, r, info.hover_blades));
    }
    if let Some(r) = cruise {
        exc.extend(damping::excitations(1, r, info.cruise_blades));
    }
    if exc.is_empty() {
        components::banner(ui, Status::Warn, t("sensorfov.no_rpm"));
        return;
    }
    let inp = DamperInput {
        mass_kg: st.mass_kg,
        n_dampers: st.n_dampers,
        k_per_n_mm: st.k_per,
        zeta: st.zeta,
        max_sag_mm: st.max_sag_mm,
    };
    let rep = damping::analyze(&inp, &exc);

    ui.add_space(space::S);
    egui::Grid::new("sfov_damp_res")
        .num_columns(2)
        .show(ui, |ui| {
            row(
                ui,
                t("sensorfov.res_k_total"),
                format!("{} N/mm", fmt(rep.k_total, 2)),
            );
            row(ui, t("sensorfov.res_fn"), format!("{} Hz", fmt(rep.f_n, 1)));
            row(
                ui,
                t("sensorfov.res_sag"),
                format!("{} mm", fmt(rep.static_defl_mm, 2)),
            );
        });
    ui.add_space(space::XS);
    egui::Grid::new("sfov_damp_tab")
        .striped(true)
        .show(ui, |ui| {
            for h in [
                "sensorfov.col_source",
                "sensorfov.col_freq",
                "sensorfov.col_r",
                "sensorfov.col_t",
                "sensorfov.col_iso",
            ] {
                ui.label(egui::RichText::new(t(h)).strong());
            }
            ui.end_row();
            for r in &rep.results {
                ui.label(source_label(&r.exc));
                ui.label(format!("{} Hz", fmt(r.exc.freq_hz, 1)));
                ui.label(fmt(r.r, 2));
                ui.label(fmt(r.t, 3));
                let color = if r.amplified {
                    tk.danger
                } else if r.isolation_pct >= 80.0 {
                    tk.success
                } else {
                    tk.warning
                };
                ui.label(
                    egui::RichText::new(format!("{} %", fmt(r.isolation_pct, 1))).color(color),
                );
                ui.end_row();
            }
        });
    show_damping_notes(ui, &rep, st);
    show_curve(ui, tk, &rep, st.zeta);
}

fn show_damping_notes(ui: &mut Ui, rep: &DampingReport, st: &SfState) {
    let mut any = false;
    for r in &rep.results {
        if r.near_resonance {
            any = true;
            components::banner(
                ui,
                Status::Err,
                &tf(
                    "sensorfov.warn_resonance_fmt",
                    &[source_label(&r.exc), fmt(r.r, 2)],
                ),
            );
        } else if r.amplified {
            any = true;
            components::banner(
                ui,
                Status::Warn,
                &tf("sensorfov.warn_amplified_fmt", &[source_label(&r.exc)]),
            );
        }
    }
    if rep.too_soft {
        any = true;
        components::banner(
            ui,
            Status::Warn,
            &tf(
                "sensorfov.warn_soft_fmt",
                &[fmt(rep.static_defl_mm, 1), fmt(st.max_sag_mm, 1)],
            ),
        );
    }
    if !any && rep.results.iter().all(|r| r.isolation_pct >= 80.0) {
        components::banner(ui, Status::Ok, t("sensorfov.ok_isolation"));
    }
    if let Some(rec) = &rep.recommendation {
        ui.add_space(space::XS);
        ui.label(egui::RichText::new(t("sensorfov.rec_title")).strong());
        ui.label(tf(
            "sensorfov.rec_range_fmt",
            &[
                fmt(rec.k_min_sag, 2),
                fmt(rec.k_max_total_80, 2),
                fmt(rec.per_damper_min_sag, 2),
                fmt(rec.per_damper_max_80, 2),
            ],
        ));
        ui.label(tf(
            "sensorfov.rec_target_fmt",
            &[
                fmt(rec.k_target_r3, 2),
                fmt(rec.per_damper_target, 2),
                damping::shore_hint(rec.per_damper_target).to_string(),
            ],
        ));
        if !rec.feasible {
            components::banner(ui, Status::Warn, t("sensorfov.rec_infeasible"));
        }
    }
}

fn show_curve(ui: &mut Ui, tk: &Tokens, rep: &DampingReport, zeta: f64) {
    if rep.f_n <= 0.0 {
        return;
    }
    let f_max = rep
        .results
        .iter()
        .map(|r| r.exc.freq_hz)
        .fold(rep.f_n * 2.0, f64::max)
        * 1.25;
    let pts: Vec<[f64; 2]> = damping::curve(rep.f_n, zeta, f_max, 240)
        .into_iter()
        .map(|q| [q[0], q[1].min(10.0)])
        .collect();
    ui.add_space(space::S);
    ui.label(egui::RichText::new(t("sensorfov.curve_title")).strong());
    Plot::new("sfov_curve")
        .height(240.0)
        .x_axis_label(t("sensorfov.curve_x"))
        .y_axis_label(t("sensorfov.curve_y"))
        .legend(Legend::default())
        .allow_scroll(false)
        .show(ui, |pu| {
            pu.line(
                Line::new(t("sensorfov.curve_line"), PlotPoints::from(pts))
                    .color(tk.accent)
                    .width(2.0),
            );
            pu.vline(VLine::new(t("sensorfov.curve_fn"), rep.f_n).color(tk.warning));
            for r in &rep.results {
                pu.vline(VLine::new(source_label(&r.exc), r.exc.freq_hz).color(tk.danger));
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Library, evaluate};
    use crate::i18n::Lang;
    use crate::project::DroneType;
    use crate::ui::theme::{ThemeChoice, install};
    use egui::{RawInput, Rect};

    #[test]
    fn renders_headless_in_both_themes_and_languages() {
        for dt in DroneType::ALL {
            let mut p = Project::default();
            p.set_drone_type(dt);
            let m = evaluate(&p, &Library::embedded());
            for choice in [ThemeChoice::Dark, ThemeChoice::Light] {
                for lang in [Lang::Tr, Lang::En] {
                    crate::i18n::with_lang(lang, || {
                        let ctx = egui::Context::default();
                        install(&ctx, choice);
                        for _ in 0..2 {
                            let mut input = RawInput::default();
                            input.screen_rect =
                                Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1100.0, 900.0)));
                            let mut out = ctx.run_ui(input, |ui| {
                                egui::CentralPanel::default().show(ui, |ui| show(ui, &p, &m));
                            });
                            out.textures_delta.clear();
                        }
                    });
                }
            }
        }
    }
}
