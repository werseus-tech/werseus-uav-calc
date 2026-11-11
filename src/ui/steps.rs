// SPDX-License-Identifier: AGPL-3.0-or-later
//! Adım ekranı dağıtımı: hangi ekranın girdileri sol panele, sonuçları sağ panele,
//! grafiği merkez görünüme gideceğine buradan karar verilir.
//!
//! Sihirbazda ekran `StepId`'den, uzman modunda sekmeden (`Tab`) türetilir. Uzman sekmelerinin
//! eşlemesi eski çalışma alanıyla birebir aynıdır (ör. multirotorda "Ağırlık" sekmesi eskisi gibi
//! sabit kanat ağırlık ekranını açar; "Multirotor" sekmesi tüm multirotor adımlarını birlikte).
use super::step_ui;
use super::wizard::StepId;
use super::{Tab, help};
use crate::calc::{Library, Model};
use crate::i18n::t;
use crate::project::{DroneType, Project};
use egui::Ui;

/// Çizilecek ekran.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Mission,
    Weight,
    MrMass,
    Geometry,
    Aero,
    DesignPoint,
    Endurance,
    Propulsion,
    Stability,
    Structure,
    Summary,
    Outputs,
    MrFrame,
    MrPropulsion,
    MrBattery,
    MrCg,
    MrStructure,
    /// Uzman modundaki "Multirotor" sekmesi: çerçeve, kütle, motor, pil, AG birlikte.
    MultirotorAll,
    /// Uzman modundaki "2D Şema" sekmesi (şema merkez görünümün grafiğidir).
    Schematic,
    /// Uzman modundaki rüzgâr tüneli (başka paketin işi; merkezde yerleşim yuvasında çizilir).
    WindTunnel,
}

/// Merkezde görünümün altında yer alan, taşınmamış (başka paketin) içerik yuvası.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CenterSlot {
    Outputs,
    WindTunnel,
}

pub fn screen_for_step(step: StepId, dt: DroneType) -> Screen {
    match step {
        StepId::Mission => Screen::Mission,
        StepId::Weight => {
            if dt == DroneType::Multirotor {
                Screen::MrMass
            } else {
                Screen::Weight
            }
        }
        StepId::Geometry => Screen::Geometry,
        StepId::Aero => Screen::Aero,
        StepId::DesignPoint => Screen::DesignPoint,
        StepId::Endurance => Screen::Endurance,
        StepId::Propulsion => Screen::Propulsion,
        StepId::Stability => Screen::Stability,
        StepId::Structure => Screen::Structure,
        StepId::Summary => Screen::Summary,
        StepId::Outputs => Screen::Outputs,
        StepId::MrFrame => Screen::MrFrame,
        StepId::MrPropulsion => Screen::MrPropulsion,
        StepId::MrBatteryLoiter => Screen::MrBattery,
        StepId::MrCG => Screen::MrCg,
        StepId::MrStructure => Screen::MrStructure,
    }
}

/// Uzman sekmesinin ekranı (eski `workspace_body` eşlemesiyle aynı).
pub fn screen_for_tab(tab: Tab, dt: DroneType) -> Screen {
    match tab {
        Tab::Summary => Screen::Summary,
        Tab::Mission => Screen::Mission,
        Tab::Weight => Screen::Weight,
        Tab::Geometry => Screen::Geometry,
        Tab::Aero => Screen::Aero,
        Tab::DesignPoint => Screen::DesignPoint,
        Tab::Endurance => Screen::Endurance,
        Tab::Propulsion => Screen::Propulsion,
        Tab::Stability => Screen::Stability,
        Tab::Structure => {
            if dt == DroneType::Multirotor {
                Screen::MrStructure
            } else {
                Screen::Structure
            }
        }
        Tab::Multirotor => Screen::MultirotorAll,
        Tab::Schematic => Screen::Schematic,
        Tab::WindTunnel => Screen::WindTunnel,
    }
}

