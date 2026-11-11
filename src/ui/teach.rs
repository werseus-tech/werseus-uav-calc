// SPDX-License-Identifier: AGPL-3.0-or-later
//! Öğretmen Modu Arayüzü: Ders okuyucu, seviye seçimi, etkileşimli mini testler,
//! 'Neden bu sonuç çıktı?' penceresi ve Sınıf Raporu dışa aktarma paneli.

use crate::calc::Model;
use crate::i18n::{Lang, current_lang, load_settings, save_settings, t, tf};
use crate::project::{DroneType, Project};
use crate::teach::{
    ResultMetric, StaticExplanation, StudentLevel, TeacherSettings, all_lessons,
    build_socratic_teacher_prompts, explain_metric, generate_classroom_report, lesson_for_id,
    lesson_for_step,
};
use crate::ui::components::{self, Status, section_header};
use crate::ui::panel_kit::tinted_frame;
use crate::ui::theme::{self, space};
use crate::ui::wizard::StepId;
use egui::{RichText, Ui};
use std::sync::Arc;

/// Öğretmen Modu arayüz durum yöneticisi.
pub struct TeacherUiState {
    pub is_open: bool,
    pub selected_lesson_id: u8,
    pub why_dialog: Option<(ResultMetric, StaticExplanation)>,
    pub show_report_dialog: bool,
    pub report_content: String,
    pub teacher_settings: TeacherSettings,
    pub status_feedback: Option<String>,
}

impl TeacherUiState {
    pub fn new(settings: TeacherSettings) -> Self {
        Self {
            is_open: false,
            selected_lesson_id: 1,
            why_dialog: None,
            show_report_dialog: false,
            report_content: String::new(),
            teacher_settings: settings,
            status_feedback: None,
        }
    }

    /// Belirtilen sihirbaz adımıyla eşleşen dersi açar.
    pub fn open_for_step(&mut self, step: StepId, dt: DroneType) {
        self.selected_lesson_id = lesson_for_step(step, dt);
        self.is_open = true;
    }

    /// Bir sonuç metriği için 'Neden bu sonuç çıktı?' penceresini açar.
    pub fn open_why(&mut self, metric: ResultMetric, p: &Project, m: &Model, lang: Lang) {
        let expl = explain_metric(metric, p, m, self.teacher_settings.level, lang);
        self.why_dialog = Some((metric, expl));
    }

    /// Ayarları diske (AppSettings) yazar.
    pub fn persist_settings(&self) {
        let mut app_settings = load_settings();
        app_settings.teacher = self.teacher_settings.clone();
        save_settings(&app_settings);
    }

    /// Sınıf raporunu hazırlar ve diyalogu açar.
    pub fn open_classroom_report(&mut self, p: &Project, m: &Model, lang: Lang) {
        self.report_content = generate_classroom_report(p, m, &self.teacher_settings, lang);
        self.show_report_dialog = true;
    }

