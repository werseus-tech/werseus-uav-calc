// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kullanıcı onaylı hata ve geri bildirim bildirim sistemi (e-posta + GitHub) ve basit günlükleme.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// Resmi proje destek e-posta adresi.
pub const SUPPORT_EMAIL: &str = "info.wsseng@gmail.com";

/// GitHub sorun bildirim adresi.
pub const ISSUES_URL: &str = "https://github.com/werseus-tech/werseus-uav-calc/issues/new";

static LOGGER_INITIALIZED: AtomicBool = AtomicBool::new(false);
static LOG_MUTEX: Mutex<()> = Mutex::new(());

/// AppData altında WerSeuS ana dizini.
pub fn app_data_dir() -> PathBuf {
    #[cfg(windows)]
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Users\Default\AppData\Roaming"));
    #[cfg(not(windows))]
    let base = std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".config"))
        .unwrap_or_else(|| PathBuf::from("/tmp"));

    base.join("WerSeuS")
}

static LOGS_DIR_OVERRIDE: std::sync::RwLock<Option<PathBuf>> = std::sync::RwLock::new(None);

pub fn set_custom_logs_dir(dir: Option<PathBuf>) {
    if let Ok(mut lock) = LOGS_DIR_OVERRIDE.write() {
        *lock = dir;
    }
}

/// Günlük dosyalarının saklandığı dizin: %APPDATA%\WerSeuS\logs\ (testlerde temp dizin)
pub fn logs_dir() -> PathBuf {
    if let Ok(lock) = LOGS_DIR_OVERRIDE.read() {
        if let Some(ref d) = *lock {
            return d.clone();
        }
    }
    #[cfg(test)]
    {
        std::env::temp_dir().join("werseus_test_logs")
    }
    #[cfg(not(test))]
    {
        app_data_dir().join("logs")
    }
}

static REPORTS_DIR_OVERRIDE: std::sync::RwLock<Option<PathBuf>> = std::sync::RwLock::new(None);

pub fn set_custom_reports_dir(dir: Option<PathBuf>) {
    if let Ok(mut lock) = REPORTS_DIR_OVERRIDE.write() {
        *lock = dir;
    }
}

/// Metni belirtilen bayt sınırını aşmayacak en yakın UTF-8 karakter sınırından güvenle keser.
pub fn safe_truncate_str(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        s
    } else {
        let mut end = max_bytes;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        &s[..end]
    }
}

/// Kullanıcı belgeleri altında hata raporu zip'lerinin kaydedileceği dizin.
pub fn reports_dir() -> PathBuf {
    if let Ok(lock) = REPORTS_DIR_OVERRIDE.read() {
        if let Some(ref d) = *lock {
            return d.clone();
        }
    }
    #[cfg(windows)]
    let base = std::env::var_os("USERPROFILE")
        .map(|u| PathBuf::from(u).join("Documents"))
        .unwrap_or_else(app_data_dir);
    #[cfg(not(windows))]
    let base = std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join("Documents"))
        .unwrap_or_else(app_data_dir);

    base.join("WerSeuS").join("reports")
}

/// Basit yerel günlükleme: app.log dosyasına yazar; 1 MB'ı aşınca 3 dosyaya kadar döndürür.
pub fn log_event(level: &str, msg: &str) {
    let _guard = LOG_MUTEX.lock().unwrap();
    let dir = logs_dir();
    let _ = fs::create_dir_all(&dir);
    let log_file = dir.join("app.log");

    // Döndürme kontrolü (1 MB = 1_048_576 bayt)
    if let Ok(meta) = fs::metadata(&log_file) {
        if meta.len() >= 1_048_576 {
            let log_2 = dir.join("app.log.2");
            let log_1 = dir.join("app.log.1");
            let _ = fs::remove_file(&log_2);
            let _ = fs::rename(&log_1, &log_2);
            let _ = fs::rename(&log_file, &log_1);
        }
    }

    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&log_file) {
        let ts = current_timestamp_str();
        let _ = writeln!(f, "[{}] [{}] {}", ts, level, msg);
    }
}

pub fn log_info(msg: &str) {
    log_event("INFO", msg);
}

pub fn log_warn(msg: &str) {
    log_event("WARN", msg);
}

pub fn log_error(msg: &str) {
    log_event("ERROR", msg);
}

