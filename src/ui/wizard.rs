// SPDX-License-Identifier: AGPL-3.0-or-later
//! Sihirbaz akışı, adım validasyonu, ilerleme çubuğu ve dirty-tracking yönetimi.
use crate::calc::Model;
use crate::calc::geometry::TailKind;
use crate::i18n::{t, tf};
use crate::project::{DroneType, Project, WizardStepState};
use egui::{RichText, Stroke, Ui};

/// Sihirbaz adımlarının benzersiz tanımlayıcısı.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StepId {
    Mission,
    Weight,
    Geometry,
    Aero,
    DesignPoint,
    Endurance,
    Propulsion,
    Stability,
    Structure,
    Summary,
    Outputs,
    // Multirotor / VTOL'e özel adımlar
    MrFrame,
    MrPropulsion,
    MrBatteryLoiter,
    MrCG,
    MrStructure,
}

impl StepId {
    pub fn title(&self) -> &'static str {
        match self {
            StepId::Mission => t("wizard.step.mission.title"),
            StepId::Weight => t("wizard.step.weight.title"),
            StepId::Geometry => t("wizard.step.geometry.title"),
            StepId::Aero => t("wizard.step.aero.title"),
            StepId::DesignPoint => t("wizard.step.design_point.title"),
            StepId::Endurance => t("wizard.step.endurance.title"),
            StepId::Propulsion => t("wizard.step.propulsion.title"),
            StepId::Stability => t("wizard.step.stability.title"),
            StepId::Structure => t("wizard.step.structure.title"),
            StepId::Summary => t("wizard.step.summary.title"),
            StepId::Outputs => t("wizard.step.outputs.title"),
            StepId::MrFrame => t("wizard.step.mr_frame.title"),
            StepId::MrPropulsion => t("wizard.step.mr_propulsion.title"),
            StepId::MrBatteryLoiter => t("wizard.step.mr_battery_loiter.title"),
            StepId::MrCG => t("wizard.step.mr_cg.title"),
            StepId::MrStructure => t("wizard.step.mr_structure.title"),
        }
    }

    pub fn short_name(&self) -> &'static str {
        match self {
            StepId::Mission => t("wizard.step.mission.short"),
            StepId::Weight => t("wizard.step.weight.short"),
            StepId::Geometry => t("wizard.step.geometry.short"),
            StepId::Aero => t("wizard.step.aero.short"),
            StepId::DesignPoint => t("wizard.step.design_point.short"),
            StepId::Endurance => t("wizard.step.endurance.short"),
            StepId::Propulsion => t("wizard.step.propulsion.short"),
            StepId::Stability => t("wizard.step.stability.short"),
            StepId::Structure => t("wizard.step.structure.short"),
            StepId::Summary => t("wizard.step.summary.short"),
            StepId::Outputs => t("wizard.step.outputs.short"),
            StepId::MrFrame => t("wizard.step.mr_frame.short"),
            StepId::MrPropulsion => t("wizard.step.mr_propulsion.short"),
            StepId::MrBatteryLoiter => t("wizard.step.mr_battery_loiter.short"),
            StepId::MrCG => t("wizard.step.mr_cg.short"),
            StepId::MrStructure => t("wizard.step.mr_structure.short"),
        }
    }
}

/// Seçilen drone türüne göre sıralı adımları döndürür.
pub fn steps_for(dt: DroneType) -> Vec<StepId> {
    match dt {
        DroneType::Multirotor => vec![
            StepId::Mission,
            StepId::MrFrame,
            StepId::Weight,
            StepId::MrPropulsion,
            StepId::MrBatteryLoiter,
            StepId::MrCG,
            StepId::MrStructure,
            StepId::Summary,
            StepId::Outputs,
        ],
        DroneType::VtolQuadplane => vec![
            StepId::Mission,
            StepId::Geometry,
            StepId::MrFrame,
            StepId::Weight,
            StepId::Aero,
            StepId::DesignPoint,
            StepId::Propulsion,
            StepId::Endurance,
            StepId::Stability,
            StepId::Structure,
            StepId::MrStructure,
            StepId::Summary,
            StepId::Outputs,
        ],
        _ => vec![
            StepId::Mission,
            StepId::Weight,
            StepId::Geometry,
            StepId::Aero,
            StepId::DesignPoint,
            StepId::Endurance,
            StepId::Propulsion,
            StepId::Stability,
            StepId::Structure,
            StepId::Summary,
            StepId::Outputs,
        ],
    }
}

