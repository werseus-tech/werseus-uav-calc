// SPDX-License-Identifier: AGPL-3.0-or-later
//! Multirotor yapı ve kol analizi: konsol kiriş momenti, gerilme, uç sehimi,
//! doğal frekans/rezonans kontrolü.

use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MrArmSectionType {
    #[default]
    Round,
    Square,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MrArmMaterialPreset {
    #[default]
    PultrudedCf,
    RollWrappedCf,
    Al6061,
    Petg,
    CfPetg,
    Custom,
}

impl MrArmMaterialPreset {
    pub fn properties(&self) -> (f64, f64, f64) {
        // (sigma_allow_pa, e_modulus_pa, rho_kg_m3)
        match self {
            Self::PultrudedCf => (600.0e6, 120.0e9, 1550.0),
            Self::RollWrappedCf => (500.0e6, 70.0e9, 1500.0),
            Self::Al6061 => (240.0e6, 69.0e9, 2700.0),
            Self::Petg => (40.0e6, 2.1e9, 1270.0),
            Self::CfPetg => (50.0e6, 3.5e9, 1300.0),
            Self::Custom => (600.0e6, 120.0e9, 1550.0),
        }
    }
}

#[derive(Clone, Debug)]
pub struct MrStructureIn {
    pub d_frame: f64,
    pub r_plate: f64,
    pub l_arm_override: f64,
    pub arm_count: u32,
    pub coaxial: bool,
    pub t_max_motor: f64,
    pub t_hover_motor: f64,
    pub m_motor: f64,
    pub n_limit: f64,
    pub fs: f64,
    pub section_type: MrArmSectionType,
    pub d_o: f64,
    pub d_i: f64,
    pub sigma_allow: f64,
    pub e_modulus: f64,
    pub rho: f64,
    pub rpm_hover: f64,
    pub prop_blades: u32,
}

#[derive(Clone, Debug, Default)]
pub struct MrStructureOut {
    pub l_arm: f64,
    pub n_coax: f64,
    pub f_thrust_max: f64,
    pub m_limit: f64,
    pub m_ult: f64,
    pub area: f64,
    pub inertia_i: f64,
    pub z_section: f64,
    pub sigma_max: f64,
    pub ms: f64,
    pub deflection_max: f64,
    pub deflection_limit: f64,
    pub deflection_ok: bool,
    pub m_arm_single: f64,
    pub m_arm_total: f64,
    pub f_natural: f64,
    pub f_rot_hover: f64,
    pub f_bpf_hover: f64,
    pub resonance_rot_warn: bool,
    pub resonance_bpf_warn: bool,
}

fn safe_div(n: f64, d: f64) -> f64 {
    if !n.is_finite() || !d.is_finite() || d.abs() < 1e-12 {
        0.0
    } else {
        n / d
    }
}

pub fn arm_section_properties(
    section_type: MrArmSectionType,
    d_o: f64,
    d_i: f64,
) -> (f64, f64, f64) {
    if !d_o.is_finite() || !d_i.is_finite() || d_o <= 0.0 || d_i < 0.0 || d_i >= d_o {
        return (0.0, 0.0, 0.0);
    }
    match section_type {
        MrArmSectionType::Round => {
            let area = PI * 0.25 * (d_o.powi(2) - d_i.powi(2));
            let inertia = PI / 64.0 * (d_o.powi(4) - d_i.powi(4));
            let c = d_o * 0.5;
            let z = safe_div(inertia, c);
            (area, inertia, z)
        }
        MrArmSectionType::Square => {
            let area = d_o.powi(2) - d_i.powi(2);
            let inertia = (d_o.powi(4) - d_i.powi(4)) / 12.0;
            let c = d_o * 0.5;
            let z = safe_div(inertia, c);
            (area, inertia, z)
        }
    }
}

pub fn mr_structure(i: &MrStructureIn) -> MrStructureOut {
    let l_arm = if i.l_arm_override.is_finite() && i.l_arm_override > 0.0 {
        i.l_arm_override
    } else {
        (i.d_frame * 0.5 - i.r_plate).max(0.01)
    };

    let n_coax = if i.coaxial { 2.0 } else { 1.0 };
    let f_thrust_max = i.t_max_motor.max(0.0) * n_coax;
    let n_limit = if i.n_limit.is_finite() && i.n_limit > 0.0 {
        i.n_limit
    } else {
        2.0
    };
    let fs = if i.fs.is_finite() && i.fs > 0.0 {
        i.fs
    } else {
        1.5
    };

    let m_limit = f_thrust_max * l_arm * n_limit;
    let m_ult = m_limit * fs;

    let (area, inertia_i, z_section) = arm_section_properties(i.section_type, i.d_o, i.d_i);

    let sigma_max = safe_div(m_ult, z_section);
    let ms = if sigma_max > 0.0 && i.sigma_allow.is_finite() && i.sigma_allow > 0.0 {
        i.sigma_allow / sigma_max - 1.0
    } else {
        f64::NAN
    };

    let ei = i.e_modulus.max(0.0) * inertia_i;
    let deflection_max = if ei > 0.0 && l_arm > 0.0 {
        f_thrust_max * l_arm.powi(3) / (3.0 * ei)
    } else {
        0.0
    };

    let deflection_limit = (0.001f64).max(l_arm / 200.0);
    let deflection_ok = deflection_max <= 0.001 || deflection_max <= (l_arm / 200.0);

    let rho = i.rho.max(0.0);
    let m_arm_single = area * l_arm * rho;
    let arm_cnt = i.arm_count as f64;
    let m_arm_total = arm_cnt * m_arm_single;

    let m_tip = i.m_motor.max(0.0) * n_coax;
    let m_eff = m_tip + 0.236 * m_arm_single;

    let f_natural = if ei > 0.0 && m_eff > 0.0 && l_arm > 0.0 {
        let omega = (3.0 * ei / (m_eff * l_arm.powi(3))).sqrt();
        omega / (2.0 * PI)
    } else {
        0.0
    };

    let f_rot_hover = if i.rpm_hover.is_finite() && i.rpm_hover > 0.0 {
        i.rpm_hover / 60.0
    } else {
        0.0
    };

    let blades = i.prop_blades.max(1) as f64;
    let f_bpf_hover = f_rot_hover * blades;

    let resonance_rot_warn = f_rot_hover > 0.0
        && f_natural > 0.0
        && ((f_natural - f_rot_hover).abs() / f_rot_hover) <= 0.20;

    let resonance_bpf_warn = f_bpf_hover > 0.0
        && f_natural > 0.0
        && ((f_natural - f_bpf_hover).abs() / f_bpf_hover) <= 0.20;

    MrStructureOut {
        l_arm,
        n_coax,
        f_thrust_max,
        m_limit,
        m_ult,
        area,
        inertia_i,
        z_section,
        sigma_max,
        ms,
        deflection_max,
        deflection_limit,
        deflection_ok,
        m_arm_single,
        m_arm_total,
        f_natural,
        f_rot_hover,
        f_bpf_hover,
        resonance_rot_warn,
        resonance_bpf_warn,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64, tol: f64) {
        assert!(
            ((a - b) / b).abs() < tol,
            "Beklenen: {b}, Gerçekleşen: {a}, Tol: {tol}"
        );
    }

    #[test]
    fn test_arm_round_section_mechanics() {
        // 16mm OD, 14mm ID pultruded CF tube
        let (area, inertia, z) = arm_section_properties(MrArmSectionType::Round, 0.016, 0.014);
        assert!(area > 0.0);
        assert!(inertia > 0.0);
        assert!(z > 0.0);

        let input = MrStructureIn {
            d_frame: 0.550,
            r_plate: 0.055,
            l_arm_override: 0.0,
            arm_count: 6,
            coaxial: false,
            t_max_motor: 15.0, // 15 N max thrust
            t_hover_motor: 5.0,
            m_motor: 0.060, // 60 g motor
            n_limit: 2.0,
            fs: 1.5,
            section_type: MrArmSectionType::Round,
            d_o: 0.016,
            d_i: 0.014,
            sigma_allow: 600.0e6,
            e_modulus: 120.0e9,
            rho: 1550.0,
            rpm_hover: 6000.0, // 100 Hz rot, 200 Hz BPF
            prop_blades: 2,
        };

        let out = mr_structure(&input);
        near(out.l_arm, 0.220, 0.001);
        near(out.m_limit, 15.0 * 0.220 * 2.0, 0.001); // 6.6 N*m
        near(out.m_ult, 6.6 * 1.5, 0.001); // 9.9 N*m
        assert!(out.sigma_max > 0.0 && out.sigma_max < 100.0e6);
        assert!(out.ms > 1.0, "Karbon borunun emniyet marjı pozitif olmalı");
        assert!(
            out.deflection_max < 0.001,
            "Sehim 1 mm'den küçük olmalı: {}",
            out.deflection_max
        );
        assert!(out.deflection_ok);
        assert!(
            out.f_natural > 50.0 && out.f_natural < 300.0,
            "Doğal frekans mantıklı aralıkta: {}",
            out.f_natural
        );
        assert_eq!(out.f_rot_hover, 100.0);
        assert_eq!(out.f_bpf_hover, 200.0);
    }

    #[test]
    fn test_arm_square_section() {
        let (area, inertia, z) = arm_section_properties(MrArmSectionType::Square, 0.015, 0.013);
        assert!(area > 0.0);
        assert!(inertia > 0.0);
        assert!(z > 0.0);
    }

    #[test]
    fn test_resonance_detection() {
        // Doğal frekansı rotasyon frekansına yakın ayarlayalım
        let input = MrStructureIn {
            d_frame: 0.500,
            r_plate: 0.050,
            l_arm_override: 0.200,
            arm_count: 4,
            coaxial: false,
            t_max_motor: 10.0,
            t_hover_motor: 4.0,
            m_motor: 0.050,
            n_limit: 2.0,
            fs: 1.5,
            section_type: MrArmSectionType::Round,
            d_o: 0.016,
            d_i: 0.014,
            sigma_allow: 600.0e6,
            e_modulus: 120.0e9,
            rho: 1550.0,
            rpm_hover: 0.0, // RPM'i daha sonra frekansa eşitleyeceğiz
            prop_blades: 2,
        };
        let out1 = mr_structure(&input);
        let fnat = out1.f_natural;
        assert!(fnat > 0.0);

        // RPM = fnat * 60 (birebir çakışma)
        let mut input_res = input.clone();
        input_res.rpm_hover = fnat * 60.0;
        let out_res = mr_structure(&input_res);
        assert!(out_res.resonance_rot_warn, "Rezonans uyarısı tetiklenmeli");
    }

    #[test]
    fn test_nan_guards() {
        let input = MrStructureIn {
            d_frame: f64::NAN,
            r_plate: 0.050,
            l_arm_override: 0.0,
            arm_count: 4,
            coaxial: false,
            t_max_motor: f64::NAN,
            t_hover_motor: 0.0,
            m_motor: 0.0,
            n_limit: 2.0,
            fs: 1.5,
            section_type: MrArmSectionType::Round,
            d_o: 0.016,
            d_i: 0.016, // Sıfır kalınlık
            sigma_allow: 0.0,
            e_modulus: 0.0,
            rho: 0.0,
            rpm_hover: 0.0,
            prop_blades: 2,
        };
        let out = mr_structure(&input);
        assert_eq!(out.inertia_i, 0.0);
        assert_eq!(out.sigma_max, 0.0);
        assert!(!out.f_natural.is_nan());
    }
}
