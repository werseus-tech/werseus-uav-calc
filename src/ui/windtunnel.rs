// SPDX-License-Identifier: AGPL-3.0-or-later
//! Rüzgâr tüneli sekmesi: 2D profil (panel + sınır tabaka) ve 3D VLM sonuçları.
//! Hesap arka plan iş parçacığında çalışır, iptal edilebilir; proje değerlerini DEĞİŞTİRMEZ,
//! yalnızca "programın formülleri vs. tünel" karşılaştırması gösterir.
use super::components::{self, Status, section_header};
use super::theme::{self, space};
use super::widgets::{fmt, section};
use crate::calc::Model;
use crate::i18n::{t, tf};
use crate::project::Project;
use crate::windtunnel::airfoil::Airfoil;
use crate::windtunnel::panel::PanelModel;
use crate::windtunnel::polar::{PolarOptions, PolarResult, STALL_SEP_X, polar};
use crate::windtunnel::vlm::{Surface, VlmResult, solve};
use egui::Ui;
use egui_plot::{Legend, Line, Plot, PlotPoints};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Cp,
    Polar,
    Span,
    Compare,
}

/// Bir α için Cp eğrileri (x/c, Cp).
#[derive(Clone, Debug)]
struct CpSet {
    alpha: f64,
    upper: Vec<[f64; 2]>,
    lower: Vec<[f64; 2]>,
}

/// Arka plan işinin girdileri (UI iş parçacığında projeden toplanır).
#[derive(Clone)]
struct Inputs {
    foil: Airfoil,
    re: f64,
    b: f64,
    c_r: f64,
    taper: f64,
    sweep_le_deg: f64,
    mac: f64,
    /// Kök hücum kenarına göre MAC çeyrek-kordu (m).
    x_ac: f64,
    /// Yatay kuyruk: (açıklık, kord, hücum kenarı x).
    tail: Option<(f64, f64, f64)>,
}

struct Results {
    foil_name: String,
    re: f64,
    polar: PolarResult,
    cp_sets: Vec<CpSet>,
    wing: VlmResult,
    wing_tail: Option<VlmResult>,
    x_root_le: f64,
}

enum Msg {
    Done(Box<Results>),
    Failed(String),
}

pub struct WtState {
    view: View,
    cp_idx: usize,
    rx: Option<Receiver<Msg>>,
    cancel: Arc<AtomicBool>,
    error: Option<String>,
    results: Option<Results>,
}

impl Default for WtState {
    fn default() -> Self {
        WtState {
            view: View::Cp,
            cp_idx: 1,
            rx: None,
            cancel: Arc::new(AtomicBool::new(false)),
            error: None,
            results: None,
        }
    }
}

fn collect_inputs(p: &Project, m: &Model) -> Result<Inputs, String> {
    if !p.drone_type.has_wing() {
        return Err(t("wt.err_no_wing").to_string());
    }
    let foil = Airfoil::from_points(&p.airfoil.name, &p.airfoil.points)
        .map_err(|e| tf("wt.err_airfoil", &[&e]))?;
    let re = m.aero.re_wing;
    if !(re.is_finite() && re > 1e4) {
        return Err(t("wt.err_re").to_string());
    }
    if !(p.b > 0.0 && p.s > 0.0 && m.wing.c_r > 0.0 && m.wing.mac > 0.0) {
        return Err(t("wt.err_geom").to_string());
    }
    let x_ac = m.wing.x_le_mac + 0.25 * m.wing.mac;
    let tail = if p.tail_kind == crate::calc::geometry::TailKind::Conventional
        && p.s_h > 0.0
        && p.l_h > 0.0
    {
        // Kuyruk açıklığı için sabit AR = 4 varsayımı (projede yatay kuyruk açıklığı yok).
        let bt = (4.0 * p.s_h).sqrt();
        let ct = p.s_h / bt;
        Some((bt, ct, x_ac + p.l_h - 0.25 * ct))
    } else {
        None
    };
    Ok(Inputs {
        foil,
        re,
        b: p.b,
        c_r: m.wing.c_r,
        taper: p.taper,
        sweep_le_deg: p.sweep_le_deg,
        mac: m.wing.mac,
        x_ac,
        tail,
    })
}

