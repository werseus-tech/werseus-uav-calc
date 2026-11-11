// SPDX-License-Identifier: AGPL-3.0-or-later
//! Öğretmene teslim edilecek Sınıf Raporu (Classroom Report) Markdown üreticisi.
//! Tamamlanan dersler, mini test sonuçları ve İHA tasarım özetini içerir.
//! Kişisel veri (öğrenci adı vb.) yalnızca kullanıcı elle girerse eklenir; varsayılan anonimdir.

use crate::calc::Model;
use crate::i18n::Lang;
use crate::project::{DroneType, Project};
use crate::teach::lessons::all_lessons;
use crate::teach::quiz::QuizQuestion;
use crate::teach::state::TeacherSettings;

/// Öğretmene teslim edilecek Markdown sınıf raporunu oluşturur.
pub fn generate_classroom_report(
    project: &Project,
    model: &Model,
    teacher: &TeacherSettings,
    lang: Lang,
) -> String {
    let mut md = String::with_capacity(8192);

    let app_version = env!("CARGO_PKG_VERSION");
    let student_display = if teacher.student_name.trim().is_empty() {
        match lang {
            Lang::Tr => "Belirtilmedi (Anonim Öğrenci)",
            Lang::En => "Unspecified (Anonymous Student)",
        }
    } else {
        teacher.student_name.trim()
    };

    let level_str = teacher.level.label(lang);
    let platform_str = project.drone_type.label();

    // 1. Başlık ve Öğrenci Üstbilgisi
    match lang {
        Lang::Tr => {
            md.push_str("# WerSeuS – UAV Calc: Öğrenci Tasarım & Ders Tamamlama Raporu\n\n");
            md.push_str(&format!(
                "**Üretici**: WerSeuS – UAV Calc v{app_version}\n\n"
            ));
            md.push_str("| Bilgi Alanı | Değer |\n|---|---|\n");
            md.push_str(&format!("| **Öğrenci Adı / No** | {student_display} |\n"));
            md.push_str(&format!("| **Öğrenci Seviyesi** | {level_str} |\n"));
            md.push_str(&format!("| **Tasarım Projesi** | {} |\n", project.name));
            md.push_str(&format!("| **Platform Türü** | {platform_str} |\n"));
            md.push_str(&format!(
                "| **Ders Tamamlama Oranı** | {} / 8 (%{:.0}) |\n\n---\n\n",
                teacher.completed_count(),
                teacher.progress_ratio() * 100.0
            ));
        }
        Lang::En => {
            md.push_str("# WerSeuS – UAV Calc: Student Design & Course Completion Report\n\n");
            md.push_str(&format!(
                "**Generator**: WerSeuS – UAV Calc v{app_version}\n\n"
            ));
            md.push_str("| Metadata Field | Value |\n|---|---|\n");
            md.push_str(&format!("| **Student Name / ID** | {student_display} |\n"));
            md.push_str(&format!("| **Student Level** | {level_str} |\n"));
            md.push_str(&format!("| **Design Project** | {} |\n", project.name));
            md.push_str(&format!("| **Platform Type** | {platform_str} |\n"));
            md.push_str(&format!(
                "| **Course Completion** | {} / 8 ({:.0}%) |\n\n---\n\n",
                teacher.completed_count(),
                teacher.progress_ratio() * 100.0
            ));
        }
    }

    // 2. Bölüm: Ders Yolu ve İlerleme Tablosu
    let lessons = all_lessons(lang);
    match lang {
        Lang::Tr => {
            md.push_str("## 1. Ders Yolu İlerlemesi\n\n");
            md.push_str("| No | Ders Başlığı | Durum | Mini Test Başarısı |\n|---|---|---|---|\n");
            for l in &lessons {
                let status_icon = if teacher.is_lesson_completed(l.id) {
                    "[x] Tamamlandı"
                } else {
                    "[ ] Devam Ediyor"
                };
                let (correct, answered) = teacher.quiz_score_for_lesson(l);
                let score_str = if answered == 0 {
                    "Çözülmedi".to_string()
                } else {
                    format!("{correct} / 3 Doğru")
                };
                md.push_str(&format!(
                    "| {} | {} | {} | {} |\n",
                    l.id, l.title, status_icon, score_str
                ));
            }
            md.push_str("\n---\n\n");
        }
        Lang::En => {
            md.push_str("## 1. Learning Path Progress\n\n");
            md.push_str("| No | Lesson Title | Status | Mini Quiz Score |\n|---|---|---|---|\n");
            for l in &lessons {
                let status_icon = if teacher.is_lesson_completed(l.id) {
                    "[x] Completed"
                } else {
                    "[ ] In Progress"
                };
                let (correct, answered) = teacher.quiz_score_for_lesson(l);
                let score_str = if answered == 0 {
                    "Not taken".to_string()
                } else {
                    format!("{correct} / 3 Correct")
                };
                md.push_str(&format!(
                    "| {} | {} | {} | {} |\n",
                    l.id, l.title, status_icon, score_str
                ));
            }
            md.push_str("\n---\n\n");
        }
    }

    // 3. Bölüm: Mini Test Soru ve Yanıt Detayları
    match lang {
        Lang::Tr => {
            md.push_str("## 2. Mini Test Soru ve Yanıt Dökümü\n\n");
        }
        Lang::En => {
            md.push_str("## 2. Mini Quiz Question & Answer Details\n\n");
        }
    }

    for l in &lessons {
        let (correct, answered) = teacher.quiz_score_for_lesson(l);
        if answered == 0 {
            continue;
        }

        match lang {
            Lang::Tr => md.push_str(&format!(
                "### Ders {}: {} (Puan: {}/3)\n\n",
                l.id, l.title, correct
            )),
            Lang::En => md.push_str(&format!(
                "### Lesson {}: {} (Score: {}/3)\n\n",
                l.id, l.title, correct
            )),
        }

        for q in &l.quiz {
            if let Some(chosen_idx) = teacher.get_answer(l.id, q.number) {
                let is_ok = q.is_correct(chosen_idx);
                let mark = if is_ok {
                    "✓ (Doğru)"
                } else {
                    "✗ (Yanlış)"
                };
                let chosen_opt_text = q.options.get(chosen_idx).map(|s| s.as_str()).unwrap_or("—");
                let correct_opt_text = q
                    .options
                    .get(q.correct_index)
                    .map(|s| s.as_str())
                    .unwrap_or("—");

                md.push_str(&format!("**Soru {}.** {}\n\n", q.number, q.question));
                md.push_str(&format!(
                    "- **Öğrenci Yanıtı:** {}) {} — {}\n",
                    QuizQuestion::option_letter(chosen_idx),
                    chosen_opt_text,
                    mark
                ));
                if !is_ok {
                    md.push_str(&format!(
                        "- **Doğru Yanıt:** {}) {}\n",
                        QuizQuestion::option_letter(q.correct_index),
                        correct_opt_text
                    ));
                }
                md.push_str(&format!("- **Çözüm Açıklaması:** {}\n\n", q.explanation));
            }
        }
    }
    md.push_str("---\n\n");

    // 4. Bölüm: Öğrencinin Tasarım Projesi Özet Sonuçları
    match lang {
        Lang::Tr => {
            md.push_str("## 3. İHA Tasarım Projesi Özet Sonuçları\n\n");
            md.push_str("| Parametre / Sonuç | Değer | Birim |\n|---|---|---|\n");
        }
        Lang::En => {
            md.push_str("## 3. UAV Conceptual Design Results Summary\n\n");
            md.push_str("| Metric / Result | Value | Unit |\n|---|---|---|\n");
        }
    }

    let mtow = if project.drone_type == DroneType::Multirotor {
        model.mr.m_mtow
    } else {
        model.weight.m_mtow
    };
    let m_empty = (mtow - model.weight.m_battery - project.m_payload).max(0.0);

    match lang {
        Lang::Tr => {
            md.push_str(&format!("| Kalkış Kütlesi (MTOW) | {:.2} | kg |\n", mtow));
            md.push_str(&format!("| Boş Yapı Kütlesi | {:.2} | kg |\n", m_empty));
            md.push_str(&format!(
                "| Faydalı Yük Kütlesi | {:.2} | kg |\n",
                project.m_payload
            ));
            md.push_str(&format!(
                "| Batarya Kütlesi | {:.2} | kg |\n",
                model.weight.m_battery
            ));
            md.push_str(&format!(
                "| Batarya Enerjisi | {:.1} | Wh |\n",
                model.weight.pack_wh
            ));

            if project.drone_type != DroneType::Multirotor {
                md.push_str(&format!("| Kanat Açıklığı (b) | {:.2} | m |\n", project.b));
                md.push_str(&format!("| Kanat Alanı (S) | {:.3} | m² |\n", project.s));
                md.push_str(&format!(
                    "| Kanat Açıklık Oranı (AR) | {:.1} | — |\n",
                    model.wing.ar
                ));
                md.push_str(&format!(
                    "| Tutunma Hızı (V_stall) | {:.2} | m/s |\n",
                    model.aero.v_stall
                ));
                md.push_str(&format!(
                    "| Seyir Hızı (V_cr) | {:.1} | m/s |\n",
                    project.v_cr
                ));
                md.push_str(&format!(
                    "| Azami Süzülme Oranı (L/D_max) | {:.1} | — |\n",
                    model.aero.ld_max
                ));
                md.push_str(&format!(
                    "| Seyir Süresi | {:.1} | dk |\n",
                    model.perf_range.endurance_min()
                ));
                md.push_str(&format!(
                    "| Seyir Menzili | {:.1} | km |\n",
                    model.perf_range.range_m / 1000.0
                ));
                md.push_str(&format!(
                    "| Statik Marj (SM) | %{:.1} | — |\n",
                    model.stab.sm * 100.0
                ));
            } else {
                let frame_size_mm = project.mr_frame_size * 1000.0;
                let prop_dia_in = project.mr_d_prop / 0.0254;
                md.push_str(&format!("| Çerçeve Çapı | {:.0} | mm |\n", frame_size_mm));
                md.push_str(&format!(
                    "| Kol Sayısı | {} | adet |\n",
                    project.mr_arm_count
                ));
                md.push_str(&format!("| Pervane Çapı | {:.1} | inç |\n", prop_dia_in));
                md.push_str(&format!(
                    "| İtki/Ağırlık Oranı (T/W) | {:.2} | — |\n",
                    model.mr.tw_ratio
                ));
                md.push_str(&format!(
                    "| Askı Gazı | %{:.1} | — |\n",
                    model.mr.hover_throttle * 100.0
                ));
                md.push_str(&format!(
                    "| Disk Yüklemesi (DL) | {:.1} | N/m² |\n",
                    model.mr.disk_loading
                ));
                md.push_str(&format!(
                    "| Askıda Kalma Süresi | {:.1} | dk |\n",
                    model.mr.hover_endurance_min
                ));
            }
            md.push_str("\n---\n\n");
            md.push_str("## 4. Öğretmen Notu ve Değerlendirmesi\n\n");
            md.push_str("| Değerlendirme Ölçütü | Puan / Not |\n|---|---|\n");
            md.push_str("| Ders ve Mini Test Başarısı |       / 50 |\n");
            md.push_str("| Tasarım Tutarlılığı ve Kısıt Uyumu |       / 50 |\n");
            md.push_str("| **Toplam Başarı Puanı** |       / 100 |\n\n");
            md.push_str(
                "**Öğretmen Görüşü ve İmzası:**\n\n\n\n_____________________________________\n",
            );
        }
        Lang::En => {
            md.push_str(&format!("| Takeoff Mass (MTOW) | {:.2} | kg |\n", mtow));
            md.push_str(&format!(
                "| Empty Structural Mass | {:.2} | kg |\n",
                m_empty
            ));
            md.push_str(&format!(
                "| Payload Mass | {:.2} | kg |\n",
                project.m_payload
            ));
            md.push_str(&format!(
                "| Battery Mass | {:.2} | kg |\n",
                model.weight.m_battery
            ));
            md.push_str(&format!(
                "| Battery Energy | {:.1} | Wh |\n",
                model.weight.pack_wh
            ));

            if project.drone_type != DroneType::Multirotor {
                md.push_str(&format!("| Wingspan (b) | {:.2} | m |\n", project.b));
                md.push_str(&format!("| Wing Area (S) | {:.3} | m² |\n", project.s));
                md.push_str(&format!(
                    "| Aspect Ratio (AR) | {:.1} | — |\n",
                    model.wing.ar
                ));
                md.push_str(&format!(
                    "| Stall Speed (V_stall) | {:.2} | m/s |\n",
                    model.aero.v_stall
                ));
                md.push_str(&format!(
                    "| Cruise Speed (V_cr) | {:.1} | m/s |\n",
                    project.v_cr
                ));
                md.push_str(&format!(
                    "| Max Glide Ratio (L/D_max) | {:.1} | — |\n",
                    model.aero.ld_max
                ));
                md.push_str(&format!(
                    "| Cruise Endurance | {:.1} | min |\n",
                    model.perf_range.endurance_min()
                ));
                md.push_str(&format!(
                    "| Cruise Range | {:.1} | km |\n",
                    model.perf_range.range_m / 1000.0
                ));
                md.push_str(&format!(
                    "| Static Margin (SM) | {:.1}% | — |\n",
                    model.stab.sm * 100.0
                ));
            } else {
                let frame_size_mm = project.mr_frame_size * 1000.0;
                let prop_dia_in = project.mr_d_prop / 0.0254;
                md.push_str(&format!("| Frame Diagonal | {:.0} | mm |\n", frame_size_mm));
                md.push_str(&format!(
                    "| Arm Count | {} | rotors |\n",
                    project.mr_arm_count
                ));
                md.push_str(&format!(
                    "| Propeller Diameter | {:.1} | in |\n",
                    prop_dia_in
                ));
                md.push_str(&format!(
                    "| Thrust-to-Weight (T/W) | {:.2} | — |\n",
                    model.mr.tw_ratio
                ));
                md.push_str(&format!(
                    "| Hover Throttle | {:.1}% | — |\n",
                    model.mr.hover_throttle * 100.0
                ));
                md.push_str(&format!(
                    "| Disk Loading (DL) | {:.1} | N/m² |\n",
                    model.mr.disk_loading
                ));
                md.push_str(&format!(
                    "| Hover Endurance | {:.1} | min |\n",
                    model.mr.hover_endurance_min
                ));
            }
            md.push_str("\n---\n\n");
            md.push_str("## 4. Instructor Evaluation & Sign-off\n\n");
            md.push_str("| Evaluation Criteria | Points |\n|---|---|\n");
            md.push_str("| Quiz & Theory Mastery |       / 50 |\n");
            md.push_str("| Design Viability & Sizing Consistency |       / 50 |\n");
            md.push_str("| **Total Grade** |       / 100 |\n\n");
            md.push_str("**Instructor Feedback & Signature:**\n\n\n\n_____________________________________\n");
        }
    }

    md
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Library, evaluate};

    #[test]
    fn test_generate_classroom_report_flow() {
        let p = Project::default();
        let lib = Library::embedded();
        let m = evaluate(&p, &lib);

        let mut teacher = TeacherSettings::default();
        teacher.student_name = "Ali Veli".to_string();
        teacher.set_lesson_completed(1, true);
        teacher.set_answer(1, 1, 1); // B is correct
        teacher.set_answer(1, 2, 2); // C is correct

        for lang in [Lang::Tr, Lang::En] {
            let report = generate_classroom_report(&p, &m, &teacher, lang);
            assert!(report.contains("Ali Veli"));
            assert!(report.contains("WerSeuS – UAV Calc"));
            assert!(report.contains("Ders 1") || report.contains("Lesson 1"));
            assert!(report.contains("MTOW"));
            assert!(report.contains("1 / 8"));
        }
    }

    #[test]
    fn test_generate_classroom_report_default_anonymous() {
        let p = Project::default();
        let lib = Library::embedded();
        let m = evaluate(&p, &lib);
        let teacher = TeacherSettings::default();

        let tr_rep = generate_classroom_report(&p, &m, &teacher, Lang::Tr);
        assert!(tr_rep.contains("Belirtilmedi (Anonim"));

        let en_rep = generate_classroom_report(&p, &m, &teacher, Lang::En);
        assert!(en_rep.contains("Unspecified (Anonymous"));
    }
}
