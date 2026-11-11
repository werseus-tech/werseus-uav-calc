// SPDX-License-Identifier: AGPL-3.0-or-later
//! Her adım ve sekme için akademik kaynaklı, Unicode formüller ve kullanıcının anlık değerleriyle sayısal hesaplamalar.
use crate::calc::{Model, performance};
use crate::project::{DroneType, Project};
use crate::ui::panel_kit::surface_frame;
use crate::ui::theme::{self, font, space};
use egui::{RichText, Ui};

pub struct FormulaDoc {
    pub name: &'static str,
    pub formula: &'static str,
    pub symbols: Vec<&'static str>,
    pub source: &'static str,
    pub evaluation: String,
}

pub fn render_formulas_section(ui: &mut Ui, id_salt: &str, title: &str, formulas: &[FormulaDoc]) {
    let tk = theme::tokens(ui);
    ui.add_space(space::S);
    let header = crate::ui::step_ui::short_header(ui, &format!("• {title}"), tk.accent, true);
    egui::CollapsingHeader::new(header)
        .id_salt(id_salt)
        .default_open(false)
        .show(ui, |ui| {
            ui.add_space(space::XS);
            for f in formulas {
                surface_frame(tk).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.add(
                        egui::Label::new(RichText::new(f.name).strong().color(tk.accent)).wrap(),
                    );
                    ui.add(
                        egui::Label::new(RichText::new(f.source).small().color(tk.text_weak))
                            .wrap(),
                    );
                    ui.add_space(space::XS / 2.0);
                    ui.add(
                        egui::Label::new(
                            RichText::new(f.formula)
                                .monospace()
                                .strong()
                                .size(font::BODY)
                                .color(tk.text),
                        )
                        .wrap(),
                    );
                    ui.add_space(space::XS);
                    ui.add(
                        egui::Label::new(
                            RichText::new(format!(
                                "{}: {}",
                                crate::i18n::t("formula.numeric_eval"),
                                f.evaluation
                            ))
                            .strong()
                            .color(tk.success),
                        )
                        .wrap(),
                    );
                    ui.add_space(space::XS);
                    for s in &f.symbols {
                        ui.add(
                            egui::Label::new(
                                RichText::new(format!("• {s}")).small().color(tk.text_weak),
                            )
                            .wrap(),
                        );
                    }
                });
                ui.add_space(space::XS);
            }
        });
}

/// Görev & Atmosfer Formülleri
pub fn show_mission_formulas(ui: &mut Ui, p: &Project, m: &Model) {
    let a = &m.atmo;
    let formulas = [
        FormulaDoc {
            name: crate::i18n::t("formula.f01.name"),
            formula: "T = T₀ − L · h",
            symbols: vec![
                crate::i18n::t("formula.f01.sym.0"),
                crate::i18n::t("formula.f01.sym.1"),
                crate::i18n::t("formula.f01.sym.2"),
                crate::i18n::t("formula.f01.sym.3"),
            ],
            source: crate::i18n::t("formula.f01.source"),
            evaluation: format!(
                "T = 288.15 − 0.0065 · {:.0} = {:.1} K ({:.1} °C)",
                p.h,
                a.t,
                a.t - 273.15
            ),
        },
        FormulaDoc {
            name: crate::i18n::t("formula.f02.name"),
            formula: "p = p₀ · (1 − L·h / T₀)^(g / (R·L))",
            symbols: vec![
                crate::i18n::t("formula.f02.sym.0"),
                crate::i18n::t("formula.f02.sym.1"),
                crate::i18n::t("formula.f02.sym.2"),
            ],
            source: crate::i18n::t("formula.f02.source"),
            evaluation: format!(
                "p = 101325 · (1 − 0.0065 · {:.0} / 288.15)^5.256 = {:.0} Pa",
                p.h, a.p
            ),
        },
        FormulaDoc {
            name: crate::i18n::t("formula.f03.name"),
            formula: "ρ = p / (R · T)",
            symbols: vec![
                crate::i18n::t("formula.f03.sym.0"),
                crate::i18n::t("formula.f03.sym.1"),
            ],
            source: crate::i18n::t("formula.f03.source"),
            evaluation: format!(
                "ρ = {:.0} / (287.05 · {:.1}) = {:.4} kg/m³",
                a.p, a.t, a.rho
            ),
        },
        FormulaDoc {
            name: crate::i18n::t("formula.f04.name"),
            formula: "μ = μ₀ · (T/T₀)^1.5 · (T₀ + S_μ) / (T + S_μ)",
            symbols: vec![
                crate::i18n::t("formula.f04.sym.0"),
                crate::i18n::t("formula.f04.sym.1"),
            ],
            source: crate::i18n::t("formula.f04.source"),
            evaluation: format!(
                "μ = 1.7894e-5 · ({:.1}/288.15)^1.5 · 398.55 / ({:.1} + 110.4) = {:.3e} Pa·s",
                a.t, a.t, a.mu
            ),
        },
        FormulaDoc {
            name: crate::i18n::t("formula.f05.name"),
            formula: "a = √(γ · R · T)  ,  M = V / a",
            symbols: vec![
                crate::i18n::t("formula.f05.sym.0"),
                crate::i18n::t("formula.f05.sym.1"),
            ],
            source: crate::i18n::t("formula.f05.source"),
            evaluation: format!(
                "a = √(1.4 · 287.05 · {:.1}) = {:.1} m/s | M = {:.1} / {:.1} = {:.3}",
                a.t, a.a, p.v_cr, a.a, m.aero.mach
            ),
        },
        FormulaDoc {
            name: crate::i18n::t("formula.f06.name"),
            formula: "Re = (ρ · V · c) / μ",
            symbols: vec![
                crate::i18n::t("formula.f06.sym.0"),
                crate::i18n::t("formula.f06.sym.1"),
                crate::i18n::t("formula.f06.sym.2"),
                crate::i18n::t("formula.f06.sym.3"),
            ],
            source: crate::i18n::t("formula.f06.source"),
            evaluation: format!(
                "Re = ({:.4} · {:.1} · {:.4}) / {:.3e} = {:.2e}",
                a.rho, p.v_cr, m.wing.mac, a.mu, m.aero.re_wing
            ),
        },
    ];
    render_formulas_section(
        ui,
        "mission_form",
        crate::i18n::t("formula.section.mission"),
        &formulas,
    );
}

