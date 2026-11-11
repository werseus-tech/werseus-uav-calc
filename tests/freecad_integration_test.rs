// SPDX-License-Identifier: AGPL-3.0-or-later
//! FreeCAD entegrasyon testleri: Headless FreeCADCmd ile tum parca turlerinin (kanat, yatay kuyruk,
//! dikey kuyruk, V-kuyruk, govde, multirotor kolu, multirotor merkez plakasi, motor yuvasi) uretimi,
//! isValid() dogrulamasi ve Unicode yol destegi.

use std::fs;
use std::path::PathBuf;
use werseus_uav_calc::cad::{
    ExportOptions, FuselageCadParams, MotorMountCadParams, MultirotorArmCadParams,
    MultirotorPlateCadParams, TailCadParams, WingCadParams, detect_freecad,
    generate_fuselage_script, generate_motor_mount_script, generate_multirotor_arm_script,
    generate_multirotor_plate_script, generate_tail_script, generate_wing_script,
    run_freecad_script_sync,
};
use werseus_uav_calc::calc::geometry::TailKind;
use werseus_uav_calc::calc::multirotor::MultirotorLayout;
use werseus_uav_calc::calc::{Library, evaluate};
use werseus_uav_calc::project::load_example;

#[test]
#[ignore = "FreeCAD gerekir; yalnızca elle: cargo test -- --ignored"]
fn test_freecad_headless_integration() {
    let Some(inst) = detect_freecad(None) else {
        println!("FreeCAD bulunamadı, entegrasyon testi atlandı.");
        return;
    };

    println!(
        "FreeCAD bulundu: {} ({})",
        inst.version,
        inst.cmd_path.display()
    );

    // Çıktılar yalnızca geçici klasöre.
    let target_dir = std::env::temp_dir().join("werseus_cad_integration_test");
    fs::create_dir_all(&target_dir).expect("Hedef test klasörü oluşturulamadı");

    let lib = Library::embedded();

    // 1. Sabit Kanat Testi (fixed_wing_2kg_1600mm)
    {
        let p = load_example("fixed_wing_2kg_1600mm.json")
            .expect("fixed_wing_2kg_1600mm.json yüklenemedi");
        let m = evaluate(&p, &lib);

        let params = WingCadParams::from_project(&p, &m);
        let opts = ExportOptions {
            export_fcstd: true,
            export_step: true,
            export_stl: true,
            out_dir: target_dir.clone(),
        };

        let script = generate_wing_script(&params, &opts);
        let script_path = target_dir.join("test_wing_2kg.py");
        fs::write(&script_path, script).expect("test_wing_2kg.py yazılamadı");

        let res = run_freecad_script_sync(&inst.cmd_path, &script_path);
        assert!(
            res.is_ok(),
            "Kanat FreeCAD çalıştırması başarısız: {:?}",
            res.err()
        );
        let r = res.unwrap();
        assert!(r.valid, "Kanat katısı geçerli olmalı (valid == true)");
        assert!(
            r.volume_mm3 > 1000.0,
            "Kanat hacmi pozitif olmalı: {} mm3",
            r.volume_mm3
        );
        assert!(
            r.param_update_ok,
            "Kanat parametre güncellemesi başarılı olmalı"
        );
        assert!(
            PathBuf::from(&r.fcstd_path).is_file(),
            "Kanat .FCStd dosyası diskte mevcut olmalı: {}",
            r.fcstd_path
        );
        println!(
            "Kanat başarıyla üretildi: Hacim={:.1} cm3, Dilimler={}",
            r.volume_mm3 / 1000.0,
            r.num_segments
        );
    }

    // 2. Yatay Kuyruk Testi (Conventional, angle 0 deg)
    {
        let p = load_example("fixed_wing_2kg_1600mm.json")
            .expect("fixed_wing_2kg_1600mm.json yüklenemedi");
        let m = evaluate(&p, &lib);

        let params = TailCadParams::from_project(&p, &m, TailKind::Conventional);
        let opts = ExportOptions {
            export_fcstd: true,
            export_step: false,
            export_stl: false,
            out_dir: target_dir.clone(),
        };

        let script = generate_tail_script(&params, &opts);
        let script_path = target_dir.join("test_tail_horizontal.py");
        fs::write(&script_path, script).expect("test_tail_horizontal.py yazılamadı");

        let res = run_freecad_script_sync(&inst.cmd_path, &script_path);
        assert!(
            res.is_ok(),
            "Yatay kuyruk çalıştırması başarısız: {:?}",
            res.err()
        );
        let r = res.unwrap();
        assert!(r.valid, "Yatay kuyruk katısı geçerli olmalı");
        assert!(r.volume_mm3 > 100.0, "Yatay kuyruk hacmi pozitif olmalı");
        assert!(PathBuf::from(&r.fcstd_path).is_file());
        println!(
            "Yatay kuyruk başarıyla üretildi: Hacim={:.1} cm3",
            r.volume_mm3 / 1000.0
        );
    }

    // 3. Dikey Kuyruk Testi (Conventional, angle 90 deg)
    {
        let p = load_example("fixed_wing_2kg_1600mm.json")
            .expect("fixed_wing_2kg_1600mm.json yüklenemedi");
        let m = evaluate(&p, &lib);

        let mut params = TailCadParams::from_project(&p, &m, TailKind::Conventional);
        params.name = format!("{}_Dikey_Kuyruk", p.name.replace(' ', "_"));
        params.angle_deg = 90.0;
        let opts = ExportOptions {
            export_fcstd: true,
            export_step: false,
            export_stl: false,
            out_dir: target_dir.clone(),
        };

        let script = generate_tail_script(&params, &opts);
        let script_path = target_dir.join("test_tail_vertical.py");
        fs::write(&script_path, script).expect("test_tail_vertical.py yazılamadı");

        let res = run_freecad_script_sync(&inst.cmd_path, &script_path);
        assert!(
            res.is_ok(),
            "Dikey kuyruk çalıştırması başarısız: {:?}",
            res.err()
        );
        let r = res.unwrap();
        assert!(r.valid, "Dikey kuyruk katısı geçerli olmalı");
        assert!(r.volume_mm3 > 100.0, "Dikey kuyruk hacmi pozitif olmalı");
        assert!(PathBuf::from(&r.fcstd_path).is_file());
        println!(
            "Dikey kuyruk başarıyla üretildi: Hacim={:.1} cm3",
            r.volume_mm3 / 1000.0
        );
    }

    // 4. V-Kuyruk Testi (VTail, angle 35 deg)
    {
        let p = load_example("fixed_wing_2kg_1600mm.json")
            .expect("fixed_wing_2kg_1600mm.json yüklenemedi");
        let m = evaluate(&p, &lib);

        let params = TailCadParams::from_project(&p, &m, TailKind::VTail);
        let opts = ExportOptions {
            export_fcstd: true,
            export_step: false,
            export_stl: false,
            out_dir: target_dir.clone(),
        };

        let script = generate_tail_script(&params, &opts);
        let script_path = target_dir.join("test_tail_vtail.py");
        fs::write(&script_path, script).expect("test_tail_vtail.py yazılamadı");

        let res = run_freecad_script_sync(&inst.cmd_path, &script_path);
        assert!(
            res.is_ok(),
            "V-kuyruk çalıştırması başarısız: {:?}",
            res.err()
        );
        let r = res.unwrap();
        assert!(r.valid, "V-kuyruk katısı geçerli olmalı");
        assert!(r.volume_mm3 > 100.0, "V-kuyruk hacmi pozitif olmalı");
        assert!(PathBuf::from(&r.fcstd_path).is_file());
        println!(
            "V-kuyruk başarıyla üretildi: Hacim={:.1} cm3",
            r.volume_mm3 / 1000.0
        );
    }

    // 5. Gövde Testi (fixed_wing_2kg_1600mm)
    {
        let p = load_example("fixed_wing_2kg_1600mm.json")
            .expect("fixed_wing_2kg_1600mm.json yüklenemedi");
        let m = evaluate(&p, &lib);

        let params = FuselageCadParams::from_project(&p, &m);
        let opts = ExportOptions {
            export_fcstd: true,
            export_step: false,
            export_stl: false,
            out_dir: target_dir.clone(),
        };

        let script = generate_fuselage_script(&params, &opts);
        let script_path = target_dir.join("test_fuselage_2kg.py");
        fs::write(&script_path, script).expect("test_fuselage_2kg.py yazılamadı");

        let res = run_freecad_script_sync(&inst.cmd_path, &script_path);
        assert!(
            res.is_ok(),
            "Gövde FreeCAD çalıştırması başarısız: {:?}",
            res.err()
        );
        let r = res.unwrap();
        assert!(r.valid, "Gövde katısı geçerli olmalı (valid == true)");
        assert!(r.volume_mm3 > 1000.0, "Gövde hacmi pozitif olmalı");
        assert!(PathBuf::from(&r.fcstd_path).is_file());
        println!(
            "Gövde başarıyla üretildi: Hacim={:.1} cm3",
            r.volume_mm3 / 1000.0
        );
    }

    // 6. Multirotor Kolu Testi (quad_250_x)
    {
        let p = load_example("quad_250_x.json").expect("quad_250_x.json yüklenemedi");
        let m = evaluate(&p, &lib);

        let params = MultirotorArmCadParams::from_project(&p, &m);
        let opts = ExportOptions {
            export_fcstd: true,
            export_step: false,
            export_stl: false,
            out_dir: target_dir.clone(),
        };

        let script = generate_multirotor_arm_script(&params, &opts);
        let script_path = target_dir.join("test_arm_250.py");
        fs::write(&script_path, script).expect("test_arm_250.py yazılamadı");

        let res = run_freecad_script_sync(&inst.cmd_path, &script_path);
        assert!(
            res.is_ok(),
            "Multirotor kol FreeCAD çalıştırması başarısız: {:?}",
            res.err()
        );
        let r = res.unwrap();
        assert!(r.valid, "Kol katısı geçerli olmalı (valid == true)");
        assert!(r.volume_mm3 > 100.0, "Kol hacmi pozitif olmalı");
        assert!(PathBuf::from(&r.fcstd_path).is_file());
        println!(
            "Multirotor kolu başarıyla üretildi: Hacim={:.1} cm3",
            r.volume_mm3 / 1000.0
        );
    }

    // 7. Multirotor Merkez Plakası Testleri (X, +, H düzenleri; 3, 4, 6, 8 kol)
    {
        let plate_cases = [
            (MultirotorLayout::X, 4, "Plate_X_4"),
            (MultirotorLayout::X, 6, "Plate_X_6"),
            (MultirotorLayout::X, 8, "Plate_X_8"),
            (MultirotorLayout::Plus, 4, "Plate_Plus_4"),
            (MultirotorLayout::Plus, 6, "Plate_Plus_6"),
            (MultirotorLayout::Plus, 8, "Plate_Plus_8"),
            (MultirotorLayout::H, 4, "Plate_H_4"),
            (MultirotorLayout::X, 3, "Plate_X_3"),
        ];

        for (layout, arm_count, tag) in plate_cases {
            let params = MultirotorPlateCadParams {
                name: tag.into(),
                layout,
                arm_count,
                plate_radius_mm: 50.0,
                thickness_mm: 2.5,
                arm_width_mm: 24.0,
                chassis_bolt_dia_mm: 3.0,
                fc_mount_pattern_mm: 30.5,
                center_hole_dia_mm: 20.0,
                arm_hole_r_mm: [25.0, 38.0],
            };
            let opts = ExportOptions {
                export_fcstd: true,
                export_step: false,
                export_stl: false,
                out_dir: target_dir.clone(),
            };

            let script = generate_multirotor_plate_script(&params, &opts);
            let script_path = target_dir.join(format!("test_plate_{tag}.py"));
            fs::write(&script_path, script).expect("Plaka betiği yazılamadı");

            let res = run_freecad_script_sync(&inst.cmd_path, &script_path);
            assert!(
                res.is_ok(),
                "Merkez plaka ({tag}) çalıştırması başarısız: {:?}",
                res.err()
            );
            let r = res.unwrap();
            assert!(
                r.valid,
                "Merkez plaka ({tag}) katısı geçerli olmalı (NULL shape olmamalı)"
            );
            assert!(
                r.volume_mm3 > 1000.0,
                "Merkez plaka ({tag}) hacmi pozitif olmalı: {}",
                r.volume_mm3
            );
            assert!(
                PathBuf::from(&r.fcstd_path).is_file(),
                "Merkez plaka ({tag}) .FCStd dosyası diskte olmalı"
            );
            println!(
                "Merkez plaka ({tag}) başarıyla üretildi: Hacim={:.1} cm3",
                r.volume_mm3 / 1000.0
            );
        }
    }

    // 8. Motor Yuvası Testi (Tractor / Pusher)
    {
        let params = MotorMountCadParams {
            name: "Motor_Yuvasi_Test".into(),
            mount_type: "tractor".into(),
            plate_size_mm: 50.0,
            thickness_mm: 4.0,
            motor_bolt_pattern_mm: 19.0,
            motor_bolt_dia_mm: 3.0,
            shaft_hole_dia_mm: 8.0,
            firewall_bolt_spacing_mm: 40.0,
            firewall_bolt_dia_mm: 3.5,
        };
        let opts = ExportOptions {
            export_fcstd: true,
            export_step: false,
            export_stl: false,
            out_dir: target_dir.clone(),
        };

        let script = generate_motor_mount_script(&params, &opts);
        let script_path = target_dir.join("test_motor_mount.py");
        fs::write(&script_path, script).expect("Motor yuvası betiği yazılamadı");

        let res = run_freecad_script_sync(&inst.cmd_path, &script_path);
        assert!(
            res.is_ok(),
            "Motor yuvası çalıştırması başarısız: {:?}",
            res.err()
        );
        let r = res.unwrap();
        assert!(r.valid, "Motor yuvası katısı geçerli olmalı");
        assert!(r.volume_mm3 > 500.0, "Motor yuvası hacmi pozitif olmalı");
        assert!(PathBuf::from(&r.fcstd_path).is_file());
        println!(
            "Motor yuvası başarıyla üretildi: Hacim={:.1} cm3",
            r.volume_mm3 / 1000.0
        );
    }

    // 9. Unicode Proje Adı ve Çıktı Klasörü Testi
    // Proje adı "Test_Tasarım_Çğışöü", çıktı klasörü "<geçici>/werseus_cad_test/Türkçe Klasör/"
    {
        let unicode_out_dir = std::env::temp_dir()
            .join("werseus_cad_test")
            .join("Türkçe Klasör");
        fs::create_dir_all(&unicode_out_dir).expect("Türkçe klasör oluşturulamadı");

        let params = MultirotorPlateCadParams {
            name: "Test_Tasarım_Çğışöü_Merkez_Plaka".into(),
            layout: MultirotorLayout::X,
            arm_count: 4,
            plate_radius_mm: 55.0,
            thickness_mm: 3.0,
            arm_width_mm: 24.0,
            chassis_bolt_dia_mm: 3.0,
            fc_mount_pattern_mm: 30.5,
            center_hole_dia_mm: 20.0,
            arm_hole_r_mm: [25.0, 38.0],
        };

        let opts = ExportOptions {
            export_fcstd: true,
            export_step: false,
            export_stl: false,
            out_dir: unicode_out_dir.clone(),
        };

        let script = generate_multirotor_plate_script(&params, &opts);
        let script_path = unicode_out_dir.join("Unicode_Plaka_Betiği.py");
        fs::write(&script_path, script).expect("Unicode yoldaki betik yazılamadı");

        let res = run_freecad_script_sync(&inst.cmd_path, &script_path);
        assert!(res.is_ok(), "Unicode yol testi başarısız: {:?}", res.err());
        let r = res.unwrap();
        assert!(
            r.valid,
            "Unicode yoldan üretilen katı geçerli olmalı (valid == true)"
        );
        assert!(r.volume_mm3 > 1000.0, "Hacim pozitif olmalı");
        assert!(
            PathBuf::from(&r.fcstd_path).is_file(),
            "Unicode klasördeki .FCStd dosyası diskte mevcut olmalı: {}",
            r.fcstd_path
        );
        println!(
            "Unicode yol testi BAŞARILI: Hacim={:.1} cm3, Dosya={}",
            r.volume_mm3 / 1000.0,
            r.fcstd_path
        );
    }
    let _ = fs::remove_dir_all(&target_dir);
    let _ = fs::remove_dir_all(std::env::temp_dir().join("werseus_cad_test"));
}

