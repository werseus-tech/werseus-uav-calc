// SPDX-License-Identifier: AGPL-3.0-or-later
//! Tüm kullanıcı girdilerini tutan, JSON'a kaydedilebilen proje yapısı (SI birimleri).
//! Varsayılanlar: spec Bölüm 10 "2 kg, 1.6 m kanat açıklığı" referans tasarımı.
use crate::calc::constraint::RecCriterion;
use crate::calc::geometry::TailKind;
use crate::calc::multirotor::MultirotorLayout;
use serde::{Deserialize, Serialize};

/// Sihirbaz adımlarının tamamlanma ve güncellenme durumları.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum WizardStepState {
    #[default]
    NotVisited,
    NeedsUpdate,
    Completed,
}

/// Desteklenen İHA / Drone platform türleri.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DroneType {
    #[default]
    FixedWingConventional,
    FixedWingVTail,
    FlyingWing,
    Multirotor,
    VtolQuadplane,
}

#[allow(dead_code)]
impl DroneType {
    pub const ALL: [DroneType; 5] = [
        DroneType::FixedWingConventional,
        DroneType::FixedWingVTail,
        DroneType::FlyingWing,
        DroneType::Multirotor,
        DroneType::VtolQuadplane,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            DroneType::FixedWingConventional => crate::i18n::t("drone_type.fixed_wing_conv.label"),
            DroneType::FixedWingVTail => crate::i18n::t("drone_type.fixed_wing_vtail.label"),
            DroneType::FlyingWing => crate::i18n::t("drone_type.flying_wing.label"),
            DroneType::Multirotor => crate::i18n::t("drone_type.multirotor.label"),
            DroneType::VtolQuadplane => crate::i18n::t("drone_type.vtol_quadplane.label"),
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            DroneType::FixedWingConventional => crate::i18n::t("drone_type.fixed_wing_conv.desc"),
            DroneType::FixedWingVTail => crate::i18n::t("drone_type.fixed_wing_vtail.desc"),
            DroneType::FlyingWing => crate::i18n::t("drone_type.flying_wing.desc"),
            DroneType::Multirotor => crate::i18n::t("drone_type.multirotor.desc"),
            DroneType::VtolQuadplane => crate::i18n::t("drone_type.vtol_quadplane.desc"),
        }
    }

    pub fn pros(&self) -> &'static str {
        match self {
            DroneType::FixedWingConventional => crate::i18n::t("drone_type.fixed_wing_conv.pros"),
            DroneType::FixedWingVTail => crate::i18n::t("drone_type.fixed_wing_vtail.pros"),
            DroneType::FlyingWing => crate::i18n::t("drone_type.flying_wing.pros"),
            DroneType::Multirotor => crate::i18n::t("drone_type.multirotor.pros"),
            DroneType::VtolQuadplane => crate::i18n::t("drone_type.vtol_quadplane.pros"),
        }
    }

    pub fn cons(&self) -> &'static str {
        match self {
            DroneType::FixedWingConventional => crate::i18n::t("drone_type.fixed_wing_conv.cons"),
            DroneType::FixedWingVTail => crate::i18n::t("drone_type.fixed_wing_vtail.cons"),
            DroneType::FlyingWing => crate::i18n::t("drone_type.flying_wing.cons"),
            DroneType::Multirotor => crate::i18n::t("drone_type.multirotor.cons"),
            DroneType::VtolQuadplane => crate::i18n::t("drone_type.vtol_quadplane.cons"),
        }
    }

    pub fn is_fixed_wing(&self) -> bool {
        matches!(
            self,
            DroneType::FixedWingConventional
                | DroneType::FixedWingVTail
                | DroneType::FlyingWing
                | DroneType::VtolQuadplane
        )
    }

    pub fn has_wing(&self) -> bool {
        self.is_fixed_wing()
    }

    pub fn is_multirotor(&self) -> bool {
        matches!(self, DroneType::Multirotor | DroneType::VtolQuadplane)
    }
}

/// Bileşen kütlesinin Ağırlık Dökümü'ne bağlanması.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Link {
    None,
    Payload,
    Avionics,
    Propulsion,
    Battery,
    Wing,
    FuseTail,
}

