// SPDX-License-Identifier: AGPL-3.0-or-later
//! Topoloji sekmesi: düz parçalar için 2B SIMP hafifletme. Hesap arka plan iş parçacığında
//! çalışır (ilerleme + iptal); proje değerlerini DEĞİŞTİRMEZ. Durum egui belleğindedir.
use super::components::{self, Status, section_header};
use super::theme::{self, space};
use super::widgets::{fmt, section};
use crate::cad::layout::Layout;
use crate::calc::Model;
use crate::i18n::{t, tf};
use crate::project::Project;
use crate::topology::simp::{Cell, Problem, SimpParams, SimpResult, optimize};
use crate::topology::{self, Part, PlateMaterial, Settings};
use egui::Ui;
use egui_plot::{Line, Plot, PlotPoints};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::sync::{Arc, Mutex};
use std::time::Instant;

enum Msg {
    Iter {
        compliance: f64,
        vol_frac: f64,
        density: Vec<f64>,
    },
    Done(Box<SimpResult>),
    Failed(String),
}

pub struct TopoState {
    settings: Settings,
    /// Yük çarpanı için projedeki değeri kullan.
    auto_load: bool,
    rx: Option<Receiver<Msg>>,
    cancel: Arc<AtomicBool>,
    error: Option<String>,
    problem: Option<Arc<Problem>>,
    params: Option<SimpParams>,
    load_factor_used: f64,
    history: Vec<f64>,
    live_density: Vec<f64>,
    live_vol: f64,
    result: Option<SimpResult>,
    started: Option<Instant>,
    elapsed_s: f64,
    export_status: Option<String>,
}

impl TopoState {
    fn new(part: Part) -> Self {
        TopoState {
            settings: Settings::new(part),
            auto_load: true,
            rx: None,
            cancel: Arc::new(AtomicBool::new(false)),
            error: None,
            problem: None,
            params: None,
            load_factor_used: 0.0,
            history: Vec::new(),
            live_density: Vec::new(),
            live_vol: 0.0,
            result: None,
            started: None,
            elapsed_s: 0.0,
            export_status: None,
        }
    }

    fn running(&self) -> bool {
        self.rx.is_some()
    }
}

type Shared = Arc<Mutex<TopoState>>;

/// Hesabı başlatır (problem UI iş parçacığında kurulur, çözüm arka planda).
fn start(st: &mut TopoState, p: &Project, m: &Model, ctx: &egui::Context) {
    st.error = None;
    st.export_status = None;
    let mut s = st.settings;
    if st.auto_load {
        s.load_factor = 0.0;
    }
    let layout = Layout::new(p, m);
    let prob = match topology::build_problem(p, m, &layout, &s) {
        Ok(x) => Arc::new(x),
        Err(e) => {
            st.error = Some(err_text(&e));
            return;
        }
    };
    let par = topology::simp_params(&layout, &s);
    st.load_factor_used = if s.load_factor > 0.0 {
        s.load_factor
    } else {
        topology::project_load_factor(p, s.part)
    };
    st.problem = Some(prob.clone());
    st.params = Some(par);
    st.result = None;
    st.history.clear();
    st.live_density.clear();
    st.started = Some(Instant::now());
    let (tx, rx) = channel();
    st.cancel = Arc::new(AtomicBool::new(false));
    let cancel = st.cancel.clone();
    let ctx = ctx.clone();
    st.rx = Some(rx);
    let spawned = std::thread::Builder::new()
        .name("topology".into())
        .spawn(move || {
            let tx2 = tx.clone();
            let ctx2 = ctx.clone();
            let r = optimize(&prob, &par, &cancel, &mut |i| {
                let _ = tx2.send(Msg::Iter {
                    compliance: i.compliance,
                    vol_frac: i.vol_frac,
                    density: i.density.to_vec(),
                });
                ctx2.request_repaint();
            });
            let msg = match r {
                Ok(r) => Msg::Done(Box::new(r)),
                Err(e) => Msg::Failed(e),
            };
            let _ = tx.send(msg);
            ctx.request_repaint();
        });
    if spawned.is_err() {
        st.rx = None;
        st.error = Some(t("topology.err_thread").to_string());
    }
}