/// Parçaları ve montajı üretir (iptal/süre sınırı pratikte yok); özet ya da hata iletisi.
fn run_outputs_blocking(
    cmd_path: std::path::PathBuf,
    p: werseus_uav_calc::project::Project,
    m: werseus_uav_calc::calc::Model,
    ids: Vec<werseus_uav_calc::cad::outputs::PartId>,
    out_dir: std::path::PathBuf,
) -> Result<werseus_uav_calc::cad::outputs::OutputsSummary, String> {
    use werseus_uav_calc::cad::outputs::{RunEvent, run_outputs_controlled};
    use werseus_uav_calc::cad::{CadLimits, CancelFlag};
    let limits = CadLimits {
        part_secs: u64::MAX / 4,
        assembly_secs: u64::MAX / 4,
        total_secs: u64::MAX / 4,
    };
    let rx = run_outputs_controlled(cmd_path, p, m, ids, out_dir, limits, CancelFlag::new());
    while let Ok(ev) = rx.recv() {
        match ev {
            RunEvent::Done(s) => return Ok(*s),
            RunEvent::Error(e) => return Err(e),
            RunEvent::Aborted { .. } => return Err("aborted".into()),
            RunEvent::Log(_) | RunEvent::Progress(_) => {}
        }
    }
    Err("kanal kapandı".into())
}