/// panic hook kurar: beklenmedik çökmeleri %APPDATA%\WerSeuS\logs\crash-<zaman>.log dosyasına yazar.
pub fn setup_panic_hook() {
    if LOGGER_INITIALIZED.swap(true, Ordering::SeqCst) {
        return;
    }

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let ts = current_timestamp_compact();
        let dir = logs_dir();
        let _ = fs::create_dir_all(&dir);
        let crash_path = dir.join(format!("crash-{}.log", ts));

        let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
            *s
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.as_str()
        } else {
            "Bilinmeyen panik nedeni (unknown panic payload)"
        };

        let location = if let Some(loc) = info.location() {
            format!("{}:{}:{}", loc.file(), loc.line(), loc.column())
        } else {
            "Konum bilgisi yok".into()
        };

        let backtrace = std::backtrace::Backtrace::force_capture();

        let raw_report = format!(
            "WerSeuS – UAV Calc Çökme Günlüğü (Crash Log)\n\
             Zaman: {}\n\
             Sürüm: {}\n\
             Konum: {}\n\
             Mesaj: {}\n\n\
             --- Yığın İzi (Backtrace) ---\n\
             {:?}\n",
            current_timestamp_str(),
            env!("CARGO_PKG_VERSION"),
            location,
            payload,
            backtrace
        );

        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let masked_report = mask_sensitive_info(&raw_report);
            let _ = fs::write(&crash_path, &masked_report);
            log_error(&format!("PANIC kaydedildi: {} -> {}", location, payload));
        }))
        .unwrap_or_else(|_| {
            let _ = fs::write(&crash_path, &raw_report);
        });

        // Kullanıcıya basit bir yerel bildirim penceresi göster (mümkünse)
        #[cfg(windows)]
        {
            let _ = rfd::MessageDialog::new()
                .set_level(rfd::MessageLevel::Error)
                .set_title("WerSeuS – UAV Calc")
                .set_description(format!(
                    "Program beklenmedik bir hatayla karşılaştı ve kapandı.\n\
                     Hata günlüğü kaydedildi:\n{}\n\n\
                     Programı yeniden başlattığınızda raporu iletmeniz için bir pencere açılacaktır.",
                    crash_path.display()
                ))
                .show();
        }

        default_hook(info);
    }));
}

/// Henüz işlenmemiş (gönderilmemiş / kapatılmamış) en son çökme günlüğünü arar.
pub fn find_unprocessed_crash() -> Option<PathBuf> {
    let dir = logs_dir();
    let entries = fs::read_dir(dir).ok()?;

    let mut crashes = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with("crash-") && name.ends_with(".log") && !name.ends_with(".processed")
            {
                crashes.push(path);
            }
        }
    }

    crashes.sort();
    crashes.pop()
}

/// Çökme günlüğünü 'işlendi' olarak işaretler (tekrar sorulmaması için).
pub fn mark_crash_processed(path: &Path) {
    let mut new_name = path.as_os_str().to_os_string();
    new_name.push(".processed");
    let _ = fs::rename(path, new_name);
}

/// app.log dosyasından son N satırı okur.
pub fn read_last_log_lines(n: usize) -> Vec<String> {
    let log_file = logs_dir().join("app.log");
    let Ok(content) = fs::read_to_string(log_file) else {
        return Vec::new();
    };

    let lines: Vec<&str> = content.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].iter().map(|s| s.to_string()).collect()
}

/// Gizlilik maskelemesi:
/// 1. %USERPROFILE% ve C:\Users\<kullanıcı> yollarını '~' ile değiştirir.
/// 2. Olası API anahtarlarını maskeler.
/// 3. Bilgisayar adı (COMPUTERNAME), IP vb. asla eklenmez.
pub fn mask_sensitive_info(text: &str) -> String {
    let mut out = text.to_string();

    // Kullanıcı profil yolu maskeleme
    if let Some(userprofile) = std::env::var_os("USERPROFILE") {
        let up_str = userprofile.to_string_lossy();
        if !up_str.is_empty() {
            out = out.replace(&*up_str, "~");
            // JSON-kaçışlı (çift ters bölü) ve ileri bölülü varyantlar
            let escaped = up_str.replace('\\', "\\\\");
            if escaped != *up_str {
                out = out.replace(&escaped, "~");
            }
            let fwd = up_str.replace('\\', "/");
            if fwd != *up_str {
                out = out.replace(&fwd, "~");
            }
        }
    }

    // Genel C:\Users\xxx kalıbını da maskele
    let user_pat = regex_lite_user_path(&out);
    if !user_pat.is_empty() {
        for p in user_pat {
            out = out.replace(&p, "~");
        }
    }

    // Bilgisayar adını temizle
    if let Ok(comp_name) = std::env::var("COMPUTERNAME") {
        if comp_name.len() >= 3 {
            out = out.replace(&comp_name, "[HOSTNAME]");
        }
    }

    // Olası API anahtarlarını maskele (sk-..., ai-...)
    out = mask_api_keys(&out);

    out
}

/// Yol ayırıcı uzunluğu: `/`, tek ters bölü veya JSON-kaçışlı çift ters bölü için 1 veya 2; yoksa 0.
fn path_sep_len(bytes: &[u8], i: usize) -> usize {
    match bytes.get(i) {
        Some(b'/') => 1,
        Some(b'\\') => {
            if bytes.get(i + 1) == Some(&b'\\') {
                2
            } else {
                1
            }
        }
        _ => 0,
    }
}

