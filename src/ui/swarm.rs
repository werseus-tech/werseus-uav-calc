// SPDX-License-Identifier: AGPL-3.0-or-later
//! Sürü (swarm) sekmesi: tasarımın N özdeş kopyası düzende — iz etkileşimi ve güç, mesh telsiz
//! ağı, aralık / çarpışma denetimi. Hesaplar `crate::swarm` içindedir; bu dosya yalnız girdi,
//! çizim ve tablo. Durum egui belleğinde tutulur, proje dosyasına yazılmaz; proje DEĞİŞMEZ.
use super::components::{self, NumberInput, Status, number_row, section_header, segmented};
use super::theme::{self, Tokens, radius, space};
use super::widgets::{fmt, section};
use crate::calc::Model;
use crate::i18n::{t, tf};
use crate::project::{DroneType, Project};
use crate::swarm::formation::{FormationKind, SpacingLevel};
use crate::swarm::mesh::{RadioPreset, max_range_m};
use crate::swarm::wake::{self, ROLL_HAZARD_SHARE, TYPICAL_ROLL_AUTHORITY};
use crate::swarm::{self, Mode, SwarmInputs, SwarmResult};
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum View {
    Formation,
    Network,
    Safety,
}

/// Arayüz durumu (egui belleğinde).
#[derive(Clone)]
struct SwarmUi {
    dt: DroneType,
    inputs: SwarmInputs,
    view: View,
    cache: Option<(SwarmInputs, Vec<u64>, Arc<SwarmResult>)>,
}

impl SwarmUi {
    fn new(dt: DroneType) -> Self {
        SwarmUi {
            dt,
            inputs: SwarmInputs::for_type(dt),
            view: View::Formation,
            cache: None,
        }
    }
}

fn state_id() -> egui::Id {
    egui::Id::NULL.with("swarm_ui_state")
}

/// Sonucu etkileyen tasarım sayıları (değişince yeniden hesaplanır).
fn fingerprint(p: &Project, m: &Model) -> Vec<u64> {
    [
        p.b,
        p.s,
        p.mr_frame_size,
        p.mr_d_prop,
        p.mr_v_cruise,
        m.aero.cla_w,
        m.aero.k,
        m.weight.w,
        m.atmo.rho,
        m.perf_cr.v,
        m.perf_cr.p_total,
        m.mr.w,
        m.mr.a_disk_total,
        m.mr.p_elec_hover_total,
        m.mr.p_total_hover,
        m.mr.p_total_cruise,
        crate::calc::performance::eta_pt(&m.perf_in),
    ]
    .iter()
    .map(|x| x.to_bits())
    .chain(std::iter::once(p.drone_type as u64))
    .collect()
}

fn result_for(st: &mut SwarmUi, p: &Project, m: &Model) -> Arc<SwarmResult> {
    let fp = fingerprint(p, m);
    if let Some((inp, f, r)) = &st.cache {
        if *inp == st.inputs && *f == fp {
            return r.clone();
        }
    }
    let r = Arc::new(swarm::analyse(&st.inputs, p, m));
    st.cache = Some((st.inputs.clone(), fp, r.clone()));
    r
}

pub fn show(ui: &mut Ui, p: &Project, m: &Model) {
    let id = state_id();
    let mut st = ui
        .ctx()
        .data(|d| d.get_temp::<SwarmUi>(id))
        .filter(|s| s.dt == p.drone_type)
        .unwrap_or_else(|| SwarmUi::new(p.drone_type));
    let tk = theme::tokens(ui);

    section_header(ui, t("tab_swarm"));
    components::banner(ui, Status::Info, t("swarm.disclaimer"));
    ui.add_space(space::XS);

    inputs(ui, p, &mut st.inputs);
    let r = result_for(&mut st, p, m);

    ui.add_space(space::S);
    ui.separator();
    summary_line(ui, tk, &r);
    ui.add_space(space::XS);
    segmented(
        ui,
        &mut st.view,
        &[
            (View::Formation, t("swarm.view_formation"), ""),
            (View::Network, t("swarm.view_network"), ""),
            (View::Safety, t("swarm.view_safety"), ""),
        ],
    );
    ui.add_space(space::XS);
    match st.view {
        View::Formation => show_formation(ui, &r),
        View::Network => show_network(ui, &st.inputs, &r),
        View::Safety => show_safety(ui, &st.inputs, &r),
    }
    ui.ctx().data_mut(|d| d.insert_temp(id, st));
}

fn mode_label(mode: Mode) -> &'static str {
    match mode {
        Mode::Wing => t("swarm.mode_wing"),
        Mode::RotorHover => t("swarm.mode_hover"),
        Mode::RotorCruise => t("swarm.mode_cruise"),
    }
}

fn kind_label(k: FormationKind) -> &'static str {
    match k {
        FormationKind::Line => t("swarm.kind_line"),
        FormationKind::Column => t("swarm.kind_column"),
        FormationKind::Vee => t("swarm.kind_vee"),
        FormationKind::Echelon => t("swarm.kind_echelon"),
        FormationKind::Grid => t("swarm.kind_grid"),
        FormationKind::Circle => t("swarm.kind_circle"),
    }
}

