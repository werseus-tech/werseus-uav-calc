// SPDX-License-Identifier: AGPL-3.0-or-later
//! Vehicle-level structural analysis built on the beam models: which members apply to the
//! vehicle type, their natural frequencies against the rotor speed range, and static load cases.
//!
//! Inputs are taken from the project and the evaluated `Model` (arm geometry, material and
//! loads of the existing arm structure step; wing spar of the existing wing structure step), so
//! the results stay consistent with those steps. Simplified beam model, not a 3D FEA.

use super::beam::{BeamMode, BeamModel, PointForce, PointMass, ShaftModel};
use super::material::{self, MatProps, StructMaterial};
use crate::calc::Model;
use crate::calc::mr_structure::{MrArmSectionType, arm_section_properties};
use crate::project::{DroneType, Project};
use std::f64::consts::PI;

pub const G0: f64 = 9.80665;

/// Section override for the arm.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ArmSection {
    /// Tube section of the arm structure step (round/square, outer/inner size).
    Project,
    /// Solid flat plate arm, bending about its weak axis.
    Plate { width: f64, thickness: f64 },
}

/// User settings of the analysis screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FeaSettings {
    pub n_el: usize,
    /// Hard landing vertical load factor (g).
    pub landing_g: f64,
    /// `None` = material of the arm structure step.
    pub arm_material: Option<StructMaterial>,
    pub arm_section: ArmSection,
    /// `None` = wing structure step values (allowable stress) with a pultruded-carbon modulus.
    pub wing_material: Option<StructMaterial>,
    /// Relative margin around an excitation band that is still reported as "caution".
    pub margin: f64,
    /// Lowest operating rotor speed of the lift rotors as a fraction of hover RPM.
    pub low_rpm_frac: f64,
}