impl Link {
    pub const ALL: [Link; 7] = [
        Link::None,
        Link::Payload,
        Link::Avionics,
        Link::Propulsion,
        Link::Battery,
        Link::Wing,
        Link::FuseTail,
    ];
    pub fn label(&self) -> &'static str {
        match self {
            Link::None => crate::i18n::t("link.none"),
            Link::Payload => crate::i18n::t("link.payload"),
            Link::Avionics => crate::i18n::t("link.avionics"),
            Link::Propulsion => crate::i18n::t("link.propulsion"),
            Link::Battery => crate::i18n::t("link.battery"),
            Link::Wing => crate::i18n::t("link.wing"),
            Link::FuseTail => crate::i18n::t("link.fuse_tail"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Component {
    pub name: String,
    pub link: Link,
    /// Elle girilen kütle [kg] (link = None ise kullanılır).
    pub mass: f64,
    /// Burundan geriye doğru [m]
    pub x: f64,
    /// Sağa doğru [m] (simetri ekseni = 0)
    pub y: f64,
    /// Yukarı doğru [m] (gövde ekseni = 0)
    pub z: f64,
}

fn comp(name: &str, link: Link, x: f64) -> Component {
    Component {
        name: name.into(),
        link,
        mass: 0.0,
        x,
        y: 0.0,
        z: 0.0,
    }
}

/// Multirotor için gövde merkezine göre tanımlanan bileşenler.
/// Datum: Çerçeve merkezi (0, 0), x: ileri (+), y: sağa (+) [m]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MrComponent {
    pub name: String,
    /// Kütle [kg]
    pub mass: f64,
    /// Çerçeve merkezinden ileri doğru [m] (x ileri +)
    pub x: f64,
    /// Çerçeve merkezinden sağa doğru [m] (y sağ +)
    pub y: f64,
}

impl MrComponent {
    pub fn new(name: impl Into<String>, mass: f64, x: f64, y: f64) -> Self {
        Self {
            name: name.into(),
            mass,
            x,
            y,
        }
    }
}

pub fn default_mr_components() -> Vec<MrComponent> {
    vec![
        MrComponent::new(crate::i18n::t("comp.default.battery"), 0.320, -0.035, 0.0),
        MrComponent::new(crate::i18n::t("comp.default.fc_gps"), 0.040, 0.0, 0.0),
        MrComponent::new(
            crate::i18n::t("comp.default.camera_payload"),
            0.300,
            0.040,
            0.0,
        ),
        MrComponent::new(crate::i18n::t("comp.default.esc_group"), 0.030, 0.0, 0.0),
        MrComponent::new(crate::i18n::t("comp.default.rx_vtx"), 0.020, -0.030, 0.0),
    ]
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Project {
    pub name: String,
    pub drone_type: DroneType,

    // --- Görev & atmosfer
    pub h: f64,
    pub v_cr: f64,

    // --- Kütleler (kg)
    pub m_payload: f64,
    pub m_avionics: f64,
    pub m_propulsion: f64,
    pub m_fuse_tail: f64,
    pub cell_name: String,
    pub n_series: u32,
    pub n_parallel: u32,
    pub pack_overhead: f64,

    // --- Yapı
    pub material_id: String,
    pub material_name: String,
    pub rho_mat: f64,
    pub t_wall: f64,
    pub k_infill: f64,
    pub rho_cf: f64,
    pub d_spar_o: f64,
    pub d_spar_i: f64,
    pub l_spar: f64,

    // --- Kanat
    pub b: f64,
    pub s: f64,
    pub taper: f64,
    pub sweep_le_deg: f64,
    pub tc_w: f64,
    pub airfoil: crate::cad::AirfoilProfile,

    // --- Kuyruk
    pub tail_kind: TailKind,
    pub s_h: f64,
    pub s_v: f64,
    pub s_vtail: f64,
    pub gamma_v_deg: f64,
    pub l_h: f64,
    pub l_v: f64,
    pub vh_target: f64,
    pub vv_target: f64,
    pub cla_h: f64,
    pub eta_h: f64,
    pub c_tail: f64,
    pub tc_t: f64,
    #[serde(default = "crate::cad::AirfoilProfile::default_tail_symmetric")]
    pub tail_airfoil_h: crate::cad::AirfoilProfile,
    #[serde(default = "crate::cad::AirfoilProfile::default_tail_symmetric")]
    pub tail_airfoil_v: crate::cad::AirfoilProfile,

    // --- Aerodinamik
    pub cl_alpha_2d: f64,
    pub cl_max_2d: f64,
    pub l_fuse: f64,
    pub d_fuse: f64,
    pub s_wet_fuse: f64,
    pub cd_misc: f64,
    pub q_tail: f64,
    /// Basit/Gelişmiş mod ve basit mod cevapları (yeni başlayanlar için türetme katmanı).
    pub aero_simple: crate::calc::aero_presets::AeroSimple,
    /// Diğer adımların basit mod cevapları ve özelleştirme durumu.
    pub simple: crate::calc::presets::SimpleState,

    // --- Kısıtlar
    pub v_stall_req: f64,
    pub rc: f64,
    pub v_y: f64,
    pub phi_turn_deg: f64,
    pub v_turn: f64,
    pub v_launch: f64,
    pub gamma_launch_deg: f64,
    pub a_launch: f64,
    pub eta_p_cr: f64,
    pub eta_p_climb: f64,
    pub eta_mot: f64,
    pub eta_esc: f64,
    pub pw_margin: f64,
    pub ws_margin: f64,
    /// Önerilen tasarım noktası seçim kriteri.
    pub rec_criterion: RecCriterion,

    // --- Havada kalma
    pub dod: f64,
    pub p_hotel: f64,

    // --- İtki
    pub tw_static: f64,
    pub d_prop: f64,
    pub m_tip_max: f64,
    pub fm: f64,
    pub p_max_elec: f64,
    pub motor_margin: f64,
    pub prop_pitch: f64,
    pub prop_slip: f64,

    // --- Stabilite
    pub components: Vec<Component>,
    pub x_root_le: f64,
    pub x_ac_frac: f64,
    pub sm_target: f64,

    // --- Yapı
    pub n_limit: f64,
    pub fs: f64,
    pub sigma_allow: f64,

    // --- Multirotor & VTOL kaldırma girdileri
    pub mr_frame_size: f64,
    pub mr_arm_count: u32,
    pub mr_layout: MultirotorLayout,
    pub mr_coaxial: bool,
    pub mr_prop_clearance: f64,
    pub mr_d_prop: f64,
    pub mr_r_plate: f64,
    pub mr_m_frame: f64,
    pub mr_m_motor: f64,
    pub mr_m_esc_tot: f64,
    pub mr_m_fc: f64,
    pub mr_m_misc: f64,
    pub mr_t_max_motor: f64,
    pub mr_v_cruise: f64,
    pub mr_components: Vec<MrComponent>,
    pub mr_h_lx: f64,
    pub mr_h_wy: f64,
    pub mr_coax_kappa: f64,
    pub mr_flat_plate_area: f64,
    pub mr_profile_power_ratio: f64,

    // --- Multirotor & VTOL Yapı (Kol) girdileri
    pub mr_arm_section: crate::calc::mr_structure::MrArmSectionType,
    pub mr_arm_material: crate::calc::mr_structure::MrArmMaterialPreset,
    pub mr_arm_d_o: f64,
    pub mr_arm_d_i: f64,
    pub mr_arm_l_override: f64,
    pub mr_arm_sigma_allow: f64,
    pub mr_arm_e_modulus: f64,
    pub mr_arm_rho: f64,
    pub mr_arm_n_limit: f64,
    pub mr_arm_fs: f64,
    pub mr_hover_rpm_override: f64,
    pub mr_prop_blades: u32,

    // --- Batarya Kaynağı ve Özel Batarya girdileri
    pub battery_source: crate::calc::custom_battery::BatterySourceKind,
    pub generic_battery_template: crate::calc::custom_battery::GenericBatteryTemplate,
    pub generic_cell: crate::calc::custom_battery::CustomCellData,
    pub custom_cell: crate::calc::custom_battery::CustomCellData,
    pub custom_pack: crate::calc::custom_battery::CustomPackData,

    // --- Sihirbaz ve arayüz modu ayarları
    pub expert_mode: bool,
    pub wizard_step: usize,
    pub step_states: Vec<WizardStepState>,
}

impl Default for Project {
    fn default() -> Self {
        Project {
            name: crate::i18n::t("project.default_name").into(),
            drone_type: DroneType::FixedWingConventional,
            h: 500.0,
            v_cr: 18.0,
            m_payload: 0.350,
            m_avionics: 0.150,
            m_propulsion: 0.200,
            m_fuse_tail: 0.3385,
            cell_name: "Molicel P42A 21700".into(),
            n_series: 4,
            n_parallel: 2,
            pack_overhead: 1.16,
            material_id: "lw_pla_foamed".into(),
            material_name: "LW-PLA (köpük, akış %50-60)".into(),
            rho_mat: 600.0,
            t_wall: 0.0005,
            k_infill: 1.30,
            rho_cf: 1550.0,
            d_spar_o: 0.010,
            d_spar_i: 0.008,
            l_spar: 1.300,
            b: 1.6,
            s: 0.32,
            taper: 0.6,
            sweep_le_deg: 5.0,
            tc_w: 0.10,
            airfoil: crate::cad::AirfoilProfile::default(),
            tail_kind: TailKind::Conventional,
            s_h: 0.055,
            s_v: 0.028,
            s_vtail: 0.083,
            gamma_v_deg: 35.0,
            l_h: 0.65,
            l_v: 0.65,
            vh_target: 0.55,
            vv_target: 0.035,
            cla_h: 4.0,
            eta_h: 0.90,
            c_tail: 0.10,
            tc_t: 0.09,
            tail_airfoil_h: crate::cad::AirfoilProfile::default_tail_symmetric(),
            tail_airfoil_v: crate::cad::AirfoilProfile::default_tail_symmetric(),
            cl_alpha_2d: 5.95,
            cl_max_2d: 1.25,
            l_fuse: 0.9,
            d_fuse: 0.11,
            s_wet_fuse: 0.235,
            cd_misc: 0.004,
            q_tail: 1.05,
            aero_simple: Default::default(),
            simple: Default::default(),
            v_stall_req: 10.0,
            rc: 3.5,
            v_y: 12.5,
            phi_turn_deg: 30.0,
            v_turn: 16.0,
            v_launch: 11.0,
            gamma_launch_deg: 10.0,
            a_launch: 1.0,
            eta_p_cr: 0.74,
            eta_p_climb: 0.58,
            eta_mot: 0.85,
            eta_esc: 0.95,
            pw_margin: 1.15,
            ws_margin: 1.0,
            rec_criterion: RecCriterion::MinPower,
            dod: 0.80,
            p_hotel: 14.0,
            tw_static: 0.60,
            d_prop: 0.254,
            m_tip_max: 0.45,
            fm: 0.65,
            p_max_elec: 240.0,
            motor_margin: 1.10,
            prop_pitch: 0.1524,
            prop_slip: 0.20,
            components: vec![
                comp(
                    crate::i18n::t("comp.default.payload_camera"),
                    Link::Payload,
                    0.090,
                ),
                comp(
                    crate::i18n::t("comp.default.avionics_fc"),
                    Link::Avionics,
                    0.200,
                ),
                comp(crate::i18n::t("comp.default.battery"), Link::Battery, 0.150),
                comp(crate::i18n::t("comp.default.wing_shell"), Link::Wing, 0.314),
                comp(
                    crate::i18n::t("comp.default.fuse_tail"),
                    Link::FuseTail,
                    0.600,
                ),
                comp(
                    crate::i18n::t("comp.default.pusher_prop"),
                    Link::Propulsion,
                    0.880,
                ),
            ],
            x_root_le: 0.200,
            x_ac_frac: 0.25,
            sm_target: 0.10,
            n_limit: 3.8,
            fs: 1.5,
            sigma_allow: 5.0e8,

            // Multirotor / VTOL varsayılanları (250 mm Quad X)
            mr_frame_size: 0.250,
            mr_arm_count: 4,
            mr_layout: MultirotorLayout::X,
            mr_coaxial: false,
            mr_prop_clearance: 0.010,
            mr_d_prop: 0.127, // 5 inç
            mr_r_plate: 0.035,
            mr_m_frame: 0.120,
            mr_m_motor: 0.030,
            mr_m_esc_tot: 0.030,
            mr_m_fc: 0.040,
            mr_m_misc: 0.050,
            mr_t_max_motor: 7.5,
            mr_v_cruise: 12.0,
            mr_components: default_mr_components(),
            mr_h_lx: 0.0,
            mr_h_wy: 0.0,
            mr_coax_kappa: 1.28,
            mr_flat_plate_area: 0.01,
            mr_profile_power_ratio: 0.15,

            // Multirotor Yapı varsayılanları (16mm OD, 14mm ID karbon boru)
            mr_arm_section: crate::calc::mr_structure::MrArmSectionType::Round,
            mr_arm_material: crate::calc::mr_structure::MrArmMaterialPreset::PultrudedCf,
            mr_arm_d_o: 0.016,
            mr_arm_d_i: 0.014,
            mr_arm_l_override: 0.0,
            mr_arm_sigma_allow: 600.0e6,
            mr_arm_e_modulus: 120.0e9,
            mr_arm_rho: 1550.0,
            mr_arm_n_limit: 2.0,
            mr_arm_fs: 1.5,
            mr_hover_rpm_override: 0.0,
            mr_prop_blades: 2,

            // Batarya Kaynağı varsayılanları
            battery_source: crate::calc::custom_battery::BatterySourceKind::Library,
            generic_battery_template:
                crate::calc::custom_battery::GenericBatteryTemplate::LiIon21700,
            generic_cell: crate::calc::custom_battery::GenericBatteryTemplate::LiIon21700
                .default_cell(),
            custom_cell: crate::calc::custom_battery::CustomCellData::default(),
            custom_pack: crate::calc::custom_battery::CustomPackData::default(),

            // Sihirbaz modu varsayılanları
            expert_mode: false,
            wizard_step: 0,
            step_states: Vec::new(),
        }
    }
}

pub static EX_FIXED_WING: &str = include_str!("../examples/fixed_wing_2kg_1600mm.json");
pub static EX_FLYING_WING: &str = include_str!("../examples/flying_wing_1200mm.json");
pub static EX_QUAD_250: &str = include_str!("../examples/quad_250_x.json");
pub static EX_QUAD_275: &str = include_str!("../examples/quad_275_x.json");
pub static EX_HEXA_550: &str = include_str!("../examples/hexa_550_x.json");
pub static EX_VTOL: &str = include_str!("../examples/vtol_quadplane.json");

/// Örnek projeyi yükler (disk üzerindeki examples/ öncelikli, yoksa gömülü kopya).
pub fn load_example(filename: &str) -> Result<Project, String> {
    let on_disk_paths = [
        std::path::Path::new("examples").join(filename),
        if let Ok(exe) = std::env::current_exe() {
            exe.parent()
                .map(|p| p.join("examples").join(filename))
                .unwrap_or_default()
        } else {
            std::path::PathBuf::new()
        },
    ];

    for path in &on_disk_paths {
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(path) {
                if let Ok(proj) = Project::from_json(&content) {
                    return Ok(proj);
                }
            }
        }
    }

    let embedded = match filename {
        "fixed_wing_2kg_1600mm.json" => EX_FIXED_WING,
        "flying_wing_1200mm.json" => EX_FLYING_WING,
        "quad_250_x.json" => EX_QUAD_250,
        "quad_275_x.json" => EX_QUAD_275,
        "hexa_550_x.json" => EX_HEXA_550,
        "vtol_quadplane.json" => EX_VTOL,
        _ => return Err(format!("Bilinmeyen örnek dosya: {}", filename)),
    };

    Project::from_json(embedded)
}

impl Project {
    /// Kuyruk sürüklemesinde kullanılan t/c'nin profillerden otomatik değeri. Klasik kuyrukta
    /// yatay ve dikey yüzeyin profil t/c'leri alanla ağırlıklandırılır (S_h·tc_h + S_v·tc_v)/(S_h+S_v);
    /// V kuyrukta V profili; uçan kanatta mevcut değer korunur.
    pub fn tail_tc_from_profiles(&self) -> f64 {
        match self.tail_kind {
            TailKind::Conventional => {
                let (th, tv) = (
                    self.tail_airfoil_h.max_thickness(),
                    self.tail_airfoil_v.max_thickness(),
                );
                let (sh, sv) = (self.s_h.max(0.0), self.s_v.max(0.0));
                if sh + sv > 0.0 {
                    (sh * th + sv * tv) / (sh + sv)
                } else {
                    0.5 * (th + tv)
                }
            }
            TailKind::VTail => self.tail_airfoil_v.max_thickness(),
            TailKind::FlyingWing => self.tc_t,
        }
    }

    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| e.to_string())
    }