    /// Ana egui arayüz çizimi.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        p: &Project,
        m: &Model,
        ai_state: &mut crate::ai::state::AiAssistantState,
        ai_config: &crate::ai::config::AiConfig,
        cred_store: Arc<dyn crate::ai::credential::CredentialStorage>,
    ) {
        let lang = current_lang();

        // 1. Ana Öğretmen Modu Penceresi
        if self.is_open {
            let mut is_open = self.is_open;
            egui::Window::new(t("teach.window_title"))
                .open(&mut is_open)
                .default_size([880.0, 640.0])
                .min_width(600.0)
                .min_height(450.0)
                .resizable(true)
                .show(ctx, |ui| {
                    self.render_teacher_window_content(ui, p, m, lang);
                });
            self.is_open = is_open;
        }

        // 2. 'Neden bu sonuç çıktı?' Diyaloğu
        if let Some((metric, expl)) = self.why_dialog.clone() {
            let mut why_open = true;
            egui::Window::new(t("teach.why_modal_title"))
                .open(&mut why_open)
                .default_size([560.0, 480.0])
                .resizable(true)
                .collapsible(false)
                .show(ctx, |ui| {
                    self.render_why_content(
                        ui, metric, &expl, p, m, lang, ai_state, ai_config, cred_store,
                    );
                });
            if !why_open {
                self.why_dialog = None;
            }
        }

        // 3. Sınıf Raporu Diyaloğu
        if self.show_report_dialog {
            let mut report_open = self.show_report_dialog;
            egui::Window::new(t("teach.report_title"))
                .open(&mut report_open)
                .default_size([720.0, 560.0])
                .resizable(true)
                .show(ctx, |ui| {
                    self.render_report_dialog_content(ui, ctx, p, m, lang);
                });
            self.show_report_dialog = report_open;
        }
    }

    fn render_teacher_window_content(&mut self, ui: &mut Ui, p: &Project, m: &Model, lang: Lang) {
        let tk = theme::tokens(ui);
        // Üst Araç Çubuğu: Seviye seçimi, Öğrenci Adı ve Sınıf Raporu düğmesi
        ui.horizontal(|ui| {
            ui.label(RichText::new(t("teach.level_label")).strong());
            let prev_level = self.teacher_settings.level;
            egui::ComboBox::from_id_salt("teacher_student_level_combo")
                .selected_text(self.teacher_settings.level.label(lang))
                .show_ui(ui, |ui| {
                    for lvl in StudentLevel::ALL {
                        ui.selectable_value(&mut self.teacher_settings.level, lvl, lvl.label(lang));
                    }
                });
            if self.teacher_settings.level != prev_level {
                self.persist_settings();
            }

            ui.separator();

            ui.label(t("teach.student_name_label"));
            let name_edit = ui.add(
                egui::TextEdit::singleline(&mut self.teacher_settings.student_name)
                    .hint_text(t("teach.student_name_placeholder"))
                    .desired_width(180.0),
            );
            if name_edit.lost_focus() {
                self.persist_settings();
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(
                        RichText::new(t("teach.btn_classroom_report"))
                            .strong()
                            .color(tk.accent),
                    )
                    .clicked()
                {
                    self.open_classroom_report(p, m, lang);
                }
            });
        });

        ui.add_space(space::XS / 2.0);
        ui.label(
            RichText::new(self.teacher_settings.level.description(lang))
                .small()
                .color(tk.text_weak),
        );

        ui.separator();

        // İki sütunlu düzen: Sol: Ders Yolu Listesi, Sağ: Seçili Ders Okuyucu & Quiz
        ui.columns(2, |cols| {
            // SOL SÜTUN: Ders Yolu
            section_header(&mut cols[0], t("teach.path_heading"));

            let completed_count = self.teacher_settings.completed_count();
            let progress_pct = (self.teacher_settings.progress_ratio() * 100.0) as u32;
            cols[0].label(tf(
                "teach.progress_fmt",
                &[format!("{completed_count}"), format!("{progress_pct}")],
            ));
            cols[0].add(
                egui::ProgressBar::new(self.teacher_settings.progress_ratio())
                    .desired_width(cols[0].available_width()),
            );
            cols[0].add_space(space::S);

            let lessons = all_lessons(lang);
            egui::ScrollArea::vertical()
                .id_salt("teacher_lesson_list_scroll")
                .max_height(480.0)
                .show(&mut cols[0], |ui| {
                    for l in &lessons {
                        let is_done = self.teacher_settings.is_lesson_completed(l.id);
                        let is_active = self.selected_lesson_id == l.id;

                        let status_prefix = if is_done { "[OK]" } else { "[•]" };
                        let (correct, ans) = self.teacher_settings.quiz_score_for_lesson(l);
                        let score_tag = if ans > 0 {
                            format!(" ({correct}/3)")
                        } else {
                            String::new()
                        };

                        let item_text = format!("{status_prefix} {}. {}{score_tag}", l.id, l.title);
                        let text_color = if is_active {
                            tk.accent
                        } else if is_done {
                            tk.success
                        } else {
                            tk.text
                        };

                        if ui
                            .selectable_label(is_active, RichText::new(item_text).color(text_color))
                            .clicked()
                        {
                            self.selected_lesson_id = l.id;
                        }
                    }
                });

            // SAĞ SÜTUN: Seçili Ders Okuyucu & Etkileşimli Mini Test
            let selected_id = self.selected_lesson_id;
            if let Some(lesson) = lesson_for_id(selected_id, lang) {
                egui::ScrollArea::vertical()
                    .id_salt("teacher_lesson_body_scroll")
                    .max_height(520.0)
                    .show(&mut cols[1], |ui| {
                        // Ders Başlığı ve Tamamlama Düğmesi
                        ui.horizontal(|ui| {
                            ui.heading(RichText::new(&lesson.title).strong().size(16.0));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let is_done =
                                        self.teacher_settings.is_lesson_completed(lesson.id);
                                    let btn_text = if is_done {
                                        t("teach.btn_mark_incompleted")
                                    } else {
                                        t("teach.btn_mark_completed")
                                    };
                                    let btn_color = if is_done { tk.danger } else { tk.success };

                                    if ui
                                        .button(RichText::new(btn_text).color(btn_color))
                                        .clicked()
                                    {
                                        self.teacher_settings
                                            .set_lesson_completed(lesson.id, !is_done);
                                        self.persist_settings();
                                    }
                                },
                            );
                        });

                        ui.add_space(space::XS);

                        // 1. Ne Öğreneceksin
                        if !lesson.what_you_will_learn.is_empty() {
                            tinted_frame(tk, tk.info_soft).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.strong(RichText::new(t("teach.sec_what_learn")).color(tk.info));
                                for item in &lesson.what_you_will_learn {
                                    ui.label(format!("• {item}"));
                                }
                            });
                            ui.add_space(space::XS);
                        }

                        // 2. Anlatım
                        if !lesson.explanation.is_empty() {
                            egui::CollapsingHeader::new(
                                RichText::new(t("teach.sec_explanation")).strong(),
                            )
                            .id_salt(("lesson_expl", lesson.id))
                            .default_open(true)
                            .show(ui, |ui| {
                                ui.label(&lesson.explanation);
                            });
                            ui.add_space(space::XS);
                        }

                        // 3. Programda Nerede Görürsün
                        if !lesson.where_in_program.is_empty() {
                            tinted_frame(tk, tk.warning_soft).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.strong(
                                    RichText::new(t("teach.sec_where_in_program"))
                                        .color(tk.warning),
                                );
                                ui.label(&lesson.where_in_program);
                            });
                            ui.add_space(space::XS);
                        }

                        // 4. Mini Deney
                        if !lesson.mini_experiment.is_empty() {
                            tinted_frame(tk, tk.success_soft).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.strong(
                                    RichText::new(t("teach.sec_mini_experiment")).color(tk.success),
                                );
                                ui.label(&lesson.mini_experiment);
                            });
                            ui.add_space(space::XS);
                        }

                        // 5. Sık Yapılan Yanlışlar
                        if !lesson.misconceptions.is_empty() {
                            egui::CollapsingHeader::new(
                                RichText::new(t("teach.sec_misconceptions")).strong(),
                            )
                            .id_salt(("lesson_misc", lesson.id))
                            .default_open(false)
                            .show(ui, |ui| {
                                for item in &lesson.misconceptions {
                                    ui.label(format!("• {item}"));
                                }
                            });
                            ui.add_space(space::XS);
                        }

                        // 6. Etkileşimli Mini Test
                        let (correct, _answered) =
                            self.teacher_settings.quiz_score_for_lesson(&lesson);
                        let quiz_len = lesson.quiz.len();
                        components::section::section_header_with(ui, t("teach.sec_quiz"), |ui| {
                            ui.strong(
                                RichText::new(tf(
                                    "teach.quiz_score_fmt",
                                    &[format!("{correct}"), format!("{quiz_len}")],
                                ))
                                .color(tk.accent),
                            );
                        });
                        ui.add_space(space::XS / 2.0);

                        for q in &lesson.quiz {
                            tinted_frame(tk, tk.surface_alt).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.strong(format!("{}. {}", q.number, q.question));
                                ui.add_space(space::XS);

                                let chosen_opt =
                                    self.teacher_settings.get_answer(lesson.id, q.number);

                                for (opt_idx, opt_text) in q.options.iter().enumerate() {
                                    let is_selected = chosen_opt == Some(opt_idx);
                                    let opt_letter =
                                        crate::teach::quiz::QuizQuestion::option_letter(opt_idx);
                                    let radio_label = format!("{opt_letter}) {opt_text}");

                                    if ui.radio(is_selected, radio_label).clicked() {
                                        self.teacher_settings
                                            .set_answer(lesson.id, q.number, opt_idx);
                                        self.persist_settings();
                                    }
                                }

                                // Seçim yapıldıysa anında doğru/yanlış ve açıklama gösterimi
                                if let Some(selected) = chosen_opt {
                                    ui.add_space(space::XS);
                                    let is_correct = q.is_correct(selected);
                                    if is_correct {
                                        ui.colored_label(tk.success, t("teach.quiz_correct"));
                                    } else {
                                        let correct_letter =
                                            crate::teach::quiz::QuizQuestion::option_letter(
                                                q.correct_index,
                                            );
                                        ui.colored_label(tk.danger, t("teach.quiz_incorrect"));
                                        ui.label(
                                            RichText::new(
                                                tf(
                                                    "teach.quiz_correct_is",
                                                    &[format!("{correct_letter}")],
                                                )
                                                .to_string(),
                                            )
                                            .strong(),
                                        );
                                    }
                                    ui.add_space(space::XS / 2.0);
                                    ui.label(
                                        RichText::new(&q.explanation).small().color(tk.text_weak),
                                    );
                                }
                            });
                            ui.add_space(space::XS);
                        }
                    });
            }
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn render_why_content(
        &mut self,
        ui: &mut Ui,
        metric: ResultMetric,
        expl: &StaticExplanation,
        p: &Project,
        m: &Model,
        lang: Lang,
        ai_state: &mut crate::ai::state::AiAssistantState,
        ai_config: &crate::ai::config::AiConfig,
        cred_store: Arc<dyn crate::ai::credential::CredentialStorage>,
    ) {
        let tk = theme::tokens(ui);
        ui.heading(RichText::new(&expl.metric_name).strong());
        ui.add_space(space::XS / 2.0);

        // Hesaplanan Değer
        ui.horizontal(|ui| {
            ui.label(t("teach.calculated_val_label"));
            ui.strong(
                RichText::new(&expl.current_value_str)
                    .size(theme::font::SUBHEAD + 1.5)
                    .color(tk.success),
            );
        });

        ui.separator();

        // 1. Matematiksel Formül
        ui.strong(t("teach.why_formula_label"));
        tinted_frame(tk, tk.surface_alt).show(ui, |ui| {
            ui.monospace(RichText::new(&expl.formula).strong().color(tk.text));
        });

        ui.add_space(space::XS);

        // 2. Seviyeye Göre Sezgisel / Mühendislik Açıklaması
        ui.label(&expl.intuition);
        ui.add_space(space::S);

        // 3. Girdi Parametrelerinin Etkileri
        ui.strong(t("teach.why_effects_label"));
        for eff in &expl.input_effects {
            ui.label(eff);
        }

        ui.add_space(space::S);
        ui.separator();

        // Alt Çubuk: İlgili Derse Git ve Sokratik Yapay Zekâ Öğretmen
        ui.horizontal(|ui| {
            if ui
                .button(format!(
                    "{} {}",
                    t("teach.read_related_lesson"),
                    expl.related_lesson_title
                ))
                .clicked()
            {
                self.selected_lesson_id = expl.related_lesson_id;
                self.is_open = true;
                self.why_dialog = None;
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(t("teach.btn_close")).clicked() {
                    self.why_dialog = None;
                }

                if ai_config.enabled {
                    let btn_text = t("teach.ask_socratic_ai");
                    if ui
                        .button(RichText::new(btn_text).color(tk.accent).strong())
                        .clicked()
                    {
                        let (sys, user) = build_socratic_teacher_prompts(
                            metric,
                            p,
                            m,
                            self.teacher_settings.level,
                            lang,
                        );
                        // AI paneline soru yönlendir
                        ai_state.is_panel_open = true;
                        ai_state.start_socratic_teacher_request(
                            ai_config,
                            p,
                            sys,
                            user,
                            cred_store.clone(),
                        );
                        self.why_dialog = None;
                    }
                }
            });
        });
    }

    fn render_report_dialog_content(
        &mut self,
        ui: &mut Ui,
        ctx: &egui::Context,
        _p: &Project,
        _m: &Model,
        _lang: Lang,
    ) {
        ui.horizontal(|ui| {
            ui.heading(t("teach.report_title"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(t("teach.btn_close")).clicked() {
                    self.show_report_dialog = false;
                }

                if ui.button(t("teach.btn_copy_clipboard")).clicked() {
                    ctx.copy_text(self.report_content.clone());
                    self.status_feedback = Some(t("teach.report_copied").to_string());
                }

                if ui.button(t("teach.btn_save_file")).clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .set_file_name("sinif_raporu.md")
                        .add_filter("Markdown", &["md", "markdown"])
                        .save_file()
                    {
                        if std::fs::write(&path, &self.report_content).is_ok() {
                            self.status_feedback =
                                Some(tf("teach.report_saved", &[path.display().to_string()]));
                        }
                    }
                }
            });
        });

        if let Some(ref fb) = self.status_feedback {
            components::banner(ui, Status::Ok, fb);
        }

        ui.separator();

        egui::ScrollArea::vertical()
            .id_salt("teacher_report_preview_scroll")
            .max_height(460.0)
            .show(ui, |ui| {
                ui.monospace(&self.report_content);
            });
    }
}
