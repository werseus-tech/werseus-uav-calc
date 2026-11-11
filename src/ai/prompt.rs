// SPDX-License-Identifier: AGPL-3.0-or-later
//! Yapay zeka asistanı için sistem istemi ve kullanıcı istemleri oluşturucu (TR + EN).

use crate::ai::fields::generate_schema_prompt;
use crate::calc::Model;
use crate::i18n::{Lang, t_lang};
use crate::project::Project;

/// Sistem istemini oluşturur. Modele mühendislik rolünü, şemayı ve katı JSON formatını iletir.
pub fn build_system_prompt(project: &Project, lang: Lang) -> String {
    let schema_text = generate_schema_prompt(project);
    match lang {
        Lang::Tr => format!(
            "Sen 'WerSeuS – UAV Calc' İHA ön tasarım yazılımının kıdemli havacılık mühendisliği asistanısın.\n\
             Görevin, kullanıcının tasarım hedeflerini, kısıtlarını ve uyarılarını analiz ederek aerodinamik, ağırlık, itki ve stabilite açısından tutarlı mühendislik önerileri sunmaktır.\n\n\
             {}\n\n\
             KURALLAR VE ÇIKTI FORMATI:\n\
             1. Yalnızca yukarıdaki 'İzin Verilen Alanlar' listesinde tanımlı alanları önerebilirsin. Tanınmayan alanlar otomatik reddedilecektir.\n\
             2. Önerdiğin sayısal değerler kesinlikle belirtilen [min .. max] aralığında ve SI birimlerinde olmalıdır.\n\
             3. Yanıtın YALNIZCA geçerli bir JSON nesnesi olmalıdır. JSON dışında hiçbir metin, markdown açıklaması veya önsöz YAZMA.\n\
             4. JSON formatı tam olarak şu yapıda olmalıdır:\n\
             {{\n  \"explanation\": \"Önerilen tasarım stratejisinin ve kararların Türkçe mühendislik gerekçesi.\",\n  \"changes\": [\n    {{\"field\": \"alan_adi\", \"value\": 12.34, \"reason\": \"Değişiklik gerekçesi\"}}\n  ]\n}}\n",
            schema_text
        ),
        Lang::En => format!(
            "You are the senior aeronautical engineering assistant of the 'WerSeuS – UAV Calc' conceptual UAV design suite.\n\
             Your task is to analyze user mission objectives, constraints, and warnings to formulate aerodynamically, structurally, and propulsion-wise sound engineering recommendations.\n\n\
             {}\n\n\
             RULES AND OUTPUT FORMAT:\n\
             1. You may ONLY propose changes for fields listed in 'Allowed Fields' above. Unrecognized fields are strictly rejected.\n\
             2. Numerical values MUST strictly lie within the specified [min .. max] range in SI units.\n\
             3. Your output MUST be ONLY a single valid JSON object. Do NOT include any markdown preamble, postscript, or outside chatter.\n\
             4. The JSON format must strictly adhere to:\n\
             {{\n  \"explanation\": \"Engineering rationale for proposed design and trade-offs.\",\n  \"changes\": [\n    {{\"field\": \"field_id\", \"value\": 12.34, \"reason\": \"Engineering reason\"}}\n  ]\n}}\n",
            schema_text
        ),
    }
}

/// 'Hızlı Tasarım' istemini oluşturur.
pub fn build_quick_design_prompt(user_text: &str, lang: Lang) -> String {
    match lang {
        Lang::Tr => format!(
            "GÖREV: Kullanıcı aşağıdaki doğal dil gereksinimleriyle yeni bir İHA tasarlamak istiyor:\n\
             \"{}\"\n\n\
             Lütfen bu görev için en uygun drone_type'ı seç, gerekli kanat açıklığı (b), alan (s), faydalı yük (m_payload), seyir hızı (v_cr), batarya hücre konfigürasyonu (n_series, n_parallel) ve itki parametrelerini belirle ve JSON formatında döndür.",
            user_text
        ),
        Lang::En => format!(
            "TASK: The user wants to design a new UAV with the following natural language requirements:\n\
             \"{}\"\n\n\
             Please select the best suited drone_type, appropriate wing sizing (b, s), payload (m_payload), cruise speed (v_cr), battery cell layout (n_series, n_parallel), and propulsion parameters, returning them in the specified JSON format.",
            user_text
        ),
    }
}

/// 'Bu Adımı Açıkla / Yorumla' istemini oluşturur.
pub fn build_explain_step_prompt(
    project: &Project,
    model: &Model,
    step_name: &str,
    lang: Lang,
) -> String {
    let mut warnings_summary = String::new();
    for w in &model.warnings {
        warnings_summary.push_str(&format!("- [{}] {}\n", w.area_name(), w.text()));
    }
    if warnings_summary.is_empty() {
        warnings_summary = t_lang("summary.all_clear", lang).to_string();
    }

    match lang {
        Lang::Tr => format!(
            "GÖREV: Kullanıcı mevcut adım olan '{}' adımındaki parametreleri ve sonuçları inceliyor.\n\
             Mevcut Platform: {:?}, MTOW: {:.2} kg, Seyir Hızı: {:.1} m/s\n\
             Aktif Uyarılar:\n{}\n\n\
             Lütfen bu adımdaki tasarım durumunu sade bir mühendislik diliyle yorumla, olası darboğazları açıkla ve gerekirse performansı veya güvenliği artıracak parametre değişiklikleri öner (JSON formatında).",
            step_name,
            project.drone_type,
            if project.drone_type == crate::project::DroneType::Multirotor {
                model.mr.m_mtow
            } else {
                model.weight.m_mtow
            },
            project.v_cr,
            warnings_summary
        ),
        Lang::En => format!(
            "TASK: The user is inspecting the current design step '{}'.\n\
             Current Platform: {:?}, MTOW: {:.2} kg, Cruise Speed: {:.1} m/s\n\
             Active Warnings:\n{}\n\n\
             Please evaluate this design state in clear engineering terms, explain any bottlenecks or risks, and propose parameter changes to improve performance or safety (in JSON format).",
            step_name,
            project.drone_type,
            if project.drone_type == crate::project::DroneType::Multirotor {
                model.mr.m_mtow
            } else {
                model.weight.m_mtow
            },
            project.v_cr,
            warnings_summary
        ),
    }
}

/// 'Uyarıyı Düzelt' istemini oluşturur.
pub fn build_fix_warning_prompt(
    _project: &Project,
    _model: &Model,
    warning_text: &str,
    lang: Lang,
) -> String {
    match lang {
        Lang::Tr => format!(
            "GÖREV: Tasarımda şu mühendislik uyarısı tespit edildi:\n\
             \"{}\"\n\n\
             Lütfen bu uyarıyı gidermek için gerekli parametre değişikliklerini gerekçeleriyle birlikte öner ve JSON formatında döndür.",
            warning_text
        ),
        Lang::En => format!(
            "TASK: The following engineering warning was flagged in the design:\n\
             \"{}\"\n\n\
             Please suggest parameter modifications to safely resolve this warning, explaining your rationale in the specified JSON format.",
            warning_text
        ),
    }
}