fn run_job(inp: &Inputs, cancel: &AtomicBool) -> Result<Results, String> {
    let opts = PolarOptions {
        reynolds: inp.re,
        ..Default::default()
    };
    let pol = polar(&inp.foil, &opts, cancel)?;
    // Cp eğrileri.
    let pm = PanelModel::new(&inp.foil.repaneled(opts.n_panels))?;
    let le = (0..pm.n)
        .min_by(|a, b| pm.mid[*a][0].total_cmp(&pm.mid[*b][0]))
        .unwrap_or(pm.n / 2);
    let mut cp_sets = Vec::new();
    for a in [0.0, 4.0, 8.0, 12.0] {
        let sol = pm.solve(a)?;
        let pts: Vec<[f64; 2]> = (0..pm.n).map(|i| [pm.mid[i][0], sol.cp[i]]).collect();
        cp_sets.push(CpSet {
            alpha: a,
            upper: pts[..=le].to_vec(),
            lower: pts[le..].to_vec(),
        });
    }
    if cancel.load(Ordering::Relaxed) {
        return Err(t("wt.cancelled").to_string());
    }
    // VLM: önce yalnız kanat, sonra (varsa) kanat + yatay kuyruk. α = 3° (model doğrusaldır).
    let wing = Surface::trapezoid("wing", inp.b, inp.c_r, inp.taper, inp.sweep_le_deg, 0.0, 24);
    // Referans alan: VLM'in gerçekten kullandığı trapez alan (proje S ile aynı olmalı).
    let s_ref = wing.area();
    let w = solve(
        std::slice::from_ref(&wing),
        s_ref,
        inp.mac,
        inp.x_ac,
        3.0,
        6,
        cancel,
    )?;
    let wt = match inp.tail {
        Some((bt, ct, x0)) => {
            let tail = Surface::trapezoid("tail", bt, ct, 1.0, 0.0, x0, 12);
            Some(solve(
                &[wing, tail],
                s_ref,
                inp.mac,
                inp.x_ac,
                3.0,
                6,
                cancel,
            )?)
        }
        None => None,
    };
    Ok(Results {
        foil_name: inp.foil.name.clone(),
        re: inp.re,
        polar: pol,
        cp_sets,
        wing: w,
        wing_tail: wt,
        x_root_le: 0.0,
    })
}

fn start(st: &mut WtState, p: &Project, m: &Model, ctx: &egui::Context) {
    st.error = None;
    let inp = match collect_inputs(p, m) {
        Ok(i) => i,
        Err(e) => {
            st.error = Some(e);
            return;
        }
    };
    let x_root_le = p.x_root_le;
    let (tx, rx) = channel();
    st.cancel = Arc::new(AtomicBool::new(false));
    let cancel = st.cancel.clone();
    let ctx = ctx.clone();
    st.rx = Some(rx);
    let spawned = std::thread::Builder::new()
        .name("windtunnel".into())
        .spawn(move || {
            let msg = match run_job(&inp, &cancel) {
                Ok(mut r) => {
                    r.x_root_le = x_root_le;
                    Msg::Done(Box::new(r))
                }
                Err(e) => Msg::Failed(e),
            };
            let _ = tx.send(msg);
            ctx.request_repaint();
        });
    if spawned.is_err() {
        st.rx = None;
        st.error = Some(t("wt.err_thread").to_string());
    }
}

fn poll(st: &mut WtState) {
    let Some(rx) = &st.rx else { return };
    match rx.try_recv() {
        Ok(Msg::Done(r)) => {
            st.results = Some(*r);
            st.rx = None;
        }
        Ok(Msg::Failed(e)) => {
            st.error = Some(e);
            st.rx = None;
        }
        Err(TryRecvError::Disconnected) => {
            st.error = Some(t("wt.err_disconnected").to_string());
            st.rx = None;
        }
        Err(TryRecvError::Empty) => {}
    }
}