    pub fn from_json(s: &str) -> Result<Project, String> {
        let mut p: Project = serde_json::from_str(s).map_err(|e| e.to_string())?;
        // Eski JSON dosyalarında drone_type yoksa ve tail_kind farklıysa uyum sağla:
        if !s.contains("drone_type") {
            match p.tail_kind {
                TailKind::VTail => p.drone_type = DroneType::FixedWingVTail,
                TailKind::FlyingWing => p.drone_type = DroneType::FlyingWing,
                _ => {}
            }
        }
        if p.mr_components.is_empty() {
            p.mr_components = default_mr_components();
        }
        if p.mr_coax_kappa <= 0.0 {
            p.mr_coax_kappa = 1.28;
        }
        if p.mr_flat_plate_area <= 0.0 {
            p.mr_flat_plate_area = 0.01;
        }
        if p.mr_profile_power_ratio <= 0.0 {
            p.mr_profile_power_ratio = 0.15;
        }
        if p.material_id.is_empty() {
            p.material_id = crate::calc::weight::resolve_material_id(&p.material_name).to_string();
        }
        p.mr_arm_count = match p.mr_arm_count {
            3 => 3,
            4 => 4,
            6 => 6,
            8 => 8,
            n if n < 4 => 3,
            n if n < 6 => 4,
            n if n < 8 => 6,
            _ => 8,
        };
        if p.mr_arm_count != 4 && p.mr_layout == MultirotorLayout::H {
            p.mr_layout = MultirotorLayout::X;
        }

        // Katı adım kilidi sıkıştırması: wizard_step = min(wizard_step, max_accessible_idx)
        let lib = crate::calc::Library::embedded();
        let model = crate::calc::evaluate(&p, &lib);
        crate::ui::wizard::clamp_wizard_step(&mut p, &model);

        Ok(p)
    }