/// Etkin ekran.
pub fn current_screen(p: &Project, tab: Tab) -> Screen {
    if p.expert_mode {
        screen_for_tab(tab, p.drone_type)
    } else {
        screen_for_step(super::shell::nav::current_step(p), p.drone_type)
    }
}

/// Merkezde görünümün altında çizilecek yuva (Çıktılar adımı, uzman Özet sekmesindeki çıktılar,
/// rüzgâr tüneli). Diğer ekranlarda merkez tamamen görünüme aittir.
pub fn center_slot(screen: Screen, expert: bool) -> Option<CenterSlot> {
    match screen {
        Screen::Outputs => Some(CenterSlot::Outputs),
        Screen::Summary if expert => Some(CenterSlot::Outputs),
        Screen::WindTunnel => Some(CenterSlot::WindTunnel),
        _ => None,
    }
}

/// Ekranın sol panelde girdisi var mı? (Çıktılar/rüzgâr tüneli yuvada, şema yalnız görünüm.)
pub fn has_inputs(screen: Screen) -> bool {
    !matches!(
        screen,
        Screen::Outputs | Screen::WindTunnel | Screen::Schematic
    )
}

/// Bu ekranın uyarı alanları (sağ panelde "bu adımın uyarıları" olarak öne alınır).
pub fn warning_areas(screen: Screen) -> &'static [&'static str] {
    match screen {
        Screen::Mission => &["area.mission"],
        Screen::Weight => &["area.weight"],
        Screen::MrMass => &["area.multirotor", "area.weight"],
        Screen::Geometry => &["area.geometry"],
        Screen::Aero => &["area.aero"],
        Screen::DesignPoint => &["area.design_point"],
        Screen::Endurance => &["area.range"],
        Screen::Propulsion => &["area.propulsion"],
        Screen::Stability => &["area.stability"],
        Screen::Structure => &["area.structure"],
        Screen::MrFrame
        | Screen::MrPropulsion
        | Screen::MrBattery
        | Screen::MrCg
        | Screen::MultirotorAll => &["area.multirotor"],
        Screen::MrStructure => &["area.mr_structure"],
        Screen::Summary | Screen::Outputs | Screen::Schematic | Screen::WindTunnel => &[],
    }
}

/// Merkez görünümün "Grafik" seçeneğinin adı (yoksa `None`).
pub fn chart_label(screen: Screen) -> Option<&'static str> {
    match screen {
        Screen::Weight => Some(t("ui.chart.mass_shares")),
        Screen::Aero => Some(t("ui.chart.polar")),
        Screen::DesignPoint => Some(t("ui.chart.constraint")),
        Screen::Endurance => Some(t("ui.chart.power")),
        Screen::Schematic => Some(t("tab_schematic")),
        _ => None,
    }
}

/// Merkez görünümün grafiği. `h`: kullanılabilir yükseklik.
pub fn chart(ui: &mut Ui, screen: Screen, p: &Project, m: &Model, h: f32) {
    match screen {
        Screen::Weight => super::weight::chart(ui, p, m, h),
        Screen::Aero => super::aero::chart(ui, p, m, h),
        Screen::DesignPoint => super::design_point::chart(ui, p, m, h),
        Screen::Endurance => super::endurance::chart(ui, p, m, h),
        Screen::Schematic => super::schematic::show(ui, p, m, (h - 40.0).max(120.0)),
        _ => {}
    }
}

/// Ekranın formül bölümleri var mı? (merkez görünümün "Formüller" seçeneği)
pub fn has_formulas(screen: Screen) -> bool {
    !matches!(
        screen,
        Screen::Outputs | Screen::WindTunnel | Screen::Schematic | Screen::MrStructure
    )
}

