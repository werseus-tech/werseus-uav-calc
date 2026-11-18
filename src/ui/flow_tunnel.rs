// SPDX-License-Identifier: AGPL-3.0-or-later
//! Canlı rüzgâr tüneli ekranı: tasarımın seçilen 2D kesiti etrafındaki akışı (LBM) canlı
//! gösterir. Görüntü NİTELDİR; yanındaki sayılar uygulamanın mevcut formüllerinden ve panel +
//! VLM kodundan gelir (bu animasyondan kuvvet üretilmez).
//!
//! Çözücü arka plan iş parçacığında çalışır (`windtunnel::flowsim`); bu dosya yalnız durum
//! gönderir, son anlık görüntüyü alır, renklendirir ve izleyici parçacıkları/akım çizgilerini
//! çizer. Hiçbir çağrı çözücüyü beklemez. Renkler tema tokenlarından türetilir.
use super::components::{self, Status, section_header};
use super::theme::{self, Tokens, space};
use super::widgets::{fmt, section};
use super::windtunnel::{self as wt, WtState};
use crate::calc::Model;
use crate::geom3d::{ViewOptions, from_project};
use crate::i18n::{t, tf};
use crate::project::{DroneType, Project};
use crate::windtunnel::flowsim::{
    AIRSPEED_MAX, AIRSPEED_MIN, FlowSim, Quality, SimSetup, Snapshot, default_station,
};
use crate::windtunnel::lbm::{POROUS, SOLID};
use crate::windtunnel::section::{ALPHA_MAX, ALPHA_MIN, CutKind, CutParams, SectionOf};
use egui::{Color32, Pos2, Rect, Stroke, Ui, vec2};
use std::sync::Arc;

/// Renklendirilen alan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VizMode {
    Speed,
    Vorticity,
    Pressure,
}

/// Akışın üstüne çizilen çizgiler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overlay {
    None,
    Streamlines,
    Smoke,
}

/// Görüntü dokusu için çarpımsal ton (değiştirmez).
const NEUTRAL_TINT: Color32 = Color32::from_rgba_premultiplied(255, 255, 255, 255);

/// Parçacık üst sınırı (çizim maliyetini sınırlar).
const MAX_PARTICLES: usize = 5000;
const SMOKE_SEEDS: usize = 26;
const STREAM_SEEDS: usize = 30;

pub struct FlowUiState {
    sim: Option<FlowSim>,
    start_error: bool,
    cut: CutKind,
    stations: [f64; 4],
    alpha: f64,
    airspeed: Option<f64>,
    quality: Quality,
    mode: VizMode,
    overlay: Overlay,
    last_sent: Option<SimSetup>,
    snap: Option<Arc<Snapshot>>,
    tex: Option<egui::TextureHandle>,
    tex_key: Option<(u64, VizMode, bool)>,
    particles: Vec<[f32; 2]>,
    part_step: u64,
    emit_step: u64,
    streams: Vec<Vec<[f32; 2]>>,
    stream_seq: u64,
}

impl Default for FlowUiState {
    fn default() -> Self {
        FlowUiState {
            sim: None,
            start_error: false,
            cut: CutKind::Section,
            stations: CutKind::ALL.map(default_station),
            alpha: 4.0,
            airspeed: None,
            quality: Quality::Medium,
            mode: VizMode::Speed,
            overlay: Overlay::Smoke,
            last_sent: None,
            snap: None,
            tex: None,
            tex_key: None,
            particles: Vec::new(),
            part_step: 0,
            emit_step: 0,
            streams: Vec::new(),
            stream_seq: 0,
        }
    }
}

fn cut_index(k: CutKind) -> usize {
    CutKind::ALL.iter().position(|c| *c == k).unwrap_or(0)
}

fn cut_label(k: CutKind, dt: DroneType) -> &'static str {
    match k {
        CutKind::Section if dt == DroneType::Multirotor => t("ft.cut_arm"),
        CutKind::Section => t("ft.cut_section"),
        CutKind::Side => t("ft.cut_side"),
        CutKind::Top => t("ft.cut_top"),
        CutKind::Front => t("ft.cut_front"),
    }
}

