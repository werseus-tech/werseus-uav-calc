// SPDX-License-Identifier: AGPL-3.0-or-later
//! Yapay zeka asistanı entegrasyon testleri.
//!
//! Kurallar:
//! 1. Gerçek bulut API'lerine (OpenAI, Anthropic, Gemini) istek atılmaz.
//! 2. Gerçek kullanıcı dizinlerine veya gerçek Windows Credential Manager'a yazılmaz (MockCredentialStore kullanılır).
//! 3. Proje serileştirildiğinde API anahtarı içermez.
//! 4. AI kapalıyken ağ çağrısı yapılmaz.
//! 5. Canlı test: Ollama çalışıyorsa qwen3:8b ile test edilir; Ollama yoksa atlanır.

use std::sync::Arc;
use werseus_uav_calc::ai::client::parse_ai_response;
use werseus_uav_calc::ai::config::{AiConfig, AiProviderType};
use werseus_uav_calc::ai::credential::{CredentialStorage, MockCredentialStore};
use werseus_uav_calc::ai::prompt::{build_quick_design_prompt, build_system_prompt};
use werseus_uav_calc::ai::provider::{AiProvider, MockAiProvider, OllamaProvider};
use werseus_uav_calc::ai::state::AiAssistantState;
use werseus_uav_calc::i18n::Lang;
use werseus_uav_calc::project::Project;

#[test]
fn test_project_json_never_contains_api_key() {
    let mut project = Project::default();
    project.name = "GizliProje".into();

    let fake_api_key = "sk-proj-FAKE-SECRET-KEY-1234567890abcdef-DO-NOT-LEAK";
    let store = MockCredentialStore::new();
    store.set("WerSeuS_AI_ApiKey_openai", fake_api_key).unwrap();

    let json_str = project.to_json().expect("JSON export failed");
    assert!(
        !json_str.contains(fake_api_key),
        "API anahtarı serileştirilen proje JSON'unda BULUNAMAZ!"
    );
    assert!(
        !json_str.contains("sk-"),
        "Proje JSON'unda herhangi bir API anahtarı kalıntısı bulunamaz!"
    );
}

#[test]
fn test_ai_disabled_makes_zero_network_calls() {
    let project = Project::default();
    let cred_store = Arc::new(MockCredentialStore::new());

    let mut config = AiConfig::default();
    config.enabled = false; // AI kapalı!

    let mut state = AiAssistantState::new();
    state.start_request(
        &config,
        &project,
        "1 kg kameralı drone tasarla".into(),
        Lang::Tr,
        cred_store,
    );

    assert!(
        !state.is_working,
        "AI kapalıyken istek başlatılmamalı ve arka plan iş parçacığı çalıştırılmamalıdır"
    );
    assert!(
        state.error_message.is_some(),
        "Kullanıcıya AI'nın kapalı olduğuna dair hata bildirilmelidir"
    );
}

#[test]
fn test_mock_provider_flow() {
    let project = Project::default();
    let cred_store = MockCredentialStore::new();
    let config = AiConfig::default();

    let mock_json = r#"{
        "explanation": "Test amaçlı hızlı tasarım önerisi",
        "changes": [
            {"field": "b", "value": 2.2, "reason": "Açıklık artırıldı"},
            {"field": "v_cr", "value": 20.0, "reason": "Seyir hızı belirlendi"}
        ]
    }"#;
    let mock_provider = MockAiProvider::new(Some(mock_json.into()));

    let reply = mock_provider
        .send_chat(&config, "system", "user", &cred_store)
        .expect("Mock chat should succeed");
    assert_eq!(
        mock_provider
            .call_count
            .load(std::sync::atomic::Ordering::SeqCst),
        1
    );

    let proposal = parse_ai_response(&reply, &project).expect("Parsing mock JSON failed");
    assert_eq!(proposal.items.len(), 2);
    assert_eq!(proposal.items[0].field, "b");
    assert_eq!(proposal.items[1].field, "v_cr");
}

