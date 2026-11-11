// SPDX-License-Identifier: AGPL-3.0-or-later
//! Tek Tuşla Kaldırma (Uninstall) ve Ayak İzi (Footprint) Yönetimi.
//! Programın sistemde bıraktığı tüm konumları tespit eder, boyutlandırır ve güvenli şekilde temizler.

use std::fs;
use std::path::{Path, PathBuf};

/// Kaldırılabilir ayak izi kategorileri.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FootprintCategory {
    pub id: &'static str,
    pub title_key: &'static str,
    pub desc_key: &'static str,
    pub paths: Vec<PathBuf>,
    pub total_bytes: u64,
    pub selected: bool,
    pub is_user_data: bool,
}

/// Dağıtılan standart dosyaların listesi. Kaldırma sırasında yalnızca bu dosyalar silinir.
pub const DISTRIBUTED_FILES: &[&str] = &[
    "README.md",
    "LICENSE",
    "THIRD-PARTY-NOTICES.md",
    "PRIVACY.md",
    "DISCLAIMER.md",
    "COMMERCIAL-LICENSE.md",
    "CLA.md",
    "examples/fixed_wing_2kg_1600mm.json",
    "examples/quad_250_x.json",
    "examples/quad_275_x.json",
    "examples/hexa_550_x.json",
    "examples/flying_wing_1200mm.json",
    "examples/vtol_quadplane.json",
];

/// Tüm bilinen ayak izi konumlarını listeler.
pub fn scan_footprint() -> Vec<FootprintCategory> {
    let mut categories = Vec::new();

    // 1. Ayarlar, Günlükler, Önbellek ve Kimlik Bilgileri (Varsayılan: İŞARETLİ)
    let mut cat1_paths = Vec::new();
    #[cfg(windows)]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            let p = PathBuf::from(appdata).join("WerSeuS");
            if p.exists() {
                cat1_paths.push(p);
            }
        }
        if let Some(localappdata) = std::env::var_os("LOCALAPPDATA") {
            let p = PathBuf::from(localappdata).join("WerSeuS");
            if p.exists() {
                cat1_paths.push(p);
            }
        }
        let db_dir = crate::db::db_dir();
        if db_dir.exists() && !cat1_paths.iter().any(|p| db_dir.starts_with(p)) {
            cat1_paths.push(db_dir);
        }
    }
    #[cfg(not(windows))]
    {
        if let Some(home) = std::env::var_os("HOME") {
            let p = PathBuf::from(home).join(".config").join("WerSeuS");
            if p.exists() {
                cat1_paths.push(p);
            }
        }
    }
    let cat1_size = calculate_total_size(&cat1_paths);
    categories.push(FootprintCategory {
        id: "settings_and_logs",
        title_key: "uninstall.cat_settings_title",
        desc_key: "uninstall.cat_settings_desc",
        paths: cat1_paths,
        total_bytes: cat1_size,
        selected: true,
        is_user_data: false,
    });

    // 2. Hata Raporları Arşivi (Varsayılan: İŞARETSİZ)
    let mut cat2_paths = Vec::new();
    let rep_dir = crate::support::reports_dir();
    if rep_dir.exists() {
        cat2_paths.push(rep_dir);
    }
    let cat2_size = calculate_total_size(&cat2_paths);
    categories.push(FootprintCategory {
        id: "bug_reports",
        title_key: "uninstall.cat_reports_title",
        desc_key: "uninstall.cat_reports_desc",
        paths: cat2_paths,
        total_bytes: cat2_size,
        selected: false,
        is_user_data: false,
    });

    // 3. FreeCAD Çıktıları ve Kullanıcı Projeleri (Varsayılan: İŞARETSİZ — Kullanıcı Verisi)
    let mut cat3_paths = Vec::new();
    #[cfg(windows)]
    let docs = std::env::var_os("USERPROFILE")
        .map(|u| PathBuf::from(u).join("Documents"))
        .unwrap_or_else(|| PathBuf::from(r"C:\Users\Default\Documents"));
    #[cfg(not(windows))]
    let docs = std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join("Documents"))
        .unwrap_or_else(|| PathBuf::from("/tmp"));

    let cad_dir = docs.join("WerSeuS").join("cad");
    if cad_dir.exists() {
        cat3_paths.push(cad_dir);
    }
    let cat3_size = calculate_total_size(&cat3_paths);
    categories.push(FootprintCategory {
        id: "cad_and_projects",
        title_key: "uninstall.cat_cad_title",
        desc_key: "uninstall.cat_cad_desc",
        paths: cat3_paths,
        total_bytes: cat3_size,
        selected: false,
        is_user_data: true,
    });

    // 4. Programın Kendisi (yalnızca exe ve dağıtılan belgeler) (Varsayılan: İŞARETLİ)
    let mut cat4_paths = Vec::new();
    if let Ok(exe_path) = std::env::current_exe() {
        if exe_path.exists() {
            cat4_paths.push(exe_path.clone());
            if let Some(parent) = exe_path.parent() {
                // Geliştirme/depo kökündeki exe için dağıtım dosyaları (README vb.) listelenmez.
                if is_safe_dedicated_program_dir(parent) {
                    for file_rel in DISTRIBUTED_FILES {
                        let df = parent.join(file_rel);
                        if df.exists() {
                            cat4_paths.push(df);
                        }
                    }
                }
            }
        }
    }
    let cat4_size = calculate_total_size(&cat4_paths);
    categories.push(FootprintCategory {
        id: "program_files",
        title_key: "uninstall.cat_app_title",
        desc_key: "uninstall.cat_app_desc",
        paths: cat4_paths,
        total_bytes: cat4_size,
        selected: true,
        is_user_data: false,
    });

    categories
}

