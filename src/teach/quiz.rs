// SPDX-License-Identifier: AGPL-3.0-or-later
//! Ders mini testlerinin (quiz) ayrıştırılması, veri modelleri ve doğrulama mantığı.

use serde::{Deserialize, Serialize};

/// Çoktan seçmeli bir mini test sorusu.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuizQuestion {
    /// Soru numarası (1-tabanlı, ör. 1, 2, 3)
    pub number: u8,
    /// Soru metni
    pub question: String,
    /// Şıklar (A, B, C, D)
    pub options: Vec<String>,
    /// Doğru şıkkın indeksi (0: A, 1: B, 2: C, 3: D)
    pub correct_index: usize,
    /// Cevap açıklaması ve çözümü
    pub explanation: String,
}

impl QuizQuestion {
    /// Verilen şık indeksinin doğru olup olmadığını döndürür.
    pub fn is_correct(&self, chosen_idx: usize) -> bool {
        chosen_idx == self.correct_index
    }

    /// Şık harfini (A, B, C, D) döndürür.
    pub fn option_letter(idx: usize) -> char {
        match idx {
            0 => 'A',
            1 => 'B',
            2 => 'C',
            3 => 'D',
            _ => '?',
        }
    }
}

/// Markdown ders içeriğinden `### Mini test` veya `### Mini quiz` bölümünü ayıklar ve soruları yapılandırır.
pub fn parse_quiz_from_markdown(markdown: &str) -> Vec<QuizQuestion> {
    let mut questions = Vec::new();

    // 1. Mini test / Mini quiz başlığını bul
    let quiz_start = if let Some(idx) = markdown.find("### Mini test") {
        idx + "### Mini test".len()
    } else if let Some(idx) = markdown.find("### Mini quiz") {
        idx + "### Mini quiz".len()
    } else {
        return questions;
    };

    let quiz_text = &markdown[quiz_start..];
    let lines: Vec<&str> = quiz_text.lines().collect();

    let mut current_q: Option<QuizQuestion> = None;
    let mut in_explanation = false;

    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Yeni bir bölüm başlığı gelirse testi bitir
        if trimmed.starts_with('#') {
            break;
        }

        // Soru başlangıcı: `**1. `, `**2. `, `**3. ` vb.
        if is_question_start(trimmed) {
            if let Some(q) = current_q.take() {
                questions.push(q);
            }
            in_explanation = false;

            let (num, prompt) = extract_question_prompt(trimmed);
            current_q = Some(QuizQuestion {
                number: num,
                question: prompt,
                options: Vec::with_capacity(4),
                correct_index: 0,
                explanation: String::new(),
            });
            continue;
        }

        let Some(ref mut q) = current_q else {
            continue;
        };

        // Şıklar: `A) `, `B) `, `C) `, `D) `
        if is_option_line(trimmed) {
            in_explanation = false;
            let opt_text = extract_option_text(trimmed);
            q.options.push(opt_text);
            continue;
        }

        // Doğru cevap: `*Doğru Cevap:* X` veya `*Correct Answer:* X`
        if let Some(correct_letter) = extract_correct_answer(trimmed) {
            in_explanation = false;
            q.correct_index = match correct_letter {
                'A' | 'a' => 0,
                'B' | 'b' => 1,
                'C' | 'c' => 2,
                'D' | 'd' => 3,
                _ => 0,
            };
            continue;
        }

        // Açıklama: `*Açıklama:* ...` veya `*Explanation:* ...`
        if let Some(expl) = extract_explanation_start(trimmed) {
            in_explanation = true;
            q.explanation.push_str(expl);
            continue;
        }

        // Açıklamanın devam eden satırları
        if in_explanation {
            if !q.explanation.is_empty() {
                q.explanation.push(' ');
            }
            q.explanation.push_str(trimmed);
        }
    }

    if let Some(q) = current_q {
        questions.push(q);
    }

    questions
}

