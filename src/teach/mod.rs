// SPDX-License-Identifier: AGPL-3.0-or-later
//! Öğretmen Modu: Seviye seçimi, sihirbaz "Bana öğret" dersleri,
//! etkileşimli mini testler, "Neden bu sonuç çıktı?" ve Sınıf Raporu.

pub mod explain;
pub mod lessons;
pub mod quiz;
pub mod report;
pub mod state;

pub use explain::{
    ResultMetric, StaticExplanation, build_socratic_teacher_prompts, explain_metric,
};
pub use lessons::{Lesson, all_lessons, lesson_for_id, lesson_for_step};
pub use quiz::{QuizQuestion, parse_quiz_from_markdown};
pub use report::generate_classroom_report;
pub use state::{StudentLevel, TeacherSettings};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Library, evaluate};
    use crate::i18n::Lang;
    use crate::project::Project;
    use crate::ui::wizard::StepId;

    #[test]
    fn test_teacher_mode_never_modifies_project() {
        let initial_project = Project::default();
        let p = initial_project.clone();
        let lib = Library::embedded();
        let m = evaluate(&p, &lib);

        // 1. Ayarlar ve seviye manipülasyonları
        let mut teacher = TeacherSettings::default();
        teacher.level = StudentLevel::Advanced;
        teacher.student_name = "Ahmet Yılmaz".to_string();

        // 2. Ders tamamlama ve quiz çözümü
        for lesson_id in 1..=8 {
            teacher.set_lesson_completed(lesson_id, true);
            teacher.set_answer(lesson_id, 1, 1);
            teacher.set_answer(lesson_id, 2, 2);
            teacher.set_answer(lesson_id, 3, 0);
        }

        // 3. Adım eşlemeleri ve ders okuma
        for step in [
            StepId::Mission,
            StepId::Weight,
            StepId::Geometry,
            StepId::Aero,
            StepId::DesignPoint,
            StepId::Endurance,
            StepId::Propulsion,
            StepId::Stability,
            StepId::Structure,
            StepId::MrPropulsion,
            StepId::MrCG,
            StepId::MrStructure,
        ] {
            let lid = lesson_for_step(step, p.drone_type);
            assert!((1..=8).contains(&lid));
            let lesson = lesson_for_id(lid, Lang::Tr).unwrap();
            assert!(!lesson.title.is_empty());
        }

        // 4. "Neden bu sonuç çıktı?" sorgulamaları
        for metric in [
            ResultMetric::VStall,
            ResultMetric::LdMax,
            ResultMetric::StaticMargin,
            ResultMetric::HoverThrottle,
            ResultMetric::DiskLoading,
            ResultMetric::FlightTime,
            ResultMetric::WingLoading,
            ResultMetric::SparStress,
        ] {
            let expl = explain_metric(metric, &p, &m, teacher.level, Lang::Tr);
            assert!(!expl.formula.is_empty());

            let (sys, user) =
                build_socratic_teacher_prompts(metric, &p, &m, teacher.level, Lang::Tr);
            assert!(sys.contains("\"changes\": []"));
            assert!(!user.is_empty());
        }

        // 5. Sınıf raporu üretimi
        let tr_report = generate_classroom_report(&p, &m, &teacher, Lang::Tr);
        let en_report = generate_classroom_report(&p, &m, &teacher, Lang::En);
        assert!(tr_report.contains("Ahmet Yılmaz"));
        assert!(en_report.contains("Ahmet Yılmaz"));

        // Kural 5: Proje hiçbir şekilde değişmemiş olmalıdır
        let p_json = serde_json::to_string(&p).unwrap();
        let initial_json = serde_json::to_string(&initial_project).unwrap();
        assert_eq!(
            p_json, initial_json,
            "Kural 5 İhlali: Öğretmen modu işlemleri projeyi değiştirmiş!"
        );
    }
}
