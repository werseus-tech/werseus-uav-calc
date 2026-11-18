// SPDX-License-Identifier: AGPL-3.0-or-later
//! Electrical component records (motor, propeller, ESC, battery) and their validation.

use super::standards::MountPattern;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Where a record came from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Source {
    /// Shipped with the app as a typical example (not a real product).
    Example,
    /// Typed in by the user.
    User,
    /// Imported from a shared file (the file name).
    Imported(String),
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Example => f.write_str("Example"),
            Self::User => f.write_str("User"),
            Self::Imported(name) => write!(f, "Imported: {name}"),
        }
    }
}

/// Battery chemistry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Chemistry {
    LiPo,
    LiHv,
    LiIon,
    LiFePo4,
}

impl Chemistry {
    pub const ALL: [Chemistry; 4] = [Self::LiPo, Self::LiHv, Self::LiIon, Self::LiFePo4];

    pub fn label(self) -> &'static str {
        match self {
            Self::LiPo => "LiPo",
            Self::LiHv => "LiHV",
            Self::LiIon => "Li-ion",
            Self::LiFePo4 => "LiFePO4",
        }
    }

    /// Nominal cell voltage in volts.
    pub fn nominal_cell_v(self) -> f64 {
        match self {
            Self::LiPo => 3.7,
            Self::LiHv => 3.8,
            Self::LiIon => 3.6,
            Self::LiFePo4 => 3.2,
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        let want = s.trim().to_lowercase();
        Self::ALL
            .into_iter()
            .find(|c| c.label().to_lowercase() == want)
    }
}