#[cfg(all(windows, not(test)))]
type CredEnumerateWFn = unsafe extern "system" fn(
    *const u16,
    u32,
    *mut u32,
    *mut *mut *mut crate::ai::credential::CredentialW,
) -> i32;

#[cfg(all(windows, not(test)))]
type CredDeleteWFn = unsafe extern "system" fn(*const u16, u32, u32) -> i32;
#[cfg(all(windows, not(test)))]
type CredFreeFn = unsafe extern "system" fn(*mut std::ffi::c_void);

/// Windows Kimlik Bilgisi Yöneticisi temizleme kancası (yapay zeka anahtarları için).
/// CredDeleteW ile WerSeuS_AI_ApiKey_* hedeflerini siler (CredEnumerateW ile filtreleyerek).
pub fn clear_credential_vault() {
    #[cfg(all(windows, not(test)))]
    {
        use std::ffi::CString;
        unsafe extern "system" {
            fn LoadLibraryA(lpLibFileName: *const i8) -> *mut std::ffi::c_void;
            fn GetProcAddress(
                hModule: *mut std::ffi::c_void,
                lpProcName: *const i8,
            ) -> *mut std::ffi::c_void;
        }

        unsafe {
            if let Ok(lib_name) = CString::new("advapi32.dll") {
                let hmod = LoadLibraryA(lib_name.as_ptr());
                if !hmod.is_null() {
                    let p_enum =
                        GetProcAddress(hmod, CString::new("CredEnumerateW").unwrap().as_ptr());
                    let p_del = GetProcAddress(hmod, CString::new("CredDeleteW").unwrap().as_ptr());
                    let p_free = GetProcAddress(hmod, CString::new("CredFree").unwrap().as_ptr());

                    if !p_del.is_null() {
                        let cred_delete: CredDeleteWFn = std::mem::transmute(p_del);

                        if !p_enum.is_null() && !p_free.is_null() {
                            let cred_enum: CredEnumerateWFn = std::mem::transmute(p_enum);
                            let cred_free: CredFreeFn = std::mem::transmute(p_free);

                            let mut count = 0u32;
                            let mut creds_ptr: *mut *mut crate::ai::credential::CredentialW =
                                std::ptr::null_mut();
                            let filter: Vec<u16> = "WerSeuS*"
                                .encode_utf16()
                                .chain(std::iter::once(0))
                                .collect();

                            let ret = cred_enum(filter.as_ptr(), 0, &mut count, &mut creds_ptr);
                            if ret != 0 && !creds_ptr.is_null() {
                                let slice = std::slice::from_raw_parts(creds_ptr, count as usize);
                                for &c in slice {
                                    if !c.is_null() && !(*c).target_name.is_null() {
                                        let mut len = 0;
                                        while *(*c).target_name.add(len) != 0 {
                                            len += 1;
                                        }
                                        let target_slice =
                                            std::slice::from_raw_parts((*c).target_name, len);
                                        let target_name = String::from_utf16_lossy(target_slice);
                                        if target_name.starts_with("WerSeuS_AI_ApiKey_")
                                            || target_name.starts_with("WerSeuS_")
                                        {
                                            let _ = cred_delete(
                                                (*c).target_name,
                                                crate::ai::credential::CRED_TYPE_GENERIC,
                                                0,
                                            );
                                        }
                                    }
                                }
                                cred_free(creds_ptr as *mut _);
                            }
                        }

                        // Bilinen hedefleri de doğrudan CredDeleteW ile sil
                        for target in &[
                            "WerSeuS_AI_ApiKey_openai",
                            "WerSeuS_AI_ApiKey_anthropic",
                            "WerSeuS_AI_ApiKey_gemini",
                        ] {
                            let wide: Vec<u16> =
                                target.encode_utf16().chain(std::iter::once(0)).collect();
                            let _ = cred_delete(
                                wide.as_ptr(),
                                crate::ai::credential::CRED_TYPE_GENERIC,
                                0,
                            );
                        }
                    }
                }
            }
        }
    }
    crate::support::log_info("Kimlik bilgisi yöneticisi temizleme kancası çağrıldı.");
}

