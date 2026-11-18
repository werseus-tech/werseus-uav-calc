// SPDX-License-Identifier: AGPL-3.0-or-later
//! The regulatory rule table. EVERY threshold, requirement text key, source document name and
//! "last checked" date lives in this one file so it can be updated in one place.
//!
//! Conventions:
//! - Masses are kilograms, speeds m/s, energies joules, dimensions metres.
//! - `mass_lt_kg` is an EXCLUSIVE upper bound ("less than 250 g"); `mass_ge_kg` is inclusive.
//! - `verify: true` marks a value or text the author could not confirm against the current
//!   official publication; the UI shows it as "verify" instead of presenting it as settled.
//! - Texts are i18n keys (`regulation.*`) present in both language files.
//! - The data is informational. It is not legal advice and the rules change.

/// Date the whole table was last reviewed (ISO 8601). Update it whenever any rule is re-checked.
pub const LAST_CHECKED: &str = "2026-11-18";

/// 1 lb in kg (international avoirdupois pound).
pub const LB_KG: f64 = 0.453_592_37;
/// 0.55 lb: FAA registration / Part 107 Category 1 weight limit (about 249.5 g).
pub const FAA_055_LB_KG: f64 = 0.55 * LB_KG;
/// 55 lb: upper limit of a "small unmanned aircraft" in 14 CFR Part 107 (about 24.95 kg).
pub const FAA_55_LB_KG: f64 = 55.0 * LB_KG;
/// 87 knots (100 mph) ground speed limit of 14 CFR 107.51 in m/s.
pub const FAA_87_KT_MS: f64 = 87.0 * 1852.0 / 3600.0;
/// 11 ft-lbf of transferred kinetic energy (14 CFR 107.115 injury threshold) in joules.
pub const FAA_11_FTLBF_J: f64 = 11.0 * 1.355_817_948_331_4;
/// Maximum speed of EASA class C0 / C1 (Delegated Regulation (EU) 2019/945).
pub const EASA_C01_SPEED_MS: f64 = 19.0;
/// Impact-energy criterion of EASA class C1 (alternative to the 900 g limit).
pub const EASA_C1_ENERGY_J: f64 = 80.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Authority {
    Easa,
    Faa,
    Shgm,
}

impl Authority {
    pub const ALL: [Authority; 3] = [Authority::Easa, Authority::Faa, Authority::Shgm];