fn project_airspeed(p: &Project) -> f64 {
    let v = if p.drone_type == DroneType::Multirotor {
        p.mr_v_cruise
    } else {
        p.v_cr
    };
    if v.is_finite() && v > 0.0 {
        v.clamp(AIRSPEED_MIN, AIRSPEED_MAX)
    } else {
        15.0
    }
}

impl FlowUiState {
    fn ensure_sim(&mut self) {
        if self.sim.is_none() && !self.start_error {
            match FlowSim::start() {
                Ok(s) => self.sim = Some(s),
                Err(_) => self.start_error = true,
            }
        }
    }

    fn restart_sim(&mut self) {
        self.sim = None;
        self.start_error = false;
        self.last_sent = None;
        self.snap = None;
        self.particles.clear();
        self.streams.clear();
        self.ensure_sim();
    }

    fn setup(&self, p: &Project, m: &Model, opts: &ViewOptions) -> SimSetup {
        let spec = from_project(p, m, opts);
        // Tasarım değişmediyse aynı Arc'ı kullan (karşılaştırma ucuz, kopya yok).
        let spec = match &self.last_sent {
            Some(s) if *s.spec == spec => s.spec.clone(),
            _ => Arc::new(spec),
        };
        SimSetup {
            spec,
            cut: CutParams {
                kind: self.cut,
                station: self.stations[cut_index(self.cut)],
                alpha_deg: self.alpha,
            }
            .sanitized(),
            quality: self.quality,
            airspeed: self.airspeed.unwrap_or_else(|| project_airspeed(p)),
        }
    }
}

/// Ekran. `opts`: 3D görünümün seçenekleri (aynı uçak görünsün diye); `wts`: panel + VLM
/// sonuçları (mevcut rüzgâr tüneli durumu).
pub fn show(
    ui: &mut Ui,
    p: &Project,
    m: &Model,
    opts: &ViewOptions,
    st: &mut FlowUiState,
    wts: &mut WtState,
) {
    wt::poll_results(wts);
    st.ensure_sim();
    let tk = theme::tokens(ui);
    section_header(ui, t("tab_flowtunnel"));
    components::banner(ui, Status::Info, t("ft.qualitative"));
    ui.add_space(space::XS);

    if st.start_error {
        components::banner(ui, Status::Err, t("ft.err_thread"));
        return;
    }
    let alive = st.sim.as_ref().is_some_and(|s| s.alive());
    if !alive {
        components::banner(ui, Status::Err, t("ft.err_dead"));
        if ui.button(t("ft.restart")).clicked() {
            st.restart_sim();
        }
        return;
    }

    controls(ui, p, st);

    // Durumu gönder (yalnız değiştiyse) ve son görüntüyü al.
    let setup = st.setup(p, m, opts);
    if st.last_sent.as_ref() != Some(&setup) {
        if let Some(sim) = &st.sim {
            sim.set_setup(setup.clone());
        }
        st.last_sent = Some(setup);
    }
    let (running, busy) = match &st.sim {
        Some(sim) => {
            sim.touch();
            if let Some(s) = sim.latest() {
                if st.snap.as_ref().is_none_or(|o| o.seq != s.seq) {
                    st.snap = Some(s);
                }
            }
            (sim.is_running(), sim.is_busy())
        }
        None => (false, false),
    };
    if running || busy {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(33));
    }

    ui.add_space(space::XS);
    let avail = ui.available_width();
    let side_w = 290.0;
    if avail > 760.0 {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(avail - side_w - space::M);
                picture(ui, st, tk, busy);
            });
            ui.add_space(space::S);
            ui.vertical(|ui| {
                ui.set_width(side_w);
                numbers(ui, p, m, st, wts);
            });
        });
    } else {
        picture(ui, st, tk, busy);
        ui.add_space(space::S);
        numbers(ui, p, m, st, wts);
    }
}