/// Verilen yolların toplam bayt boyutunu özyinelemeli olarak hesaplar.
pub fn calculate_total_size(paths: &[PathBuf]) -> u64 {
    let mut total = 0;
    for p in paths {
        total += path_size(p);
    }
    total
}

fn path_size(p: &Path) -> u64 {
    if p.is_file() {
        fs::metadata(p).map(|m| m.len()).unwrap_or(0)
    } else if p.is_dir() {
        let mut sum = 0;
        if let Ok(entries) = fs::read_dir(p) {
            for entry in entries.flatten() {
                sum += path_size(&entry.path());
            }
        }
        sum
    } else {
        0
    }
}

/// Bir dizinin program için özel tahsis edilmiş güvenli bir klasör olup olmadığını doğrular.
/// Asla silinmemesi gereken sistem, kullanıcı kökü veya paylaşılan klasörleri reddeder.
pub fn is_safe_dedicated_program_dir(dir: &Path) -> bool {
    let Ok(canonical) = dir.canonicalize() else {
        return false;
    };

    // Sürücü kökü kontrolü (ör. C:\)
    if canonical.parent().is_none() {
        return false;
    }

    // Tehlikeli sistem klasörleri kontrolü
    let dangerous_names = [
        "windows",
        "system32",
        "program files",
        "program files (x86)",
        "users",
        "desktop",
        "documents",
        "downloads",
        "start-up",
    ];

    let path_str = canonical.to_string_lossy().to_lowercase();
    for d in dangerous_names {
        if path_str.ends_with(&format!("\\{}", d)) || path_str == d {
            return false;
        }
    }

    // Kullanıcı profilinin kendisi mi?
    if let Some(userprofile) = std::env::var_os("USERPROFILE") {
        if let Ok(up_canon) = PathBuf::from(userprofile).canonicalize() {
            if canonical == up_canon {
                return false;
            }
        }
    }

    // Program klasörü içinde werseus-uav-calc.exe bulunmalı (eski iha_tasarim.exe kaldırıldı)
    let has_exe = canonical.join("werseus-uav-calc.exe").exists();

    // Ve klasör adı werseus veya uav veya dist benzeri olmalı
    let folder_name = canonical
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_lowercase();
    let is_dedicated_name = folder_name.contains("werseus")
        || folder_name.contains("uav")
        || folder_name.starts_with("dist");

    has_exe && is_dedicated_name
}

/// Dağıtılan standart dosyaları program klasöründen temizler.
/// Kullanıcının kendi dosyalarına kesinlikle dokunmaz.
pub fn clean_program_distributed_files(
    program_dir: &Path,
) -> (Vec<PathBuf>, Vec<(PathBuf, String)>) {
    let mut deleted = Vec::new();
    let mut failed = Vec::new();

    for file_rel in DISTRIBUTED_FILES {
        let f = program_dir.join(file_rel);
        if f.is_file() {
            match fs::remove_file(&f) {
                Ok(()) => deleted.push(f),
                Err(e) => failed.push((f, e.to_string())),
            }
        }
    }

    // examples klasörü boş kalmışsa sil (özyinelemesiz)
    let examples_dir = program_dir.join("examples");
    if examples_dir.is_dir() {
        let _ = fs::remove_dir(&examples_dir);
    }

    (deleted, failed)
}

