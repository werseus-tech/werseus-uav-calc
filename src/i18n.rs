// SPDX-License-Identifier: AGPL-3.0-or-later
//! Çok dilli arayüz altyapısı (i18n).
//! Metin tabloları assets/i18n/tr.json ve assets/i18n/en.json dosyalarından derleme anında gömülür.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Lang {
    #[default]
    Tr = 0,
    En = 1,
}

static CURRENT_LANG: AtomicU8 = AtomicU8::new(0);

static TR_JSON: &str = include_str!("../assets/i18n/tr.json");
static EN_JSON: &str = include_str!("../assets/i18n/en.json");

static TR_MAP: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
static EN_MAP: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();

fn parse_map(json_str: &'static str) -> HashMap<&'static str, &'static str> {
    let raw: HashMap<String, String> =
        serde_json::from_str(json_str).expect("i18n JSON parse error");
    let mut map = HashMap::with_capacity(raw.len());
    for (k, v) in raw {
        let k_static: &'static str = Box::leak(k.into_boxed_str());
        let v_static: &'static str = Box::leak(v.into_boxed_str());
        map.insert(k_static, v_static);
    }
    map
}

pub fn tr_table() -> &'static HashMap<&'static str, &'static str> {
    TR_MAP.get_or_init(|| parse_map(TR_JSON))
}

pub fn en_table() -> &'static HashMap<&'static str, &'static str> {
    EN_MAP.get_or_init(|| parse_map(EN_JSON))
}

thread_local! {
    /// Yalnız bu iş parçacığı için dil geçersiz kılma (testler genel dili değiştirmeden dil sınar).
    static LANG_OVERRIDE: std::cell::Cell<Option<Lang>> = const { std::cell::Cell::new(None) };
}

/// `f` çalışırken bu iş parçacığında dil `lang` olur; genel dil ve diğer iş parçacıkları etkilenmez
/// (paralel testlerde dil yarışını önler). Çıkışta (panikte de) eski değer geri yüklenir.
pub fn with_lang<R>(lang: Lang, f: impl FnOnce() -> R) -> R {
    struct Restore(Option<Lang>);
    impl Drop for Restore {
        fn drop(&mut self) {
            LANG_OVERRIDE.with(|c| c.set(self.0));
        }
    }
    let _restore = Restore(LANG_OVERRIDE.with(|c| c.replace(Some(lang))));
    f()
}

pub fn current_lang() -> Lang {
    if let Some(l) = LANG_OVERRIDE.with(|c| c.get()) {
        return l;
    }
    match CURRENT_LANG.load(Ordering::Relaxed) {
        1 => Lang::En,
        _ => Lang::Tr,
    }
}

pub fn set_lang_memory(lang: Lang) {
    let val = match lang {
        Lang::Tr => 0,
        Lang::En => 1,
    };
    CURRENT_LANG.store(val, Ordering::Relaxed);
}

pub fn set_lang(lang: Lang) {
    set_lang_memory(lang);
    let mut s = load_settings();
    s.lang = lang;
    save_settings(&s);
}

pub fn t(key: &'static str) -> &'static str {
    t_lang(key, current_lang())
}

pub fn t_lang(key: &'static str, lang: Lang) -> &'static str {
    let tr = tr_table();
    let en = en_table();

    match lang {
        Lang::En => {
            if let Some(&val) = en.get(key) {
                if !val.trim().is_empty() {
                    return val;
                }
            }
            // Boş veya eksik EN değeri için Türkçe metne fallback
            if let Some(&val) = tr.get(key) {
                return val;
            }
        }
        Lang::Tr => {
            if let Some(&val) = tr.get(key) {
                return val;
            }
        }
    }
    // Bilinmeyen anahtar paniklemek yerine anahtarın kendisini döndürür
    key
}

pub fn tf(key: &'static str, args: &[impl AsRef<str>]) -> String {
    tf_lang(key, current_lang(), args)
}

pub fn tf_lang(key: &'static str, lang: Lang, args: &[impl AsRef<str>]) -> String {
    let mut template = t_lang(key, lang).to_string();
    for (i, arg) in args.iter().enumerate() {
        let needle = format!("{{{}}}", i);
        if template.contains(&needle) {
            template = template.replace(&needle, arg.as_ref());
        } else if template.contains("{}") {
            template = template.replacen("{}", arg.as_ref(), 1);
        }
    }
    template
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default)]
    pub lang: Lang,
    #[serde(default)]
    pub freecad_cmd_path: Option<String>,
    #[serde(default)]
    pub ai_config: crate::ai::config::AiConfig,
    #[serde(default)]
    pub teacher: crate::teach::TeacherSettings,
    /// Arayüz teması (açık/koyu); eski ayar dosyalarında yoksa koyu.
    #[serde(default)]
    pub theme: crate::ui::theme::ThemeChoice,
    /// Arayüz ölçeği (0.8-1.5); eski ayar dosyalarında yoksa 1.0.
    #[serde(default = "default_ui_scale")]
    pub ui_scale: f32,
    /// 3D görünümde ızgara varsayılanı (eski dosyalarda açık).
    #[serde(default = "default_true")]
    pub view_grid: bool,
    /// 3D görünüm varsayılanı ortografik mi? (false = perspektif)
    #[serde(default)]
    pub view_ortho: bool,
    /// CAD üretimi süre sınırları (parça, montaj, toplam).
    #[serde(default)]
    pub cad_limits: crate::cad::CadLimits,
}