fn preset_label(pr: RadioPreset) -> &'static str {
    match pr {
        RadioPreset::Mhz915 => t("swarm.preset_915"),
        RadioPreset::Ghz24 => t("swarm.preset_24"),
        RadioPreset::Ghz58 => t("swarm.preset_58"),
    }
}

fn inputs(ui: &mut Ui, p: &Project, inp: &mut SwarmInputs) {
    let modes = swarm::modes_for(p.drone_type);
    egui::CollapsingHeader::new(egui::RichText::new(t("swarm.group_formation")).strong())
        .id_salt("swarm_in_formation")
        .default_open(true)
        .show(ui, |ui| {
            if modes.len() > 1 {
                let opts: Vec<(Mode, &str, &str)> =
                    modes.iter().map(|&md| (md, mode_label(md), "")).collect();
                segmented(ui, &mut inp.mode, &opts);
            }
            let kinds: Vec<(FormationKind, &str, &str)> = FormationKind::ALL
                .iter()
                .map(|&k| (k, kind_label(k), ""))
                .collect();
            ui.horizontal_wrapped(|ui| {
                segmented(ui, &mut inp.kind, &kinds);
            });
            let mut n = inp.n as u32;
            if components::u32_row(ui, t("swarm.n"), &mut n, 1, 40, t("swarm.n_help")) {
                inp.n = n as usize;
            }
            ui.horizontal(|ui| {
                ui.label(t("swarm.length_unit"));
                segmented(
                    ui,
                    &mut inp.in_sizes,
                    &[
                        (true, t("swarm.unit_sizes"), t("swarm.unit_sizes_help")),
                        (false, "m", ""),
                    ],
                );
            });
            let size = swarm::vehicle_size(p, inp.mode);
            let unit = if inp.in_sizes { t("swarm.unit_l") } else { "m" };
            let note = |v: f64| inp.in_sizes.then(|| tf("swarm.eq_m", &[&fmt(v * size, 2)]));
            let note_s = note(inp.spacing);
            number_row(
                ui,
                NumberInput::new(t("swarm.spacing"), &mut inp.spacing)
                    .unit(unit)
                    .range(0.0, 500.0)
                    .help(t("swarm.spacing_help"))
                    .note(note_s),
            );
            let note_x = note(inp.step_x);
            number_row(
                ui,
                NumberInput::new(t("swarm.step_x"), &mut inp.step_x)
                    .unit(unit)
                    .range(-500.0, 500.0)
                    .help(t("swarm.step_x_help"))
                    .note(note_x),
            );
            number_row(
                ui,
                NumberInput::new(t("swarm.step_h"), &mut inp.step_h)
                    .unit("m")
                    .range(-100.0, 100.0)
                    .help(t("swarm.step_h_help")),
            );
            number_row(
                ui,
                NumberInput::new(t("swarm.pos_err"), &mut inp.pos_err)
                    .unit("m")
                    .range(0.0, 20.0)
                    .help(t("swarm.pos_err_help")),
            );
            ui.label(
                egui::RichText::new(tf("swarm.size_line", &[&fmt(size, 2)]))
                    .color(theme::tokens(ui).text_weak),
            );
        });
    egui::CollapsingHeader::new(egui::RichText::new(t("swarm.group_radio")).strong())
        .id_salt("swarm_in_radio")
        .default_open(false)
        .show(ui, |ui| {
            let presets: Vec<(RadioPreset, &str, &str)> = RadioPreset::ALL
                .iter()
                .map(|&pr| (pr, preset_label(pr), t("swarm.preset_help")))
                .collect();
            if segmented(ui, &mut inp.preset, &presets) {
                inp.radio = inp.preset.radio();
            }
            let r = &mut inp.radio;
            for (label, v, u, lo, hi, help) in [
                (t("swarm.freq"), &mut r.freq_mhz, "MHz", 100.0, 10_000.0, ""),
                (t("swarm.tx"), &mut r.tx_dbm, "dBm", -10.0, 36.0, ""),
                (
                    t("swarm.gain"),
                    &mut r.gain_dbi,
                    "dBi",
                    -5.0,
                    20.0,
                    t("swarm.gain_help"),
                ),
                (t("swarm.sens"), &mut r.sens_dbm, "dBm", -140.0, -40.0, ""),
                (
                    t("swarm.fade"),
                    &mut r.fade_db,
                    "dB",
                    0.0,
                    40.0,
                    t("swarm.fade_help"),
                ),
                (
                    t("swarm.rate"),
                    &mut r.rate_mbps,
                    "Mbit/s",
                    0.01,
                    1000.0,
                    "",
                ),
                (
                    t("swarm.spread"),
                    &mut inp.spread,
                    "m",
                    0.0,
                    20_000.0,
                    t("swarm.spread_help"),
                ),
                (
                    t("swarm.gs_distance"),
                    &mut inp.gs_distance,
                    "m",
                    0.0,
                    50_000.0,
                    "",
                ),
                (t("swarm.altitude"), &mut inp.altitude, "m", 0.0, 5000.0, ""),
            ] {
                number_row(
                    ui,
                    NumberInput::new(label, v)
                        .unit(u)
                        .range(lo, hi)
                        .decimals(1)
                        .help(help),
                );
            }
        });
}

