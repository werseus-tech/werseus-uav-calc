// SPDX-License-Identifier: AGPL-3.0-or-later
//! Yapay zeka sağlayıcıları için ortak trait ve uygulamalar (Ollama, OpenAI uyumlu, Anthropic, Gemini, Mock).

use crate::ai::config::AiConfig;
use crate::ai::credential::CredentialStorage;
use crate::i18n::{t, tf};
use serde_json::json;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiError {
    Network(String),
    Http(u16, String),
    Parse(String),
    Timeout(u64),
    MissingApiKey(String),
    TlsDisabled(String),
    Cancelled,
}

impl std::fmt::Display for AiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AiError::Network(s) => write!(f, "{}", tf("ai.err.network_fmt", &[s.as_str()])),
            AiError::Http(code, s) => {
                write!(
                    f,
                    "{}",
                    tf("ai.err.http_fmt", &[code.to_string(), s.clone()])
                )
            }
            AiError::Parse(s) => write!(f, "{}", tf("ai.err.parse_fmt", &[s.as_str()])),
            AiError::Timeout(secs) => {
                write!(f, "{}", tf("ai.err.timeout_fmt", &[secs.to_string()]))
            }
            AiError::MissingApiKey(p) => {
                write!(f, "{}", tf("ai.err.missing_key_fmt", &[p.as_str()]))
            }
            AiError::TlsDisabled(s) => write!(f, "{}", s),
            AiError::Cancelled => write!(f, "{}", t("ai.err.request_cancelled")),
        }
    }
}

impl std::error::Error for AiError {}

/// URL'nin yerel bir loopback / yerel makine adresi olup olmadığını denetler.
pub fn is_loopback_host(url: &str) -> bool {
    let lower = url.to_lowercase();
    let host_part = if let Some(stripped) = lower.strip_prefix("http://") {
        stripped
    } else if let Some(stripped) = lower.strip_prefix("https://") {
        stripped
    } else {
        &lower
    };

    let authority = host_part.split('/').next().unwrap_or("");
    let host = if authority.starts_with('[') {
        if let Some(end_bracket) = authority.find(']') {
            &authority[1..end_bracket]
        } else {
            authority.trim_matches(|c| c == '[' || c == ']')
        }
    } else {
        authority.split(':').next().unwrap_or("")
    };

    host == "localhost" || host == "127.0.0.1" || host == "::1" || host == "0.0.0.0"
}

/// Uzak sunuculara düz metin HTTP üzerinden API anahtarı gönderimini engeller.
pub fn check_plaintext_api_key_guard(url: &str, api_key: &str) -> Result<(), AiError> {
    let trimmed_key = api_key.trim();
    if trimmed_key.is_empty() {
        return Ok(());
    }

    let lower = url.to_lowercase();
    if lower.starts_with("http://") && !is_loopback_host(url) {
        return Err(AiError::Network(t("ai.err.plaintext_key").into()));
    }

    Ok(())
}

/// Tüm AI sağlayıcıları tarafından uygulanan ortak trait.
pub trait AiProvider: Send + Sync {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn send_chat(
        &self,
        config: &AiConfig,
        system_prompt: &str,
        user_prompt: &str,
        cred_store: &dyn CredentialStorage,
    ) -> Result<String, AiError>;
    fn fetch_models(
        &self,
        config: &AiConfig,
        cred_store: &dyn CredentialStorage,
    ) -> Result<Vec<String>, AiError>;
}

/// Ortak HTTP istemcisi: HTTPS için işletim sisteminin TLS yığını (Windows'ta SChannel, native-tls)
/// kullanılır; C derleyicisi gerekmez. TLS başlatılamazsa `Err` döner ve HTTPS istekleri açık
/// hata mesajıyla reddedilir (HTTP/loopback istekleri etkilenmez).
fn tls_agent() -> &'static Result<ureq::Agent, String> {
    static AGENT: std::sync::OnceLock<Result<ureq::Agent, String>> = std::sync::OnceLock::new();
    AGENT.get_or_init(|| {
        native_tls::TlsConnector::new()
            .map(|c| {
                ureq::AgentBuilder::new()
                    .tls_connector(std::sync::Arc::new(c))
                    .build()
            })
            .map_err(|e| e.to_string())
    })
}

