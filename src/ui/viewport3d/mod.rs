// SPDX-License-Identifier: AGPL-3.0-or-later
//! Merkez görünüm: aracın canlı 3D modeli (CAD tarzı kamera), 2D şemaya geçiş ve (adım
//! sağlıyorsa) adıma özgü grafik (kısıt diyagramı, polar, güç eğrisi, kütle payları...).
//!
//! - Sahne `geom3d` ile parametrelerden üretilir; proje değişince (yalnız o zaman) yenilenir.
//! - Çizim `gl.rs` içindeki küçük gölgelendiriciyle `egui::PaintCallback` üzerinden yapılır.
//! - Kamera matematiği `camera.rs` içinde saf ve testlidir; burada yalnız fare/dokunmatik
//!   yüzey girdisi kameraya bağlanır.
//! - GL kullanılamazsa (eski sürücü, gölgelendirici hatası) bir sonraki karede 2D şemaya
//!   sessizce düşülür.
pub mod camera;
pub mod gl;

use crate::calc::Model;
use crate::geom3d::{self, AircraftSpec, PartKind, Scene, Vec3, ViewOptions};
use crate::i18n::t;
use crate::project::{DroneType, Project};
use crate::ui::components::format::{fmt_length_m, fmt_mass_kg, fmt_percent};
use crate::ui::components::{segmented, toggle_chip, tool_button};
use crate::ui::theme::{self, Tokens, font, radius, space};
use camera::{OrbitCamera, Preset, Projection};
use egui::{Color32, Pos2, Rect, RichText, Sense, Stroke, Ui, vec2};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Görünüm türü.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMode {
    Model3d,
    Schematic2d,
    /// Etkin adımın grafiği (adımın grafiği yoksa 3D model gösterilir; seçim korunur).
    Chart,
    /// Etkin adımın formülleri ve anlık değerlerle hesapları.
    Formulas,
}

/// Merkez görünüme adımın sağladığı grafik: seçici etiketi ve çizim fonksiyonu
/// (`draw(ui, yükseklik)`).
pub struct ChartSpec<'a> {
    pub label: &'a str,
    pub draw: &'a mut dyn FnMut(&mut Ui, f32),
}

pub struct ViewportState {
    pub mode: ViewMode,
    pub cam: OrbitCamera,
    goal: Option<OrbitCamera>,
    pub show_cg: bool,
    pub show_props: bool,
    pub show_grid: bool,
    pub opts: ViewOptions,
    scene: Arc<Scene>,
    scene_version: u64,
    last_spec: Option<AircraftSpec>,
    last_type: Option<DroneType>,
    needs_fit: bool,
    grid: Arc<gl::GridData>,
    grid_key: Option<(i32, i32, i32, i32)>,
    renderer: Arc<Mutex<Option<gl::Renderer>>>,
    gl_failed: Arc<AtomicBool>,
    /// GL çizimi en az bir kez başarılı oldu mu (yalnız tanı amaçlı).
    gl_ok: Arc<AtomicBool>,
}

