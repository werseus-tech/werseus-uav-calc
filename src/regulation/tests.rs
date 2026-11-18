// SPDX-License-Identifier: AGPL-3.0-or-later
use super::rules::{self, GROUPS, TIERS};
use super::*;
use crate::calc::{Library, evaluate};
use crate::project::{self, DroneType, Project};

fn facts(mass_kg: f64, speed: f64, dim: f64) -> DesignFacts {
    DesignFacts::new(mass_kg, speed, SpeedSource::UserOverride, dim).unwrap()
}

fn matched_id(group: &str, f: &DesignFacts) -> Option<&'static str> {
    let g = GROUPS.iter().find(|g| g.id == group).unwrap();
    check_group(g, f).matched_tier().map(|t| t.id)
}

#[test]
fn kinetic_energy_formula() {
    assert!((kinetic_energy_j(2.0, 10.0) - 100.0).abs() < 1e-12);
    assert!((kinetic_energy_j(0.25, 19.0) - 45.125).abs() < 1e-9);
    assert_eq!(kinetic_energy_j(1.0, 0.0), 0.0);
    let f = facts(0.5, 12.0, 0.4);
    assert!((f.energy_j - 36.0).abs() < 1e-12);
    assert!((f.energy_at_class_cap_j - 0.5 * 0.5 * 361.0).abs() < 1e-12);
}

#[test]
fn easa_class_mass_boundaries() {
    // Slow and small so only mass decides.
    let id = |m: f64| matched_id("easa_class", &facts(m, 10.0, 0.5)).unwrap();
    assert_eq!(id(0.249), "easa_c0");
    assert_eq!(id(0.250), "easa_c1");
    assert_eq!(id(0.251), "easa_c1");
    assert_eq!(id(0.899), "easa_c1");
    // 900 g with 10 m/s is 45 J: the 80 J alternative still keeps it in C1.
    assert_eq!(id(0.900), "easa_c1");
    assert_eq!(id(0.901), "easa_c1");
    // At speed the energy exceeds 80 J, so the mass limit applies.
    let fast = |m: f64| matched_id("easa_class", &facts(m, 18.0, 0.5)).unwrap();
    assert_eq!(fast(0.899), "easa_c1");
    assert_eq!(fast(0.900), "easa_c2");
    assert_eq!(fast(0.901), "easa_c2");
    assert_eq!(id(3.999), "easa_c2");
    assert_eq!(id(4.0), "easa_c3");
    assert_eq!(id(24.999), "easa_c3");
    assert_eq!(id(25.0), "easa_specific");
}

#[test]
fn easa_speed_and_dimension_limits() {
    // 200 g but 25 m/s: not C0/C1 (19 m/s cap), falls to C2.
    assert_eq!(
        matched_id("easa_class", &facts(0.2, 25.0, 0.3)),
        Some("easa_c2")
    );
    // 3.5 kg with a 3.5 m span: too long for C2/C3, only C4 remains.
    assert_eq!(
        matched_id("easa_class", &facts(3.5, 15.0, 3.5)),
        Some("easa_c4")
    );
}

#[test]
fn easa_c1_energy_alternative() {
    // 1.2 kg at 11 m/s = 72.6 J <= 80 J: C1 by the energy alternative.
    let f = facts(1.2, 11.0, 0.6);
    assert!(f.energy_j <= 80.0);
    assert_eq!(matched_id("easa_class", &f), Some("easa_c1"));
    // 1.2 kg at 12 m/s = 86.4 J > 80 J: C2.
    assert_eq!(
        matched_id("easa_class", &facts(1.2, 12.0, 0.6)),
        Some("easa_c2")
    );
}

#[test]
fn faa_registration_and_part107_bands() {
    let id = |m: f64| matched_id("faa_reg", &facts(m, 15.0, 0.5)).unwrap();
    assert_eq!(id(0.249), "faa_under");
    // 0.55 lb is 249.48 g, so 250 g already needs registration.
    assert_eq!(id(0.250), "faa_mid");
    assert_eq!(id(0.251), "faa_mid");
    assert_eq!(id(24.9), "faa_mid");
    // 55 lb is 24.95 kg: 25 kg is over.
    assert_eq!(id(25.0), "faa_over");
    assert_eq!(
        matched_id("faa_part107", &facts(24.9, 20.0, 1.0)),
        Some("faa_p107")
    );
    assert_eq!(matched_id("faa_part107", &facts(25.0, 20.0, 1.0)), None);
    // 100 mph ground speed limit.
    assert_eq!(
        matched_id("faa_part107", &facts(2.0, 44.0, 1.0)),
        Some("faa_p107")
    );
    assert_eq!(matched_id("faa_part107", &facts(2.0, 45.0, 1.0)), None);
}

#[test]
fn faa_over_people_energy() {
    assert_eq!(
        matched_id("faa_people", &facts(0.2, 20.0, 0.3)),
        Some("faa_cat1")
    );
    // 1 kg at 5 m/s = 12.5 J <= 14.9 J.
    assert_eq!(
        matched_id("faa_people", &facts(1.0, 5.0, 0.5)),
        Some("faa_cat23")
    );
    // 1 kg at 6 m/s = 18 J.
    assert_eq!(matched_id("faa_people", &facts(1.0, 6.0, 0.5)), None);
}

