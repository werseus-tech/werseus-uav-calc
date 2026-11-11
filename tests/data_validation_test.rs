// SPDX-License-Identifier: AGPL-3.0-or-later
//! Topluluk veritabanı (data/) doğrulama entegrasyon testi.
//! Depodaki tüm JSON dosyalarının manifest hash'i, şema sürümleri, benzersiz ID'leri
//! ve Wh/kg fiziksel tutarlılığı doğrulanır.

use std::collections::HashSet;
use std::path::PathBuf;
use werseus_uav_calc::db::{self, DataFile, DatabaseCell, DatabasePack, Manifest};

fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data")
}

#[test]
fn test_repository_manifest_and_data_files_integrity() {
    let dir = data_dir();
    let manifest_path = dir.join("manifest.json");
    assert!(manifest_path.is_file(), "manifest.json bulunamadı!");

    let manifest_bytes = std::fs::read(&manifest_path).expect("manifest.json okunamadı");
    let manifest: Manifest =
        serde_json::from_slice(&manifest_bytes).expect("manifest.json geçersiz JSON");

    assert_eq!(manifest.schema_version, 1, "schema_version 1 olmalı");
    assert!(!manifest.updated.is_empty(), "updated tarihi boş olamaz");
    assert!(!manifest.files.is_empty(), "files boş olamaz");

    let required_files = [
        "cells.json",
        "packs.json",
        "motors.json",
        "props.json",
        "escs.json",
    ];
    for required in required_files {
        assert!(
            manifest.files.contains_key(required),
            "Manifest içinde '{}' eksik!",
            required
        );
    }

    for (filename, entry) in &manifest.files {
        let file_path = dir.join(filename);
        assert!(
            file_path.is_file(),
            "Manifestte kayıtlı dosya bulunamadı: {}",
            filename
        );

        let file_bytes = std::fs::read(&file_path).expect("Dosya okunamadı");
        let hash = db::compute_sha256_normalized_text(&file_bytes);
        assert_eq!(
            hash.to_lowercase(),
            entry.sha256.to_lowercase(),
            "SHA-256 uyuşmazlığı: {}",
            filename
        );

        // Dosya özel kontrolleri
        if filename == "cells.json" {
            let cells_file: DataFile<DatabaseCell> =
                serde_json::from_slice(&file_bytes).expect("cells.json parse hatası");
            assert_eq!(cells_file.schema_version, entry.schema_version);

            let mut ids = HashSet::new();
            for cell in &cells_file.items {
                assert!(!cell.id.trim().is_empty(), "Cell id boş olamaz");
                assert!(
                    cell.id
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                    "Cell id yalnızca ASCII küçük harf, rakam ve alt çizgi içerebilir: {}",
                    cell.id
                );
                assert!(
                    ids.insert(cell.id.clone()),
                    "Mükerrer cell id bulundu: {}",
                    cell.id
                );

                assert!(!cell.name.trim().is_empty(), "Cell name boş olamaz");
                assert!(
                    !cell.source.trim().is_empty(),
                    "Cell source (veri sayfası) boş olamaz"
                );
                // source_url ve source_date alanları mevcut olmalı (bilinmiyorsa boş dize, uydurma yasak)
                let _ = &cell.source_url;
                let _ = &cell.source_date;
                assert!(
                    !cell.verified_by.trim().is_empty(),
                    "Cell verified_by boş olamaz"
                );

                assert!(cell.nominal_v > 0.0 && cell.nominal_v <= 5.5);
                assert!(cell.capacity_ah > 0.0 && cell.capacity_ah <= 100.0);
                assert!(cell.mass_g > 0.0 && cell.mass_g <= 2000.0);
                assert!(cell.max_cont_a > 0.0);

                let wh_kg = cell.wh_per_kg();
                assert!(
                    (50.0..=450.0).contains(&wh_kg),
                    "Cell {} Wh/kg makul fiziksel aralık dışında: {}",
                    cell.name,
                    wh_kg
                );
            }
        } else if filename == "packs.json" {
            let packs_file: DataFile<DatabasePack> =
                serde_json::from_slice(&file_bytes).expect("packs.json parse hatası");
            assert_eq!(packs_file.schema_version, entry.schema_version);

            let mut ids = HashSet::new();
            for pack in &packs_file.items {
                assert!(ids.insert(pack.id.clone()), "Mükerrer pack id: {}", pack.id);
                assert!(!pack.source.trim().is_empty());
                assert!(!pack.verified_by.trim().is_empty());
            }
        }
    }
}

#[test]
fn test_legacy_project_json_backwards_compatible_with_community_db() {
    let legacy_json = r#"{
        "name": "Eski v0.0.1 Tasarımı",
        "drone_type": "FixedWingConventional",
        "cell_name": "Molicel P42A 21700",
        "battery_source": "Library",
        "n_series": 4,
        "n_parallel": 2,
        "m_payload": 0.3,
        "b": 1.4,
        "s": 0.28
    }"#;

    let p: werseus_uav_calc::project::Project =
        serde_json::from_str(legacy_json).expect("Eski JSON okunamadı!");
    assert_eq!(p.cell_name, "Molicel P42A 21700");

    let lib = werseus_uav_calc::calc::Library::load_from(&data_dir());
    let cell = lib.cell(&p.cell_name);
    assert_eq!(cell.name, "Molicel P42A 21700");
    assert_eq!(cell.nominal_v, 3.6);
    assert_eq!(cell.capacity_ah, 4.2);

    let m = werseus_uav_calc::calc::evaluate(&p, &lib);
    // 4S2P = 8 cells * 70g = 560g * 1.08 = ~0.605 kg
    assert!(m.weight.m_battery > 0.55 && m.weight.m_battery < 0.65);
    // 4 * 3.6 V = 14.4 V
    assert!((m.weight.pack_voltage - 14.4).abs() < 1e-4);
    // 8 * 3.6 * 4.2 = 120.96 Wh
    assert!((m.weight.pack_wh - 120.96).abs() < 1e-4);
}