impl Default for ViewportState {
    fn default() -> Self {
        Self {
            mode: ViewMode::Model3d,
            cam: OrbitCamera::default(),
            goal: None,
            show_cg: true,
            show_props: true,
            show_grid: true,
            opts: ViewOptions::default(),
            scene: Arc::new(Scene::default()),
            scene_version: 0,
            last_spec: None,
            last_type: None,
            needs_fit: true,
            grid: Arc::new(gl::GridData::default()),
            grid_key: None,
            renderer: Arc::new(Mutex::new(None)),
            gl_failed: Arc::new(AtomicBool::new(false)),
            gl_ok: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl ViewportState {
    pub fn gl_failed(&self) -> bool {
        self.gl_failed.load(Ordering::Relaxed)
    }

    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    /// Proje değiştiyse sahneyi yeniden üretir. Tür değişince kamera yeniden sığdırılır.
    pub fn update_scene(&mut self, p: &Project, m: &Model) {
        let spec = geom3d::from_project(p, m, &self.opts);
        if self.last_spec.as_ref() != Some(&spec) {
            let scene = geom3d::build(&spec);
            self.cam.scene_radius = scene.radius();
            self.cam.sanitize();
            self.scene = Arc::new(scene);
            self.scene_version = self.scene_version.wrapping_add(1);
            self.last_spec = Some(spec);
        }
        if self.last_type != Some(p.drone_type) {
            self.last_type = Some(p.drone_type);
            self.needs_fit = true;
        }
    }

    fn fitted(&self, base: OrbitCamera, aspect: f32) -> OrbitCamera {
        let mut c = base;
        c.fit(self.scene.center(), self.scene.radius(), aspect);
        c
    }

    /// Hazır görünüme yumuşak geçiş (sığdırarak).
    pub fn go_preset(&mut self, p: Preset, aspect: f32) {
        let base = self.cam.with_preset(p);
        self.goal = Some(self.fitted(base, aspect));
    }

    pub fn fit(&mut self, aspect: f32) {
        self.goal = Some(self.fitted(self.cam, aspect));
    }

    /// Kameranın şu an hangi hazır görünümde olduğu (yaklaşık), araç çubuğu vurgusu için.
    pub fn current_preset(&self) -> Option<Preset> {
        let c = self.goal.unwrap_or(self.cam);
        Preset::ALL.into_iter().find(|p| {
            let (y, pt) = p.angles();
            camera::wrap_angle(c.yaw - y).abs() < 0.02 && (c.pitch - pt).abs() < 0.02
        })
    }

    fn ensure_grid(&mut self) {
        let s = &self.scene;
        let r = s.radius();
        let half = r * 2.2;
        let step = geom3d::grid::nice_step(half * 2.0, 24);
        let c = s.center();
        let z0 = s.bounds_min.z - r * 0.04;
        let key = (
            (c.x / step).round() as i32,
            (c.y / step).round() as i32,
            (half / step).round() as i32,
            (z0 * 1000.0).round() as i32,
        );
        if self.grid_key == Some(key) && !self.grid.minor.is_empty() {
            return;
        }
        let mut g = gl::GridData {
            version: self.grid.version.wrapping_add(1),
            ..Default::default()
        };
        for l in geom3d::grid::grid_lines(c.x, c.y, half, step, z0) {
            let dst = if l.major { &mut g.major } else { &mut g.minor };
            dst.push(l.a);
            dst.push(l.b);
        }
        self.grid = Arc::new(g);
        self.grid_key = Some(key);
    }
}

/// Parça türü renkleri (tema bazlı, sakin gri-mavi tonlar; motorlar sıcak vurgu).
pub fn part_colors(tk: &Tokens) -> Vec<(PartKind, [f32; 4])> {
    let c = |r, g, b| Color32::from_rgb(r, g, b);
    let (wing, tail, body, boom, motor) = if tk.dark {
        (
            c(158, 176, 202),
            c(138, 158, 186),
            c(118, 130, 148),
            c(96, 106, 122),
            c(214, 160, 72),
        )
    } else {
        (
            c(198, 210, 228),
            c(178, 194, 216),
            c(160, 170, 186),
            c(124, 134, 150),
            c(214, 150, 54),
        )
    };
    vec![
        (PartKind::Wing, gl::rgba(wing, 1.0)),
        (PartKind::Tail, gl::rgba(tail, 1.0)),
        (PartKind::Body, gl::rgba(body, 1.0)),
        (PartKind::Boom, gl::rgba(boom, 1.0)),
        (PartKind::Arm, gl::rgba(boom, 1.0)),
        (PartKind::Motor, gl::rgba(motor, 1.0)),
        (PartKind::Rotor, gl::rgba(tk.accent, 0.22)),
    ]
}

/// Köşe özeti: açıklık · kütle · statik marj (multirotorda: motorlar arası · kütle · askı gazı).
pub fn summary_text(p: &Project, m: &Model, scene: &Scene) -> String {
    if p.drone_type == DroneType::Multirotor {
        let size = fmt_length_m(p.mr_frame_size);
        format!(
            "{} {} · {} · {} {}",
            size,
            t("ui.view.frame"),
            fmt_mass_kg(m.mr.m_mtow),
            t("ui.view.hover"),
            fmt_percent(m.mr.hover_throttle, 0)
        )
    } else {
        let span = if p.b.is_finite() && p.b > 0.0 {
            p.b
        } else {
            (scene.bounds_max.y - scene.bounds_min.y) as f64
        };
        format!(
            "{} {} · {} · SM {}",
            fmt_length_m(span),
            t("ui.view.span"),
            fmt_mass_kg(m.weight.m_mtow),
            fmt_percent(m.stab.sm, 1)
        )
    }
}

/// Kamera tabanlı ışık yönü (sol-üst-önden, sahneyle dönmez: her açıda okunur).
fn light_dir(cam: &OrbitCamera) -> Vec3 {
    let (right, up) = cam.basis();
    (cam.back_dir() * 0.75 + up * 0.55 - right * 0.35).normalized()
}

/// Görünümü çizer (araç çubuğu + tuval). `chart`: adımın grafiği (varsa seçicide üçüncü seçenek).
pub fn show(
    ui: &mut Ui,
    st: &mut ViewportState,
    p: &Project,
    m: &Model,
    chart: Option<ChartSpec>,
    formulas: Option<&mut dyn FnMut(&mut Ui)>,
    toolbar_extra: impl FnOnce(&mut Ui),
) {
    let tk = theme::tokens(ui);
    st.update_scene(p, m);
    let failed = st.gl_failed();
    // Kullanıcı "Grafik"i seçtiyse ama bu adımın grafiği yoksa 3D gösterilir (seçim korunur;
    // grafiği olan adıma dönünce grafik yeniden görünür).
    let shown = match (st.mode, chart.is_some(), formulas.is_some()) {
        (ViewMode::Chart, false, _) | (ViewMode::Formulas, _, false) => ViewMode::Model3d,
        (mode, _, _) => mode,
    };
    let shown = if failed && shown == ViewMode::Model3d {
        ViewMode::Schematic2d
    } else {
        shown
    };

    // --- Araç çubuğu ---
    let mut preset_req: Option<Preset> = None;
    let mut fit_req = false;
    let avail_w = ui.available_width();
    let aspect_guess = (avail_w / ui.available_height().max(1.0)).max(0.2);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = space::S - 2.0;
        let mut mode = shown;
        let mut opts = vec![
            (
                ViewMode::Model3d,
                t("ui.view.mode_3d"),
                t("ui.view.mode_3d_tip"),
            ),
            (
                ViewMode::Schematic2d,
                t("ui.view.mode_2d"),
                t("ui.view.mode_2d_tip"),
            ),
        ];
        if let Some(c) = chart.as_ref() {
            opts.push((ViewMode::Chart, t("ui.view.mode_chart"), c.label));
        }
        if formulas.is_some() {
            opts.push((
                ViewMode::Formulas,
                t("ui.view.mode_formulas"),
                t("ui.view.mode_formulas_tip"),
            ));
        }
        if segmented(ui, &mut mode, &opts) && !(failed && mode == ViewMode::Model3d) {
            st.mode = mode;
        }
        if let (ViewMode::Chart, Some(c)) = (shown, chart.as_ref()) {
            ui.separator();
            ui.label(
                RichText::new(c.label)
                    .size(font::CAPTION)
                    .strong()
                    .color(tk.text),
            );
        }
        if shown == ViewMode::Model3d {
            ui.separator();
            let mut cur = st.current_preset();
            let before = cur;
            segmented(
                ui,
                &mut cur,
                &[
                    (Some(Preset::Iso), t("ui.view.iso"), t("ui.view.iso_tip")),
                    (Some(Preset::Top), t("ui.view.top"), ""),
                    (Some(Preset::Front), t("ui.view.front"), ""),
                    (Some(Preset::Side), t("ui.view.side"), ""),
                ],
            );
            if cur != before {
                preset_req = cur;
            }
            if tool_button(ui, t("ui.view.fit"), t("ui.view.fit_tip")).clicked() {
                fit_req = true;
            }
            let mut ortho = st.cam.projection == Projection::Orthographic;
            if toggle_chip(ui, &mut ortho, t("ui.view.ortho"), t("ui.view.ortho_tip")).changed() {
                let pr = if ortho {
                    Projection::Orthographic
                } else {
                    Projection::Perspective
                };
                st.cam.projection = pr;
                if let Some(g) = st.goal.as_mut() {
                    g.projection = pr;
                }
            }
            ui.separator();
            toggle_chip(ui, &mut st.show_cg, t("ui.view.cg"), t("ui.view.cg_tip"));
            toggle_chip(
                ui,
                &mut st.show_props,
                t("ui.view.props"),
                t("ui.view.props_tip"),
            );
            toggle_chip(ui, &mut st.show_grid, t("ui.view.grid"), "");
        }
        ui.separator();
        toolbar_extra(ui);
    });
    ui.add_space(space::XS);

    let (rect, resp) = ui.allocate_exact_size(
        vec2(ui.available_width(), ui.available_height().max(120.0)),
        Sense::click_and_drag(),
    );
    let aspect = if rect.height() > 1.0 {
        rect.width() / rect.height()
    } else {
        aspect_guess
    };

    if shown == ViewMode::Formulas {
        if let Some(f) = formulas {
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
            child.set_clip_rect(rect.intersect(ui.clip_rect()));
            egui::ScrollArea::vertical()
                .id_salt("view_formulas")
                .auto_shrink([false, false])
                .show(&mut child, |ui| f(ui));
        }
        return;
    }
    if shown == ViewMode::Chart {
        if let Some(c) = chart {
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
            child.set_clip_rect(rect.intersect(ui.clip_rect()));
            (c.draw)(&mut child, rect.height());
        }
        return;
    }
    if shown == ViewMode::Schematic2d {
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        crate::ui::schematic::show(&mut child, p, m, (rect.height() - 40.0).max(80.0));
        if failed {
            child.label(
                RichText::new(t("ui.view.gl_fallback"))
                    .size(font::CAPTION)
                    .color(tk.text_weak),
            );
        }
        return;
    }

    if st.needs_fit {
        st.cam = st.fitted(st.cam.with_preset(Preset::Iso), aspect);
        st.goal = None;
        st.needs_fit = false;
    }
    if let Some(pr) = preset_req {
        st.go_preset(pr, aspect);
    }
    if fit_req || resp.double_clicked() {
        st.fit(aspect);
    }

    // --- Girdi: sürükle / tekerlek / kıstırma ---
    let (primary, middle, secondary, shift) = ui.input(|i| {
        (
            i.pointer.button_down(egui::PointerButton::Primary),
            i.pointer.button_down(egui::PointerButton::Middle),
            i.pointer.button_down(egui::PointerButton::Secondary),
            i.modifiers.shift,
        )
    });
    if resp.dragged() {
        let d = resp.drag_delta();
        if d != egui::Vec2::ZERO {
            st.goal = None;
            match camera::drag_mode(primary, middle, secondary, shift) {
                Some(camera::DragMode::Orbit) => st.cam.orbit(d.x, d.y),
                Some(camera::DragMode::Pan) => st.cam.pan(d.x, d.y, rect.height()),
                None => {}
            }
        }
    }
    if resp.hovered() {
        let (scroll, zoom, hover) = ui.input(|i| {
            (
                i.smooth_scroll_delta.y,
                i.zoom_delta(),
                i.pointer.hover_pos(),
            )
        });
        let mut factor = 1.0_f32;
        if scroll.abs() > 0.0 {
            factor *= (-scroll * 0.0018).exp();
        }
        if (zoom - 1.0).abs() > 1e-4 && zoom > 0.0 {
            factor /= zoom;
        }
        if (factor - 1.0).abs() > 1e-5 {
            st.goal = None;
            let ndc = hover
                .map(|h| {
                    (
                        (h.x - rect.left()) / rect.width() * 2.0 - 1.0,
                        1.0 - (h.y - rect.top()) / rect.height() * 2.0,
                    )
                })
                .unwrap_or((0.0, 0.0));
            st.cam.zoom_at(factor, ndc, aspect);
        }
    }
    if let Some(goal) = st.goal {
        let dt = ui.input(|i| i.stable_dt);
        if camera::approach(&mut st.cam, &goal, dt) {
            st.goal = None;
        } else {
            ui.ctx().request_repaint();
        }
    }
    if resp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    }