fn http_agent() -> ureq::Agent {
    match tls_agent() {
        Ok(a) => a.clone(),
        Err(_) => ureq::AgentBuilder::new().build(),
    }
}

/// HTTPS uç noktası için TLS yığınının kullanılabilir olduğunu doğrular.
fn check_https_supported(url: &str) -> Result<(), AiError> {
    if url.to_lowercase().starts_with("https://") {
        if let Err(e) = tls_agent() {
            return Err(AiError::TlsDisabled(tf(
                "ai.err.tls_fmt",
                std::slice::from_ref(&e),
            )));
        }
    }
    Ok(())
}

/// Ollama yerel sağlayıcısı.
pub struct OllamaProvider;

impl AiProvider for OllamaProvider {
    fn id(&self) -> &'static str {
        "ollama"
    }

    fn display_name(&self) -> &'static str {
        "Ollama (Yerel)"
    }

    fn send_chat(
        &self,
        config: &AiConfig,
        system_prompt: &str,
        user_prompt: &str,
        _cred_store: &dyn CredentialStorage,
    ) -> Result<String, AiError> {
        let endpoint = format!("{}/api/chat", config.ollama_url.trim_end_matches('/'));
        check_https_supported(&endpoint)?;

        let body = json!({
            "model": config.ollama_model,
            "stream": false,
            "format": "json",
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_prompt}
            ]
        });

        let resp = http_agent()
            .post(&endpoint)
            .timeout(Duration::from_secs(config.timeout_secs))
            .send_json(body)
            .map_err(|e| match e {
                ureq::Error::Status(code, r) => {
                    let msg = r.into_string().unwrap_or_default();
                    AiError::Http(code, msg)
                }
                ureq::Error::Transport(t) => {
                    if t.kind() == ureq::ErrorKind::Io {
                        AiError::Timeout(config.timeout_secs)
                    } else {
                        AiError::Network(t.to_string())
                    }
                }
            })?;

        let parsed: serde_json::Value = resp
            .into_json()
            .map_err(|e| AiError::Parse(e.to_string()))?;

        if let Some(content) = parsed
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
        {
            Ok(content.to_string())
        } else {
            Err(AiError::Parse(tf(
                "ai.err.no_content_fmt",
                &["Ollama", "message.content"],
            )))
        }
    }

    fn fetch_models(
        &self,
        config: &AiConfig,
        _cred_store: &dyn CredentialStorage,
    ) -> Result<Vec<String>, AiError> {
        let endpoint = format!("{}/api/tags", config.ollama_url.trim_end_matches('/'));
        check_https_supported(&endpoint)?;

        let resp = http_agent()
            .get(&endpoint)
            .timeout(Duration::from_secs(5))
            .call()
            .map_err(|e| AiError::Network(e.to_string()))?;

        let parsed: serde_json::Value = resp
            .into_json()
            .map_err(|e| AiError::Parse(e.to_string()))?;

        let mut models = Vec::new();
        if let Some(arr) = parsed.get("models").and_then(|m| m.as_array()) {
            for item in arr {
                if let Some(name) = item.get("name").and_then(|n| n.as_str()) {
                    models.push(name.to_string());
                }
            }
        }
        Ok(models)
    }
}

/// OpenAI uyumlu uç nokta (LM Studio, LocalAI, vLLM, OpenRouter vb.).
pub struct OpenAiCompatibleProvider;

