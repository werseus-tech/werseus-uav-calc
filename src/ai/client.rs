// SPDX-License-Identifier: AGPL-3.0-or-later
//! AI yanıtlarını esnek ayrıştırma, doğrulama, değişiklik uygulama ve geri alma motoru.

use crate::ai::fields::{AiValue, find_field};
use crate::i18n::{t, tf};
use crate::project::Project;
use serde::Deserialize;

/// Modele sunulan ve kullanıcı tarafından onaylanabilecek değişiklik önerisi maddesi.
#[derive(Clone, Debug, PartialEq)]
pub struct ProposalItem {
    pub field: String,
    pub field_label: String,
    pub unit: String,
    pub old_value: AiValue,
    pub new_value: AiValue,
    pub reason: String,
    pub is_valid: bool,
    pub validation_error: Option<String>,
    pub selected: bool,
}

/// AI yanıtından çıkarılan yapılandırılmış öneri paketi.
#[derive(Clone, Debug, PartialEq)]
pub struct AiProposal {
    pub explanation: String,
    pub items: Vec<ProposalItem>,
}

/// Geri alma (Undo) için saklanan eski değer kaydı.
#[derive(Clone, Debug, PartialEq)]
pub struct AiUndoRecord {
    pub field: String,
    pub previous_value: AiValue,
}

#[derive(Deserialize)]
struct RawResponse {
    #[serde(default)]
    explanation: String,
    #[serde(default)]
    changes: Vec<RawChange>,
}

#[derive(Deserialize)]
struct RawChange {
    field: Option<String>,
    value: Option<serde_json::Value>,
    new_value: Option<serde_json::Value>,
    reason: Option<String>,
}

/// Metin içerisindeki en dış `{ ... }` veya ```json ... ``` bloğunu ayıklar.
pub fn extract_json_block(text: &str) -> Option<&str> {
    let trimmed = text.trim();

    // 1. ```json ... ``` veya ``` ... ``` bloklarını ara
    if let Some(start_idx) = trimmed.find("```") {
        let after_ticks = &trimmed[start_idx + 3..];
        let content_start = if let Some(newline_pos) = after_ticks.find('\n') {
            newline_pos + 1
        } else {
            0
        };
        let body = &after_ticks[content_start..];
        if let Some(end_ticks) = body.find("```") {
            let inner = body[..end_ticks].trim();
            if inner.starts_with('{') && inner.ends_with('}') {
                return Some(inner);
            }
        }
    }

    // 2. İlk '{' ile son '}' arasındaki aralığı ayıkla
    let start_brace = trimmed.find('{')?;
    let end_brace = trimmed.rfind('}')?;
    if end_brace >= start_brace {
        Some(&trimmed[start_brace..=end_brace])
    } else {
        None
    }
}

/// Ham model yanıtını esnek ayrıştırır, doğrular ve `AiProposal` oluşturur.
pub fn parse_ai_response(raw_text: &str, project: &Project) -> Result<AiProposal, String> {
    let json_str = extract_json_block(raw_text).ok_or_else(|| t("ai.err.no_json").to_string())?;

    let raw: RawResponse = serde_json::from_str(json_str).map_err(|e| {
        tf(
            "ai.err.json_parse_fmt",
            &[e.to_string(), json_str.to_string()],
        )
    })?;

    let mut items = Vec::new();
    for raw_c in raw.changes {
        let field_id = match raw_c.field {
            Some(f) if !f.trim().is_empty() => f.trim().to_string(),
            _ => continue,
        };

        let reason = raw_c.reason.unwrap_or_default();
        let json_val = match raw_c.value.or(raw_c.new_value) {
            Some(v) => v,
            None => continue,
        };

        let ai_val = match json_val {
            serde_json::Value::Number(n) => {
                if let Some(f) = n.as_f64() {
                    AiValue::Num(f)
                } else {
                    continue;
                }
            }
            serde_json::Value::String(s) => {
                if let Ok(f) = s.trim().parse::<f64>() {
                    AiValue::Num(f)
                } else {
                    AiValue::Str(s)
                }
            }
            _ => continue,
        };

        if let Some(field_def) = find_field(&field_id) {
            let field_label = crate::i18n::t(field_def.desc_key).to_string();
            let old_val = (field_def.get)(project);
            let unit = field_def.unit.to_string();

            match field_def.validate(&ai_val) {
                Ok(()) => {
                    items.push(ProposalItem {
                        field: field_id,
                        field_label,
                        unit,
                        old_value: old_val,
                        new_value: ai_val,
                        reason,
                        is_valid: true,
                        validation_error: None,
                        selected: true,
                    });
                }
                Err(err_msg) => {
                    items.push(ProposalItem {
                        field: field_id,
                        field_label,
                        unit,
                        old_value: old_val,
                        new_value: ai_val,
                        reason,
                        is_valid: false,
                        validation_error: Some(err_msg),
                        selected: false,
                    });
                }
            }
        } else {
            // Tanınmayan alan
            items.push(ProposalItem {
                field_label: tf("ai.err.unknown_field_label_fmt", &[field_id.as_str()]),
                field: field_id.clone(),
                unit: "-".into(),
                old_value: AiValue::Str("?".into()),
                new_value: ai_val,
                reason,
                is_valid: false,
                validation_error: Some(tf("ai.err.unknown_field_fmt", &[field_id.as_str()])),
                selected: false,
            });
        }
    }

    Ok(AiProposal {
        explanation: raw.explanation,
        items,
    })
}

