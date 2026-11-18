// SPDX-License-Identifier: AGPL-3.0-or-later
//! Mechanical standards used in small multirotor and fixed-wing builds: motor mount bolt
//! patterns, flight-controller / ESC stack patterns, carbon plate thicknesses and arm profiles.
//!
//! Everything here is pure data and pure functions so geometry / CAD code can use it later.
//! Hole spacing values are centre-to-centre distances in millimetres.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// How the four bolt holes of a [`MountPattern`] are arranged around the pattern centre.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MountLayout {
    /// Two orthogonal bolt-circle diameters across the shaft centre (motor bases):
    /// holes at `[±x_mm / 2, 0]` and `[0, ±y_mm / 2]`.
    #[default]
    MotorBcd,
    /// Rectangular corner spacing (flight-controller / ESC stacks):
    /// holes at `[±x_mm / 2, ±y_mm / 2]`.
    Rectangular,
}

/// A four-bolt mounting pattern: spacing dimensions in mm, nominal bolt diameter, and hole layout.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MountPattern {
    pub x_mm: f64,
    pub y_mm: f64,
    /// Nominal thread diameter in mm (M2 = 2.0, M3 = 3.0).
    pub bolt_mm: f64,
    #[serde(default)]
    pub layout: MountLayout,
}

impl MountPattern {
    /// Creates a motor mount pattern defined by two orthogonal bolt-circle diameters.
    pub const fn new(x_mm: f64, y_mm: f64, bolt_mm: f64) -> Self {
        Self {
            x_mm,
            y_mm,
            bolt_mm,
            layout: MountLayout::MotorBcd,
        }
    }

    /// Creates a rectangular corner-spaced bolt pattern (used for FC / ESC stacks).
    pub const fn stack(x_mm: f64, y_mm: f64, bolt_mm: f64) -> Self {
        Self {
            x_mm,
            y_mm,
            bolt_mm,
            layout: MountLayout::Rectangular,
        }
    }

    /// Opposite hole-to-hole distance across the centre (bolt circle diameter).
    pub fn diagonal_mm(&self) -> f64 {
        match self.layout {
            MountLayout::MotorBcd => self.x_mm.max(self.y_mm),
            MountLayout::Rectangular => self.x_mm.hypot(self.y_mm),
        }
    }

    /// The four hole centres relative to the pattern centre.
    pub fn holes(&self) -> [[f64; 2]; 4] {
        let (hx, hy) = (self.x_mm / 2.0, self.y_mm / 2.0);
        match self.layout {
            MountLayout::MotorBcd => [[-hx, 0.0], [hx, 0.0], [0.0, -hy], [0.0, hy]],
            MountLayout::Rectangular => [[-hx, -hy], [hx, -hy], [hx, hy], [-hx, hy]],
        }
    }

    /// Close-fit clearance hole diameter for the bolt, rounded to 0.1 mm.
    pub fn clearance_hole_mm(&self) -> f64 {
        if self.bolt_mm <= 2.0 {
            2.2
        } else if self.bolt_mm <= 2.5 {
            2.7
        } else if self.bolt_mm <= 3.0 {
            3.2
        } else {
            self.bolt_mm + 0.4
        }
    }
}

impl fmt::Display for MountPattern {
    /// `16x16 M3`, `30.5x30.5 M3`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}x{} M{}", self.x_mm, self.y_mm, self.bolt_mm)
    }
}

impl FromStr for MountPattern {
    type Err = String;

    /// Parses `16x16 M3` (also `16 x 19 m3`; a decimal comma is accepted).
    fn from_str(s: &str) -> Result<Self, String> {
        let bad = || format!("\"{s}\" is not a pattern like 16x16 M3");
        let norm = s.trim().to_lowercase().replace(',', ".");
        let (dims, bolt) = norm.split_once('m').ok_or_else(bad)?;
        let (x, y) = dims.trim().split_once('x').ok_or_else(bad)?;
        let num = |v: &str| {
            v.trim()
                .parse::<f64>()
                .ok()
                .filter(|n| n.is_finite() && *n > 0.0)
                .ok_or_else(bad)
        };
        Ok(Self::new(num(x)?, num(y)?, num(bolt)?))
    }
}

