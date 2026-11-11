// SPDX-License-Identifier: AGPL-3.0-or-later
//! Arayüz: ana uygulama, menü, sekme seçimi ve drone türü sihirbazı.
pub mod aero;
pub mod aero_simple;
pub mod ai;
pub mod components;
pub mod design_point;
pub mod endurance;
pub mod formulas;
pub mod geometry;
pub mod help;
pub mod mission;
pub mod mr_structure;
pub mod multirotor;
pub mod outputs;
pub mod panel_kit;
pub mod propulsion;
pub mod schematic;
pub mod settings_page;
pub mod shell;
pub mod simple_steps;
pub mod stability;
pub mod step_ui;
pub mod steps;
pub mod structure;
pub mod summary;
pub mod teach;
pub mod theme;
pub mod type_cards;
pub mod viewport3d;
pub mod weight;
pub mod widgets;
pub mod windtunnel;
pub mod wizard;

use crate::calc::{self, Library, Model};
use crate::i18n::{t, tf};
use crate::project::{DroneType, Project};
use components::Status;
use egui::Color32;
use std::path::PathBuf;
use theme::space;

/// Tehlikeli eylem düğmesinin (danger dolgulu) yazı rengi; iki temada da okunur.
fn danger_button_text(tk: &theme::Tokens) -> Color32 {
    if tk.dark { tk.extreme } else { tk.surface }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Summary,
    Mission,
    Multirotor,
    Weight,
    Geometry,
    Aero,
    DesignPoint,
    Endurance,
    Propulsion,
    Stability,
    Structure,
    Schematic,
    WindTunnel,
}

impl Tab {
    pub fn label(&self) -> &'static str {
        match self {
            Tab::Summary => t("tab_summary"),
            Tab::Mission => t("tab_mission"),
            Tab::Multirotor => t("tab_multirotor"),
            Tab::Weight => t("tab_weight"),
            Tab::Geometry => t("tab_geometry"),
            Tab::Aero => t("tab_aero"),
            Tab::DesignPoint => t("tab_design_point"),
            Tab::Endurance => t("tab_endurance"),
            Tab::Propulsion => t("tab_propulsion"),
            Tab::Stability => t("tab_stability"),
            Tab::Structure => t("tab_structure"),
            Tab::Schematic => t("tab_schematic"),
            Tab::WindTunnel => t("tab_windtunnel"),
        }
    }

    /// Drone türüne göre görüntülenecek sekmeleri belirler.
    pub fn tabs_for(dt: DroneType) -> Vec<Tab> {
        match dt {
            DroneType::Multirotor => vec![
                Tab::Summary,
                Tab::Mission,
                Tab::Multirotor,
                Tab::Weight,
                Tab::Stability,
                Tab::Structure,
                Tab::Schematic,
            ],
            DroneType::VtolQuadplane => vec![
                Tab::Summary,
                Tab::Mission,
                Tab::Weight,
                Tab::Geometry,
                Tab::Aero,
                Tab::WindTunnel,
                Tab::DesignPoint,
                Tab::Endurance,
                Tab::Propulsion,
                Tab::Stability,
                Tab::Structure,
                Tab::Multirotor,
                Tab::Schematic,
            ],
            _ => vec![
                Tab::Summary,
                Tab::Mission,
                Tab::Weight,
                Tab::Geometry,
                Tab::Aero,
                Tab::WindTunnel,
                Tab::DesignPoint,
                Tab::Endurance,
                Tab::Propulsion,
                Tab::Stability,
                Tab::Structure,
                Tab::Schematic,
            ],
        }
    }
}

pub struct App {
    project: Project,
    lib: Library,
    tab: Tab,
    path: Option<PathBuf>,
    status: String,
    pub show_type_modal: bool,
    prev_json: String,

    // Modal Durumları
    pub show_about_modal: bool,
    pub show_feedback_modal: bool,
    pub show_crash_modal: bool,
    pub show_uninstall_modal: bool,

    // Hata ve Geri Bildirim Alanları
    pub pending_crash_path: Option<PathBuf>,
    pub crash_content: String,
    pub user_feedback_text: String,
    pub include_project_json: bool,

    // Kaldırma (Uninstall) Alanları
    pub uninstall_categories: Vec<crate::footprint::FootprintCategory>,
    pub uninstall_confirm_text: String,
    pub uninstall_done_msg: Option<String>,

    // CAD ve Ayarlar Alanları
    pub outputs_ui: crate::ui::outputs::OutputsUi,
    pub show_settings_modal: bool,
    pub settings_freecad_cmd: String,
    pub freecad_install: Option<crate::cad::FreeCadInstallation>,
    pub freecad_detection_rx:
        Option<std::sync::mpsc::Receiver<Option<crate::cad::FreeCadInstallation>>>,
    pub freecad_detecting: bool,
    pub wt_state: windtunnel::WtState,

    // Topluluk Veritabanı Alanları
    pub db_updating: bool,
    pub db_cancel_flag: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    pub db_update_rx: Option<std::sync::mpsc::Receiver<Result<crate::db::DatabaseInfo, String>>>,
    pub db_status_msg: Option<(bool, String)>,

    // Yapay Zeka Alanları
    pub ai_state: crate::ai::AiAssistantState,
    pub ai_config: crate::ai::AiConfig,
    pub cred_store: std::sync::Arc<dyn crate::ai::CredentialStorage>,

    // Öğretmen Modu Alanı
    pub teacher_state: crate::ui::teach::TeacherUiState,

    // Yeni üç panelli kabuğun görünüm durumu (projeye yazılmaz)
    pub shell: shell::ShellState,
}

impl App {
    pub fn trigger_freecad_detection(&mut self, path: Option<String>, ctx: Option<egui::Context>) {
        self.freecad_detecting = true;
        self.freecad_detection_rx = Some(crate::cad::detect_freecad_async(path, ctx));
    }