/// `clean_program_distributed_files` öncesinde klasörün güvenli ve ayrılmış olduğunu doğrular.
/// Güvenli değilse (ör. depo kökü, Desktop, Documents) hiçbir şey silmez.
pub fn clean_program_distributed_files_checked(
    program_dir: &Path,
) -> (Vec<PathBuf>, Vec<(PathBuf, String)>) {
    if !is_safe_dedicated_program_dir(program_dir) {
        return (Vec::new(), Vec::new());
    }
    clean_program_distributed_files(program_dir)
}

/// Kaldırma sonucunu kullanıcıya gösterilecek iletiye çevirir (başarısız yolları listeler).
pub fn uninstall_result_message(res: &UninstallResult, lang: crate::i18n::Lang) -> String {
    use crate::i18n::{t_lang, tf_lang};
    if res.failed_paths.is_empty() {
        return t_lang("uninstall_success", lang).to_string();
    }
    let mut list = String::new();
    for (p, e) in res.failed_paths.iter().take(10) {
        list.push_str(&format!(
            "
  - {}: {}",
            p.display(),
            e
        ));
    }
    if res.failed_paths.len() > 10 {
        list.push_str(&format!(
            "
  ... (+{})",
            res.failed_paths.len() - 10
        ));
    }
    tf_lang(
        "uninstall_partial",
        lang,
        &[
            res.deleted_paths.len().to_string(),
            res.failed_paths.len().to_string(),
            list,
        ],
    )
}

/// Programın kendi kendini güvenli şekilde silmesi için komut dizesi üretir.
/// Asla `rmdir /s` kullanmaz; yalnızca exe'yi siler ve klasör tamamen boşsa rmdir dener.
#[cfg(windows)]
pub fn build_self_delete_command(exe_path: &Path, parent_dir: Option<&Path>) -> String {
    let mut parts = vec![
        "timeout /t 2 /nobreak > NUL".to_string(),
        format!("del /f /q \"{}\"", exe_path.display()),
    ];
    if let Some(parent) = parent_dir {
        // Asla /s /q kullanma! Klasör içinde kullanıcı dosyası varsa rmdir güvenle başarısız olur.
        parts.push(format!("rmdir \"{}\"", parent.display()));
    }
    format!("/c \"{}\"", parts.join(" & "))
}

/// Kaldırma işlemi sonucu.
#[derive(Clone, Debug, Default)]
pub struct UninstallResult {
    pub deleted_paths: Vec<PathBuf>,
    pub failed_paths: Vec<(PathBuf, String)>,
    pub detached_spawned: bool,
}

/// Seçili kategorileri siler.
pub fn execute_uninstall(
    categories: &[FootprintCategory],
    delete_program: bool,
) -> UninstallResult {
    let mut res = UninstallResult::default();

    // 1. Dosya ve klasörleri sil
    for cat in categories {
        if !cat.selected {
            continue;
        }
        if cat.id == "program_files" {
            // Program klasörü en son işlenir
            continue;
        }

        for p in &cat.paths {
            if !p.exists() {
                continue;
            }
            if p.is_dir() {
                match fs::remove_dir_all(p) {
                    Ok(()) => res.deleted_paths.push(p.clone()),
                    Err(e) => res.failed_paths.push((p.clone(), e.to_string())),
                }
            } else if p.is_file() {
                match fs::remove_file(p) {
                    Ok(()) => res.deleted_paths.push(p.clone()),
                    Err(e) => res.failed_paths.push((p.clone(), e.to_string())),
                }
            }
        }
    }

    clear_credential_vault();

    // 2. Programın kendisini silme (Yalnızca exe + dağıtılan bilinen belgeler)
    if delete_program {
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(parent) = exe_path.parent() {
                // Yalnızca ayrılmış program klasöründe dağıtım dosyalarına dokun.
                let (deleted, failed) = clean_program_distributed_files_checked(parent);
                res.deleted_paths.extend(deleted);
                res.failed_paths.extend(failed);

                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    use std::process::Command;
                    let parent_arg = if is_safe_dedicated_program_dir(parent) {
                        Some(parent)
                    } else {
                        None
                    };
                    let raw_cmd = build_self_delete_command(&exe_path, parent_arg);
                    let mut cmd = Command::new("cmd");
                    cmd.creation_flags(crate::cad::runner::CREATE_NO_WINDOW);
                    cmd.raw_arg(&raw_cmd);
                    if cmd.spawn().is_ok() {
                        res.detached_spawned = true;
                    }
                }
            }
        }
    }

    res
}