fn controls(ui: &mut Ui, p: &Project, st: &mut FlowUiState) {
    let dt = p.drone_type;
    ui.horizontal_wrapped(|ui| {
        ui.label(t("ft.cut"));
        for k in CutKind::ALL {
            if ui.selectable_label(st.cut == k, cut_label(k, dt)).clicked() && st.cut != k {
                st.cut = k;
                st.particles.clear();
            }
        }
    });
    ui.horizontal_wrapped(|ui| {
        let i = cut_index(st.cut);
        let (label, lo, hi) = match st.cut {
            CutKind::Section if dt == DroneType::Multirotor => (t("ft.station_arm"), 0.05, 0.95),
            CutKind::Section => (t("ft.station_section"), 0.0, 0.95),
            _ => (t("ft.station_offset"), -0.95, 0.95),
        };
        ui.label(label);
        ui.add(egui::Slider::new(&mut st.stations[i], lo..=hi).fixed_decimals(2));
        ui.separator();
        let a_label = match st.cut {
            CutKind::Top => t("ft.alpha_top"),
            CutKind::Front => t("ft.alpha_front"),
            _ => t("ft.alpha"),
        };
        ui.label(a_label);
        ui.add(
            egui::Slider::new(&mut st.alpha, ALPHA_MIN..=ALPHA_MAX)
                .step_by(0.5)
                .suffix("°"),
        );
    });
    ui.horizontal_wrapped(|ui| {
        ui.label(t("ft.airspeed"));
        let mut v = st.airspeed.unwrap_or_else(|| project_airspeed(p));
        if ui
            .add(
                egui::Slider::new(&mut v, AIRSPEED_MIN..=AIRSPEED_MAX)
                    .step_by(0.5)
                    .suffix(" m/s"),
            )
            .changed()
        {
            st.airspeed = Some(v);
        }
        ui.separator();
        ui.label(t("ft.quality"));
        egui::ComboBox::from_id_salt("ft_quality")
            .selected_text(quality_label(st.quality))
            .show_ui(ui, |ui| {
                for q in Quality::ALL {
                    ui.selectable_value(&mut st.quality, q, quality_label(q));
                }
            });
    });
    ui.horizontal_wrapped(|ui| {
        ui.label(t("ft.mode"));
        ui.selectable_value(&mut st.mode, VizMode::Speed, t("ft.mode_speed"));
        ui.selectable_value(&mut st.mode, VizMode::Vorticity, t("ft.mode_vort"));
        ui.selectable_value(&mut st.mode, VizMode::Pressure, t("ft.mode_press"));
        ui.separator();
        ui.label(t("ft.overlay"));
        ui.selectable_value(&mut st.overlay, Overlay::None, t("ft.ov_none"));
        ui.selectable_value(&mut st.overlay, Overlay::Streamlines, t("ft.ov_stream"));
        ui.selectable_value(&mut st.overlay, Overlay::Smoke, t("ft.ov_smoke"));
        ui.separator();
        if let Some(sim) = &st.sim {
            let running = sim.is_running();
            let label = if running { t("ft.pause") } else { t("ft.play") };
            if ui.button(label).clicked() {
                sim.set_running(!running);
            }
            if ui.button(t("ft.reset")).clicked() {
                sim.reset();
                st.particles.clear();
            }
        }
    });
}

fn quality_label(q: Quality) -> &'static str {
    match q {
        Quality::Low => t("ft.q_low"),
        Quality::Medium => t("ft.q_med"),
        Quality::High => t("ft.q_high"),
    }
}

// ---------------------------------------------------------------------------------------------
// Renklendirme (tema tokenlarından)
// ---------------------------------------------------------------------------------------------

fn lerp_c(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    // Opak renk: premultiplied ile aynı.
    Color32::from_rgba_premultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), 255)
}

/// Sıralı renk ölçeği (yavaş → hızlı).
fn seq_color(tk: &Tokens, v: f32) -> Color32 {
    let stops = [tk.viewport_bg, tk.info, tk.success, tk.warning, tk.danger];
    let x = v.clamp(0.0, 1.0) * (stops.len() - 1) as f32;
    let i = (x.floor() as usize).min(stops.len() - 2);
    lerp_c(stops[i], stops[i + 1], x - i as f32)
}

/// Iraksak ölçek: −1 → `neg`, 0 → zemin, +1 → `pos`.
fn div_color(tk: &Tokens, v: f32, neg: Color32, pos: Color32) -> Color32 {
    if v < 0.0 {
        lerp_c(tk.viewport_bg, neg, -v)
    } else {
        lerp_c(tk.viewport_bg, pos, v)
    }
}