type AssemblyOut = (
    werseus_uav_calc::cad::CadExecutionResult,
    werseus_uav_calc::cad::outputs::PairClasses,
    Vec<String>,
);

/// Montajı (zaman sınırı pratikte yok) çalıştırır.
fn run_assembly_blocking(
    cmd_path: &std::path::Path,
    items: &[werseus_uav_calc::cad::outputs::AssemblyItem],
    out_dir: &std::path::Path,
    script_dir: &std::path::Path,
) -> Result<AssemblyOut, String> {
    use werseus_uav_calc::cad::CancelFlag;
    use werseus_uav_calc::cad::outputs::run_assembly_controlled;
    run_assembly_controlled(
        cmd_path,
        items,
        out_dir,
        script_dir,
        std::time::Duration::from_secs(3600),
        &CancelFlag::new(),
        |_| {},
    )
    .map_err(|e| format!("{e:?}"))
}

/// Çıktılar adımı uçtan uca, tek örnek: tüm parçalar + montaj + BOM (yalnızca geçici klasöre).
/// Üretim başarılı, çakışma = 0, temaslar listeleniyor, bağlantısız parça yok.
fn e2e_example(ex: &str) {
    use werseus_uav_calc::cad::outputs::available_parts;
    let Some(inst) = detect_freecad(None) else {
        println!("FreeCAD bulunamadı, test atlandı.");
        return;
    };
    let lib = Library::embedded();
    let p = load_example(ex).expect("örnek yüklenemedi");
    let m = evaluate(&p, &lib);
    let out = std::env::temp_dir().join(format!("werseus_outputs_e2e_{}", ex.replace('.', "_")));
    let _ = fs::remove_dir_all(&out);
    let ids = available_parts(&p);
    let s = run_outputs_blocking(
        inst.cmd_path.clone(),
        p.clone(),
        m,
        ids.clone(),
        out.clone(),
    )
    .expect("üretim");
    let mc = &s.mass;
    println!(
        "E2E|{ex}|parca={}|temas={}|cakisma={}|bagsiz={}|olculemeyen={}|cad_kanat_g={:.0}|prog_kanat_g={:.0}|cad_diger_g={:.0}|prog_diger_g={:.0}|cad_top_g={:.0}|prog_top_g={:.0}|oran={:.2}|aktarim={}",
        s.parts.len(),
        s.contacts.len(),
        s.interferences.len(),
        s.floating.len(),
        s.check_failed.len(),
        mc.cad_wing_kg * 1000.0,
        mc.prog_wing_kg * 1000.0,
        mc.cad_other_kg * 1000.0,
        mc.prog_other_kg * 1000.0,
        mc.cad_total_kg() * 1000.0,
        mc.prog_total_kg() * 1000.0,
        mc.ratio(),
        mc.transfer_allowed()
    );
    for c in &s.contacts {
        println!("  temas: {} - {}", c.a, c.b);
    }
    for i in &s.interferences {
        println!("  CAKISMA: {} x {}: {:.2} mm3", i.a, i.b, i.volume_mm3);
    }
    assert!(s.failed.is_empty(), "{ex}: {:?}", s.failed);
    assert_eq!(s.parts.len(), ids.len());
    assert!(out.join("BOM.md").exists() && out.join("BOM.csv").exists());
    assert!(out.join("Montaj_Raporu.md").exists());
    assert!(!s.assembly_fcstd.is_empty() && fs::metadata(&s.assembly_fcstd).is_ok());
    assert!(!s.assembly_step.is_empty() && fs::metadata(&s.assembly_step).is_ok());
    assert!(s.mass_kg > 0.0);
    assert!(
        s.check_failed.is_empty(),
        "{ex}: ölçülemeyen çift: {:?}",
        s.check_failed
    );
    assert!(
        s.interferences.is_empty(),
        "{ex}: çakışma: {:?}",
        s.interferences
    );
    assert!(!s.contacts.is_empty(), "{ex}: temas listelenmeli");
    assert!(s.floating.is_empty(), "{ex}: bağlantısız: {:?}", s.floating);
    let _ = fs::remove_dir_all(&out);
}

