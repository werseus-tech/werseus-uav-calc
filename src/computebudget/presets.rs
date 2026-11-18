// SPDX-License-Identifier: AGPL-3.0-or-later
//! Built-in example companion computers and sensors. The numbers are clearly approximate,
//! typical values for the bare board (no carrier extras); users are expected to edit them
//! and to check the manufacturer datasheet.

/// A companion computer example.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ComputerPreset {
    pub name: &'static str,
    pub idle_w: f64,
    pub load_w: f64,
    pub mass_g: f64,
}

/// A sensor example.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SensorPreset {
    pub name: &'static str,
    pub power_w: f64,
    pub mass_g: f64,
}

pub const COMPUTERS: &[ComputerPreset] = &[
    ComputerPreset {
        name: "Raspberry Pi 4 (class)",
        idle_w: 2.7,
        load_w: 6.4,
        mass_g: 46.0,
    },
    ComputerPreset {
        name: "Raspberry Pi 5 (class)",
        idle_w: 3.0,
        load_w: 8.0,
        mass_g: 46.0,
    },
    ComputerPreset {
        name: "Jetson Nano (class)",
        idle_w: 2.0,
        load_w: 10.0,
        mass_g: 140.0,
    },
    ComputerPreset {
        name: "Jetson Orin Nano (class)",
        idle_w: 4.0,
        load_w: 15.0,
        mass_g: 175.0,
    },
];

pub const SENSORS: &[SensorPreset] = &[
    SensorPreset {
        name: "RGB camera (CSI/USB)",
        power_w: 1.5,
        mass_g: 30.0,
    },
    SensorPreset {
        name: "Depth camera",
        power_w: 3.5,
        mass_g: 90.0,
    },
    SensorPreset {
        name: "1D laser rangefinder",
        power_w: 0.5,
        mass_g: 10.0,
    },
    SensorPreset {
        name: "2D lidar",
        power_w: 2.5,
        mass_g: 100.0,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_are_sane() {
        for c in COMPUTERS {
            assert!(c.idle_w > 0.0 && c.load_w >= c.idle_w && c.mass_g > 0.0);
        }
        for s in SENSORS {
            assert!(s.power_w > 0.0 && s.mass_g > 0.0);
        }
    }
}