pub fn show(ui: &mut Ui, p: &Project, m: &Model, st: &mut WtState) {
    poll(st);
    let tk = theme::tokens(ui);
    section_header(ui, t("tab_windtunnel"));
    components::banner(ui, Status::Info, t("wt.disclaimer"));
    ui.add_space(space::XS);
    ui.label(tf(
        "wt.inputs_line",
        &[
            p.airfoil.name.as_str(),
            &format!("{:.0}", m.aero.re_wing),
            &fmt(p.b, 2),
            &fmt(p.s, 3),
        ],
    ));
    ui.horizontal(|ui| {
        let running = st.rx.is_some();
        if ui
            .add_enabled(!running, egui::Button::new(t("wt.run")))
            .clicked()
        {
            start(st, p, m, ui.ctx());
        }
        if running {
            if ui.button(t("wt.cancel")).clicked() {
                st.cancel.store(true, Ordering::Relaxed);
            }
            ui.spinner();
            ui.label(t("wt.running"));
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
    });
    if let Some(e) = &st.error {
        ui.colored_label(tk.danger, e);
    }
    let Some(r) = &st.results else {
        ui.label(t("wt.no_results"));
        return;
    };
    ui.separator();
    ui.label(tf(
        "wt.result_header",
        &[r.foil_name.as_str(), &format!("{:.0}", r.re)],
    ));
    ui.horizontal(|ui| {
        ui.selectable_value(&mut st.view, View::Cp, t("wt.view_cp"));
        ui.selectable_value(&mut st.view, View::Polar, t("wt.view_polar"));
        ui.selectable_value(&mut st.view, View::Span, t("wt.view_span"));
        ui.selectable_value(&mut st.view, View::Compare, t("wt.view_compare"));
    });
    match st.view {
        View::Cp => show_cp(ui, r, &mut st.cp_idx),
        View::Polar => show_polar(ui, r),
        View::Span => show_span(ui, r),
        View::Compare => show_compare(ui, p, m, r),
    }
}

fn show_cp(ui: &mut Ui, r: &Results, idx: &mut usize) {
    section(ui, t("wt.cp_title"));
    ui.horizontal(|ui| {
        ui.label("α");
        for (i, s) in r.cp_sets.iter().enumerate() {
            ui.selectable_value(idx, i, format!("{:.0}°", s.alpha));
        }
    });
    let Some(set) = r.cp_sets.get(*idx) else {
        return;
    };
    Plot::new("wt_cp")
        .height(320.0)
        .x_axis_label("x/c")
        .y_axis_label(t("wt.cp_axis"))
        .legend(Legend::default())
        .allow_scroll(false)
        .show(ui, |pu| {
            // Cp ekseni ters çizilir (emme yukarı): −Cp.
            let neg = |v: &Vec<[f64; 2]>| -> PlotPoints<'_> {
                PlotPoints::from(v.iter().map(|q| [q[0], -q[1]]).collect::<Vec<_>>())
            };
            pu.line(Line::new(t("wt.upper"), neg(&set.upper)).width(2.0));
            pu.line(Line::new(t("wt.lower"), neg(&set.lower)).width(2.0));
        });
    ui.label(egui::RichText::new(t("wt.cp_note")).weak());
}

