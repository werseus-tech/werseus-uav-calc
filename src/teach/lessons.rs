// SPDX-License-Identifier: AGPL-3.0-or-later
//! Gömülü ders içerikleri (include_str!), ders ayrıştırma ve adım-ders eşleme motoru.

use crate::i18n::Lang;
use crate::project::DroneType;
use crate::teach::quiz::{QuizQuestion, parse_quiz_from_markdown};
use crate::ui::wizard::StepId;

// 16 doğrulanmış ders dosyasının include_str! ile ikiliye gömülmesi (çevrimdışı çalışır)
pub const DERS01_TR: &str = include_str!("../../assets/lessons/ders01.tr.md");
pub const DERS01_EN: &str = include_str!("../../assets/lessons/ders01.en.md");
pub const DERS02_TR: &str = include_str!("../../assets/lessons/ders02.tr.md");
pub const DERS02_EN: &str = include_str!("../../assets/lessons/ders02.en.md");
pub const DERS03_TR: &str = include_str!("../../assets/lessons/ders03.tr.md");
pub const DERS03_EN: &str = include_str!("../../assets/lessons/ders03.en.md");
pub const DERS04_TR: &str = include_str!("../../assets/lessons/ders04.tr.md");
pub const DERS04_EN: &str = include_str!("../../assets/lessons/ders04.en.md");
pub const DERS05_TR: &str = include_str!("../../assets/lessons/ders05.tr.md");
pub const DERS05_EN: &str = include_str!("../../assets/lessons/ders05.en.md");
pub const DERS06_TR: &str = include_str!("../../assets/lessons/ders06.tr.md");
pub const DERS06_EN: &str = include_str!("../../assets/lessons/ders06.en.md");
pub const DERS07_TR: &str = include_str!("../../assets/lessons/ders07.tr.md");
pub const DERS07_EN: &str = include_str!("../../assets/lessons/ders07.en.md");
pub const DERS08_TR: &str = include_str!("../../assets/lessons/ders08.tr.md");
pub const DERS08_EN: &str = include_str!("../../assets/lessons/ders08.en.md");

/// Yapılandırılmış ders nesnesi.
#[derive(Clone, Debug, PartialEq)]
pub struct Lesson {
    pub id: u8,
    pub title: String,
    pub what_you_will_learn: Vec<String>,
    pub explanation: String,
    pub where_in_program: String,
    pub mini_experiment: String,
    pub misconceptions: Vec<String>,
    pub quiz: Vec<QuizQuestion>,
    pub raw_markdown: &'static str,
}

/// Sihirbaz adımına göre önerilen ders numarasını döndürür (1..=8).
pub fn lesson_for_step(step: StepId, dt: DroneType) -> u8 {
    match step {
        StepId::Mission => 1,
        StepId::Weight => 4,
        StepId::Geometry => 2,
        StepId::Aero => 1,
        StepId::DesignPoint => 6,
        StepId::Endurance => 4,
        StepId::Propulsion => 7,
        StepId::Stability => 3,
        StepId::Structure => 8,
        StepId::Summary | StepId::Outputs => {
            if dt == DroneType::Multirotor {
                5
            } else {
                6
            }
        }
        StepId::MrFrame => 5,
        StepId::MrPropulsion => 5,
        StepId::MrBatteryLoiter => 4,
        StepId::MrCG => 3,
        StepId::MrStructure => 8,
    }
}

