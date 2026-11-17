// SPDX-License-Identifier: AGPL-3.0-or-later
//! Structural strength and vibration screen: beam finite-element results per member (arm,
//! VTOL boom, half wing) with mode-shape sketch, frequency-vs-RPM (Campbell) chart and static
//! load-case table. Reads the project; the material/section choices here are a what-if for
//! this screen only and never change project values.
use super::components::{self, NumberInput, Status, number_row, section_header, u32_row};
use super::step_ui;
use super::theme::{self, space};
use super::widgets::fmt;
use crate::calc::Model;
use crate::i18n::{t, tf};
use crate::project::Project;
use crate::structfea::analysis::{
    self, ArmSection, ExcitationSource, FeaSettings, Harmonic, LoadCase, LoadCaseResult,
    MemberKind, MemberResult, ModeKind, ModeResult, NotApplicable, Risk,
};
use crate::structfea::material::StructMaterial;
use egui::{Color32, Ui};
use egui_plot::{HLine, Legend, Line, Plot, PlotPoints, Points, Polygon};

/// Screen state (kept in egui memory, not saved with the project).
#[derive(Clone, Copy, Debug, PartialEq)]
struct SfState {
    n_el: u32,
    landing_g: f64,
    margin_pct: f64,
    arm_material: Option<StructMaterial>,
    plate: bool,
    plate_w: f64,
    plate_t: f64,
    wing_material: Option<StructMaterial>,
}

impl Default for SfState {
    fn default() -> Self {
        let d = FeaSettings::default();
        Self {
            n_el: d.n_el as u32,
            landing_g: d.landing_g,
            margin_pct: d.margin * 100.0,
            arm_material: None,
            plate: false,
            plate_w: 0.022,
            plate_t: 0.004,
            wing_material: None,
        }
    }
}

impl SfState {
    fn settings(&self) -> FeaSettings {
        FeaSettings {
            n_el: self.n_el as usize,
            landing_g: self.landing_g,
            arm_material: self.arm_material,
            arm_section: if self.plate {
                ArmSection::Plate {
                    width: self.plate_w,
                    thickness: self.plate_t,
                }
            } else {
                ArmSection::Project
            },
            wing_material: self.wing_material,
            margin: self.margin_pct / 100.0,
            ..FeaSettings::default()
        }
    }
}

fn state_id() -> egui::Id {
    egui::Id::unique("structfea_state")
}

/// Mode display name, e.g. "Bending mode 2".
pub fn mode_name(kind: ModeKind, index: usize) -> String {
    match kind {
        ModeKind::Bending => tf("structfea.mode.bending_fmt", &[index.to_string()]),
        ModeKind::Torsion => tf("structfea.mode.torsion_fmt", &[index.to_string()]),
    }
}

fn member_name(kind: MemberKind) -> &'static str {
    match kind {
        MemberKind::Arm => t("structfea.member.arm"),
        MemberKind::Boom => t("structfea.member.boom"),
        MemberKind::WingHalf => t("structfea.member.wing"),
    }
}

fn source_name(s: ExcitationSource) -> &'static str {
    match s {
        ExcitationSource::LiftRotor => t("structfea.src.lift"),
        ExcitationSource::CruiseProp => t("structfea.src.cruise"),
    }
}

fn harmonic_name(h: Harmonic) -> &'static str {
    match h {
        Harmonic::Rev1 => t("structfea.harm.rev1"),
        Harmonic::BladePass => t("structfea.harm.bpf"),
    }
}

fn case_name(c: LoadCase, landing_g: f64) -> String {
    match c {
        LoadCase::HardLanding => tf("structfea.case.landing_fmt", &[fmt(landing_g, 1)]),
        LoadCase::MaxThrust => t("structfea.case.thrust").to_string(),
        LoadCase::WingBending => t("structfea.case.wing").to_string(),
    }
}

fn material_items() -> Vec<(Option<StructMaterial>, &'static str)> {
    let mut v = vec![(None, t("structfea.mat.project"))];
    v.extend(StructMaterial::ALL.iter().map(|m| (Some(*m), t(m.key()))));
    v
}