/// Anlık görüntüden RGBA görüntü (y yukarı → satır aşağı).
pub fn colorize(s: &Snapshot, mode: VizMode, tk: &Tokens) -> egui::ColorImage {
    let (nx, ny) = (s.nx, s.ny);
    let u = s.u_lat.max(1e-6) as f32;
    let body = tk.border_strong;
    let porous_tint = tk.text_weak;
    let l_lat = s.units.l_lat.max(1.0) as f32;
    let mut px = vec![Color32::TRANSPARENT; nx * ny];
    let at = |x: usize, y: usize| y * nx + x;
    for y in 0..ny {
        for x in 0..nx {
            let k = at(x, y);
            let c = s.cells[k];
            let col = if c == SOLID {
                body
            } else {
                let v = match mode {
                    VizMode::Speed => {
                        let sp = (s.ux[k] * s.ux[k] + s.uy[k] * s.uy[k]).sqrt() / u;
                        seq_color(tk, sp / 1.6)
                    }
                    VizMode::Vorticity => {
                        let (xm, xp) = (x.saturating_sub(1), (x + 1).min(nx - 1));
                        let (ym, yp) = (y.saturating_sub(1), (y + 1).min(ny - 1));
                        let dvdx = (s.uy[at(xp, y)] - s.uy[at(xm, y)]) / (xp - xm).max(1) as f32;
                        let dudy = (s.ux[at(x, yp)] - s.ux[at(x, ym)]) / (yp - ym).max(1) as f32;
                        // Boyutsuz girdap: ω·L/U; ±12 tam doygunluk.
                        let w = (dvdx - dudy) * l_lat / u / 12.0;
                        div_color(tk, w.clamp(-1.0, 1.0), tk.info, tk.danger)
                    }
                    VizMode::Pressure => {
                        // Cp = (p − p∞)/(½ρU²), p = ρ c_s².
                        let cp = (s.rho[k] - 1.0) / 3.0 / (0.5 * u * u);
                        let v = if cp < 0.0 { cp / 2.0 } else { cp };
                        div_color(tk, v.clamp(-1.0, 1.0), tk.accent, tk.warning)
                    }
                };
                if c == POROUS {
                    lerp_c(v, porous_tint, 0.45)
                } else {
                    v
                }
            };
            px[(ny - 1 - y) * nx + x] = col;
        }
    }
    egui::ColorImage::new([nx, ny], px)
}

/// Görüntüdeki hızın çift doğrusal aradeğerlemesi (kafes birimi). Katıda/dışarıda `None`.
fn sample(s: &Snapshot, x: f32, y: f32) -> Option<(f32, f32)> {
    let (nx, ny) = (s.nx, s.ny);
    if !(x >= 0.0 && y >= 0.0 && x <= (nx - 1) as f32 && y <= (ny - 1) as f32) {
        return None;
    }
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(nx - 1), (y0 + 1).min(ny - 1));
    if s.cells[y0 * nx + x0] == SOLID {
        return None;
    }
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let bl = |v: &[f32]| {
        let a = v[y0 * nx + x0] * (1.0 - fx) + v[y0 * nx + x1] * fx;
        let b = v[y1 * nx + x0] * (1.0 - fx) + v[y1 * nx + x1] * fx;
        a * (1.0 - fy) + b * fy
    };
    Some((bl(&s.ux), bl(&s.uy)))
}

/// Akım çizgileri: girişteki tohumlardan anlık hız alanında RK2 ile.
pub fn streamlines(s: &Snapshot) -> Vec<Vec<[f32; 2]>> {
    let mut out = Vec::with_capacity(STREAM_SEEDS);
    let h = 0.8f32;
    let max_n = 3 * s.nx;
    for k in 0..STREAM_SEEDS {
        let y0 = (k as f32 + 0.5) / STREAM_SEEDS as f32 * (s.ny - 1) as f32;
        let mut p = [1.0f32, y0];
        let mut line = vec![p];
        for n in 0..max_n {
            let dir = |q: [f32; 2]| -> Option<[f32; 2]> {
                let (u, v) = sample(s, q[0], q[1])?;
                let n = (u * u + v * v).sqrt();
                (n > 1e-7).then(|| [u / n, v / n])
            };
            let Some(d1) = dir(p) else { break };
            let mid = [p[0] + 0.5 * h * d1[0], p[1] + 0.5 * h * d1[1]];
            let Some(d2) = dir(mid) else { break };
            p = [p[0] + h * d2[0], p[1] + h * d2[1]];
            if n % 3 == 2 {
                line.push(p);
            }
            if p[0] >= (s.nx - 2) as f32 {
                break;
            }
        }
        line.push(p);
        if line.len() > 2 {
            out.push(line);
        }
    }
    out
}