/// Ağırlık Dökümü Formülleri
pub fn show_weight_formulas(ui: &mut Ui, p: &Project, m: &Model) {
    let w = &m.weight;
    let formulas = [
        FormulaDoc {
            name: crate::i18n::t("formula.f07.name"),
            formula: "m_batt = N_cells · m_cell · k_overhead  ,  E_usable = E_pack · DoD",
            symbols: vec![
                crate::i18n::t("formula.f07.sym.0"),
                crate::i18n::t("formula.f07.sym.1"),
                crate::i18n::t("formula.f07.sym.2"),
            ],
            source: crate::i18n::t("formula.f07.source"),
            evaluation: format!(
                "m_batt = {} · {:.3} kg · {:.2} = {:.3} kg ({} g) | E_usable = {:.1} Wh · {:.2} = {:.1} Wh",
                w.n_cells,
                m.cell.mass_g / 1000.0,
                p.pack_overhead,
                w.m_battery,
                (w.m_battery * 1000.0) as i64,
                w.pack_wh,
                p.dod,
                performance::e_usable_j(&m.perf_in) / 3600.0
            ),
        },
        FormulaDoc {
            name: crate::i18n::t("formula.f08.name"),
            formula: "A_wet,w = 2 · S · (1 + 0.2 · (t/c))",
            symbols: vec![
                crate::i18n::t("formula.f08.sym.0"),
                crate::i18n::t("formula.f08.sym.1"),
                crate::i18n::t("formula.f08.sym.2"),
            ],
            source: crate::i18n::t("formula.f08.source"),
            evaluation: format!(
                "A_wet,w = 2 · {:.3} · (1 + 0.2 · {:.2}) = {:.3} m²",
                p.s, p.tc_w, w.a_wet_w
            ),
        },
        FormulaDoc {
            name: crate::i18n::t("formula.f09.name"),
            formula: "m_shell = A_wet,w · t_wall · ρ_mat · k_int",
            symbols: vec![
                crate::i18n::t("formula.f09.sym.0"),
                crate::i18n::t("formula.f09.sym.1"),
                crate::i18n::t("formula.f09.sym.2"),
            ],
            source: crate::i18n::t("formula.f09.source"),
            evaluation: format!(
                "m_shell = {:.3} · {:.5} · {:.0} · {:.2} = {:.3} kg ({} g)",
                w.a_wet_w,
                p.t_wall,
                p.rho_mat,
                p.k_infill,
                w.m_shell,
                (w.m_shell * 1000.0) as i64
            ),
        },
        FormulaDoc {
            name: crate::i18n::t("formula.f10.name"),
            formula: "m_spar = (π/4) · (d_out² − d_in²) · l_spar · ρ_cf",
            symbols: vec![
                crate::i18n::t("formula.f10.sym.0"),
                crate::i18n::t("formula.f10.sym.1"),
                crate::i18n::t("formula.f10.sym.2"),
            ],
            source: crate::i18n::t("formula.f10.source"),
            evaluation: format!(
                "m_spar = (π/4) · ({:.3}² − {:.3}²) · {:.3} · {:.0} = {:.3} kg ({:.1} g)",
                p.d_spar_o,
                p.d_spar_i,
                p.l_spar,
                p.rho_cf,
                w.m_spar,
                w.m_spar * 1000.0
            ),
        },
        FormulaDoc {
            name: crate::i18n::t("formula.f11.name"),
            formula: "MTOW = m_payload + m_avionics + m_propulsion + m_fuse_tail + m_wing + m_batt",
            symbols: vec![
                crate::i18n::t("formula.f11.sym.0"),
                crate::i18n::t("formula.f11.sym.1"),
            ],
            source: crate::i18n::t("formula.f11.source"),
            evaluation: format!(
                "MTOW = {:.3} + {:.3} + {:.3} + {:.3} + {:.3} + {:.3} = {:.3} kg (W = {:.2} N, Batarya: %{:.1})",
                p.m_payload,
                p.m_avionics,
                p.m_propulsion,
                p.m_fuse_tail,
                w.m_wing,
                w.m_battery,
                w.m_mtow,
                w.w,
                w.f_batt * 100.0
            ),
        },
    ];
    render_formulas_section(
        ui,
        "weight_form",
        crate::i18n::t("formula.section.weight"),
        &formulas,
    );
}