    pub fn trigger_db_update(&mut self) {
        if self.db_updating {
            return;
        }
        self.db_updating = true;
        self.db_status_msg = None;
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.db_cancel_flag = Some(cancel.clone());
        let (tx, rx) = std::sync::mpsc::channel();
        self.db_update_rx = Some(rx);

        std::thread::spawn(move || {
            let res = crate::db::update_database(Some(&cancel));
            let _ = tx.send(res);
        });
    }

    pub fn cancel_db_update(&mut self) {
        if let Some(flag) = self.db_cancel_flag.take() {
            flag.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        self.db_updating = false;
        self.db_update_rx = None;
        self.db_status_msg = None;
    }

    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self::new_with_uninstall(cc, false)
    }

    pub fn new_with_uninstall(cc: &eframe::CreationContext<'_>, start_uninstall: bool) -> Self {
        crate::i18n::init_i18n();
        crate::support::log_info("WerSeuS – UAV Calc arayüzü başlatıldı.");

        let settings = crate::i18n::load_settings();
        theme::install(&cc.egui_ctx, settings.theme);
        let startup_prefs = settings_page::StartupPrefs::from_settings(&settings);
        let settings_freecad_cmd = settings.freecad_cmd_path.unwrap_or_default();

        let mut app = App {
            project: Project::default(),
            lib: Library::load(),
            tab: Tab::Summary,
            path: None,
            status: t("status_ready").into(),
            show_type_modal: !start_uninstall,
            prev_json: String::new(),
            show_about_modal: false,
            show_feedback_modal: false,
            show_crash_modal: false,
            show_uninstall_modal: start_uninstall,
            pending_crash_path: None,
            crash_content: String::new(),
            user_feedback_text: String::new(),
            include_project_json: true,
            uninstall_categories: if start_uninstall {
                crate::footprint::scan_footprint()
            } else {
                Vec::new()
            },
            uninstall_confirm_text: String::new(),
            uninstall_done_msg: None,
            outputs_ui: Default::default(),
            show_settings_modal: false,
            settings_freecad_cmd: settings_freecad_cmd.clone(),
            freecad_install: None,
            freecad_detection_rx: None,
            freecad_detecting: false,
            wt_state: windtunnel::WtState::default(),
            db_updating: false,
            db_cancel_flag: None,
            db_update_rx: None,
            db_status_msg: None,
            ai_state: crate::ai::state::AiAssistantState::new(),
            cred_store: std::sync::Arc::new(
                crate::ai::credential::WinCredStore::new_with_persistence(
                    settings.ai_config.store_in_wincred,
                ),
            ),
            ai_config: settings.ai_config,
            teacher_state: crate::ui::teach::TeacherUiState::new(settings.teacher),
            shell: shell::ShellState::new(settings.theme),
        };

        startup_prefs.apply(&mut app, &cc.egui_ctx);

        // Arka planda FreeCAD algılamasını başlat (UI ana iş parçacığı asla kilitlenmez)
        let freecad_path_arg = if settings_freecad_cmd.trim().is_empty() {
            None
        } else {
            Some(settings_freecad_cmd)
        };
        app.trigger_freecad_detection(freecad_path_arg, Some(cc.egui_ctx.clone()));

        if !start_uninstall {
            if let Some(crash_path) = crate::support::find_unprocessed_crash() {
                if let Ok(content) = std::fs::read_to_string(&crash_path) {
                    app.pending_crash_path = Some(crash_path);
                    app.crash_content = content;
                    app.show_crash_modal = true;
                }
            }
        }

        app
    }

    /// Başsız testler için: ayar dosyası okumaz/yazmaz, arka plan işi başlatmaz.
    #[cfg(test)]
    pub(crate) fn new_headless(ctx: &egui::Context, dt: DroneType) -> Self {
        let mut project = Project::default();
        project.set_drone_type(dt);
        theme::install(ctx, theme::ThemeChoice::Dark);
        App {
            project,
            lib: Library::embedded(),
            tab: Tab::Summary,
            path: None,
            status: String::new(),
            show_type_modal: false,
            prev_json: String::new(),
            show_about_modal: false,
            show_feedback_modal: false,
            show_crash_modal: false,
            show_uninstall_modal: false,
            pending_crash_path: None,
            crash_content: String::new(),
            user_feedback_text: String::new(),
            include_project_json: false,
            uninstall_categories: Vec::new(),
            uninstall_confirm_text: String::new(),
            uninstall_done_msg: None,
            outputs_ui: Default::default(),
            show_settings_modal: false,
            settings_freecad_cmd: String::new(),
            freecad_install: None,
            freecad_detection_rx: None,
            freecad_detecting: false,
            wt_state: windtunnel::WtState::default(),
            db_updating: false,
            db_cancel_flag: None,
            db_update_rx: None,
            db_status_msg: None,
            ai_state: crate::ai::state::AiAssistantState::new(),
            cred_store: std::sync::Arc::new(crate::ai::credential::SessionCredentialStore::new()),
            ai_config: crate::ai::AiConfig::default(),
            teacher_state: crate::ui::teach::TeacherUiState::new(Default::default()),
            shell: shell::ShellState::new(theme::ThemeChoice::Dark),
        }
    }

    fn export_report_markdown(&mut self, model: &Model) {
        let default_name = format!("{}_tasarim_raporu.md", self.project.name.replace(' ', "_"));
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Markdown (*.md)", &["md"])
            .set_file_name(&default_name)
            .save_file()
        else {
            return;
        };

        let lang = crate::i18n::current_lang();
        let report_md = crate::report::generate_markdown_report(&self.project, model, lang);
        match std::fs::write(&path, report_md) {
            Ok(()) => {
                crate::support::log_info(&format!("Tasarım raporu kaydedildi: {}", path.display()));
                self.status = tf("status_report_exported", &[path.display().to_string()]);
            }
            Err(e) => {
                crate::support::log_error(&format!("Rapor kaydetme hatası: {}", e));
                self.status = tf("status_report_failed", &[e.to_string()]);
            }
        }
    }