fn settings_ui(ui: &mut Ui, st: &mut SfState, has_arm: bool, has_wing: bool) {
    section_header(ui, t("structfea.inputs_heading"));
    u32_row(
        ui,
        t("structfea.n_el"),
        &mut st.n_el,
        4,
        60,
        t("structfea.n_el.desc"),
    );
    number_row(
        ui,
        NumberInput::new(t("structfea.landing_g"), &mut st.landing_g)
            .unit("g")
            .range(1.0, 15.0)
            .decimals(1)
            .help(t("structfea.landing_g.desc")),
    );
    number_row(
        ui,
        NumberInput::new(t("structfea.margin"), &mut st.margin_pct)
            .unit("%")
            .range(0.0, 50.0)
            .decimals(0)
            .help(t("structfea.margin.desc")),
    );
    let items = material_items();
    if has_arm {
        step_ui::combo(
            ui,
            "structfea_arm_mat",
            t("structfea.arm_material"),
            "",
            &mut st.arm_material,
            &items,
        );
        step_ui::combo(
            ui,
            "structfea_arm_section",
            t("structfea.arm_section"),
            "",
            &mut st.plate,
            &[
                (false, t("structfea.section.project")),
                (true, t("structfea.section.plate")),
            ],
        );
        if st.plate {
            number_row(
                ui,
                NumberInput::new(t("structfea.plate_w"), &mut st.plate_w)
                    .scale(1000.0)
                    .unit("mm")
                    .range(3.0, 80.0)
                    .decimals(1),
            );
            number_row(
                ui,
                NumberInput::new(t("structfea.plate_t"), &mut st.plate_t)
                    .scale(1000.0)
                    .unit("mm")
                    .range(0.5, 20.0)
                    .decimals(1),
            );
        }
    }
    if has_wing {
        step_ui::combo(
            ui,
            "structfea_wing_mat",
            t("structfea.wing_material"),
            "",
            &mut st.wing_material,
            &items,
        );
    }
    step_ui::note(ui, t("structfea.override_note"));
    step_ui::note(ui, t("structfea.plastic_note"));
}

/// Screen entry point (centre slot).
pub fn show(ui: &mut Ui, p: &Project, m: &Model) {
    let mut st: SfState = ui
        .ctx()
        .data_mut(|d| *d.get_temp_mut_or_default::<SfState>(state_id()));
    section_header(ui, t("structfea.title"));
    components::banner(ui, Status::Info, t("structfea.disclaimer"));
    ui.add_space(space::XS);

    let report = analysis::analyse(p, m, &st.settings());
    let has_arm = report
        .members
        .iter()
        .any(|x| matches!(x.kind, MemberKind::Arm | MemberKind::Boom));
    let has_wing = report
        .members
        .iter()
        .any(|x| x.kind == MemberKind::WingHalf);
    settings_ui(ui, &mut st, has_arm, has_wing);
    ui.ctx().data_mut(|d| d.insert_temp(state_id(), st));

    for na in &report.not_applicable {
        let key = match na {
            NotApplicable::NoWing => "structfea.na.no_wing",
            NotApplicable::NoArms => "structfea.na.no_arms",
        };
        ui.add_space(space::XS);
        components::banner(ui, Status::Neutral, t(key));
    }

    for (i, mbr) in report.members.iter().enumerate() {
        ui.add_space(space::S);
        egui::CollapsingHeader::new(egui::RichText::new(member_name(mbr.kind)).strong())
            .id_salt(("structfea_member", i))
            .default_open(true)
            .show(ui, |ui| member_ui(ui, p, m, mbr, &st, i));
    }
}