/// Behaviour shared by all record kinds.
pub trait Record: Clone + PartialEq {
    fn name(&self) -> &str;
    fn source(&self) -> &Source;
    fn set_source(&mut self, source: Source);
    fn notes(&self) -> &str;
    /// One line with the key values, for lists.
    fn summary(&self) -> String;
    /// All problems found, in plain English. Empty means valid.
    fn validate(&self) -> Vec<String>;
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Motor {
    pub name: String,
    pub kv: f64,
    pub r_internal_ohm: f64,
    pub i_noload_a: f64,
    /// Voltage at which `i_noload_a` was measured.
    pub i_noload_v: f64,
    pub max_current_a: f64,
    pub max_power_w: f64,
    pub mass_g: f64,
    pub mount: Option<MountPattern>,
    #[serde(default)]
    pub notes: String,
    pub source: Source,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Propeller {
    pub name: String,
    pub diameter_in: f64,
    pub pitch_in: f64,
    pub blades: u32,
    pub mass_g: f64,
    pub ct: Option<f64>,
    pub cp: Option<f64>,
    #[serde(default)]
    pub notes: String,
    pub source: Source,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Esc {
    pub name: String,
    pub continuous_a: f64,
    pub burst_a: Option<f64>,
    pub cells_min: u32,
    pub cells_max: u32,
    pub mass_g: f64,
    #[serde(default)]
    pub notes: String,
    pub source: Source,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Battery {
    pub name: String,
    pub chemistry: Chemistry,
    pub cells_s: u32,
    pub parallel_p: u32,
    /// Capacity of the whole pack in mAh (already includes the parallel cells).
    pub capacity_mah: f64,
    pub c_rating: f64,
    pub r_internal_mohm: Option<f64>,
    pub mass_g: f64,
    #[serde(default)]
    pub notes: String,
    pub source: Source,
}

/// Collects validation messages.
struct Check<'a> {
    kind: &'a str,
    errors: Vec<String>,
}

impl<'a> Check<'a> {
    fn new(kind: &'a str, name: &str) -> Self {
        let mut c = Self {
            kind,
            errors: Vec::new(),
        };
        let n = name.trim();
        if n.is_empty() {
            c.errors.push(format!("{}: name must not be empty", c.kind));
        } else if n.chars().count() > 100 {
            c.errors
                .push(format!("{}: name is longer than 100 characters", c.kind));
        }
        c
    }

    fn range(&mut self, field: &str, v: f64, min: f64, max: f64, unit: &str) {
        if !v.is_finite() {
            self.errors
                .push(format!("{}: {field} is not a number", self.kind));
        } else if v < min || v > max {
            self.errors.push(format!(
                "{}: {field} = {v} is outside the plausible range {min} to {max} {unit}",
                self.kind
            ));
        }
    }

    fn opt_range(&mut self, field: &str, v: Option<f64>, min: f64, max: f64, unit: &str) {
        if let Some(v) = v {
            self.range(field, v, min, max, unit);
        }
    }

    fn int_range(&mut self, field: &str, v: u32, min: u32, max: u32) {
        if v < min || v > max {
            self.errors.push(format!(
                "{}: {field} = {v} is outside the plausible range {min} to {max}",
                self.kind
            ));
        }
    }
}

impl Record for Motor {
    fn notes(&self) -> &str {
        &self.notes
    }
    fn summary(&self) -> String {
        let mount = self.mount.map(|m| format!(" | {m}")).unwrap_or_default();
        format!(
            "{} KV | {} ohm | {} A | {} g{mount}",
            self.kv, self.r_internal_ohm, self.max_current_a, self.mass_g
        )
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn source(&self) -> &Source {
        &self.source
    }
    fn set_source(&mut self, source: Source) {
        self.source = source;
    }
    fn validate(&self) -> Vec<String> {
        let mut c = Check::new("motor", &self.name);
        c.range("kv", self.kv, 1.0, 20_000.0, "rpm/V");
        c.range(
            "internal resistance",
            self.r_internal_ohm,
            0.000_1,
            10.0,
            "ohm",
        );
        c.range("no-load current", self.i_noload_a, 0.0, 50.0, "A");
        c.range("no-load voltage", self.i_noload_v, 0.1, 100.0, "V");
        c.range("max current", self.max_current_a, 0.1, 1000.0, "A");
        c.range("max power", self.max_power_w, 1.0, 50_000.0, "W");
        c.range("mass", self.mass_g, 0.1, 20_000.0, "g");
        if let Some(m) = &self.mount {
            c.range("mount X spacing", m.x_mm, 3.0, 100.0, "mm");
            c.range("mount Y spacing", m.y_mm, 3.0, 100.0, "mm");
            c.range("mount bolt", m.bolt_mm, 1.0, 10.0, "mm");
        }
        c.errors
    }
}

impl Record for Propeller {
    fn notes(&self) -> &str {
        &self.notes
    }
    fn summary(&self) -> String {
        format!(
            "{}x{} in | {} blades | {} g",
            self.diameter_in, self.pitch_in, self.blades, self.mass_g
        )
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn source(&self) -> &Source {
        &self.source
    }
    fn set_source(&mut self, source: Source) {
        self.source = source;
    }
    fn validate(&self) -> Vec<String> {
        let mut c = Check::new("propeller", &self.name);
        c.range("diameter", self.diameter_in, 0.5, 100.0, "in");
        c.range("pitch", self.pitch_in, 0.1, 100.0, "in");
        c.int_range("blades", self.blades, 1, 8);
        c.range("mass", self.mass_g, 0.1, 2000.0, "g");
        c.opt_range("Ct", self.ct, 0.0, 2.0, "");
        c.opt_range("Cp", self.cp, 0.0, 2.0, "");
        c.errors
    }
}

impl Record for Esc {
    fn notes(&self) -> &str {
        &self.notes
    }
    fn summary(&self) -> String {
        format!(
            "{} A | {}-{}S | {} g",
            self.continuous_a, self.cells_min, self.cells_max, self.mass_g
        )
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn source(&self) -> &Source {
        &self.source
    }
    fn set_source(&mut self, source: Source) {
        self.source = source;
    }
    fn validate(&self) -> Vec<String> {
        let mut c = Check::new("ESC", &self.name);
        c.range("continuous current", self.continuous_a, 0.5, 500.0, "A");
        c.opt_range("burst current", self.burst_a, 0.5, 1000.0, "A");
        if let Some(b) = self.burst_a {
            if b.is_finite() && self.continuous_a.is_finite() && b < self.continuous_a {
                c.errors
                    .push("ESC: burst current is lower than the continuous current".into());
            }
        }
        c.int_range("minimum cells", self.cells_min, 1, 24);
        c.int_range("maximum cells", self.cells_max, 1, 24);
        if self.cells_min > self.cells_max {
            c.errors
                .push("ESC: minimum cells is greater than maximum cells".into());
        }
        c.range("mass", self.mass_g, 0.1, 2000.0, "g");
        c.errors
    }
}

impl Record for Battery {
    fn notes(&self) -> &str {
        &self.notes
    }
    fn summary(&self) -> String {
        format!(
            "{} {}S{}P | {} mAh | {}C | {} g",
            self.chemistry.label(),
            self.cells_s,
            self.parallel_p,
            self.capacity_mah,
            self.c_rating,
            self.mass_g
        )
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn source(&self) -> &Source {
        &self.source
    }
    fn set_source(&mut self, source: Source) {
        self.source = source;
    }
    fn validate(&self) -> Vec<String> {
        let mut c = Check::new("battery", &self.name);
        c.int_range("series cells", self.cells_s, 1, 24);
        c.int_range("parallel cells", self.parallel_p, 1, 16);
        c.range("capacity", self.capacity_mah, 10.0, 200_000.0, "mAh");
        c.range("C rating", self.c_rating, 0.1, 500.0, "C");
        c.opt_range(
            "internal resistance",
            self.r_internal_mohm,
            0.01,
            2000.0,
            "mOhm",
        );
        c.range("mass", self.mass_g, 1.0, 50_000.0, "g");
        c.errors
    }
}

impl Battery {
    /// Nominal pack voltage in volts.
    pub fn nominal_v(&self) -> f64 {
        self.cells_s as f64 * self.chemistry.nominal_cell_v()
    }

    /// Continuous current implied by the C rating, in amperes.
    pub fn max_cont_a(&self) -> f64 {
        self.c_rating * self.capacity_mah / 1000.0
    }
}
