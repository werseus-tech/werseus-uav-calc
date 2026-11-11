// SPDX-License-Identifier: AGPL-3.0-or-later
//! Topluluk bileşen veritabanı (Community Component Database) yönetimi.
//! GitHub raw üzerinden güncelleme, SHA-256 ve şema doğrulama, yerel kalıcılık (%APPDATA%\WerSeuS\db\).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub const EMBEDDED_MANIFEST: &str = include_str!("../../data/manifest.json");
pub const EMBEDDED_CELLS: &str = include_str!("../../data/cells.json");
pub const EMBEDDED_PACKS: &str = include_str!("../../data/packs.json");
pub const EMBEDDED_MOTORS: &str = include_str!("../../data/motors.json");
pub const EMBEDDED_PROPS: &str = include_str!("../../data/props.json");
pub const EMBEDDED_ESCS: &str = include_str!("../../data/escs.json");

pub const GITHUB_RAW_BASE_URL: &str =
    "https://raw.githubusercontent.com/werseus-tech/werseus-uav-calc/main/data";

/// Güvenlik İzin Listesi (Whitelist): Yalnızca bilinen ve onaylanmış 5 veri dosyası işlenir.
pub const ALLOWED_DATA_FILES: [&str; 5] = [
    "cells.json",
    "packs.json",
    "motors.json",
    "props.json",
    "escs.json",
];