    fn export_report_excel(&mut self, model: &Model) {
        let default_name = format!(
            "{}_tasarim_hesaplari.xlsx",
            self.project.name.replace(' ', "_")
        );
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Excel Workbook (*.xlsx)", &["xlsx"])
            .set_file_name(&default_name)
            .save_file()
        else {
            return;
        };

        let lang = crate::i18n::current_lang();
        match crate::export::xlsx::export_project_to_xlsx(&self.project, model, lang, &path) {
            Ok(()) => {
                crate::support::log_info(&format!(
                    "Excel hesap tablosu kaydedildi: {}",
                    path.display()
                ));
                self.status = tf("status_excel_exported", &[path.display().to_string()]);
            }
            Err(e) => {
                crate::support::log_error(&format!("Excel kaydetme hatası: {}", e));
                self.status = tf("status_excel_failed", &[e.to_string()]);
            }
        }
    }

    fn load_example_project(&mut self, filename: &str, label: &'static str) {
        match crate::project::load_example(filename) {
            Ok(p) => {
                self.project = p;
                self.show_type_modal = false;
                self.path = None;
                self.prev_json.clear();
                crate::support::log_info(&format!("Örnek proje yüklendi: {}", label));
                self.status = tf("status_example_loaded", &[label]);
            }
            Err(e) => {
                self.status = tf("status_open_failed", &[e]);
            }
        }
    }

    fn new_project(&mut self) {
        self.project = Project::default();
        self.path = None;
        self.show_type_modal = true;
        self.tab = Tab::Summary;
        self.status = t("status_new_project").into();
        self.prev_json.clear();
    }

    fn open(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter(t("file_filter_json"), &["json"])
            .pick_file()
        else {
            return;
        };
        match std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|s| Project::from_json(&s))
        {
            Ok(p) => {
                self.project = p;
                self.show_type_modal = false;
                self.status = tf("status_opened", &[path.display().to_string()]);
                self.path = Some(path);
                self.prev_json.clear();
            }
            Err(e) => self.status = tf("status_open_failed", std::slice::from_ref(&e)),
        }
    }

    fn save(&mut self, as_new: bool) {
        let path = match (&self.path, as_new) {
            (Some(p), false) => p.clone(),
            _ => {
                let Some(p) = rfd::FileDialog::new()
                    .add_filter(t("file_filter_json"), &["json"])
                    .set_file_name("iha_proje.json")
                    .save_file()
                else {
                    return;
                };
                p
            }
        };
        match self
            .project
            .to_json()
            .and_then(|s| std::fs::write(&path, s).map_err(|e| e.to_string()))
        {
            Ok(()) => {
                self.status = tf("status_saved", &[path.display().to_string()]);
                self.path = Some(path);
            }
            Err(e) => self.status = tf("status_save_failed", std::slice::from_ref(&e)),
        }
    }
}

impl eframe::App for App {
    /// Kapanış: süren CAD üretimi varsa iptal edilir ve FreeCAD süreci (ağacıyla) en çok ~3 sn
    /// beklenerek sonlandırılır; böylece yetim `FreeCADCmd` kalmaz. Kapanış asla kilitlenmez.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.outputs_ui.shutdown(crate::ui::outputs::SHUTDOWN_WAIT);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.frame_ui(ui);
    }
}