fn show_polar(ui: &mut Ui, r: &Results) {
    let tk = theme::tokens(ui);
    let pol = &r.polar;
    section(ui, t("wt.polar_title"));
    let line = |name: &str, f: &dyn Fn(&crate::windtunnel::polar::PolarPoint) -> [f64; 2]| {
        Line::new(
            name.to_string(),
            PlotPoints::from(pol.points.iter().map(f).collect::<Vec<_>>()),
        )
        .width(2.0)
    };
    ui.columns(3, |c| {
        Plot::new("wt_cla")
            .height(240.0)
            .x_axis_label("α [°]")
            .y_axis_label("CL")
            .allow_scroll(false)
            .show(&mut c[0], |pu| {
                pu.line(line("CL", &|p| [p.alpha_deg, p.cl]));
            });
        Plot::new("wt_cdcl")
            .height(240.0)
            .x_axis_label("CD")
            .y_axis_label("CL")
            .allow_scroll(false)
            .show(&mut c[1], |pu| {
                pu.line(line("CL–CD", &|p| [p.cd, p.cl]));
            });
        Plot::new("wt_cm")
            .height(240.0)
            .x_axis_label("α [°]")
            .y_axis_label("CM c/4")
            .allow_scroll(false)
            .show(&mut c[2], |pu| {
                pu.line(line("CM", &|p| [p.alpha_deg, p.cm_c4]));
            });
    });
    egui::Grid::new("wt_polar_sum")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label(t("wt.row_cla2d"));
            ui.label(format!("{} /rad", fmt(pol.cl_alpha_per_rad, 2)));
            ui.end_row();
            ui.label(t("wt.row_al0"));
            ui.label(format!("{}°", fmt(pol.alpha_l0_deg, 2)));
            ui.end_row();
            ui.label(t("wt.row_cdmin"));
            ui.label(fmt(pol.cd_min, 4));
            ui.end_row();
        });
    // Ayrılma / tutunma uyarıları.
    match pol.stall {
        Some((a, cl)) => {
            ui.colored_label(tk.warning, tf("wt.warn_stall", &[&fmt(a, 0), &fmt(cl, 2)]));
        }
        None => {
            ui.label(t("wt.no_stall"));
        }
    }
    if let Some(p) = pol
        .points
        .iter()
        .find(|p| p.x_sep_upper.is_some_and(|x| x < STALL_SEP_X))
    {
        ui.colored_label(
            tk.warning,
            tf(
                "wt.warn_sep",
                &[&fmt(p.alpha_deg, 0), &fmt(p.x_sep_upper.unwrap_or(1.0), 2)],
            ),
        );
    }
    if let Some(p) = pol
        .points
        .iter()
        .find(|p| p.lam_sep_upper || p.lam_sep_lower)
    {
        ui.label(tf("wt.warn_lamsep", &[&fmt(p.alpha_deg, 0)]));
    }
    if pol.points.iter().any(|p| !p.bl_ok) {
        ui.colored_label(tk.danger, t("wt.warn_bl_failed"));
    }
    ui.label(egui::RichText::new(t("wt.polar_note")).color(tk.text_weak));
}

fn show_span(ui: &mut Ui, r: &Results) {
    section(ui, t("wt.span_title"));
    let v = r.wing_tail.as_ref().unwrap_or(&r.wing);
    let wing_strips: Vec<[f64; 2]> = v
        .strips
        .iter()
        .filter(|s| s.surface == 0)
        .map(|s| [s.y, s.cl])
        .collect();
    let tail_strips: Vec<[f64; 2]> = v
        .strips
        .iter()
        .filter(|s| s.surface == 1)
        .map(|s| [s.y, s.cl])
        .collect();
    Plot::new("wt_span")
        .height(300.0)
        .x_axis_label("y [m]")
        .y_axis_label("cl")
        .legend(Legend::default())
        .allow_scroll(false)
        .show(ui, |pu| {
            pu.line(Line::new(t("wt.wing"), PlotPoints::from(wing_strips)).width(2.0));
            if !tail_strips.is_empty() {
                pu.line(Line::new(t("wt.tail"), PlotPoints::from(tail_strips)).width(2.0));
            }
        });
    ui.label(tf(
        "wt.span_summary",
        &[
            &fmt(v.alpha_deg, 1),
            &fmt(v.cl, 3),
            &fmt(v.cd_i, 4),
            &fmt(r.wing.e, 3),
        ],
    ));
    ui.label(egui::RichText::new(t("wt.vlm_note")).weak());
}

fn row(ui: &mut Ui, label: &str, proj: Option<f64>, tun: Option<f64>, dec: usize, note: &str) {
    ui.label(label);
    ui.label(proj.map(|v| fmt(v, dec)).unwrap_or_else(|| "—".into()));
    ui.label(tun.map(|v| fmt(v, dec)).unwrap_or_else(|| "—".into()));
    match (proj, tun) {
        (Some(a), Some(b)) if a.abs() > 1e-9 => {
            ui.label(format!("{:+.1} %", 100.0 * (b - a) / a.abs()))
        }
        _ => ui.label("—"),
    };
    ui.label(egui::RichText::new(note).weak());
    ui.end_row();
}