#[test]
fn test_json_parsing_edge_cases() {
    let p = Project::default();

    // 1. Bozuk JSON
    let broken_json = "{ invalid_json: without quotes }";
    assert!(parse_ai_response(broken_json, &p).is_err());

    // 2. Markdown içine sarılı
    let md_json = "```json\n{\n\"explanation\": \"ok\",\n\"changes\": [{\"field\": \"s\", \"value\": 0.45, \"reason\": \"alan\"}]\n}\n```";
    let res = parse_ai_response(md_json, &p).expect("Markdown JSON should parse");
    assert_eq!(res.items.len(), 1);
    assert_eq!(res.items[0].field, "s");

    // 3. Aralık dışı değer (b = 999.0 m olamaz)
    let out_of_range_json = r#"{"explanation": "out", "changes": [{"field": "b", "value": 999.0, "reason": "too big"}]}"#;
    let res = parse_ai_response(out_of_range_json, &p).expect("Should parse but mark invalid");
    assert_eq!(res.items.len(), 1);
    assert!(!res.items[0].is_valid);
    assert!(!res.items[0].selected);
}

#[test]
fn test_live_ollama_qwen3_quick_design() {
    // Opt-in: loads qwen3 into the local Ollama. Run with WERSEUS_LIVE_OLLAMA=1.
    if std::env::var("WERSEUS_LIVE_OLLAMA").as_deref() != Ok("1") {
        println!("Canlı Ollama testi atlandı (WERSEUS_LIVE_OLLAMA=1 ayarlanmadı).");
        return;
    }
    let cred_store = MockCredentialStore::new();
    let mut config = AiConfig::default();
    config.enabled = true;
    config.provider = AiProviderType::Ollama;
    config.ollama_url = "http://127.0.0.1:11434".into();
    config.ollama_model = "qwen3:8b".into();
    config.timeout_secs = 60;

    let provider = OllamaProvider;

    // Önce Ollama çalışıyor mu kontrol et (/api/tags)
    let models = match provider.fetch_models(&config, &cred_store) {
        Ok(m) => m,
        Err(_) => {
            println!("Ollama (http://127.0.0.1:11434) ulaşılamaz durumda, canlı test atlanıyor.");
            return;
        }
    };

    println!("Ollama bulundu! Mevcut modeller: {:?}", models);
    if !models.iter().any(|m| m.contains("qwen3")) {
        println!("qwen3 modeli Ollama'da yüklü değil, canlı test atlanıyor.");
        return;
    }

    let project = Project::default();
    let lang = Lang::Tr;
    let system_prompt = build_system_prompt(&project, lang);
    let user_prompt = build_quick_design_prompt(
        "1 kg kamera taşıyan, 90 dk havada kalan, sırt çantasına sığan bir gözetleme İHA'sı",
        lang,
    );

    println!("Ollama qwen3:8b'ye canlı istek gönderiliyor...");
    let raw_reply = provider
        .send_chat(&config, &system_prompt, &user_prompt, &cred_store)
        .expect("Canlı Ollama isteği başarısız oldu");

    println!("Ollama Ham Yanıtı:\n{}", raw_reply);

    let proposal = parse_ai_response(&raw_reply, &project)
        .expect("Ollama yanıtı geçerli bir JSON önerisi olarak ayrıştırılamadı");

    println!("Ayrıştırılan Açıklama: {}", proposal.explanation);
    println!("Ayrıştırılan Değişiklik Sayısı: {}", proposal.items.len());
    for item in &proposal.items {
        println!(
            "  - Alan: {} | Eski: {:?} -> Yeni: {:?} | Geçerli mi: {} | Gerekçe: {}",
            item.field, item.old_value, item.new_value, item.is_valid, item.reason
        );
    }

    // En az bir geçerli değişiklik üretilmiş olmalı
    let valid_count = proposal.items.iter().filter(|i| i.is_valid).count();
    assert!(
        valid_count >= 1,
        "Ollama en az 1 geçerli parametre değişikliği önermiş olmalıdır (Bulunan: {})",
        valid_count
    );
}
