// SPDX-License-Identifier: AGPL-3.0-or-later
//! Üç panelli kabuk: üst çubuk, sol adım paneli + etkin adımın girdileri, merkezde adım başlığı ve
//! canlı görünüm (3D model / 2D şema / adıma özgü grafik), sağda adımın sonuçları + canlı analiz.
//!
//! Tüm adım ekranları bu düzene taşındı (`ui::steps`). Merkezin altında yalnız başka paketin
//! içeriği (Çıktılar, rüzgâr tüneli) ya da dar pencerede/kapalı panelde gösterilemeyen
//! girdi/sonuçlar için bir çalışma alanı açılır.
//!
//! Durum mimarisi:
//! - Proje verisi ve sihirbaz durumu tek kaynaktır (`Project`); gezinme `nav` üzerinden yapılır.
//! - Kabuğa özgü görünüm durumu (`ShellState`: panel tercihleri, 3D kamera) projeye yazılmaz,
//!   böylece kayıt dosyası ve hesaplar etkilenmez.
//! - Paneller bir "istek" listesi döndürür (`ShellActions`), `App` bunları kare sonunda uygular
//!   (öğretmen penceresi, yapay zekâ isteği, adıma gitme). Bu, eski ekranlardaki kalıbın aynısıdır.
pub mod analysis;
pub mod layout;
pub mod nav;
#[cfg(test)]
mod overflow_tests;

use super::components::{self, StepVisual, step_item, step_item_with_tip};
use super::theme::{self, ThemeChoice, font, radius, space};
use super::viewport3d::{self, ViewportState};
use super::wizard::{self, StepId};
use super::{App, Tab};
use crate::calc::Model;
use crate::i18n::{t, tf};
use crate::project::{DroneType, Project};
use egui::{RichText, Ui};
use layout::ShellPrefs;

/// Kabuğun oturumluk görünüm durumu.
pub struct ShellState {
    pub prefs: ShellPrefs,
    pub viewport: ViewportState,
    pub show_info_notes: bool,
    pub theme: ThemeChoice,
    /// Son karede sol/sağ panelin dış dikdörtgeni (taşma testleri ve hata ayıklama için).
    /// `left_inputs`: sol paneldeki adım girdileri kutusu.
    pub panel_rects: PanelRects,
}

/// Son karedeki panel dikdörtgenleri (yalnız ölçüm; hesaba/kayda girmez).
#[derive(Clone, Copy, Debug, Default)]
pub struct PanelRects {
    pub left: Option<egui::Rect>,
    pub right: Option<egui::Rect>,
    pub left_inputs: Option<egui::Rect>,
}

impl ShellState {
    pub fn new(theme: ThemeChoice) -> Self {
        Self {
            prefs: ShellPrefs::default(),
            viewport: ViewportState::default(),
            show_info_notes: false,
            theme,
            panel_rects: PanelRects::default(),
        }
    }
}

/// Bu karede panellerden gelen istekler.
#[derive(Default)]
pub struct ShellActions {
    pub go_to: Option<StepId>,
    pub teach_step: Option<StepId>,
    pub teach_why: Option<crate::teach::ResultMetric>,
    pub fix_warning: Option<String>,
}

impl ShellActions {
    /// Uyarı listesinden/analiz panelinden gelen istekleri ekler (boş olanlar mevcutları ezmez).
    pub fn merge(&mut self, a: analysis::AnalysisActions) {
        if a.go_to.is_some() {
            self.go_to = a.go_to;
        }
        if a.why.is_some() {
            self.teach_why = a.why;
        }
        if a.fix_warning.is_some() {
            self.fix_warning = a.fix_warning;
        }
    }
}

/// Merkezde görünümün altında çalışma alanı gerekiyor mu? (başka paketin içeriği ya da
/// dar pencerede/kapalı panelde gösterilemeyen girdi/sonuçlar)
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CenterNeeds {
    pub slot: Option<super::steps::CenterSlot>,
    pub inputs: bool,
    pub results: bool,
}

impl CenterNeeds {
    pub fn any(&self) -> bool {
        self.slot.is_some() || self.inputs || self.results
    }
}

/// Etkin adım (sihirbazda geçerli adım; uzman modunda sekmenin adım karşılığı).
pub fn active_step(p: &Project, tab: Tab) -> Option<StepId> {
    if p.expert_mode {
        super::simple_steps::step_for_tab(tab, p.drone_type)
    } else {
        Some(nav::current_step(p))
    }
}