fn member_ui(ui: &mut Ui, p: &Project, m: &Model, mbr: &MemberResult, st: &SfState, idx: usize) {
    if !mbr.valid {
        components::banner(ui, Status::Err, t("structfea.invalid"));
        return;
    }
    ui.label(tf(
        "structfea.member_info_fmt",
        &[
            fmt(mbr.length * 1000.0, 0),
            fmt(mbr.tip_mass * 1000.0, 0),
            fmt(mbr.material.e / 1e9, 1),
            fmt(mbr.material.sigma_allow / 1e6, 0),
        ],
    ));
    match mbr.kind {
        MemberKind::Arm => step_ui::note(ui, t("structfea.arm_assume")),
        MemberKind::Boom => {
            step_ui::note(ui, t("structfea.arm_assume"));
            step_ui::note(
                ui,
                &tf(
                    "structfea.vtol_assume",
                    &[fmt(analysis::vtol_boom_y(p) * 1000.0, 0)],
                ),
            );
        }
        MemberKind::WingHalf => step_ui::note(ui, t("structfea.wing_assume")),
    }

    resonance_summary(ui, mbr);
    modes_table(ui, mbr);
    ui.add_space(space::XS);
    ui.columns(2, |c| {
        mode_shape_plot(&mut c[0], mbr, idx);
        campbell_plot(&mut c[1], mbr, st.margin_pct / 100.0, idx);
    });
    step_ui::note(ui, t("structfea.campbell_note"));
    if mbr.excitations.iter().any(|e| !e.direct) {
        step_ui::note(ui, t("structfea.indirect_note"));
    }
    ui.add_space(space::XS);
    loads_table(ui, mbr, st.landing_g, idx);
    step_ui::note(ui, t("structfea.loads_note"));
    step_ui::note(ui, t("structfea.landing_note"));
    if mbr.kind == MemberKind::Arm && st.arm_material.is_none() && !st.plate {
        consistency_line(ui, m, mbr);
    }
}

fn resonance_summary(ui: &mut Ui, mbr: &MemberResult) {
    section_header(ui, t("structfea.resonance_heading"));
    if mbr.excitations.is_empty() {
        step_ui::note(ui, t("structfea.no_excitation"));
        return;
    }
    for e in &mbr.excitations {
        step_ui::note(
            ui,
            &tf(
                "structfea.op_range_fmt",
                &[
                    source_name(e.source).to_string(),
                    fmt(e.rpm_lo, 0),
                    fmt(e.rpm_hi, 0),
                    fmt(e.rpm_dwell, 0),
                    e.blades.to_string(),
                ],
            ),
        );
    }
    if mbr.resonances.is_empty() {
        components::banner(ui, Status::Ok, t("structfea.res_ok"));
        return;
    }
    let mut sorted = mbr.resonances.clone();
    sorted.sort_by_key(|r| std::cmp::Reverse(r.risk));
    for r in &sorted {
        let args = [
            mode_name(r.mode_kind, r.mode_index),
            fmt(r.freq_hz, 0),
            source_name(r.source).to_string(),
            harmonic_name(r.harmonic).to_string(),
            fmt(r.rpm_cross, 0),
        ];
        let (status, key) = match r.risk {
            Risk::High => (Status::Err, "structfea.res_high_fmt"),
            Risk::Caution => (Status::Warn, "structfea.res_caution_fmt"),
        };
        components::banner(ui, status, &tf(key, &args));
    }
    step_ui::note(ui, t("structfea.res_advice"));
}

fn crossing_rpm(md: &ModeResult, h: f64) -> f64 {
    md.freq_hz * 60.0 / h
}

fn modes_table(ui: &mut Ui, mbr: &MemberResult) {
    section_header(ui, t("structfea.modes_heading"));
    let lift = mbr
        .excitations
        .iter()
        .find(|e| e.direct)
        .or(mbr.excitations.first())
        .copied();
    egui::Grid::new(("structfea_modes", mbr.kind as u8))
        .striped(true)
        .num_columns(4)
        .show(ui, |ui| {
            ui.strong(t("structfea.col.mode"));
            ui.strong(t("structfea.col.freq"));
            ui.strong(t("structfea.col.rpm1x"));
            ui.strong(t("structfea.col.rpmbpf"));
            ui.end_row();
            for md in &mbr.modes {
                ui.label(mode_name(md.kind, md.index));
                ui.label(format!("{} Hz", fmt(md.freq_hz, 1)));
                match lift {
                    Some(e) => {
                        ui.label(fmt(crossing_rpm(md, 1.0), 0));
                        ui.label(fmt(crossing_rpm(md, e.blades.max(1) as f64), 0));
                    }
                    None => {
                        ui.label("—");
                        ui.label("—");
                    }
                }
                ui.end_row();
            }
        });
}