impl AiProvider for OpenAiCompatibleProvider {
    fn id(&self) -> &'static str {
        "openai_compatible"
    }

    fn display_name(&self) -> &'static str {
        "OpenAI Uyumlu"
    }

    fn send_chat(
        &self,
        config: &AiConfig,
        system_prompt: &str,
        user_prompt: &str,
        cred_store: &dyn CredentialStorage,
    ) -> Result<String, AiError> {
        let endpoint = format!(
            "{}/chat/completions",
            config.openai_url.trim_end_matches('/')
        );
        check_https_supported(&endpoint)?;

        let api_key = cred_store
            .get("WerSeuS_AI_ApiKey_openai")
            .unwrap_or_default();

        check_plaintext_api_key_guard(&endpoint, &api_key)?;

        let mut req = http_agent()
            .post(&endpoint)
            .timeout(Duration::from_secs(config.timeout_secs));
        if !api_key.trim().is_empty() {
            req = req.set("Authorization", &format!("Bearer {}", api_key.trim()));
        }

        let body = json!({
            "model": config.openai_model,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_prompt}
            ]
        });

        let resp = req.send_json(body).map_err(|e| match e {
            ureq::Error::Status(code, r) => {
                let msg = r.into_string().unwrap_or_default();
                AiError::Http(code, msg)
            }
            ureq::Error::Transport(t) => {
                if t.kind() == ureq::ErrorKind::Io {
                    AiError::Timeout(config.timeout_secs)
                } else {
                    AiError::Network(t.to_string())
                }
            }
        })?;

        let parsed: serde_json::Value = resp
            .into_json()
            .map_err(|e| AiError::Parse(e.to_string()))?;

        if let Some(content) = parsed
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
        {
            Ok(content.to_string())
        } else {
            Err(AiError::Parse(tf(
                "ai.err.no_content_fmt",
                &["OpenAI", "choices[0].message.content"],
            )))
        }
    }

    fn fetch_models(
        &self,
        config: &AiConfig,
        cred_store: &dyn CredentialStorage,
    ) -> Result<Vec<String>, AiError> {
        let endpoint = format!("{}/models", config.openai_url.trim_end_matches('/'));
        check_https_supported(&endpoint)?;

        let api_key = cred_store
            .get("WerSeuS_AI_ApiKey_openai")
            .unwrap_or_default();

        check_plaintext_api_key_guard(&endpoint, &api_key)?;

        let mut req = http_agent().get(&endpoint).timeout(Duration::from_secs(5));
        if !api_key.trim().is_empty() {
            req = req.set("Authorization", &format!("Bearer {}", api_key.trim()));
        }

        let resp = req.call().map_err(|e| match e {
            ureq::Error::Status(code, r) => {
                let msg = r.into_string().unwrap_or_default();
                AiError::Http(code, msg)
            }
            ureq::Error::Transport(t) => {
                if t.kind() == ureq::ErrorKind::Io {
                    AiError::Timeout(5)
                } else {
                    AiError::Network(t.to_string())
                }
            }
        })?;
        let parsed: serde_json::Value = resp
            .into_json()
            .map_err(|e| AiError::Parse(e.to_string()))?;

        let mut models = Vec::new();
        if let Some(arr) = parsed.get("data").and_then(|d| d.as_array()) {
            for item in arr {
                if let Some(id) = item.get("id").and_then(|i| i.as_str()) {
                    models.push(id.to_string());
                }
            }
        }
        Ok(models)
    }
}

/// Anthropic (Claude) sağlayıcısı.
pub struct AnthropicProvider;

impl AiProvider for AnthropicProvider {
    fn id(&self) -> &'static str {
        "anthropic"
    }

    fn display_name(&self) -> &'static str {
        "Anthropic (Claude)"
    }

    fn send_chat(
        &self,
        config: &AiConfig,
        system_prompt: &str,
        user_prompt: &str,
        cred_store: &dyn CredentialStorage,
    ) -> Result<String, AiError> {
        let endpoint = &config.anthropic_url;
        check_https_supported(endpoint)?;

        let api_key = cred_store
            .get("WerSeuS_AI_ApiKey_anthropic")
            .filter(|k| !k.trim().is_empty())
            .ok_or_else(|| AiError::MissingApiKey("Anthropic".into()))?;

        check_plaintext_api_key_guard(endpoint, &api_key)?;

        let body = json!({
            "model": config.anthropic_model,
            "max_tokens": 4096,
            "system": system_prompt,
            "messages": [
                {"role": "user", "content": user_prompt}
            ]
        });

        let resp = http_agent()
            .post(endpoint)
            .set("x-api-key", &api_key)
            .set("anthropic-version", "2023-06-01")
            .set("content-type", "application/json")
            .timeout(Duration::from_secs(config.timeout_secs))
            .send_json(body)
            .map_err(|e| match e {
                ureq::Error::Status(code, r) => {
                    let msg = r.into_string().unwrap_or_default();
                    AiError::Http(code, msg)
                }
                ureq::Error::Transport(t) => {
                    if t.kind() == ureq::ErrorKind::Io {
                        AiError::Timeout(config.timeout_secs)
                    } else {
                        AiError::Network(t.to_string())
                    }
                }
            })?;

        let parsed: serde_json::Value = resp
            .into_json()
            .map_err(|e| AiError::Parse(e.to_string()))?;

        if let Some(content) = parsed
            .get("content")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|block| block.get("text"))
            .and_then(|t| t.as_str())
        {
            Ok(content.to_string())
        } else {
            Err(AiError::Parse(tf(
                "ai.err.no_content_fmt",
                &["Anthropic", "content[0].text"],
            )))
        }
    }

    fn fetch_models(
        &self,
        config: &AiConfig,
        _cred_store: &dyn CredentialStorage,
    ) -> Result<Vec<String>, AiError> {
        Ok(vec![
            config.anthropic_model.clone(),
            "claude-sonnet-5-5".into(),
            "claude-3-7-sonnet-20250219".into(),
            "claude-3-5-haiku-20241022".into(),
        ])
    }
}