/// Ham markdown metninden başlık ve alt bölümleri ayrıştırır.
pub fn parse_lesson(id: u8, raw: &'static str) -> Lesson {
    let mut title = format!("Ders {id}");
    let mut what_you_will_learn = Vec::new();
    let mut explanation = String::new();
    let mut where_in_program = String::new();
    let mut mini_experiment = String::new();
    let mut misconceptions = Vec::new();

    #[derive(Copy, Clone, PartialEq, Eq)]
    enum Section {
        None,
        WhatLearn,
        Explanation,
        WhereInProgram,
        MiniExp,
        Misconceptions,
        Quiz,
    }

    let mut current_sec = Section::None;

    for line in raw.lines() {
        let trimmed = line.trim();

        // Başlık: `# Ders X: ...` veya `# Lesson X: ...`
        if trimmed.starts_with("# ") {
            title = trimmed
                .strip_prefix("# ")
                .unwrap_or(trimmed)
                .trim()
                .to_string();
            current_sec = Section::None;
            continue;
        }

        // Alt bölümler: `### ...`
        if trimmed.starts_with("### ") {
            let sec_name = trimmed
                .strip_prefix("### ")
                .unwrap_or(trimmed)
                .trim()
                .to_lowercase();
            if sec_name.contains("ne öğreneceksin") || sec_name.contains("what you will learn") {
                current_sec = Section::WhatLearn;
            } else if sec_name.contains("anlatım")
                || sec_name.contains("explanation")
                || sec_name.contains("core concepts")
            {
                current_sec = Section::Explanation;
            } else if sec_name.contains("programda nerede") || sec_name.contains("where you see") {
                current_sec = Section::WhereInProgram;
            } else if sec_name.contains("mini deney") || sec_name.contains("mini experiment") {
                current_sec = Section::MiniExp;
            } else if sec_name.contains("sık yapılan")
                || sec_name.contains("yanlışlar")
                || sec_name.contains("misconceptions")
            {
                current_sec = Section::Misconceptions;
            } else if sec_name.contains("mini test") || sec_name.contains("mini quiz") {
                current_sec = Section::Quiz;
            } else {
                current_sec = Section::None;
            }
            continue;
        }

        match current_sec {
            Section::WhatLearn => {
                if trimmed.starts_with('-') || trimmed.starts_with('*') {
                    let point = trimmed[1..].trim().to_string();
                    if !point.is_empty() {
                        what_you_will_learn.push(point);
                    }
                }
            }
            Section::Explanation => {
                if !trimmed.is_empty() {
                    if !explanation.is_empty() {
                        explanation.push_str("\n\n");
                    }
                    explanation.push_str(trimmed);
                }
            }
            Section::WhereInProgram => {
                if !trimmed.is_empty() {
                    if !where_in_program.is_empty() {
                        where_in_program.push('\n');
                    }
                    where_in_program.push_str(trimmed);
                }
            }
            Section::MiniExp => {
                if !trimmed.is_empty() {
                    if !mini_experiment.is_empty() {
                        mini_experiment.push('\n');
                    }
                    mini_experiment.push_str(trimmed);
                }
            }
            Section::Misconceptions => {
                if !trimmed.is_empty() {
                    misconceptions.push(trimmed.to_string());
                }
            }
            Section::None | Section::Quiz => {}
        }
    }

    let quiz = parse_quiz_from_markdown(raw);

    Lesson {
        id,
        title,
        what_you_will_learn,
        explanation,
        where_in_program,
        mini_experiment,
        misconceptions,
        quiz,
        raw_markdown: raw,
    }
}

/// Dilediğiniz dildeki tüm 8 dersi yapılandırılmış liste olarak döndürür.
pub fn all_lessons(lang: Lang) -> Vec<Lesson> {
    let raw_sources = match lang {
        Lang::Tr => [
            (1, DERS01_TR),
            (2, DERS02_TR),
            (3, DERS03_TR),
            (4, DERS04_TR),
            (5, DERS05_TR),
            (6, DERS06_TR),
            (7, DERS07_TR),
            (8, DERS08_TR),
        ],
        Lang::En => [
            (1, DERS01_EN),
            (2, DERS02_EN),
            (3, DERS03_EN),
            (4, DERS04_EN),
            (5, DERS05_EN),
            (6, DERS06_EN),
            (7, DERS07_EN),
            (8, DERS08_EN),
        ],
    };

    raw_sources
        .into_iter()
        .map(|(id, raw)| parse_lesson(id, raw))
        .collect()
}