    // --- GL çizimi ---
    if st.show_grid {
        st.ensure_grid();
    }
    let mut colors = part_colors(tk);
    if !st.show_props {
        colors.retain(|(k, _)| *k != PartKind::Rotor);
    }
    let vp = st.cam.view_proj(aspect);
    let eye = st.cam.eye();
    let ld = light_dir(&st.cam);
    let frame = gl::FrameData {
        view_proj: vp.to_cols_array(),
        eye: [eye.x, eye.y, eye.z],
        light_dir: [ld.x, ld.y, ld.z],
        clear: gl::rgba(tk.viewport_bg, 1.0),
        grid_minor: gl::rgba(tk.viewport_grid, 1.0),
        grid_major: gl::rgba(tk.viewport_grid_major, 1.0),
        colors,
        scene: st.scene.clone(),
        scene_version: st.scene_version,
        grid: if st.show_grid {
            st.grid.clone()
        } else {
            Arc::new(gl::GridData {
                version: u64::MAX - 1,
                ..Default::default()
            })
        },
    };
    let renderer = st.renderer.clone();
    let failed_flag = st.gl_failed.clone();
    let ok_flag = st.gl_ok.clone();
    let ctx = ui.ctx().clone();
    let cb = eframe::egui_glow::CallbackFn::new(move |info, painter| {
        if failed_flag.load(Ordering::Relaxed) {
            return;
        }
        let gl = painter.gl();
        let Ok(mut guard) = renderer.lock() else {
            failed_flag.store(true, Ordering::Relaxed);
            return;
        };
        if guard.is_none() {
            match gl::Renderer::new(gl) {
                Ok(r) => *guard = Some(r),
                Err(e) => {
                    crate::support::log_error(&format!("3D görünüm başlatılamadı: {e}"));
                    failed_flag.store(true, Ordering::Relaxed);
                    ctx.request_repaint();
                    return;
                }
            }
        }
        if let Some(r) = guard.as_mut() {
            match r.paint(gl, &info, painter.intermediate_fbo(), &frame) {
                Ok(()) => ok_flag.store(true, Ordering::Relaxed),
                Err(e) => {
                    // Önce MSAA'sız yeniden dene; o da olmazsa 2D şemaya düş.
                    gl_bind_out(gl, painter.intermediate_fbo());
                    if !r.disable_msaa(gl) {
                        crate::support::log_error(&format!("3D görünüm çizim hatası: {e}"));
                        failed_flag.store(true, Ordering::Relaxed);
                    }
                    ctx.request_repaint();
                }
            }
        }
    });
    let painter = ui.painter_at(rect);
    // GL yokken (ör. başsız test) zemin yine de dolu görünsün.
    painter.rect_filled(rect, theme::corner(radius::M), tk.viewport_bg);
    painter.add(egui::PaintCallback {
        rect,
        callback: Arc::new(cb),
    });