/// Her adım için zorunlu (öncelikli) parametre listesi (tek bir merkezden tanımlı).
pub fn step_required_params(step: StepId) -> Vec<&'static str> {
    match step {
        StepId::Mission => vec![t("wizard.req.altitude"), t("wizard.req.cruise_speed")],
        StepId::Weight => vec![
            t("wizard.req.payload"),
            t("wizard.req.avionics"),
            t("wizard.req.propulsion_mass"),
            t("wizard.req.fuse_tail_mass"),
            t("wizard.req.battery_sp"),
            t("wizard.req.pack_overhead"),
            t("wizard.req.mtow"),
        ],
        StepId::Geometry => vec![
            t("wizard.req.wingspan"),
            t("wizard.req.wing_area"),
            t("wizard.req.taper"),
            t("wizard.req.tc"),
            t("wizard.req.tail_geo"),
        ],
        StepId::Aero => vec![
            t("wizard.req.cla_2d"),
            t("wizard.req.clmax_2d"),
            t("wizard.req.swet_fuse"),
            t("wizard.req.cd0"),
        ],
        StepId::DesignPoint => vec![
            t("wizard.req.vstall_req"),
            t("wizard.req.climb_rate"),
            t("wizard.req.power_margin"),
        ],
        StepId::Endurance => vec![t("wizard.req.dod"), t("wizard.req.photel")],
        StepId::Propulsion => vec![
            t("wizard.req.prop_diam"),
            t("wizard.req.prop_pitch"),
            t("wizard.req.p_max"),
            t("wizard.req.tw_static"),
        ],
        StepId::Stability => vec![
            t("wizard.req.components"),
            t("wizard.req.sm_target"),
            t("wizard.req.sm_calc"),
        ],
        StepId::Structure => vec![
            t("wizard.req.n_limit"),
            t("wizard.req.fs"),
            t("wizard.req.sigma_allow"),
            t("wizard.req.spar_diam"),
        ],
        StepId::Summary => vec![t("wizard.req.summary_prev")],
        StepId::Outputs => vec![t("wizard.req.outputs_prev")],
        StepId::MrFrame => vec![
            t("wizard.req.mr_d_frame"),
            t("wizard.req.mr_arms"),
            t("wizard.req.prop_diam"),
            t("wizard.req.mr_prop_clear"),
        ],
        StepId::MrPropulsion => vec![
            t("wizard.req.mr_t_max"),
            t("wizard.req.mr_fm"),
            t("wizard.req.mr_eta_mot"),
            t("wizard.req.mr_eta_esc"),
        ],
        StepId::MrBatteryLoiter => vec![t("wizard.req.mr_batt_sp"), t("wizard.req.mr_v_cruise")],
        StepId::MrCG => vec![t("wizard.req.mr_cg_offset")],
        StepId::MrStructure => vec![
            t("wizard.req.mr_arm_section"),
            t("wizard.req.mr_arm_material"),
            t("wizard.req.mr_arm_n_limit"),
        ],
    }
}

/// Tüm adımları baştan sona kontrol ederek ilk geçersiz adımı döndürür.
pub fn first_invalid_step(p: &Project, m: &Model) -> Option<(usize, StepId, &'static str)> {
    let steps = steps_for(p.drone_type);
    for (i, &step) in steps.iter().enumerate() {
        if let Err(msg) = validate_step(step, p, m) {
            return Some((i, step, msg));
        }
    }
    None
}

/// Tüm zorunlu adımların geçerli olup olmadığını sorgular.
pub fn all_steps_valid(p: &Project, m: &Model) -> bool {
    first_invalid_step(p, m).is_none()
}