#[test]
#[ignore = "FreeCAD gerekir; yalnızca elle: cargo test -- --ignored"]
fn e2e_fixed_wing_2kg_1600mm() {
    e2e_example("fixed_wing_2kg_1600mm.json");
}

#[test]
#[ignore = "FreeCAD gerekir; yalnızca elle: cargo test -- --ignored"]
fn e2e_flying_wing_1200mm() {
    e2e_example("flying_wing_1200mm.json");
}

#[test]
#[ignore = "FreeCAD gerekir; yalnızca elle: cargo test -- --ignored"]
fn e2e_hexa_550_x() {
    e2e_example("hexa_550_x.json");
}

#[test]
#[ignore = "FreeCAD gerekir; yalnızca elle: cargo test -- --ignored"]
fn e2e_quad_250_x() {
    e2e_example("quad_250_x.json");
}

#[test]
#[ignore = "FreeCAD gerekir; yalnızca elle: cargo test -- --ignored"]
fn e2e_quad_275_x() {
    e2e_example("quad_275_x.json");
}

#[test]
#[ignore = "FreeCAD gerekir; yalnızca elle: cargo test -- --ignored"]
fn e2e_vtol_quadplane() {
    e2e_example("vtol_quadplane.json");
}

/// Örnek klasöründeki her proje için bir uçtan uca test olduğunu doğrular (FreeCAD gerekmez).
#[test]
fn every_example_has_an_e2e_test() {
    let src = include_str!("freecad_integration_test.rs");
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");
    for e in fs::read_dir(dir).unwrap() {
        let name = e.unwrap().file_name().to_string_lossy().to_string();
        if let Some(stem) = name.strip_suffix(".json") {
            assert!(
                src.contains(&format!("fn e2e_{stem}()")),
                "{name} için e2e testi yok"
            );
        }
    }
}