/// `X:\Users\<ad>` ve JSON-kaçışlı `X:\Users\<ad>` (ve `/` varyantı) kalıplarını bulur.
fn regex_lite_user_path(s: &str) -> Vec<String> {
    let mut res = Vec::new();
    let bytes = s.as_bytes();
    if bytes.len() < 9 {
        return res;
    }

    let mut i = 0;
    while i + 9 <= bytes.len() {
        if s.is_char_boundary(i) && bytes[i].is_ascii_alphabetic() && bytes[i + 1] == b':' {
            let sep1 = path_sep_len(bytes, i + 2);
            let users_at = i + 2 + sep1;
            if sep1 > 0
                && users_at + 5 <= bytes.len()
                && bytes[users_at..users_at + 5].eq_ignore_ascii_case(b"users")
            {
                let sep2 = path_sep_len(bytes, users_at + 5);
                if sep2 > 0 {
                    let start = i;
                    let after_prefix = users_at + 5 + sep2;
                    let mut end_offset = 0;
                    for (idx, ch) in s[after_prefix..].char_indices() {
                        if ch == '\\'
                            || ch == '/'
                            || ch == '"'
                            || ch == '\''
                            || ch == ';'
                            || ch.is_whitespace()
                            || idx >= 50
                        {
                            end_offset = idx;
                            break;
                        }
                        end_offset = idx + ch.len_utf8();
                    }

                    if end_offset > 0 {
                        let full_user_path = &s[start..after_prefix + end_offset];
                        if !res.contains(&full_user_path.to_string()) {
                            res.push(full_user_path.to_string());
                        }
                        i = after_prefix + end_offset;
                        continue;
                    }
                }
            }
        }
        i += 1;
    }
    res
}

fn is_likely_secret_token(s: &str) -> bool {
    if s.len() < 32 {
        return false;
    }
    let is_hex = s.chars().all(|c| c.is_ascii_hexdigit());
    if is_hex {
        return true;
    }
    let has_lower = s.chars().any(|c| c.is_ascii_lowercase());
    let has_upper = s.chars().any(|c| c.is_ascii_uppercase());
    let has_digit = s.chars().any(|c| c.is_ascii_digit());
    has_digit && (has_lower || has_upper)
}

fn mask_header_token(text: &str, header_prefix: &str, min_len: usize) -> String {
    let mut out = String::new();
    let prefix_bytes = header_prefix.as_bytes();
    let bytes = text.as_bytes();
    let mut i = 0;

    while i + prefix_bytes.len() <= bytes.len() {
        if text.is_char_boundary(i)
            && bytes[i..i + prefix_bytes.len()].eq_ignore_ascii_case(prefix_bytes)
        {
            out.push_str(&text[i..i + prefix_bytes.len()]);
            let after = &text[i + prefix_bytes.len()..];

            let mut token_start = 0;
            for (idx, c) in after.char_indices() {
                if c == ' ' || c == '\t' || c == '"' || c == '\'' || c == ':' {
                    token_start = idx + c.len_utf8();
                } else {
                    break;
                }
            }
            out.push_str(&after[..token_start]);
            let token_slice = &after[token_start..];

            let mut token_end = token_slice.len();
            for (idx, c) in token_slice.char_indices() {
                if !c.is_ascii_alphanumeric()
                    && c != '_'
                    && c != '-'
                    && c != '.'
                    && c != '+'
                    && c != '/'
                    && c != '='
                {
                    token_end = idx;
                    break;
                }
            }

            let token = &token_slice[..token_end];
            if token.len() >= min_len {
                out.push_str("[REDACTED_API_KEY]");
            } else {
                out.push_str(token);
            }

            i += prefix_bytes.len() + token_start + token_end;
        } else {
            if let Some(ch) = text[i..].chars().next() {
                out.push(ch);
                i += ch.len_utf8();
            } else {
                break;
            }
        }
    }
    if i < text.len() {
        out.push_str(&text[i..]);
    }
    out
}