/// Önerilen değişiklikleri (yalnızca geçerli ve seçili olanları) projeye uygular.
/// Eski değerleri geri alma kaydı (`AiUndoRecord`) olarak döndürür.
pub fn apply_proposal(
    project: &mut Project,
    proposal: &AiProposal,
    selected_only: bool,
) -> Result<Vec<AiUndoRecord>, String> {
    let mut undo_records = Vec::new();

    for item in &proposal.items {
        if !item.is_valid {
            continue;
        }
        if selected_only && !item.selected {
            continue;
        }

        if let Some(field_def) = find_field(&item.field) {
            let old_val = (field_def.get)(project);
            (field_def.set)(project, &item.new_value)?;
            undo_records.push(AiUndoRecord {
                field: item.field.clone(),
                previous_value: old_val,
            });
        }
    }

    Ok(undo_records)
}

/// Geri alma (Undo) işlemini gerçekleştirir.
pub fn undo_proposal(project: &mut Project, records: &[AiUndoRecord]) -> Result<(), String> {
    for rec in records.iter().rev() {
        if let Some(field_def) = find_field(&rec.field) {
            (field_def.set)(project, &rec.previous_value)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_block_bare() {
        let raw = r#"{"explanation": "hello", "changes": []}"#;
        assert_eq!(extract_json_block(raw), Some(raw));
    }

    #[test]
    fn test_extract_json_block_markdown() {
        let raw = "Here is the plan:\n```json\n{\"explanation\": \"test\", \"changes\": []}\n```\nHope it helps!";
        let extracted = extract_json_block(raw).unwrap();
        assert_eq!(extracted, r#"{"explanation": "test", "changes": []}"#);
    }

    #[test]
    fn test_extract_json_block_with_thinking() {
        let raw = "Thinking: let me analyze...\n\n{\"explanation\": \"done\", \"changes\": [{\"field\": \"b\", \"value\": 2.0, \"reason\": \"wider span\"}]}";
        let extracted = extract_json_block(raw).unwrap();
        assert!(extracted.starts_with('{') && extracted.ends_with('}'));
    }

    #[test]
    fn test_parse_valid_changes() {
        let p = Project::default();
        let raw = r#"{
            "explanation": "Sabit kanat iyileştirmesi",
            "changes": [
                {"field": "b", "value": 2.4, "reason": "Daha iyi süzülüş"},
                {"field": "m_payload", "value": 1.0, "reason": "1 kg kamera"}
            ]
        }"#;
        let proposal = parse_ai_response(raw, &p).unwrap();
        assert_eq!(proposal.explanation, "Sabit kanat iyileştirmesi");
        assert_eq!(proposal.items.len(), 2);
        assert!(proposal.items[0].is_valid);
        assert_eq!(proposal.items[0].new_value, AiValue::Num(2.4));
    }

    #[test]
    fn test_parse_invalid_and_unknown_fields() {
        let p = Project::default();
        let raw = r#"{
            "explanation": "Hatalı alan testi",
            "changes": [
                {"field": "b", "value": 99.0, "reason": "Aralık dışı açıklık"},
                {"field": "flux_capacitor", "value": 1.21, "reason": "Bilinmeyen alan"}
            ]
        }"#;
        let proposal = parse_ai_response(raw, &p).unwrap();
        assert_eq!(proposal.items.len(), 2);

        // b out of range
        assert!(!proposal.items[0].is_valid);
        assert!(proposal.items[0].validation_error.is_some());
        assert!(!proposal.items[0].selected);

        // flux_capacitor unknown
        assert!(!proposal.items[1].is_valid);
        assert!(proposal.items[1].validation_error.is_some());
        assert!(!proposal.items[1].selected);
    }

    #[test]
    fn test_apply_and_undo() {
        let mut p = Project::default();
        let original_b = p.b;

        let raw = r#"{
            "explanation": "Test",
            "changes": [
                {"field": "b", "value": 2.5, "reason": "Açıklık artırımı"}
            ]
        }"#;
        let proposal = parse_ai_response(raw, &p).unwrap();

        let undo_records = apply_proposal(&mut p, &proposal, true).unwrap();
        assert_eq!(p.b, 2.5);

        undo_proposal(&mut p, &undo_records).unwrap();
        assert_eq!(p.b, original_b);
    }
}
