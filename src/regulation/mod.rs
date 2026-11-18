// SPDX-License-Identifier: AGPL-3.0-or-later
//! Regulatory class check (EASA, FAA, Turkey SHGM). Informational screening only: it compares
//! the design's mass, estimated maximum speed, dimension and impact energy against the rule
//! table in [`rules`]. It is not legal advice and the rules change.
pub mod rules;

use crate::calc::Model;
use crate::project::{DroneType, Project};
use rules::{GROUPS, Group, Tier, tiers_of};

/// Standard gravity (m/s^2), used for the free-fall energy reference.
pub const G: f64 = 9.806_65;

/// Kinetic energy 1/2 m v^2 in joules (mass in kg, speed in m/s).
pub fn kinetic_energy_j(mass_kg: f64, speed_ms: f64) -> f64 {
    0.5 * mass_kg * speed_ms * speed_ms
}

/// Where the speed used in the checks comes from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpeedSource {
    /// The user typed a maximum speed.
    UserOverride,
    /// Fixed wing / VTOL: the fastest of the design cruise, best-endurance and best-range speeds.
    /// The program has no top-speed model, so the real maximum is higher.
    FixedWingDesign,
    /// Multirotor: the fastest of the forward-flight cruise and best-range speeds.
    MultirotorDesign,
}

impl SpeedSource {
    pub fn key(self) -> &'static str {
        match self {
            SpeedSource::UserOverride => "regulation.speed_src.user",
            SpeedSource::FixedWingDesign => "regulation.speed_src.fixed_wing",
            SpeedSource::MultirotorDesign => "regulation.speed_src.multirotor",
        }
    }
}

/// The design numbers the rules are evaluated against.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DesignFacts {
    pub mtow_kg: f64,
    pub v_max_ms: f64,
    pub speed_source: SpeedSource,
    /// Largest characteristic dimension (wingspan, length or rotor tip-to-tip span), metres.
    pub dimension_m: f64,
    /// 1/2 m v^2 at `v_max_ms`.
    pub energy_j: f64,
    /// 1/2 m v^2 at the 19 m/s speed cap of EASA classes C0 / C1 (worst case allowed there).
    pub energy_at_class_cap_j: f64,
}

impl DesignFacts {
    /// Builds the facts from raw numbers (also used by the tests).
    pub fn new(
        mtow_kg: f64,
        v_max_ms: f64,
        speed_source: SpeedSource,
        dimension_m: f64,
    ) -> Option<DesignFacts> {
        let ok = |x: f64| x.is_finite() && x > 0.0;
        if !ok(mtow_kg) || !(v_max_ms.is_finite() && v_max_ms >= 0.0) || !ok(dimension_m) {
            return None;
        }
        Some(DesignFacts {
            mtow_kg,
            v_max_ms,
            speed_source,
            dimension_m,
            energy_j: kinetic_energy_j(mtow_kg, v_max_ms),
            energy_at_class_cap_j: kinetic_energy_j(mtow_kg, rules::EASA_C01_SPEED_MS),
        })
    }

    /// Collects MTOW, best-estimate maximum speed and dimension from the project and model.
    /// `speed_override` (m/s, > 0) replaces the estimate. `None` when the model has no valid mass.
    pub fn from_model(p: &Project, m: &Model, speed_override: Option<f64>) -> Option<DesignFacts> {
        let multirotor = p.drone_type == DroneType::Multirotor;
        let mtow = if multirotor {
            m.mr.m_mtow
        } else {
            m.weight.m_mtow
        };
        let best = |v: &[f64]| {
            v.iter()
                .copied()
                .filter(|x| x.is_finite() && *x > 0.0)
                .fold(0.0_f64, f64::max)
        };
        let (est, src) = if multirotor {
            (
                best(&[p.mr_v_cruise, m.mr.v_best_range]),
                SpeedSource::MultirotorDesign,
            )
        } else {
            (
                best(&[p.v_cr, m.perf_cr.v, m.perf_end.v, m.perf_range.v]),
                SpeedSource::FixedWingDesign,
            )
        };
        let (v, src) = match speed_override {
            Some(o) if o.is_finite() && o > 0.0 => (o, SpeedSource::UserOverride),
            _ => (est, src),
        };
        let mut dim = 0.0_f64;
        if p.drone_type.has_wing() {
            dim = dim.max(best(&[p.b, p.l_fuse]));
        }
        if p.drone_type.is_multirotor() {
            // Motor-to-motor diameter plus one propeller diameter approximates tip-to-tip span.
            dim = dim.max(best(&[p.mr_frame_size]) + best(&[p.mr_d_prop]));
        }
        DesignFacts::new(mtow, v, src, dim)
    }
}