fn mask_api_keys(s: &str) -> String {
    let mut out = s.to_string();

    // 1. HTTP Başlık maskelemeleri
    out = mask_header_token(&out, "bearer", 16);
    out = mask_header_token(&out, "x-api-key:", 16);
    out = mask_header_token(&out, "x-api-key", 16);
    out = mask_header_token(&out, "x-goog-api-key:", 16);
    out = mask_header_token(&out, "x-goog-api-key", 16);

    // 2. Token-based maskeleme: sk-, sk-ant-, AIza, ai- ve 32+ karakter gizli dizgiler
    let mut result = String::with_capacity(out.len());
    let mut chars = out.chars().peekable();

    while let Some(c) = chars.next() {
        // AIza... (Google API Key)
        if c == 'A' && chars.peek() == Some(&'I') {
            let mut buf = String::from("A");
            buf.push(chars.next().unwrap()); // 'I'
            if chars.peek() == Some(&'z') {
                buf.push(chars.next().unwrap()); // 'z'
                if chars.peek() == Some(&'a') {
                    buf.push(chars.next().unwrap()); // 'a'
                    let mut key_len = 4;
                    while let Some(&nc) = chars.peek() {
                        if nc.is_ascii_alphanumeric() || nc == '_' || nc == '-' {
                            chars.next();
                            key_len += 1;
                        } else {
                            break;
                        }
                    }
                    if key_len >= 30 {
                        result.push_str("[REDACTED_API_KEY]");
                    } else {
                        result.push_str(&buf);
                    }
                    continue;
                }
            }
            result.push_str(&buf);
            continue;
        }

        // sk- veya sk-ant- (OpenAI / Anthropic)
        if (c == 's' || c == 'S') && (chars.peek() == Some(&'k') || chars.peek() == Some(&'K')) {
            let mut buf = String::new();
            buf.push(c);
            buf.push(chars.next().unwrap()); // 'k'
            if chars.peek() == Some(&'-') {
                buf.push(chars.next().unwrap()); // '-'
                let mut token_len = 3;
                while let Some(&nc) = chars.peek() {
                    if nc.is_ascii_alphanumeric() || nc == '_' || nc == '-' {
                        chars.next();
                        token_len += 1;
                    } else {
                        break;
                    }
                }
                if token_len >= 16 {
                    result.push_str("[REDACTED_API_KEY]");
                } else {
                    result.push_str(&buf);
                }
                continue;
            } else {
                result.push_str(&buf);
                continue;
            }
        }

        // ai-... (OpenRouter / diğer AI tokenları)
        if (c == 'a' || c == 'A') && (chars.peek() == Some(&'i') || chars.peek() == Some(&'I')) {
            let mut buf = String::new();
            buf.push(c);
            buf.push(chars.next().unwrap()); // 'i'
            if chars.peek() == Some(&'-') {
                buf.push(chars.next().unwrap()); // '-'
                let mut token_len = 3;
                while let Some(&nc) = chars.peek() {
                    if nc.is_ascii_alphanumeric() || nc == '_' || nc == '-' {
                        chars.next();
                        token_len += 1;
                    } else {
                        break;
                    }
                }
                if token_len >= 16 {
                    result.push_str("[REDACTED_API_KEY]");
                } else {
                    result.push_str(&buf);
                }
                continue;
            } else {
                result.push_str(&buf);
                continue;
            }
        }

        // 32+ hex/base64 sürekli karakter (rastgele token / secret)
        if c.is_ascii_alphanumeric() {
            let mut word = String::new();
            word.push(c);
            while let Some(&nc) = chars.peek() {
                if nc.is_ascii_alphanumeric() || nc == '+' || nc == '/' || nc == '=' {
                    word.push(chars.next().unwrap());
                } else {
                    break;
                }
            }
            if word.len() >= 32 && is_likely_secret_token(&word) {
                result.push_str("[REDACTED_API_KEY]");
            } else {
                result.push_str(&word);
            }
            continue;
        }

        result.push(c);
    }

    result
}

/// Standart sistem özeti bilgisi.
pub fn generate_system_summary(lang: &str, drone_type: &str) -> String {
    format!(
        "Uygulama Sürümü: {}\n\
         İşletim Sistemi: {}\n\
         Mimari: {}\n\
         Dil: {}\n\
         Platform Türü: {}\n\
         Zaman: {}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        lang,
        drone_type,
        current_timestamp_str()
    )
}

/// Standart hata rapor metni oluşturur.
pub fn build_full_report_text(
    user_explanation: &str,
    system_summary: &str,
    project_json: Option<&str>,
    log_tail: &[String],
    crash_content: Option<&str>,
) -> String {
    let mut out = String::new();
    out.push_str("=== WERSEUS UAV CALC HATA / GERİ BİLDİRİM RAPORU ===\n\n");

    out.push_str("--- 1. KULLANICI AÇIKLAMASI ---\n");
    if user_explanation.trim().is_empty() {
        out.push_str("(Kullanıcı açıklama girmedi)\n\n");
    } else {
        out.push_str(user_explanation.trim());
        out.push_str("\n\n");
    }

    out.push_str("--- 2. SİSTEM ÖZETİ ---\n");
    out.push_str(system_summary);
    out.push('\n');

    if let Some(crash) = crash_content {
        out.push_str("--- 3. ÇÖKME AYRINTILARI (CRASH LOG) ---\n");
        out.push_str(crash.trim());
        out.push_str("\n\n");
    }

    if let Some(p_json) = project_json {
        out.push_str("--- 4. PROJE DOSYASI (JSON) ---\n");
        out.push_str(p_json);
        out.push_str("\n\n");
    }

    out.push_str("--- 5. SON GÜNLÜK SATIRLARI (SON 200) ---\n");
    for l in log_tail {
        out.push_str(l);
        out.push('\n');
    }

    mask_sensitive_info(&out)
}

