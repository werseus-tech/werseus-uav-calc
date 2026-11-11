// SPDX-License-Identifier: AGPL-3.0-or-later
//! "Çıktılar" sihirbaz adımı: parça seçimi, hedef klasör, CAD üretimi (FCStd + STEP), montaj,
//! malzeme listesi ve montaj kontrolleri. Mantık `crate::cad::outputs` içindedir; burası yalnızca
//! arayüzdür.

use super::theme;
use crate::cad::outputs::{
    AbortReason, FREECAD_DOWNLOAD_URL, MASS_RATIO_MAX, MASS_RATIO_MIN, MassTarget, OutputsProgress,
    OutputsSummary, RunEvent, RunPhase, Selection, apply_mass_to_project, available_parts,
    design_fingerprint, mass_check, mass_target_label, run_outputs_controlled, stage_label,
    undo_mass_transfer,
};
use crate::cad::{CadLimits, CancelFlag, FreeCadInstallation, sanitize_filename};
use crate::calc::Model;
use crate::i18n::{t, tf};
use crate::project::{DroneType, Project};
use egui::{RichText, Ui};
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

/// Varsayılan CAD çıktı klasörü: Belgeler\WerSeuS\cad\<proje_adi>\
pub fn default_cad_dir(project_name: &str) -> PathBuf {
    let sanitized = sanitize_filename(project_name);

    #[cfg(windows)]
    let base = std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .map(|p| p.join("Documents"))
        .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from));

    #[cfg(not(windows))]
    let base = std::env::var_os("HOME").map(PathBuf::from);

    let root = base.unwrap_or_else(|| PathBuf::from("."));
    root.join("WerSeuS").join("cad").join(sanitized)
}

/// Adımın kalıcı arayüz durumu (uygulama başına bir tane).
#[derive(Default)]
pub struct OutputsUi {
    pub selection: Selection,
    pub out_dir: PathBuf,
    pub running: bool,
    pub log: Vec<String>,
    pub rx: Option<Receiver<RunEvent>>,
    pub summary: Option<OutputsSummary>,
    pub error: Option<String>,
    /// Çalışan üretimi durduran bayrak (İptal düğmesi).
    pub cancel: Option<CancelFlag>,
    /// İptal istendi, süreç sonlanması bekleniyor.
    pub cancelling: bool,
    pub started: Option<Instant>,
    pub progress: Option<OutputsProgress>,
    /// İptal / toplam süre aşımı iletisi (üretim tamamlanmadı).
    pub notice: Option<String>,
    /// Süre sınırları (ayarlardan; başlangıçta `AppSettings.cad_limits`).
    pub limits: CadLimits,
    /// Çalışma BAŞLARKEN geçerli olan (kısılmış) süre sınırları; iletiler bunu gösterir, böylece
    /// çalışırken Ayarlar'da değişiklik yapılsa da mesaj gerçekte uygulanan limiti söyler.
    run_limits: CadLimits,
    pub confirm_apply: bool,
    pub applied_msg: Option<String>,
    /// Geri alma için: aktarılan alan, eski değer ve yazılan değer.
    pub undo: Option<(MassTarget, f64, f64)>,
    init_for: Option<DroneType>,
}

impl OutputsUi {
    fn ensure_init(&mut self, p: &Project) {
        if self.init_for != Some(p.drone_type) {
            self.selection = Selection::all_for(p);
            self.init_for = Some(p.drone_type);
        }
        if self.out_dir.as_os_str().is_empty() {
            self.out_dir = default_cad_dir(&p.name);
        }
    }

    fn poll(&mut self) {
        use std::sync::mpsc::TryRecvError;
        let mut finished = false;
        if let Some(rx) = &self.rx {
            loop {
                match rx.try_recv() {
                    Ok(RunEvent::Log(s)) => self.log.push(s),
                    Ok(RunEvent::Progress(pr)) => self.progress = Some(pr),
                    Ok(RunEvent::Done(sum)) => {
                        self.summary = Some(*sum);
                        finished = true;
                        break;
                    }
                    Ok(RunEvent::Aborted {
                        reason,
                        completed,
                        removed,
                    }) => {
                        self.notice = Some(aborted_message(
                            reason,
                            completed,
                            removed,
                            &self.run_limits,
                        ));
                        finished = true;
                        break;
                    }
                    Ok(RunEvent::Error(e)) => {
                        self.error = Some(e);
                        finished = true;
                        break;
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        // Arka plan iş parçacığı sonuç göndermeden bitti (ör. panik).
                        self.error = Some(t("out.worker_lost").to_string());
                        finished = true;
                        break;
                    }
                }
            }
        }
        if finished {
            self.running = false;
            self.rx = None;
            self.cancel = None;
            self.cancelling = false;
            self.started = None;
            self.progress = None;
        }
    }

