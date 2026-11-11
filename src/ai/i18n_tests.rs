// SPDX-License-Identifier: AGPL-3.0-or-later
//! Yapay zekâ durum/hata iletileri seçili dilde çıkar: EN arayüzde Türkçe metin görünmez.
//! `with_lang` yalnız bu iş parçacığının dilini değiştirir (paralel testlerde yarış olmaz).

use crate::ai::client::parse_ai_response;
use crate::ai::config::{AiConfig, AiProviderType};
use crate::ai::credential::SessionCredentialStore;
use crate::ai::provider::AiError;
use crate::ai::state::AiAssistantState;
use crate::i18n::{Lang, en_table, tr_table, with_lang};
use crate::project::Project;
use std::sync::Arc;

const TR_LETTERS: &str = "çğıöşüÇĞİÖŞÜâîû";

fn has_turkish(s: &str) -> bool {
    s.chars().any(|c| TR_LETTERS.contains(c))
}

fn store() -> Arc<dyn crate::ai::credential::CredentialStorage> {
    Arc::new(SessionCredentialStore::new())
}

#[test]
fn every_ai_message_key_has_english_without_turkish_letters() {
    let en = en_table();
    let tr = tr_table();
    let mut n = 0;
    for (k, v) in en.iter() {
        if k.starts_with("ai.msg.")
            || k.starts_with("ai.err.")
            || k.starts_with("ai.provider.")
            || k.starts_with("ai.panel.")
            || k.starts_with("status_ai_")
            || k.starts_with("set.ai.")
        {
            n += 1;
            assert!(!has_turkish(v), "{k}: EN değerinde Türkçe harf: {v}");
            assert!(tr.contains_key(k), "{k}: TR yok");
        }
    }
    assert!(n >= 30, "beklenenden az anahtar: {n}");
}

#[test]
fn state_messages_follow_language() {
    let disabled = AiConfig::default();
    assert!(!disabled.enabled);
    for (lang, needle) in [(Lang::En, "turned off"), (Lang::Tr, "kapalıdır")] {
        with_lang(lang, || {
            let mut st = AiAssistantState::new();
            st.start_request(&disabled, &Project::default(), "x".into(), lang, store());
            let m = st.error_message.expect("kapalıyken hata iletisi");
            assert!(m.contains(needle), "{lang:?}: {m}");
            assert_eq!(has_turkish(&m), lang == Lang::Tr, "{m}");

            let mut st = AiAssistantState::new();
            st.cancel_request();
            let m = st.status_message.expect("iptal iletisi");
            assert_eq!(has_turkish(&m), lang == Lang::Tr, "{m}");
        });
    }
}

#[test]
fn cloud_and_teacher_messages_are_english_in_en() {
    with_lang(Lang::En, || {
        // Bulut sağlayıcı + kabul edilmemiş uyarı: öğretmen yolu açıklayıcı hata verir.
        let cfg = AiConfig {
            enabled: true,
            provider: AiProviderType::Anthropic,
            cloud_disclaimer_accepted: false,
            ..AiConfig::default()
        };
        let mut st = AiAssistantState::new();
        st.start_socratic_teacher_request(
            &cfg,
            &Project::default(),
            "s".into(),
            "u".into(),
            store(),
        );
        let m = st.error_message.expect("hata");
        assert!(m.contains("cloud notice"), "{m}");
        assert!(!has_turkish(&m));
    });
}

#[test]
fn apply_and_undo_status_messages_follow_language() {
    let raw = r#"{"explanation": "x", "changes": [{"field": "b", "value": 2.5, "reason": "r"}]}"#;
    for lang in [Lang::En, Lang::Tr] {
        with_lang(lang, || {
            let mut p = Project::default();
            let mut st = AiAssistantState::new();
            st.active_proposal = Some(parse_ai_response(raw, &p).unwrap());
            st.apply_active_proposal(&mut p, false).unwrap();
            let m = st.status_message.clone().unwrap();
            assert!(m.contains('1'), "{m}");
            assert_eq!(has_turkish(&m), lang == Lang::Tr, "{lang:?}: {m}");
            st.undo_last_proposal(&mut p).unwrap();
            let m = st.status_message.clone().unwrap();
            assert_eq!(has_turkish(&m), lang == Lang::Tr, "{lang:?}: {m}");
        });
    }
}

#[test]
fn provider_errors_are_english_in_en() {
    with_lang(Lang::En, || {
        let errs = [
            AiError::Network("boom".into()),
            AiError::Http(503, "busy".into()),
            AiError::Parse("bad".into()),
            AiError::Timeout(45),
            AiError::MissingApiKey("Gemini".into()),
            AiError::Cancelled,
        ];
        for e in errs {
            let m = e.to_string();
            assert!(!has_turkish(&m), "{e:?}: {m}");
            assert!(!m.starts_with("ai."), "anahtar sızdı: {m}");
        }
        assert!(AiError::Timeout(45).to_string().contains("45s"));
        assert!(AiError::Timeout(45).to_string().contains("timed out"));
        assert!(
            AiError::Http(503, "busy".into())
                .to_string()
                .contains("503")
        );
    });
    with_lang(Lang::Tr, || {
        assert!(has_turkish(&AiError::Cancelled.to_string()));
        assert!(AiError::Timeout(45).to_string().contains("45s"));
    });
}

#[test]
fn response_validation_errors_are_english_in_en() {
    with_lang(Lang::En, || {
        let p = Project::default();
        let e = parse_ai_response("no json here", &p).unwrap_err();
        assert!(!has_turkish(&e) && e.contains("JSON"), "{e}");
        let e = parse_ai_response("{ not valid json }", &p).unwrap_err();
        assert!(!has_turkish(&e) && e.contains("parse error"), "{e}");

        let raw = r#"{"explanation":"x","changes":[
            {"field":"b","value":99.0,"reason":"r"},
            {"field":"flux_capacitor","value":1.0,"reason":"r"},
            {"field":"drone_type","value":"zeppelin","reason":"r"}
        ]}"#;
        let prop = parse_ai_response(raw, &p).unwrap();
        for it in &prop.items {
            assert!(!it.is_valid);
            let m = it.validation_error.clone().unwrap();
            assert!(!has_turkish(&m), "{m}");
            assert!(!has_turkish(&it.field_label), "{}", it.field_label);
        }
        assert!(
            prop.items[0]
                .validation_error
                .as_ref()
                .unwrap()
                .contains("outside the valid range")
        );
    });
}

#[test]
fn provider_labels_are_localized() {
    for p in AiProviderType::ALL {
        with_lang(Lang::En, || assert!(!has_turkish(p.label()), "{:?}", p));
        with_lang(Lang::Tr, || assert!(!p.label().starts_with("ai.")));
    }
}