/// Bir adımın zorunlu girdilerinin geçerli olup olmadığını denetler.
pub fn validate_step(step: StepId, p: &Project, m: &Model) -> Result<(), &'static str> {
    match step {
        StepId::Mission => {
            if !p.h.is_finite() || p.h < 0.0 || p.h > 11000.0 {
                return Err(t("wizard.val.h_invalid"));
            }
            if !p.v_cr.is_finite() || p.v_cr < 5.0 || p.v_cr > 100.0 {
                return Err(t("wizard.val.v_cr_invalid"));
            }
            Ok(())
        }
        StepId::Weight => {
            if !p.m_payload.is_finite() || p.m_payload < 0.0 {
                return Err(t("wizard.val.payload_neg"));
            }
            if !p.m_avionics.is_finite() || p.m_avionics <= 0.0 {
                return Err(t("wizard.val.avionics_zero"));
            }
            if !p.m_propulsion.is_finite() || p.m_propulsion <= 0.0 {
                return Err(t("wizard.val.propulsion_zero"));
            }
            if !p.m_fuse_tail.is_finite() || p.m_fuse_tail < 0.0 {
                return Err(t("wizard.val.fuse_tail_neg"));
            }
            match p.battery_source {
                crate::calc::custom_battery::BatterySourceKind::Library => {
                    if p.n_series < 1 || p.n_parallel < 1 {
                        return Err(t("wizard.val.batt_sp_zero"));
                    }
                }
                crate::calc::custom_battery::BatterySourceKind::GenericTemplate => {
                    if p.n_series < 1 || p.n_parallel < 1 {
                        return Err(t("wizard.val.batt_sp_zero"));
                    }
                    if !p.generic_cell.nominal_v.is_finite()
                        || p.generic_cell.nominal_v <= 0.0
                        || !p.generic_cell.capacity_ah.is_finite()
                        || p.generic_cell.capacity_ah <= 0.0
                        || !p.generic_cell.mass_g.is_finite()
                        || p.generic_cell.mass_g <= 0.0
                    {
                        return Err(t("wizard.val.cell_data_invalid"));
                    }
                }
                crate::calc::custom_battery::BatterySourceKind::CustomCell => {
                    if p.n_series < 1 || p.n_parallel < 1 {
                        return Err(t("wizard.val.batt_sp_zero"));
                    }
                    if !p.custom_cell.nominal_v.is_finite()
                        || p.custom_cell.nominal_v <= 0.0
                        || !p.custom_cell.capacity_ah.is_finite()
                        || p.custom_cell.capacity_ah <= 0.0
                        || !p.custom_cell.mass_g.is_finite()
                        || p.custom_cell.mass_g <= 0.0
                    {
                        return Err(t("wizard.val.cell_data_invalid"));
                    }
                }
                crate::calc::custom_battery::BatterySourceKind::CustomPack => {
                    if !p.custom_pack.pack_voltage.is_finite()
                        || p.custom_pack.pack_voltage <= 0.0
                        || !p.custom_pack.capacity_mah.is_finite()
                        || p.custom_pack.capacity_mah <= 0.0
                        || !p.custom_pack.pack_mass_g.is_finite()
                        || p.custom_pack.pack_mass_g <= 0.0
                    {
                        return Err(t("wizard.val.pack_data_invalid"));
                    }
                }
            }
            if !p.pack_overhead.is_finite() || p.pack_overhead < 1.0 {
                return Err(t("wizard.val.pack_overhead_low"));
            }
            if !m.weight.m_mtow.is_finite() || m.weight.m_mtow <= 0.0 {
                return Err(t("wizard.val.mtow_zero"));
            }
            Ok(())
        }
        StepId::Geometry => {
            if !p.b.is_finite() || p.b <= 0.0 {
                return Err(t("wizard.val.b_zero"));
            }
            if !p.s.is_finite() || p.s <= 0.0 {
                return Err(t("wizard.val.s_zero"));
            }
            if !p.taper.is_finite() || p.taper <= 0.0 || p.taper > 1.0 {
                return Err(t("wizard.val.taper_range"));
            }
            if !p.tc_w.is_finite() || p.tc_w <= 0.0 || p.tc_w > 0.30 {
                return Err(t("wizard.val.tc_range"));
            }
            match p.tail_kind {
                TailKind::Conventional => {
                    if p.s_h <= 0.0 || p.s_v <= 0.0 || p.l_h <= 0.0 || p.l_v <= 0.0 {
                        return Err(t("wizard.val.tail_geo_zero"));
                    }
                }
                TailKind::VTail => {
                    if p.s_vtail <= 0.0 || p.gamma_v_deg <= 0.0 || p.l_h <= 0.0 {
                        return Err(t("wizard.val.vtail_zero"));
                    }
                }
                TailKind::FlyingWing => {}
            }
            Ok(())
        }
        StepId::Aero => {
            if !p.cl_alpha_2d.is_finite() || p.cl_alpha_2d <= 0.0 {
                return Err(t("wizard.val.cla_2d_zero"));
            }
            if !p.cl_max_2d.is_finite() || p.cl_max_2d <= 0.0 {
                return Err(t("wizard.val.clmax_2d_zero"));
            }
            if !p.s_wet_fuse.is_finite() || p.s_wet_fuse <= 0.0 {
                return Err(t("wizard.val.swet_zero"));
            }
            if !m.aero.cd0.is_finite() || m.aero.cd0 <= 0.0 {
                return Err(t("wizard.val.cd0_nan"));
            }
            Ok(())
        }
        StepId::DesignPoint => {
            if !p.v_stall_req.is_finite() || p.v_stall_req <= 0.0 {
                return Err(t("wizard.val.vstall_zero"));
            }
            if !p.rc.is_finite() || p.rc < 0.0 {
                return Err(t("wizard.val.rc_neg"));
            }
            if !p.pw_margin.is_finite() || p.pw_margin < 1.0 {
                return Err(t("wizard.val.pw_margin_low"));
            }
            Ok(())
        }
        StepId::Endurance => {
            if !p.dod.is_finite() || p.dod <= 0.0 || p.dod > 1.0 {
                return Err(t("wizard.val.dod_range"));
            }
            if !p.p_hotel.is_finite() || p.p_hotel < 0.0 {
                return Err(t("wizard.val.photel_neg"));
            }
            Ok(())
        }
        StepId::Propulsion => {
            if !p.d_prop.is_finite() || p.d_prop <= 0.0 {
                return Err(t("wizard.val.dprop_zero"));
            }
            if !p.prop_pitch.is_finite() || p.prop_pitch <= 0.0 {
                return Err(t("wizard.val.pitch_zero"));
            }
            if !p.p_max_elec.is_finite() || p.p_max_elec <= 0.0 {
                return Err(t("wizard.val.pmax_zero"));
            }
            if !p.tw_static.is_finite() || p.tw_static <= 0.0 {
                return Err(t("wizard.val.tw_zero"));
            }
            Ok(())
        }
        StepId::Stability => {
            if p.components.is_empty() {
                return Err(t("wizard.val.comps_empty"));
            }
            if !p.sm_target.is_finite() {
                return Err(t("wizard.val.sm_target_nan"));
            }
            if !m.stab.sm.is_finite() {
                return Err(t("wizard.val.sm_nan"));
            }
            Ok(())
        }
        StepId::Structure => {
            if !p.n_limit.is_finite() || p.n_limit <= 0.0 {
                return Err(t("wizard.val.nlimit_zero"));
            }
            if !p.fs.is_finite() || p.fs < 1.0 {
                return Err(t("wizard.val.fs_low"));
            }
            if !p.sigma_allow.is_finite() || p.sigma_allow <= 0.0 {
                return Err(t("wizard.val.sigma_zero"));
            }
            if !p.d_spar_o.is_finite() || p.d_spar_o <= 0.0 || p.d_spar_i >= p.d_spar_o {
                return Err(t("wizard.val.spar_diam_invalid"));
            }
            Ok(())
        }
        StepId::Summary | StepId::Outputs => Ok(()),

        // Multirotor Adımları
        StepId::MrFrame => {
            if !p.mr_frame_size.is_finite() || p.mr_frame_size <= 0.0 {
                return Err(t("wizard.val.dframe_zero"));
            }
            if p.mr_arm_count != 3
                && p.mr_arm_count != 4
                && p.mr_arm_count != 6
                && p.mr_arm_count != 8
            {
                return Err(t("wizard.val.arms_low"));
            }
            if !p.mr_d_prop.is_finite() || p.mr_d_prop <= 0.0 {
                return Err(t("wizard.val.mr_dprop_zero"));
            }
            if !p.mr_prop_clearance.is_finite() || p.mr_prop_clearance < 0.0 || m.mr.prop_overlap {
                return Err(t("wizard.val.prop_overlap"));
            }
            Ok(())
        }
        StepId::MrPropulsion => {
            if !p.mr_t_max_motor.is_finite() || p.mr_t_max_motor <= 0.0 {
                return Err(t("wizard.val.tmax_zero"));
            }
            if !p.fm.is_finite() || p.fm <= 0.0 || p.fm > 1.0 {
                return Err(t("wizard.val.fm_range"));
            }
            if !p.eta_mot.is_finite() || p.eta_mot <= 0.0 || p.eta_mot > 1.0 {
                return Err(t("wizard.val.eta_mot_range"));
            }
            if !p.eta_esc.is_finite() || p.eta_esc <= 0.0 || p.eta_esc > 1.0 {
                return Err(t("wizard.val.eta_esc_range"));
            }
            Ok(())
        }
        StepId::MrBatteryLoiter => {
            if p.n_series < 1 || p.n_parallel < 1 {
                return Err(t("wizard.val.mr_batt_sp"));
            }
            if !p.mr_v_cruise.is_finite() || p.mr_v_cruise <= 0.0 {
                return Err(t("wizard.val.mr_vcruise_zero"));
            }
            Ok(())
        }
        StepId::MrCG => {
            if !m.mr.cg_offset_d.is_finite() {
                return Err(t("wizard.val.cg_offset_nan"));
            }
            Ok(())
        }
        StepId::MrStructure => {
            if !p.mr_arm_d_o.is_finite()
                || p.mr_arm_d_o <= 0.0
                || p.mr_arm_d_i >= p.mr_arm_d_o
                || p.mr_arm_d_i < 0.0
            {
                return Err(t("wizard.val.mr_arm_diam_invalid"));
            }
            if !p.mr_arm_sigma_allow.is_finite() || p.mr_arm_sigma_allow <= 0.0 {
                return Err(t("wizard.val.mr_arm_sigma_zero"));
            }
            if !p.mr_arm_e_modulus.is_finite() || p.mr_arm_e_modulus <= 0.0 {
                return Err(t("wizard.val.mr_arm_e_zero"));
            }
            if !p.mr_arm_n_limit.is_finite() || p.mr_arm_n_limit <= 0.0 {
                return Err(t("wizard.val.mr_arm_nlimit_zero"));
            }
            if !p.mr_arm_fs.is_finite() || p.mr_arm_fs < 1.0 {
                return Err(t("wizard.val.mr_arm_fs_low"));
            }
            if !m.mr_st.ms.is_finite() {
                return Err(t("wizard.val.mr_arm_ms_nan"));
            }
            Ok(())
        }
    }
}

