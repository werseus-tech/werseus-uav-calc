// SPDX-License-Identifier: AGPL-3.0-or-later
//! Basit mod bütünleşik testleri: çeviri tamlığı, özelleştirme koruması, eski dosyalar,
//! örnek projelerde ve her drone türünde sihirbaz kilidinin türetilen değerlerle açılması.
//! Gerçek %APPDATA%/Belgeler/Kimlik Bilgisi Yöneticisi ya da ağ kullanılmaz.
use super::testutil::apply_all;
use super::*;
use crate::calc::{Library, evaluate};
use crate::i18n::{Lang, t_lang};
use crate::ui::wizard::first_invalid_step;

const EXAMPLES: [&str; 6] = [
    "fixed_wing_2kg_1600mm.json",
    "flying_wing_1200mm.json",
    "hexa_550_x.json",
    "quad_250_x.json",
    "quad_275_x.json",
    "vtol_quadplane.json",
];

/// Makro öneklerinden gelen (gerçek anahtar olmayan) sabitler.
const PREFIXES: [&str; 16] = [
    "simple.mission.goal",
    "simple.mission.endurance",
    "simple.mission.payload",
    "simple.mission.launch",
    "simple.mission.terrain",
    "simple.weight.budget",
    "simple.weight.build",
    "simple.frame.class",
    "simple.frame.motors",
    "simple.frame.transport",
    "simple.dp.speed",
    "simple.dp.wind",
    "simple.dp.climb",
    "simple.prop.priority",
    "simple.prop.mr_style",
    "simple.struct.strength",
];

fn missing_in(lang: Lang, key: &str) -> bool {
    let k: &'static str = Box::leak(key.to_string().into_boxed_str());
    t_lang(k, lang) == key
}

#[test]
fn every_choice_and_field_key_is_translated() {
    let mut keys: Vec<&'static str> = Vec::new();
    macro_rules! add {
        ($($t:ty),+) => { $( for &v in <$t>::ALL { keys.push(v.key()); keys.push(v.desc_key()); } )+ };
    }
    add!(
        mission::Goal,
        mission::EnduranceGoal,
        mission::PayloadClass,
        mission::Launch,
        mission::Terrain,
        weight::Budget,
        weight::Build,
        frame::FrameClass,
        frame::MotorCount,
        frame::Transport,
        design_point::SpeedClass,
        design_point::Wind,
        design_point::Climb,
        propulsion::Priority,
        propulsion::MrStyle,
        structure::Strength
    );
    for f in Field::ALL {
        keys.push(f.key());
    }
    for k in keys {
        assert!(!missing_in(Lang::Tr, k), "TR eksik: {k}");
        assert!(!missing_in(Lang::En, k), "EN eksik: {k}");
    }
}

#[test]
fn every_simple_literal_in_sources_is_translated() {
    const SRC: &[&str] = &[
        include_str!("mission.rs"),
        include_str!("weight.rs"),
        include_str!("frame.rs"),
        include_str!("design_point.rs"),
        include_str!("endurance.rs"),
        include_str!("propulsion.rs"),
        include_str!("stability.rs"),
        include_str!("structure.rs"),
        include_str!("mod.rs"),
        include_str!("../../ui/simple_steps.rs"),
    ];
    let mut n = 0;
    for src in SRC {
        let mut rest: &str = src;
        while let Some(i) = rest.find("\"simple.") {
            let tail = &rest[i + 1..];
            let end = tail.find('"').unwrap();
            let key = &tail[..end];
            rest = &tail[end + 1..];
            if key.ends_with('.') || key.contains(' ') || PREFIXES.contains(&key) {
                continue;
            }
            n += 1;
            assert!(!missing_in(Lang::Tr, key), "TR eksik: {key}");
            assert!(!missing_in(Lang::En, key), "EN eksik: {key}");
        }
    }
    assert!(n > 100, "yeterli anahtar taranmalı: {n}");
}

#[test]
fn simple_texts_have_no_3d_printing_terms() {
    // simple.* altındaki tüm TR/EN metinleri taranır (kaynak dosyalardaki anahtarlar üzerinden).
    const SRC: &[&str] = &[
        include_str!("mission.rs"),
        include_str!("weight.rs"),
        include_str!("frame.rs"),
        include_str!("design_point.rs"),
        include_str!("endurance.rs"),
        include_str!("propulsion.rs"),
        include_str!("stability.rs"),
        include_str!("structure.rs"),
        include_str!("../../ui/simple_steps.rs"),
    ];
    for src in SRC {
        let mut rest: &str = src;
        while let Some(i) = rest.find("\"simple.") {
            let tail = &rest[i + 1..];
            let end = tail.find('"').unwrap();
            let key = &tail[..end];
            rest = &tail[end + 1..];
            for lang in [Lang::Tr, Lang::En] {
                let k: &'static str = Box::leak(key.to_string().into_boxed_str());
                let v = t_lang(k, lang).to_lowercase();
                for bad in ["print", "baskı", "filament", "nozzle", "infill", "slicer"] {
                    assert!(!v.contains(bad), "{key}: '{bad}' geçmemeli");
                }
            }
        }
    }
}