fn pct(x: f64) -> String {
    if x.is_finite() {
        format!("{:+.1} %", 100.0 * x)
    } else {
        "—".into()
    }
}

fn summary_line(ui: &mut Ui, tk: &Tokens, r: &SwarmResult) {
    let tot = r.p_total();
    let alone = r.p_alone_total();
    let frac = if alone > 0.0 { tot / alone - 1.0 } else { 0.0 };
    let col = if frac < -1e-3 {
        tk.success
    } else if frac > 1e-3 {
        tk.warning
    } else {
        tk.text
    };
    ui.horizontal_wrapped(|ui| {
        ui.label(tf(
            "swarm.summary",
            &[
                mode_label(r.mode),
                &r.positions.len().to_string(),
                &fmt(r.v, 1),
                &fmt(alone, 0),
                &fmt(tot, 0),
            ],
        ));
        ui.colored_label(col, pct(frac));
    });
}

// ------------------------------------------------------------------------------------------
// Çizim yardımcıları
// ------------------------------------------------------------------------------------------

/// Dünya (a, b) → ekran eşlemesi; eşit ölçek, kenar boşluklu.
struct Fit {
    rect: Rect,
    min: [f64; 2],
    max: [f64; 2],
    scale: f64,
}

impl Fit {
    fn new(rect: Rect, pts: impl Iterator<Item = [f64; 2]>, pad_world: f64) -> Self {
        let mut min = [f64::INFINITY; 2];
        let mut max = [f64::NEG_INFINITY; 2];
        for q in pts {
            for k in 0..2 {
                if q[k].is_finite() {
                    min[k] = min[k].min(q[k]);
                    max[k] = max[k].max(q[k]);
                }
            }
        }
        for k in 0..2 {
            if !min[k].is_finite() {
                min[k] = -1.0;
                max[k] = 1.0;
            }
            min[k] -= pad_world;
            max[k] += pad_world;
        }
        let inner = rect.shrink(16.0);
        let sx = inner.width() as f64 / (max[0] - min[0]).max(1e-6);
        let sy = inner.height() as f64 / (max[1] - min[1]).max(1e-6);
        Fit {
            rect: inner,
            min,
            max,
            scale: sx.min(sy),
        }
    }
    /// a → sağa, b → yukarı.
    fn pos(&self, a: f64, b: f64) -> Pos2 {
        let cx = 0.5 * (self.min[0] + self.max[0]);
        let cy = 0.5 * (self.min[1] + self.max[1]);
        let c = self.rect.center();
        Pos2::new(
            c.x + ((a - cx) * self.scale) as f32,
            c.y - ((b - cy) * self.scale) as f32,
        )
    }
    fn len(&self, d: f64) -> f32 {
        (d * self.scale) as f32
    }
}

fn canvas(ui: &mut Ui, h: f32, title: &str) -> (Rect, egui::Painter) {
    let tk = theme::tokens(ui);
    ui.label(egui::RichText::new(title).strong());
    let w = ui.available_width().max(260.0);
    let (resp, painter) = ui.allocate_painter(Vec2::new(w, h), Sense::hover());
    let rect = resp.rect;
    painter.rect_filled(rect, theme::corner(radius::M), tk.viewport_bg);
    painter.rect_stroke(
        rect,
        theme::corner(radius::M),
        Stroke::new(1.0, tk.border),
        egui::StrokeKind::Inside,
    );
    (rect, painter)
}

fn effect_color(tk: &Tokens, d_power: f64, p_alone: f64) -> Color32 {
    let f = if p_alone > 0.0 {
        d_power / p_alone
    } else {
        0.0
    };
    if f < -0.005 {
        tk.success
    } else if f > 0.05 {
        tk.danger
    } else if f > 0.005 {
        tk.warning
    } else {
        tk.text
    }
}

fn label_at(p: &egui::Painter, pos: Pos2, text: String, col: Color32) {
    p.text(
        pos + Vec2::new(6.0, -6.0),
        Align2::LEFT_BOTTOM,
        text,
        FontId::proportional(11.0),
        col,
    );
}

// ------------------------------------------------------------------------------------------
// Düzen ve iz görünümü
// ------------------------------------------------------------------------------------------