/// Kanat & Kuyruk Geometrisi Formülleri
pub fn show_geometry_formulas(ui: &mut Ui, p: &Project, m: &Model) {
    let w = &m.wing;
    let t = &m.tail;
    let formulas = [
        FormulaDoc {
            name: crate::i18n::t("formula.f12.name"),
            formula: "AR = b² / S",
            symbols: vec![
                crate::i18n::t("formula.f12.sym.0"),
                crate::i18n::t("formula.f12.sym.1"),
                crate::i18n::t("formula.f12.sym.2"),
            ],
            source: crate::i18n::t("formula.f12.source"),
            evaluation: format!("AR = {:.3}² / {:.3} = {:.2}", p.b, p.s, w.ar),
        },
        FormulaDoc {
            name: crate::i18n::t("formula.f13.name"),
            formula: "c_r = (2 · S) / (b · (1 + λ))  ,  c_t = λ · c_r",
            symbols: vec![
                crate::i18n::t("formula.f13.sym.0"),
                crate::i18n::t("formula.f13.sym.1"),
            ],
            source: crate::i18n::t("formula.f13.source"),
            evaluation: format!(
                "c_r = (2 · {:.3}) / ({:.2} · (1 + {:.2})) = {:.3} m ({} mm) | c_t = {:.2} · {:.3} = {:.3} m ({} mm)",
                p.s,
                p.b,
                p.taper,
                w.c_r,
                (w.c_r * 1000.0) as i64,
                p.taper,
                w.c_r,
                w.c_t,
                (w.c_t * 1000.0) as i64
            ),
        },
        FormulaDoc {
            name: crate::i18n::t("formula.f14.name"),
            formula: "MAC = (2/3) · c_r · (1 + λ + λ²) / (1 + λ)  ,  y_mac = (b/6) · (1 + 2λ) / (1 + λ)",
            symbols: vec![
                crate::i18n::t("formula.f14.sym.0"),
                crate::i18n::t("formula.f14.sym.1"),
            ],
            source: crate::i18n::t("formula.f14.source"),
            evaluation: format!(
                "MAC = (2/3) · {:.3} · (1 + {:.2} + {:.3}) / (1 + {:.2}) = {:.4} m ({} mm) | y_mac = {} mm",
                w.c_r,
                p.taper,
                p.taper * p.taper,
                p.taper,
                w.mac,
                (w.mac * 1000.0) as i64,
                (w.y_mac * 1000.0) as i64
            ),
        },
        FormulaDoc {
            name: crate::i18n::t("formula.f15.name"),
            formula: "W/S = (MTOW · g) / S",
            symbols: vec![
                crate::i18n::t("formula.f15.sym.0"),
                crate::i18n::t("formula.f15.sym.1"),
            ],
            source: crate::i18n::t("formula.f15.source"),
            evaluation: format!(
                "W/S = {:.2} / {:.3} = {:.1} N/m² ({:.2} kg/m²)",
                m.weight.w,
                p.s,
                m.ws,
                m.ws / 9.80665
            ),
        },
        FormulaDoc {
            name: crate::i18n::t("formula.f16.name"),
            formula: "V_H = (S_h · l_h) / (S · MAC)  ,  V_V = (S_v · l_v) / (S · b)",
            symbols: vec![
                crate::i18n::t("formula.f16.sym.0"),
                crate::i18n::t("formula.f16.sym.1"),
                crate::i18n::t("formula.f16.sym.2"),
            ],
            source: crate::i18n::t("formula.f16.source"),
            evaluation: format!(
                "V_H = ({:.4} · {:.3}) / ({:.3} · {:.4}) = {:.3} | V_V = ({:.4} · {:.3}) / ({:.3} · {:.2}) = {:.4}",
                t.s_h_eff, p.l_h, p.s, w.mac, t.v_h, t.s_v_eff, p.l_v, p.s, p.b, t.v_v
            ),
        },
    ];
    render_formulas_section(
        ui,
        "geom_form",
        crate::i18n::t("formula.section.wing_tail"),
        &formulas,
    );
}

/// Aerodinamik Formülleri
pub fn show_aero_formulas(ui: &mut Ui, p: &Project, m: &Model) {
    let a = &m.aero;
    let formulas = [
        FormulaDoc {
            name: crate::i18n::t("formula.f17.name"),
            formula: "CLα = Clα / (√(1 + (Clα / (π·AR))²) + Clα / (π·AR))",
            symbols: vec![
                crate::i18n::t("formula.f17.sym.0"),
                crate::i18n::t("formula.f17.sym.1"),
                crate::i18n::t("formula.f17.sym.2"),
            ],
            source: crate::i18n::t("formula.f17.source"),
            evaluation: format!(
                "CLα = {:.3} / (√(1 + ({:.3}/(π·{:.1}))²) + {:.3}/(π·{:.1})) = {:.3} 1/rad ({:.4} 1/°)",
                p.cl_alpha_2d,
                p.cl_alpha_2d,
                m.wing.ar,
                p.cl_alpha_2d,
                m.wing.ar,
                a.cla_w,
                a.cla_w.to_radians()
            ),
        },
        FormulaDoc {
            name: crate::i18n::t("formula.f18.name"),
            formula: "e = 1 / (1.05 + 0.007·π·AR)  ,  k = 1 / (π · e · AR)",
            symbols: vec![
                crate::i18n::t("formula.f18.sym.0"),
                crate::i18n::t("formula.f18.sym.1"),
            ],
            source: crate::i18n::t("formula.f18.source"),
            evaluation: format!(
                "e = 1 / (1.05 + 0.007·π·{:.2}) = {:.3} | k = 1 / (π · {:.3} · {:.2}) = {:.4}",
                m.wing.ar, a.e, a.e, m.wing.ar, a.k
            ),
        },
        FormulaDoc {
            name: crate::i18n::t("formula.f19.name"),
            formula: "Cf = 0.455 / (log₁₀ Re)^2.58",
            symbols: vec![
                crate::i18n::t("formula.f19.sym.0"),
                crate::i18n::t("formula.f19.sym.1"),
            ],
            source: crate::i18n::t("formula.f19.source"),
            evaluation: format!(
                "Cf_kanat = 0.455 / (log₁₀ {:.2e})^2.58 = {:.5}",
                a.re_wing,
                a.items.first().map(|x| x.cf).unwrap_or(0.0)
            ),
        },
        FormulaDoc {
            name: crate::i18n::t("formula.f20.name"),
            formula: "CD0 = (1/S) · ∑ (Cf_i · FF_i · Q_i · S_wet,i) + CD_misc",
            symbols: vec![
                crate::i18n::t("formula.f20.sym.0"),
                crate::i18n::t("formula.f20.sym.1"),
                crate::i18n::t("formula.f20.sym.2"),
            ],
            source: crate::i18n::t("formula.f20.source"),
            evaluation: format!(
                "CD0 = {:.5} ({})",
                a.cd0,
                crate::i18n::t("formula.eval.parasite_drag")
            ),
        },
        FormulaDoc {
            name: crate::i18n::t("formula.f21.name"),
            formula: "(L/D)_max = 1 / (2 · √(CD0 · k))  ,  CL_cr = 2 · (W/S) / (ρ · V_cr²)",
            symbols: vec![
                crate::i18n::t("formula.f21.sym.0"),
                crate::i18n::t("formula.f21.sym.1"),
            ],
            source: crate::i18n::t("formula.f21.source"),
            evaluation: format!(
                "(L/D)_max = 1 / (2 · √({:.4} · {:.4})) = {:.1} | CL_cr = (2 · {:.1}) / ({:.3} · {:.1}²) = {:.3}",
                a.cd0,
                a.k,
                a.ld_max,
                m.ws,
                m.atmo.rho,
                p.v_cr,
                2.0 * m.ws / (m.atmo.rho * p.v_cr * p.v_cr)
            ),
        },
    ];
    render_formulas_section(
        ui,
        "aero_form",
        crate::i18n::t("formula.section.aero"),
        &formulas,
    );
}