#[test]
fn old_project_files_open_with_default_simple_state() {
    let p: Project = serde_json::from_str("{}").unwrap();
    assert!(p.simple.active.is_empty() && p.simple.custom.is_empty());
    assert!(p.aero_simple.simple_mode, "varsayılan Basit");
    for f in EXAMPLES {
        let p = crate::project::load_example(f).unwrap();
        assert!(p.simple.active.is_empty(), "{f}");
    }
}

#[test]
fn state_roundtrips_through_json() {
    let mut p = Project::default();
    let snapshot = p.clone();
    p.simple.activate(SimpleStep::Weight, &snapshot);
    p.simple.custom.push(Field::Avionics);
    p.simple.applied_now.push(Field::Payload);
    let s = serde_json::to_string(&p).unwrap();
    let q: Project = serde_json::from_str(&s).unwrap();
    assert_eq!(q.simple.active, vec![SimpleStep::Weight]);
    assert_eq!(q.simple.custom, vec![Field::Avionics]);
    assert!(q.simple.applied_now.is_empty(), "geçici liste kaydedilmez");
}

#[test]
fn examples_keep_their_values_and_still_complete_the_wizard() {
    let lib = Library::embedded();
    for f in EXAMPLES {
        let mut p = crate::project::load_example(f).unwrap();
        let before = Snapshot::take(&p);
        apply_all(&mut p, &lib);
        // Elle girilmiş (varsayılandan farklı) değerler ezilmedi
        let d = Project::default();
        for (i, fld) in Field::ALL.iter().enumerate() {
            let was_default = (before.0[i] - fld.get(&d)).abs() <= 1e-9;
            if !was_default {
                assert!(
                    (fld.get(&p) - before.0[i]).abs() <= 1e-9,
                    "{f}: {fld:?} ezildi"
                );
            }
        }
        let m = evaluate(&p, &lib);
        assert!(m.inputs_ok, "{f}");
        assert!(first_invalid_step(&p, &m).is_none(), "{f}: sihirbaz kilidi");
    }
}

#[test]
fn fresh_project_of_every_type_completes_wizard_with_derived_values() {
    let lib = Library::embedded();
    for dt in DroneType::ALL {
        let mut p = Project::default();
        p.set_drone_type(dt);
        let m0 = evaluate(&p, &lib);
        assert!(first_invalid_step(&p, &m0).is_none(), "{dt:?} başlangıç");
        apply_all(&mut p, &lib);
        let m = evaluate(&p, &lib);
        assert!(m.inputs_ok, "{dt:?}");
        assert!(
            first_invalid_step(&p, &m).is_none(),
            "{dt:?}: {:?}",
            first_invalid_step(&p, &m)
        );
        // Varsayılan projede hiçbir değer özelleştirilmiş sayılmaz
        assert!(p.simple.custom.is_empty(), "{dt:?}: {:?}", p.simple.custom);
        assert_eq!(p.simple.active.len(), SimpleStep::ALL.len());
    }
}

#[test]
fn sampled_answer_combinations_keep_wizard_satisfiable() {
    let lib = Library::embedded();
    let mut seed = 7usize;
    let mut pick = |n: usize| {
        seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        (seed >> 8) % n
    };
    for _ in 0..24 {
        for dt in DroneType::ALL {
            let mut p = Project::default();
            p.set_drone_type(dt);
            let a = &mut p.simple;
            a.mission.goal = mission::Goal::ALL[pick(5)];
            a.mission.endurance = mission::EnduranceGoal::ALL[pick(4)];
            a.mission.payload = mission::PayloadClass::ALL[pick(5)];
            a.mission.launch = mission::Launch::ALL[pick(3)];
            a.mission.terrain = mission::Terrain::ALL[pick(4)];
            a.weight.budget = weight::Budget::ALL[pick(3)];
            a.weight.build = weight::Build::ALL[pick(3)];
            a.frame.class = frame::FrameClass::ALL[pick(4)];
            a.frame.motors = frame::MotorCount::ALL[pick(3)];
            a.design_point.speed = design_point::SpeedClass::ALL[pick(3)];
            a.design_point.wind = design_point::Wind::ALL[pick(3)];
            a.design_point.climb = design_point::Climb::ALL[pick(3)];
            a.propulsion.priority = propulsion::Priority::ALL[pick(3)];
            a.propulsion.mr_style = propulsion::MrStyle::ALL[pick(3)];
            apply_all(&mut p, &lib);
            let m = evaluate(&p, &lib);
            assert!(
                first_invalid_step(&p, &m).is_none(),
                "{dt:?}: {:?}",
                first_invalid_step(&p, &m)
            );
        }
    }
}