#[test]
fn shgm_class_boundaries() {
    let id = |m: f64| matched_id("shgm_class", &facts(m, 10.0, 0.5)).unwrap();
    assert_eq!(id(0.499), "shgm_0");
    assert_eq!(id(0.5), "shgm_1");
    assert_eq!(id(3.999), "shgm_1");
    assert_eq!(id(4.0), "shgm_2");
    assert_eq!(id(25.0), "shgm_3");
    // Every SHGM tier is flagged for verification.
    assert!(
        TIERS
            .iter()
            .filter(|t| t.group == "shgm_class")
            .all(|t| t.verify)
    );
}

#[test]
fn margin_and_failures_are_reported() {
    let f = facts(0.24, 10.0, 0.4);
    let r = check_group(GROUPS.iter().find(|g| g.id == "easa_class").unwrap(), &f);
    assert_eq!(r.matched_tier().unwrap().id, "easa_c0");
    assert!((r.mass_headroom_kg(&f).unwrap() - 0.01).abs() < 1e-12);
    assert_eq!(r.next_tier().unwrap().id, "easa_c1");
    // A 300 g design is blocked from C0 by mass (50 g over).
    let f = facts(0.3, 10.0, 0.4);
    let r = check_group(GROUPS.iter().find(|g| g.id == "easa_class").unwrap(), &f);
    let blocked: Vec<_> = r.blocked_before_match().collect();
    assert_eq!(blocked.len(), 1);
    let fl = blocked[0].failures[0];
    assert_eq!(fl.criterion, Criterion::Mass);
    assert!((fl.value - fl.limit - 0.05).abs() < 1e-12);
}

#[test]
fn invalid_facts_are_rejected() {
    assert!(DesignFacts::new(0.0, 10.0, SpeedSource::UserOverride, 1.0).is_none());
    assert!(DesignFacts::new(f64::NAN, 10.0, SpeedSource::UserOverride, 1.0).is_none());
    assert!(DesignFacts::new(1.0, f64::INFINITY, SpeedSource::UserOverride, 1.0).is_none());
}

#[test]
fn table_integrity() {
    let ids: std::collections::HashSet<_> = TIERS.iter().map(|t| t.id).collect();
    assert_eq!(ids.len(), TIERS.len(), "duplicate tier id");
    for g in GROUPS {
        assert!(
            TIERS.iter().any(|t| t.group == g.id),
            "empty group {}",
            g.id
        );
    }
    for t in TIERS {
        assert!(
            GROUPS.iter().any(|g| g.id == t.group),
            "{} has no group",
            t.id
        );
        assert!(!t.source.trim().is_empty(), "{} has no source", t.id);
        assert_eq!(t.checked.len(), 10, "{} date format", t.id);
        assert!(!t.reqs.is_empty(), "{} has no requirements", t.id);
        if let Some(lt) = t.mass_lt_kg {
            assert!(lt > t.mass_ge_kg, "{} mass bounds", t.id);
        }
    }
    assert_eq!(rules::LAST_CHECKED.len(), 10);
    let tr = crate::i18n::tr_table();
    let en = crate::i18n::en_table();
    for key in rules::all_keys() {
        for (name, table) in [("tr", tr), ("en", en)] {
            let v = table.get(key);
            assert!(
                v.is_some_and(|s| !s.trim().is_empty()),
                "{name}: missing key {key}"
            );
        }
    }
    // Every regulation.* key in the files is also present in both.
    for key in tr.keys().filter(|k| k.starts_with("regulation.")) {
        assert!(en.contains_key(key), "en missing {key}");
    }
}

#[test]
fn every_vehicle_type_classifies() {
    let lib = Library::embedded();
    let mut projects: Vec<(String, Project)> = DroneType::ALL
        .iter()
        .map(|&dt| {
            let mut p = Project::default();
            p.set_drone_type(dt);
            (format!("{dt:?}"), p)
        })
        .collect();
    for file in [
        "fixed_wing_2kg_1600mm.json",
        "flying_wing_1200mm.json",
        "quad_250_x.json",
        "quad_275_x.json",
        "hexa_550_x.json",
        "vtol_quadplane.json",
    ] {
        projects.push((file.to_string(), project::load_example(file).unwrap()));
    }
    for (name, p) in projects {
        let m = evaluate(&p, &lib);
        let f = DesignFacts::from_model(&p, &m, None)
            .unwrap_or_else(|| panic!("{name}: no valid facts"));
        assert!(
            f.mtow_kg > 0.0 && f.v_max_ms > 0.0 && f.dimension_m > 0.0,
            "{name}"
        );
        let results = evaluate_all(&f);
        assert_eq!(results.len(), GROUPS.len());
        for r in &results {
            if r.group.id == "easa_class" || r.group.id == "faa_reg" || r.group.id == "shgm_class" {
                assert!(r.matched.is_some(), "{name}: {} unmatched", r.group.id);
            }
        }
        // A user override replaces the estimate.
        let o = DesignFacts::from_model(&p, &m, Some(33.0)).unwrap();
        assert_eq!(o.speed_source, SpeedSource::UserOverride);
        assert_eq!(o.v_max_ms, 33.0);
    }
}