    // --- Üst katman: AG/NP işaretleri, eksen göstergesi, özet, ipucu ---
    let to_screen = |w: Vec3| -> Option<Pos2> {
        st.cam
            .project(w, aspect)
            .map(|(x, y)| {
                Pos2::new(
                    rect.left() + x * rect.width(),
                    rect.top() + y * rect.height(),
                )
            })
            .filter(|p| rect.expand(20.0).contains(*p))
    };
    if st.show_cg {
        if let Some(cg) = st.scene.cg.and_then(to_screen) {
            draw_cg(&painter, cg, tk);
            painter.text(
                cg + vec2(10.0, -10.0),
                egui::Align2::LEFT_BOTTOM,
                t("ui.view.cg_short"),
                egui::FontId::proportional(font::CAPTION),
                tk.text,
            );
        }
        if let Some(np) = st.scene.np.and_then(to_screen) {
            draw_np(&painter, np, tk);
            painter.text(
                np + vec2(10.0, 10.0),
                egui::Align2::LEFT_TOP,
                t("ui.view.np_short"),
                egui::FontId::proportional(font::CAPTION),
                tk.text_weak,
            );
        }
    }
    draw_axes(&painter, rect, &st.cam, tk);
    let summary = summary_text(p, m, &st.scene);
    let galley = painter.layout_no_wrap(summary, egui::FontId::proportional(font::BODY), tk.text);
    let chip = Rect::from_min_size(
        rect.left_top() + vec2(space::S, space::S),
        galley.size() + vec2(space::M * 2.0 - 4.0, 8.0),
    );
    painter.rect_filled(chip, theme::corner(radius::L), tk.surface);
    painter.rect_stroke(
        chip,
        theme::corner(radius::L),
        Stroke::new(1.0, tk.border),
        egui::StrokeKind::Inside,
    );
    painter.galley(chip.min + vec2(space::M - 2.0, 4.0), galley, tk.text);
    if rect.width() > 520.0 {
        painter.text(
            rect.right_bottom() - vec2(space::S, space::S),
            egui::Align2::RIGHT_BOTTOM,
            t("ui.view.hint"),
            egui::FontId::proportional(font::CAPTION),
            tk.text_weak,
        );
    }
    if st.scene.is_empty() {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            t("ui.view.empty"),
            egui::FontId::proportional(font::BODY),
            tk.text_weak,
        );
    }
}