// ============================================================================
// Veri Modelleri (Data Models)
// ============================================================================

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestFileEntry {
    pub sha256: String,
    pub schema_version: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    pub schema_version: u32,
    pub updated: String,
    pub files: HashMap<String, ManifestFileEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DataFile<T> {
    pub schema_version: u32,
    pub updated: String,
    pub items: Vec<T>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DatabaseCell {
    pub id: String,
    pub name: String,
    pub chemistry: String,
    pub nominal_v: f64,
    pub capacity_ah: f64,
    pub mass_g: f64,
    pub max_cont_a: f64,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub source_url: String,
    #[serde(default)]
    pub source_date: String,
    #[serde(default)]
    pub verified_by: String,
}

impl DatabaseCell {
    pub fn wh(&self) -> f64 {
        self.nominal_v * self.capacity_ah
    }

    pub fn wh_per_kg(&self) -> f64 {
        if self.mass_g > 0.0 {
            self.wh() / (self.mass_g / 1000.0)
        } else {
            0.0
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DatabasePack {
    pub id: String,
    pub name: String,
    pub chemistry: String,
    pub pack_voltage: f64,
    pub s_count: u32,
    pub capacity_mah: f64,
    pub pack_mass_g: f64,
    pub max_cont_a: f64,
    pub c_rating: f64,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub source_url: String,
    #[serde(default)]
    pub source_date: String,
    #[serde(default)]
    pub verified_by: String,
}

impl DatabasePack {
    pub fn capacity_ah(&self) -> f64 {
        self.capacity_mah / 1000.0
    }
    pub fn wh(&self) -> f64 {
        self.pack_voltage * self.capacity_ah()
    }
    pub fn wh_per_kg(&self) -> f64 {
        if self.pack_mass_g > 0.0 {
            self.wh() / (self.pack_mass_g / 1000.0)
        } else {
            0.0
        }
    }
}

/// Motor, pervane ve ESC için genel bileşen yapısı (boş liste geçerlidir).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DatabaseComponentItem {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub source_url: String,
    #[serde(default)]
    pub source_date: String,
    #[serde(default)]
    pub verified_by: String,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DbSource {
    Embedded,
    Community,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseInfo {
    pub source: DbSource,
    pub schema_version: u32,
    pub updated: String,
    pub cell_count: usize,
    pub pack_count: usize,
    pub motor_count: usize,
    pub prop_count: usize,
    pub esc_count: usize,
}

impl Default for DatabaseInfo {
    fn default() -> Self {
        Self {
            source: DbSource::Embedded,
            schema_version: 1,
            updated: "2026-11-10".into(),
            cell_count: 0,
            pack_count: 0,
            motor_count: 0,
            prop_count: 0,
            esc_count: 0,
        }
    }
}

// ============================================================================
// Dizin Yönetimi (Directory Management)
// ============================================================================

/// Üretim ortamında aktif veritabanı dizinini döner (%APPDATA%\WerSeuS\db\).
/// Güvenlik notu: Çevre değişkeni (WERSEUS_DB_DIR) desteği kaldırılmıştır;
/// böylece kaldırma taramasının (uninstaller) keyfi sistem klasörlerini silmesi engellenir.
pub fn db_dir() -> PathBuf {
    crate::support::app_data_dir().join("db")
}

// ============================================================================
// Standart SHA-256 (NIST FIPS 180-4) ve Satır Sonu Normalizasyonu
// ============================================================================

/// CRLF (\r\n) satır sonlarını LF (\n) ile değiştirerek byte dizisini normalize eder.
/// Windows'ta `core.autocrlf=true` ile checkout edilmiş dosyalarda veya derleme anında
/// `include_str!` makrosu kaynaklı satır sonu farklılıklarında deterministik SHA-256
/// hash üretilmesini garanti eder.
pub fn normalize_crlf_to_lf(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        if data[i] == b'\r' && i + 1 < data.len() && data[i + 1] == b'\n' {
            out.push(b'\n');
            i += 2;
        } else {
            out.push(data[i]);
            i += 1;
        }
    }
    out
}

/// Standart SHA-256 özetini hex string olarak hesaplar (ham byte girdisi).
pub fn compute_sha256(data: &[u8]) -> String {
    let hash = sha256_digest(data);
    let mut out = String::with_capacity(64);
    for b in hash {
        use std::fmt::Write;
        let _ = write!(out, "{:02x}", b);
    }
    out
}

/// Metin/JSON verisini satır sonu normalizasyonundan (CRLF -> LF) geçirerek
/// deterministik SHA-256 hash hesaplar.
pub fn compute_sha256_normalized_text(data: &[u8]) -> String {
    let normalized = normalize_crlf_to_lf(data);
    compute_sha256(&normalized)
}

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

#[allow(clippy::chunks_exact_to_as_chunks)]
fn sha256_digest(input: &[u8]) -> [u8; 32] {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    let bit_len = (input.len() as u64).wrapping_mul(8);
    let mut msg = input.to_vec();
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0x00);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut h_var = h[7];

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h_var
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h_var = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(h_var);
    }

    let mut out = [0u8; 32];
    for (i, val) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&val.to_be_bytes());
    }
    out
}

// ============================================================================
// Veritabanı Yükleme (Database Loading)
// ============================================================================

pub fn load_embedded_cells() -> Vec<DatabaseCell> {
    serde_json::from_str::<DataFile<DatabaseCell>>(EMBEDDED_CELLS)
        .map(|df| df.items)
        .unwrap_or_default()
}

pub fn load_embedded_packs() -> Vec<DatabasePack> {
    serde_json::from_str::<DataFile<DatabasePack>>(EMBEDDED_PACKS)
        .map(|df| df.items)
        .unwrap_or_default()
}

pub fn load_embedded_motors() -> Vec<DatabaseComponentItem> {
    serde_json::from_str::<DataFile<DatabaseComponentItem>>(EMBEDDED_MOTORS)
        .map(|df| df.items)
        .unwrap_or_default()
}

pub fn load_embedded_props() -> Vec<DatabaseComponentItem> {
    serde_json::from_str::<DataFile<DatabaseComponentItem>>(EMBEDDED_PROPS)
        .map(|df| df.items)
        .unwrap_or_default()
}

pub fn load_embedded_escs() -> Vec<DatabaseComponentItem> {
    serde_json::from_str::<DataFile<DatabaseComponentItem>>(EMBEDDED_ESCS)
        .map(|df| df.items)
        .unwrap_or_default()
}

pub fn load_embedded_manifest() -> Manifest {
    serde_json::from_str::<Manifest>(EMBEDDED_MANIFEST).unwrap_or(Manifest {
        schema_version: 1,
        updated: "2026-11-10".into(),
        files: HashMap::new(),
    })
}

pub fn load_active_database() -> (Vec<DatabaseCell>, Vec<DatabasePack>, DatabaseInfo) {
    let dir = db_dir();
    load_active_database_from_dir(&dir)
}

pub fn load_active_database_from_dir(
    dir: &Path,
) -> (Vec<DatabaseCell>, Vec<DatabasePack>, DatabaseInfo) {
    let manifest_path = dir.join("manifest.json");

    if manifest_path.is_file() {
        if let Ok(manifest_bytes) = std::fs::read(&manifest_path) {
            if let Ok(manifest) = serde_json::from_slice::<Manifest>(&manifest_bytes) {
                if manifest.schema_version == 1 {
                    if let (Some(cells), Some(packs)) = (
                        validate_and_load_file::<DatabaseCell>(dir, "cells.json", &manifest),
                        validate_and_load_file::<DatabasePack>(dir, "packs.json", &manifest),
                    ) {
                        let motors = validate_and_load_file::<DatabaseComponentItem>(
                            dir,
                            "motors.json",
                            &manifest,
                        )
                        .unwrap_or_default();
                        let props = validate_and_load_file::<DatabaseComponentItem>(
                            dir,
                            "props.json",
                            &manifest,
                        )
                        .unwrap_or_default();
                        let escs = validate_and_load_file::<DatabaseComponentItem>(
                            dir,
                            "escs.json",
                            &manifest,
                        )
                        .unwrap_or_default();

                        let info = DatabaseInfo {
                            source: DbSource::Community,
                            schema_version: manifest.schema_version,
                            updated: manifest.updated,
                            cell_count: cells.len(),
                            pack_count: packs.len(),
                            motor_count: motors.len(),
                            prop_count: props.len(),
                            esc_count: escs.len(),
                        };
                        return (cells, packs, info);
                    }
                }
            }
        }
    }

    // Doğrulama başarısızsa veya yerel dosya yoksa gömülü kopyaya fallback
    let cells = load_embedded_cells();
    let packs = load_embedded_packs();
    let manifest = load_embedded_manifest();
    let info = DatabaseInfo {
        source: DbSource::Embedded,
        schema_version: manifest.schema_version,
        updated: manifest.updated,
        cell_count: cells.len(),
        pack_count: packs.len(),
        motor_count: load_embedded_motors().len(),
        prop_count: load_embedded_props().len(),
        esc_count: load_embedded_escs().len(),
    };
    (cells, packs, info)
}

fn validate_and_load_file<T: serde::de::DeserializeOwned>(
    dir: &Path,
    filename: &str,
    manifest: &Manifest,
) -> Option<Vec<T>> {
    let file_entry = manifest.files.get(filename)?;
    let path = dir.join(filename);
    let bytes = std::fs::read(&path).ok()?;

    let computed_hash = compute_sha256_normalized_text(&bytes);
    if !computed_hash.eq_ignore_ascii_case(&file_entry.sha256) {
        crate::support::log_warn(&format!(
            "Veritabanı dosyası SHA-256 uyuşmazlığı ({filename}): beklenen {}, hesaplanan {}",
            file_entry.sha256, computed_hash
        ));
        return None;
    }

    let parsed = serde_json::from_slice::<DataFile<T>>(&bytes).ok()?;
    if parsed.schema_version != file_entry.schema_version {
        crate::support::log_warn(&format!(
            "Veritabanı dosyası şema sürümü uyuşmazlığı ({filename})"
        ));
        return None;
    }

    Some(parsed.items)
}

pub fn reset_to_embedded() -> Result<(), String> {
    reset_to_embedded_in_dir(&db_dir())
}

pub fn reset_to_embedded_in_dir(dir: &Path) -> Result<(), String> {
    if dir.exists() {
        for file in ALLOWED_DATA_FILES.iter().chain(&["manifest.json"]) {
            let p = dir.join(file);
            if p.exists() {
                let _ = std::fs::remove_file(p);
            }
        }
    }
    crate::support::log_info("Topluluk veritabanı temizlendi, gömülü listeye dönüldü.");
    Ok(())
}

// ============================================================================
// Ağ İstemcisi ve Güncelleme (HTTP Fetcher & Updater)
// ============================================================================

pub trait HttpFetcher: Send + Sync {
    fn get(&self, url: &str) -> Result<Vec<u8>, String>;
}

pub struct UreqFetcher {
    pub timeout: std::time::Duration,
}

impl Default for UreqFetcher {
    fn default() -> Self {
        Self {
            timeout: std::time::Duration::from_secs(10),
        }
    }
}

impl HttpFetcher for UreqFetcher {
    fn get(&self, url: &str) -> Result<Vec<u8>, String> {
        let mut builder = ureq::AgentBuilder::new()
            .timeout_connect(self.timeout)
            .timeout_read(self.timeout)
            .timeout_write(self.timeout);

        if let Ok(connector) = native_tls::TlsConnector::new() {
            builder = builder.tls_connector(Arc::new(connector));
        }

        let agent = builder.build();
        let resp = agent
            .get(url)
            .set("User-Agent", "WerSeuS-UAV-Calc-DbUpdater")
            .call()
            .map_err(|e| match e {
                ureq::Error::Status(404, _) => format!("404 Not Found: {url}"),
                ureq::Error::Status(code, _) => format!("HTTP {code}: {url}"),
                ureq::Error::Transport(t) => format!("Ağ/Taşıyıcı Hatası: {t}"),
            })?;

        let mut bytes = Vec::new();
        use std::io::Read;
        resp.into_reader()
            .take(10 * 1024 * 1024) // azami 10 MB güvenlik sınırı
            .read_to_end(&mut bytes)
            .map_err(|e| format!("İçerik okuma hatası: {e}"))?;

        Ok(bytes)
    }
}

/// Topluluk bileşen veritabanını uzak depodan indirir ve doğrular.
///
/// # Güvenlik Notları
/// - **Manifest İmzası:** Manifest dosyası kriptografik olarak imzalanmamıştır.
///   SHA-256 hash kontrolleri veri aktarımı sırasındaki bozulmaları ve ağ hatalarını tespit eder;
///   ancak GitHub deposunun ele geçirilmesi veya yetkisiz değiştirilmesi durumuna karşı dijital
///   imza güvencesi sağlamaz.
/// - **Dizin Aşma Koruması:** Manifest içindeki dosya adları kesin izin listesi (`ALLOWED_DATA_FILES`)
///   ile denetlenir. Yalnızca bilinen 5 dosya adı kabul edilir, `..` veya dizin ayraçları içeren
///   herhangi bir giriş güvenlik ihlali sayılarak reddedilir.
/// - **Atomik Yazma ve İptal Emniyeti:** İndirilen dosyalar önce geçici dosyalara (.tmp) yazılır.
///   Yazma öncesinde ve sırasında iptal bayrağı kontrol edilir; işlem iptal edilirse hedef dosyalar
///   asla değiştirilmez ve geçici dosyalar temizlenir.
pub fn update_database_with_fetcher(
    target_dir: &Path,
    fetcher: &dyn HttpFetcher,
    cancel_flag: Option<&AtomicBool>,
) -> Result<DatabaseInfo, String> {
    if let Some(flag) = cancel_flag {
        if flag.load(Ordering::Relaxed) {
            return Err("İşlem kullanıcı tarafından iptal edildi.".into());
        }
    }

    let manifest_url = format!("{GITHUB_RAW_BASE_URL}/manifest.json");
    crate::support::log_info(&format!("Veritabanı manifesti indiriliyor: {manifest_url}"));

    let manifest_bytes = fetcher.get(&manifest_url)?;

    let manifest: Manifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| format!("Manifest ayrıştırma hatası (geçersiz JSON): {e}"))?;

    if manifest.schema_version != 1 {
        return Err(format!(
            "Desteklenmeyen manifest şema sürümü: {}",
            manifest.schema_version
        ));
    }

    let mut downloaded_files: Vec<(String, Vec<u8>)> = Vec::new();

    for (filename, entry) in &manifest.files {
        if let Some(flag) = cancel_flag {
            if flag.load(Ordering::Relaxed) {
                return Err("İşlem kullanıcı tarafından iptal edildi.".into());
            }
        }

        // İzin listesi (whitelist) kontrolü: Yalnızca bilinen 5 dosya kabul edilir
        if !ALLOWED_DATA_FILES.contains(&filename.as_str()) {
            return Err(format!(
                "Güvenlik hatası: Manifest içinde onaylanmamış dosya adı ('{filename}'). Yalnızca izin verilen veri dosyaları güncellenebilir."
            ));
        }

        // Dizin aşma (directory traversal) koruması
        if filename.contains('/') || filename.contains('\\') || filename.contains("..") {
            return Err(format!(
                "Güvenlik hatası: Dosya adında geçersiz yol ayracı tespit edildi ('{filename}')."
            ));
        }

        let file_url = format!("{GITHUB_RAW_BASE_URL}/{filename}");
        crate::support::log_info(&format!("Dosya indiriliyor: {file_url}"));

        let file_bytes = fetcher.get(&file_url)?;

        let computed_hash = compute_sha256_normalized_text(&file_bytes);
        if !computed_hash.eq_ignore_ascii_case(&entry.sha256) {
            return Err(format!(
                "SHA-256 doğrulama hatası ({filename}): beklenen {}, gelen {}",
                entry.sha256, computed_hash
            ));
        }

        // Şema doğrulaması
        if filename == "cells.json" {
            let cells_file: DataFile<DatabaseCell> = serde_json::from_slice(&file_bytes)
                .map_err(|e| format!("{filename} ayrıştırma hatası: {e}"))?;
            if cells_file.schema_version != entry.schema_version {
                return Err(format!("{filename} şema sürümü uyuşmazlığı"));
            }
        } else if filename == "packs.json" {
            let packs_file: DataFile<DatabasePack> = serde_json::from_slice(&file_bytes)
                .map_err(|e| format!("{filename} ayrıştırma hatası: {e}"))?;
            if packs_file.schema_version != entry.schema_version {
                return Err(format!("{filename} şema sürümü uyuşmazlığı"));
            }
        } else if filename == "motors.json" || filename == "props.json" || filename == "escs.json" {
            let comp_file: DataFile<DatabaseComponentItem> = serde_json::from_slice(&file_bytes)
                .map_err(|e| format!("{filename} ayrıştırma hatası: {e}"))?;
            if comp_file.schema_version != entry.schema_version {
                return Err(format!("{filename} şema sürümü uyuşmazlığı"));
            }
        }

        downloaded_files.push((filename.clone(), file_bytes));
    }

    // Yazma öncesi iptal kontrolü (İptal edildiyse hiçbir dosya yazılmamalı)
    if let Some(flag) = cancel_flag {
        if flag.load(Ordering::Relaxed) {
            return Err("İşlem kullanıcı tarafından iptal edildi.".into());
        }
    }

    // Tüm doğrulamalar başarılı; hedef dizine ATOMİK yazma işlemi
    let _ = std::fs::create_dir_all(target_dir);

    let temp_suffix = format!(".tmp.{}", std::process::id());
    let mut staged_files: Vec<(PathBuf, PathBuf)> = Vec::new();

    for (filename, bytes) in &downloaded_files {
        let dest = target_dir.join(filename);
        let temp_dest = target_dir.join(format!("{filename}{temp_suffix}"));
        if let Err(e) = std::fs::write(&temp_dest, bytes) {
            for (_, staged_tmp) in &staged_files {
                let _ = std::fs::remove_file(staged_tmp);
            }
            let _ = std::fs::remove_file(&temp_dest);
            return Err(format!("Geçici dosya yazılamadı ({filename}): {e}"));
        }
        staged_files.push((dest, temp_dest));
    }

    let manifest_dest = target_dir.join("manifest.json");
    let manifest_temp_dest = target_dir.join(format!("manifest.json{temp_suffix}"));
    if let Err(e) = std::fs::write(&manifest_temp_dest, &manifest_bytes) {
        for (_, staged_tmp) in &staged_files {
            let _ = std::fs::remove_file(staged_tmp);
        }
        let _ = std::fs::remove_file(&manifest_temp_dest);
        return Err(format!("Geçici manifest yazılamadı: {e}"));
    }
    staged_files.push((manifest_dest, manifest_temp_dest));

    // Yeniden adlandırma öncesi son iptal kontrolü
    if let Some(flag) = cancel_flag {
        if flag.load(Ordering::Relaxed) {
            for (_, staged_tmp) in &staged_files {
                let _ = std::fs::remove_file(staged_tmp);
            }
            return Err("İşlem kullanıcı tarafından iptal edildi.".into());
        }
    }

    // Geçici dosyaları hedef konuma yeniden adlandır
    for (final_dest, temp_dest) in &staged_files {
        if final_dest.exists() {
            let _ = std::fs::remove_file(final_dest);
        }
        if let Err(e) = std::fs::rename(temp_dest, final_dest) {
            // Kalan geçici dosyaları temizle
            for (_, staged_tmp) in &staged_files {
                let _ = std::fs::remove_file(staged_tmp);
            }
            return Err(format!(
                "Dosya atomik olarak güncellenemedi ({}): {e}",
                final_dest.display()
            ));
        }
    }

    crate::support::log_info("Topluluk veritabanı başarıyla güncellendi.");

    let (cells, packs, info) = load_active_database_from_dir(target_dir);
    let _ = (cells, packs);
    Ok(info)
}