impl App {
    /// Üst çubuk: marka, proje menüsü ve adı, araç türü, mod, dil, tema, öğretmen, YZ, dışa aktar.
    pub(crate) fn shell_top_bar(&mut self, ui: &mut Ui, model: &Model) {
        let tk = theme::tokens(ui);
        let compact = layout::topbar_compact(ui.available_width());
        egui::MenuBar::new().ui(ui, |ui| {
            ui.spacing_mut().item_spacing.x = space::S;
            ui.label(
                RichText::new("WerSeuS")
                    .size(font::SUBHEAD)
                    .strong()
                    .color(tk.accent),
            )
            .on_hover_text("WerSeuS – UAV Calc");
            self.project_menu(ui, model);
            if compact {
                self.more_menu(ui);
            }
            ui.add(
                egui::TextEdit::singleline(&mut self.project.name)
                    .desired_width(if compact { 120.0 } else { 170.0 })
                    .hint_text(t("project_name_label")),
            )
            .on_hover_text(t("project_name_label"));
            if ui
                .button(tf("btn_platform_fmt", &[self.project.drone_type.label()]))
                .on_hover_text(t("menu_change_type"))
                .clicked()
            {
                self.show_type_modal = true;
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = space::S - 2.0;
                // Sağdan sola: panel düğmeleri, dışa aktar, YZ, öğretmen, tema, dil, mod.
                components::toggle_chip(
                    ui,
                    &mut self.shell.prefs.right_open,
                    t("ui.top.analysis"),
                    t("ui.top.analysis_tip"),
                );
                components::toggle_chip(
                    ui,
                    &mut self.shell.prefs.left_open,
                    t("ui.top.steps"),
                    t("ui.top.steps_tip"),
                );
                ui.separator();
                ui.menu_button(t("ui.top.export"), |ui| {
                    if ui.button(t("menu_export_markdown")).clicked() {
                        self.export_report_markdown(model);
                        ui.close();
                    }
                    if ui.button(t("menu_export_excel")).clicked() {
                        self.export_report_excel(model);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button(t("ui.top.export_outputs")).clicked() {
                        let mut tab = self.tab;
                        nav::go_to_step(&mut self.project, &mut tab, model, StepId::Outputs);
                        self.tab = tab;
                        ui.close();
                    }
                });
                ui.menu_button(t("menu_ai"), |ui| {
                    if ui.button(t("menu_ai_panel")).clicked() {
                        self.ai_state.is_panel_open = !self.ai_state.is_panel_open;
                        ui.close();
                    }
                    if ui.button(t("menu_ai_settings")).clicked() {
                        crate::ui::settings_page::set_section(
                            ui.ctx(),
                            crate::ui::settings_page::SettingsSection::Ai,
                        );
                        self.show_settings_modal = true;
                        ui.close();
                    }
                });
                if !compact {
                    if ui.button(t("menu_teacher_mode")).clicked() {
                        self.teacher_state.is_open = true;
                    }
                    self.theme_button(ui);
                    self.lang_menu(ui);
                    if ui.button(t("menu_settings")).clicked() {
                        self.show_settings_modal = true;
                    }
                    self.help_menu(ui);
                }
                ui.separator();
                self.expert_toggle(ui, model);
                let mut simple = self.project.aero_simple.simple_mode;
                if components::segmented(
                    ui,
                    &mut simple,
                    &[
                        (true, t("aeros.mode_simple"), t("aeros.mode_simple.desc")),
                        (
                            false,
                            t("aeros.mode_advanced"),
                            t("aeros.mode_advanced.desc"),
                        ),
                    ],
                ) {
                    self.project.aero_simple.simple_mode = simple;
                }
            });
        });
    }

    fn project_menu(&mut self, ui: &mut Ui, model: &Model) {
        ui.menu_button(t("menu_project"), |ui| {
            if ui.button(t("menu_new")).clicked() {
                self.new_project();
                ui.close();
            }
            if ui.button(t("menu_change_type")).clicked() {
                self.show_type_modal = true;
                ui.close();
            }
            if ui.button(t("menu_open")).clicked() {
                self.open();
                ui.close();
            }
            if ui.button(t("menu_save")).clicked() {
                self.save(false);
                ui.close();
            }
            if ui.button(t("menu_save_as")).clicked() {
                self.save(true);
                ui.close();
            }
            ui.separator();
            if ui.button(t("menu_export_markdown")).clicked() {
                self.export_report_markdown(model);
                ui.close();
            }
            if ui.button(t("menu_export_excel")).clicked() {
                self.export_report_excel(model);
                ui.close();
            }
            ui.menu_button(t("menu_examples"), |ui| {
                for (file, key) in [
                    ("fixed_wing_2kg_1600mm.json", "example_fixed_wing_label"),
                    ("flying_wing_1200mm.json", "example_flying_wing_label"),
                    ("quad_250_x.json", "example_quad_250_label"),
                    ("quad_275_x.json", "example_quad_275_label"),
                    ("vtol_quadplane.json", "example_vtol_label"),
                ] {
                    if ui.button(t(key)).clicked() {
                        self.load_example_project(file, t(key));
                        ui.close();
                    }
                }
            });
        });
    }