/// Tasarım Noktası Formülleri
pub fn show_design_point_formulas(ui: &mut Ui, p: &Project, m: &Model) {
    let c = &m.cons;
    let formulas = [
        FormulaDoc {
            name: crate::i18n::t("formula.f22.name"),
            formula: "V_stall = √(2 · (W/S) / (ρ · CL_max)) ≤ V_stall_req  ⇒  (W/S)_max = 0.5 · ρ · V_req² · CL_max",
            symbols: vec![
                crate::i18n::t("formula.f22.sym.0"),
                crate::i18n::t("formula.f22.sym.1"),
            ],
            source: crate::i18n::t("formula.f22.source"),
            evaluation: format!(
                "V_stall = √(2 · {:.1} / ({:.3} · {:.3})) = {:.2} m/s ({} ≤ {:.1}) | (W/S)_max = 0.5 · {:.3} · {:.1}² · {:.3} = {:.1} N/m²",
                c.ws,
                m.atmo.rho,
                m.aero.cl_max,
                c.v_stall,
                crate::i18n::t("formula.eval.req"),
                p.v_stall_req,
                m.atmo.rho,
                p.v_stall_req,
                m.aero.cl_max,
                c.ws_max
            ),
        },
        FormulaDoc {
            name: crate::i18n::t("formula.f23.name"),
            formula: "(P/W)_cr = (V_cr / η_prop) · (CD0 / CL_cr + k · CL_cr)",
            symbols: vec![
                crate::i18n::t("formula.f23.sym.0"),
                crate::i18n::t("formula.f23.sym.1"),
            ],
            source: crate::i18n::t("formula.f23.source"),
            evaluation: format!(
                "(P/W)_cr = ({:.1} / {:.2}) · ({:.4} / {:.3} + {:.4} · {:.3}) = {:.2} W/N",
                p.v_cr, p.eta_p_cr, m.aero.cd0, c.cruise.cl, m.aero.k, c.cruise.cl, c.cruise.pw
            ),
        },
        FormulaDoc {
            name: crate::i18n::t("formula.f24.name"),
            formula: "(P/W)_climb = (1 / η_climb) · [ R/C + V_y · (CD0 / CL_y + k · CL_y) ]",
            symbols: vec![
                crate::i18n::t("formula.f24.sym.0"),
                crate::i18n::t("formula.f24.sym.1"),
            ],
            source: crate::i18n::t("formula.f24.source"),
            evaluation: format!(
                "(P/W)_climb = (1 / {:.2}) · [{:.1} + {:.1} · ({:.4}/{:.2} + {:.4}·{:.2})] = {:.2} W/N",
                p.eta_p_climb,
                p.rc,
                p.v_y,
                m.aero.cd0,
                c.climb.cl,
                m.aero.k,
                c.climb.cl,
                c.climb.pw
            ),
        },
        FormulaDoc {
            name: crate::i18n::t("formula.f25.name"),
            formula: "(P/W)_turn = (V_turn / η_prop) · [ CD0 / CL_turn + k · n² · CL_turn ]",
            symbols: vec![
                crate::i18n::t("formula.f25.sym.0"),
                crate::i18n::t("formula.f25.sym.1"),
            ],
            source: crate::i18n::t("formula.f25.source"),
            evaluation: format!(
                "(P/W)_turn = ({:.1} / {:.2}) · [{:.4}/{:.2} + {:.4}·{:.2}²·{:.2}] = {:.2} W/N",
                p.v_turn,
                p.eta_p_cr,
                m.aero.cd0,
                c.turn.cl,
                m.aero.k,
                1.0 / p.phi_turn_deg.to_radians().cos(),
                c.turn.cl,
                c.turn.pw
            ),
        },
        FormulaDoc {
            name: crate::i18n::t("formula.f26.name"),
            formula: "T/W = (a_launch / g) + sin(γ) + 1 / (L/D)_launch",
            symbols: vec![
                crate::i18n::t("formula.f26.sym.0"),
                crate::i18n::t("formula.f26.sym.1"),
                crate::i18n::t("formula.f26.sym.2"),
            ],
            source: crate::i18n::t("formula.f26.source"),
            evaluation: format!(
                "T/W = ({:.1} / 9.81) + sin({:.0}°) + 1 / {:.1} = {:.3} (Statik P/W = {:.2} W/N)",
                p.a_launch, p.gamma_launch_deg, c.launch.ld, c.launch_tw, c.launch.pw
            ),
        },
    ];
    render_formulas_section(
        ui,
        "dp_form",
        crate::i18n::t("formula.section.constraint"),
        &formulas,
    );
}