fn palette(ui: &Ui) -> [Color32; 4] {
    let tk = theme::tokens(ui);
    [tk.accent, tk.info, tk.success, tk.warning]
}

fn mode_shape_plot(ui: &mut Ui, mbr: &MemberResult, idx: usize) {
    section_header(ui, t("structfea.shape_heading"));
    let colors = palette(ui);
    let axis = theme::tokens(ui).border_strong;
    Plot::new(("structfea_shape", idx))
        .height(220.0)
        .legend(Legend::default())
        .x_axis_label(t("structfea.axis.span"))
        .y_axis_label(t("structfea.axis.shape"))
        .include_y(-1.1)
        .include_y(1.1)
        .allow_scroll(false)
        .show(ui, |pu| {
            pu.hline(HLine::new("", 0.0).color(axis));
            for (k, md) in mbr
                .modes
                .iter()
                .filter(|md| md.kind == ModeKind::Bending)
                .enumerate()
            {
                let pts: Vec<[f64; 2]> = mbr
                    .node_x
                    .iter()
                    .zip(&md.shape)
                    .map(|(x, w)| [x * 1000.0, *w])
                    .collect();
                pu.line(
                    Line::new(
                        format!(
                            "{} ({} Hz)",
                            mode_name(md.kind, md.index),
                            fmt(md.freq_hz, 0)
                        ),
                        PlotPoints::from(pts),
                    )
                    .color(colors[k % colors.len()])
                    .width(2.0),
                );
            }
        });
}

fn campbell_plot(ui: &mut Ui, mbr: &MemberResult, margin: f64, idx: usize) {
    section_header(ui, t("structfea.campbell_heading"));
    if mbr.excitations.is_empty() {
        step_ui::note(ui, t("structfea.no_excitation"));
        return;
    }
    let tk = theme::tokens(ui);
    let rpm_max = mbr
        .excitations
        .iter()
        .map(|e| e.rpm_hi)
        .fold(0.0f64, f64::max)
        * 1.25;
    let f_exc_max = mbr
        .excitations
        .iter()
        .map(|e| e.blades.max(1) as f64 * rpm_max / 60.0)
        .fold(0.0f64, f64::max);
    let f_mode_min = mbr
        .modes
        .iter()
        .map(|m| m.freq_hz)
        .fold(f64::INFINITY, f64::min);
    let y_max = (f_exc_max * 1.3).max(f_mode_min * 1.2);
    let colors = palette(ui);
    Plot::new(("structfea_campbell", idx))
        .height(220.0)
        .legend(Legend::default())
        .x_axis_label(t("structfea.axis.rpm"))
        .y_axis_label(t("structfea.axis.freq"))
        .include_x(0.0)
        .include_x(rpm_max)
        .include_y(0.0)
        .include_y(y_max)
        .allow_scroll(false)
        .show(ui, |pu| {
            for e in &mbr.excitations {
                let op = vec![
                    [e.rpm_lo, 0.0],
                    [e.rpm_hi, 0.0],
                    [e.rpm_hi, y_max],
                    [e.rpm_lo, y_max],
                ];
                pu.polygon(
                    Polygon::new(source_name(e.source), PlotPoints::from(op))
                        .fill_color(tk.accent_soft.gamma_multiply(0.5))
                        .stroke(egui::Stroke::NONE),
                );
                let mut harmonics = vec![(Harmonic::Rev1, 1.0)];
                if e.blades > 1 {
                    harmonics.push((Harmonic::BladePass, e.blades as f64));
                }
                for (harm, h) in harmonics {
                    let name = format!("{} {}", source_name(e.source), harmonic_name(harm));
                    pu.line(
                        Line::new(
                            name.clone(),
                            PlotPoints::from(vec![[0.0, 0.0], [rpm_max, h * rpm_max / 60.0]]),
                        )
                        .color(tk.text_weak)
                        .width(1.0),
                    );
                    // Risk bands along the excitation line: operating range widened by the
                    // margin (warning) and dwell RPM +/- margin (danger).
                    let seg = |a: f64, b: f64| {
                        PlotPoints::from(vec![[a, h * a / 60.0], [b, h * b / 60.0]])
                    };
                    pu.line(
                        Line::new(
                            name.clone(),
                            seg(e.rpm_lo * (1.0 - margin), e.rpm_hi * (1.0 + margin)),
                        )
                        .color(tk.warning)
                        .width(5.0),
                    );
                    pu.line(
                        Line::new(
                            name,
                            seg(e.rpm_dwell * (1.0 - margin), e.rpm_dwell * (1.0 + margin)),
                        )
                        .color(tk.danger)
                        .width(5.0),
                    );
                }
            }
            for (k, md) in mbr.modes.iter().enumerate() {
                if md.freq_hz > y_max {
                    continue;
                }
                pu.hline(
                    HLine::new(
                        format!(
                            "{} ({} Hz)",
                            mode_name(md.kind, md.index),
                            fmt(md.freq_hz, 0)
                        ),
                        md.freq_hz,
                    )
                    .color(colors[k % colors.len()])
                    .width(1.5),
                );
            }
            let marks: Vec<[f64; 2]> = mbr
                .resonances
                .iter()
                .map(|r| [r.rpm_cross, r.freq_hz])
                .collect();
            if !marks.is_empty() {
                pu.points(
                    Points::new(t("structfea.resonance_heading"), PlotPoints::from(marks))
                        .color(tk.danger)
                        .radius(5.0),
                );
            }
        });
}