    /// Dar pencerede yardım/ayarlar/öğretmen/tema/dil tek menüde.
    fn more_menu(&mut self, ui: &mut Ui) {
        ui.menu_button(t("ui.top.more"), |ui| {
            if ui.button(t("menu_teacher_mode")).clicked() {
                self.teacher_state.is_open = true;
                ui.close();
            }
            if ui.button(t("menu_settings")).clicked() {
                self.show_settings_modal = true;
                ui.close();
            }
            self.theme_button(ui);
            self.lang_menu(ui);
            self.help_menu(ui);
        });
    }

    fn help_menu(&mut self, ui: &mut Ui) {
        ui.menu_button(t("menu_help"), |ui| {
            if ui.button(t("menu_about")).clicked() {
                self.show_about_modal = true;
                ui.close();
            }
            if ui.button(t("menu_report_bug")).clicked() {
                self.show_feedback_modal = true;
                ui.close();
            }
            ui.separator();
            if ui.button(t("menu_uninstall")).clicked() {
                self.uninstall_categories = crate::footprint::scan_footprint();
                self.uninstall_confirm_text.clear();
                self.uninstall_done_msg = None;
                self.show_uninstall_modal = true;
                ui.close();
            }
        });
    }

    fn lang_menu(&mut self, ui: &mut Ui) {
        let cur = crate::i18n::current_lang();
        let label = match cur {
            crate::i18n::Lang::Tr => "TR",
            crate::i18n::Lang::En => "EN",
        };
        ui.menu_button(label, |ui| {
            if ui
                .selectable_label(cur == crate::i18n::Lang::Tr, "Türkçe (TR)")
                .clicked()
            {
                crate::i18n::set_lang(crate::i18n::Lang::Tr);
                ui.close();
            }
            if ui
                .selectable_label(cur == crate::i18n::Lang::En, "English (EN)")
                .clicked()
            {
                crate::i18n::set_lang(crate::i18n::Lang::En);
                ui.close();
            }
        })
        .response
        .on_hover_text(t("ui.top.lang_tip"));
    }

    fn theme_button(&mut self, ui: &mut Ui) {
        let label = match self.shell.theme {
            ThemeChoice::Dark => t("ui.top.theme_light"),
            ThemeChoice::Light => t("ui.top.theme_dark"),
        };
        if ui
            .button(label)
            .on_hover_text(t("ui.top.theme_tip"))
            .clicked()
        {
            self.shell.theme = self.shell.theme.toggled();
            theme::install(ui.ctx(), self.shell.theme);
            let mut s = crate::i18n::load_settings();
            s.theme = self.shell.theme;
            crate::i18n::save_settings(&s);
        }
    }

    /// Uzman (serbest gezinme) anahtarı: tüm adımlar geçerli değilse kilitli (eski kural).
    fn expert_toggle(&mut self, ui: &mut Ui, model: &Model) {
        if wizard::all_steps_valid(&self.project, model) {
            let mut on = self.project.expert_mode;
            if components::toggle_chip(ui, &mut on, t("expert_mode"), t("ui.top.expert_tip"))
                .changed()
            {
                self.project.expert_mode = on;
                if on && !Tab::tabs_for(self.project.drone_type).contains(&self.tab) {
                    self.tab = Tab::Summary;
                }
            }
        } else {
            let missing = wizard::first_invalid_step(&self.project, model)
                .map(|(_, s, _)| s.title())
                .unwrap_or(t("wizard.missing_step"));
            let mut dummy = false;
            ui.add_enabled(false, egui::Checkbox::new(&mut dummy, t("expert_mode")))
                .on_disabled_hover_text(tf("wizard.expert_mode_missing_tooltip", &[missing]));
        }
    }