fn default_ui_scale() -> f32 {
    1.0
}

fn default_true() -> bool {
    true
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            lang: detect_os_lang(),
            freecad_cmd_path: None,
            ai_config: crate::ai::config::AiConfig::default(),
            teacher: crate::teach::TeacherSettings::default(),
            theme: crate::ui::theme::ThemeChoice::default(),
            ui_scale: default_ui_scale(),
            view_grid: true,
            view_ortho: false,
            cad_limits: crate::cad::CadLimits::default(),
        }
    }
}

static SETTINGS_PATH_OVERRIDE: std::sync::RwLock<Option<PathBuf>> = std::sync::RwLock::new(None);

pub fn set_custom_settings_path(path: Option<PathBuf>) {
    if let Ok(mut lock) = SETTINGS_PATH_OVERRIDE.write() {
        *lock = path;
    }
}

pub fn settings_path() -> Option<PathBuf> {
    if let Ok(lock) = SETTINGS_PATH_OVERRIDE.read() {
        if let Some(ref p) = *lock {
            return Some(p.clone());
        }
    }
    #[cfg(windows)]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(not(windows))]
    let base = std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config"));

    base.map(|p| p.join("WerSeuS").join("settings.json"))
}

pub fn load_settings() -> AppSettings {
    if let Some(path) = settings_path() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(settings) = serde_json::from_str::<AppSettings>(&content) {
                return settings;
            }
        }
    }
    let default_settings = AppSettings::default();
    save_settings(&default_settings);
    default_settings
}

pub fn save_settings(settings: &AppSettings) {
    if let Some(path) = settings_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(content) = serde_json::to_string_pretty(settings) {
            let _ = std::fs::write(&path, content);
        }
    }
}

pub fn init_i18n() {
    let settings = load_settings();
    let val = match settings.lang {
        Lang::Tr => 0,
        Lang::En => 1,
    };
    CURRENT_LANG.store(val, Ordering::Relaxed);
}