fn show_formation(ui: &mut Ui, r: &SwarmResult) {
    let tk = theme::tokens(ui);
    section(ui, t("swarm.formation_title"));
    let h = 260.0;
    // Üstten görünüş: uçuş yönü yukarı (x kıç yönü → aşağı), y sağa.
    let (rect, painter) = canvas(ui, h, t("swarm.top_view"));
    let pad = r.size;
    let fit = Fit::new(
        rect,
        r.positions
            .iter()
            .map(|q| [q[1], -q[0]])
            .chain(r.positions.iter().map(|q| [q[1], -q[0] - 2.0 * r.size])),
        pad,
    );
    if r.mode == Mode::Wing {
        draw_wing_field(&painter, tk, &fit, r);
    } else {
        draw_rotor_wake_top(&painter, tk, &fit, r);
    }
    for (i, q) in r.positions.iter().enumerate() {
        let c = fit.pos(q[1], -q[0]);
        let col = effect_color(tk, r.p_each[i] - r.p_alone, r.p_alone);
        if r.mode == Mode::Wing {
            let hw = fit.len(0.5 * r.size).max(4.0);
            painter.line_segment(
                [c - Vec2::new(hw, 0.0), c + Vec2::new(hw, 0.0)],
                Stroke::new(3.0, col),
            );
            painter.line_segment(
                [c - Vec2::new(0.0, hw * 0.25), c + Vec2::new(0.0, hw * 0.5)],
                Stroke::new(2.0, col),
            );
            if r.wing_fx.get(i).is_some_and(|e| e.roll_hazard()) {
                painter.circle_stroke(c, hw + 3.0, Stroke::new(1.5, tk.danger));
            }
        } else {
            let rad = fit.len(0.5 * r.size).max(3.0);
            painter.circle_stroke(c, rad, Stroke::new(2.0, col));
        }
        label_at(&painter, c, format!("{}", i + 1), tk.text);
    }
    painter.text(
        rect.left_top() + Vec2::new(8.0, 6.0),
        Align2::LEFT_TOP,
        t("swarm.flight_dir_up"),
        FontId::proportional(11.0),
        tk.text_weak,
    );

    ui.add_space(space::XS);
    // Yandan görünüş: uçuş yönü sola (x kıç → sağa), h yukarı.
    let (rect, painter) = canvas(ui, 180.0, t("swarm.side_view"));
    let wake_depth = if r.mode == Mode::Wing {
        r.size
    } else {
        r.safe_sep.map(|s| s.0).unwrap_or(r.size).min(10.0 * r.size)
    };
    let fit = Fit::new(
        rect,
        r.positions
            .iter()
            .map(|q| [q[0], q[2]])
            .chain(r.positions.iter().map(|q| [q[0], q[2] - wake_depth])),
        pad,
    );
    if r.mode.is_rotor() {
        draw_rotor_wake_side(&painter, tk, &fit, r, wake_depth);
    } else {
        // Girdap çiftinin çökmesi: kıçtan geriye düşen iz çizgisi.
        for q in &r.positions {
            let x_end = fit.max[0];
            let drop = if r.v > 0.0 {
                r.wake_speed * (x_end - q[0]) / r.v
            } else {
                0.0
            };
            painter.line_segment(
                [fit.pos(q[0], q[2]), fit.pos(x_end, q[2] - drop)],
                Stroke::new(1.0, tk.info.gamma_multiply(0.7)),
            );
        }
    }
    for (i, q) in r.positions.iter().enumerate() {
        let c = fit.pos(q[0], q[2]);
        let col = effect_color(tk, r.p_each[i] - r.p_alone, r.p_alone);
        let hw = fit.len(0.5 * r.size).clamp(3.0, 30.0);
        painter.line_segment(
            [c - Vec2::new(hw, 0.0), c + Vec2::new(hw, 0.0)],
            Stroke::new(3.0, col),
        );
        label_at(&painter, c, format!("{}", i + 1), tk.text);
    }
    painter.text(
        rect.left_top() + Vec2::new(8.0, 6.0),
        Align2::LEFT_TOP,
        t("swarm.flight_dir_left"),
        FontId::proportional(11.0),
        tk.text_weak,
    );
    legend(ui, tk, r.mode);

    vehicle_table(ui, r);
    totals(ui, tk, r);
}

fn legend(ui: &mut Ui, tk: &Tokens, mode: Mode) {
    ui.horizontal_wrapped(|ui| {
        if mode == Mode::Wing {
            ui.colored_label(tk.success, t("swarm.legend_upwash"));
            ui.colored_label(tk.danger, t("swarm.legend_downwash"));
        } else {
            ui.colored_label(tk.danger, t("swarm.legend_rotor_wake"));
        }
        ui.label("·");
        ui.colored_label(tk.success, t("swarm.legend_saving"));
        ui.colored_label(tk.warning, t("swarm.legend_penalty"));
    });
}

