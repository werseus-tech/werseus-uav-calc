// SPDX-License-Identifier: AGPL-3.0-or-later
//! Asistan arayüz durumu (sohbet geçmişi, arka plan istek iş parçacığı, öneri durumu).

use crate::ai::client::{
    AiProposal, AiUndoRecord, apply_proposal, parse_ai_response, undo_proposal,
};
use crate::ai::config::AiConfig;
use crate::ai::credential::CredentialStorage;
use crate::ai::prompt::build_system_prompt;
use crate::ai::provider::{AiProvider, get_provider};
use crate::i18n::{Lang, t, tf};
use crate::project::Project;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};

#[derive(Clone, Debug, PartialEq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

#[derive(Clone, Debug)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub text: String,
    pub timestamp: String,
}

pub struct AiAssistantState {
    pub is_panel_open: bool,
    pub input_text: String,
    pub chat_history: Vec<ChatMessage>,
    pub active_proposal: Option<AiProposal>,
    pub undo_stack: Vec<Vec<AiUndoRecord>>,
    pub is_working: bool,
    pub status_message: Option<String>,
    pub error_message: Option<String>,
    pub cancel_flag: Option<Arc<AtomicBool>>,
    pub proposal_rx: Option<Receiver<Result<AiProposal, String>>>,
    pub show_settings_modal: bool,
    pub show_cloud_warning_modal: bool,
    pub pending_prompt: Option<String>,
    pub available_models: Vec<String>,
    pub is_fetching_models: bool,
    pub models_rx: Option<Receiver<Result<Vec<String>, String>>>,
}

impl Default for AiAssistantState {
    fn default() -> Self {
        Self::new()
    }
}

impl AiAssistantState {
    pub fn new() -> Self {
        Self {
            is_panel_open: false,
            input_text: String::new(),
            chat_history: Vec::new(),
            active_proposal: None,
            undo_stack: Vec::new(),
            is_working: false,
            status_message: None,
            error_message: None,
            cancel_flag: None,
            proposal_rx: None,
            show_settings_modal: false,
            show_cloud_warning_modal: false,
            pending_prompt: None,
            available_models: Vec::new(),
            is_fetching_models: false,
            models_rx: None,
        }
    }

    pub fn clear_chat(&mut self) {
        self.chat_history.clear();
        self.active_proposal = None;
        self.error_message = None;
        self.status_message = None;
    }

    pub fn cancel_request(&mut self) {
        if let Some(ref flag) = self.cancel_flag {
            flag.store(true, Ordering::SeqCst);
        }
        self.is_working = false;
        self.proposal_rx = None;
        self.cancel_flag = None;
        self.status_message = Some(t("ai.msg.cancelled").into());
    }

    /// Yeni bir sohbet veya tasarım isteğini arka plan iş parçacığında başlatır.
    pub fn start_request(
        &mut self,
        config: &AiConfig,
        project: &Project,
        user_prompt: String,
        lang: Lang,
        cred_store: Arc<dyn CredentialStorage>,
    ) {
        if !config.enabled {
            self.error_message = Some(t("ai.msg.disabled").into());
            return;
        }

        // Bulut sağlayıcı uyarısı kontrolü
        if config.provider.is_cloud() && !config.cloud_disclaimer_accepted {
            self.pending_prompt = Some(user_prompt);
            self.show_cloud_warning_modal = true;
            return;
        }

        let now = chrono_now_time_str();
        self.chat_history.push(ChatMessage {
            role: MessageRole::User,
            text: user_prompt.clone(),
            timestamp: now,
        });

        self.is_working = true;
        self.error_message = None;
        self.status_message = Some(t("ai.msg.thinking").into());

        let cancel_flag = Arc::new(AtomicBool::new(false));
        self.cancel_flag = Some(cancel_flag.clone());

        let (tx, rx) = channel();
        self.proposal_rx = Some(rx);

        let system_prompt = build_system_prompt(project, lang);
        let config_clone = config.clone();
        let project_clone = project.clone();
        let provider = get_provider(config.provider);

        std::thread::spawn(move || {
            if cancel_flag.load(Ordering::SeqCst) {
                return;
            }

            let send_res = provider.send_chat(
                &config_clone,
                &system_prompt,
                &user_prompt,
                cred_store.as_ref(),
            );

            if cancel_flag.load(Ordering::SeqCst) {
                return;
            }

            let result = match send_res {
                Ok(raw_reply) => parse_ai_response(&raw_reply, &project_clone),
                Err(e) => Err(e.to_string()),
            };

            let _ = tx.send(result);
        });
    }