/// Menzil & Havada Kalma Formülleri
pub fn show_endurance_formulas(ui: &mut Ui, p: &Project, m: &Model) {
    let formulas = [
        FormulaDoc {
            name: crate::i18n::t("formula.f27.name"),
            formula: "R = E_usable · η_total · ((L/D) / W) · (1 / (1 + P_hotel / P_prop_elec))",
            symbols: vec![
                crate::i18n::t("formula.f27.sym.0"),
                crate::i18n::t("formula.f27.sym.1"),
                crate::i18n::t("formula.f27.sym.2"),
                crate::i18n::t("formula.f27.sym.3"),
            ],
            source: crate::i18n::t("formula.f27.source"),
            evaluation: format!(
                "R = {:.0} J · {:.3} · ({:.1} / {:.2} N) · (1 / (1 + {:.1} / {:.1})) = {:.1} km",
                performance::e_usable_j(&m.perf_in),
                performance::eta_pt(&m.perf_in),
                m.perf_cr.ld,
                m.weight.w,
                p.p_hotel,
                m.perf_cr.p_prop_elec,
                m.perf_cr.range_km()
            ),
        },
        FormulaDoc {
            name: crate::i18n::t("formula.f28.name"),
            formula: "t = E_usable / (P_prop_elec + P_hotel)",
            symbols: vec![
                crate::i18n::t("formula.f28.sym.0"),
                crate::i18n::t("formula.f28.sym.1"),
            ],
            source: crate::i18n::t("formula.f28.source"),
            evaluation: format!(
                "t = {:.1} Wh / ({:.1} W + {:.1} W) = {:.2} saat ({:.1} dakika)",
                performance::e_usable_j(&m.perf_in) / 3600.0,
                m.perf_cr.p_prop_elec,
                p.p_hotel,
                m.perf_cr.endurance_h(),
                m.perf_cr.endurance_h() * 60.0
            ),
        },
        FormulaDoc {
            name: crate::i18n::t("formula.f29.name"),
            formula: "V_range = √(2 · (W/S) / (ρ · √(CD0 / k)))",
            symbols: vec![crate::i18n::t("formula.f29.sym.0")],
            source: crate::i18n::t("formula.f29.source"),
            evaluation: format!(
                "V_range = √(2 · {:.1} / ({:.3} · √({:.4} / {:.4}))) = {:.1} m/s ({:.1} km/h)",
                m.ws,
                m.atmo.rho,
                m.aero.cd0,
                m.aero.k,
                m.aero.v_range,
                m.aero.v_range * 3.6
            ),
        },
        FormulaDoc {
            name: crate::i18n::t("formula.f30.name"),
            formula: "V_loiter = √(2 · (W/S) / (ρ · √(3·CD0 / k)))  (Limit: ≥ 1.2 · V_stall)",
            symbols: vec![
                crate::i18n::t("formula.f30.sym.0"),
                crate::i18n::t("formula.f30.sym.1"),
            ],
            source: crate::i18n::t("formula.f30.source"),
            evaluation: format!(
                "V_loiter = 1.2 · {:.2} = {:.1} m/s ({:.1} km/h) [{}: {:.1} m/s]",
                m.aero.v_stall,
                m.aero.v_end,
                m.aero.v_end * 3.6,
                crate::i18n::t("formula.eval.free_formula"),
                m.aero.v_end_unclamped
            ),
        },
    ];
    render_formulas_section(
        ui,
        "end_form",
        crate::i18n::t("formula.section.endurance"),
        &formulas,
    );
}