/// Saf koddan gelen hata kodunu kullanıcı metnine çevirir.
fn err_text(code: &str) -> String {
    let text = match code.split(':').next().unwrap_or("") {
        "vol_low" => t("topology.err.vol_low"),
        "no_design" => t("topology.err.no_design"),
        "no_load" => t("topology.err.no_load"),
        "bad_params" => t("topology.err.bad_params"),
        "bad_geometry" => t("topology.err.bad_geometry"),
        "no_weight" => t("topology.err.no_weight"),
        "not_pd" => t("topology.err.not_pd"),
        _ => return tf("topology.err", &[code.to_string()]),
    };
    text.to_string()
}

fn poll(st: &mut TopoState) {
    loop {
        let Some(rx) = &st.rx else { return };
        match rx.try_recv() {
            Ok(Msg::Iter {
                compliance,
                vol_frac,
                density,
            }) => {
                st.history.push(compliance);
                st.live_density = density;
                st.live_vol = vol_frac;
            }
            Ok(Msg::Done(r)) => {
                st.history = r.history.clone();
                st.result = Some(*r);
                st.rx = None;
                st.elapsed_s = st.started.map(|s| s.elapsed().as_secs_f64()).unwrap_or(0.0);
            }
            Ok(Msg::Failed(e)) => {
                st.error = Some(err_text(&e));
                st.rx = None;
            }
            Err(TryRecvError::Disconnected) => {
                st.error = Some(t("topology.err_disconnected").to_string());
                st.rx = None;
            }
            Err(TryRecvError::Empty) => return,
        }
    }
}

fn state_handle(ui: &Ui, dt: crate::project::DroneType) -> Shared {
    let id = egui::Id::unique("topology_state");
    let existing: Option<Shared> = ui.data(|d| d.get_temp(id));
    if let Some(s) = existing {
        return s;
    }
    let first = topology::available_parts(dt)
        .first()
        .copied()
        .unwrap_or(Part::Rib);
    let s: Shared = Arc::new(Mutex::new(TopoState::new(first)));
    ui.data_mut(|d| d.insert_temp(id, s.clone()));
    s
}

pub fn show(ui: &mut Ui, p: &Project, m: &Model) {
    let handle = state_handle(ui, p.drone_type);
    let mut guard = match handle.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    show_state(ui, p, m, &mut guard);
}