#[test]
fn customized_value_survives_simple_advanced_switch() {
    let lib = Library::embedded();
    let mut p = Project::default();
    p.aero_simple.simple_mode = true;
    let m = evaluate(&p, &lib);
    let ctx = Ctx::from_model(&p, &m, &lib.materials);
    answer_changed(&mut p, SimpleStep::Weight, &ctx);
    let derived = p.m_avionics;
    // Gelişmiş moda geç ve elle değiştir
    p.aero_simple.simple_mode = false;
    p.simple.begin_frame();
    let snap = Snapshot::take(&p);
    p.m_avionics = 0.222;
    let mut st = std::mem::take(&mut p.simple);
    st.mark_edits(&snap, &p);
    p.simple = st;
    assert!(p.simple.is_custom(Field::Avionics));
    // Gelişmişteyken senkron çalışmaz
    assert!(!sync(&mut p, &ctx));
    // Basite dön: değer korunur, cevap değişse bile ezilmez
    p.aero_simple.simple_mode = true;
    p.simple.weight.budget = weight::Budget::Large;
    sync(&mut p, &ctx);
    assert!((p.m_avionics - 0.222).abs() < 1e-12);
    assert!((derived - 0.222).abs() > 1e-6);
    // Diğer türetilmiş alanlar yine güncellenir
    let fuse_large = p.m_fuse_tail;
    p.simple.weight.budget = weight::Budget::Small;
    sync(&mut p, &ctx);
    assert!(p.m_fuse_tail < fuse_large);
    // "Varsayılana dön" ile türetme geri gelir
    p.simple.reset_step(SimpleStep::Weight);
    sync(&mut p, &ctx);
    assert!((p.m_avionics - 0.075).abs() < 1e-9);
}

#[test]
fn simple_modes_own_writes_are_not_flagged_as_customization() {
    let lib = Library::embedded();
    let mut p = Project::default();
    let m = evaluate(&p, &lib);
    let ctx = Ctx::from_model(&p, &m, &lib.materials);
    let snapshot = p.clone();
    p.simple.activate(SimpleStep::Mission, &snapshot);
    p.simple.begin_frame();
    let snap = Snapshot::take(&p);
    p.simple.mission.goal = mission::Goal::Fun;
    sync(&mut p, &ctx);
    let mut st = std::mem::take(&mut p.simple);
    st.mark_edits(&snap, &p);
    p.simple = st;
    assert!(p.simple.custom.is_empty(), "{:?}", p.simple.custom);
    assert!((p.v_cr - 22.0).abs() < 1e-12);
}

#[test]
fn sync_is_idempotent_and_skips_advanced_mode() {
    let lib = Library::embedded();
    let mut p = Project::default();
    let m = evaluate(&p, &lib);
    let ctx = Ctx::from_model(&p, &m, &lib.materials);
    answer_changed(&mut p, SimpleStep::Mission, &ctx);
    assert!(!sync(&mut p, &ctx), "ikinci çağrı değişiklik yapmamalı");
    p.aero_simple.simple_mode = false;
    p.simple.mission.goal = mission::Goal::Cargo;
    assert!(!sync(&mut p, &ctx));
}

#[test]
fn activation_marks_non_default_values_as_custom() {
    let mut p = Project::default();
    p.m_avionics = 0.3;
    let mut st = SimpleState::default();
    st.activate(SimpleStep::Weight, &p);
    assert!(st.is_custom(Field::Avionics));
    assert!(!st.is_custom(Field::Propulsion));
    let n = st.custom.len();
    st.activate(SimpleStep::Weight, &p);
    assert_eq!(st.custom.len(), n);
}

#[test]
fn material_identity_follows_build_choice() {
    let lib = Library::embedded();
    let mut p = Project::default();
    p.simple.weight.build = weight::Build::Reinforced;
    apply_all(&mut p, &lib);
    assert_eq!(p.material_id, "cf_petg");
    assert!((p.rho_mat - 1300.0).abs() < 1e-9);
    assert!(!p.material_name.is_empty());
}