/// Adımın akademik kaynaklı formülleri ve anlık değerlerle hesapları (`ui::formulas`). Formül
/// başlıkları uzun olduğundan dar sağ panel yerine merkez görünümde ("Formüller") çizilir.
pub fn formulas(ui: &mut Ui, screen: Screen, p: &Project, m: &Model) {
    use super::formulas as f;
    match screen {
        Screen::Mission => f::show_mission_formulas(ui, p, m),
        Screen::Weight | Screen::MrMass => f::show_weight_formulas(ui, p, m),
        Screen::Geometry => f::show_geometry_formulas(ui, p, m),
        Screen::Aero => f::show_aero_formulas(ui, p, m),
        Screen::DesignPoint => f::show_design_point_formulas(ui, p, m),
        Screen::Endurance => f::show_endurance_formulas(ui, p, m),
        Screen::Propulsion => f::show_propulsion_formulas(ui, p, m),
        Screen::Stability => f::show_stability_formulas(ui, p, m),
        Screen::Structure => f::show_structure_formulas(ui, p, m),
        Screen::Summary => f::show_summary_formulas(ui, p, m),
        Screen::MrFrame => f::show_mr_frame_formulas(ui, p, m),
        Screen::MrPropulsion => f::show_mr_propulsion_formulas(ui, p, m),
        Screen::MrBattery => f::show_mr_battery_formulas(ui, p, m),
        Screen::MrCg => f::show_mr_cg_formulas(ui, p, m),
        Screen::MultirotorAll => {
            f::show_mr_frame_formulas(ui, p, m);
            f::show_weight_formulas(ui, p, m);
            f::show_mr_propulsion_formulas(ui, p, m);
            f::show_mr_battery_formulas(ui, p, m);
            f::show_mr_cg_formulas(ui, p, m);
        }
        Screen::MrStructure | Screen::Outputs | Screen::WindTunnel | Screen::Schematic => {}
    }
}

/// Sol panelin başındaki adım tanıtımı.
pub fn intro(ui: &mut Ui, screen: Screen) {
    let (title, what, hlp): (&str, &str, Option<&str>) = match screen {
        Screen::Mission => (
            t("wizard.step.mission.title"),
            t("wizard.step.mission.what_why"),
            Some(help::gorev()),
        ),
        Screen::Weight => (
            t("weight_heading"),
            t("weight_intro"),
            Some(help::agirlik()),
        ),
        Screen::MrMass => (t("mr.mass.title"), t("mr.mass.intro"), None),
        Screen::Geometry => (t("geo_heading"), t("geo_intro"), Some(help::geometri())),
        Screen::Aero => (t("aero.title"), t("aero.intro"), Some(help::aero())),
        Screen::DesignPoint => (t("dp_heading"), t("dp_intro"), Some(help::tasarim())),
        Screen::Endurance => (t("end_heading"), t("end_intro"), Some(help::menzil())),
        Screen::Propulsion => (t("prop_heading"), t("prop_intro"), Some(help::itki())),
        Screen::Stability => (t("stab_heading"), t("stab_intro"), Some(help::stabilite())),
        Screen::Structure => (t("struct_heading"), t("struct_intro"), Some(help::yapi())),
        Screen::Summary => (t("summary.title"), t("summary.intro"), Some(help::ozet())),
        Screen::MrFrame => (t("mr.frame.title"), t("mr.frame.intro"), None),
        Screen::MrPropulsion => (t("mr.thrust.title"), t("mr.thrust.intro"), None),
        Screen::MrBattery => (t("mr.range.title"), t("mr.range.intro"), None),
        Screen::MrCg => (t("mr.cg.title"), t("mr.cg.intro"), None),
        Screen::MrStructure => (t("mr_struct.title"), t("mr_struct.subtitle"), None),
        Screen::MultirotorAll => (t("tab_multirotor"), t("mr.frame.intro"), None),
        Screen::Outputs | Screen::Schematic | Screen::WindTunnel => return,
    };
    step_ui::intro(ui, title, what, hlp);
}

/// Basit modda özgün ekranı kendisi basit moda duyarlı olan adımlar (soru paneli üstte, ardından
/// özgün ekran açık). Diğerlerinde özgün (ham değerli) ekran "Ayrıntılı değerler" altında kapalı.
fn simple_aware(step: StepId) -> bool {
    matches!(step, StepId::Aero | StepId::Geometry | StepId::DesignPoint)
}