/// Çerçeve tamponunu egui'nin çizdiği hedefe geri bağlar.
fn gl_bind_out(gl: &eframe::glow::Context, fbo: Option<eframe::glow::Framebuffer>) {
    use eframe::glow::HasContext;
    unsafe { gl.bind_framebuffer(eframe::glow::FRAMEBUFFER, fbo) };
}

/// Ağırlık merkezi simgesi: dörde bölünmüş daire (iki çeyrek dolu).
fn draw_cg(p: &egui::Painter, c: Pos2, tk: &Tokens) {
    let r = 7.0;
    let dark = if tk.dark {
        Color32::from_rgb(20, 22, 26)
    } else {
        Color32::from_rgb(30, 34, 42)
    };
    p.circle_filled(c, r + 1.5, Color32::WHITE);
    p.circle_filled(c, r, Color32::WHITE);
    for q in [0, 2] {
        let a0 = std::f32::consts::FRAC_PI_2 * q as f32;
        let mut pts = vec![c];
        for i in 0..=8 {
            let a = a0 + std::f32::consts::FRAC_PI_2 * i as f32 / 8.0;
            pts.push(c + vec2(a.cos() * r, -a.sin() * r));
        }
        p.add(egui::Shape::convex_polygon(pts, dark, Stroke::NONE));
    }
    p.circle_stroke(c, r, Stroke::new(1.2, dark));
}