/// E-posta bağlantısı (mailto:) URL'si üretir.
/// Eğer gövde 1800 karakterden uzunsa, zip oluşturup kısaltılmış gövde döner.
pub fn prepare_mailto_link(
    full_report: &str,
    user_expl: &str,
    project_json: Option<&str>,
) -> (String, Option<PathBuf>) {
    let subject = format!("[WerSeuS {}] Hata Raporu", env!("CARGO_PKG_VERSION"));
    let encoded_full = url_encode(full_report);

    // mailto: toplam URL uzunluğu 2000 sınırını aşmamalı (encoded_full <= 1800 güvenli limit)
    if encoded_full.len() <= 1800 {
        let encoded_subject = url_encode(&subject);
        let link = format!(
            "mailto:{}?subject={}&body={}",
            SUPPORT_EMAIL, encoded_subject, encoded_full
        );
        (link, None)
    } else {
        // Zip oluştur
        let zip_path = create_report_zip(full_report, project_json);
        let zip_display = zip_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "report.zip".into());

        let short_body = format!(
            "WerSeuS UAV Calc Hata Raporu (Özet):\n\n\
             {}\n\n\
             [ÖNEMLİ] Rapor içeriği uzun olduğu için ayrıntılı rapor zip dosyası olarak kaydedildi.\n\
             Lütfen ekteki dosyayı bu e-postaya ekleyiniz:\n\
             {}\n",
            user_expl.trim(),
            zip_display
        );

        let encoded_subject = url_encode(&subject);
        let encoded_body = url_encode(&short_body);
        let link = format!(
            "mailto:{}?subject={}&body={}",
            SUPPORT_EMAIL, encoded_subject, encoded_body
        );
        (link, zip_path)
    }
}

/// GitHub Issue açma URL'si üretir (toplam URL <= 6000 karakter).
pub fn prepare_github_issue_url(
    title: &str,
    full_report: &str,
    project_json: Option<&str>,
) -> (String, Option<PathBuf>) {
    let max_url_len = 6000;
    let base = ISSUES_URL;

    let (body_text, zip_path) = if full_report.len() > 3500 {
        let zip = create_report_zip(full_report, project_json);
        let zip_msg = if let Some(ref z) = zip {
            format!(
                "\n\n*(Ayrıntılı proje ve log zip dosyası oluşturuldu: `{}`. Lütfen issue'ya sürükleyip bırakarak ekleyin.)*",
                z.display()
            )
        } else {
            String::new()
        };
        let truncated = safe_truncate_str(full_report, 2500);
        (format!("{}...\n{}", truncated, zip_msg), zip)
    } else {
        (full_report.to_string(), None)
    };

    let enc_title = url_encode(title);
    let enc_body = url_encode(&body_text);
    let url = format!(
        "{}?title={}&body={}&labels=bug,crash",
        base, enc_title, enc_body
    );

    if url.len() > max_url_len {
        let safe_body = safe_truncate_str(&body_text, 1000);
        let enc_safe = url_encode(safe_body);
        (
            format!(
                "{}?title={}&body={}&labels=bug,crash",
                base, enc_title, enc_safe
            ),
            zip_path,
        )
    } else {
        (url, zip_path)
    }
}

/// Raporu Belgeler\WerSeuS\reports\report-<zaman>.zip olarak paketler.
pub fn create_report_zip(report_text: &str, project_json: Option<&str>) -> Option<PathBuf> {
    let rep_dir = reports_dir();
    let _ = fs::create_dir_all(&rep_dir);
    let ts = current_timestamp_compact();
    let zip_path = rep_dir.join(format!("report-{}.zip", ts));

    let mut entries = vec![("report.txt", report_text.as_bytes().to_vec())];
    if let Some(pj) = project_json {
        let masked_pj = mask_sensitive_info(pj);
        entries.push(("project.json", masked_pj.into_bytes()));
    }

    if write_simple_zip(&zip_path, &entries).is_ok() {
        Some(zip_path)
    } else {
        None
    }
}