fn show_compare(ui: &mut Ui, p: &Project, m: &Model, r: &Results) {
    section(ui, t("wt.compare_title"));
    ui.label(t("wt.compare_intro"));
    let np = r
        .wing_tail
        .as_ref()
        .unwrap_or(&r.wing)
        .x_np
        .map(|x| x + r.x_root_le);
    egui::Grid::new("wt_cmp")
        .num_columns(5)
        .striped(true)
        .show(ui, |ui| {
            for h in [
                "wt.col_quantity",
                "wt.col_project",
                "wt.col_tunnel",
                "wt.col_diff",
                "wt.col_note",
            ] {
                ui.label(egui::RichText::new(t(h)).strong());
            }
            ui.end_row();
            row(
                ui,
                t("wt.row_cla2d"),
                Some(p.cl_alpha_2d),
                Some(r.polar.cl_alpha_per_rad),
                2,
                "/rad",
            );
            row(
                ui,
                t("wt.row_clmax2d"),
                Some(p.cl_max_2d),
                r.polar.stall.map(|s| s.1),
                2,
                t("wt.note_estimate"),
            );
            row(
                ui,
                t("wt.row_al0"),
                None,
                Some(r.polar.alpha_l0_deg),
                2,
                "°",
            );
            row(ui, t("wt.row_cdmin"), None, Some(r.polar.cd_min), 4, "");
            row(
                ui,
                t("wt.row_cla_wing"),
                Some(m.aero.cla_w),
                Some(r.wing.cl_alpha_per_rad),
                2,
                "/rad",
            );
            row(
                ui,
                t("wt.row_e"),
                Some(m.aero.e),
                Some(r.wing.e),
                3,
                t("wt.note_wing_only"),
            );
            row(ui, t("wt.row_xnp"), Some(m.stab.x_np_nose), np, 3, "m");
        });
    ui.label(egui::RichText::new(t("wt.compare_note")).weak());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_project_runs_through_tunnel() {
        let p = Project::default();
        let lib = crate::calc::Library::load();
        let m = crate::calc::evaluate(&p, &lib);
        let before = serde_json::to_string(&p).unwrap();
        let inp = collect_inputs(&p, &m).expect("varsayılan projeden girdi toplanmalı");
        let r = run_job(&inp, &AtomicBool::new(false)).expect("tünel çalışmalı");
        assert!(r.wing.cl_alpha_per_rad > 2.0 && r.wing.cl_alpha_per_rad < 7.0);
        assert!(r.polar.cl_alpha_per_rad > 5.0);
        assert_eq!(r.cp_sets.len(), 4);
        // Tünel proje değerlerini değiştirmez: üretim yolu (girdi toplama + iş) sonrası proje
        // anlık görüntüsü ve Model aynı kalır.
        assert_eq!(before, serde_json::to_string(&p).unwrap());
        let m2 = crate::calc::evaluate(&p, &lib);
        assert_eq!(m.aero.cla_w, m2.aero.cla_w);
    }

    /// Sonuçlu rüzgâr tüneli dört görünümde, iki temada ve iki dilde başsız çizilir (panik yok).
    #[test]
    fn results_render_in_every_view_and_both_themes() {
        use crate::ui::theme::{self, ThemeChoice};
        let p = Project::default();
        let m = crate::calc::evaluate(&p, &crate::calc::Library::embedded());
        let inp = collect_inputs(&p, &m).expect("girdi");
        for choice in [ThemeChoice::Dark, ThemeChoice::Light] {
            for lang in [crate::i18n::Lang::Tr, crate::i18n::Lang::En] {
                let ctx = egui::Context::default();
                theme::install(&ctx, choice);
                let mut st = WtState::default();
                st.results = Some(run_job(&inp, &AtomicBool::new(false)).expect("tünel"));
                st.error = Some("örnek hata".into());
                for view in [View::Cp, View::Polar, View::Span, View::Compare] {
                    st.view = view;
                    crate::i18n::with_lang(lang, || {
                        for _ in 0..2 {
                            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                                egui::CentralPanel::default().show(ui, |ui| {
                                    show(ui, &p, &m, &mut st);
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