/// Sabit kanat: üstten görünüşte liderlerin düşey hız alanı (yukarı yeşil, aşağı kırmızı).
fn draw_wing_field(painter: &egui::Painter, tk: &Tokens, fit: &Fit, r: &SwarmResult) {
    if r.wing_fx.is_empty() {
        return;
    }
    let n = r.positions.len();
    let shoes: Vec<wake::Horseshoe> = r
        .positions
        .iter()
        .map(|q| wake::Horseshoe {
            pos: *q,
            half_span: 0.5 * wake::VORTEX_SPAN_RATIO * r.size,
            // Birim sirkülasyon: renk yalnız işaret ve göreli büyüklük gösterir.
            gamma: 1.0,
            core: wake::CORE_FRACTION * wake::VORTEX_SPAN_RATIO * r.size,
            v: 0.0,
        })
        .collect();
    let (nx, ny) = (48usize, 32usize);
    let cw = fit.rect.width() / nx as f32;
    let ch = fit.rect.height() / ny as f32;
    let ref_w = 1.0 / (2.0 * std::f64::consts::PI * 0.5 * wake::VORTEX_SPAN_RATIO * r.size);
    let h0 = r.positions.iter().map(|q| q[2]).sum::<f64>() / n.max(1) as f64;
    for ix in 0..nx {
        for iy in 0..ny {
            let sp = Pos2::new(
                fit.rect.left() + (ix as f32 + 0.5) * cw,
                fit.rect.top() + (iy as f32 + 0.5) * ch,
            );
            let c = fit.rect.center();
            let a = 0.5 * (fit.min[0] + fit.max[0]) + ((sp.x - c.x) as f64) / fit.scale;
            let b = 0.5 * (fit.min[1] + fit.max[1]) - ((sp.y - c.y) as f64) / fit.scale;
            let p = [-b, a, h0];
            let w: f64 = shoes.iter().map(|s| s.velocity(p)[2]).sum();
            let k = (w / ref_w).clamp(-1.0, 1.0);
            if k.abs() < 0.03 {
                continue;
            }
            let col = if k > 0.0 { tk.success } else { tk.danger };
            painter.rect_filled(
                Rect::from_center_size(sp, Vec2::new(cw, ch)),
                0.0,
                col.gamma_multiply((0.45 * k.abs()) as f32),
            );
        }
    }
}

fn rotor_vehicle_from(r: &SwarmResult) -> Option<f64> {
    (r.wake_speed > 0.0).then_some(r.wake_speed)
}

/// Multirotor: iz merkez çizgisi boyunca yarı-genişlik daireleri (yandan).
fn draw_rotor_wake_side(
    painter: &egui::Painter,
    tk: &Tokens,
    fit: &Fit,
    r: &SwarmResult,
    depth: f64,
) {
    let Some(v_h) = rotor_vehicle_from(r) else {
        return;
    };
    let r_eq = rotor_r_eq(r);
    let v_i = wake::induced_velocity(v_h, r.v, 0.0);
    for q in &r.positions {
        let (mut x, mut z, mut s) = (0.0, 0.0, 0.0);
        let ds = (depth / 40.0).max(1e-3);
        while z < depth && s < 50.0 * depth {
            let (w, rad) = wake::wake_profile(s, v_i, r_eq);
            let k = (w / (2.0 * v_h)).clamp(0.0, 1.0) as f32;
            painter.circle_filled(
                fit.pos(q[0] + x, q[2] - z),
                fit.len(rad).max(1.0),
                tk.danger.gamma_multiply(0.12 * k + 0.02),
            );
            let m = (r.v * r.v + w * w).sqrt().max(1e-9);
            x += ds * r.v / m;
            z += ds * w / m;
            s += ds;
        }
    }
}

/// Eşdeğer tek disk yarıçapı √(A/π).
fn rotor_r_eq(r: &SwarmResult) -> f64 {
    r.rotor_r_eq.max(1e-3)
}

/// Multirotor: üstten görünüşte iz izdüşümü (kıç yönüne taşınan bant).
fn draw_rotor_wake_top(painter: &egui::Painter, tk: &Tokens, fit: &Fit, r: &SwarmResult) {
    let Some(v_h) = rotor_vehicle_from(r) else {
        return;
    };
    let r_eq = rotor_r_eq(r);
    let v_i = wake::induced_velocity(v_h, r.v, 0.0);
    let depth = r.safe_sep.map(|s| s.0).unwrap_or(r.size).min(10.0 * r.size);
    for q in &r.positions {
        let (mut x, mut z, mut s) = (0.0, 0.0, 0.0);
        let ds = (depth / 30.0).max(1e-3);
        while z < depth && s < 50.0 * depth {
            let (w, rad) = wake::wake_profile(s, v_i, r_eq);
            let k = (w / (2.0 * v_h)).clamp(0.0, 1.0) as f32;
            painter.circle_filled(
                fit.pos(q[1], -(q[0] + x)),
                fit.len(rad).max(1.0),
                tk.danger.gamma_multiply(0.08 * k + 0.01),
            );
            let m = (r.v * r.v + w * w).sqrt().max(1e-9);
            x += ds * r.v / m;
            z += ds * w / m;
            s += ds;
        }
    }
}