    pub fn name_key(self) -> &'static str {
        match self {
            Authority::Easa => "regulation.auth.easa",
            Authority::Faa => "regulation.auth.faa",
            Authority::Shgm => "regulation.auth.shgm",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ReqKind {
    Registration,
    RemoteId,
    GeoAwareness,
    PilotCompetency,
    Lights,
    Operations,
}

impl ReqKind {
    pub fn label_key(self) -> &'static str {
        match self {
            ReqKind::Registration => "regulation.kind.registration",
            ReqKind::RemoteId => "regulation.kind.remote_id",
            ReqKind::GeoAwareness => "regulation.kind.geo_awareness",
            ReqKind::PilotCompetency => "regulation.kind.pilot",
            ReqKind::Lights => "regulation.kind.lights",
            ReqKind::Operations => "regulation.kind.operations",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ReqLevel {
    Required,
    NotRequired,
    Conditional,
    /// Informational limits (operating conditions), not a yes/no obligation.
    Info,
}

impl ReqLevel {
    pub fn label_key(self) -> &'static str {
        match self {
            ReqLevel::Required => "regulation.level.required",
            ReqLevel::NotRequired => "regulation.level.not_required",
            ReqLevel::Conditional => "regulation.level.conditional",
            ReqLevel::Info => "regulation.level.info",
        }
    }
}

/// One requirement attached to a tier.
#[derive(Clone, Copy, Debug)]
pub struct Req {
    pub kind: ReqKind,
    pub level: ReqLevel,
    pub text_key: &'static str,
    pub verify: bool,
}

const fn req(kind: ReqKind, level: ReqLevel, text_key: &'static str, verify: bool) -> Req {
    Req {
        kind,
        level,
        text_key,
        verify,
    }
}

/// A group is one independent question answered per authority ("which class mark?",
/// "which registration band?"). Its tiers are tried in table order; the first that passes wins.
#[derive(Clone, Copy, Debug)]
pub struct Group {
    pub id: &'static str,
    pub authority: Authority,
    pub title_key: &'static str,
    /// Shown when no tier of the group passes.
    pub none_key: &'static str,
}

/// One category / class / band with its numeric criteria and requirements.
#[derive(Clone, Copy, Debug)]
pub struct Tier {
    pub id: &'static str,
    pub group: &'static str,
    pub name_key: &'static str,
    pub desc_key: &'static str,
    /// Inclusive lower mass bound (kg).
    pub mass_ge_kg: f64,
    /// Exclusive upper mass bound (kg).
    pub mass_lt_kg: Option<f64>,
    /// If set, the mass upper bound is waived when the impact energy is at most this (J).
    pub mass_waiver_energy_j: Option<f64>,
    pub speed_max_ms: Option<f64>,
    pub energy_max_j: Option<f64>,
    pub dim_max_m: Option<f64>,
    pub reqs: &'static [Req],
    /// Official document the numbers and texts come from (proper names, not translated).
    pub source: &'static str,
    /// Date this tier was last checked (ISO 8601).
    pub checked: &'static str,
    /// True when any threshold of this tier is unconfirmed.
    pub verify: bool,
}

use ReqKind as K;
use ReqLevel as L;

const SRC_EASA_945: &str =
    "Commission Delegated Regulation (EU) 2019/945 (as amended by (EU) 2020/1058), Annex";
const SRC_EASA_947: &str = "Commission Implementing Regulation (EU) 2019/947 (as amended), Art. 4, 14, 20 and Annex UAS.OPEN";
const SRC_FAA_107: &str = "14 CFR Part 107 (Small Unmanned Aircraft Systems), incl. Subpart D";
const SRC_FAA_REG: &str =
    "14 CFR Part 48 (registration), Part 89 (Remote Identification) and 49 U.S.C. 44809";
const SRC_SHGM: &str = "SHGM, Insansiz Hava Araci Sistemleri Talimati (SHT-IHA)";

// ---------------------------------------------------------------------------------------------
// Groups
// ---------------------------------------------------------------------------------------------

pub static GROUPS: &[Group] = &[
    Group {
        id: "easa_class",
        authority: Authority::Easa,
        title_key: "regulation.group.easa_class",
        none_key: "regulation.group.easa_class.none",
    },
    Group {
        id: "easa_unmarked",
        authority: Authority::Easa,
        title_key: "regulation.group.easa_unmarked",
        none_key: "regulation.group.easa_unmarked.none",
    },
    Group {
        id: "faa_reg",
        authority: Authority::Faa,
        title_key: "regulation.group.faa_reg",
        none_key: "regulation.group.faa_reg.none",
    },
    Group {
        id: "faa_part107",
        authority: Authority::Faa,
        title_key: "regulation.group.faa_part107",
        none_key: "regulation.group.faa_part107.none",
    },
    Group {
        id: "faa_people",
        authority: Authority::Faa,
        title_key: "regulation.group.faa_people",
        none_key: "regulation.group.faa_people.none",
    },
    Group {
        id: "shgm_class",
        authority: Authority::Shgm,
        title_key: "regulation.group.shgm_class",
        none_key: "regulation.group.shgm_class.none",
    },
];

// ---------------------------------------------------------------------------------------------
// Requirement sets
// ---------------------------------------------------------------------------------------------

static REQ_EASA_C0: &[Req] = &[
    req(
        K::Registration,
        L::Conditional,
        "regulation.req.easa_reg_c0",
        false,
    ),
    req(
        K::RemoteId,
        L::NotRequired,
        "regulation.req.easa_rid_none",
        true,
    ),
    req(
        K::GeoAwareness,
        L::NotRequired,
        "regulation.req.easa_geo_none",
        true,
    ),
    req(
        K::PilotCompetency,
        L::NotRequired,
        "regulation.req.easa_pilot_c0",
        true,
    ),
    req(
        K::Lights,
        L::NotRequired,
        "regulation.req.easa_light_none",
        true,
    ),
    req(
        K::Operations,
        L::Info,
        "regulation.req.easa_ops_a1_c0",
        false,
    ),
];

static REQ_EASA_C1: &[Req] = &[
    req(
        K::Registration,
        L::Required,
        "regulation.req.easa_reg",
        false,
    ),
    req(
        K::RemoteId,
        L::Required,
        "regulation.req.easa_rid_yes",
        false,
    ),
    req(
        K::GeoAwareness,
        L::Required,
        "regulation.req.easa_geo_yes",
        false,
    ),
    req(
        K::PilotCompetency,
        L::Required,
        "regulation.req.easa_pilot_a1a3",
        false,
    ),
    req(
        K::Lights,
        L::Required,
        "regulation.req.easa_light_green",
        true,
    ),
    req(
        K::Operations,
        L::Info,
        "regulation.req.easa_ops_a1_c1",
        false,
    ),
];

static REQ_EASA_C2: &[Req] = &[
    req(
        K::Registration,
        L::Required,
        "regulation.req.easa_reg",
        false,
    ),
    req(
        K::RemoteId,
        L::Required,
        "regulation.req.easa_rid_yes",
        false,
    ),
    req(
        K::GeoAwareness,
        L::Required,
        "regulation.req.easa_geo_yes",
        false,
    ),
    req(
        K::PilotCompetency,
        L::Required,
        "regulation.req.easa_pilot_a2",
        false,
    ),
    req(
        K::Lights,
        L::Required,
        "regulation.req.easa_light_green",
        true,
    ),
    req(K::Operations, L::Info, "regulation.req.easa_ops_a2", false),
];

static REQ_EASA_C3: &[Req] = &[
    req(
        K::Registration,
        L::Required,
        "regulation.req.easa_reg",
        false,
    ),
    req(
        K::RemoteId,
        L::Required,
        "regulation.req.easa_rid_yes",
        false,
    ),
    req(
        K::GeoAwareness,
        L::Required,
        "regulation.req.easa_geo_yes",
        false,
    ),
    req(
        K::PilotCompetency,
        L::Required,
        "regulation.req.easa_pilot_a1a3",
        false,
    ),
    req(
        K::Lights,
        L::Required,
        "regulation.req.easa_light_green",
        true,
    ),
    req(K::Operations, L::Info, "regulation.req.easa_ops_a3", false),
];

static REQ_EASA_C4: &[Req] = &[
    req(
        K::Registration,
        L::Required,
        "regulation.req.easa_reg",
        false,
    ),
    req(
        K::RemoteId,
        L::NotRequired,
        "regulation.req.easa_rid_none",
        true,
    ),
    req(
        K::GeoAwareness,
        L::NotRequired,
        "regulation.req.easa_geo_none",
        true,
    ),
    req(
        K::PilotCompetency,
        L::Required,
        "regulation.req.easa_pilot_a1a3",
        false,
    ),
    req(
        K::Lights,
        L::NotRequired,
        "regulation.req.easa_light_none",
        true,
    ),
    req(
        K::Operations,
        L::Info,
        "regulation.req.easa_ops_a3_c4",
        false,
    ),
];

static REQ_EASA_SPECIFIC: &[Req] = &[
    req(
        K::Registration,
        L::Required,
        "regulation.req.easa_reg",
        false,
    ),
    req(
        K::RemoteId,
        L::Conditional,
        "regulation.req.easa_spec_rid",
        true,
    ),
    req(
        K::PilotCompetency,
        L::Required,
        "regulation.req.easa_spec_pilot",
        false,
    ),
    req(
        K::Operations,
        L::Info,
        "regulation.req.easa_spec_ops",
        false,
    ),
];

static REQ_EASA_UNMARKED_A1: &[Req] = &[
    req(
        K::Registration,
        L::Conditional,
        "regulation.req.easa_reg_c0",
        true,
    ),
    req(
        K::RemoteId,
        L::Conditional,
        "regulation.req.easa_rid_unmarked",
        true,
    ),
    req(
        K::PilotCompetency,
        L::Conditional,
        "regulation.req.easa_pilot_unmarked_a1",
        true,
    ),
    req(
        K::Operations,
        L::Info,
        "regulation.req.easa_ops_unmarked_a1",
        true,
    ),
];

static REQ_EASA_UNMARKED_A3: &[Req] = &[
    req(
        K::Registration,
        L::Required,
        "regulation.req.easa_reg",
        false,
    ),
    req(
        K::RemoteId,
        L::Conditional,
        "regulation.req.easa_rid_unmarked",
        true,
    ),
    req(
        K::PilotCompetency,
        L::Required,
        "regulation.req.easa_pilot_a1a3",
        false,
    ),
    req(K::Operations, L::Info, "regulation.req.easa_ops_a3", false),
];

static REQ_FAA_UNDER: &[Req] = &[
    req(
        K::Registration,
        L::Conditional,
        "regulation.req.faa_reg_none",
        false,
    ),
    req(
        K::RemoteId,
        L::NotRequired,
        "regulation.req.faa_rid_none",
        false,
    ),
    req(
        K::PilotCompetency,
        L::Required,
        "regulation.req.faa_pilot_both",
        false,
    ),
    req(K::Lights, L::Conditional, "regulation.req.faa_light", false),
    req(K::Operations, L::Info, "regulation.req.faa_ops", false),
];

static REQ_FAA_MID: &[Req] = &[
    req(
        K::Registration,
        L::Required,
        "regulation.req.faa_reg_yes",
        false,
    ),
    req(
        K::RemoteId,
        L::Required,
        "regulation.req.faa_rid_yes",
        false,
    ),
    req(
        K::PilotCompetency,
        L::Required,
        "regulation.req.faa_pilot_both",
        false,
    ),
    req(K::Lights, L::Conditional, "regulation.req.faa_light", false),
    req(K::Operations, L::Info, "regulation.req.faa_ops", false),
];

static REQ_FAA_OVER: &[Req] = &[
    req(
        K::Registration,
        L::Required,
        "regulation.req.faa_reg_over",
        true,
    ),
    req(
        K::RemoteId,
        L::Conditional,
        "regulation.req.faa_rid_over",
        true,
    ),
    req(
        K::PilotCompetency,
        L::Required,
        "regulation.req.faa_pilot_over",
        true,
    ),
    req(K::Operations, L::Info, "regulation.req.faa_ops_over", true),
];

static REQ_FAA_P107: &[Req] = &[
    req(
        K::Registration,
        L::Required,
        "regulation.req.faa_reg_107",
        false,
    ),
    req(
        K::PilotCompetency,
        L::Required,
        "regulation.req.faa_pilot_107",
        false,
    ),
    req(K::Operations, L::Info, "regulation.req.faa_ops_107", false),
];

static REQ_FAA_CAT1: &[Req] = &[req(
    K::Operations,
    L::Info,
    "regulation.req.faa_people_cat1",
    true,
)];

static REQ_FAA_CAT23: &[Req] = &[req(
    K::Operations,
    L::Info,
    "regulation.req.faa_people_cat23",
    true,
)];

static REQ_SHGM_0: &[Req] = &[
    req(
        K::Registration,
        L::Conditional,
        "regulation.req.shgm_reg_0",
        true,
    ),
    req(K::RemoteId, L::NotRequired, "regulation.req.shgm_rid", true),
    req(
        K::PilotCompetency,
        L::Conditional,
        "regulation.req.shgm_pilot_0",
        true,
    ),
    req(K::Operations, L::Info, "regulation.req.shgm_ops", true),
];

static REQ_SHGM_1: &[Req] = &[
    req(
        K::Registration,
        L::Required,
        "regulation.req.shgm_reg",
        true,
    ),
    req(K::RemoteId, L::NotRequired, "regulation.req.shgm_rid", true),
    req(
        K::PilotCompetency,
        L::Required,
        "regulation.req.shgm_pilot_1",
        true,
    ),
    req(K::Operations, L::Info, "regulation.req.shgm_ops", true),
];

static REQ_SHGM_2: &[Req] = &[
    req(
        K::Registration,
        L::Required,
        "regulation.req.shgm_reg",
        true,
    ),
    req(K::RemoteId, L::NotRequired, "regulation.req.shgm_rid", true),
    req(
        K::PilotCompetency,
        L::Required,
        "regulation.req.shgm_pilot_2",
        true,
    ),
    req(K::Operations, L::Info, "regulation.req.shgm_ops", true),
];

static REQ_SHGM_3: &[Req] = &[
    req(
        K::Registration,
        L::Required,
        "regulation.req.shgm_reg",
        true,
    ),
    req(
        K::PilotCompetency,
        L::Required,
        "regulation.req.shgm_pilot_3",
        true,
    ),
    req(K::Operations, L::Info, "regulation.req.shgm_ops_3", true),
];

// ---------------------------------------------------------------------------------------------
// Tiers (in evaluation order within each group)
// ---------------------------------------------------------------------------------------------

const fn tier(
    id: &'static str,
    group: &'static str,
    name_key: &'static str,
    desc_key: &'static str,
    reqs: &'static [Req],
    source: &'static str,
    verify: bool,
) -> Tier {
    Tier {
        id,
        group,
        name_key,
        desc_key,
        mass_ge_kg: 0.0,
        mass_lt_kg: None,
        mass_waiver_energy_j: None,
        speed_max_ms: None,
        energy_max_j: None,
        dim_max_m: None,
        reqs,
        source,
        checked: LAST_CHECKED,
        verify,
    }
}

pub static TIERS: &[Tier] = &[
    // --- EASA open category, class marks -----------------------------------------------------
    Tier {
        mass_lt_kg: Some(0.25),
        speed_max_ms: Some(EASA_C01_SPEED_MS),
        // Characteristic dimension limit of C0 is unconfirmed.
        dim_max_m: Some(1.0),
        ..tier(
            "easa_c0",
            "easa_class",
            "regulation.tier.easa_c0",
            "regulation.tier.easa_c0.desc",
            REQ_EASA_C0,
            SRC_EASA_945,
            true,
        )
    },
    Tier {
        mass_lt_kg: Some(0.9),
        // The 80 J alternative to the 900 g limit: exact wording is unconfirmed.
        mass_waiver_energy_j: Some(EASA_C1_ENERGY_J),
        speed_max_ms: Some(EASA_C01_SPEED_MS),
        ..tier(
            "easa_c1",
            "easa_class",
            "regulation.tier.easa_c1",
            "regulation.tier.easa_c1.desc",
            REQ_EASA_C1,
            SRC_EASA_945,
            true,
        )
    },
    Tier {
        mass_lt_kg: Some(4.0),
        dim_max_m: Some(3.0),
        ..tier(
            "easa_c2",
            "easa_class",
            "regulation.tier.easa_c2",
            "regulation.tier.easa_c2.desc",
            REQ_EASA_C2,
            SRC_EASA_945,
            false,
        )
    },
    Tier {
        mass_lt_kg: Some(25.0),
        dim_max_m: Some(3.0),
        ..tier(
            "easa_c3",
            "easa_class",
            "regulation.tier.easa_c3",
            "regulation.tier.easa_c3.desc",
            REQ_EASA_C3,
            SRC_EASA_945,
            true,
        )
    },
    Tier {
        mass_lt_kg: Some(25.0),
        ..tier(
            "easa_c4",
            "easa_class",
            "regulation.tier.easa_c4",
            "regulation.tier.easa_c4.desc",
            REQ_EASA_C4,
            SRC_EASA_945,
            true,
        )
    },
    Tier {
        mass_ge_kg: 25.0,
        ..tier(
            "easa_specific",
            "easa_class",
            "regulation.tier.easa_specific",
            "regulation.tier.easa_specific.desc",
            REQ_EASA_SPECIFIC,
            SRC_EASA_947,
            false,
        )
    },
    // --- EASA open category, privately built / no class mark ---------------------------------
    Tier {
        mass_lt_kg: Some(0.25),
        ..tier(
            "easa_u_a1",
            "easa_unmarked",
            "regulation.tier.easa_u_a1",
            "regulation.tier.easa_u_a1.desc",
            REQ_EASA_UNMARKED_A1,
            SRC_EASA_947,
            true,
        )
    },
    Tier {
        mass_lt_kg: Some(25.0),
        ..tier(
            "easa_u_a3",
            "easa_unmarked",
            "regulation.tier.easa_u_a3",
            "regulation.tier.easa_u_a3.desc",
            REQ_EASA_UNMARKED_A3,
            SRC_EASA_947,
            true,
        )
    },
    Tier {
        mass_ge_kg: 25.0,
        ..tier(
            "easa_u_specific",
            "easa_unmarked",
            "regulation.tier.easa_specific",
            "regulation.tier.easa_specific.desc",
            REQ_EASA_SPECIFIC,
            SRC_EASA_947,
            false,
        )
    },
    // --- FAA: registration / Remote ID band ---------------------------------------------------
    Tier {
        mass_lt_kg: Some(FAA_055_LB_KG),
        ..tier(
            "faa_under",
            "faa_reg",
            "regulation.tier.faa_under",
            "regulation.tier.faa_under.desc",
            REQ_FAA_UNDER,
            SRC_FAA_REG,
            false,
        )
    },
    Tier {
        mass_ge_kg: FAA_055_LB_KG,
        mass_lt_kg: Some(FAA_55_LB_KG),
        ..tier(
            "faa_mid",
            "faa_reg",
            "regulation.tier.faa_mid",
            "regulation.tier.faa_mid.desc",
            REQ_FAA_MID,
            SRC_FAA_REG,
            false,
        )
    },
    Tier {
        mass_ge_kg: FAA_55_LB_KG,
        ..tier(
            "faa_over",
            "faa_reg",
            "regulation.tier.faa_over",
            "regulation.tier.faa_over.desc",
            REQ_FAA_OVER,
            SRC_FAA_REG,
            true,
        )
    },
    // --- FAA: Part 107 eligibility --------------------------------------------------------------
    Tier {
        mass_lt_kg: Some(FAA_55_LB_KG),
        speed_max_ms: Some(FAA_87_KT_MS),
        ..tier(
            "faa_p107",
            "faa_part107",
            "regulation.tier.faa_p107",
            "regulation.tier.faa_p107.desc",
            REQ_FAA_P107,
            SRC_FAA_107,
            false,
        )
    },
    // --- FAA: operations over people (Part 107 Subpart D), screening only ------------------------
    Tier {
        mass_lt_kg: Some(FAA_055_LB_KG),
        ..tier(
            "faa_cat1",
            "faa_people",
            "regulation.tier.faa_cat1",
            "regulation.tier.faa_cat1.desc",
            REQ_FAA_CAT1,
            SRC_FAA_107,
            true,
        )
    },
    Tier {
        energy_max_j: Some(FAA_11_FTLBF_J),
        mass_lt_kg: Some(FAA_55_LB_KG),
        ..tier(
            "faa_cat23",
            "faa_people",
            "regulation.tier.faa_cat23",
            "regulation.tier.faa_cat23.desc",
            REQ_FAA_CAT23,
            SRC_FAA_107,
            true,
        )
    },
    // --- Turkey SHGM: every threshold here is unconfirmed (verify) --------------------------------
    Tier {
        mass_lt_kg: Some(0.5),
        ..tier(
            "shgm_0",
            "shgm_class",
            "regulation.tier.shgm_0",
            "regulation.tier.shgm_0.desc",
            REQ_SHGM_0,
            SRC_SHGM,
            true,
        )
    },
    Tier {
        mass_ge_kg: 0.5,
        mass_lt_kg: Some(4.0),
        ..tier(
            "shgm_1",
            "shgm_class",
            "regulation.tier.shgm_1",
            "regulation.tier.shgm_1.desc",
            REQ_SHGM_1,
            SRC_SHGM,
            true,
        )
    },
    Tier {
        mass_ge_kg: 4.0,
        mass_lt_kg: Some(25.0),
        ..tier(
            "shgm_2",
            "shgm_class",
            "regulation.tier.shgm_2",
            "regulation.tier.shgm_2.desc",
            REQ_SHGM_2,
            SRC_SHGM,
            true,
        )
    },
    Tier {
        mass_ge_kg: 25.0,
        ..tier(
            "shgm_3",
            "shgm_class",
            "regulation.tier.shgm_3",
            "regulation.tier.shgm_3.desc",
            REQ_SHGM_3,
            SRC_SHGM,
            true,
        )
    },
];

/// The tiers of one group, in evaluation order.
pub fn tiers_of(group_id: &str) -> impl Iterator<Item = &'static Tier> + '_ {
    TIERS.iter().filter(move |t| t.group == group_id)
}

/// Every i18n key referenced by the table (used by the integrity test).
pub fn all_keys() -> Vec<&'static str> {
    let mut keys = Vec::new();
    for a in Authority::ALL {
        keys.push(a.name_key());
    }
    for g in GROUPS {
        keys.push(g.title_key);
        keys.push(g.none_key);
    }
    for t in TIERS {
        keys.push(t.name_key);
        keys.push(t.desc_key);
        for r in t.reqs {
            keys.push(r.text_key);
            keys.push(r.kind.label_key());
            keys.push(r.level.label_key());
        }
    }
    keys.sort_unstable();
    keys.dedup();
    keys
}