fn loads_table(ui: &mut Ui, mbr: &MemberResult, landing_g: f64, idx: usize) {
    section_header(ui, t("structfea.loads_heading"));
    let tk = theme::tokens(ui);
    egui::Grid::new(("structfea_loads", idx))
        .striped(true)
        .num_columns(7)
        .show(ui, |ui| {
            for key in [
                "structfea.col.case",
                "structfea.col.moment",
                "structfea.col.stress",
                "structfea.col.deflection",
                "structfea.col.sf",
                "structfea.col.required",
                "structfea.col.status",
            ] {
                ui.strong(t(key));
            }
            ui.end_row();
            for c in &mbr.load_cases {
                load_row(ui, tk, c, landing_g);
            }
        });
}

fn load_row(ui: &mut Ui, tk: &theme::Tokens, c: &LoadCaseResult, landing_g: f64) {
    ui.label(case_name(c.case, landing_g));
    if !c.applies {
        for _ in 0..5 {
            ui.label("—");
        }
        ui.label(egui::RichText::new(t("structfea.case_na")).color(tk.text_weak));
        ui.end_row();
        return;
    }
    ui.label(format!("{} N·m", fmt(c.max_moment, 2)));
    ui.label(format!("{} MPa", fmt(c.max_stress / 1e6, 1)));
    ui.label(format!("{} mm", fmt(c.max_deflection * 1000.0, 2)));
    ui.label(if c.safety_factor.is_finite() {
        fmt(c.safety_factor, 2)
    } else {
        "∞".to_string()
    });
    ui.label(fmt(c.required_sf, 2));
    if c.ok() {
        ui.label(egui::RichText::new(t("structfea.status_ok")).color(tk.success));
    } else {
        ui.label(
            egui::RichText::new(t("structfea.status_low"))
                .color(tk.danger)
                .strong(),
        );
    }
    ui.end_row();
}