fn show_state(ui: &mut Ui, p: &Project, m: &Model, st: &mut TopoState) {
    poll(st);
    let tk = theme::tokens(ui);
    section_header(ui, t("tab_topology"));
    components::banner(ui, Status::Info, t("topology.disclaimer"));
    ui.add_space(space::XS);
    let parts = topology::available_parts(p.drone_type);
    if parts.is_empty() {
        ui.label(t("topology.no_parts"));
        return;
    }
    if !parts.contains(&st.settings.part) {
        set_part(st, parts[0]);
    }
    let running = st.running();
    ui.add_enabled_ui(!running, |ui| {
        ui.horizontal(|ui| {
            ui.label(t("topology.part"));
            for part in &parts {
                if ui
                    .selectable_label(st.settings.part == *part, t(part.label_key()))
                    .clicked()
                {
                    set_part(st, *part);
                }
            }
        });
        let desc = match st.settings.part {
            Part::Arm => t("topology.part_desc.arm"),
            Part::Plate => t("topology.part_desc.plate"),
            Part::Rib => t("topology.part_desc.rib"),
        };
        ui.label(egui::RichText::new(desc).weak());
        inputs(ui, p, m, st);
    });
    ui.add_space(space::XS);
    ui.horizontal(|ui| {
        if ui
            .add_enabled(!running, egui::Button::new(t("topology.run")))
            .clicked()
        {
            start(st, p, m, ui.ctx());
        }
        if running {
            if ui.button(t("topology.cancel")).clicked() {
                st.cancel.store(true, Ordering::Relaxed);
            }
            ui.spinner();
            ui.label(tf("topology.running", &[st.history.len().to_string()]));
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
    });
    if let Some(e) = &st.error {
        ui.colored_label(tk.danger, e);
    }
    let Some(prob) = st.problem.clone() else {
        ui.label(t("topology.no_results"));
        return;
    };
    ui.separator();
    let density: &[f64] = match &st.result {
        Some(r) => &r.density,
        None => &st.live_density,
    };
    section(ui, t("topology.density_title"));
    draw_density(ui, &prob, density);
    ui.label(egui::RichText::new(t("topology.legend")).weak());
    if !st.history.is_empty() {
        section(ui, t("topology.compliance_title"));
        let pts: PlotPoints<'_> = st
            .history
            .iter()
            .enumerate()
            .map(|(i, c)| [(i + 1) as f64, *c])
            .collect();
        Plot::new("topology_compliance")
            .height(170.0)
            .x_axis_label(t("topology.axis_iter"))
            .y_axis_label(t("topology.axis_compliance"))
            .allow_scroll(false)
            .show(ui, |pu| {
                pu.line(Line::new(t("topology.compliance_title"), pts).width(2.0));
            });
    }
    if running {
        ui.label(tf("topology.live_vol", &[fmt(100.0 * st.live_vol, 1)]));
    }
    if let (Some(r), Some(par)) = (st.result.take(), st.params) {
        results(ui, st, &prob, &r, &par);
        st.result = Some(r);
    }
}

fn set_part(st: &mut TopoState, part: Part) {
    st.settings.part = part;
    st.settings.resolution = part.default_resolution();
    st.settings.thickness_mm = 0.0;
    st.problem = None;
    st.result = None;
    st.history.clear();
    st.error = None;
}

fn inputs(ui: &mut Ui, p: &Project, m: &Model, st: &mut TopoState) {
    let layout = Layout::new(p, m);
    let s = &mut st.settings;
    egui::Grid::new("topology_inputs")
        .num_columns(2)
        .spacing([space::M, space::XS])
        .show(ui, |ui| {
            ui.label(t("topology.material"));
            egui::ComboBox::from_id_salt("topology_material")
                .selected_text(t(s.material.label_key()))
                .show_ui(ui, |ui| {
                    for mt in PlateMaterial::ALL {
                        ui.selectable_value(&mut s.material, mt, t(mt.label_key()));
                    }
                });
            ui.end_row();

            let t_def = topology::default_thickness_mm(&layout, s.part);
            ui.label(t("topology.thickness"));
            ui.horizontal(|ui| {
                if s.thickness_mm <= 0.0 {
                    s.thickness_mm = t_def;
                }
                ui.add(
                    egui::DragValue::new(&mut s.thickness_mm)
                        .speed(0.1)
                        .range(0.3..=20.0)
                        .suffix(" mm"),
                );
                ui.label(
                    egui::RichText::new(tf("topology.thickness_default", &[fmt(t_def, 1)])).weak(),
                );
            });
            ui.end_row();

            ui.label(t("topology.vol_frac"));
            ui.add(egui::Slider::new(&mut s.vol_frac, 0.15..=0.9).fixed_decimals(2));
            ui.end_row();

            ui.label(t("topology.penal"));
            ui.add(
                egui::DragValue::new(&mut s.penal)
                    .speed(0.05)
                    .range(1.0..=5.0),
            );
            ui.end_row();

            ui.label(t("topology.rmin"));
            ui.add(egui::Slider::new(&mut s.rmin, 1.2..=6.0).fixed_decimals(1));
            ui.end_row();

            ui.label(t("topology.resolution"));
            ui.add(egui::Slider::new(&mut s.resolution, 30..=240));
            ui.end_row();

            ui.label(t("topology.load_factor"));
            ui.horizontal(|ui| {
                let n_proj = topology::project_load_factor(p, s.part);
                ui.checkbox(
                    &mut st.auto_load,
                    tf("topology.load_auto", &[fmt(n_proj, 2)]),
                );
                if !st.auto_load {
                    if s.load_factor <= 0.0 {
                        s.load_factor = n_proj;
                    }
                    ui.add(
                        egui::DragValue::new(&mut s.load_factor)
                            .speed(0.05)
                            .range(0.5..=20.0),
                    );
                }
            });
            ui.end_row();

            ui.label(t("topology.max_iter"));
            ui.add(egui::DragValue::new(&mut s.max_iter).range(10..=300));
            ui.end_row();
        });
    // Izgara bilgisi (yalnız bilgi): gerçek ızgarayı kurmadan, hücre sayısını tahmin etmeyiz.
    if let Ok(prob) = topology::build_problem(p, m, &layout, &{
        let mut q = *s;
        if st.auto_load {
            q.load_factor = 0.0;
        }
        q
    }) {
        ui.label(
            egui::RichText::new(tf(
                "topology.grid_info",
                &[
                    prob.grid.nx.to_string(),
                    prob.grid.ny.to_string(),
                    fmt(prob.h_m * 1e3, 2),
                    fmt(s.rmin * prob.h_m * 1e3, 1),
                ],
            ))
            .weak(),
        );
    }
}

/// Yoğunluk alanı: tema renkleriyle gri tonlama (boşluk → zemin, katı → metin rengi).
fn draw_density(ui: &mut Ui, prob: &Problem, density: &[f64]) {
    let tk = theme::tokens(ui);
    let g = prob.grid;
    let avail = ui.available_width().max(100.0);
    let scale = (avail / g.nx as f32).min(320.0 / g.ny as f32).max(1.0);
    let size = egui::vec2(g.nx as f32 * scale, g.ny as f32 * scale);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, tk.viewport_bg);
    let solid_fixed = tk.text.lerp_to_gamma(tk.accent, 0.45);
    let mut mesh = egui::Mesh::default();
    for ey in 0..g.ny {
        for ex in 0..g.nx {
            let e = g.elem(ex, ey);
            let d = density.get(e).copied().unwrap_or(0.0).clamp(0.0, 1.0) as f32;
            let col = match prob.cells[e] {
                Cell::Outside | Cell::Hole => continue,
                Cell::Solid => solid_fixed,
                Cell::Design => {
                    if d < 0.02 {
                        continue;
                    }
                    tk.viewport_bg.lerp_to_gamma(tk.text, d)
                }
            };
            let x0 = rect.left() + ex as f32 * scale;
            let y0 = rect.bottom() - (ey + 1) as f32 * scale;
            mesh.add_colored_rect(
                egui::Rect::from_min_size(egui::pos2(x0, y0), egui::vec2(scale, scale)),
                col,
            );
        }
    }
    painter.add(egui::Shape::mesh(mesh));
    // Mesnetler ve yükler.
    let node_screen = |ix: usize, iy: usize| {
        egui::pos2(
            rect.left() + ix as f32 * scale,
            rect.bottom() - iy as f32 * scale,
        )
    };
    for iy in 0..=g.ny {
        for ix in 0..=g.nx {
            let n = g.node(ix, iy);
            let pos = node_screen(ix, iy);
            if prob.fixed[2 * n] || prob.fixed[2 * n + 1] {
                painter.circle_filled(pos, (scale * 0.35).clamp(1.0, 2.5), tk.info);
            }
            let (fx, fy) = (prob.loads[2 * n], prob.loads[2 * n + 1]);
            let mag = (fx * fx + fy * fy).sqrt();
            if mag > 1e-12 {
                let v = egui::vec2(fx as f32, -(fy as f32)) / mag as f32 * 14.0;
                painter.arrow(pos - v, v, egui::Stroke::new(1.2, tk.danger));
            }
        }
    }
    painter.rect_stroke(
        rect,
        0.0,
        egui::Stroke::new(1.0, tk.border),
        egui::StrokeKind::Inside,
    );
}

