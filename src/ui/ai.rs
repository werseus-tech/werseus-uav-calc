// SPDX-License-Identifier: AGPL-3.0-or-later
//! Yapay zeka asistanı arayüz paneli (sağ kenar açılır/kapanır panel, ayarlar modalı, bulut uyarısı).

use crate::ai::config::{AiConfig, AiProviderType};
use crate::ai::credential::{CredentialStorage, mask_api_key};
use crate::ai::prompt::{build_explain_step_prompt, build_quick_design_prompt};
use crate::ai::state::{AiAssistantState, MessageRole};
use crate::calc::Model;
use crate::i18n::{Lang, current_lang, t};
use crate::project::Project;
use crate::ui::components::{self, Status};
use crate::ui::panel_kit::{accent_frame, surface_frame, tinted_frame};
use crate::ui::theme::{self, space};
use std::sync::Arc;

pub struct AiPanelAction {
    pub applied_count: usize,
    pub undone: bool,
}

/// Sağ kenarda yer alan açılır-kapanır AI Asistan Panelini çizer.
pub fn show_ai_panel(
    ui: &mut egui::Ui,
    state: &mut AiAssistantState,
    config: &mut AiConfig,
    project: &mut Project,
    model: &Model,
    step_title: &str,
    cred_store: &Arc<dyn CredentialStorage>,
) -> AiPanelAction {
    let mut action = AiPanelAction {
        applied_count: 0,
        undone: false,
    };

    if !state.is_panel_open {
        return action;
    }

    state.poll();
    let lang = current_lang();

    egui::Panel::right("ai_assistant_panel")
        .default_size(380.0)
        .show(ui, |ui| {
            let tk = theme::tokens(ui);
            // Başlık ve Üst Butonlar
            ui.horizontal(|ui| {
                ui.heading(format!("[AI] {}", t("ai.panel_title")));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("X").on_hover_text(t("ai.btn_dismiss")).clicked() {
                        state.is_panel_open = false;
                    }
                    if ui
                        .button(t("ai.panel.btn_settings"))
                        .on_hover_text(t("menu_ai_settings"))
                        .clicked()
                    {
                        state.show_settings_modal = true;
                    }
                    if ui
                        .button(t("ai.panel.btn_clear"))
                        .on_hover_text(t("ai.btn_clear"))
                        .clicked()
                    {
                        state.clear_chat();
                    }
                });
            });

            // Sağlayıcı Bilgi Etiketi
            ui.horizontal(|ui| {
                let badge_text = if config.enabled {
                    match config.provider {
                        AiProviderType::Ollama => {
                            format!("Ollama: {}", config.ollama_model)
                        }
                        AiProviderType::OpenAiCompatible => {
                            format!("OpenAI: {}", config.openai_model)
                        }
                        AiProviderType::Anthropic => {
                            format!("Anthropic: {}", config.anthropic_model)
                        }
                        AiProviderType::Gemini => {
                            format!("Gemini: {}", config.gemini_model)
                        }
                    }
                } else {
                    t("ai.panel.off").to_string()
                };

                let badge_color = if config.enabled {
                    tk.accent
                } else {
                    tk.text_weak
                };

                ui.colored_label(badge_color, format!("● {}", badge_text));
            });

            ui.separator();

            // AI Kapalı ise Bilgilendirme Kartı
            if !config.enabled {
                surface_frame(tk).show(ui, |ui| {
                    ui.label(t("ai.disabled_banner"));
                    ui.add_space(space::S);
                    if ui.button(t("ai.btn_enable")).clicked() {
                        config.enabled = true;
                        let mut settings = crate::i18n::load_settings();
                        settings.ai_config.enabled = true;
                        crate::i18n::save_settings(&settings);
                    }
                });
                return;
            }

            // Hızlı Eylem Butonları
            ui.horizontal(|ui| {
                if ui
                    .button(format!("• {}", t("ai.btn_explain_step")))
                    .clicked()
                {
                    let prompt = build_explain_step_prompt(project, model, step_title, lang);
                    state.start_request(config, project, prompt, lang, cred_store.clone());
                }
            });

            ui.add_space(space::XS);

            // Durum / Hata Bildirimleri
            if let Some(ref err) = state.error_message {
                components::banner(ui, Status::Err, err);
            }
            if let Some(ref st) = state.status_message {
                components::banner(ui, Status::Ok, st);
            }

            // Aktif Öneri Kartı (Varsa)
            let can_undo = !state.undo_stack.is_empty();
            let mut prop_action = None;
            if let Some(ref mut proposal) = state.active_proposal {
                ui.separator();
                accent_frame(tk).show(ui, |ui| {
                    ui.strong(t("ai.proposal_heading"));
                    ui.weak(&proposal.explanation);
                    ui.add_space(space::S);

                    egui::ScrollArea::vertical()
                        .id_salt("proposal_items_scroll")
                        .max_height(200.0)
                        .show(ui, |ui| {
                            for item in &mut proposal.items {
                                ui.horizontal(|ui| {
                                    if item.is_valid {
                                        ui.checkbox(&mut item.selected, "");
                                        ui.strong(&item.field_label);
                                        ui.label(format!(
                                            "{} → {}",
                                            item.old_value.format_display(&item.unit),
                                            item.new_value.format_display(&item.unit)
                                        ));
                                    } else {
                                        ui.colored_label(tk.danger, "[X]");
                                        ui.strong(&item.field_label);
                                        if let Some(ref err) = item.validation_error {
                                            ui.colored_label(tk.danger, err);
                                        }
                                    }
                                });
                                if !item.reason.is_empty() {
                                    ui.indent(item.field.clone(), |ui| {
                                        ui.weak(format!(
                                            "{}: {}",
                                            t("ai.reason_label"),
                                            item.reason
                                        ));
                                    });
                                }
                            }
                        });

                    ui.add_space(space::S);
                    ui.horizontal_wrapped(|ui| {
                        let has_valid_selected =
                            proposal.items.iter().any(|i| i.is_valid && i.selected);
                        let apply_selected_btn = egui::Button::new(t("ai.btn_apply_selected"));
                        if ui
                            .add_enabled(has_valid_selected, apply_selected_btn)
                            .clicked()
                        {
                            prop_action = Some(0);
                        }

                        let has_any_valid = proposal.items.iter().any(|i| i.is_valid);
                        let apply_all_btn = egui::Button::new(t("ai.btn_apply_all"));
                        if ui.add_enabled(has_any_valid, apply_all_btn).clicked() {
                            prop_action = Some(1);
                        }

                        let undo_btn = egui::Button::new(t("ai.btn_undo"));
                        if ui.add_enabled(can_undo, undo_btn).clicked() {
                            prop_action = Some(2);
                        }

                        if ui.button(t("ai.btn_dismiss")).clicked() {
                            prop_action = Some(3);
                        }
                    });
                });
            }

            match prop_action {
                Some(0) => {
                    if let Ok(n) = state.apply_active_proposal(project, true) {
                        action.applied_count = n;
                    }
                }
                Some(1) => {
                    if let Ok(n) = state.apply_active_proposal(project, false) {
                        action.applied_count = n;
                    }
                }
                Some(2) => {
                    if let Ok(true) = state.undo_last_proposal(project) {
                        action.undone = true;
                    }
                }
                Some(3) => {
                    state.active_proposal = None;
                }
                _ => {}
            }

            ui.separator();

            // Sohbet Geçmişi
            let available_height = ui.available_height() - 90.0;
            egui::ScrollArea::vertical()
                .id_salt("ai_chat_scroll")
                .max_height(available_height.max(120.0))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if state.chat_history.is_empty() {
                        ui.weak(t("ai.input_placeholder"));
                    }
                    for msg in &state.chat_history {
                        let is_user = msg.role == MessageRole::User;
                        let bg_color = if is_user {
                            tk.accent_soft
                        } else {
                            tk.surface_alt
                        };

                        tinted_frame(tk, bg_color).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.strong(if is_user {
                                    t("ai.panel.role_user")
                                } else {
                                    t("ai.panel.role_assistant")
                                });
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.weak(&msg.timestamp);
                                    },
                                );
                            });
                            ui.add_space(space::XS / 2.0);
                            ui.label(&msg.text);
                        });
                        ui.add_space(space::XS);
                    }

                    if state.is_working {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(t("ai.thinking"));
                            if ui.button(t("ai.btn_cancel")).clicked() {
                                state.cancel_request();
                            }
                        });
                    }
                });

            ui.separator();

            // Alt Doğal Dil Girdi Alanı
            ui.horizontal(|ui| {
                let send_shortcut =
                    ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.ctrl);
                let text_edit = egui::TextEdit::multiline(&mut state.input_text)
                    .hint_text(t("ai.input_placeholder"))
                    .desired_rows(2)
                    .desired_width(ui.available_width() - 70.0);
                ui.add(text_edit);

                let can_send = !state.is_working && !state.input_text.trim().is_empty();
                let send_btn = egui::Button::new(t("ai.btn_send"));
                if ui.add_enabled(can_send, send_btn).clicked() || (send_shortcut && can_send) {
                    let prompt_text = std::mem::take(&mut state.input_text);
                    let prompt = build_quick_design_prompt(&prompt_text, lang);
                    state.start_request(config, project, prompt, lang, cred_store.clone());
                }
            });
        });

    action
}