fn simple_active(p: &Project, simple_step: Option<StepId>) -> Option<StepId> {
    match simple_step {
        Some(s) if p.aero_simple.simple_mode && !matches!(s, StepId::Summary | StepId::Outputs) => {
            Some(s)
        }
        _ => None,
    }
}

/// Ekranın girdileri (sol panel).
pub fn inputs(ui: &mut Ui, screen: Screen, p: &mut Project, m: &Model, lib: &Library) {
    match screen {
        Screen::Mission => super::mission::inputs(ui, p, m),
        Screen::Weight => super::weight::inputs(ui, p, m, lib),
        Screen::MrMass => super::multirotor::mass_inputs(ui, p, m, lib),
        Screen::Geometry => super::geometry::inputs_panel(ui, p, m),
        Screen::Aero => super::aero::inputs(ui, p, m),
        Screen::DesignPoint => super::design_point::inputs(ui, p, m),
        Screen::Endurance => super::endurance::inputs(ui, p, m),
        Screen::Propulsion => super::propulsion::inputs(ui, p, m),
        Screen::Stability => super::stability::inputs(ui, p, m),
        Screen::Structure => super::structure::inputs(ui, p, m),
        Screen::Summary => super::summary::inputs(ui, p, m),
        Screen::MrFrame => super::multirotor::frame_inputs(ui, p, m),
        Screen::MrPropulsion => super::multirotor::motors_inputs(ui, p, m),
        Screen::MrBattery => super::multirotor::battery_inputs(ui, p, m),
        Screen::MrCg => super::multirotor::cg_inputs(ui, p, m),
        Screen::MrStructure => super::mr_structure::inputs(ui, p, m),
        Screen::MultirotorAll => {
            for (i, (title, f)) in [
                (
                    t("mr.frame.heading"),
                    super::multirotor::frame_inputs as fn(&mut Ui, &mut Project, &Model),
                ),
                (t("mr.thrust.heading"), super::multirotor::motors_inputs),
                (t("mr.range.heading"), super::multirotor::battery_inputs),
                (t("mr.cg.heading"), super::multirotor::cg_inputs),
            ]
            .into_iter()
            .enumerate()
            {
                egui::CollapsingHeader::new(title)
                    .id_salt(("mr_all_in", i))
                    .default_open(i == 0)
                    .show(ui, |ui| f(ui, p, m));
                if i == 0 {
                    egui::CollapsingHeader::new(t("mr.mass.heading"))
                        .id_salt("mr_all_in_mass")
                        .default_open(false)
                        .show(ui, |ui| super::multirotor::mass_inputs(ui, p, m, lib));
                }
            }
        }
        Screen::Outputs | Screen::Schematic | Screen::WindTunnel => {}
    }
}

/// Sol panel içeriği: basit mod soruları + özgün girdiler (basit modda gerekiyorsa katlanmış).
pub fn left_panel(
    ui: &mut Ui,
    screen: Screen,
    simple_step: Option<StepId>,
    p: &mut Project,
    m: &Model,
    lib: &Library,
) {
    intro(ui, screen);
    match simple_active(p, simple_step) {
        Some(step) => {
            super::simple_steps::inputs(ui, p, m, lib, step);
            if simple_aware(step) {
                inputs(ui, screen, p, m, lib);
            } else {
                ui.add_space(space_s());
                egui::CollapsingHeader::new(egui::RichText::new(t("simple.raw_values")).strong())
                    .id_salt(("simple_raw", step as u8))
                    // Sorusu olmayan (yalnız sonuç gösteren) adımlarda ham girdiler açık başlar.
                    .default_open(!super::simple_steps::has_questions(step))
                    .show(ui, |ui| inputs(ui, screen, p, m, lib));
            }
        }
        None => inputs(ui, screen, p, m, lib),
    }
}

fn space_s() -> f32 {
    super::theme::space::S
}