/// Kontrol körleşmemiş olmalı: bilerek çakıştırılmış yerleşimde alarm vermeli.
#[test]
#[ignore = "FreeCAD gerekir; yalnızca elle: cargo test -- --ignored"]
fn interference_check_detects_seeded_overlap() {
    use werseus_uav_calc::cad::outputs::{assembly_items, available_parts};
    let Some(inst) = detect_freecad(None) else {
        println!("FreeCAD bulunamadı, test atlandı.");
        return;
    };
    let lib = Library::embedded();
    let p = load_example("fixed_wing_2kg_1600mm.json").unwrap();
    let m = evaluate(&p, &lib);
    let out = std::env::temp_dir().join("werseus_outputs_seeded_overlap");
    let _ = fs::remove_dir_all(&out);
    let s = run_outputs_blocking(
        inst.cmd_path.clone(),
        p.clone(),
        m.clone(),
        available_parts(&p),
        out.clone(),
    )
    .expect("üretim");
    assert!(s.failed.is_empty(), "{:?}", s.failed);
    let mut items = assembly_items(&p, &m, &s.parts);
    // 1) Sol kanadı 40 mm geriye kaydır: yuvasından çıkıp gövde kabuğuna girer.
    // 2) Motor yuvasını 3 mm gövdenin içine it.
    for it in &mut items {
        if it.label == "Wing_L" {
            it.m[3] += 40.0;
        }
        if it.label == "MotorMount" {
            it.m[3] += 3.0;
        }
    }
    let script_dir = out.join("scripts");
    fs::create_dir_all(&script_dir).unwrap();
    let asm_dir = out.join("seeded");
    let (_r, classes, _fl) =
        run_assembly_blocking(&inst.cmd_path, &items, &asm_dir, &script_dir).expect("montaj");
    for i in &classes.interferences {
        println!(
            "  seeded CAKISMA: {} x {}: {:.1} mm3",
            i.a, i.b, i.volume_mm3
        );
    }
    let hit = |a: &str, b: &str| {
        classes
            .interferences
            .iter()
            .any(|i| (i.a == a && i.b == b) || (i.a == b && i.b == a))
    };
    assert!(
        hit("Fuselage", "Wing_L"),
        "kanat-gövde çakışması yakalanmalı"
    );
    assert!(
        hit("Fuselage", "MotorMount"),
        "motor yuvası-gövde çakışması yakalanmalı"
    );
    let _ = fs::remove_dir_all(&out);
}