pub fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir_named(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("{}_{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn distributed_files_untouched_in_unsafe_dir() {
        // Depo kökü benzeri klasör: werseus exe yok / adı uygun değil.
        let d = temp_dir_named("repo_root_sim");
        fs::write(d.join("README.md"), "x").unwrap();
        fs::write(d.join("LICENSE"), "x").unwrap();
        fs::write(d.join("werseus-uav-calc.exe"), "x").unwrap();
        let (deleted, failed) = clean_program_distributed_files_checked(&d);
        assert!(deleted.is_empty() && failed.is_empty());
        assert!(d.join("README.md").exists() && d.join("LICENSE").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn distributed_files_removed_in_dedicated_dir() {
        let d = temp_dir_named("werseus_dedicated_sim");
        fs::write(d.join("README.md"), "x").unwrap();
        fs::write(d.join("my_own_notes.txt"), "x").unwrap();
        fs::write(d.join("werseus-uav-calc.exe"), "x").unwrap();
        let (deleted, failed) = clean_program_distributed_files_checked(&d);
        assert!(failed.is_empty());
        assert_eq!(deleted.len(), 1);
        assert!(!d.join("README.md").exists());
        assert!(
            d.join("my_own_notes.txt").exists(),
            "kullanıcı dosyası korunmalı"
        );
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn uninstall_message_reflects_failures() {
        use crate::i18n::Lang;
        let ok = UninstallResult::default();
        assert_eq!(
            uninstall_result_message(&ok, Lang::En),
            crate::i18n::t_lang("uninstall_success", Lang::En)
        );
        let mut bad = UninstallResult::default();
        bad.deleted_paths.push(PathBuf::from("a"));
        bad.failed_paths
            .push((PathBuf::from("C:/x/locked.json"), "denied".into()));
        for lang in [Lang::En, Lang::Tr] {
            let msg = uninstall_result_message(&bad, lang);
            assert_ne!(msg, crate::i18n::t_lang("uninstall_success", lang));
            assert!(msg.contains("locked.json") && msg.contains("denied"));
            assert!(!msg.contains("{0}") && !msg.contains("{2}"));
        }
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(500), "500 B");
        assert_eq!(format_size(2048), "2.0 KB");
        assert_eq!(format_size(15 * 1024 * 1024), "15.0 MB");
    }

    #[test]
    fn test_dangerous_path_rejection() {
        // C:\ sürücü kökü asla program klasörü olarak silinmemeli
        assert!(!is_safe_dedicated_program_dir(Path::new(r"C:\")));
        assert!(!is_safe_dedicated_program_dir(Path::new(r"C:\Windows")));
        assert!(!is_safe_dedicated_program_dir(Path::new(
            r"C:\Program Files"
        )));
        assert!(!is_safe_dedicated_program_dir(Path::new(r"C:\Users")));
    }

    #[test]
    fn test_uninstall_in_temp_directory_preserves_unselected() {
        let base_tmp =
            std::env::temp_dir().join(format!("werseus_uninst_test_{}", std::process::id()));
        let _ = fs::create_dir_all(&base_tmp);

        let settings_dir = base_tmp.join("settings");
        let reports_dir = base_tmp.join("reports");
        let user_cad_dir = base_tmp.join("cad");

        fs::create_dir_all(&settings_dir).unwrap();
        fs::write(settings_dir.join("app.log"), "sample log").unwrap();

        fs::create_dir_all(&reports_dir).unwrap();
        fs::write(reports_dir.join("report.zip"), "dummy zip").unwrap();

        fs::create_dir_all(&user_cad_dir).unwrap();
        fs::write(
            user_cad_dir.join("user_project.FCStd"),
            "precious design data",
        )
        .unwrap();

        let categories = vec![
            FootprintCategory {
                id: "settings_and_logs",
                title_key: "cat1",
                desc_key: "desc1",
                paths: vec![settings_dir.clone()],
                total_bytes: 100,
                selected: true, // silinecek
                is_user_data: false,
            },
            FootprintCategory {
                id: "bug_reports",
                title_key: "cat2",
                desc_key: "desc2",
                paths: vec![reports_dir.clone()],
                total_bytes: 100,
                selected: false, // KORUNACAK
                is_user_data: false,
            },
            FootprintCategory {
                id: "cad_and_projects",
                title_key: "cat3",
                desc_key: "desc3",
                paths: vec![user_cad_dir.clone()],
                total_bytes: 100,
                selected: false, // KORUNACAK (Kullanıcı verisi!)
                is_user_data: true,
            },
        ];

        let result = execute_uninstall(&categories, false);

        assert!(result.deleted_paths.contains(&settings_dir));
        assert!(
            !settings_dir.exists(),
            "İşaretli ayarlar klasörü silinmeliydi"
        );

        // İşaretsiz klasörler KESİNLİKLE silinmemeli
        assert!(reports_dir.exists(), "İşaretsiz raporlar klasörü korunmalı");
        assert!(
            user_cad_dir.exists(),
            "İşaretsiz CAD klasörü kesinlikle korunmalı"
        );
        assert!(
            user_cad_dir.join("user_project.FCStd").exists(),
            "Kullanıcı dosyası silinmemeli"
        );

        let _ = fs::remove_dir_all(&base_tmp);
    }

    #[test]
    fn test_uninstall_preserves_user_files_in_program_dir() {
        let base_tmp =
            std::env::temp_dir().join(format!("werseus_prog_clean_test_{}", std::process::id()));
        let _ = fs::create_dir_all(&base_tmp);

        // Standart dağıtılan dosyaları oluştur
        fs::write(base_tmp.join("werseus-uav-calc.exe"), "dummy exe").unwrap();
        fs::write(base_tmp.join("README.md"), "# Readme").unwrap();
        fs::write(base_tmp.join("LICENSE"), "License text").unwrap();
        fs::create_dir_all(base_tmp.join("examples")).unwrap();
        fs::write(
            base_tmp.join("examples").join("fixed_wing_2kg_1600mm.json"),
            "{}",
        )
        .unwrap();

        // Kullanıcının oluşturduğu özel dosyalar
        let user_file1 = base_tmp.join("my_plan.json");
        let user_file2 = base_tmp.join("custom_profile.FCStd");
        fs::write(&user_file1, "{\"name\":\"My Custom UAV\"}").unwrap();
        fs::write(&user_file2, "custom cad binary data").unwrap();

        // Dağıtılan dosyaları temizle
        let (deleted, failed) = clean_program_distributed_files(&base_tmp);
        assert!(failed.is_empty(), "Temizleme hatası olmamalı: {:?}", failed);
        assert!(
            deleted.contains(&base_tmp.join("README.md")),
            "README silinmeli"
        );
        assert!(
            deleted.contains(&base_tmp.join("LICENSE")),
            "LICENSE silinmeli"
        );
        assert!(
            !base_tmp.join("examples").exists(),
            "Boşalan examples klasörü silinmeli"
        );

        // Kullanıcı dosyaları KESİNLİKLE KORUNMALI
        assert!(
            user_file1.exists(),
            "my_plan.json kesinlikle korunmalı, silinmemeli!"
        );
        assert!(
            user_file2.exists(),
            "custom_profile.FCStd kesinlikle korunmalı, silinmemeli!"
        );

        // Program klasörünü özyinelemesiz silme denemesi (rmdir parent)
        // İçinde kullanıcı dosyası kaldığı için başarısız olmalı ve klasör korunmalı
        let rmdir_result = fs::remove_dir(&base_tmp);
        assert!(
            rmdir_result.is_err(),
            "İçinde kullanıcı dosyası olan klasör rmdir ile silinememeli"
        );
        assert!(
            base_tmp.exists(),
            "Kullanıcı dosyası içeren program klasörü varlığını sürdürmeli"
        );

        // Kendi kendini silme komutunun rmdir /s veya /q İÇERMEDİĞİNİ doğrula
        #[cfg(windows)]
        {
            let cmd =
                build_self_delete_command(&base_tmp.join("werseus-uav-calc.exe"), Some(&base_tmp));
            assert!(
                !cmd.contains("rmdir /s"),
                "build_self_delete_command asla rmdir /s içermemeli!"
            );
            assert!(
                !cmd.contains("/s /q"),
                "build_self_delete_command asla /s /q içermemeli!"
            );
            assert!(
                !cmd.contains("\\\""),
                "build_self_delete_command cmd /c için kaçış hatası (\\\") içermemeli!"
            );
        }

        // Test temizliği
        let _ = fs::remove_dir_all(&base_tmp);
    }
}