/// Belirtilen numara ve dildeki dersi döndürür.
pub fn lesson_for_id(id: u8, lang: Lang) -> Option<Lesson> {
    let raw = match (id, lang) {
        (1, Lang::Tr) => DERS01_TR,
        (1, Lang::En) => DERS01_EN,
        (2, Lang::Tr) => DERS02_TR,
        (2, Lang::En) => DERS02_EN,
        (3, Lang::Tr) => DERS03_TR,
        (3, Lang::En) => DERS03_EN,
        (4, Lang::Tr) => DERS04_TR,
        (4, Lang::En) => DERS04_EN,
        (5, Lang::Tr) => DERS05_TR,
        (5, Lang::En) => DERS05_EN,
        (6, Lang::Tr) => DERS06_TR,
        (6, Lang::En) => DERS06_EN,
        (7, Lang::Tr) => DERS07_TR,
        (7, Lang::En) => DERS07_EN,
        (8, Lang::Tr) => DERS08_TR,
        (8, Lang::En) => DERS08_EN,
        _ => return None,
    };

    Some(parse_lesson(id, raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_16_lessons_parse_successfully() {
        for lang in [Lang::Tr, Lang::En] {
            let lessons = all_lessons(lang);
            assert_eq!(lessons.len(), 8, "Her dilde tam 8 ders bulunmalıdır");

            for l in lessons {
                assert!(l.id >= 1 && l.id <= 8);
                assert!(!l.title.is_empty(), "Ders {} başlığı boş olamaz", l.id);
                assert!(
                    !l.what_you_will_learn.is_empty(),
                    "Ders {} 'Ne öğreneceksin' boş olamaz (dil: {:?})",
                    l.id,
                    lang
                );
                assert!(
                    !l.explanation.is_empty(),
                    "Ders {} anlatımı boş olamaz (dil: {:?})",
                    l.id,
                    lang
                );
                assert!(
                    !l.where_in_program.is_empty(),
                    "Ders {} 'Programda nerede görürsün' boş olamaz (dil: {:?})",
                    l.id,
                    lang
                );
                assert!(
                    !l.mini_experiment.is_empty(),
                    "Ders {} mini deneyi boş olamaz (dil: {:?})",
                    l.id,
                    lang
                );
                assert_eq!(
                    l.quiz.len(),
                    3,
                    "Ders {} tam olarak 3 quiz sorusu içermelidir (dil: {:?})",
                    l.id,
                    lang
                );
                for q in &l.quiz {
                    assert_eq!(
                        q.options.len(),
                        4,
                        "Ders {} soru {} tam 4 şık içermelidir",
                        l.id,
                        q.number
                    );
                    assert!(
                        q.correct_index <= 3,
                        "Ders {} soru {} doğru cevap indeksi 0..=3 olmalıdır",
                        l.id,
                        q.number
                    );
                    assert!(
                        !q.explanation.is_empty(),
                        "Ders {} soru {} çözümü boş olamaz",
                        l.id,
                        q.number
                    );
                }
            }
        }
    }

    #[test]
    fn test_step_to_lesson_mapping() {
        assert_eq!(
            lesson_for_step(StepId::Mission, DroneType::FixedWingConventional),
            1
        );
        assert_eq!(
            lesson_for_step(StepId::Weight, DroneType::FixedWingConventional),
            4
        );
        assert_eq!(
            lesson_for_step(StepId::Geometry, DroneType::FixedWingConventional),
            2
        );
        assert_eq!(
            lesson_for_step(StepId::Aero, DroneType::FixedWingConventional),
            1
        );
        assert_eq!(
            lesson_for_step(StepId::DesignPoint, DroneType::FixedWingConventional),
            6
        );
        assert_eq!(
            lesson_for_step(StepId::Endurance, DroneType::FixedWingConventional),
            4
        );
        assert_eq!(
            lesson_for_step(StepId::Propulsion, DroneType::FixedWingConventional),
            7
        );
        assert_eq!(
            lesson_for_step(StepId::Stability, DroneType::FixedWingConventional),
            3
        );
        assert_eq!(
            lesson_for_step(StepId::Structure, DroneType::FixedWingConventional),
            8
        );
        assert_eq!(
            lesson_for_step(StepId::MrPropulsion, DroneType::Multirotor),
            5
        );
        assert_eq!(lesson_for_step(StepId::MrCG, DroneType::Multirotor), 3);
        assert_eq!(
            lesson_for_step(StepId::MrStructure, DroneType::Multirotor),
            8
        );
    }
}