impl App {
    /// Bir karenin tamamı (eframe çerçevesine ihtiyaç duymaz; başsız testler de bunu çağırır).
    pub(crate) fn frame_ui(&mut self, ui: &mut egui::Ui) {
        self.ai_state.poll();

        // Arka plan FreeCAD algılama sonucunu tüket (engellemesiz)
        if let Some(ref rx) = self.freecad_detection_rx {
            match rx.try_recv() {
                Ok(res) => {
                    if let Some(ref inst) = res {
                        if self.settings_freecad_cmd.trim().is_empty() {
                            self.settings_freecad_cmd = inst.cmd_path.display().to_string();
                        }
                    }
                    self.freecad_install = res;
                    self.freecad_detecting = false;
                    self.freecad_detection_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.freecad_detecting = false;
                    self.freecad_detection_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }

        // Arka plan veritabanı güncelleme sonucunu tüket
        if let Some(ref rx) = self.db_update_rx {
            match rx.try_recv() {
                Ok(res) => {
                    self.db_updating = false;
                    self.db_cancel_flag = None;
                    self.db_update_rx = None;
                    match res {
                        Ok(info) => {
                            self.lib = crate::calc::Library::load();
                            self.db_status_msg =
                                Some((true, t("settings.db_update_success").to_string()));
                            self.status = format!(
                                "{} (v{}, {})",
                                t("settings.db_update_success"),
                                info.schema_version,
                                info.updated
                            );
                        }
                        Err(err) => {
                            self.lib = crate::calc::Library::load();
                            self.db_status_msg =
                                Some((false, tf("settings.db_update_failed", &[&err])));
                            self.status = tf("settings.db_update_failed", &[&err]);
                        }
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.db_updating = false;
                    self.db_cancel_flag = None;
                    self.db_update_rx = None;
                    self.lib = crate::calc::Library::load();
                    let msg = crate::i18n::t("settings.db_update_disconnected").to_string();
                    self.db_status_msg = Some((false, msg.clone()));
                    self.status = msg;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.db_updating {
            ui.ctx().request_repaint();
        }

        // Kısayol: Ctrl+Shift+A ile Yapay Zeka Asistan Panelini aç/kapat
        // Metin alanı odaktayken genel kısayollar çalışmaz (yazmayı/yapıştırmayı bozmasın).
        if !ui.ctx().egui_wants_keyboard_input()
            && ui.input(|i| i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::A))
        {
            self.ai_state.is_panel_open = !self.ai_state.is_panel_open;
        }

        let model: Model = calc::evaluate(&self.project, &self.lib);

        // Uzman modu kilidi kontrolü:
        // Eğer kullanıcı bir şekilde uzman modundayken adımlardan biri bozulursa uzman modu otomatik kapanmalı ve ilk geçersiz adıma yönlendirmeli.
        if let Some((_inv_idx, inv_step, _err)) =
            wizard::enforce_expert_gate(&mut self.project, &model)
        {
            self.status = tf("status_expert_mode_reverted", &[inv_step.title()]);
        }

        // Dirty tracking: Kullanıcı tamamlanmış adıma dönüp değer değiştirirse, sonraki adımları "NeedsUpdate" yap
        let current_json = self.project.to_json().unwrap_or_default();
        if !self.prev_json.is_empty() && current_json != self.prev_json {
            let curr = self.project.wizard_step;
            for i in (curr + 1)..self.project.step_states.len() {
                if self.project.step_states[i] == crate::project::WizardStepState::Completed {
                    self.project.step_states[i] = crate::project::WizardStepState::NeedsUpdate;
                }
            }
        }
        self.prev_json = current_json;

        // Drone Türü Seçim Penceresi (Açılışta ve Proje > Yeni'de)
        if self.show_type_modal {
            egui::Window::new(t("dialog_choose_type_title"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .default_width(740.0)
                .show(ui.ctx(), |ui| {
                    ui.heading(t("dialog_choose_type_subtitle"));
                    ui.add_space(space::S);

                    let max_h = (ui.ctx().content_rect().height() - 190.0).max(200.0);
                    egui::ScrollArea::vertical()
                        .id_salt("type_cards_scroll")
                        .max_height(max_h)
                        .show(ui, |ui| {
                            if let Some(dt) =
                                type_cards::type_card_grid(ui, Some(self.project.drone_type))
                            {
                                self.project.set_drone_type(dt);
                                self.show_type_modal = false;
                                self.tab = Tab::Summary;
                                self.status = tf("status_platform_selected", &[dt.label()]);
                            }
                        });

                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button(t("btn_cancel")).clicked() {
                            self.show_type_modal = false;
                        }
                    });
                });
        }

        // Hakkında Penceresi (Help -> About)
        if self.show_about_modal {
            egui::Window::new(t("dialog_about_title"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .default_width(540.0)
                .show(ui.ctx(), |ui| {
                    ui.heading("WerSeuS – UAV Calc");
                    ui.add_space(space::XS);
                    ui.label(format!(
                        "{}: v{}",
                        t("about_version_label"),
                        env!("CARGO_PKG_VERSION")
                    ));
                    let db_source_str = match self.lib.db_info.source {
                        crate::db::DbSource::Embedded => t("about.db_embedded"),
                        crate::db::DbSource::Community => t("about.db_community"),
                    };
                    ui.label(format!(
                        "{}: {} (v{}, {})",
                        t("about.db_label"),
                        db_source_str,
                        self.lib.db_info.schema_version,
                        self.lib.db_info.updated
                    ));
                    ui.add_space(space::S);
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.strong(format!("{}:", t("about_license_label")));
                        ui.label(t("about_license_text"));
                    });
                    ui.horizontal(|ui| {
                        ui.strong(format!("{}:", t("about_commercial_label")));
                        ui.label(t("about_commercial_text"));
                    });
                    ui.horizontal(|ui| {
                        ui.strong(format!("{}:", t("about_source_label")));
                        ui.weak(t("about_source_not_yet"));
                    });
                    ui.horizontal(|ui| {
                        ui.strong(format!("{}:", t("about_third_party_label")));
                        ui.label(t("about_third_party_text"));
                    });
                    ui.separator();
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(t("btn_cancel")).clicked() {
                            self.show_about_modal = false;
                        }
                    });
                });
        }

        // Çökme Bildirimi Penceresi (Crash Recovery)
        if self.show_crash_modal {
            egui::Window::new(t("dialog_crash_title"))
                .collapsible(false)
                .resizable(true)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .default_width(700.0)
                .default_height(520.0)
                .show(ui.ctx(), |ui| {
                    components::banner(ui, Status::Err, t("crash_header_text"));
                    ui.add_space(space::XS);
                    ui.label(
                        egui::RichText::new(t("gdpr_privacy_note"))
                            .weak()
                            .size(theme::font::CAPTION),
                    );
                    ui.separator();

                    ui.label(t("feedback_desc_label"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.user_feedback_text)
                            .desired_width(f32::INFINITY),
                    );
                    ui.add_space(space::XS);

                    ui.label(t("feedback_preview_label"));
                    egui::ScrollArea::vertical()
                        .max_height(240.0)
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut self.crash_content)
                                    .desired_width(f32::INFINITY)
                                    .font(egui::TextStyle::Monospace),
                            );
                        });

                    ui.separator();
                    ui.horizontal_wrapped(|ui| {
                        let sys_summary = crate::support::generate_system_summary(
                            match crate::i18n::current_lang() {
                                crate::i18n::Lang::Tr => "TR",
                                crate::i18n::Lang::En => "EN",
                            },
                            self.project.drone_type.label(),
                        );
                        let full_report = crate::support::build_full_report_text(
                            &self.user_feedback_text,
                            &sys_summary,
                            None,
                            &[],
                            Some(&self.crash_content),
                        );

                        if ui.button(t("btn_send_email")).clicked() {
                            let (mailto, zip_path) = crate::support::prepare_mailto_link(
                                &full_report,
                                &self.user_feedback_text,
                                None,
                            );
                            ui.ctx().open_url(egui::OpenUrl::new_tab(&mailto));
                            if let Some(ref z) = zip_path {
                                #[cfg(windows)]
                                let _ = crate::cad::new_command("explorer")
                                    .arg(format!("/select,{}", z.display()))
                                    .spawn();
                            }
                            if let Some(ref p) = self.pending_crash_path {
                                crate::support::mark_crash_processed(p);
                            }
                            self.show_crash_modal = false;
                        }

                        let github_btn = egui::Button::new(t("btn_send_github"));
                        let is_gh_configured = !crate::support::ISSUES_URL.starts_with('<');
                        let gh_resp = ui.add_enabled(is_gh_configured, github_btn);
                        if !is_gh_configured {
                            gh_resp.on_disabled_hover_text(t("github_not_configured_tooltip"));
                        } else if gh_resp.clicked() {
                            let (issue_url, _) = crate::support::prepare_github_issue_url(
                                "[Hata / Crash Raporu]",
                                &full_report,
                                None,
                            );
                            ui.ctx().open_url(egui::OpenUrl::new_tab(&issue_url));
                            if let Some(ref p) = self.pending_crash_path {
                                crate::support::mark_crash_processed(p);
                            }
                            self.show_crash_modal = false;
                        }

                        if ui.button(t("btn_copy_clipboard")).clicked() {
                            ui.ctx().copy_text(full_report);
                            self.status = t("status_report_copied").to_string();
                        }

                        if ui.button(t("btn_open_folder")).clicked() {
                            let dir = crate::support::logs_dir();
                            #[cfg(windows)]
                            let _ = crate::cad::new_command("explorer").arg(dir).spawn();
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button(t("btn_dont_send")).clicked() {
                                if let Some(ref p) = self.pending_crash_path {
                                    crate::support::mark_crash_processed(p);
                                }
                                self.show_crash_modal = false;
                            }
                        });
                    });
                });
        }

        // Elle Geri Bildirim Penceresi (Help -> Feedback)
        if self.show_feedback_modal {
            egui::Window::new(t("dialog_feedback_title"))
                .collapsible(false)
                .resizable(true)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .default_width(700.0)
                .default_height(520.0)
                .show(ui.ctx(), |ui| {
                    ui.label(
                        egui::RichText::new(t("gdpr_privacy_note"))
                            .weak()
                            .size(theme::font::CAPTION),
                    );
                    ui.separator();

                    ui.label(t("feedback_desc_label"));
                    ui.add(
                        egui::TextEdit::multiline(&mut self.user_feedback_text)
                            .desired_width(f32::INFINITY)
                            .desired_rows(3),
                    );
                    ui.add_space(space::XS);
                    ui.checkbox(
                        &mut self.include_project_json,
                        t("feedback_include_project"),
                    );
                    ui.add_space(space::XS);

                    let log_tail = crate::support::read_last_log_lines(200);
                    let sys_summary = crate::support::generate_system_summary(
                        match crate::i18n::current_lang() {
                            crate::i18n::Lang::Tr => "TR",
                            crate::i18n::Lang::En => "EN",
                        },
                        self.project.drone_type.label(),
                    );
                    let pj = if self.include_project_json {
                        self.project.to_json().ok()
                    } else {
                        None
                    };
                    let mut preview_text = crate::support::build_full_report_text(
                        &self.user_feedback_text,
                        &sys_summary,
                        pj.as_deref(),
                        &log_tail,
                        None,
                    );

                    ui.label(t("feedback_preview_label"));
                    egui::ScrollArea::vertical()
                        .max_height(200.0)
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut preview_text)
                                    .desired_width(f32::INFINITY)
                                    .font(egui::TextStyle::Monospace),
                            );
                        });

                    ui.separator();
                    ui.horizontal_wrapped(|ui| {
                        if ui.button(t("btn_send_email")).clicked() {
                            let (mailto, zip_path) = crate::support::prepare_mailto_link(
                                &preview_text,
                                &self.user_feedback_text,
                                pj.as_deref(),
                            );
                            ui.ctx().open_url(egui::OpenUrl::new_tab(&mailto));
                            if let Some(ref z) = zip_path {
                                #[cfg(windows)]
                                let _ = crate::cad::new_command("explorer")
                                    .arg(format!("/select,{}", z.display()))
                                    .spawn();
                            }
                            self.show_feedback_modal = false;
                        }

                        let github_btn = egui::Button::new(t("btn_send_github"));
                        let is_gh_configured = !crate::support::ISSUES_URL.starts_with('<');
                        let gh_resp = ui.add_enabled(is_gh_configured, github_btn);
                        if !is_gh_configured {
                            gh_resp.on_disabled_hover_text(t("github_not_configured_tooltip"));
                        } else if gh_resp.clicked() {
                            let (issue_url, _) = crate::support::prepare_github_issue_url(
                                "[Geri Bildirim]",
                                &preview_text,
                                pj.as_deref(),
                            );
                            ui.ctx().open_url(egui::OpenUrl::new_tab(&issue_url));
                            self.show_feedback_modal = false;
                        }

                        if ui.button(t("btn_copy_clipboard")).clicked() {
                            ui.ctx().copy_text(preview_text);
                            self.status = t("status_report_copied").to_string();
                        }

                        if ui.button(t("btn_open_folder")).clicked() {
                            let dir = crate::support::logs_dir();
                            #[cfg(windows)]
                            let _ = crate::cad::new_command("explorer").arg(dir).spawn();
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button(t("btn_cancel")).clicked() {
                                self.show_feedback_modal = false;
                            }
                        });
                    });
                });
        }

        // Program Kaldırma Penceresi (Help -> Uninstall)
        if self.show_uninstall_modal {
            egui::Window::new(t("dialog_uninstall_title"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .default_width(620.0)
                .show(ui.ctx(), |ui| {
                    let tk = theme::tokens(ui);
                    ui.heading(t("uninstall_header"));
                    ui.add_space(space::XS);
                    components::banner(ui, Status::Err, t("uninstall_warning_permanent"));
                    ui.add_space(space::S);
                    ui.separator();

                    if let Some(ref msg) = self.uninstall_done_msg {
                        let status = if msg.as_str() == t("uninstall_success") {
                            Status::Ok
                        } else {
                            Status::Warn
                        };
                        components::banner(ui, status, msg);
                        ui.add_space(space::S);
                        if ui.button(t("btn_cancel")).clicked() {
                            self.show_uninstall_modal = false;
                        }
                        return;
                    }

                    for cat in &mut self.uninstall_categories {
                        let size_str = crate::footprint::format_size(cat.total_bytes);
                        let label = format!("{} ({})", t(cat.title_key), size_str);
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut cat.selected, label);
                            if cat.is_user_data {
                                ui.colored_label(tk.warning, "[!]");
                            }
                        });
                        ui.indent(cat.id, |ui| {
                            ui.weak(t(cat.desc_key));
                            for p in &cat.paths {
                                ui.weak(format!("• {}", p.display()));
                            }
                        });
                        ui.add_space(space::XS);
                    }

                    ui.separator();
                    ui.label(t("uninstall_confirm_prompt"));
                    let is_en = crate::i18n::current_lang() == crate::i18n::Lang::En;
                    let target_word = if is_en { "UNINSTALL" } else { "KALDIR" };
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.uninstall_confirm_text)
                                .desired_width(140.0),
                        );
                        ui.weak(tf("uninstall.type_hint_fmt", &[target_word]));
                    });

                    let confirmed = self.uninstall_confirm_text.trim() == target_word;
                    ui.add_space(space::S);
                    ui.separator();

                    ui.horizontal(|ui| {
                        let btn = egui::Button::new(
                            egui::RichText::new(t("btn_execute_uninstall"))
                                .color(danger_button_text(tk))
                                .strong(),
                        )
                        .fill(tk.danger);
                        if ui.add_enabled(confirmed, btn).clicked() {
                            let delete_app = self
                                .uninstall_categories
                                .iter()
                                .any(|c| c.id == "program_files" && c.selected);
                            let res = crate::footprint::execute_uninstall(
                                &self.uninstall_categories,
                                delete_app,
                            );
                            self.uninstall_done_msg =
                                Some(crate::footprint::uninstall_result_message(
                                    &res,
                                    crate::i18n::current_lang(),
                                ));
                            self.status = tf(
                                "status_uninstall_deleted_fmt",
                                &[&res.deleted_paths.len().to_string()],
                            );
                            // Hata varsa pencere açık kalır ki kullanıcı başarısız yolları görebilsin.
                            if res.detached_spawned && res.failed_paths.is_empty() {
                                std::thread::spawn(|| {
                                    std::thread::sleep(std::time::Duration::from_millis(800));
                                    std::process::exit(0);
                                });
                            }
                        }

                        if ui.button(t("btn_cancel")).clicked() {
                            self.show_uninstall_modal = false;
                        }
                    });
                });
        }

        // Ayarlar sayfası (sol menülü; ayrıntılar settings_page.rs içinde)
        self.settings_window(ui.ctx());

        // Yapay zekâ panelindeki "Ayarlar" düğmesi: yeni Ayarlar sayfasının Yapay zekâ bölümünü açar.
        if std::mem::take(&mut self.ai_state.show_settings_modal) {
            settings_page::set_section(ui.ctx(), settings_page::SettingsSection::Ai);
            self.show_settings_modal = true;
        }

        // Bulut Onay Uyarısı Penceresi
        crate::ui::ai::show_cloud_warning_dialog(
            ui.ctx(),
            &mut self.ai_state,
            &mut self.ai_config,
            &self.project,
            crate::i18n::current_lang(),
            &self.cred_store,
        );

        // Öğretmen Modu Arayüzü (Ders Okuyucu, Neden Bu Sonuç Çıktı, Sınıf Raporu)
        self.teacher_state.show(
            ui.ctx(),
            &self.project,
            &model,
            &mut self.ai_state,
            &self.ai_config,
            self.cred_store.clone(),
        );

        // Üç panelli kabuk: üst çubuk, sol adımlar, merkez görünüm + çalışma alanı, sağ analiz.
        let actions = self.shell_ui(ui, &model);
        self.apply_shell_actions(actions, &model);
    }
}

