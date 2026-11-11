// SPDX-License-Identifier: AGPL-3.0-or-later
//! Yapay zeka asistanı yapılandırma ve sağlayıcı türleri.

use serde::{Deserialize, Serialize};

/// Desteklenen yapay zeka sağlayıcı türleri.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AiProviderType {
    #[default]
    Ollama,
    OpenAiCompatible,
    Anthropic,
    Gemini,
}

impl AiProviderType {
    pub const ALL: [AiProviderType; 4] = [
        AiProviderType::Ollama,
        AiProviderType::OpenAiCompatible,
        AiProviderType::Anthropic,
        AiProviderType::Gemini,
    ];

    pub fn id(&self) -> &'static str {
        match self {
            AiProviderType::Ollama => "ollama",
            AiProviderType::OpenAiCompatible => "openai_compatible",
            AiProviderType::Anthropic => "anthropic",
            AiProviderType::Gemini => "gemini",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            AiProviderType::Ollama => crate::i18n::t("ai.provider.ollama"),
            AiProviderType::OpenAiCompatible => crate::i18n::t("ai.provider.openai"),
            AiProviderType::Anthropic => crate::i18n::t("ai.provider.anthropic"),
            AiProviderType::Gemini => crate::i18n::t("ai.provider.gemini"),
        }
    }

    pub fn is_cloud(&self) -> bool {
        matches!(self, AiProviderType::Anthropic | AiProviderType::Gemini)
    }
}

/// Asistan yapılandırması (diske settings.json içine kaydedilebilir, API anahtarı içermez).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AiConfig {
    /// Yapay zeka asistanı açık mı? (Varsayılan: false, kapalıyken hiçbir ağ isteği yapılmaz)
    pub enabled: bool,
    /// Seçili sağlayıcı türü
    pub provider: AiProviderType,
    /// Ollama taban URL'i
    pub ollama_url: String,
    /// Ollama model adı
    pub ollama_model: String,
    /// OpenAI uyumlu uç nokta taban URL'i
    pub openai_url: String,
    /// OpenAI uyumlu model adı
    pub openai_model: String,
    /// Anthropic uç nokta URL'i
    pub anthropic_url: String,
    /// Anthropic model adı
    pub anthropic_model: String,
    /// Gemini uç nokta URL şablonu
    pub gemini_url: String,
    /// Gemini model adı
    pub gemini_model: String,
    /// İstek zaman aşımı süresi (saniye, varsayılan 60)
    pub timeout_secs: u64,
    /// Anahtar Windows Kimlik Bilgisi Yöneticisi'nde saklansın mı? (false ise yalnızca oturum belleğinde)
    pub store_in_wincred: bool,
    /// Bulut sağlayıcı veri aktarımı ve maliyet uyarısı onaylandı mı?
    pub cloud_disclaimer_accepted: bool,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: AiProviderType::Ollama,
            ollama_url: "http://127.0.0.1:11434".into(),
            ollama_model: "qwen3:8b".into(),
            openai_url: "http://localhost:1234/v1".into(),
            openai_model: "local-model".into(),
            anthropic_url: "https://api.anthropic.com/v1/messages".into(),
            anthropic_model: "claude-sonnet-5-5".into(),
            gemini_url:
                "https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent"
                    .into(),
            gemini_model: "gemini-3.8-flash".into(),
            timeout_secs: 60,
            store_in_wincred: false,
            cloud_disclaimer_accepted: false,
        }
    }
}