/// Yapay zekâ ayarlarını diske yazar (API anahtarı hariç; anahtar yalnız kimlik deposundadır).
pub fn save_ai_config(config: &AiConfig, cred_store: &Arc<dyn CredentialStorage>) {
    cred_store.set_store_in_wincred(config.store_in_wincred);
    let mut settings = crate::i18n::load_settings();
    settings.ai_config = config.clone();
    crate::i18n::save_settings(&settings);
}

/// Yapay zekâ ayar formu (ayar penceresi ve Ayarlar sayfası ortak kullanır): sağlayıcı, adres,
/// model, maskeli API anahtarı alanı, anahtar saklama kipi.
pub fn ai_settings_form(
    ui: &mut egui::Ui,
    state: &mut AiAssistantState,
    config: &mut AiConfig,
    cred_store: &Arc<dyn CredentialStorage>,
) {
    {
        {
            ui.checkbox(&mut config.enabled, t("ai.enable_checkbox"));
            ui.separator();

            ui.horizontal(|ui| {
                ui.label(t("ai.provider_label"));
                egui::ComboBox::from_id_salt("ai_provider_combo")
                    .selected_text(config.provider.label())
                    .show_ui(ui, |ui| {
                        for p in AiProviderType::ALL {
                            ui.selectable_value(&mut config.provider, p, p.label());
                        }
                    });
            });

            ui.add_space(space::S);

            match config.provider {
                AiProviderType::Ollama => {
                    ui.horizontal(|ui| {
                        ui.label(t("ai.url_label"));
                        ui.add(
                            egui::TextEdit::singleline(&mut config.ollama_url).desired_width(320.0),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label(t("ai.model_label"));
                        ui.add(
                            egui::TextEdit::singleline(&mut config.ollama_model)
                                .desired_width(200.0),
                        );
                        if ui.button(t("ai.btn_fetch_models")).clicked() {
                            state.fetch_models(config, cred_store.clone());
                        }
                    });

                    if !state.available_models.is_empty() {
                        ui.horizontal(|ui| {
                            ui.label(t("ai.panel.detected_models"));
                            for m in &state.available_models {
                                if ui.small_button(m).clicked() {
                                    config.ollama_model = m.clone();
                                }
                            }
                        });
                    }
                }
                AiProviderType::OpenAiCompatible => {
                    ui.horizontal(|ui| {
                        ui.label(t("ai.url_label"));
                        ui.add(
                            egui::TextEdit::singleline(&mut config.openai_url).desired_width(320.0),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label(t("ai.model_label"));
                        ui.add(
                            egui::TextEdit::singleline(&mut config.openai_model)
                                .desired_width(200.0),
                        );
                    });

                    render_api_key_field(ui, "WerSeuS_AI_ApiKey_openai", cred_store);
                }
                AiProviderType::Anthropic => {
                    ui.horizontal(|ui| {
                        ui.label(t("ai.model_label"));
                        ui.add(
                            egui::TextEdit::singleline(&mut config.anthropic_model)
                                .desired_width(240.0),
                        );
                    });
                    render_api_key_field(ui, "WerSeuS_AI_ApiKey_anthropic", cred_store);
                }
                AiProviderType::Gemini => {
                    ui.horizontal(|ui| {
                        ui.label(t("ai.model_label"));
                        ui.add(
                            egui::TextEdit::singleline(&mut config.gemini_model)
                                .desired_width(240.0),
                        );
                    });
                    render_api_key_field(ui, "WerSeuS_AI_ApiKey_gemini", cred_store);
                }
            }

            ui.add_space(space::S);
            ui.separator();

            // Anahtar Saklama Seçimi
            ui.label(t("ai.storage_mode"));
            let old_store_in_wincred = config.store_in_wincred;
            ui.radio_value(&mut config.store_in_wincred, false, t("ai.storage_session"));
            ui.radio_value(&mut config.store_in_wincred, true, t("ai.storage_wincred"));
            if old_store_in_wincred != config.store_in_wincred {
                cred_store.set_store_in_wincred(config.store_in_wincred);
            }

            ui.add_space(space::XS);
            ui.colored_label(theme::tokens(ui).success, t("ai.key_security_notice"));
        }
    }
}

fn render_api_key_field(ui: &mut egui::Ui, target: &str, cred_store: &Arc<dyn CredentialStorage>) {
    let current_key = cred_store.get(target).unwrap_or_default();
    let mut key_buffer = current_key.clone();

    ui.horizontal(|ui| {
        ui.label(t("ai.api_key_label"));
        let edit = egui::TextEdit::singleline(&mut key_buffer)
            .password(true)
            .desired_width(260.0);
        let resp = ui.add(edit);
        if resp.changed() {
            let _ = cred_store.set(target, &key_buffer);
        }
        if !current_key.is_empty() {
            ui.weak(format!("({})", mask_api_key(&current_key)));
            if ui.small_button(t("ai.panel.btn_forget_key")).clicked() {
                let _ = cred_store.delete(target);
            }
        }
    });
}

/// Bulut sağlayıcı veri aktarımı ve maliyet onay penceresini (Modal) çizer.
pub fn show_cloud_warning_dialog(
    ctx: &egui::Context,
    state: &mut AiAssistantState,
    config: &mut AiConfig,
    project: &Project,
    lang: Lang,
    cred_store: &Arc<dyn CredentialStorage>,
) {
    if !state.show_cloud_warning_modal {
        return;
    }

    egui::Window::new(t("ai.cloud_warning_title"))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .default_width(520.0)
        .show(ctx, |ui| {
            ui.colored_label(
                theme::tokens(ui).warning,
                format!("[!] {}", t("ai.panel.cloud_attention")),
            );
            ui.add_space(space::XS);
            ui.label(t("ai.cloud_warning_text"));
            ui.add_space(space::S);
            ui.separator();

            ui.horizontal(|ui| {
                if ui.button(t("ai.btn_accept_cloud")).clicked() {
                    config.cloud_disclaimer_accepted = true;
                    let mut settings = crate::i18n::load_settings();
                    settings.ai_config.cloud_disclaimer_accepted = true;
                    crate::i18n::save_settings(&settings);
                    state.show_cloud_warning_modal = false;

                    if let Some(prompt) = state.pending_prompt.take() {
                        state.start_request(config, project, prompt, lang, cred_store.clone());
                    }
                }

                if ui.button(t("btn_cancel")).clicked() {
                    state.pending_prompt = None;
                    state.show_cloud_warning_modal = false;
                }
            });
        });
}