/// Standart ZIP dosyasını (Store / No Compression) saf Rust ile oluşturur.
/// Dış crate gerektirmez; tüm işletim sistemlerinde doğrudan açılabilir.
pub fn write_simple_zip(dest: &Path, files: &[(&str, Vec<u8>)]) -> std::io::Result<()> {
    let mut file = File::create(dest)?;
    let mut central_dir = Vec::new();
    let mut local_offset = 0u32;

    for (name, data) in files {
        let name_bytes = name.as_bytes();
        let crc = crc32_simple(data);
        let size = data.len() as u32;

        // Local file header
        file.write_all(&0x04034b50u32.to_le_bytes())?; // signature
        file.write_all(&20u16.to_le_bytes())?; // version needed
        file.write_all(&0u16.to_le_bytes())?; // flags
        file.write_all(&0u16.to_le_bytes())?; // compression = 0 (store)
        file.write_all(&0u16.to_le_bytes())?; // time
        file.write_all(&0u16.to_le_bytes())?; // date
        file.write_all(&crc.to_le_bytes())?;
        file.write_all(&size.to_le_bytes())?; // comp size
        file.write_all(&size.to_le_bytes())?; // uncomp size
        file.write_all(&(name_bytes.len() as u16).to_le_bytes())?;
        file.write_all(&0u16.to_le_bytes())?; // extra len
        file.write_all(name_bytes)?;
        file.write_all(data)?;

        // Central directory entry
        central_dir.write_all(&0x02014b50u32.to_le_bytes())?;
        central_dir.write_all(&20u16.to_le_bytes())?; // version made by
        central_dir.write_all(&20u16.to_le_bytes())?; // version needed
        central_dir.write_all(&0u16.to_le_bytes())?; // flags
        central_dir.write_all(&0u16.to_le_bytes())?; // compression = 0
        central_dir.write_all(&0u16.to_le_bytes())?; // time
        central_dir.write_all(&0u16.to_le_bytes())?; // date
        central_dir.write_all(&crc.to_le_bytes())?;
        central_dir.write_all(&size.to_le_bytes())?;
        central_dir.write_all(&size.to_le_bytes())?;
        central_dir.write_all(&(name_bytes.len() as u16).to_le_bytes())?;
        central_dir.write_all(&0u16.to_le_bytes())?; // extra len
        central_dir.write_all(&0u16.to_le_bytes())?; // comment len
        central_dir.write_all(&0u16.to_le_bytes())?; // disk number
        central_dir.write_all(&0u16.to_le_bytes())?; // internal attr
        central_dir.write_all(&0u32.to_le_bytes())?; // external attr
        central_dir.write_all(&local_offset.to_le_bytes())?;
        central_dir.write_all(name_bytes)?;

        local_offset += 30 + name_bytes.len() as u32 + size;
    }

    let cd_offset = local_offset;
    let cd_size = central_dir.len() as u32;
    file.write_all(&central_dir)?;

    // End of central directory record (EOCD)
    file.write_all(&0x06054b50u32.to_le_bytes())?;
    file.write_all(&0u16.to_le_bytes())?; // disk number
    file.write_all(&0u16.to_le_bytes())?; // disk with CD
    file.write_all(&(files.len() as u16).to_le_bytes())?; // entries on disk
    file.write_all(&(files.len() as u16).to_le_bytes())?; // total entries
    file.write_all(&cd_size.to_le_bytes())?;
    file.write_all(&cd_offset.to_le_bytes())?;
    file.write_all(&0u16.to_le_bytes())?; // comment len

    Ok(())
}