#[test]
fn simple_mode_surfaces_have_no_printing_terms() {
    // Basit moddaki tüm metinler: simple.* ve aeros.* anahtarlarının her TR/EN değeri
    // (aero_simple.rs yüzey seçenekleri dahil). Eski gelişmiş ekranların (yapı/baskı sayfaları)
    // metinleri kapsam dışıdır.
    const BAD: [&str; 11] = [
        "print", "baskı", "baski", "filament", "nozzle", "infill", "slicer", "yazıcı", "3d ",
        "3d-", "petg",
    ];
    for (lang, raw) in [
        ("tr", include_str!("../../../assets/i18n/tr.json")),
        ("en", include_str!("../../../assets/i18n/en.json")),
    ] {
        let d: std::collections::BTreeMap<String, String> = serde_json::from_str(raw).unwrap();
        let mut n = 0;
        for (k, v) in &d {
            if !(k.starts_with("simple.") || k.starts_with("aeros.")) {
                continue;
            }
            n += 1;
            let low = v.to_lowercase();
            for bad in BAD {
                assert!(!low.contains(bad), "{lang} {k}: {bad}");
            }
            assert!(
                !low.split(|c: char| !c.is_alphanumeric())
                    .any(|w| w == "pla"),
                "{lang} {k}"
            );
        }
        assert!(n > 150, "{lang}: {n}");
    }
}

#[test]
fn no_printing_terms_in_any_i18n_value_or_ui_source() {
    // Baskı/dilimleme kavramları programda hiçbir yerde olmamalı: TÜM i18n değerleri (TR+EN),
    // malzeme notları ve UI kaynak dizgileri taranır.
    const BAD: [&str; 14] = [
        "print",
        "baskı",
        "baski",
        "filament",
        "nozzle",
        "nozül",
        "infill",
        "slicer",
        "yazıcı",
        "kobra",
        "basılı",
        "3d baskı",
        "tabla",
        "segmentasyon",
    ];
    let mut n = 0;
    for (lang, raw) in [
        ("tr", include_str!("../../../assets/i18n/tr.json")),
        ("en", include_str!("../../../assets/i18n/en.json")),
    ] {
        let d: std::collections::BTreeMap<String, String> = serde_json::from_str(raw).unwrap();
        for (k, v) in &d {
            n += 1;
            let low = v.to_lowercase();
            for bad in BAD {
                assert!(!low.contains(bad), "{lang} {k}: '{bad}'");
            }
            assert!(
                !low.contains("240 mm") && !low.contains("236 mm"),
                "{lang} {k}"
            );
            for w in low.split(|c: char| !c.is_alphanumeric()) {
                assert!(w != "bed", "{lang} {k}: '{w}'");
            }
        }
    }
    assert!(n > 4000, "yeterli değer taranmalı: {n}");
    // Malzeme notları
    let mats: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../../../assets/materials.json")).unwrap();
    for m in &mats {
        let notes = m["notes"].as_str().unwrap_or("").to_lowercase();
        for bad in BAD {
            assert!(!notes.contains(bad), "{}: '{bad}'", m["id"]);
        }
        assert!(m.get("print_flow_mm3_s").is_none());
        assert!(m.get("filament_density_kg_m3").is_none());
    }
    // UI kaynakları (tanımlayıcılar hariç: yalnızca kesin baskı terimleri)
    for (name, src) in [
        ("structure.rs", include_str!("../../ui/structure.rs")),
        ("summary.rs", include_str!("../../ui/summary.rs")),
        ("mr_structure.rs", include_str!("../../ui/mr_structure.rs")),
        ("multirotor.rs", include_str!("../../ui/multirotor.rs")),
        ("schematic.rs", include_str!("../../ui/schematic.rs")),
        ("formulas.rs", include_str!("../../ui/formulas.rs")),
        ("weight.rs", include_str!("../../ui/weight.rs")),
        ("outputs.rs", include_str!("../../ui/outputs.rs")),
    ] {
        let low = src.to_lowercase();
        for bad in [
            "kobra", "baskı", "filament", "nozzle", "yazıcı", "240 mm", "236 mm",
        ] {
            assert!(!low.contains(bad), "{name}: '{bad}'");
        }
    }
}

#[test]
fn old_project_files_with_print_fields_still_open() {
    // Eski proje dosyalarındaki baskı alanları sessizce yok sayılır.
    let p: Project = serde_json::from_str(
        r#"{"bed_z":0.25,"bed_margin":0.01,"print_efficiency":0.6,"name":"eski"}"#,
    )
    .unwrap();
    assert_eq!(p.name, "eski");
}