    /// Sokratik Öğretmen Modu için özel sistem istemiyle arka plan AI isteği başlatır.
    /// Değer değiştirmez (changes daima boştur).
    pub fn start_socratic_teacher_request(
        &mut self,
        config: &AiConfig,
        project: &Project,
        system_prompt: String,
        user_prompt: String,
        cred_store: Arc<dyn CredentialStorage>,
    ) {
        self.start_socratic_teacher_request_with(
            get_provider(config.provider),
            config,
            project,
            system_prompt,
            user_prompt,
            cred_store,
        );
    }

    /// Sağlayıcı dışarıdan verilebilen sürüm (testte sahte istemci kullanılır).
    pub fn start_socratic_teacher_request_with(
        &mut self,
        provider: Box<dyn AiProvider>,
        config: &AiConfig,
        project: &Project,
        system_prompt: String,
        user_prompt: String,
        cred_store: Arc<dyn CredentialStorage>,
    ) {
        if !config.enabled {
            self.error_message = Some(t("ai.msg.disabled").into());
            return;
        }

        if config.provider.is_cloud() && !config.cloud_disclaimer_accepted {
            // Bekleyen istem normal (değer önerebilen) yoldan gönderileceği için kullanılmaz.
            self.error_message = Some(t("ai.msg.cloud_accept_first").into());
            return;
        }

        let now = chrono_now_time_str();
        self.chat_history.push(ChatMessage {
            role: MessageRole::User,
            text: user_prompt.clone(),
            timestamp: now,
        });

        self.is_working = true;
        self.error_message = None;
        self.status_message = Some(t("ai.msg.teacher_waiting").into());

        let cancel_flag = Arc::new(AtomicBool::new(false));
        self.cancel_flag = Some(cancel_flag.clone());

        let (tx, rx) = channel();
        self.proposal_rx = Some(rx);

        let config_clone = config.clone();
        let project_clone = project.clone();

        std::thread::spawn(move || {
            if cancel_flag.load(Ordering::SeqCst) {
                return;
            }

            let send_res = provider.send_chat(
                &config_clone,
                &system_prompt,
                &user_prompt,
                cred_store.as_ref(),
            );

            if cancel_flag.load(Ordering::SeqCst) {
                return;
            }

            let result = match send_res {
                Ok(raw_reply) => Ok(teacher_reply_to_proposal(&raw_reply, &project_clone)),
                Err(e) => Err(e.to_string()),
            };

            let _ = tx.send(result);
        });
    }

    /// Model listesini arka planda sorgular.
    pub fn fetch_models(&mut self, config: &AiConfig, cred_store: Arc<dyn CredentialStorage>) {
        self.is_fetching_models = true;
        let (tx, rx) = channel();
        self.models_rx = Some(rx);

        let config_clone = config.clone();
        let provider = get_provider(config.provider);

        std::thread::spawn(move || {
            let res = provider
                .fetch_models(&config_clone, cred_store.as_ref())
                .map_err(|e| e.to_string());
            let _ = tx.send(res);
        });
    }

    /// Her eframe çizim döngüsünde arka plan kanallarını yoklar.
    pub fn poll(&mut self) {
        if let Some(ref rx) = self.proposal_rx {
            if let Ok(result) = rx.try_recv() {
                self.is_working = false;
                self.proposal_rx = None;
                self.cancel_flag = None;

                match result {
                    Ok(proposal) => {
                        let now = chrono_now_time_str();
                        self.chat_history.push(ChatMessage {
                            role: MessageRole::Assistant,
                            text: proposal.explanation.clone(),
                            timestamp: now,
                        });
                        self.active_proposal = Some(proposal);
                        self.status_message = Some(t("ai.msg.proposals_ready").into());
                    }
                    Err(e) => {
                        self.error_message = Some(e);
                        self.status_message = None;
                    }
                }
            }
        }

        if let Some(ref rx) = self.models_rx {
            if let Ok(res) = rx.try_recv() {
                self.is_fetching_models = false;
                self.models_rx = None;
                match res {
                    Ok(models) => {
                        self.available_models = models;
                    }
                    Err(e) => {
                        self.error_message = Some(tf("ai.msg.models_failed_fmt", &[e]));
                    }
                }
            }
        }
    }