fn vehicle_table(ui: &mut Ui, r: &SwarmResult) {
    let tk = theme::tokens(ui);
    section(ui, t("swarm.table_title"));
    let wing = r.mode == Mode::Wing;
    egui::ScrollArea::horizontal()
        .id_salt("swarm_tbl_scroll")
        .show(ui, |ui| {
            egui::Grid::new("swarm_vehicle_tbl")
                .striped(true)
                .num_columns(if wing { 8 } else { 7 })
                .show(ui, |ui| {
                    let mut heads = vec![
                        t("swarm.col_no"),
                        t("swarm.col_pos"),
                        if wing {
                            t("swarm.col_upwash")
                        } else {
                            t("swarm.col_downwash")
                        },
                        if wing {
                            t("swarm.col_ddi")
                        } else {
                            t("swarm.col_tloss")
                        },
                        t("swarm.col_dp"),
                        t("swarm.col_p"),
                    ];
                    if wing {
                        heads.push(t("swarm.col_roll"));
                    }
                    heads.push(t("swarm.col_note"));
                    for h in heads {
                        ui.label(egui::RichText::new(h).strong());
                    }
                    ui.end_row();
                    for (i, q) in r.positions.iter().enumerate() {
                        let dp = r.p_each[i] - r.p_alone;
                        ui.label(format!("{}", i + 1));
                        ui.label(format!(
                            "{} / {} / {}",
                            fmt(q[0], 1),
                            fmt(q[1], 1),
                            fmt(q[2], 1)
                        ));
                        let mut note = String::new();
                        if wing {
                            let e = r.wing_fx[i];
                            ui.label(fmt(e.w_mean, 2));
                            ui.label(pct(e.d_drag_frac));
                            ui.colored_label(effect_color(tk, dp, r.p_alone), fmt(dp, 1));
                            ui.label(fmt(r.p_each[i], 0));
                            let rc = if e.roll_hazard() { tk.danger } else { tk.text };
                            ui.colored_label(rc, fmt(e.cl_roll, 4));
                            if e.roll_hazard() {
                                note = t("swarm.note_roll").to_string();
                            }
                        } else {
                            let e = r.rotor_fx[i];
                            ui.label(fmt(e.w, 2));
                            ui.label(pct(-e.thrust_loss));
                            ui.colored_label(effect_color(tk, dp, r.p_alone), fmt(dp, 1));
                            ui.label(fmt(r.p_each[i], 0));
                            if e.thrust_loss > wake::SAFE_THRUST_LOSS {
                                note = t("swarm.note_in_wake").to_string();
                            }
                        }
                        ui.label(egui::RichText::new(note).color(tk.danger));
                        ui.end_row();
                    }
                });
        });
}

fn totals(ui: &mut Ui, tk: &Tokens, r: &SwarmResult) {
    let tot = r.p_total();
    let alone = r.p_alone_total();
    egui::Grid::new("swarm_totals")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label(t("swarm.p_alone_each"));
            ui.label(format!("{} W", fmt(r.p_alone, 1)));
            ui.end_row();
            ui.label(t("swarm.p_n_alone"));
            ui.label(format!("{} W", fmt(alone, 0)));
            ui.end_row();
            ui.label(t("swarm.p_formation"));
            ui.label(format!("{} W", fmt(tot, 0)));
            ui.end_row();
            ui.label(t("swarm.p_change"));
            let f = if alone > 0.0 { tot / alone - 1.0 } else { 0.0 };
            ui.colored_label(
                if f < 0.0 { tk.success } else { tk.text },
                format!("{} W ({})", fmt(tot - alone, 1), pct(f)),
            );
            ui.end_row();
        });
    match r.mode {
        Mode::Wing => {
            if let Some((b, f)) = r.best_lateral {
                ui.label(tf("swarm.best_lateral", &[&fmt(b, 2), &pct(f)]));
            }
            ui.label(tf("swarm.sink_line", &[&fmt(r.wake_speed, 2)]));
            ui.label(egui::RichText::new(t("swarm.wing_note")).color(tk.text_weak));
        }
        Mode::RotorHover | Mode::RotorCruise => {
            if let Some((zv, dh)) = r.safe_sep {
                ui.label(tf(
                    "swarm.safe_sep",
                    &[
                        &fmt(zv, 1),
                        &fmt(dh, 2),
                        &fmt(100.0 * wake::SAFE_THRUST_LOSS, 0),
                    ],
                ));
            }
            ui.label(tf("swarm.vh_line", &[&fmt(r.wake_speed, 2)]));
            ui.label(egui::RichText::new(t("swarm.rotor_note")).color(tk.text_weak));
        }
    }
}

// ------------------------------------------------------------------------------------------
// Ağ görünümü
// ------------------------------------------------------------------------------------------