/// Duman parçacıklarını yeni görüntüye göre ilerletir ve girişten yenilerini salar.
pub fn advance_particles(
    parts: &mut Vec<[f32; 2]>,
    part_step: &mut u64,
    emit_step: &mut u64,
    s: &Snapshot,
) {
    if s.step < *part_step
        || parts
            .iter()
            .any(|p| !(p[0].is_finite() && p[1].is_finite()))
    {
        parts.clear();
        *part_step = s.step;
        *emit_step = s.step;
    }
    let d = (s.step - *part_step).min(400) as f32;
    *part_step = s.step;
    if d > 0.0 {
        let u = s.u_lat.max(1e-4) as f32;
        let nsub = ((d * u * 1.8) / 0.7).ceil().clamp(1.0, 60.0) as usize;
        let h = d / nsub as f32;
        parts.retain_mut(|p| {
            for _ in 0..nsub {
                let Some((vx, vy)) = sample(s, p[0], p[1]) else {
                    return false;
                };
                p[0] += vx * h;
                p[1] += vy * h;
            }
            p[0] < (s.nx - 2) as f32 && p[1] > 0.0 && p[1] < (s.ny - 1) as f32
        });
    }
    // Yeni parçacıklar: ~1.4 hücre aralıkla (duman çizgisi gibi görünür).
    let spacing = (1.4 / s.u_lat.max(1e-4)) as u64;
    let spacing = spacing.max(1);
    let mut n = s.step.saturating_sub(*emit_step) / spacing;
    if n > 0 {
        *emit_step += n * spacing;
        n = n.min(4);
        for m in 0..n {
            // Aynı karede birikmiş gruplar tek noktada üst üste binmesin: eski grup akış
            // yönünde ~1.4 hücre ilerlemiş sayılır.
            let x = 1.5 + (n - 1 - m) as f32 * 1.4;
            for k in 0..SMOKE_SEEDS {
                if parts.len() >= MAX_PARTICLES {
                    break;
                }
                let y = (k as f32 + 0.5) / SMOKE_SEEDS as f32 * (s.ny - 1) as f32;
                parts.push([x, y]);
            }
        }
    }
}