/// İtki Sistemi Formülleri
pub fn show_propulsion_formulas(ui: &mut Ui, p: &Project, m: &Model) {
    let pr = &m.prop;
    let formulas = [
        FormulaDoc {
            name: crate::i18n::t("formula.f31.name"),
            formula: "P_ideal = T^1.5 / √(2 · ρ₀ · A_prop)  ,  P_shaft = P_ideal / FM",
            symbols: vec![
                crate::i18n::t("formula.f31.sym.0"),
                crate::i18n::t("formula.f31.sym.1"),
                crate::i18n::t("formula.f31.sym.2"),
            ],
            source: crate::i18n::t("formula.f31.source"),
            evaluation: format!(
                "P_ideal = {:.1}^1.5 / √(2 · 1.225 · {:.4}) = {:.1} W | P_shaft = {:.1} / {:.2} = {:.1} W",
                pr.t_static, pr.a_disk, pr.p_ideal, pr.p_ideal, p.fm, pr.p_shaft_static
            ),
        },
        FormulaDoc {
            name: crate::i18n::t("formula.f32.name"),
            formula: "P_elec = P_shaft / (η_mot · η_esc)  ,  I_peak = P_elec / V_pack",
            symbols: vec![
                crate::i18n::t("formula.f32.sym.0"),
                crate::i18n::t("formula.f32.sym.1"),
            ],
            source: crate::i18n::t("formula.f32.source"),
            evaluation: format!(
                "P_elec = {:.1} / ({:.2} · {:.2}) = {:.1} W | I_peak = {:.1} / {:.1} V = {:.1} A ({}: {:.0} A)",
                pr.p_shaft_static,
                p.eta_mot,
                p.eta_esc,
                pr.p_elec_static,
                pr.p_elec_static,
                m.weight.pack_voltage,
                pr.i_peak,
                crate::i18n::t("formula.eval.limit"),
                m.prop_in.pack_max_cont_a
            ),
        },
        FormulaDoc {
            name: crate::i18n::t("formula.f33.name"),
            formula: "RPM_max = (M_tip,max · a) / (π · D_prop) · 60",
            symbols: vec![
                crate::i18n::t("formula.f33.sym.0"),
                crate::i18n::t("formula.f33.sym.1"),
            ],
            source: crate::i18n::t("formula.f33.source"),
            evaluation: format!(
                "RPM_max = ({:.2} · {:.1}) / (π · {:.3}) · 60 = {:.0} RPM",
                p.m_tip_max, m.atmo.a, p.d_prop, pr.rpm_max
            ),
        },
        FormulaDoc {
            name: crate::i18n::t("formula.f34.name"),
            formula: "RPM_cr = V_cr / (pitch · (1 − slip)) · 60",
            symbols: vec![
                crate::i18n::t("formula.f34.sym.0"),
                crate::i18n::t("formula.f34.sym.1"),
            ],
            source: crate::i18n::t("formula.f34.source"),
            evaluation: format!(
                "RPM_cr = {:.1} / ({:.3} · (1 − {:.2})) · 60 = {:.0} RPM",
                p.v_cr, p.prop_pitch, p.prop_slip, pr.rpm_cr
            ),
        },
    ];
    render_formulas_section(
        ui,
        "prop_form",
        crate::i18n::t("formula.section.propulsion"),
        &formulas,
    );
}

/// Ağırlık Merkezi & Stabilite Formülleri
pub fn show_stability_formulas(ui: &mut Ui, p: &Project, m: &Model) {
    let s = &m.stab;
    let formulas = [
        FormulaDoc {
            name: crate::i18n::t("formula.f35.name"),
            formula: "x_cg = (∑ m_i · x_i) / (∑ m_i)",
            symbols: vec![
                crate::i18n::t("formula.f35.sym.0"),
                crate::i18n::t("formula.f35.sym.1"),
            ],
            source: crate::i18n::t("formula.f35.source"),
            evaluation: format!(
                "x_cg = {:.4} kg·m / {:.3} kg = {:.3} m ({}: {} mm, MAC: %{:.1})",
                s.cg_nose.sum_mx,
                s.cg_nose.mass,
                s.cg_nose.x,
                crate::i18n::t("formula.eval.from_nose"),
                (s.cg_nose.x * 1000.0) as i64,
                s.x_cg_rel / m.wing.mac * 100.0
            ),
        },
        FormulaDoc {
            name: crate::i18n::t("formula.f36.name"),
            formula: "x_np = x_ac,w + (CLα,h / CLα,w) · η_h · (S_h / S) · l_h · (1 − dε/dα)",
            symbols: vec![
                crate::i18n::t("formula.f36.sym.0"),
                crate::i18n::t("formula.f36.sym.1"),
                crate::i18n::t("formula.f36.sym.2"),
            ],
            source: crate::i18n::t("formula.f36.source"),
            evaluation: format!(
                "x_np = {:.4} + ({:.2}/{:.2}) · {:.2} · ({:.3}/{:.3}) · {:.3} · (1 − {:.3}) = {:.4} m ({} mm, MAC: %{:.1})",
                s.np.x_ac_w,
                p.cla_h,
                m.aero.cla_w,
                p.eta_h,
                m.tail.s_h_eff,
                p.s,
                p.l_h,
                s.np.deps,
                s.np.x_np,
                (s.np.x_np * 1000.0) as i64,
                s.np.x_np / m.wing.mac * 100.0
            ),
        },
        FormulaDoc {
            name: crate::i18n::t("formula.f37.name"),
            formula: "SM = (x_np − x_cg) / MAC",
            symbols: vec![
                crate::i18n::t("formula.f37.sym.0"),
                crate::i18n::t("formula.f37.sym.1"),
            ],
            source: crate::i18n::t("formula.f37.source"),
            evaluation: format!(
                "SM = ({:.4} − {:.4}) / {:.4} = %{:.1} (Bant: %{:.0} - %{:.0})",
                s.np.x_np,
                s.x_cg_rel,
                m.wing.mac,
                s.sm * 100.0,
                s.band.0 * 100.0,
                s.band.1 * 100.0
            ),
        },
        FormulaDoc {
            name: crate::i18n::t("formula.f38.name"),
            formula: "x_batt = [ (M_fixed + m_batt) · x_cg,target − ∑ m_fixed · x_fixed ] / m_batt",
            symbols: vec![
                crate::i18n::t("formula.f38.sym.0"),
                crate::i18n::t("formula.f38.sym.1"),
            ],
            source: crate::i18n::t("formula.f38.source"),
            evaluation: format!(
                "x_batt_rec = {:.0} mm ({}: {:.0} mm)",
                s.batt_x_nose * 1000.0,
                crate::i18n::t("formula.eval.shift_from_current"),
                s.batt_x_shift * 1000.0
            ),
        },
    ];
    render_formulas_section(
        ui,
        "stab_form",
        crate::i18n::t("formula.section.stability"),
        &formulas,
    );
}