fn results(ui: &mut Ui, st: &mut TopoState, prob: &Problem, r: &SimpResult, par: &SimpParams) {
    let tk = theme::tokens(ui);
    section(ui, t("topology.results"));
    let saving = 100.0 * (1.0 - r.mass_after_kg / r.mass_before_kg.max(1e-12));
    let g = |kg: f64| format!("{} g", fmt(kg * 1000.0, 2));
    egui::Grid::new("topology_results")
        .num_columns(2)
        .spacing([space::M, space::XS])
        .show(ui, |ui| {
            let mut row = |k: &str, v: String| {
                ui.label(k);
                ui.label(v);
                ui.end_row();
            };
            row(t("topology.mass_before"), g(r.mass_before_kg));
            row(t("topology.mass_after"), g(r.mass_after_kg));
            row(t("topology.saving"), format!("{} %", fmt(saving, 1)));
            row(
                t("topology.vol_final"),
                format!("{} %", fmt(100.0 * r.volume_fraction, 1)),
            );
            if let (Some(c0), Some(c1)) = (r.history.first(), r.history.last()) {
                row(
                    t("topology.compliance"),
                    format!("{} → {} J", fmt(*c0, 4), fmt(*c1, 4)),
                );
            }
            row(
                t("topology.iterations"),
                format!(
                    "{} ({}), {} s",
                    r.iterations,
                    if r.cancelled {
                        t("topology.cancelled")
                    } else if r.converged {
                        t("topology.converged")
                    } else {
                        t("topology.iter_limit")
                    },
                    fmt(st.elapsed_s, 1)
                ),
            );
            row(
                t("topology.load_used"),
                format!("n = {}", fmt(st.load_factor_used, 2)),
            );
            row(
                t("topology.stress_est"),
                format!("{} MPa", fmt(r.vm_p99 / 1e6, 1)),
            );
            row(
                t("topology.stress_peak"),
                format!("{} MPa", fmt(r.vm_max / 1e6, 1)),
            );
            row(
                t("topology.stress_allow"),
                format!("{} MPa", fmt(par.mat.sigma_allow / 1e6, 0)),
            );
        });
    if r.safety_factor.is_finite() {
        let status = if r.safety_factor < 1.0 {
            Status::Err
        } else if r.safety_factor < 1.5 {
            Status::Warn
        } else {
            Status::Ok
        };
        components::banner(
            ui,
            status,
            &tf("topology.sf_line", &[fmt(r.safety_factor, 2)]),
        );
    }
    ui.label(egui::RichText::new(t("topology.stress_note")).weak());
    let _ = tk;
    ui.add_space(space::XS);
    ui.horizontal(|ui| {
        if ui.button(t("topology.export_dxf")).clicked() {
            export(st, prob, r);
        }
        ui.label(egui::RichText::new(t("topology.dxf_hint")).weak());
    });
    if let Some(s) = &st.export_status {
        ui.label(s);
    }
}