fn show_network(ui: &mut Ui, inp: &SwarmInputs, r: &SwarmResult) {
    let tk = theme::tokens(ui);
    section(ui, t("swarm.network_title"));
    let mesh = &r.mesh;
    let gs = r.mesh_nodes.len().saturating_sub(1);
    let (rect, painter) = canvas(ui, 300.0, t("swarm.network_view"));
    let fit = Fit::new(
        rect,
        r.mesh_nodes.iter().map(|q| [q[1], -q[0]]),
        0.05 * inp.spread.max(10.0),
    );
    let fade = inp.radio.fade_db;
    for l in mesh.links.iter().filter(|l| l.up) {
        let a = r.mesh_nodes[l.a];
        let b = r.mesh_nodes[l.b];
        let col = if l.margin >= fade + 6.0 {
            tk.success
        } else {
            tk.warning
        };
        painter.line_segment(
            [fit.pos(a[1], -a[0]), fit.pos(b[1], -b[0])],
            Stroke::new(1.5, col.gamma_multiply(0.8)),
        );
    }
    for (i, q) in r.mesh_nodes.iter().enumerate() {
        let c = fit.pos(q[1], -q[0]);
        let cut = mesh.articulation.contains(&i);
        let reach = mesh.hops.get(i).copied().flatten();
        let col = match reach {
            None => tk.danger,
            Some(_) if cut => tk.warning,
            Some(_) => tk.accent,
        };
        if i == gs {
            painter.rect_filled(Rect::from_center_size(c, Vec2::splat(10.0)), 1.0, col);
            label_at(&painter, c, t("swarm.gs").to_string(), tk.text);
        } else {
            painter.circle_filled(c, 5.0, col);
            let hops = reach.map(|h| h.to_string()).unwrap_or_else(|| "×".into());
            label_at(&painter, c, format!("{} ({})", i + 1, hops), tk.text);
        }
    }
    ui.horizontal_wrapped(|ui| {
        ui.colored_label(
            tk.success,
            tf("swarm.legend_margin_good", &[&fmt(fade + 6.0, 0)]),
        );
        ui.colored_label(tk.warning, tf("swarm.legend_margin_low", &[&fmt(fade, 0)]));
        ui.colored_label(tk.danger, t("swarm.legend_unreachable"));
    });

    if mesh.connected {
        components::banner(ui, Status::Ok, t("swarm.connected"));
    } else {
        components::banner(ui, Status::Err, t("swarm.disconnected"));
    }
    let cuts: Vec<String> = mesh
        .articulation
        .iter()
        .map(|&i| {
            if i == gs {
                t("swarm.gs").to_string()
            } else {
                (i + 1).to_string()
            }
        })
        .collect();
    if cuts.is_empty() {
        ui.label(t("swarm.no_cut"));
    } else {
        ui.colored_label(tk.warning, tf("swarm.cut_nodes", &[&cuts.join(", ")]));
    }
    ui.label(tf(
        "swarm.range_line",
        &[&fmt(max_range_m(&inp.radio), 0), &fmt(inp.radio.fade_db, 0)],
    ));

    egui::Grid::new("swarm_mesh_tbl")
        .striped(true)
        .num_columns(4)
        .show(ui, |ui| {
            for h in [
                t("swarm.col_node"),
                t("swarm.col_hops"),
                t("swarm.col_gs_margin"),
                t("swarm.col_tput"),
            ] {
                ui.label(egui::RichText::new(h).strong());
            }
            ui.end_row();
            for i in 0..gs {
                ui.label((i + 1).to_string());
                ui.label(
                    mesh.hops[i]
                        .map(|h| h.to_string())
                        .unwrap_or_else(|| "—".into()),
                );
                let m = mesh
                    .links
                    .iter()
                    .find(|l| l.a == i && l.b == gs)
                    .map(|l| l.margin)
                    .unwrap_or(f64::NAN);
                ui.label(format!("{} dB", fmt(m, 1)));
                ui.label(format!("{} Mbit/s", fmt(mesh.throughput[i], 2)));
                ui.end_row();
            }
        });
    ui.label(egui::RichText::new(t("swarm.network_note")).color(tk.text_weak));
}

// ------------------------------------------------------------------------------------------
// Güvenlik görünümü
// ------------------------------------------------------------------------------------------