/// Bir örnek için tüm parçaları üretir ve (etiket, düzenleme) çiftleriyle bozulmuş montajları
/// ayrı ayrı çalıştırır; her bozulma için beklenen (a, b) çakışması yakalanmalıdır.
/// (ad, [(etiket, matris dizini, öteleme)], [beklenen çakışma çiftleri]).
type SeededCase<'a> = (
    &'a str,
    &'a [(&'a str, usize, f64)],
    &'a [(&'a str, &'a str)],
);

fn seeded_cases(ex: &str, cases: &[SeededCase]) {
    use werseus_uav_calc::cad::outputs::{assembly_items, available_parts};
    let Some(inst) = detect_freecad(None) else {
        println!("FreeCAD bulunamadı, test atlandı.");
        return;
    };
    let lib = Library::embedded();
    let p = load_example(ex).unwrap();
    let m = evaluate(&p, &lib);
    let out = std::env::temp_dir().join(format!("werseus_seeded2_{}", ex.replace('.', "_")));
    let _ = fs::remove_dir_all(&out);
    let s = run_outputs_blocking(
        inst.cmd_path.clone(),
        p.clone(),
        m.clone(),
        available_parts(&p),
        out.clone(),
    )
    .expect("üretim");
    assert!(s.failed.is_empty(), "{:?}", s.failed);
    assert!(
        s.interferences.is_empty(),
        "bozulmamış düzenek temiz olmalı"
    );
    let script_dir = out.join("scripts");
    fs::create_dir_all(&script_dir).unwrap();
    for (k, (name, shifts, expect)) in cases.iter().enumerate() {
        let mut items = assembly_items(&p, &m, &s.parts);
        for (label, idx, d) in shifts.iter() {
            let it = items
                .iter_mut()
                .find(|i| i.label == *label)
                .expect("etiket yok");
            it.m[*idx] += d;
        }
        let (_r, classes, fl) = run_assembly_blocking(
            &inst.cmd_path,
            &items,
            &out.join(format!("seeded_{k}")),
            &script_dir,
        )
        .expect("montaj");
        println!(
            "SEEDED|{ex}|{name}|cakisma={:?}|bagsiz={fl:?}",
            classes
                .interferences
                .iter()
                .map(|i| format!("{}x{}:{:.0}", i.a, i.b, i.volume_mm3))
                .collect::<Vec<_>>()
        );
        for (a, b) in expect.iter() {
            assert!(
                classes
                    .interferences
                    .iter()
                    .any(|i| (i.a == *a && i.b == *b) || (i.a == *b && i.b == *a)),
                "{name}: {a} x {b} çakışması yakalanmalı: {:?}",
                classes.interferences
            );
        }
    }
    let _ = fs::remove_dir_all(&out);
}

/// Matris dizinleri: 3 = x, 7 = y, 11 = z öteleme. Bilinen sınır: OCC bulanık ortak hacim işlemi
/// B-spline kabuklarda (kanat-kanat gibi) gerçek çakışmayı boş döndürebiliyor; bu yüzden yalnızca
/// güvenilir yakalanan bozulmalar burada doğrulanır.
#[test]
#[ignore = "FreeCAD gerekir; yalnızca elle: cargo test -- --ignored"]
fn seeded_overlaps_fixed_wing_more() {
    seeded_cases(
        "fixed_wing_2kg_1600mm.json",
        &[
            // Sol kanadı 5 mm yukarı kaydır: yuva kenarı kanat kabuğuna girer.
            (
                "kanat 5 mm yukari",
                &[("Wing_L", 11, 5.0)],
                &[("Fuselage", "Wing_L")],
            ),
            // Sol kanadı 40 mm geriye kaydır.
            (
                "kanat 40 mm geri",
                &[("Wing_L", 3, 40.0)],
                &[("Fuselage", "Wing_L")],
            ),
            // Kanat kök yönünde 20 mm içeri: yuva ekseni boyunca kayma. Kiriş boruyla kesişir.
            (
                "kanat 20 mm iceri (kayma)",
                &[("Wing_L", 7, 20.0)],
                &[("Wing_L", "WingSpar_L")],
            ),
            // Motor yuvasını firewall'ın içine 6 mm göm (itme yönünün tersi: +x).
            (
                "motor firewall icine",
                &[("MotorMount", 3, 6.0)],
                &[("Fuselage", "MotorMount")],
            ),
            // Dikey kuyruğu yatay kuyruğun içine 10 mm indir.
            (
                "dikey kuyruk yatayin icine",
                &[("TailV", 11, -10.0)],
                &[("TailH_L", "TailV")],
            ),
        ],
    );
}