/// Which numeric criterion a failure refers to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Criterion {
    Mass,
    Speed,
    Energy,
    Dimension,
}

impl Criterion {
    pub fn key(self) -> &'static str {
        match self {
            Criterion::Mass => "regulation.crit.mass",
            Criterion::Speed => "regulation.crit.speed",
            Criterion::Energy => "regulation.crit.energy",
            Criterion::Dimension => "regulation.crit.dimension",
        }
    }
}

/// One failed criterion of a tier. Values use the base units of the table (kg, m/s, J, m).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Failure {
    pub criterion: Criterion,
    pub value: f64,
    pub limit: f64,
    /// `true`: the value is above a maximum; `false`: below a minimum.
    pub above_max: bool,
}

#[derive(Clone, Debug)]
pub struct TierResult {
    pub tier: &'static Tier,
    pub passed: bool,
    pub failures: Vec<Failure>,
}

/// Evaluates one tier against the design facts.
pub fn check_tier(tier: &'static Tier, f: &DesignFacts) -> TierResult {
    let mut failures = Vec::new();
    if f.mtow_kg < tier.mass_ge_kg {
        failures.push(Failure {
            criterion: Criterion::Mass,
            value: f.mtow_kg,
            limit: tier.mass_ge_kg,
            above_max: false,
        });
    }
    if let Some(lt) = tier.mass_lt_kg {
        let waived = tier.mass_waiver_energy_j.is_some_and(|e| f.energy_j <= e);
        if f.mtow_kg >= lt && !waived {
            failures.push(Failure {
                criterion: Criterion::Mass,
                value: f.mtow_kg,
                limit: lt,
                above_max: true,
            });
            if let Some(e) = tier.mass_waiver_energy_j {
                failures.push(Failure {
                    criterion: Criterion::Energy,
                    value: f.energy_j,
                    limit: e,
                    above_max: true,
                });
            }
        }
    }
    if let Some(v) = tier.speed_max_ms {
        if f.v_max_ms > v {
            failures.push(Failure {
                criterion: Criterion::Speed,
                value: f.v_max_ms,
                limit: v,
                above_max: true,
            });
        }
    }
    if let Some(e) = tier.energy_max_j {
        if f.energy_j > e {
            failures.push(Failure {
                criterion: Criterion::Energy,
                value: f.energy_j,
                limit: e,
                above_max: true,
            });
        }
    }
    if let Some(d) = tier.dim_max_m {
        if f.dimension_m > d {
            failures.push(Failure {
                criterion: Criterion::Dimension,
                value: f.dimension_m,
                limit: d,
                above_max: true,
            });
        }
    }
    TierResult {
        tier,
        passed: failures.is_empty(),
        failures,
    }
}

/// Result for one group: every tier's verdict plus the first tier that passes.
#[derive(Clone, Debug)]
pub struct GroupResult {
    pub group: &'static Group,
    pub tiers: Vec<TierResult>,
    /// Index into `tiers` of the first passing tier.
    pub matched: Option<usize>,
}

impl GroupResult {
    pub fn matched_tier(&self) -> Option<&'static Tier> {
        self.matched.map(|i| self.tiers[i].tier)
    }

    /// Mass headroom (kg) under the matched tier's upper mass bound, if it has one.
    pub fn mass_headroom_kg(&self, f: &DesignFacts) -> Option<f64> {
        self.matched_tier()?.mass_lt_kg.map(|lt| lt - f.mtow_kg)
    }

    /// The tier that comes after the matched one (what the design falls into when it grows).
    pub fn next_tier(&self) -> Option<&'static Tier> {
        let i = self.matched? + 1;
        self.tiers.get(i).map(|r| r.tier)
    }

    /// Tiers listed before the matched one (or all, when none matches) with what blocks them.
    pub fn blocked_before_match(&self) -> impl Iterator<Item = &TierResult> {
        let end = self.matched.unwrap_or(self.tiers.len());
        self.tiers[..end].iter()
    }
}

pub fn check_group(group: &'static Group, f: &DesignFacts) -> GroupResult {
    let tiers: Vec<TierResult> = tiers_of(group.id).map(|t| check_tier(t, f)).collect();
    let matched = tiers.iter().position(|r| r.passed);
    GroupResult {
        group,
        tiers,
        matched,
    }
}

/// Evaluates every group of the table.
pub fn evaluate_all(f: &DesignFacts) -> Vec<GroupResult> {
    GROUPS.iter().map(|g| check_group(g, f)).collect()
}

#[cfg(test)]
mod tests;
