// SPDX-License-Identifier: AGPL-3.0-or-later
//! Tam Ayarlar sayfası: soldaki menüde bölümler, sağda seçili bölümün içeriği.
//!
//! Yalnız programda gerçekten karşılığı olan ayarlar bağlanır (sahte düğme yok):
//! Genel (dil, tema), Görünüm (3D ızgara, kamera varsayılanı, arayüz ölçeği), Yapay zekâ,
//! CAD/FreeCAD (yol, sınama, süre sınırları), Veri ve gizlilik, Veritabanı, Hakkında.
//! Renk ve boşluklar `theme` tokenlarından gelir.

use super::components::{self, Status, section_header, segmented};
use super::theme::{self, ThemeChoice, font, radius, space};
use super::viewport3d::camera::Projection;
use super::{App, ai};
use crate::cad::CadLimits;
use crate::i18n::{AppSettings, Lang, t, tf};
use egui::{Align, Layout, RichText, Ui, vec2};

/// Ayarlar sayfasının bölümleri (soldaki menü sırası).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SettingsSection {
    General,
    View,
    Ai,
    Cad,
    Privacy,
    Database,
    About,
}

impl SettingsSection {
    pub const ALL: [SettingsSection; 7] = [
        SettingsSection::General,
        SettingsSection::View,
        SettingsSection::Ai,
        SettingsSection::Cad,
        SettingsSection::Privacy,
        SettingsSection::Database,
        SettingsSection::About,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SettingsSection::General => t("set.nav.general"),
            SettingsSection::View => t("set.nav.view"),
            SettingsSection::Ai => t("set.nav.ai"),
            SettingsSection::Cad => t("set.nav.cad"),
            SettingsSection::Privacy => t("set.nav.privacy"),
            SettingsSection::Database => t("set.nav.database"),
            SettingsSection::About => t("set.nav.about"),
        }
    }
}

/// Proje bağlantısı (Hakkında).
pub const PROJECT_URL: &str = "https://github.com/werseus-tech/werseus-uav-calc";

/// Açılışta ayar dosyasından okunan, uygulamaya bir kez uygulanan tercihler.
#[derive(Clone, Copy, Debug)]
pub struct StartupPrefs {
    pub ui_scale: f32,
    pub view_grid: bool,
    pub view_ortho: bool,
    pub cad_limits: CadLimits,
}

impl StartupPrefs {
    pub fn from_settings(s: &AppSettings) -> Self {
        Self {
            ui_scale: clamp_scale(s.ui_scale),
            view_grid: s.view_grid,
            view_ortho: s.view_ortho,
            cad_limits: s.cad_limits.clamped(),
        }
    }

    /// Tercihleri uygulamaya uygular (3D varsayılanları, arayüz ölçeği, CAD süre sınırları).
    pub fn apply(self, app: &mut App, ctx: &egui::Context) {
        ctx.set_zoom_factor(self.ui_scale);
        app.shell.viewport.show_grid = self.view_grid;
        app.shell.viewport.cam.projection = projection_for(self.view_ortho);
        app.outputs_ui.limits = self.cad_limits;
    }
}

pub const SCALE_MIN: f32 = 0.8;
pub const SCALE_MAX: f32 = 1.5;

/// Bozuk/NaN ölçeği güvenli aralığa getirir.
pub fn clamp_scale(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(SCALE_MIN, SCALE_MAX)
    } else {
        1.0
    }
}

fn projection_for(ortho: bool) -> Projection {
    if ortho {
        Projection::Orthographic
    } else {
        Projection::Perspective
    }
}

/// Ayar dosyasını okuyup değiştirir ve yazar (yalnız kullanıcı bir denetimi değiştirince çağrılır).
fn persist(f: impl FnOnce(&mut AppSettings)) {
    let mut s = crate::i18n::load_settings();
    f(&mut s);
    crate::i18n::save_settings(&s);
}