fn export(st: &mut TopoState, prob: &Problem, r: &SimpResult) {
    let name = match st.settings.part {
        Part::Arm => "arm_cutouts.dxf",
        Part::Plate => "plate_cutouts.dxf",
        Part::Rib => "rib_cutouts.dxf",
    };
    let Some(path) = rfd::FileDialog::new()
        .add_filter("DXF (*.dxf)", &["dxf"])
        .set_file_name(name)
        .save_file()
    else {
        return;
    };
    let txt = topology::export_dxf(prob, r);
    st.export_status = Some(match std::fs::write(&path, txt) {
        Ok(()) => tf("topology.dxf_saved", &[path.display().to_string()]),
        Err(e) => tf("topology.dxf_err", &[e.to_string()]),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Library, evaluate};
    use crate::project::DroneType;

    fn fixture(dt: DroneType) -> (Project, Model) {
        let mut p = Project::default();
        p.set_drone_type(dt);
        let m = evaluate(&p, &Library::embedded());
        (p, m)
    }

    fn quick(st: &mut TopoState) {
        st.settings.resolution = if st.settings.part == Part::Rib {
            110
        } else {
            40
        };
        st.settings.max_iter = 8;
    }

    /// Arka plan iş parçacığı yolu: başlat, bitene dek yokla, sonuç gelir.
    #[test]
    fn thread_run_delivers_result() {
        let (p, m) = fixture(DroneType::Multirotor);
        let mut st = TopoState::new(Part::Plate);
        quick(&mut st);
        let ctx = egui::Context::default();
        start(&mut st, &p, &m, &ctx);
        assert!(st.running());
        let t0 = Instant::now();
        while st.running() && t0.elapsed().as_secs() < 60 {
            std::thread::sleep(std::time::Duration::from_millis(20));
            poll(&mut st);
        }
        assert!(st.error.is_none(), "{:?}", st.error);
        let r = st.result.as_ref().expect("sonuç");
        assert!(r.mass_after_kg < r.mass_before_kg);
        assert_eq!(st.history.len(), r.iterations);
    }

    #[test]
    fn cancel_via_flag_stops_thread() {
        let (p, m) = fixture(DroneType::Multirotor);
        let mut st = TopoState::new(Part::Arm);
        quick(&mut st);
        st.settings.max_iter = 300;
        let ctx = egui::Context::default();
        start(&mut st, &p, &m, &ctx);
        st.cancel.store(true, Ordering::Relaxed);
        let t0 = Instant::now();
        while st.running() && t0.elapsed().as_secs() < 60 {
            std::thread::sleep(std::time::Duration::from_millis(20));
            poll(&mut st);
        }
        let r = st.result.as_ref().expect("sonuç");
        assert!(r.cancelled);
        assert!(r.iterations < 300);
    }

    /// Her araç türü, iki tema ve iki dilde başsız çizim (panik yok), sonuçlu ve sonuçsuz.
    #[test]
    fn renders_in_both_themes_and_languages() {
        use crate::ui::theme::{self, ThemeChoice};
        for dt in DroneType::ALL {
            let (p, m) = fixture(dt);
            for choice in [ThemeChoice::Dark, ThemeChoice::Light] {
                for lang in [crate::i18n::Lang::Tr, crate::i18n::Lang::En] {
                    let ctx = egui::Context::default();
                    theme::install(&ctx, choice);
                    let part = topology::available_parts(dt)[0];
                    let mut st = TopoState::new(part);
                    quick(&mut st);
                    st.error = Some("örnek hata".into());
                    let render = |st: &mut TopoState| {
                        crate::i18n::with_lang(lang, || {
                            for _ in 0..2 {
                                let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                                    egui::CentralPanel::default().show(ui, |ui| {
                                        show_state(ui, &p, &m, st);
                                    });
                                });
                                out.textures_delta.clear();
                            }
                        });
                    };
                    render(&mut st);
                    let layout = Layout::new(&p, &m);
                    let s = st.settings;
                    let prob = topology::build_problem(&p, &m, &layout, &s).unwrap();
                    let par = topology::simp_params(&layout, &s);
                    let r = optimize(&prob, &par, &AtomicBool::new(false), &mut |_| {}).unwrap();
                    st.problem = Some(Arc::new(prob));
                    st.params = Some(par);
                    st.history = r.history.clone();
                    st.result = Some(r);
                    render(&mut st);
                }
            }
        }
    }
}