    /// Aktif öneriyi projeye uygular ve eski değerleri geri alma yığınına ekler.
    pub fn apply_active_proposal(
        &mut self,
        project: &mut Project,
        selected_only: bool,
    ) -> Result<usize, String> {
        let Some(ref proposal) = self.active_proposal else {
            return Ok(0);
        };

        let records = apply_proposal(project, proposal, selected_only)?;
        let count = records.len();
        if count > 0 {
            self.undo_stack.push(records);
            self.status_message = Some(tf("ai.msg.applied_fmt", &[count.to_string()]));
            self.active_proposal = None;
        }
        Ok(count)
    }

    /// En son uygulanan öneriyi geri alır.
    pub fn undo_last_proposal(&mut self, project: &mut Project) -> Result<bool, String> {
        let Some(records) = self.undo_stack.pop() else {
            return Ok(false);
        };
        undo_proposal(project, &records)?;
        self.status_message = Some(tf("ai.msg.undone_fmt", &[records.len().to_string()]));
        Ok(true)
    }
}

/// Öğretmen yanıtını öneriye çevirir. Öğretmen modu değer değiştirmez:
/// model yanıtında `changes` olsa bile tüm öğeler atılır; JSON yoksa ham metin açıklama olur.
pub fn teacher_reply_to_proposal(raw_reply: &str, project: &Project) -> AiProposal {
    match parse_ai_response(raw_reply, project) {
        Ok(mut proposal) => {
            proposal.items.clear();
            proposal
        }
        Err(_) => AiProposal {
            explanation: raw_reply.trim().to_string(),
            items: Vec::new(),
        },
    }
}

fn chrono_now_time_str() -> String {
    let now = std::time::SystemTime::now();
    let duration = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    let hours = (secs / 3600) % 24;
    let mins = (secs / 60) % 60;
    let s = secs % 60;
    format!("{:02}:{:02}:{:02}", hours, mins, s)
}

#[cfg(test)]
mod teacher_tests {
    use super::*;
    use crate::ai::credential::SessionCredentialStore;
    use crate::ai::provider::MockAiProvider;
    use crate::calc::{Library, evaluate};
    use crate::teach::{ResultMetric, StudentLevel, build_socratic_teacher_prompts};

    #[test]
    fn socratic_request_with_fake_client_drops_changes_and_keeps_project() {
        let p = Project::default();
        let before = serde_json::to_string(&p).unwrap();
        let m = evaluate(&p, &Library::embedded());
        let (sys, user) = build_socratic_teacher_prompts(
            ResultMetric::VStall,
            &p,
            &m,
            StudentLevel::Beginner,
            Lang::Tr,
        );
        // Sahte model değer değiştirmeye çalışsa bile öğretmen yolu bunu atmalı.
        let canned = r#"{"explanation": "Sence S artarsa ne olur?", "changes": [{"field": "b", "value": 9.9, "reason": "x"}]}"#;
        let mock = MockAiProvider::new(Some(canned.into()));
        let config = AiConfig {
            enabled: true,
            ..AiConfig::default()
        };
        let mut st = AiAssistantState::new();
        st.start_socratic_teacher_request_with(
            Box::new(mock),
            &config,
            &p,
            sys,
            user,
            Arc::new(SessionCredentialStore::new()),
        );
        for _ in 0..200 {
            st.poll();
            if !st.is_working {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let prop = st.active_proposal.expect("öneri gelmeli");
        assert!(
            prop.items.is_empty(),
            "öğretmen yolu değer önerisi taşımamalı"
        );
        assert!(prop.explanation.contains("Sence"));
        assert_eq!(before, serde_json::to_string(&p).unwrap());
    }

    #[test]
    fn teacher_reply_plain_text_becomes_explanation() {
        let p = Project::default();
        let prop = teacher_reply_to_proposal("Sence ne olur?", &p);
        assert!(prop.items.is_empty());
        assert_eq!(prop.explanation, "Sence ne olur?");
    }

    #[test]
    fn teacher_disabled_ai_makes_no_request() {
        let p = Project::default();
        let config = AiConfig {
            enabled: false,
            ..AiConfig::default()
        };
        let mut st = AiAssistantState::new();
        st.start_socratic_teacher_request_with(
            Box::new(MockAiProvider::new(None)),
            &config,
            &p,
            "s".into(),
            "u".into(),
            Arc::new(SessionCredentialStore::new()),
        );
        assert!(!st.is_working);
        assert!(st.error_message.is_some());
    }
}
