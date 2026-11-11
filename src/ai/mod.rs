// SPDX-License-Identifier: AGPL-3.0-or-later
//! Yapay zeka asistanı modülü (WerSeuS AI Assistant).
//!
//! İsteğe bağlı olarak yerel Ollama, yerel OpenAI uyumlu sunucular veya bulut sağlayıcıları
//! (Anthropic, Gemini) üzerinden İHA boyutlandırma, kural açıklaması ve uyarı düzeltme önerileri sunar.

pub mod client;
pub mod config;
pub mod credential;
pub mod fields;
pub mod prompt;
pub mod provider;
pub mod state;

#[cfg(test)]
mod i18n_tests;

pub use config::{AiConfig, AiProviderType};
pub use credential::{
    CredentialStorage, MockCredentialStore, SessionCredentialStore, WinCredStore, mask_api_key,
};
pub use fields::{AI_FIELDS, AiFieldDef, AiValue, find_field};
pub use prompt::{
    build_explain_step_prompt, build_fix_warning_prompt, build_quick_design_prompt,
    build_system_prompt,
};
pub use provider::{AiError, AiProvider, OllamaProvider, get_provider};
pub use state::AiAssistantState;