/// Sihirbaz adımı için erişilebilir azami indeks (katı adım kilidi sınırı).
pub fn max_accessible_step_idx(p: &Project, m: &Model) -> usize {
    let steps = steps_for(p.drone_type);
    match first_invalid_step(p, m) {
        Some((inv_idx, _, _)) => inv_idx,
        None => steps.len().saturating_sub(1),
    }
}

/// Sihirbaz adımını erişilebilir azami indekse sıkıştırır.
pub fn clamp_wizard_step(p: &mut Project, m: &Model) {
    let max_idx = max_accessible_step_idx(p, m);
    if p.wizard_step > max_idx {
        p.wizard_step = max_idx;
    }
}

/// Uzman modu kilidi kontrolü:
/// Adımlardan biri geçersizse uzman modu kapatılır ve kullanıcı ilk geçersiz adıma yönlendirilir.
pub fn enforce_expert_gate(p: &mut Project, m: &Model) -> Option<(usize, StepId, &'static str)> {
    if p.expert_mode {
        if let Some((inv_idx, inv_step, err)) = first_invalid_step(p, m) {
            p.expert_mode = false;
            p.wizard_step = inv_idx;
            return Some((inv_idx, inv_step, err));
        }
    }
    None
}

/// Sihirbaz durum senkronizasyonu: adımların boyutunu doğrular ve adımı kilit sınırına sıkıştırır.
pub fn sync_wizard_steps(p: &mut Project, m: &Model) {
    let steps = steps_for(p.drone_type);
    if p.step_states.len() != steps.len() {
        p.step_states
            .resize(steps.len(), WizardStepState::NotVisited);
    }
    clamp_wizard_step(p, m);
}

