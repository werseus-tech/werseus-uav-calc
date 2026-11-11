// SPDX-License-Identifier: AGPL-3.0-or-later
//! Ağırlık dökümü: yapı kütle tahmini, batarya paketi, MTOW. SI birimleri (kg, m).
use super::atmosphere::G;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Material {
    /// Sabit benzersiz malzeme kimliği (dil ve ad değişimlerinden bağımsız).
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub category: String,
    pub density_kg_m3: f64,
    #[serde(default)]
    pub notes: String,
}

/// Türkçe veya eski malzeme adlarını sabit id değerine eşler (geriye dönük uyumluluk).
pub fn resolve_material_id(name_or_id: &str) -> &'static str {
    match name_or_id {
        "lw_pla_foamed" | "LW-PLA (köpük, akış %50-60)" => "lw_pla_foamed",
        "lw_pla_unfoamed" | "LW-PLA (köpürtülmemiş)" => "lw_pla_unfoamed",
        "pla" | "PLA" => "pla",
        "petg" | "PETG" => "petg",
        "cf_petg" | "CF-PETG" => "cf_petg",
        "asa" | "ASA" => "asa",
        "tpu_95a" | "TPU 95A" => "tpu_95a",
        "carbon_tube_pultruded" | "Karbon boru (pultruded)" => "carbon_tube_pultruded",
        "carbon_tube_roll_wrapped" | "Karbon boru (sarım/prepreg)" => "carbon_tube_roll_wrapped",
        "aluminum_6061_tube" | "Alüminyum 6061 boru" => "aluminum_6061_tube",
        "epp_foam_30" | "EPP köpük 30 g/L" => "epp_foam_30",
        "epo_foam" | "EPO köpük" => "epo_foam",
        "depron_xps" | "Depron XPS" => "depron_xps",
        "balsa_medium" | "Balsa (orta)" => "balsa_medium",
        "plywood_poplar_3mm" | "Kontrplak kavak 3 mm" => "plywood_poplar_3mm",
        "covering_film" | "Kaplama filmi (PET/EVA)" => "covering_film",
        _ => "unknown",
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Cell {
    #[serde(default)]
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

impl From<crate::db::DatabaseCell> for Cell {
    fn from(c: crate::db::DatabaseCell) -> Self {
        Self {
            id: c.id,
            name: c.name,
            chemistry: c.chemistry,
            nominal_v: c.nominal_v,
            capacity_ah: c.capacity_ah,
            mass_g: c.mass_g,
            max_cont_a: c.max_cont_a,
            source: c.source,
            source_url: c.source_url,
            source_date: c.source_date,
            verified_by: c.verified_by,
        }
    }
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: "Default Cell".into(),
            chemistry: "Li-ion".into(),
            nominal_v: 3.6,
            capacity_ah: 4.2,
            mass_g: 70.0,
            max_cont_a: 45.0,
            source: String::new(),
            source_url: String::new(),
            source_date: String::new(),
            verified_by: String::new(),
        }
    }
}

impl Cell {
    pub fn wh(&self) -> f64 {
        self.nominal_v * self.capacity_ah
    }
    /// Hücre seviyesinde özgül enerji [Wh/kg] (her zaman yeniden hesaplanır).
    pub fn wh_per_kg(&self) -> f64 {
        self.wh() / (self.mass_g / 1000.0)
    }
}

pub fn load_materials() -> Vec<Material> {
    let mut list: Vec<Material> =
        serde_json::from_str(include_str!("../../assets/materials.json")).unwrap_or_default();
    for mat in &mut list {
        if mat.id.is_empty() {
            mat.id = resolve_material_id(&mat.name).to_string();
        }
    }
    list
}
pub fn load_cells() -> Vec<Cell> {
    let (db_cells, _, _) = crate::db::load_active_database();
    db_cells.into_iter().map(Cell::from).collect()
}

#[derive(Clone, Debug)]
pub struct WeightIn {
    pub m_payload: f64,
    pub m_avionics: f64,
    pub m_propulsion: f64,
    pub m_fuse_tail: f64,
    pub cell: Cell,
    pub n_series: u32,
    pub n_parallel: u32,
    /// Paket ek yük çarpanı (kablo, BMS, ambalaj): paket kütlesi = hücre kütlesi × bu.
    pub pack_overhead: f64,
    pub s: f64,
    pub tc_w: f64,
    pub t_wall: f64,
    pub rho_mat: f64,
    pub k_infill: f64,
    pub rho_cf: f64,
    pub d_o: f64,
    pub d_i: f64,
    pub l_spar: f64,
}