#[test]
#[ignore = "FreeCAD gerekir; yalnızca elle: cargo test -- --ignored"]
fn seeded_overlaps_multirotor_and_vtol() {
    seeded_cases(
        "quad_250_x.json",
        &[(
            "kol plakanin icine",
            &[("MrArm_0", 11, -1.5)],
            &[("MrPlate", "MrArm_0")],
        )],
    );
    seeded_cases(
        "vtol_quadplane.json",
        &[(
            "bom kanadin icine",
            &[("Boom_L", 11, 6.0)],
            &[("Boom_L", "BoomClamp_L")],
        )],
    );
}

/// Bir bozulma senaryosu: kopyaların kaydırılması (global ya da parça yerel ekseninde) ve
/// beklenen sonuç: hangi çiftler çakışmalı, hangileri çakışmamalı, hangi kopya oturmamış olmalı.
struct Probe<'a> {
    name: &'a str,
    /// (etiket, yerel eksende mi, kayma [mm])
    shifts: &'a [(&'a str, bool, [f64; 3])],
    interf: &'a [(&'a str, &'a str)],
    clean: &'a [(&'a str, &'a str)],
    unseated: &'a [&'a str],
}

fn probe_cases(ex: &str, cases: &[Probe]) {
    use werseus_uav_calc::cad::outputs::{assembly_items, available_parts};
    let Some(inst) = detect_freecad(None) else {
        println!("FreeCAD bulunamadı, test atlandı.");
        return;
    };
    let lib = Library::embedded();
    let p = load_example(ex).unwrap();
    let m = evaluate(&p, &lib);
    let out = std::env::temp_dir().join(format!("werseus_probe_{}", ex.replace('.', "_")));
    let _ = fs::remove_dir_all(&out);
    let s = run_outputs_blocking(
        inst.cmd_path.clone(),
        p.clone(),
        m.clone(),
        available_parts(&p),
        out.clone(),
    )
    .expect("üretim");
    assert!(s.failed.is_empty(), "{:?}", s.failed);
    assert!(
        s.interferences.is_empty(),
        "{ex}: taban temiz olmalı: {:?}",
        s.interferences
    );
    assert!(
        s.unseated.is_empty(),
        "{ex}: taban oturmuş olmalı: {:?}",
        s.unseated
    );
    let script_dir = out.join("scripts");
    fs::create_dir_all(&script_dir).unwrap();
    let mut errors = Vec::new();
    for (k, c) in cases.iter().enumerate() {
        let mut items = assembly_items(&p, &m, &s.parts);
        for (label, local, d) in c.shifts {
            let it = items
                .iter_mut()
                .find(|i| i.label == *label)
                .expect("etiket");
            let g = if *local {
                // r · d (r sütunları yerel eksenler; satır öncelikli 4x4)
                [
                    it.m[0] * d[0] + it.m[1] * d[1] + it.m[2] * d[2],
                    it.m[4] * d[0] + it.m[5] * d[1] + it.m[6] * d[2],
                    it.m[8] * d[0] + it.m[9] * d[1] + it.m[10] * d[2],
                ]
            } else {
                *d
            };
            it.m[3] += g[0];
            it.m[7] += g[1];
            it.m[11] += g[2];
        }
        let (_r, cl, _fl) = run_assembly_blocking(
            &inst.cmd_path,
            &items,
            &out.join(format!("case_{k}")),
            &script_dir,
        )
        .expect("montaj");
        let found: Vec<String> = cl
            .interferences
            .iter()
            .map(|i| format!("{}x{}:{:.0}", i.a, i.b, i.volume_mm3))
            .collect();
        println!(
            "PROBE|{ex}|{}|cakisma={found:?}|oturmamis={:?}|olculemeyen={:?}",
            c.name, cl.unseated, cl.failed
        );
        let hit = |a: &str, b: &str| {
            cl.interferences
                .iter()
                .any(|i| (i.a == a && i.b == b) || (i.a == b && i.b == a))
        };
        for (a, b) in c.interf {
            if !hit(a, b) {
                errors.push(format!("{}: {a} x {b} yakalanmadı", c.name));
            }
        }
        for (a, b) in c.clean {
            if hit(a, b) {
                errors.push(format!("{}: {a} x {b} yanlış alarm", c.name));
            }
        }
        for l in c.unseated {
            if !cl.unseated.iter().any(|u| u.starts_with(&format!("{l}:"))) {
                errors.push(format!("{}: {l} oturmamış bildirilmedi", c.name));
            }
        }
        if !cl.failed.is_empty() {
            errors.push(format!("{}: ölçülemeyen {:?}", c.name, cl.failed));
        }
    }
    assert!(errors.is_empty(), "{ex}: {errors:#?}");
    let _ = fs::remove_dir_all(&out);
}

