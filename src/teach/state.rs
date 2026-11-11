// SPDX-License-Identifier: AGPL-3.0-or-later
//! Öğretmen modu ayarları, öğrenci seviyesi ve ilerleme durumu yönetimi.

use crate::i18n::Lang;
use crate::teach::lessons::Lesson;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Öğrenci / kullanıcı seviyesi.
/// Açıklama metinlerinin derinliğini, matematik seviyesini ve üslubunu etkiler.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Default)]
pub enum StudentLevel {
    /// Başlangıç: Günlük hayattan benzetmeler, sezgisel kavrama, ağır formüllerden arındırılmış dil.
    #[default]
    Beginner,
    /// Orta: Standart mühendislik kuralı ve temel formüller, pratik ödünleşimler.
    Intermediate,
    /// İleri: Tam aerodinamik/yapısal denklemler, türevler ve kısıt marj analizleri.
    Advanced,
}

impl StudentLevel {
    pub const ALL: [StudentLevel; 3] = [
        StudentLevel::Beginner,
        StudentLevel::Intermediate,
        StudentLevel::Advanced,
    ];

    pub fn label(&self, lang: Lang) -> &'static str {
        match (self, lang) {
            (StudentLevel::Beginner, Lang::Tr) => "Başlangıç",
            (StudentLevel::Beginner, Lang::En) => "Beginner",
            (StudentLevel::Intermediate, Lang::Tr) => "Orta",
            (StudentLevel::Intermediate, Lang::En) => "Intermediate",
            (StudentLevel::Advanced, Lang::Tr) => "İleri",
            (StudentLevel::Advanced, Lang::En) => "Advanced",
        }
    }

    pub fn description(&self, lang: Lang) -> &'static str {
        match (self, lang) {
            (StudentLevel::Beginner, Lang::Tr) => {
                "Sezgisel günlük benzetmeler, sade anlatım, formülsüz temel mantık."
            }
            (StudentLevel::Beginner, Lang::En) => {
                "Intuitive everyday analogies, simple language, concepts without heavy formulas."
            }
            (StudentLevel::Intermediate, Lang::Tr) => {
                "Temel mühendislik formülleri, pratik kurallar ve tasarım dengeleri."
            }
            (StudentLevel::Intermediate, Lang::En) => {
                "Core engineering formulas, standard rules of thumb, and design trade-offs."
            }
            (StudentLevel::Advanced, Lang::Tr) => {
                "Ayrıntılı havacılık fiziği, tam denklemler, duyarlılık ve marj analizleri."
            }
            (StudentLevel::Advanced, Lang::En) => {
                "In-depth aerodynamics, exact governing equations, sensitivities and margins."
            }
        }
    }
}

/// Öğretmen modu kullanıcı ayarları (proje dışı ayarda saklanır, projeyi kirletmez).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct TeacherSettings {
    /// Seçili öğrenci seviyesi
    #[serde(default)]
    pub level: StudentLevel,
    /// Tamamlanan ders numaraları (1..=8)
    #[serde(default)]
    pub completed_lessons: Vec<u8>,
    /// Mini test yanıtları: `"l_{id}_q_{num}"` -> `secilen_sik_indeksi`
    #[serde(default)]
    pub quiz_answers: HashMap<String, usize>,
    /// Öğrenci adı / kimliği (yalnızca kullanıcı isterse elle girer, varsayılan boş)
    #[serde(default)]
    pub student_name: String,
}

impl TeacherSettings {
    pub fn is_lesson_completed(&self, id: u8) -> bool {
        self.completed_lessons.contains(&id)
    }

    pub fn set_lesson_completed(&mut self, id: u8, completed: bool) {
        if completed {
            if !self.completed_lessons.contains(&id) {
                self.completed_lessons.push(id);
                self.completed_lessons.sort_unstable();
            }
        } else {
            self.completed_lessons.retain(|&x| x != id);
        }
    }

    fn key(lesson_id: u8, q_num: u8) -> String {
        format!("l_{lesson_id}_q_{q_num}")
    }

    pub fn get_answer(&self, lesson_id: u8, q_num: u8) -> Option<usize> {
        self.quiz_answers.get(&Self::key(lesson_id, q_num)).copied()
    }

    pub fn set_answer(&mut self, lesson_id: u8, q_num: u8, chosen_idx: usize) {
        self.quiz_answers
            .insert(Self::key(lesson_id, q_num), chosen_idx);
    }

    /// Bir ders için (doğru_sayısı, yanıtlanan_sayısı) döndürür.
    pub fn quiz_score_for_lesson(&self, lesson: &Lesson) -> (usize, usize) {
        let mut correct = 0;
        let mut answered = 0;
        for q in &lesson.quiz {
            if let Some(chosen) = self.get_answer(lesson.id, q.number) {
                answered += 1;
                if q.is_correct(chosen) {
                    correct += 1;
                }
            }
        }
        (correct, answered)
    }

    /// Tamamlanan ders sayısı (0..=8).
    pub fn completed_count(&self) -> usize {
        self.completed_lessons.len()
    }

    /// İlerleme oranı (0.0 ..= 1.0).
    pub fn progress_ratio(&self) -> f32 {
        (self.completed_lessons.len() as f32 / 8.0).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_teacher_settings_flow() {
        let mut ts = TeacherSettings::default();
        assert_eq!(ts.level, StudentLevel::Beginner);
        assert_eq!(ts.completed_count(), 0);
        assert_eq!(ts.progress_ratio(), 0.0);

        ts.set_lesson_completed(1, true);
        ts.set_lesson_completed(3, true);
        assert!(ts.is_lesson_completed(1));
        assert!(ts.is_lesson_completed(3));
        assert!(!ts.is_lesson_completed(2));
        assert_eq!(ts.completed_count(), 2);
        assert_eq!(ts.progress_ratio(), 0.25);

        ts.set_answer(1, 1, 2);
        assert_eq!(ts.get_answer(1, 1), Some(2));
        assert_eq!(ts.get_answer(1, 2), None);

        ts.set_lesson_completed(1, false);
        assert!(!ts.is_lesson_completed(1));
        assert_eq!(ts.completed_count(), 1);
    }
}