    /// Drone türüne göre varsayılan konfigürasyon ayarlarını senkronize eder.
    pub fn set_drone_type(&mut self, dt: DroneType) {
        self.drone_type = dt;
        self.wizard_step = 0;
        self.step_states.clear();
        match dt {
            DroneType::FixedWingConventional => {
                self.tail_kind = TailKind::Conventional;
            }
            DroneType::FixedWingVTail => {
                self.tail_kind = TailKind::VTail;
            }
            DroneType::FlyingWing => {
                self.tail_kind = TailKind::FlyingWing;
            }
            DroneType::Multirotor => {
                if self.mr_components.is_empty() {
                    self.mr_components = default_mr_components();
                }
                // Multirotor bileşenleri tablosunu kontrol et
                if self.components.is_empty() {
                    self.components = vec![
                        comp(crate::i18n::t("comp.default.fc_rx"), Link::Avionics, 0.0),
                        comp(crate::i18n::t("comp.default.battery"), Link::Battery, 0.0),
                        comp(
                            crate::i18n::t("comp.default.fpv_camera_gimbal"),
                            Link::Payload,
                            0.04,
                        ),
                    ];
                }
            }
            DroneType::VtolQuadplane => {
                self.tail_kind = TailKind::Conventional;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_roundtrip() {
        let p = Project::default();
        let s = p.to_json().unwrap();
        let q = Project::from_json(&s).unwrap();
        assert_eq!(q.components.len(), p.components.len());
        assert!((q.b - 1.6).abs() < 1e-12);
        assert_eq!(q.drone_type, DroneType::FixedWingConventional);
        assert_eq!(q.mr_arm_count, 4);
        assert!(!q.expert_mode);
        assert_eq!(q.wizard_step, 0);
    }

    #[test]
    fn expert_mode_and_wizard_serde() {
        let mut p = Project::default();
        p.expert_mode = true;
        p.wizard_step = 4;
        p.step_states = vec![
            WizardStepState::Completed,
            WizardStepState::Completed,
            WizardStepState::NeedsUpdate,
        ];
        let s = p.to_json().unwrap();
        let q = Project::from_json(&s).unwrap();
        assert!(q.expert_mode);
        assert_eq!(q.wizard_step, 4);
        assert_eq!(q.step_states.len(), 3);
        assert_eq!(q.step_states[0], WizardStepState::Completed);
        assert_eq!(q.step_states[1], WizardStepState::Completed);
        assert_eq!(q.step_states[2], WizardStepState::NeedsUpdate);

        // Eski JSON dosyalarında expert_mode yoksa varsayılan false olmalı
        let old_json = r#"{"name": "Eski JSON"}"#;
        let old_p = Project::from_json(old_json).unwrap();
        assert!(
            !old_p.expert_mode,
            "Eski JSON için varsayılan uzman modu kapalı (sihirbaz) olmalı"
        );
        assert_eq!(old_p.wizard_step, 0);
        assert!(old_p.step_states.is_empty());
    }

    #[test]
    fn partial_json_uses_defaults() {
        let q = Project::from_json("{\"b\": 1.8}").unwrap();
        assert!((q.b - 1.8).abs() < 1e-12);
        assert!((q.s - 0.32).abs() < 1e-12);
        assert_eq!(q.drone_type, DroneType::FixedWingConventional);
        assert!((q.mr_frame_size - 0.250).abs() < 1e-12);
    }

    #[test]
    fn old_json_without_drone_type_compatibility() {
        let json = r#"{"name": "Eski Proje", "tail_kind": "VTail"}"#;
        let p = Project::from_json(json).unwrap();
        assert_eq!(p.tail_kind, TailKind::VTail);
        assert_eq!(p.drone_type, DroneType::FixedWingVTail);
    }

    #[test]
    fn drone_type_switching() {
        let mut p = Project::default();
        p.set_drone_type(DroneType::FlyingWing);
        assert_eq!(p.tail_kind, TailKind::FlyingWing);
        p.set_drone_type(DroneType::Multirotor);
        assert!(p.drone_type.is_multirotor());
        assert!(!p.drone_type.is_fixed_wing());
    }

    #[test]
    fn generate_and_verify_example_projects() {
        // Verify each distributed example file can be loaded and evaluated without calculation errors
        for filename in &[
            "examples/fixed_wing_2kg_1600mm.json",
            "examples/quad_250_x.json",
            "examples/quad_275_x.json",
            "examples/flying_wing_1200mm.json",
            "examples/vtol_quadplane.json",
        ] {
            let s = std::fs::read_to_string(filename).unwrap();
            let parsed = Project::from_json(&s).unwrap();
            let lib = crate::calc::Library::embedded();
            let model = crate::calc::evaluate(&parsed, &lib);
            for w in &model.warnings {
                if w.sev == crate::calc::Sev::Err {
                    eprintln!("{} ERROR [{}]: {}", filename, w.area_key, w.msg_key);
                }
            }
            assert!(
                model
                    .warnings
                    .iter()
                    .all(|w| w.sev != crate::calc::Sev::Err),
                "Example {} produced calculation errors!",
                filename
            );
        }
    }

    #[test]
    fn test_wizard_step_clamped_on_load_with_invalid_step() {
        // Proje JSON'unda adım 1 (Weight: m_avionics = 0) geçersiz ve wizard_step = 4 kaydedilmiş olsun.
        let mut p = Project::default();
        p.m_avionics = 0.0;
        p.wizard_step = 4;
        let json = p.to_json().unwrap();

        // Yüklendiğinde adım kilitli sınıra (1) düşmeli
        let loaded = Project::from_json(&json).unwrap();
        assert_eq!(
            loaded.wizard_step, 1,
            "Geçersiz adım 1 varken wizard_step 4'ten 1'e sıkıştırılmalıdır"
        );
    }

    #[test]
    fn test_arm_count_sanitized_and_h_layout_restricted() {
        // arm_count 0 olunca 3'e sıkıştırılmalı
        let mut p = Project::default();
        p.mr_arm_count = 0;
        let json = p.to_json().unwrap();
        let loaded = Project::from_json(&json).unwrap();
        assert_eq!(loaded.mr_arm_count, 3);

        // 6 kol ile H düzeni seçilirse X'e düşmeli
        let mut p_h = Project::default();
        p_h.mr_arm_count = 6;
        p_h.mr_layout = MultirotorLayout::H;
        let json_h = p_h.to_json().unwrap();
        let loaded_h = Project::from_json(&json_h).unwrap();
        assert_eq!(loaded_h.mr_arm_count, 6);
        assert_eq!(loaded_h.mr_layout, MultirotorLayout::X);
    }

    #[test]
    fn test_material_name_to_id_backwards_compatibility() {
        let json = r#"{"material_name": "LW-PLA (köpük, akış %50-60)"}"#;
        let p = Project::from_json(json).unwrap();
        assert_eq!(p.material_id, "lw_pla_foamed");
    }

    #[test]
    fn test_wing_airfoil_mutation_does_not_mutate_tail_and_vice_versa() {
        let mut p = Project::default();

        // Başlangıç kuyruk durumunu kaydet
        let orig_tail_kind = p.tail_kind;
        let orig_s_h = p.s_h;
        let orig_s_v = p.s_v;
        let orig_s_vtail = p.s_vtail;
        let orig_l_h = p.l_h;
        let orig_l_v = p.l_v;
        let orig_tc_t = p.tc_t;
        let orig_tail_h_code = p.tail_airfoil_h.naca_code.clone();
        let orig_tail_v_code = p.tail_airfoil_v.naca_code.clone();

        // Kanat profilini ve kalınlığını değiştir
        crate::ui::aero_simple::select_naca_for_wing(&mut p, "4412");
        assert_eq!(p.airfoil.naca_code, "4412");
        assert_eq!(p.tc_w, 0.12);

        // Kuyruk alanlarının ve profilinin ASLA değişmediğini doğrula
        assert_eq!(p.tail_kind, orig_tail_kind);
        assert_eq!(p.s_h, orig_s_h);
        assert_eq!(p.s_v, orig_s_v);
        assert_eq!(p.s_vtail, orig_s_vtail);
        assert_eq!(p.l_h, orig_l_h);
        assert_eq!(p.l_v, orig_l_v);
        assert_eq!(p.tc_t, orig_tc_t);
        assert_eq!(p.tail_airfoil_h.naca_code, orig_tail_h_code);
        assert_eq!(p.tail_airfoil_v.naca_code, orig_tail_v_code);

        // Başlangıç kanat durumunu kaydet
        let orig_b = p.b;
        let orig_s = p.s;
        let orig_taper = p.taper;
        let orig_tc_w = p.tc_w;
        let orig_wing_code = p.airfoil.naca_code.clone();

        // Kuyruk profilini ve kalınlığını değiştir
        crate::ui::aero_simple::select_naca_for_tail(&mut p, "0012");
        assert_eq!(p.tail_airfoil_h.naca_code, "0012");
        assert_eq!(p.tail_airfoil_v.naca_code, "0012");
        assert_eq!(p.tc_t, 0.12);

        // Kanat alanlarının ve profilinin ASLA değişmediğini doğrula
        assert_eq!(p.b, orig_b);
        assert_eq!(p.s, orig_s);
        assert_eq!(p.taper, orig_taper);
        assert_eq!(p.tc_w, orig_tc_w);
        assert_eq!(p.airfoil.naca_code, orig_wing_code);
    }

    #[test]
    fn test_old_project_json_without_tail_airfoils_backwards_compatible() {
        // Eski proje JSON formatı: tail_airfoil_h ve tail_airfoil_v alanları yok
        let old_json = r#"{
            "name": "Eski Tasarım",
            "drone_type": "FixedWingConventional",
            "b": 1.4,
            "s": 0.28,
            "tc_w": 0.12,
            "tc_t": 0.09,
            "airfoil": {
                "name": "NACA 2412",
                "is_naca": true,
                "naca_code": "2412",
                "closed_te": true,
                "num_points": 100,
                "points": []
            }
        }"#;

        let p = Project::from_json(old_json).expect("Eski JSON geriye uyumlu yüklenmeli");
        assert_eq!(p.name, "Eski Tasarım");
        assert_eq!(p.airfoil.naca_code, "2412");
        assert_eq!(p.tc_t, 0.09);
        assert_eq!(p.tail_airfoil_h.naca_code, "0009");
        assert_eq!(p.tail_airfoil_v.naca_code, "0009");
        assert!(!p.tail_airfoil_h.points.is_empty());
        assert!(!p.tail_airfoil_v.points.is_empty());
    }
}