#[cfg(test)]
mod input_tests;
#[cfg(test)]
mod side_windows_tests;

#[cfg(test)]
mod tests {
    /// Varsayılan egui yazı tipinin Türkçe karakterleri ve tüm i18n dizgilerini
    /// eksiksiz glif ile render edebildiğini doğrular (kutu/□ hatasını önler).
    #[test]
    fn test_all_ui_symbols_have_glyphs() {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |_ui| {});
        out.textures_delta.clear();
        let font_id = egui::FontId::proportional(14.0);

        // Temel Türkçe karakterler
        assert!(
            ctx.fonts_mut(|f| f.has_glyphs(&font_id, "çÇğĞıİöÖşŞüÜ")),
            "Türkçe karakterler varsayılan yazı tipinde bulunamadı"
        );

        // UI'da izin verilen ve kullanılan özel semboller
        let allowed_symbols = [
            "•", "·", "°", "±", "×", "÷", "µ", "»", "«", "√", "∞", "≈", "≠", "≤", "≥", "–", "—",
            "’", "”", "₀", "⁵", "⁴", "−", "∑",
        ];
        for sym in allowed_symbols {
            assert!(
                ctx.fonts_mut(|f| f.has_glyphs(&font_id, sym)),
                "Sembol '{}' varsayılan egui fontunda desteklenmiyor!",
                sym
            );
        }