/// Klasörü işletim sisteminin dosya gezgininde açar (yoksa oluşturur).
pub fn open_folder(path: &std::path::Path) {
    let _ = std::fs::create_dir_all(path);
    #[cfg(windows)]
    let _ = crate::cad::new_command("explorer").arg(path).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(path).spawn();
    #[cfg(all(not(windows), not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}

fn section_id() -> egui::Id {
    egui::Id::unique("settings_page_section")
}

/// Seçili bölüm (oturum belleğinde; yoksa Genel).
pub fn current_section(ctx: &egui::Context) -> SettingsSection {
    ctx.data(|d| d.get_temp::<SettingsSection>(section_id()))
        .unwrap_or(SettingsSection::General)
}

pub fn set_section(ctx: &egui::Context, s: SettingsSection) {
    ctx.data_mut(|d| d.insert_temp(section_id(), s));
}

/// Bir ayar satırı: solda ad + açıklama, sağda denetim.
fn setting_row(ui: &mut Ui, label: &str, desc: &str, control: impl FnOnce(&mut Ui)) {
    let tk = theme::tokens(ui);
    ui.add_space(space::XS);
    ui.horizontal_wrapped(|ui| {
        ui.vertical(|ui| {
            ui.set_max_width((ui.available_width() * 0.55).max(180.0));
            ui.label(RichText::new(label).size(font::BODY).color(tk.text));
            if !desc.is_empty() {
                ui.label(RichText::new(desc).size(font::CAPTION).color(tk.text_weak));
            }
        });
        ui.with_layout(Layout::top_down(Align::Min), control);
    });
    ui.add_space(space::XS);
}

fn card(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let tk = theme::tokens(ui);
    egui::Frame::new()
        .fill(tk.surface)
        .stroke(egui::Stroke::new(1.0, tk.border))
        .corner_radius(theme::corner(radius::M))
        .inner_margin(theme::margin(space::M))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
}

fn weak(ui: &mut Ui, text: &str) {
    let tk = theme::tokens(ui);
    ui.label(RichText::new(text).size(font::CAPTION).color(tk.text_weak));
}

impl App {
    /// Ayarlar penceresi (`show_settings_modal` açıkken). Soldaki menüden bölüm seçilir.
    pub(crate) fn settings_window(&mut self, ctx: &egui::Context) -> Option<egui::Rect> {
        if !self.show_settings_modal {
            return None;
        }
        let mut open = true;
        let win = egui::Window::new(t("settings.title"))
            .collapsible(false)
            .resizable(true)
            .open(&mut open)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .default_size([760.0, 500.0])
            .min_size([560.0, 360.0])
            .max_width((ctx.content_rect().width() - 32.0).max(560.0))
            .show(ctx, |ui| self.settings_contents(ui));
        if !open {
            self.show_settings_modal = false;
        }
        win.map(|r| r.response.rect)
    }

    /// Pencere içeriği: sol menü + seçili bölüm.
    pub(crate) fn settings_contents(&mut self, ui: &mut Ui) {
        let tk = theme::tokens(ui);
        let menu_w = 168.0;
        let h = (ui.available_height() - 4.0).max(300.0);
        let section = current_section(ui.ctx());
        let mut next = section;
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(vec2(menu_w, h), Layout::top_down(Align::Min), |ui| {
                ui.set_width(menu_w);
                for s in SettingsSection::ALL {
                    let sel = s == section;
                    let text = RichText::new(s.label()).size(font::BODY).color(if sel {
                        tk.accent
                    } else {
                        tk.text
                    });
                    let b =
                        egui::Button::selectable(sel, text).min_size(vec2(menu_w - space::S, 28.0));
                    if ui.add(b).clicked() {
                        next = s;
                    }
                }
            });
            ui.separator();
            let w = (ui.available_width() - space::L).max(240.0);
            ui.allocate_ui_with_layout(vec2(w, h), Layout::top_down(Align::Min), |ui| {
                ui.set_width(w);
                egui::ScrollArea::vertical()
                    .id_salt("settings_scroll")
                    .max_height(h)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width(w - space::L);
                        self.settings_section_ui(ui, section);
                    });
            });
        });
        if next != section {
            set_section(ui.ctx(), next);
        }
    }

    fn settings_section_ui(&mut self, ui: &mut Ui, section: SettingsSection) {
        match section {
            SettingsSection::General => self.settings_general(ui),
            SettingsSection::View => self.settings_view(ui),
            SettingsSection::Ai => self.settings_ai(ui),
            SettingsSection::Cad => self.settings_cad(ui),
            SettingsSection::Privacy => self.settings_privacy(ui),
            SettingsSection::Database => self.settings_database(ui),
            SettingsSection::About => self.settings_about(ui),
        }
    }

    fn settings_general(&mut self, ui: &mut Ui) {
        section_header(ui, SettingsSection::General.label());
        card(ui, |ui| {
            setting_row(ui, t("set.lang"), t("set.lang.desc"), |ui| {
                let mut lang = crate::i18n::current_lang();
                if segmented(
                    ui,
                    &mut lang,
                    &[(Lang::Tr, "Türkçe", ""), (Lang::En, "English", "")],
                ) {
                    crate::i18n::set_lang(lang);
                }
            });
            ui.separator();
            setting_row(ui, t("set.theme"), t("set.theme.desc"), |ui| {
                let mut choice = self.shell.theme;
                if segmented(
                    ui,
                    &mut choice,
                    &[
                        (ThemeChoice::Dark, t("set.theme.dark"), ""),
                        (ThemeChoice::Light, t("set.theme.light"), ""),
                    ],
                ) {
                    self.shell.theme = choice;
                    theme::install(ui.ctx(), choice);
                    persist(|s| s.theme = choice);
                }
            });
        });
    }

    fn settings_view(&mut self, ui: &mut Ui) {
        section_header(ui, SettingsSection::View.label());
        weak(ui, t("set.view.note"));
        card(ui, |ui| {
            setting_row(ui, t("set.view.grid"), t("set.view.grid.desc"), |ui| {
                let mut on = self.shell.viewport.show_grid;
                if ui.checkbox(&mut on, t("set.view.grid.on")).changed() {
                    self.shell.viewport.show_grid = on;
                    persist(|s| s.view_grid = on);
                }
            });
            ui.separator();
            setting_row(ui, t("set.view.proj"), t("set.view.proj.desc"), |ui| {
                let mut ortho = self.shell.viewport.cam.projection == Projection::Orthographic;
                if segmented(
                    ui,
                    &mut ortho,
                    &[
                        (false, t("set.view.persp"), ""),
                        (true, t("set.view.ortho"), ""),
                    ],
                ) {
                    self.shell.viewport.cam.projection = projection_for(ortho);
                    persist(|s| s.view_ortho = ortho);
                }
            });
            ui.separator();
            setting_row(ui, t("set.view.scale"), t("set.view.scale.desc"), |ui| {
                let id = egui::Id::unique("settings_ui_scale_pending");
                let mut v: f32 = ui
                    .ctx()
                    .data(|d| d.get_temp(id))
                    .unwrap_or_else(|| ui.ctx().zoom_factor());
                let r = ui.add(
                    egui::Slider::new(&mut v, SCALE_MIN..=SCALE_MAX)
                        .step_by(0.05)
                        .custom_formatter(|x, _| format!("{:.0}%", x * 100.0)),
                );
                let reset = ui.small_button(t("set.view.scale.reset")).clicked();
                if reset {
                    v = 1.0;
                }
                ui.ctx().data_mut(|d| d.insert_temp(id, v));
                // Sürükleme sürerken ölçek değiştirilirse düzen kayar; bırakınca uygulanır.
                if reset || (r.changed() && !r.dragged()) || r.drag_stopped() {
                    let v = clamp_scale(v);
                    ui.ctx().set_zoom_factor(v);
                    persist(|s| s.ui_scale = v);
                }
            });
        });
    }

    fn settings_ai(&mut self, ui: &mut Ui) {
        section_header(ui, SettingsSection::Ai.label());
        weak(ui, t("set.ai.intro"));
        ui.add_space(space::S);
        card(ui, |ui| {
            ai::ai_settings_form(
                ui,
                &mut self.ai_state,
                &mut self.ai_config,
                &self.cred_store,
            );
        });
        ui.add_space(space::S);
        if components::primary_button(ui, t("set.ai.save"), true).clicked() {
            ai::save_ai_config(&self.ai_config, &self.cred_store);
            self.status = t("set.ai.saved").to_string();
        }
    }

    fn settings_cad(&mut self, ui: &mut Ui) {
        section_header(ui, SettingsSection::Cad.label());
        weak(ui, t("settings.freecad_desc"));
        ui.add_space(space::S);
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(t("settings.freecad_path_label"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.settings_freecad_cmd)
                        .desired_width((ui.available_width() - 90.0).max(120.0)),
                );
                if ui.button(t("settings.btn_browse")).clicked() {
                    if let Some(p) = rfd::FileDialog::new()
                        .add_filter("FreeCADCmd", &["exe", "*"])
                        .pick_file()
                    {
                        self.settings_freecad_cmd = p.display().to_string();
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                let idle = !self.freecad_detecting;
                if ui
                    .add_enabled(idle, egui::Button::new(t("settings.btn_autodetect")))
                    .clicked()
                {
                    self.settings_freecad_cmd.clear();
                    self.trigger_freecad_detection(None, Some(ui.ctx().clone()));
                }
                let typed = self.settings_freecad_cmd.trim().to_string();
                if ui
                    .add_enabled(
                        idle && !typed.is_empty(),
                        egui::Button::new(t("set.cad.test")),
                    )
                    .on_hover_text(t("set.cad.test.tip"))
                    .clicked()
                {
                    self.trigger_freecad_detection(Some(typed), Some(ui.ctx().clone()));
                }
            });
            // Anlık algılama durumu (kare döngüsü asla süreç başlatmaz).
            if self.freecad_detecting {
                components::banner(ui, Status::Info, t("settings.status_detecting"));
            } else if let Some(inst) = &self.freecad_install {
                let msg = tf(
                    "settings.status_detected",
                    &[&inst.version, &inst.cmd_path.display().to_string()],
                );
                components::banner(ui, Status::Ok, &msg);
            } else {
                components::banner(ui, Status::Warn, t("settings.status_not_found"));
            }
        });

        ui.add_space(space::S);
        section_header(ui, t("set.cad.limits"));
        weak(ui, t("set.cad.limits.desc"));
        ui.add_space(space::XS);
        card(ui, |ui| {
            let lim = &mut self.outputs_ui.limits;
            minutes_row(ui, t("set.cad.limit_part"), &mut lim.part_secs);
            minutes_row(ui, t("set.cad.limit_asm"), &mut lim.assembly_secs);
            minutes_row(ui, t("set.cad.limit_total"), &mut lim.total_secs);
            if ui.small_button(t("set.cad.limits.default")).clicked() {
                *lim = CadLimits::default();
            }
        });
        ui.add_space(space::S);
        if components::primary_button(ui, t("settings.btn_save"), true).clicked() {
            self.save_cad_settings(ui.ctx());
        }
    }

    /// CAD yolunu ve süre sınırlarını kaydeder; FreeCAD'i yeniden algılar.
    pub(crate) fn save_cad_settings(&mut self, ctx: &egui::Context) {
        let trimmed = self.settings_freecad_cmd.trim().to_string();
        let path = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        };
        self.outputs_ui.limits = self.outputs_ui.limits.clamped();
        let limits = self.outputs_ui.limits;
        persist(|s| {
            s.freecad_cmd_path = path.clone();
            s.cad_limits = limits;
        });
        self.trigger_freecad_detection(path, Some(ctx.clone()));
        self.status = t("set.cad.saved").to_string();
    }

    fn settings_privacy(&mut self, ui: &mut Ui) {
        section_header(ui, SettingsSection::Privacy.label());
        components::banner(ui, Status::Ok, t("set.privacy.statement"));
        ui.add_space(space::S);
        card(ui, |ui| {
            weak(ui, t("set.privacy.report_note"));
            ui.add_space(space::S);
            let logs = crate::support::logs_dir();
            let reports = crate::support::reports_dir();
            ui.horizontal_wrapped(|ui| {
                if ui.button(t("set.privacy.open_logs")).clicked() {
                    open_folder(&logs);
                }
                if ui.button(t("set.privacy.open_reports")).clicked() {
                    open_folder(&reports);
                }
                if ui.button(t("set.privacy.report_bug")).clicked() {
                    self.show_feedback_modal = true;
                }
            });
            ui.add_space(space::XS);
            weak(
                ui,
                &tf("set.privacy.logs_path", &[logs.display().to_string()]),
            );
            weak(
                ui,
                &tf("set.privacy.reports_path", &[reports.display().to_string()]),
            );
        });
    }

    fn settings_database(&mut self, ui: &mut Ui) {
        section_header(ui, SettingsSection::Database.label());
        weak(ui, t("settings.db_desc"));
        ui.add_space(space::S);
        card(ui, |ui| {
            let src_name = match self.lib.db_info.source {
                crate::db::DbSource::Embedded => t("settings.db_source_embedded"),
                crate::db::DbSource::Community => t("settings.db_source_community"),
            };
            ui.horizontal_wrapped(|ui| {
                ui.strong(t("settings.db_source_label"));
                ui.label(src_name);
            });
            ui.label(tf(
                "settings.db_version_fmt",
                &[
                    self.lib.db_info.schema_version.to_string(),
                    self.lib.db_info.updated.clone(),
                    self.lib.db_info.cell_count.to_string(),
                    self.lib.db_info.pack_count.to_string(),
                ],
            ));
            ui.add_space(space::XS);
            ui.horizontal_wrapped(|ui| {
                if !self.db_updating {
                    if ui.button(t("settings.btn_update_db")).clicked() {
                        self.trigger_db_update();
                    }
                    if self.lib.db_info.source == crate::db::DbSource::Community
                        && ui.button(t("settings.btn_reset_db")).clicked()
                    {
                        let _ = crate::db::reset_to_embedded();
                        self.lib = crate::calc::Library::load();
                        self.db_status_msg =
                            Some((true, t("settings.db_source_embedded").to_string()));
                    }
                } else {
                    ui.spinner();
                    ui.label(t("settings.db_updating"));
                    if ui.button(t("settings.btn_cancel_update")).clicked() {
                        self.cancel_db_update();
                    }
                }
            });
            if let Some((ok, msg)) = &self.db_status_msg {
                components::banner(ui, Status::from_ok(*ok), msg);
            }
        });
    }

    fn settings_about(&mut self, ui: &mut Ui) {
        section_header(ui, SettingsSection::About.label());
        card(ui, |ui| {
            ui.label(
                RichText::new("WerSeuS – UAV Calc")
                    .size(font::HEADING)
                    .strong(),
            );
            ui.label(format!(
                "{}: v{}",
                t("about_version_label"),
                env!("CARGO_PKG_VERSION")
            ));
            ui.horizontal_wrapped(|ui| {
                ui.strong(format!("{}:", t("about_license_label")));
                ui.label(t("about_license_text"));
            });
            ui.horizontal_wrapped(|ui| {
                ui.strong(format!("{}:", t("set.about.project")));
                ui.hyperlink_to(PROJECT_URL, PROJECT_URL);
            });
            let db_src = match self.lib.db_info.source {
                crate::db::DbSource::Embedded => t("about.db_embedded"),
                crate::db::DbSource::Community => t("about.db_community"),
            };
            ui.label(format!(
                "{}: {} (v{}, {})",
                t("about.db_label"),
                db_src,
                self.lib.db_info.schema_version,
                self.lib.db_info.updated
            ));
        });
    }
}