pub fn detect_os_lang() -> Lang {
    // 1. Windows işletim sistemi varsayılan UI dili (Windows API öncelikli)
    #[cfg(windows)]
    {
        unsafe extern "system" {
            fn GetUserDefaultUILanguage() -> u16;
        }
        let lcid = unsafe { GetUserDefaultUILanguage() };
        let primary = lcid & 0x03ff;
        if primary == 0x001f {
            return Lang::Tr;
        } else if primary == 0x0009 {
            return Lang::En;
        }
    }

    // 2. Ortam değişkenleri kontrolü (LANG, LC_ALL, LC_MESSAGES, LANGUAGE)
    for var in ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"] {
        if let Ok(val) = std::env::var(var) {
            let lower = val.to_lowercase();
            if lower.starts_with("tr") || lower.contains("turkish") {
                return Lang::Tr;
            } else if lower.starts_with("en") || lower.contains("english") {
                return Lang::En;
            }
        }
    }

    Lang::En
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count_format_specifiers(s: &str) -> usize {
        let mut count = 0;
        let mut in_brace = false;
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '{' {
                if chars.peek() == Some(&'{') {
                    chars.next(); // skip escaped {{
                } else {
                    in_brace = true;
                }
            } else if c == '}' && in_brace {
                count += 1;
                in_brace = false;
            }
        }
        count
    }

    #[test]
    fn test_i18n_table_completeness() {
        let tr = tr_table();
        let en = en_table();

        assert!(!tr.is_empty(), "tr.json boş olamaz");
        assert!(!en.is_empty(), "en.json boş olamaz");

        // TR ve EN anahtar kümelerinin tam eşitliği, boş değer olmaması ve format belirteci eşitliği
        assert_eq!(tr.len(), en.len(), "TR ve EN anahtar sayıları eşit olmalı");
        for (&key, &tr_val) in tr.iter() {
            let en_val = en
                .get(key)
                .unwrap_or_else(|| panic!("EN tablosunda '{}' anahtarı eksik!", key));
            assert!(
                !en_val.trim().is_empty(),
                "EN tablosunda '{}' anahtarının değeri boş olamaz!",
                key
            );
            let tr_fmt_count = count_format_specifiers(tr_val);
            let en_fmt_count = count_format_specifiers(en_val);
            assert_eq!(
                tr_fmt_count, en_fmt_count,
                "Format belirteci sayısı uyuşmuyor: '{}'! TR ({}): {:?}, EN ({}): {:?}",
                key, tr_fmt_count, tr_val, en_fmt_count, en_val
            );
        }
        for key in en.keys() {
            assert!(
                tr.contains_key(key),
                "TR tablosunda '{}' anahtarı eksik!",
                key
            );
        }
    }

    fn extract_keys_from_source(src: &str) -> Vec<&str> {
        let mut keys = Vec::new();
        let bytes = src.as_bytes();
        let n = bytes.len();
        let mut i = 0;

        let is_ident_char = |b: u8| b.is_ascii_alphanumeric() || b == b'_';

        while i < n {
            if (i == 0 || !is_ident_char(bytes[i - 1])) && is_ident_char(bytes[i]) {
                let start = i;
                while i < n && is_ident_char(bytes[i]) {
                    i += 1;
                }
                let ident = &src[start..i];

                if matches!(ident, "t" | "tf" | "t_lang" | "tf_lang") {
                    let mut j = i;
                    while j < n && bytes[j].is_ascii_whitespace() {
                        j += 1;
                    }
                    if j < n && bytes[j] == b'(' {
                        j += 1;
                        while j < n && bytes[j].is_ascii_whitespace() {
                            j += 1;
                        }
                        if j < n && bytes[j] == b'"' {
                            let str_start = j + 1;
                            let mut k = str_start;
                            while k < n && bytes[k] != b'"' {
                                if bytes[k] == b'\\' {
                                    k += 1;
                                }
                                k += 1;
                            }
                            if k < n {
                                let key = &src[str_start..k];
                                if !key.is_empty()
                                    && key
                                        .chars()
                                        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
                                {
                                    keys.push(key);
                                }
                                i = k + 1;
                                continue;
                            }
                        }
                    }
                } else if matches!(ident, "area_key" | "msg_key") {
                    let mut j = i;
                    while j < n && bytes[j].is_ascii_whitespace() {
                        j += 1;
                    }
                    if j < n && bytes[j] == b':' {
                        j += 1;
                        while j < n && bytes[j].is_ascii_whitespace() {
                            j += 1;
                        }
                        if j < n && bytes[j] == b'"' {
                            let str_start = j + 1;
                            let mut k = str_start;
                            while k < n && bytes[k] != b'"' {
                                if bytes[k] == b'\\' {
                                    k += 1;
                                }
                                k += 1;
                            }
                            if k < n {
                                let key = &src[str_start..k];
                                if !key.is_empty()
                                    && key
                                        .chars()
                                        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
                                {
                                    keys.push(key);
                                }
                                i = k + 1;
                                continue;
                            }
                        }
                    }
                }
            } else {
                i += 1;
            }
        }
        keys
    }

    #[test]
    fn test_all_code_keys_exist_in_tr_table() {
        const SOURCES: &[&str] = &[
            include_str!("main.rs"),
            include_str!("lib.rs"),
            include_str!("project.rs"),
            include_str!("calc/aero.rs"),
            include_str!("calc/atmosphere.rs"),
            include_str!("calc/constraint.rs"),
            include_str!("calc/custom_battery.rs"),
            include_str!("calc/geometry.rs"),
            include_str!("calc/mod.rs"),
            include_str!("calc/mr_structure.rs"),
            include_str!("calc/multirotor.rs"),
            include_str!("calc/performance.rs"),
            include_str!("calc/propulsion.rs"),
            include_str!("calc/stability.rs"),
            include_str!("calc/structure.rs"),
            include_str!("calc/weight.rs"),
            include_str!("ui/aero.rs"),
            include_str!("ui/aero_simple.rs"),
            include_str!("calc/aero_presets.rs"),
            include_str!("calc/presets/mod.rs"),
            include_str!("calc/presets/design_point.rs"),
            include_str!("calc/presets/endurance.rs"),
            include_str!("calc/presets/frame.rs"),
            include_str!("calc/presets/mission.rs"),
            include_str!("calc/presets/propulsion.rs"),
            include_str!("calc/presets/stability.rs"),
            include_str!("calc/presets/structure.rs"),
            include_str!("calc/presets/weight.rs"),
            include_str!("ui/simple_steps.rs"),
            include_str!("ui/design_point.rs"),
            include_str!("ui/endurance.rs"),
            include_str!("ui/formulas.rs"),
            include_str!("ui/geometry.rs"),
            include_str!("ui/help.rs"),
            include_str!("ui/mission.rs"),
            include_str!("ui/mod.rs"),
            include_str!("ui/mr_structure.rs"),
            include_str!("ui/multirotor.rs"),
            include_str!("ui/propulsion.rs"),
            include_str!("ui/schematic.rs"),
            include_str!("ui/stability.rs"),
            include_str!("ui/structure.rs"),
            include_str!("ui/step_ui.rs"),
            include_str!("ui/steps.rs"),
            include_str!("ui/summary.rs"),
            include_str!("ui/weight.rs"),
            include_str!("ui/widgets.rs"),
            include_str!("ui/wizard.rs"),
            include_str!("ui/theme.rs"),
            include_str!("ui/components/mod.rs"),
            include_str!("ui/components/card.rs"),
            include_str!("ui/components/alert.rs"),
            include_str!("ui/components/input.rs"),
            include_str!("ui/components/section.rs"),
            include_str!("ui/components/step.rs"),
            include_str!("ui/components/empty.rs"),
            include_str!("ui/components/toggle.rs"),
            include_str!("ui/shell/mod.rs"),
            include_str!("ui/shell/analysis.rs"),
            include_str!("ui/shell/nav.rs"),
            include_str!("ui/viewport3d/mod.rs"),
            include_str!("ui/teach.rs"),
            include_str!("ui/outputs.rs"),
            include_str!("ui/settings_page.rs"),
            include_str!("ui/type_cards.rs"),
            include_str!("cad/outputs.rs"),
            include_str!("ui/ai.rs"),
            include_str!("ai/client.rs"),
            include_str!("ai/config.rs"),
            include_str!("ai/credential.rs"),
            include_str!("ai/fields.rs"),
            include_str!("ai/mod.rs"),
            include_str!("ai/prompt.rs"),
            include_str!("ai/provider.rs"),
            include_str!("ai/state.rs"),
            include_str!("cad/mod.rs"),
            include_str!("cad/airfoil.rs"),
            include_str!("cad/freecad.rs"),
            include_str!("cad/runner.rs"),
            include_str!("report.rs"),
            include_str!("export/xlsx.rs"),
            include_str!("support.rs"),
            include_str!("footprint.rs"),
        ];

        let tr = tr_table();
        let mut missing_keys = Vec::new();
        let mut total_keys_found = 0;

        for src in SOURCES {
            let keys = extract_keys_from_source(src);
            for key in keys {
                total_keys_found += 1;
                if !tr.contains_key(key) {
                    missing_keys.push(key);
                }
            }
        }

        assert!(
            missing_keys.is_empty(),
            "tr.json tablosunda bulunamayan anahtarlar ({}/{}): {:?}",
            missing_keys.len(),
            total_keys_found,
            missing_keys
        );
        assert!(
            total_keys_found >= 1000,
            "En az 1000 i18n anahtarı tespit edilmiş olmalıdır (bulunan: {})",
            total_keys_found
        );
    }

    #[test]
    fn test_i18n_en_translation_active() {
        // EN çevirileri aktif ve TR metinlerinden bağımsız dönmeli
        let tr_text = t_lang("btn_next", Lang::Tr);
        let en_text = t_lang("btn_next", Lang::En);
        assert_eq!(tr_text, "İleri >");
        assert_eq!(en_text, "Next >");
    }

    #[test]
    fn test_i18n_unknown_key_returns_key_itself() {
        let key = "completely_unknown_key_12345";
        let res = t(key);
        assert_eq!(res, key, "Bilinmeyen anahtar kendisini döndürmeli");
    }

    #[test]
    fn test_i18n_language_switching() {
        let prev = current_lang();
        set_lang_memory(Lang::Tr);
        assert_eq!(current_lang(), Lang::Tr);
        assert_eq!(t("btn_next"), "İleri >");

        set_lang_memory(Lang::En);
        assert_eq!(current_lang(), Lang::En);
        assert_eq!(t("btn_next"), "Next >");

        set_lang_memory(prev);
    }

    #[test]
    fn test_custom_settings_path_isolation() {
        let tmp =
            std::env::temp_dir().join(format!("werseus_settings_test_{}.json", std::process::id()));
        set_custom_settings_path(Some(tmp.clone()));

        let s = AppSettings {
            lang: Lang::En,
            freecad_cmd_path: None,
            ai_config: Default::default(),
            ..Default::default()
        };
        save_settings(&s);
        assert!(tmp.exists(), "Ayar dosyası özel konuma kaydedilmeli");
        let loaded = load_settings();
        assert_eq!(loaded.lang, Lang::En);

        let _ = std::fs::remove_file(&tmp);
        set_custom_settings_path(None);
    }

    #[test]
    fn test_tf_formatting() {
        let res = tf("completely_unknown_key_with_{0}_and_{1}", &["foo", "bar"]);
        assert_eq!(res, "completely_unknown_key_with_foo_and_bar");
    }
}
