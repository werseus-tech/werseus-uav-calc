// SPDX-License-Identifier: AGPL-3.0-or-later

use werseus_uav_calc::calc::{Library, Sev, evaluate};
use werseus_uav_calc::i18n::Lang;
use werseus_uav_calc::project::{DroneType, load_example};
use werseus_uav_calc::report::generate_markdown_report;

#[test]
fn test_fixed_wing_2kg_example() {
    let lib = Library::embedded();
    let p = load_example("fixed_wing_2kg_1600mm.json").expect("Failed to load fixed_wing example");
    assert_eq!(p.drone_type, DroneType::FixedWingConventional);
    assert!((p.b - 1.6).abs() < 1e-4);

    let m = evaluate(&p, &lib);
    assert!(
        (m.weight.m_mtow - 2.0).abs() < 0.25,
        "MTOW 2.0 kg civarında olmalı: {}",
        m.weight.m_mtow
    );
    assert!(
        m.cons.v_stall <= 11.0,
        "Stall hızı <= 11 m/s olmalı: {}",
        m.cons.v_stall
    );
    assert!(
        m.stab.in_band,
        "Statik marjin kabul edilebilir bantta olmalı"
    );
    assert!(m.warnings.iter().all(|w| w.sev != Sev::Err));
    assert!(
        werseus_uav_calc::ui::wizard::all_steps_valid(&p, &m),
        "quad_250_x sihirbaz doğrulamasından geçmeli"
    );

    let report = generate_markdown_report(&p, &m, Lang::Tr);
    assert!(report.contains("Fixed-Wing"));
    assert!(report.contains("Sürtünme Kutbu"));
}

#[test]
fn test_flying_wing_1200mm_example() {
    let lib = Library::embedded();
    let p = load_example("flying_wing_1200mm.json").expect("Failed to load flying_wing example");
    assert_eq!(p.drone_type, DroneType::FlyingWing);
    assert!((p.b - 1.2).abs() < 1e-4);

    let m = evaluate(&p, &lib);
    assert!(m.stab.in_band, "Uçan kanat statik marjini bantta olmalı");
    assert!(m.warnings.iter().all(|w| w.sev != Sev::Err));

    let report = generate_markdown_report(&p, &m, Lang::En);
    assert!(report.contains("Flying Wing"));
}

#[test]
fn test_quad_250_example() {
    let lib = Library::embedded();
    let p = load_example("quad_250_x.json").expect("Failed to load quad_250 example");
    assert_eq!(p.drone_type, DroneType::Multirotor);
    assert_eq!(p.mr_arm_count, 4);
    assert!((p.mr_frame_size - 0.250).abs() < 1e-4);

    let m = evaluate(&p, &lib);
    assert!(m.mr.tw_ratio >= 1.6, "T/W >= 1.6 olmalı: {}", m.mr.tw_ratio);
    assert!(
        m.mr.hover_throttle <= 0.65,
        "Askı gazı <= %65 olmalı: {}",
        m.mr.hover_throttle
    );
    assert!(
        m.mr.hover_endurance_min > 5.0,
        "Askı süresi > 5 dk olmalı: {}",
        m.mr.hover_endurance_min
    );
    assert!(m.warnings.iter().all(|w| w.sev != Sev::Err));
    assert!(
        werseus_uav_calc::ui::wizard::all_steps_valid(&p, &m),
        "quad_250_x sihirbaz doğrulamasından geçmeli"
    );

    let report = generate_markdown_report(&p, &m, Lang::Tr);
    assert!(report.contains("Multirotor"));
    assert!(report.contains("Disk"));
}

#[test]
fn test_quad_275_example() {
    let lib = Library::embedded();
    let p = load_example("quad_275_x.json").expect("Failed to load quad_275 example");
    assert_eq!(p.drone_type, DroneType::Multirotor);
    assert_eq!(p.mr_arm_count, 4);
    assert!((p.mr_frame_size - 0.275).abs() < 1e-4);

    let m = evaluate(&p, &lib);
    assert!(m.mr.tw_ratio >= 1.6);
    assert!(m.warnings.iter().all(|w| w.sev != Sev::Err));
    assert!(
        werseus_uav_calc::ui::wizard::all_steps_valid(&p, &m),
        "quad_275_x sihirbaz doğrulamasından geçmeli"
    );
}

#[test]
fn test_hexa_550_example() {
    let lib = Library::embedded();
    let p = load_example("hexa_550_x.json").expect("Failed to load hexa_550 example");
    assert_eq!(p.drone_type, DroneType::Multirotor);
    assert_eq!(p.mr_arm_count, 6);
    assert!((p.mr_frame_size - 0.550).abs() < 1e-4);

    let m = evaluate(&p, &lib);
    assert!(m.mr.tw_ratio >= 1.6, "T/W >= 1.6 olmalı: {}", m.mr.tw_ratio);
    assert!(m.mr_st.ms > 0.0, "Kol emniyet marjı pozitif olmalı");
    assert!(
        m.mr_st.deflection_ok,
        "Kol uç sehimi limitler dahilinde olmalı"
    );
    assert!(m.warnings.iter().all(|w| w.sev != Sev::Err));
    assert!(
        werseus_uav_calc::ui::wizard::all_steps_valid(&p, &m),
        "hexa_550_x sihirbaz doğrulamasından geçmeli"
    );

    let report = generate_markdown_report(&p, &m, Lang::Tr);
    assert!(report.contains("Multirotor"));
}

#[test]
fn test_vtol_quadplane_example() {
    let lib = Library::embedded();
    let p = load_example("vtol_quadplane.json").expect("Failed to load vtol_quadplane example");
    assert_eq!(p.drone_type, DroneType::VtolQuadplane);
    assert!((p.b - 1.8).abs() < 1e-4);
    assert_eq!(p.mr_arm_count, 4);

    let m = evaluate(&p, &lib);
    assert!(m.mr.tw_ratio >= 1.6, "VTOL T/W >= 1.6 olmalı");
    assert!(m.warnings.iter().all(|w| w.sev != Sev::Err));
    assert!(
        werseus_uav_calc::ui::wizard::all_steps_valid(&p, &m),
        "vtol_quadplane sihirbaz doğrulamasından geçmeli"
    );
}