fn is_question_start(line: &str) -> bool {
    let t = line.trim_start();
    if !t.starts_with("**") {
        return false;
    }
    let rest = &t[2..];
    let digit_count = rest.chars().take_while(|c| c.is_ascii_digit()).count();
    if digit_count == 0 {
        return false;
    }
    let after_digits = &rest[digit_count..];
    after_digits.starts_with(". ") || after_digits.starts_with(".\t")
}

fn extract_question_prompt(line: &str) -> (u8, String) {
    let t = line.trim_start().strip_prefix("**").unwrap_or(line);
    let digit_str: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
    let num: u8 = digit_str.parse().unwrap_or(1);
    let after_num = t[digit_str.len()..].trim_start();
    let after_dot = after_num.strip_prefix('.').unwrap_or(after_num).trim();
    let clean = after_dot.strip_suffix("**").unwrap_or(after_dot).trim();
    (num, clean.to_string())
}

fn is_option_line(line: &str) -> bool {
    let bytes = line.as_bytes();
    if bytes.len() >= 3 {
        let first = bytes[0];
        let second = bytes[1];
        let third = bytes[2];
        matches!(first, b'A'..=b'D')
            && (second == b')' || second == b'.')
            && third.is_ascii_whitespace()
    } else {
        false
    }
}

fn extract_option_text(line: &str) -> String {
    if line.len() >= 3 {
        line[2..].trim().to_string()
    } else {
        line.to_string()
    }
}

fn extract_correct_answer(line: &str) -> Option<char> {
    let tr_tag = "*Doğru Cevap:*";
    let en_tag = "*Correct Answer:*";

    let after_tag = if let Some(pos) = line.find(tr_tag) {
        Some(&line[pos + tr_tag.len()..])
    } else {
        line.find(en_tag).map(|pos| &line[pos + en_tag.len()..])
    };

    after_tag.and_then(|text| {
        text.chars()
            .find(|c| matches!(c, 'A'..='D' | 'a'..='d'))
            .map(|c| c.to_ascii_uppercase())
    })
}

fn extract_explanation_start(line: &str) -> Option<&str> {
    let tr_tag = "*Açıklama:*";
    let en_tag = "*Explanation:*";

    if let Some(pos) = line.find(tr_tag) {
        Some(line[pos + tr_tag.len()..].trim())
    } else {
        line.find(en_tag)
            .map(|pos| line[pos + en_tag.len()..].trim())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_quiz() {
        let sample_md = r#"
# Ders 1

### Mini test
**1. Sabit uçuşta kaldırma neye eşit olmalıdır?**
A) İtki
B) Ağırlık
C) Sürükleme
D) Sıfır
*Doğru Cevap:* B
*Açıklama:* Düşey dengede L = W olmalıdır.

**2. Hız 2 katına çıkınca kaldırma ne olur?**
A) 2 katına çıkar
B) Yarıya iner
C) 4 katına çıkar
D) Değişmez
*Doğru Cevap:* C
*Açıklama:* Kaldırma formülünde hız kareseldir.
"#;

        let questions = parse_quiz_from_markdown(sample_md);
        assert_eq!(questions.len(), 2);

        assert_eq!(questions[0].number, 1);
        assert_eq!(
            questions[0].question,
            "Sabit uçuşta kaldırma neye eşit olmalıdır?"
        );
        assert_eq!(questions[0].options.len(), 4);
        assert_eq!(questions[0].options[0], "İtki");
        assert_eq!(questions[0].options[1], "Ağırlık");
        assert_eq!(questions[0].correct_index, 1);
        assert_eq!(questions[0].explanation, "Düşey dengede L = W olmalıdır.");

        assert_eq!(questions[1].number, 2);
        assert_eq!(questions[1].correct_index, 2);
        assert_eq!(
            questions[1].explanation,
            "Kaldırma formülünde hız kareseldir."
        );
    }
}