fn crc32_simple(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

/// Standart RFC 3986 URL Encoding
pub fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
            out.push(b as char);
        } else if b == b' ' {
            out.push_str("%20");
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

fn current_timestamp_str() -> String {
    // Hafif yerel zaman damgası
    let now = std::time::SystemTime::now();
    let dur = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();
    // Yaklaşık UTC formatı (harici chrono crate'i olmadan)
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let days = secs / 86400;
    format!("EpochDay-{:04} {:02}:{:02}:{:02} UTC", days, h, m, s)
}

static TIMESTAMP_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn current_timestamp_compact() -> String {
    let now = std::time::SystemTime::now();
    let dur = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let cnt = TIMESTAMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}-{}", dur.as_millis(), cnt)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `reports_dir` geçersiz kılması süreç geneli olduğundan, kullanan testler seri çalışır ve
    /// gerçek Documents\WerSeuS\reports klasörüne asla yazılmaz.
    static REPORTS_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct ReportsGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        dir: PathBuf,
    }

    impl ReportsGuard {
        fn new(tag: &str) -> Self {
            let lock = REPORTS_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let dir =
                std::env::temp_dir().join(format!("werseus_rep_{}_{}", tag, std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            let _ = fs::create_dir_all(&dir);
            set_custom_reports_dir(Some(dir.clone()));
            ReportsGuard { _lock: lock, dir }
        }
    }

    impl Drop for ReportsGuard {
        fn drop(&mut self) {
            set_custom_reports_dir(None);
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn test_mask_user_profile_and_keys() {
        let input = "Hata konumu: C:\\Users\\ahmet\\Desktop\\proj\\src\\main.rs, key: sk-abcdef1234567890abcdef123456";
        let masked = mask_sensitive_info(input);
        assert!(!masked.contains("C:\\Users\\ahmet"));
        assert!(masked.contains('~'));
        assert!(!masked.contains("sk-abcdef"));
        assert!(masked.contains("[REDACTED_API_KEY]"));
    }

    #[test]
    fn test_url_encode() {
        let text = "WerSeuS Hata: v0.0.1 & test!";
        let enc = url_encode(text);
        assert_eq!(enc, "WerSeuS%20Hata%3A%20v0.0.1%20%26%20test%21");
    }

    #[test]
    fn test_zip_creation() {
        let tmp_dir =
            std::env::temp_dir().join(format!("werseus_zip_test_{}", current_timestamp_compact()));
        let _ = fs::create_dir_all(&tmp_dir);
        let zip_file = tmp_dir.join("test.zip");

        let files = [
            ("hello.txt", b"Hello, World!".to_vec()),
            ("sub/data.json", b"{\"status\": \"ok\"}".to_vec()),
        ];
        write_simple_zip(&zip_file, &files).unwrap();

        assert!(zip_file.exists());
        assert!(fs::metadata(&zip_file).unwrap().len() > 100);

        let _ = fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_mailto_length_limit_triggers_zip() {
        let guard = ReportsGuard::new("t1");
        let tmp_reports = guard.dir.clone();

        let huge_text = "A".repeat(2500);
        let (link, zip_opt) = prepare_mailto_link(&huge_text, "Açıklama", None);
        assert!(
            link.len() < 2000,
            "Mailto link çok uzun olmamalı: {}",
            link.len()
        );
        assert!(zip_opt.is_some(), "Uzun gövde ZIP dosyası oluşturmalı");
        if let Some(z) = zip_opt {
            assert!(
                z.starts_with(&tmp_reports),
                "Zip dosyası geçici test klasöründe olmalı"
            );
            let _ = fs::remove_file(z);
        }
    }

    #[test]
    fn test_mark_crash_processed() {
        let tmp = std::env::temp_dir().join("test_crash.log");
        fs::write(&tmp, "test crash").unwrap();
        mark_crash_processed(&tmp);
        let processed = std::env::temp_dir().join("test_crash.log.processed");
        assert!(processed.exists());
        let _ = fs::remove_file(processed);
    }

    #[test]
    fn test_logs_written_to_temp_dir_during_test() {
        // Madde 8: Testler sırasında loglar %APPDATA%'ya değil temp dizine yazılmalı
        let l_dir = logs_dir();
        assert!(
            l_dir.starts_with(std::env::temp_dir()),
            "Test esnasında logs_dir temp dizininde olmalı: {:?}",
            l_dir
        );

        log_info("Madde 8 test log mesajı");
        let log_file = l_dir.join("app.log");
        assert!(log_file.exists(), "app.log temp dizininde oluşturulmalı");
        let content = fs::read_to_string(&log_file).unwrap();
        assert!(content.contains("Madde 8 test log mesajı"));
    }

    #[test]
    fn test_regex_lite_user_path_turkish_and_unicode() {
        // Madde 4: Türkçe karakterli kullanıcı adlarında panik olmamalı ve doğru maskelenmeli
        let input = "Hata 1: C:\\Users\\İsmail\\Desktop\\proj\\src\\main.rs; Hata 2: C:\\Users\\Şevket_Ğ\\Documents\\cad.FCStd; Hata 3: C:\\Users\\Özgür_Çelik\\AppData\\Roaming";
        let masked = mask_sensitive_info(input);

        assert!(
            !masked.contains("İsmail"),
            "İsmail kullanıcı adı maskelenmeli: {}",
            masked
        );
        assert!(
            !masked.contains("Şevket_Ğ"),
            "Şevket_Ğ kullanıcı adı maskelenmeli: {}",
            masked
        );
        assert!(
            !masked.contains("Özgür_Çelik"),
            "Özgür_Çelik kullanıcı adı maskelenmeli: {}",
            masked
        );
        assert!(
            masked.contains("~\\Desktop\\proj\\src\\main.rs"),
            "Yol başı ~ ile değiştirilmeli: {}",
            masked
        );
    }

    #[test]
    fn test_safe_truncate_str_and_github_issue_url_multibyte() {
        // Madde 5: Çok baytlı karakterlerin ortasından dilimleme paniği önlenmeli
        let text = "ğüşiöç".repeat(200); // Türkçe 2 baytlık UTF-8 karakterler
        for cut in 1..20 {
            let truncated = safe_truncate_str(&text, cut);
            assert!(
                truncated.len() <= cut,
                "Kesilen uzunluk sınırı aşamaz: {} <= {}",
                truncated.len(),
                cut
            );
            // Karakter sınırında olduğunu doğrula
            assert!(text.is_char_boundary(truncated.len()));
        }

        // GitHub issue URL uzunluk aşımında güvenli dilimleme (zip geçici dizine yazılır)
        let _guard = ReportsGuard::new("gh");
        let huge_turkish_text = "ğüşiöç ".repeat(1200);
        let (url, _zip) = prepare_github_issue_url(
            "Test Türkçe Başlık",
            &huge_turkish_text,
            Some("{\"project\":\"test\"}"),
        );
        assert!(
            url.len() <= 6500,
            "Issue URL sınırlandırılmalı: {}",
            url.len()
        );
    }

    #[test]
    fn test_safe_truncate_turkish_near_cut_points() {
        // 2 baytlık karakterler: tek sayılı sınır karakterin ortasına denk gelir.
        let two = "ğ".repeat(1500);
        assert_eq!(safe_truncate_str(&two, 2500).len(), 2500);
        assert_eq!(safe_truncate_str(&two, 1001).len(), 1000);
        // 3 baytlık karakter (€) ve karışık Türkçe metin
        let mixed = format!("{}€İıŞş{}", "a".repeat(999), "ç".repeat(50));
        for cut in 995..1010 {
            let t = safe_truncate_str(&mixed, cut);
            assert!(t.len() <= cut);
            assert!(mixed.is_char_boundary(t.len()));
            assert!(mixed.starts_with(t));
        }
        // 1000. bayt '€' karakterinin ortasında: karakter tamamen dışarıda kalmalı
        assert_eq!(safe_truncate_str(&mixed, 1000).len(), 999);
        assert_eq!(safe_truncate_str(&mixed, 0), "");
        assert_eq!(safe_truncate_str("ğ", 1), "");
    }

    #[test]
    fn test_github_issue_url_turkish_no_zip_fallback_cut() {
        // Rapor 3500 bayttan kısa (zip yok) ama kodlanmış URL 6000'i aşar: 1000 bayt kesimi
        // Türkçe karakter sınırında yapılmalı ve panik olmamalı.
        let report = "ş".repeat(1700); // 3400 bayt
        assert!(report.len() <= 3500);
        let (url, zip) = prepare_github_issue_url("Başlık", &report, None);
        assert!(zip.is_none());
        assert!(url.starts_with(ISSUES_URL));
        assert!(url.len() < 6000, "Kesilmiş URL kısa olmalı: {}", url.len());
    }

    #[test]
    fn test_mailto_encoded_length_triggers_zip() {
        // Madde 6: Metin ham olarak < 1000 karakter olsa bile url_encode hali > 1800 olunca zip tetiklenmeli
        let _guard = ReportsGuard::new("m6");

        // 'ğ' URL-encode olunca %C4%9F (6 bayt) olur. 400 adet 'ğ' -> 2400 bayt!
        let turkish_heavy = "ğ".repeat(400);
        assert!(
            turkish_heavy.len() < 1000,
            "Ham karakter sayısı: {}",
            turkish_heavy.len()
        );
        assert!(
            url_encode(&turkish_heavy).len() > 1800,
            "Encoded boyutu 1800'ü aşmalı: {}",
            url_encode(&turkish_heavy).len()
        );

        let (link, zip_opt) = prepare_mailto_link(&turkish_heavy, "Kullanıcı özeti", None);
        assert!(
            link.len() < 2000,
            "Mailto linki Windows sınırını aşmamalı: {}",
            link.len()
        );
        assert!(
            zip_opt.is_some(),
            "Encoded uzunluğu 1800'ü aştığı için ZIP oluşturulmalıydı"
        );

        if let Some(z) = zip_opt {
            let _ = fs::remove_file(z);
        }
    }

    #[test]
    fn test_mask_all_api_key_types_and_project_json() {
        // Madde 11: Farklı sağlayıcı anahtarlarının maskelenmesi
        let text = "\
            openai: sk-abcdef1234567890abcdef1234567890\n\
            anthropic: sk-ant-api03-abcdef1234567890abcdef1234567890\n\
            gemini: AIzaSyD1234567890abcdef1234567890abcdef12\n\
            openrouter: ai-abcdef1234567890abcdef1234567890\n\
            bearer: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9\n\
            header: x-api-key: mysecretkey1234567890abcdef1234567890\n\
            hex_token: d41d8cd98f00b204e9800998ecf8427e12345678\n\
        ";

        let masked = mask_sensitive_info(text);
        assert!(!masked.contains("sk-abcdef"));
        assert!(!masked.contains("sk-ant-api03"));
        assert!(!masked.contains("AIzaSyD"));
        assert!(!masked.contains("ai-abcdef"));
        assert!(!masked.contains("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"));
        assert!(!masked.contains("mysecretkey1234567890"));
        assert!(!masked.contains("d41d8cd98f00b204e9800998ecf8427e"));

        // ZIP içindeki project.json'un da maskelendiğini doğrula
        let _guard = ReportsGuard::new("pj");

        let secret_pj = "{\"api_key\": \"sk-ant-api03-secret1234567890abcdef\", \"user_path\": \"C:\\\\Users\\\\İsmail\\\\test\"}";
        let zip = create_report_zip("Hata raporu", Some(secret_pj)).unwrap();

        // ZIP içeriğini oku ve project.json içinde açık anahtar olmadığını doğrula
        let zip_bytes = fs::read(&zip).unwrap();
        let zip_str = String::from_utf8_lossy(&zip_bytes);
        assert!(
            !zip_str.contains("sk-ant-api03-secret"),
            "Zip içindeki project.json anahtarı açık içermemeli!"
        );
        // Girdi JSON-kaçışlıdır (çift ters bölü); kullanıcı adı açık kalmamalı.
        assert!(
            !zip_str.contains("İsmail"),
            "Zip içindeki project.json kullanıcı adını açık içermemeli!"
        );
        assert!(zip_str.contains('~'), "Yol '~' ile değiştirilmiş olmalı");
        let _ = fs::remove_file(zip);
    }

    #[test]
    fn test_json_escaped_user_paths_are_masked() {
        let raw =
            r#"{"p":"C:\\Users\\Ayşe\\Desktop\\a.json","q":"D:/Users/Bob/x","r":"c:\\users\\Zed"}"#;
        assert!(
            raw.contains(r"C:\\Users\\Ayşe"),
            "test girdisi kaçışlı olmalı"
        );
        let masked = mask_sensitive_info(raw);
        for name in ["Ayşe", "Bob", "Zed"] {
            assert!(!masked.contains(name), "{name} maskelenmeli: {masked}");
        }
        assert!(masked.contains(r"~\\Desktop\\a.json"), "{masked}");
        // Kaçışsız yol da çalışmaya devam eder
        assert_eq!(mask_sensitive_info(r"C:\Users\Ayşe\Desktop"), r"~\Desktop");
    }
}