/// Yapı Formülleri
pub fn show_structure_formulas(ui: &mut Ui, p: &Project, m: &Model) {
    let st = &m.st;
    let formulas = [
        FormulaDoc {
            name: crate::i18n::t("formula.f39.name"),
            formula: "M_b = n_ult · W · (b / (3 · π))",
            symbols: vec![
                crate::i18n::t("formula.f39.sym.0"),
                crate::i18n::t("formula.f39.sym.1"),
                crate::i18n::t("formula.f39.sym.2"),
            ],
            source: crate::i18n::t("formula.f39.source"),
            evaluation: format!(
                "M_b = {:.2} · {:.2} · ({:.2} / (3 · π)) = {:.2} N·m",
                st.n_ult, m.weight.w, p.b, st.m_b_ult
            ),
        },
        FormulaDoc {
            name: crate::i18n::t("formula.f40.name"),
            formula: "Z = (π · (d_out⁴ − d_in⁴)) / (32 · d_out)  ,  σ = M_b / Z",
            symbols: vec![
                crate::i18n::t("formula.f40.sym.0"),
                crate::i18n::t("formula.f40.sym.1"),
                crate::i18n::t("formula.f40.sym.2"),
            ],
            source: crate::i18n::t("formula.f40.source"),
            evaluation: format!(
                "Z = (π · ({:.3}⁴ − {:.3}⁴)) / (32 · {:.3}) = {:.3e} m³ | σ = {:.2} / {:.3e} = {:.0} MPa",
                p.d_spar_o,
                p.d_spar_i,
                p.d_spar_o,
                st.z_spar,
                st.m_b_ult,
                st.z_spar,
                st.sigma_max / 1e6
            ),
        },
        FormulaDoc {
            name: crate::i18n::t("formula.f41.name"),
            formula: "MS = (σ_allow / σ_max) − 1",
            symbols: vec![
                crate::i18n::t("formula.f41.sym.0"),
                crate::i18n::t("formula.f41.sym.1"),
            ],
            source: crate::i18n::t("formula.f41.source"),
            evaluation: format!(
                "MS = ({:.0} MPa / {:.0} MPa) − 1 = {:+.2} ({})",
                p.sigma_allow / 1e6,
                st.sigma_max / 1e6,
                st.ms,
                if st.ms >= 0.0 {
                    crate::i18n::t("status.safe")
                } else {
                    crate::i18n::t("status.risk_broken")
                }
            ),
        },
    ];
    render_formulas_section(
        ui,
        "struct_form",
        crate::i18n::t("formula.section.structure"),
        &formulas,
    );
}

/// Multirotor Çerçeve Formülleri
pub fn show_mr_frame_formulas(ui: &mut Ui, p: &Project, m: &Model) {
    let mr = &m.mr;
    let formulas = [
        FormulaDoc {
            name: crate::i18n::t("formula.f43.name"),
            formula: "S_adj = D_frame · sin(π / n)",
            symbols: vec![
                crate::i18n::t("formula.f43.sym.0"),
                crate::i18n::t("formula.f43.sym.1"),
                crate::i18n::t("formula.f43.sym.2"),
            ],
            source: crate::i18n::t("formula.f43.source"),
            evaluation: format!(
                "S_adj = {:.1} mm · sin(π / {}) = {:.1} mm",
                p.mr_frame_size * 1000.0,
                p.mr_arm_count.max(3),
                mr.s_adj * 1000.0
            ),
        },
        FormulaDoc {
            name: crate::i18n::t("formula.f44.name"),
            formula: "D_prop,max = S_adj − δ_clearance",
            symbols: vec![
                crate::i18n::t("formula.f44.sym.0"),
                crate::i18n::t("formula.f44.sym.1"),
            ],
            source: crate::i18n::t("formula.f44.source"),
            evaluation: format!(
                "D_prop,max = {:.1} mm − {:.1} mm = {:.1} mm ({:.2} in)",
                mr.s_adj * 1000.0,
                p.mr_prop_clearance * 1000.0,
                mr.d_prop_max * 1000.0,
                mr.d_prop_max_inch
            ),
        },
        FormulaDoc {
            name: crate::i18n::t("formula.f45.name"),
            formula: "L_arm = (D_frame / 2) − R_plate",
            symbols: vec![
                crate::i18n::t("formula.f45.sym.0"),
                crate::i18n::t("formula.f45.sym.1"),
            ],
            source: crate::i18n::t("formula.f45.source"),
            evaluation: format!(
                "L_arm = ({:.1} / 2) − {:.1} = {:.1} mm",
                p.mr_frame_size * 1000.0,
                p.mr_r_plate * 1000.0,
                mr.l_arm * 1000.0
            ),
        },
    ];
    render_formulas_section(
        ui,
        "mr_frame_form",
        crate::i18n::t("formula.section.mr_frame"),
        &formulas,
    );
}