/// A named standard pattern.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NamedPattern {
    pub name: &'static str,
    pub pattern: MountPattern,
}

const fn np(name: &'static str, x: f64, y: f64, bolt: f64) -> NamedPattern {
    NamedPattern {
        name,
        pattern: MountPattern::new(x, y, bolt),
    }
}

const fn sp(name: &'static str, x: f64, y: f64, bolt: f64) -> NamedPattern {
    NamedPattern {
        name,
        pattern: MountPattern::stack(x, y, bolt),
    }
}

/// Motor mount bolt patterns (two orthogonal bolt-circle diameters on the motor base).
pub const MOTOR_MOUNTS: &[NamedPattern] = &[
    np("9x9 M2", 9.0, 9.0, 2.0),
    np("12x12 M2", 12.0, 12.0, 2.0),
    np("16x16 M3", 16.0, 16.0, 3.0),
    np("16x19 M3", 16.0, 19.0, 3.0),
    np("19x19 M3", 19.0, 19.0, 3.0),
    np("25x25 M3", 25.0, 25.0, 3.0),
];

/// Flight-controller / ESC stack patterns (rectangular X/Y corner spacing).
pub const STACK_PATTERNS: &[NamedPattern] = &[
    sp("16x16 M2", 16.0, 16.0, 2.0),
    sp("20x20 M2", 20.0, 20.0, 2.0),
    sp("20x20 M3", 20.0, 20.0, 3.0),
    sp("25.5x25.5 M2", 25.5, 25.5, 2.0),
    sp("30.5x30.5 M3", 30.5, 30.5, 3.0),
];

/// Standard carbon-fibre plate thicknesses in mm.
pub const CARBON_PLATE_THICKNESSES_MM: &[f64] = &[1.5, 2.0, 3.0, 4.0, 5.0, 6.0];

/// Looks up a motor mount pattern by its name (`"16x16 M3"`).
pub fn motor_mount(name: &str) -> Option<MountPattern> {
    find(MOTOR_MOUNTS, name)
}

/// Looks up a stack pattern by its name (`"30.5x30.5 M3"`).
pub fn stack_pattern(name: &str) -> Option<MountPattern> {
    find(STACK_PATTERNS, name)
}

fn find(list: &[NamedPattern], name: &str) -> Option<MountPattern> {
    let want = name.trim().to_lowercase();
    list.iter()
        .find(|p| p.name.to_lowercase() == want)
        .map(|p| p.pattern)
}

/// Arm cross-section. All dimensions in mm.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum ArmProfile {
    /// Flat plate arm lying in the rotor plane: `width_mm` is the horizontal arm width,
    /// `thickness_mm` the vertical plate thickness in the bending direction.
    FlatPlate { width_mm: f64, thickness_mm: f64 },
    /// Round tube with outer and inner diameter.
    RoundTube { od_mm: f64, id_mm: f64 },
    /// Square tube with outer side and wall thickness.
    SquareTube { side_mm: f64, wall_mm: f64 },
}

impl ArmProfile {
    /// Material cross-section area in mm^2.
    pub fn area_mm2(&self) -> f64 {
        match *self {
            Self::FlatPlate {
                width_mm,
                thickness_mm,
            } => width_mm.max(0.0) * thickness_mm.max(0.0),
            Self::RoundTube { od_mm, id_mm } => {
                let od = od_mm.max(0.0);
                let id = id_mm.clamp(0.0, od);
                std::f64::consts::PI / 4.0 * (od * od - id * id)
            }
            Self::SquareTube { side_mm, wall_mm } => {
                let side = side_mm.max(0.0);
                let inner = (side - 2.0 * wall_mm.max(0.0)).max(0.0);
                side * side - inner * inner
            }
        }
    }