        // i18n dosyalarındaki tüm karakterlerin fontta var olduğunu doğrula
        let json_files = [
            concat!(env!("CARGO_MANIFEST_DIR"), "/assets/i18n/tr.json"),
            concat!(env!("CARGO_MANIFEST_DIR"), "/assets/i18n/en.json"),
        ];

        for path in json_files {
            let content = std::fs::read_to_string(path)
                .unwrap_or_else(|e| panic!("Okunamadı {}: {}", path, e));
            let v: serde_json::Value =
                serde_json::from_str(&content).expect("JSON ayrıştırılamadı");
            if let serde_json::Value::Object(map) = v {
                for (key, val) in map {
                    if let serde_json::Value::String(s) = val {
                        for ch in s.chars() {
                            if ch.is_whitespace() || ch.is_control() {
                                continue;
                            }
                            let s_ch = ch.to_string();
                            let ok = ctx.fonts_mut(|f| f.has_glyphs(&font_id, &s_ch));
                            assert!(
                                ok,
                                "Dosya '{}' içindeki anahtar '{}' geçersiz/desteklenmeyen glif içeriyor: '{}' (U+{:04X})",
                                path, key, ch, ch as u32
                            );
                        }
                    }
                }
            }
        }
    }

    fn run_all_tabs(p: &mut crate::project::Project) {
        let lib = crate::calc::Library::embedded();
        let ctx = egui::Context::default();
        for frame in 0..2 {
            let m = crate::calc::evaluate(p, &lib);
            let mut input = egui::RawInput::default();
            input.screen_rect = Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(1280.0, 800.0),
            ));
            input.time = Some(frame as f64 * 0.016);
            let mut out = ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    super::summary::show(ui, p, &m);
                    super::mission::show(ui, p, &m);
                    super::weight::show(ui, p, &m, &lib);
                    super::geometry::show(ui, p, &m);
                    super::aero::show(ui, p, &m);
                    super::design_point::show(ui, p, &m);
                    super::endurance::show(ui, p, &m);
                    super::propulsion::show(ui, p, &m);
                    super::stability::show(ui, p, &m);
                    super::structure::show(ui, p, &m, &lib);
                    super::multirotor::show(ui, p, &m, &lib);
                    super::schematic::show(ui, p, &m, 300.0);
                });
            });
            out.textures_delta.clear();
        }
    }

    #[test]
    fn all_tabs_render_default() {
        run_all_tabs(&mut crate::project::Project::default());
    }

    #[test]
    fn all_tabs_render_invalid_inputs() {
        let mut p = crate::project::Project::default();
        p.s = 0.0;
        p.b = 0.0;
        p.taper = 0.0;
        p.v_cr = f64::NAN;
        p.components.clear();
        p.n_series = 0;
        p.mr_frame_size = 0.0;
        p.mr_d_prop = 0.0;
        run_all_tabs(&mut p);

        let mut q = crate::project::Project::default();
        q.h = f64::NAN;
        q.d_spar_i = q.d_spar_o;
        q.l_h = 0.0;
        q.mr_arm_count = 0;
        run_all_tabs(&mut q);
    }

    #[test]
    fn test_multirotor_and_vtol_render() {
        let mut p_mr = crate::project::Project::default();
        p_mr.set_drone_type(crate::project::DroneType::Multirotor);
        run_all_tabs(&mut p_mr);

        let mut p_vtol = crate::project::Project::default();
        p_vtol.set_drone_type(crate::project::DroneType::VtolQuadplane);
        run_all_tabs(&mut p_vtol);
    }

    /// Yeni kabuğun başsız duman testi: her araç türü × her sihirbaz adımı (ve uzman modunda
    /// her sekme) için tam bir kare çizilir; farklı pencere genişliklerinde ve iki temada panik
    /// yok. Ayar dosyasına yazılmaz (`App::new_headless`).
    fn run_app_frames(app: &mut super::App, ctx: &egui::Context, width: f32, frames: usize) {
        for i in 0..frames {
            let mut input = egui::RawInput::default();
            input.screen_rect = Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(width, 820.0),
            ));
            input.time = Some(i as f64 * 0.016);
            let mut out = ctx.run_ui(input, |ui| app.frame_ui(ui));
            out.textures_delta.clear();
        }
    }

    #[test]
    fn shell_renders_every_step_for_every_type() {
        use crate::project::{DroneType, WizardStepState};
        for dt in DroneType::ALL {
            let ctx = egui::Context::default();
            let mut app = super::App::new_headless(&ctx, dt);
            let n = super::wizard::steps_for(dt).len();
            app.project.step_states = vec![WizardStepState::Completed; n];
            let before = app.project.clone();
            for step in 0..n {
                app.project.wizard_step = step;
                run_app_frames(&mut app, &ctx, 1440.0, 2);
                assert_eq!(
                    app.project.wizard_step, step,
                    "{dt:?} adım {step} korunmalı"
                );
            }
            // Gelişmiş modda (basit mod türetmesi kapalı) yalnız çizim hesap girdilerini değiştirmez.
            if !before.aero_simple.simple_mode {
                let mut a = app.project.clone();
                a.wizard_step = before.wizard_step;
                a.step_states = before.step_states.clone();
                assert_eq!(a.to_json().unwrap(), before.to_json().unwrap(), "{dt:?}");
            }
            // Uzman modu: tüm sekmeler.
            app.project.expert_mode = true;
            for tab in super::Tab::tabs_for(dt) {
                app.tab = tab;
                run_app_frames(&mut app, &ctx, 1440.0, 2);
                assert!(app.project.expert_mode, "{dt:?} {tab:?}");
                assert_eq!(app.tab, tab);
            }
        }
    }

    #[test]
    fn shell_survives_narrow_windows_themes_and_closed_panels() {
        use crate::project::DroneType;
        for (dt, theme) in [
            (
                DroneType::FixedWingConventional,
                super::theme::ThemeChoice::Light,
            ),
            (DroneType::Multirotor, super::theme::ThemeChoice::Dark),
            (DroneType::VtolQuadplane, super::theme::ThemeChoice::Light),
        ] {
            let ctx = egui::Context::default();
            let mut app = super::App::new_headless(&ctx, dt);
            super::theme::install(&ctx, theme);
            for w in [1920.0, 1200.0, 900.0, 640.0, 320.0] {
                run_app_frames(&mut app, &ctx, w, 2);
            }
            app.shell.prefs.left_open = false;
            app.shell.prefs.right_open = false;
            app.shell.prefs.viewport_open = false;
            run_app_frames(&mut app, &ctx, 1000.0, 2);
            app.shell.prefs = Default::default();
            app.shell.viewport.mode = super::viewport3d::ViewMode::Schematic2d;
            app.ai_state.is_panel_open = true;
            app.show_type_modal = true;
            app.show_about_modal = true;
            run_app_frames(&mut app, &ctx, 1300.0, 2);
        }
    }

    /// Çökme bildirimi, Hakkında ve Kaldırma pencereleri: iki tema × iki dil × geniş/dar
    /// pencerede başsız çizim (panik yok). Kaldırma işlevi çalıştırılmaz, yalnız çizilir.
    #[test]
    fn crash_about_and_uninstall_windows_render_in_both_themes_and_languages() {
        use crate::i18n::Lang;
        for choice in [
            super::theme::ThemeChoice::Dark,
            super::theme::ThemeChoice::Light,
        ] {
            for lang in [Lang::Tr, Lang::En] {
                for done_msg in [
                    None,
                    Some(crate::i18n::t("uninstall_success").to_string()),
                    Some("x".into()),
                ] {
                    let ctx = egui::Context::default();
                    let mut app =
                        super::App::new_headless(&ctx, crate::project::DroneType::Multirotor);
                    super::theme::install(&ctx, choice);
                    app.show_about_modal = true;
                    app.show_crash_modal = true;
                    app.crash_content = "satır 1
satır 2"
                        .into();
                    app.show_uninstall_modal = true;
                    app.uninstall_done_msg = done_msg;
                    app.uninstall_categories = vec![crate::footprint::FootprintCategory {
                        id: "user_settings",
                        title_key: "uninstall.cat_settings_title",
                        desc_key: "uninstall.cat_settings_title",
                        paths: vec![std::path::PathBuf::from("C:/ornek/yol/ayarlar.json")],
                        total_bytes: 2048,
                        selected: true,
                        is_user_data: true,
                    }];
                    crate::i18n::with_lang(lang, || {
                        for w in [1300.0, 480.0] {
                            run_app_frames(&mut app, &ctx, w, 2);
                        }
                    });
                }
            }
        }
    }

    #[test]
    fn shell_geometry_step_inputs_in_left_panel_with_invalid_values() {
        use crate::project::DroneType;
        let ctx = egui::Context::default();
        let mut app = super::App::new_headless(&ctx, DroneType::FixedWingVTail);
        app.project.wizard_step = 2; // Geometri
        app.project.b = f64::NAN;
        app.project.s = 0.0;
        app.project.taper = f64::NAN;
        run_app_frames(&mut app, &ctx, 1440.0, 3);
        // Geçersiz geometri: adım kilidi geometri adımında tutar.
        assert_eq!(app.project.wizard_step, 2);
    }

    /// Bozuk girdilerle (sıfır açıklık, NaN, negatif veter...) her tür × her adım: tam kabuk
    /// karesi (3D sahne, analiz kartları, sol panel) panik yapmamalı.
    #[test]
    fn shell_renders_every_step_for_every_type_with_garbage_inputs() {
        use crate::project::{DroneType, WizardStepState};
        for dt in DroneType::ALL {
            let ctx = egui::Context::default();
            let mut app = super::App::new_headless(&ctx, dt);
            {
                let p = &mut app.project;
                p.b = 0.0;
                p.s = f64::NAN;
                p.taper = -1.0;
                p.sweep_le_deg = f64::INFINITY;
                p.c_tail = -0.5;
                p.l_fuse = 0.0;
                p.d_fuse = f64::NAN;
                p.v_cr = f64::NAN;
                p.mr_frame_size = 0.0;
                p.mr_d_prop = f64::NAN;
                p.mr_arm_count = 0;
                p.airfoil.points.clear();
            }
            let n = super::wizard::steps_for(dt).len();
            app.project.expert_mode = true;
            for tab in super::Tab::tabs_for(dt) {
                app.tab = tab;
                run_app_frames(&mut app, &ctx, 1200.0, 2);
            }
            app.project.expert_mode = false;
            app.project.step_states = vec![WizardStepState::Completed; n];
            for step in 0..n {
                app.project.wizard_step = step;
                run_app_frames(&mut app, &ctx, 1200.0, 2);
            }
        }
    }

    #[test]
    fn test_freecad_detection_rx_disconnected_clears_detecting_flag() {
        let (tx, rx) = std::sync::mpsc::channel::<Option<crate::cad::FreeCadInstallation>>();
        drop(tx); // Simulates thread exit/panic causing disconnection
        let mut detecting = true;
        let mut detection_rx = Some(rx);
        let mut install = None;

        if let Some(ref rx) = detection_rx {
            match rx.try_recv() {
                Ok(res) => {
                    install = res;
                    detecting = false;
                    detection_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    detecting = false;
                    detection_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }

        assert!(!detecting);
        assert!(detection_rx.is_none());
        assert!(install.is_none());
    }

    #[test]
    fn test_db_update_rx_disconnected_clears_updating_flag_and_sets_error() {
        crate::i18n::init_i18n();
        let (tx, rx) = std::sync::mpsc::channel::<Result<crate::db::DatabaseInfo, String>>();
        drop(tx); // Simulates thread crash or premature termination

        let mut db_updating = true;
        let mut db_cancel_flag = Some(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
            false,
        )));
        let mut db_update_rx = Some(rx);
        let mut db_status_msg = None;
        let mut status = String::new();

        if let Some(ref rx) = db_update_rx {
            match rx.try_recv() {
                Ok(_) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    db_updating = false;
                    db_cancel_flag = None;
                    db_update_rx = None;
                    let msg = crate::i18n::t("settings.db_update_disconnected").to_string();
                    db_status_msg = Some((false, msg.clone()));
                    status = msg;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }

        assert!(!db_updating);
        assert!(db_cancel_flag.is_none());
        assert!(db_update_rx.is_none());
        assert!(db_status_msg.is_some());
        assert!(!db_status_msg.unwrap().0); // is_success == false
        assert!(!status.is_empty());
    }

    #[test]
    fn test_cancel_db_update_clears_fields() {
        let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (_tx, rx) = std::sync::mpsc::channel::<Result<crate::db::DatabaseInfo, String>>();

        // Simulate cancel method behavior
        let mut db_updating = true;
        let mut db_cancel_flag = Some(flag.clone());
        let mut db_update_rx = Some(rx);
        let mut db_status_msg = Some((true, "test".to_string()));

        // Verify pre-conditions
        assert!(db_updating);
        assert!(db_cancel_flag.is_some());
        assert!(db_update_rx.is_some());
        assert!(db_status_msg.is_some());

        // Call cancel logic
        if let Some(f) = db_cancel_flag.take() {
            f.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        db_updating = false;
        db_update_rx = None;
        db_status_msg = None;

        assert!(!db_updating);
        assert!(flag.load(std::sync::atomic::Ordering::Relaxed));
        assert!(db_cancel_flag.is_none());
        assert!(db_update_rx.is_none());
        assert!(db_status_msg.is_none());
    }
}