fn show_safety(ui: &mut Ui, inp: &SwarmInputs, r: &SwarmResult) {
    let tk = theme::tokens(ui);
    section(ui, t("swarm.safety_title"));
    ui.label(tf(
        "swarm.required_line",
        &[
            &fmt(r.size, 2),
            &fmt(inp.pos_err, 2),
            &fmt(r.required_spacing, 2),
        ],
    ));
    match r.min_pair {
        Some((i, j, d)) => {
            ui.label(tf(
                "swarm.min_pair_line",
                &[&(i + 1).to_string(), &(j + 1).to_string(), &fmt(d, 2)],
            ));
        }
        None => {
            ui.label(t("swarm.single_vehicle"));
        }
    }
    match r.worst_spacing() {
        SpacingLevel::Ok => components::banner(ui, Status::Ok, t("swarm.spacing_ok")),
        SpacingLevel::Tight => components::banner(ui, Status::Warn, t("swarm.spacing_tight")),
        SpacingLevel::Conflict => components::banner(ui, Status::Err, t("swarm.spacing_conflict")),
    };
    for (i, j, d, lv) in r.spacing_issues.iter().take(12) {
        let col = if *lv == SpacingLevel::Conflict {
            tk.danger
        } else {
            tk.warning
        };
        ui.colored_label(
            col,
            tf(
                "swarm.pair_issue",
                &[&(i + 1).to_string(), &(j + 1).to_string(), &fmt(*d, 2)],
            ),
        );
    }
    if r.spacing_issues.len() > 12 {
        ui.label(tf(
            "swarm.more_issues",
            &[&(r.spacing_issues.len() - 12).to_string()],
        ));
    }
    ui.add_space(space::S);
    match r.mode {
        Mode::Wing => {
            let hz: Vec<String> = r
                .wing_fx
                .iter()
                .enumerate()
                .filter(|(_, e)| e.roll_hazard())
                .map(|(i, _)| (i + 1).to_string())
                .collect();
            if hz.is_empty() {
                ui.label(tf(
                    "swarm.roll_ok",
                    &[
                        &fmt(100.0 * ROLL_HAZARD_SHARE, 0),
                        &fmt(TYPICAL_ROLL_AUTHORITY, 2),
                    ],
                ));
            } else {
                ui.colored_label(tk.danger, tf("swarm.roll_hazard", &[&hz.join(", ")]));
            }
        }
        Mode::RotorHover | Mode::RotorCruise => {
            let bad: Vec<String> = r
                .rotor_fx
                .iter()
                .enumerate()
                .filter(|(_, e)| e.thrust_loss > wake::SAFE_THRUST_LOSS)
                .map(|(i, _)| (i + 1).to_string())
                .collect();
            if bad.is_empty() {
                ui.label(t("swarm.wake_ok"));
            } else {
                ui.colored_label(tk.danger, tf("swarm.wake_hazard", &[&bad.join(", ")]));
            }
            if let Some((zv, dh)) = r.safe_sep {
                ui.label(tf(
                    "swarm.safe_sep",
                    &[
                        &fmt(zv, 1),
                        &fmt(dh, 2),
                        &fmt(100.0 * wake::SAFE_THRUST_LOSS, 0),
                    ],
                ));
            }
        }
    }
    ui.label(egui::RichText::new(t("swarm.safety_note")).color(tk.text_weak));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Library, evaluate};

    /// Sürü sekmesi her türde, üç görünümde, iki temada ve iki dilde başsız çizilir (panik yok).
    #[test]
    fn renders_every_type_view_theme_and_language() {
        use crate::ui::theme::ThemeChoice;
        let lib = Library::embedded();
        for dt in DroneType::ALL {
            let mut p = Project::default();
            p.set_drone_type(dt);
            let m = evaluate(&p, &lib);
            for choice in [ThemeChoice::Dark, ThemeChoice::Light] {
                for lang in [crate::i18n::Lang::Tr, crate::i18n::Lang::En] {
                    let ctx = egui::Context::default();
                    theme::install(&ctx, choice);
                    for view in [View::Formation, View::Network, View::Safety] {
                        for &mode in swarm::modes_for(dt) {
                            let mut st = SwarmUi::new(dt);
                            st.view = view;
                            st.inputs.mode = mode;
                            ctx.data_mut(|d| d.insert_temp(state_id(), st));
                            crate::i18n::with_lang(lang, || {
                                for _ in 0..2 {
                                    let mut input = egui::RawInput::default();
                                    input.screen_rect = Some(Rect::from_min_size(
                                        Pos2::ZERO,
                                        Vec2::new(1000.0, 800.0),
                                    ));
                                    let mut out = ctx.run_ui(input, |ui| {
                                        egui::CentralPanel::default().show(ui, |ui| {
                                            egui::ScrollArea::vertical()
                                                .show(ui, |ui| show(ui, &p, &m));
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
    }

    /// Kaynaktaki her `t("…")` / `tf("…")` anahtarı iki dil tablosunda da var.
    #[test]
    fn all_keys_exist_in_both_languages() {
        let src = include_str!("swarm.rs");
        let tr = crate::i18n::tr_table();
        let en = crate::i18n::en_table();
        let mut n = 0;
        for pat in ["t(", "tf("] {
            for (pos, _) in src.match_indices(pat) {
                // `fmt(`, `pct(` vb. ile karışmasın: önceki karakter tanımlayıcı olmamalı.
                let prev = src[..pos].chars().last().unwrap_or(' ');
                if prev.is_alphanumeric() || prev == '_' {
                    continue;
                }
                // Çok satırlı çağrılar: `tf(\n    "anahtar"`; `(` ile `"` arasında boşluk olabilir.
                let rest = src[pos + pat.len()..].trim_start();
                let Some(rest) = rest.strip_prefix('"') else {
                    continue;
                };
                let key = &rest[..rest.find('"').unwrap()];
                if key.is_empty() || key.contains(' ') || key == "…" {
                    continue;
                }
                assert!(tr.contains_key(key), "tr.json eksik: {key}");
                assert!(en.contains_key(key), "en.json eksik: {key}");
                n += 1;
            }
        }
        assert!(n > 50, "anahtar taraması çalışmadı: {n}");
    }

    #[test]
    fn cache_recomputes_only_on_change() {
        let p = Project::default();
        let m = evaluate(&p, &Library::embedded());
        let mut st = SwarmUi::new(p.drone_type);
        let a = result_for(&mut st, &p, &m);
        let b = result_for(&mut st, &p, &m);
        assert!(Arc::ptr_eq(&a, &b));
        st.inputs.n = 7;
        let c = result_for(&mut st, &p, &m);
        assert!(!Arc::ptr_eq(&a, &c));
        assert_eq!(c.positions.len(), 7);
    }
}