/// Google Gemini sağlayıcısı.
pub struct GeminiProvider;

impl AiProvider for GeminiProvider {
    fn id(&self) -> &'static str {
        "gemini"
    }

    fn display_name(&self) -> &'static str {
        "Google Gemini"
    }

    fn send_chat(
        &self,
        config: &AiConfig,
        system_prompt: &str,
        user_prompt: &str,
        cred_store: &dyn CredentialStorage,
    ) -> Result<String, AiError> {
        let endpoint = config.gemini_url.replace("{model}", &config.gemini_model);
        check_https_supported(&endpoint)?;

        let api_key = cred_store
            .get("WerSeuS_AI_ApiKey_gemini")
            .filter(|k| !k.trim().is_empty())
            .ok_or_else(|| AiError::MissingApiKey("Google Gemini".into()))?;

        check_plaintext_api_key_guard(&endpoint, &api_key)?;

        let body = json!({
            "contents": [
                {"role": "user", "parts": [{"text": user_prompt}]}
            ],
            "systemInstruction": {
                "parts": [{"text": system_prompt}]
            }
        });

        let resp = http_agent()
            .post(&endpoint)
            .set("x-goog-api-key", &api_key)
            .set("content-type", "application/json")
            .timeout(Duration::from_secs(config.timeout_secs))
            .send_json(body)
            .map_err(|e| match e {
                ureq::Error::Status(code, r) => {
                    let msg = r.into_string().unwrap_or_default();
                    AiError::Http(code, msg)
                }
                ureq::Error::Transport(t) => {
                    if t.kind() == ureq::ErrorKind::Io {
                        AiError::Timeout(config.timeout_secs)
                    } else {
                        AiError::Network(t.to_string())
                    }
                }
            })?;

        let parsed: serde_json::Value = resp
            .into_json()
            .map_err(|e| AiError::Parse(e.to_string()))?;

        if let Some(content) = parsed
            .get("candidates")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|cand| cand.get("content"))
            .and_then(|cont| cont.get("parts"))
            .and_then(|parts| parts.as_array())
            .and_then(|p_arr| p_arr.first())
            .and_then(|part| part.get("text"))
            .and_then(|t| t.as_str())
        {
            Ok(content.to_string())
        } else {
            Err(AiError::Parse(tf(
                "ai.err.no_content_fmt",
                &["Gemini", "candidates[0].content.parts[0].text"],
            )))
        }
    }

    fn fetch_models(
        &self,
        config: &AiConfig,
        _cred_store: &dyn CredentialStorage,
    ) -> Result<Vec<String>, AiError> {
        Ok(vec![
            config.gemini_model.clone(),
            "gemini-3.8-flash".into(),
            "gemini-2.5-flash".into(),
            "gemini-2.5-pro".into(),
        ])
    }
}

/// Testler için sahte sağlayıcı.
#[derive(Default)]
pub struct MockAiProvider {
    pub call_count: std::sync::atomic::AtomicUsize,
    pub canned_response: std::sync::RwLock<Option<String>>,
}

impl MockAiProvider {
    pub fn new(canned: Option<String>) -> Self {
        Self {
            call_count: std::sync::atomic::AtomicUsize::new(0),
            canned_response: std::sync::RwLock::new(canned),
        }
    }
}