/// Sihirbaz başlık çubuğu: İlerleme göstergesi, adımlar, İleri / Geri düğmeleri ve hata mesajı.
pub fn render_wizard_header(ui: &mut Ui, p: &mut Project, m: &Model) -> Option<StepId> {
    sync_wizard_steps(p, m);
    let steps = steps_for(p.drone_type);
    let total = steps.len();
    let max_accessible_idx = max_accessible_step_idx(p, m);
    p.wizard_step = p.wizard_step.min(max_accessible_idx);
    let current_idx = p.wizard_step.min(total.saturating_sub(1));
    let current_step = steps[current_idx];

    // İlk geçersiz adım tespiti (katı adım kilidi)
    let first_inv = first_invalid_step(p, m);

    // Uzman modu kilidi
    enforce_expert_gate(p, m);
    let can_expert = first_inv.is_none();

    // Mevcut adımın validasyonu
    let val_result = validate_step(current_step, p, m);
    let is_valid = val_result.is_ok();

    let mut teach_clicked = false;
    let tk = crate::ui::theme::tokens(ui);

    ui.horizontal(|ui| {
        // Sol: Adım Numarası ve İlerleme
        ui.strong(
            RichText::new(format!(
                "{} {} / {}: {}",
                t("step_counter"),
                current_idx + 1,
                total,
                current_step.title()
            ))
            .size(15.0)
            .color(tk.accent),
        );

        let progress = (current_idx as f32 + 1.0) / total as f32;
        ui.add(egui::ProgressBar::new(progress).desired_width(140.0));

        let teach_btn = egui::Button::new(
            RichText::new(t("teach.btn_teach_me"))
                .strong()
                .color(tk.accent),
        );
        if ui.add(teach_btn).clicked() {
            teach_clicked = true;
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // İleri Düğmesi: yalnızca geçerliyse ve kilitli olmayan bir sonraki adım varsa aktif
            let next_btn = egui::Button::new(t("btn_next"));
            let next_enabled =
                is_valid && current_idx < max_accessible_idx && current_idx < total - 1;
            if ui.add_enabled(next_enabled, next_btn).clicked() {
                p.step_states[current_idx] = WizardStepState::Completed;
                p.wizard_step = current_idx + 1;
            }

            // Geri Düğmesi
            let back_btn = egui::Button::new(t("btn_back"));
            if ui.add_enabled(current_idx > 0, back_btn).clicked() {
                p.wizard_step = current_idx.saturating_sub(1);
            }

            // Uzman Modu Geçişi (yalnızca tüm adımlar eksiksiz geçerliyse açılabilir)
            ui.separator();
            let expert_cb = egui::Checkbox::new(&mut p.expert_mode, t("expert_mode"));
            let resp = ui.add_enabled(can_expert, expert_cb);
            if !can_expert {
                if let Some((_, inv_step, err)) = first_inv {
                    resp.on_disabled_hover_text(tf(
                        "wizard.expert_mode_locked",
                        &[inv_step.title(), err],
                    ));
                }
            }
        });
    });

    // İleri gidilemiyorsa kırmızı hata mesajı
    if let Err(reason) = val_result {
        ui.add_space(2.0);
        ui.colored_label(
            tk.danger,
            format!("[!] {} — {}", t("cannot_advance"), reason),
        );
    }

    ui.add_space(3.0);

    // Adım çipleri (Breadcrumbs)
    ui.horizontal_wrapped(|ui| {
        for (idx, step) in steps.iter().enumerate() {
            let is_current = idx == current_idx;
            let is_locked = idx > max_accessible_idx;
            let state = p
                .step_states
                .get(idx)
                .copied()
                .unwrap_or(WizardStepState::NotVisited);

            if is_locked {
                // Kilitli adım: Gri, kilit simgesi, tıklanamaz, üzerine gelince açıklama
                let label = format!("[#] {} {}", idx + 1, step.short_name());
                let chip_btn =
                    egui::Button::new(RichText::new(label).color(tk.text_weak).size(12.0))
                        .fill(tk.surface)
                        .stroke(Stroke::new(1.0, tk.border));

                let resp = ui.add_enabled(false, chip_btn);
                if let Some((_, inv_step, err)) = first_inv {
                    resp.on_disabled_hover_text(tf(
                        "wizard.locked_step_tooltip",
                        &[inv_step.title(), err],
                    ));
                }
            } else {
                let (label, bg_col, text_col, border_col) = match (is_current, state) {
                    (true, _) => (
                        format!("> {} {}", idx + 1, step.short_name()),
                        tk.accent_soft,
                        tk.text,
                        tk.accent,
                    ),
                    (false, WizardStepState::Completed) => (
                        format!("• {} {}", idx + 1, step.short_name()),
                        tk.success_soft,
                        tk.success,
                        tk.success,
                    ),
                    (false, WizardStepState::NeedsUpdate) => (
                        format!("[!] {} {}", idx + 1, step.short_name()),
                        tk.warning_soft,
                        tk.warning,
                        tk.warning,
                    ),
                    (false, WizardStepState::NotVisited) => (
                        format!("{} {}", idx + 1, step.short_name()),
                        tk.surface_alt,
                        tk.text_weak,
                        tk.border,
                    ),
                };

                let chip_btn = egui::Button::new(RichText::new(label).color(text_col).size(12.0))
                    .fill(bg_col)
                    .stroke(Stroke::new(if is_current { 1.5 } else { 1.0 }, border_col));

                // Erişilebilir adımlara tıklanabilir
                let can_click =
                    is_current || state != WizardStepState::NotVisited || idx <= current_idx + 1;
                if ui.add_enabled(can_click, chip_btn).clicked() {
                    p.wizard_step = idx;
                }
            }
        }
    });

    ui.add_space(4.0);

    // Her adımın başında t("wizard.card_req_title") listesi gösterimi
    render_step_requirements(ui, current_step);

    ui.separator();

    if teach_clicked {
        Some(current_step)
    } else {
        None
    }
}