fn consistency_line(ui: &mut Ui, m: &Model, mbr: &MemberResult) {
    let f1 = mbr
        .modes
        .iter()
        .find(|x| x.kind == ModeKind::Bending)
        .map(|x| x.freq_hz)
        .unwrap_or(f64::NAN);
    let sigma_ult = mbr
        .load_cases
        .iter()
        .find(|c| c.case == LoadCase::MaxThrust)
        .map(|c| c.max_stress * c.required_sf)
        .unwrap_or(f64::NAN);
    step_ui::note(
        ui,
        &tf(
            "structfea.consistency_fmt",
            &[
                fmt(f1, 0),
                fmt(m.mr_st.f_natural, 0),
                fmt(sigma_ult / 1e6, 1),
                fmt(m.mr_st.sigma_max / 1e6, 1),
            ],
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Library, evaluate};
    use crate::project::{self, DroneType};

    /// Every `structfea.` key used in this file and in the material table exists in both
    /// language tables.
    #[test]
    fn all_structfea_keys_exist_in_both_languages() {
        let src = concat!(
            include_str!("structfea.rs"),
            include_str!("../structfea/material.rs")
        );
        let tr = crate::i18n::tr_table();
        let en = crate::i18n::en_table();
        let mut n = 0;
        let quoted = concat!('"', "structfea.");
        for (pos, _) in src.match_indices(quoted) {
            let rest = &src[pos + 1..];
            let part = &rest[..rest.find('"').unwrap_or(0)];
            let is_key = part.len() > "structfea.".len()
                && !part.ends_with(".rs")
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_');
            if is_key {
                n += 1;
                assert!(tr.contains_key(part), "TR missing {part}");
                assert!(en.contains_key(part), "EN missing {part}");
            }
        }
        assert!(n > 40, "only {n} keys found");
        for k in en.keys().filter(|k| k.starts_with("structfea.")) {
            assert!(tr.contains_key(k), "TR missing {k}");
        }
    }

    /// The screen renders headless for every sample project without panicking.
    #[test]
    fn renders_for_all_sample_projects() {
        let lib = Library::embedded();
        let mut projects: Vec<Project> = [
            project::EX_FIXED_WING,
            project::EX_FLYING_WING,
            project::EX_QUAD_250,
            project::EX_HEXA_550,
            project::EX_VTOL,
        ]
        .iter()
        .map(|s| Project::from_json(s).unwrap())
        .collect();
        let mut broken = Project::default();
        broken.drone_type = DroneType::Multirotor;
        broken.mr_arm_d_i = broken.mr_arm_d_o;
        projects.push(broken);
        for p in projects {
            let m = evaluate(&p, &lib);
            let ctx = egui::Context::default();
            for _ in 0..2 {
                let mut input = egui::RawInput::default();
                input.screen_rect = Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 900.0),
                ));
                let mut out = ctx.run_ui(input, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| show(ui, &p, &m));
                });
                out.textures_delta.clear();
            }
        }
    }

    /// The screen is reachable as an expert tab for every vehicle type and draws its
    /// content in the centre slot of the full application frame.
    #[test]
    fn expert_tab_renders_in_app_shell() {
        for dt in DroneType::ALL {
            assert!(crate::ui::Tab::tabs_for(dt).contains(&crate::ui::Tab::StructFea));
            let ctx = egui::Context::default();
            let mut app = crate::ui::App::new_headless(&ctx, dt);
            app.project.expert_mode = true;
            app.tab = crate::ui::Tab::StructFea;
            assert_eq!(
                crate::ui::steps::current_screen(&app.project, app.tab),
                crate::ui::steps::Screen::StructFea
            );
            let mut text = String::new();
            for _ in 0..3 {
                let mut input = egui::RawInput::default();
                input.screen_rect = Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1600.0, 1000.0),
                ));
                let mut out = ctx.run_ui(input, |ui| app.frame_ui(ui));
                out.textures_delta.clear();
                text.clear();
                for cs in &out.shapes {
                    if let egui::epaint::Shape::Text(tx) = &cs.shape {
                        text.push_str(tx.galley.text());
                    }
                }
            }
            assert!(
                text.contains(t("structfea.title")),
                "{dt:?}: screen title not drawn"
            );
        }
    }
}