pub fn update_database(cancel_flag: Option<&AtomicBool>) -> Result<DatabaseInfo, String> {
    let dir = db_dir();
    let fetcher = UreqFetcher::default();
    update_database_with_fetcher(&dir, &fetcher, cancel_flag)
}

// ============================================================================
// Testler (Tests)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    struct MockFetcher {
        responses: HashMap<String, Result<Vec<u8>, String>>,
    }

    impl HttpFetcher for MockFetcher {
        fn get(&self, url: &str) -> Result<Vec<u8>, String> {
            match self.responses.get(url) {
                Some(Ok(bytes)) => Ok(bytes.clone()),
                Some(Err(err)) => Err(err.clone()),
                None => Err(format!("404 Not Found: {url}")),
            }
        }
    }

    #[test]
    fn test_sha256_nist_vectors() {
        assert_eq!(
            compute_sha256(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            compute_sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn test_line_ending_normalization_and_hash() {
        let lf_data = b"line1\nline2\nline3\n";
        let crlf_data = b"line1\r\nline2\r\nline3\r\n";
        let mixed_data = b"line1\r\nline2\nline3\r\n";

        assert_eq!(normalize_crlf_to_lf(lf_data), lf_data.to_vec());
        assert_eq!(normalize_crlf_to_lf(crlf_data), lf_data.to_vec());
        assert_eq!(normalize_crlf_to_lf(mixed_data), lf_data.to_vec());

        let lf_hash = compute_sha256_normalized_text(lf_data);
        let crlf_hash = compute_sha256_normalized_text(crlf_data);
        let mixed_hash = compute_sha256_normalized_text(mixed_data);

        assert_eq!(lf_hash, crlf_hash);
        assert_eq!(lf_hash, mixed_hash);

        // Gömülü hücrelerin CRLF'e dönüştürülmüş kopyası da aynı hash'i vermelidir
        let embedded_crlf = EMBEDDED_CELLS.replace("\r\n", "\n").replace('\n', "\r\n");
        let hash_crlf = compute_sha256_normalized_text(embedded_crlf.as_bytes());
        let hash_lf = compute_sha256_normalized_text(EMBEDDED_CELLS.as_bytes());
        assert_eq!(hash_crlf, hash_lf);
    }

    #[test]
    fn test_embedded_files_valid_and_match_manifest() {
        let manifest = load_embedded_manifest();
        assert_eq!(manifest.schema_version, 1);

        for (filename, embedded_content) in [
            ("cells.json", EMBEDDED_CELLS),
            ("packs.json", EMBEDDED_PACKS),
            ("motors.json", EMBEDDED_MOTORS),
            ("props.json", EMBEDDED_PROPS),
            ("escs.json", EMBEDDED_ESCS),
        ] {
            let entry = manifest
                .files
                .get(filename)
                .unwrap_or_else(|| panic!("Manifestte {filename} eksik"));
            let computed = compute_sha256_normalized_text(embedded_content.as_bytes());
            assert_eq!(
                computed.to_lowercase(),
                entry.sha256.to_lowercase(),
                "Hash mismatch for {filename}"
            );
        }

        let cells = load_embedded_cells();
        assert!(!cells.is_empty());
        for cell in &cells {
            assert!(!cell.id.is_empty());
            assert!(!cell.source.is_empty());
            assert!(!cell.verified_by.is_empty());
            let _ = &cell.source_url;
            let _ = &cell.source_date;
            let wh_kg = cell.wh_per_kg();
            assert!(
                (50.0..=450.0).contains(&wh_kg),
                "Cell {} Wh/kg out of range: {}",
                cell.name,
                wh_kg
            );
        }
    }

    #[test]
    fn test_mock_fetcher_successful_update_and_fallback() {
        let temp_dir =
            std::env::temp_dir().join(format!("werseus_db_test_ok_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        let _ = std::fs::create_dir_all(&temp_dir);

        let mock_cells_content = r#"{
  "schema_version": 1,
  "updated": "2026-11-15",
  "items": [
    {
      "id": "mock_cell_1",
      "name": "Mock Cell 21700",
      "chemistry": "Li-ion",
      "nominal_v": 3.7,
      "capacity_ah": 5.0,
      "mass_g": 70.0,
      "max_cont_a": 50.0,
      "source": "Mock Cell Data Sheet",
      "source_url": "",
      "source_date": "",
      "verified_by": "tester"
    }
  ]
}"#;
        let mock_cells_hash = compute_sha256_normalized_text(mock_cells_content.as_bytes());

        let mock_packs_content = r#"{
  "schema_version": 1,
  "updated": "2026-11-15",
  "items": []
}"#;
        let mock_packs_hash = compute_sha256_normalized_text(mock_packs_content.as_bytes());

        let mock_manifest = format!(
            r#"{{
  "schema_version": 1,
  "updated": "2026-11-15",
  "files": {{
    "cells.json": {{
      "sha256": "{mock_cells_hash}",
      "schema_version": 1
    }},
    "packs.json": {{
      "sha256": "{mock_packs_hash}",
      "schema_version": 1
    }}
  }}
}}"#
        );

        let mut responses = HashMap::new();
        responses.insert(
            format!("{GITHUB_RAW_BASE_URL}/manifest.json"),
            Ok(mock_manifest.into_bytes()),
        );
        responses.insert(
            format!("{GITHUB_RAW_BASE_URL}/cells.json"),
            Ok(mock_cells_content.as_bytes().to_vec()),
        );
        responses.insert(
            format!("{GITHUB_RAW_BASE_URL}/packs.json"),
            Ok(mock_packs_content.as_bytes().to_vec()),
        );

        let fetcher = MockFetcher { responses };

        let result = update_database_with_fetcher(&temp_dir, &fetcher, None);
        assert!(result.is_ok(), "Update failed: {:?}", result.err());

        let info = result.unwrap();
        assert_eq!(info.source, DbSource::Community);
        assert_eq!(info.updated, "2026-11-15");
        assert_eq!(info.cell_count, 1);

        let (active_cells, _, active_info) = load_active_database_from_dir(&temp_dir);
        assert_eq!(active_info.source, DbSource::Community);
        assert_eq!(active_cells.len(), 1);
        assert_eq!(active_cells[0].id, "mock_cell_1");

        // Gömülü listeye dönme
        assert!(reset_to_embedded_in_dir(&temp_dir).is_ok());
        let (emb_cells, _, emb_info) = load_active_database_from_dir(&temp_dir);
        assert_eq!(emb_info.source, DbSource::Embedded);
        assert_ne!(emb_cells[0].id, "mock_cell_1");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_mock_fetcher_whitelist_and_traversal_rejection() {
        let temp_dir =
            std::env::temp_dir().join(format!("werseus_db_test_sec_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);

        // 1. Bilinmeyen dosya adı
        let malicious_manifest = r#"{
  "schema_version": 1,
  "updated": "2026-11-15",
  "files": {
    "malicious_payload.exe": {
      "sha256": "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
      "schema_version": 1
    }
  }
}"#;
        let mut responses = HashMap::new();
        responses.insert(
            format!("{GITHUB_RAW_BASE_URL}/manifest.json"),
            Ok(malicious_manifest.as_bytes().to_vec()),
        );

        let fetcher = MockFetcher { responses };
        let res = update_database_with_fetcher(&temp_dir, &fetcher, None);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("Güvenlik hatası"));

        // 2. Dizin aşma (directory traversal)
        let traversal_manifest = r#"{
  "schema_version": 1,
  "updated": "2026-11-15",
  "files": {
    "../escape.json": {
      "sha256": "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
      "schema_version": 1
    }
  }
}"#;
        let mut responses2 = HashMap::new();
        responses2.insert(
            format!("{GITHUB_RAW_BASE_URL}/manifest.json"),
            Ok(traversal_manifest.as_bytes().to_vec()),
        );

        let fetcher2 = MockFetcher {
            responses: responses2,
        };
        let res2 = update_database_with_fetcher(&temp_dir, &fetcher2, None);
        assert!(res2.is_err());
        assert!(res2.unwrap_err().contains("Güvenlik hatası"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_mock_fetcher_cancellation_before_write() {
        let temp_dir =
            std::env::temp_dir().join(format!("werseus_db_test_cancel_w_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);

        let flag = Arc::new(AtomicBool::new(false));

        let mock_cells_content = r#"{"schema_version":1,"updated":"2026-11-15","items":[]}"#;
        let mock_cells_hash = compute_sha256_normalized_text(mock_cells_content.as_bytes());

        let mock_manifest = format!(
            r#"{{
  "schema_version": 1,
  "updated": "2026-11-15",
  "files": {{
    "cells.json": {{
      "sha256": "{mock_cells_hash}",
      "schema_version": 1
    }}
  }}
}}"#
        );

        struct CancellingFetcher {
            manifest: Vec<u8>,
            cells: Vec<u8>,
            flag: Arc<AtomicBool>,
        }

        impl HttpFetcher for CancellingFetcher {
            fn get(&self, url: &str) -> Result<Vec<u8>, String> {
                if url.ends_with("manifest.json") {
                    Ok(self.manifest.clone())
                } else if url.ends_with("cells.json") {
                    // Hücreyi indirdikten hemen sonra iptal bayrağını kaldır
                    self.flag.store(true, Ordering::Relaxed);
                    Ok(self.cells.clone())
                } else {
                    Err("404".into())
                }
            }
        }

        let fetcher = CancellingFetcher {
            manifest: mock_manifest.into_bytes(),
            cells: mock_cells_content.as_bytes().to_vec(),
            flag: flag.clone(),
        };

        let res = update_database_with_fetcher(&temp_dir, &fetcher, Some(&flag));
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("iptal"));

        // Hedef dizinde manifest.json ve cells.json asla oluşturulmamış olmalı
        assert!(!temp_dir.join("manifest.json").exists());
        assert!(!temp_dir.join("cells.json").exists());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_mock_fetcher_404_falls_back_gracefully() {
        let temp_dir =
            std::env::temp_dir().join(format!("werseus_db_test_404_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);

        let fetcher = MockFetcher {
            responses: HashMap::new(), // her şey 404
        };

        let res = update_database_with_fetcher(&temp_dir, &fetcher, None);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("404"));

        // Fallback sağlam çalışmalı
        let (_, _, info) = load_active_database_from_dir(&temp_dir);
        assert_eq!(info.source, DbSource::Embedded);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_mock_fetcher_corrupted_json_rejected() {
        let temp_dir =
            std::env::temp_dir().join(format!("werseus_db_test_corrupt_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);

        let mut responses = HashMap::new();
        responses.insert(
            format!("{GITHUB_RAW_BASE_URL}/manifest.json"),
            Ok(b"not json at all".to_vec()),
        );

        let fetcher = MockFetcher { responses };
        let res = update_database_with_fetcher(&temp_dir, &fetcher, None);
        assert!(res.is_err());

        let (_, _, info) = load_active_database_from_dir(&temp_dir);
        assert_eq!(info.source, DbSource::Embedded);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_mock_fetcher_hash_mismatch_rejected() {
        let temp_dir =
            std::env::temp_dir().join(format!("werseus_db_test_hash_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);

        let mock_manifest = r#"{
  "schema_version": 1,
  "updated": "2026-11-15",
  "files": {
    "cells.json": {
      "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
      "schema_version": 1
    }
  }
}"#;
        let mut responses = HashMap::new();
        responses.insert(
            format!("{GITHUB_RAW_BASE_URL}/manifest.json"),
            Ok(mock_manifest.as_bytes().to_vec()),
        );
        responses.insert(
            format!("{GITHUB_RAW_BASE_URL}/cells.json"),
            Ok(b"{\"schema_version\":1,\"updated\":\"2026-11-15\",\"items\":[]}".to_vec()),
        );

        let fetcher = MockFetcher { responses };
        let res = update_database_with_fetcher(&temp_dir, &fetcher, None);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("SHA-256"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_mock_fetcher_schema_version_mismatch_rejected() {
        let temp_dir =
            std::env::temp_dir().join(format!("werseus_db_test_schema_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);

        let mock_manifest = r#"{
  "schema_version": 99,
  "updated": "2026-11-15",
  "files": {}
}"#;
        let mut responses = HashMap::new();
        responses.insert(
            format!("{GITHUB_RAW_BASE_URL}/manifest.json"),
            Ok(mock_manifest.as_bytes().to_vec()),
        );

        let fetcher = MockFetcher { responses };
        let res = update_database_with_fetcher(&temp_dir, &fetcher, None);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("şema sürümü"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_motors_props_escs_schema_validation() {
        let temp_dir =
            std::env::temp_dir().join(format!("werseus_db_test_mpescs_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);

        let mock_motors = r#"{"schema_version":2,"updated":"2026-11-15","items":[]}"#;
        let mock_motors_hash = compute_sha256_normalized_text(mock_motors.as_bytes());

        let mock_manifest = format!(
            r#"{{
  "schema_version": 1,
  "updated": "2026-11-15",
  "files": {{
    "motors.json": {{
      "sha256": "{mock_motors_hash}",
      "schema_version": 1
    }}
  }}
}}"#
        );

        let mut responses = HashMap::new();
        responses.insert(
            format!("{GITHUB_RAW_BASE_URL}/manifest.json"),
            Ok(mock_manifest.into_bytes()),
        );
        responses.insert(
            format!("{GITHUB_RAW_BASE_URL}/motors.json"),
            Ok(mock_motors.as_bytes().to_vec()),
        );

        let fetcher = MockFetcher { responses };
        let res = update_database_with_fetcher(&temp_dir, &fetcher, None);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("şema sürümü"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_werseus_db_dir_env_var_ignored() {
        let fake_path = "C:\\WerSeuS_Arbitrary_Fake_Path_12345";
        unsafe {
            std::env::set_var("WERSEUS_DB_DIR", fake_path);
        }
        let actual = db_dir();
        unsafe {
            std::env::remove_var("WERSEUS_DB_DIR");
        }

        assert_ne!(
            actual,
            PathBuf::from(fake_path),
            "db_dir() must ignore WERSEUS_DB_DIR environment variable"
        );
        assert!(
            actual.ends_with(Path::new("WerSeuS").join("db"))
                || actual.ends_with(Path::new("test_app_data").join("db"))
                || actual.ends_with(Path::new(".werseus_test_app_data").join("db"))
                || actual.ends_with(Path::new("db"))
        );
    }

    #[test]
    fn test_cancellation() {
        let temp_dir =
            std::env::temp_dir().join(format!("werseus_db_test_cancel_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);

        let flag = AtomicBool::new(true); // önceden iptal edilmiş
        let fetcher = MockFetcher {
            responses: HashMap::new(),
        };

        let res = update_database_with_fetcher(&temp_dir, &fetcher, Some(&flag));
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("iptal"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
