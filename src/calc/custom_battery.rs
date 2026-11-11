// SPDX-License-Identifier: AGPL-3.0-or-later
//! Özel batarya yönetimi ve yerel kalıcılık (%APPDATA%\WerSeuS\custom_batteries.json).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CustomCellData {
    pub name: String,
    pub chemistry: String,
    pub nominal_v: f64,
    pub capacity_ah: f64,
    pub mass_g: f64,
    pub max_cont_a: f64,
    pub r_int_mohm: f64,
}

impl Default for CustomCellData {
    fn default() -> Self {
        Self {
            name: "Özel Hücre".into(),
            chemistry: "LiPo".into(),
            nominal_v: 3.7,
            capacity_ah: 4.5,
            mass_g: 65.0,
            max_cont_a: 45.0,
            r_int_mohm: 2.0,
        }
    }
}

impl CustomCellData {
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
pub struct CustomPackData {
    pub name: String,
    pub chemistry: String,
    pub pack_voltage: f64,
    pub s_count: u32,
    pub capacity_mah: f64,
    pub pack_mass_g: f64,
    pub max_cont_a: f64,
    pub c_rating: f64,
}

impl Default for CustomPackData {
    fn default() -> Self {
        Self {
            name: "Özel Hazır Paket".into(),
            chemistry: "LiPo".into(),
            pack_voltage: 14.8,
            s_count: 4,
            capacity_mah: 5000.0,
            pack_mass_g: 480.0,
            max_cont_a: 150.0,
            c_rating: 30.0,
        }
    }
}

impl CustomPackData {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BatterySourceKind {
    #[default]
    Library,
    GenericTemplate,
    CustomCell,
    CustomPack,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GenericBatteryTemplate {
    #[default]
    LiIon21700,
    LiIon18650,
    LiPoGeneric,
    LiIonSemiSolid,
}

impl GenericBatteryTemplate {
    pub fn default_cell(&self) -> CustomCellData {
        match self {
            Self::LiIon21700 => CustomCellData {
                name: "Genel Li-ion 21700 (Tipik)".into(),
                chemistry: "Li-ion".into(),
                nominal_v: 3.6,
                capacity_ah: 4.5,
                mass_g: 70.0,
                max_cont_a: 35.0,
                r_int_mohm: 12.0,
            },
            Self::LiIon18650 => CustomCellData {
                name: "Genel Li-ion 18650 (Tipik)".into(),
                chemistry: "Li-ion".into(),
                nominal_v: 3.6,
                capacity_ah: 3.0,
                mass_g: 47.0,
                max_cont_a: 15.0,
                r_int_mohm: 20.0,
            },
            Self::LiPoGeneric => CustomCellData {
                name: "Genel LiPo (Tipik ~150 Wh/kg)".into(),
                chemistry: "LiPo".into(),
                nominal_v: 3.7,
                capacity_ah: 5.0,
                // ~150 Wh/kg -> 3.7 * 5.0 / 0.150 = 123.3 g
                mass_g: 123.0,
                max_cont_a: 300.0, // 60C * 5Ah
                r_int_mohm: 1.5,
            },
            Self::LiIonSemiSolid => CustomCellData {
                name: "Genel Yarı Katı / Silikon Anot (Tipik ~300 Wh/kg)".into(),
                chemistry: "Li-ion Solid".into(),
                nominal_v: 3.6,
                capacity_ah: 4.5,
                // ~300 Wh/kg -> 3.6 * 4.5 / 0.300 = 54 g
                mass_g: 54.0,
                max_cont_a: 30.0,
                r_int_mohm: 25.0,
            },
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SavedCustomBattery {
    pub id: String,
    pub name: String,
    pub is_pack: bool,
    pub cell: Option<CustomCellData>,
    pub pack: Option<CustomPackData>,
}

pub fn custom_batteries_path() -> PathBuf {
    if let Ok(p) = std::env::var("WERSEUS_CUSTOM_BATTERIES_PATH") {
        return PathBuf::from(p);
    }
    #[cfg(windows)]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            let dir = PathBuf::from(appdata).join("WerSeuS");
            let _ = std::fs::create_dir_all(&dir);
            return dir.join("custom_batteries.json");
        }
    }
    #[cfg(not(windows))]
    {
        if let Some(home) = std::env::var_os("HOME") {
            let dir = PathBuf::from(home).join(".config").join("WerSeuS");
            let _ = std::fs::create_dir_all(&dir);
            return dir.join("custom_batteries.json");
        }
    }
    PathBuf::from("custom_batteries.json")
}

pub fn load_saved_batteries() -> Vec<SavedCustomBattery> {
    let p = custom_batteries_path();
    if p.exists() {
        if let Ok(data) = std::fs::read_to_string(&p) {
            if let Ok(list) = serde_json::from_str::<Vec<SavedCustomBattery>>(&data) {
                return list;
            }
        }
    }
    Vec::new()
}

pub fn save_custom_battery(item: SavedCustomBattery) -> Result<(), String> {
    let mut list = load_saved_batteries();
    list.retain(|x| x.id != item.id);
    list.push(item);
    let p = custom_batteries_path();
    if let Some(parent) = p.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let data = serde_json::to_string_pretty(&list).map_err(|e| e.to_string())?;
    std::fs::write(&p, data).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn delete_saved_battery(id: &str) -> Result<(), String> {
    let mut list = load_saved_batteries();
    list.retain(|x| x.id != id);
    let p = custom_batteries_path();
    let data = serde_json::to_string_pretty(&list).map_err(|e| e.to_string())?;
    std::fs::write(&p, data).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_custom_battery_crud_isolation() {
        let temp_dir = std::env::temp_dir().join(format!("werseus_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let test_file = temp_dir.join("test_custom_batteries.json");

        // Güvenli ortam değişkeni ayarla
        unsafe {
            std::env::set_var("WERSEUS_CUSTOM_BATTERIES_PATH", test_file.to_str().unwrap());
        }

        let item = SavedCustomBattery {
            id: "test_bat_1".into(),
            name: "Test LiPo 6S".into(),
            is_pack: true,
            cell: None,
            pack: Some(CustomPackData {
                name: "Test LiPo 6S".into(),
                chemistry: "LiPo".into(),
                pack_voltage: 22.2,
                s_count: 6,
                capacity_mah: 6000.0,
                pack_mass_g: 800.0,
                max_cont_a: 180.0,
                c_rating: 30.0,
            }),
        };

        save_custom_battery(item.clone()).expect("Kaydedilemedi");
        let list = load_saved_batteries();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "test_bat_1");
        assert_eq!(list[0].pack.as_ref().unwrap().pack_voltage, 22.2);

        delete_saved_battery("test_bat_1").expect("Silinemedi");
        let list2 = load_saved_batteries();
        assert_eq!(list2.len(), 0);

        unsafe {
            std::env::remove_var("WERSEUS_CUSTOM_BATTERIES_PATH");
        }
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    use crate::calc::{Library, evaluate};
    use crate::project::Project;

    fn near(a: f64, b: f64) {
        assert!((a - b).abs() <= 1e-6 * b.abs().max(1.0), "{a} != {b}");
    }

    #[test]
    fn cell_and_pack_wh_and_wh_per_kg() {
        let c = CustomCellData::default();
        near(c.wh(), 3.7 * 4.5);
        near(c.wh_per_kg(), 3.7 * 4.5 / 0.065);
        let z = CustomCellData { mass_g: 0.0, ..c };
        near(z.wh_per_kg(), 0.0);

        let p = CustomPackData::default();
        near(p.capacity_ah(), 5.0);
        near(p.wh(), 14.8 * 5.0);
        near(p.wh_per_kg(), 14.8 * 5.0 / 0.48);
        let zp = CustomPackData {
            pack_mass_g: 0.0,
            ..p
        };
        near(zp.wh_per_kg(), 0.0);
    }

    #[test]
    fn generic_templates_have_plausible_energy_density() {
        let lipo = GenericBatteryTemplate::LiPoGeneric
            .default_cell()
            .wh_per_kg();
        assert!((140.0..160.0).contains(&lipo), "{lipo}");
        let semi = GenericBatteryTemplate::LiIonSemiSolid
            .default_cell()
            .wh_per_kg();
        assert!((290.0..310.0).contains(&semi), "{semi}");
    }

    #[test]
    fn evaluate_energy_and_current_per_source_kind() {
        let lib = Library::embedded();

        // Genel şablon: 6S2P
        let mut p = Project::default();
        p.battery_source = BatterySourceKind::GenericTemplate;
        p.generic_cell = GenericBatteryTemplate::LiIon21700.default_cell();
        p.n_series = 6;
        p.n_parallel = 2;
        let m = evaluate(&p, &lib);
        near(m.weight.pack_wh, 12.0 * 3.6 * 4.5);
        near(m.weight.pack_voltage, 6.0 * 3.6);
        near(m.prop_in.pack_max_cont_a, 2.0 * 35.0);

        // Özel hücre: 4S3P
        p.battery_source = BatterySourceKind::CustomCell;
        p.custom_cell = CustomCellData::default();
        p.n_series = 4;
        p.n_parallel = 3;
        let m = evaluate(&p, &lib);
        near(m.weight.pack_wh, 12.0 * 3.7 * 4.5);
        near(m.prop_in.pack_max_cont_a, 3.0 * 45.0);

        // Özel paket: n_parallel Project'te 3 olsa bile paket tek parçadır -> akım tek paket değeri
        p.battery_source = BatterySourceKind::CustomPack;
        p.custom_pack = CustomPackData::default(); // 14.8 V, 5 Ah, 150 A
        p.n_parallel = 3;
        let m = evaluate(&p, &lib);
        near(m.weight.pack_wh, 14.8 * 5.0);
        near(m.weight.pack_voltage, 14.8);
        near(m.prop_in.pack_max_cont_a, 150.0);
        near(m.weight.m_battery, 0.48);

        // max_cont_a = 0 ise C-oranından türetilir: 30C * 5 Ah
        p.custom_pack.max_cont_a = 0.0;
        let m = evaluate(&p, &lib);
        near(m.prop_in.pack_max_cont_a, 30.0 * 5.0);
    }
}