#[derive(Clone, Debug, Default)]
pub struct WeightOut {
    pub a_wet_w: f64,
    pub m_shell: f64,
    pub a_spar: f64,
    pub m_spar: f64,
    pub m_wing: f64,
    pub m_struct: f64,
    pub n_cells: u32,
    pub pack_voltage: f64,
    pub pack_capacity_ah: f64,
    pub pack_wh: f64,
    pub m_battery: f64,
    pub e_pack_wh_kg: f64,
    pub m_mtow: f64,
    pub w: f64,
    pub f_batt: f64,
    pub f_struct: f64,
    pub f_payload: f64,
    pub f_avionics: f64,
    pub f_propulsion: f64,
}

pub fn wing_wetted_area(s: f64, tc_w: f64) -> f64 {
    2.0 * s * (1.0 + 0.2 * tc_w)
}

pub fn spar_area(d_o: f64, d_i: f64) -> f64 {
    std::f64::consts::FRAC_PI_4 * (d_o * d_o - d_i * d_i)
}

pub fn weight(i: &WeightIn) -> WeightOut {
    let a_wet_w = wing_wetted_area(i.s, i.tc_w);
    let m_shell = a_wet_w * i.t_wall * i.rho_mat * i.k_infill;
    let a_spar = spar_area(i.d_o, i.d_i);
    let m_spar = a_spar * i.l_spar * i.rho_cf;
    let m_wing = m_shell + m_spar;
    let m_struct = m_wing + i.m_fuse_tail;
    let n_cells = i.n_series * i.n_parallel;
    let m_battery = n_cells as f64 * i.cell.mass_g / 1000.0 * i.pack_overhead;
    let pack_wh = n_cells as f64 * i.cell.wh();
    let m_mtow = i.m_payload + i.m_avionics + i.m_propulsion + m_battery + m_struct;
    WeightOut {
        a_wet_w,
        m_shell,
        a_spar,
        m_spar,
        m_wing,
        m_struct,
        n_cells,
        pack_voltage: i.n_series as f64 * i.cell.nominal_v,
        pack_capacity_ah: i.n_parallel as f64 * i.cell.capacity_ah,
        pack_wh,
        m_battery,
        e_pack_wh_kg: pack_wh / m_battery,
        m_mtow,
        w: m_mtow * G,
        f_batt: m_battery / m_mtow,
        f_struct: m_struct / m_mtow,
        f_payload: i.m_payload / m_mtow,
        f_avionics: i.m_avionics / m_mtow,
        f_propulsion: i.m_propulsion / m_mtow,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn near(a: f64, b: f64, tol: f64) {
        assert!(((a - b) / b).abs() < tol, "{a} vs {b}");
    }
    pub fn default_in() -> WeightIn {
        let cells = load_cells();
        WeightIn {
            m_payload: 0.35,
            m_avionics: 0.15,
            m_propulsion: 0.20,
            m_fuse_tail: 0.3385,
            cell: cells.into_iter().find(|c| c.name.contains("P42A")).unwrap(),
            n_series: 4,
            n_parallel: 2,
            pack_overhead: 1.16,
            s: 0.32,
            tc_w: 0.10,
            t_wall: 0.0005,
            rho_mat: 600.0,
            k_infill: 1.30,
            rho_cf: 1550.0,
            d_o: 0.010,
            d_i: 0.008,
            l_spar: 1.3,
        }
    }
    #[test]
    fn spec_example() {
        let o = weight(&default_in());
        near(o.a_wet_w, 0.653, 0.005);
        near(o.m_shell, 0.255, 0.01);
        near(o.a_spar, 2.83e-5, 0.01);
        near(o.m_spar, 0.0570, 0.01);
        near(o.m_wing, 0.312, 0.01);
        near(o.m_struct, 0.650, 0.01);
        near(o.m_mtow, 2.00, 0.01);
        near(o.w, 19.6, 0.01);
        near(o.f_batt, 0.325, 0.01);
        near(o.pack_wh, 121.0, 0.01);
        near(o.e_pack_wh_kg, 186.0, 0.01);
    }
    #[test]
    fn cell_data_sane() {
        let cells = load_cells();
        let vtc6 = cells.iter().find(|c| c.name.contains("VTC6")).unwrap();
        near(vtc6.mass_g, 46.6, 0.001);
        near(vtc6.wh_per_kg(), 3.6 * 3.0 / 0.0466, 0.001);
        let m = load_materials();
        assert!(
            m.iter()
                .any(|x| x.name == "PETG" && x.density_kg_m3 == 1270.0)
        );
    }
    #[test]
    fn cell_discharge_ratings_match_datasheets() {
        let cells = load_cells();
        let get = |key: &str| cells.iter().find(|c| c.name.contains(key)).unwrap();
        // Molicel P50B: 60 A sürekli (sıcaklık kesmeli); 12 A yanlıştı.
        near(get("P50B").max_cont_a, 60.0, 1e-9);
        near(get("P42A").max_cont_a, 45.0, 1e-9);
        near(get("P45B").max_cont_a, 45.0, 1e-9);
        near(get("50S").max_cont_a, 25.0, 1e-9);
        near(get("40T").max_cont_a, 35.0, 1e-9);
        near(get("M50LT").max_cont_a, 7.3, 1e-9);
        near(get("VTC6").max_cont_a, 15.0, 1e-9);
        near(get("35E").max_cont_a, 8.0, 1e-9);
        // Genel LiPo: 100 g / 18.5 Wh (185 Wh/kg) fazla iyimserdi; yüksek akımlı paket ~130 g.
        let lipo = get("LiPo");
        assert!(lipo.wh_per_kg() <= 150.0, "{}", lipo.wh_per_kg());
    }
    #[test]
    fn poplar_plywood_is_wood_not_film() {
        let m = load_materials();
        let ply = m.iter().find(|x| x.name.contains("Kontrplak")).unwrap();
        assert_eq!(ply.category, "wood");
    }

    #[test]
    fn test_battery_sources_and_templates() {
        use crate::calc::custom_battery::{
            BatterySourceKind, CustomPackData, GenericBatteryTemplate,
        };

        // 1. Şablon testleri
        let t21700 = GenericBatteryTemplate::LiIon21700.default_cell();
        assert_eq!(t21700.nominal_v, 3.6);
        assert_eq!(t21700.capacity_ah, 4.5);
        assert_eq!(t21700.mass_g, 70.0);
        assert_eq!(t21700.max_cont_a, 35.0);

        let t18650 = GenericBatteryTemplate::LiIon18650.default_cell();
        assert_eq!(t18650.nominal_v, 3.6);
        assert_eq!(t18650.capacity_ah, 3.0);
        assert_eq!(t18650.mass_g, 47.0);

        let tlipo = GenericBatteryTemplate::LiPoGeneric.default_cell();
        assert_eq!(tlipo.nominal_v, 3.7);
        assert!((tlipo.wh_per_kg() - 150.0).abs() < 5.0);

        let tsolid = GenericBatteryTemplate::LiIonSemiSolid.default_cell();
        assert_eq!(tsolid.nominal_v, 3.6);
        assert!((tsolid.wh_per_kg() - 300.0).abs() < 5.0);

        // 2. Özel paket testi (4S 5000 mAh 480 g)
        let pack = CustomPackData {
            name: "Test LiPo 4S".into(),
            chemistry: "LiPo".into(),
            pack_voltage: 14.8,
            s_count: 4,
            capacity_mah: 5000.0,
            pack_mass_g: 480.0,
            max_cont_a: 150.0,
            c_rating: 30.0,
        };
        near(pack.wh(), 74.0, 0.001);
        near(pack.wh_per_kg(), 74.0 / 0.480, 0.001);

        // 3. Proje geriye dönük uyumluluk testi (Eski JSON)
        let old_json = r#"{
            "name": "Eski Proje",
            "b": 1.6,
            "s": 0.32
        }"#;
        let proj = crate::project::Project::from_json(old_json).expect("Eski JSON açılmalı");
        assert_eq!(proj.battery_source, BatterySourceKind::Library);
        assert_eq!(
            proj.mr_arm_section,
            crate::calc::mr_structure::MrArmSectionType::Round
        );
    }
}