fn picture(ui: &mut Ui, st: &mut FlowUiState, tk: &Tokens, busy: bool) {
    let Some(s) = st.snap.clone() else {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(t("ft.starting"));
        });
        return;
    };
    // Doku yalnız yeni görüntü/mod/tema değişince yeniden üretilir.
    let key = (s.seq, st.mode, tk.dark);
    if st.tex_key != Some(key) || st.tex.is_none() {
        let img = colorize(&s, st.mode, tk);
        match &mut st.tex {
            Some(h) => h.set(img, egui::TextureOptions::LINEAR),
            None => {
                st.tex = Some(ui.ctx().load_texture(
                    "flow_tunnel_field",
                    img,
                    egui::TextureOptions::LINEAR,
                ));
            }
        }
        st.tex_key = Some(key);
    }
    match st.overlay {
        Overlay::Smoke => {
            advance_particles(&mut st.particles, &mut st.part_step, &mut st.emit_step, &s)
        }
        Overlay::Streamlines if st.stream_seq != s.seq => {
            st.streams = streamlines(&s);
            st.stream_seq = s.seq;
        }
        _ => {}
    }
    let w = ui.available_width().max(160.0);
    let h = (w * s.ny as f32 / s.nx as f32).min(560.0);
    let w = h * s.nx as f32 / s.ny as f32;
    let (rect, _) = ui.allocate_exact_size(vec2(w, h), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    if let Some(tex) = &st.tex {
        painter.image(
            tex.id(),
            rect,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            // Nötr ton: doku kendi (tokenlardan türetilmiş) renkleriyle çizilir.
            NEUTRAL_TINT,
        );
    }
    let to_screen = |x: f32, y: f32| {
        Pos2::new(
            rect.left() + x / s.nx as f32 * rect.width(),
            rect.bottom() - y / s.ny as f32 * rect.height(),
        )
    };
    match st.overlay {
        Overlay::Streamlines => {
            let stroke = Stroke::new(1.0, tk.text.gamma_multiply(0.7));
            for l in &st.streams {
                let pts: Vec<Pos2> = l.iter().map(|q| to_screen(q[0], q[1])).collect();
                painter.add(egui::Shape::line(pts, stroke));
            }
        }
        Overlay::Smoke => {
            let c = tk.text.gamma_multiply(0.75);
            let r = (rect.width() / s.nx as f32 * 0.55).clamp(0.8, 2.0);
            for q in &st.particles {
                painter.circle_filled(to_screen(q[0], q[1]), r, c);
            }
        }
        Overlay::None => {}
    }
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0, tk.border),
        egui::StrokeKind::Inside,
    );
    // Akış yönü oku (sol üst).
    let a0 = rect.left_top() + vec2(10.0, 12.0);
    painter.arrow(a0, vec2(28.0, 0.0), Stroke::new(1.5, tk.text_weak));
    if busy {
        painter.text(
            rect.right_top() + vec2(-8.0, 8.0),
            egui::Align2::RIGHT_TOP,
            t("ft.rebuilding"),
            egui::FontId::proportional(12.0),
            tk.text_weak,
        );
    }
    ui.label(
        egui::RichText::new(match st.mode {
            VizMode::Speed => t("ft.legend_speed"),
            VizMode::Vorticity => t("ft.legend_vort"),
            VizMode::Pressure => t("ft.legend_press"),
        })
        .color(tk.text_weak),
    );
    if s.empty {
        ui.colored_label(tk.warning, t("ft.empty_cut"));
    }
    if s.recoveries > 0 {
        ui.colored_label(
            tk.warning,
            tf("ft.recovered", &[&format!("{}", s.recoveries)]),
        );
    }
}

fn grid_row(ui: &mut Ui, label: &str, value: String) {
    ui.label(label);
    ui.label(value);
    ui.end_row();
}

fn opt(v: f64, dec: usize, unit: &str) -> String {
    if v.is_finite() {
        format!("{} {unit}", fmt(v, dec)).trim_end().to_string()
    } else {
        "—".into()
    }
}