impl AiProvider for MockAiProvider {
    fn id(&self) -> &'static str {
        "mock"
    }

    fn display_name(&self) -> &'static str {
        "Mock Provider"
    }

    fn send_chat(
        &self,
        _config: &AiConfig,
        _system: &str,
        _user: &str,
        _cred: &dyn CredentialStorage,
    ) -> Result<String, AiError> {
        self.call_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let lock = self.canned_response.read().unwrap();
        if let Some(ref text) = *lock {
            Ok(text.clone())
        } else {
            Ok(r#"{"explanation": "Mock test response", "changes": [{"field": "b", "value": 2.2, "reason": "Test"}]}"#.into())
        }
    }

    fn fetch_models(
        &self,
        _config: &AiConfig,
        _cred: &dyn CredentialStorage,
    ) -> Result<Vec<String>, AiError> {
        Ok(vec!["mock-model".into()])
    }
}

pub fn get_provider(provider_type: crate::ai::config::AiProviderType) -> Box<dyn AiProvider> {
    match provider_type {
        crate::ai::config::AiProviderType::Ollama => Box::new(OllamaProvider),
        crate::ai::config::AiProviderType::OpenAiCompatible => Box::new(OpenAiCompatibleProvider),
        crate::ai::config::AiProviderType::Anthropic => Box::new(AnthropicProvider),
        crate::ai::config::AiProviderType::Gemini => Box::new(GeminiProvider),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn https_is_accepted_by_tls_check_and_request_builder() {
        assert!(check_https_supported("https://api.openai.com/v1/chat/completions").is_ok());
        assert!(check_https_supported("HTTPS://generativelanguage.googleapis.com").is_ok());
        assert!(check_https_supported("http://127.0.0.1:11434").is_ok());
        // Ağ çağrısı yapılmaz; yalnızca istek nesnesi kurulur.
        let req = http_agent().post("https://api.anthropic.com/v1/messages");
        assert_eq!(req.url(), "https://api.anthropic.com/v1/messages");
    }

    #[test]
    fn test_timeout_error_formatting_dynamic() {
        for secs in [30, 60, 90, 120] {
            let err = AiError::Timeout(secs);
            let msg = format!("{}", err);
            assert!(
                msg.contains(&format!("{}s", secs)),
                "Hata mesajı dinamik saniyeyi içermeli: {}",
                msg
            );
        }
    }

    #[test]
    fn test_is_loopback_host() {
        assert!(is_loopback_host("http://localhost:11434"));
        assert!(is_loopback_host("http://127.0.0.1:8000/v1"));
        assert!(is_loopback_host("http://[::1]:8080"));
        assert!(is_loopback_host("http://0.0.0.0:1234/api"));
        assert!(!is_loopback_host("http://api.openai.com/v1"));
        assert!(!is_loopback_host("http://api.anthropic.com"));
        assert!(!is_loopback_host("http://remote-gpu-server.net:8000"));
    }

    #[test]
    fn test_plaintext_api_key_guard() {
        // Uzak sunucuya API anahtarı ile HTTP isteği engellenmeli
        let remote_err = check_plaintext_api_key_guard(
            "http://api.openai.com/v1/chat/completions",
            "sk-1234567890abcdef",
        );
        assert!(remote_err.is_err(), "Uzak HTTP API anahtarı engellenmeli");

        // Uzak sunucuya API anahtarsız izin verilmeli
        let remote_empty_key =
            check_plaintext_api_key_guard("http://api.openai.com/v1/chat/completions", "");
        assert!(
            remote_empty_key.is_ok(),
            "Anahtarsız istek güvenlik hatası üretmemeli"
        );

        // Yerel loopback adreslerine anahtarlı da olsa izin verilmeli
        let local_res = check_plaintext_api_key_guard(
            "http://localhost:1234/v1/chat/completions",
            "sk-local-test",
        );
        assert!(local_res.is_ok());

        let ip_res = check_plaintext_api_key_guard(
            "http://127.0.0.1:8000/v1/chat/completions",
            "sk-local-test",
        );
        assert!(ip_res.is_ok());

        let ipv6_res =
            check_plaintext_api_key_guard("http://[::1]:8080/v1/chat/completions", "sk-local-test");
        assert!(ipv6_res.is_ok());
    }
}