    /// Second moment of area in mm^4 for vertical bending across the arm thickness.
    pub fn second_moment_mm4(&self) -> f64 {
        match *self {
            Self::FlatPlate {
                width_mm,
                thickness_mm,
            } => width_mm.max(0.0) * thickness_mm.max(0.0).powi(3) / 12.0,
            Self::RoundTube { od_mm, id_mm } => {
                let od = od_mm.max(0.0);
                let id = id_mm.clamp(0.0, od);
                std::f64::consts::PI / 64.0 * (od.powi(4) - id.powi(4))
            }
            Self::SquareTube { side_mm, wall_mm } => {
                let side = side_mm.max(0.0);
                let inner = (side - 2.0 * wall_mm.max(0.0)).max(0.0);
                (side.powi(4) - inner.powi(4)) / 12.0
            }
        }
    }

    /// Mass per metre in g/m for a material density in kg/m^3.
    pub fn mass_per_m_g(&self, density_kg_m3: f64) -> f64 {
        // 1 mm^2 over 1 m of length is 1e-6 m^3.
        self.area_mm2() * 1e-6 * density_kg_m3 * 1000.0
    }

    /// Human readable size, e.g. `16/14 mm round tube`.
    pub fn label(&self) -> String {
        match *self {
            Self::FlatPlate {
                width_mm,
                thickness_mm,
            } => format!("{width_mm}x{thickness_mm} mm plate"),
            Self::RoundTube { od_mm, id_mm } => format!("{od_mm}/{id_mm} mm round tube"),
            Self::SquareTube { side_mm, wall_mm } => {
                format!("{side_mm}x{side_mm}x{wall_mm} mm square tube")
            }
        }
    }
}

const fn plate(width_mm: f64, thickness_mm: f64) -> ArmProfile {
    ArmProfile::FlatPlate {
        width_mm,
        thickness_mm,
    }
}

const fn round(od_mm: f64, id_mm: f64) -> ArmProfile {
    ArmProfile::RoundTube { od_mm, id_mm }
}

const fn square(side_mm: f64, wall_mm: f64) -> ArmProfile {
    ArmProfile::SquareTube { side_mm, wall_mm }
}