/// Kabuk-kabuk, kabuk-boru, katı-kabuk: bulanık boolean'ın kaçırdığı kör noktalar ve bilinçli
/// temasın korunduğu negatif durumlar.
#[test]
#[ignore = "FreeCAD gerekir; yalnızca elle: cargo test -- --ignored"]
fn blind_spots_fixed_wing() {
    probe_cases(
        "fixed_wing_2kg_1600mm.json",
        &[
            Probe {
                name: "kabuk-kabuk: kanat 20 mm kok yonune (sol x sag ust uste)",
                shifts: &[("Wing_L", false, [0.0, 20.0, 0.0])],
                interf: &[("Wing_L", "Wing_R")],
                clean: &[],
                // Kanat yuvanın daha dar kesitine girer: gövdeyle teması kalmaz.
                unseated: &["Wing_L"],
            },
            Probe {
                name: "kabuk-kabuk: kanat 20 mm geri (yuva duvarina)",
                shifts: &[("Wing_L", false, [20.0, 0.0, 0.0])],
                interf: &[("Fuselage", "Wing_L")],
                clean: &[],
                unseated: &[],
            },
            Probe {
                // Daralan kanat dışarı kayınca yuvadaki kesit kanadın daha geniş kesitine denk
                // gelir: kama gibi sıkışma = gerçek girişim.
                name: "kabuk-kabuk: kanat 3 mm disari (yuva ekseninde, kama)",
                shifts: &[("Wing_L", false, [0.0, -3.0, 0.0])],
                interf: &[("Fuselage", "Wing_L")],
                clean: &[],
                unseated: &[],
            },
            Probe {
                name: "kabuk-kabuk negatif: kanat 0.03 mm yukari (tolerans ici)",
                shifts: &[("Wing_L", false, [0.0, 0.0, 0.03])],
                interf: &[],
                clean: &[("Fuselage", "Wing_L"), ("Wing_L", "Wing_R")],
                unseated: &[],
            },
            Probe {
                name: "kabuk-kabuk: yatay kuyruk 3 mm yukari (yuva kenari)",
                shifts: &[("TailH_L", false, [0.0, 0.0, 3.0])],
                interf: &[("Fuselage", "TailH_L")],
                clean: &[],
                unseated: &[],
            },
            Probe {
                name: "kabuk-boru: kiris 12 mm yukari (ust kabuga)",
                shifts: &[("WingSpar_L", false, [0.0, 0.0, 12.0])],
                interf: &[("Wing_L", "WingSpar_L")],
                clean: &[],
                unseated: &[],
            },
            Probe {
                name: "kabuk-boru negatif: kiris kendi ekseninde 1 mm (delikte kayar)",
                shifts: &[("WingSpar_L", true, [0.0, 0.0, 1.0])],
                interf: &[],
                clean: &[("Wing_L", "WingSpar_L")],
                unseated: &[],
            },
            Probe {
                name: "kati-kabuk: motor yuvasi 3 mm burna",
                shifts: &[("MotorMount", false, [3.0, 0.0, 0.0])],
                interf: &[("Fuselage", "MotorMount")],
                clean: &[],
                unseated: &[],
            },
            Probe {
                name: "kati-kabuk negatif: motor yuvasi firewall uzerinde 2 mm yana",
                shifts: &[("MotorMount", false, [0.0, 2.0, 0.0])],
                interf: &[],
                clean: &[("Fuselage", "MotorMount")],
                unseated: &[],
            },
        ],
    );
}

/// Katı-katı ve katı-boru çiftleri (multirotor, VTOL).
#[test]
#[ignore = "FreeCAD gerekir; yalnızca elle: cargo test -- --ignored"]
fn blind_spots_multirotor_and_vtol() {
    probe_cases(
        "quad_250_x.json",
        &[
            Probe {
                name: "kati-kati: motor yuvasi 1 mm kola",
                shifts: &[("MrMotorMount_0", false, [0.0, 0.0, -1.0])],
                interf: &[("MrArm_0", "MrMotorMount_0")],
                clean: &[],
                unseated: &[],
            },
            Probe {
                name: "kati-kati negatif: kol plaka uzerinde 2 mm disari kayar",
                shifts: &[("MrArm_0", true, [2.0, 0.0, 0.0])],
                interf: &[],
                clean: &[("MrPlate", "MrArm_0")],
                unseated: &[],
            },
            Probe {
                name: "kati-kati: kol 0.5 mm havada (oturmamis)",
                shifts: &[("MrArm_1", false, [0.0, 0.0, 0.5])],
                interf: &[],
                clean: &[("MrPlate", "MrArm_1")],
                unseated: &["MrArm_1"],
            },
        ],
    );
    probe_cases(
        "vtol_quadplane.json",
        &[
            Probe {
                name: "kati-boru: kaldirma yuvasi 1 mm boma",
                shifts: &[("LiftMount_LF", false, [0.0, 0.0, -1.0])],
                interf: &[("Boom_L", "LiftMount_LF")],
                clean: &[],
                unseated: &[],
            },
            Probe {
                name: "kati-boru negatif: kaldirma yuvasi bom boyunca 3 mm",
                shifts: &[("LiftMount_LF", false, [3.0, 0.0, 0.0])],
                interf: &[],
                clean: &[("Boom_L", "LiftMount_LF")],
                unseated: &[],
            },
            Probe {
                name: "kati-kabuk: kelepce 2 mm yukari (kanat kabuguna)",
                shifts: &[("BoomClamp_L", false, [0.0, 0.0, 2.0])],
                interf: &[("BoomClamp_L", "Wing_L")],
                clean: &[],
                unseated: &[],
            },
            Probe {
                name: "boru-kati negatif: bom kelepcede kendi ekseninde 5 mm",
                shifts: &[("Boom_L", true, [0.0, 0.0, 5.0])],
                interf: &[],
                clean: &[("Boom_L", "BoomClamp_L")],
                unseated: &[],
            },
        ],
    );
}