impl Default for FeaSettings {
    fn default() -> Self {
        Self {
            n_el: 20,
            landing_g: 4.0,
            arm_material: None,
            arm_section: ArmSection::Project,
            wing_material: None,
            margin: 0.15,
            low_rpm_frac: 0.6,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemberKind {
    /// Multirotor arm (cantilever from the centre plate, motor at the tip).
    Arm,
    /// VTOL lift-motor boom section, modelled like an arm (motor at the tip).
    Boom,
    /// Half wing spar (cantilever from the root).
    WingHalf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModeKind {
    Bending,
    Torsion,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModeResult {
    pub kind: ModeKind,
    /// 1-based index within its kind.
    pub index: usize,
    pub freq_hz: f64,
    /// Node displacements (bending only; empty for torsion).
    pub shape: Vec<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExcitationSource {
    LiftRotor,
    CruiseProp,
}

/// Rotor speed range acting on a member.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Excitation {
    pub source: ExcitationSource,
    pub rpm_lo: f64,
    pub rpm_hi: f64,
    pub blades: u32,
    /// Rotor mounted on the member itself (direct excitation) or elsewhere on the airframe.
    pub direct: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Risk {
    Caution,
    High,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Harmonic {
    /// Once per revolution (imbalance).
    Rev1,
    /// Blade-pass frequency.
    BladePass,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Resonance {
    pub mode_kind: ModeKind,
    pub mode_index: usize,
    pub freq_hz: f64,
    pub source: ExcitationSource,
    pub harmonic: Harmonic,
    pub risk: Risk,
    /// Rotor speed at which the excitation equals the mode frequency.
    pub rpm_cross: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadCase {
    HardLanding,
    MaxThrust,
    WingBending,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LoadCaseResult {
    pub case: LoadCase,
    /// `false` when the case does not apply to this member (row shown as "not applicable").
    pub applies: bool,
    pub max_moment: f64,
    pub max_stress: f64,
    pub max_deflection: f64,
    /// sigma_allow / sigma at limit load.
    pub safety_factor: f64,
    /// Required factor of safety (structure step input).
    pub required_sf: f64,
}

impl LoadCaseResult {
    pub fn ok(&self) -> bool {
        !self.applies || self.safety_factor >= self.required_sf
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MemberResult {
    pub kind: MemberKind,
    pub length: f64,
    pub material: MatProps,
    pub tip_mass: f64,
    /// `false` when the section/material inputs are invalid (no numbers are reported).
    pub valid: bool,
    pub node_x: Vec<f64>,
    pub modes: Vec<ModeResult>,
    pub excitations: Vec<Excitation>,
    pub resonances: Vec<Resonance>,
    pub load_cases: Vec<LoadCaseResult>,
}

impl MemberResult {
    pub fn worst_risk(&self) -> Option<Risk> {
        self.resonances.iter().map(|r| r.risk).max()
    }
}

/// Members that do not apply to the vehicle type (shown in the UI with a reason).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotApplicable {
    /// Multirotor: there is no wing.
    NoWing,
    /// Fixed-wing / flying wing: there are no motor arms.
    NoArms,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FeaReport {
    pub members: Vec<MemberResult>,
    pub not_applicable: Vec<NotApplicable>,
}

/// Section properties: (area, I, Z, torsion constant J, polar second moment Ip).
pub fn section(sec: ArmSection, kind: MrArmSectionType, d_o: f64, d_i: f64) -> [f64; 5] {
    match sec {
        ArmSection::Project => {
            let (a, i, z) = arm_section_properties(kind, d_o, d_i);
            if a <= 0.0 {
                return [0.0; 5];
            }
            let j = match kind {
                MrArmSectionType::Round => PI / 32.0 * (d_o.powi(4) - d_i.powi(4)),
                MrArmSectionType::Square => {
                    // Bredt thin-wall closed section: J = 4 A_m^2 t / s_m = a_m^3 t.
                    let t = 0.5 * (d_o - d_i);
                    let a_m = 0.5 * (d_o + d_i);
                    a_m.powi(3) * t
                }
            };
            [a, i, z, j, 2.0 * i]
        }
        ArmSection::Plate { width, thickness } => {
            let (w, t) = (width, thickness);
            if !(w.is_finite() && t.is_finite() && w > 0.0 && t > 0.0) {
                return [0.0; 5];
            }
            let (long, short) = if w >= t { (w, t) } else { (t, w) };
            let a = w * t;
            let i = w * t.powi(3) / 12.0;
            let z = w * t * t / 6.0;
            let j = long * short.powi(3) / 3.0 * (1.0 - 0.63 * short / long);
            let ip = w * t * (w * w + t * t) / 12.0;
            [a, i, z, j, ip]
        }
    }
}

fn pos(v: f64, fallback: f64) -> f64 {
    if v.is_finite() && v > 0.0 {
        v
    } else {
        fallback
    }
}

/// Polar inertia of a motor about an axis through its centre, normal to the motor shaft
/// (the arm axis): solid cylinder with height = radius and an effective density of 3000 kg/m^3.
pub fn motor_tip_inertia(m_motor: f64) -> f64 {
    if !(m_motor.is_finite() && m_motor > 0.0) {
        return 0.0;
    }
    let r = (m_motor / (3000.0 * PI)).cbrt();
    m_motor * (3.0 * r * r + r * r) / 12.0
}

/// Lift rotor operating speed range from the arm structure step's hover RPM and the
/// max/hover thrust ratio (thrust ~ RPM^2).
pub fn lift_rotor_band(p: &Project, m: &Model, s: &FeaSettings) -> Option<Excitation> {
    let rpm_hover = m.mr_st.f_rot_hover * 60.0;
    if !(rpm_hover.is_finite() && rpm_hover > 0.0) {
        return None;
    }
    let ratio = if m.mr.t_hover_motor.is_finite() && m.mr.t_hover_motor > 0.0 {
        (p.mr_t_max_motor / m.mr.t_hover_motor).clamp(1.0, 9.0)
    } else {
        1.7
    };
    Some(Excitation {
        source: ExcitationSource::LiftRotor,
        rpm_lo: rpm_hover * s.low_rpm_frac.clamp(0.1, 1.0),
        rpm_hi: rpm_hover * ratio.sqrt(),
        blades: p.mr_prop_blades.max(1),
        direct: true,
    })
}

/// Cruise propeller speed range (fixed-wing propulsion step), two blades assumed.
pub fn cruise_prop_band(m: &Model) -> Option<Excitation> {
    let lo = m.prop.rpm_cr;
    let hi = m.prop.rpm_max.max(lo);
    if !(lo.is_finite() && hi.is_finite() && lo > 0.0) {
        return None;
    }
    Some(Excitation {
        source: ExcitationSource::CruiseProp,
        rpm_lo: lo,
        rpm_hi: hi,
        blades: 2,
        direct: false,
    })
}

/// Resonance check of every mode against every excitation band (1x and blade-pass).
pub fn find_resonances(modes: &[ModeResult], exc: &[Excitation], margin: f64) -> Vec<Resonance> {
    let margin = if margin.is_finite() {
        margin.clamp(0.0, 1.0)
    } else {
        0.15
    };
    let mut out = Vec::new();
    for md in modes {
        let f = md.freq_hz;
        if !(f.is_finite() && f > 0.0) {
            continue;
        }
        for e in exc {
            for (harm, h) in [
                (Harmonic::Rev1, 1.0),
                (Harmonic::BladePass, e.blades.max(1) as f64),
            ] {
                if harm == Harmonic::BladePass && e.blades <= 1 {
                    continue;
                }
                let lo = h * e.rpm_lo / 60.0;
                let hi = h * e.rpm_hi / 60.0;
                let risk = if f >= lo && f <= hi {
                    Some(Risk::High)
                } else if f >= lo * (1.0 - margin) && f <= hi * (1.0 + margin) {
                    Some(Risk::Caution)
                } else {
                    None
                };
                if let Some(risk) = risk {
                    out.push(Resonance {
                        mode_kind: md.kind,
                        mode_index: md.index,
                        freq_hz: f,
                        source: e.source,
                        harmonic: harm,
                        risk,
                        rpm_cross: f * 60.0 / h,
                    });
                }
            }
        }
    }
    out
}

fn bending_modes(beam: &BeamModel, count: usize) -> Vec<ModeResult> {
    beam.modes(count)
        .into_iter()
        .enumerate()
        .map(|(i, BeamMode { freq_hz, shape })| ModeResult {
            kind: ModeKind::Bending,
            index: i + 1,
            freq_hz,
            shape,
        })
        .collect()
}

fn invalid_member(kind: MemberKind, length: f64, mat: MatProps, tip_mass: f64) -> MemberResult {
    MemberResult {
        kind,
        length,
        material: mat,
        tip_mass,
        valid: false,
        node_x: Vec::new(),
        modes: Vec::new(),
        excitations: Vec::new(),
        resonances: Vec::new(),
        load_cases: Vec::new(),
    }
}

/// Arm material: the arm structure step's values or an override.
pub fn arm_material(p: &Project, s: &FeaSettings) -> MatProps {
    match s.arm_material {
        Some(mat) => mat.props(),
        None => MatProps {
            sigma_allow: p.mr_arm_sigma_allow,
            e: p.mr_arm_e_modulus,
            g: material::arm_preset_shear_modulus(p.mr_arm_material, p.mr_arm_e_modulus),
            rho: p.mr_arm_rho,
        },
    }
}

/// Multirotor arm / VTOL boom: cantilever from the centre plate (or wing) with the motor(s) at
/// the tip. Length, thrust, tip mass, section and loads come from the arm structure step.
pub fn analyse_arm(p: &Project, m: &Model, s: &FeaSettings, kind: MemberKind) -> MemberResult {
    let st = &m.mr_st;
    let mat = arm_material(p, s);
    let l = st.l_arm;
    let n_coax = if p.mr_coaxial { 2.0 } else { 1.0 };
    let tip_mass = p.mr_m_motor.max(0.0) * n_coax;
    let [area, inertia, z, j, ip] =
        section(s.arm_section, p.mr_arm_section, p.mr_arm_d_o, p.mr_arm_d_i);
    let ei = mat.e * inertia;
    let mu = mat.rho * area;
    let n_el = s.n_el.clamp(2, 200);
    let ok = [l, ei, mu, z, mat.sigma_allow]
        .iter()
        .all(|v| v.is_finite() && *v > 0.0)
        && tip_mass.is_finite();
    if !ok {
        return invalid_member(kind, l, mat, tip_mass);
    }
    let beam = BeamModel {
        length: l,
        n_el,
        ei,
        mu,
        point_masses: vec![PointMass {
            x: l,
            mass: tip_mass,
            rot_inertia: 0.0,
        }],
    };
    let mut modes = bending_modes(&beam, 3);
    let shaft = ShaftModel {
        length: l,
        n_el,
        gj: mat.g * j,
        rho_ip: mat.rho * ip,
        tip_inertia: motor_tip_inertia(p.mr_m_motor) * n_coax,
    };
    if let Some(&f) = shaft.frequencies(1).first() {
        modes.push(ModeResult {
            kind: ModeKind::Torsion,
            index: 1,
            freq_hz: f,
            shape: Vec::new(),
        });
    }

    let n_limit = pos(p.mr_arm_n_limit, 2.0);
    let fs = pos(p.mr_arm_fs, 1.5);
    let case = |case: LoadCase, forces: &[PointForce], q: f64| {
        let r = beam.solve_static(forces, &|_| q);
        let sigma = r.max_moment / z;
        LoadCaseResult {
            case,
            applies: true,
            max_moment: r.max_moment,
            max_stress: sigma,
            max_deflection: r.max_deflection,
            safety_factor: if sigma > 0.0 {
                mat.sigma_allow / sigma
            } else {
                f64::INFINITY
            },
            required_sf: fs,
        }
    };
    let gl = pos(s.landing_g, 4.0);
    let load_cases = vec![
        case(
            LoadCase::HardLanding,
            &[PointForce {
                x: l,
                force: -tip_mass * G0 * gl,
            }],
            -mu * G0 * gl,
        ),
        // Same definition as the arm structure step: max thrust of the motor(s) times the
        // limit load factor at the tip (arm self weight neglected).
        case(
            LoadCase::MaxThrust,
            &[PointForce {
                x: l,
                force: st.f_thrust_max * n_limit,
            }],
            0.0,
        ),
        LoadCaseResult {
            case: LoadCase::WingBending,
            applies: false,
            max_moment: 0.0,
            max_stress: 0.0,
            max_deflection: 0.0,
            safety_factor: 0.0,
            required_sf: fs,
        },
    ];
    let excitations: Vec<Excitation> = lift_rotor_band(p, m, s).into_iter().collect();
    let resonances = find_resonances(&modes, &excitations, s.margin);
    MemberResult {
        kind,
        length: l,
        material: mat,
        tip_mass,
        valid: true,
        node_x: beam.node_x(),
        modes,
        excitations,
        resonances,
        load_cases,
    }
}

/// Spanwise position of a VTOL boom: lateral offset of an X layout of the given frame
/// diagonal, kept inside the half span.
pub fn vtol_boom_y(p: &Project) -> f64 {
    let y = p.mr_frame_size.max(0.0) / (2.0 * std::f64::consts::SQRT_2);
    y.min(0.9 * p.b.max(0.0) * 0.5)
}

/// Wing material: structure step values (sigma_allow, carbon density) or an override.
pub fn wing_material(p: &Project, s: &FeaSettings) -> MatProps {
    match s.wing_material {
        Some(mat) => mat.props(),
        None => MatProps {
            sigma_allow: p.sigma_allow,
            e: material::WING_SPAR_DEFAULT_E,
            g: StructMaterial::CfTubePultruded.props().g,
            rho: p.rho_cf,
        },
    }
}

/// Half wing: spar tube of the wing structure step as a cantilever from the root carrying the
/// wing mass (uniform per span) and, for a VTOL, the boom with its two lift motors.
pub fn analyse_wing(p: &Project, m: &Model, s: &FeaSettings) -> MemberResult {
    let mat = wing_material(p, s);
    let l = 0.5 * p.b;
    let (area, inertia, z) =
        arm_section_properties(MrArmSectionType::Round, p.d_spar_o, p.d_spar_i);
    let ei = mat.e * inertia;
    let spar_mu = mat.rho * area;
    let mu = if m.weight.m_wing.is_finite() && m.weight.m_wing > 0.0 && p.b > 0.0 {
        (m.weight.m_wing / p.b).max(spar_mu)
    } else {
        spar_mu
    };
    let vtol = p.drone_type == DroneType::VtolQuadplane;
    let n_coax = if p.mr_coaxial { 2.0 } else { 1.0 };
    // Boom: two lift motors (front + rear) and a boom of length 2 l_arm.
    let boom_mass = if vtol {
        2.0 * p.mr_m_motor.max(0.0) * n_coax + m.mr_st.m_arm_single.max(0.0) * 2.0
    } else {
        0.0
    };
    let y_boom = vtol_boom_y(p);
    let n_el = s.n_el.clamp(2, 200);
    let ok = [l, ei, mu, z, mat.sigma_allow]
        .iter()
        .all(|v| v.is_finite() && *v > 0.0)
        && boom_mass.is_finite();
    if !ok {
        return invalid_member(MemberKind::WingHalf, l, mat, boom_mass);
    }
    let beam = BeamModel {
        length: l,
        n_el,
        ei,
        mu,
        point_masses: if vtol {
            vec![PointMass {
                x: y_boom,
                mass: boom_mass,
                rot_inertia: 0.0,
            }]
        } else {
            Vec::new()
        },
    };
    let modes = bending_modes(&beam, 3);
    let fs = pos(p.fs, 1.5);
    let make = |case: LoadCase, forces: &[PointForce], q: &dyn Fn(f64) -> f64| {
        let r = beam.solve_static(forces, q);
        let sigma = r.max_moment / z;
        LoadCaseResult {
            case,
            applies: true,
            max_moment: r.max_moment,
            max_stress: sigma,
            max_deflection: r.max_deflection,
            safety_factor: if sigma > 0.0 {
                mat.sigma_allow / sigma
            } else {
                f64::INFINITY
            },
            required_sf: fs,
        }
    };
    let gl = pos(s.landing_g, 4.0);
    let boom_force = |f: f64| {
        if vtol {
            vec![PointForce {
                x: y_boom,
                force: f,
            }]
        } else {
            Vec::new()
        }
    };
    let landing = make(
        LoadCase::HardLanding,
        &boom_force(-boom_mass * G0 * gl),
        &|_| -mu * G0 * gl,
    );
    let max_thrust = if vtol {
        let f = 2.0 * m.mr_st.f_thrust_max * pos(p.mr_arm_n_limit, 2.0);
        let mut r = make(LoadCase::MaxThrust, &boom_force(f), &|_| 0.0);
        r.required_sf = pos(p.mr_arm_fs, 1.5);
        r
    } else {
        LoadCaseResult {
            case: LoadCase::MaxThrust,
            applies: false,
            max_moment: 0.0,
            max_stress: 0.0,
            max_deflection: 0.0,
            safety_factor: 0.0,
            required_sf: fs,
        }
    };
    // Elliptic lift of n W / 2 on the half wing, same as the wing structure step
    // (no inertia relief), so the root moment is n W b / (3 pi).
    let n_limit = pos(p.n_limit, 3.8);
    let q0 = 2.0 * n_limit * m.weight.w / (PI * l);
    let bending = make(LoadCase::WingBending, &[], &|y: f64| {
        let r = (y / l).clamp(0.0, 1.0);
        q0 * (1.0 - r * r).max(0.0).sqrt()
    });
    let mut excitations = Vec::new();
    if vtol {
        excitations.extend(lift_rotor_band(p, m, s));
    }
    excitations.extend(cruise_prop_band(m));
    let resonances = find_resonances(&modes, &excitations, s.margin);
    MemberResult {
        kind: MemberKind::WingHalf,
        length: l,
        material: mat,
        tip_mass: boom_mass,
        valid: true,
        node_x: beam.node_x(),
        modes,
        excitations,
        resonances,
        load_cases: vec![landing, max_thrust, bending],
    }
}

/// Runs the analysis for every member that applies to the vehicle type.
pub fn analyse(p: &Project, m: &Model, s: &FeaSettings) -> FeaReport {
    let mut members = Vec::new();
    let mut not_applicable = Vec::new();
    match p.drone_type {
        DroneType::Multirotor => {
            members.push(analyse_arm(p, m, s, MemberKind::Arm));
            not_applicable.push(NotApplicable::NoWing);
        }
        DroneType::VtolQuadplane => {
            members.push(analyse_arm(p, m, s, MemberKind::Boom));
            members.push(analyse_wing(p, m, s));
        }
        DroneType::FixedWingConventional | DroneType::FixedWingVTail | DroneType::FlyingWing => {
            members.push(analyse_wing(p, m, s));
            not_applicable.push(NotApplicable::NoArms);
        }
    }
    FeaReport {
        members,
        not_applicable,
    }
}