fn numbers(ui: &mut Ui, p: &Project, m: &Model, st: &FlowUiState, wts: &mut WtState) {
    let tk = theme::tokens(ui);
    section(ui, t("ft.numbers"));
    if p.drone_type.has_wing() {
        ui.label(egui::RichText::new(t("ft.formula_hdr")).strong());
        egui::Grid::new("ft_formula").num_columns(2).show(ui, |ui| {
            grid_row(ui, t("ft.row_cla"), opt(m.aero.cla_w, 2, "/rad"));
            grid_row(ui, t("ft.row_clmax"), opt(m.aero.cl_max, 2, ""));
            grid_row(ui, t("ft.row_cd0"), opt(m.aero.cd0, 4, ""));
            grid_row(ui, t("ft.row_ld"), opt(m.aero.ld_max, 1, ""));
            grid_row(ui, t("ft.row_e"), opt(m.aero.e, 2, ""));
            grid_row(ui, t("ft.row_vstall"), opt(m.aero.v_stall, 1, "m/s"));
        });
        ui.add_space(space::S);
        // α yalnız yan ve kanat kesitinde hücum açısıdır; üst/ön kesitte yana kayma / yatış.
        let alpha_is_aoa = matches!(st.cut, CutKind::Side | CutKind::Section);
        let hdr = if alpha_is_aoa {
            tf("ft.panel_hdr", &[&fmt(st.alpha, 1)])
        } else {
            t("ft.panel_hdr_plain").to_string()
        };
        ui.label(egui::RichText::new(hdr).strong());
        if wt::is_computing(wts) {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(t("ft.panel_running"));
            });
        } else if let Some((cla, np)) = wt::vlm_summary(wts) {
            egui::Grid::new("ft_panel").num_columns(2).show(ui, |ui| {
                match alpha_is_aoa.then(|| wt::polar_at(wts, st.alpha)).flatten() {
                    Some((cl, cd, cm)) => {
                        grid_row(ui, t("ft.row_cl2d"), opt(cl, 3, ""));
                        grid_row(ui, t("ft.row_cd2d"), opt(cd, 4, ""));
                        grid_row(ui, t("ft.row_cm2d"), opt(cm, 3, ""));
                    }
                    None => {
                        let key = if alpha_is_aoa {
                            "ft.alpha_outside"
                        } else {
                            "ft.polar_needs_aoa"
                        };
                        ui.label(egui::RichText::new(t(key)).color(tk.text_weak));
                        ui.end_row();
                    }
                }
                if let Some((a, cl)) = wt::stall(wts) {
                    grid_row(
                        ui,
                        t("ft.row_stall"),
                        format!("{}° / cl {}", fmt(a, 1), fmt(cl, 2)),
                    );
                }
                grid_row(ui, t("ft.row_cla_vlm"), opt(cla, 2, "/rad"));
                grid_row(
                    ui,
                    t("ft.row_np"),
                    np.map(|x| opt(x, 3, "m")).unwrap_or_else(|| "—".into()),
                );
            });
            if ui.small_button(t("ft.run_panel")).clicked() {
                wt::start_compute(wts, p, m, ui.ctx());
            }
        } else {
            ui.label(egui::RichText::new(t("ft.panel_none")).color(tk.text_weak));
            if ui.button(t("ft.run_panel")).clicked() {
                wt::start_compute(wts, p, m, ui.ctx());
            }
        }
        if let Some(e) = wt::last_error(wts) {
            ui.colored_label(tk.danger, e);
        }
    } else {
        ui.label(egui::RichText::new(t("ft.mr_hdr")).strong());
        egui::Grid::new("ft_mr").num_columns(2).show(ui, |ui| {
            grid_row(ui, t("ft.row_fpa"), opt(p.mr_flat_plate_area, 4, "m²"));
            grid_row(ui, t("ft.row_vcr"), opt(p.mr_v_cruise, 1, "m/s"));
            grid_row(ui, t("ft.row_pcr"), opt(m.mr.p_total_cruise, 0, "W"));
            grid_row(ui, t("ft.row_phover"), opt(m.mr.p_total_hover, 0, "W"));
        });
    }

    ui.add_space(space::S);
    section(ui, t("ft.sim_hdr"));
    if let Some(s) = &st.snap {
        let u = s.units;
        ui.label(tf(
            "ft.re_note",
            &[&format!("{:.2e}", u.re_phys), &format!("{:.0}", u.re_sim)],
        ));
        ui.label(
            egui::RichText::new(tf(
                "ft.sim_info",
                &[
                    &format!("{}", s.nx),
                    &format!("{}", s.ny),
                    &format!("{:.0}", s.steps_per_sec),
                    &format!("{:.4}", u.tau),
                ],
            ))
            .color(tk.text_weak),
        );
        ui.label(
            egui::RichText::new(tf(
                "ft.sim_time",
                &[
                    &format!("{:.3}", s.step as f64 * u.dt),
                    &format!("{:.1}", u.dx * 1000.0),
                ],
            ))
            .color(tk.text_weak),
        );
        if s.cut.kind != CutKind::Section || s.section_of != SectionOf::Wing {
            ui.label(
                egui::RichText::new(tf("ft.offset", &[&fmt(s.offset_m, 3)])).color(tk.text_weak),
            );
        }
        match s.section_of {
            SectionOf::Arm if s.cut.kind == CutKind::Section => {
                ui.label(t("ft.note_arm"));
            }
            SectionOf::Fallback if s.cut.kind == CutKind::Section => {
                ui.label(t("ft.note_fallback"));
            }
            _ => {}
        }
    }
    match st.cut {
        CutKind::Front => {
            ui.label(t("ft.note_front"));
        }
        CutKind::Top => {
            ui.label(t("ft.note_top"));
        }
        _ => {}
    }
    ui.add_space(space::XS);
    ui.label(egui::RichText::new(t("ft.cut_2d")).color(tk.text_weak));
    ui.label(egui::RichText::new(t("ft.note_props")).color(tk.text_weak));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::ThemeChoice;
    use std::time::{Duration, Instant};

    /// Ekran her araç türünde, her kesitte, iki temada ve iki dilde başsız çizilir; çözücü
    /// arka planda görüntü üretir ve bir süre sonra resim + çizgiler çizilir (panik yok).
    #[test]
    fn renders_for_every_type_cut_theme_and_language() {
        for dt in DroneType::ALL {
            let mut p = Project::default();
            p.set_drone_type(dt);
            let m = crate::calc::evaluate(&p, &crate::calc::Library::embedded());
            let mut st = FlowUiState {
                quality: Quality::Low,
                ..Default::default()
            };
            let mut wts = WtState::default();
            let opts = ViewOptions::default();
            for (ci, cut) in CutKind::ALL.into_iter().enumerate() {
                st.cut = cut;
                st.overlay = [Overlay::Smoke, Overlay::Streamlines, Overlay::None][ci % 3];
                st.mode = [VizMode::Speed, VizMode::Vorticity, VizMode::Pressure][ci % 3];
                for choice in [ThemeChoice::Dark, ThemeChoice::Light] {
                    for lang in [crate::i18n::Lang::Tr, crate::i18n::Lang::En] {
                        let ctx = egui::Context::default();
                        theme::install(&ctx, choice);
                        // The texture belongs to the previous context: rebuild it in this one.
                        st.tex = None;
                        st.tex_key = None;
                        crate::i18n::with_lang(lang, || {
                            let t0 = Instant::now();
                            loop {
                                let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                                    egui::CentralPanel::default().show(ui, |ui| {
                                        show(ui, &p, &m, &opts, &mut st, &mut wts);
                                    });
                                });
                                out.textures_delta.clear();
                                let ready = st
                                    .snap
                                    .as_ref()
                                    .is_some_and(|s| s.cut.kind == cut && s.step > 5);
                                if ready || t0.elapsed() > Duration::from_secs(30) {
                                    break;
                                }
                                std::thread::sleep(Duration::from_millis(15));
                            }
                        });
                    }
                }
                let s = st.snap.as_ref().expect("görüntü yok");
                assert_eq!(s.cut.kind, cut, "{dt:?}");
            }
        }
    }

    #[test]
    fn colorize_and_overlays_handle_all_modes() {
        let p = Project::default();
        let m = crate::calc::evaluate(&p, &crate::calc::Library::embedded());
        let mut eng = crate::windtunnel::flowsim::Engine::new(2);
        eng.apply(SimSetup {
            spec: Arc::new(from_project(&p, &m, &ViewOptions::default())),
            cut: CutParams::default(),
            quality: Quality::Low,
            airspeed: 20.0,
        });
        eng.run(300);
        let s = eng.snapshot(1, 0.0).unwrap();
        for tk in [&theme::DARK, &theme::LIGHT] {
            for mode in [VizMode::Speed, VizMode::Vorticity, VizMode::Pressure] {
                let img = colorize(&s, mode, tk);
                assert_eq!(img.size, [s.nx, s.ny]);
            }
        }
        let lines = streamlines(&s);
        assert!(lines.len() > STREAM_SEEDS / 2);
        let mut parts = Vec::new();
        let (mut a, mut b) = (0, 0);
        advance_particles(&mut parts, &mut a, &mut b, &s);
        // Several emission intervals elapsed at once: the groups must not stack on one x.
        let spacing = ((1.4 / s.u_lat.max(1e-4)) as u64).max(1);
        let mut stacked = Vec::new();
        let (mut a2, mut b2) = (s.step, s.step.saturating_sub(4 * spacing));
        advance_particles(&mut stacked, &mut a2, &mut b2, &s);
        let mut xs: Vec<i32> = stacked.iter().map(|q| (q[0] * 100.0) as i32).collect();
        xs.sort_unstable();
        xs.dedup();
        assert!(xs.len() >= 2 || s.step < 4 * spacing, "{xs:?}");
        assert!(parts.len() <= MAX_PARTICLES);
        assert!(parts.iter().all(|q| q[0].is_finite() && q[1].is_finite()));
    }
}