/// Nötr nokta simgesi: içi boş daire + artı.
fn draw_np(p: &egui::Painter, c: Pos2, tk: &Tokens) {
    let col = tk.accent;
    p.circle_stroke(c, 6.0, Stroke::new(1.8, col));
    p.line_segment(
        [c - vec2(9.0, 0.0), c + vec2(9.0, 0.0)],
        Stroke::new(1.2, col),
    );
    p.line_segment(
        [c - vec2(0.0, 9.0), c + vec2(0.0, 9.0)],
        Stroke::new(1.2, col),
    );
}

/// Köşedeki X/Y/Z eksen göstergesi (kamera yönelimini gösterir).
fn draw_axes(p: &egui::Painter, rect: Rect, cam: &OrbitCamera, tk: &Tokens) {
    let origin = rect.left_bottom() + vec2(38.0, -38.0);
    let (right, up) = cam.basis();
    let fwd = -cam.back_dir();
    let len = 24.0;
    let mut axes = [
        (Vec3::X, "X", Color32::from_rgb(222, 76, 70)),
        (Vec3::Y, "Y", Color32::from_rgb(70, 170, 90)),
        (Vec3::Z, "Z", Color32::from_rgb(70, 120, 230)),
    ];
    // Arkadaki eksen önce çizilsin.
    axes.sort_by(|a, b| b.0.dot(fwd).total_cmp(&a.0.dot(fwd)));
    p.circle_filled(origin, 30.0, tk.surface.gamma_multiply(0.85));
    for (axis, label, col) in axes {
        let d = vec2(axis.dot(right), -axis.dot(up)) * len;
        let end = origin + d;
        p.line_segment([origin, end], Stroke::new(2.0, col));
        p.circle_filled(end, 7.0, col);
        p.text(
            end,
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(9.5),
            Color32::WHITE,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(dt: DroneType) -> (Project, Model) {
        let mut p = Project::default();
        p.set_drone_type(dt);
        let m = crate::calc::evaluate(&p, &crate::calc::Library::embedded());
        (p, m)
    }

    #[test]
    fn scene_rebuilds_only_on_change() {
        let (mut p, m) = eval(DroneType::FixedWingConventional);
        let mut st = ViewportState::default();
        st.update_scene(&p, &m);
        let v = st.scene_version;
        st.update_scene(&p, &m);
        assert_eq!(st.scene_version, v);
        p.b = 2.0;
        let m = crate::calc::evaluate(&p, &crate::calc::Library::embedded());
        st.update_scene(&p, &m);
        assert_ne!(st.scene_version, v);
        assert!(!st.scene().is_empty());
    }

    #[test]
    fn presets_animate_and_are_detected() {
        let (p, m) = eval(DroneType::Multirotor);
        let mut st = ViewportState::default();
        st.update_scene(&p, &m);
        st.go_preset(Preset::Top, 1.6);
        let goal = st.goal.unwrap();
        for _ in 0..600 {
            if camera::approach(&mut st.cam, &goal, 0.016) {
                break;
            }
        }
        st.goal = None;
        assert_eq!(st.current_preset(), Some(Preset::Top));
    }

    #[test]
    fn summary_mentions_span_and_mass() {
        let (p, m) = eval(DroneType::FixedWingConventional);
        let s = summary_text(&p, &m, &Scene::default());
        assert!(s.contains(&fmt_length_m(p.b)));
        assert!(s.contains(&fmt_mass_kg(m.weight.m_mtow)));
        let (p, m) = eval(DroneType::Multirotor);
        let s = summary_text(&p, &m, &Scene::default());
        assert!(s.contains(&fmt_mass_kg(m.mr.m_mtow)));
    }

    #[test]
    fn grid_is_built_and_cached() {
        let (p, m) = eval(DroneType::FixedWingConventional);
        let mut st = ViewportState::default();
        st.update_scene(&p, &m);
        st.ensure_grid();
        let v = st.grid.version;
        assert!(!st.grid.minor.is_empty() || !st.grid.major.is_empty());
        st.ensure_grid();
        assert_eq!(st.grid.version, v);
    }

    /// Her tür ve görünüm modu için başsız kare: panik yok (GL geri çağrısı burada çalışmaz).
    #[test]
    fn viewport_renders_headless_for_all_types() {
        for dt in DroneType::ALL {
            let (p, m) = eval(dt);
            let mut st = ViewportState::default();
            let ctx = egui::Context::default();
            for mode in [
                ViewMode::Model3d,
                ViewMode::Schematic2d,
                ViewMode::Chart,
                ViewMode::Formulas,
            ] {
                st.mode = mode;
                for with_chart in [false, true] {
                    let mut input = egui::RawInput::default();
                    input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, vec2(900.0, 500.0)));
                    let mut drawn = false;
                    let mut out = ctx.run_ui(input, |ui| {
                        egui::CentralPanel::default().show(ui, |ui| {
                            let mut draw = |ui: &mut Ui, _h: f32| {
                                drawn = true;
                                ui.label("chart");
                            };
                            let chart = with_chart.then_some(ChartSpec {
                                label: "Grafik",
                                draw: &mut draw,
                            });
                            show(ui, &mut st, &p, &m, chart, None, |_| {})
                        });
                    });
                    out.textures_delta.clear();
                    // Grafik yalnız seçiliyse ve adım sağlıyorsa çizilir.
                    assert_eq!(
                        drawn,
                        with_chart && mode == ViewMode::Chart,
                        "{dt:?} {mode:?}"
                    );
                    // Grafiği olmayan adımda seçim korunur.
                    assert_eq!(st.mode, mode);
                }
            }
            // GL hatası simülasyonu: 2D'ye düşer, panik yok.
            st.gl_failed.store(true, Ordering::Relaxed);
            st.mode = ViewMode::Model3d;
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                egui::CentralPanel::default()
                    .show(ui, |ui| show(ui, &mut st, &p, &m, None, None, |_| {}));
            });
            out.textures_delta.clear();
        }
    }
}