/// Her adımın başında gösterilen zorunlu gereksinimler kartı.
pub fn render_step_requirements(ui: &mut Ui, step: StepId) {
    let reqs = step_required_params(step);
    if reqs.is_empty() {
        return;
    }
    let tk = crate::ui::theme::tokens(ui);
    crate::ui::panel_kit::surface_frame(tk).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.strong(
                RichText::new(t("wizard.card_req_header"))
                    .color(tk.accent)
                    .size(12.0),
            );
        });
        for req in reqs {
            ui.label(RichText::new(format!("• {req}")).size(11.0).color(tk.text));
        }
    });
    ui.add_space(4.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wizard_step_sequence_per_drone_type() {
        // Sabit kanat: 10 adım, Özet ile biter
        for dt in [
            DroneType::FixedWingConventional,
            DroneType::FixedWingVTail,
            DroneType::FlyingWing,
        ] {
            let steps = steps_for(dt);
            assert_eq!(steps.len(), 11, "{:?} için adım sayısı 11 olmalı", dt);
            assert_eq!(steps[0], StepId::Mission);
            assert_eq!(steps[1], StepId::Weight);
            assert_eq!(steps[2], StepId::Geometry);
            assert_eq!(steps[3], StepId::Aero);
            assert_eq!(steps[4], StepId::DesignPoint);
            assert_eq!(steps[5], StepId::Endurance);
            assert_eq!(steps[6], StepId::Propulsion);
            assert_eq!(steps[7], StepId::Stability);
            assert_eq!(steps[8], StepId::Structure);
            assert_eq!(steps[9], StepId::Summary);
            assert_eq!(steps[10], StepId::Outputs);
        }

        // Multirotor: 9 adım, Çıktılar ile biter
        let mr_steps = steps_for(DroneType::Multirotor);
        assert_eq!(mr_steps.len(), 9);
        assert_eq!(mr_steps[0], StepId::Mission);
        assert_eq!(mr_steps[1], StepId::MrFrame);
        assert_eq!(mr_steps[2], StepId::Weight);
        assert_eq!(mr_steps[3], StepId::MrPropulsion);
        assert_eq!(mr_steps[4], StepId::MrBatteryLoiter);
        assert_eq!(mr_steps[5], StepId::MrCG);
        assert_eq!(mr_steps[6], StepId::MrStructure);
        assert_eq!(mr_steps[7], StepId::Summary);
        assert_eq!(mr_steps[8], StepId::Outputs);

        // VTOL: 13 adım
        let vtol_steps = steps_for(DroneType::VtolQuadplane);
        assert_eq!(vtol_steps.len(), 13);
        assert_eq!(vtol_steps[9], StepId::Structure);
        assert_eq!(vtol_steps[10], StepId::MrStructure);
        assert_eq!(vtol_steps[11], StepId::Summary);
        assert_eq!(vtol_steps[12], StepId::Outputs);
    }

    #[test]
    fn test_default_project_all_steps_valid() {
        let p = Project::default();
        let lib = crate::calc::Library::embedded();
        let m = crate::calc::evaluate(&p, &lib);

        let steps = steps_for(p.drone_type);
        for s in steps {
            let res = validate_step(s, &p, &m);
            assert!(
                res.is_ok(),
                "Adım {:?} varsayılan projede geçerli olmalı, hata: {:?}",
                s,
                res.err()
            );
        }

        // Multirotor varsayılanları da geçerli olmalı
        let mut p_mr = Project::default();
        p_mr.set_drone_type(DroneType::Multirotor);
        let m_mr = crate::calc::evaluate(&p_mr, &lib);
        for s in steps_for(DroneType::Multirotor) {
            let res = validate_step(s, &p_mr, &m_mr);
            assert!(
                res.is_ok(),
                "Multirotor adım {:?} geçerli olmalı, hata: {:?}",
                s,
                res.err()
            );
        }
    }

    #[test]
    fn test_invalid_inputs_prevent_advancing() {
        let mut p = Project::default();
        let lib = crate::calc::Library::embedded();
        let m = crate::calc::evaluate(&p, &lib);

        // 1. Görev geçersiz hız
        p.v_cr = 0.0;
        assert!(validate_step(StepId::Mission, &p, &m).is_err());
        p.v_cr = f64::NAN;
        assert!(validate_step(StepId::Mission, &p, &m).is_err());
        p.v_cr = 18.0;

        // 2. Ağırlık geçersiz batarya
        p.n_series = 0;
        assert!(validate_step(StepId::Weight, &p, &m).is_err());
        p.n_series = 4;

        // 3. Geometri kanat alanı sıfır
        p.s = 0.0;
        assert!(validate_step(StepId::Geometry, &p, &m).is_err());
        p.s = -0.5;
        assert!(validate_step(StepId::Geometry, &p, &m).is_err());
        p.s = 0.32;

        // 4. Yapı boru iç çapı dış çaptan büyük
        p.d_spar_i = p.d_spar_o + 0.005;
        assert!(validate_step(StepId::Structure, &p, &m).is_err());
        p.d_spar_i = 0.008;

        // 5. Multirotor çerçeve boyutu sıfır
        p.mr_frame_size = 0.0;
        assert!(validate_step(StepId::MrFrame, &p, &m).is_err());
    }

    #[test]
    fn test_dirty_tracking_marks_needs_update() {
        let mut p = Project::default();
        let lib = crate::calc::Library::embedded();
        let m = crate::calc::evaluate(&p, &lib);
        sync_wizard_steps(&mut p, &m);

        // 3 adım tamamlanmış olsun
        p.step_states[0] = WizardStepState::Completed;
        p.step_states[1] = WizardStepState::Completed;
        p.step_states[2] = WizardStepState::Completed;

        // Kullanıcı 0. adıma geri dönüp değişiklik yaparsa > 0 adımlar NeedsUpdate olmalı
        let current_step = 0;
        for i in (current_step + 1)..p.step_states.len() {
            if p.step_states[i] == WizardStepState::Completed {
                p.step_states[i] = WizardStepState::NeedsUpdate;
            }
        }

        assert_eq!(p.step_states[0], WizardStepState::Completed);
        assert_eq!(p.step_states[1], WizardStepState::NeedsUpdate);
        assert_eq!(p.step_states[2], WizardStepState::NeedsUpdate);

        // 1. adıma geçip doğrulanıp ileri denince tekrar Completed olmalı
        p.step_states[1] = WizardStepState::Completed;
        assert_eq!(p.step_states[1], WizardStepState::Completed);
        // 2. adım hala NeedsUpdate kalmalı
        assert_eq!(p.step_states[2], WizardStepState::NeedsUpdate);
    }

    #[test]
    fn test_strict_step_locking_prevents_opening_later_steps() {
        let mut p = Project::default();
        let lib = crate::calc::Library::embedded();
        let m = crate::calc::evaluate(&p, &lib);

        // Varsayılanda kilit yok: tüm adımlar geçerli
        assert!(all_steps_valid(&p, &m));
        assert!(first_invalid_step(&p, &m).is_none());

        // 1. adımdaki (Weight) m_avionics sıfır yapılırsa:
        p.m_avionics = 0.0;
        let m_inv = crate::calc::evaluate(&p, &lib);
        let first = first_invalid_step(&p, &m_inv);
        assert!(first.is_some());
        let (inv_idx, inv_step, _) = first.unwrap();
        assert_eq!(inv_idx, 1);
        assert_eq!(inv_step, StepId::Weight);

        // Adım 1'den sonraki adımlar kilitli olmalı (max_accessible_idx = 1)
        let total = steps_for(p.drone_type).len();
        for idx in 2..total {
            assert!(idx > inv_idx, "Adım {} kilitli olmalı", idx);
        }
    }

    #[test]
    fn test_expert_mode_lock_and_unlock() {
        let mut p = Project::default();
        let lib = crate::calc::Library::embedded();
        let m = crate::calc::evaluate(&p, &lib);

        // Tüm adımlar geçerliyken uzman modu açılabilir
        assert!(all_steps_valid(&p, &m));
        p.expert_mode = true;

        // Önceki bir adımdaki (Geometry) kanat açıklığı bozulursa:
        p.b = 0.0;
        let m_inv = crate::calc::evaluate(&p, &lib);
        assert!(!all_steps_valid(&p, &m_inv));

        // first_invalid_step Geometry'yi bulmalı ve uzman modu kilitlenmeli
        let first = first_invalid_step(&p, &m_inv);
        assert!(first.is_some());
        let (first_idx, step, _) = first.unwrap();
        assert_eq!(step, StepId::Geometry);

        // enforce_expert_gate saf fonksiyonu uzman modunu kapatmalı ve geçersiz adıma düşürmeli
        let gate_res = enforce_expert_gate(&mut p, &m_inv);
        assert!(gate_res.is_some());
        let (revert_idx, revert_step, _) = gate_res.unwrap();
        assert_eq!(revert_step, StepId::Geometry);
        assert_eq!(revert_idx, first_idx);
        assert_eq!(p.wizard_step, revert_idx);
        assert!(
            !p.expert_mode,
            "Geçersiz parametre olduğunda uzman modu zorunlu kapanmalıdır"
        );
    }

    #[test]
    fn test_step_required_params_non_empty() {
        // Her adımın en az 1 zorunlu parametre kuralı olmalı
        for step in [
            StepId::Mission,
            StepId::Weight,
            StepId::Geometry,
            StepId::Aero,
            StepId::DesignPoint,
            StepId::Endurance,
            StepId::Propulsion,
            StepId::Stability,
            StepId::Structure,
            StepId::Summary,
            StepId::Outputs,
            StepId::MrFrame,
            StepId::MrPropulsion,
            StepId::MrBatteryLoiter,
            StepId::MrCG,
        ] {
            let reqs = step_required_params(step);
            assert!(
                !reqs.is_empty(),
                "Adım {:?} için zorunlu parametre listesi boş olamaz",
                step
            );
        }
    }

    #[test]
    fn test_all_step_required_params_checked_by_validation() {
        // Her sihirbaz adımının validate_step fonksiyonunun, step_required_params
        // listesindeki TÜM parametreleri eksiksiz denetlediğini otomatik doğrular.
        let lib = crate::calc::Library::embedded();

        let steps_to_test = [
            StepId::Mission,
            StepId::Weight,
            StepId::Geometry,
            StepId::Aero,
            StepId::DesignPoint,
            StepId::Endurance,
            StepId::Propulsion,
            StepId::Stability,
            StepId::Structure,
            StepId::MrFrame,
            StepId::MrPropulsion,
            StepId::MrBatteryLoiter,
            StepId::MrCG,
        ];

        for &step in &steps_to_test {
            let reqs = step_required_params(step);

            for &param in &reqs {
                let mut p = Project::default();
                p.set_drone_type(match step {
                    StepId::MrFrame
                    | StepId::MrPropulsion
                    | StepId::MrBatteryLoiter
                    | StepId::MrCG => DroneType::Multirotor,
                    _ => DroneType::FixedWingConventional,
                });
                let mut m = crate::calc::evaluate(&p, &lib);

                // Temel durum geçerli olmalı
                assert!(
                    validate_step(step, &p, &m).is_ok(),
                    "Adım {:?} temel durumda geçerli olmalıdır",
                    step
                );

                // İlgili parametreyi geçersiz kıl
                let mut invalidated = true;
                if param == t("wizard.req.altitude") {
                    p.h = -10.0;
                } else if param == t("wizard.req.cruise_speed") {
                    p.v_cr = 0.0;
                } else if param == t("wizard.req.payload") {
                    p.m_payload = -1.0;
                } else if param == t("wizard.req.avionics") {
                    p.m_avionics = 0.0;
                } else if param == t("wizard.req.propulsion_mass") {
                    p.m_propulsion = 0.0;
                } else if param == t("wizard.req.fuse_tail_mass") {
                    p.m_fuse_tail = -1.0;
                } else if param == t("wizard.req.battery_sp") {
                    p.n_series = 0;
                } else if param == t("wizard.req.pack_overhead") {
                    p.pack_overhead = 0.5;
                } else if param == t("wizard.req.mtow") {
                    m.weight.m_mtow = 0.0;
                } else if param == t("wizard.req.wingspan") {
                    p.b = 0.0;
                } else if param == t("wizard.req.wing_area") {
                    p.s = 0.0;
                } else if param == t("wizard.req.taper") {
                    p.taper = 0.0;
                } else if param == t("wizard.req.tc") {
                    p.tc_w = 0.0;
                } else if param == t("wizard.req.tail_geo") {
                    p.s_h = 0.0;
                } else if param == t("wizard.req.cla_2d") {
                    p.cl_alpha_2d = 0.0;
                } else if param == t("wizard.req.clmax_2d") {
                    p.cl_max_2d = 0.0;
                } else if param == t("wizard.req.swet_fuse") {
                    p.s_wet_fuse = 0.0;
                } else if param == t("wizard.req.cd0") {
                    m.aero.cd0 = 0.0;
                } else if param == t("wizard.req.vstall_req") {
                    p.v_stall_req = 0.0;
                } else if param == t("wizard.req.climb_rate") {
                    p.rc = -1.0;
                } else if param == t("wizard.req.power_margin") {
                    p.pw_margin = 0.5;
                } else if param == t("wizard.req.dod") {
                    p.dod = 0.0;
                } else if param == t("wizard.req.photel") {
                    p.p_hotel = -1.0;
                } else if param == t("wizard.req.prop_diam") {
                    if step == StepId::MrFrame {
                        p.mr_d_prop = 0.0;
                    } else {
                        p.d_prop = 0.0;
                    }
                } else if param == t("wizard.req.prop_pitch") {
                    p.prop_pitch = 0.0;
                } else if param == t("wizard.req.p_max") {
                    p.p_max_elec = 0.0;
                } else if param == t("wizard.req.tw_static") {
                    p.tw_static = 0.0;
                } else if param == t("wizard.req.components") {
                    p.components.clear();
                } else if param == t("wizard.req.sm_target") {
                    p.sm_target = f64::NAN;
                } else if param == t("wizard.req.sm_calc") {
                    m.stab.sm = f64::NAN;
                } else if param == t("wizard.req.n_limit") {
                    p.n_limit = 0.0;
                } else if param == t("wizard.req.fs") {
                    p.fs = 0.5;
                } else if param == t("wizard.req.sigma_allow") {
                    p.sigma_allow = 0.0;
                } else if param == t("wizard.req.spar_diam") {
                    p.d_spar_i = p.d_spar_o + 0.01;
                } else if param == t("wizard.req.mr_d_frame") {
                    p.mr_frame_size = 0.0;
                } else if param == t("wizard.req.mr_arms") {
                    p.mr_arm_count = 5;
                } else if param == t("wizard.req.mr_prop_clear") {
                    m.mr.prop_overlap = true;
                } else if param == t("wizard.req.mr_t_max") {
                    p.mr_t_max_motor = 0.0;
                } else if param == t("wizard.req.mr_fm") {
                    p.fm = 0.0;
                } else if param == t("wizard.req.mr_eta_mot") {
                    p.eta_mot = 0.0;
                } else if param == t("wizard.req.mr_eta_esc") {
                    p.eta_esc = 0.0;
                } else if param == t("wizard.req.mr_batt_sp") {
                    p.n_series = 0;
                } else if param == t("wizard.req.mr_v_cruise") {
                    p.mr_v_cruise = 0.0;
                } else if param == t("wizard.req.mr_cg_offset") {
                    m.mr.cg_offset_d = f64::NAN;
                } else {
                    invalidated = false;
                }

                assert!(
                    invalidated,
                    "Zorunlu parametre '{}' için test invalidasyonu tanımlanmamış!",
                    param
                );
                assert!(
                    validate_step(step, &p, &m).is_err(),
                    "Adım {:?} içindeki '{}' parametresi geçersiz yapıldığında validate_step hata vermelidir!",
                    step,
                    param
                );
            }
        }
    }
}