/// Ekranın sonuçları (sağ panel).
pub fn results(
    ui: &mut Ui,
    screen: Screen,
    p: &mut Project,
    m: &Model,
    lib: &Library,
    teach_why: &mut Option<crate::teach::ResultMetric>,
) {
    match screen {
        Screen::Mission => super::mission::results(ui, p, m),
        Screen::Weight => super::weight::results(ui, p, m),
        Screen::MrMass => super::multirotor::mass_results(ui, p, m),
        Screen::Geometry => super::geometry::results(ui, p, m),
        Screen::Aero => super::aero::results(ui, p, m),
        Screen::DesignPoint => super::design_point::results(ui, p, m),
        Screen::Endurance => super::endurance::results(ui, p, m),
        Screen::Propulsion => super::propulsion::results(ui, p, m),
        Screen::Stability => super::stability::results(ui, p, m),
        Screen::Structure => super::structure::results(ui, p, m),
        Screen::Summary => super::summary::results(ui, p, m, teach_why),
        Screen::MrFrame => super::multirotor::frame_results(ui, p, m),
        Screen::MrPropulsion => super::multirotor::motors_results(ui, p, m),
        Screen::MrBattery => super::multirotor::battery_results(ui, p, m),
        Screen::MrCg => super::multirotor::cg_results(ui, p, m),
        Screen::MrStructure => super::mr_structure::results(ui, p, m),
        Screen::MultirotorAll => {
            for (i, (title, f)) in [
                (
                    t("mr.frame.heading"),
                    super::multirotor::frame_results as fn(&mut Ui, &mut Project, &Model),
                ),
                (t("mr.mass.heading"), super::multirotor::mass_results),
                (t("mr.thrust.heading"), super::multirotor::motors_results),
                (t("mr.range.heading"), super::multirotor::battery_results),
                (t("mr.cg.heading"), super::multirotor::cg_results),
            ]
            .into_iter()
            .enumerate()
            {
                egui::CollapsingHeader::new(title)
                    .id_salt(("mr_all_out", i))
                    .default_open(i == 0)
                    .show(ui, |ui| f(ui, p, m));
            }
        }
        Screen::Outputs | Screen::Schematic | Screen::WindTunnel => {}
    }
    let _ = lib;
}

/// Sağ paneldeki adım bölümü: basit mod sonuçları + özgün sonuçlar.
pub fn right_panel(
    ui: &mut Ui,
    screen: Screen,
    simple_step: Option<StepId>,
    p: &mut Project,
    m: &Model,
    lib: &Library,
    teach_why: &mut Option<crate::teach::ResultMetric>,
) {
    match simple_active(p, simple_step) {
        Some(step) => {
            super::simple_steps::results(ui, p, m, lib, step);
            if simple_aware(step) {
                results(ui, screen, p, m, lib, teach_why);
            } else {
                ui.add_space(space_s());
                egui::CollapsingHeader::new(egui::RichText::new(t("simple.raw_values")).strong())
                    .id_salt(("simple_raw_out", step as u8))
                    .default_open(false)
                    .show(ui, |ui| results(ui, screen, p, m, lib, teach_why));
            }
        }
        None => results(ui, screen, p, m, lib, teach_why),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_step_maps_to_a_screen_with_inputs_except_outputs() {
        for dt in DroneType::ALL {
            for step in super::super::wizard::steps_for(dt) {
                let s = screen_for_step(step, dt);
                assert_eq!(has_inputs(s), step != StepId::Outputs, "{dt:?} {step:?}");
                assert_eq!(center_slot(s, false).is_some(), step == StepId::Outputs);
            }
        }
        assert_eq!(
            screen_for_step(StepId::Weight, DroneType::Multirotor),
            Screen::MrMass
        );
        // Uzman eşlemesi eski çalışma alanıyla aynı.
        assert_eq!(
            screen_for_tab(Tab::Weight, DroneType::Multirotor),
            Screen::Weight
        );
        assert_eq!(
            screen_for_tab(Tab::Structure, DroneType::Multirotor),
            Screen::MrStructure
        );
        assert_eq!(
            center_slot(Screen::Summary, true),
            Some(CenterSlot::Outputs)
        );
        assert_eq!(center_slot(Screen::Summary, false), None);
    }
}