/// Commonly stocked arm sizes.
pub const ARM_PROFILES: &[ArmProfile] = &[
    plate(10.0, 3.0),
    plate(12.0, 4.0),
    plate(15.0, 4.0),
    plate(20.0, 5.0),
    round(10.0, 8.0),
    round(12.0, 10.0),
    round(16.0, 14.0),
    round(20.0, 18.0),
    round(25.0, 23.0),
    square(10.0, 1.0),
    square(16.0, 1.0),
    square(20.0, 1.5),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stack_30_5_uses_m3() {
        let p = stack_pattern("30.5x30.5 M3").unwrap();
        assert_eq!((p.x_mm, p.y_mm, p.bolt_mm), (30.5, 30.5, 3.0));
        assert!((p.diagonal_mm() - 43.134).abs() < 1e-3);
        assert_eq!(
            p.holes(),
            [
                [-15.25, -15.25],
                [15.25, -15.25],
                [15.25, 15.25],
                [-15.25, 15.25]
            ]
        );
    }

    #[test]
    fn stack_patterns_match_standards() {
        assert_eq!(stack_pattern("16x16 M2").unwrap().bolt_mm, 2.0);
        assert_eq!(stack_pattern("20x20 M3").unwrap().bolt_mm, 3.0);
        assert_eq!(stack_pattern("20x20 M2").unwrap().bolt_mm, 2.0);
        assert_eq!(stack_pattern("25.5x25.5 M2").unwrap().x_mm, 25.5);
    }

    #[test]
    fn motor_mounts_match_standards() {
        for (name, x, y, m) in [
            ("9x9 M2", 9.0, 9.0, 2.0),
            ("12x12 M2", 12.0, 12.0, 2.0),
            ("16x16 M3", 16.0, 16.0, 3.0),
            ("16x19 M3", 16.0, 19.0, 3.0),
            ("19x19 M3", 19.0, 19.0, 3.0),
            ("25x25 M3", 25.0, 25.0, 3.0),
        ] {
            let pat = motor_mount(name).unwrap();
            assert_eq!(pat, MountPattern::new(x, y, m), "{name}");
            assert_eq!(pat.diagonal_mm(), x.max(y));
            assert_eq!(
                pat.holes(),
                [
                    [-x / 2.0, 0.0],
                    [x / 2.0, 0.0],
                    [0.0, -y / 2.0],
                    [0.0, y / 2.0]
                ]
            );
        }
    }

    #[test]
    fn names_are_consistent_with_values() {
        for p in MOTOR_MOUNTS {
            assert_eq!(p.pattern.to_string(), p.name);
            assert_eq!(p.name.parse::<MountPattern>().unwrap(), p.pattern);
        }
        for p in STACK_PATTERNS {
            assert_eq!(p.pattern.to_string(), p.name);
            let parsed = p.name.parse::<MountPattern>().unwrap();
            assert_eq!(
                (parsed.x_mm, parsed.y_mm, parsed.bolt_mm),
                (p.pattern.x_mm, p.pattern.y_mm, p.pattern.bolt_mm)
            );
        }
    }

    #[test]
    fn plate_thicknesses() {
        assert_eq!(CARBON_PLATE_THICKNESSES_MM, &[1.5, 2.0, 3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn pattern_parse_errors_and_holes() {
        assert!("nonsense".parse::<MountPattern>().is_err());
        assert!("16x M3".parse::<MountPattern>().is_err());
        assert!("NaNx16 M3".parse::<MountPattern>().is_err());
        assert!("infx16 M3".parse::<MountPattern>().is_err());
        assert!("-16x16 M3".parse::<MountPattern>().is_err());
        assert!("16x16 M0".parse::<MountPattern>().is_err());
        assert_eq!(
            "16,5 x 16 m3".parse::<MountPattern>().unwrap(),
            MountPattern::new(16.5, 16.0, 3.0)
        );
        let h = MountPattern::new(20.0, 20.0, 3.0).holes();
        assert_eq!(h[0], [-10.0, 0.0]);
        assert_eq!(h[1], [10.0, 0.0]);
        assert_eq!(h[2], [0.0, -10.0]);
        assert_eq!(h[3], [0.0, 10.0]);
        let sh = MountPattern::stack(20.0, 20.0, 3.0).holes();
        assert_eq!(sh[0], [-10.0, -10.0]);
        assert_eq!(sh[2], [10.0, 10.0]);
        assert_eq!(MountPattern::new(20.0, 20.0, 3.0).clearance_hole_mm(), 3.2);
    }

    #[test]
    fn arm_profile_geometry() {
        let p = plate(12.0, 4.0);
        assert_eq!(p.area_mm2(), 48.0);
        assert!((p.second_moment_mm4() - 12.0 * 64.0 / 12.0).abs() < 1e-9);
        assert!((plate(15.0, 4.0).second_moment_mm4() - 80.0).abs() < 1e-9);
        assert!((round(16.0, 14.0).area_mm2() - 47.124).abs() < 1e-2);
        assert_eq!(square(10.0, 1.0).area_mm2(), 36.0);
        // Degenerate tube dimensions clamp the inner void instead of going negative or inverting.
        assert_eq!(round(10.0, 12.0).area_mm2(), 0.0);
        assert_eq!(round(10.0, 12.0).second_moment_mm4(), 0.0);
        assert_eq!(square(10.0, 6.0).area_mm2(), 100.0);
        assert!((square(10.0, 6.0).second_moment_mm4() - 10_000.0 / 12.0).abs() < 1e-9);
        // 100 mm^2 of carbon at 1600 kg/m^3 is 160 g/m.
        assert!((plate(20.0, 5.0).mass_per_m_g(1600.0) - 160.0).abs() < 1e-9);
        assert!(ARM_PROFILES.iter().all(|a| a.area_mm2() > 0.0));
    }
}
