// SPDX-License-Identifier: AGPL-3.0-or-later
//! Strength/stiffness data for the structural analysis.
//!
//! Values are typical, conservative hobby-grade figures (allowable bending stress, Young's
//! modulus, shear modulus, density). Densities match `assets/materials.json` where the project
//! already lists the material; strengths/moduli of the tube and plastic grades match the arm
//! presets in `calc::mr_structure`. Plastics are listed for strength only.

use crate::calc::mr_structure::MrArmMaterialPreset;

/// Material properties used by the beam model (SI units).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MatProps {
    pub sigma_allow: f64,
    pub e: f64,
    pub g: f64,
    pub rho: f64,
}

/// Selectable materials.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructMaterial {
    CfTubePultruded,
    CfTubeRollWrapped,
    CfPlate,
    Al6061,
    Al7075,
    PlywoodPoplar,
    BalsaMedium,
    Pla,
    Petg,
    Abs,
    CfPetg,
    CfNylon,
}

impl StructMaterial {
    pub const ALL: [StructMaterial; 12] = [
        Self::CfTubePultruded,
        Self::CfTubeRollWrapped,
        Self::CfPlate,
        Self::Al6061,
        Self::Al7075,
        Self::PlywoodPoplar,
        Self::BalsaMedium,
        Self::Pla,
        Self::Petg,
        Self::Abs,
        Self::CfPetg,
        Self::CfNylon,
    ];

    pub fn props(self) -> MatProps {
        let (sigma, e, g, rho) = match self {
            Self::CfTubePultruded => (600.0e6, 120.0e9, 4.5e9, 1550.0),
            Self::CfTubeRollWrapped => (500.0e6, 70.0e9, 12.0e9, 1500.0),
            Self::CfPlate => (400.0e6, 50.0e9, 5.0e9, 1550.0),
            Self::Al6061 => (240.0e6, 69.0e9, 26.0e9, 2700.0),
            Self::Al7075 => (430.0e6, 71.7e9, 26.9e9, 2810.0),
            Self::PlywoodPoplar => (30.0e6, 6.0e9, 0.5e9, 450.0),
            Self::BalsaMedium => (12.0e6, 3.5e9, 0.15e9, 150.0),
            Self::Pla => (45.0e6, 3.5e9, 1.3e9, 1240.0),
            Self::Petg => (40.0e6, 2.1e9, 0.78e9, 1270.0),
            Self::Abs => (35.0e6, 2.2e9, 0.8e9, 1050.0),
            Self::CfPetg => (50.0e6, 3.5e9, 1.3e9, 1300.0),
            Self::CfNylon => (65.0e6, 6.0e9, 2.2e9, 1170.0),
        };
        MatProps {
            sigma_allow: sigma,
            e,
            g,
            rho,
        }
    }

    /// i18n key of the display name.
    pub fn key(self) -> &'static str {
        match self {
            Self::CfTubePultruded => "structfea.mat.cf_tube_pultruded",
            Self::CfTubeRollWrapped => "structfea.mat.cf_tube_roll_wrapped",
            Self::CfPlate => "structfea.mat.cf_plate",
            Self::Al6061 => "structfea.mat.al6061",
            Self::Al7075 => "structfea.mat.al7075",
            Self::PlywoodPoplar => "structfea.mat.plywood",
            Self::BalsaMedium => "structfea.mat.balsa",
            Self::Pla => "structfea.mat.pla",
            Self::Petg => "structfea.mat.petg",
            Self::Abs => "structfea.mat.abs",
            Self::CfPetg => "structfea.mat.cf_petg",
            Self::CfNylon => "structfea.mat.cf_nylon",
        }
    }
}

/// Shear modulus for the existing arm material presets (the presets only carry sigma, E, rho).
/// `Custom` falls back to an isotropic estimate `G = E / 2.6` (Poisson ratio 0.3).
pub fn arm_preset_shear_modulus(preset: MrArmMaterialPreset, e: f64) -> f64 {
    match preset {
        MrArmMaterialPreset::PultrudedCf => StructMaterial::CfTubePultruded.props().g,
        MrArmMaterialPreset::RollWrappedCf => StructMaterial::CfTubeRollWrapped.props().g,
        MrArmMaterialPreset::Al6061 => StructMaterial::Al6061.props().g,
        MrArmMaterialPreset::Petg => StructMaterial::Petg.props().g,
        MrArmMaterialPreset::CfPetg => StructMaterial::CfPetg.props().g,
        MrArmMaterialPreset::Custom => e.max(0.0) / 2.6,
    }
}

/// Young's modulus assumed for the wing spar when the project values are used: the wing
/// structure step stores only the allowable stress, so a pultruded carbon tube is assumed.
pub const WING_SPAR_DEFAULT_E: f64 = 120.0e9;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_match_existing_arm_table() {
        let pairs = [
            (
                MrArmMaterialPreset::PultrudedCf,
                StructMaterial::CfTubePultruded,
            ),
            (
                MrArmMaterialPreset::RollWrappedCf,
                StructMaterial::CfTubeRollWrapped,
            ),
            (MrArmMaterialPreset::Al6061, StructMaterial::Al6061),
            (MrArmMaterialPreset::Petg, StructMaterial::Petg),
            (MrArmMaterialPreset::CfPetg, StructMaterial::CfPetg),
        ];
        for (old, new) in pairs {
            let (s, e, r) = old.properties();
            let p = new.props();
            assert_eq!((s, e, r), (p.sigma_allow, p.e, p.rho), "{old:?}");
        }
    }

    #[test]
    fn densities_match_material_library() {
        let lib = crate::calc::weight::load_materials();
        let find = |id: &str| lib.iter().find(|m| m.id == id).unwrap().density_kg_m3;
        assert_eq!(
            StructMaterial::BalsaMedium.props().rho,
            find("balsa_medium")
        );
        assert_eq!(
            StructMaterial::PlywoodPoplar.props().rho,
            find("plywood_poplar_3mm")
        );
        assert_eq!(StructMaterial::Pla.props().rho, find("pla"));
        assert_eq!(StructMaterial::Petg.props().rho, find("petg"));
        assert_eq!(StructMaterial::CfPetg.props().rho, find("cf_petg"));
        assert_eq!(
            StructMaterial::Al6061.props().rho,
            find("aluminum_6061_tube")
        );
    }

    #[test]
    fn all_props_positive() {
        for m in StructMaterial::ALL {
            let p = m.props();
            assert!(p.sigma_allow > 0.0 && p.e > 0.0 && p.g > 0.0 && p.rho > 0.0);
        }
    }
}