/// Multirotor Motor & İtki Formülleri
pub fn show_mr_propulsion_formulas(ui: &mut Ui, _p: &Project, m: &Model) {
    let mr = &m.mr;
    let formulas = [
        FormulaDoc {
            name: crate::i18n::t("formula.f46.name"),
            formula: "P_ideal,kol = [T_kol^1.5 / √(2·ρ·A)] · κ_int",
            symbols: vec![
                crate::i18n::t("formula.f46.sym.0"),
                crate::i18n::t("formula.f46.sym.1"),
                crate::i18n::t("formula.f46.sym.2"),
            ],
            source: crate::i18n::t("formula.f46.source"),
            evaluation: format!(
                "P_ideal = {:.1} W / motor (Toplam: {:.1} W)",
                mr.p_ideal_motor, mr.p_ideal_total
            ),
        },
        FormulaDoc {
            name: crate::i18n::t("formula.f47.name"),
            formula: "P_elec = P_ideal / (FM · η_mot · η_esc)  ,  Throttle_hover ≈ √(1 / (T/W))",
            symbols: vec![
                crate::i18n::t("formula.f47.sym.0"),
                crate::i18n::t("formula.f47.sym.1"),
                crate::i18n::t("formula.f47.sym.2"),
            ],
            source: crate::i18n::t("formula.f47.source"),
            evaluation: format!(
                "P_elec = {:.1} W/motor (Toplam: {:.1} W) | Throttle = %{:.1} (T/W = {:.2})",
                mr.p_elec_motor,
                mr.p_elec_hover_total,
                mr.hover_throttle * 100.0,
                mr.tw_ratio
            ),
        },
    ];
    render_formulas_section(
        ui,
        "mr_prop_form",
        crate::i18n::t("formula.section.mr_prop"),
        &formulas,
    );
}

/// Multirotor Pil & Uçuş Süresi Formülleri
pub fn show_mr_battery_formulas(ui: &mut Ui, p: &Project, m: &Model) {
    let mr = &m.mr;
    let formulas = [
        FormulaDoc {
            name: crate::i18n::t("formula.f48.name"),
            formula: "t_hover = [ E_usable / (P_hover_elec + P_avionics) ] · 60",
            symbols: vec![
                crate::i18n::t("formula.f48.sym.0"),
                crate::i18n::t("formula.f48.sym.1"),
                crate::i18n::t("formula.f48.sym.2"),
            ],
            source: crate::i18n::t("formula.f48.source"),
            evaluation: format!(
                "t_hover = [{:.1} Wh / ({:.1} W + {:.1} W)] · 60 = {:.1} dakika ({:.2} saat)",
                mr.e_usable_wh,
                mr.p_elec_hover_total,
                p.p_hotel,
                mr.hover_endurance_min,
                mr.hover_endurance_min / 60.0
            ),
        },
        FormulaDoc {
            name: crate::i18n::t("formula.f49.name"),
            formula: "P(V) = P_induced(V)/FM + ½·ρ·V³·f  ,  Range = V · [E_usable / P_elec(V)] · 3.6",
            symbols: vec![
                crate::i18n::t("formula.f49.sym.0"),
                crate::i18n::t("formula.f49.sym.1"),
                crate::i18n::t("formula.f49.sym.2"),
            ],
            source: crate::i18n::t("formula.f49.source"),
            evaluation: format!(
                "P_cruise = {:.1} W (%85: {:.1} W) | {}: {:.1} min | {}: {:.1} km ({}: {:.1} km @ {:.0} m/s)",
                mr.p_total_cruise,
                mr.p_total_cruise_85,
                crate::i18n::t("formula.eval.duration"),
                mr.cruise_endurance_min,
                crate::i18n::t("formula.eval.range"),
                mr.range_km,
                crate::i18n::t("formula.eval.best"),
                mr.range_best_km,
                mr.v_best_range
            ),
        },
    ];
    render_formulas_section(
        ui,
        "mr_batt_form",
        crate::i18n::t("formula.section.mr_batt"),
        &formulas,
    );
}

/// Multirotor AG Formülleri
pub fn show_mr_cg_formulas(ui: &mut Ui, _p: &Project, m: &Model) {
    let mr = &m.mr;
    let formulas = [FormulaDoc {
        name: crate::i18n::t("formula.f50.name"),
        formula: "x_geom = (1/n) · ∑ x_motor,i  ,  Δ_cg = √((x_cg − x_geom)² + (y_cg − y_geom)²)",
        symbols: vec![
            crate::i18n::t("formula.f50.sym.0"),
            crate::i18n::t("formula.f50.sym.1"),
            crate::i18n::t("formula.f50.sym.2"),
        ],
        source: crate::i18n::t("formula.f50.source"),
        evaluation: format!(
            "Geom: ({:.1}, {:.1}) mm | AG: ({:.1}, {:.1}) mm | {}: {:.1} mm (%{:.1} D_frame, {}: {})",
            mr.geom_center_x * 1000.0,
            mr.geom_center_y * 1000.0,
            mr.cg_x * 1000.0,
            mr.cg_y * 1000.0,
            crate::i18n::t("formula.eval.offset"),
            mr.cg_offset_d * 1000.0,
            mr.cg_offset_frac * 100.0,
            crate::i18n::t("formula.eval.limit_3pct"),
            if mr.cg_offset_ok {
                crate::i18n::t("status.suitable")
            } else {
                crate::i18n::t("status.unbalanced")
            }
        ),
    }];
    render_formulas_section(
        ui,
        "mr_cg_form",
        crate::i18n::t("formula.section.mr_cg"),
        &formulas,
    );
}

/// Özet Sekmesi Formülleri
pub fn show_summary_formulas(ui: &mut Ui, p: &Project, m: &Model) {
    if p.drone_type == DroneType::Multirotor {
        show_mr_propulsion_formulas(ui, p, m);
        show_mr_battery_formulas(ui, p, m);
    } else {
        show_geometry_formulas(ui, p, m);
        show_aero_formulas(ui, p, m);
        show_design_point_formulas(ui, p, m);
        show_endurance_formulas(ui, p, m);
    }
}