/// Saniye cinsinden saklanan süreyi dakika olarak düzenler (en az 1, en çok 720 dk).
fn minutes_row(ui: &mut Ui, label: &str, secs: &mut u64) {
    setting_row(ui, label, "", |ui| {
        let mut mins = (*secs / 60).clamp(1, CadLimits::MAX_SECS / 60);
        if ui
            .add(egui::DragValue::new(&mut mins).range(1..=CadLimits::MAX_SECS / 60))
            .changed()
        {
            *secs = mins * 60;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::DroneType;

    fn frame(app: &mut App, ctx: &egui::Context, size: egui::Vec2) -> Option<egui::Rect> {
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size));
        let mut rect = None;
        let mut out = ctx.run_ui(input, |ui| {
            rect = app.settings_window(ui.ctx());
        });
        out.textures_delta.clear();
        rect
    }

    #[test]
    fn startup_prefs_sanitize_bad_values() {
        let s = AppSettings {
            ui_scale: f32::NAN,
            ..Default::default()
        };
        assert_eq!(StartupPrefs::from_settings(&s).ui_scale, 1.0);
        let s = AppSettings {
            ui_scale: 9.0,
            ..Default::default()
        };
        assert_eq!(StartupPrefs::from_settings(&s).ui_scale, SCALE_MAX);
        let s = AppSettings {
            cad_limits: CadLimits {
                part_secs: 0,
                assembly_secs: 10,
                total_secs: u64::MAX,
            },
            ..Default::default()
        };
        let p = StartupPrefs::from_settings(&s);
        assert_eq!(p.cad_limits.part_secs, CadLimits::MIN_SECS);
        assert_eq!(p.cad_limits.total_secs, CadLimits::MAX_SECS);
    }

    #[test]
    fn old_settings_file_opens_with_defaults() {
        // Eski ve yeni biçimler: yeni alanların hiçbiri yok.
        let old = r#"{"lang":"En","freecad_cmd_path":"C:/FreeCAD/bin/FreeCADCmd.exe"}"#;
        let s: AppSettings = serde_json::from_str(old).expect("eski ayar dosyası açılmalı");
        assert_eq!(s.lang, Lang::En);
        assert_eq!(s.ui_scale, 1.0);
        assert!(s.view_grid);
        assert!(!s.view_ortho);
        assert_eq!(s.cad_limits, CadLimits::default());
        assert_eq!(s.theme, ThemeChoice::default());
        // Boş nesne de açılır; bilinmeyen alanlar yok sayılır.
        let s: AppSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(s.ui_scale, 1.0);
        let s: AppSettings = serde_json::from_str(r#"{"gelecek_alan": 5}"#).unwrap();
        assert!(s.view_grid);
        // Yeni alanlar gidiş-dönüşte korunur.
        let mut s = AppSettings::default();
        s.ui_scale = 1.25;
        s.view_ortho = true;
        s.view_grid = false;
        s.cad_limits.part_secs = 600;
        let back: AppSettings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back.ui_scale, 1.25);
        assert!(back.view_ortho && !back.view_grid);
        assert_eq!(back.cad_limits.part_secs, 600);
    }

    /// Her bölüm iki temada, iki dilde, dar ve geniş pencerede başsız çizilir: panik yok ve
    /// pencere ekrandan taşmaz; içerikteki hiçbir öğe pencerenin yatay sınırını aşmaz.
    #[test]
    fn every_section_renders_in_both_themes_without_overflow() {
        for choice in [ThemeChoice::Dark, ThemeChoice::Light] {
            for lang in [Lang::Tr, Lang::En] {
                for size in [vec2(1280.0, 800.0), vec2(700.0, 560.0)] {
                    for section in SettingsSection::ALL {
                        let ctx = egui::Context::default();
                        let mut app = App::new_headless(&ctx, DroneType::FixedWingConventional);
                        theme::install(&ctx, choice);
                        app.shell.theme = choice;
                        app.show_settings_modal = true;
                        set_section(&ctx, section);
                        crate::i18n::with_lang(lang, || {
                            let mut win = None;
                            for _ in 0..3 {
                                win = frame(&mut app, &ctx, size);
                            }
                            let win =
                                win.unwrap_or_else(|| panic!("pencere çizilmedi: {section:?}"));
                            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
                            assert!(
                                screen.expand(1.0).contains_rect(win),
                                "{section:?} {choice:?} {lang:?}: pencere {win:?} ekran {screen:?}"
                            );
                            let widgets: Vec<egui::Rect> = ctx.viewport(|v| {
                                v.prev_pass
                                    .widgets
                                    .layers()
                                    .flat_map(|(_, rs)| rs.iter().map(|w| w.rect))
                                    .collect()
                            });
                            for w in widgets {
                                if w.width() < 400.0
                                    && win.shrink(12.0).contains(w.center())
                                    // Pencerenin 6 px'lik yeniden boyutlandırma tutamaçları kenarda durur.
                                    && w.width() > 8.0
                                    && w.height() > 8.0
                                {
                                    assert!(
                                        w.min.x >= win.min.x - 1.0 && w.max.x <= win.max.x + 1.0,
                                        "{section:?} {choice:?} {lang:?}: öğe {w:?} pencere {win:?}"
                                    );
                                }
                            }
                        });
                    }
                }
            }
        }
    }

    #[test]
    fn window_stays_closed_when_flag_is_off_and_close_button_clears_flag() {
        let ctx = egui::Context::default();
        let mut app = App::new_headless(&ctx, DroneType::Multirotor);
        assert!(frame(&mut app, &ctx, vec2(1000.0, 700.0)).is_none());
        app.show_settings_modal = true;
        assert!(frame(&mut app, &ctx, vec2(1000.0, 700.0)).is_some());
    }

    #[test]
    fn section_selection_is_remembered_and_all_sections_have_labels() {
        let ctx = egui::Context::default();
        assert_eq!(current_section(&ctx), SettingsSection::General);
        set_section(&ctx, SettingsSection::Cad);
        assert_eq!(current_section(&ctx), SettingsSection::Cad);
        for s in SettingsSection::ALL {
            assert!(!s.label().is_empty());
            assert_ne!(s.label(), "set.nav.general");
        }
    }

    #[test]
    fn privacy_page_states_nothing_is_sent_automatically_in_both_languages() {
        for lang in [Lang::Tr, Lang::En] {
            let s = crate::i18n::t_lang("set.privacy.statement", lang);
            assert!(s.len() > 40);
        }
        assert!(
            crate::i18n::t_lang("set.privacy.statement", Lang::En)
                .contains("No data is sent automatically")
        );
    }
}