    /// Üretimi başlatır (parça seçimi ve klasör hazır olmalı).
    fn start(
        &mut self,
        fc: &FreeCadInstallation,
        p: &Project,
        m: &Model,
        chosen: Vec<crate::cad::outputs::PartId>,
    ) {
        self.summary = None;
        self.error = None;
        self.notice = None;
        self.applied_msg = None;
        self.confirm_apply = false;
        self.undo = None;
        self.log.clear();
        self.running = true;
        self.cancelling = false;
        self.progress = None;
        self.started = Some(Instant::now());
        let cancel = CancelFlag::new();
        self.cancel = Some(cancel.clone());
        self.run_limits = self.limits.clamped();
        self.rx = Some(run_outputs_controlled(
            fc.cmd_path.clone(),
            p.clone(),
            m.clone(),
            chosen,
            self.out_dir.clone(),
            self.run_limits,
            cancel,
        ));
    }

    /// İptal ister; arka plan iş parçacığı FreeCAD sürecini sonlandırıp yarım dosyaları siler.
    pub fn request_cancel(&mut self) {
        if let Some(c) = &self.cancel {
            c.cancel();
            self.cancelling = true;
        }
    }
}

impl OutputsUi {
    /// Uygulama kapanırken: üretim sürüyorsa iptal bayrağını kurar ve FreeCAD sürecinin (ağacıyla)
    /// sonlanmasını en çok `wait` kadar bekler (yetim süreç kalmasın). Kapanışı asla kilitlemez;
    /// süre dolarsa vazgeçer. Süreç sonlanıp temizlendiyse `true` döner.
    pub fn shutdown(&mut self, wait: Duration) -> bool {
        if !self.running {
            return true;
        }
        self.request_cancel();
        let deadline = Instant::now() + wait;
        loop {
            self.poll();
            if !self.running {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}

/// Kapanışta üretimin sonlanması için en uzun bekleme.
pub const SHUTDOWN_WAIT: Duration = Duration::from_secs(3);

/// `mm:ss` (bir saati aşarsa `s:mm:ss`).
pub fn format_elapsed(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

/// Üretimin yarıda kalma iletisi (iptal ya da toplam süre aşımı).
pub fn aborted_message(
    reason: AbortReason,
    completed: usize,
    removed: usize,
    limits: &CadLimits,
) -> String {
    match reason {
        AbortReason::Cancelled => tf(
            "out.cancelled_fmt",
            &[completed.to_string(), removed.to_string()],
        ),
        AbortReason::TotalTimeout => tf(
            "out.total_timeout_fmt",
            &[
                (limits.clamped().total_secs / 60).to_string(),
                completed.to_string(),
                removed.to_string(),
            ],
        ),
    }
}

/// Canlı ilerleme satırı: hangi parça/aşama, kaçta kaç, geçen süre.
pub fn progress_text(pr: &OutputsProgress, elapsed: Duration) -> String {
    let el = format_elapsed(elapsed);
    match pr.phase {
        RunPhase::Parts => tf(
            "out.progress_part_fmt",
            &[
                pr.current.to_string(),
                pr.total.to_string(),
                pr.part.clone(),
                el,
            ],
        ),
        RunPhase::Assembly => {
            let stage = pr
                .stage
                .as_ref()
                .and_then(|s| {
                    stage_label(&s.id).map(|l| match (s.done, s.total) {
                        (Some(d), Some(tt)) => format!("{l} {d}/{tt}"),
                        _ => l.to_string(),
                    })
                })
                .unwrap_or_else(|| t("out.stage.start").to_string());
            tf("out.progress_asm_fmt", &[stage, el])
        }
    }
}

/// İlerleme çubuğu oranı (0..1): parçalar + montaj, montaj son dilim.
pub fn progress_fraction(pr: &OutputsProgress) -> f32 {
    let total = pr.total.max(1) as f32 + 1.0;
    match pr.phase {
        RunPhase::Parts => ((pr.current.saturating_sub(1)) as f32 / total).clamp(0.0, 1.0),
        RunPhase::Assembly => {
            let base = pr.total as f32 / total;
            let part = match &pr.stage {
                Some(s) => match (s.done, s.total) {
                    (Some(d), Some(tt)) if tt > 0 => (d as f32 / tt as f32).clamp(0.0, 1.0),
                    _ => 0.0,
                },
                None => 0.0,
            };
            (base + part / total).clamp(0.0, 1.0)
        }
    }
}

/// Adımı çizer.
pub fn show(
    ui: &mut Ui,
    p: &mut Project,
    m: &Model,
    st: &mut OutputsUi,
    fc: Option<&FreeCadInstallation>,
) {
    st.ensure_init(p);
    st.poll();
    if st.running {
        ui.ctx().request_repaint();
    }
    let tk = theme::tokens(ui);

    ui.heading(t("out.heading"));
    ui.label(RichText::new(t("out.intro")).weak());
    ui.add_space(6.0);

    ui.strong(t("out.parts_heading"));
    ui.horizontal(|ui| {
        if ui.small_button(t("out.select_all")).clicked() {
            st.selection = Selection::all_for(p);
        }
        if ui.small_button(t("out.select_none")).clicked() {
            for id in available_parts(p) {
                st.selection.set(id, false);
            }
        }
    });
    for id in available_parts(p) {
        let mut on = st.selection.contains(id);
        if ui.checkbox(&mut on, id.title()).changed() {
            st.selection.set(id, on);
        }
    }
    ui.add_space(6.0);

    ui.horizontal(|ui| {
        ui.strong(t("out.dir_label"));
        let mut s = st.out_dir.to_string_lossy().to_string();
        if ui
            .add(egui::TextEdit::singleline(&mut s).desired_width(380.0))
            .changed()
        {
            st.out_dir = PathBuf::from(s);
        }
        if ui.button(t("btn_browse")).clicked() {
            if let Some(f) = rfd::FileDialog::new()
                .set_directory(&st.out_dir)
                .pick_folder()
            {
                st.out_dir = f;
            }
        }
    });
    ui.add_space(4.0);

    match fc {
        Some(f) => {
            ui.colored_label(tk.success, tf("out.freecad_found", &[f.version.as_str()]));
        }
        None => {
            ui.colored_label(tk.warning, t("out.freecad_missing"));
            ui.hyperlink_to(t("out.freecad_install_link"), FREECAD_DOWNLOAD_URL);
        }
    }
    ui.add_space(6.0);

    let chosen = st.selection.selected_for(p);
    let can_run = fc.is_some() && !st.running && !chosen.is_empty();
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                can_run,
                egui::Button::new(RichText::new(t("out.btn_generate")).strong()),
            )
            .clicked()
        {
            if let Some(f) = fc {
                st.start(f, p, m, chosen.clone());
            }
        }
        if chosen.is_empty() {
            ui.label(RichText::new(t("out.no_parts")).weak());
        }
        if st.running {
            let label = if st.cancelling {
                t("out.cancelling")
            } else {
                t("out.btn_cancel")
            };
            if ui
                .add_enabled(!st.cancelling, egui::Button::new(label))
                .on_hover_text(t("out.btn_cancel_tip"))
                .clicked()
            {
                st.request_cancel();
            }
        }
    });

    if st.running {
        let elapsed = st.started.map(|t0| t0.elapsed()).unwrap_or_default();
        ui.horizontal(|ui| {
            ui.spinner();
            match &st.progress {
                Some(pr) => {
                    ui.label(progress_text(pr, elapsed));
                }
                None => {
                    ui.label(format!("{} {}", t("out.running"), format_elapsed(elapsed)));
                }
            }
        });
        if let Some(pr) = &st.progress {
            ui.add(egui::ProgressBar::new(progress_fraction(pr)).desired_width(360.0));
            if pr.phase == RunPhase::Assembly {
                ui.label(RichText::new(t("out.asm_slow_note")).weak());
            }
        }
    }

    if let Some(n) = &st.notice {
        crate::ui::components::banner(ui, crate::ui::components::Status::Warn, n);
    }
    if let Some(e) = &st.error {
        ui.colored_label(tk.danger, e);
    }
    if !st.log.is_empty() {
        ui.collapsing(t("out.log"), |ui| {
            for l in &st.log {
                ui.label(RichText::new(l).monospace().size(11.0));
            }
        });
    }

    let Some(sum) = st.summary.clone() else {
        return;
    };
    ui.separator();
    ui.strong(tf("out.done_fmt", &[sum.parts.len().to_string()]));
    for f in &sum.failed {
        ui.colored_label(tk.warning, format!("{} {}", t("out.failed"), f));
    }
    if ui.button(t("btn_open_folder")).clicked() {
        #[cfg(windows)]
        let _ = crate::cad::new_command("explorer")
            .arg(&sum.out_dir)
            .spawn();
    }
    if let Some(f) = fc {
        if let Some(gui) = &f.gui_path {
            if !sum.assembly_fcstd.is_empty() && ui.button(t("out.open_assembly")).clicked() {
                let _ = crate::cad::new_command(gui)
                    .arg(&sum.assembly_fcstd)
                    .spawn();
            }
        }
    }

    ui.add_space(6.0);
    ui.strong(t("out.bom_heading"));
    egui::Grid::new("out_bom").striped(true).show(ui, |ui| {
        for h in [
            t("out.bom.part"),
            t("out.bom.qty"),
            t("out.bom.material"),
            t("out.bom.unit_mass_g"),
            t("out.bom.total_mass_g"),
        ] {
            ui.strong(h);
        }
        ui.end_row();
        for r in &sum.bom {
            ui.label(&r.part);
            ui.label(r.qty.to_string());
            ui.label(&r.material);
            ui.label(
                r.unit_mass_g
                    .map(|v| format!("{v:.1}"))
                    .unwrap_or("-".into()),
            );
            ui.label(
                r.total_mass_g()
                    .map(|v| format!("{v:.1}"))
                    .unwrap_or("-".into()),
            );
            ui.end_row();
        }
    });
    ui.label(RichText::new(t("out.bom.note_solid")).weak());

    ui.add_space(6.0);
    ui.strong(t("out.report_heading"));
    ui.label(format!(
        "{}: {:.1} g",
        t("out.report.mass"),
        sum.mass_kg * 1000.0
    ));
    if sum.cg_mm.len() == 3 {
        ui.label(format!(
            "{}: X={:.1}, Y={:.1}, Z={:.1} mm",
            t("out.report.cg"),
            sum.cg_mm[0],
            sum.cg_mm[1],
            sum.cg_mm[2]
        ));
    }
    if sum.interferences.is_empty() {
        ui.colored_label(tk.success, t("out.report.no_interference"));
    } else {
        ui.colored_label(tk.danger, t("out.report.interference"));
        for i in &sum.interferences {
            ui.label(format!("{} x {}: {:.1} mm³", i.a, i.b, i.volume_mm3));
        }
    }
    ui.collapsing(
        tf("out.report.contacts_fmt", &[sum.contacts.len().to_string()]),
        |ui| {
            ui.label(RichText::new(t("out.report.contacts_hint")).weak());
            for c in &sum.contacts {
                ui.label(RichText::new(format!("{} - {}", c.a, c.b)).monospace());
            }
        },
    );
    if !sum.floating.is_empty() {
        ui.colored_label(
            tk.warning,
            format!("{}: {}", t("out.report.floating"), sum.floating.join(", ")),
        );
    }
    if !sum.unseated.is_empty() {
        ui.colored_label(
            tk.warning,
            format!("{}: {}", t("out.report.unseated"), sum.unseated.join(", ")),
        );
    }
    for f in &sum.check_failed {
        ui.colored_label(tk.warning, format!("{}: {f}", t("out.report.check_failed")));
    }

    // CAD kütlesi ile programın tahmini; tasarıma geri aktarma yalnızca kullanıcı onayıyla.
    // Program tarafı her karede güncel değerlerle; CAD tarafı üretim anındaki parçalardan.
    let mc = &mass_check(p, m, &sum.parts);
    let stale = design_fingerprint(p) != sum.fingerprint;
    if mc.cad_total_kg() > 0.0 {
        ui.add_space(6.0);
        ui.strong(t("out.back_heading"));
        egui::Grid::new("out_mass").striped(true).show(ui, |ui| {
            ui.label("");
            ui.strong("CAD [g]");
            ui.strong(format!("{} [g]", t("out.mass.program")));
            ui.end_row();
            if mc.target == MassTarget::FuseTail {
                ui.label(t("out.mass.wing"));
                ui.label(format!("{:.0}", mc.cad_wing_kg * 1000.0));
                ui.label(format!("{:.0}", mc.prog_wing_kg * 1000.0));
                ui.end_row();
            }
            ui.label(mass_target_label(mc.target));
            ui.label(format!("{:.0}", mc.cad_other_kg * 1000.0));
            ui.label(format!("{:.0}", mc.prog_other_kg * 1000.0));
            ui.end_row();
            ui.strong(t("out.mass.total"));
            ui.strong(format!("{:.0}", mc.cad_total_kg() * 1000.0));
            ui.strong(format!("{:.0}", mc.prog_total_kg() * 1000.0));
            ui.end_row();
        });
        ui.label(format!("{}: {:.2}", t("out.mass.ratio"), mc.ratio()));
        ui.label(RichText::new(t("out.back_note")).weak());
        let allowed = mc.transfer_allowed() && !stale;
        if stale {
            ui.colored_label(tk.warning, t("out.mass.stale"));
        }
        if !mc.complete {
            ui.colored_label(tk.warning, t("out.mass.incomplete"));
        } else if !mc.ratio_ok() {
            ui.colored_label(
                tk.warning,
                tf(
                    "out.mass.mismatch_fmt",
                    &[
                        format!("{:.2}", mc.ratio()),
                        format!("{MASS_RATIO_MIN:.1}"),
                        format!("{MASS_RATIO_MAX:.1}"),
                    ],
                ),
            );
        }
        if !st.confirm_apply {
            if ui
                .add_enabled(allowed, egui::Button::new(t("out.btn_apply_mass")))
                .clicked()
            {
                st.confirm_apply = true;
            }
        } else {
            ui.colored_label(
                tk.warning,
                tf(
                    "out.apply_confirm_fmt",
                    &[
                        mass_target_label(mc.target).to_string(),
                        format!("{:.0}", mc.prog_other_kg * 1000.0),
                        format!("{:.0}", mc.cad_other_kg * 1000.0),
                    ],
                ),
            );
            ui.horizontal(|ui| {
                if ui.button(t("out.apply_yes")).clicked() {
                    if let Some(old) = apply_mass_to_project(p, mc) {
                        st.undo = Some((mc.target, old, mc.cad_other_kg));
                        st.applied_msg = Some(t("out.applied").to_string());
                    }
                    st.confirm_apply = false;
                }
                if ui.button(t("btn_cancel")).clicked() {
                    st.confirm_apply = false;
                }
            });
        }
        if let Some(msg) = st.applied_msg.clone() {
            ui.horizontal(|ui| {
                ui.colored_label(tk.success, msg);
                if let Some((target, old, applied)) = st.undo {
                    if ui.button(t("out.btn_undo_mass")).clicked() {
                        st.applied_msg = Some(
                            if undo_mass_transfer(p, target, old, applied) {
                                t("out.undone")
                            } else {
                                t("out.undo_skipped")
                            }
                            .to_string(),
                        );
                        st.undo = None;
                    }
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cad::runner::StageInfo;
    use crate::i18n::{Lang, with_lang};
    use crate::ui::theme::{self, ThemeChoice};

    const TR_LETTERS: &str = "çğıöşüÇĞİÖŞÜ";

    fn has_turkish(s: &str) -> bool {
        s.chars().any(|c| TR_LETTERS.contains(c))
    }

    fn part_progress(cur: usize, total: usize) -> OutputsProgress {
        OutputsProgress {
            phase: RunPhase::Parts,
            current: cur,
            total,
            part: "Wing".into(),
            stage: None,
        }
    }

    fn asm_progress(stage: Option<StageInfo>) -> OutputsProgress {
        OutputsProgress {
            phase: RunPhase::Assembly,
            current: 6,
            total: 6,
            part: String::new(),
            stage,
        }
    }

    #[test]
    fn elapsed_text_formats() {
        assert_eq!(format_elapsed(Duration::from_secs(0)), "00:00");
        assert_eq!(format_elapsed(Duration::from_secs(75)), "01:15");
        assert_eq!(format_elapsed(Duration::from_secs(3725)), "1:02:05");
    }

    #[test]
    fn progress_text_shows_part_index_total_and_elapsed() {
        with_lang(Lang::En, || {
            let s = progress_text(&part_progress(3, 12), Duration::from_secs(83));
            assert!(
                s.contains("3/12") && s.contains("Wing") && s.contains("01:23"),
                "{s}"
            );
            assert!(!has_turkish(&s));
            let s = progress_text(
                &asm_progress(Some(StageInfo {
                    id: "pairs".into(),
                    done: Some(4),
                    total: Some(9),
                })),
                Duration::from_secs(10),
            );
            assert!(s.contains("Interference check 4/9"), "{s}");
            // Bilinmeyen aşama kimliği ekrana basılmaz.
            let s = progress_text(
                &asm_progress(Some(StageInfo {
                    id: "evil_stage".into(),
                    done: None,
                    total: None,
                })),
                Duration::from_secs(1),
            );
            assert!(!s.contains("evil_stage"), "{s}");
            let s = progress_text(&asm_progress(None), Duration::from_secs(1));
            assert!(s.contains("starting"), "{s}");
        });
        with_lang(Lang::Tr, || {
            let s = progress_text(&part_progress(1, 2), Duration::from_secs(5));
            assert!(s.starts_with("Parça 1/2"), "{s}");
        });
    }

    #[test]
    fn progress_fraction_is_monotonic_and_bounded() {
        let mut last = -1.0_f32;
        for cur in 1..=6 {
            let f = progress_fraction(&part_progress(cur, 6));
            assert!((0.0..=1.0).contains(&f) && f >= last, "{cur}: {f}");
            last = f;
        }
        for (d, tt) in [(0, 9), (4, 9), (9, 9)] {
            let f = progress_fraction(&asm_progress(Some(StageInfo {
                id: "pairs".into(),
                done: Some(d),
                total: Some(tt),
            })));
            assert!(
                (0.0..=1.0).contains(&f) && f >= last - 1e-6,
                "{d}/{tt}: {f}"
            );
            last = f;
        }
        assert!(progress_fraction(&part_progress(0, 0)).is_finite());
    }

    #[test]
    fn aborted_messages_are_localized_and_say_not_completed() {
        let lim = CadLimits::default();
        with_lang(Lang::En, || {
            let c = aborted_message(AbortReason::Cancelled, 2, 1, &lim);
            assert!(
                c.contains("cancelled") && c.contains("not completed"),
                "{c}"
            );
            assert!(!has_turkish(&c));
            let t = aborted_message(AbortReason::TotalTimeout, 2, 1, &lim);
            assert!(t.contains("60 min") && t.contains("Settings > CAD"), "{t}");
            assert!(!has_turkish(&t));
        });
        with_lang(Lang::Tr, || {
            let c = aborted_message(AbortReason::Cancelled, 0, 0, &lim);
            assert!(c.contains("iptal edildi"), "{c}");
        });
    }

    #[test]
    fn running_state_renders_in_both_themes_and_cancel_flags_the_run() {
        let p = Project::default();
        let m = crate::calc::evaluate(&p, &crate::calc::Library::embedded());
        let fc = FreeCadInstallation {
            cmd_path: PathBuf::from("fake"),
            gui_path: None,
            version: "FreeCAD test".into(),
        };
        for choice in [ThemeChoice::Dark, ThemeChoice::Light] {
            for lang in [Lang::Tr, Lang::En] {
                let ctx = egui::Context::default();
                theme::install(&ctx, choice);
                let mut st = OutputsUi::default();
                st.running = true;
                st.started = Some(Instant::now());
                st.cancel = Some(CancelFlag::new());
                st.progress = Some(asm_progress(Some(StageInfo {
                    id: "load".into(),
                    done: Some(2),
                    total: Some(8),
                })));
                st.notice = Some("not completed".into());
                with_lang(lang, || {
                    for _ in 0..2 {
                        let mut pp = p.clone();
                        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                            egui::CentralPanel::default().show(ui, |ui| {
                                show(ui, &mut pp, &m, &mut st, Some(&fc));
                            });
                        });
                        out.textures_delta.clear();
                    }
                });
                let flag = st.cancel.clone().unwrap();
                assert!(!flag.is_cancelled());
                st.request_cancel();
                assert!(st.cancelling && flag.is_cancelled());
            }
        }
    }

    /// Sahte FreeCADCmd (30 sn bekleyen toplu iş) ile arayüz yolu: Üret -> İptal -> "iptal edildi".
    #[cfg(windows)]
    #[test]
    fn fake_process_cancel_through_the_ui_state() {
        let p = Project::default();
        let m = crate::calc::evaluate(&p, &crate::calc::Library::embedded());
        let dir = std::env::temp_dir().join(format!("werseus_ui_cancel_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("fake_freecadcmd.cmd");
        std::fs::write(&fake, "@echo off\r\nping -n 30 127.0.0.1 >nul\r\n").unwrap();
        let fc = FreeCadInstallation {
            cmd_path: fake,
            gui_path: None,
            version: "fake".into(),
        };
        let mut st = OutputsUi::default();
        st.out_dir = dir.join("out");
        let chosen = vec![crate::cad::outputs::PartId::Wing];
        st.start(&fc, &p, &m, chosen);
        assert!(st.running);
        std::thread::sleep(Duration::from_millis(1200));
        st.request_cancel();
        let t0 = Instant::now();
        while st.running {
            assert!(t0.elapsed() < Duration::from_secs(30), "iptal sonuçlanmadı");
            std::thread::sleep(Duration::from_millis(50));
            st.poll();
        }
        assert!(st.summary.is_none() && st.error.is_none());
        let n = st.notice.clone().expect("iptal iletisi");
        assert!(n.contains("iptal") || n.contains("cancel"), "{n}");
        assert!(!st.cancelling && st.cancel.is_none() && st.progress.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Çıkışta iptal: sahte uzun süreç sınırlı sürede ölür, kapanış kilitlenmez.
    #[cfg(windows)]
    #[test]
    fn shutdown_cancels_running_process_within_bound() {
        let p = Project::default();
        let m = crate::calc::evaluate(&p, &crate::calc::Library::embedded());
        let dir = std::env::temp_dir().join(format!("werseus_ui_exit_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("fake_freecadcmd.cmd");
        std::fs::write(&fake, "@echo off\r\nping -n 30 127.0.0.1 >nul\r\n").unwrap();
        let fc = FreeCadInstallation {
            cmd_path: fake,
            gui_path: None,
            version: "fake".into(),
        };
        let mut st = OutputsUi::default();
        st.out_dir = dir.join("out");
        st.start(&fc, &p, &m, vec![crate::cad::outputs::PartId::Wing]);
        assert!(st.running);
        std::thread::sleep(Duration::from_millis(1200));
        let t0 = Instant::now();
        let ok = st.shutdown(SHUTDOWN_WAIT);
        let took = t0.elapsed();
        assert!(ok && !st.running, "süreç sınırlı sürede sonlanmadı");
        assert!(took < Duration::from_secs(4), "{took:?}");
        // Ağaç dahil: bu klasöre ait ping artığı kalmamalı (cmd + ping zinciri öldürüldü).
        assert!(st.notice.is_some());
        // Çalışmıyorsa anında döner.
        assert!(st.shutdown(Duration::from_millis(1)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Üretim başlarken alınan limit iletide görünür; çalışırken ayar değişse de.
    #[test]
    fn total_timeout_message_uses_limits_captured_at_start() {
        let mut st = OutputsUi::default();
        st.run_limits = CadLimits {
            part_secs: 300,
            assembly_secs: 900,
            total_secs: 7200,
        };
        st.limits.total_secs = 300; // çalışırken Ayarlar'da değiştirildi
        let (tx, rx) = std::sync::mpsc::channel();
        st.rx = Some(rx);
        st.running = true;
        tx.send(RunEvent::Aborted {
            reason: AbortReason::TotalTimeout,
            completed: 1,
            removed: 0,
        })
        .unwrap();
        st.poll();
        let n = st.notice.clone().expect("ileti");
        assert!(n.contains("120") && !n.contains("(5 "), "{n}");
        assert!(!st.running);
    }
}