    /// Sol panel: adım listesi + (taşınmış ekranlarda) girdiler.
    pub(crate) fn shell_left_panel(
        &mut self,
        ui: &mut Ui,
        model: &Model,
        rail: bool,
        act: &mut ShellActions,
    ) {
        let tk = theme::tokens(ui);
        wizard::sync_wizard_steps(&mut self.project, model);
        let entries = nav::entries(&self.project, model, self.tab);
        if rail {
            ui.add_space(space::S);
            egui::ScrollArea::vertical().show(ui, |ui| {
                for e in &entries {
                    let r = step_item(ui, e.number, e.title, e.visual, e.selected, true);
                    if r.clicked() && e.clickable {
                        let mut tab = self.tab;
                        nav::select(&mut self.project, &mut tab, model, e.target);
                        self.tab = tab;
                    }
                }
            });
            return;
        }

        ui.add_space(space::S);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(t("ui.left.title"))
                    .size(font::HEADING)
                    .strong()
                    .color(tk.text),
            );
            if !self.project.expert_mode {
                let total = entries.len();
                let cur = nav::current_index(&self.project) + 1;
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    components::badge(ui, components::Status::Info, &format!("{cur} / {total}"));
                });
            }
        });
        if self.project.expert_mode {
            ui.label(
                RichText::new(t("ui.left.expert_note"))
                    .size(font::CAPTION)
                    .color(tk.text_weak),
            );
        } else {
            let done = entries
                .iter()
                .filter(|e| e.visual == StepVisual::Done)
                .count();
            let frac = if entries.is_empty() {
                0.0
            } else {
                done as f32 / entries.len() as f32
            };
            ui.add(
                egui::ProgressBar::new(frac)
                    .desired_height(4.0)
                    .fill(tk.accent),
            );
        }
        ui.add_space(space::XS);

        egui::ScrollArea::vertical()
            .id_salt("left_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for e in &entries {
                    let tip = if e.visual == StepVisual::Locked {
                        wizard::first_invalid_step(&self.project, model)
                            .map(|(_, s, err)| tf("wizard.locked_step_tooltip", &[s.title(), err]))
                            .unwrap_or_default()
                    } else if !e.clickable {
                        t("ui.left.visit_in_order").to_string()
                    } else {
                        String::new()
                    };
                    let r = step_item_with_tip(
                        ui, e.number, e.title, e.visual, e.selected, false, &tip,
                    );
                    if r.clicked() && e.clickable {
                        let mut tab = self.tab;
                        nav::select(&mut self.project, &mut tab, model, e.target);
                        self.tab = tab;
                    }
                }

                let screen = super::steps::current_screen(&self.project, self.tab);
                if super::steps::has_inputs(screen) {
                    ui.add_space(space::M);
                    let simple_step = active_step(&self.project, self.tab);
                    let lib = &self.lib;
                    let resp = egui::Frame::new()
                        .fill(tk.surface)
                        .stroke(egui::Stroke::new(1.0, tk.border))
                        .corner_radius(theme::corner(radius::M))
                        .inner_margin(theme::margin(space::S + 2.0))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(
                                RichText::new(t("ui.left.inputs"))
                                    .size(font::CAPTION)
                                    .strong()
                                    .color(tk.text_weak),
                            );
                            let p = &mut self.project;
                            super::step_ui::fixed_width(ui, |ui| {
                                super::steps::left_panel(ui, screen, simple_step, p, model, lib);
                            });
                        });
                    self.shell.panel_rects.left_inputs = Some(resp.response.rect);
                }
                ui.add_space(space::L);
                let _ = act;
            });
    }

    /// Sağ panelin adım bölümü: bu adımın uyarıları + sonuçları (sağ panel ya da yedek alan).
    fn step_results(&mut self, ui: &mut Ui, model: &Model, act: &mut ShellActions) {
        let tk = theme::tokens(ui);
        let screen = super::steps::current_screen(&self.project, self.tab);
        if !super::steps::has_inputs(screen) {
            return;
        }
        ui.add_space(space::S);
        ui.add(
            egui::Label::new(
                RichText::new(tf("ui.right.step_results_fmt", &[self.screen_title()]))
                    .size(font::HEADING)
                    .strong()
                    .color(tk.text),
            )
            .truncate(),
        );
        let areas = super::steps::warning_areas(screen);
        if !areas.is_empty() {
            let list: Vec<_> = analysis::sorted_warnings(model)
                .into_iter()
                .filter(|w| areas.contains(&w.area_key))
                .collect();
            if !list.is_empty() {
                let a = analysis::warning_list(ui, &self.project, &list, false);
                act.merge(a);
            }
        }
        let simple_step = active_step(&self.project, self.tab);
        let mut why = None;
        let (p, lib) = (&mut self.project, &self.lib);
        super::step_ui::fixed_width(ui, |ui| {
            super::steps::right_panel(ui, screen, simple_step, p, model, lib, &mut why);
        });
        if why.is_some() {
            act.teach_why = why;
        }
    }

    /// Etkin ekranın başlığı (sihirbazda adım adı, uzman modunda sekme adı).
    fn screen_title(&self) -> String {
        if self.project.expert_mode {
            self.tab.label().to_string()
        } else {
            nav::strip_step_prefix(nav::current_step(&self.project).title()).to_string()
        }
    }

    /// Sağ panel: etkin adımın sonuçları, ardından canlı analiz (ana kartlar + kalan uyarılar).
    pub(crate) fn shell_right_panel(&mut self, ui: &mut Ui, model: &Model, act: &mut ShellActions) {
        // Yalnız dikey kaydırma; dış genişlik panel genişliğine sabitlenir. Böylece içerik
        // (ne olursa olsun) paneli büyütemez: taşan şey kırpılır, panel kayıp sağa çıkmaz.
        let w = ui.available_width();
        egui::ScrollArea::vertical()
            .id_salt("right_scroll")
            .auto_shrink([false, false])
            .max_width(w)
            .show(ui, |ui| {
                ui.set_max_width(w);
                self.step_results(ui, model, act);
                let screen = super::steps::current_screen(&self.project, self.tab);
                if super::steps::has_inputs(screen) {
                    ui.add_space(space::M);
                    ui.separator();
                }
                let areas = super::steps::warning_areas(screen);
                let summary = screen == super::steps::Screen::Summary;
                let a = analysis::show(
                    ui,
                    &self.project,
                    model,
                    &mut self.shell.show_info_notes,
                    areas,
                    summary,
                );
                act.merge(a);
            });
    }

    /// Merkez görünümü (3D / 2D / adımın grafiği) verilen alana çizer.
    fn draw_viewport(&mut self, ui: &mut Ui, model: &Model) {
        let screen = super::steps::current_screen(&self.project, self.tab);
        let mut hide = false;
        let proj = &self.project;
        let mut draw = |ui: &mut Ui, h: f32| super::steps::chart(ui, screen, proj, model, h);
        let chart = super::steps::chart_label(screen).map(|l| viewport3d::ChartSpec {
            label: l,
            draw: &mut draw,
        });
        let mut draw_formulas = |ui: &mut Ui| super::steps::formulas(ui, screen, proj, model);
        let formulas: Option<&mut dyn FnMut(&mut Ui)> = if super::steps::has_formulas(screen) {
            Some(&mut draw_formulas)
        } else {
            None
        };
        viewport3d::show(
            ui,
            &mut self.shell.viewport,
            proj,
            model,
            chart,
            formulas,
            |ui| {
                if components::tool_button(ui, t("ui.view.hide"), t("ui.view.hide_tip")).clicked() {
                    hide = true;
                }
            },
        );
        if hide {
            self.shell.prefs.viewport_open = false;
        }
    }

    /// Merkez: üstte adım başlığı (İleri/Geri/Öğret, doğrulama), altında görünüm. Görünüm merkezi
    /// tamamen kaplar; yalnız `needs` doluysa (Çıktılar/rüzgâr tüneli yuvası, dar pencerede ya da
    /// kapalı panelde gösterilemeyen girdi/sonuçlar) altta yeniden boyutlanabilen çalışma alanı açılır.
    pub(crate) fn shell_center(
        &mut self,
        ui: &mut Ui,
        model: &Model,
        needs: CenterNeeds,
        act: &mut ShellActions,
    ) {
        let tk = theme::tokens(ui);
        egui::Panel::top("center_step_header")
            .frame(
                egui::Frame::new()
                    .fill(tk.bg)
                    .inner_margin(egui::Margin::symmetric(14, 8)),
            )
            .show(ui, |ui| self.step_header(ui, model, act));

        let view_frame = egui::Frame::new()
            .fill(tk.bg)
            .inner_margin(egui::Margin::symmetric(8, 6));
        if !self.shell.prefs.viewport_open {
            egui::Panel::top("center_viewport_collapsed")
                .frame(
                    egui::Frame::new()
                        .fill(tk.bg)
                        .inner_margin(egui::Margin::symmetric(8, 4)),
                )
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(viewport3d::summary_text(
                                &self.project,
                                model,
                                self.shell.viewport.scene(),
                            ))
                            .color(tk.text_weak),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if components::tool_button(ui, t("ui.view.show"), "").clicked() {
                                self.shell.prefs.viewport_open = true;
                            }
                        });
                    });
                });
            self.workspace(ui, model, needs, act);
            return;
        }
        if !needs.any() {
            egui::CentralPanel::default()
                .frame(view_frame)
                .show(ui, |ui| self.draw_viewport(ui, model));
            return;
        }
        let total_h = ui.available_height();
        let outputs_slot = matches!(needs.slot, Some(super::steps::CenterSlot::Outputs));
        let (id, h, lo, hi) = if outputs_slot {
            // Çıktılar adımı: çalışma alanı asıl içerik; görünüm küçük kalır (kendi boyutunu hatırlar).
            let (def, min, max) = layout::outputs_viewport_limits(total_h);
            ("center_viewport_outputs", def, min, max)
        } else {
            let h = layout::viewport_height(total_h, self.shell.prefs.viewport_frac, 200.0, 140.0);
            ("center_viewport", h, 160.0, (total_h - 120.0).max(170.0))
        };
        let resp = egui::Panel::top(id)
            .resizable(true)
            .default_size(h)
            .size_range(lo..=hi.max(lo))
            .frame(view_frame)
            .show(ui, |ui| self.draw_viewport(ui, model));
        if total_h > 1.0 && !outputs_slot {
            self.shell.prefs.viewport_frac = resp.response.rect.height() / total_h;
        }
        self.workspace(ui, model, needs, act);
    }

    /// Görünümün altındaki çalışma alanı (yalnız gerektiğinde).
    fn workspace(
        &mut self,
        ui: &mut Ui,
        model: &Model,
        needs: CenterNeeds,
        act: &mut ShellActions,
    ) {
        let tk = theme::tokens(ui);
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(tk.bg)
                    .inner_margin(egui::Margin::symmetric(14, 8)),
            )
            .show(ui, |ui| {
                egui::ScrollArea::both()
                    .id_salt("workspace_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if needs.any() {
                            self.workspace_body(ui, model, needs, act);
                        } else {
                            components::empty_state(
                                ui,
                                t("ui.view.hidden_title"),
                                t("ui.view.hidden_body"),
                            );
                        }
                    });
            });
    }

    /// Çalışma alanı başlığı: adım adı, sayaç, Öğret / Geri / İleri, doğrulama şeridi.
    fn step_header(&mut self, ui: &mut Ui, model: &Model, act: &mut ShellActions) {
        let tk = theme::tokens(ui);
        if self.project.expert_mode {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(self.tab.label())
                        .size(font::HEADING)
                        .strong()
                        .color(tk.text),
                );
                components::badge(ui, components::Status::Info, t("ui.left.expert_badge"));
            });
            ui.add_space(space::XS);
            return;
        }
        let steps = wizard::steps_for(self.project.drone_type);
        let idx = nav::current_index(&self.project);
        let step = nav::current_step(&self.project);
        let validation = wizard::validate_step(step, &self.project, model);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(tf(
                        "ui.step.counter_fmt",
                        &[(idx + 1).to_string(), steps.len().to_string()],
                    ))
                    .size(font::CAPTION)
                    .color(tk.text_weak),
                );
                ui.label(
                    RichText::new(step.title())
                        .size(font::HEADING)
                        .strong()
                        .color(tk.text),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let can_next = nav::can_go_next(&self.project, model);
                if components::primary_button(ui, t("btn_next"), can_next).clicked() {
                    nav::go_next(&mut self.project, model);
                }
                if ui
                    .add_enabled(idx > 0, egui::Button::new(t("btn_back")))
                    .clicked()
                {
                    nav::go_back(&mut self.project);
                }
                if ui
                    .button(t("teach.btn_teach_me"))
                    .on_hover_text(t("ui.step.teach_tip"))
                    .clicked()
                {
                    act.teach_step = Some(step);
                }
            });
        });
        if let Err(reason) = validation {
            components::banner(
                ui,
                components::Status::Err,
                &format!("{} — {}", t("cannot_advance"), reason),
            );
        }
        let reqs = wizard::step_required_params(step);
        if !reqs.is_empty() {
            egui::CollapsingHeader::new(
                RichText::new(t("wizard.card_req_header"))
                    .size(font::CAPTION)
                    .color(tk.text_weak),
            )
            .id_salt(("step_reqs", step as u8))
            .default_open(false)
            .show(ui, |ui| {
                for r in reqs {
                    ui.label(
                        RichText::new(format!("• {r}"))
                            .size(font::CAPTION)
                            .color(tk.text_weak),
                    );
                }
            });
        }
        ui.add_space(space::XS);
        let r = ui.available_rect_before_wrap();
        ui.painter().hline(
            r.left()..=r.right(),
            ui.cursor().top(),
            egui::Stroke::new(1.0, tk.border),
        );
        ui.add_space(space::S);
    }

    /// Çalışma alanı içeriği: başka paketin yuvası (Çıktılar / rüzgâr tüneli) ve panelde
    /// gösterilemeyen girdi/sonuçlar (dar pencere, rail modu, kapatılmış panel).
    fn workspace_body(
        &mut self,
        ui: &mut Ui,
        model: &Model,
        needs: CenterNeeds,
        act: &mut ShellActions,
    ) {
        if needs.inputs {
            let screen = super::steps::current_screen(&self.project, self.tab);
            let simple_step = active_step(&self.project, self.tab);
            components::section::section_header(ui, t("ui.left.inputs"));
            super::steps::left_panel(ui, screen, simple_step, &mut self.project, model, &self.lib);
        }
        if needs.results {
            self.step_results(ui, model, act);
        }
        match needs.slot {
            Some(super::steps::CenterSlot::Outputs) => {
                if needs.inputs || needs.results {
                    ui.separator();
                }
                let freecad = self.freecad_install.as_ref();
                super::outputs::show(ui, &mut self.project, model, &mut self.outputs_ui, freecad);
            }
            Some(super::steps::CenterSlot::WindTunnel) => {
                super::windtunnel::show(ui, &self.project, model, &mut self.wt_state);
            }
            None => {}
        }
    }

    /// Kabuğun tamamı (App::ui içinden çağrılır). İstekler kare sonunda uygulanır.
    pub(crate) fn shell_ui(&mut self, ui: &mut Ui, model: &Model) -> ShellActions {
        let tk = theme::tokens(ui);
        let mut act = ShellActions::default();
        // Adım durumlarını her karede eşitle (sol panel gizli olsa bile adım kilidi geçerli).
        wizard::sync_wizard_steps(&mut self.project, model);

        egui::Panel::top("topbar")
            .frame(
                egui::Frame::new()
                    .fill(tk.surface)
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .stroke(egui::Stroke::new(1.0, tk.border)),
            )
            .show(ui, |ui| self.shell_top_bar(ui, model));

        self.status_bar(ui, model);

        let current_step_title = if self.project.expert_mode {
            self.tab.label().to_string()
        } else {
            nav::current_step(&self.project).title().to_string()
        };
        let ai_action = crate::ui::ai::show_ai_panel(
            ui,
            &mut self.ai_state,
            &mut self.ai_config,
            &mut self.project,
            model,
            &current_step_title,
            &self.cred_store,
        );
        if ai_action.applied_count > 0 {
            self.status = tf(
                "status_ai_applied_fmt",
                &[ai_action.applied_count.to_string()],
            );
        }
        if ai_action.undone {
            self.status = t("status_ai_undone").to_string();
        }

        let lay = layout::compute(ui.available_width(), &self.shell.prefs);
        // Basit mod: kare başı eşitleme + özelleştirme takibi (sol ve sağ panel ve yedek alan
        // boyunca yapılan elle düzenlemeler "özelleştirildi" sayılır).
        let snap = super::simple_steps::frame_begin(ui.ctx(), &mut self.project, model, &self.lib);
        let screen = super::steps::current_screen(&self.project, self.tab);
        let needs = CenterNeeds {
            slot: super::steps::center_slot(screen, self.project.expert_mode),
            inputs: super::steps::has_inputs(screen) && (!lay.show_left || lay.left_rail),
            results: super::steps::has_inputs(screen) && !lay.show_right,
        };
        let side_frame = egui::Frame::new()
            .fill(tk.surface)
            .inner_margin(egui::Margin::symmetric(10, 4))
            .stroke(egui::Stroke::new(1.0, tk.border));
        self.shell.panel_rects = PanelRects::default();
        if lay.show_left {
            let panel = if lay.left_rail {
                egui::Panel::left("shell_left_rail")
                    .resizable(false)
                    .exact_size(layout::LEFT_RAIL)
            } else {
                egui::Panel::left("shell_left")
                    .resizable(true)
                    .default_size(layout::LEFT_DEFAULT.min(lay.left_max))
                    .size_range(layout::LEFT_MIN..=lay.left_max)
            };
            let resp = panel.frame(side_frame).show(ui, |ui| {
                self.shell_left_panel(ui, model, lay.left_rail, &mut act)
            });
            self.shell.panel_rects.left = Some(resp.response.rect);
        }
        if lay.show_right {
            let resp = egui::Panel::right("shell_right")
                .resizable(true)
                .default_size(layout::RIGHT_DEFAULT.min(lay.right_max))
                .size_range(layout::RIGHT_MIN..=lay.right_max)
                .frame(side_frame)
                .show(ui, |ui| self.shell_right_panel(ui, model, &mut act));
            self.shell.panel_rects.right = Some(resp.response.rect);
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(tk.bg))
            .show(ui, |ui| self.shell_center(ui, model, needs, &mut act));
        super::simple_steps::frame_end(&mut self.project, &snap);
        act
    }

    /// Alt durum çubuğu: kütle, hata/uyarı sayısı, durum iletisi, sorumluluk notu.
    fn status_bar(&mut self, ui: &mut Ui, model: &Model) {
        let tk = theme::tokens(ui);
        egui::Panel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(tk.surface)
                    .inner_margin(egui::Margin::symmetric(10, 3))
                    .stroke(egui::Stroke::new(1.0, tk.border)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (errs, warns, _) = analysis::warning_counts(model);
                    let mtow_g = if self.project.drone_type == DroneType::Multirotor {
                        model.mr.m_mtow * 1000.0
                    } else {
                        model.weight.m_mtow * 1000.0
                    };
                    ui.label(
                        RichText::new(tf("status_mtow_fmt", &[super::widgets::fmt(mtow_g, 0)]))
                            .size(font::CAPTION),
                    );
                    ui.separator();
                    ui.label(
                        RichText::new(tf("status_err_count", &[format!("{errs}")]))
                            .size(font::CAPTION)
                            .color(if errs > 0 { tk.danger } else { tk.success }),
                    );
                    ui.label(
                        RichText::new(tf("status_warn_count", &[format!("{warns}")]))
                            .size(font::CAPTION)
                            .color(if warns > 0 { tk.warning } else { tk.success }),
                    );
                    ui.separator();
                    ui.label(
                        RichText::new(&self.status)
                            .size(font::CAPTION)
                            .color(tk.text),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(t("status_disclaimer"))
                                .size(font::CAPTION)
                                .color(tk.text_weak),
                        );
                    });
                });
            });
    }

    /// Kabuk isteklerini uygular.
    pub(crate) fn apply_shell_actions(&mut self, act: ShellActions, model: &Model) {
        if let Some(step) = act.go_to {
            let mut tab = self.tab;
            nav::go_to_step(&mut self.project, &mut tab, model, step);
            self.tab = tab;
        }
        if let Some(step) = act.teach_step {
            self.teacher_state
                .open_for_step(step, self.project.drone_type);
        }
        if let Some(metric) = act.teach_why {
            self.teacher_state
                .open_why(metric, &self.project, model, crate::i18n::current_lang());
        }
        if let Some(warn_msg) = act.fix_warning {
            let lang = crate::i18n::current_lang();
            let prompt =
                crate::ai::prompt::build_fix_warning_prompt(&self.project, model, &warn_msg, lang);
            self.ai_state.is_panel_open = true;
            self.ai_state.start_request(
                &self.ai_config,
                &self.project,
                prompt,
                lang,
                self.cred_store.clone(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn center_needs_any() {
        assert!(!CenterNeeds::default().any());
        let n = CenterNeeds {
            inputs: true,
            ..Default::default()
        };
        assert!(n.any());
        let n = CenterNeeds {
            slot: Some(crate::ui::steps::CenterSlot::Outputs),
            ..Default::default()
        };
        assert!(n.any());
    }

    #[test]
    fn active_step_follows_mode() {
        let mut p = Project::default();
        assert_eq!(active_step(&p, Tab::Aero), Some(StepId::Mission));
        p.expert_mode = true;
        assert_eq!(active_step(&p, Tab::Aero), Some(StepId::Aero));
        assert_eq!(active_step(&p, Tab::WindTunnel), None);
    }
}
